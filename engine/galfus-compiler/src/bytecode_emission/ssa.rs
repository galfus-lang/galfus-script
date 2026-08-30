use std::collections;

use galfus_ir::mir;

use galfus_ir::mir::*;
use std::collections::HashMap;

pub fn convert_to_ssa(func: &mut MirFunction) {
    let mut preds: HashMap<BlockId, Vec<BlockId>> = HashMap::new();
    for block in &func.blocks {
        let succs = match &block.terminator.0 {
            Terminator::Jump { target, .. } => vec![*target],
            Terminator::Branch {
                true_block,
                false_block,
                ..
            } => vec![*true_block, *false_block],
            _ => vec![],
        };
        for s in succs {
            preds.entry(s).or_default().push(block.id);
        }
    }

    let mut new_locals = func.locals.clone();
    let mut current_def: HashMap<(BlockId, LocalId), LocalId> = HashMap::new();
    let mut block_parameters: HashMap<BlockId, Vec<(LocalId, LocalId)>> = HashMap::new();
    let mut phi_operands: HashMap<LocalId, Vec<(BlockId, LocalId)>> = HashMap::new();

    // Seed parameters
    let entry = BlockId::new(0);
    for i in 0..func.parameter_types.len() {
        let id = LocalId::new(i as u32);
        current_def.insert((entry, id), id);
    }

    struct SsaBuilder<'a> {
        preds: &'a HashMap<BlockId, Vec<BlockId>>,
        new_locals: &'a mut Vec<LocalDecl>,
        current_def: &'a mut HashMap<(BlockId, LocalId), LocalId>,
        block_parameters: &'a mut HashMap<BlockId, Vec<(LocalId, LocalId)>>,
        phi_operands: &'a mut HashMap<LocalId, Vec<(BlockId, LocalId)>>,
    }

    impl<'a> SsaBuilder<'a> {
        fn write_variable(&mut self, block: BlockId, variable: LocalId, value: LocalId) {
            self.current_def.insert((block, variable), value);
        }

        fn read_variable(&mut self, block: BlockId, variable: LocalId) -> LocalId {
            if let Some(&val) = self.current_def.get(&(block, variable)) {
                return val;
            }
            let preds = self.preds.get(&block).cloned().unwrap_or_default();
            let val = if preds.is_empty() {
                variable
            } else if preds.len() == 1 {
                self.read_variable(preds[0], variable)
            } else {
                let phi_decl = self
                    .new_locals
                    .iter()
                    .find(|d| d.id == variable)
                    .unwrap()
                    .clone();
                let phi_id = LocalId::new(self.new_locals.len() as u32);
                self.new_locals.push(LocalDecl {
                    id: phi_id,
                    ty: phi_decl.ty,
                    is_owned: phi_decl.is_owned,
                });

                self.block_parameters
                    .entry(block)
                    .or_default()
                    .push((variable, phi_id));
                self.write_variable(block, variable, phi_id);

                let mut ops = Vec::new();
                for pred in preds {
                    ops.push((pred, variable));
                }
                self.phi_operands.insert(phi_id, ops);
                phi_id
            };
            self.write_variable(block, variable, val);
            val
        }

        fn replace_operand(&mut self, block: BlockId, operand: &mut Operand) {
            if let Operand::Local(id) = operand {
                *id = self.read_variable(block, *id);
            }
        }

        fn replace_rvalue(&mut self, block: BlockId, rvalue: &mut RValue) {
            match rvalue {
                RValue::Use(op)
                | RValue::UnaryOp(_, op)
                | RValue::Cast(op, _)
                | RValue::Copy(op)
                | RValue::ChoiceVariantIs(op, _)
                | RValue::ImportedChoiceVariantIs(op, _, _)
                | RValue::Instanceof(op, _)
                | RValue::Len(op) => {
                    self.replace_operand(block, op);
                }
                RValue::BinaryOp(_, op1, op2) | RValue::ArrayIndex(op1, op2) => {
                    self.replace_operand(block, op1);
                    self.replace_operand(block, op2);
                }
                RValue::NewStruct { fields, .. }
                | RValue::NewArray(_, fields)
                | RValue::NewTuple(_, fields) => {
                    for op in fields {
                        self.replace_operand(block, op);
                    }
                }
                RValue::NewArrayDynamic(_, elems) => {
                    for elem in elems {
                        match elem {
                            ArrayLiteralElement::Single(op) | ArrayLiteralElement::Spread(op) => {
                                self.replace_operand(block, op);
                            }
                        }
                    }
                }
                RValue::NewArrayZeroedDynamic { length, .. } => {
                    self.replace_operand(block, length);
                }
                RValue::MemberAccess(op, _) => {
                    self.replace_operand(block, op);
                }
                RValue::Choice(_, _, Some(op)) => {
                    self.replace_operand(block, op);
                }
                _ => {}
            }
        }

        fn replace_instruction(
            &mut self,
            block: BlockId,
            inst: &mut (mir::Instruction, Option<galfus_core::Span>),
        ) {
            match &mut inst.0 {
                Instruction::Assign(target, rvalue) => {
                    self.replace_rvalue(block, rvalue);
                    let orig_target = *target;
                    let target_decl = self
                        .new_locals
                        .iter()
                        .find(|d| d.id == orig_target)
                        .unwrap()
                        .clone();
                    let new_id = LocalId::new(self.new_locals.len() as u32);
                    self.new_locals.push(LocalDecl {
                        id: new_id,
                        ty: target_decl.ty,
                        is_owned: target_decl.is_owned,
                    });
                    *target = new_id;
                    self.write_variable(block, orig_target, new_id);
                }
                Instruction::Drop(id) => {
                    *id = self.read_variable(block, *id);
                }
                Instruction::StoreGlobal(_, op) => {
                    self.replace_operand(block, op);
                }
                Instruction::StoreIndex { arr, idx, val } => {
                    self.replace_operand(block, arr);
                    self.replace_operand(block, idx);
                    self.replace_operand(block, val);
                }
                Instruction::StoreField { obj, val, .. } => {
                    self.replace_operand(block, obj);
                    self.replace_operand(block, val);
                }

                Instruction::Call {
                    args, destination, ..
                } => {
                    for op in args {
                        self.replace_operand(block, op);
                    }
                    let orig_target = *destination;
                    let target_decl = self
                        .new_locals
                        .iter()
                        .find(|d| d.id == orig_target)
                        .unwrap()
                        .clone();
                    let new_id = LocalId::new(self.new_locals.len() as u32);
                    self.new_locals.push(LocalDecl {
                        id: new_id,
                        ty: target_decl.ty,
                        is_owned: target_decl.is_owned,
                    });
                    *destination = new_id;
                    self.write_variable(block, orig_target, new_id);
                }
                Instruction::ConstraintCall {
                    obj,
                    args,
                    destination,
                    ..
                } => {
                    self.replace_operand(block, obj);
                    for op in args {
                        self.replace_operand(block, op);
                    }
                    let orig_target = *destination;
                    let target_decl = self
                        .new_locals
                        .iter()
                        .find(|d| d.id == orig_target)
                        .unwrap()
                        .clone();
                    let new_id = LocalId::new(self.new_locals.len() as u32);
                    self.new_locals.push(LocalDecl {
                        id: new_id,
                        ty: target_decl.ty,
                        is_owned: target_decl.is_owned,
                    });
                    *destination = new_id;
                    self.write_variable(block, orig_target, new_id);
                }
                Instruction::IndirectCall {
                    func,
                    args,
                    destination,
                } => {
                    self.replace_operand(block, func);
                    for op in args {
                        self.replace_operand(block, op);
                    }
                    let orig_target = *destination;
                    let target_decl = self
                        .new_locals
                        .iter()
                        .find(|d| d.id == orig_target)
                        .unwrap()
                        .clone();
                    let new_id = LocalId::new(self.new_locals.len() as u32);
                    self.new_locals.push(LocalDecl {
                        id: new_id,
                        ty: target_decl.ty,
                        is_owned: target_decl.is_owned,
                    });
                    *destination = new_id;
                    self.write_variable(block, orig_target, new_id);
                }
                Instruction::Await {
                    future,
                    destination,
                } => {
                    self.replace_operand(block, future);
                    let orig_target = *destination;
                    let target_decl = self
                        .new_locals
                        .iter()
                        .find(|d| d.id == orig_target)
                        .unwrap()
                        .clone();
                    let new_id = LocalId::new(self.new_locals.len() as u32);
                    self.new_locals.push(LocalDecl {
                        id: new_id,
                        ty: target_decl.ty,
                        is_owned: target_decl.is_owned,
                    });
                    *destination = new_id;
                    self.write_variable(block, orig_target, new_id);
                }
                Instruction::AwaitAll {
                    futures,
                    destination,
                }
                | Instruction::AwaitRace {
                    futures,
                    destination,
                } => {
                    for fut in futures {
                        self.replace_operand(block, fut);
                    }
                    let orig_target = *destination;
                    let target_decl = self
                        .new_locals
                        .iter()
                        .find(|d| d.id == orig_target)
                        .unwrap()
                        .clone();
                    let new_id = LocalId::new(self.new_locals.len() as u32);
                    self.new_locals.push(LocalDecl {
                        id: new_id,
                        ty: target_decl.ty,
                        is_owned: target_decl.is_owned,
                    });
                    *destination = new_id;
                    self.write_variable(block, orig_target, new_id);
                }
            }
        }
    }

    let mut builder = SsaBuilder {
        preds: &preds,
        new_locals: &mut new_locals,
        current_def: &mut current_def,
        block_parameters: &mut block_parameters,
        phi_operands: &mut phi_operands,
    };

    let mut new_blocks = func.blocks.clone();

    // Process instructions forward
    for block in &mut new_blocks {
        let block_id = block.id;
        for inst in &mut block.instructions {
            builder.replace_instruction(block_id, inst);
        }

        // Handle terminator
        match &mut block.terminator.0 {
            Terminator::Return(Some(op)) => builder.replace_operand(block_id, op),
            Terminator::Branch { cond, .. } => builder.replace_operand(block_id, cond),
            _ => {}
        }
    }

    // Evaluate phi operands until fixed point (which evaluates all back-edges correctly)
    let mut evaluated_phis = collections::HashSet::new();
    loop {
        let mut to_evaluate = Vec::new();
        for (phi_id, ops) in builder.phi_operands.iter() {
            if !evaluated_phis.contains(phi_id) {
                to_evaluate.push((*phi_id, ops.clone()));
            }
        }
        if to_evaluate.is_empty() {
            break;
        }
        for (phi_id, ops) in to_evaluate {
            for (pred_block, orig_var) in ops {
                builder.read_variable(pred_block, orig_var);
            }
            evaluated_phis.insert(phi_id);
        }
    }

    // A block's `parameters` corresponds to the `phi_id`s created.
    for block in &mut new_blocks {
        let block_id = block.id;
        if let Some(params) = builder.block_parameters.get(&block_id) {
            for (_orig, phi) in params {
                let phi_decl = builder
                    .new_locals
                    .iter()
                    .find(|d| d.id == *phi)
                    .unwrap()
                    .clone();
                block.parameters.push(phi_decl);
            }
        }
    }

    // For terminators, we update the args
    for block in &mut new_blocks {
        let block_id = block.id;

        let mut get_args = |target_block: BlockId| -> Vec<Operand> {
            let mut args = Vec::new();
            let params = builder.block_parameters.get(&target_block).cloned();
            if let Some(params) = params {
                for (orig, _phi_id) in params {
                    let val = builder.read_variable(block_id, orig);
                    let is_parameter = (orig.raw() as usize) < func.parameter_types.len();
                    let operand = if val == orig && !is_parameter {
                        Operand::Constant(Constant::Null)
                    } else {
                        Operand::Local(val)
                    };
                    args.push(operand);
                }
            }
            args
        };

        match &mut block.terminator.0 {
            Terminator::Jump { target, args } => {
                *args = get_args(*target);
            }
            Terminator::Branch {
                true_block,
                true_args,
                false_block,
                false_args,
                ..
            } => {
                *true_args = get_args(*true_block);
                *false_args = get_args(*false_block);
            }
            _ => {}
        }
    }

    func.locals = builder.new_locals.clone();
    func.blocks = new_blocks;
}
