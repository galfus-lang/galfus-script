use super::function::FunctionBuilder;
use galfus_core::{NodeId, SymbolId, TypeId};
use galfus_frontend::{SymbolKind, SyntaxNodeKind, TypeKind};
use galfus_ir::mir::*;

impl<'b, 'a> FunctionBuilder<'b, 'a> {
    pub(super) fn variant_pattern_symbols(&self, pattern: NodeId) -> Option<(SymbolId, SymbolId)> {
        let resolution = self.builder.graph.resolution()?;
        let owner_symbol = resolution.reference_symbol(pattern)?;
        let variant_symbol = resolution.path_reference_symbol(pattern)?;
        Some((owner_symbol, variant_symbol))
    }

    fn get_imported_choice_variant(&self, pattern: NodeId) -> Option<(String, Vec<TypeId>)> {
        let resolution = self.builder.graph.resolution()?;
        if let Some(owner_symbol) = resolution.reference_symbol(pattern)
            && let Some(choice) = self
                .builder
                .type_result
                .imported_symbol_choices
                .get(&owner_symbol)
        {
            let variant_name = self
                .builder
                .graph
                .syntax()
                .child(pattern, 1)
                .map(|node| self.builder.node_text(node))?;
            let variant = choice.variants.iter().find(|v| v.name == variant_name)?;
            return Some((variant.name.clone(), variant.payload_types.clone()));
        }

        let variant_ty = self.builder.type_result.layer().node_type(pattern)?;
        let table = self.builder.type_result.layer().table();
        let (_root, segments) = match table.kind(variant_ty) {
            Some(TypeKind::Path { root, segments }) => (*root, segments),
            _ => return None,
        };
        if segments.len() != 2 {
            return None;
        }
        let choice_name = &segments[0];
        let variant_name = &segments[1];

        let choice = self
            .builder
            .type_result
            .imported_path_choices
            .values()
            .find(|c| c.name == *choice_name)?;
        let variant = choice.variants.iter().find(|v| v.name == *variant_name)?;

        Some((variant.name.clone(), variant.payload_types.clone()))
    }

    pub(super) fn lower_pattern_check(
        &mut self,
        pattern_node_id: NodeId,
        subject: &Operand,
        success_block: BlockId,
        failure_block: BlockId,
    ) {
        let syntax = self.builder.graph.syntax();
        let pattern_node = syntax.node(pattern_node_id).unwrap();
        let resolution = self.builder.graph.resolution();

        match pattern_node.kind() {
            SyntaxNodeKind::LiteralPattern => {
                let literal_expr = syntax.child(pattern_node_id, 0).unwrap();
                let literal_op = self.lower_expression(literal_expr);
                let bool_ty = self
                    .builder
                    .type_result
                    .layer()
                    .table()
                    .primitive(galfus_frontend::PrimitiveType::Bool);

                let cond_temp = self.declare_local(None, bool_ty);
                self.current_instructions.push((
                    Instruction::ConstraintCall {
                        method_name: "compare".to_string(),
                        obj: subject.clone(),
                        args: vec![literal_op],
                        destination: cond_temp,
                    },
                    None,
                ));
                self.terminate_block(Terminator::Branch {
                    cond: Operand::Local(cond_temp),
                    true_block: success_block,
                    true_args: Vec::new(),
                    false_block: failure_block,
                    false_args: Vec::new(),
                });
            }
            SyntaxNodeKind::WildcardPattern => {
                self.terminate_block(Terminator::Jump {
                    target: success_block,
                    args: Vec::new(),
                });
            }
            SyntaxNodeKind::BindingPattern => {
                if let Some(res) = resolution {
                    let ident = syntax
                        .first_child_of_kind(pattern_node_id, SyntaxNodeKind::Identifier)
                        .unwrap_or(pattern_node_id);
                    if let Some(symbol) = res.declaration_symbol(ident) {
                        let ty = self
                            .builder
                            .type_result
                            .layer()
                            .symbol_type(symbol)
                            .unwrap_or_else(|| TypeId::new(0));
                        let local_id = self.declare_local(Some(symbol), ty);
                        self.symbol_to_local.insert(symbol, local_id);

                        self.current_instructions.push((
                            Instruction::Assign(local_id, RValue::Use(subject.clone())),
                            None,
                        ));
                    }
                }
                self.terminate_block(Terminator::Jump {
                    target: success_block,
                    args: Vec::new(),
                });
            }
            SyntaxNodeKind::VariantPattern => {
                let symbols = self.variant_pattern_symbols(pattern_node_id);
                let variant_data =
                    symbols.and_then(|(_, vs)| resolution.and_then(|res| res.symbol(vs)));
                if let (Some((owner_symbol, variant_symbol)), Some(variant_data)) =
                    (symbols, variant_data)
                    && self.get_imported_choice_variant(pattern_node_id).is_none()
                {
                    match variant_data.kind() {
                        SymbolKind::EnumVariant => {
                            let val = self.get_enum_variant_value(variant_symbol);
                            let pattern_ty = self
                                .builder
                                .type_result
                                .layer()
                                .node_type(pattern_node_id)
                                .unwrap_or_else(|| TypeId::new(0));

                            let casted_temp = self.declare_local(None, pattern_ty);
                            self.current_instructions.push((
                                Instruction::Assign(
                                    casted_temp,
                                    RValue::Cast(
                                        Operand::Constant(Constant::Int32(val as i32)),
                                        pattern_ty,
                                    ),
                                ),
                                None,
                            ));

                            let bool_ty = self
                                .builder
                                .type_result
                                .layer()
                                .table()
                                .primitive(galfus_frontend::PrimitiveType::Bool);
                            let cond_temp = self.declare_local(None, bool_ty);
                            self.current_instructions.push((
                                Instruction::Assign(
                                    cond_temp,
                                    RValue::BinaryOp(
                                        MirBinaryOp::Equal,
                                        subject.clone(),
                                        Operand::Local(casted_temp),
                                    ),
                                ),
                                None,
                            ));
                            self.terminate_block(Terminator::Branch {
                                cond: Operand::Local(cond_temp),
                                true_block: success_block,
                                true_args: Vec::new(),
                                false_block: failure_block,
                                false_args: Vec::new(),
                            });
                        }
                        SymbolKind::ChoiceVariant => {
                            let variant_name = self
                                .builder
                                .string_table
                                .resolve(variant_data.name())
                                .unwrap_or("")
                                .to_string();
                            let bool_ty = self
                                .builder
                                .type_result
                                .layer()
                                .node_type(pattern_node_id)
                                .unwrap_or_else(|| TypeId::new(0));

                            let cond_temp = self.declare_local(None, bool_ty);
                            self.current_instructions.push((
                                Instruction::Assign(
                                    cond_temp,
                                    RValue::ChoiceVariantIs(subject.clone(), variant_symbol),
                                ),
                                None,
                            ));

                            let payload_extract_block = self.builder.next_block();
                            self.terminate_block(Terminator::Branch {
                                cond: Operand::Local(cond_temp),
                                true_block: payload_extract_block,
                                true_args: Vec::new(),
                                false_block: failure_block,
                                false_args: Vec::new(),
                            });

                            self.blocks.last_mut().unwrap().id = payload_extract_block;
                            self.current_block = payload_extract_block;

                            if let Some(payload_node_id) = syntax.first_child_of_kind(
                                pattern_node_id,
                                SyntaxNodeKind::VariantPatternPayload,
                            ) {
                                let payload_node = syntax.node(payload_node_id).unwrap();
                                let payload_patterns = payload_node.children();

                                let payload_types = if let Some((_, imported_payload_types)) =
                                    self.get_imported_choice_variant(pattern_node_id)
                                {
                                    imported_payload_types
                                } else {
                                    self.choice_variant_payload_types(owner_symbol, variant_symbol)
                                };

                                if !payload_patterns.is_empty() {
                                    let payload_ty = if payload_patterns.len() > 1 {
                                        self.find_tuple_type(&payload_types)
                                    } else {
                                        payload_types[0]
                                    };

                                    let payload_temp = self.declare_local(None, payload_ty);
                                    self.current_instructions.push((
                                        Instruction::Assign(
                                            payload_temp,
                                            RValue::MemberAccess(subject.clone(), variant_name),
                                        ),
                                        None,
                                    ));

                                    let payload_op = Operand::Local(payload_temp);
                                    if payload_patterns.len() == 1 {
                                        self.lower_pattern_check(
                                            payload_patterns[0],
                                            &payload_op,
                                            success_block,
                                            failure_block,
                                        );
                                    } else {
                                        for (i, &child_pattern) in
                                            payload_patterns.iter().enumerate()
                                        {
                                            let element_ty = payload_types[i];
                                            let element_temp = self.declare_local(None, element_ty);
                                            self.current_instructions.push((
                                                Instruction::Assign(
                                                    element_temp,
                                                    RValue::MemberAccess(
                                                        payload_op.clone(),
                                                        i.to_string(),
                                                    ),
                                                ),
                                                None,
                                            ));

                                            let next_field_block =
                                                if i == payload_patterns.len() - 1 {
                                                    success_block
                                                } else {
                                                    self.builder.next_block()
                                                };

                                            self.lower_pattern_check(
                                                child_pattern,
                                                &Operand::Local(element_temp),
                                                next_field_block,
                                                failure_block,
                                            );

                                            if i < payload_patterns.len() - 1 {
                                                self.blocks.last_mut().unwrap().id =
                                                    next_field_block;
                                                self.current_block = next_field_block;
                                            }
                                        }
                                    }
                                } else {
                                    self.terminate_block(Terminator::Jump {
                                        target: success_block,
                                        args: Vec::new(),
                                    });
                                }
                            } else {
                                self.terminate_block(Terminator::Jump {
                                    target: success_block,
                                    args: Vec::new(),
                                });
                            }
                        }
                        _ => {
                            self.terminate_block(Terminator::Jump {
                                target: failure_block,
                                args: Vec::new(),
                            });
                        }
                    }
                } else if let Some((variant_name, payload_types)) =
                    self.get_imported_choice_variant(pattern_node_id)
                {
                    let variant_ty = self
                        .builder
                        .type_result
                        .layer()
                        .node_type(pattern_node_id)
                        .unwrap();

                    let bool_ty = self
                        .builder
                        .type_result
                        .layer()
                        .table()
                        .primitive(galfus_frontend::PrimitiveType::Bool);

                    let cond_temp = self.declare_local(None, bool_ty);
                    self.current_instructions.push((
                        Instruction::Assign(
                            cond_temp,
                            RValue::Instanceof(subject.clone(), variant_ty),
                        ),
                        None,
                    ));

                    let payload_extract_block = self.builder.next_block();
                    self.terminate_block(Terminator::Branch {
                        cond: Operand::Local(cond_temp),
                        true_block: payload_extract_block,
                        true_args: Vec::new(),
                        false_block: failure_block,
                        false_args: Vec::new(),
                    });

                    self.blocks.last_mut().unwrap().id = payload_extract_block;
                    self.current_block = payload_extract_block;

                    if let Some(payload_node_id) = syntax
                        .first_child_of_kind(pattern_node_id, SyntaxNodeKind::VariantPatternPayload)
                    {
                        let payload_node = syntax.node(payload_node_id).unwrap();
                        let payload_patterns = payload_node.children();

                        if !payload_patterns.is_empty() {
                            let payload_ty = if payload_patterns.len() > 1 {
                                self.find_tuple_type(&payload_types)
                            } else {
                                payload_types[0]
                            };

                            let payload_temp = self.declare_local(None, payload_ty);
                            self.current_instructions.push((
                                Instruction::Assign(
                                    payload_temp,
                                    RValue::MemberAccess(subject.clone(), variant_name),
                                ),
                                None,
                            ));

                            let payload_op = Operand::Local(payload_temp);
                            if payload_patterns.len() == 1 {
                                self.lower_pattern_check(
                                    payload_patterns[0],
                                    &payload_op,
                                    success_block,
                                    failure_block,
                                );
                            } else {
                                for (i, &child_pattern) in payload_patterns.iter().enumerate() {
                                    let element_ty = payload_types[i];
                                    let element_temp = self.declare_local(None, element_ty);
                                    self.current_instructions.push((
                                        Instruction::Assign(
                                            element_temp,
                                            RValue::MemberAccess(payload_op.clone(), i.to_string()),
                                        ),
                                        None,
                                    ));

                                    let next_field_block = if i == payload_patterns.len() - 1 {
                                        success_block
                                    } else {
                                        self.builder.next_block()
                                    };

                                    self.lower_pattern_check(
                                        child_pattern,
                                        &Operand::Local(element_temp),
                                        next_field_block,
                                        failure_block,
                                    );

                                    if i < payload_patterns.len() - 1 {
                                        self.blocks.last_mut().unwrap().id = next_field_block;
                                        self.current_block = next_field_block;
                                    }
                                }
                            }
                        } else {
                            self.terminate_block(Terminator::Jump {
                                target: success_block,
                                args: Vec::new(),
                            });
                        }
                    } else {
                        self.terminate_block(Terminator::Jump {
                            target: success_block,
                            args: Vec::new(),
                        });
                    }
                } else {
                    self.terminate_block(Terminator::Jump {
                        target: failure_block,
                        args: Vec::new(),
                    });
                }
            }

            SyntaxNodeKind::TypePattern => {
                let type_node = self.first_type_child(pattern_node_id).unwrap();
                let pattern_type = self
                    .builder
                    .type_result
                    .layer()
                    .node_type(type_node)
                    .unwrap_or_else(|| TypeId::new(0));

                let bool_ty = self
                    .builder
                    .type_result
                    .layer()
                    .node_type(pattern_node_id)
                    .unwrap_or_else(|| TypeId::new(0));

                let cond_temp = self.declare_local(None, bool_ty);
                self.current_instructions.push((
                    Instruction::Assign(
                        cond_temp,
                        RValue::Instanceof(subject.clone(), pattern_type),
                    ),
                    None,
                ));

                let type_check_success = self.builder.next_block();
                self.terminate_block(Terminator::Branch {
                    cond: Operand::Local(cond_temp),
                    true_block: type_check_success,
                    true_args: Vec::new(),
                    false_block: failure_block,
                    false_args: Vec::new(),
                });

                self.blocks.last_mut().unwrap().id = type_check_success;
                self.current_block = type_check_success;

                if let Some(binding_node_id) = syntax
                    .first_child_of_kind(pattern_node_id, SyntaxNodeKind::TypePatternBinding)
                    .filter(|_| resolution.is_some())
                {
                    let symbols = self.declaration_symbols_in_node(
                        binding_node_id,
                        &[SymbolKind::TypePatternBinding],
                    );
                    for symbol in symbols {
                        let local_id = self.declare_local(Some(symbol), pattern_type);
                        self.symbol_to_local.insert(symbol, local_id);

                        self.current_instructions.push((
                            Instruction::Assign(local_id, RValue::Use(subject.clone())),
                            None,
                        ));
                    }
                }

                self.terminate_block(Terminator::Jump {
                    target: success_block,
                    args: Vec::new(),
                });
            }

            SyntaxNodeKind::StructPattern => {
                let pattern_type = self
                    .builder
                    .type_result
                    .layer()
                    .node_type(pattern_node_id)
                    .unwrap_or_else(|| TypeId::new(0));

                let bool_ty = self
                    .builder
                    .type_result
                    .layer()
                    .table()
                    .primitive(galfus_frontend::PrimitiveType::Bool);

                let cond_temp = self.declare_local(None, bool_ty);
                self.current_instructions.push((
                    Instruction::Assign(
                        cond_temp,
                        RValue::Instanceof(subject.clone(), pattern_type),
                    ),
                    None,
                ));

                let struct_check_success = self.builder.next_block();
                self.terminate_block(Terminator::Branch {
                    cond: Operand::Local(cond_temp),
                    true_block: struct_check_success,
                    true_args: Vec::new(),
                    false_block: failure_block,
                    false_args: Vec::new(),
                });

                self.blocks.last_mut().unwrap().id = struct_check_success;
                self.current_block = struct_check_success;

                let fields = &pattern_node.children()[1..];
                if fields.is_empty() {
                    self.terminate_block(Terminator::Jump {
                        target: success_block,
                        args: Vec::new(),
                    });
                    return;
                }

                for (i, &field) in fields.iter().enumerate() {
                    let field_node = syntax.node(field).unwrap();
                    let field_ident = syntax
                        .first_child_of_kind(field, SyntaxNodeKind::Identifier)
                        .unwrap();
                    let field_name = self.builder.node_text(field_ident).to_string();

                    let field_ty = self
                        .builder
                        .type_result
                        .layer()
                        .node_type(field)
                        .unwrap_or_else(|| TypeId::new(0));

                    let field_temp = self.declare_local(None, field_ty);
                    self.current_instructions.push((
                        Instruction::Assign(
                            field_temp,
                            RValue::MemberAccess(subject.clone(), field_name),
                        ),
                        None,
                    ));

                    let field_op = Operand::Local(field_temp);

                    let next_field_block = if i == fields.len() - 1 {
                        success_block
                    } else {
                        self.builder.next_block()
                    };

                    if field_node.children().len() > 1 {
                        let inner_pattern = field_node.child(1).unwrap();
                        self.lower_pattern_check(
                            inner_pattern,
                            &field_op,
                            next_field_block,
                            failure_block,
                        );
                    } else {
                        if let Some(res) = resolution {
                            let ident = syntax
                                .first_child_of_kind(field, SyntaxNodeKind::Identifier)
                                .unwrap();
                            if let Some(symbol) = res.declaration_symbol(ident) {
                                let local_id = self.declare_local(Some(symbol), field_ty);
                                self.symbol_to_local.insert(symbol, local_id);
                                self.current_instructions.push((
                                    Instruction::Assign(local_id, RValue::Use(field_op)),
                                    None,
                                ));
                            }
                        }
                        self.terminate_block(Terminator::Jump {
                            target: next_field_block,
                            args: Vec::new(),
                        });
                    }

                    if i < fields.len() - 1 {
                        self.blocks.last_mut().unwrap().id = next_field_block;
                        self.current_block = next_field_block;
                    }
                }
            }
            _ => {
                self.terminate_block(Terminator::Jump {
                    target: failure_block,
                    args: Vec::new(),
                });
            }
        }
    }
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
            let resolution = self.builder.graph.resolution();
            if let Some(symbol) = resolution.and_then(|res| res.declaration_symbol(pattern_node_id))
            {
                let ty = self.symbol_type(symbol).unwrap_or_else(|| TypeId::new(0));
                let local_id = self.declare_local(Some(symbol), ty);
                self.current_instructions
                    .push((Instruction::Assign(local_id, RValue::Use(operand)), None));
            }
            return;
        }

        let child = match pattern_node.first_child() {
            Some(c) => c,
            None => return,
        };
        let child_node = syntax.node(child).unwrap();

        match child_node.kind() {
            SyntaxNodeKind::Identifier => {
                let resolution = self.builder.graph.resolution();
                if let Some(symbol) = resolution.and_then(|res| res.declaration_symbol(child)) {
                    let ty = self.symbol_type(symbol).unwrap_or_else(|| TypeId::new(0));
                    let local_id = self.declare_local(Some(symbol), ty);
                    self.current_instructions
                        .push((Instruction::Assign(local_id, RValue::Use(operand)), None));
                }
            }
            SyntaxNodeKind::StructBindingPattern => {
                for field_id in child_node.children() {
                    let field = syntax.node(*field_id).unwrap();
                    let field_name_node = field.first_child().unwrap();
                    let field_name = self.builder.node_text(field_name_node).to_string();
                    let value_pattern = field.child(1).unwrap_or(field_name_node);

                    let temp_ty = TypeId::new(0); // we should lookup proper type
                    let temp_local = self.declare_local(None, temp_ty);
                    self.current_instructions.push((
                        Instruction::Assign(
                            temp_local,
                            RValue::MemberAccess(operand.clone(), field_name),
                        ),
                        None,
                    ));
                    self.lower_destructuring_binding(value_pattern, Operand::Local(temp_local));
                }
            }
            SyntaxNodeKind::ArrayBindingPattern | SyntaxNodeKind::TupleBindingPattern => {
                for (i, element_id) in child_node.children().iter().enumerate() {
                    let element = syntax.node(*element_id).unwrap();
                    if element.kind() == SyntaxNodeKind::RestBindingPattern {
                        let rest_target = element.first_child().unwrap();
                        self.lower_static_array_rest_binding(rest_target, operand.clone(), i);
                        break;
                    } else {
                        let temp_ty = TypeId::new(0);
                        let temp_local = self.declare_local(None, temp_ty);

                        let idx_operand = Operand::Constant(Constant::Int32(i as i32));
                        self.current_instructions.push((
                            Instruction::Assign(
                                temp_local,
                                RValue::ArrayIndex(operand.clone(), idx_operand),
                            ),
                            None,
                        ));
                        self.lower_destructuring_binding(*element_id, Operand::Local(temp_local));
                    }
                }
            }
            _ => {
                // Wildcard or other
            }
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
            let element_local = self.declare_local(None, *element_type);
            self.current_instructions.push((
                Instruction::Assign(
                    element_local,
                    RValue::ArrayIndex(
                        operand.clone(),
                        Operand::Constant(Constant::Int32(index as i32)),
                    ),
                ),
                None,
            ));
            elements.push(Operand::Local(element_local));
        }

        let rest_local = self.declare_local(None, rest_type);
        self.current_instructions.push((
            Instruction::Assign(rest_local, RValue::NewArray(rest_type, elements)),
            None,
        ));
        self.lower_destructuring_binding(rest_target, Operand::Local(rest_local));
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

        let syntax_node = self.builder.graph.syntax().node(node)?;
        syntax_node
            .children()
            .iter()
            .find_map(|child| self.binding_symbol_type(*child))
    }
}
