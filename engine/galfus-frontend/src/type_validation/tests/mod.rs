mod access;
mod assignments;
mod calls;
mod constraints;
mod control_flow;
mod declarations;
mod decorators;
mod function_expressions;
mod function_stamps;
mod generic_expressions;
mod hardening;
mod initializers;
mod instanceof;
mod iteration_constraints;
mod iteration_iterators;
mod iteration_loops;
mod iteration_ranges;
mod literals;
mod matches;
mod operators;
mod returns;
mod structs;
mod type_aliases;
mod typeofs;
mod variants;

use super::*;
use crate::type_validation::check_declaration_types;
use crate::type_validation::check_definition_types;
use crate::{
    ModuleAst, PrimitiveType, SymbolKind, SyntaxNodeKind, TypeDiagnosticCode, TypeKind, parse,
    resolve,
};
use galfus_core::{DiagnosticCodeKind, NodeId, SourceId, SymbolId};

fn source(text: &str) -> SourceFile {
    SourceFile::new(SourceId::new(0), "test.gfs".to_string(), text.to_string())
}

fn check_source(text: &str) -> (SourceFile, ModuleAst, TypeCheckResult, crate::StringTable) {
    let source = source(text);

    let parse_result = parse(&source);
    assert!(
        !parse_result.has_errors(),
        "{:?}",
        parse_result.diagnostics()
    );

    let mut string_table = crate::StringTable::new();
    let resolve_result = resolve(&source, parse_result.into_ast(), &mut string_table);
    assert!(
        !resolve_result.has_errors(),
        "{:?}",
        resolve_result.diagnostics()
    );

    let graph = resolve_result.into_ast();
    let result = check_declaration_types(&source, &graph, &string_table, false);
    let result = check_definition_types(&source, &graph, result, &string_table, false);

    assert!(!result.has_errors(), "{:?}", result.diagnostics());

    (source, graph, result, string_table)
}

fn proxy_source(text: &str) -> SourceFile {
    SourceFile::new(SourceId::new(0), "test.gfp".to_string(), text.to_string())
}

fn check_proxy_source(text: &str) -> (SourceFile, ModuleAst, TypeCheckResult, crate::StringTable) {
    let source = proxy_source(text);

    let parse_result = parse(&source);
    assert!(
        !parse_result.has_errors(),
        "{:?}",
        parse_result.diagnostics()
    );

    let mut string_table = crate::StringTable::new();
    let resolve_result = resolve(&source, parse_result.into_ast(), &mut string_table);
    assert!(
        !resolve_result.has_errors(),
        "{:?}",
        resolve_result.diagnostics()
    );

    let graph = resolve_result.into_ast();
    let result = check_declaration_types(&source, &graph, &string_table, false);
    let result = check_definition_types(&source, &graph, result, &string_table, false);

    (source, graph, result, string_table)
}

fn check_source_named(
    name: &str,
    text: &str,
) -> (SourceFile, ModuleAst, TypeCheckResult, crate::StringTable) {
    let source = SourceFile::new(SourceId::new(0), name.to_string(), text.to_string());

    let parse_result = parse(&source);
    let mut string_table = crate::StringTable::new();
    let resolve_result = resolve(&source, parse_result.into_ast(), &mut string_table);

    let graph = resolve_result.into_ast();
    let result = check_declaration_types(&source, &graph, &string_table, false);
    let result = check_definition_types(&source, &graph, result, &string_table, false);

    (source, graph, result, string_table)
}

fn symbol_by_name_and_kind(
    graph: &ModuleAst,
    name: &str,
    kind: SymbolKind,
    string_table: &crate::StringTable,
) -> SymbolId {
    let resolution = graph.resolution().unwrap();

    resolution
        .symbols()
        .iter()
        .find(|symbol| {
            string_table.resolve(symbol.name()).unwrap_or("") == name && symbol.kind() == kind
        })
        .map(|symbol| symbol.id())
        .unwrap_or_else(|| panic!("missing symbol `{name}` of kind {kind:?}"))
}

fn find_node_by_kind_and_text(
    source: &SourceFile,
    graph: &ModuleAst,
    kind: SyntaxNodeKind,
    text: &str,
) -> Option<NodeId> {
    let root = graph.syntax().root()?;
    find_node_by_kind_and_text_from(source, graph, root, kind, text)
}

fn find_node_by_kind_and_text_from(
    source: &SourceFile,
    graph: &ModuleAst,
    node: NodeId,
    kind: SyntaxNodeKind,
    text: &str,
) -> Option<NodeId> {
    let syntax_node = graph.syntax().node(node)?;

    if syntax_node.kind() == kind && source.slice(syntax_node.span()) == Some(text) {
        return Some(node);
    }

    for child in syntax_node.children() {
        if let Some(found) = find_node_by_kind_and_text_from(source, graph, *child, kind, text) {
            return Some(found);
        }
    }

    None
}

fn find_node_by_kind(graph: &ModuleAst, kind: SyntaxNodeKind) -> Option<NodeId> {
    let root = graph.syntax().root()?;
    find_node_by_kind_from(graph, root, kind)
}

fn find_node_by_kind_from(graph: &ModuleAst, node: NodeId, kind: SyntaxNodeKind) -> Option<NodeId> {
    let syntax_node = graph.syntax().node(node)?;

    if syntax_node.kind() == kind {
        return Some(node);
    }

    for child in syntax_node.children() {
        if let Some(found) = find_node_by_kind_from(graph, *child, kind) {
            return Some(found);
        }
    }

    None
}
mod proxy_modules;
