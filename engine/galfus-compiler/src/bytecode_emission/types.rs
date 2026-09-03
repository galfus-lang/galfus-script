use super::LowerCtx;
use galfus_bytecode::instruction::TypeIdx;
use galfus_bytecode::{
    BytecodeType, ChoiceLayout, ChoiceLayoutIdx, ChoiceVariantLayout, FieldLayout, OwnershipKind,
    StructLayout, StructLayoutIdx,
};
use galfus_core::{OpaqueTypeId, SymbolId, TypeId};
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
        let is_null_primitive = matches!(
            ctx.type_result.layer().table().kind(ty),
            Some(TypeKind::Primitive(PrimitiveType::Null))
        );
        if is_null_primitive || !matches!(ctx.types[idx.raw() as usize], BytecodeType::Null) {
            return idx;
        }
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
                _ if ctx.imported_struct_fields.contains_key(symbol) => {
                    let layout_idx = get_or_create_struct_layout(ctx, *symbol);
                    BytecodeType::Struct(layout_idx)
                }
                _ if ctx.type_result.imported_symbol_choices.contains_key(symbol) => {
                    let choice = ctx.type_result.imported_symbol_choices.get(symbol).unwrap();
                    let layout_idx = get_or_create_imported_choice_layout(ctx, choice);
                    BytecodeType::Choice(layout_idx)
                }
                Some(SymbolKind::Struct) => {
                    if ctx.is_adapter_proxy {
                        let name = ctx
                            .string_table
                            .resolve(resolution.symbol(*symbol).unwrap().name())
                            .unwrap_or("")
                            .to_string();
                        let proxy_name = ctx.proxy_name.as_ref().unwrap();
                        BytecodeType::AdapterHandle(
                            OpaqueTypeId::new(proxy_name.clone(), name)
                                .expect("adapter proxy types have a module path and name"),
                        )
                    } else {
                        let layout_idx = get_or_create_struct_layout(ctx, *symbol);
                        BytecodeType::Struct(layout_idx)
                    }
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
                    let base_type = base_type
                        .and_then(|node| ctx.type_result.layer().node_type(node))
                        .unwrap_or_else(|| {
                            ctx.type_result
                                .layer()
                                .table()
                                .primitive(PrimitiveType::Int32)
                        });
                    let base_idx = crate::bytecode_emission::types::lower_type(ctx, base_type);
                    ctx.types[base_idx.raw() as usize].clone()
                }
                _ => BytecodeType::Null,
            }
        }
        Some(TypeKind::Path { root, segments }) => {
            if ctx.imported_struct_fields.contains_key(root) {
                let layout_idx = get_or_create_struct_layout(ctx, *root);
                BytecodeType::Struct(layout_idx)
            } else if let Some(struct_symbol) =
                imported_struct_symbol_for_path(ctx, segments.as_slice())
            {
                let layout_idx = get_or_create_struct_layout(ctx, struct_symbol);
                BytecodeType::Struct(layout_idx)
            } else if *root == SymbolId::new(0)
                && let Some(name) = segments.first()
                && let Some(name_id) = ctx.string_table.get(name)
                && let Some(resolution) = ctx.graph.resolution()
                && let Some(symbol) = resolution.lookup_symbol(resolution.module_scope(), name_id)
                && ctx.type_result.imported_struct_fields.contains_key(&symbol)
            {
                let layout_idx = get_or_create_struct_layout(ctx, symbol);
                BytecodeType::Struct(layout_idx)
            } else {
                let choice_from_symbol = ctx.type_result.imported_symbol_choices.get(root);
                let imported_choice = choice_from_symbol.or_else(|| {
                    ctx.type_result
                        .imported_path_choices
                        .values()
                        .find(|choice| {
                            segments
                                .iter()
                                .position(|segment| segment == &choice.name)
                                .is_some()
                        })
                });

                let Some(choice) = imported_choice else {
                    return next_idx;
                };

                let layout_idx = get_or_create_imported_choice_layout(ctx, choice);
                let variant_name = segments
                    .iter()
                    .position(|segment| segment == &choice.name)
                    .and_then(|choice_segment| segments.get(choice_segment + 1))
                    .or_else(|| choice_from_symbol.and_then(|_| segments.first()));
                match variant_name {
                    None => BytecodeType::Choice(layout_idx),
                    Some(variant_name) => choice
                        .variants
                        .iter()
                        .position(|variant| variant.name == *variant_name)
                        .map(|variant_idx| {
                            BytecodeType::ChoiceVariant(layout_idx, variant_idx as u16)
                        })
                        .unwrap_or(BytecodeType::Null),
                }
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
                _ => BytecodeType::Any,
            }
        }
        Some(TypeKind::Tuple { elements }) => {
            let elem_idxs = elements
                .iter()
                .map(|&e| crate::bytecode_emission::types::lower_type(ctx, e))
                .collect();
            BytecodeType::Tuple(elem_idxs)
        }
        Some(TypeKind::GenericInstance { base, arguments }) => {
            if let Some(choice_symbol) = local_choice_symbol_for_type(ctx, *base) {
                BytecodeType::Choice(get_or_create_generic_choice_layout(
                    ctx,
                    ty,
                    choice_symbol,
                    arguments,
                ))
            } else if let Some(choice) = imported_choice_for_type(ctx, *base) {
                BytecodeType::Choice(get_or_create_generic_imported_choice_layout(
                    ctx, ty, &choice, arguments,
                ))
            } else {
                let base_idx = crate::bytecode_emission::types::lower_type(ctx, *base);
                ctx.types[base_idx.raw() as usize].clone()
            }
        }
        _ => BytecodeType::Null,
    };

    ctx.types[next_idx.raw() as usize] = image_type.clone();

    next_idx
}

fn local_choice_symbol_for_type(ctx: &LowerCtx, ty: TypeId) -> Option<SymbolId> {
    let TypeKind::Named { symbol } = ctx.type_result.layer().table().kind(ty)? else {
        return None;
    };
    ctx.graph
        .resolution()?
        .symbol(*symbol)
        .filter(|symbol| symbol.kind() == SymbolKind::Choice)
        .map(|_| *symbol)
}

fn imported_choice_for_type(
    ctx: &LowerCtx,
    ty: TypeId,
) -> Option<galfus_frontend::LoweredImportedChoice> {
    match ctx.type_result.layer().table().kind(ty)? {
        TypeKind::Named { symbol } => ctx.type_result.imported_symbol_choices.get(symbol).cloned(),
        TypeKind::Path { root, segments } => ctx
            .type_result
            .imported_symbol_choices
            .get(root)
            .cloned()
            .or_else(|| {
                ctx.type_result
                    .imported_path_choices
                    .values()
                    .find(|choice| segments.iter().any(|segment| segment == &choice.name))
                    .cloned()
            }),
        _ => None,
    }
}

fn imported_struct_symbol_for_path(ctx: &LowerCtx, segments: &[String]) -> Option<SymbolId> {
    let name = segments.last()?;
    let table = ctx.type_result.layer().table();

    ctx.imported_struct_fields.keys().copied().find(|symbol| {
        let Some(function_ty) = ctx.type_result.layer().symbol_type(*symbol) else {
            return false;
        };
        let Some(TypeKind::Function(function)) = table.kind(function_ty) else {
            return false;
        };
        matches!(
            table.kind(function.return_type()),
            Some(TypeKind::Path { segments, .. }) if segments.last() == Some(name)
        )
    })
}

pub(super) fn lower_choice_variant_type(
    ctx: &mut LowerCtx,
    instance_ty: TypeId,
    variant_symbol: SymbolId,
) -> TypeIdx {
    let Some((_, variant_index)) =
        crate::bytecode_emission::helpers::find_choice_for_variant(ctx, variant_symbol)
    else {
        unreachable!("choice variant pattern must resolve to its owner choice");
    };

    let type_idx = crate::bytecode_emission::types::lower_type(ctx, instance_ty);
    let layout_idx = match &ctx.types[type_idx.raw() as usize] {
        BytecodeType::Choice(layout_idx) => *layout_idx,
        _ => unreachable!("choice variant pattern operand must have a choice type"),
    };
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

pub(super) fn lower_imported_choice_variant_type(
    ctx: &mut LowerCtx,
    instance_ty: TypeId,
    _choice_name: &str,
    variant_name: &str,
) -> TypeIdx {
    let type_idx = crate::bytecode_emission::types::lower_type(ctx, instance_ty);
    let layout_idx = match &ctx.types[type_idx.raw() as usize] {
        BytecodeType::Choice(layout_idx) => *layout_idx,
        _ => unreachable!("imported choice variant pattern operand must have a choice type"),
    };

    let Some(variant_index) = ctx.choice_layouts[layout_idx.raw() as usize]
        .variants
        .iter()
        .position(|variant| variant.name == variant_name)
    else {
        unreachable!("imported choice variant pattern must resolve to its variant");
    };
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
    let struct_name = resolution
        .symbol(struct_symbol)
        .and_then(|symbol| ctx.string_table.resolve(symbol.name()))
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

    let canonical_name = format!("{}::{}", ctx.module_path, choice_name);
    if let Some(pos) = ctx
        .choice_layouts
        .iter()
        .position(|layout| layout.name == canonical_name)
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
        name: canonical_name,
        variants,
    });

    next_idx
}

fn get_or_create_generic_choice_layout(
    ctx: &mut LowerCtx,
    instance_ty: TypeId,
    choice_symbol: SymbolId,
    arguments: &[TypeId],
) -> ChoiceLayoutIdx {
    if let Some(&idx) = ctx.generic_choice_map.get(&instance_ty) {
        return idx;
    }

    let resolution = ctx.graph.resolution().unwrap();
    let choice_name = resolution
        .symbol(choice_symbol)
        .and_then(|symbol| ctx.string_table.resolve(symbol.name()))
        .unwrap_or("");
    let full_choice_name = format!("{}::{}", ctx.module_path, choice_name);
    let canonical_name = if arguments.is_empty() {
        full_choice_name
    } else {
        let arg_names: Vec<_> = arguments
            .iter()
            .map(|&ty| {
                let ty_idx = lower_type(ctx, ty);
                canonical_bytecode_type_name(ctx, ty_idx)
            })
            .collect();
        format!("{}<{}>", full_choice_name, arg_names.join(", "))
    };
    let next_idx = ChoiceLayoutIdx(ctx.choice_layouts.len() as u16);
    ctx.generic_choice_map.insert(instance_ty, next_idx);
    ctx.choice_layouts.push(ChoiceLayout {
        name: canonical_name,
        variants: Vec::new(),
    });

    let previous_substitutions = std::mem::take(&mut ctx.active_substitutions);
    ctx.active_substitutions = previous_substitutions.clone();
    for (parameter, argument) in choice_generic_parameters(ctx, choice_symbol)
        .into_iter()
        .zip(arguments.iter().copied())
    {
        ctx.active_substitutions.insert(parameter, argument);
    }

    let variants = get_choice_variants(ctx, choice_symbol)
        .into_iter()
        .map(|(name, payload_ty)| ChoiceVariantLayout {
            name,
            payload_ty: payload_ty.map(|ty| lower_type(ctx, ty)),
        })
        .collect();
    ctx.active_substitutions = previous_substitutions;
    ctx.choice_layouts[next_idx.raw() as usize].variants = variants;
    next_idx
}

fn choice_generic_parameters(ctx: &LowerCtx, choice_symbol: SymbolId) -> Vec<SymbolId> {
    let Some(root) = ctx.graph.syntax().root() else {
        return Vec::new();
    };
    let Some(choice_item) =
        crate::bytecode_emission::helpers::choice_item_node_for_symbol(ctx, root, choice_symbol)
    else {
        return Vec::new();
    };
    let Some(parameters) = ctx
        .graph
        .syntax()
        .first_child_of_kind(choice_item, SyntaxNodeKind::GenericParameterList)
    else {
        return Vec::new();
    };
    let Some(node) = ctx.graph.syntax().node(parameters) else {
        return Vec::new();
    };
    let Some(resolution) = ctx.graph.resolution() else {
        return Vec::new();
    };

    node.children()
        .iter()
        .filter_map(|parameter| {
            let identifier = ctx
                .graph
                .syntax()
                .first_child_of_kind(*parameter, SyntaxNodeKind::Identifier)?;
            resolution.declaration_symbol(identifier)
        })
        .collect()
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
    if symbol_data.kind() != SymbolKind::TypeAlias
        && symbol_data.kind() != SymbolKind::ImportBinding
    {
        return ty;
    }
    if visited.contains(symbol) {
        return ty;
    }
    visited.push(*symbol);
    let underlying_ty = ctx.type_result.layer().symbol_type(*symbol).unwrap_or(ty);
    if underlying_ty == ty {
        return ty;
    }
    crate::bytecode_emission::types::resolve_alias_type_with_visited(ctx, underlying_ty, visited)
}

pub fn get_struct_fields(ctx: &LowerCtx, struct_symbol: SymbolId) -> Vec<(String, TypeId)> {
    if let Some(fields) = ctx.imported_struct_fields.get(&struct_symbol) {
        return fields.clone();
    }
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
    imported_choice_for_type(ctx, ty)
}

pub fn get_or_create_imported_choice_layout(
    ctx: &mut LowerCtx,
    choice: &galfus_frontend::LoweredImportedChoice,
) -> ChoiceLayoutIdx {
    let canonical_name = if choice.module_path.is_empty() {
        choice.name.clone()
    } else {
        format!("{}::{}", choice.module_path, choice.name)
    };
    if let Some(pos) = ctx
        .choice_layouts
        .iter()
        .position(|c| c.name == canonical_name)
    {
        return ChoiceLayoutIdx(pos as u16);
    }

    let next_idx = ChoiceLayoutIdx(ctx.choice_layouts.len() as u16);

    ctx.choice_layouts.push(ChoiceLayout {
        name: canonical_name,
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

pub fn get_or_create_generic_imported_choice_layout(
    ctx: &mut LowerCtx,
    instance_ty: TypeId,
    choice: &galfus_frontend::LoweredImportedChoice,
    arguments: &[TypeId],
) -> ChoiceLayoutIdx {
    if let Some(&idx) = ctx.generic_choice_map.get(&instance_ty) {
        return idx;
    }

    let canonical_name = if choice.module_path.is_empty() {
        choice.name.clone()
    } else {
        format!("{}::{}", choice.module_path, choice.name)
    };
    let canonical_name = if arguments.is_empty() {
        canonical_name
    } else {
        let arg_names: Vec<_> = arguments
            .iter()
            .map(|&ty| {
                let ty_idx = lower_type(ctx, ty);
                canonical_bytecode_type_name(ctx, ty_idx)
            })
            .collect();
        format!("{}<{}>", canonical_name, arg_names.join(", "))
    };
    let next_idx = ChoiceLayoutIdx(ctx.choice_layouts.len() as u16);
    ctx.generic_choice_map.insert(instance_ty, next_idx);
    ctx.choice_layouts.push(ChoiceLayout {
        name: canonical_name,
        variants: Vec::new(),
    });

    let previous_substitutions = std::mem::take(&mut ctx.active_substitutions);
    ctx.active_substitutions = previous_substitutions.clone();
    for (parameter, argument) in choice
        .generic_parameters
        .iter()
        .copied()
        .zip(arguments.iter().copied())
    {
        ctx.active_substitutions.insert(parameter, argument);
    }

    let variants = choice
        .variants
        .iter()
        .map(|variant| {
            let payload_ty = match variant.payload_types.as_slice() {
                [] => None,
                [payload] => Some(lower_type(ctx, *payload)),
                payloads => Some(lower_type(
                    ctx,
                    crate::bytecode_emission::helpers::find_tuple_type(ctx, payloads),
                )),
            };
            ChoiceVariantLayout {
                name: variant.name.clone(),
                payload_ty,
            }
        })
        .collect();
    ctx.active_substitutions = previous_substitutions;
    ctx.choice_layouts[next_idx.raw() as usize].variants = variants;
    next_idx
}

pub fn canonical_bytecode_type_name(ctx: &LowerCtx, ty: TypeIdx) -> String {
    match &ctx.types[ty.raw() as usize] {
        BytecodeType::Null => "null".to_string(),
        BytecodeType::Bool => "bool".to_string(),
        BytecodeType::Int8 => "i8".to_string(),
        BytecodeType::Int16 => "i16".to_string(),
        BytecodeType::Int32 => "i32".to_string(),
        BytecodeType::Int64 => "i64".to_string(),
        BytecodeType::Uint8 => "u8".to_string(),
        BytecodeType::Uint16 => "u16".to_string(),
        BytecodeType::Uint32 => "u32".to_string(),
        BytecodeType::Uint64 => "u64".to_string(),
        BytecodeType::Float32 => "f32".to_string(),
        BytecodeType::Float64 => "f64".to_string(),
        BytecodeType::AdapterHandle(id) => format!("handle<{}>", id.name()),
        BytecodeType::Struct(idx) => ctx.struct_layouts[idx.raw() as usize].name.clone(),
        BytecodeType::Array(inner) => format!("[{}]", canonical_bytecode_type_name(ctx, *inner)),
        BytecodeType::Nullable(inner) => format!("{}?", canonical_bytecode_type_name(ctx, *inner)),
        BytecodeType::Tuple(elements) => {
            let elems: Vec<_> = elements
                .iter()
                .map(|e| canonical_bytecode_type_name(ctx, *e))
                .collect();
            format!("({})", elems.join(", "))
        }
        BytecodeType::Choice(idx) => ctx.choice_layouts[idx.raw() as usize].name.clone(),
        BytecodeType::Constraint(name) => name.clone(),
        BytecodeType::Function { params, ret } => {
            let p: Vec<_> = params
                .iter()
                .map(|e| canonical_bytecode_type_name(ctx, *e))
                .collect();
            format!(
                "fn({}) -> {}",
                p.join(", "),
                canonical_bytecode_type_name(ctx, *ret)
            )
        }
        BytecodeType::ChoiceVariant(idx, variant) => {
            let choice_name = &ctx.choice_layouts[idx.raw() as usize].name;
            format!("{}::{}", choice_name, variant)
        }
        BytecodeType::Any => "any".to_string(),
    }
}
