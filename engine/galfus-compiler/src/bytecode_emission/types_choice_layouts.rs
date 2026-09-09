use super::LowerCtx;
use galfus_bytecode::instruction::TypeIdx;
use galfus_bytecode::{BytecodeType, ChoiceLayout, ChoiceLayoutIdx, ChoiceVariantLayout};
use galfus_core::{DefId, SymbolId, TypeId};
use galfus_frontend::SyntaxNodeKind;

pub fn get_choice_variants(
    ctx: &LowerCtx,
    choice_symbol: SymbolId,
) -> Vec<(String, Option<TypeId>)> {
    let resolution = match ctx.graph.resolution() {
        Some(resolution) => resolution,
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
                    && let Some(identifier) =
                        syntax.first_child_of_kind(child, SyntaxNodeKind::Identifier)
                {
                    let variant_name =
                        crate::bytecode_emission::helpers::node_text(ctx, identifier).to_string();
                    if let Some(variant_symbol) = resolution.declaration_symbol(identifier) {
                        let payload_types =
                            choice_variant_payload_types(ctx, choice_symbol, variant_symbol);
                        let payload_ty = match payload_types.as_slice() {
                            [] => None,
                            [payload] => Some(*payload),
                            payloads => Some(crate::bytecode_emission::helpers::find_tuple_type(
                                ctx, payloads,
                            )),
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
        Some(resolution) => resolution,
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
    let variant_node = choice_node.children().iter().find_map(|&child| {
        crate::bytecode_emission::helpers::find_choice_variant_node_by_name(
            ctx,
            child,
            ctx.string_table.resolve(variant_data.name()).unwrap_or(""),
        )
    });
    let Some(variant_node) = variant_node else {
        return Vec::new();
    };
    let Some(payload) = crate::bytecode_emission::helpers::find_descendant_of_kind(
        ctx,
        variant_node,
        SyntaxNodeKind::ChoicePayload,
    ) else {
        return Vec::new();
    };
    let Some(payload_node) = ctx.graph.syntax().node(payload) else {
        return Vec::new();
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
    super::types::imported_choice_for_type(ctx, ty)
}

pub fn get_or_create_imported_choice_layout(
    ctx: &mut LowerCtx,
    choice: &galfus_frontend::LoweredImportedChoice,
) -> ChoiceLayoutIdx {
    let canonical_name = format!("{:?}::{}", choice.def_id, choice.name);
    if let Some(pos) = ctx
        .choice_layouts
        .iter()
        .position(|layout| layout.name == canonical_name)
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
        .map(|variant| {
            let payload_ty = match variant.payload_types.as_slice() {
                [] => None,
                [payload] => Some(crate::bytecode_emission::types::lower_type(ctx, *payload)),
                payloads => Some(crate::bytecode_emission::types::lower_type(
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

    ctx.choice_layouts[next_idx.raw() as usize].variants = variants;
    next_idx
}

pub fn get_or_create_generic_imported_choice_layout(
    ctx: &mut LowerCtx,
    _instance_ty: TypeId,
    choice: &galfus_frontend::LoweredImportedChoice,
    arguments: &[TypeId],
) -> ChoiceLayoutIdx {
    let full_choice_name = format!("{:?}::{}", choice.def_id, choice.name);
    let canonical_name =
        intern_generic_choice_layout(ctx, choice.def_id, full_choice_name, arguments);
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
                [payload] => Some(crate::bytecode_emission::types::lower_type(ctx, *payload)),
                payloads => Some(crate::bytecode_emission::types::lower_type(
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

pub(super) fn intern_generic_choice_layout(
    ctx: &mut LowerCtx,
    def_id: DefId,
    full_choice_name: String,
    arguments: &[TypeId],
) -> String {
    let argument_names = arguments
        .iter()
        .map(|&ty| {
            let ty_idx = crate::bytecode_emission::types::lower_type(ctx, ty);
            canonical_bytecode_type_name(ctx, ty_idx)
        })
        .collect::<Vec<_>>();
    let global_layout_id =
        ctx.generic_choice_layouts
            .intern(crate::bytecode_emission::GenericChoiceLayoutKey {
                def_id,
                arguments: argument_names.clone(),
            });
    if argument_names.is_empty() {
        format!("{}#{}", full_choice_name, global_layout_id.raw())
    } else {
        format!(
            "{}<{}>#{}",
            full_choice_name,
            argument_names.join(", "),
            global_layout_id.raw()
        )
    }
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
            let elements = elements
                .iter()
                .map(|element| canonical_bytecode_type_name(ctx, *element))
                .collect::<Vec<_>>();
            format!("({})", elements.join(", "))
        }
        BytecodeType::Choice(idx) => ctx.choice_layouts[idx.raw() as usize].name.clone(),
        BytecodeType::Constraint(name) => name.clone(),
        BytecodeType::Function { params, ret } => {
            let parameters = params
                .iter()
                .map(|parameter| canonical_bytecode_type_name(ctx, *parameter))
                .collect::<Vec<_>>();
            format!(
                "fn({}) -> {}",
                parameters.join(", "),
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
