pub mod constants;
mod expression;
pub mod function;
pub mod helpers;
mod module;
pub mod ssa;
pub mod types;

use crate::bytecode_emission::constants::HashableConstant;
use galfus_bytecode::instruction::{ConstIdx, FuncIdx, TypeIdx};
use galfus_bytecode::*;
use galfus_core::{FunctionId, SymbolId, TypeId};
use galfus_frontend::{ModuleGraph, TypeCheckResult};
use galfus_ir::mir::Constant as MirConstant;
pub use module::*;
use std::collections::HashMap;

pub struct LowerCtx<'a> {
    pub type_result: &'a TypeCheckResult,
    pub graph: &'a ModuleGraph,
    pub source_text: &'a str,
    pub string_table: &'a galfus_frontend::StringTable,
    pub is_adapter_proxy: bool,
    pub proxy_name: Option<String>,
    pub types: Vec<BytecodeType>,
    pub struct_layouts: Vec<StructLayout>,
    pub choice_layouts: Vec<ChoiceLayout>,
    pub type_map: HashMap<TypeId, TypeIdx>,
    pub struct_map: HashMap<SymbolId, StructLayoutIdx>,
    pub choice_map: HashMap<SymbolId, ChoiceLayoutIdx>,
    /// Layouts for concrete `choice<T>` instantiations. A choice symbol alone
    /// is insufficient because every payload can depend on its arguments.
    pub generic_choice_map: HashMap<TypeId, ChoiceLayoutIdx>,
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
}

impl<'a> LowerCtx<'a> {
    pub fn new(
        type_result: &'a TypeCheckResult,
        graph: &'a ModuleGraph,
        source_text: &'a str,
        mir_constants: &'a [MirConstant],
        string_table: &'a galfus_frontend::StringTable,
        is_adapter_proxy: bool,
        proxy_name: Option<String>,
    ) -> Self {
        Self {
            type_result,
            graph,
            source_text,
            string_table,
            is_adapter_proxy,
            proxy_name,
            types: Vec::new(),
            struct_layouts: Vec::new(),
            choice_layouts: Vec::new(),
            type_map: HashMap::new(),
            struct_map: HashMap::new(),
            choice_map: HashMap::new(),
            generic_choice_map: HashMap::new(),
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
        }
    }
}
