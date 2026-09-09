use super::function::FunctionBuilder;
use galfus_core::{NodeId, TypeId};
use galfus_frontend::{SyntaxNodeKind, TypeKind};
use galfus_ir::mir::*;

impl<'b, 'a> FunctionBuilder<'b, 'a> {
    pub(super) fn lower_destructuring_binding(
        &mut self,
        mut pattern_node_id: NodeId,
        operand: Operand,
    ) {
        let syntax = self.builder.graph.syntax();
        let mut pattern_node = syntax.node(pattern_node_id).unwrap();

        if pattern_node.kind() == SyntaxNodeKind::ForBinding {
            pattern_node_id = pattern_node.first_child().unwrap();
            pattern_node = syntax.node(pattern_node_id).unwrap();
        }

        if pattern_node.kind() == SyntaxNodeKind::Identifier {
            self.bind_destructuring_identifier(pattern_node_id, operand);
            return;
        }

        let Some(child) = pattern_node.first_child() else {
            return;
        };
        let child_node = syntax.node(child).unwrap();
        match child_node.kind() {
            SyntaxNodeKind::Identifier => self.bind_destructuring_identifier(child, operand),
            SyntaxNodeKind::StructBindingPattern => {
                for field_id in child_node.children() {
                    let field = syntax.node(*field_id).unwrap();
                    let field_name_node = field.first_child().unwrap();
                    let field_name = self.builder.node_text(field_name_node).to_owned();
                    let value_pattern = field.child(1).unwrap_or(field_name_node);
                    let local = self.declare_local(None, TypeId::new(0));
                    self.current_instructions.push((
                        Instruction::Assign(
                            local,
                            RValue::MemberAccess(operand.clone(), field_name),
                        ),
                        None,
                    ));
                    self.lower_destructuring_binding(value_pattern, Operand::Local(local));
                }
            }
            SyntaxNodeKind::ArrayBindingPattern | SyntaxNodeKind::TupleBindingPattern => {
                for (index, element_id) in child_node.children().iter().enumerate() {
                    let element = syntax.node(*element_id).unwrap();
                    if element.kind() == SyntaxNodeKind::RestBindingPattern {
                        self.lower_static_array_rest_binding(
                            element.first_child().unwrap(),
                            operand.clone(),
                            index,
                        );
                        break;
                    }
                    let local = self.declare_local(None, TypeId::new(0));
                    self.current_instructions.push((
                        Instruction::Assign(
                            local,
                            RValue::ArrayIndex(
                                operand.clone(),
                                Operand::Constant(Constant::Int32(index as i32)),
                            ),
                        ),
                        None,
                    ));
                    self.lower_destructuring_binding(*element_id, Operand::Local(local));
                }
            }
            _ => {}
        }
    }

    fn bind_destructuring_identifier(&mut self, identifier: NodeId, operand: Operand) {
        let symbol = self
            .builder
            .graph
            .resolution()
            .and_then(|resolution| resolution.declaration_symbol(identifier));
        if let Some(symbol) = symbol {
            let type_id = self.symbol_type(symbol).unwrap_or_else(|| TypeId::new(0));
            let local = self.declare_local(Some(symbol), type_id);
            self.current_instructions
                .push((Instruction::Assign(local, RValue::Use(operand)), None));
        }
    }

    fn lower_static_array_rest_binding(
        &mut self,
        rest_target: NodeId,
        operand: Operand,
        start: usize,
    ) {
        let Some(length) = self.static_array_length_for_operand(&operand) else {
            return;
        };
        let Some(rest_type) = self
            .node_type(rest_target)
            .or_else(|| self.binding_symbol_type(rest_target))
        else {
            return;
        };
        let Some(TypeKind::Array {
            element: element_type,
        }) = self.builder.type_result.layer().table().kind(rest_type)
        else {
            return;
        };
        let mut elements = Vec::new();
        for index in start..length {
            let local = self.declare_local(None, *element_type);
            self.current_instructions.push((
                Instruction::Assign(
                    local,
                    RValue::ArrayIndex(
                        operand.clone(),
                        Operand::Constant(Constant::Int32(index as i32)),
                    ),
                ),
                None,
            ));
            elements.push(Operand::Local(local));
        }
        let local = self.declare_local(None, rest_type);
        self.current_instructions.push((
            Instruction::Assign(local, RValue::NewArray(rest_type, elements)),
            None,
        ));
        self.lower_destructuring_binding(rest_target, Operand::Local(local));
    }

    fn static_array_length_for_operand(&self, operand: &Operand) -> Option<usize> {
        let Operand::Local(local) = operand else {
            return None;
        };
        let rvalue = self
            .current_instructions
            .iter()
            .rev()
            .find_map(|(instruction, _)| {
                let Instruction::Assign(destination, rvalue) = instruction else {
                    return None;
                };
                (*destination == *local).then_some(rvalue)
            })?;
        match rvalue {
            RValue::Use(operand) => self.static_array_length_for_operand(operand),
            RValue::NewArray(_, elements) => Some(elements.len()),
            RValue::NewArrayDynamic(_, elements) => {
                elements.iter().try_fold(0usize, |length, element| {
                    let element_length = match element {
                        ArrayLiteralElement::Single(_) => 1,
                        ArrayLiteralElement::Spread(operand) => {
                            self.static_array_length_for_operand(operand)?
                        }
                    };
                    length.checked_add(element_length)
                })
            }
            _ => None,
        }
    }

    fn binding_symbol_type(&self, node: NodeId) -> Option<TypeId> {
        let resolution = self.builder.graph.resolution()?;
        if let Some(symbol) = resolution.declaration_symbol(node) {
            return self.symbol_type(symbol);
        }
        self.builder
            .graph
            .syntax()
            .node(node)?
            .children()
            .iter()
            .find_map(|child| self.binding_symbol_type(*child))
    }
}
