//! Module compilation: produces one `BytecodeNode` per `CompiledModule`.
//!
//! Each compiled module:
//! - Declares `ExportSlot`s for its public symbols.
//! - Declares `ImportSlot`s for symbols it uses from other modules.
//! - Contains only its own functions; cross-module calls target an import slot
//!   via a local `FuncIdx` that the runtime resolves at load time.

use std::collections;
use std::iter;

use crate::CompilerState;

use crate::compile::{
    context::MyWorkspaceContext,
    globals::{image_local_count, rewrite_global_indices},
};
use crate::input::CompiledModule;
/// Compile all modules in `modules`, each producing its own
/// `BytecodeModule` with imports and exports declared.
///
/// Cross-module calls are represented as `Call` instructions that target a
/// `FuncIdx` in the local import table. The runtime is responsible for
/// resolving these at load time.
use anyhow::Result;
use galfus_bytecode::graph::BytecodeNode;
use galfus_bytecode::{
    BytecodeFunction, BytecodeGraphTransaction, BytecodeModule, BytecodeType, ExportSlot,
    ImportEdge, ImportSlot,
    instruction::{FuncIdx, TypeIdx},
};
use std::collections::{HashMap, HashSet};

pub fn compile_modules(
    modules: &mut [CompiledModule],
    state: &mut CompilerState,
) -> Result<Vec<BytecodeNode>> {
    let module_ids = modules.iter().map(CompiledModule::id).collect();
    compile_changed_modules(modules, state, &module_ids)
}

/// Compile only modules identified by `changed_modules`.
///
/// All modules remain available as semantic context so imports and generic
/// specializations can be resolved across module boundaries. Only the selected
/// modules are lowered into new `BytecodeNode`s.
pub fn compile_changed_modules(
    modules: &mut [CompiledModule],
    state: &mut CompilerState,
    changed_modules: &HashSet<galfus_core::ModuleId>,
) -> Result<Vec<BytecodeNode>> {
    if changed_modules.is_empty() {
        return Ok(Vec::new());
    }

    // Phase 1: Build MIR only for changed modules. Generic specializations can
    // add functions to an imported module, which then becomes affected too.
    let mut ws_ctx = MyWorkspaceContext::new(modules, state);
    let mut affected_modules = modules
        .iter()
        .enumerate()
        .filter_map(|(index, module)| changed_modules.contains(&module.id()).then_some(index))
        .collect::<HashSet<_>>();
    let mut pending_modules = affected_modules.iter().copied().collect::<Vec<_>>();
    let mut mir_modules = iter::repeat_with(|| None)
        .take(modules.len())
        .collect::<Vec<Option<galfus_ir::mir::MirModule>>>();

    while let Some(module_index) = pending_modules.pop() {
        let module = &modules[module_index];
        let type_res = module.type_result().ok_or_else(|| {
            anyhow::anyhow!(
                "Module is missing type checking result: {}",
                module.path().as_str()
            )
        })?;
        let mir = crate::semantic_to_mir::MirBuilder::new(
            module.graph(),
            type_res,
            module.source().text(),
        )
        .with_workspace_module_id(module.id())
        .with_workspace_ctx(&mut ws_ctx)
        .build();
        mir_modules[module_index] = Some(mir);

        for (module_id, specialized) in ws_ctx.state.specialised_functions.iter() {
            if !specialized.is_empty() {
                if let Some(target_index) = modules.iter().position(|m| m.id() == *module_id) {
                    if affected_modules.insert(target_index) {
                        pending_modules.push(target_index);
                    }
                }
            }
        }
    }

    // Append specialized functions.
    for module_index in &affected_modules {
        let module_id = modules[*module_index].id();
        let mut specialized = ws_ctx
            .state
            .specialised_functions
            .get(&module_id)
            .cloned()
            .unwrap_or_default();
        mir_modules[*module_index]
            .as_mut()
            .expect("affected module has MIR")
            .functions
            .append(&mut specialized);
    }
    let specialized_targets = ws_ctx.state.specialised_id_to_target.clone();
    drop(ws_ctx);

    // Phase 2: Compile each module independently.
    let mut outputs = Vec::new();
    for mod_idx in 0..modules.len() {
        let module_id = modules[mod_idx].id();
        if !affected_modules.contains(&mod_idx) {
            continue;
        }
        let path = modules[mod_idx].path().clone();
        let semantic_revision = modules[mod_idx].semantic_revision();
        let (image, metadata) =
            compile_single_module(modules, &mir_modules, &specialized_targets, mod_idx)?;
        if let Err(errors) = galfus_bytecode::validation::validate_bytecode_module(&image) {
            return Err(anyhow::anyhow!(
                "BytecodeModule validation failed for `{}`: {:?}",
                path.as_str(),
                errors
            ));
        }
        outputs.push(BytecodeNode {
            id: module_id,
            path,
            semantic_revision,
            module: image,
            metadata: Some(metadata),
        });
    }

    Ok(outputs)
}

/// Compile changed modules and package them into one graph transaction.
pub fn compile_transaction(
    modules: &mut [CompiledModule],
    state: &mut CompilerState,
    changed_modules: &HashSet<galfus_core::ModuleId>,
    base_version: u64,
    semantic_revision: galfus_core::SemanticRevision,
    removed_modules: Vec<galfus_core::ModuleId>,
    edges: Vec<ImportEdge>,
) -> Result<BytecodeGraphTransaction> {
    let upserted_modules = compile_changed_modules(modules, state, changed_modules)?;
    Ok(BytecodeGraphTransaction {
        base_version,
        semantic_revision,
        upserted_modules,
        removed_modules,
        edges,
    })
}

fn compile_single_module(
    modules: &mut [CompiledModule],
    mir_modules: &[Option<galfus_ir::mir::MirModule>],
    specialized_targets: &HashMap<
        galfus_core::FunctionId,
        (galfus_core::ModuleId, galfus_core::FunctionId),
    >,
    mod_idx: usize,
) -> Result<(BytecodeModule, galfus_bytecode::graph::ExecutionMetadata)> {
    use crate::compile::resolve::{
        collect_call_targets, resolve_import_target, resolve_local_call_target,
    };
    use galfus_frontend::SymbolKind;

    let mir_mod = mir_modules[mod_idx]
        .as_ref()
        .expect("compiled module has MIR");

    // Collect all cross-module call targets: (local_func_id → (target_mod, target_func)).
    let mut cross_module_calls: HashMap<
        galfus_core::FunctionId,
        (galfus_core::ModuleId, galfus_core::FunctionId),
    > = HashMap::new();

    for func in &mir_mod.functions {
        let mut targets = Vec::new();
        collect_call_targets(&func.blocks, &mut targets);
        for func_id in targets {
            if let Some(&resolved) = specialized_targets.get(&func_id) {
                cross_module_calls.insert(func_id, resolved);
            } else if let Some(resolved) = resolve_import_target(modules, mod_idx, func_id) {
                cross_module_calls.insert(func_id, resolved);
            } else if let Some(local_id) =
                resolve_local_call_target(modules, mod_idx, mir_mod, func_id)
            {
                // Local call — no import needed.
                let _ = local_id;
            }
        }
    }

    // Build the import table: one ImportSlot per unique (target_mod, target_func).
    // We assign local FuncIdx starting after the module's own functions.
    let own_func_count = mir_mod.functions.len() as u16;
    let mut import_slots: Vec<ImportSlot> = Vec::new();
    // Map: (target_mod_id, target_func_id) → local FuncIdx in the import table.
    let mut import_func_map: HashMap<(galfus_core::ModuleId, galfus_core::FunctionId), FuncIdx> =
        HashMap::new();

    for (&local_id, &(target_module_id, target_func_id)) in &cross_module_calls {
        let entry = import_func_map
            .entry((target_module_id, target_func_id))
            .or_insert_with(|| {
                let slot_idx = own_func_count + import_slots.len() as u16;
                let target_mod_idx = modules
                    .iter()
                    .position(|m| m.id() == target_module_id)
                    .unwrap_or(0);
                let target_module = &modules[target_mod_idx];
                let symbol_name = target_module
                    .graph()
                    .resolution()
                    .and_then(|res| {
                        res.exports().iter().find_map(|export| {
                            if export.symbol().raw() == target_func_id.raw() {
                                Some(export.name().to_string())
                            } else {
                                None
                            }
                        })
                    })
                    .unwrap_or_else(|| format!("func_{}", target_func_id.raw()));

                let module_name = target_module.path().as_str().to_string();

                import_slots.push(ImportSlot {
                    module_name,
                    symbol_name,
                    // Type info not yet resolved — placeholder.
                    ty: TypeIdx(0),
                    kind: galfus_bytecode::ImportKind::Function,
                });

                FuncIdx(slot_idx)
            });
        let _ = (local_id, entry);
    }

    // Build local func map: local func_id → local FuncIdx (0-based within this module).
    let mut local_func_map: HashMap<galfus_core::FunctionId, FuncIdx> = HashMap::new();
    for (i, func) in mir_mod.functions.iter().enumerate() {
        local_func_map.insert(func.id, FuncIdx(i as u16));
    }

    // Collect exports from this module's resolution.
    let mut export_slots: Vec<ExportSlot> = Vec::new();
    if let Some(resolution) = modules[mod_idx].graph().resolution() {
        for export in resolution.exports() {
            if export.kind() == SymbolKind::Function {
                let func_id = galfus_core::FunctionId::new(export.symbol().raw());
                if let Some(&local_idx) = local_func_map.get(&func_id) {
                    export_slots.push(ExportSlot {
                        symbol_name: export.name().to_string(),
                        kind: galfus_bytecode::ExportKind::Function(local_idx),
                    });
                }
            } else if export.kind() == SymbolKind::Var || export.kind() == SymbolKind::Const {
                export_slots.push(ExportSlot {
                    symbol_name: export.name().to_string(),
                    kind: galfus_bytecode::ExportKind::Global(
                        galfus_bytecode::instruction::GlobalIdx(export.symbol().raw() as u16),
                    ),
                });
            }
        }
    }
    for &(target_module_id, target_func_id) in specialized_targets.values() {
        if target_module_id != modules[mod_idx].id() {
            continue;
        }
        let Some(&local_idx) = local_func_map.get(&target_func_id) else {
            continue;
        };
        if export_slots
            .iter()
            .any(|export| matches!(export.kind, galfus_bytecode::ExportKind::Function(idx) if idx == local_idx))
        {
            continue;
        }
        export_slots.push(ExportSlot {
            symbol_name: format!("func_{}", target_func_id.raw()),
            kind: galfus_bytecode::ExportKind::Function(local_idx),
        });
    }

    // Lowering phase.
    let module = &modules[mod_idx];
    let type_res = module
        .type_result()
        .ok_or_else(|| anyhow::anyhow!("Missing type result for {}", module.path().as_str()))?;

    let mut ctx = crate::bytecode_emission::LowerCtx::new(
        type_res,
        module.graph(),
        module.source().text(),
        &mir_mod.constant_pool,
    );

    // Register local functions in ctx.
    for func in &mir_mod.functions {
        let local_idx = local_func_map[&func.id];
        ctx.function_map.insert(func.id, local_idx);
        ctx.function_names.insert(func.id, func.name.clone());
        ctx.function_return_types.insert(func.id, func.return_type);
    }

    let mut execution_metadata = galfus_bytecode::graph::ExecutionMetadata {
        spans: collections::HashMap::new(),
    };

    // Register cross-module calls as import function slots.
    for (&local_id, &(target_mod_idx, target_func_id)) in &cross_module_calls {
        if let Some(&import_idx) = import_func_map.get(&(target_mod_idx, target_func_id)) {
            ctx.function_map.insert(local_id, import_idx);
        }
    }

    let mut functions: Vec<BytecodeFunction> = Vec::new();
    let mut init_func_idx: Option<FuncIdx> = None;

    for mir_func in &mir_mod.functions {
        let is_init = mir_func.name == "__init_module";
        let local_func_idx = local_func_map[&mir_func.id];

        if is_init {
            init_func_idx = Some(local_func_idx);
        }

        ctx.active_substitutions = mir_func.type_substitutions.clone();

        let return_ty = crate::bytecode_emission::types::lower_type(&mut ctx, mir_func.return_type);
        for &param_ty in &mir_func.parameter_types {
            crate::bytecode_emission::types::lower_type(&mut ctx, param_ty);
        }

        let param_count = mir_func.parameter_types.len() as u16;
        let local_count = image_local_count(mir_func, param_count);

        let mut emitter = crate::bytecode_emission::function::FnEmitter::new(
            &mut ctx,
            mir_func,
            param_count,
            local_count,
        );
        let (mut instructions, function_spans) = emitter.emit();
        execution_metadata
            .spans
            .insert(local_func_idx, function_spans);

        rewrite_global_indices(&mut instructions, modules, mod_idx)?;

        functions.push(BytecodeFunction {
            name: mir_func.name.clone(),
            param_count: param_count.try_into().unwrap(),
            local_count,
            temp_count: emitter.temp_count_max,
            return_ty,
            instructions,
        });
    }

    let null_type_idx = if let Some(pos) = ctx
        .types
        .iter()
        .position(|t| matches!(t, BytecodeType::Null))
    {
        TypeIdx(pos as u16)
    } else {
        let idx = TypeIdx(ctx.types.len() as u16);
        ctx.types.push(BytecodeType::Null);
        idx
    };
    let _ = null_type_idx;

    let module = BytecodeModule {
        name: module.path().as_str().to_string(),
        constants: ctx.constant_pool,
        functions,
        types: ctx.types,
        struct_layouts: ctx.struct_layouts,
        choice_layouts: ctx.choice_layouts,
        imports: import_slots,
        exports: export_slots,
        init_func_idx,
    };
    Ok((module, execution_metadata))
}
