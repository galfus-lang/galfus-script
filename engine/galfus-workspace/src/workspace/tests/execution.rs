use super::compilation::io_catalog;
use super::*;

#[test]
fn run_passes_read_terminator_to_the_io_provider() {
    let mut workspace = Workspace::new();
    workspace.set_catalog(io_catalog(galfus_contract::STD_IO_SOURCE));
    workspace
        .load_config(
            br#"
            [module]
            name = "read-terminator"
            target = "app"
            [entry]
            path = "main.gfs"
            "#,
        )
        .expect("valid configuration");
    workspace
        .load_module(
            "main.gfs",
            br#"
            import { read } from "std/io"

            export fn main(args: [[u8]]): i32 {
                read("!")
                return 0
            }
            "#,
        )
        .expect("valid entry module");

    assert!(workspace.check().is_valid);
    workspace.compile().expect("workspace compiles");

    let terminator = Arc::new(Mutex::new(Vec::new()));
    let providers = Providers::with_host(Box::new(TerminatorIo {
        terminator: Arc::clone(&terminator),
    }));
    let executor = std::rc::Rc::new(CooperativeDriver::new());
    let code = workspace
        .run(&[], Some(providers), executor)
        .expect("entry executes");
    assert_eq!(code, galfus_contract::BoundaryValue::I32(0));
    assert_eq!(*terminator.lock().expect("terminator state"), b"!");
}

#[test]
fn run_specializes_nested_generic_types_across_modules() {
    let mut workspace = Workspace::new();
    workspace
        .load_config(
            br#"
            [module]
            name = "cross-module-nested-generics"
            target = "app"
            [entry]
            path = "main.gfs"
            "#,
        )
        .expect("valid configuration");
    workspace
        .load_module(
            "main.gfs",
            br#"
            import { identity } from "./generic"

            export fn main(args: [[u8]]): i32 {
                var values: [i32] = [32]
                return identity(values).length + 41
            }
            "#,
        )
        .expect("valid entry module");
    workspace
        .load_module(
            "generic.gfs",
            br#"
            export fn identity<T>(values: [T]): [T] {
                return values
            }
            "#,
        )
        .expect("valid generic module");

    let check = workspace.check();
    assert!(check.is_valid, "check diagnostics: {:?}", check.diagnostics);
    workspace.compile().expect("workspace compiles");
    let executor = std::rc::Rc::new(CooperativeDriver::new());
    let code = workspace.run(&[], None, executor).expect("entry executes");
    assert_eq!(code, galfus_contract::BoundaryValue::I32(42));
}

#[test]
fn run_specializes_explicit_imported_generic_typeof_parameter() {
    let mut workspace = Workspace::new();
    workspace
        .load_config(
            br#"
            [module]
            name = "cross-module-typeof"
            target = "app"
            [entry]
            path = "main.gfs"
            "#,
        )
        .expect("valid configuration");
    workspace
        .load_module(
            "main.gfs",
            br#"
            import { dispatch } from "./generic"

            export fn main(args: [[u8]]): i32 {
                return dispatch<i32>(0)
            }
            "#,
        )
        .expect("valid entry module");
    workspace
        .load_module(
            "generic.gfs",
            br#"
            export fn dispatch<T: i32 | bool>(value: T): i32 {
                return typeof T {
                    i32 => 42,
                    bool => 0,
                }
            }
            "#,
        )
        .expect("valid generic module");

    let check = workspace.check();
    assert!(check.is_valid, "check diagnostics: {:?}", check.diagnostics);
    workspace.compile().expect("workspace compiles");
    let executor = std::rc::Rc::new(CooperativeDriver::new());
    let code = workspace.run(&[], None, executor).expect("entry executes");
    assert_eq!(code, galfus_contract::BoundaryValue::I32(42));
}

#[test]
fn run_specializes_generic_anchored_range_iterator_methods() {
    let mut workspace = Workspace::new();
    workspace
        .load_config(
            br#"
            [module]
            name = "generic-range-method"
            target = "app"
            [entry]
            path = "main.gfs"
            "#,
        )
        .expect("valid configuration");
    workspace
        .load_module(
            "main.gfs",
            br#"
            export fn main(args: [[u8]]): i32 {
                var total = 0
                for value in 2::4%2 {
                    total += value
                }
                return total
            }
            "#,
        )
        .expect("valid entry module");

    let check = workspace.check();
    assert!(check.is_valid, "check diagnostics: {:?}", check.diagnostics);
    workspace.compile().expect("workspace compiles");
    let executor = std::rc::Rc::new(CooperativeDriver::new());
    let code = workspace.run(&[], None, executor).expect("entry executes");
    assert_eq!(code, galfus_contract::BoundaryValue::I32(20));
}

#[test]
fn run_synchronizes_the_runtime_module_graph() {
    let mut workspace = Workspace::new();
    workspace
        .load_config(
            br#"
            [module]
            name = "runtime-sync"
            target = "app"
            [entry]
            path = "main.gfs"
            "#,
        )
        .expect("valid configuration");
    workspace
        .load_module(
            "main.gfs",
            "import { helper } from \"./helper\"\nexport fn main(args: [[u8]]): i32 { return helper() }".as_bytes(),
        )
        .expect("valid entry module");
    workspace
        .load_module("helper.gfs", b"export fn helper(): i32 { return 1 }")
        .expect("valid helper module");

    assert!(workspace.check().is_valid);
    let package = workspace.compile().expect("workspace compiles").package;
    let first = package.graph();
    first
        .modules()
        .find(|image| image.path().as_str() == "main.gfs")
        .expect("main image");
    first
        .modules()
        .find(|image| image.path().as_str() == "helper.gfs")
        .expect("helper image");
    let executor = std::rc::Rc::new(CooperativeDriver::new());
    let code = workspace.run(&[], None, executor).expect("entry executes");
    assert_eq!(code, galfus_contract::BoundaryValue::I32(1));

    assert!(matches!(
        workspace.remove_module("helper.gfs"),
        Ok(RemoveResult::Success)
    ));
    workspace
        .load_module(
            "main.gfs",
            b"export fn main(args: [[u8]]): i32 { return 0 }",
        )
        .expect("valid entry module");
    assert!(workspace.check().is_valid);
    workspace.compile().expect("workspace recompiles");
    let executor2 = std::rc::Rc::new(CooperativeDriver::new());
    let code2 = workspace.run(&[], None, executor2).expect("entry executes");
    assert_eq!(code2, galfus_contract::BoundaryValue::I32(0));
}

#[test]
fn run_propagates_runtime_start_error_on_entry_signature_mismatch() {
    let mut workspace = Workspace::new();
    workspace
        .load_config(
            br#"
            [module]
            name = "bad-entry"
            target = "app"
            [entry]
            path = "main.gfs"
            "#,
        )
        .expect("valid configuration");
    workspace
        .load_module(
            "main.gfs",
            br#"
            export fn main(): i32 {
                return 0
            }
            "#,
        )
        .expect("valid entry module");

    assert!(workspace.check().is_valid);
    workspace.compile().expect("workspace compiles");

    let executor = std::rc::Rc::new(CooperativeDriver::new());
    let result = workspace.run(&[], None, executor);

    assert!(matches!(
        result,
        Err(crate::state::WorkspaceRunError::RuntimeStart(
            galfus_runtime::RuntimeError::EntryArityMismatch { .. }
        ))
    ));
}
