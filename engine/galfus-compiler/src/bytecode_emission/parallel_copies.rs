use super::function::FnEmitter;
use galfus_bytecode::Instruction;
use galfus_bytecode::instruction::Reg;
use galfus_ir::mir::Operand;
use std::collections;

impl FnEmitter<'_, '_> {
    pub(super) fn emit_parallel_copies(&mut self, destinations: &[Reg], sources: &[Operand]) {
        assert_eq!(destinations.len(), sources.len());
        if destinations.is_empty() {
            return;
        }
        let temp_count_before = self.temp_count_current;
        let source_registers = sources
            .iter()
            .map(|source| match source {
                Operand::Local(local) => Reg(local.raw() as u16),
                _ => {
                    let temporary = self.alloc_temp();
                    self.load_operand_to(source, temporary);
                    temporary
                }
            })
            .collect::<Vec<_>>();
        let mut in_degree = collections::BTreeMap::new();
        let mut edges = collections::BTreeMap::new();
        for (&destination, &source) in destinations.iter().zip(&source_registers) {
            if destination != source {
                edges.insert(destination, source);
                *in_degree.entry(source).or_insert(0) += 1;
                in_degree.entry(destination).or_insert(0);
            }
        }
        let mut ready = in_degree
            .iter()
            .filter_map(|(&node, &degree)| {
                (degree == 0 && edges.contains_key(&node)).then_some(node)
            })
            .collect::<collections::BTreeSet<_>>();
        while !edges.is_empty() {
            if let Some(destination) = ready.pop_first() {
                let source = edges.remove(&destination).unwrap();
                self.instructions.push(Instruction::Move {
                    dest: destination,
                    src: source,
                });
                let degree = in_degree.get_mut(&source).unwrap();
                *degree -= 1;
                if *degree == 0 && edges.contains_key(&source) {
                    ready.insert(source);
                }
                continue;
            }
            let destination = *edges.keys().next().unwrap();
            let source = edges.remove(&destination).unwrap();
            let temporary = self.alloc_temp();
            self.instructions.push(Instruction::Move {
                dest: temporary,
                src: source,
            });
            edges.insert(destination, temporary);
            in_degree.insert(temporary, 1);
            let degree = in_degree.get_mut(&source).unwrap();
            *degree -= 1;
            if *degree == 0 && edges.contains_key(&source) {
                ready.insert(source);
            }
        }
        self.temp_count_current = temp_count_before;
    }
}
