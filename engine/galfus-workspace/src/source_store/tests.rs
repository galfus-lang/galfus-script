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

    assert_eq!(
        reloaded.0, initial.0,
        "ModuleId must be stable across reloads"
    );
    assert_eq!(
        reloaded.1, initial.1,
        "SourceId must be stable across reloads"
    );
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

#[test]
fn reload_updates_the_origin_and_bytes() {
    let mut store = SourceStore::new();
    let module_path = path("src/mod.gfs");

    store
        .load_module(
            module_path.clone(),
            Arc::from(&b"v1"[..]),
            ModuleOrigin::User,
            Revision::new(1),
        )
        .expect("first load");

    store
        .load_module(
            module_path.clone(),
            Arc::from(&b"v2"[..]),
            ModuleOrigin::Builtin,
            Revision::new(2),
        )
        .expect("second load should succeed, updating the entry");

    let entry = store.get(&module_path).expect("still present");
    assert_eq!(&*entry.bytes, b"v2");
    assert_eq!(entry.revision, Revision::new(2));
    assert_eq!(entry.origin, ModuleOrigin::Builtin);
}

#[test]
fn remove_makes_path_absent() {
    let mut store = SourceStore::new();
    let module_path = path("src/gone.gfs");

    store
        .load_module(
            module_path.clone(),
            Arc::from(&b"data"[..]),
            ModuleOrigin::User,
            Revision::new(1),
        )
        .expect("load");

    let removed = store.remove_module(&module_path);
    assert!(removed.is_some(), "remove returns the entry");
    assert!(
        store.get(&module_path).is_none(),
        "entry is gone after remove"
    );
}

#[test]
fn iter_returns_entries_sorted_by_module_id() {
    let mut store = SourceStore::new();
    store
        .load_module(
            path("src/a.gfs"),
            Arc::from(&b"a"[..]),
            ModuleOrigin::User,
            Revision::new(1),
        )
        .unwrap();
    store
        .load_module(
            path("src/b.gfs"),
            Arc::from(&b"b"[..]),
            ModuleOrigin::User,
            Revision::new(1),
        )
        .unwrap();
    store
        .load_module(
            path("src/c.gfs"),
            Arc::from(&b"c"[..]),
            ModuleOrigin::User,
            Revision::new(1),
        )
        .unwrap();

    let ids: Vec<u32> = store.iter().map(|e| e.module_id.raw()).collect();
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(
        ids, sorted,
        "iter() must yield entries in ascending ModuleId order"
    );
}
