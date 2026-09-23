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
mod module_resolver;
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
use crate::module_resolver::ChunkModuleProducer;
use crate::preflight::{
    CapabilityValidatedModuleProducer, LazyCapabilityPreflight, validate_package_capabilities,
};
use galfus_bytecode::{
    BytecodeType, CURRENT_BYTECODE_FORMAT_VERSION, ExportKind, ModuleCatalog, ModuleResolveContext,
    ModuleResolveError, PackageEntryPoint, validate_bytecode_format,
};
use galfus_contract::{
    AdapterBindings, AdapterModuleRequirement, CURRENT_NUMERIC_SEMANTICS_VERSION, LimitsMetadata,
    ProviderModuleRequirement, Providers, RuntimeCapabilities, validate_numeric_semantics,
};
use galfus_vm::{VirtualMachine, VmPanic, VmValue};

pub use driver::CooperativeDriver;
#[cfg(feature = "metrics")]
pub use execution::FutureMetrics;
pub use execution::{
    CancellationReport, CompletionMetrics, Execution, ExecutionHandle, ExecutionState,
    ShutdownReport,
};
pub use execution_host::{ExecutionHost, HostBootstrapError};
pub use module_resolver::{ModuleInitializationPlanError, ModuleProducer, ModuleResolver};
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
    #[error("workspace capability declaration is invalid: {reason}")]
    SourceCapabilityConfiguration { reason: String },
    #[error("package numeric semantics are incompatible: {0}")]
    NumericSemantics(galfus_contract::PackageCompatibilityError),
    #[error(transparent)]
    EagerModuleResolution(#[from] ModuleResolveError),
    #[error("module initialization dependency cycle: {cycle:?}")]
    InitializationDependencyCycle { cycle: Vec<galfus_core::ModuleId> },
    #[error(transparent)]
    BytecodeFormat(#[from] galfus_bytecode::BytecodeFormatError),
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

/// Runtime inputs available from a checked workspace before module bodies exist.
#[derive(Clone)]
pub struct SourceRuntimeConfiguration {
    catalog: sync::Arc<ModuleCatalog>,
    entry_point: PackageEntryPoint,
    limits: LimitsMetadata,
    adapter_requirements: Vec<AdapterModuleRequirement>,
    provider_requirements: Vec<ProviderModuleRequirement>,
}

impl SourceRuntimeConfiguration {
    pub fn new(
        catalog: sync::Arc<ModuleCatalog>,
        entry_point: PackageEntryPoint,
        limits: LimitsMetadata,
        adapter_requirements: Vec<AdapterModuleRequirement>,
        provider_requirements: Vec<ProviderModuleRequirement>,
    ) -> Self {
        Self {
            catalog,
            entry_point,
            limits,
            adapter_requirements,
            provider_requirements,
        }
    }
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

    /// Starts a standalone execution after preflighting every package capability.
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
        validate_bytecode_format(package.versions().bytecode_format())?;
        validate_numeric_semantics(package.versions().numeric_semantics())
            .map_err(RuntimeError::NumericSemantics)?;
        validate_package_capabilities(
            package.adapter_requirements(),
            package.provider_requirements(),
            providers.as_ref(),
            &adapter_bindings,
        )?;
        driver.configure_limits(package.limits()).map_err(|_| {
            RuntimeError::EventQueueCapacityExceeded {
                requested: package.limits().max_event_queue,
            }
        })?;

        let module_resolver = create_eager_module_resolver(
            sync::Arc::new(package.catalog().clone()),
            sync::Arc::new(package.chunks().clone()),
        )?;
        start_with_module_resolver(
            sync::Arc::new(package.catalog().clone()),
            package
                .entry_point()
                .ok_or(RuntimeError::MissingPackageEntry)?,
            package.limits(),
            providers,
            adapter_bindings,
            module_resolver,
            args,
            driver,
        )
    }

    /// Starts from a checked source catalog without preloading module bodies.
    pub fn start_with_source_producer(
        configuration: SourceRuntimeConfiguration,
        producer: sync::Arc<dyn ModuleProducer>,
        capabilities: RuntimeCapabilities,
        args: &[Vec<u8>],
        driver: Rc<dyn ExecutionDriver>,
    ) -> Result<Execution, RuntimeError> {
        validate_bytecode_format(CURRENT_BYTECODE_FORMAT_VERSION)?;
        validate_numeric_semantics(CURRENT_NUMERIC_SEMANTICS_VERSION)
            .map_err(RuntimeError::NumericSemantics)?;
        let (providers, adapter_bindings) = capabilities.into_runtime_handles();
        let capability_preflight = LazyCapabilityPreflight::new(
            configuration.catalog.as_ref(),
            configuration.adapter_requirements.as_slice(),
            configuration.provider_requirements.as_slice(),
            providers.clone(),
            adapter_bindings.clone(),
        )?;
        driver
            .configure_limits(&configuration.limits)
            .map_err(|_| RuntimeError::EventQueueCapacityExceeded {
                requested: configuration.limits.max_event_queue,
            })?;
        let resolver = sync::Arc::new(ModuleResolver::new(
            configuration.catalog.as_ref(),
            sync::Arc::new(CapabilityValidatedModuleProducer::new(
                producer,
                capability_preflight,
            )),
        ));
        start_with_module_resolver(
            configuration.catalog,
            &configuration.entry_point,
            &configuration.limits,
            providers,
            adapter_bindings,
            resolver,
            args,
            driver,
        )
    }
}

fn create_eager_module_resolver(
    catalog: sync::Arc<ModuleCatalog>,
    chunks: sync::Arc<galfus_bytecode::ModuleChunkStore>,
) -> Result<sync::Arc<ModuleResolver>, RuntimeError> {
    let producer: sync::Arc<dyn ModuleProducer> =
        sync::Arc::new(ChunkModuleProducer::new(chunks, catalog.clone()));
    let resolver = sync::Arc::new(ModuleResolver::new(catalog.as_ref(), producer));
    resolver.preload_all()?;
    Ok(resolver)
}

#[allow(clippy::too_many_arguments)]
fn start_with_module_resolver(
    catalog: sync::Arc<ModuleCatalog>,
    entry: &PackageEntryPoint,
    limits: &LimitsMetadata,
    providers: Option<sync::Arc<sync::Mutex<Providers>>>,
    adapter_bindings: sync::Arc<sync::Mutex<AdapterBindings>>,
    module_resolver: sync::Arc<ModuleResolver>,
    args: &[Vec<u8>],
    driver: Rc<dyn ExecutionDriver>,
) -> Result<Execution, RuntimeError> {
    let quota = sync::Arc::new(sync::Mutex::new(galfus_vm::quota::GlobalQuota::new(
        limits.clone(),
    )));
    let mut orchestrator = crate::orchestrator::Orchestrator::new(quota.clone());
    let module_id = catalog
        .iter()
        .find(|descriptor| descriptor.module_path() == entry.module_path())
        .map(|descriptor| descriptor.module_id())
        .ok_or_else(|| RuntimeError::ModuleNotLoaded(entry.module_path().as_str().to_string()))?;
    let entry_name = entry.function_name();
    let entry_module = module_resolver.ensure_module(module_id)?;
    let image = entry_module.module();
    let abi = EntryAbi::default_app();
    let entry_idx = image
        .exports
        .iter()
        .find(|export| export.symbol_name == entry_name)
        .and_then(|export| match export.kind {
            ExportKind::Function(f) => Some(f),
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

    let thread_quota = sync::Arc::new(galfus_vm::quota::ThreadQuota::new(limits.clone()));
    let mut thread = galfus_vm::thread::VmThreadState::new(quota.clone(), thread_quota);
    let mut initializers = VecDeque::new();
    let initialization_plan = module_resolver
        .initialization_plan(module_id)
        .map_err(|error| match error {
            ModuleInitializationPlanError::UnknownModule { module_id } => {
                RuntimeError::EagerModuleResolution(ModuleResolveError::UnknownModule {
                    context: ModuleResolveContext::new(module_id),
                })
            }
            ModuleInitializationPlanError::DependencyCycle { cycle } => {
                RuntimeError::InitializationDependencyCycle { cycle }
            }
        })?;
    for initialized_module_id in initialization_plan {
        if thread.is_module_initialized(initialized_module_id) {
            continue;
        }
        if let Some(init_idx) = module_resolver
            .ensure_module(initialized_module_id)?
            .module
            .init_func_idx
        {
            initializers.push_back((initialized_module_id, init_idx));
        } else {
            thread.mark_module_initialized(initialized_module_id);
        }
    }

    let vm = VirtualMachine::from_ready_modules(module_resolver.loaded_modules())
        .with_provider_handle(providers);

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

    orchestrator.set_vm(sync::Arc::new(vm));
    orchestrator.set_module_resolver(module_resolver.clone());
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
    )
    .with_module_resolver(module_resolver))
}

fn build_entry_args(
    thread: &mut galfus_vm::thread::VmThreadState,
    vm: &VirtualMachine,
    module_id: galfus_core::ModuleId,
    args: &[Vec<u8>],
) -> Result<VmValue, RuntimeError> {
    let module = vm.get_module(module_id).map_err(|error| {
        RuntimeError::VmPanic(VmPanic {
            error,
            stack_trace: Vec::new(),
        })
    })?;
    let args_array_ty = module
        .types
        .iter()
        .enumerate()
        .find(|(_, ty)| {
            matches!(ty, BytecodeType::Array(element)
                if matches!(module.types.get(element.raw() as usize), Some(BytecodeType::Array(inner))
                    if matches!(module.types.get(inner.raw() as usize), Some(BytecodeType::Uint8))))
        })
        .map(|(index, _)| galfus_bytecode::instruction::TypeIdx(index as u16))
        .ok_or(RuntimeError::MissingArgumentType("[[u8]]"))?;

    let value = galfus_contract::SurfaceValue::List(
        args.iter()
            .map(|arg| galfus_contract::SurfaceValue::Bytes(arg.clone()))
            .collect(),
    );
    crate::task::encode_surface_into_thread_heap(
        &mut thread.heap,
        &galfus_contract::SurfaceSchema::List(Box::new(galfus_contract::SurfaceSchema::Bytes)),
        value,
        args_array_ty,
        module_id,
        module,
        None,
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
