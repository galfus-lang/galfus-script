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

    // Find the deepest node containing the offset and track the path
    let mut current_node = syntax_graph.root()?;
    let mut path_stack = Vec::new();

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

    let kind = syntax_graph.node(current_node)?.kind();
    if kind == galfus_frontend::SyntaxNodeKind::SourceFile {
        return None;
    }

    let type_result = semantic_module.type_result()?;
    let mut node_type_id = None;
    let mut type_node = current_node;

    let mut nodes_to_check = vec![current_node];
    let mut current_child = current_node;
    for &parent in path_stack.iter().rev().skip(1) {
        if let Some(parent_node) = syntax_graph.node(parent) {
            use galfus_frontend::SyntaxNodeKind::*;
            let is_bubble = match parent_node.kind() {
                NameExpression | Path | NamedType => true,
                VariantPattern => true,
                PathExpression | MemberExpression => parent_node.child(1) == Some(current_child),
                GenericExpression | GenericType => parent_node.child(0) == Some(current_child),
                _ => false,
            };
            if is_bubble {
                nodes_to_check.push(parent);
            } else {
                break;
            }
        } else {
            break;
        }
        current_child = parent;
    }
    nodes_to_check.reverse();

    for &node in &nodes_to_check {
        if let Some(tid) = type_result.layer().node_type(node) {
            node_type_id = Some(tid);
            type_node = node;
            break;
        }
    }

    if node_type_id.is_none()
        && let Some(resolution) = semantic_module.graph().resolution()
    {
        for &node in &nodes_to_check {
            if let Some(sym) = resolution
                .reference_symbol(node)
                .or_else(|| resolution.path_reference_symbol(node))
                .or_else(|| resolution.type_reference_symbol(node))
                .or_else(|| resolution.type_path_reference_symbol(node))
                .or_else(|| resolution.declaration_symbol(node))
                && let Some(tid) = type_result.layer().symbol_type(sym)
            {
                node_type_id = Some(tid);
                type_node = node;
                break;
            }
        }
    }

    let mut doc_hover = None;
    if node_type_id.is_none() {
        let kind = syntax_graph.node(current_node)?.kind();
        use galfus_frontend::SyntaxNodeKind::*;
        let doc = match kind {
            ImportItem => Some(
                "Imports symbols from another module.\n\n```galfus\nimport { myFunc } from \"my_module\"\n```",
            ),
            ExportItem => Some(
                "Exports symbols from this module so they can be imported elsewhere.\n\n```galfus\nexport fn myFunc() {}\n```",
            ),
            FunctionItem => Some(
                "Declares a function.\n\n```galfus\nfn greet(name: [u8]): [u8] {\n    return \"Hello!\"\n}\n```",
            ),
            VarItem | VarStatement => {
                Some("Declares a mutable variable.\n\n```galfus\nvar count = 0\ncount = 1\n```")
            }
            ConstItem | ConstStatement => {
                Some("Declares an immutable constant.\n\n```galfus\nconst PI = 3.1415\n```")
            }
            StructItem => Some(
                "Declares a struct type.\n\n```galfus\nstruct Point {\n    x: f32,\n    y: f32\n}\n```",
            ),
            EnumItem => Some(
                "Declares an enum type with distinct values.\n\n```galfus\nenum Status {\n    Pending,\n    Done\n}\n```",
            ),
            ChoiceItem => Some(
                "Declares a choice (discriminated union) type that can carry payloads.\n\n```galfus\nchoice Result<T, E> {\n    Ok(T),\n    Err(E)\n}\n```",
            ),
            ConstraintItem => Some(
                "Declares a type constraint interface that defines required methods.\n\n```galfus\nconstraint Stringable {\n    fn toString(self): [u8]\n}\n```",
            ),
            MatchExpression => Some(
                "Pattern matching expression. Evaluates cases sequentially and executes the matched arm.\n\n```galfus\nmatch value {\n    0 => \"zero\",\n    _ => \"other\"\n}\n```",
            ),
            IfStatement => Some(
                "Conditional if statement.\n\n```galfus\nif condition {\n    // do something\n} else {\n    // do something else\n}\n```",
            ),
            ForStatement => Some(
                "For loop statement, iterates over a range or iterable.\n\n```galfus\nfor i in 0..10 {\n    // loop body\n}\n```",
            ),
            LoopStatement => Some(
                "Infinite loop statement. Use `break` to exit.\n\n```galfus\nloop {\n    break\n}\n```",
            ),
            ReturnStatement => {
                Some("Returns a value from a function.\n\n```galfus\nreturn result\n```")
            }
            BreakStatement => Some("Breaks out of a loop.\n\n```galfus\nbreak\n```"),
            ContinueStatement => {
                Some("Continues to the next loop iteration.\n\n```galfus\ncontinue\n```")
            }
            AwaitExpression | AwaitAllExpression | AwaitRaceExpression => Some(
                "Waits for an asynchronous operation to resolve.\n\n```galfus\nlet data = await fetch()\n```",
            ),
            TypeAliasItem => Some("Declares a type alias.\n\n```galfus\ntype ID = i64 | [u8]\n```"),
            _ => None,
        };

        if let Some(d) = doc {
            doc_hover = Some(d);
        } else if !matches!(kind, Identifier | Path | NameExpression | StringLiteral) {
            return None;
        }
    }

    if let Some(doc) = doc_hover {
        let kind = syntax_graph.node(current_node)?.kind();
        let hover_text = format!("**Galfus Keyword**: `{:?}`\n\n{}", kind, doc);
        return Some(Hover {
            contents: HoverContents::Scalar(MarkedString::String(hover_text)),
            range: None,
        });
    }

    let current_node = type_node;

    let mut symbol_id = None;
    if let Some(resolution) = semantic_module.graph().resolution() {
        for &node in nodes_to_check.iter().rev() {
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

    let mut type_name = node_type_id
        .map(|id| format_type(workspace, snapshot, semantic_module, id, true))
        .unwrap_or_else(|| "unknown".to_string());

    if type_name == "unknown"
        && node_type_id.is_none()
        && let Some(text) = source.slice(syntax_graph.node(current_node)?.span())
        && matches!(
            text,
            "i8" | "i16"
                | "i32"
                | "i64"
                | "u8"
                | "u16"
                | "u32"
                | "u64"
                | "f32"
                | "f64"
                | "bool"
                | "string"
                | "void"
                | "any"
        )
    {
        type_name = format!("Primitive type `{}`", text);
    }

    if let Some(sym_id) = symbol_id
        && let Some(resolution) = semantic_module.graph().resolution()
        && let Some(symbol) = resolution.symbol(sym_id)
    {
        use galfus_frontend::SymbolKind;
        if matches!(
            symbol.kind(),
            SymbolKind::ChoiceVariant | SymbolKind::EnumVariant
        ) {
            let variant_name = snapshot
                .string_table()
                .resolve(symbol.name())
                .unwrap_or("")
                .to_string();
            let mut payload_text = String::new();
            if let Some(decl) = syntax_graph.node(symbol.declaration()) {
                for &child in decl.children() {
                    if let Some(c) = syntax_graph.node(child)
                        && c.kind() == galfus_frontend::SyntaxNodeKind::ChoicePayload
                        && let Some(text) = source.slice(c.span())
                    {
                        payload_text = text.to_string();
                    }
                }
            }
            type_name = format!("{}::{}{}", type_name, variant_name, payload_text);
        }
    }

    let mut hover_text = format!(
        "**Galfus Node**: `{:?}`\n\nType: {}",
        syntax_graph.node(current_node)?.kind(),
        type_name
    );

    if let Some(sym_id) = symbol_id
        && let Some(resolution) = semantic_module.graph().resolution()
        && let Some(symbol) = resolution.symbol(sym_id)
    {
        use galfus_frontend::SymbolKind;
        if matches!(
            symbol.kind(),
            SymbolKind::ImportBinding | SymbolKind::ImportNamespace
        ) && let Some(import_id) = resolution.import_for_symbol(symbol.id())
            && let Some(import_record) = resolution.import(import_id)
        {
            hover_text.push_str(&format!("\n\n**Origin**: `{}`", import_record.source()));
        }
    }

    Some(Hover {
        contents: HoverContents::Scalar(MarkedString::String(hover_text)),
        range: None,
    })
}

fn encode_command_args(uri: &str, line: u32, col: u32) -> String {
    let json = format!("[\"{}\",{},{}]", uri, line, col);
    let mut encoded = String::new();
    for b in json.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(b as char);
            }
            _ => {
                encoded.push_str(&format!("%{:02X}", b));
            }
        }
    }
    encoded
}

fn find_global_export_link(
    workspace: &crate::workspace::Workspace,
    snapshot: &FrontendSnapshot,
    name: &str,
) -> Option<String> {
    for module in snapshot.semantic_graph().modules() {
        if let Some(res) = module.graph().resolution()
            && let Some(export_id) = res.export_by_name(name)
            && let Some(record) = res.export_record(export_id)
            && let Some(loc) =
                crate::lsp::definition::node_location(workspace, module, record.declaration())
        {
            let uri = loc.uri;
            let line = loc.range.start.line + 1;
            let col = loc.range.start.character + 1;

            if uri.scheme() == "galfus" {
                let encoded_args = encode_command_args(uri.as_str(), line, col);
                return Some(format!(
                    "[{}](command:galfus.openVirtual?{})",
                    name, encoded_args
                ));
            } else {
                return Some(format!("[{}]({}#{},{})", name, uri, line, col));
            }
        }
    }
    None
}

pub(crate) fn format_type(
    workspace: &crate::workspace::Workspace,
    snapshot: &FrontendSnapshot,
    module: &SemanticModule,
    type_id: TypeId,
    with_links: bool,
) -> String {
    let Some(type_result) = module.type_result() else {
        return "unknown".to_string();
    };

    let table = type_result.layer().table();
    let Some(kind) = table.kind(type_id) else {
        return "unknown".to_string();
    };

    let resolution = module.graph().resolution();

    match kind {
        galfus_frontend::TypeKind::Primitive(p) => p.name().to_string(),
        galfus_frontend::TypeKind::Named { symbol } => {
            let name = get_symbol_name(snapshot, resolution, *symbol)
                .unwrap_or_else(|| "unknown".to_string());
            if with_links {
                find_global_export_link(workspace, snapshot, &name).unwrap_or(name)
            } else {
                name
            }
        }
        galfus_frontend::TypeKind::Path { root, segments } => {
            let root_name = if root.raw() == 0 {
                "unknown".to_string()
            } else {
                get_symbol_name(snapshot, resolution, *root)
                    .unwrap_or_else(|| "unknown".to_string())
            };
            let path = segments.join("::");
            if path.is_empty() {
                if with_links {
                    find_global_export_link(workspace, snapshot, &root_name).unwrap_or(root_name)
                } else {
                    root_name
                }
            } else if root_name == "null" || root_name == "unknown" {
                if with_links {
                    find_global_export_link(workspace, snapshot, &path).unwrap_or(path)
                } else {
                    path
                }
            } else {
                let full = format!("{}::{}", root_name, path);
                if with_links {
                    find_global_export_link(workspace, snapshot, &full).unwrap_or(full)
                } else {
                    full
                }
            }
        }
        galfus_frontend::TypeKind::GenericParameter { symbol } => {
            get_symbol_name(snapshot, resolution, *symbol).unwrap_or_else(|| "unknown".to_string())
        }
        galfus_frontend::TypeKind::Array { element } => {
            format!(
                "[{}]",
                format_type(workspace, snapshot, module, *element, with_links)
            )
        }
        galfus_frontend::TypeKind::Range { element } => {
            format!(
                "range<{}>",
                format_type(workspace, snapshot, module, *element, with_links)
            )
        }
        galfus_frontend::TypeKind::Tuple { elements } => {
            let elems: Vec<String> = elements
                .iter()
                .map(|e| format_type(workspace, snapshot, module, *e, with_links))
                .collect();
            format!("({})", elems.join(", "))
        }
        galfus_frontend::TypeKind::Union { members } => {
            let members: Vec<String> = members
                .iter()
                .map(|m| format_type(workspace, snapshot, module, *m, with_links))
                .collect();
            members.join(" | ")
        }
        galfus_frontend::TypeKind::Function(f) => {
            let params: Vec<String> = f
                .parameters()
                .iter()
                .map(|p| {
                    let mut text = format_type(workspace, snapshot, module, p.ty(), with_links);
                    if p.is_rest() {
                        text = format!("...{}", text);
                    }
                    if p.has_default() {
                        if let Some(val) = p.default_value() {
                            text = format!("{} = {}", text, val);
                        } else {
                            text = format!("{} =", text);
                        }
                    }
                    text
                })
                .collect();
            format!(
                "fn({}): {}",
                params.join(", "),
                format_type(workspace, snapshot, module, f.return_type(), with_links)
            )
        }
        galfus_frontend::TypeKind::GenericInstance { base, arguments } => {
            let base_name = format_type(workspace, snapshot, module, *base, with_links);
            let args: Vec<String> = arguments
                .iter()
                .map(|a| format_type(workspace, snapshot, module, *a, with_links))
                .collect();
            format!("{}<{}>", base_name, args.join(", "))
        }
        galfus_frontend::TypeKind::Error => "error".to_string(),
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
