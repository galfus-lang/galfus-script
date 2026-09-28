use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_FIXTURE_ID: AtomicUsize = AtomicUsize::new(0);

#[test]
fn small_fixture_leaves_disconnected_modules_without_imports() {
    let root = test_root();
    let fixture = build_fixture(root.as_path(), 10, ClosureScope::Small).expect("build fixture");

    assert_eq!(fixture.entry_closure_modules, SMALL_CLOSURE_MODULES);
    let root_module =
        std::fs::read_to_string(fixture.path.join("src/module_00000.gfs")).expect("root module");
    let disconnected = std::fs::read_to_string(fixture.path.join("src/module_00008.gfs"))
        .expect("disconnected module");
    assert!(root_module.contains("module_00001.gfs"));
    assert!(!disconnected.contains("import"));
}

#[test]
fn all_reachable_fixture_connects_every_generated_module() {
    let root = test_root();
    let fixture =
        build_fixture(root.as_path(), 10, ClosureScope::AllReachable).expect("build fixture");

    assert_eq!(fixture.entry_closure_modules, 10);
    let module = std::fs::read_to_string(fixture.path.join("src/module_00003.gfs"))
        .expect("intermediate module");
    assert!(module.contains("module_00007.gfs"));
    assert!(module.contains("module_00008.gfs"));
}

fn test_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".tmp")
        .join("benchmark")
        .join(format!(
            "workspace-scaling-fixture-test-{}",
            NEXT_FIXTURE_ID.fetch_add(1, Ordering::Relaxed)
        ))
}
