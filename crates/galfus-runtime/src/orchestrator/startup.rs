use std::collections::VecDeque;

pub(crate) struct StartupPlan {
    pub(crate) initializers:
        VecDeque<(galfus_core::ModuleId, galfus_bytecode::instruction::FuncIdx)>,
    pub(crate) entry_module_id: galfus_core::ModuleId,
    pub(crate) entry_func: galfus_bytecode::instruction::FuncIdx,
    pub(crate) entry_args: galfus_vm::VmValue,
}
