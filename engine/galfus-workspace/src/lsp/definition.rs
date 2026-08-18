use crate::workspace::Workspace;
use galfus_core::{ModulePath, RowCol};
use lsp_types::{Location, Position, Range, Url};

pub fn goto_definition(workspace: &Workspace, path: &str, position: Position) -> Option<Location> {
    let module_path = ModulePath::new(path)?;
    let snapshot = workspace.frontend_snapshot()?;
    let semantic_graph = snapshot.semantic_graph();

    let module_id = semantic_graph.module_by_path(&module_path)?;
    let semantic_module = semantic_graph.get(module_id)?;
    let source = semantic_module.source();

    let row_col = RowCol {
        row: (position.line + 1) as usize,
        column: (position.character + 1) as usize,
    };
    let offset = source.offset(&row_col)?;

    let syntax_graph = semantic_module.graph().syntax();
    let mut path_stack = Vec::new();
    let mut current_node = syntax_graph.root()?;

    loop {
        path_stack.push(current_node);
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

    let resolution = semantic_module.graph().resolution()?;

    let mut symbol_id = None;
    let deepest = path_stack.last().copied()?;
    let deepest_node = syntax_graph.node(deepest)?;

    if deepest_node.kind() == galfus_frontend::SyntaxNodeKind::Identifier {
        for &node in path_stack.iter().rev() {
            if let Some(sym) = resolution
                .reference_symbol(node)
                .or_else(|| resolution.path_reference_symbol(node))
                .or_else(|| resolution.type_reference_symbol(node))
                .or_else(|| resolution.type_path_reference_symbol(node))
                .or_else(|| resolution.declaration_symbol(node))
            {
                symbol_id = Some(sym);
                break;
            }
        }
    }

    let symbol_id = symbol_id?;
    let symbol = resolution.symbol(symbol_id)?;

    use galfus_frontend::SymbolKind;

    // Handle imports explicitly
    if matches!(
        symbol.kind(),
        SymbolKind::ImportBinding | SymbolKind::ImportNamespace
    ) {
        if let Some(import_id) = resolution.import_for_symbol(symbol.id()) {
            if let Some(import_record) = resolution.import(import_id) {
                let imported_name = import_record.imported_name();
                let source_path = import_record.source();

                // Find matching import edge
                let edge = semantic_graph.import_edges().iter().find(|e| {
                    e.from() == module_id
                        && e.source() == source_path
                        && e.imported_name() == imported_name
                });

                if let Some(e) = edge {
                    if let Some(to_module_id) = e.to() {
                        let to_module = semantic_graph.get(to_module_id)?;
                        let to_resolution = to_module.graph().resolution()?;

                        if let Some(export_name) = e.export_name() {
                            if let Some(export_id) = to_resolution.export_by_name(export_name) {
                                if let Some(export_record) = to_resolution.export_record(export_id)
                                {
                                    let dec_node = export_record.declaration();
                                    return node_location(workspace, to_module, dec_node);
                                }
                            }
                        } else if symbol.kind() == SymbolKind::ImportNamespace {
                            // Point to the root of the file
                            return node_location(
                                workspace,
                                to_module,
                                to_module.graph().syntax().root()?,
                            );
                        }
                    }
                }
            }
        }
    }

    // Default: Return the location of the declaration in the current module
    node_location(workspace, semantic_module, symbol.declaration())
}

pub(crate) fn node_location(
    workspace: &Workspace,
    module: &galfus_frontend::modules::SemanticModule,
    node_id: galfus_core::NodeId,
) -> Option<Location> {
    let syntax = module.graph().syntax();
    let node = syntax.node(node_id)?;
    let span = node.span();

    let start_rc = module.source().row_col(span.start())?;
    let end_rc = module.source().row_col(span.end()).unwrap_or(start_rc);

    let range = Range {
        start: Position {
            line: (start_rc.row - 1) as u32,
            character: (start_rc.column - 1) as u32,
        },
        end: Position {
            line: (end_rc.row - 1) as u32,
            character: (end_rc.column - 1) as u32,
        },
    };

    let module_path = module.path();
    let path = module_path.as_str();

    let is_virtual = if let Some(root) = &workspace.root_path {
        !root.join(path).exists()
    } else {
        !std::path::Path::new(path).exists()
    };
    let url = if is_virtual {
        Url::parse(&format!("galfus://virtual/{}", path)).ok()?
    } else if let Some(root) = &workspace.root_path {
        let full_path = root.join(path);
        crate::lsp::file_path_to_uri(&full_path).ok()?
    } else {
        Url::parse(&format!("file:///{}", path)).ok()?
    };

    Some(Location { uri: url, range })
}
