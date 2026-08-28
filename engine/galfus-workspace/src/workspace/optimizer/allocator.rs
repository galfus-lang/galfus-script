use std::collections::{BTreeMap, BTreeSet};

use galfus_bytecode::{BytecodeFunction, Reg};

use super::liveness::instruction_def_use;

/// Reuses non-parameter registers only when their complete live intervals do
/// not overlap. Operand windows become relative constraints, so they remain
/// contiguous after their members are assigned physical slots.
///
/// Returns `false` without modifying `func` when overlapping windows impose
/// incompatible constraints. The caller then retains dense compaction only.
pub fn allocate_registers(
    func: &mut BytecodeFunction,
    intervals: &[Option<(usize, usize)>],
    register_count: usize,
) -> bool {
    let parameter_count = func.param_count as usize;
    let mut constraints = WeightedUnionFind::new(register_count);

    for instruction in &func.instructions {
        let mut definitions = Vec::new();
        let mut uses = Vec::new();
        let mut ranges = Vec::new();
        instruction_def_use(instruction, &mut definitions, &mut uses, &mut ranges);
        for (start, count) in ranges {
            for offset in 1..count as usize {
                if !constraints.union(
                    start.raw() as usize,
                    start.raw() as usize + offset,
                    offset as i32,
                ) {
                    return false;
                }
            }
        }
    }

    let mut groups = BTreeMap::<usize, Vec<(usize, i32)>>::new();
    for register in 0..register_count {
        let (root, offset) = constraints.find(register);
        groups.entry(root).or_default().push((register, offset));
    }

    let mut requests = Vec::with_capacity(groups.len());
    for members in groups.into_values() {
        let min_offset = members.iter().map(|(_, offset)| *offset).min().unwrap_or(0);
        let members = members
            .into_iter()
            .map(|(register, offset)| Member {
                register,
                offset: (offset - min_offset) as usize,
                interval: intervals[register],
            })
            .collect::<Vec<_>>();
        let width = members
            .iter()
            .map(|member| member.offset + 1)
            .max()
            .unwrap_or(1);
        if members
            .iter()
            .map(|member| member.offset)
            .collect::<BTreeSet<_>>()
            .len()
            != members.len()
        {
            return false;
        }

        let fixed_base = members
            .iter()
            .filter(|member| member.register < parameter_count)
            .try_fold(None, |base, member| {
                let candidate = member.register.checked_sub(member.offset);
                match (base, candidate) {
                    (_, None) => None,
                    (None, Some(candidate)) => Some(Some(candidate)),
                    (Some(base), Some(candidate)) if base == candidate => Some(Some(base)),
                    _ => None,
                }
            });
        let Some(fixed_base) = fixed_base else {
            return false;
        };
        if members.iter().any(|member| {
            member.register >= parameter_count
                && fixed_base.is_some_and(|base| base + member.offset < parameter_count)
        }) {
            return false;
        }

        let start = members
            .iter()
            .filter_map(|member| member.interval.map(|interval| interval.0))
            .min()
            .unwrap_or(usize::MAX);
        requests.push(Request {
            members,
            width,
            fixed_base,
            start,
        });
    }
    requests.sort_by_key(|request| (request.fixed_base.is_none(), request.start));

    let mut occupancy = vec![Vec::<(usize, usize)>::new(); parameter_count];
    let mut remap = vec![None; register_count];
    for request in requests {
        let base = match request.fixed_base {
            Some(base)
                if base + request.width <= register_count && fits(&request, base, &occupancy) =>
            {
                base
            }
            Some(_) => return false,
            None => {
                let mut base = parameter_count;
                while base + request.width <= register_count && !fits(&request, base, &occupancy) {
                    base += 1;
                }
                if base + request.width > register_count {
                    return false;
                }
                base
            }
        };
        if occupancy.len() < base + request.width {
            occupancy.resize(base + request.width, Vec::new());
        }
        for member in request.members {
            let physical = base + member.offset;
            remap[member.register] = Some(Reg(physical as u16));
            if let Some(interval) = member.interval {
                occupancy[physical].push(interval);
            }
        }
    }

    if remap.iter().any(Option::is_none) {
        return false;
    }
    for instruction in &mut func.instructions {
        super::remap_instruction_registers(instruction, &remap);
    }
    let register_total = remap
        .iter()
        .flatten()
        .map(|register| register.raw() as usize + 1)
        .max()
        .unwrap_or(parameter_count);
    func.local_count = (register_total - parameter_count) as u16;
    func.temp_count = 0;
    true
}

struct Member {
    register: usize,
    offset: usize,
    interval: Option<(usize, usize)>,
}

struct Request {
    members: Vec<Member>,
    width: usize,
    fixed_base: Option<usize>,
    start: usize,
}

fn fits(request: &Request, base: usize, occupancy: &[Vec<(usize, usize)>]) -> bool {
    request.members.iter().all(|member| {
        let Some(interval) = member.interval else {
            return true;
        };
        occupancy.get(base + member.offset).is_none_or(|occupied| {
            occupied
                .iter()
                .all(|existing| existing.1 < interval.0 || interval.1 < existing.0)
        })
    })
}

struct WeightedUnionFind {
    parent: Vec<usize>,
    offset_to_parent: Vec<i32>,
}

impl WeightedUnionFind {
    fn new(length: usize) -> Self {
        Self {
            parent: (0..length).collect(),
            offset_to_parent: vec![0; length],
        }
    }

    fn find(&mut self, node: usize) -> (usize, i32) {
        if self.parent[node] == node {
            return (node, 0);
        }
        let parent = self.parent[node];
        let (root, parent_offset) = self.find(parent);
        self.offset_to_parent[node] += parent_offset;
        self.parent[node] = root;
        (root, self.offset_to_parent[node])
    }

    /// Records `position(right) = position(left) + distance`.
    fn union(&mut self, left: usize, right: usize, distance: i32) -> bool {
        let (left_root, left_offset) = self.find(left);
        let (right_root, right_offset) = self.find(right);
        if left_root == right_root {
            return right_offset == left_offset + distance;
        }
        self.parent[right_root] = left_root;
        self.offset_to_parent[right_root] = left_offset + distance - right_offset;
        true
    }
}
