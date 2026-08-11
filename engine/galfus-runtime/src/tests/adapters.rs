use super::*;
use galfus_bytecode::instruction::{ConstIdx, FuncIdx, Instruction, Reg, TypeIdx};
use galfus_bytecode::{
    BytecodeFunction, BytecodeGraph, BytecodeModule, BytecodeNode, BytecodeType, Constant,
    ConstantPool, ExportKind, ExportSlot, ImportEdge, PackageEntryPoint, PackageImage,
};
use galfus_contract::{
    AdapterBindings, AdapterModuleBinding, AdapterModuleDescriptor, AdapterModuleRequirement,
    BoundaryValue, CURRENT_BOUNDARY_ABI_VERSION, CancellationOutcome, ExecutionTarget,
    MessageInjector, RuntimeCapabilities,
};
use galfus_core::{HandleId, ModuleId, ModulePath, OpaqueTypeId, SemanticRevision};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::thread::ThreadId;

#[derive(Default)]
struct DemoAdapterState {
    dispatch_threads: Vec<ThreadId>,
    completion_threads: Vec<ThreadId>,
    cancellations: Vec<(String, galfus_core::ThreadId, galfus_core::RequestId)>,
    releases: Vec<(String, u64)>,
}

struct DemoAdapter {
    state: Arc<Mutex<DemoAdapterState>>,
    complete: bool,
}

impl AdapterModuleBinding for DemoAdapter {
    fn descriptor(&self) -> galfus_contract::AdapterModuleDescriptor {
        galfus_contract::AdapterModuleDescriptor::empty()
    }

    fn dispatch(
        &mut self,
        symbol: &str,
        _thread_id: galfus_core::ThreadId,
        _request_lease: galfus_core::RequestLease,
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
                injector.inject_system_response(
                    galfus_core::ThreadId::new(0),
                    galfus_core::RequestLease::new(galfus_core::RequestId::new(0), 1),
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
            vec![AdapterModuleRequirement {
                proxy_module: "graphics.gfp".to_string(),
                descriptor: AdapterModuleDescriptor::empty(),
                boundary_abi: CURRENT_BOUNDARY_ABI_VERSION,
            }],
            Vec::new(),
        )
        .expect("package adapter requirement matches the reachable proxy"),
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
