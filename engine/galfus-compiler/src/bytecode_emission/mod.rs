pub mod constants;
mod expression;
mod expression_immediates;
mod expression_operands;
pub mod function;
mod function_calls;
mod function_terminators;
pub mod helpers;
mod module;
mod parallel_copies;
pub mod ssa;
pub mod types;
mod types_choice_layouts;
mod types_structs;

#[cfg(test)]
mod tests;

use crate::bytecode_emission::constants::HashableConstant;
use galfus_bytecode::instruction::{ConstIdx, FuncIdx, TypeIdx};
use galfus_bytecode::*;
use galfus_core::{DefId, FunctionId, SymbolId, TypeId};
use galfus_frontend::{ModuleGraph, TypeCheckResult};
use galfus_ir::mir::Constant as MirConstant;
pub use module::*;
use std::collections::HashMap;

/// Stable identity assigned to a generic layout for the lifetime of a compiler
/// session. `ChoiceLayoutIdx` cannot serve this purpose because it is an index
/// into a single `BytecodeModule`'s layout vector.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct GlobalChoiceLayoutId(u32);

impl GlobalChoiceLayoutId {
    pub const fn raw(self) -> u32 {
        self.0
    }
}

/// Canonical key for a concrete choice instantiation.
///
/// Type ids are owned by individual type tables, so arguments are represented
/// by their bytecode-level canonical form before crossing module boundaries.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct GenericChoiceLayoutKey {
    pub def_id: DefId,
    pub arguments: Vec<String>,
}

pub type GlobalChoiceLayouts = HashMap<GenericChoiceLayoutKey, GlobalChoiceLayoutId>;

#[derive(Debug, Default, Clone)]
pub struct GenericChoiceLayoutCache {
    layouts: GlobalChoiceLayouts,
    next_id: u32,
}

impl GenericChoiceLayoutCache {
    pub fn intern(&mut self, key: GenericChoiceLayoutKey) -> GlobalChoiceLayoutId {
        if let Some(id) = self.layouts.get(&key).copied() {
            return id;
        }

        let id = GlobalChoiceLayoutId(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        self.layouts.insert(key, id);
        id
    }

    pub fn retain_modules(
        &mut self,
        live_modules: &std::collections::HashSet<galfus_core::ModuleId>,
        changed_modules: &std::collections::HashSet<galfus_core::ModuleId>,
    ) {
        self.layouts.retain(|key, _| {
            key.def_id.module == galfus_core::ModuleId::new(0)
                || (live_modules.contains(&key.def_id.module)
                    && !changed_modules.contains(&key.def_id.module))
        });
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.layouts.len()
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.layouts.is_empty()
    }
}

pub struct LowerCtx<'a> {
    pub type_result: &'a TypeCheckResult,
    pub graph: &'a ModuleGraph,
    pub source_text: &'a str,
    pub module_id: galfus_core::ModuleId,
    pub module_path: &'a str,
    pub string_table: &'a galfus_frontend::StringTable,
    pub is_adapter_proxy: bool,
    pub proxy_name: Option<String>,
    pub types: Vec<BytecodeType>,
    pub struct_layouts: Vec<StructLayout>,
    pub choice_layouts: Vec<ChoiceLayout>,
    pub type_map: HashMap<TypeId, TypeIdx>,
    pub struct_map: HashMap<SymbolId, StructLayoutIdx>,
    pub choice_map: HashMap<SymbolId, ChoiceLayoutIdx>,
    /// Project-wide interning of concrete choice identities. The materialized
    /// `ChoiceLayoutIdx` remains local because bytecode modules own their
    /// layout vectors.
    pub generic_choice_layouts: &'a mut GenericChoiceLayoutCache,
    pub constant_pool: ConstantPool,
    pub constants_map: HashMap<HashableConstant, ConstIdx>,
    pub function_map: HashMap<FunctionId, FuncIdx>,
    pub function_names: HashMap<FunctionId, String>,
    pub function_return_types: HashMap<FunctionId, TypeId>,
    pub function_param_types: HashMap<FunctionId, Vec<TypeId>>,
    pub async_return_type_overrides: HashMap<FunctionId, TypeIdx>,
    pub imported_struct_fields: HashMap<SymbolId, Vec<(String, TypeId)>>,
    pub active_substitutions: HashMap<SymbolId, TypeId>,
    pub function_is_async: HashMap<FunctionId, bool>,
    pub mir_constants: &'a [MirConstant],
    /// Errors found while lowering MIR that must prevent bytecode publication.
    pub emission_errors: Vec<String>,
}

impl<'a> LowerCtx<'a> {
    pub fn new(
        module_id: galfus_core::ModuleId,
        type_result: &'a TypeCheckResult,
        graph: &'a ModuleGraph,
        source_text: &'a str,
        mir_constants: &'a [MirConstant],
        string_table: &'a galfus_frontend::StringTable,
        module_path: &'a str,
        is_adapter_proxy: bool,
        proxy_name: Option<String>,
        generic_choice_layouts: &'a mut GenericChoiceLayoutCache,
    ) -> Self {
        Self {
            type_result,
            graph,
            source_text,
            module_id,
            module_path,
            string_table,
            is_adapter_proxy,
            proxy_name,
            types: Vec::new(),
            struct_layouts: Vec::new(),
            choice_layouts: Vec::new(),
            type_map: HashMap::new(),
            struct_map: HashMap::new(),
            choice_map: HashMap::new(),
            generic_choice_layouts,
            constant_pool: ConstantPool {
                constants: Vec::new(),
            },
            constants_map: HashMap::new(),
            function_map: HashMap::new(),
            function_names: HashMap::new(),
            function_return_types: HashMap::new(),
            function_param_types: HashMap::new(),
            async_return_type_overrides: HashMap::new(),
            imported_struct_fields: type_result
                .imported_struct_fields
                .iter()
                .map(|(symbol, fields)| {
                    (
                        *symbol,
                        fields
                            .iter()
                            .map(|field| (field.name.clone(), field.ty))
                            .collect(),
                    )
                })
                .collect(),
            active_substitutions: HashMap::new(),
            function_is_async: HashMap::new(),
            mir_constants,
            emission_errors: Vec::new(),
        }
    }
}
