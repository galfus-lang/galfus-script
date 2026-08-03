use super::*;

fn path(value: &str) -> ModulePath {
    ModulePath::new(value).expect("valid module path")
}

#[test]
fn updating_a_path_preserves_its_stable_ids() {
    let mut store = SourceStore::new();
    let module_path = path("src/main.gfs");

    let initial = store
        .load_module(
            module_path.clone(),
            Arc::from(&b"fn main(): i32 { return 1 }"[..]),
            ModuleOrigin::User,
            Revision::new(1),
        )
        .expect("load returns IDs");
    let updated = store
        .load_module(
            module_path.clone(),
            Arc::from(&b"fn main(): i32 { return 2 }"[..]),
            ModuleOrigin::User,
            Revision::new(2),
        )
        .expect("update returns IDs");

    assert_eq!(updated, initial);
    let entry = store.get(&module_path).expect("stored module");
    assert_eq!(entry.module_id, initial.0);
    assert_eq!(entry.source_id, initial.1);
    assert_eq!(entry.revision, Revision::new(2));
    assert_eq!(&*entry.bytes, b"fn main(): i32 { return 2 }");
}

#[test]
fn reloading_a_removed_path_preserves_stable_ids() {
    let mut store = SourceStore::new();
    let module_path = path("src/main.gfs");

    let initial = store
        .load_module(
            module_path.clone(),
            Arc::from(&b"first"[..]),
            ModuleOrigin::User,
            Revision::new(1),
        )
        .expect("load returns IDs");
    store.remove_module(&module_path).expect("module exists");
    let reloaded = store
        .load_module(
            module_path,
            Arc::from(&b"second"[..]),
            ModuleOrigin::User,
            Revision::new(2),
        )
        .expect("load returns IDs");

    assert_eq!(reloaded.0, initial.0, "ModuleId must be stable across reloads");
    assert_eq!(reloaded.1, initial.1, "SourceId must be stable across reloads");
}

#[test]
fn colliding_paths_return_a_deterministic_error() {
    let mut store = SourceStore::new();

    // These two paths produce the exact same 32-bit FNV-1a hash for ModuleId
    let path_a = path("src/f74958.gfs");
    let path_b = path("src/f438592.gfs");

    store
        .load_module(
            path_a.clone(),
            Arc::from(&b"first"[..]),
            ModuleOrigin::User,
            Revision::new(1),
        )
        .expect("load returns IDs");

    let collision_error = store
        .load_module(
            path_b.clone(),
            Arc::from(&b"second"[..]),
            ModuleOrigin::User,
            Revision::new(2),
        )
        .expect_err("should return collision error");

    assert_eq!(
        collision_error,
        LoadModuleError::Collision {
            attempted: path_b,
            existing: path_a,
        }
    );
}
