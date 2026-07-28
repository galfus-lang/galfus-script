
use crate::ExpressionInferrer;
use galfus_core::{NodeId, SymbolId, TypeId};

impl<'a> ExpressionInferrer<'a> {
    pub(super) fn generic_parameter_bound_type(&self, _symbol: SymbolId) -> Option<TypeId> {
        None
    }

    pub(super) fn check_struct_literal_fields(
        &mut self,
        _node: NodeId,
        _fields: NodeId,
        _struct_symbol: SymbolId,
        expected: TypeId,
        _struct_name: &str,
    ) -> TypeId {
        expected
    }

    pub(super) fn infer_struct_literal_type(
        &mut self,
        _node: NodeId,
        expected: Option<TypeId>,
    ) -> Option<TypeId> {
        expected
    }

    pub(super) fn infer_match_expression_type(
        &mut self,
        _node: NodeId,
        expected: Option<TypeId>,
    ) -> Option<TypeId> {
        expected
    }

    pub(super) fn infer_path_variant_expression_type(
        &mut self,
        _node: NodeId,
        _expected: Option<TypeId>,
    ) -> Option<TypeId> {
        None
    }

    pub(super) fn infer_function_expression_type(
        &mut self,
        _node: NodeId,
        expected: Option<TypeId>,
    ) -> Option<TypeId> {
        expected
    }

    pub(super) fn is_fieldless_struct_type(&mut self, _type_id: TypeId) -> bool {
        false
    }

    pub(super) fn infer_call_expression_type(
        &mut self,
        _node: NodeId,
        _expected: Option<TypeId>,
    ) -> Option<TypeId> {
        None
    }

    pub(super) fn infer_typeof_expression_type(
        &mut self,
        _node: NodeId,
        _expected: Option<TypeId>,
    ) -> Option<TypeId> {
        None
    }
}
