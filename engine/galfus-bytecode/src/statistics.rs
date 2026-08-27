#[cfg(test)]
mod tests;

use crate::{BytecodeModule, Instruction, PackageEncodingError, PackageImage};

/// Structural bytecode counts used for deterministic compiler baselines.
///
/// The collector does not execute or transform a package. Counts are based on
/// the final bytecode image and can therefore be compared across compilations.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct BytecodePackageStatistics {
    pub encoded_package_bytes: usize,
    pub module_count: usize,
    pub function_count: usize,
    pub instruction_count: usize,
    pub constant_count: usize,
    pub type_count: usize,
    pub layout_count: usize,
    pub parameter_register_count: usize,
    pub local_register_count: usize,
    pub temporary_register_count: usize,
    pub frame_register_count: usize,
    pub local_call_count: usize,
    pub import_call_count: usize,
    pub dynamic_call_count: usize,
    pub branch_count: usize,
    pub future_creation_count: usize,
    pub functions: Vec<BytecodeFunctionStatistics>,
}

/// Per-function portion of [`BytecodePackageStatistics`].
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct BytecodeFunctionStatistics {
    pub module_name: String,
    pub function_name: String,
    pub function_index: usize,
    pub instruction_count: usize,
    pub parameter_register_count: usize,
    pub local_register_count: usize,
    pub temporary_register_count: usize,
    pub frame_register_count: usize,
    pub local_call_count: usize,
    pub import_call_count: usize,
    pub dynamic_call_count: usize,
    pub branch_count: usize,
    pub future_creation_count: usize,
}

/// Collects statistics from a final package and its encoded representation.
pub fn collect_package_statistics(
    package: &PackageImage,
) -> Result<BytecodePackageStatistics, PackageEncodingError> {
    let mut statistics = BytecodePackageStatistics {
        encoded_package_bytes: package.to_bytecode()?.len(),
        module_count: package.graph().len(),
        ..BytecodePackageStatistics::default()
    };

    for node in package.graph().modules() {
        let module_statistics = collect_module_statistics(&node.module, node.path().as_str());
        statistics.function_count += module_statistics.functions.len();
        statistics.instruction_count += module_statistics.instruction_count;
        statistics.constant_count += module_statistics.constant_count;
        statistics.type_count += module_statistics.type_count;
        statistics.layout_count += module_statistics.layout_count;
        statistics.parameter_register_count += module_statistics.parameter_register_count;
        statistics.local_register_count += module_statistics.local_register_count;
        statistics.temporary_register_count += module_statistics.temporary_register_count;
        statistics.frame_register_count += module_statistics.frame_register_count;
        statistics.local_call_count += module_statistics.local_call_count;
        statistics.import_call_count += module_statistics.import_call_count;
        statistics.dynamic_call_count += module_statistics.dynamic_call_count;
        statistics.branch_count += module_statistics.branch_count;
        statistics.future_creation_count += module_statistics.future_creation_count;
        statistics.functions.extend(module_statistics.functions);
    }
    statistics.functions.sort_by(|left, right| {
        left.module_name
            .cmp(&right.module_name)
            .then(left.function_index.cmp(&right.function_index))
    });
    Ok(statistics)
}

fn collect_module_statistics(
    module: &BytecodeModule,
    module_name: &str,
) -> BytecodePackageStatistics {
    let mut statistics = BytecodePackageStatistics {
        function_count: module.functions.len(),
        constant_count: module.constants.constants.len(),
        type_count: module.types.len(),
        layout_count: module.struct_layouts.len() + module.choice_layouts.len(),
        ..BytecodePackageStatistics::default()
    };
    let local_function_count = module.functions.len();

    for (function_index, function) in module.functions.iter().enumerate() {
        let mut function_statistics = BytecodeFunctionStatistics {
            module_name: module_name.to_string(),
            function_name: function.name.clone(),
            function_index,
            instruction_count: function.instructions.len(),
            parameter_register_count: usize::from(function.param_count),
            local_register_count: usize::from(function.local_count),
            temporary_register_count: usize::from(function.temp_count),
            frame_register_count: usize::from(function.param_count)
                + usize::from(function.local_count)
                + usize::from(function.temp_count),
            ..BytecodeFunctionStatistics::default()
        };
        for instruction in &function.instructions {
            match instruction {
                Instruction::Call { func, .. } | Instruction::TailCall { func, .. } => {
                    if usize::from(func.raw()) < local_function_count {
                        function_statistics.local_call_count += 1;
                    } else {
                        function_statistics.import_call_count += 1;
                    }
                }
                Instruction::CallMethod { .. } | Instruction::CallDynamic { .. } => {
                    function_statistics.dynamic_call_count += 1;
                }
                Instruction::Jump { .. }
                | Instruction::JumpTrue { .. }
                | Instruction::JumpFalse { .. }
                | Instruction::JumpNull { .. } => function_statistics.branch_count += 1,
                Instruction::CreateFuture { .. } | Instruction::CreateAwaitFuture { .. } => {
                    function_statistics.future_creation_count += 1;
                }
                _ => {}
            }
        }
        statistics.instruction_count += function_statistics.instruction_count;
        statistics.parameter_register_count += function_statistics.parameter_register_count;
        statistics.local_register_count += function_statistics.local_register_count;
        statistics.temporary_register_count += function_statistics.temporary_register_count;
        statistics.frame_register_count += function_statistics.frame_register_count;
        statistics.local_call_count += function_statistics.local_call_count;
        statistics.import_call_count += function_statistics.import_call_count;
        statistics.dynamic_call_count += function_statistics.dynamic_call_count;
        statistics.branch_count += function_statistics.branch_count;
        statistics.future_creation_count += function_statistics.future_creation_count;
        statistics.functions.push(function_statistics);
    }
    statistics
}
