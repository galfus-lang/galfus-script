use galfus_core::{ModulePath, RowCol, SymbolId, TypeId};
use galfus_frontend::ResolutionLayer;
use galfus_frontend::modules::{FrontendSnapshot, SemanticModule};
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

    let type_name = format_type(&snapshot, semantic_module, node_type_id);
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

fn format_type(snapshot: &FrontendSnapshot, module: &SemanticModule, type_id: TypeId) -> String {
    let Some(type_result) = module.type_result() else {
        return "<unknown>".to_string();
    };

    let table = type_result.layer().table();
    let Some(kind) = table.kind(type_id) else {
        return "<unknown>".to_string();
    };

    let resolution = module.graph().resolution();

    match kind {
        galfus_frontend::TypeKind::Primitive(p) => p.name().to_string(),
        galfus_frontend::TypeKind::Named { symbol } => {
            get_symbol_name(snapshot, resolution, *symbol).unwrap_or_else(|| "unknown".to_string())
        }
        galfus_frontend::TypeKind::Path { root, segments } => {
            let root_name = get_symbol_name(snapshot, resolution, *root)
                .unwrap_or_else(|| "unknown".to_string());
            let path = segments.join("::");
            format!("{}::{}", root_name, path)
        }
        galfus_frontend::TypeKind::GenericParameter { symbol } => {
            get_symbol_name(snapshot, resolution, *symbol).unwrap_or_else(|| "unknown".to_string())
        }
        galfus_frontend::TypeKind::Array { element } => {
            format!("[{}]", format_type(snapshot, module, *element))
        }
        galfus_frontend::TypeKind::Range { element } => {
            format!("range<{}>", format_type(snapshot, module, *element))
        }
        galfus_frontend::TypeKind::Tuple { elements } => {
            let elems: Vec<String> = elements
                .iter()
                .map(|e| format_type(snapshot, module, *e))
                .collect();
            format!("({})", elems.join(", "))
        }
        galfus_frontend::TypeKind::Union { members } => {
            let members: Vec<String> = members
                .iter()
                .map(|m| format_type(snapshot, module, *m))
                .collect();
            members.join(" | ")
        }
        galfus_frontend::TypeKind::Function(f) => {
            let params: Vec<String> = f
                .parameters()
                .iter()
                .map(|p| {
                    let mut text = format_type(snapshot, module, p.ty());
                    if p.is_rest() {
                        text = format!("...{}", text);
                    }
                    if p.has_default() {
                        text = format!("{} =", text);
                    }
                    text
                })
                .collect();
            format!(
                "fn({}): {}",
                params.join(", "),
                format_type(snapshot, module, f.return_type())
            )
        }
        galfus_frontend::TypeKind::GenericInstance { base, arguments } => {
            let base_name = format_type(snapshot, module, *base);
            let args: Vec<String> = arguments
                .iter()
                .map(|a| format_type(snapshot, module, *a))
                .collect();
            format!("{}<{}>", base_name, args.join(", "))
        }
        galfus_frontend::TypeKind::Error => "<error>".to_string(),
    }
}

fn get_symbol_name(
    snapshot: &FrontendSnapshot,
    resolution: Option<&ResolutionLayer>,
    symbol_id: SymbolId,
) -> Option<String> {
    let res = resolution?;
    let sym = res.symbol(symbol_id)?;
    let name = snapshot.string_table().resolve(sym.name())?;
    Some(name.to_string())
}
