use super::function::FunctionBuilder;
use galfus_core::{NodeId, TypeId};
use galfus_frontend::{SymbolKind, SyntaxNodeKind};
use galfus_ir::mir::*;

impl<'b, 'a> FunctionBuilder<'b, 'a> {
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
                        return_type: bool_ty,
                    },
                    None,
                ));
                self.close_current_block(Terminator::Branch {
                    cond: Operand::Local(cond_temp),
                    true_block: success_block,
                    true_args: Vec::new(),
                    false_block: failure_block,
                    false_args: Vec::new(),
                });
            }
            SyntaxNodeKind::WildcardPattern => {
                self.close_current_block(Terminator::Jump {
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
                self.close_current_block(Terminator::Jump {
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
                            self.close_current_block(Terminator::Branch {
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
                            self.close_current_block(Terminator::Branch {
                                cond: Operand::Local(cond_temp),
                                true_block: payload_extract_block,
                                true_args: Vec::new(),
                                false_block: failure_block,
                                false_args: Vec::new(),
                            });

                            self.begin_block(payload_extract_block);

                            if let Some(payload_node_id) = syntax.first_child_of_kind(
                                pattern_node_id,
                                SyntaxNodeKind::VariantPatternPayload,
                            ) {
                                let payload_node = syntax.node(payload_node_id).unwrap();
                                let payload_patterns = payload_node.children();

                                let payload_types = if let Some((_, _, imported_payload_types)) =
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
                                                self.begin_block(next_field_block);
                                            }
                                        }
                                    }
                                } else {
                                    self.close_current_block(Terminator::Jump {
                                        target: success_block,
                                        args: Vec::new(),
                                    });
                                }
                            } else {
                                self.close_current_block(Terminator::Jump {
                                    target: success_block,
                                    args: Vec::new(),
                                });
                            }
                        }
                        _ => {
                            self.close_current_block(Terminator::Jump {
                                target: failure_block,
                                args: Vec::new(),
                            });
                        }
                    }
                } else if let Some((choice_name, variant_name, payload_types)) =
                    self.get_imported_choice_variant(pattern_node_id)
                {
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
                            RValue::ImportedChoiceVariantIs(
                                subject.clone(),
                                choice_name,
                                variant_name.clone(),
                            ),
                        ),
                        None,
                    ));

                    let payload_extract_block = self.builder.next_block();
                    self.close_current_block(Terminator::Branch {
                        cond: Operand::Local(cond_temp),
                        true_block: payload_extract_block,
                        true_args: Vec::new(),
                        false_block: failure_block,
                        false_args: Vec::new(),
                    });

                    self.begin_block(payload_extract_block);

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
                                        self.begin_block(next_field_block);
                                    }
                                }
                            }
                        } else {
                            self.close_current_block(Terminator::Jump {
                                target: success_block,
                                args: Vec::new(),
                            });
                        }
                    } else {
                        self.close_current_block(Terminator::Jump {
                            target: success_block,
                            args: Vec::new(),
                        });
                    }
                } else {
                    self.close_current_block(Terminator::Jump {
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
                    .node_type(pattern_node_id)
                    .or_else(|| self.builder.type_result.layer().node_type(type_node))
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

                let type_check_success = self.builder.next_block();
                self.close_current_block(Terminator::Branch {
                    cond: Operand::Local(cond_temp),
                    true_block: type_check_success,
                    true_args: Vec::new(),
                    false_block: failure_block,
                    false_args: Vec::new(),
                });

                self.begin_block(type_check_success);

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

                self.close_current_block(Terminator::Jump {
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
                self.close_current_block(Terminator::Branch {
                    cond: Operand::Local(cond_temp),
                    true_block: struct_check_success,
                    true_args: Vec::new(),
                    false_block: failure_block,
                    false_args: Vec::new(),
                });

                self.begin_block(struct_check_success);

                let fields = &pattern_node.children()[1..];
                if fields.is_empty() {
                    self.close_current_block(Terminator::Jump {
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
                        self.close_current_block(Terminator::Jump {
                            target: next_field_block,
                            args: Vec::new(),
                        });
                    }

                    if i < fields.len() - 1 {
                        self.begin_block(next_field_block);
                    }
                }
            }
            _ => {
                self.close_current_block(Terminator::Jump {
                    target: failure_block,
                    args: Vec::new(),
                });
            }
        }
    }
}
