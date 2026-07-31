use crate::input::CompiledModule;
use galfus_core::{FunctionId, NodeId, SymbolId};
use galfus_frontend::{SymbolKind, SyntaxNodeKind};

pub(super) fn collect_call_targets(
    blocks: &[galfus_ir::mir::BasicBlock],
    targets: &mut Vec<FunctionId>,
) {
    for bb in blocks {
        for inst in &bb.instructions {
            match &inst.0 {
                galfus_ir::mir::Instruction::Assign(_, value) => {
                    collect_rvalue_function_targets(value, targets);
                }
                galfus_ir::mir::Instruction::StoreGlobal(_, operand) => {
                    collect_operand_function_target(operand, targets);
                }
                galfus_ir::mir::Instruction::StoreIndex { arr, idx, val } => {
                    collect_operand_function_target(arr, targets);
                    collect_operand_function_target(idx, targets);
                    collect_operand_function_target(val, targets);
                }
                galfus_ir::mir::Instruction::StoreField { obj, val, .. } => {
                    collect_operand_function_target(obj, targets);
                    collect_operand_function_target(val, targets);
                }
                galfus_ir::mir::Instruction::Call { func, args, .. } => {
                    targets.push(*func);
                    collect_operand_function_targets(args, targets);
                }
                galfus_ir::mir::Instruction::IndirectCall { func, args, .. } => {
                    collect_operand_function_target(func, targets);
                    collect_operand_function_targets(args, targets);
                }
                galfus_ir::mir::Instruction::ConstraintCall { obj, args, .. } => {
                    collect_operand_function_target(obj, targets);
                    collect_operand_function_targets(args, targets);
                }
                galfus_ir::mir::Instruction::Drop(_) => {}
            }
        }
        match &bb.terminator.0 {
            galfus_ir::mir::Terminator::Return(Some(operand)) => {
                collect_operand_function_target(operand, targets);
            }
            galfus_ir::mir::Terminator::Jump { args, .. } => {
                collect_operand_function_targets(args, targets);
            }
            galfus_ir::mir::Terminator::Branch {
                cond,
                true_args,
                false_args,
                ..
            } => {
                collect_operand_function_target(cond, targets);
                collect_operand_function_targets(true_args, targets);
                collect_operand_function_targets(false_args, targets);
            }
            galfus_ir::mir::Terminator::Return(None) | galfus_ir::mir::Terminator::Panic(_) => {}
        }
    }
}

fn collect_rvalue_function_targets(value: &galfus_ir::mir::RValue, targets: &mut Vec<FunctionId>) {
    use galfus_ir::mir::RValue;

    match value {
        RValue::Use(operand)
        | RValue::UnaryOp(_, operand)
        | RValue::Cast(operand, _)
        | RValue::Copy(operand)
        | RValue::MemberAccess(operand, _)
        | RValue::ChoiceVariantIs(operand, _)
        | RValue::Instanceof(operand, _)
        | RValue::Len(operand) => collect_operand_function_target(operand, targets),
        RValue::BinaryOp(_, left, right) | RValue::ArrayIndex(left, right) => {
            collect_operand_function_target(left, targets);
            collect_operand_function_target(right, targets);
        }
        RValue::NewStruct { fields, .. }
        | RValue::NewArray(_, fields)
        | RValue::NewTuple(_, fields) => collect_operand_function_targets(fields, targets),
        RValue::NewArrayDynamic(_, elements) => {
            for element in elements {
                let operand = match element {
                    galfus_ir::mir::ArrayLiteralElement::Single(operand)
                    | galfus_ir::mir::ArrayLiteralElement::Spread(operand) => operand,
                };
                collect_operand_function_target(operand, targets);
            }
        }
        RValue::NewArrayZeroedDynamic { length, .. } => {
            collect_operand_function_target(length, targets);
        }
        RValue::Choice(_, _, payload) => {
            if let Some(operand) = payload {
                collect_operand_function_target(operand, targets);
            }
        }
        RValue::NewArrayZeroed { .. } | RValue::LoadGlobal(_) => {}
    }
}

fn collect_operand_function_targets(
    operands: &[galfus_ir::mir::Operand],
    targets: &mut Vec<FunctionId>,
) {
    for operand in operands {
        collect_operand_function_target(operand, targets);
    }
}

fn collect_operand_function_target(
    operand: &galfus_ir::mir::Operand,
    targets: &mut Vec<FunctionId>,
) {
    if let galfus_ir::mir::Operand::Constant(galfus_ir::mir::Constant::Function(func)) = operand {
        targets.push(*func);
    }
}

pub(super) fn resolve_import_target(
    modules: &[CompiledModule],
    mod_idx: usize,
    func_id: FunctionId,
) -> Option<(galfus_core::ModuleId, FunctionId)> {
    let module = &modules[mod_idx];
    let resolution = module.graph().resolution()?;
    let symbol_id = SymbolId::new(func_id.raw());

    let mut import_symbol = symbol_id;

    if let Some(node_id) = path_call_target_node(func_id) {
        if let Some(syntax_node) = module.graph().syntax().node(node_id)
            && syntax_node.kind() == SyntaxNodeKind::PathExpression
            && let Some(root_node) = syntax_node.first_child()
        {
            let root_sym = resolution.reference_symbol(root_node);
            if let Some(root_symbol) = root_sym {
                let import_found = resolution
                    .imports()
                    .iter()
                    .find(|imp| imp.local_symbol() == root_symbol);
                if let Some(import) = import_found
                    && let Some(member_node) = syntax_node.child(1)
                {
                    let syntax = module.graph().syntax();
                    let member_node_data = syntax.node(member_node)?;
                    let member_span = member_node_data.span();
                    let member_name = module.source().slice(member_span)?;
                    let target_idx = import_target_index(modules, mod_idx, import.source());
                    if let Some(target_idx) = target_idx {
                        let target_mod = &modules[target_idx];
                        let target_resolution = target_mod.graph().resolution()?;
                        for export in target_resolution.exports() {
                            if export.name() == member_name {
                                return Some((
                                    target_mod.id(),
                                    FunctionId::new(export.symbol().raw()),
                                ));
                            }
                        }
                    }
                }
            }
        }

        if let Some(syntax_node) = module.graph().syntax().node(node_id)
            && syntax_node.kind() == SyntaxNodeKind::PathExpression
            && let Some(member_node) = syntax_node.child(1)
            && let Some(member_node_data) = module.graph().syntax().node(member_node)
            && let Some(member_name) = module.source().slice(member_node_data.span())
            && let Some(receiver) = syntax_node.child(0)
            && let Some(source) = import_source_for_expression(module, receiver)
            && let Some(target_idx) = import_target_index(modules, mod_idx, source)
            && let Some(target_resolution) = modules[target_idx].graph().resolution()
        {
            let mut candidates = target_resolution.exports().iter().filter_map(|export| {
                let matches_member = export.name() == member_name
                    || export.name().ends_with(&format!("::{member_name}"));
                (export.kind() == SymbolKind::Function && matches_member).then_some((
                    modules[target_idx].id(),
                    FunctionId::new(export.symbol().raw()),
                ))
            });
            let first = candidates.next();
            if first.is_some() && candidates.next().is_none() {
                return first;
            }
        }

        import_symbol = module
            .graph()
            .syntax()
            .node(node_id)
            .and_then(|_| resolution.reference_symbol(node_id))
            .unwrap_or(symbol_id);
    }

    if let Some(import) = resolution
        .imports()
        .iter()
        .find(|imp| imp.local_symbol() == import_symbol)
        && let Some(imported_name) = import.imported_name()
    {
        let target_idx = import_target_index(modules, mod_idx, import.source())?;
        let target_mod = &modules[target_idx];
        let target_resolution = target_mod.graph().resolution()?;
        for export in target_resolution.exports() {
            if export.name() == imported_name {
                return Some((target_mod.id(), FunctionId::new(export.symbol().raw())));
            }
        }
    }

    let mut candidates = modules[mod_idx]
        .graph()
        .resolution()?
        .imports()
        .iter()
        .filter_map(|import| {
            let target_idx = import_target_index(modules, mod_idx, import.source())?;
            let target_resolution = modules[target_idx].graph().resolution()?;
            target_resolution
                .exports()
                .iter()
                .find(|export| {
                    export.kind() == SymbolKind::Function && export.symbol().raw() == func_id.raw()
                })
                .map(|export| {
                    (
                        modules[target_idx].id(),
                        FunctionId::new(export.symbol().raw()),
                    )
                })
        });
    let first = candidates.next();
    if first.is_some() && candidates.next().is_none() {
        return first;
    }

    None
}

pub(super) fn resolve_local_call_target(
    modules: &[CompiledModule],
    mod_idx: usize,
    mir_mod: &galfus_ir::mir::MirModule,
    func_id: FunctionId,
) -> Option<FunctionId> {
    let module = &modules[mod_idx];
    let node_id = path_call_target_node(func_id)?;
    let node = module.graph().syntax().node(node_id)?;
    if node.kind() != SyntaxNodeKind::PathExpression {
        return None;
    }
    if let Some(symbol) = module.graph().resolution()?.path_reference_symbol(node_id) {
        return Some(FunctionId::new(symbol.raw()));
    }

    let member_node = node.child(1)?;
    let member_node_data = module.graph().syntax().node(member_node)?;
    let member_name = module.source().slice(member_node_data.span())?;
    let mut candidates = mir_mod.functions.iter().filter_map(|function| {
        let matches_member =
            function.name == member_name || function.name.ends_with(&format!("::{member_name}"));
        matches_member.then_some(function.id)
    });
    let first = candidates.next();
    if first.is_some() && candidates.next().is_none() {
        return first;
    }

    None
}

const PATH_CALL_TARGET_TAG: u32 = 0x8000_0000;

fn path_call_target_node(func_id: FunctionId) -> Option<NodeId> {
    let raw = func_id.raw();
    (raw & PATH_CALL_TARGET_TAG != 0).then(|| NodeId::new(raw & !PATH_CALL_TARGET_TAG))
}

/// Resolve the index into `modules` for a relative import from `mod_idx`.
pub(super) fn import_target_index(
    modules: &[CompiledModule],
    mod_idx: usize,
    source: &str,
) -> Option<usize> {
    let direct_path = galfus_core::ModulePath::new(source)
        .or_else(|| galfus_core::ModulePath::new(format!("{source}.gfs").as_str()));
    if let Some(target) = direct_path {
        if let Some(index) = modules.iter().position(|module| module.path() == &target) {
            return Some(index);
        }
    }

    let target = galfus_frontend::modules::resolution::resolve_relative_import(
        modules[mod_idx].path(),
        source,
    )?;
    modules.iter().position(|m| m.path() == &target)
}

fn import_source_for_expression(module: &CompiledModule, expr: NodeId) -> Option<&str> {
    let syntax = module.graph().syntax();
    let resolution = module.graph().resolution()?;
    let node = syntax.node(expr)?;

    match node.kind() {
        SyntaxNodeKind::CallExpression | SyntaxNodeKind::GenericExpression => {
            let target = node.child(0)?;
            import_source_for_expression(module, target)
        }
        SyntaxNodeKind::PathExpression => {
            let root = node.child(0)?;
            import_source_for_expression(module, root)
        }
        SyntaxNodeKind::NameExpression | SyntaxNodeKind::Identifier => {
            let symbol = resolution.reference_symbol(expr).or_else(|| {
                let ident = syntax.first_child_of_kind(expr, SyntaxNodeKind::Identifier)?;
                resolution.reference_symbol(ident)
            })?;
            resolution
                .imports()
                .iter()
                .find(|import| import.local_symbol() == symbol)
                .map(|import| import.source())
        }
        _ => None,
    }
}
