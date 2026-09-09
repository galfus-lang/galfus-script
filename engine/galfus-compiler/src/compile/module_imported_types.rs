use galfus_bytecode::{BytecodeType, instruction::TypeIdx};
use galfus_core::TypeId;
use galfus_frontend::{ModuleAst, SymbolKind, TypeCheckResult, TypeKind, TypeTable};

pub(super) fn lower_imported_async_return_type(
    ctx: &mut crate::bytecode_emission::LowerCtx,
    target_graph: &ModuleAst,
    target_types: &TypeCheckResult,
    target_module_id: galfus_core::ModuleId,
    return_type: TypeId,
) -> Option<TypeIdx> {
    let (choice_def_id, target_arguments) = imported_choice_def_id_and_arguments(
        target_graph,
        target_types,
        target_module_id,
        return_type,
    )?;
    let choice = ctx
        .type_result
        .imported_symbol_choices
        .values()
        .chain(ctx.type_result.imported_namespace_choices.values())
        .find(|choice| choice.def_id == choice_def_id)?
        .clone();
    let layout = if target_arguments.is_empty() {
        crate::bytecode_emission::types::get_or_create_imported_choice_layout(ctx, &choice)
    } else {
        let arguments = target_arguments
            .into_iter()
            .map(|argument| translate_imported_type(ctx, target_types, argument))
            .collect::<Option<Vec<_>>>()?;
        let instance_ty = find_imported_choice_instance(ctx, &choice, &arguments)?;
        crate::bytecode_emission::types::get_or_create_generic_imported_choice_layout(
            ctx,
            instance_ty,
            &choice,
            &arguments,
        )
    };
    let type_idx = TypeIdx(ctx.types.len() as u16);
    ctx.types.push(BytecodeType::Choice(layout));
    Some(type_idx)
}

fn imported_choice_def_id_and_arguments(
    graph: &ModuleAst,
    types: &TypeCheckResult,
    module_id: galfus_core::ModuleId,
    ty: TypeId,
) -> Option<(galfus_core::DefId, Vec<TypeId>)> {
    let table = types.layer().table();
    let (ty, arguments) = match table.kind(ty)? {
        TypeKind::GenericInstance { base, arguments } => (*base, arguments.clone()),
        _ => (ty, Vec::new()),
    };
    match table.kind(ty)? {
        TypeKind::Named { symbol } => types
            .imported_symbol_choices
            .get(symbol)
            .map(|choice| (choice.def_id, arguments.clone()))
            .or_else(|| {
                graph
                    .resolution()?
                    .symbol(*symbol)
                    .filter(|symbol| symbol.kind() == SymbolKind::Choice)
                    .map(|_| (galfus_core::DefId::new(module_id, *symbol), arguments))
            }),
        TypeKind::Path { root, segments } => {
            let (choice_name, _) = segments.split_first()?;
            types
                .imported_symbol_choices
                .get(root)
                .map(|choice| (choice.def_id, arguments.clone()))
                .or_else(|| {
                    types
                        .imported_namespace_choices
                        .get(&(*root, choice_name.clone()))
                        .map(|choice| (choice.def_id, arguments))
                })
        }
        _ => None,
    }
}

fn translate_imported_type(
    ctx: &crate::bytecode_emission::LowerCtx,
    source_types: &TypeCheckResult,
    source_ty: TypeId,
) -> Option<TypeId> {
    let target_table = ctx.type_result.layer().table();
    (0..target_table.len())
        .map(|index| TypeId::new(index as u32))
        .find(|candidate| {
            types_are_equivalent(
                target_table,
                *candidate,
                source_types.layer().table(),
                source_ty,
            )
        })
}

fn types_are_equivalent(
    left_table: &TypeTable,
    left_ty: TypeId,
    right_table: &TypeTable,
    right_ty: TypeId,
) -> bool {
    match (left_table.kind(left_ty), right_table.kind(right_ty)) {
        (Some(TypeKind::Primitive(left)), Some(TypeKind::Primitive(right))) => left == right,
        (Some(TypeKind::Array { element: left }), Some(TypeKind::Array { element: right })) => {
            types_are_equivalent(left_table, *left, right_table, *right)
        }
        (Some(TypeKind::Tuple { elements: left }), Some(TypeKind::Tuple { elements: right })) => {
            left.len() == right.len()
                && left.iter().zip(right).all(|(left, right)| {
                    types_are_equivalent(left_table, *left, right_table, *right)
                })
        }
        _ => false,
    }
}

fn find_imported_choice_instance(
    ctx: &crate::bytecode_emission::LowerCtx,
    choice: &galfus_frontend::LoweredImportedChoice,
    arguments: &[TypeId],
) -> Option<TypeId> {
    let table = ctx.type_result.layer().table();
    (0..table.len())
        .map(|index| TypeId::new(index as u32))
        .find(|ty| {
            let Some(TypeKind::GenericInstance {
                base,
                arguments: candidate_arguments,
            }) = table.kind(*ty)
            else {
                return false;
            };
            candidate_arguments == arguments
                && crate::bytecode_emission::types::find_imported_choice_for_type(ctx, *base)
                    .is_some_and(|candidate| candidate.def_id == choice.def_id)
        })
}
