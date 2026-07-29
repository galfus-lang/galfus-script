use super::function::FunctionBuilder;
use super::*;
use galfus_frontend::{AssignmentOperatorKind, BinaryOperatorKind, UnaryOperatorKind};

impl<'b, 'a> FunctionBuilder<'b, 'a> {
    pub(super) fn lower_binary_op(&self, op_node_id: NodeId) -> MirBinaryOp {
        let syntax = self.builder.graph.syntax();
        if let Some(op) = syntax
            .node(op_node_id)
            .and_then(|node| node.binary_operator())
        {
            return match op {
                BinaryOperatorKind::Add => MirBinaryOp::Add,
                BinaryOperatorKind::Subtract => MirBinaryOp::Subtract,
                BinaryOperatorKind::Multiply => MirBinaryOp::Multiply,
                BinaryOperatorKind::Divide => MirBinaryOp::Divide,
                BinaryOperatorKind::Remainder => MirBinaryOp::Remainder,
                BinaryOperatorKind::Power => MirBinaryOp::Power,
                BinaryOperatorKind::ShiftLeft => MirBinaryOp::ShiftLeft,
                BinaryOperatorKind::ShiftRight => MirBinaryOp::ShiftRight,
                BinaryOperatorKind::BitwiseAnd => MirBinaryOp::BitwiseAnd,
                BinaryOperatorKind::BitwiseOr => MirBinaryOp::BitwiseOr,
                BinaryOperatorKind::BitwiseXor => MirBinaryOp::BitwiseXor,
                BinaryOperatorKind::Equal => MirBinaryOp::Equal,
                BinaryOperatorKind::NotEqual => MirBinaryOp::NotEqual,
                BinaryOperatorKind::Less => MirBinaryOp::Less,
                BinaryOperatorKind::LessEqual => MirBinaryOp::LessEqual,
                BinaryOperatorKind::Greater => MirBinaryOp::Greater,
                BinaryOperatorKind::GreaterEqual => MirBinaryOp::GreaterEqual,
                BinaryOperatorKind::LogicalAnd => MirBinaryOp::LogicalAnd,
                BinaryOperatorKind::LogicalOr => MirBinaryOp::LogicalOr,
                BinaryOperatorKind::NullFallback => MirBinaryOp::NullFallback,
            };
        }
        MirBinaryOp::Add
    }

    pub(super) fn lower_unary_op(&self, op_node_id: NodeId) -> MirUnaryOp {
        let syntax = self.builder.graph.syntax();
        if let Some(op) = syntax
            .node(op_node_id)
            .and_then(|node| node.unary_operator())
        {
            return match op {
                UnaryOperatorKind::Negate => MirUnaryOp::Negate,
                UnaryOperatorKind::Not => MirUnaryOp::Not,
                UnaryOperatorKind::BitwiseNot => MirUnaryOp::BitwiseNot,
            };
        }
        MirUnaryOp::Negate
    }

    pub(super) fn lower_assignment_binary_op(&self, op_node_id: NodeId) -> Option<MirBinaryOp> {
        let syntax = self.builder.graph.syntax();
        let operator = syntax.node(op_node_id)?.assignment_operator()?;

        Some(match operator {
            AssignmentOperatorKind::Assign => return None,
            AssignmentOperatorKind::AddAssign => MirBinaryOp::Add,
            AssignmentOperatorKind::SubtractAssign => MirBinaryOp::Subtract,
            AssignmentOperatorKind::MultiplyAssign => MirBinaryOp::Multiply,
            AssignmentOperatorKind::DivideAssign => MirBinaryOp::Divide,
            AssignmentOperatorKind::RemainderAssign => MirBinaryOp::Remainder,
            AssignmentOperatorKind::PowerAssign => MirBinaryOp::Power,
            AssignmentOperatorKind::BitwiseAndAssign => MirBinaryOp::BitwiseAnd,
            AssignmentOperatorKind::BitwiseOrAssign => MirBinaryOp::BitwiseOr,
            AssignmentOperatorKind::BitwiseXorAssign => MirBinaryOp::BitwiseXor,
            AssignmentOperatorKind::ShiftLeftAssign => MirBinaryOp::ShiftLeft,
            AssignmentOperatorKind::ShiftRightAssign => MirBinaryOp::ShiftRight,
            AssignmentOperatorKind::NullFallbackAssign => MirBinaryOp::NullFallback,
        })
    }

    pub(super) fn struct_symbol_for_type(&self, ty: TypeId) -> Option<SymbolId> {
        self.builder.struct_symbol_for_type(ty)
    }

    pub(super) fn find_struct_field_default_expr(
        &self,
        struct_symbol: SymbolId,
        field_name: &str,
    ) -> Option<NodeId> {
        self.builder
            .find_struct_field_default_expr(struct_symbol, field_name)
    }

    pub(super) fn get_struct_fields(&self, struct_symbol: SymbolId) -> Vec<(String, TypeId)> {
        self.builder.get_struct_fields(struct_symbol)
    }

    pub(super) fn find_tuple_type(&self, elements: &[TypeId]) -> TypeId {
        self.builder.find_tuple_type(elements)
    }
}

pub(super) fn parse_int(text: &str) -> Option<i64> {
    let clean = text.trim();
    if clean.starts_with("0x") || clean.starts_with("0X") {
        i64::from_str_radix(&clean[2..].replace('_', ""), 16).ok()
    } else if clean.starts_with("0o") || clean.starts_with("0O") {
        i64::from_str_radix(&clean[2..].replace('_', ""), 8).ok()
    } else if clean.starts_with("0b") || clean.starts_with("0B") {
        i64::from_str_radix(&clean[2..].replace('_', ""), 2).ok()
    } else {
        clean.replace('_', "").parse::<i64>().ok()
    }
}
