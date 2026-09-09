use super::function::FunctionBuilder;
use super::function_helpers::parse_int;
use galfus_core::{NodeId, TypeId};
use galfus_frontend::{RangeDesugarTarget, SyntaxNodeKind, TypeKind};
use galfus_ir::mir::*;

impl<'b, 'a> FunctionBuilder<'b, 'a> {
    pub(super) fn lower_primitive_literal(&mut self, expression_id: NodeId) -> Operand {
        let kind = self
            .builder
            .graph
            .syntax()
            .node(expression_id)
            .map(|node| node.kind());
        match kind {
            Some(SyntaxNodeKind::IntegerLiteral) => self.lower_integer_literal(expression_id),
            Some(SyntaxNodeKind::FloatLiteral) => self.lower_float_literal(expression_id),
            Some(SyntaxNodeKind::StringLiteral) => {
                let text = self.builder.node_text(expression_id);
                let value = if (text.starts_with('"') && text.ends_with('"'))
                    || (text.starts_with('\'') && text.ends_with('\''))
                {
                    &text[1..text.len() - 1]
                } else {
                    text
                };
                Operand::Constant(Constant::String(unescape_string(value)))
            }
            Some(SyntaxNodeKind::BoolLiteral) => Operand::Constant(Constant::Bool(
                self.builder.node_text(expression_id) == "true",
            )),
            _ => Operand::Constant(Constant::Null),
        }
    }

    pub(super) fn lower_range_expression(&mut self, expression_id: NodeId) -> Operand {
        let syntax = self.builder.graph.syntax();
        let Some(target) = self.builder.type_result.range_desugar(expression_id) else {
            return Operand::Constant(Constant::Null);
        };
        let Some(start) = syntax.child(expression_id, 0) else {
            return Operand::Constant(Constant::Null);
        };
        let Some(end_or_count) = syntax.child(expression_id, 2) else {
            return Operand::Constant(Constant::Null);
        };
        let start_operand = self.lower_expression(start);
        let end_or_count_operand = self.lower_expression(end_or_count);
        let range_type = self
            .node_type(expression_id)
            .unwrap_or_else(|| TypeId::new(0));
        let item_type = self.node_type(start).unwrap_or_else(|| TypeId::new(0));
        let (function_name, arguments, concrete_types) = match target {
            RangeDesugarTarget::Exclusive => (
                "range",
                vec![start_operand, end_or_count_operand],
                Vec::new(),
            ),
            RangeDesugarTarget::Stepped => {
                let step = syntax
                    .child(expression_id, 3)
                    .and_then(|step| syntax.first_child(step))
                    .map(|step| self.lower_expression(step))
                    .unwrap_or(Operand::Constant(Constant::Int32(1)));
                (
                    "rangeSteps",
                    vec![start_operand, end_or_count_operand, step],
                    vec![item_type],
                )
            }
        };
        let Some(context) = self.builder.workspace_ctx.as_deref_mut() else {
            return Operand::Constant(Constant::Null);
        };
        let Some(module_id) = self.builder.workspace_module_id else {
            return Operand::Constant(Constant::Null);
        };
        let Some(function) = context.specialize_builtin_function(
            module_id,
            expression_id,
            "std/iterable",
            function_name,
            concrete_types,
        ) else {
            return Operand::Constant(Constant::Null);
        };
        let destination = self.declare_local(None, range_type);
        self.current_instructions.push((
            Instruction::Call {
                func: function,
                args: arguments,
                destination,
                is_external: false,
            },
            None,
        ));
        Operand::Local(destination)
    }

    fn lower_integer_literal(&self, expression_id: NodeId) -> Operand {
        let value = parse_int(self.builder.node_text(expression_id)).unwrap_or(0) as i128;
        let type_id = self
            .node_type(expression_id)
            .map(|ty| self.builder.resolve_alias_type(ty));
        let constant =
            match type_id.and_then(|ty| self.builder.type_result.layer().table().kind(ty)) {
                Some(TypeKind::Primitive(galfus_frontend::PrimitiveType::Int8)) => {
                    Constant::Int8(value as i8)
                }
                Some(TypeKind::Primitive(galfus_frontend::PrimitiveType::Int16)) => {
                    Constant::Int16(value as i16)
                }
                Some(TypeKind::Primitive(galfus_frontend::PrimitiveType::Int64)) => {
                    Constant::Int64(value as i64)
                }
                Some(TypeKind::Primitive(galfus_frontend::PrimitiveType::Uint8)) => {
                    Constant::Uint8(value as u8)
                }
                Some(TypeKind::Primitive(galfus_frontend::PrimitiveType::Uint16)) => {
                    Constant::Uint16(value as u16)
                }
                Some(TypeKind::Primitive(galfus_frontend::PrimitiveType::Uint32)) => {
                    Constant::Uint32(value as u32)
                }
                Some(TypeKind::Primitive(galfus_frontend::PrimitiveType::Uint64)) => {
                    Constant::Uint64(value as u64)
                }
                _ => Constant::Int32(value as i32),
            };
        Operand::Constant(constant)
    }

    fn lower_float_literal(&self, expression_id: NodeId) -> Operand {
        let value = self
            .builder
            .node_text(expression_id)
            .parse::<f64>()
            .unwrap_or(0.0);
        let type_id = self
            .node_type(expression_id)
            .map(|ty| self.builder.resolve_alias_type(ty));
        if matches!(
            type_id.and_then(|ty| self.builder.type_result.layer().table().kind(ty)),
            Some(TypeKind::Primitive(galfus_frontend::PrimitiveType::Float64))
        ) {
            Operand::Constant(Constant::Float64(galfus_core::normalize_f64(value)))
        } else {
            Operand::Constant(Constant::Float32(galfus_core::normalize_f32(value as f32)))
        }
    }
}

fn unescape_string(value: &str) -> String {
    let mut result = String::new();
    let mut characters = value.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            result.push(character);
            continue;
        }
        match characters.next() {
            Some('n') => result.push('\n'),
            Some('t') => result.push('\t'),
            Some('r') => result.push('\r'),
            Some('"') => result.push('"'),
            Some('\'') => result.push('\''),
            Some('\\') => result.push('\\'),
            Some(other) => {
                result.push('\\');
                result.push(other);
            }
            None => result.push('\\'),
        }
    }
    result
}
