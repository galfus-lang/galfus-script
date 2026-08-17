use super::function::FunctionBuilder;
use super::function_helpers::parse_int;
use galfus_core::{FunctionId, NodeId, SymbolId, TypeId};
use galfus_frontend::{
    PathReferenceKind, RangeDesugarTarget, SymbolKind, SyntaxNodeKind, TypeKind,
};
use galfus_ir::mir::*;
use std::collections::HashMap;

impl<'b, 'a> FunctionBuilder<'b, 'a> {
    pub(super) fn lower_expression(&mut self, expr_id: NodeId) -> Operand {
        let syntax = self.builder.graph.syntax();
        let Some(node) = syntax.node(expr_id) else {
            return Operand::Constant(Constant::Null);
        };
        let resolution = self.builder.graph.resolution();

        match node.kind() {
            SyntaxNodeKind::IntegerLiteral => {
                let text = self.builder.node_text(expr_id);
                let val = parse_int(text).unwrap_or(0) as i128;

                let mut constant = Constant::Int32(val as i32);
                if let Some(ty) = self.node_type(expr_id) {
                    let resolved = self.builder.resolve_alias_type(ty);
                    if let Some(TypeKind::Primitive(p)) =
                        self.builder.type_result.layer().table().kind(resolved)
                    {
                        constant = match p {
                            galfus_frontend::PrimitiveType::Int8 => Constant::Int8(val as i8),
                            galfus_frontend::PrimitiveType::Int16 => Constant::Int16(val as i16),
                            galfus_frontend::PrimitiveType::Int32 => Constant::Int32(val as i32),
                            galfus_frontend::PrimitiveType::Int64 => Constant::Int64(val as i64),
                            galfus_frontend::PrimitiveType::Uint8 => Constant::Uint8(val as u8),
                            galfus_frontend::PrimitiveType::Uint16 => Constant::Uint16(val as u16),
                            galfus_frontend::PrimitiveType::Uint32 => Constant::Uint32(val as u32),
                            galfus_frontend::PrimitiveType::Uint64 => Constant::Uint64(val as u64),
                            _ => constant,
                        };
                    }
                }
                Operand::Constant(constant)
            }

            SyntaxNodeKind::FloatLiteral => {
                let text = self.builder.node_text(expr_id);
                let val = text.parse::<f64>().unwrap_or(0.0);

                let mut constant = Constant::Float32(galfus_core::normalize_f32(val as f32));
                if let Some(ty) = self.node_type(expr_id) {
                    let resolved = self.builder.resolve_alias_type(ty);
                    if let Some(TypeKind::Primitive(p)) =
                        self.builder.type_result.layer().table().kind(resolved)
                        && p == &galfus_frontend::PrimitiveType::Float64
                    {
                        constant = Constant::Float64(galfus_core::normalize_f64(val));
                    }
                }
                Operand::Constant(constant)
            }

            SyntaxNodeKind::StringLiteral => {
                let text = self.builder.node_text(expr_id);
                let val = if (text.starts_with('"') && text.ends_with('"'))
                    || (text.starts_with('\'') && text.ends_with('\''))
                {
                    &text[1..text.len() - 1]
                } else {
                    text
                };
                let unescaped = unescape_string(val);
                Operand::Constant(Constant::String(unescaped))
            }

            SyntaxNodeKind::BoolLiteral => {
                let text = self.builder.node_text(expr_id);
                let val = text == "true";
                Operand::Constant(Constant::Bool(val))
            }

            SyntaxNodeKind::NullLiteral => Operand::Constant(Constant::Null),

            SyntaxNodeKind::RangeExpression => {
                let Some(target) = self.builder.type_result.range_desugar(expr_id) else {
                    return Operand::Constant(Constant::Null);
                };
                let Some(start) = node.child(0) else {
                    return Operand::Constant(Constant::Null);
                };
                let Some(end_or_count) = node.child(2) else {
                    return Operand::Constant(Constant::Null);
                };
                let start_operand = self.lower_expression(start);
                let end_or_count_operand = self.lower_expression(end_or_count);
                let range_type = self.node_type(expr_id).unwrap_or_else(|| TypeId::new(0));
                let item_type = self.node_type(start).unwrap_or_else(|| TypeId::new(0));
                let (function_name, arguments, concrete_types) = match target {
                    RangeDesugarTarget::Exclusive => (
                        "range",
                        vec![start_operand, end_or_count_operand],
                        Vec::new(),
                    ),
                    RangeDesugarTarget::Stepped => {
                        let step = syntax
                            .child(expr_id, 3)
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
                let Some(ctx_ptr) = self.builder.workspace_ctx else {
                    return Operand::Constant(Constant::Null);
                };
                let Some(caller_module_id) = self.builder.workspace_module_id else {
                    return Operand::Constant(Constant::Null);
                };
                let ctx = unsafe { &mut *ctx_ptr };
                let Some(function) = ctx.specialize_builtin_function(
                    caller_module_id,
                    expr_id,
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

            SyntaxNodeKind::TypeofExpression => {
                let Some(subject) = node.child(0) else {
                    return Operand::Constant(Constant::Null);
                };
                let Some(arms) = node.child(1) else {
                    return Operand::Constant(Constant::Null);
                };
                let Some(subject_type) = self.typeof_subject_type(subject) else {
                    return Operand::Constant(Constant::Null);
                };

                for arm in syntax
                    .node(arms)
                    .into_iter()
                    .flat_map(|arms| arms.children())
                {
                    let Some(pattern) = syntax.child(*arm, 0) else {
                        continue;
                    };
                    let Some(body) = syntax.child(*arm, 1).and_then(|body| syntax.child(body, 0))
                    else {
                        continue;
                    };
                    let is_wildcard = syntax
                        .node(pattern)
                        .is_some_and(|pattern| pattern.kind() == SyntaxNodeKind::WildcardPattern);
                    let matches_subject = self.node_type(pattern).is_some_and(|pattern_type| {
                        self.builder.is_same_type(pattern_type, subject_type)
                    });

                    if is_wildcard || matches_subject {
                        if syntax
                            .node(body)
                            .is_some_and(|body| body.kind() == SyntaxNodeKind::Block)
                        {
                            self.lower_block(body);
                            return Operand::Constant(Constant::Null);
                        }
                        return self.lower_expression(body);
                    }
                }

                Operand::Constant(Constant::Null)
            }

            SyntaxNodeKind::NameExpression | SyntaxNodeKind::Identifier => {
                if let Some(res) = resolution {
                    let symbol = res.reference_symbol(expr_id).or_else(|| {
                        let ident =
                            syntax.first_child_of_kind(expr_id, SyntaxNodeKind::Identifier)?;
                        res.reference_symbol(ident)
                    });
                    if let Some(sym) = symbol {
                        if let Some(local_id) = self.symbol_to_local.get(&sym).copied() {
                            return Operand::Local(local_id);
                        } else {
                            if matches!(
                                res.symbol(sym).map(|symbol| symbol.kind()),
                                Some(SymbolKind::Function)
                            ) {
                                return Operand::Constant(Constant::Function(
                                    self.function_id_for_symbol(sym, expr_id),
                                ));
                            }
                            let is_global = matches!(
                                res.symbol(sym).map(|s| s.kind()),
                                Some(galfus_frontend::SymbolKind::Var)
                                    | Some(galfus_frontend::SymbolKind::Const)
                            );
                            if is_global {
                                let name = res
                                    .symbol(sym)
                                    .map(|s| {
                                        self.builder
                                            .string_table
                                            .resolve(s.name())
                                            .unwrap_or("")
                                            .to_string()
                                    })
                                    .unwrap_or_default();
                                let ty = self
                                    .builder
                                    .type_result
                                    .layer()
                                    .symbol_type(sym)
                                    .unwrap_or_else(|| TypeId::new(0));
                                let temp_id = self.declare_local(None, ty);
                                self.current_instructions.push((
                                    Instruction::Assign(temp_id, RValue::LoadGlobal(name)),
                                    None,
                                ));
                                return Operand::Local(temp_id);
                            }
                        }
                    }
                }
                Operand::Constant(Constant::Null)
            }

            SyntaxNodeKind::BinaryExpression => {
                let left = node.child(0).unwrap();
                let op_node = node.child(1).unwrap();
                let right = node.child(2).unwrap();

                let left_operand = self.lower_expression(left);
                let right_operand = self.lower_expression(right);

                let left_ty = match &left_operand {
                    Operand::Local(local_id) => self
                        .locals
                        .iter()
                        .find(|local| local.id == *local_id)
                        .map(|local| local.ty)
                        .unwrap_or_else(|| self.node_type(left).unwrap_or_else(|| TypeId::new(0))),
                    _ => self.node_type(left).unwrap_or_else(|| TypeId::new(0)),
                };
                let right_ty = self.node_type(right).unwrap_or_else(|| TypeId::new(0));
                let right_operand = self.insert_cast_if_needed(right_operand, right_ty, left_ty);

                let op = self.lower_binary_op(op_node);

                let ty = self.node_type(expr_id).unwrap_or_else(|| TypeId::new(0));

                let temp_id = self.declare_local(None, ty);
                self.current_instructions.push((
                    Instruction::Assign(temp_id, RValue::BinaryOp(op, left_operand, right_operand)),
                    None,
                ));
                Operand::Local(temp_id)
            }

            SyntaxNodeKind::UnaryExpression => {
                let op_node = node.child(0).unwrap();
                let operand_node = node.child(1).unwrap();

                let operand = self.lower_expression(operand_node);
                let op = self.lower_unary_op(op_node);

                let ty = self.node_type(expr_id).unwrap_or_else(|| TypeId::new(0));

                let temp_id = self.declare_local(None, ty);
                self.current_instructions.push((
                    Instruction::Assign(temp_id, RValue::UnaryOp(op, operand)),
                    None,
                ));
                Operand::Local(temp_id)
            }

            SyntaxNodeKind::CastExpression => {
                let type_node = node.child(0).unwrap();
                let val_node = node.child(1).unwrap();
                let operand = self.lower_expression(val_node);

                let ty = self
                    .node_type(expr_id)
                    .or_else(|| self.node_type(type_node))
                    .unwrap_or_else(|| TypeId::new(0));

                let temp_id = self.declare_local(None, ty);
                self.current_instructions.push((
                    Instruction::Assign(temp_id, RValue::Cast(operand, ty)),
                    None,
                ));
                Operand::Local(temp_id)
            }

            SyntaxNodeKind::CopyExpression => {
                let value_node = node.child(0).unwrap();
                let operand = self.lower_expression(value_node);
                let ty = self.node_type(expr_id).unwrap_or_else(|| TypeId::new(0));
                let temp_id = self.declare_local(None, ty);
                self.current_instructions
                    .push((Instruction::Assign(temp_id, RValue::Copy(operand)), None));
                Operand::Local(temp_id)
            }

            SyntaxNodeKind::GroupedExpression => {
                if let Some(inner) = node.first_child() {
                    self.lower_expression(inner)
                } else {
                    Operand::Constant(Constant::Null)
                }
            }

            SyntaxNodeKind::CallExpression => {
                let target_node = node.child(0).unwrap();
                let arg_list_node = node.child(1).unwrap();
                let anchored_receiver = self.anchored_call_receiver(target_node);

                let mut expected_params = None;
                if self.is_choice_variant_call_target(target_node) {
                    if let Some((_, _, payload_types)) =
                        self.get_choice_variant_payload(target_node)
                    {
                        expected_params = Some(
                            payload_types
                                .into_iter()
                                .map(|ty| (ty, false))
                                .collect::<Vec<_>>(),
                        );
                    }
                } else {
                    let target_ty = self.node_type(target_node);
                    let resolved_target_ty = target_ty.map(|t| self.builder.resolve_alias_type(t));
                    if let Some(resolved_target_ty) = resolved_target_ty
                        && let Some(TypeKind::Function(f)) = self
                            .builder
                            .type_result
                            .layer()
                            .table()
                            .kind(resolved_target_ty)
                    {
                        expected_params = Some(
                            f.parameters()
                                .iter()
                                .map(|p| (p.ty(), p.is_rest()))
                                .collect::<Vec<_>>(),
                        );
                    }
                }

                let mut args = Vec::new();
                let mut arg_types = Vec::new();
                let parameter_offset = if let Some(receiver_node) = anchored_receiver {
                    let receiver_op = self.lower_expression(receiver_node);
                    let receiver_ty = self
                        .node_type(receiver_node)
                        .unwrap_or_else(|| TypeId::new(0));
                    let casted_receiver = if let Some(ref params) = expected_params
                        && let Some(&(expected_ty, _)) = params.first()
                    {
                        self.insert_cast_if_needed(receiver_op, receiver_ty, expected_ty)
                    } else {
                        receiver_op
                    };
                    args.push(casted_receiver);
                    arg_types.push(receiver_ty);
                    1
                } else {
                    0
                };

                if let Some(arg_list) = syntax.node(arg_list_node) {
                    for (i, &arg_id) in arg_list.children().iter().enumerate() {
                        let arg_expr = syntax
                            .node(arg_id)
                            .and_then(|n| {
                                if n.kind() == SyntaxNodeKind::Argument {
                                    syntax.child(arg_id, 0)
                                } else {
                                    Some(arg_id)
                                }
                            })
                            .unwrap_or(arg_id);

                        let arg_op = self.lower_expression(arg_expr);

                        let arg_ty = self.node_type(arg_expr).unwrap_or_else(|| TypeId::new(0));

                        let casted_op = if let Some(ref params) = expected_params {
                            let parameter_index = i + parameter_offset;
                            if let Some(&(expected_ty, is_rest)) = params.get(parameter_index) {
                                let target_expected_ty = if is_rest {
                                    let resolved_param_ty =
                                        self.builder.resolve_alias_type(expected_ty);
                                    match self
                                        .builder
                                        .type_result
                                        .layer()
                                        .table()
                                        .kind(resolved_param_ty)
                                    {
                                        Some(TypeKind::Array { element }) => *element,
                                        _ => expected_ty,
                                    }
                                } else {
                                    expected_ty
                                };
                                self.insert_cast_if_needed(arg_op, arg_ty, target_expected_ty)
                            } else if let Some(&(expected_ty, is_rest)) = params.last() {
                                if is_rest {
                                    let resolved_param_ty =
                                        self.builder.resolve_alias_type(expected_ty);
                                    let target_expected_ty = match self
                                        .builder
                                        .type_result
                                        .layer()
                                        .table()
                                        .kind(resolved_param_ty)
                                    {
                                        Some(TypeKind::Array { element }) => *element,
                                        _ => expected_ty,
                                    };
                                    self.insert_cast_if_needed(arg_op, arg_ty, target_expected_ty)
                                } else {
                                    arg_op
                                }
                            } else {
                                arg_op
                            }
                        } else {
                            arg_op
                        };

                        args.push(casted_op);
                        arg_types.push(arg_ty);
                    }
                }

                if let Some(ref params) = expected_params {
                    let provided = args.len() - parameter_offset;
                    if provided < params.len() {
                        for &(expected_ty, is_rest) in params.iter().skip(provided) {
                            if !is_rest {
                                args.push(Operand::Constant(Constant::Null));
                                arg_types.push(expected_ty);
                            }
                        }
                    }
                }

                // Check if it's a choice variant constructor call!
                let payload = if self.is_choice_variant_call_target(target_node) {
                    self.get_choice_variant_payload(target_node)
                } else {
                    None
                };
                if let Some((variant_name, owner_type, _payload_types)) = payload {
                    let payload_op = if args.is_empty() {
                        None
                    } else if args.len() == 1 {
                        Some(args[0].clone())
                    } else {
                        // Multiple arguments => build a tuple
                        let tuple_type = self.builder.find_tuple_type(&arg_types);
                        let tuple_temp = self.declare_local(None, tuple_type);
                        self.current_instructions.push((
                            Instruction::Assign(tuple_temp, RValue::NewTuple(tuple_type, args)),
                            None,
                        ));
                        Some(Operand::Local(tuple_temp))
                    };

                    let choice_temp = self.declare_local(None, owner_type);
                    self.current_instructions.push((
                        Instruction::Assign(
                            choice_temp,
                            RValue::Choice(owner_type, variant_name, payload_op),
                        ),
                        None,
                    ));
                    return Operand::Local(choice_temp);
                }

                let mut real_target = target_node;
                while let Some(node) = syntax.node(real_target)
                    && node.kind() == SyntaxNodeKind::GenericExpression
                {
                    if let Some(inner) = node.first_child() {
                        real_target = inner;
                    } else {
                        break;
                    }
                }

                let mut is_namespace_call = false;
                if let Some(target) = syntax.node(real_target)
                    && target.kind() == SyntaxNodeKind::PathExpression
                    && let Some(root_node) = target.first_child()
                    && let Some(root_symbol) =
                        resolution.and_then(|res| res.reference_symbol(root_node))
                    && let Some(sym_data) = resolution.and_then(|res| res.symbol(root_symbol))
                    && sym_data.kind() == SymbolKind::ImportNamespace
                {
                    is_namespace_call = true;
                }

                let anchored_function_symbol = anchored_receiver
                    .and_then(|receiver| self.anchored_function_symbol(receiver, target_node));
                let target_symbol = anchored_function_symbol.or_else(|| {
                    anchored_receiver
                        .is_none()
                        .then(|| self.call_target_symbol(target_node))
                        .flatten()
                });
                let is_dynamic_anchored_method =
                    anchored_receiver.is_some() && anchored_function_symbol.is_none();

                // Detect constraint method call: receiver is of a constraint type.
                // Example: `item::stringify()` where `item: Stringable`.
                // The receiver is child(0) of the PathExpression; its type resolves to
                // a Named type whose symbol has SymbolKind::Constraint.
                let is_constraint_method = syntax
                    .node(real_target)
                    .and_then(|target| {
                        if target.kind() != SyntaxNodeKind::PathExpression {
                            return None;
                        }
                        let receiver_node = target.child(0)?;
                        // receiver must be a name expression (local variable), not a module namespace
                        let receiver_kind = syntax.node(receiver_node)?.kind();
                        if !matches!(
                            receiver_kind,
                            SyntaxNodeKind::NameExpression | SyntaxNodeKind::Identifier
                        ) {
                            return None;
                        }
                        // Look up the type of the receiver
                        let receiver_ty =
                            self.builder.type_result.layer().node_type(receiver_node)?;
                        let receiver_ty = self.builder.resolve_alias_type(receiver_ty);
                        // Check if the type is Named and its symbol is a Constraint
                        if let Some(TypeKind::Named { symbol }) =
                            self.builder.type_result.layer().table().kind(receiver_ty)
                        {
                            let sym_data = self.builder.graph.resolution()?.symbol(*symbol)?;
                            if sym_data.kind() == SymbolKind::Constraint {
                                return Some(());
                            }
                        }
                        None
                    })
                    .is_some();

                if is_constraint_method || is_dynamic_anchored_method {
                    // Extract method name from the PathExpression member node (child 1).
                    let method_name = syntax
                        .node(real_target)
                        .and_then(|n| n.child(1))
                        .and_then(|member_node| {
                            syntax.node(member_node).map(|mn| {
                                let span = mn.span();
                                if span.start() < self.builder.source_text.len()
                                    && span.end() <= self.builder.source_text.len()
                                {
                                    self.builder.source_text[span.start()..span.end()].to_string()
                                } else {
                                    String::new()
                                }
                            })
                        })
                        .unwrap_or_default();

                    // For `item::stringify()`, the receiver `item` is child(0) of the
                    // PathExpression. `anchored_receiver` is None (item is NameExpression),
                    // so args is empty. We must lower the receiver node directly.
                    let obj = if !args.is_empty() {
                        // Receiver was already lowered as args[0] (anchored_receiver case).
                        args[0].clone()
                    } else if let Some(receiver_node) =
                        syntax.node(real_target).and_then(|n| n.child(0))
                    {
                        self.lower_expression(receiver_node)
                    } else {
                        Operand::Constant(Constant::Null)
                    };
                    // Extra args (beyond the receiver) are args[1..] or all of args if
                    // no receiver was pre-lowered.
                    let extra_args = if !args.is_empty() {
                        args[1..].to_vec()
                    } else {
                        vec![]
                    };

                    let ty = self.node_type(expr_id).unwrap_or_else(|| TypeId::new(0));
                    let temp_id = self.declare_local(None, ty);

                    self.current_instructions.push((
                        Instruction::ConstraintCall {
                            method_name,
                            obj,
                            args: extra_args,
                            destination: temp_id,
                        },
                        None,
                    ));

                    return Operand::Local(temp_id);
                }

                let is_indirect = if let Some(target_sym) = target_symbol {
                    if let Some(res) = self.builder.graph.resolution() {
                        if let Some(sym_data) = res.symbol(target_sym) {
                            matches!(
                                sym_data.kind(),
                                SymbolKind::Var
                                    | SymbolKind::Const
                                    | SymbolKind::Parameter
                                    | SymbolKind::RestParameter
                                    | SymbolKind::ForBinding
                                    | SymbolKind::PatternBinding
                            )
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                };

                let ty = self.node_type(expr_id).unwrap_or_else(|| TypeId::new(0));
                let temp_id = self.declare_local(None, ty);

                if is_indirect {
                    let func_op = self.lower_expression(target_node);
                    let instruction = if self.is_future_type(ty) {
                        Instruction::Assign(
                            temp_id,
                            RValue::CreateIndirectFuture {
                                func: func_op,
                                args,
                            },
                        )
                    } else {
                        Instruction::IndirectCall {
                            func: func_op,
                            args,
                            destination: temp_id,
                        }
                    };
                    self.current_instructions.push((instruction, None));
                } else {
                    let mut func_id = if is_namespace_call {
                        path_call_function_id(real_target)
                    } else if anchored_receiver.is_some() {
                        target_symbol
                            .map(|sym| FunctionId::new(sym.raw()))
                            .unwrap_or_else(|| path_call_function_id(real_target))
                    } else {
                        target_symbol
                            .map(|sym| self.function_id_for_symbol(sym, real_target))
                            .unwrap_or_else(|| FunctionId::new(target_node.raw()))
                    };

                    if let Some(symbol) = target_symbol
                        && let Some(specialized) =
                            self.specialize_generic_call(symbol, target_node, &arg_types)
                    {
                        func_id = specialized;
                    }

                    let is_external = target_symbol
                        .and_then(|sym| self.builder.type_result.layer().symbol_type(sym))
                        .and_then(
                            |ty| match self.builder.type_result.layer().table().kind(ty) {
                                Some(galfus_frontend::TypeKind::Function(f)) => {
                                    Some(f.is_external())
                                }
                                _ => None,
                            },
                        )
                        .unwrap_or(false);

                    let instruction = if self.is_future_type(ty) {
                        Instruction::Assign(
                            temp_id,
                            RValue::CreateFuture {
                                func: func_id,
                                args,
                                is_external,
                            },
                        )
                    } else {
                        Instruction::Call {
                            func: func_id,
                            args,
                            destination: temp_id,
                            is_external,
                        }
                    };
                    self.current_instructions.push((instruction, None));
                }

                Operand::Local(temp_id)
            }
            SyntaxNodeKind::PathExpression => {
                let kind = resolution.and_then(|res| res.path_reference_kind(expr_id));
                if let Some(kind) = kind {
                    match kind {
                        PathReferenceKind::EnumVariant => {
                            if let Some(variant_symbol) =
                                resolution.and_then(|res| res.path_reference_symbol(expr_id))
                            {
                                let val = self.get_enum_variant_value(variant_symbol);
                                return Operand::Constant(Constant::Int32(val as i32));
                            }
                        }
                        PathReferenceKind::ChoiceVariant => {
                            if let Some((variant_name, owner_type, _payload_types)) =
                                self.get_choice_variant_payload(expr_id)
                            {
                                let choice_temp = self.declare_local(None, owner_type);
                                self.current_instructions.push((
                                    Instruction::Assign(
                                        choice_temp,
                                        RValue::Choice(owner_type, variant_name, None),
                                    ),
                                    None,
                                ));
                                return Operand::Local(choice_temp);
                            }
                        }
                        _ => {}
                    }
                }
                Operand::Constant(Constant::Null)
            }

            SyntaxNodeKind::StructLiteral | SyntaxNodeKind::InferredStructLiteral => {
                self.lower_struct_literal(expr_id, node)
            }

            SyntaxNodeKind::ArrayLiteral => self.lower_array_literal(expr_id, node),

            SyntaxNodeKind::TupleExpression => self.lower_tuple_literal(expr_id, node),

            SyntaxNodeKind::MemberExpression | SyntaxNodeKind::NullSafeMemberExpression => {
                let obj_node = node.child(0).unwrap();
                let member_node = node.child(1).unwrap();
                let member_name = self.builder.node_text(member_node).to_string();

                let obj_operand = self.lower_expression(obj_node);

                let ty = self.node_type(expr_id).unwrap_or_else(|| TypeId::new(0));

                let temp_id = self.declare_local(None, ty);
                let obj_ty = self.node_type(obj_node).unwrap_or_else(|| TypeId::new(0));

                let resolved_obj_ty = if let Operand::Local(l) = obj_operand {
                    self.locals
                        .iter()
                        .find(|decl| decl.id == l)
                        .map(|decl| decl.ty)
                        .unwrap_or(obj_ty)
                } else {
                    obj_ty
                };

                let resolved_obj_ty = self.builder.resolve_alias_type(resolved_obj_ty);

                let is_array_length = member_name == "length"
                    && matches!(
                        self.builder
                            .type_result
                            .layer()
                            .table()
                            .kind(resolved_obj_ty),
                        Some(TypeKind::Array { .. })
                    );

                let rval = if is_array_length {
                    RValue::Len(obj_operand)
                } else {
                    RValue::MemberAccess(obj_operand, member_name)
                };

                self.current_instructions
                    .push((Instruction::Assign(temp_id, rval), None));
                Operand::Local(temp_id)
            }

            SyntaxNodeKind::IndexExpression => {
                let target_node = node.child(0).unwrap();
                let index_node = node.child(1).unwrap();

                let target_operand = self.lower_expression(target_node);
                let index_operand = self.lower_expression(index_node);

                let ty = self.node_type(expr_id).unwrap_or_else(|| TypeId::new(0));

                let target_ty = self
                    .node_type(target_node)
                    .unwrap_or_else(|| TypeId::new(0));

                let resolved_target = self.resolve_alias_type(target_ty);

                let is_tuple = matches!(
                    self.builder
                        .type_result
                        .layer()
                        .table()
                        .kind(resolved_target),
                    Some(TypeKind::Tuple { .. })
                );

                let temp_id = self.declare_local(None, ty);

                if is_tuple {
                    let index_str = match index_operand {
                        Operand::Constant(Constant::Int8(val)) => val.to_string(),
                        Operand::Constant(Constant::Int16(val)) => val.to_string(),
                        Operand::Constant(Constant::Int32(val)) => val.to_string(),
                        Operand::Constant(Constant::Int64(val)) => val.to_string(),
                        Operand::Constant(Constant::Uint8(val)) => val.to_string(),
                        Operand::Constant(Constant::Uint16(val)) => val.to_string(),
                        Operand::Constant(Constant::Uint32(val)) => val.to_string(),
                        Operand::Constant(Constant::Uint64(val)) => val.to_string(),
                        _ => "0".to_string(),
                    };
                    self.current_instructions.push((
                        Instruction::Assign(
                            temp_id,
                            RValue::MemberAccess(target_operand, index_str),
                        ),
                        None,
                    ));
                } else {
                    self.current_instructions.push((
                        Instruction::Assign(
                            temp_id,
                            RValue::ArrayIndex(target_operand, index_operand),
                        ),
                        None,
                    ));
                }
                Operand::Local(temp_id)
            }

            SyntaxNodeKind::MatchExpression | SyntaxNodeKind::InstanceofExpression => {
                let subject_node = node.child(0).unwrap();
                let arms_node = node.child(1).unwrap();

                let subject_type = self
                    .node_type(subject_node)
                    .unwrap_or_else(|| TypeId::new(0));

                let match_type = self.node_type(expr_id).unwrap_or_else(|| TypeId::new(0));

                let subject_op = self.lower_expression(subject_node);

                let subject_temp = self.declare_local(None, subject_type);
                self.current_instructions.push((
                    Instruction::Assign(subject_temp, RValue::Use(subject_op)),
                    None,
                ));
                let subject_local_op = Operand::Local(subject_temp);

                let match_result = self.declare_local(None, match_type);

                let arms_syntax_node = syntax.node(arms_node).unwrap();
                let arm_nodes = arms_syntax_node.children().to_vec();
                let match_end = self.builder.next_block();
                let mut next_condition_block = None;

                for arm_node in arm_nodes {
                    if let Some(block) = next_condition_block {
                        self.blocks.last_mut().unwrap().id = block;
                        self.current_block = block;
                    }

                    let pattern_node = syntax.child(arm_node, 0).unwrap();
                    let body_node = syntax
                        .child(arm_node, 1)
                        .and_then(|body| syntax.child(body, 0))
                        .unwrap();

                    let arm_body_block = self.builder.next_block();
                    let next_arm_block = self.builder.next_block();

                    self.lower_pattern_check(
                        pattern_node,
                        &subject_local_op,
                        arm_body_block,
                        next_arm_block,
                    );

                    self.blocks.last_mut().unwrap().id = arm_body_block;
                    self.current_block = arm_body_block;

                    let body_op = if syntax
                        .node(body_node)
                        .is_some_and(|n| n.kind() == SyntaxNodeKind::Block)
                    {
                        self.lower_block(body_node);
                        Operand::Constant(Constant::Null)
                    } else {
                        self.lower_expression(body_node)
                    };

                    self.current_instructions.push((
                        Instruction::Assign(match_result, RValue::Use(body_op)),
                        None,
                    ));

                    if !self.is_terminated() {
                        self.terminate_block(Terminator::Jump {
                            target: match_end,
                            args: Vec::new(),
                        });
                    }

                    next_condition_block = Some(next_arm_block);
                }

                if let Some(block) = next_condition_block {
                    self.blocks.last_mut().unwrap().id = block;
                    self.current_block = block;
                    self.terminate_block(Terminator::Panic(
                        "non-exhaustive match expression".to_string(),
                    ));
                }

                self.blocks.last_mut().unwrap().id = match_end;
                self.current_block = match_end;

                Operand::Local(match_result)
            }

            SyntaxNodeKind::NewArrayExpression => {
                self.lower_new_array_expression(expr_id, node, /* dummy */ &[])
            }
            kind if kind.is_function_expression() => {
                let ty = self
                    .builder
                    .type_result
                    .layer()
                    .node_type(expr_id)
                    .unwrap_or_else(|| galfus_core::TypeId::new(0));

                let caller_next_local = self.builder.next_local_id;
                let caller_next_block = self.builder.next_block_id;
                if let Some(func) = self.builder.build_function_expression(expr_id, ty) {
                    let func_id = func.id;
                    self.builder.specialized_functions.push(func);
                    self.builder.next_local_id = caller_next_local;
                    self.builder.next_block_id = caller_next_block;
                    Operand::Constant(Constant::Function(func_id))
                } else {
                    self.builder.next_local_id = caller_next_local;
                    self.builder.next_block_id = caller_next_block;
                    Operand::Constant(Constant::Null)
                }
            }

            SyntaxNodeKind::AwaitExpression => {
                let target_node = node.child(0).unwrap();
                let future_op = self.lower_expression(target_node);
                let ty = self.node_type(expr_id).unwrap_or_else(|| TypeId::new(0));
                let temp_id = self.declare_local(None, ty);

                self.current_instructions.push((
                    Instruction::Await {
                        future: future_op,
                        destination: temp_id,
                    },
                    None,
                ));

                Operand::Local(temp_id)
            }

            SyntaxNodeKind::AwaitAllExpression => {
                let target_id = node.child(0).unwrap();
                let futures = self.lower_await_futures_list(target_id);
                let ty = self.node_type(expr_id).unwrap_or_else(|| TypeId::new(0));
                let temp_id = self.declare_local(None, ty);

                self.current_instructions.push((
                    Instruction::AwaitAll {
                        futures,
                        destination: temp_id,
                    },
                    None,
                ));

                Operand::Local(temp_id)
            }

            SyntaxNodeKind::AwaitRaceExpression => {
                let target_id = node.child(0).unwrap();
                let futures = self.lower_await_futures_list(target_id);
                let ty = self.node_type(expr_id).unwrap_or_else(|| TypeId::new(0));
                let temp_id = self.declare_local(None, ty);

                self.current_instructions.push((
                    Instruction::AwaitRace {
                        futures,
                        destination: temp_id,
                    },
                    None,
                ));

                Operand::Local(temp_id)
            }

            _ => Operand::Constant(Constant::Null),
        }
    }

    fn call_target_symbol(&self, target: NodeId) -> Option<SymbolId> {
        let syntax = self.builder.graph.syntax();
        let resolution = self.builder.graph.resolution()?;
        let node = syntax.node(target)?;

        match node.kind() {
            SyntaxNodeKind::NameExpression => resolution.reference_symbol(target).or_else(|| {
                let ident = syntax.first_child_of_kind(target, SyntaxNodeKind::Identifier)?;
                resolution.reference_symbol(ident)
            }),
            SyntaxNodeKind::PathExpression => resolution
                .path_reference_symbol(target)
                .or_else(|| resolution.reference_symbol(target)),
            SyntaxNodeKind::GenericExpression => syntax
                .child(target, 0)
                .and_then(|inner| self.call_target_symbol(inner)),
            _ => None,
        }
    }

    fn typeof_subject_type(&self, subject: NodeId) -> Option<TypeId> {
        let syntax = self.builder.graph.syntax();
        let resolution = self.builder.graph.resolution()?;

        let generic_parameter_type = resolution
            .reference_symbol(subject)
            .or_else(|| {
                syntax
                    .first_child_of_kind(subject, SyntaxNodeKind::Identifier)
                    .and_then(|identifier| resolution.reference_symbol(identifier))
            })
            .and_then(|symbol| self.builder.type_result.layer().symbol_type(symbol))
            .filter(|ty| {
                matches!(
                    self.builder.type_result.layer().table().kind(*ty),
                    Some(TypeKind::GenericParameter { .. })
                )
            })
            .map(|ty| self.substitute_type(ty));

        generic_parameter_type.or_else(|| self.node_type(subject))
    }

    fn function_id_for_symbol(&self, symbol: SymbolId, target: NodeId) -> FunctionId {
        let is_import = self
            .builder
            .graph
            .resolution()
            .and_then(|resolution| resolution.import_for_symbol(symbol))
            .is_some();

        if is_import {
            path_call_function_id(target)
        } else {
            FunctionId::new(symbol.raw())
        }
    }

    fn anchored_call_receiver(&self, target: NodeId) -> Option<NodeId> {
        if self.is_choice_variant_call_target(target) {
            return None;
        }

        let syntax = self.builder.graph.syntax();
        let node = syntax.node(target)?;
        if node.kind() != SyntaxNodeKind::PathExpression {
            return None;
        }

        let receiver = node.child(0)?;
        let receiver_kind = syntax.node(receiver)?.kind();
        if matches!(
            receiver_kind,
            SyntaxNodeKind::Identifier | SyntaxNodeKind::Path | SyntaxNodeKind::GenericExpression
        ) {
            None
        } else if receiver_kind == SyntaxNodeKind::NameExpression {
            // Check if it's a namespace or struct type. If so, it's a static call.
            if let Some(res) = self.builder.graph.resolution()
                && let Some(sym) = res.reference_symbol(receiver)
                && let Some(sym_data) = res.symbol(sym)
                && matches!(
                    sym_data.kind(),
                    SymbolKind::ImportNamespace | SymbolKind::Struct
                )
            {
                None
            } else {
                Some(receiver)
            }
        } else {
            Some(receiver)
        }
    }

    fn anchored_function_symbol(&self, receiver: NodeId, target: NodeId) -> Option<SymbolId> {
        let syntax = self.builder.graph.syntax();
        let resolution = self.builder.graph.resolution()?;
        let member = syntax.child(target, 1)?;
        let member_name = self.builder.node_text(member);

        let receiver_ty = self.node_type(receiver)?;
        let receiver_ty = self.builder.resolve_alias_type(receiver_ty);
        let TypeKind::Named { symbol } =
            self.builder.type_result.layer().table().kind(receiver_ty)?
        else {
            return None;
        };

        let receiver_symbol = resolution.symbol(*symbol)?;
        if receiver_symbol.kind() != SymbolKind::Struct {
            return None;
        }

        let receiver_name = self.builder.string_table.resolve(receiver_symbol.name())?;
        let function_name = format!("{receiver_name}::{member_name}");
        resolution
            .symbols()
            .iter()
            .find(|symbol| {
                symbol.kind() == SymbolKind::Function
                    && self
                        .builder
                        .string_table
                        .resolve(symbol.name())
                        .unwrap_or("")
                        == function_name.as_str()
            })
            .map(|symbol| symbol.id())
    }

    fn specialize_generic_call(
        &mut self,
        symbol: SymbolId,
        target_node: NodeId,
        arg_types: &[TypeId],
    ) -> Option<FunctionId> {
        let original_id = FunctionId::new(symbol.raw());
        let function_item = self.builder.function_item_for_symbol(symbol);

        if let Some(function_item) = function_item {
            let generic_params = self
                .builder
                .generic_parameters_for_function_item(function_item);

            if generic_params.is_empty() {
                return None;
            }

            let concrete_types =
                self.concrete_generic_arguments(target_node, &generic_params, arg_types)?;
            if concrete_types.len() != generic_params.len() {
                return None;
            }

            let key = (original_id, concrete_types.clone());

            if let Some(func_id) = self.builder.specialisations.get(&key).copied() {
                return Some(func_id);
            }

            if self.builder.active_specialisations.contains(&key) {
                return Some(original_id);
            }

            let specialized_id = self.builder.next_specialized_function_id();
            self.builder
                .specialisations
                .insert(key.clone(), specialized_id);
            self.builder.active_specialisations.insert(key.clone());

            let substitutions = generic_params
                .into_iter()
                .zip(concrete_types)
                .collect::<HashMap<_, _>>();

            let caller_next_local = self.builder.next_local_id;
            let caller_next_block = self.builder.next_block_id;
            if let Some(mut function) = self.builder.build_function_with_substitutions(
                function_item,
                Some(specialized_id),
                substitutions,
            ) {
                function.name = format!("{}#{}", function.name, specialized_id.raw());
                self.builder.specialized_functions.push(function);
            }
            self.builder.next_local_id = caller_next_local;
            self.builder.next_block_id = caller_next_block;

            self.builder.active_specialisations.remove(&key);

            Some(specialized_id)
        } else if let Some(ctx_ptr) = self.builder.workspace_ctx {
            let caller_module_id = self.builder.workspace_module_id?;
            let ctx = unsafe { &mut *ctx_ptr };
            if let Some((target_mod_idx, target_symbol)) =
                ctx.resolve_import(caller_module_id, target_node)
            {
                if let Some(generic_params) = ctx.get_generic_params(target_mod_idx, target_symbol)
                {
                    if generic_params.is_empty() {
                        return None;
                    }

                    let concrete_types = self
                        .concrete_generic_arguments(target_node, &generic_params, arg_types)
                        .or_else(|| {
                            ctx.infer_imported_generic_arguments(
                                caller_module_id,
                                target_mod_idx,
                                target_symbol,
                                &generic_params,
                                arg_types,
                            )
                        })?;
                    if concrete_types.len() != generic_params.len() {
                        return None;
                    }

                    let substitutions = generic_params
                        .into_iter()
                        .zip(concrete_types.clone())
                        .collect::<HashMap<_, _>>();

                    let specialized_id = ctx.specialize_function(
                        caller_module_id,
                        target_node,
                        target_mod_idx,
                        target_symbol,
                        concrete_types,
                        substitutions,
                    );
                    Some(specialized_id)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    }

    fn concrete_generic_arguments(
        &self,
        target_node: NodeId,
        generic_params: &[SymbolId],
        arg_types: &[TypeId],
    ) -> Option<Vec<TypeId>> {
        let syntax = self.builder.graph.syntax();

        if syntax
            .node(target_node)
            .is_some_and(|node| node.kind() == SyntaxNodeKind::GenericExpression)
            && let Some(argument_list) = syntax.child(target_node, 1)
            && let Some(argument_node) = syntax.node(argument_list)
        {
            let explicit = argument_node
                .children()
                .iter()
                .filter_map(|argument| {
                    self.node_type(*argument).or_else(|| {
                        self.first_type_child(*argument)
                            .and_then(|type_node| self.node_type(type_node))
                    })
                })
                .collect::<Vec<_>>();

            if !explicit.is_empty() {
                return Some(explicit);
            }
        }

        self.infer_generic_arguments_from_call(target_node, generic_params, arg_types)
    }

    fn infer_generic_arguments_from_call(
        &self,
        target_node: NodeId,
        generic_params: &[SymbolId],
        arg_types: &[TypeId],
    ) -> Option<Vec<TypeId>> {
        let target_ty = self.node_type(target_node)?;
        let target_ty = self.builder.resolve_alias_type(target_ty);
        let TypeKind::Function(function) =
            self.builder.type_result.layer().table().kind(target_ty)?
        else {
            return None;
        };

        let mut substitutions = HashMap::new();

        for (parameter, &arg_ty) in function.parameters().iter().zip(arg_types.iter()) {
            self.infer_generic_argument_from_types(
                generic_params,
                parameter.ty(),
                arg_ty,
                &mut substitutions,
            );
        }

        generic_params
            .iter()
            .map(|param| substitutions.get(param).copied())
            .collect()
    }

    fn infer_generic_argument_from_types(
        &self,
        generic_params: &[SymbolId],
        parameter_ty: TypeId,
        argument_ty: TypeId,
        substitutions: &mut HashMap<SymbolId, TypeId>,
    ) {
        let parameter_ty = self.builder.resolve_alias_type(parameter_ty);
        let argument_ty = self.builder.resolve_alias_type(argument_ty);

        match self.builder.type_result.layer().table().kind(parameter_ty) {
            Some(TypeKind::GenericParameter { symbol }) if generic_params.contains(symbol) => {
                substitutions.entry(*symbol).or_insert(argument_ty);
            }
            Some(TypeKind::Array { element }) => {
                if let Some(TypeKind::Array {
                    element: arg_element,
                }) = self.builder.type_result.layer().table().kind(argument_ty)
                {
                    self.infer_generic_argument_from_types(
                        generic_params,
                        *element,
                        *arg_element,
                        substitutions,
                    );
                }
            }
            Some(TypeKind::Tuple { elements }) => {
                if let Some(TypeKind::Tuple {
                    elements: arg_elements,
                }) = self.builder.type_result.layer().table().kind(argument_ty)
                {
                    for (element, arg_element) in elements.iter().zip(arg_elements.iter()) {
                        self.infer_generic_argument_from_types(
                            generic_params,
                            *element,
                            *arg_element,
                            substitutions,
                        );
                    }
                }
            }
            _ => {}
        }
    }

    fn lower_await_futures_list(&mut self, target_id: NodeId) -> Vec<Operand> {
        let syntax = self.builder.graph.syntax();
        let mut futures = Vec::new();

        let inner_id = if let Some(target_node) = syntax.node(target_id) {
            if target_node.kind() == SyntaxNodeKind::GroupedExpression {
                target_node.first_child().unwrap_or(target_id)
            } else {
                target_id
            }
        } else {
            target_id
        };

        if let Some(inner_node) = syntax.node(inner_id) {
            if inner_node.kind() == SyntaxNodeKind::TupleExpression {
                for &child_id in inner_node.children() {
                    futures.push(self.lower_expression(child_id));
                }
            } else {
                futures.push(self.lower_expression(inner_id));
            }
        } else {
            futures.push(self.lower_expression(target_id));
        }

        futures
    }
}

const PATH_CALL_TARGET_TAG: u32 = 0x8000_0000;

fn path_call_function_id(node: NodeId) -> FunctionId {
    FunctionId::new(PATH_CALL_TARGET_TAG | node.raw())
}

pub(crate) fn unescape_string(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
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
        } else {
            result.push(c);
        }
    }
    result
}
