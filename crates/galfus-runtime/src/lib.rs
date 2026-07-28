pub mod queue;
pub mod registry;
pub mod task;
#[cfg(test)]
mod tests;

use std::sync;

use galfus_bytecode::BytecodeModule;
use galfus_contract::Providers;
use galfus_vm::thread::VirtualThread;
use galfus_vm::{HeapObject, VirtualMachine, VmPanic, VmValue};
use queue::{BlockedQueue, RunnableQueue};
use registry::{ThreadId, ThreadRegistry};

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
    registry: ThreadRegistry,
    runnable: RunnableQueue,
    blocked: BlockedQueue,
}

impl Runtime {
    pub fn new(
        graph: sync::Arc<galfus_bytecode::BytecodeGraph>,
        providers: Option<Providers>,
    ) -> Self {
        Self {
            graph,
            providers: providers.map(|p| sync::Arc::new(sync::Mutex::new(p))),
            registry: ThreadRegistry::new(),
            runnable: RunnableQueue::new(),
            blocked: BlockedQueue::new(),
        }
    }

    /// Cria uma nova thread a partir de um módulo e função de entrada
    pub fn spawn_thread(
        &mut self,
        thread: VirtualThread,
        executor: &dyn galfus_contract::ThreadExecutor,
    ) -> ThreadId {
        let id = ThreadId::from_executor(executor.allocate_thread_id())
            .expect("thread executor returned the reserved thread ID 0");
        self.registry.register(id, thread);
        self.runnable.enqueue(id);
        id
    }

    /// O Host deve chamar esta função para bombear os cronômetros das threads bloqueadas
    pub fn tick_timeouts(&mut self, delta_ms: u64) {
        let woke_up = self.blocked.tick_timeouts(delta_ms);
        for id in woke_up {
            // Se a thread ainda existir, mandamos de volta para runnable
            if self.registry.contains(id) {
                self.runnable.enqueue(id);
            }
        }
    }

    /// Retorna o próximo ThreadId pronto para executar
    pub fn next_runnable(&mut self) -> Option<ThreadId> {
        self.runnable.dequeue()
    }

    /// Execute an entry exported by a module loaded in the given BytecodeGraph.
    pub fn build_module_entry(
        mut self,
        module_id: galfus_core::ModuleId,
        entry_name: &str,
        args: &[Vec<u8>],
        executor: sync::Arc<dyn galfus_contract::ThreadExecutor>,
    ) -> Result<Box<dyn galfus_contract::RunnableTask>, RuntimeError> {
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
                vm.run_function(&mut thread, initialized_module_id, init_idx, vec![])?;
            }
            thread.mark_module_initialized(initialized_module_id);
        }

        let entry_args = build_entry_args(&mut thread, &vm, module_id, args)?;
        vm.prepare_function(&mut thread, module_id, entry_idx, vec![entry_args])
            .map_err(RuntimeError::VmPanic)?;

        let main_thread_id = self.spawn_thread(thread, executor.as_ref());
        let _ = self.registry.mark_running(main_thread_id);
        let main_thread = self.registry.take(main_thread_id).unwrap();

        let task = Box::new(task::RuntimeTask {
            thread_id: main_thread_id,
            thread: main_thread,
            vm,
            registry: sync::Arc::new(sync::Mutex::new(self.registry)),
            blocked: sync::Arc::new(sync::Mutex::new(self.blocked)),
            executor,
        });

        Ok(task)
    }
}

fn build_entry_args(
    thread: &mut galfus_vm::thread::VirtualThread,
    vm: &VirtualMachine,
    module_id: galfus_core::ModuleId,
    args: &[Vec<u8>],
) -> Result<VmValue, RuntimeError> {
    let uint8_ty = find_type(&vm.graph.get(module_id).unwrap().module, |ty| {
        matches!(ty, galfus_bytecode::BytecodeType::Uint8)
    })
    .ok_or(RuntimeError::MissingArgumentType("u8"))?;
    let byte_array_ty = vm
        .graph.get(module_id).unwrap().module
        .types
        .iter()
        .enumerate()
        .find(|(_, ty)| {
            matches!(ty, galfus_bytecode::BytecodeType::Array(element)
                if matches!(vm.graph.get(module_id).unwrap().module.types.get(element.raw() as usize), Some(galfus_bytecode::BytecodeType::Uint8)))
        })
        .map(|(index, _)| galfus_bytecode::instruction::TypeIdx(index as u16))
        .ok_or(RuntimeError::MissingArgumentType("[u8]"))?;
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

    let mut arg_values = Vec::with_capacity(args.len());
    for arg in args {
        let elements = arg.iter().copied().map(VmValue::Uint8).collect();
        let arg_ref = thread.heap.alloc(HeapObject::Array {
            element_ty: uint8_ty,
            elements,
        });
        arg_values.push(VmValue::Object(arg_ref));
    }

    let args_ref = thread.heap.alloc(HeapObject::Array {
        element_ty: byte_array_ty,
        elements: arg_values,
    });

    let _args_array_ty = args_array_ty;
    Ok(VmValue::Object(args_ref))
}

fn find_type(
    module: &BytecodeModule,
    predicate: impl Fn(&galfus_bytecode::BytecodeType) -> bool,
) -> Option<galfus_bytecode::instruction::TypeIdx> {
    module
        .types
        .iter()
        .position(predicate)
        .map(|index| galfus_bytecode::instruction::TypeIdx(index as u16))
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
