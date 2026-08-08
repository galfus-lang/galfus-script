use crate::ImportKind;
use crate::instruction;

use super::*;
use crate::{BytecodeModule, ConstantPool, ImportSlot};
use std::collections::HashMap;

fn compiled_module(id: ModuleId, revision: SemanticRevision) -> BytecodeNode {
    BytecodeNode {
        id,
        path: ModulePath::new(format!("src/{}.gfs", id.raw()).as_str()).expect("valid path"),
        semantic_revision: revision,
        module: BytecodeModule {
            name: id.raw().to_string(),
            constants: ConstantPool::default(),
            functions: Vec::new(),
            types: Vec::new(),
            struct_layouts: Vec::new(),
            choice_layouts: Vec::new(),
            imports: Vec::new(),
            exports: Vec::new(),
            init_func_idx: None,
        },
        metadata: None,
    }
}

fn transaction(
    graph: &BytecodeGraph,
    revision: SemanticRevision,
    upserted_modules: Vec<BytecodeNode>,
    removed_modules: Vec<ModuleId>,
    edges: Vec<ImportEdge>,
) -> BytecodeGraphTransaction {
    BytecodeGraphTransaction {
        base_version: graph.version(),
        semantic_revision: revision,
        upserted_modules,
        removed_modules,
        edges,
    }
}

#[test]
fn apply_returns_a_new_validated_snapshot() {
    let main = ModuleId::new(41);
    let utilities = ModuleId::new(7);
    let graph = BytecodeGraph::new();

    let next = graph
        .apply(transaction(
            &graph,
            SemanticRevision::new(3),
            vec![
                compiled_module(main, SemanticRevision::new(3)),
                compiled_module(utilities, SemanticRevision::new(2)),
            ],
            vec![],
            vec![ImportEdge {
                from: main,
                to: utilities,
            }],
        ))
        .expect("transaction is valid");

    assert_eq!(graph.version(), 0);
    assert!(graph.is_empty());
    assert_eq!(next.version(), 1);
    assert_eq!(
        next.get(main).map(BytecodeNode::semantic_revision),
        Some(SemanticRevision::new(3))
    );
    assert_eq!(next.deps_of(main).collect::<Vec<_>>(), vec![utilities]);
    assert_eq!(next.dependents_of(utilities), vec![main]);
}

#[test]
fn modules_are_exposed_in_canonical_module_id_order() {
    let first = ModuleId::new(37);
    let second = ModuleId::new(4);
    let third = ModuleId::new(19);

    let graph = BytecodeGraph::new();
    let inserted_forward = graph
        .apply(transaction(
            &graph,
            SemanticRevision::new(1),
            vec![
                compiled_module(first, SemanticRevision::new(1)),
                compiled_module(second, SemanticRevision::new(1)),
                compiled_module(third, SemanticRevision::new(1)),
            ],
            vec![],
            vec![],
        ))
        .expect("transaction is valid");
    let graph = BytecodeGraph::new();
    let inserted_reverse = graph
        .apply(transaction(
            &graph,
            SemanticRevision::new(1),
            vec![
                compiled_module(third, SemanticRevision::new(1)),
                compiled_module(second, SemanticRevision::new(1)),
                compiled_module(first, SemanticRevision::new(1)),
            ],
            vec![],
            vec![],
        ))
        .expect("transaction is valid");

    let ordered_ids = vec![second, third, first];
    assert_eq!(
        inserted_forward
            .modules()
            .map(BytecodeNode::id)
            .collect::<Vec<_>>(),
        ordered_ids
    );
    assert_eq!(
        inserted_reverse
            .modules()
            .map(BytecodeNode::id)
            .collect::<Vec<_>>(),
        ordered_ids
    );
}

#[test]
fn apply_rejects_a_stale_transaction_without_changing_the_snapshot() {
    let module = ModuleId::new(1);
    let graph = BytecodeGraph::new();
    let next = graph
        .apply(transaction(
            &graph,
            SemanticRevision::new(1),
            vec![compiled_module(module, SemanticRevision::new(1))],
            vec![],
            vec![],
        ))
        .expect("initial transaction is valid");

    let error = next
        .apply(BytecodeGraphTransaction {
            base_version: graph.version(),
            semantic_revision: SemanticRevision::new(2),
            upserted_modules: vec![],
            removed_modules: vec![module],
            edges: vec![],
        })
        .expect_err("stale transaction must fail");

    assert!(matches!(
        error,
        BytecodeGraphTransactionError::StaleBaseVersion {
            expected: 0,
            actual: 1
        }
    ));
    assert_eq!(next.version(), 1);
    assert!(next.get(module).is_some());
}

#[test]
fn apply_rejects_conflicting_module_operations() {
    let existing = ModuleId::new(1);
    let graph = BytecodeGraph::new()
        .apply(transaction(
            &BytecodeGraph::new(),
            SemanticRevision::new(1),
            vec![compiled_module(existing, SemanticRevision::new(1))],
            vec![],
            vec![],
        ))
        .expect("initial transaction is valid");
    let duplicate = compiled_module(ModuleId::new(2), SemanticRevision::new(2));
    let mut same_path = compiled_module(ModuleId::new(3), SemanticRevision::new(2));
    same_path.path = duplicate.path.clone();
    let mut retained_path = compiled_module(ModuleId::new(4), SemanticRevision::new(2));
    retained_path.path = graph.get(existing).expect("existing module").path.clone();

    assert!(matches!(
        graph.apply(transaction(
            &graph,
            SemanticRevision::new(2),
            vec![],
            vec![existing, existing],
            vec![],
        )),
        Err(BytecodeGraphTransactionError::DuplicateRemovedModule { module_id }) if module_id == existing
    ));
    assert!(matches!(
        graph.apply(transaction(
            &graph,
            SemanticRevision::new(2),
            vec![duplicate.clone(), duplicate.clone()],
            vec![],
            vec![],
        )),
        Err(BytecodeGraphTransactionError::DuplicateUpsertedModule { module_id }) if module_id == duplicate.id
    ));
    assert!(matches!(
        graph.apply(transaction(
            &graph,
            SemanticRevision::new(2),
            vec![duplicate.clone(), same_path.clone()],
            vec![],
            vec![],
        )),
        Err(BytecodeGraphTransactionError::DuplicateUpsertedModulePath { path, .. }) if path == duplicate.path
    ));
    assert!(matches!(
        graph.apply(transaction(
            &graph,
            SemanticRevision::new(2),
            vec![compiled_module(existing, SemanticRevision::new(2))],
            vec![existing],
            vec![],
        )),
        Err(BytecodeGraphTransactionError::ConflictingModuleOperations { module_id }) if module_id == existing
    ));
    assert!(matches!(
        graph.apply(transaction(
            &graph,
            SemanticRevision::new(2),
            vec![retained_path],
            vec![],
            vec![],
        )),
        Err(BytecodeGraphTransactionError::RetainedModulePathConflict { path, existing: found, .. })
            if path == graph.get(existing).expect("existing module").path && found == existing
    ));
    assert_eq!(graph.version(), 1);
    assert!(graph.get(existing).is_some());
}

#[test]
fn apply_rejects_invalid_imports_without_changing_the_snapshot() {
    let module = ModuleId::new(1);
    let graph = BytecodeGraph::new();
    let mut invalid = compiled_module(module, SemanticRevision::new(1));
    invalid.module.imports.push(ImportSlot {
        module_name: "missing.gfs".to_string(),
        symbol_name: "missing".to_string(),
        ty: instruction::TypeIdx(0),
        kind: ImportKind::Function,
    });

    let error = graph
        .apply(transaction(
            &graph,
            SemanticRevision::new(1),
            vec![invalid],
            vec![],
            vec![],
        ))
        .expect_err("invalid import must fail");

    let BytecodeGraphTransactionError::InvalidGraph(errors) = error else {
        panic!("invalid import must fail validation");
    };
    assert!(matches!(
        errors.errors(),
        [BytecodeGraphValidationError::MissingImportedModule { .. }]
    ));
    assert_eq!(graph.version(), 0);
    assert!(graph.is_empty());
}

#[test]
fn validation_collects_all_errors_in_canonical_module_order() {
    let first = ModuleId::new(31);
    let second = ModuleId::new(5);
    let revision = SemanticRevision::new(1);

    let mut first_node = compiled_module(first, revision);
    first_node.module.imports.push(ImportSlot {
        module_name: "z_missing.gfs".to_string(),
        symbol_name: "value".to_string(),
        ty: instruction::TypeIdx(0),
        kind: ImportKind::Function,
    });
    let mut second_node = compiled_module(second, revision);
    second_node.module.imports.push(ImportSlot {
        module_name: "a_missing.gfs".to_string(),
        symbol_name: "value".to_string(),
        ty: instruction::TypeIdx(0),
        kind: ImportKind::Function,
    });

    let forward = BytecodeGraph {
        version: 0,
        modules: HashMap::from([(first, first_node.clone()), (second, second_node.clone())]),
        ids_by_path: HashMap::new(),
        edges: Vec::new(),
    };
    let reverse = BytecodeGraph {
        version: 0,
        modules: HashMap::from([(second, second_node), (first, first_node)]),
        ids_by_path: HashMap::new(),
        edges: Vec::new(),
    };

    let forward_errors = forward.validate().expect_err("graph must be invalid");
    let reverse_errors = reverse.validate().expect_err("graph must be invalid");

    assert_eq!(forward_errors, reverse_errors);
    assert_eq!(
        forward_errors.errors(),
        [
            BytecodeGraphValidationError::MissingImportedModule {
                importer: second,
                module_path: "a_missing.gfs".to_string(),
            },
            BytecodeGraphValidationError::MissingImportedModule {
                importer: first,
                module_path: "z_missing.gfs".to_string(),
            },
        ]
    );
}

#[test]
fn execution_metadata_resolves_the_span_for_an_instruction() {
    let function = instruction::FuncIdx(3);
    let span = galfus_core::Span::new(galfus_core::SourceId::new(9), 18, 27);
    let metadata = ExecutionMetadata {
        spans: HashMap::from([(function, HashMap::from([(4, span)]))]),
    };

    assert_eq!(metadata.span_for(function, 4), Some(span));
    assert_eq!(metadata.span_for(function, 5), None);
}
