use super::resolve::import_target_index;
use crate::input::CompiledModule;
use anyhow::Result;
use galfus_bytecode::instruction::{GlobalIdx, Instruction};

fn canonical_global_ref(
    modules: &[CompiledModule],
    mod_idx: usize,
    local_pos: u16,
) -> Result<(galfus_core::ModuleId, GlobalIdx)> {
    let module = modules
        .get(mod_idx)
        .ok_or_else(|| anyhow::anyhow!("invalid module index `{mod_idx}` during global rewrite"))?;
    let resolution = module.graph().resolution().ok_or_else(|| {
        anyhow::anyhow!(
            "missing resolver output for module `{}` during global rewrite",
            module.path().as_str()
        )
    })?;
    let symbol = resolution
        .symbols()
        .get(local_pos as usize)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "missing local/global symbol at position `{local_pos}` in module `{}`",
                module.path().as_str()
            )
        })?;

    if let Some(import) = resolution
        .imports()
        .iter()
        .find(|import| import.local_symbol() == symbol.id())
    {
        let imported_name = import.imported_name().ok_or_else(|| {
            anyhow::anyhow!(
                "module import `{}` in `{}` cannot be used as a global value directly",
                import.source(),
                module.path().as_str()
            )
        })?;
        let target_idx =
            import_target_index(modules, mod_idx, import.source()).ok_or_else(|| {
                anyhow::anyhow!(
                    "could not resolve import `{}` from module `{}` while rewriting global `{}`",
                    import.source(),
                    module.path().as_str(),
                    imported_name
                )
            })?;
        let target = &modules[target_idx];
        let target_resolution = target.graph().resolution().ok_or_else(|| {
            anyhow::anyhow!(
                "missing resolver output for imported module `{}` during global rewrite",
                target.path().as_str()
            )
        })?;
        let target_global_idx = target_resolution
            .symbols()
            .iter()
            .position(|target_symbol| target_symbol.name() == imported_name)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "could not locate imported global `{}` in module `{}`",
                    imported_name,
                    target.path().as_str()
                )
            })?;
        return Ok((target.id(), GlobalIdx(target_global_idx as u16)));
    }

    Ok((module.id(), GlobalIdx(local_pos)))
}

pub(super) fn rewrite_global_indices(
    instructions: &mut [Instruction],
    modules: &[CompiledModule],
    mod_idx: usize,
) -> Result<()> {
    for instruction in instructions {
        match instruction {
            Instruction::LoadGlobal {
                module_id,
                global_idx,
                ..
            }
            | Instruction::StoreGlobal {
                module_id,
                global_idx,
                ..
            } => {
                (*module_id, *global_idx) =
                    canonical_global_ref(modules, mod_idx, global_idx.raw())?;
            }
            _ => {}
        }
    }

    Ok(())
}

pub(super) fn image_local_count(mir_func: &galfus_ir::mir::MirFunction, param_count: u16) -> u16 {
    let max_local_id = mir_func
        .locals
        .iter()
        .map(|local| local.id.raw() as u16)
        .max()
        .map(|max_id| max_id + 1)
        .unwrap_or(param_count);

    max_local_id.saturating_sub(param_count)
}
