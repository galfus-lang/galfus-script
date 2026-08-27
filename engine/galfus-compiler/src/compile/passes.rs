#[cfg(test)]
mod tests;

use anyhow::{Result, anyhow};
use galfus_ir::mir::{Instruction, MirModule, Operand, RValue};

use super::{inline::inline_functions, tco::optimize_tail_calls};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MirPassConfiguration {
    pub local_simplification: bool,
    pub inlining: bool,
    pub tail_calls: bool,
}

impl Default for MirPassConfiguration {
    fn default() -> Self {
        Self {
            local_simplification: true,
            inlining: true,
            tail_calls: true,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct MirPassReport {
    pub instructions_before: usize,
    pub instructions_after: usize,
    pub calls_before: usize,
    pub calls_after: usize,
    pub simplified_instructions: usize,
    pub inlined_calls: usize,
    pub tail_calls: usize,
}

pub fn run(module: &mut MirModule, configuration: MirPassConfiguration) -> Result<MirPassReport> {
    validate(module, "before")?;
    let mut report = MirPassReport {
        instructions_before: instruction_count(module),
        calls_before: call_count(module),
        ..MirPassReport::default()
    };

    if configuration.local_simplification {
        report.simplified_instructions = simplify_local_identities(module);
        validate(module, "after local simplification")?;
    }
    if configuration.inlining {
        report.inlined_calls = inline_functions(module);
        validate(module, "after inlining")?;
    }
    if configuration.tail_calls {
        report.tail_calls = optimize_tail_calls(module);
        validate(module, "after tail-call recognition")?;
    }

    report.instructions_after = instruction_count(module);
    report.calls_after = call_count(module);
    Ok(report)
}

fn validate(module: &MirModule, stage: &str) -> Result<()> {
    galfus_ir::validate_module(module)
        .map_err(|errors| anyhow!("MIR validation failed {stage}: {errors:?}"))
}

fn simplify_local_identities(module: &mut MirModule) -> usize {
    let mut removed = 0;
    for function in &mut module.functions {
        for block in &mut function.blocks {
            block.instructions.retain(|(instruction, _)| {
                let is_identity = matches!(
                    instruction,
                    Instruction::Assign(destination, RValue::Use(Operand::Local(source)))
                        if destination == source
                );
                removed += usize::from(is_identity);
                !is_identity
            });
        }
    }
    removed
}

fn instruction_count(module: &MirModule) -> usize {
    module
        .functions
        .iter()
        .map(|function| {
            function
                .blocks
                .iter()
                .map(|block| block.instructions.len() + 1)
                .sum::<usize>()
        })
        .sum()
}

fn call_count(module: &MirModule) -> usize {
    module
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.instructions)
        .filter(|(instruction, _)| matches!(instruction, Instruction::Call { .. }))
        .count()
}
