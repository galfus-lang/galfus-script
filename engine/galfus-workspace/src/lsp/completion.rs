use galfus_core::{ModulePath, RowCol};
use galfus_frontend::{SymbolKind, SyntaxNodeKind, TypeKind};
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionResponse, Position, Range, TextEdit,
};
use std::collections::HashSet;

use crate::workspace::Workspace;

pub fn completion(
    workspace: &Workspace,
    path: &str,
    position: Position,
) -> Option<CompletionResponse> {
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

    let root = syntax_graph.root()?;

    let mut path_stack = Vec::new();
    let mut temp = root;
    loop {
        path_stack.push(temp);
        let mut found = false;
        if let Some(n) = syntax_graph.node(temp) {
            for child_id in n.children() {
                if let Some(child) = syntax_graph.node(*child_id) {
                    let span = child.span();
                    let target = offset.saturating_sub(1);
                    if span.start() <= target && target <= span.end() {
                        temp = *child_id;
                        found = true;
                        break;
                    }
                }
            }
        }
        if !found {
            break;
        }
    }

    let resolution = semantic_module.graph().resolution()?;
    let string_table = snapshot.string_table();
    let type_result = semantic_module.type_result();
    let mut items = Vec::new();
    let mut seen_labels = HashSet::new();

    // Context: Import statements
    let mut import_item_node = None;
    let mut in_import_source = false;
    let mut in_named_import_list = false;

    for &node in path_stack.iter().rev() {
        if let Some(n) = syntax_graph.node(node) {
            match n.kind() {
                SyntaxNodeKind::ImportItem => {
                    import_item_node = Some(node);
                    break;
                }
                SyntaxNodeKind::ImportSource => {
                    in_import_source = true;
                }
                SyntaxNodeKind::NamedImportList => {
                    in_named_import_list = true;
                }
                _ => {}
            }
        }
    }

    if let Some(item_node) = import_item_node {
        if in_import_source {
            for module in semantic_graph.modules() {
                if module.id() == module_id {
                    continue;
                }
                let mut path_str = module.path().as_str().to_string();
                if let Some(stripped) = path_str.strip_suffix(".gfs") {
                    path_str = stripped.to_string();
                } else if let Some(stripped) = path_str.strip_suffix(".gfp") {
                    path_str = stripped.to_string();
                }

                if seen_labels.insert(path_str.clone()) {
                    items.push(CompletionItem {
                        label: path_str,
                        kind: Some(CompletionItemKind::MODULE),
                        detail: Some("Galfus Module".to_string()),
                        sort_text: Some("0".to_string()),
                        ..Default::default()
                    });
                }
            }
            items.sort_by(|a, b| a.label.cmp(&b.label));
            return Some(CompletionResponse::List(lsp_types::CompletionList {
                is_incomplete: false,
                items,
            }));
        }

        if in_named_import_list {
            if let Some(import_item) = syntax_graph.node(item_node)
                && let Some(source_node_id) = import_item.child(1)
                && let Some(source_node) = syntax_graph.node(source_node_id)
                && let Some(string_node_id) = source_node.first_child()
                && let Some(string_node) = syntax_graph.node(string_node_id)
            {
                let literal_text = source.slice(string_node.span()).unwrap_or("");
                let clean_path = literal_text.trim_matches('"');
                let mut found_target_module = None;
                for ext in ["", ".gfs", ".gfp"] {
                    let mut p = clean_path.to_string();
                    if !ext.is_empty() && !p.ends_with(".gfs") && !p.ends_with(".gfp") {
                        p.push_str(ext);
                    }
                    if let Some(module_path) = ModulePath::new(&p)
                        && let Some(id) = semantic_graph.module_by_path(&module_path)
                    {
                        found_target_module = semantic_graph.get(id);
                        break;
                    }
                }

                if let Some(target_module) = found_target_module
                    && let Some(target_resolution) = target_module.graph().resolution()
                {
                    for export_record in target_resolution.exports() {
                        let label = export_record.name().to_string();
                        if seen_labels.insert(label.clone()) {
                            let kind = match export_record.kind() {
                                SymbolKind::Function => CompletionItemKind::FUNCTION,
                                SymbolKind::Struct => CompletionItemKind::STRUCT,
                                SymbolKind::Enum | SymbolKind::Choice => CompletionItemKind::ENUM,
                                SymbolKind::Constraint => CompletionItemKind::INTERFACE,
                                SymbolKind::Const => CompletionItemKind::CONSTANT,
                                _ => CompletionItemKind::TEXT,
                            };

                            items.push(CompletionItem {
                                label,
                                kind: Some(kind),
                                sort_text: Some("0".to_string()),
                                ..Default::default()
                            });
                        }
                    }
                }
            }
            items.sort_by(|a, b| a.label.cmp(&b.label));
            return Some(CompletionResponse::List(lsp_types::CompletionList {
                is_incomplete: false,
                items,
            }));
        }
    }

    let mut is_member_access = false;
    let mut is_path_access = false;
    let mut target_node = None;

    if path_stack.len() >= 2 {
        let parent_node = path_stack[path_stack.len() - 2];
        let current = path_stack[path_stack.len() - 1];
        if let Some(parent) = syntax_graph.node(parent_node) {
            match parent.kind() {
                SyntaxNodeKind::MemberExpression | SyntaxNodeKind::NullSafeMemberExpression => {
                    if parent.child(1) == Some(current) {
                        is_member_access = true;
                        target_node = parent.first_child();
                    }
                }
                SyntaxNodeKind::PathExpression | SyntaxNodeKind::Path
                    if parent.child(1) == Some(current) =>
                {
                    is_path_access = true;
                    target_node = parent.first_child();
                }
                _ => {}
            }
        }
    }

    let mut scope_id = None;
    for &node in path_stack.iter().rev() {
        if let Some(s) = resolution.node_scope(node) {
            scope_id = Some(s);
            break;
        }
    }
    let mut current_scope_id = scope_id.unwrap_or_else(|| resolution.module_scope());

    if !is_member_access && !is_path_access {
        let text_before_cursor = &source.text()[..offset];
        let trimmed = text_before_cursor.trim_end();
        if trimmed.ends_with('.') || trimmed.ends_with("::") {
            let is_path = trimmed.ends_with("::");
            if is_path {
                is_path_access = true;
            } else {
                is_member_access = true;
            }

            let trim_str = if is_path { "::" } else { "." };
            let target_offset = trimmed
                .trim_end_matches(trim_str)
                .trim_end()
                .len()
                .saturating_sub(1);

            let mut target_path_stack = Vec::new();
            let mut curr = root;
            loop {
                target_path_stack.push(curr);
                let mut found = false;
                if let Some(n) = syntax_graph.node(curr) {
                    for child_id in n.children() {
                        if let Some(child) = syntax_graph.node(*child_id) {
                            let span = child.span();
                            if span.start() <= target_offset && target_offset <= span.end() {
                                curr = *child_id;
                                found = true;
                                break;
                            }
                        }
                    }
                }
                if !found {
                    break;
                }
            }

            // Go up from Identifier to NameExpression or Path if needed
            let mut resolved_target = curr;
            if let Some(n) = syntax_graph.node(curr)
                && n.kind() == SyntaxNodeKind::Identifier
                && target_path_stack.len() >= 2
            {
                let parent = target_path_stack[target_path_stack.len() - 2];
                if let Some(p) = syntax_graph.node(parent)
                    && (p.kind() == SyntaxNodeKind::NameExpression
                        || p.kind() == SyntaxNodeKind::Path
                        || p.kind() == SyntaxNodeKind::PathExpression)
                {
                    resolved_target = parent;
                }
            }
            target_node = Some(resolved_target);
        }
    }

    if let Some(target) = target_node {
        let mut target_symbol = None;
        if is_path_access {
            target_symbol = resolution
                .reference_symbol(target)
                .or_else(|| resolution.path_reference_symbol(target))
                .or_else(|| resolution.type_reference_symbol(target))
                .or_else(|| resolution.type_path_reference_symbol(target))
                .or_else(|| resolution.declaration_symbol(target));
        }

        if target_symbol.is_none() {
            let mut resolved_type_id;

            if let Some(type_result) = type_result {
                resolved_type_id = type_result.layer().node_type(target);

                if resolved_type_id.is_none()
                    && let Some(n) = syntax_graph.node(target)
                {
                    let is_ident = n.kind() == SyntaxNodeKind::Identifier;
                    let is_name_expr = n.kind() == SyntaxNodeKind::NameExpression;
                    if is_ident || is_name_expr {
                        let text_span = if is_name_expr {
                            syntax_graph
                                .node(n.first_child().unwrap())
                                .map(|c| c.span())
                                .unwrap_or(n.span())
                        } else {
                            n.span()
                        };
                        if let Some(text) = source.slice(text_span)
                            && let Some(name_id) = string_table.get(text)
                        {
                            let mut curr_scope = current_scope_id;
                            loop {
                                if let Some(scope) = resolution.scope(curr_scope) {
                                    if let Some(&sym_id) = scope.symbols().get(&name_id) {
                                        resolved_type_id = type_result.layer().symbol_type(sym_id);
                                        if is_path_access {
                                            target_symbol = Some(sym_id);
                                        }
                                        break;
                                    }
                                    if let Some(parent) = scope.parent() {
                                        curr_scope = parent;
                                    } else {
                                        break;
                                    }
                                } else {
                                    break;
                                }
                            }
                        }
                    }
                }

                if let Some(type_id) = resolved_type_id
                    && let Some(type_kind) = type_result.layer().table().kind(type_id)
                {
                    match type_kind {
                        TypeKind::Array { .. } if is_member_access => {
                            items.push(CompletionItem {
                                label: "length".to_string(),
                                kind: Some(CompletionItemKind::PROPERTY),
                                sort_text: Some("0".to_string()),
                                ..Default::default()
                            });
                        }
                        TypeKind::Named { symbol } => {
                            target_symbol = Some(*symbol);
                        }
                        _ => {}
                    }
                }
            }
        }

        let mut final_sym = target_symbol;
        let mut final_res = resolution;

        if let Some(sym) = target_symbol
            && let Some(symbol) = resolution.symbol(sym)
            && matches!(
                symbol.kind(),
                SymbolKind::ImportBinding | SymbolKind::ImportNamespace
            )
            && let Some(import_id) = resolution.import_for_symbol(sym)
            && let Some(import_record) = resolution.import(import_id)
        {
            let imported_name = import_record.imported_name();
            let source_path = import_record.source();
            let semantic_graph = snapshot.semantic_graph();

            if let Some(edge) = semantic_graph.import_edges().iter().find(|e| {
                e.from() == module_id
                    && e.source() == source_path
                    && e.imported_name() == imported_name
            }) && let Some(to_module_id) = edge.to()
                && let Some(to_module) = semantic_graph.get(to_module_id)
                && let Some(to_res) = to_module.graph().resolution()
            {
                if let Some(export_name) = edge.export_name() {
                    if let Some(export_id) = to_res.export_by_name(export_name)
                        && let Some(export_record) = to_res.export_record(export_id)
                    {
                        final_sym = Some(export_record.symbol());
                        final_res = to_res;
                    }
                } else if symbol.kind() == SymbolKind::ImportNamespace {
                    final_sym = None; // For namespace, we just want to iterate over its exports
                    for export_record in to_res.exports() {
                        let export_sym_id = export_record.symbol();
                        if let Some(export_sym) = to_res.symbol(export_sym_id)
                            && let Some(name) = string_table.resolve(export_sym.name())
                        {
                            if name.is_empty() {
                                continue;
                            }
                            let kind = match export_sym.kind() {
                                SymbolKind::StructField => CompletionItemKind::FIELD,
                                SymbolKind::EnumVariant | SymbolKind::ChoiceVariant => {
                                    CompletionItemKind::ENUM_MEMBER
                                }
                                SymbolKind::Function => CompletionItemKind::METHOD,
                                _ => CompletionItemKind::PROPERTY,
                            };
                            items.push(CompletionItem {
                                label: name.to_string(),
                                kind: Some(kind),
                                sort_text: Some("0".to_string()),
                                ..Default::default()
                            });
                        }
                    }
                }
            }
        }

        if let Some(sym) = final_sym
            && let Some(member_scope) = final_res.member_scope(sym)
            && let Some(scope) = final_res.scope(member_scope)
        {
            for (name_id, symbol_id) in scope.symbols() {
                if let Some(member_sym) = final_res.symbol(*symbol_id)
                    && let Some(name) = string_table.resolve(*name_id)
                {
                    if name.is_empty() {
                        continue;
                    }

                    let kind = match member_sym.kind() {
                        SymbolKind::StructField => CompletionItemKind::FIELD,
                        SymbolKind::EnumVariant | SymbolKind::ChoiceVariant => {
                            CompletionItemKind::ENUM_MEMBER
                        }
                        SymbolKind::Function => CompletionItemKind::METHOD,
                        _ => CompletionItemKind::PROPERTY,
                    };

                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: Some(kind),
                        sort_text: Some("0".to_string()),
                        ..Default::default()
                    });
                }
            }
        }

        if is_member_access || is_path_access {
            items.sort_by(|a, b| a.label.cmp(&b.label));
            items.dedup_by(|a, b| a.label == b.label);
            return Some(CompletionResponse::List(lsp_types::CompletionList {
                is_incomplete: false,
                items,
            }));
        }
    }

    loop {
        if let Some(scope) = resolution.scope(current_scope_id) {
            for (name_id, symbol_id) in scope.symbols() {
                if let Some(symbol) = resolution.symbol(*symbol_id)
                    && let Some(name) = string_table.resolve(*name_id)
                {
                    if name.is_empty() {
                        continue;
                    }

                    let label = name.to_string();
                    if seen_labels.insert(label.clone()) {
                        let kind = match symbol.kind() {
                            SymbolKind::Function => CompletionItemKind::FUNCTION,
                            SymbolKind::TypeAlias => CompletionItemKind::CLASS,
                            SymbolKind::Struct => CompletionItemKind::STRUCT,
                            SymbolKind::Enum | SymbolKind::Choice => CompletionItemKind::ENUM,
                            SymbolKind::Constraint => CompletionItemKind::INTERFACE,
                            SymbolKind::Var | SymbolKind::Const => CompletionItemKind::VARIABLE,
                            SymbolKind::Parameter
                            | SymbolKind::RestParameter
                            | SymbolKind::ForBinding
                            | SymbolKind::PatternBinding
                            | SymbolKind::TypePatternBinding => CompletionItemKind::VARIABLE,
                            SymbolKind::GenericParameter => CompletionItemKind::TYPE_PARAMETER,
                            SymbolKind::StructField => CompletionItemKind::FIELD,
                            SymbolKind::EnumVariant | SymbolKind::ChoiceVariant => {
                                CompletionItemKind::ENUM_MEMBER
                            }
                            _ => CompletionItemKind::TEXT,
                        };

                        items.push(CompletionItem {
                            label,
                            kind: Some(kind),
                            sort_text: Some("2".to_string()),
                            ..Default::default()
                        });
                    }
                }
            }

            if let Some(parent) = scope.parent() {
                current_scope_id = parent;
            } else {
                break;
            }
        } else {
            break;
        }
    }

    if let Some(builtin_scope_id) = resolution.builtin_scope()
        && let Some(scope) = resolution.scope(builtin_scope_id)
    {
        for (name_id, symbol_id) in scope.symbols() {
            if let Some(symbol) = resolution.symbol(*symbol_id)
                && let Some(name) = string_table.resolve(*name_id)
            {
                let label = name.to_string();
                if seen_labels.insert(label.clone()) {
                    let kind = match symbol.kind() {
                        SymbolKind::Function => CompletionItemKind::FUNCTION,
                        SymbolKind::Struct => CompletionItemKind::STRUCT,
                        SymbolKind::Enum | SymbolKind::Choice => CompletionItemKind::ENUM,
                        SymbolKind::Constraint => CompletionItemKind::INTERFACE,
                        SymbolKind::Const => CompletionItemKind::CONSTANT,
                        _ => CompletionItemKind::TEXT,
                    };
                    items.push(CompletionItem {
                        label,
                        kind: Some(kind),
                        sort_text: Some("3".to_string()),
                        ..Default::default()
                    });
                }
            }
        }
    }

    for module in semantic_graph.modules() {
        if module.id() == module_id {
            continue;
        }

        let ext_resolution = module.graph().resolution();
        if ext_resolution.is_none() {
            continue;
        }
        let ext_resolution = ext_resolution.unwrap();

        for export_record in ext_resolution.exports() {
            let label = export_record.name().to_string();

            if seen_labels.insert(label.clone()) {
                let kind = match export_record.kind() {
                    SymbolKind::Function => CompletionItemKind::FUNCTION,
                    SymbolKind::Struct => CompletionItemKind::STRUCT,
                    SymbolKind::Enum | SymbolKind::Choice => CompletionItemKind::ENUM,
                    SymbolKind::Constraint => CompletionItemKind::INTERFACE,
                    SymbolKind::Const => CompletionItemKind::CONSTANT,
                    _ => CompletionItemKind::TEXT,
                };

                let mut module_path_str = module.path().as_str().to_string();
                if let Some(stripped) = module_path_str.strip_suffix(".gfs") {
                    module_path_str = stripped.to_string();
                } else if let Some(stripped) = module_path_str.strip_suffix(".gfp") {
                    module_path_str = stripped.to_string();
                }

                let text_edit = TextEdit {
                    range: Range {
                        start: Position {
                            line: 0,
                            character: 0,
                        },
                        end: Position {
                            line: 0,
                            character: 0,
                        },
                    },
                    new_text: format!("import {{ {} }} from \"{}\"\n", label, module_path_str),
                };

                items.push(CompletionItem {
                    label: label.clone(),
                    kind: Some(kind),
                    detail: Some(format!("Auto-import from \"{}\"", module_path_str)),
                    sort_text: Some("4".to_string()),
                    additional_text_edits: Some(vec![text_edit]),
                    ..Default::default()
                });
            }
        }
    }

    items.sort_by(|a, b| a.label.cmp(&b.label));
    items.dedup_by(|a, b| a.label == b.label);

    Some(CompletionResponse::List(lsp_types::CompletionList {
        is_incomplete: false,
        items,
    }))
}
