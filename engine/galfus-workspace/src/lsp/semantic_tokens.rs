use crate::lsp::anchored::value_anchor_function;
use crate::workspace::Workspace;
use galfus_core::ModulePath;
use galfus_frontend::{SymbolKind, SyntaxNodeKind};
use lsp_types::{SemanticToken, SemanticTokens};

pub fn semantic_tokens_full(workspace: &Workspace, path: &str) -> Option<SemanticTokens> {
    let module_path = ModulePath::new(path)?;
    let snapshot = workspace.frontend_snapshot()?;
    let semantic_graph = snapshot.semantic_graph();

    let module_id = semantic_graph.module_by_path(&module_path)?;
    let semantic_module = semantic_graph.get(module_id)?;
    let source = semantic_module.source();

    let syntax_graph = semantic_module.graph().syntax();
    let root = syntax_graph.root()?;
    let resolution = semantic_module.graph().resolution()?;

    let mut raw_tokens = Vec::new();
    let mut parents = std::collections::HashMap::new();

    let mut stack = vec![root];
    while let Some(node_id) = stack.pop() {
        if let Some(node) = syntax_graph.node(node_id) {
            let mut children = node.children().to_vec();
            for &child in &children {
                parents.insert(child, node_id);
            }
            children.reverse();
            stack.extend(children);

            if node.kind() == SyntaxNodeKind::Identifier {
                let mut current = Some(node_id);
                let mut symbol_id = None;

                while let Some(curr) = current {
                    if let Some(sym) = resolution
                        .path_reference_symbol(curr)
                        .or_else(|| resolution.reference_symbol(curr))
                        .or_else(|| resolution.type_reference_symbol(curr))
                        .or_else(|| resolution.type_path_reference_symbol(curr))
                        .or_else(|| resolution.declaration_symbol(curr))
                    {
                        symbol_id = Some(sym);
                        break;
                    }
                    current = parents.get(&curr).copied();
                }

                let anchored_function = parents.get(&node_id).and_then(|parent| {
                    syntax_graph
                        .node(*parent)
                        .filter(|node| node.kind() == SyntaxNodeKind::PathExpression)
                        .filter(|node| node.child(1) == Some(node_id))
                        .and_then(|_| value_anchor_function(snapshot, semantic_module, *parent))
                });

                let token_kind = anchored_function.map(|_| SymbolKind::Function).or_else(|| {
                    symbol_id
                        .and_then(|symbol| resolution.symbol(symbol).map(|symbol| symbol.kind()))
                });

                if let Some((token_type, token_modifiers)) = token_kind.and_then(map_symbol_kind) {
                    let span = node.span();
                    if let Some(start_rc) = source.row_col(span.start()) {
                        raw_tokens.push(Token {
                            line: (start_rc.row - 1) as u32,
                            start_char: (start_rc.column - 1) as u32,
                            length: (span.len()) as u32,
                            token_type,
                            token_modifiers,
                        });
                    }
                }
            }
        }
    }

    raw_tokens.sort_by(|a, b| a.line.cmp(&b.line).then(a.start_char.cmp(&b.start_char)));

    let mut data = Vec::new();
    let mut prev_line = 0;
    let mut prev_char = 0;

    for token in raw_tokens {
        let delta_line = token.line - prev_line;
        let delta_start = if delta_line == 0 {
            token.start_char - prev_char
        } else {
            token.start_char
        };

        data.push(SemanticToken {
            delta_line,
            delta_start,
            length: token.length,
            token_type: token.token_type,
            token_modifiers_bitset: token.token_modifiers,
        });

        prev_line = token.line;
        prev_char = token.start_char;
    }

    Some(SemanticTokens {
        result_id: None,
        data,
    })
}

struct Token {
    line: u32,
    start_char: u32,
    length: u32,
    token_type: u32,
    token_modifiers: u32,
}

fn map_symbol_kind(kind: SymbolKind) -> Option<(u32, u32)> {
    let type_index = match kind {
        SymbolKind::Function => 12,
        SymbolKind::TypeAlias => 1,
        SymbolKind::Struct => 5,
        SymbolKind::Enum => 3,
        SymbolKind::Choice => 3,
        SymbolKind::Constraint => 4,
        SymbolKind::Var => 8,
        SymbolKind::Const => 8,
        SymbolKind::Parameter | SymbolKind::RestParameter => 7,
        SymbolKind::GenericParameter => 6,
        SymbolKind::ForBinding | SymbolKind::PatternBinding | SymbolKind::TypePatternBinding => 8,
        SymbolKind::StructField => 9,
        SymbolKind::EnumVariant | SymbolKind::ChoiceVariant => 10,
        SymbolKind::ConstraintField => 9,
        SymbolKind::ConstraintFunction => 13,
        SymbolKind::ImportNamespace => return None,
        SymbolKind::ImportBinding => return None,
        SymbolKind::BuiltinType => 1,
    };

    let mut modifiers = 0;
    if kind == SymbolKind::Const {
        modifiers |= 1 << 2; // readonly
    }

    Some((type_index, modifiers))
}
