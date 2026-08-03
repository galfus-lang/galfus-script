use super::LowerCtx;
use galfus_bytecode::instruction::TypeIdx;
use galfus_bytecode::{
    BytecodeType, ChoiceLayout, ChoiceLayoutIdx, ChoiceVariantLayout, FieldLayout, OwnershipKind,
    StructLayout, StructLayoutIdx,
};
use galfus_core::{SymbolId, TypeId};
use galfus_frontend::{PrimitiveType, SymbolKind, SyntaxNodeKind, TypeKind};
use std::collections::HashSet;

pub fn resolve_type_with_substitutions(ctx: &LowerCtx, ty: TypeId) -> TypeId {
    let mut current = crate::bytecode_emission::types::resolve_alias_type(ctx, ty);
    loop {
        let table = ctx.type_result.layer().table();
        match table.kind(current) {
            Some(TypeKind::GenericParameter { symbol }) => {
                if let Some(&substituted) = ctx.active_substitutions.get(symbol) {
                    let next =
                        crate::bytecode_emission::types::resolve_alias_type(ctx, substituted);
                    if next == current {
                        break;
                    }
                    current = next;
                } else {
                    break;
                }
            }
            _ => break,
        }
    }
    current
}

pub fn lower_type(ctx: &mut LowerCtx, ty: TypeId) -> TypeIdx {
    let ty = resolve_type_with_substitutions(ctx, ty);

    if let Some(&idx) = ctx.type_map.get(&ty) {
        return idx;
    }

    let next_idx = TypeIdx(ctx.types.len() as u16);
    ctx.type_map.insert(ty, next_idx);
    ctx.types.push(BytecodeType::Null);

    let table = ctx.type_result.layer().table();
    let image_type = match table.kind(ty) {
        Some(TypeKind::Primitive(prim)) => lower_primitive(ctx, *prim),
        Some(TypeKind::Named { symbol }) => {
            let resolution = ctx.graph.resolution().unwrap();
            let sym_kind = resolution.symbol(*symbol).map(|s| s.kind());
            match sym_kind {
                Some(SymbolKind::Struct) => {
                    let layout_idx = get_or_create_struct_layout(ctx, *symbol);
                    BytecodeType::Struct(layout_idx)
                }
                Some(SymbolKind::Choice) => {
                    let layout_idx =
                        crate::bytecode_emission::types::get_or_create_choice_layout(ctx, *symbol);
                    BytecodeType::Choice(layout_idx)
                }
                Some(SymbolKind::ChoiceVariant) => {
                    if let Some((choice_symbol, variant_idx)) =
                        crate::bytecode_emission::helpers::find_choice_for_variant(ctx, *symbol)
                    {
                        let layout_idx =
                            crate::bytecode_emission::types::get_or_create_choice_layout(
                                ctx,
                                choice_symbol,
                            );
                        BytecodeType::ChoiceVariant(layout_idx, variant_idx as u16)
                    } else {
                        BytecodeType::Null
                    }
                }
                Some(SymbolKind::Constraint) => BytecodeType::Constraint(
                    resolution
                        .symbol(*symbol)
                        .map(|symbol| symbol.name().to_string())
                        .unwrap_or_default(),
                ),
                Some(SymbolKind::Enum) => {
                    let base_type =
                        crate::bytecode_emission::helpers::type_item_for_symbol(ctx, *symbol)
                            .and_then(|enum_item| {
                                let syntax = ctx.graph.syntax();
                                syntax
                                    .node(enum_item)?
                                    .children()
                                    .iter()
                                    .copied()
                                    .find(|child| {
                                        syntax
                                            .node(*child)
                                            .is_some_and(|node| node.kind().is_type())
                                    })
                            });
                    if let Some(base_type) = base_type {
                        let ty = ctx
                            .type_result
                            .layer()
                            .node_type(base_type)
                            .unwrap_or_else(|| TypeId::new(0));
                        let base_idx = crate::bytecode_emission::types::lower_type(ctx, ty);
                        ctx.types[base_idx.raw() as usize].clone()
                    } else {
                        BytecodeType::Null
                    }
                }
                _ => BytecodeType::Null,
            }
        }
        Some(TypeKind::Path { root, segments }) => {
            if segments.len() == 1 {
                if let Some(choice) = find_imported_choice_for_type(ctx, ty) {
                    let layout_idx = get_or_create_imported_choice_layout(ctx, &choice);
                    BytecodeType::Choice(layout_idx)
                } else {
                    BytecodeType::Null
                }
            } else if segments.len() == 2 {
                let choice_name = &segments[0];
                let variant_name = &segments[1];
                let choice = ctx
                    .type_result
                    .imported_symbol_choices
                    .get(root)
                    .or_else(|| {
                        ctx.type_result
                            .imported_path_choices
                            .values()
                            .find(|c| c.name == *choice_name)
                    });
                if let Some(choice) = choice {
                    let layout_idx = get_or_create_imported_choice_layout(ctx, choice);
                    let variant_idx = choice
                        .variants
                        .iter()
                        .position(|v| v.name == *variant_name)
                        .unwrap_or(0);
                    BytecodeType::ChoiceVariant(layout_idx, variant_idx as u16)
                } else {
                    BytecodeType::Null
                }
            } else {
                BytecodeType::Null
            }
        }
        Some(TypeKind::Array { element }) => {
            let elem_idx = crate::bytecode_emission::types::lower_type(ctx, *element);
            BytecodeType::Array(elem_idx)
        }
        Some(TypeKind::Union { members }) => {
            let null_ty = table.primitive(PrimitiveType::Null);
            let mut non_null_members = members
                .iter()
                .copied()
                .filter(|member| resolve_type_with_substitutions(ctx, *member) != null_ty);
            match (non_null_members.next(), non_null_members.next()) {
                (Some(member), None) if members.len() == 2 => {
                    let member = crate::bytecode_emission::types::lower_type(ctx, member);
                    BytecodeType::Nullable(member)
                }
                _ => BytecodeType::Null,
            }
        }
        Some(TypeKind::Tuple { elements }) => {
            let elem_idxs = elements
                .iter()
                .map(|&e| crate::bytecode_emission::types::lower_type(ctx, e))
                .collect();
            BytecodeType::Tuple(elem_idxs)
        }
        Some(TypeKind::GenericInstance { base, .. }) => {
            let base_idx = crate::bytecode_emission::types::lower_type(ctx, *base);
            ctx.types[base_idx.raw() as usize].clone()
        }
        _ => BytecodeType::Null,
    };

    ctx.types[next_idx.raw() as usize] = image_type.clone();

    next_idx
}

pub(super) fn lower_choice_variant_type(ctx: &mut LowerCtx, variant_symbol: SymbolId) -> TypeIdx {
    let Some((choice_symbol, variant_index)) =
        crate::bytecode_emission::helpers::find_choice_for_variant(ctx, variant_symbol)
    else {
        {
            return TypeIdx(0);
        }
    };
    let layout_idx =
        crate::bytecode_emission::types::get_or_create_choice_layout(ctx, choice_symbol);
    let variant_index = variant_index as u16;

    if let Some(index) = ctx.types.iter().position(|ty| {
        matches!(
            ty,
            BytecodeType::ChoiceVariant(existing_layout, existing_variant)
                if *existing_layout == layout_idx && *existing_variant == variant_index
        )
    }) {
        return TypeIdx(index as u16);
    }

    let type_idx = TypeIdx(ctx.types.len() as u16);
    ctx.types
        .push(BytecodeType::ChoiceVariant(layout_idx, variant_index));
    type_idx
}

fn lower_primitive(_ctx: &LowerCtx, prim: PrimitiveType) -> BytecodeType {
    match prim {
        PrimitiveType::Null => BytecodeType::Null,
        PrimitiveType::Bool => BytecodeType::Bool,
        PrimitiveType::Int8 => BytecodeType::Int8,
        PrimitiveType::Int16 => BytecodeType::Int16,
        PrimitiveType::Int32 => BytecodeType::Int32,
        PrimitiveType::Int64 => BytecodeType::Int64,
        PrimitiveType::Uint8 => BytecodeType::Uint8,
        PrimitiveType::Uint16 => BytecodeType::Uint16,
        PrimitiveType::Uint32 => BytecodeType::Uint32,
        PrimitiveType::Uint64 => BytecodeType::Uint64,
        PrimitiveType::Float16 => BytecodeType::Float32,
        PrimitiveType::Float32 => BytecodeType::Float32,
        PrimitiveType::Float64 => BytecodeType::Float64,
    }
}

pub fn get_or_create_struct_layout(ctx: &mut LowerCtx, struct_symbol: SymbolId) -> StructLayoutIdx {
    if let Some(&idx) = ctx.struct_map.get(&struct_symbol) {
        return idx;
    }

    let next_idx = StructLayoutIdx(ctx.struct_layouts.len() as u16);
    ctx.struct_map.insert(struct_symbol, next_idx);

    let resolution = ctx.graph.resolution().unwrap();
    let symbol_data = resolution.symbol(struct_symbol).unwrap();
    let struct_name = ctx
        .string_table
        .resolve(symbol_data.name())
        .unwrap_or("")
        .to_string();

    let raw_fields = crate::bytecode_emission::types::get_struct_fields(ctx, struct_symbol);
    let fields = raw_fields
        .into_iter()
        .map(|(name, ty)| {
            let ty_idx = crate::bytecode_emission::types::lower_type(ctx, ty);
            FieldLayout {
                name,
                ty: ty_idx,
                offset: 0,
                ownership: OwnershipKind::Value,
            }
        })
        .collect();

    ctx.struct_layouts.push(StructLayout {
        name: struct_name,
        fields,
        constraints: crate::bytecode_emission::types::get_struct_constraints(ctx, struct_symbol),
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
                .and_then(|res| res.reference_symbol(base))
                .or_else(|| resolution.and_then(|res| res.type_reference_symbol(base)))
                .or_else(|| resolution.and_then(|res| res.type_path_reference_symbol(base)))
                .and_then(|symbol| resolution.and_then(|res| res.symbol(symbol)))
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

pub fn get_or_create_choice_layout(ctx: &mut LowerCtx, choice_symbol: SymbolId) -> ChoiceLayoutIdx {
    if let Some(&idx) = ctx.choice_map.get(&choice_symbol) {
        return idx;
    }

    let resolution = ctx.graph.resolution().unwrap();
    let symbol_data = resolution.symbol(choice_symbol).unwrap();
    let choice_name = ctx
        .string_table
        .resolve(symbol_data.name())
        .unwrap_or("")
        .to_string();

    if let Some(pos) = ctx
        .choice_layouts
        .iter()
        .position(|layout| layout.name == choice_name)
    {
        let idx = ChoiceLayoutIdx(pos as u16);
        ctx.choice_map.insert(choice_symbol, idx);
        return idx;
    }

    let next_idx = ChoiceLayoutIdx(ctx.choice_layouts.len() as u16);
    ctx.choice_map.insert(choice_symbol, next_idx);

    let raw_variants = crate::bytecode_emission::types::get_choice_variants(ctx, choice_symbol);
    let variants = raw_variants
        .into_iter()
        .map(|(name, payload_ty)| {
            let payload_idx =
                payload_ty.map(|ty| crate::bytecode_emission::types::lower_type(ctx, ty));
            ChoiceVariantLayout {
                name,
                payload_ty: payload_idx,
            }
        })
        .collect();

    ctx.choice_layouts.push(ChoiceLayout {
        name: choice_name,
        variants,
    });

    next_idx
}

pub fn resolve_alias_type(ctx: &LowerCtx, ty: TypeId) -> TypeId {
    let mut visited = Vec::new();
    crate::bytecode_emission::types::resolve_alias_type_with_visited(ctx, ty, &mut visited)
}

pub fn resolve_alias_type_with_visited(
    ctx: &LowerCtx,
    ty: TypeId,
    visited: &mut Vec<SymbolId>,
) -> TypeId {
    let table = ctx.type_result.layer().table();
    let Some(TypeKind::Named { symbol }) = table.kind(ty) else {
        return ty;
    };
    let Some(resolution) = ctx.graph.resolution() else {
        return ty;
    };
    let Some(symbol_data) = resolution.symbol(*symbol) else {
        return ty;
    };
    if symbol_data.kind() != SymbolKind::TypeAlias {
        return ty;
    }
    if visited.contains(symbol) {
        return ty;
    }
    visited.push(*symbol);
    let underlying_ty = ctx.type_result.layer().symbol_type(*symbol).unwrap_or(ty);
    crate::bytecode_emission::types::resolve_alias_type_with_visited(ctx, underlying_ty, visited)
}

pub fn get_struct_fields(ctx: &LowerCtx, struct_symbol: SymbolId) -> Vec<(String, TypeId)> {
    let mut visited = HashSet::new();
    crate::bytecode_emission::types::get_struct_fields_internal(ctx, struct_symbol, &mut visited)
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
        Some(res) => res,
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
            .and_then(|fl| syntax.node(fl))
            .map(|n| n.children())
            .unwrap_or(&[]);

        for &field_child in field_children {
            let node_kind = syntax.node(field_child).map(|n| n.kind());
            if node_kind == Some(SyntaxNodeKind::StructExpansion) {
                let target_sym = syntax
                    .child(field_child, 0)
                    .and_then(|target| ctx.type_result.layer().node_type(target))
                    .and_then(|target_ty| {
                        crate::bytecode_emission::helpers::struct_symbol_for_type(ctx, target_ty)
                    });
                if let Some(target_sym) = target_sym {
                    for (exp_name, exp_ty) in
                        crate::bytecode_emission::types::get_struct_fields_internal(
                            ctx, target_sym, visited,
                        )
                    {
                        if !fields.iter().any(|(n, _)| *n == exp_name) {
                            fields.push((exp_name, exp_ty));
                        }
                    }
                }
            } else if node_kind == Some(SyntaxNodeKind::StructField)
                && let Some(ident_node) =
                    syntax.first_child_of_kind(field_child, SyntaxNodeKind::Identifier)
            {
                let name_str =
                    crate::bytecode_emission::helpers::node_text(ctx, ident_node).to_string();
                let field_ty = resolution
                    .declaration_symbol(ident_node)
                    .and_then(|sym| ctx.type_result.layer().symbol_type(sym))
                    .or_else(|| ctx.type_result.layer().node_type(field_child));
                if let Some(ty) = field_ty
                    && !fields.iter().any(|(n, _)| *n == name_str)
                {
                    fields.push((name_str, ty));
                }
            }
        }
    }

    if let Some(scope) = resolution
        .member_scope(struct_symbol)
        .and_then(|ms| resolution.scope(ms))
    {
        for (name, &symbol) in scope.symbols() {
            let field_ty = resolution
                .symbol(symbol)
                .filter(|sd| sd.kind() == SymbolKind::StructField)
                .and_then(|_| ctx.type_result.layer().symbol_type(symbol));
            if let Some(ty) = field_ty {
                let name_str = ctx.string_table.resolve(*name).unwrap_or("").to_string();
                if let Some(existing) = fields.iter_mut().find(|(n, _)| *n == name_str) {
                    existing.1 = ty;
                } else {
                    fields.push((name_str, ty));
                }
            }
        }
    }
    fields
}

pub fn get_choice_variants(
    ctx: &LowerCtx,
    choice_symbol: SymbolId,
) -> Vec<(String, Option<TypeId>)> {
    let resolution = match ctx.graph.resolution() {
        Some(res) => res,
        None => return Vec::new(),
    };
    let mut variants = Vec::new();
    let root = ctx.graph.syntax().root().unwrap();
    if let Some(choice_node_id) =
        crate::bytecode_emission::helpers::choice_item_node_for_symbol(ctx, root, choice_symbol)
    {
        let syntax = ctx.graph.syntax();
        let variant_list_node = syntax
            .first_child_of_kind(choice_node_id, SyntaxNodeKind::ChoiceVariantList)
            .unwrap_or(choice_node_id);
        if let Some(choice_node) = syntax.node(variant_list_node) {
            for &child in choice_node.children() {
                if let Some(variant_node) = syntax.node(child)
                    && variant_node.kind() == SyntaxNodeKind::ChoiceVariant
                    && let Some(ident_node) =
                        syntax.first_child_of_kind(child, SyntaxNodeKind::Identifier)
                {
                    let variant_name =
                        crate::bytecode_emission::helpers::node_text(ctx, ident_node).to_string();
                    if let Some(variant_symbol) = resolution.declaration_symbol(ident_node) {
                        let payload_types =
                            crate::bytecode_emission::types::choice_variant_payload_types(
                                ctx,
                                choice_symbol,
                                variant_symbol,
                            );
                        let payload_ty = if payload_types.is_empty() {
                            None
                        } else if payload_types.len() == 1 {
                            Some(payload_types[0])
                        } else {
                            Some(crate::bytecode_emission::helpers::find_tuple_type(
                                ctx,
                                &payload_types,
                            ))
                        };
                        variants.push((variant_name, payload_ty));
                    }
                }
            }
        }
    }
    variants
}

fn choice_variant_payload_types(
    ctx: &LowerCtx,
    owner_symbol: SymbolId,
    variant_symbol: SymbolId,
) -> Vec<TypeId> {
    let resolution = match ctx.graph.resolution() {
        Some(res) => res,
        None => return Vec::new(),
    };
    let variant_data = match resolution.symbol(variant_symbol) {
        Some(data) => data,
        None => return Vec::new(),
    };
    let root = ctx.graph.syntax().root().unwrap();
    let choice_item = match crate::bytecode_emission::helpers::choice_item_node_for_symbol(
        ctx,
        root,
        owner_symbol,
    ) {
        Some(node) => node,
        None => return Vec::new(),
    };
    let choice_node = match ctx.graph.syntax().node(choice_item) {
        Some(node) => node,
        None => return Vec::new(),
    };
    let mut variant_node = None;
    for &child in choice_node.children() {
        if let Some(node) = crate::bytecode_emission::helpers::find_choice_variant_node_by_name(
            ctx,
            child,
            ctx.string_table.resolve(variant_data.name()).unwrap_or(""),
        ) {
            variant_node = Some(node);
            break;
        }
    }
    let variant_node_id = match variant_node {
        Some(id) => id,
        None => return Vec::new(),
    };
    let payload = match crate::bytecode_emission::helpers::find_descendant_of_kind(
        ctx,
        variant_node_id,
        SyntaxNodeKind::ChoicePayload,
    ) {
        Some(id) => id,
        None => return Vec::new(),
    };
    let payload_node = match ctx.graph.syntax().node(payload) {
        Some(node) => node,
        None => return Vec::new(),
    };
    payload_node
        .children()
        .iter()
        .filter_map(|child| {
            let type_node =
                crate::bytecode_emission::helpers::first_type_child(ctx, *child).unwrap_or(*child);
            ctx.type_result.layer().node_type(type_node)
        })
        .collect()
}

pub fn find_imported_choice_for_type(
    ctx: &LowerCtx,
    ty: TypeId,
) -> Option<galfus_frontend::LoweredImportedChoice> {
    let table = ctx.type_result.layer().table();
    let (_root, segments) = match table.kind(ty) {
        Some(TypeKind::Path { root, segments }) => (*root, segments),
        _ => return None,
    };
    if segments.len() != 1 {
        return None;
    }
    let choice_name = &segments[0];
    ctx.type_result
        .imported_path_choices
        .values()
        .find(|c| c.name == *choice_name)
        .cloned()
}

pub fn get_or_create_imported_choice_layout(
    ctx: &mut LowerCtx,
    choice: &galfus_frontend::LoweredImportedChoice,
) -> ChoiceLayoutIdx {
    if let Some(pos) = ctx
        .choice_layouts
        .iter()
        .position(|c| c.name == choice.name)
    {
        return ChoiceLayoutIdx(pos as u16);
    }

    let next_idx = ChoiceLayoutIdx(ctx.choice_layouts.len() as u16);

    ctx.choice_layouts.push(ChoiceLayout {
        name: choice.name.clone(),
        variants: Vec::new(),
    });

    let variants = choice
        .variants
        .iter()
        .map(|v| {
            let payload_idx = if v.payload_types.is_empty() {
                None
            } else if v.payload_types.len() == 1 {
                Some(crate::bytecode_emission::types::lower_type(
                    ctx,
                    v.payload_types[0],
                ))
            } else {
                Some(crate::bytecode_emission::types::lower_type(
                    ctx,
                    crate::bytecode_emission::helpers::find_tuple_type(ctx, &v.payload_types),
                ))
            };
            ChoiceVariantLayout {
                name: v.name.clone(),
                payload_ty: payload_idx,
            }
        })
        .collect();

    ctx.choice_layouts[next_idx.raw() as usize].variants = variants;
    next_idx
}
