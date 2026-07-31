use super::DeclarationTypeChecker;
use crate::{
    PrimitiveType, RangeDesugarTarget, RangeOperatorKind, SyntaxNodeKind, TypeDiagnosticCode,
    TypeKind, UnaryOperatorKind,
};
use galfus_core::{Diagnostic, NodeId, TypeId};

#[derive(Debug, Clone, Copy, PartialEq)]
enum RangeLiteralValue {
    Integer(i64),
    Float(f64),
}

impl RangeLiteralValue {
    fn is_zero(self) -> bool {
        match self {
            Self::Integer(value) => value == 0,
            Self::Float(value) => value == 0.0,
        }
    }
}

impl<'a> DeclarationTypeChecker<'a> {
    pub(super) fn infer_range_expression_type(&mut self, node: NodeId) -> Option<TypeId> {
        let start = self.graph.syntax().child(node, 0)?;
        let operator = self.graph.syntax().child(node, 1)?;
        let end_or_count = self.graph.syntax().child(node, 2)?;

        let operator_kind = self
            .graph
            .syntax()
            .node(operator)
            .and_then(|node| node.range_operator())?;

        match operator_kind {
            RangeOperatorKind::Exclusive => {
                self.range_desugars
                    .insert(node, RangeDesugarTarget::Exclusive);
                self.infer_exclusive_range_type(node, start, end_or_count)
            }
            RangeOperatorKind::Quantity => {
                self.range_desugars
                    .insert(node, RangeDesugarTarget::Stepped);
                self.infer_quantity_range_type(node, start, end_or_count)
            }
        }
    }

    fn infer_exclusive_range_type(
        &mut self,
        range: NodeId,
        start: NodeId,
        end: NodeId,
    ) -> Option<TypeId> {
        let i32 = self.layer.table().primitive(PrimitiveType::Int32);
        self.bind_range_operand_type(start, i32);
        self.bind_range_operand_type(end, i32);

        let start_value = self.integer_range_literal(start, "integer literal");
        let end_value = self.integer_range_literal(end, "integer literal");

        if let (Some(start_value), Some(end_value)) = (start_value, end_value) {
            match end_value.checked_sub(start_value) {
                Some(0) => self.report_invalid_range_value(range, "range must not be empty"),
                Some(_) => {}
                None => self.report_invalid_range_value(range, "range difference overflows i32"),
            }
        }

        Some(self.layer.table_mut().intern_range(i32))
    }

    fn infer_quantity_range_type(
        &mut self,
        range: NodeId,
        start: NodeId,
        count: NodeId,
    ) -> Option<TypeId> {
        let start_value = self.range_literal(start, "integer or float literal");
        let count_value = self.integer_range_literal(count, "integer literal count");

        let item_type = match start_value {
            Some(RangeLiteralValue::Integer(_)) => {
                self.layer.table().primitive(PrimitiveType::Int32)
            }
            Some(RangeLiteralValue::Float(_)) => {
                self.layer.table().primitive(PrimitiveType::Float32)
            }
            None => self.layer.table_mut().error(),
        };

        self.bind_range_operand_type(start, item_type);
        self.bind_range_operand_type(count, self.layer.table().primitive(PrimitiveType::Int32));

        if let Some(count_value) = count_value
            && count_value <= 0
        {
            self.report_invalid_range_value(count, "range count must be greater than zero");
        }

        if let Some(step) = self.graph.syntax().child(range, 3) {
            self.validate_range_step(range, item_type, start_value, count_value, step);
        } else if let (Some(start_value), Some(count_value)) = (start_value, count_value) {
            self.validate_range_bounds(range, start_value, count_value, None);
        }

        Some(self.layer.table_mut().intern_range(item_type))
    }

    fn validate_range_step(
        &mut self,
        range: NodeId,
        item_type: TypeId,
        start_value: Option<RangeLiteralValue>,
        count_value: Option<i64>,
        step: NodeId,
    ) {
        let Some(step_expression) = self.graph.syntax().child(step, 0) else {
            return;
        };

        self.bind_range_operand_type(step_expression, item_type);

        let step_value = self.range_literal(step_expression, "integer or float literal step");
        if let Some(step_value) = step_value
            && step_value.is_zero()
        {
            self.report_invalid_range_value(step, "range step must not be zero");
        }

        if !self.range_literal_matches_type(step_value, item_type) {
            self.report_invalid_range_value(
                step,
                "range step must have the same numeric family as start",
            );
            return;
        }

        if let (Some(start_value), Some(count_value)) = (start_value, count_value) {
            self.validate_range_bounds(range, start_value, count_value, step_value);
        }
    }

    fn validate_range_bounds(
        &mut self,
        range: NodeId,
        start_value: RangeLiteralValue,
        count_value: i64,
        step_value: Option<RangeLiteralValue>,
    ) {
        if count_value <= 0 {
            return;
        }

        match (start_value, step_value) {
            (RangeLiteralValue::Integer(start), Some(RangeLiteralValue::Integer(step))) => {
                let Some(offset) = (count_value - 1).checked_mul(step) else {
                    self.report_invalid_range_value(range, "range end overflows i32");
                    return;
                };

                match start.checked_add(offset) {
                    Some(end) if i32::try_from(end).is_ok() => {}
                    _ => self.report_invalid_range_value(range, "range end overflows i32"),
                }
            }
            (RangeLiteralValue::Integer(start), None) => match start.checked_add(count_value - 1) {
                Some(end) if i32::try_from(end).is_ok() => {}
                _ => self.report_invalid_range_value(range, "range end overflows i32"),
            },
            (RangeLiteralValue::Float(start), Some(RangeLiteralValue::Float(step))) => {
                let end = start + ((count_value - 1) as f64 * step);
                if !end.is_finite() {
                    self.report_invalid_range_value(range, "range end must be finite");
                }
            }
            (RangeLiteralValue::Float(start), None) => {
                let end = start + (count_value - 1) as f64;
                if !end.is_finite() {
                    self.report_invalid_range_value(range, "range end must be finite");
                }
            }
            _ => {}
        }
    }

    fn integer_range_literal(&mut self, node: NodeId, expected: &str) -> Option<i64> {
        match self.range_literal(node, expected) {
            Some(RangeLiteralValue::Integer(value)) => Some(value),
            Some(RangeLiteralValue::Float(_)) => {
                let f32 = self.layer.table().primitive(PrimitiveType::Float32);
                self.report_invalid_range_operand_type(node, expected, f32);
                None
            }
            None => None,
        }
    }

    fn range_literal(&mut self, node: NodeId, expected: &str) -> Option<RangeLiteralValue> {
        let syntax_node = self.graph.syntax().node(node)?;

        match syntax_node.kind() {
            SyntaxNodeKind::IntegerLiteral => match self.parse_integer_literal_value(node) {
                Some(value) if i32::try_from(value).is_ok() => {
                    Some(RangeLiteralValue::Integer(value))
                }
                None => {
                    self.report_invalid_range_value(node, "range integer literal must fit i32");
                    None
                }
                Some(_) => {
                    self.report_invalid_range_value(node, "range integer literal must fit i32");
                    None
                }
            },
            SyntaxNodeKind::FloatLiteral => match self.parse_float_literal_value(node) {
                Some(value) if value.is_finite() => Some(RangeLiteralValue::Float(value)),
                _ => {
                    self.report_invalid_range_value(node, "range float literal must be finite");
                    None
                }
            },
            SyntaxNodeKind::UnaryExpression => self.signed_range_literal(node, expected),
            _ => {
                let error = self.layer.table_mut().error();
                self.report_invalid_range_operand_type(node, expected, error);
                None
            }
        }
    }

    fn signed_range_literal(&mut self, node: NodeId, expected: &str) -> Option<RangeLiteralValue> {
        let operator = self.graph.syntax().child(node, 0)?;
        let operand = self.graph.syntax().child(node, 1)?;
        let is_negative = self
            .graph
            .syntax()
            .node(operator)
            .and_then(|node| node.unary_operator())
            == Some(UnaryOperatorKind::Negate);

        if !is_negative {
            let error = self.layer.table_mut().error();
            self.report_invalid_range_operand_type(node, expected, error);
            return None;
        }

        match self.range_literal(operand, expected)? {
            RangeLiteralValue::Integer(value) => Some(RangeLiteralValue::Integer(-value)),
            RangeLiteralValue::Float(value) => Some(RangeLiteralValue::Float(-value)),
        }
    }

    fn bind_range_operand_type(&mut self, node: NodeId, ty: TypeId) {
        let Some(syntax_node) = self.graph.syntax().node(node) else {
            return;
        };

        match syntax_node.kind() {
            SyntaxNodeKind::IntegerLiteral | SyntaxNodeKind::FloatLiteral => {
                self.layer.bind_node_type(node, ty);
            }
            SyntaxNodeKind::UnaryExpression => {
                if let Some(operand) = self.graph.syntax().child(node, 1) {
                    self.bind_range_operand_type(operand, ty);
                }
                self.layer.bind_node_type(node, ty);
            }
            _ => {}
        }
    }

    fn range_literal_matches_type(&self, literal: Option<RangeLiteralValue>, ty: TypeId) -> bool {
        let ty = self.resolve_alias_type(ty);
        matches!(
            (literal, self.layer.table().kind(ty)),
            (
                Some(RangeLiteralValue::Integer(_)),
                Some(TypeKind::Primitive(PrimitiveType::Int32)),
            ) | (
                Some(RangeLiteralValue::Float(_)),
                Some(TypeKind::Primitive(PrimitiveType::Float32)),
            ) | (None, Some(TypeKind::Error))
        )
    }

    fn parse_integer_literal_value(&self, node: NodeId) -> Option<i64> {
        let text = self.node_text(node);
        let text = text.replace('_', "");

        let (negative, digits) = text
            .strip_prefix('-')
            .map(|digits| (true, digits))
            .unwrap_or((false, text.as_str()));

        let (radix, digits) = if let Some(digits) = digits.strip_prefix("0x") {
            (16, digits)
        } else if let Some(digits) = digits.strip_prefix("0X") {
            (16, digits)
        } else if let Some(digits) = digits.strip_prefix("0b") {
            (2, digits)
        } else if let Some(digits) = digits.strip_prefix("0B") {
            (2, digits)
        } else if let Some(digits) = digits.strip_prefix("0o") {
            (8, digits)
        } else if let Some(digits) = digits.strip_prefix("0O") {
            (8, digits)
        } else {
            (10, digits)
        };

        let value = i64::from_str_radix(digits, radix).ok()?;
        Some(if negative { -value } else { value })
    }

    fn parse_float_literal_value(&self, node: NodeId) -> Option<f64> {
        let text = self.node_text(node).replace('_', "");
        text.parse::<f64>().ok()
    }

    fn report_invalid_range_value(&mut self, node: NodeId, message: &str) {
        let span = self
            .graph
            .syntax()
            .node(node)
            .map(|node| node.span())
            .unwrap_or_else(|| self.source.span());

        self.diagnostics.push(Diagnostic::error_with_message(
            TypeDiagnosticCode::InvalidRangeOperandType,
            message.to_string(),
            span,
        ));
    }
}
