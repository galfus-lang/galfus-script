use galfus_ir::mir::{BlockId, MirFunction, Terminator};
use std::collections::HashMap;

pub(super) struct Dominators {
    indices: HashMap<BlockId, usize>,
    sets: Vec<Vec<bool>>,
}

impl Dominators {
    pub(super) fn dominates(&self, block: BlockId, candidate: BlockId) -> bool {
        let Some(&block_index) = self.indices.get(&block) else {
            return false;
        };
        let Some(&candidate_index) = self.indices.get(&candidate) else {
            return false;
        };
        self.sets[block_index][candidate_index]
    }
}

pub(super) fn dominators(function: &MirFunction) -> Dominators {
    let indices = function
        .blocks
        .iter()
        .enumerate()
        .map(|(index, block)| (block.id, index))
        .collect::<HashMap<_, _>>();
    let entry = function.blocks.first().map(|block| block.id);
    let mut predecessors = vec![Vec::new(); function.blocks.len()];
    for block in &function.blocks {
        for successor in successors(&block.terminator.0) {
            if let (Some(&source), Some(&target)) =
                (indices.get(&block.id), indices.get(&successor))
            {
                predecessors[target].push(source);
            }
        }
    }
    let mut sets = function
        .blocks
        .iter()
        .enumerate()
        .map(|(index, block)| {
            if Some(block.id) == entry || predecessors[index].is_empty() {
                let mut initial = vec![false; function.blocks.len()];
                initial[index] = true;
                initial
            } else {
                vec![true; function.blocks.len()]
            }
        })
        .collect::<Vec<_>>();
    let mut changed = true;
    while changed {
        changed = false;
        for (index, block) in function.blocks.iter().enumerate() {
            if Some(block.id) == entry {
                continue;
            }
            let Some((&first, rest)) = predecessors[index].split_first() else {
                continue;
            };
            let mut next = sets[first].clone();
            for predecessor in rest {
                for (candidate, dominates) in next.iter_mut().zip(&sets[*predecessor]) {
                    *candidate &= *dominates;
                }
            }
            next[index] = true;
            if sets[index] != next {
                sets[index] = next;
                changed = true;
            }
        }
    }
    Dominators { indices, sets }
}

fn successors(terminator: &Terminator) -> Vec<BlockId> {
    match terminator {
        Terminator::Jump { target, .. } => vec![*target],
        Terminator::Branch {
            true_block,
            false_block,
            ..
        } => vec![*true_block, *false_block],
        _ => Vec::new(),
    }
}
