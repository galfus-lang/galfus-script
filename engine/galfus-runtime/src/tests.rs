mod adapters;

use std::collections;
use std::sync;

use super::*;
use galfus_bytecode::instruction::{ConstIdx, FuncIdx, GlobalIdx, Instruction, Reg, TypeIdx};
use galfus_bytecode::{
    BytecodeFunction, BytecodeGraph, BytecodeModule, BytecodeNode, BytecodeType, Constant,
    ConstantPool, ExportSlot, ImportEdge, ImportSlot,
};
use galfus_core::{ModuleId, ModulePath, SemanticRevision};

struct StartupProvider {
    calls: sync::Arc<sync::Mutex<Vec<String>>>,
    pending: sync::Arc<
        sync::Mutex<Option<(usize, u64, sync::Arc<dyn galfus_contract::MessageInjector>)>>,
    >,
    fail_initializer: bool,
}

impl galfus_contract::HostProvider for StartupProvider {
    fn dispatch(
        &mut self,
        thread_id: usize,
        request_id: u64,
        name: &str,
        _args: &[galfus_contract::BoundaryValue],
        injector: sync::Arc<dyn galfus_contract::MessageInjector>,
    ) {
        self.calls.lock().unwrap().push(name.to_string());
        if name == "initialize" && self.fail_initializer {
            injector.inject_system_response(
                thread_id,
                request_id,
                Err(galfus_contract::ExecutionFailure::new(
                    galfus_contract::ExecutionFailureKind::ProviderFailure,
                    "initializer rejected",
                )),
            );
        } else if name == "initialize" {
            *self.pending.lock().unwrap() = Some((thread_id, request_id, injector));
        } else {
            injector.inject_system_response(
                thread_id,
                request_id,
                Ok(galfus_contract::BoundaryValue::Null),
            );
        }
    }
}

fn startup_graph() -> (sync::Arc<BytecodeGraph>, ModuleId) {
    let module_id = ModuleId::new(1);
    let module = BytecodeModule {
        name: "main.gfs".to_string(),
        constants: ConstantPool {
            constants: vec![
                Constant::String("initialize".to_string()),
                Constant::String("entry".to_string()),
                Constant::Int32(42),
            ],
        },
        functions: vec![
            BytecodeFunction {
                name: "__init_module".to_string(),
                param_count: 0,
                local_count: 0,
                temp_count: 1,
                return_ty: TypeIdx(0),
                proxy_metadata: None,
                instructions: vec![
                    Instruction::CreateFuture {
                        dest: Reg(0),
                        func: FuncIdx(2),
                        args_start: Reg(0),
                        arg_count: 0,
                        arg_types: vec![],
                        return_type: TypeIdx(0),
                    },
                    Instruction::AwaitFuture {
                        dest: Reg(0),
                        future_id: Reg(0),
                        return_type: TypeIdx(0),
                    },
                    Instruction::RetNull,
                ],
            },
            BytecodeFunction {
                name: "main".to_string(),
                param_count: 1,
                local_count: 0,
                temp_count: 1,
                return_ty: TypeIdx(4),
                proxy_metadata: None,
                instructions: vec![
                    Instruction::CreateFuture {
                        dest: Reg(1),
                        func: FuncIdx(3),
                        args_start: Reg(0),
                        arg_count: 0,
                        arg_types: vec![],
                        return_type: TypeIdx(0),
                    },
                    Instruction::AwaitFuture {
                        dest: Reg(1),
                        future_id: Reg(1),
                        return_type: TypeIdx(0),
                    },
                    Instruction::LoadConst {
                        dest: Reg(1),
                        const_idx: ConstIdx(2),
                    },
                    Instruction::Ret { src: Reg(1) },
                ],
            },
            BytecodeFunction {
                name: "__provider_initialize".to_string(),
                param_count: 0,
                local_count: 0,
                temp_count: 0,
                return_ty: TypeIdx(0),
                proxy_metadata: None,
                instructions: vec![Instruction::RetNull],
            },
            BytecodeFunction {
                name: "__provider_entry".to_string(),
                param_count: 0,
                local_count: 0,
                temp_count: 0,
                return_ty: TypeIdx(0),
                proxy_metadata: None,
                instructions: vec![Instruction::RetNull],
            },
        ],
        types: vec![
            BytecodeType::Null,
            BytecodeType::Uint8,
            BytecodeType::Array(TypeIdx(1)),
            BytecodeType::Array(TypeIdx(2)),
            BytecodeType::Int32,
        ],
        struct_layouts: vec![],
        choice_layouts: vec![],
        imports: vec![],
        exports: vec![ExportSlot {
            symbol_name: "main".to_string(),
            kind: galfus_bytecode::ExportKind::Function(FuncIdx(1)),
        }],
        init_func_idx: Some(FuncIdx(0)),
    };
    let graph = BytecodeGraph::from_modules(
        SemanticRevision::new(0),
        vec![node(module_id, "main.gfs", module)],
        vec![],
    )
    .expect("valid startup graph");
    (sync::Arc::new(graph), module_id)
}

fn start_with_provider(provider: StartupProvider) -> Execution {
    let (graph, module_id) = startup_graph();
    Runtime::new(
        graph,
        Some(galfus_contract::Providers::with_host(Box::new(provider))),
    )
    .start(
        module_id,
        "main",
        &[],
        std::rc::Rc::new(CooperativeDriver::new()),
    )
    .expect("startup execution is created")
}

#[test]
fn pending_initializer_delays_entry_until_its_completion() {
    let calls = sync::Arc::new(sync::Mutex::new(vec![]));
    let pending = sync::Arc::new(sync::Mutex::new(None));
    let mut execution = start_with_provider(StartupProvider {
        calls: calls.clone(),
        pending: pending.clone(),
        fail_initializer: false,
    });
    assert_eq!(execution.status(), ExecutionState::Initializing);

    for _ in 0..4 {
        if !calls.lock().unwrap().is_empty() {
            break;
        }
        execution.poll(100).expect("startup polling succeeds");
    }
    assert_eq!(*calls.lock().unwrap(), vec!["initialize"]);
    let (thread_id, request_id, injector) = pending
        .lock()
        .unwrap()
        .take()
        .expect("initializer is pending");
    injector.inject_system_response(
        thread_id + 1,
        request_id,
        Ok(galfus_contract::BoundaryValue::Null),
    );
    execution
        .poll(100)
        .expect("cross-thread completion is ignored safely");
    assert_eq!(*calls.lock().unwrap(), vec!["initialize"]);
    injector.inject_system_response(
        thread_id,
        request_id,
        Ok(galfus_contract::BoundaryValue::Null),
    );

    assert_eq!(
        execution.run_to_completion(),
        Ok(galfus_contract::BoundaryValue::I32(42))
    );
    assert_eq!(execution.status(), ExecutionState::Completed);
    assert_eq!(
        execution.result(),
        Some(&Ok(galfus_contract::BoundaryValue::I32(42)))
    );
    assert_eq!(*calls.lock().unwrap(), vec!["initialize", "entry"]);
}

#[test]
fn initializer_failure_preserves_the_provider_failure_as_its_cause() {
    let calls = sync::Arc::new(sync::Mutex::new(vec![]));
    let mut execution = start_with_provider(StartupProvider {
        calls,
        pending: sync::Arc::new(sync::Mutex::new(None)),
        fail_initializer: true,
    });

    let error = execution
        .run_to_completion()
        .expect_err("initializer failure stops startup");
    assert_eq!(
        error.kind,
        galfus_contract::ExecutionFailureKind::InitializationFailure
    );
    assert_eq!(
        error.cause.as_ref().map(|cause| &cause.kind),
        Some(&galfus_contract::ExecutionFailureKind::ProviderFailure)
    );
    assert_eq!(
        error.cause.as_ref().map(|cause| cause.message.as_str()),
        Some("initializer rejected")
    );
}

fn node(id: ModuleId, path: &str, module: BytecodeModule) -> BytecodeNode {
    BytecodeNode {
        id,
        path: ModulePath::new(path).expect("valid module path"),
        semantic_revision: SemanticRevision::new(0),
        module,
        metadata: None,
    }
}

#[test]
fn run_initializes_dependencies_before_the_entry_module() {
    let dependency_id = ModuleId::new(1);
    let entry_id = ModuleId::new(2);
    let dependency = BytecodeModule {
        name: "dependency.gfs".to_string(),
        constants: ConstantPool {
            constants: vec![Constant::Int32(42)],
        },
        functions: vec![BytecodeFunction {
            name: "__init_module".to_string(),
            param_count: 0,
            local_count: 0,
            temp_count: 1,
            return_ty: TypeIdx(1),
            proxy_metadata: None,
            instructions: vec![
                Instruction::LoadConst {
                    dest: Reg(0),
                    const_idx: ConstIdx(0),
                },
                Instruction::StoreGlobal {
                    module_id: dependency_id,
                    global_idx: GlobalIdx(0),
                    src: Reg(0),
                },
                Instruction::RetNull,
            ],
        }],
        types: vec![BytecodeType::Int32, BytecodeType::Null],
        struct_layouts: vec![],
        choice_layouts: vec![],
        imports: vec![],
        exports: vec![
            ExportSlot {
                symbol_name: "marker".to_string(),
                kind: galfus_bytecode::ExportKind::Function(FuncIdx(0)),
            },
            ExportSlot {
                symbol_name: "global_0".to_string(),
                kind: galfus_bytecode::ExportKind::Global(GlobalIdx(0)),
            },
        ],
        init_func_idx: Some(FuncIdx(0)),
    };
    let entry = BytecodeModule {
        name: "main.gfs".to_string(),
        constants: ConstantPool::default(),
        functions: vec![BytecodeFunction {
            name: "main".to_string(),
            param_count: 1,
            local_count: 0,
            temp_count: 1,
            return_ty: TypeIdx(3),
            proxy_metadata: None,
            instructions: vec![
                Instruction::LoadGlobal {
                    dest: Reg(1),
                    module_id: dependency_id,
                    global_idx: GlobalIdx(0),
                },
                Instruction::Ret { src: Reg(1) },
            ],
        }],
        types: vec![
            BytecodeType::Uint8,
            BytecodeType::Array(TypeIdx(0)),
            BytecodeType::Array(TypeIdx(1)),
            BytecodeType::Int32,
        ],
        struct_layouts: vec![],
        choice_layouts: vec![],
        imports: vec![ImportSlot {
            module_name: "dependency.gfs".to_string(),
            symbol_name: "marker".to_string(),
            ty: TypeIdx(3),
            kind: galfus_bytecode::ImportKind::Function,
        }],
        exports: vec![ExportSlot {
            symbol_name: "main".to_string(),
            kind: galfus_bytecode::ExportKind::Function(FuncIdx(0)),
        }],
        init_func_idx: None,
    };
    let graph = BytecodeGraph::from_modules(
        SemanticRevision::new(0),
        vec![
            node(dependency_id, "dependency.gfs", dependency),
            node(entry_id, "main.gfs", entry),
        ],
        vec![ImportEdge {
            from: entry_id,
            to: dependency_id,
        }],
    )
    .expect("valid graph");

    struct TestExecutor {
        queue: sync::Mutex<collections::VecDeque<galfus_contract::KernelTask>>,
    }
    struct ImmediateProvider;
    impl galfus_contract::HostProvider for ImmediateProvider {
        fn dispatch(
            &mut self,
            thread_id: usize,
            request_id: u64,
            _name: &str,
            _args: &[galfus_contract::BoundaryValue],
            injector: sync::Arc<dyn galfus_contract::MessageInjector>,
        ) {
            injector.inject_system_response(
                thread_id,
                request_id,
                Ok(galfus_contract::BoundaryValue::Null),
            );
        }
    }
    impl galfus_contract::KernelDriver for TestExecutor {
        fn on_exit(
            &self,
            _cb: Box<dyn Fn(Result<i32, galfus_contract::ExecutionFailure>) + Send + Sync>,
        ) {
        }
        fn run(&self) {}

        fn dispatch(&self, task: galfus_contract::KernelTask) {
            self.queue.lock().unwrap().push_back(task);
        }

        fn step(&self) -> galfus_contract::ExecutorStepResult {
            let t = self.queue.lock().unwrap().pop_front();
            let Some(t) = t else {
                return galfus_contract::ExecutorStepResult::Blocked { timeout: None };
            };
            let runnable = match t {
                galfus_contract::KernelTask::Main(x) => x,
                galfus_contract::KernelTask::Any(x) => x,
            };
            match runnable.run(100) {
                galfus_contract::ThreadResult::Discarded => {
                    galfus_contract::ExecutorStepResult::Running
                }
                galfus_contract::ThreadResult::Completed(res) => {
                    let code = if let Ok(galfus_contract::BoundaryValue::I32(c)) = res {
                        c
                    } else {
                        0
                    };
                    galfus_contract::ExecutorStepResult::Completed(code)
                }
                galfus_contract::ThreadResult::Blocked { timeout } => {
                    galfus_contract::ExecutorStepResult::Blocked { timeout }
                }
            }
        }
    }
    let executor = std::rc::Rc::new(TestExecutor {
        queue: sync::Mutex::new(collections::VecDeque::new()),
    });

    let mut task = Runtime::new(
        sync::Arc::new(graph.clone()),
        Some(galfus_contract::Providers::with_host(Box::new(
            ImmediateProvider,
        ))),
    )
    .start(entry_id, "main", &[], executor.clone())
    .expect("entry execution succeeds");

    let exit_code = match task.run_to_completion() {
        Ok(galfus_contract::BoundaryValue::I32(code)) => code,
        _ => panic!("Expected i32 exit code"),
    };
    assert_eq!(exit_code, 42);
}
