mod diagnostics;
mod function_types;
mod generics;
mod path_types;

use super::*;
use crate::SyntaxLayer;
use galfus_core::NodeId;

fn source(text: &str) -> SourceFile {
    SourceFile::new(SourceId::new(0), "main.gfs".to_string(), text.to_string())
}

fn find_named_type_by_text(
    syntax: &SyntaxLayer,
    source: &SourceFile,
    node: NodeId,
    text: &str,
) -> Option<NodeId> {
    let syntax_node = syntax.node(node)?;

    if syntax_node.kind() == SyntaxNodeKind::NamedType
        && let Some(identifier) = syntax.first_child_of_kind(node, SyntaxNodeKind::Identifier)
        && source.slice(syntax.node(identifier)?.span()) == Some(text)
    {
        return Some(node);
    }

    for child in syntax_node.children() {
        if let Some(found) = find_named_type_by_text(syntax, source, *child, text) {
            return Some(found);
        }
    }

    None
}

fn collect_named_types_by_text(
    syntax: &SyntaxLayer,
    source: &SourceFile,
    node: NodeId,
    text: &str,
    found: &mut Vec<NodeId>,
) {
    let Some(syntax_node) = syntax.node(node) else {
        return;
    };

    if syntax_node.kind() == SyntaxNodeKind::NamedType
        && let Some(identifier) = syntax.first_child_of_kind(node, SyntaxNodeKind::Identifier)
        && source.slice(syntax.node(identifier).unwrap().span()) == Some(text)
    {
        found.push(node);
    }

    for child in syntax_node.children() {
        collect_named_types_by_text(syntax, source, *child, text, found);
    }
}

fn find_path_type_by_text(
    syntax: &SyntaxLayer,
    source: &SourceFile,
    node: NodeId,
    text: &str,
) -> Option<NodeId> {
    let syntax_node = syntax.node(node)?;

    if syntax_node.kind() == SyntaxNodeKind::Path && source.slice(syntax_node.span()) == Some(text)
    {
        return Some(node);
    }

    for child in syntax_node.children() {
        if let Some(found) = find_path_type_by_text(syntax, source, *child, text) {
            return Some(found);
        }
    }

    None
}

fn find_function_anchor_by_text(
    syntax: &SyntaxLayer,
    source: &SourceFile,
    node: NodeId,
    text: &str,
) -> Option<NodeId> {
    let syntax_node = syntax.node(node)?;

    if syntax_node.kind() == SyntaxNodeKind::FunctionAnchor
        && source.slice(syntax_node.span()) == Some(text)
    {
        return Some(node);
    }

    for child in syntax_node.children() {
        if let Some(found) = find_function_anchor_by_text(syntax, source, *child, text) {
            return Some(found);
        }
    }

    None
}
