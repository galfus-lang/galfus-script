use super::{LowerCtx, types_choice_layouts::intern_generic_choice_layout};
use galfus_bytecode::instruction::TypeIdx;
use galfus_bytecode::{BytecodeType, ChoiceLayout, ChoiceLayoutIdx, ChoiceVariantLayout};
use galfus_core::{OpaqueTypeId, SymbolId, TypeId};
use galfus_frontend::{PrimitiveType, SymbolKind, SyntaxNodeKind, TypeKind};

pub use super::types_choice_layouts::{
    canonical_bytecode_type_name, find_imported_choice_for_type, get_choice_variants,
    get_or_create_generic_imported_choice_layout, get_or_create_imported_choice_layout,
};
pub use super::types_structs::{get_or_create_struct_layout, get_struct_fields};

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
                let Some((choice, variant_name)) =
                    imported_choice_for_path(ctx, *root, segments.as_slice())
                else {
                    return next_idx;
                };

                let layout_idx = get_or_create_imported_choice_layout(ctx, &choice);
                match variant_name {
                    None => BytecodeType::Choice(layout_idx),
                    Some(variant_name) => choice
                        .variants
                        .iter()
                        .position(|variant| variant.name == variant_name)
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

pub(super) fn imported_choice_for_type(
    ctx: &LowerCtx,
    ty: TypeId,
) -> Option<galfus_frontend::LoweredImportedChoice> {
    let mut current = resolve_alias_type(ctx, ty);
    loop {
        match ctx.type_result.layer().table().kind(current)? {
            TypeKind::Named { symbol } => {
                return ctx.type_result.imported_symbol_choices.get(symbol).cloned();
            }
            TypeKind::Path { root, segments } => {
                return imported_choice_for_path(ctx, *root, segments.as_slice())
                    .map(|(choice, _)| choice);
            }
            TypeKind::GenericInstance { base, .. } => {
                current = *base;
            }
            _ => return None,
        }
    }
}

fn imported_choice_for_path<'a>(
    ctx: &LowerCtx,
    root: SymbolId,
    segments: &'a [String],
) -> Option<(galfus_frontend::LoweredImportedChoice, Option<&'a str>)> {
    if let Some(choice) = ctx.type_result.imported_symbol_choices.get(&root) {
        return Some((choice.clone(), segments.first().map(String::as_str)));
    }

    let (choice_name, remaining_segments) = segments.split_first()?;
    let choice = ctx
        .type_result
        .imported_namespace_choices
        .get(&(root, choice_name.clone()))?
        .clone();
    Some((choice, remaining_segments.first().map(String::as_str)))
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
    let Some((choice_symbol, variant_index)) =
        crate::bytecode_emission::helpers::find_choice_for_variant(ctx, variant_symbol)
    else {
        unreachable!("choice variant pattern must resolve to its owner choice");
    };

    let type_idx = crate::bytecode_emission::types::lower_type(ctx, instance_ty);
    let layout_idx = match &ctx.types[type_idx.raw() as usize] {
        BytecodeType::Choice(layout_idx) => *layout_idx,
        _ => {
            let resolved_ty = resolve_type_with_substitutions(ctx, instance_ty);
            let generic_arguments = match ctx.type_result.layer().table().kind(resolved_ty) {
                Some(TypeKind::GenericInstance { base, arguments })
                    if local_choice_symbol_for_type(ctx, *base) == Some(choice_symbol) =>
                {
                    Some(arguments.clone())
                }
                _ => None,
            };

            if let Some(arguments) = generic_arguments {
                get_or_create_generic_choice_layout(ctx, resolved_ty, choice_symbol, &arguments)
            } else {
                get_or_create_choice_layout(ctx, choice_symbol)
            }
        }
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
    choice_name: &str,
    variant_name: &str,
) -> TypeIdx {
    let type_idx = crate::bytecode_emission::types::lower_type(ctx, instance_ty);
    let layout_idx = match &ctx.types[type_idx.raw() as usize] {
        BytecodeType::Choice(layout_idx) => *layout_idx,
        _ => {
            ctx.emission_errors.push(format!(
                "cannot lower imported choice pattern `{choice_name}::{variant_name}`: operand type {instance_ty:?} is not a choice"
            ));
            return type_idx;
        }
    };

    let Some(layout) = ctx.choice_layouts.get(layout_idx.raw() as usize) else {
        ctx.emission_errors.push(format!(
            "cannot lower imported choice pattern `{choice_name}::{variant_name}`: choice layout {layout_idx:?} is unavailable"
        ));
        return type_idx;
    };
    let Some(variant_index) = layout
        .variants
        .iter()
        .position(|variant| variant.name == variant_name)
    else {
        ctx.emission_errors.push(format!(
            "cannot lower imported choice pattern `{choice_name}::{variant_name}`: variant is unavailable"
        ));
        return type_idx;
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

    let def_id = galfus_core::DefId::new(ctx.module_id, choice_symbol);
    let canonical_name = format!("{:?}::{}", def_id, choice_name);
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
    _instance_ty: TypeId,
    choice_symbol: SymbolId,
    arguments: &[TypeId],
) -> ChoiceLayoutIdx {
    let resolution = ctx.graph.resolution().unwrap();
    let choice_name = resolution
        .symbol(choice_symbol)
        .and_then(|symbol| ctx.string_table.resolve(symbol.name()))
        .unwrap_or("");
    let def_id = galfus_core::DefId::new(ctx.module_id, choice_symbol);
    let full_choice_name = format!("{:?}::{}", def_id, choice_name);
    let canonical_name = intern_generic_choice_layout(ctx, def_id, full_choice_name, arguments);
    if let Some(index) = ctx
        .choice_layouts
        .iter()
        .position(|layout| layout.name == canonical_name)
    {
        return ChoiceLayoutIdx(index as u16);
    }

    let next_idx = ChoiceLayoutIdx(ctx.choice_layouts.len() as u16);
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
