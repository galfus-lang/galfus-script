use std::collections;

use super::function::FunctionBuilder;
use galfus_core::{NodeId, TypeId};
use galfus_frontend::{ImportedStructFieldDefault, SyntaxNode, SyntaxNodeKind, TypeKind};
use galfus_ir::mir::*;

impl<'b, 'a> FunctionBuilder<'b, 'a> {
    pub(super) fn lower_struct_literal(&mut self, expr_id: NodeId, node: &SyntaxNode) -> Operand {
        let syntax = self.builder.graph.syntax();
        let struct_type = self
            .builder
            .type_result
            .layer()
            .node_type(expr_id)
            .unwrap_or_else(|| TypeId::new(0));

        if let Some(struct_symbol) = self.struct_symbol_for_type(struct_type) {
            let fields_list_node = if node.kind() == SyntaxNodeKind::StructLiteral {
                node.children().last().copied()
            } else {
                node.child(0)
            };

            let mut field_values = collections::HashMap::new();
            let mut spread_operands = Vec::new();

            let field_children = fields_list_node
                .and_then(|list_id| syntax.node(list_id))
                .map(|n| n.children())
                .unwrap_or(&[]);

            for &child_id in field_children {
                if let Some(child_node) = syntax.node(child_id) {
                    match child_node.kind() {
                        SyntaxNodeKind::StructLiteralField => {
                            let name_ident = syntax
                                .first_child_of_kind(child_id, SyntaxNodeKind::Identifier)
                                .unwrap();
                            let name = self.builder.node_text(name_ident).to_string();
                            let val_expr = child_node.child(1).unwrap();
                            let op = self.lower_expression(val_expr);
                            field_values.insert(name, op);
                        }
                        SyntaxNodeKind::StructLiteralFieldShorthand => {
                            let name_ident = child_node.first_child().unwrap();
                            let name = self.builder.node_text(name_ident).to_string();
                            let op = self.lower_expression(name_ident);
                            field_values.insert(name, op);
                        }
                        SyntaxNodeKind::SpreadStructLiteralField => {
                            let spread_expr = child_node.child(0).unwrap();
                            let op = self.lower_expression(spread_expr);
                            spread_operands.push((spread_expr, op));
                        }
                        _ => {}
                    }
                }
            }

            let struct_fields_decl = self.get_struct_fields(struct_symbol);
            let mut fields = Vec::new();

            for (field_name, field_ty) in struct_fields_decl {
                if let Some(op) = field_values.remove(&field_name) {
                    fields.push(op);
                } else {
                    // Try to get from spread
                    let mut found_in_spread = false;
                    for &(spread_expr, ref spread_op) in &spread_operands {
                        let spread_ty = self
                            .builder
                            .type_result
                            .layer()
                            .node_type(spread_expr)
                            .unwrap_or_else(|| TypeId::new(0));
                        if let Some(spread_sym) = self.struct_symbol_for_type(spread_ty) {
                            let spread_fields = self.get_struct_fields(spread_sym);
                            if spread_fields.iter().any(|(n, _)| *n == field_name) {
                                let temp_id = self.declare_local(None, field_ty);
                                self.current_instructions.push((
                                    Instruction::Assign(
                                        temp_id,
                                        RValue::MemberAccess(spread_op.clone(), field_name.clone()),
                                    ),
                                    None,
                                ));
                                fields.push(Operand::Local(temp_id));
                                found_in_spread = true;
                                break;
                            }
                        }
                    }

                    if !found_in_spread {
                        if let Some(default) =
                            self.imported_struct_field_default(struct_symbol, &field_name)
                        {
                            match default {
                                ImportedStructFieldDefault::Null => {
                                    fields.push(Operand::Constant(Constant::Null));
                                }
                                ImportedStructFieldDefault::EmptyArray => {
                                    let temp_id = self.declare_local(None, field_ty);
                                    self.current_instructions.push((
                                        Instruction::Assign(
                                            temp_id,
                                            RValue::NewArray(field_ty, Vec::new()),
                                        ),
                                        None,
                                    ));
                                    fields.push(Operand::Local(temp_id));
                                }
                                ImportedStructFieldDefault::Integer(value) => {
                                    fields.push(Operand::Constant(Constant::Int32(value as i32)));
                                }
                            }
                        } else if let Some(default_expr) =
                            self.find_struct_field_default_expr(struct_symbol, &field_name)
                        {
                            let op = self.lower_expression(default_expr);
                            fields.push(op);
                        } else {
                            fields.push(Operand::Constant(Constant::Null));
                        }
                    }
                }
            }

            let temp_id = self.declare_local(None, struct_type);
            self.current_instructions.push((
                Instruction::Assign(
                    temp_id,
                    RValue::NewStruct {
                        struct_type,
                        fields,
                    },
                ),
                None,
            ));
            Operand::Local(temp_id)
        } else {
            Operand::Constant(Constant::Null)
        }
    }

    pub(super) fn lower_array_literal(&mut self, expr_id: NodeId, node: &SyntaxNode) -> Operand {
        let syntax = self.builder.graph.syntax();
        let array_type = self
            .builder
            .type_result
            .layer()
            .node_type(expr_id)
            .unwrap_or_else(|| TypeId::new(0));

        let resolved_array_type = self.builder.resolve_alias_type(array_type);
        let expected_element_type = match self
            .builder
            .type_result
            .layer()
            .table()
            .kind(resolved_array_type)
        {
            Some(TypeKind::Array { element }) => Some(*element),
            _ => None,
        };

        let has_spread = node.children().iter().any(|&child_id| {
            syntax
                .node(child_id)
                .is_some_and(|child_node| child_node.kind() == SyntaxNodeKind::SpreadArrayElement)
        });

        if has_spread {
            let mut elements = Vec::new();
            for &child_id in node.children() {
                if let Some(child_node) = syntax.node(child_id) {
                    match child_node.kind() {
                        SyntaxNodeKind::ArrayElement => {
                            let val_expr = child_node.child(0).unwrap();
                            let val_expr_ty = self
                                .builder
                                .type_result
                                .layer()
                                .node_type(val_expr)
                                .unwrap_or_else(|| TypeId::new(0));
                            let op = self.lower_expression(val_expr);
                            let casted_op = if let Some(elem_ty) = expected_element_type {
                                self.insert_cast_if_needed(op, val_expr_ty, elem_ty)
                            } else {
                                op
                            };
                            elements.push(ArrayLiteralElement::Single(casted_op));
                        }
                        SyntaxNodeKind::SpreadArrayElement => {
                            let spread_expr = child_node.child(0).unwrap();
                            let op = self.lower_expression(spread_expr);
                            elements.push(ArrayLiteralElement::Spread(op));
                        }
                        _ => {
                            let val_expr_ty = self
                                .builder
                                .type_result
                                .layer()
                                .node_type(child_id)
                                .unwrap_or_else(|| TypeId::new(0));
                            let op = self.lower_expression(child_id);
                            let casted_op = if let Some(elem_ty) = expected_element_type {
                                self.insert_cast_if_needed(op, val_expr_ty, elem_ty)
                            } else {
                                op
                            };
                            elements.push(ArrayLiteralElement::Single(casted_op));
                        }
                    }
                }
            }

            let temp_id = self.declare_local(None, array_type);
            self.current_instructions.push((
                Instruction::Assign(temp_id, RValue::NewArrayDynamic(array_type, elements)),
                None,
            ));
            Operand::Local(temp_id)
        } else {
            let mut elements = Vec::new();
            for &child_id in node.children() {
                if let Some(child_node) = syntax.node(child_id) {
                    match child_node.kind() {
                        SyntaxNodeKind::ArrayElement => {
                            let val_expr = child_node.child(0).unwrap();
                            let val_expr_ty = self
                                .builder
                                .type_result
                                .layer()
                                .node_type(val_expr)
                                .unwrap_or_else(|| TypeId::new(0));
                            let op = self.lower_expression(val_expr);
                            let casted_op = if let Some(elem_ty) = expected_element_type {
                                self.insert_cast_if_needed(op, val_expr_ty, elem_ty)
                            } else {
                                op
                            };
                            elements.push(casted_op);
                        }
                        _ => {
                            let val_expr_ty = self
                                .builder
                                .type_result
                                .layer()
                                .node_type(child_id)
                                .unwrap_or_else(|| TypeId::new(0));
                            let op = self.lower_expression(child_id);
                            let casted_op = if let Some(elem_ty) = expected_element_type {
                                self.insert_cast_if_needed(op, val_expr_ty, elem_ty)
                            } else {
                                op
                            };
                            elements.push(casted_op);
                        }
                    }
                }
            }

            let temp_id = self.declare_local(None, array_type);
            self.current_instructions.push((
                Instruction::Assign(temp_id, RValue::NewArray(array_type, elements)),
                None,
            ));
            Operand::Local(temp_id)
        }
    }

    pub(super) fn lower_tuple_literal(&mut self, expr_id: NodeId, node: &SyntaxNode) -> Operand {
        let mut elements = Vec::new();
        let mut element_types = Vec::new();
        for &child in node.children() {
            let operand = self.lower_expression(child);
            elements.push(operand);

            let ty = self
                .builder
                .type_result
                .layer()
                .node_type(child)
                .unwrap_or_else(|| TypeId::new(0));
            element_types.push(ty);
        }

        let tuple_from_type = |ty| {
            let ty = self.builder.resolve_alias_type(ty);
            matches!(
                self.builder.type_result.layer().table().kind(ty),
                Some(TypeKind::Tuple { elements: tuple_elements }) if tuple_elements.len() == element_types.len()
            )
            .then_some(ty)
        };
        let ty = self
            .node_type(expr_id)
            .and_then(tuple_from_type)
            .or_else(|| tuple_from_type(self.return_type))
            .unwrap_or_else(|| self.builder.find_tuple_type(&element_types));

        let temp_id = self.declare_local(None, ty);
        self.current_instructions.push((
            Instruction::Assign(temp_id, RValue::NewTuple(ty, elements)),
            None,
        ));
        Operand::Local(temp_id)
    }

    /// Lower `new([T], size)`.
    pub(super) fn lower_new_array_expression(
        &mut self,
        expr_id: NodeId,
        node: &SyntaxNode,
        _dummy: &[Operand],
    ) -> Operand {
        let type_layer = self.builder.type_result.layer();

        let Some(type_node) = node.child(0) else {
            return Operand::Constant(Constant::Null);
        };

        let array_type = type_layer
            .node_type(type_node)
            .or_else(|| type_layer.node_type(expr_id))
            .unwrap_or_else(|| TypeId::new(0));

        let resolved_array_type = self.builder.resolve_alias_type(array_type);

        let allocation = match type_layer.table().kind(resolved_array_type) {
            Some(TypeKind::Array { element }) => {
                let Some(length_node) = node.child(1) else {
                    return Operand::Constant(Constant::Null);
                };

                let length = self.lower_expression(length_node);
                NewArrayZeroedAllocation::Dynamic {
                    element_type: *element,
                    length,
                }
            }
            _ => return Operand::Constant(Constant::Null),
        };

        let temp_id = self.declare_local(None, array_type);

        let NewArrayZeroedAllocation::Dynamic {
            element_type,
            length,
        } = allocation;
        let rvalue = RValue::NewArrayZeroedDynamic {
            array_type,
            element_type,
            length,
        };

        self.current_instructions
            .push((Instruction::Assign(temp_id, rvalue), None));

        Operand::Local(temp_id)
    }
}

enum NewArrayZeroedAllocation {
    Dynamic {
        element_type: TypeId,
        length: Operand,
    },
}
