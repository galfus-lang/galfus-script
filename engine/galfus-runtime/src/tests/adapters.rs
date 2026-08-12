use super::*;
use galfus_bytecode::instruction::{ConstIdx, FuncIdx, Instruction, Reg, TypeIdx};
use galfus_bytecode::{
    BytecodeFunction, BytecodeGraph, BytecodeModule, BytecodeNode, BytecodeType, Constant,
    ConstantPool, ExportKind, ExportSlot, ImportEdge, PackageEntryPoint, PackageImage,
    PackageLoader, PackageMetadata,
};
use galfus_contract::LimitsMetadata;
use galfus_contract::{
    AdapterArtifact, AdapterBindings, AdapterLoadContext, AdapterLoadError, AdapterModuleBinding,
    AdapterModuleDescriptor, AdapterModuleLoader, AdapterModuleRequirement, AdapterTarget,
    BoundaryValue, CURRENT_BOUNDARY_ABI_VERSION, CancellationOutcome, ContentHash, ExecutionTarget,
    MessageInjector, ProviderModuleRequirement, Providers, RuntimeCapabilities,
    SelectedAdapterTarget, VerifiedAdapterArtifact,
};
use galfus_core::{HandleId, ModuleId, ModulePath, OpaqueTypeId, SemanticRevision, Version};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::thread::ThreadId;

#[derive(Default)]
struct DemoAdapterState {
    dispatch_threads: Vec<ThreadId>,
    completion_threads: Vec<ThreadId>,
    cancellations: Vec<(String, galfus_core::ThreadId, galfus_core::RequestId)>,
    releases: Vec<(String, u64)>,
    drops: usize,
}

struct DemoAdapter {
    state: Arc<Mutex<DemoAdapterState>>,
    complete: bool,
    descriptor: AdapterModuleDescriptor,
}

impl Drop for DemoAdapter {
    fn drop(&mut self) {
        self.state.lock().unwrap().drops += 1;
    }
}

impl AdapterModuleBinding for DemoAdapter {
    fn descriptor(&self) -> galfus_contract::AdapterModuleDescriptor {
        self.descriptor.clone()
    }

    fn dispatch(
        &mut self,
        symbol: &str,
        thread_id: galfus_core::ThreadId,
        request_lease: galfus_core::RequestLease,
        _args: &[BoundaryValue],
        injector: Arc<dyn MessageInjector>,
    ) {
        self.state
            .lock()
            .unwrap()
            .dispatch_threads
            .push(std::thread::current().id());
        if self.complete {
            let state = Arc::clone(&self.state);
            std::thread::spawn(move || {
                state
                    .lock()
                    .unwrap()
                    .completion_threads
                    .push(std::thread::current().id());
                let _ = injector.inject_system_response(
                    thread_id,
                    request_lease,
                    Ok(BoundaryValue::Handle {
                        type_id: OpaqueTypeId::new("graphics", "Texture").unwrap(),
                        binding_id: None,
                        id: HandleId::new(1),
                    }),
                );
            })
            .join()
            .expect("demo worker completes");
        }
        assert_eq!(symbol, "acquire");
    }

    fn cancel(
        &mut self,
        symbol: &str,
        thread_id: galfus_core::ThreadId,
        request_lease: galfus_core::RequestLease,
    ) -> CancellationOutcome {
        self.state.lock().unwrap().cancellations.push((
            symbol.to_string(),
            thread_id,
            request_lease.id,
        ));
        CancellationOutcome::Confirmed
    }

    fn release_handle(
        &mut self,
        type_id: &OpaqueTypeId,
        id: HandleId,
    ) -> Result<galfus_contract::HandleReleaseOutcome, galfus_contract::AdapterReleaseError> {
        self.state
            .lock()
            .unwrap()
            .releases
            .push((type_id.name().to_string(), u64::from(id.raw())));
        Ok(galfus_contract::HandleReleaseOutcome::Released)
    }
}

struct DemoAdapterLoader {
    state: Arc<Mutex<DemoAdapterState>>,
}

impl AdapterModuleLoader for DemoAdapterLoader {
    fn load_artifact(
        &self,
        _selected_target: &SelectedAdapterTarget,
        _context: &AdapterLoadContext,
    ) -> Result<Vec<u8>, AdapterLoadError> {
        Ok(b"demo-adapter".to_vec())
    }

    fn load_module(
        &self,
        requirement: &AdapterModuleRequirement,
        _selected_target: &SelectedAdapterTarget,
        artifact: VerifiedAdapterArtifact,
        _context: &AdapterLoadContext,
    ) -> Result<Box<dyn AdapterModuleBinding>, AdapterLoadError> {
        if artifact.as_bytes() != b"demo-adapter" {
            return Err(AdapterLoadError {
                code: "invalid_artifact".to_string(),
                message: "unexpected demo adapter artifact".to_string(),
            });
        }
        Ok(Box::new(DemoAdapter {
            state: Arc::clone(&self.state),
            complete: true,
            descriptor: requirement.descriptor.clone(),
        }))
    }
}

struct DeclaredProvider;

struct StaticPackageLoader(Arc<PackageImage>);

impl PackageLoader for StaticPackageLoader {
    type Error = std::convert::Infallible;

    fn load(&mut self) -> Result<Arc<PackageImage>, Self::Error> {
        Ok(Arc::clone(&self.0))
    }
}

impl galfus_contract::HostProvider for DeclaredProvider {
    fn descriptor(&self) -> galfus_contract::ProviderDescriptor {
        galfus_contract::std_io_provider_descriptor()
    }

    fn dispatch(
        &mut self,
        _thread_id: galfus_core::ThreadId,
        _request_lease: galfus_core::RequestLease,
        _name: &str,
        _args: &[BoundaryValue],
        _injector: Arc<dyn MessageInjector>,
    ) {
    }
}

fn demo_adapter_descriptor() -> AdapterModuleDescriptor {
    let target = ExecutionTarget::new("test").expect("valid target");
    let artifact = b"demo-adapter";
    AdapterModuleDescriptor {
        adapter: "demo".to_string(),
        config: Default::default(),
        targets: vec![AdapterTarget {
            target,
            locator: "memory://demo-adapter".to_string(),
            platform: "test".to_string(),
            abi: "1".to_string(),
            artifact: AdapterArtifact {
                content_hash: ContentHash::of(artifact),
                size_bytes: artifact.len() as u64,
                media_type: "application/x-galfus-demo".to_string(),
                content_version: Version::new(1, 0, 0),
            },
        }],
        exports: Vec::new(),
    }
}

fn adapter_graph() -> (Arc<BytecodeGraph>, ModuleId) {
    let module_id = ModuleId::new(1);
    let module = BytecodeModule {
        name: "main.gfs".to_string(),
        global_count: 0,
        constants: ConstantPool {
            constants: vec![
                Constant::String("graphics.gfp".to_string()),
                Constant::String("acquire".to_string()),
                Constant::Int32(0),
            ],
        },
        functions: vec![
            BytecodeFunction {
                name: "main".to_string(),
                param_count: 1,
                local_count: 0,
                temp_count: 2,
                return_ty: TypeIdx(1),
                adapter_proxy_metadata: None,
                instructions: vec![
                    Instruction::CreateFuture {
                        dest: Reg(1),
                        func: FuncIdx(1),
                        args_start: Reg(0),
                        arg_count: 0,
                        arg_types: vec![],
                        return_type: TypeIdx(0),
                    },
                    Instruction::AwaitFuture {
                        dest: Reg(2),
                        future_id: Reg(1),
                        return_type: TypeIdx(0),
                    },
                    Instruction::Drop { reg: Reg(2) },
                    Instruction::LoadConst {
                        dest: Reg(2),
                        const_idx: ConstIdx(2),
                    },
                    Instruction::Ret { src: Reg(2) },
                ],
            },
            BytecodeFunction {
                name: "acquire".to_string(),
                param_count: 0,
                local_count: 0,
                temp_count: 1,
                return_ty: TypeIdx(0),
                adapter_proxy_metadata: Some(galfus_bytecode::AdapterProxyMetadata {
                    proxy_module: "graphics.gfp".to_string(),
                    symbol: "acquire".to_string(),
                }),
                instructions: vec![Instruction::RetNull],
            },
        ],
        types: vec![
            BytecodeType::AdapterHandle(OpaqueTypeId::new("graphics", "Texture").unwrap()),
            BytecodeType::Int32,
            BytecodeType::Uint8,
            BytecodeType::Array(TypeIdx(2)),
            BytecodeType::Array(TypeIdx(3)),
        ],
        struct_layouts: vec![],
        choice_layouts: vec![],
        imports: vec![],
        exports: vec![ExportSlot {
            symbol_name: "main".to_string(),
            kind: ExportKind::Function(FuncIdx(0)),
        }],
        init_func_idx: None,
    };
    let graph = BytecodeGraph::from_modules(
        SemanticRevision::new(0),
        vec![
            BytecodeNode {
                id: module_id,
                path: ModulePath::new("main.gfs").unwrap(),
                semantic_revision: SemanticRevision::new(0),
                module,
                metadata: None,
            },
            BytecodeNode {
                id: ModuleId::new(2),
                path: ModulePath::new("graphics.gfp").unwrap(),
                semantic_revision: SemanticRevision::new(0),
                module: BytecodeModule {
                    name: "graphics.gfp".to_string(),
                    global_count: 0,
                    constants: ConstantPool::default(),
                    functions: vec![],
                    types: vec![],
                    struct_layouts: vec![],
                    choice_layouts: vec![],
                    imports: vec![],
                    exports: vec![],
                    init_func_idx: None,
                },
                metadata: None,
            },
        ],
        vec![ImportEdge {
            from: module_id,
            to: ModuleId::new(2),
        }],
    )
    .unwrap();
    (Arc::new(graph), module_id)
}

fn adapter_package(graph: Arc<BytecodeGraph>) -> Arc<PackageImage> {
    Arc::new(
        PackageImage::try_new(
            (*graph).clone(),
            ExecutionTarget::new("test").expect("valid target"),
            Some(PackageEntryPoint::new(
                ModulePath::new("main.gfs").expect("valid module path"),
                "main",
            )),
            galfus_bytecode::PackageMetadata {
                name: "test".into(),
                version: None,
                author: None,
                description: None,
            },
            galfus_contract::LimitsMetadata::default(),
            vec![AdapterModuleRequirement {
                proxy_module: "graphics.gfp".to_string(),
                descriptor: demo_adapter_descriptor(),
                boundary_abi: CURRENT_BOUNDARY_ABI_VERSION,
            }],
            Vec::new(),
        )
        .expect("package adapter requirement matches the reachable proxy"),
    )
}

fn adapter_package_with_provider(graph: Arc<BytecodeGraph>) -> Arc<PackageImage> {
    let provider = galfus_contract::std_io_provider_descriptor()
        .modules
        .into_iter()
        .next()
        .expect("std/io descriptor has a module");
    Arc::new(
        PackageImage::try_new(
            (*graph).clone(),
            ExecutionTarget::new("test").expect("valid target"),
            Some(PackageEntryPoint::new(
                ModulePath::new("main.gfs").expect("valid module path"),
                "main",
            )),
            galfus_bytecode::PackageMetadata {
                name: "test".into(),
                version: None,
                author: None,
                description: None,
            },
            galfus_contract::LimitsMetadata::default(),
            vec![AdapterModuleRequirement {
                proxy_module: "graphics.gfp".to_string(),
                descriptor: demo_adapter_descriptor(),
                boundary_abi: CURRENT_BOUNDARY_ABI_VERSION,
            }],
            vec![ProviderModuleRequirement {
                module_path: provider.module_path,
                schema_fingerprint: provider.schema_fingerprint,
                boundary_abi: provider.boundary_abi,
                exports: provider.exports,
            }],
        )
        .expect("complete package manifest"),
    )
}

fn execution_with_demo_adapter(complete: bool) -> (Execution, Arc<Mutex<DemoAdapterState>>) {
    let (graph, _) = adapter_graph();
    let state = Arc::new(Mutex::new(DemoAdapterState::default()));
    let mut bindings = AdapterBindings::default();
    bindings
        .register_module(
            "graphics.gfp",
            Box::new(DemoAdapter {
                state: Arc::clone(&state),
                complete,
                descriptor: demo_adapter_descriptor(),
            }),
        )
        .expect("adapter binding registers");
    let package = adapter_package(graph);
    let execution = Runtime::new(
        package,
        RuntimeCapabilities::builder()
            .with_adapter_bindings(bindings)
            .build(),
    )
    .start(&[], Rc::new(CooperativeDriver::new()))
    .unwrap();
    (execution, state)
}

#[test]
fn runtime_rejects_a_missing_required_adapter_before_execution() {
    let (graph, _) = adapter_graph();
    let result = Runtime::new(
        adapter_package(graph),
        RuntimeCapabilities::builder().build(),
    )
    .start(&[], Rc::new(CooperativeDriver::new()));

    assert!(matches!(
        result,
        Err(RuntimeError::AdapterRequirementUnsatisfied { proxy_module })
            if proxy_module == "graphics.gfp"
    ));
}

#[test]
fn execution_host_bootstraps_a_compiled_package_with_provider_and_adapter() {
    let (graph, _) = adapter_graph();
    let mut loader = StaticPackageLoader(adapter_package_with_provider(graph));
    let package = loader.load().expect("package image is available");
    let context = AdapterLoadContext {
        target: ExecutionTarget::new("test").expect("valid target"),
        properties: Default::default(),
    };
    let driver = Rc::new(CooperativeDriver::new());

    let missing_loader = ExecutionHost::new(AdapterLoadContext {
        target: context.target.clone(),
        properties: Default::default(),
    })
    .with_providers(Providers::with_host(Box::new(DeclaredProvider)))
    .start(Arc::clone(&package), &[], driver.clone());
    assert!(matches!(
        missing_loader,
        Err(HostBootstrapError::Preflight(PreflightError::MissingLoader(adapter)))
            if adapter == "demo"
    ));

    let state = Arc::new(Mutex::new(DemoAdapterState::default()));
    let mut host = ExecutionHost::new(context)
        .with_providers(Providers::with_host(Box::new(DeclaredProvider)));
    host.register_adapter_loader(
        "demo",
        Box::new(DemoAdapterLoader {
            state: Arc::clone(&state),
        }),
    )
    .expect("loader registers");

    let mut execution = host
        .start(package, &[], driver)
        .expect("compiled package bootstraps through the host");
    assert_eq!(
        execution.run_sync_to_completion(),
        Ok(BoundaryValue::I32(0))
    );
    assert_eq!(
        state.lock().unwrap().releases,
        vec![("Texture".to_string(), 1)]
    );
}

#[test]
fn demo_adapter_completes_from_a_worker_and_releases_its_handle_once() {
    let main_thread = std::thread::current().id();
    let (mut execution, state) = execution_with_demo_adapter(true);

    assert_eq!(
        execution.run_sync_to_completion(),
        Ok(BoundaryValue::I32(0))
    );
    let state = state.lock().unwrap();
    assert_eq!(state.dispatch_threads, vec![main_thread]);
    assert_eq!(state.completion_threads.len(), 1);
    assert_ne!(state.completion_threads[0], main_thread);
    assert_eq!(state.releases, vec![("Texture".to_string(), 1)]);
}

#[test]
fn repeated_async_adapter_executions_return_to_the_resource_baseline() {
    const CYCLES: usize = 128;

    for _ in 0..CYCLES {
        let (mut execution, state) = execution_with_demo_adapter(true);
        assert_eq!(
            execution.run_sync_to_completion(),
            Ok(BoundaryValue::I32(0))
        );
        let state = state.lock().unwrap();
        assert_eq!(state.releases, vec![("Texture".to_string(), 1)]);
        assert_eq!(state.drops, 1);
    }
}

#[test]
fn cancelling_a_pending_demo_adapter_call_targets_its_exact_symbol_once() {
    let (mut execution, state) = execution_with_demo_adapter(false);
    for _ in 0..8 {
        let _ = execution.poll(100);
        if !state.lock().unwrap().dispatch_threads.is_empty() {
            break;
        }
    }
    assert_eq!(state.lock().unwrap().dispatch_threads.len(), 1);

    execution.cancel();
    let _ = execution.poll(100);
    let state = state.lock().unwrap();
    assert_eq!(state.cancellations.len(), 1);
    assert_eq!(state.cancellations[0].0, "acquire");
}
