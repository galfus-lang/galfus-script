use std::collections;
use std::mem;

use galfus_ir::mir;

use super::MirBuilder;
use galfus_core::{NodeId, SymbolId, TypeId};
use galfus_frontend::{SymbolKind, SyntaxNodeKind, TypeKind};
use galfus_ir::mir::*;
use std::collections::HashMap;

pub struct FunctionBuilder<'b, 'a> {
    pub(super) builder: &'b mut MirBuilder<'a>,
    pub(super) locals: Vec<LocalDecl>,
    pub(super) symbol_to_local: collections::HashMap<SymbolId, LocalId>,
    pub(super) current_instructions: Vec<(Instruction, Option<galfus_core::Span>)>,
    pub(super) blocks: Vec<BasicBlock>,
    pub(super) current_block: BlockId,
    pub(super) scopes: Vec<Vec<LocalId>>,
    pub(super) return_type: TypeId,
    pub(super) type_substitutions: HashMap<SymbolId, TypeId>,
    pub(super) loop_targets: Vec<LoopTargets>,
}

pub(super) struct LoopTargets {
    pub(super) name: Option<String>,
    pub(super) break_target: BlockId,
    pub(super) continue_target: BlockId,
    pub(super) scope_depth: usize,
}

impl<'b, 'a> FunctionBuilder<'b, 'a> {
    pub(super) fn is_future_type(&self, ty: TypeId) -> bool {
        let table = self.builder.type_result.layer().table();
        let Some(TypeKind::GenericInstance { base, .. }) =
            table.kind(self.builder.resolve_alias_type(ty))
        else {
            return false;
        };
        match table.kind(*base) {
            Some(TypeKind::Named { symbol }) => self
                .builder
                .graph
                .resolution()
                .and_then(|resolution| resolution.symbol(*symbol))
                .is_some_and(|symbol| {
                    self.builder.string_table.resolve(symbol.name()) == Some("Future")
                }),
            Some(TypeKind::Path { segments, .. }) => {
                segments.last().is_some_and(|segment| segment == "Future")
            }
            _ => false,
        }
    }
    pub(super) fn node_type(&self, node: NodeId) -> Option<TypeId> {
        let ty = self.builder.type_result.layer().node_type(node);

        ty.map(|ty| self.substitute_type(ty))
    }

    pub(super) fn symbol_type(&self, symbol: SymbolId) -> Option<TypeId> {
        self.builder
            .type_result
            .layer()
            .symbol_type(symbol)
            .map(|ty| self.substitute_type(ty))
    }

    pub(super) fn substitute_type(&self, ty: TypeId) -> TypeId {
        let ty = self.builder.resolve_alias_type(ty);

        match self.builder.type_result.layer().table().kind(ty) {
            Some(TypeKind::GenericParameter { symbol }) => {
                self.type_substitutions.get(symbol).copied().unwrap_or(ty)
            }
            _ => ty,
        }
    }

    pub(super) fn declare_local(&mut self, symbol: Option<SymbolId>, ty: TypeId) -> LocalId {
        let local_id = self.builder.next_local();
        self.locals.push(LocalDecl {
            id: local_id,
            ty,
            is_owned: self.builder.is_owned_type(ty),
        });
        if let Some(sym) = symbol {
            self.symbol_to_local.insert(sym, local_id);
        }
        if let Some(current_scope) = self.scopes.last_mut() {
            current_scope.push(local_id);
        }
        local_id
    }

    pub(super) fn collect_declaration_symbols(&self, node_id: NodeId) -> Vec<SymbolId> {
        let mut symbols = Vec::new();
        self.collect_symbols_recursive(node_id, &mut symbols);
        symbols
    }

    pub(super) fn collect_symbols_recursive(&self, node_id: NodeId, symbols: &mut Vec<SymbolId>) {
        if let Some(sym) = self
            .builder
            .graph
            .resolution()
            .and_then(|res| res.declaration_symbol(node_id))
        {
            symbols.push(sym);
        }
        if let Some(node) = self.builder.graph.syntax().node(node_id) {
            for &child in node.children() {
                self.collect_symbols_recursive(child, symbols);
            }
        }
    }

    pub(super) fn is_terminated(&self) -> bool {
        let block_idx = self
            .blocks
            .iter()
            .position(|b| b.id == self.current_block)
            .unwrap();
        !matches!(
            self.blocks[block_idx].terminator.0,
            Terminator::Return(None)
        )
    }

    pub(super) fn lower_block(&mut self, block_node_id: NodeId) {
        let syntax = self.builder.graph.syntax();
        let Some(block_node) = syntax.node(block_node_id) else {
            self.flush_current_instructions();
            if !self.is_terminated() {
                let current_idx = self
                    .blocks
                    .iter()
                    .position(|b| b.id == self.current_block)
                    .unwrap();
                self.blocks[current_idx].terminator = (Terminator::Return(None), None);
            }
            return;
        };

        self.scopes.push(Vec::new());

        for &stmt_id in block_node.children() {
            self.lower_statement(stmt_id);
        }

        if let Some(scope_locals) = self.scopes.pop()
            && !self.is_terminated()
        {
            for local_id in scope_locals {
                if let Some(decl) = self.locals.iter().find(|l| l.id == local_id)
                    && self.builder.is_owned_type(decl.ty)
                {
                    self.current_instructions
                        .push((Instruction::Drop(local_id), None));
                }
            }
        }
    }

    pub(super) fn flush_current_instructions(&mut self) {
        let instructions = mem::take(&mut self.current_instructions);
        let block_idx = self
            .blocks
            .iter()
            .position(|b| b.id == self.current_block)
            .unwrap();
        self.blocks[block_idx].instructions.extend(instructions);
    }

    pub(super) fn terminate_block(&mut self, terminator: Terminator) {
        self.flush_current_instructions();
        let block_idx = self
            .blocks
            .iter()
            .position(|b| b.id == self.current_block)
            .unwrap();
        self.blocks[block_idx].terminator = (terminator, None);
        let id = self.builder.next_block();
        let bb = BasicBlock {
            id,
            parameters: Vec::new(),
            instructions: Vec::new(),
            terminator: (Terminator::Return(None), None),
        };
        self.blocks.push(bb);
        self.current_block = id;
    }

    fn loop_target_name(&self, loop_node: NodeId) -> Option<String> {
        let syntax = self.builder.graph.syntax();
        let metadata =
            syntax.first_child_of_kind(loop_node, SyntaxNodeKind::KeywordMetadataList)?;

        for item in syntax.node(metadata)?.children() {
            let pair = syntax.node(*item)?;
            if pair.kind() != SyntaxNodeKind::KeywordMetadataPair {
                continue;
            }

            let key = pair.child(0)?;
            if self.builder.node_text(key) == "name" {
                return pair
                    .child(1)
                    .map(|value| self.builder.node_text(value).to_string());
            }
        }

        None
    }

    fn loop_target_for(&self, statement: NodeId) -> Option<&LoopTargets> {
        let syntax = self.builder.graph.syntax();
        let label = syntax
            .first_child_of_kind(statement, SyntaxNodeKind::Identifier)
            .map(|node| self.builder.node_text(node));

        self.loop_targets.iter().rev().find(|target| match label {
            Some(label) => target.name.as_deref() == Some(label),
            None => true,
        })
    }

    fn index_assignment_value_type(&self, target: NodeId) -> Option<TypeId> {
        let syntax = self.builder.graph.syntax();
        let target_node = syntax.node(target)?;

        if target_node.kind() != SyntaxNodeKind::IndexExpression {
            return None;
        }

        let array_node = target_node.child(0)?;
        let array_type = self.node_type(array_node)?;
        let resolved_array_type = self.builder.resolve_alias_type(array_type);

        match self
            .builder
            .type_result
            .layer()
            .table()
            .kind(resolved_array_type)
        {
            Some(TypeKind::Array { element }) => Some(*element),
            _ => self.node_type(target),
        }
    }

    fn lower_index_assignment(
        &mut self,
        target: NodeId,
        value: NodeId,
        binary_op: Option<MirBinaryOp>,
    ) -> bool {
        let syntax = self.builder.graph.syntax();

        let Some(target_node) = syntax.node(target) else {
            return false;
        };

        if target_node.kind() != SyntaxNodeKind::IndexExpression {
            return false;
        }

        let Some(array_node) = target_node.child(0) else {
            return true;
        };

        let Some(index_node) = target_node.child(1) else {
            return true;
        };

        let array_operand = self.lower_expression(array_node);
        let index_operand = self.lower_expression(index_node);
        let value_type = self.node_type(value).unwrap_or_else(|| TypeId::new(0));

        let expected_value_type = self
            .index_assignment_value_type(target)
            .unwrap_or_else(|| TypeId::new(0));

        let value_operand = self.lower_expression(value);
        let value_operand =
            self.insert_cast_if_needed(value_operand, value_type, expected_value_type);
        let assigned_value = if let Some(binary_op) = binary_op {
            let current = self.declare_local(None, expected_value_type);
            self.current_instructions.push((
                Instruction::Assign(
                    current,
                    RValue::ArrayIndex(array_operand.clone(), index_operand.clone()),
                ),
                None,
            ));

            let result = self.declare_local(None, expected_value_type);
            self.current_instructions.push((
                Instruction::Assign(
                    result,
                    RValue::BinaryOp(binary_op, Operand::Local(current), value_operand),
                ),
                None,
            ));
            Operand::Local(result)
        } else {
            value_operand
        };

        self.current_instructions.push((
            Instruction::StoreIndex {
                arr: array_operand,
                idx: index_operand,
                val: assigned_value,
            },
            None,
        ));

        true
    }

    fn lower_member_assignment(
        &mut self,
        target: NodeId,
        value: NodeId,
        binary_op: Option<MirBinaryOp>,
    ) -> bool {
        let syntax = self.builder.graph.syntax();
        let Some(target_node) = syntax.node(target) else {
            return false;
        };
        if target_node.kind() != SyntaxNodeKind::MemberExpression {
            return false;
        }

        let Some(obj_node) = target_node.child(0) else {
            return false;
        };
        let Some(member_node) = target_node.child(1) else {
            return false;
        };

        let obj_operand = self.lower_expression(obj_node);
        let target_ty = self.node_type(target).unwrap_or_else(|| TypeId::new(0));
        let value_ty = self.node_type(value).unwrap_or_else(|| TypeId::new(0));
        let value_operand = self.lower_expression(value);
        let value_operand = self.insert_cast_if_needed(value_operand, value_ty, target_ty);
        let field_name = self.builder.node_text(member_node).to_string();
        let assigned_value = if let Some(binary_op) = binary_op {
            let current = self.declare_local(None, target_ty);
            self.current_instructions.push((
                Instruction::Assign(
                    current,
                    RValue::MemberAccess(obj_operand.clone(), field_name.clone()),
                ),
                None,
            ));

            let result = self.declare_local(None, target_ty);
            self.current_instructions.push((
                Instruction::Assign(
                    result,
                    RValue::BinaryOp(binary_op, Operand::Local(current), value_operand),
                ),
                None,
            ));
            Operand::Local(result)
        } else {
            value_operand
        };

        self.current_instructions.push((
            Instruction::StoreField {
                obj: obj_operand,
                field_name,
                val: assigned_value,
            },
            None,
        ));

        true
    }

    fn emit_control_flow_exit(
        &mut self,
        target_scope_depth: usize,
        ret_local: Option<mir::LocalId>,
    ) {
        for i in (target_scope_depth..self.scopes.len()).rev() {
            for &local_id in self.scopes[i].iter().rev() {
                if Some(local_id) == ret_local {
                    continue;
                }
                if let Some(decl) = self.locals.iter().find(|l| l.id == local_id)
                    && self.builder.is_owned_type(decl.ty)
                {
                    self.current_instructions
                        .push((Instruction::Drop(local_id), None));
                }
            }
        }
    }

    pub(super) fn lower_statement(&mut self, stmt_id: NodeId) {
        let syntax = self.builder.graph.syntax();
        let Some(node) = syntax.node(stmt_id) else {
            return;
        };
        let resolution = self.builder.graph.resolution();

        match node.kind() {
            SyntaxNodeKind::VarStatement | SyntaxNodeKind::ConstStatement => {
                if let Some(binding) =
                    syntax.first_child_of_kind(stmt_id, SyntaxNodeKind::BindingPattern)
                {
                    let initializer = syntax
                        .first_child_of_kind(stmt_id, SyntaxNodeKind::Initializer)
                        .and_then(|init| syntax.first_child(init));
                    let operand = if let Some(init_expr) = initializer {
                        self.lower_expression(init_expr)
                    } else {
                        Operand::Constant(Constant::Null)
                    };

                    self.lower_destructuring_binding(binding, operand);
                }
            }
            SyntaxNodeKind::AssignmentStatement => {
                let target = node.child(0);
                let operator = node.child(1);
                let value = node.child(2);
                if let (Some(target), Some(operator), Some(value)) = (target, operator, value) {
                    let binary_op = self.lower_assignment_binary_op(operator);
                    if self.lower_index_assignment(target, value, binary_op) {
                        return;
                    }
                    if self.lower_member_assignment(target, value, binary_op) {
                        return;
                    }
                    let operand = self.lower_expression(value);
                    let target_ty = self.node_type(target).unwrap_or_else(|| TypeId::new(0));
                    let value_ty = self.node_type(value).unwrap_or_else(|| TypeId::new(0));
                    let casted_operand = self.insert_cast_if_needed(operand, value_ty, target_ty);
                    if syntax
                        .node(target)
                        .is_some_and(|n| n.kind() == SyntaxNodeKind::NameExpression)
                    {
                        let symbol = resolution.and_then(|res| {
                            res.reference_symbol(target).or_else(|| {
                                let ident = syntax
                                    .first_child_of_kind(target, SyntaxNodeKind::Identifier)?;
                                res.reference_symbol(ident)
                            })
                        });
                        if let Some(sym) = symbol {
                            if let Some(local_id) = self.symbol_to_local.get(&sym).copied() {
                                let assigned_value = if let Some(binary_op) = binary_op {
                                    let result = self.declare_local(None, target_ty);
                                    self.current_instructions.push((
                                        Instruction::Assign(
                                            result,
                                            RValue::BinaryOp(
                                                binary_op,
                                                Operand::Local(local_id),
                                                casted_operand,
                                            ),
                                        ),
                                        None,
                                    ));
                                    Operand::Local(result)
                                } else {
                                    casted_operand
                                };
                                self.current_instructions.push((
                                    Instruction::Assign(local_id, RValue::Use(assigned_value)),
                                    None,
                                ));
                            } else {
                                let is_global = resolution.is_some_and(|res| {
                                    matches!(
                                        res.symbol(sym).map(|s| s.kind()),
                                        Some(SymbolKind::Var) | Some(SymbolKind::Const)
                                    )
                                });
                                if is_global {
                                    let name = resolution
                                        .and_then(|res| res.symbol(sym))
                                        .map(|s| {
                                            self.builder
                                                .string_table
                                                .resolve(s.name())
                                                .unwrap_or("")
                                                .to_string()
                                        })
                                        .unwrap_or_default();
                                    self.current_instructions.push((
                                        Instruction::StoreGlobal(name, casted_operand),
                                        None,
                                    ));
                                }
                            }
                        }
                    }
                }
            }
            SyntaxNodeKind::ReturnStatement => {
                let expr = node.first_child();
                let operand = expr.map(|e| {
                    let op = self.lower_expression(e);
                    let expr_ty = self.node_type(e).unwrap_or_else(|| TypeId::new(0));
                    self.insert_cast_if_needed(op, expr_ty, self.return_type)
                });
                let ret_local = match operand.as_ref() {
                    Some(Operand::Local(local_id)) => Some(*local_id),
                    _ => None,
                };
                self.emit_control_flow_exit(0, ret_local);
                self.terminate_block(Terminator::Return(operand));
            }
            SyntaxNodeKind::BreakStatement => {
                let target = self
                    .loop_target_for(stmt_id)
                    .map(|t| (t.scope_depth, t.break_target));
                if let Some((scope_depth, break_target)) = target {
                    self.emit_control_flow_exit(scope_depth, None);
                    self.terminate_block(Terminator::Jump {
                        target: break_target,
                        args: Vec::new(),
                    });
                }
            }
            SyntaxNodeKind::ContinueStatement => {
                let target = self
                    .loop_target_for(stmt_id)
                    .map(|t| (t.scope_depth, t.continue_target));
                if let Some((scope_depth, continue_target)) = target {
                    self.emit_control_flow_exit(scope_depth, None);
                    self.terminate_block(Terminator::Jump {
                        target: continue_target,
                        args: Vec::new(),
                    });
                }
            }

            SyntaxNodeKind::ExpressionStatement => {
                if let Some(expr) = node.first_child() {
                    self.lower_expression(expr);
                }
            }
            SyntaxNodeKind::Block => {
                self.lower_block(stmt_id);
            }
            SyntaxNodeKind::IfStatement => {
                let cond_node = node.child(0).unwrap();
                let then_node = node.child(1).unwrap();
                let else_clause_node = node.child(2);
                let cond = self.lower_expression(cond_node);

                let then_block = self.builder.next_block();
                let else_block = self.builder.next_block();
                let merge_block = self.builder.next_block();

                self.terminate_block(Terminator::Branch {
                    cond,
                    true_block: then_block,
                    true_args: Vec::new(),
                    false_block: if else_clause_node.is_some() {
                        else_block
                    } else {
                        merge_block
                    },
                    false_args: Vec::new(),
                });

                self.blocks.last_mut().unwrap().id = then_block;
                self.current_block = then_block;
                self.lower_block(then_node);
                if !self.is_terminated() {
                    self.terminate_block(Terminator::Jump {
                        target: merge_block,
                        args: Vec::new(),
                    });
                }

                if let Some(else_clause) = else_clause_node {
                    let clause_node = syntax.node(else_clause).unwrap();
                    if let Some(child_node) = clause_node.first_child() {
                        self.blocks.last_mut().unwrap().id = else_block;
                        self.current_block = else_block;
                        if syntax
                            .node(child_node)
                            .is_some_and(|node| node.kind() == SyntaxNodeKind::Block)
                        {
                            self.lower_block(child_node);
                        } else {
                            // `else if` is represented as a nested IfStatement, not a block.
                            self.lower_statement(child_node);
                        }
                        if !self.is_terminated() {
                            self.terminate_block(Terminator::Jump {
                                target: merge_block,
                                args: Vec::new(),
                            });
                        }
                    }
                }

                self.blocks.last_mut().unwrap().id = merge_block;
                self.current_block = merge_block;
            }
            SyntaxNodeKind::LoopStatement => {
                let body_node = syntax
                    .first_child_of_kind(stmt_id, SyntaxNodeKind::Block)
                    .unwrap();
                let condition_node = node.children().iter().copied().find(|&child| {
                    let child_kind = syntax
                        .node(child)
                        .map(|c| c.kind())
                        .unwrap_or(SyntaxNodeKind::SourceFile);
                    child_kind != SyntaxNodeKind::KeywordMetadataList
                        && child_kind != SyntaxNodeKind::Block
                });

                let loop_header = self.builder.next_block();
                let loop_body = self.builder.next_block();
                let loop_end = self.builder.next_block();

                self.terminate_block(Terminator::Jump {
                    target: loop_header,
                    args: Vec::new(),
                });
                self.blocks.last_mut().unwrap().id = loop_header;
                self.current_block = loop_header;

                self.loop_targets.push(LoopTargets {
                    name: self.loop_target_name(stmt_id),
                    break_target: loop_end,
                    continue_target: loop_header,
                    scope_depth: self.scopes.len(),
                });

                if let Some(cond_expr) = condition_node {
                    let cond_operand = self.lower_expression(cond_expr);
                    self.terminate_block(Terminator::Branch {
                        cond: cond_operand,
                        true_block: loop_body,
                        true_args: Vec::new(),
                        false_block: loop_end,
                        false_args: Vec::new(),
                    });
                } else {
                    self.terminate_block(Terminator::Jump {
                        target: loop_body,
                        args: Vec::new(),
                    });
                }

                self.blocks.last_mut().unwrap().id = loop_body;
                self.current_block = loop_body;

                self.lower_block(body_node);
                if !self.is_terminated() {
                    self.terminate_block(Terminator::Jump {
                        target: loop_header,
                        args: Vec::new(),
                    });
                }

                self.loop_targets.pop();

                self.blocks.last_mut().unwrap().id = loop_end;
                self.current_block = loop_end;
            }
            SyntaxNodeKind::ForStatement => {
                let syntax = self.builder.graph.syntax();
                let has_metadata = syntax
                    .node(syntax.child(stmt_id, 0).unwrap())
                    .unwrap()
                    .kind()
                    == SyntaxNodeKind::KeywordMetadataList;
                let offset = if has_metadata { 1 } else { 0 };
                let binding_node = syntax.child(stmt_id, offset).unwrap();
                let iterable_node = syntax.child(stmt_id, offset + 1).unwrap();
                let body_node = syntax.child(stmt_id, offset + 2).unwrap();

                let bool_type_id = self
                    .builder
                    .type_result
                    .layer()
                    .table()
                    .primitive(galfus_frontend::PrimitiveType::Bool);

                let mut iterable_operand = self.lower_expression(iterable_node);
                let iterable_ty = self
                    .node_type(iterable_node)
                    .unwrap_or_else(|| TypeId::new(0));
                let binding_ty = self
                    .collect_declaration_symbols(binding_node)
                    .first()
                    .and_then(|symbol| self.symbol_type(*symbol))
                    .unwrap_or_else(|| TypeId::new(0));

                let is_array_type = matches!(
                    self.builder.type_result.layer().table().kind(iterable_ty),
                    Some(galfus_frontend::TypeKind::Array { .. })
                );

                let mut actual_iterable_ty = iterable_ty;
                if is_array_type {
                    let element_type = match self
                        .builder
                        .type_result
                        .layer()
                        .table()
                        .kind(iterable_ty)
                        .unwrap()
                    {
                        galfus_frontend::TypeKind::Array { element } => element,
                        _ => unreachable!(),
                    };
                    let Some(ctx_ptr) = self.builder.workspace_ctx else {
                        return;
                    };
                    let Some(caller_module_id) = self.builder.workspace_module_id else {
                        return;
                    };
                    let ctx = unsafe { &mut *ctx_ptr };
                    let iter_func = ctx
                        .specialize_builtin_function(
                            caller_module_id,
                            stmt_id,
                            "std/iterable",
                            "arrayIter",
                            vec![*element_type],
                        )
                        .expect("arrayIter not found");

                    let iter_obj_ty = ctx.function_return_type(iter_func).unwrap();
                    let iter_obj_local = self.declare_local(None, iter_obj_ty);

                    self.current_instructions.push((
                        Instruction::Call {
                            func: iter_func,
                            args: vec![iterable_operand],
                            destination: iter_obj_local,
                            is_external: false,
                        },
                        None,
                    ));

                    iterable_operand = Operand::Local(iter_obj_local);
                    actual_iterable_ty = iter_obj_ty;
                }

                let iterable_local = self.declare_local(None, actual_iterable_ty);
                self.current_instructions.push((
                    Instruction::Assign(iterable_local, RValue::Use(iterable_operand)),
                    None,
                ));

                let iterator_local = self.declare_local(None, actual_iterable_ty);
                self.current_instructions.push((
                    Instruction::ConstraintCall {
                        method_name: "iter".to_string(),
                        obj: Operand::Local(iterable_local),
                        args: Vec::new(),
                        destination: iterator_local,
                    },
                    None,
                ));

                let loop_header = self.builder.next_block();
                let loop_body = self.builder.next_block();
                let loop_end = self.builder.next_block();

                self.terminate_block(Terminator::Jump {
                    target: loop_header,
                    args: Vec::new(),
                });
                self.blocks.last_mut().unwrap().id = loop_header;
                self.current_block = loop_header;

                let next_local = self.declare_local(None, binding_ty);
                self.current_instructions.push((
                    Instruction::ConstraintCall {
                        method_name: "next".to_string(),
                        obj: Operand::Local(iterator_local),
                        args: Vec::new(),
                        destination: next_local,
                    },
                    None,
                ));
                let cond_local = self.declare_local(None, bool_type_id);
                self.current_instructions.push((
                    Instruction::Assign(
                        cond_local,
                        RValue::BinaryOp(
                            MirBinaryOp::NotEqual,
                            Operand::Local(next_local),
                            Operand::Constant(Constant::Null),
                        ),
                    ),
                    None,
                ));
                self.terminate_block(Terminator::Branch {
                    cond: Operand::Local(cond_local),
                    true_block: loop_body,
                    true_args: Vec::new(),
                    false_block: loop_end,
                    false_args: Vec::new(),
                });

                self.blocks.last_mut().unwrap().id = loop_body;
                self.current_block = loop_body;
                self.lower_destructuring_binding(binding_node, Operand::Local(next_local));
                self.loop_targets.push(LoopTargets {
                    name: self.loop_target_name(stmt_id),
                    break_target: loop_end,
                    continue_target: loop_header,
                    scope_depth: self.scopes.len(),
                });
                self.lower_block(body_node);
                if !self.is_terminated() {
                    self.terminate_block(Terminator::Jump {
                        target: loop_header,
                        args: Vec::new(),
                    });
                }
                self.loop_targets.pop();

                self.blocks.last_mut().unwrap().id = loop_end;
                self.current_block = loop_end;
            }

            _ => {}
        }
    }
}
