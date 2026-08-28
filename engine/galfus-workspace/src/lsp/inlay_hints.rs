use galfus_core::{ModulePath, NodeId, RowCol};
use galfus_frontend::{SymbolKind, SyntaxNodeKind, TypeKind};
use lsp_types::{InlayHint, InlayHintKind, Position, Range};

use crate::{lsp::hover::format_type, workspace::Workspace};

pub fn inlay_hints(workspace: &Workspace, path: &str, range: Range) -> Option<Vec<InlayHint>> {
    let module_path = ModulePath::new(path)?;
    let snapshot = workspace.frontend_snapshot()?;
    let semantic_graph = snapshot.semantic_graph();
    let module_id = semantic_graph.module_by_path(&module_path)?;
    let module = semantic_graph.get(module_id)?;
    let source = module.source();
    let syntax = module.graph().syntax();
    let resolution = module.graph().resolution()?;
    let type_result = module.type_result()?;
    let mut hints = Vec::new();

    for (index, declaration) in syntax.nodes().iter().enumerate() {
        if !matches!(
            declaration.kind(),
            SyntaxNodeKind::VarItem
                | SyntaxNodeKind::ConstItem
                | SyntaxNodeKind::VarStatement
                | SyntaxNodeKind::ConstStatement
        ) || syntax
            .first_child_of_kind(NodeId::new(index as u32), SyntaxNodeKind::TypeAnnotation)
            .is_some()
        {
            continue;
        }

        let Some(binding) =
            syntax.first_child_of_kind(NodeId::new(index as u32), SyntaxNodeKind::BindingPattern)
        else {
            continue;
        };

        let mut identifiers = Vec::new();
        collect_binding_identifiers(syntax, binding, &mut identifiers);
        for identifier in identifiers {
            let Some(symbol) = resolution.declaration_symbol(identifier) else {
                continue;
            };
            let Some(symbol_data) = resolution.symbol(symbol) else {
                continue;
            };
            if !matches!(symbol_data.kind(), SymbolKind::Var | SymbolKind::Const) {
                continue;
            }
            let Some(type_id) = type_result.layer().symbol_type(symbol) else {
                continue;
            };
            if matches!(
                type_result.layer().table().kind(type_id),
                Some(TypeKind::Error)
            ) {
                continue;
            }

            let identifier_node = syntax.node(identifier)?;
            let Some(row_col) = source.row_col(identifier_node.span().end()) else {
                continue;
            };
            let position = position(row_col);
            if !contains(range, position) {
                continue;
            }

            let label = format_type(workspace, snapshot, module, type_id, false);
            if label == "unknown" || label == "error" {
                continue;
            }
            hints.push(InlayHint {
                position,
                label: format!(": {label}").into(),
                kind: Some(InlayHintKind::TYPE),
                text_edits: None,
                tooltip: None,
                padding_left: Some(false),
                padding_right: Some(true),
                data: None,
            });
        }
    }
    Some(hints)
}

fn collect_binding_identifiers(
    syntax: &galfus_frontend::SyntaxLayer,
    node: NodeId,
    identifiers: &mut Vec<NodeId>,
) {
    let Some(syntax_node) = syntax.node(node) else {
        return;
    };
    if syntax_node.kind() == SyntaxNodeKind::Identifier {
        identifiers.push(node);
        return;
    }
    for child in syntax_node.children() {
        collect_binding_identifiers(syntax, *child, identifiers);
    }
}

fn position(row_col: RowCol) -> Position {
    Position::new((row_col.row - 1) as u32, (row_col.column - 1) as u32)
}

fn contains(range: Range, position: Position) -> bool {
    (position.line, position.character) >= (range.start.line, range.start.character)
        && (position.line, position.character) <= (range.end.line, range.end.character)
}
