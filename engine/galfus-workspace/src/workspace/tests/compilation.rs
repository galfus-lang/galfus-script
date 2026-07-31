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

    assert_eq!(report.graph.len(), 2);
    assert_eq!(report.graph.edges().len(), 1);

    let main = report
        .graph
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
    let first_graph = workspace.compile().expect("initial compilation").graph;
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
    let updated_graph = workspace.compile().expect("incremental compilation").graph;

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
    let deleted_graph = workspace
        .compile()
        .expect("compilation after deletion")
        .graph;

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
    let first = workspace.compile().expect("initial compilation").graph;
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
    let updated = workspace.compile().expect("incremental compilation").graph;

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
    let graph1 = workspace.compile().unwrap().graph;
    assert!(graph1.modules().any(|m| m.path().as_str() == "a.gfs"));

    // Remove import
    workspace.load_module("main.gfs", b"const x = 2;").unwrap();
    let report = workspace.check();
    assert!(report.is_valid, "{:?}", report.diagnostics);
    let graph2 = workspace.compile().unwrap().graph;

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
fn run_reports_missing_io_provider_only_when_io_is_executed() {
    let mut workspace = Workspace::new();
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
    let exit_error = Arc::new(Mutex::new(String::new()));
    let ee = Arc::clone(&exit_error);
    executor.on_exit(Box::new(move |res| {
        if let Err(e) = res {
            *ee.lock().unwrap() = e.to_string();
        }
    }));
    workspace.run(&[], None, executor).unwrap();
    let error = exit_error.lock().unwrap().clone();
    assert!(error.contains("HostProvider missing"));
}
