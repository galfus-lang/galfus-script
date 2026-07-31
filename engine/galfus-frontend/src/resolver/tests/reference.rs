mod bindings;
mod diagnostics;
mod paths;
mod patterns;

use super::*;
use crate::{PathReferenceKind, SyntaxLayer};
use galfus_core::NodeId;

fn find_name_expression_by_text(
    syntax: &SyntaxLayer,
    source: &SourceFile,
    node: NodeId,
    text: &str,
) -> Option<NodeId> {
    let syntax_node = syntax.node(node)?;

    if syntax_node.kind() == SyntaxNodeKind::NameExpression
        && let Some(identifier) = syntax.first_child_of_kind(node, SyntaxNodeKind::Identifier)
        && source.slice(syntax.node(identifier)?.span()) == Some(text)
    {
        return Some(node);
    }

    for child in syntax_node.children() {
        if let Some(found) = find_name_expression_by_text(syntax, source, *child, text) {
            return Some(found);
        }
    }

    None
}

fn find_path_expression_by_text(
    syntax: &SyntaxLayer,
    source: &SourceFile,
    node: NodeId,
    text: &str,
) -> Option<NodeId> {
    let syntax_node = syntax.node(node)?;

    if syntax_node.kind() == SyntaxNodeKind::PathExpression
        && source.slice(syntax_node.span()) == Some(text)
    {
        return Some(node);
    }

    for child in syntax_node.children() {
        if let Some(found) = find_path_expression_by_text(syntax, source, *child, text) {
            return Some(found);
        }
    }

    None
}

fn find_node_by_kind_and_text(
    syntax: &SyntaxLayer,
    source: &SourceFile,
    node: NodeId,
    kind: SyntaxNodeKind,
    text: &str,
) -> Option<NodeId> {
    let syntax_node = syntax.node(node)?;

    if syntax_node.kind() == kind && source.slice(syntax_node.span()) == Some(text) {
        return Some(node);
    }

    for child in syntax_node.children() {
        if let Some(found) = find_node_by_kind_and_text(syntax, source, *child, kind, text) {
            return Some(found);
        }
    }

    None
}

fn find_node_by_kind(syntax: &SyntaxLayer, node: NodeId, kind: SyntaxNodeKind) -> Option<NodeId> {
    let syntax_node = syntax.node(node)?;

    if syntax_node.kind() == kind {
        return Some(node);
    }

    for child in syntax_node.children() {
        if let Some(found) = find_node_by_kind(syntax, *child, kind) {
            return Some(found);
        }
    }

    None
}
