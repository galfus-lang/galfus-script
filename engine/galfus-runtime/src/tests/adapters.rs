use super::*;
use galfus_bytecode::instruction::{ConstIdx, FuncIdx, Instruction, Reg, TypeIdx};
use galfus_bytecode::{
    BytecodeFunction, BytecodeGraph, BytecodeModule, BytecodeNode, BytecodeType, Constant,
    ConstantPool, ExportKind, ExportSlot,
};
use galfus_contract::{
    AdapterBindings, AdapterModuleBinding, BoundaryValue, CancellationOutcome, MessageInjector,
};
use galfus_core::{ModuleId, ModulePath, SemanticRevision};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::thread::ThreadId;

#[derive(Default)]
struct DemoAdapterState {
    dispatch_threads: Vec<ThreadId>,
    completion_threads: Vec<ThreadId>,
    cancellations: Vec<(String, usize, u64)>,
    releases: Vec<(String, u64)>,
}

struct DemoAdapter {
    state: Arc<Mutex<DemoAdapterState>>,
    complete: bool,
}

impl AdapterModuleBinding for DemoAdapter {
    fn dispatch(
        &mut self,
        symbol: &str,
        _thread_id: usize,
        _request_id: u64,
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
                    0,
                    0,
                    Ok(BoundaryValue::Handle {
                        proxy_module: None,
                        kind: "graphics::Texture".to_string(),
                        id: 7,
                    }),
                );
            })
            .join()
            .expect("demo worker completes");
        }
        assert_eq!(symbol, "acquire");
    }

    fn cancel(&mut self, symbol: &str, thread_id: usize, request_id: u64) -> CancellationOutcome {
        self.state
            .lock()
            .unwrap()
            .cancellations
            .push((symbol.to_string(), thread_id, request_id));
        CancellationOutcome::Confirmed
    }

    fn release_handle(&mut self, kind: &str, id: u64) {
        self.state
            .lock()
            .unwrap()
            .releases
            .push((kind.to_string(), id));
    }
}

fn adapter_graph() -> (Arc<BytecodeGraph>, ModuleId) {
    let module_id = ModuleId::new(1);
    let module = BytecodeModule {
        name: "main.gfs".to_string(),
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
            BytecodeType::AdapterHandle("graphics::Texture".to_string()),
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
        vec![BytecodeNode {
            id: module_id,
            path: ModulePath::new("main.gfs").unwrap(),
            semantic_revision: SemanticRevision::new(0),
            module,
            metadata: None,
        }],
        vec![],
    )
    .unwrap();
    (Arc::new(graph), module_id)
}

fn execution_with_demo_adapter(complete: bool) -> (Execution, Arc<Mutex<DemoAdapterState>>) {
    let (graph, module_id) = adapter_graph();
    let state = Arc::new(Mutex::new(DemoAdapterState::default()));
    let mut bindings = AdapterBindings::default();
    bindings.register_module(
        "graphics.gfp",
        Box::new(DemoAdapter {
            state: Arc::clone(&state),
            complete,
        }),
    );
    let execution = Runtime::new(graph, None)
        .with_adapter_bindings(bindings)
        .start(module_id, "main", &[], Rc::new(CooperativeDriver::new()))
        .unwrap();
    (execution, state)
}

#[test]
fn demo_adapter_completes_from_a_worker_and_releases_its_handle_once() {
    let main_thread = std::thread::current().id();
    let (mut execution, state) = execution_with_demo_adapter(true);

    assert_eq!(execution.run_to_completion(), Ok(BoundaryValue::I32(0)));
    let state = state.lock().unwrap();
    assert_eq!(state.dispatch_threads, vec![main_thread]);
    assert_eq!(state.completion_threads.len(), 1);
    assert_ne!(state.completion_threads[0], main_thread);
    assert_eq!(state.releases, vec![("graphics::Texture".to_string(), 7)]);
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
