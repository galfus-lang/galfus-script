use galfus_core::{ModulePath, RowCol};
use lsp_types::{Hover, HoverContents, MarkedString, Position};

use crate::workspace::Workspace;

pub fn hover(workspace: &Workspace, path: &str, position: Position) -> Option<Hover> {
    let module_path = ModulePath::new(path)?;
    let snapshot = workspace.frontend_snapshot()?;
    let semantic_graph = snapshot.semantic_graph();

    let module_id = semantic_graph.module_by_path(&module_path)?;
    let semantic_module = semantic_graph.get(module_id)?;
    let source = semantic_module.source();

    // Map LSP Position to byte offset
    // Position is 0-indexed. RowCol is 1-indexed.
    let row_col = RowCol {
        row: (position.line + 1) as usize,
        column: (position.character + 1) as usize,
    };

    let offset = source.offset(&row_col)?;

    let syntax_graph = semantic_module.graph().syntax();

    // Find the deepest node containing the offset
    let mut current_node = syntax_graph.root()?;

    loop {
        let mut found_child = false;
        for child_id in syntax_graph.node(current_node)?.children() {
            let child = syntax_graph.node(*child_id)?;
            let span = child.span();
            if span.start() <= offset && offset < span.end() {
                current_node = *child_id;
                found_child = true;
                break;
            }
        }
        if !found_child {
            break;
        }
    }

    // Now current_node is the deepest node under the cursor.
    let type_result = semantic_module.type_result()?;
    let node_type_id = type_result.layer().node_type(current_node)?;

    let type_name = format!("Type ID: {:?}", node_type_id);
    let hover_text = format!(
        "**Galfus Node**: `{:?}`\n\nType: {}",
        syntax_graph.node(current_node)?.kind(),
        type_name
    );

    Some(Hover {
        contents: HoverContents::Scalar(MarkedString::String(hover_text)),
        range: None,
    })
}
