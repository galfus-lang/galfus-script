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

    assert!(matches!(
        error,
        BytecodeGraphTransactionError::InvalidGraph(
            BytecodeGraphValidationError::MissingImportedModule { .. }
        )
    ));
    assert_eq!(graph.version(), 0);
    assert!(graph.is_empty());
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
