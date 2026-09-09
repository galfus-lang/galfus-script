use super::function::FunctionBuilder;
use super::function_helpers::parse_int;
use galfus_core::{NodeId, TypeId};
use galfus_frontend::{
    PathReferenceKind, RangeDesugarTarget, SymbolKind, SyntaxNodeKind, TypeKind,
};
use galfus_ir::mir::*;

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
                let Some(ctx) = self.builder.workspace_ctx.as_deref_mut() else {
                    return Operand::Constant(Constant::Null);
                };
                let Some(caller_module_id) = self.builder.workspace_module_id else {
                    return Operand::Constant(Constant::Null);
                };
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

            SyntaxNodeKind::TypeofExpression => self.lower_typeof_expression(expr_id),

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
                                    | Some(galfus_frontend::SymbolKind::ImportBinding)
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
                let lowered_arguments = self.lower_call_arguments(target_node, arg_list_node);
                self.lower_call_dispatch(
                    expr_id,
                    target_node,
                    lowered_arguments.arguments,
                    lowered_arguments.argument_types,
                    lowered_arguments.anchored_receiver,
                )
            }
            SyntaxNodeKind::PathExpression => {
                if let Some((variant_name, owner_type, _payload_types)) =
                    self.get_choice_variant_payload(expr_id)
                {
                    let expr_type = self
                        .builder
                        .type_result
                        .layer()
                        .node_type(expr_id)
                        .unwrap_or(owner_type);
                    let choice_temp = self.declare_local(None, expr_type);
                    self.current_instructions.push((
                        Instruction::Assign(
                            choice_temp,
                            RValue::Choice(expr_type, variant_name, None),
                        ),
                        None,
                    ));
                    return Operand::Local(choice_temp);
                }

                if let Some(PathReferenceKind::EnumVariant) =
                    resolution.and_then(|res| res.path_reference_kind(expr_id))
                    && let Some(variant_symbol) =
                        resolution.and_then(|res| res.path_reference_symbol(expr_id))
                {
                    let val = self.get_enum_variant_value(variant_symbol);
                    return Operand::Constant(Constant::Int32(val as i32));
                }
                if let Some(variant_symbol) =
                    resolution.and_then(|resolution| resolution.path_reference_symbol(expr_id))
                    && self
                        .owner_symbol_for_member(variant_symbol, SymbolKind::Enum)
                        .is_some()
                {
                    let value = self.get_enum_variant_value(variant_symbol);
                    return Operand::Constant(Constant::Int32(value as i32));
                }
                if let Some(resolution) = resolution
                    && let Some(root) = syntax.child(expr_id, 0)
                    && let Some(owner_symbol) = resolution.reference_symbol(root)
                    && let Some(member) = syntax.child(expr_id, 1)
                    && let Some(values) = self
                        .builder
                        .type_result
                        .imported_symbol_enum_values
                        .get(&owner_symbol)
                    && let Some((_, value)) = values
                        .iter()
                        .find(|(name, _)| name == self.builder.node_text(member))
                {
                    return Operand::Constant(Constant::Int32(*value as i32));
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
                self.lower_match_expression(expr_id)
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
                let drop_future = syntax
                    .node(target_node)
                    .is_some_and(|target| target.kind() == SyntaxNodeKind::CallExpression);

                self.current_instructions.push((
                    Instruction::Await {
                        future: future_op,
                        destination: temp_id,
                        drop_future,
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
