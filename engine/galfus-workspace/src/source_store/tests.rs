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
fn reloading_a_removed_path_allocates_fresh_ids() {
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

    assert_ne!(reloaded.0, initial.0);
    assert_ne!(reloaded.1, initial.1);
}
