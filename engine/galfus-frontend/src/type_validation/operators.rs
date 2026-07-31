use super::DeclarationTypeChecker;
use crate::{PrimitiveType, SyntaxNodeKind, TypeKind};
use galfus_core::{NodeId, TypeId};

impl<'a> DeclarationTypeChecker<'a> {
    pub(super) fn infer_binary_expression_type(
        &mut self,
        node: NodeId,
        expected: Option<TypeId>,
    ) -> Option<TypeId> {
        let left = self.graph.syntax().child(node, 0)?;
        let operator = self.graph.syntax().child(node, 1)?;
        let right = self.graph.syntax().child(node, 2)?;
        let operator_text = self.node_text(operator);

        let res = (|| {
            let (left_type, right_type) =
                self.infer_binary_operand_types(left, right, expected, operator_text.as_str())?;

            let ty = match operator_text.as_str() {
                "+" | "-" | "*" | "/" | "%" | "**" => {
                    self.check_numeric_binary_operator(operator, left_type, right_type)?
                }
                "<" | "<=" | ">" | ">=" => {
                    self.check_numeric_comparison_operator(operator, left_type, right_type)?
                }
                "==" | "!=" => self.check_equality_operator(operator, left_type, right_type)?,
                "&&" | "||" => self.check_bool_binary_operator(operator, left_type, right_type)?,
                "&" | "|" | "^" => {
                    self.check_integer_binary_operator(operator, left_type, right_type)?
                }
                "<<" | ">>" => self.check_shift_operator(operator, left_type, right_type)?,
                _ => {
                    self.report_unsupported_operator(operator, operator_text.as_str());
                    self.layer.table_mut().error()
                }
            };
            Some(ty)
        })();

        if let Some(ty) = res {
            self.layer.bind_node_type(node, ty);
        }
        res
    }

    pub(super) fn infer_unary_expression_type(
        &mut self,
        node: NodeId,
        expected: Option<TypeId>,
    ) -> Option<TypeId> {
        let operator = self.graph.syntax().child(node, 0)?;
        let operand = self.graph.syntax().child(node, 1)?;

        let operator_text = self.node_text(operator);
        let operand_type = if matches!(operator_text.as_str(), "+" | "-")
            && self
                .graph
                .syntax()
                .node(operand)
                .is_some_and(|node| node.kind() == SyntaxNodeKind::IntegerLiteral)
        {
            let expected = self
                .expected_integer_literal_type(expected)
                .unwrap_or_else(|| self.layer.table().primitive(PrimitiveType::Int32));
            let ty = self.checked_integer_literal_type_with_sign(
                operand,
                expected,
                operator_text == "-",
            );
            self.layer.bind_node_type(operand, ty);
            ty
        } else {
            self.infer_expression_type(operand)?
        };

        let ty = match operator_text.as_str() {
            "+" | "-" => self.check_numeric_unary_operator(operator, operand_type)?,

            "!" => self.check_bool_unary_operator(operator, operand_type)?,

            "~" => self.check_integer_unary_operator(operator, operand_type)?,

            _ => {
                self.report_unsupported_operator(operator, operator_text.as_str());
                self.layer.table_mut().error()
            }
        };

        self.layer.bind_node_type(node, ty);
        Some(ty)
    }

    fn infer_binary_operand_types(
        &mut self,
        left: NodeId,
        right: NodeId,
        expected: Option<TypeId>,
        operator_text: &str,
    ) -> Option<(TypeId, TypeId)> {
        let expected = self.expected_binary_result_type(expected, operator_text);

        let left_is_number_literal = self.is_number_literal_node(left);
        let right_is_number_literal = self.is_number_literal_node(right);

        match (left_is_number_literal, right_is_number_literal) {
            (true, true) => {
                let left_type = self.infer_expression_type_with_expected(left, expected)?;
                let right_type =
                    self.infer_expression_type_with_expected(right, Some(left_type))?;
                Some((left_type, right_type))
            }
            (true, false) => {
                let right_type = self.infer_expression_type_with_expected(right, expected)?;
                let left_type = self.infer_expression_type_with_expected(left, Some(right_type))?;
                Some((left_type, right_type))
            }
            (false, true) => {
                let left_type = self.infer_expression_type_with_expected(left, expected)?;
                let right_type =
                    self.infer_expression_type_with_expected(right, Some(left_type))?;
                Some((left_type, right_type))
            }
            (false, false) => {
                let left_type = self.infer_expression_type_with_expected(left, expected)?;
                let right_type = self.infer_expression_type_with_expected(right, expected)?;
                Some((left_type, right_type))
            }
        }
    }

    fn expected_binary_result_type(
        &self,
        expected: Option<TypeId>,
        operator_text: &str,
    ) -> Option<TypeId> {
        match operator_text {
            "+" | "-" | "*" | "/" | "%" | "**" | "&" | "|" | "^" | "<<" | ">>" => expected,
            _ => None,
        }
    }

    fn is_number_literal_node(&self, node: NodeId) -> bool {
        matches!(
            self.graph.syntax().node(node).map(|node| node.kind()),
            Some(SyntaxNodeKind::IntegerLiteral | SyntaxNodeKind::FloatLiteral)
        )
    }

    fn check_numeric_binary_operator(
        &mut self,
        operator: NodeId,
        left: TypeId,
        right: TypeId,
    ) -> Option<TypeId> {
        if let Some(common) = self.common_numeric_type(left, right) {
            return Some(common);
        }

        self.report_operator_type_error(operator, "compatible numeric operands", left, right);
        Some(self.layer.table_mut().error())
    }

    fn check_numeric_comparison_operator(
        &mut self,
        operator: NodeId,
        left: TypeId,
        right: TypeId,
    ) -> Option<TypeId> {
        if self.common_numeric_type(left, right).is_some() {
            return Some(self.layer.table().primitive(PrimitiveType::Bool));
        }

        self.report_operator_type_error(operator, "compatible numeric operands", left, right);
        Some(self.layer.table_mut().error())
    }

    fn check_equality_operator(
        &mut self,
        operator: NodeId,
        left: TypeId,
        right: TypeId,
    ) -> Option<TypeId> {
        if left == right || self.is_assignable(left, right) || self.is_assignable(right, left) {
            return Some(self.layer.table().primitive(PrimitiveType::Bool));
        }

        self.report_operator_type_error(operator, "comparable operands", left, right);
        Some(self.layer.table_mut().error())
    }

    fn check_bool_binary_operator(
        &mut self,
        operator: NodeId,
        left: TypeId,
        right: TypeId,
    ) -> Option<TypeId> {
        let bool_type = self.layer.table().primitive(PrimitiveType::Bool);

        if left == bool_type && right == bool_type {
            return Some(bool_type);
        }

        self.report_operator_type_error(operator, "bool operands", left, right);
        Some(self.layer.table_mut().error())
    }

    fn check_integer_binary_operator(
        &mut self,
        operator: NodeId,
        left: TypeId,
        right: TypeId,
    ) -> Option<TypeId> {
        if let Some(common) = self.common_integer_type(left, right) {
            return Some(common);
        }

        self.report_operator_type_error(operator, "compatible integer operands", left, right);
        Some(self.layer.table_mut().error())
    }

    fn check_shift_operator(
        &mut self,
        operator: NodeId,
        left: TypeId,
        right: TypeId,
    ) -> Option<TypeId> {
        if self.is_integer_type(left) && self.is_integer_type(right) {
            return Some(left);
        }

        self.report_operator_type_error(operator, "integer operands", left, right);
        Some(self.layer.table_mut().error())
    }

    fn check_numeric_unary_operator(
        &mut self,
        operator: NodeId,
        operand: TypeId,
    ) -> Option<TypeId> {
        if self.is_numeric_type(operand) {
            return Some(operand);
        }

        self.report_unary_operator_type_error(operator, "numeric operand", operand);
        Some(self.layer.table_mut().error())
    }

    fn check_bool_unary_operator(&mut self, operator: NodeId, operand: TypeId) -> Option<TypeId> {
        let bool_type = self.layer.table().primitive(PrimitiveType::Bool);

        if operand == bool_type {
            return Some(bool_type);
        }

        self.report_unary_operator_type_error(operator, "bool operand", operand);
        Some(self.layer.table_mut().error())
    }

    fn check_integer_unary_operator(
        &mut self,
        operator: NodeId,
        operand: TypeId,
    ) -> Option<TypeId> {
        if self.is_integer_type(operand) {
            return Some(operand);
        }

        self.report_unary_operator_type_error(operator, "integer operand", operand);
        Some(self.layer.table_mut().error())
    }

    pub(super) fn is_same_numeric_type(&self, left: TypeId, right: TypeId) -> bool {
        left == right && self.is_numeric_type(left)
    }

    pub(super) fn is_same_integer_type(&self, left: TypeId, right: TypeId) -> bool {
        left == right && self.is_integer_type(left)
    }

    fn common_numeric_type(&self, left: TypeId, right: TypeId) -> Option<TypeId> {
        if self.is_same_numeric_type(left, right) {
            return Some(left);
        }

        if let Some(common) = self.common_integer_type(left, right) {
            return Some(common);
        }

        let left = self.resolve_alias_type(left);
        let right = self.resolve_alias_type(right);

        match (
            self.layer.table().kind(left),
            self.layer.table().kind(right),
        ) {
            (Some(TypeKind::Error), _) => Some(left),
            (_, Some(TypeKind::Error)) => Some(right),
            (
                Some(TypeKind::Primitive(left_primitive)),
                Some(TypeKind::Primitive(right_primitive)),
            ) => self
                .common_float_numeric_primitive(*left_primitive, *right_primitive)
                .map(|primitive| self.layer.table().primitive(primitive)),
            _ => None,
        }
    }

    fn common_integer_type(&self, left: TypeId, right: TypeId) -> Option<TypeId> {
        let left = self.resolve_alias_type(left);
        let right = self.resolve_alias_type(right);

        match (
            self.layer.table().kind(left),
            self.layer.table().kind(right),
        ) {
            (Some(TypeKind::Error), _) => Some(left),
            (_, Some(TypeKind::Error)) => Some(right),
            (
                Some(TypeKind::Primitive(left_primitive)),
                Some(TypeKind::Primitive(right_primitive)),
            ) => self
                .common_integer_primitive(*left_primitive, *right_primitive)
                .map(|primitive| self.layer.table().primitive(primitive)),
            _ => None,
        }
    }

    pub(super) fn is_numeric_type(&self, ty: TypeId) -> bool {
        match self.layer.table().kind(ty) {
            Some(TypeKind::Primitive(primitive)) => self.is_numeric_primitive(*primitive),
            Some(TypeKind::GenericParameter { symbol }) => self
                .generic_parameter_bound_type(*symbol)
                .is_some_and(|bound| self.is_numeric_bound(bound)),
            Some(TypeKind::Error) => true,
            _ => false,
        }
    }

    fn is_numeric_bound(&self, ty: TypeId) -> bool {
        match self.layer.table().kind(ty) {
            Some(TypeKind::Primitive(primitive)) => self.is_numeric_primitive(*primitive),
            Some(TypeKind::Union { members }) => {
                members.iter().all(|member| self.is_numeric_bound(*member))
            }
            Some(TypeKind::GenericParameter { symbol }) => self
                .generic_parameter_bound_type(*symbol)
                .is_some_and(|bound| self.is_numeric_bound(bound)),
            Some(TypeKind::Error) => true,
            _ => false,
        }
    }

    pub(super) fn is_integer_type(&self, ty: TypeId) -> bool {
        match self.layer.table().kind(ty) {
            Some(TypeKind::Primitive(primitive)) => self.is_integer_primitive(*primitive),
            Some(TypeKind::Error) => true,
            _ => false,
        }
    }

    fn is_numeric_primitive(&self, primitive: PrimitiveType) -> bool {
        matches!(
            primitive,
            PrimitiveType::Int8
                | PrimitiveType::Int16
                | PrimitiveType::Int32
                | PrimitiveType::Int64
                | PrimitiveType::Uint8
                | PrimitiveType::Uint16
                | PrimitiveType::Uint32
                | PrimitiveType::Uint64
                | PrimitiveType::Float16
                | PrimitiveType::Float32
                | PrimitiveType::Float64
        )
    }

    fn is_integer_primitive(&self, primitive: PrimitiveType) -> bool {
        matches!(
            primitive,
            PrimitiveType::Int8
                | PrimitiveType::Int16
                | PrimitiveType::Int32
                | PrimitiveType::Int64
                | PrimitiveType::Uint8
                | PrimitiveType::Uint16
                | PrimitiveType::Uint32
                | PrimitiveType::Uint64
        )
    }

    fn common_integer_primitive(
        &self,
        left: PrimitiveType,
        right: PrimitiveType,
    ) -> Option<PrimitiveType> {
        if left.is_int() && right.is_int() {
            return Some(if self.integer_rank(left) >= self.integer_rank(right) {
                left
            } else {
                right
            });
        }

        if left.is_uint() && right.is_uint() {
            return Some(if self.integer_rank(left) >= self.integer_rank(right) {
                left
            } else {
                right
            });
        }

        None
    }

    fn common_float_numeric_primitive(
        &self,
        left: PrimitiveType,
        right: PrimitiveType,
    ) -> Option<PrimitiveType> {
        if left.is_float() && right.is_float() {
            return Some(if self.float_rank(left) >= self.float_rank(right) {
                left
            } else {
                right
            });
        }

        None
    }

    fn integer_rank(&self, primitive: PrimitiveType) -> u8 {
        match primitive {
            PrimitiveType::Int8 | PrimitiveType::Uint8 => 8,
            PrimitiveType::Int16 | PrimitiveType::Uint16 => 16,
            PrimitiveType::Int32 | PrimitiveType::Uint32 => 32,
            PrimitiveType::Int64 | PrimitiveType::Uint64 => 64,
            _ => 0,
        }
    }

    fn float_rank(&self, primitive: PrimitiveType) -> u8 {
        match primitive {
            PrimitiveType::Float16 => 16,
            PrimitiveType::Float32 => 32,
            PrimitiveType::Float64 => 64,
            _ => 0,
        }
    }
}
