#[test]
fn check_includes_configured_entry_and_exports_as_semantic_roots() {
    let mut workspace = Workspace::new();
    workspace
        .load_config(
            br#"
            [module]
            name = "semantic-roots"
            target = "app"
            entry = "main.gfs"

            [exports]
            library = "library.gfs"
            "#,
        )
        .expect("valid configuration");
    workspace
        .load_module(
            "main.gfs",
            b"export fn main(args: [[u8]]): i32 { return 0 }",
        )
        .expect("valid entry module");
    workspace
        .load_module("library.gfs", b"export fn value(): i32 { return 1 }")
        .expect("valid export module");

    assert!(workspace.check().is_valid);

    let roots = workspace.frontend.semantic_graph().roots();
    assert!(roots.iter().any(|root| {
        root.kind() == &SemanticRootKind::Entry && root.path().as_str() == "main.gfs"
    }));
    assert!(roots.iter().any(|root| {
        root.kind()
            == &SemanticRootKind::Export {
                address: "library".to_string(),
            }
            && root.path().as_str() == "library.gfs"
    }));
}

#[test]
fn compile_emits_one_module_per_source_module_with_import_slots() {
    let mut workspace = Workspace::new();
    workspace
        .load_config(
            br#"
            [module]
            name = "module-images"
            target = "app"
            entry = "main.gfs"
            "#,
        )
        .expect("valid configuration");
    workspace
        .load_module(
            "main.gfs",
            br#"
            import { add } from "./math"

            export fn main(args: [[u8]]): i32 {
                return add(20, 22)
            }
            "#,
        )
        .expect("valid main module");
    workspace
        .load_module(
            "math.gfs",
            br#"
            export fn add(left: i32, right: i32): i32 {
                return left + right
            }
            "#,
        )
        .expect("valid dependency module");

    assert!(workspace.check().is_valid);
    let report = workspace.compile().expect("workspace compiles");

    assert_eq!(report.package.graph().len(), 2);
    assert_eq!(report.package.graph().edges().len(), 1);

    let main = report
        .package
        .graph()
        .modules()
        .find(|image| image.path().as_str() == "main.gfs")
        .expect("main image");
    assert_eq!(main.module().imports.len(), 1);
    assert_eq!(main.module().imports[0].module_name, "math.gfs");
    assert_eq!(main.module().imports[0].symbol_name, "add");
    assert!(
        main.module()
            .functions
            .iter()
            .all(|function| function.name != "__init_workspace")
    );
}

#[test]
fn check_accepts_imported_adapter_proxy_declarations() {
    struct DemoSchema;
    impl galfus_contract::AdapterSchema for DemoSchema {
        fn name(&self) -> &str {
            "demo"
        }
        fn catalog_schema(&self) -> String {
            "adapter demo { fn add(i32, i32): i32 }".to_string()
        }
        fn validate_schema(
            &self,
            _descriptor: &galfus_contract::AdapterModuleDescriptor,
        ) -> Result<(), galfus_contract::AdapterValidationError> {
            Ok(())
        }
    }
    let mut workspace = Workspace::new();
    let catalog =
        galfus_contract::CapabilityCatalog::new(Vec::new(), vec![std::sync::Arc::new(DemoSchema)])
            .expect("demo catalog is valid");
    workspace.set_catalog(std::sync::Arc::new(catalog));
    workspace
        .load_config(
            br#"
            [module]
            name = "external-proxy"
            target = "app"
            entry = "main.gfs"
            "#,
        )
        .expect("valid configuration");
    workspace
        .load_module(
            "main.gfs",
            br#"
            import { add } from "./math.gfp"

            export fn main(args: [[u8]]): i32 {
                return 0
            }
            "#,
        )
        .expect("valid main module");
    workspace
        .load_module(
            "math.gfp",
            br#"---
adapter = "demo"
[config]
test = "memory"
---

export fn(async) add(left: i32, right: i32): i32
"#,
        )
        .expect("valid proxy source");

    assert_eq!(
        galfus_frontend::modules::resolve_relative_import(
            &ModulePath::new("main.gfs").unwrap(),
            "./math.gfp",
            None,
        ),
        Some(ModulePath::new("math.gfp").unwrap()),
    );

    let (is_valid, diagnostics) = {
        let check = workspace.check();
        (check.is_valid, format!("{:?}", check.diagnostics))
    };
    assert!(is_valid, "{diagnostics}");
    assert_eq!(
        workspace.adapter_descriptors[&ModulePath::new("math.gfp").unwrap()].adapter,
        "demo"
    );
    assert_eq!(
        workspace.adapter_descriptors[&ModulePath::new("math.gfp").unwrap()].exports,
        vec![galfus_contract::AdapterFunctionSignature {
            name: "add".to_string(),
            is_async: true,
            parameter_types: vec![
                galfus_contract::BoundaryType::I32,
                galfus_contract::BoundaryType::I32,
            ],
            return_type: galfus_contract::BoundaryType::I32,
        }]
    );
    let report = workspace.compile().expect("proxy compilation succeeds");
    assert_eq!(
        report.package.adapter_requirements(),
        vec![galfus_contract::AdapterModuleRequirement {
            proxy_module: "math.gfp".to_string(),
            descriptor: workspace.adapter_descriptors[&ModulePath::new("math.gfp").unwrap()]
                .clone(),
            boundary_abi: galfus_contract::CURRENT_BOUNDARY_ABI_VERSION,
        }]
    );
    let graph = report.package.graph();
    let proxy = graph
        .modules()
        .find(|module| module.path().as_str() == "math.gfp")
        .expect("proxy bytecode module");
    let function = proxy
        .module
        .functions
        .iter()
        .find(|function| function.name == "add")
        .expect("proxy export");
    let instructions = &function.instructions;
    assert!(
        matches!(
            instructions.first(),
            Some(galfus_bytecode::Instruction::RetNull)
        ),
        "{instructions:?}"
    );
    let adapter_proxy_metadata = function
        .adapter_proxy_metadata
        .as_ref()
        .expect("proxy metadata");
    assert_eq!(adapter_proxy_metadata.proxy_module, "math.gfp");
    assert_eq!(adapter_proxy_metadata.symbol, "add");
}

#[test]
fn compile_updates_changed_modules_and_removes_deleted_modules() {
    let mut workspace = Workspace::new();
    workspace
        .load_config(
            br#"
            [module]
            name = "incremental-compile"
            target = "app"
            entry = "main.gfs"
            
            [exports]
            helper = "helper.gfs"
            "#,
        )
        .expect("valid configuration");
    workspace
        .load_module(
            "main.gfs",
            br#"
            export fn main(args: [[u8]]): i32 {
                return 0
            }
            "#,
        )
        .expect("valid main module");
    workspace
        .load_module(
            "helper.gfs",
            br#"
            export fn value(): i32 {
                return 1
            }
            "#,
        )
        .expect("valid helper module");

    let check = workspace.check();
    assert!(check.is_valid, "{:?}", check.diagnostics);
    let first_package = workspace.compile().expect("initial compilation").package;
    let first_graph = first_package.graph();
    let main = first_graph
        .modules()
        .find(|image| image.path().as_str() == "main.gfs")
        .expect("main image");
    let helper = first_graph
        .modules()
        .find(|image| image.path().as_str() == "helper.gfs")
        .expect("helper image");
    let main_id = main.id();
    let helper_id = helper.id();
    let main_revision = main.semantic_revision();
    let helper_revision = helper.semantic_revision();

    workspace
        .load_module(
            "helper.gfs",
            br#"
            export fn value(): i32 {
                return 2
            }
            "#,
        )
        .expect("updated helper module");
    assert!(workspace.check().is_valid);
    let updated_package = workspace
        .compile()
        .expect("incremental compilation")
        .package;
    let updated_graph = updated_package.graph();

    assert_eq!(
        updated_graph
            .get(main_id)
            .expect("cached main image")
            .semantic_revision(),
        main_revision
    );
    assert!(
        updated_graph
            .get(helper_id)
            .expect("updated helper image")
            .semantic_revision()
            > helper_revision
    );

    assert!(matches!(
        workspace.remove_module("helper.gfs"),
        Ok(RemoveResult::Success)
    ));
    assert!(workspace.check().is_valid);
    let deleted_package = workspace
        .compile()
        .expect("compilation after deletion")
        .package;
    let deleted_graph = deleted_package.graph();

    assert_eq!(deleted_graph.len(), 1);
    assert!(deleted_graph.get(helper_id).is_none());
    assert_eq!(
        deleted_graph
            .get(main_id)
            .expect("cached main image")
            .semantic_revision(),
        main_revision
    );
}

#[test]
fn compile_rebuilds_only_changed_modules_and_transitive_dependents() {
    let mut workspace = Workspace::new();
    workspace
        .load_config(
            br#"
            [module]
            name = "dependent-compile"
            target = "app"
            entry = "main.gfs"
            "#,
        )
        .expect("valid configuration");
    workspace
        .load_module(
            "main.gfs",
            br#"
            import { value } from "./dependency"
            import { isolated } from "./isolated"

            export fn main(args: [[u8]]): i32 {
                return value() + isolated()
            }
            "#,
        )
        .expect("valid entry module");
    workspace
        .load_module(
            "dependency.gfs",
            br#"
            export fn value(): i32 {
                return 1
            }
            "#,
        )
        .expect("valid dependency module");
    workspace
        .load_module(
            "isolated.gfs",
            br#"
            export fn isolated(): i32 {
                return 0
            }
            "#,
        )
        .expect("valid isolated module");

    let check = workspace.check();
    assert!(check.is_valid, "{:?}", check.diagnostics);
    let first_package = workspace.compile().expect("initial compilation").package;
    let first = first_package.graph();
    let main = first
        .modules()
        .find(|image| image.path().as_str() == "main.gfs")
        .expect("main image");
    let dependency = first
        .modules()
        .find(|image| image.path().as_str() == "dependency.gfs")
        .expect("dependency image");
    let isolated = first
        .modules()
        .find(|image| image.path().as_str() == "isolated.gfs")
        .expect("isolated image");
    let main_revision = main.semantic_revision();
    let dependency_revision = dependency.semantic_revision();
    let isolated_revision = isolated.semantic_revision();
    let main_id = main.id();
    let dependency_id = dependency.id();
    let isolated_id = isolated.id();

    workspace
        .load_module(
            "dependency.gfs",
            br#"
            export fn value(): i32 {
                return 2
            }
            "#,
        )
        .expect("updated dependency module");
    assert!(workspace.check().is_valid);
    let updated_package = workspace
        .compile()
        .expect("incremental compilation")
        .package;
    let updated = updated_package.graph();

    assert!(
        updated
            .get(main_id)
            .expect("recompiled main")
            .semantic_revision()
            > main_revision
    );
    assert!(
        updated
            .get(dependency_id)
            .expect("recompiled dependency")
            .semantic_revision()
            > dependency_revision
    );
    assert_eq!(
        updated
            .get(isolated_id)
            .expect("cached isolated module")
            .semantic_revision(),
        isolated_revision
    );
}

#[test]
fn compile_removes_unreachable_modules() {
    let mut workspace = Workspace::new();
    assert!(matches!(
        workspace
            .load_config(
                br#"
            [module]
            name = "test"
            target = "app"
            entry = "main.gfs"
        "#
            )
            .unwrap(),
        LoadResult::Success
    ));
    workspace
        .load_module("main.gfs", b"import { x } from \"./a\"\nconst y = x;")
        .unwrap();
    workspace
        .load_module("a.gfs", b"export const x = 1;")
        .unwrap();

    let report1 = workspace.check();
    assert!(report1.is_valid, "{:?}", report1.diagnostics);
    let package1 = workspace.compile().unwrap().package;
    let graph1 = package1.graph();
    assert!(graph1.modules().any(|m| m.path().as_str() == "a.gfs"));

    // Remove import
    workspace
        .load_module("main.gfs", b"const x = 2;")
        .expect("valid replacement module");
    let report = workspace.check();
    assert!(report.is_valid, "{:?}", report.diagnostics);
    let package2 = workspace.compile().unwrap().package;
    let graph2 = package2.graph();

    // The unreachable module should be removed from the graph.
    assert!(!graph2.modules().any(|m| m.path().as_str() == "a.gfs"));
}

#[test]
fn run_requires_compile_and_executes_the_configured_entry() {
    let mut workspace = Workspace::new();
    assert!(matches!(
        workspace.run(&[], None, std::rc::Rc::new(CooperativeDriver::new())),
        Err(RunBlocked::CompileRequired)
    ));

    workspace
        .load_config(
            br#"
            [module]
            name = "run-entry"
            target = "app"
            entry = "main.gfs"
            "#,
        )
        .expect("valid configuration");
    workspace
        .load_module(
            "main.gfs",
            br#"
            export fn main(args: [[u8]]): i32 {
                return 42
            }
            "#,
        )
        .expect("valid entry module");

    assert!(matches!(
        workspace.compile(),
        Err(CompileBlocked::Dirty { .. })
    ));
    assert!(workspace.check().is_valid);
    workspace.compile().expect("workspace compiles");
    let executor = std::rc::Rc::new(CooperativeDriver::new());
    let exit_code = Arc::new(Mutex::new(0));
    let ec = Arc::clone(&exit_code);
    executor.on_exit(Box::new(move |res| {
        *ec.lock().unwrap() = res.unwrap();
    }));
    workspace.run(&[], None, executor).expect("entry executes");
    assert_eq!(*exit_code.lock().unwrap(), 42);
}

#[test]
fn run_rejects_a_missing_required_io_provider_before_execution() {
    let mut workspace = Workspace::new();
    workspace.set_catalog(io_catalog(galfus_contract::STD_IO_SOURCE));
    workspace
        .load_config(
            br#"
            [module]
            name = "missing-io-provider"
            target = "app"
            entry = "main.gfs"
            "#,
        )
        .expect("valid configuration");
    workspace
        .load_module(
            "main.gfs",
            br#"
            import { println } from "std/io"

            export fn main(args: [[u8]]): i32 {
                println("output")
                return 0
            }
            "#,
        )
        .expect("valid entry module");

    assert!(workspace.check().is_valid);
    workspace.compile().expect("workspace compiles");

    let executor = std::rc::Rc::new(CooperativeDriver::new());
    let error = workspace
        .run(&[], None, executor)
        .expect_err("a required provider must be available before execution");
    assert!(matches!(
        error,
        crate::RunBlocked::RuntimeError(message)
            if message.contains("required provider module `std/io` is unavailable or incompatible")
    ));
}

#[test]
fn compile_produces_identical_bytecode_regardless_of_module_load_order() {
    // Criterion: the compiled bytecode must be byte-for-byte identical no matter
    // in which order the source files are fed to load_module(). This verifies
    // that no HashMap or HashSet non-determinism leaks into the output.

    let config = br#"
        [module]
        name = "determinism-test"
        target = "app"
        entry = "main.gfs"
    "#;
    let main_src = br#"
        import { add } from "./math"
        import { mul } from "./ops"
        export fn main(args: [[u8]]): i32 {
            return add(mul(2, 3), 1)
        }
    "#;
    let math_src = b"export fn add(a: i32, b: i32): i32 { return a + b }";
    let ops_src = b"export fn mul(a: i32, b: i32): i32 { return a * b }";

    // Build workspace A: load in order main → math → ops
    let mut ws_a = Workspace::new();
    ws_a.load_config(config).expect("valid configuration");
    ws_a.load_module("main.gfs", main_src)
        .expect("valid main module");
    ws_a.load_module("math.gfs", math_src)
        .expect("valid math module");
    ws_a.load_module("ops.gfs", ops_src)
        .expect("valid ops module");
    assert!(ws_a.check().is_valid, "workspace A must be valid");
    let report_a = ws_a.compile().expect("workspace A must compile");
    let graph_a = report_a.package.graph();

    // Build workspace B: load in reverse order ops → math → main
    let mut ws_b = Workspace::new();
    ws_b.load_config(config).expect("valid configuration");
    ws_b.load_module("ops.gfs", ops_src)
        .expect("valid ops module");
    ws_b.load_module("math.gfs", math_src)
        .expect("valid math module");
    ws_b.load_module("main.gfs", main_src)
        .expect("valid main module");
    assert!(ws_b.check().is_valid, "workspace B must be valid");
    let report_b = ws_b.compile().expect("workspace B must compile");
    let graph_b = report_b.package.graph();

    // Compare module names and function counts.
    let mut paths_a: Vec<String> = graph_a
        .modules()
        .map(|m| m.path().as_str().to_string())
        .collect();
    let mut paths_b: Vec<String> = graph_b
        .modules()
        .map(|m| m.path().as_str().to_string())
        .collect();
    paths_a.sort();
    paths_b.sort();
    assert_eq!(
        paths_a, paths_b,
        "both workspaces must compile the same set of modules"
    );

    // Compare complete bytecode modules after locating them by logical path.
    for path in &paths_a {
        let mod_a = graph_a
            .modules()
            .find(|m| m.path().as_str() == path)
            .expect("module exists in A");
        let mod_b = graph_b
            .modules()
            .find(|m| m.path().as_str() == path)
            .expect("module exists in B");
        assert_eq!(
            mod_a.module(),
            mod_b.module(),
            "module '{}' must have identical bytecode",
            path
        );
    }
}

fn io_catalog(source: &str) -> std::sync::Arc<galfus_contract::CapabilityCatalog> {
    std::sync::Arc::new(
        galfus_contract::CapabilityCatalog::new(
            vec![galfus_contract::BridgeModule::new("std/io", source)],
            Vec::new(),
        )
        .expect("the std/io provider catalog is valid"),
    )
}

fn workspace_importing_io() -> Workspace {
    let mut workspace = Workspace::new();
    workspace
        .load_config(
            br#"
            [module]
            name = "provider-catalog"
            target = "app"
            entry = "main.gfs"
            "#,
        )
        .expect("valid configuration");
    workspace
        .load_module(
            "main.gfs",
            b"import { println } from \"std/io\"\nexport fn main(args: [[u8]]): i32 { return 0 }",
        )
        .expect("valid main module");
    workspace
}

#[test]
fn catalog_change_replaces_loaded_provider_source() {
    let mut workspace = workspace_importing_io();
    workspace.set_catalog(io_catalog(galfus_contract::STD_IO_SOURCE));
    assert!(workspace.check().is_valid);
    workspace
        .compile()
        .expect("a declarative provider does not require a concrete binder to compile");

    let updated = format!("{}\n", galfus_contract::STD_IO_SOURCE);
    workspace.set_catalog(io_catalog(&updated));
    assert!(workspace.is_dirty());
    assert!(workspace.check().is_valid);

    let provider_path = ModulePath::new("std/io.gfs").expect("valid provider path");
    let entry = workspace
        .source_state
        .store
        .get(&provider_path)
        .expect("provider is reloaded");
    assert_eq!(&*entry.bytes, updated.as_bytes());
    assert_eq!(
        entry.origin,
        crate::source_store::ModuleOrigin::ProviderCatalog
    );
}

#[test]
fn catalog_removal_unloads_provider_and_invalidates_import() {
    let mut workspace = workspace_importing_io();
    workspace.set_catalog(io_catalog(galfus_contract::STD_IO_SOURCE));
    assert!(workspace.check().is_valid);

    workspace.set_catalog(std::sync::Arc::new(
        galfus_contract::CapabilityCatalog::default(),
    ));
    assert!(workspace.is_dirty());
    assert!(!workspace.check().is_valid);
    assert!(
        workspace
            .source_state
            .store
            .get(&ModulePath::new("std/io.gfs").expect("valid provider path"))
            .is_none()
    );
}

#[test]
fn user_module_cannot_overwrite_a_catalog_provider() {
    let mut workspace = Workspace::new();
    workspace.set_catalog(io_catalog(galfus_contract::STD_IO_SOURCE));

    assert!(matches!(
        workspace.load_module("std/io.gfs", b"export fn fake(): null { return null }"),
        Err(crate::state::WorkspaceError::ReservedProviderModule(path)) if path == "std/io.gfs"
    ));
}

#[test]
fn compile_nullable_exports_with_nullable_boundary_type() {
    struct NullableSchema;
    impl galfus_contract::AdapterSchema for NullableSchema {
        fn name(&self) -> &str {
            "demo"
        }
        fn catalog_schema(&self) -> String {
            "adapter demo { fn add(i32 | null, i32): i32 | null }".to_string()
        }
        fn validate_schema(
            &self,
            _descriptor: &galfus_contract::AdapterModuleDescriptor,
        ) -> Result<(), galfus_contract::AdapterValidationError> {
            Ok(())
        }
    }

    let mut workspace = Workspace::new();
    let catalog = galfus_contract::CapabilityCatalog::new(
        Vec::new(),
        vec![std::sync::Arc::new(NullableSchema)],
    )
    .expect("demo catalog is valid");
    workspace.set_catalog(std::sync::Arc::new(catalog));
    workspace
        .load_config(
            br#"
            [module]
            name = "nullable-test"
            target = "app"
            entry = "main.gfs"
            "#,
        )
        .expect("valid configuration");
    workspace
        .load_module(
            "main.gfs",
            br#"
            import { add } from "./math.gfp"
            export fn main(args: [[u8]]): i32 {
                return 0
            }
            "#,
        )
        .expect("valid main module");
    workspace
        .load_module(
            "math.gfp",
            br#"---
adapter = "demo"
[config]
test = "memory"
---

export fn(async) add(left: i32 | null, right: i32): i32 | null
"#,
        )
        .expect("valid proxy source");

    let check = workspace.check();
    assert!(check.is_valid, "{:?}", check.diagnostics);

    let exports = &workspace.adapter_descriptors[&ModulePath::new("math.gfp").unwrap()].exports;
    assert_eq!(
        *exports,
        vec![galfus_contract::AdapterFunctionSignature {
            name: "add".to_string(),
            is_async: true,
            parameter_types: vec![
                galfus_contract::BoundaryType::Nullable(Box::new(
                    galfus_contract::BoundaryType::I32
                )),
                galfus_contract::BoundaryType::I32,
            ],
            return_type: galfus_contract::BoundaryType::Nullable(Box::new(
                galfus_contract::BoundaryType::I32
            )),
        }]
    );
}

#[test]
fn compiled_adapter_handles_are_qualified_and_do_not_collide() {
    struct AdapterSchema;

    impl galfus_contract::AdapterSchema for AdapterSchema {
        fn name(&self) -> &str {
            "demo"
        }

        fn catalog_schema(&self) -> String {
            "adapter demo { handle Window }".to_string()
        }

        fn validate_schema(
            &self,
            _descriptor: &galfus_contract::AdapterModuleDescriptor,
        ) -> Result<(), galfus_contract::AdapterValidationError> {
            Ok(())
        }
    }

    let mut workspace = Workspace::new();
    let catalog = galfus_contract::CapabilityCatalog::new(
        Vec::new(),
        vec![std::sync::Arc::new(AdapterSchema)],
    )
    .expect("adapter catalog is valid");
    workspace.set_catalog(std::sync::Arc::new(catalog));
    workspace
        .load_config(
            br#"
            [module]
            name = "qualified-handles"
            target = "app"
            entry = "main.gfs"
            "#,
        )
        .expect("valid configuration");
    workspace
        .load_module(
            "main.gfs",
            br#"
            import { create } from "./alpha.gfp"
            import beta from "./beta.gfp"

            export fn main(args: [[u8]]): i32 {
                return 0
            }
            "#,
        )
        .expect("valid main module");

    for path in ["alpha.gfp", "beta.gfp"] {
        workspace
            .load_module(
                path,
                br#"---
adapter = "demo"
[config]
test = "memory"
---

struct Window {}

export fn(async) create(): Window
export fn(async) close(window: Window): null
"#,
            )
            .expect("valid adapter proxy");
    }

    assert!(workspace.check().is_valid);
    let alpha_exports =
        &workspace.adapter_descriptors[&ModulePath::new("alpha.gfp").unwrap()].exports;
    let beta_exports =
        &workspace.adapter_descriptors[&ModulePath::new("beta.gfp").unwrap()].exports;
    assert_eq!(
        alpha_exports[0].return_type,
        galfus_contract::BoundaryType::Handle {
            type_id: galfus_core::OpaqueTypeId::new("alpha", "Window").unwrap(),
        }
    );
    assert_eq!(
        beta_exports[1].parameter_types,
        vec![galfus_contract::BoundaryType::Handle {
            type_id: galfus_core::OpaqueTypeId::new("beta", "Window").unwrap(),
        }]
    );
    let report = workspace.compile().expect("workspace compiles");
    let handles = report
        .package
        .graph()
        .modules()
        .flat_map(|module| module.module().types.iter())
        .filter_map(|ty| match ty {
            galfus_bytecode::BytecodeType::AdapterHandle(type_id) => Some(type_id),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert!(
        handles
            .iter()
            .any(|type_id| **type_id == galfus_core::OpaqueTypeId::new("alpha", "Window").unwrap()),
        "{handles:?}"
    );
    assert!(
        handles
            .iter()
            .any(|type_id| **type_id == galfus_core::OpaqueTypeId::new("beta", "Window").unwrap()),
        "{handles:?}"
    );
}
