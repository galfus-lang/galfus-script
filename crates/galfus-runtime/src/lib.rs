pub mod driver;
pub mod event;
pub mod execution;
mod kernel;
mod orchestrator;
pub mod queue;
pub mod registry;
pub mod task;
#[cfg(test)]
mod tests;

use std::collections::VecDeque;
use std::rc::Rc;
use std::sync;

use galfus_contract::{BoundaryType, BoundaryValue, Providers};
use galfus_vm::{VirtualMachine, VmPanic, VmValue};

pub use driver::CooperativeDriver;
pub use execution::{Execution, ExecutionHandle, ExecutionState};

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("module `{0}` is not loaded")]
    ModuleNotLoaded(String),
    #[error("entry function `{0}` is not exported by the entry module")]
    EntryNotExported(String),
    #[error("entry function `{name}` expects {expected} parameter(s), found {found}")]
    EntryArityMismatch {
        name: String,
        expected: usize,
        found: usize,
    },
    #[error("entry function `{name}` must return i32")]
    EntryReturnTypeMismatch { name: String },
    #[error("entry arguments require bytecode type `{0}`")]
    MissingArgumentType(&'static str),
    #[error(transparent)]
    GraphResolution(#[from] galfus_bytecode::GraphResolutionError),
    #[error("{0}")]
    VmPanic(#[from] VmPanic),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryArgsType {
    ByteArgv,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryReturnType {
    Int32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntryAbi {
    pub args_type: EntryArgsType,
    pub return_type: EntryReturnType,
}

impl EntryAbi {
    pub const fn default_app() -> Self {
        Self {
            args_type: EntryArgsType::ByteArgv,
            return_type: EntryReturnType::Int32,
        }
    }

    fn expected_param_count(self) -> u8 {
        match self.args_type {
            EntryArgsType::ByteArgv => 1,
        }
    }

    fn accepts_return_type(self, ty: &galfus_bytecode::BytecodeType) -> bool {
        match self.return_type {
            EntryReturnType::Int32 => ty == &galfus_bytecode::BytecodeType::Int32,
        }
    }
}

/// A single execution composed from one executable graph and optional host providers.
pub struct Runtime {
    graph: sync::Arc<galfus_bytecode::BytecodeGraph>,
    providers: Option<sync::Arc<sync::Mutex<Providers>>>,
}

impl Runtime {
    pub fn new(
        graph: sync::Arc<galfus_bytecode::BytecodeGraph>,
        providers: Option<Providers>,
    ) -> Self {
        Self {
            graph,
            providers: providers.map(|p| sync::Arc::new(sync::Mutex::new(p))),
        }
    }

    /// Starts a persistent execution from an exported entry point.
    pub fn start(
        self,
        module_id: galfus_core::ModuleId,
        entry_name: &str,
        args: &[Vec<u8>],
        driver: Rc<dyn galfus_contract::KernelDriver>,
    ) -> Result<Execution, RuntimeError> {
        let mut orchestrator = crate::orchestrator::Orchestrator::new();
        let graph = self.graph.clone();
        let image = &graph.get(module_id).unwrap().module;
        let abi = EntryAbi::default_app();
        let entry_idx = image
            .exports
            .iter()
            .find(|export| export.symbol_name == entry_name)
            .and_then(|export| match export.kind {
                galfus_bytecode::ExportKind::Function(f) => Some(f),
                _ => None,
            })
            .ok_or_else(|| RuntimeError::EntryNotExported(entry_name.to_string()))?;

        let entry_func = &image.functions[entry_idx.raw() as usize];
        if entry_func.param_count != abi.expected_param_count() {
            return Err(RuntimeError::EntryArityMismatch {
                name: entry_name.to_string(),
                expected: abi.expected_param_count() as usize,
                found: entry_func.param_count as usize,
            });
        }
        let return_ty = image.types.get(entry_func.return_ty.raw() as usize);
        if !return_ty.is_some_and(|ty| abi.accepts_return_type(ty)) {
            return Err(RuntimeError::EntryReturnTypeMismatch {
                name: entry_name.to_string(),
            });
        }

        let mut thread = galfus_vm::thread::VirtualThread::new();
        let vm = VirtualMachine::new(graph.clone()).with_provider_handle(self.providers.clone());

        let mut initializers = VecDeque::new();
        for initialized_module_id in graph.initialization_order(module_id)? {
            if thread.is_module_initialized(initialized_module_id) {
                continue;
            }
            if let Some(init_idx) = graph
                .get(initialized_module_id)
                .expect("initialization order only contains loaded modules")
                .module
                .init_func_idx
            {
                initializers.push_back((initialized_module_id, init_idx));
            } else {
                thread.mark_module_initialized(initialized_module_id);
            }
        }

        let entry_args = build_entry_args(&mut thread, &vm, module_id, args)?;
        let startup_plan =
            if let Some((initializer_module_id, initializer_func)) = initializers.pop_front() {
                thread.begin_module_initialization(initializer_module_id);
                vm.prepare_function(&mut thread, initializer_module_id, initializer_func, vec![])
                    .map_err(RuntimeError::VmPanic)?;
                Some(crate::orchestrator::StartupPlan {
                    initializers,
                    entry_module_id: module_id,
                    entry_func: entry_idx,
                    entry_args,
                })
            } else {
                vm.prepare_function(&mut thread, module_id, entry_idx, vec![entry_args])
                    .map_err(RuntimeError::VmPanic)?;
                None
            };

        let token = orchestrator.main_thread_token();
        let main_thread_id = orchestrator.kernel_mut(token).spawn(thread);

        if let Some(startup_plan) = startup_plan {
            orchestrator.set_startup_plan(main_thread_id, startup_plan);
        }

        let _ = orchestrator.kernel_mut(token).mark_running(main_thread_id);
        let main_thread = orchestrator
            .kernel_mut(token)
            .take_thread(main_thread_id)
            .unwrap();

        let vm = sync::Arc::new(vm);

        orchestrator.set_vm(vm);
        orchestrator.set_driver(driver.clone());
        orchestrator
            .kernel_mut(token)
            .enqueue_runnable(main_thread_id, main_thread);

        let sink = orchestrator.sink();
        let task = Box::new(orchestrator);

        Ok(Execution::new(task, driver, sink))
    }
}

fn build_entry_args(
    thread: &mut galfus_vm::thread::VirtualThread,
    vm: &VirtualMachine,
    module_id: galfus_core::ModuleId,
    args: &[Vec<u8>],
) -> Result<VmValue, RuntimeError> {
    let args_array_ty = vm
        .graph.get(module_id).unwrap().module
        .types
        .iter()
        .enumerate()
        .find(|(_, ty)| {
            matches!(ty, galfus_bytecode::BytecodeType::Array(element)
                if matches!(vm.graph.get(module_id).unwrap().module.types.get(element.raw() as usize), Some(galfus_bytecode::BytecodeType::Array(inner))
                    if matches!(vm.graph.get(module_id).unwrap().module.types.get(inner.raw() as usize), Some(galfus_bytecode::BytecodeType::Uint8))))
        })
        .map(|(index, _)| galfus_bytecode::instruction::TypeIdx(index as u16))
        .ok_or(RuntimeError::MissingArgumentType("[[u8]]"))?;

    let value = BoundaryValue::Array {
        element_type: BoundaryType::Array(Box::new(BoundaryType::U8)),
        values: args
            .iter()
            .map(|arg| BoundaryValue::Array {
                element_type: BoundaryType::U8,
                values: arg.iter().copied().map(BoundaryValue::U8).collect(),
            })
            .collect(),
    };
    crate::task::encode_into_thread_heap(
        &mut thread.heap,
        value,
        args_array_ty,
        module_id,
        &vm.graph.get(module_id).unwrap().module,
    )
    .map_err(|error| {
        RuntimeError::VmPanic(VmPanic {
            error: galfus_vm::VmError::TypeMismatch {
                expected: "entry arguments".to_string(),
                found: format!("{error:?}"),
            },
            stack_trace: vec![],
        })
    })
}

pub fn format_panic(graph: &galfus_bytecode::BytecodeGraph, panic: &VmPanic) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    writeln!(&mut out, "Runtime Panic: {}", panic.error).unwrap();
    writeln!(&mut out, "Stack trace:").unwrap();

    for (i, frame) in panic.stack_trace.iter().enumerate() {
        if let Some(module) = graph.get(frame.module_id) {
            let func_name = module
                .module
                .functions
                .get(frame.func_idx.raw() as usize)
                .map(|f| f.name.as_str())
                .unwrap_or("<unknown>");

            let location_str = module
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.span_for(frame.func_idx, frame.instruction_offset))
                .map(|span| {
                    format!(
                        "instruction {} at source#{}:{}..{}",
                        frame.instruction_offset,
                        span.source_id().raw(),
                        span.start(),
                        span.end()
                    )
                })
                .unwrap_or_else(|| format!("instruction {}", frame.instruction_offset));

            writeln!(
                &mut out,
                "  #{}: {}::{} (at {})",
                i,
                module.path.as_str(),
                func_name,
                location_str
            )
            .unwrap();
        } else {
            writeln!(
                &mut out,
                "  #{}: Module {:?} Func {:?} (at instruction {})",
                i, frame.module_id, frame.func_idx, frame.instruction_offset
            )
            .unwrap();
        }
    }

    out
}
