use super::function::FunctionBuilder;
use galfus_core::NodeId;
use galfus_frontend::SyntaxNodeKind;
use galfus_ir::mir::*;

impl<'b, 'a> FunctionBuilder<'b, 'a> {
    pub(super) fn lower_expression(&mut self, expression_id: NodeId) -> Operand {
        let syntax = self.builder.graph.syntax();
        let Some(node) = syntax.node(expression_id) else {
            return Operand::Constant(Constant::Null);
        };

        match node.kind() {
            SyntaxNodeKind::IntegerLiteral
            | SyntaxNodeKind::FloatLiteral
            | SyntaxNodeKind::StringLiteral
            | SyntaxNodeKind::BoolLiteral
            | SyntaxNodeKind::NullLiteral => self.lower_primitive_literal(expression_id),
            SyntaxNodeKind::RangeExpression => self.lower_range_expression(expression_id),
            SyntaxNodeKind::TypeofExpression => self.lower_typeof_expression(expression_id),
            SyntaxNodeKind::NameExpression | SyntaxNodeKind::Identifier => {
                self.lower_name_expression(expression_id)
            }
            SyntaxNodeKind::BinaryExpression => self.lower_binary_expression(expression_id),
            SyntaxNodeKind::UnaryExpression => self.lower_unary_expression(expression_id),
            SyntaxNodeKind::CastExpression => self.lower_cast_expression(expression_id),
            SyntaxNodeKind::CopyExpression => self.lower_copy_expression(expression_id),
            SyntaxNodeKind::GroupedExpression => self.lower_grouped_expression(expression_id),
            SyntaxNodeKind::CallExpression => {
                let target = node.child(0).unwrap();
                let arguments = node.child(1).unwrap();
                let arguments = self.lower_call_arguments(target, arguments);
                self.lower_call_dispatch(
                    expression_id,
                    target,
                    arguments.arguments,
                    arguments.argument_types,
                    arguments.anchored_receiver,
                )
            }
            SyntaxNodeKind::PathExpression => self.lower_path_expression(expression_id),
            SyntaxNodeKind::StructLiteral | SyntaxNodeKind::InferredStructLiteral => {
                self.lower_struct_literal(expression_id, node)
            }
            SyntaxNodeKind::ArrayLiteral => self.lower_array_literal(expression_id, node),
            SyntaxNodeKind::TupleExpression => self.lower_tuple_literal(expression_id, node),
            SyntaxNodeKind::MemberExpression | SyntaxNodeKind::NullSafeMemberExpression => {
                self.lower_member_expression(expression_id)
            }
            SyntaxNodeKind::IndexExpression => self.lower_index_expression(expression_id),
            SyntaxNodeKind::MatchExpression | SyntaxNodeKind::InstanceofExpression => {
                self.lower_match_expression(expression_id)
            }
            SyntaxNodeKind::NewArrayExpression => {
                self.lower_new_array_expression(expression_id, node, &[])
            }
            kind if kind.is_function_expression() => self.lower_function_expression(expression_id),
            SyntaxNodeKind::AwaitExpression => self.lower_await_expression(expression_id),
            SyntaxNodeKind::AwaitAllExpression => self.lower_await_all_expression(expression_id),
            SyntaxNodeKind::AwaitRaceExpression => self.lower_await_race_expression(expression_id),
            _ => Operand::Constant(Constant::Null),
        }
    }
}
