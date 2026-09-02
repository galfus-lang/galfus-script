use galfus_contract::LimitsMetadata;
mod adapters;

use std::collections;
use std::sync;

use super::*;
use galfus_bytecode::instruction::{ConstIdx, FuncIdx, GlobalIdx, Instruction, Reg, TypeIdx};
use galfus_bytecode::{
    BytecodeFunction, BytecodeGraph, BytecodeModule, BytecodeNode, BytecodeType, Constant,
    ConstantPool, ExecutionMetadata, ExportSlot, ImportEdge, ImportSlot, PackageEntryPoint,
    PackageImage, PackageMetadata,
};
use galfus_contract::{
    AdapterLoadContext, CURRENT_BOUNDARY_ABI_VERSION, ExecutionTarget, ProviderDescriptor,
    ProviderModuleDescriptor, ProviderModuleRequirement, Providers, RuntimeCapabilities,
    SurfaceContract, SurfaceDirection, SurfaceFunctionContract, SurfaceSchema,
};
use galfus_core::{ModuleId, ModulePath, SemanticRevision, SourceId, Span};

struct StartupProvider {
    calls: sync::Arc<sync::Mutex<Vec<String>>>,
    pending: sync::Arc<
        sync::Mutex<
            Option<(
                galfus_core::ThreadId,
                galfus_core::RequestLease,
                sync::Arc<dyn galfus_contract::MessageInjector>,
            )>,
        >,
    >,
    fail_initializer: bool,
}

fn target() -> ExecutionTarget {
    ExecutionTarget::new("test").expect("valid target")
}

impl galfus_contract::HostProvider for StartupProvider {
    fn descriptor(&self) -> galfus_contract::ProviderDescriptor {
        let contract = |operation: &str| SurfaceFunctionContract {
            provider_operation: format!("main_{operation}"),
            bridge_symbol: format!("__provider_main_{operation}"),
            parameters: Vec::new(),
            result: SurfaceContract::new(
                format!("test::__provider_main_{operation}:return"),
                1,
                SurfaceDirection::FromProvider,
                SurfaceSchema::Null,
            ),
        };
        ProviderDescriptor {
            modules: vec![ProviderModuleDescriptor {
                module_path: "test/main".to_string(),
                schema_fingerprint: 0,
                boundary_abi: CURRENT_BOUNDARY_ABI_VERSION,
                exports: Vec::new(),
                surface_contracts: vec![contract("initialize"), contract("entry")],
            }],
        }
    }

    fn dispatch_surface(
        &mut self,
        thread_id: galfus_core::ThreadId,
        request_lease: galfus_core::RequestLease,
        name: &str,
        _args: &[galfus_contract::SurfaceValue],
        injector: sync::Arc<dyn galfus_contract::MessageInjector>,
    ) -> bool {
        self.calls.lock().unwrap().push(name.to_string());
        if name == "main_initialize" && self.fail_initializer {
            let _ = injector.inject_surface_response(
                thread_id,
                request_lease,
                Err(galfus_contract::ExecutionFailure::new(
                    galfus_contract::ExecutionFailureKind::ProviderFailure,
                    "initializer rejected",
                )),
            );
        } else if name == "main_initialize" {
            *self.pending.lock().unwrap() = Some((thread_id, request_lease, injector));
        } else {
            let _ = injector.inject_surface_response(
                thread_id,
                request_lease,
                Ok(galfus_contract::SurfaceValue::Null),
            );
        }
        true
    }
}

fn startup_graph() -> (sync::Arc<BytecodeGraph>, ModuleId) {
    let module_id = ModuleId::new(1);
    let module = BytecodeModule {
        name: "main.gfs".to_string(),
        global_count: 0,
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
                adapter_proxy_metadata: None,
                instructions: vec![
                    Instruction::CreateFuture {
                        dest: Reg(0),
                        func: FuncIdx(2),
                        args_start: Reg(0),
                        arg_count: 0,
                        arg_types: Box::new([]),
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
                adapter_proxy_metadata: None,
                instructions: vec![
                    Instruction::CreateFuture {
                        dest: Reg(1),
                        func: FuncIdx(3),
                        args_start: Reg(0),
                        arg_count: 0,
                        arg_types: Box::new([]),
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
                name: "__provider_main_initialize".to_string(),
                param_count: 0,
                local_count: 0,
                temp_count: 0,
                return_ty: TypeIdx(0),
                adapter_proxy_metadata: None,
                instructions: vec![Instruction::RetNull],
            },
            BytecodeFunction {
                name: "__provider_main_entry".to_string(),
                param_count: 0,
                local_count: 0,
                temp_count: 0,
                return_ty: TypeIdx(0),
                adapter_proxy_metadata: None,
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

fn package_with_entry(
    graph: sync::Arc<BytecodeGraph>,
    module_id: ModuleId,
) -> sync::Arc<PackageImage> {
    let module_path = graph
        .get(module_id)
        .expect("entry module exists")
        .path()
        .clone();
    sync::Arc::new(
        PackageImage::try_new(
            (*graph).clone(),
            target(),
            Some(PackageEntryPoint::new(module_path, "main")),
            galfus_bytecode::PackageMetadata {
                name: "test".into(),
                version: None,
                author: None,
                email: None,
                description: None,
            },
            galfus_contract::LimitsMetadata::default(),
            Vec::new(),
            Vec::new(),
        )
        .expect("graph has no adapter proxies"),
    )
}

fn package_with_required_provider(
    graph: sync::Arc<BytecodeGraph>,
    module_id: ModuleId,
) -> sync::Arc<PackageImage> {
    let module_path = graph
        .get(module_id)
        .expect("entry module exists")
        .path()
        .clone();
    sync::Arc::new(
        PackageImage::try_new(
            (*graph).clone(),
            target(),
            Some(PackageEntryPoint::new(module_path, "main")),
            galfus_bytecode::PackageMetadata {
                name: "test".into(),
                version: None,
                author: None,
                email: None,
                description: None,
            },
            galfus_contract::LimitsMetadata::default(),
            Vec::new(),
            vec![ProviderModuleRequirement {
                alias: "main".to_string(),
                module_path: "std/io".to_string(),
                schema_fingerprint: 1,
                boundary_abi: CURRENT_BOUNDARY_ABI_VERSION,
                exports: Vec::new(),
            }],
        )
        .expect("provider requirement is valid"),
    )
}

fn start_with_provider(provider: StartupProvider) -> Execution {
    let (graph, module_id) = startup_graph();
    Runtime::new(
        package_with_entry(graph, module_id),
        RuntimeCapabilities::builder()
            .with_providers(Providers::new().with_host("main", Box::new(provider)))
            .build(),
    )
    .start(&[], std::rc::Rc::new(CooperativeDriver::new()))
    .expect("startup execution is created")
}

#[test]
fn runtime_rejects_an_unsupported_bytecode_format_before_loading_the_entry_module() {
    let graph =
        BytecodeGraph::with_format_version(galfus_bytecode::BytecodeFormatVersion::new(1, 0, 0));

    let package = sync::Arc::new(
        PackageImage::try_new(
            graph,
            target(),
            None,
            PackageMetadata {
                name: "test".into(),
                version: None,
                author: None,
                email: None,
                description: None,
            },
            LimitsMetadata::default(),
            Vec::new(),
            Vec::new(),
        )
        .expect("graph has no adapter proxies"),
    );
    let result = Runtime::new(package, RuntimeCapabilities::builder().build())
        .start(&[], std::rc::Rc::new(CooperativeDriver::new()));
    let Err(error) = result else {
        panic!("unsupported bytecode must be rejected before runtime loading");
    };

    assert!(matches!(
        error,
        RuntimeError::BytecodeFormat(galfus_bytecode::BytecodeFormatError {
            supported: galfus_bytecode::CURRENT_BYTECODE_FORMAT_VERSION,
            actual,
        }) if actual == galfus_bytecode::BytecodeFormatVersion::new(1, 0, 0)
    ));
}

#[test]
fn runtime_rejects_a_missing_required_provider_before_execution() {
    let (graph, module_id) = startup_graph();
    let result = Runtime::new(
        package_with_required_provider(graph, module_id),
        RuntimeCapabilities::builder().build(),
    )
    .start(&[], std::rc::Rc::new(CooperativeDriver::new()));

    assert!(matches!(
        result,
        Err(RuntimeError::ProviderRequirementUnsatisfied { module_path })
            if module_path == "std/io"
    ));
}

#[test]
fn execution_host_runs_preflight_before_runtime_bootstrap() {
    let (graph, module_id) = startup_graph();
    let package = package_with_required_provider(graph, module_id);
    let mismatched_context = AdapterLoadContext {
        target: ExecutionTarget::new("other").expect("valid target"),
        properties: collections::BTreeMap::new(),
    };
    let preflight_result = ExecutionHost::new(mismatched_context).start(
        sync::Arc::clone(&package),
        &[],
        std::rc::Rc::new(CooperativeDriver::new()),
    );

    assert!(matches!(
        preflight_result,
        Err(HostBootstrapError::Preflight(
            PreflightError::PackageTargetMismatch { .. }
        ))
    ));

    let context = AdapterLoadContext {
        target: target(),
        properties: collections::BTreeMap::new(),
    };
    let result =
        ExecutionHost::new(context).start(package, &[], std::rc::Rc::new(CooperativeDriver::new()));

    assert!(matches!(
        result,
        Err(HostBootstrapError::Runtime(
            RuntimeError::ProviderRequirementUnsatisfied { module_path }
        )) if module_path == "std/io"
    ));
}

#[test]
fn format_panic_uses_materialized_module_paths_and_locations() {
    let module_id = ModuleId::new(7);
    let mut metadata = ExecutionMetadata::default();
    metadata.set_function_spans(
        FuncIdx(0),
        collections::HashMap::from([(0, Span::new(SourceId::new(99), 4, 8))]),
    );
    let graph = BytecodeGraph::from_modules(
        SemanticRevision::new(0),
        vec![BytecodeNode {
            id: module_id,
            path: ModulePath::new("src/main.gfs").expect("valid module path"),
            semantic_revision: SemanticRevision::new(0),
            module: BytecodeModule {
                name: "main.gfs".to_string(),
                global_count: 0,
                constants: ConstantPool::default(),
                functions: vec![BytecodeFunction {
                    name: "main".to_string(),
                    param_count: 0,
                    local_count: 0,
                    temp_count: 0,
                    return_ty: TypeIdx(0),
                    adapter_proxy_metadata: None,
                    instructions: vec![Instruction::RetNull],
                }],
                types: vec![BytecodeType::Null],
                struct_layouts: vec![],
                choice_layouts: vec![],
                imports: vec![],
                exports: vec![],
                init_func_idx: None,
            },
            metadata: Some(metadata),
        }],
        vec![],
    )
    .expect("test graph is valid");
    let panic = galfus_vm::VmPanic {
        error: galfus_vm::VmError::Panic {
            message: "boom".to_string(),
        },
        stack_trace: vec![galfus_vm::StackFrameInfo {
            module_id,
            func_idx: FuncIdx(0),
            instruction_offset: 0,
        }],
    };

    let formatted = format_panic(&graph, &panic);

    assert!(formatted.contains("src/main.gfs::main (at instruction 0 at src/main.gfs:4..8)"));
    assert!(!formatted.contains("source#"));
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
    assert_eq!(*calls.lock().unwrap(), vec!["main_initialize"]);
    let (thread_id, request_lease, injector) = pending
        .lock()
        .unwrap()
        .take()
        .expect("initializer is pending");
    assert_eq!(
        injector.inject_surface_response(
            galfus_core::ThreadId::new(thread_id.raw() + 1),
            request_lease,
            Ok(galfus_contract::SurfaceValue::Null),
        ),
        Err(galfus_contract::MessageInjectionError::HostProtocolViolation)
    );
    execution
        .poll(100)
        .expect("cross-thread completion is ignored safely");
    assert_eq!(*calls.lock().unwrap(), vec!["main_initialize"]);
    injector
        .inject_system_response(
            thread_id,
            request_lease,
            Ok(galfus_contract::BoundaryValue::Null),
        )
        .expect("matching completion is accepted");

    assert_eq!(
        execution.run_sync_to_completion(),
        Ok(galfus_contract::BoundaryValue::I32(42))
    );
    assert_eq!(execution.status(), ExecutionState::Closed);
    assert_eq!(
        execution.result(),
        Some(&Ok(galfus_contract::BoundaryValue::I32(42)))
    );
    assert_eq!(
        *calls.lock().unwrap(),
        vec!["main_initialize", "main_entry"]
    );
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
        .run_sync_to_completion()
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
        global_count: 1,
        constants: ConstantPool {
            constants: vec![Constant::Int32(42)],
        },
        functions: vec![BytecodeFunction {
            name: "__init_module".to_string(),
            param_count: 0,
            local_count: 0,
            temp_count: 1,
            return_ty: TypeIdx(1),
            adapter_proxy_metadata: None,
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
        global_count: 0,
        constants: ConstantPool::default(),
        functions: vec![BytecodeFunction {
            name: "main".to_string(),
            param_count: 1,
            local_count: 0,
            temp_count: 1,
            return_ty: TypeIdx(3),
            adapter_proxy_metadata: None,
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
        events: sync::Arc<crate::driver::NativeEventBridge>,
    }
    struct ImmediateProvider;
    impl galfus_contract::HostProvider for ImmediateProvider {
        fn descriptor(&self) -> galfus_contract::ProviderDescriptor {
            galfus_contract::ProviderDescriptor::default()
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
    impl crate::driver::ExecutionDriver for TestExecutor {
        fn event_sink(&self) -> sync::Arc<dyn crate::driver::RuntimeEventSink> {
            self.events.clone()
        }

        fn drain_events(&self) -> Vec<(crate::event::EventSequence, crate::event::RuntimeEvent)> {
            self.events.drain()
        }

        fn has_pending_events(&self) -> bool {
            self.events.has_pending()
        }
    }
    let executor = std::rc::Rc::new(TestExecutor {
        queue: sync::Mutex::new(collections::VecDeque::new()),
        events: sync::Arc::new(crate::driver::NativeEventBridge::new()),
    });

    let package = sync::Arc::new(
        PackageImage::try_new(
            graph,
            target(),
            Some(PackageEntryPoint::new(
                ModulePath::new("main.gfs").expect("valid module path"),
                "main",
            )),
            galfus_bytecode::PackageMetadata {
                name: "test".into(),
                version: None,
                author: None,
                email: None,
                description: None,
            },
            galfus_contract::LimitsMetadata::default(),
            Vec::new(),
            Vec::new(),
        )
        .expect("graph has no adapter proxies"),
    );
    let mut task = Runtime::new(
        package,
        RuntimeCapabilities::builder()
            .with_providers(Providers::new().with_host("io", Box::new(ImmediateProvider)))
            .build(),
    )
    .start(&[], executor.clone())
    .expect("entry execution succeeds");

    let exit_code = match task.run_sync_to_completion() {
        Ok(galfus_contract::BoundaryValue::I32(code)) => code,
        _ => panic!("Expected i32 exit code"),
    };
    assert_eq!(exit_code, 42);
}
