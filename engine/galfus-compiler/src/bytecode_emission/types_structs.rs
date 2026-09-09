use std::collections::HashSet;

use super::LowerCtx;
use galfus_bytecode::{FieldLayout, OwnershipKind, StructLayout, StructLayoutIdx};
use galfus_core::{SymbolId, TypeId};
use galfus_frontend::{SymbolKind, SyntaxNodeKind};

pub fn get_or_create_struct_layout(ctx: &mut LowerCtx, struct_symbol: SymbolId) -> StructLayoutIdx {
    if let Some(&idx) = ctx.struct_map.get(&struct_symbol) {
        return idx;
    }

    let next_idx = StructLayoutIdx(ctx.struct_layouts.len() as u16);
    ctx.struct_map.insert(struct_symbol, next_idx);

    let resolution = ctx.graph.resolution().unwrap();
    let struct_name = resolution
        .symbol(struct_symbol)
        .and_then(|symbol| ctx.string_table.resolve(symbol.name()))
        .unwrap_or("")
        .to_string();
    let fields = get_struct_fields(ctx, struct_symbol)
        .into_iter()
        .map(|(name, ty)| FieldLayout {
            name,
            ty: crate::bytecode_emission::types::lower_type(ctx, ty),
            offset: 0,
            ownership: OwnershipKind::Value,
        })
        .collect();

    ctx.struct_layouts.push(StructLayout {
        name: struct_name,
        fields,
        constraints: get_struct_constraints(ctx, struct_symbol),
    });

    next_idx
}

fn get_struct_constraints(ctx: &LowerCtx, struct_symbol: SymbolId) -> Vec<String> {
    let Some(struct_item) =
        crate::bytecode_emission::helpers::type_item_for_symbol(ctx, struct_symbol)
    else {
        return Vec::new();
    };
    let syntax = ctx.graph.syntax();
    let resolution = ctx.graph.resolution();
    let Some(satisfies) = syntax.first_child_of_kind(struct_item, SyntaxNodeKind::SatisfiesClause)
    else {
        return Vec::new();
    };

    syntax
        .node(satisfies)
        .map(|node| node.children().to_vec())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|constraint_type| {
            let base =
                crate::bytecode_emission::helpers::constraint_type_base_node(ctx, constraint_type)?;
            resolution
                .and_then(|resolution| resolution.reference_symbol(base))
                .or_else(|| {
                    resolution.and_then(|resolution| resolution.type_reference_symbol(base))
                })
                .or_else(|| {
                    resolution.and_then(|resolution| resolution.type_path_reference_symbol(base))
                })
                .and_then(|symbol| resolution.and_then(|resolution| resolution.symbol(symbol)))
                .filter(|symbol| symbol.kind() == SymbolKind::Constraint)
                .map(|symbol| {
                    ctx.string_table
                        .resolve(symbol.name())
                        .unwrap_or("")
                        .to_string()
                })
        })
        .collect()
}

pub fn get_struct_fields(ctx: &LowerCtx, struct_symbol: SymbolId) -> Vec<(String, TypeId)> {
    if let Some(fields) = ctx.imported_struct_fields.get(&struct_symbol) {
        return fields.clone();
    }
    let mut visited = HashSet::new();
    get_struct_fields_internal(ctx, struct_symbol, &mut visited)
}

fn get_struct_fields_internal(
    ctx: &LowerCtx,
    struct_symbol: SymbolId,
    visited: &mut HashSet<SymbolId>,
) -> Vec<(String, TypeId)> {
    if !visited.insert(struct_symbol) {
        return Vec::new();
    }
    let resolution = match ctx.graph.resolution() {
        Some(resolution) => resolution,
        None => return Vec::new(),
    };
    let struct_symbol_data = match resolution.symbol(struct_symbol) {
        Some(data) => data,
        None => return Vec::new(),
    };

    let mut fields = Vec::new();
    let root = ctx.graph.syntax().root().unwrap();
    if let Some(item_node) = crate::bytecode_emission::helpers::find_struct_item_by_name(
        ctx,
        root,
        ctx.string_table
            .resolve(struct_symbol_data.name())
            .unwrap_or(""),
    ) {
        let syntax = ctx.graph.syntax();
        let field_children = syntax
            .first_child_of_kind(item_node, SyntaxNodeKind::StructFieldList)
            .and_then(|field_list| syntax.node(field_list))
            .map(|node| node.children())
            .unwrap_or(&[]);

        for &field_child in field_children {
            let node_kind = syntax.node(field_child).map(|node| node.kind());
            if node_kind == Some(SyntaxNodeKind::StructExpansion) {
                let target_symbol = syntax
                    .child(field_child, 0)
                    .and_then(|target| ctx.type_result.layer().node_type(target))
                    .and_then(|target_ty| {
                        crate::bytecode_emission::helpers::struct_symbol_for_type(ctx, target_ty)
                    });
                if let Some(target_symbol) = target_symbol {
                    for (expanded_name, expanded_ty) in
                        get_struct_fields_internal(ctx, target_symbol, visited)
                    {
                        if !fields.iter().any(|(name, _)| *name == expanded_name) {
                            fields.push((expanded_name, expanded_ty));
                        }
                    }
                }
            } else if node_kind == Some(SyntaxNodeKind::StructField)
                && let Some(identifier) =
                    syntax.first_child_of_kind(field_child, SyntaxNodeKind::Identifier)
            {
                let name =
                    crate::bytecode_emission::helpers::node_text(ctx, identifier).to_string();
                let field_ty = resolution
                    .declaration_symbol(identifier)
                    .and_then(|symbol| ctx.type_result.layer().symbol_type(symbol))
                    .or_else(|| ctx.type_result.layer().node_type(field_child));
                if let Some(ty) = field_ty
                    && !fields.iter().any(|(field_name, _)| *field_name == name)
                {
                    fields.push((name, ty));
                }
            }
        }
    }

    if let Some(scope) = resolution
        .member_scope(struct_symbol)
        .and_then(|member_scope| resolution.scope(member_scope))
    {
        for (name, &symbol) in scope.symbols() {
            let field_ty = resolution
                .symbol(symbol)
                .filter(|symbol_data| symbol_data.kind() == SymbolKind::StructField)
                .and_then(|_| ctx.type_result.layer().symbol_type(symbol));
            if let Some(ty) = field_ty {
                let name = ctx.string_table.resolve(*name).unwrap_or("").to_string();
                if let Some(existing) = fields
                    .iter_mut()
                    .find(|(field_name, _)| *field_name == name)
                {
                    existing.1 = ty;
                } else {
                    fields.push((name, ty));
                }
            }
        }
    }
    fields
}
