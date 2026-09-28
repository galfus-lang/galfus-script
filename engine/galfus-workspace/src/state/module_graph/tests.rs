use super::*;

fn path(value: &str) -> ModulePath {
    ModulePath::new(value).expect("valid module path")
}

#[test]
fn source_changes_invalidate_derived_artifacts_for_the_same_module_id() {
    let mut graph = IncrementalModuleStateGraph::new();
    let module_id = ModuleId::new(41);
    let module_path = path("src/main.gfs");
    let source_hash = ContentHash::of(b"first");
    let interface_hash = ContentHash::of(b"interface");
    let bytecode_hash = ContentHash::of(b"bytecode");

    graph
        .source_loaded(
            module_id,
            module_path.clone(),
            Revision::new(3),
            source_hash,
        )
        .expect("source is indexed");
    graph
        .interface_valid(module_id, SemanticRevision::new(7), interface_hash)
        .expect("loaded source accepts interface");
    graph
        .bytecode_produced(module_id, bytecode_hash)
        .expect("valid interface accepts bytecode");
    graph
        .source_loaded(
            module_id,
            module_path,
            Revision::new(4),
            ContentHash::of(b"second"),
        )
        .expect("same identity accepts newer source");

    let record = graph.get(module_id).expect("state record exists");
    assert_eq!(record.lifecycle(), ModuleLifecycle::SourceLoaded);
    assert_eq!(record.source_revision(), Some(Revision::new(4)));
    assert_eq!(record.semantic_revision(), None);
    assert_eq!(record.interface_hash(), None);
    assert_eq!(record.bytecode_hash(), None);
}

#[test]
fn lifecycle_requires_the_previous_artifact() {
    let mut graph = IncrementalModuleStateGraph::new();
    let module_id = ModuleId::new(42);

    graph
        .index(module_id, path("src/main.gfs"))
        .expect("module identity indexes");

    assert_eq!(
        graph
            .interface_valid(
                module_id,
                SemanticRevision::new(1),
                ContentHash::of(b"interface")
            )
            .expect_err("source is required"),
        ModuleStateError::SourceRequired { module_id }
    );
    assert_eq!(
        graph
            .bytecode_cached(module_id, ContentHash::of(b"bytecode"))
            .expect_err("valid interface is required"),
        ModuleStateError::SourceRequired { module_id }
    );
}

#[test]
fn changed_interface_invalidates_cached_bytecode() {
    let mut graph = IncrementalModuleStateGraph::new();
    let module_id = ModuleId::new(44);

    graph
        .source_loaded(
            module_id,
            path("src/main.gfs"),
            Revision::new(1),
            ContentHash::of(b"source"),
        )
        .expect("source loads");
    graph
        .interface_valid(
            module_id,
            SemanticRevision::new(1),
            ContentHash::of(b"first"),
        )
        .expect("interface validates");
    graph
        .bytecode_cached(module_id, ContentHash::of(b"bytecode"))
        .expect("bytecode caches");
    graph
        .interface_valid(
            module_id,
            SemanticRevision::new(2),
            ContentHash::of(b"second"),
        )
        .expect("new interface validates");

    let record = graph.get(module_id).expect("state record exists");
    assert_eq!(record.lifecycle(), ModuleLifecycle::InterfaceValid);
    assert_eq!(record.bytecode_hash(), None);
}

#[test]
fn module_id_cannot_be_rebound_to_another_path() {
    let mut graph = IncrementalModuleStateGraph::new();
    let module_id = ModuleId::new(43);
    let original = path("src/main.gfs");
    let attempted = path("src/other.gfs");

    graph
        .index(module_id, original.clone())
        .expect("module identity indexes");

    assert_eq!(
        graph
            .index(module_id, attempted.clone())
            .expect_err("identity conflict is explicit"),
        ModuleStateError::ModulePathMismatch {
            module_id,
            existing: original,
            attempted,
        }
    );
}
