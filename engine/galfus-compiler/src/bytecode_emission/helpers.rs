use super::LowerCtx;
use galfus_core::{NodeId, SymbolId, TypeId};
use galfus_frontend::{SymbolKind, SyntaxNodeKind, TypeKind};

pub fn type_item_for_symbol(ctx: &LowerCtx, symbol: SymbolId) -> Option<NodeId> {
    let resolution = ctx.graph.resolution()?;
    let member_scope = resolution.member_scope(symbol)?;
    let scope = resolution.scope(member_scope)?;
    scope.owner()
}

pub fn constraint_type_base_node(ctx: &LowerCtx, type_node: NodeId) -> Option<NodeId> {
    let syntax = ctx.graph.syntax();
    let generic_type = find_generic_type_node(ctx, type_node);

    if let Some(generic_type) = generic_type {
        let node = syntax.node(generic_type)?;
        return node.children().iter().copied().find(|child| {
            syntax
                .node(*child)
                .is_some_and(|child_node| child_node.kind() != SyntaxNodeKind::TypeArgumentList)
        });
    }

    Some(type_node)
}

pub fn find_generic_type_node(ctx: &LowerCtx, node: NodeId) -> Option<NodeId> {
    let syntax_node = ctx.graph.syntax().node(node)?;

    if syntax_node.kind() == SyntaxNodeKind::GenericType {
        return Some(node);
    }

    for child in syntax_node.children() {
        if let Some(found) = find_generic_type_node(ctx, *child) {
            return Some(found);
        }
    }

    None
}

pub fn find_struct_item_by_name(ctx: &LowerCtx, node: NodeId, struct_name: &str) -> Option<NodeId> {
    let syntax = ctx.graph.syntax();
    let syntax_node = syntax.node(node)?;
    if syntax_node.kind() == SyntaxNodeKind::StructItem {
        let has_matching_identifier = syntax
            .first_child_of_kind(node, SyntaxNodeKind::Identifier)
            .is_some_and(|id| node_text(ctx, id) == struct_name);
        if has_matching_identifier {
            return Some(node);
        }
    }
    for &child in syntax_node.children() {
        if let Some(found) = find_struct_item_by_name(ctx, child, struct_name) {
            return Some(found);
        }
    }
    None
}

pub fn node_text<'a>(ctx: &'a LowerCtx<'a>, node: NodeId) -> &'a str {
    if let Some(syntax_node) = ctx.graph.syntax().node(node) {
        let span = syntax_node.span();
        if span.start() <= ctx.source_text.len() && span.end() <= ctx.source_text.len() {
            return &ctx.source_text[span.start()..span.end()];
        }
    }
    ""
}

pub fn struct_symbol_for_type(ctx: &LowerCtx, ty: TypeId) -> Option<SymbolId> {
    let layer = ctx.type_result.layer();
    let table = layer.table();
    let mut current = ty;
    loop {
        match table.kind(current) {
            Some(TypeKind::Named { symbol }) => {
                let resolution = ctx.graph.resolution()?;
                let is_struct = resolution
                    .symbol(*symbol)
                    .is_some_and(|sd| sd.kind() == SymbolKind::Struct);
                if is_struct {
                    return Some(*symbol);
                }
                break;
            }
            Some(TypeKind::GenericInstance { base, .. }) => {
                current = *base;
            }
            _ => break,
        }
    }
    None
}

pub fn choice_item_node_for_symbol(
    ctx: &LowerCtx,
    node: NodeId,
    choice_symbol: SymbolId,
) -> Option<NodeId> {
    let syntax_node = ctx.graph.syntax().node(node)?;
    if syntax_node.kind() == SyntaxNodeKind::ChoiceItem {
        let matches_symbol = ctx.graph.resolution().is_some_and(|res| {
            ctx.graph
                .syntax()
                .first_child_of_kind(node, SyntaxNodeKind::Identifier)
                .and_then(|ident| res.declaration_symbol(ident))
                == Some(choice_symbol)
        });
        if matches_symbol {
            return Some(node);
        }
    }
    for &child in syntax_node.children() {
        if let Some(found) = choice_item_node_for_symbol(ctx, child, choice_symbol) {
            return Some(found);
        }
    }
    None
}

pub fn find_choice_variant_node_by_name(
    ctx: &LowerCtx,
    node: NodeId,
    name: &str,
) -> Option<NodeId> {
    let syntax = ctx.graph.syntax();
    let syntax_node = syntax.node(node)?;
    if syntax_node.kind() == SyntaxNodeKind::ChoiceVariant {
        let matches_name = syntax
            .first_child_of_kind(node, SyntaxNodeKind::Identifier)
            .is_some_and(|identifier| node_text(ctx, identifier) == name);
        if matches_name {
            return Some(node);
        }
    }
    for &child in syntax_node.children() {
        if let Some(found) = find_choice_variant_node_by_name(ctx, child, name) {
            return Some(found);
        }
    }
    None
}

pub fn find_descendant_of_kind(
    ctx: &LowerCtx,
    node: NodeId,
    kind: SyntaxNodeKind,
) -> Option<NodeId> {
    let syntax = ctx.graph.syntax();
    let syntax_node = syntax.node(node)?;
    for &child in syntax_node.children() {
        if let Some(child_node) = syntax.node(child) {
            if child_node.kind() == kind {
                return Some(child);
            }
            if let Some(found) = find_descendant_of_kind(ctx, child, kind) {
                return Some(found);
            }
        }
    }
    None
}

pub fn first_type_child(ctx: &LowerCtx, node: NodeId) -> Option<NodeId> {
    let syntax = ctx.graph.syntax();
    let syntax_node = syntax.node(node)?;
    for &child in syntax_node.children() {
        if let Some(child_node) = syntax.node(child)
            && matches!(
                child_node.kind(),
                SyntaxNodeKind::NamedType
                    | SyntaxNodeKind::ArrayType
                    | SyntaxNodeKind::TupleType
                    | SyntaxNodeKind::UnionType
            )
        {
            return Some(child);
        }
    }
    None
}

pub fn find_tuple_type(ctx: &LowerCtx, elements: &[TypeId]) -> TypeId {
    let table = ctx.type_result.layer().table();
    for id in 0..table.len() {
        let ty_id = TypeId::new(id as u32);
        if matches!(table.kind(ty_id), Some(TypeKind::Tuple { elements: existing }) if existing == elements)
        {
            return ty_id;
        }
    }
    TypeId::new(0)
}

pub fn find_choice_for_variant(
    ctx: &LowerCtx,
    variant_symbol: SymbolId,
) -> Option<(SymbolId, usize)> {
    let resolution = ctx.graph.resolution()?;
    let variant_data = resolution.symbol(variant_symbol)?;
    let variant_name = variant_data.name();

    let root = ctx.graph.syntax().root()?;
    let syntax = ctx.graph.syntax();

    let mut stack = vec![root];
    while let Some(node_id) = stack.pop() {
        let node = syntax.node(node_id)?;
        if node.kind() == SyntaxNodeKind::ChoiceItem
            && let Some(ident) = syntax.first_child_of_kind(node_id, SyntaxNodeKind::Identifier)
            && let Some(choice_symbol) = resolution.declaration_symbol(ident)
        {
            let variants = crate::bytecode_emission::types::get_choice_variants(ctx, choice_symbol);
            if let Some(idx) = variants
                .iter()
                .position(|(name, _)| name == ctx.string_table.resolve(variant_name).unwrap_or(""))
            {
                return Some((choice_symbol, idx));
            }
        }
        stack.extend(node.children().iter().copied().rev());
    }
    None
}
