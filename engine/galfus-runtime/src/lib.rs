//! Galfus Runtime
//!
//! See the Runtime Ownership Matrix in the Architecture Reference (`docs/Galfus_Architecture_Reference.md`)
//! for authoritative details on the lifecycle and ownership of runtime entities.

#![allow(clippy::result_large_err)]
#![allow(clippy::type_complexity)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::large_enum_variant)]

pub mod driver;
pub mod event;
pub mod execution;
pub mod execution_host;
mod kernel;
mod orchestrator;
pub mod preflight;
pub mod queue;
pub mod registry;
pub mod task;
#[cfg(test)]
mod tests;

use std::collections::VecDeque;
use std::rc::Rc;
use std::sync;

use crate::driver::ExecutionDriver;
use galfus_contract::{
    AdapterBindings, BoundaryType, BoundaryValue, Providers, RuntimeCapabilities,
    validate_numeric_semantics,
};
use galfus_vm::{VirtualMachine, VmPanic, VmValue};

pub use driver::CooperativeDriver;
pub use execution::{
    CancellationReport, CompletionMetrics, Execution, ExecutionHandle, ExecutionState,
    ShutdownReport,
};
pub use execution_host::{ExecutionHost, HostBootstrapError};
pub use preflight::{AdapterBindingPreflight, PreflightError};

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("execution driver cannot provide the requested event queue capacity {requested}")]
    EventQueueCapacityExceeded { requested: usize },
    #[error("package has no configured entry point")]
    MissingPackageEntry,
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
    #[error("required provider module `{module_path}` is unavailable or incompatible")]
    ProviderRequirementUnsatisfied { module_path: String },
    #[error("required adapter proxy module `{proxy_module}` is unavailable or incompatible")]
    AdapterRequirementUnsatisfied { proxy_module: String },
    #[error("package numeric semantics are incompatible: {0}")]
    NumericSemantics(galfus_contract::PackageCompatibilityError),
    #[error(transparent)]
    BytecodeFormat(#[from] galfus_bytecode::BytecodeFormatError),
    #[error(transparent)]
    GraphResolution(#[from] galfus_bytecode::GraphResolutionError),
    #[error(transparent)]
    GraphValidation(#[from] galfus_bytecode::BytecodeGraphValidationErrors),
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

/// A single execution composed from one package image and optional host providers.
pub struct Runtime {
    package: sync::Arc<galfus_bytecode::PackageImage>,
    capabilities: RuntimeCapabilities,
}

impl Runtime {
    pub fn new(
        package: sync::Arc<galfus_bytecode::PackageImage>,
        capabilities: RuntimeCapabilities,
    ) -> Self {
        Self {
            package,
            capabilities,
        }
    }

    /// Starts a persistent execution from the package entry point.
    pub fn start(
        self,
        args: &[Vec<u8>],
        driver: Rc<dyn ExecutionDriver>,
    ) -> Result<Execution, RuntimeError> {
        let Runtime {
            package,
            capabilities,
        } = self;
        let (providers, adapter_bindings) = capabilities.into_runtime_handles();
        package.graph().validate_format()?;
        package.graph().validate()?;
        validate_numeric_semantics(package.versions().numeric_semantics())
            .map_err(RuntimeError::NumericSemantics)?;
        preflight_capabilities(&package, providers.as_ref(), &adapter_bindings)?;
        driver.configure_limits(package.limits()).map_err(|_| {
            RuntimeError::EventQueueCapacityExceeded {
                requested: package.limits().max_event_queue,
            }
        })?;

        let quota = std::sync::Arc::new(std::sync::Mutex::new(galfus_vm::quota::GlobalQuota::new(
            package.limits().clone(),
        )));
        let mut orchestrator = crate::orchestrator::Orchestrator::new(quota.clone());
        let entry = package
            .entry_point()
            .ok_or(RuntimeError::MissingPackageEntry)?;
        let graph = sync::Arc::new(package.graph().clone());
        let module_id = graph
            .modules()
            .find(|module| module.path() == entry.module_path())
            .map(|module| module.id())
            .ok_or_else(|| {
                RuntimeError::ModuleNotLoaded(entry.module_path().as_str().to_string())
            })?;
        let entry_name = entry.function_name();
        let image = &graph
            .get(module_id)
            .expect("entry module was resolved from the graph")
            .module;
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

        let thread_quota = std::sync::Arc::new(std::sync::Mutex::new(
            galfus_vm::quota::ThreadQuota::new(package.limits().clone()),
        ));
        let mut thread = galfus_vm::thread::VmThreadState::new(quota.clone(), thread_quota);
        let vm = VirtualMachine::new(graph.clone()).with_provider_handle(providers);

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

        let root_thread_id = orchestrator
            .kernel_mut()
            .spawn(thread, None)
            .expect("failed to spawn root thread");
        orchestrator.set_root_thread(root_thread_id);

        let is_initializing = startup_plan.is_some();
        if let Some(startup_plan) = startup_plan {
            orchestrator.set_startup_plan(root_thread_id, startup_plan);
        }

        let _ = orchestrator.kernel_mut().mark_running(root_thread_id);
        let root_thread = orchestrator
            .kernel_mut()
            .take_thread(root_thread_id)
            .unwrap();

        let vm = sync::Arc::new(vm);

        orchestrator.set_vm(vm);
        orchestrator.set_adapter_bindings(Some(adapter_bindings));
        orchestrator.set_driver(driver.clone());
        orchestrator
            .kernel_mut()
            .enqueue_runnable(root_thread_id, root_thread)
            .unwrap();

        let initialization_complete = orchestrator.initialization_complete();
        Ok(Execution::new(
            orchestrator,
            driver,
            initialization_complete,
            is_initializing,
        ))
    }
}

fn preflight_capabilities(
    package: &galfus_bytecode::PackageImage,
    providers: Option<&sync::Arc<sync::Mutex<Providers>>>,
    adapter_bindings: &sync::Arc<sync::Mutex<AdapterBindings>>,
) -> Result<(), RuntimeError> {
    let bindings = adapter_bindings
        .lock()
        .expect("runtime owns the adapter capability table");
    for requirement in package.adapter_requirements() {
        if !bindings.validates(requirement) {
            return Err(RuntimeError::AdapterRequirementUnsatisfied {
                proxy_module: requirement.proxy_module.clone(),
            });
        }
    }
    drop(bindings);

    for requirement in package.provider_requirements() {
        let is_satisfied = providers
            .and_then(|providers| providers.lock().ok())
            .is_some_and(|providers| providers.validates(requirement));
        if !is_satisfied {
            return Err(RuntimeError::ProviderRequirementUnsatisfied {
                module_path: requirement.module_path.clone(),
            });
        }
    }

    Ok(())
}

fn build_entry_args(
    thread: &mut galfus_vm::thread::VmThreadState,
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
                .and_then(|metadata| {
                    metadata.location_for(frame.func_idx, frame.instruction_offset)
                })
                .map(|location| {
                    format!(
                        "instruction {} at {}:{}..{}",
                        frame.instruction_offset,
                        module.path.as_str(),
                        location.start(),
                        location.end()
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
