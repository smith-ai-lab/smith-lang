//! Which of a function's values can be, or hold, memory that the function
//! gives back as its result.
//!
//! The handler of `alloc` builds a function's result in its caller's memory
//! and its other lists and texts in memory it gives up when it returns. Which
//! is which is decided here, for each value, from the function alone.
//!
//! Two values are put in one group when memory that one is, or holds, can be
//! reached through the other: an address and the address moved from it, an
//! address and the addresses held at it, a value and what a block it jumps to
//! calls it, and every address a call takes or gives. A group is *given back*
//! when it has an address the function returns. The test errs towards giving
//! back: it never says a value stays when it may not, and a word of 8 bytes
//! that is no address is taken for one only where it moves through memory,
//! which is how a copy of a list's elements moves an address. A value that
//! holds no address is never in a group, and neither is a constant, which
//! names memory but is none the function made.

use super::common::dominated;
use super::graph::jumps;
use super::{Bits, Constant, End, Function, Instruction, Op, System, Type, Value};

/// For each value of the function, whether it is in a group that the function
/// gives back through an address it returns.
pub fn given_back(f: &Function, regions: bool) -> Vec<bool> {
    let (mut groups, constant) = grouped(f);
    let mut back = vec![false; f.types.len()];
    for block in &f.blocks {
        if let End::Return(values) = &block.end {
            for v in values.iter().filter(|v| f.types[v.0] == Type::Address) {
                back[groups.find(v.0)] = true;
            }
        }
    }
    // What a region keeps is built in the region, which is where a function
    // builds what it returns, so it is given back as that is.
    if regions {
        for i in f.blocks.iter().flat_map(|b| &b.instructions) {
            if let Op::System {
                call: System::RegionKeep,
                args,
            } = &i.op
            {
                for v in args[2..].iter().filter(|v| f.types[v.0] == Type::Address) {
                    back[groups.find(v.0)] = true;
                }
            }
        }
    }
    // A value of an arena that is, or is held with, a value that is made
    // outside it is read after it ends, so it is built in the memory outside.
    let holds = |v: usize| f.types[v] == Type::Address || f.types[v] == Type::Int(Bits::B64);
    for inside in arenas(f) {
        let mut within = vec![false; f.types.len()];
        let mut outside = vec![false; f.types.len()];
        for v in (0..f.types.len()).filter(|v| holds(*v) && !constant[*v]) {
            let group = groups.find(v);
            if inside[v] {
                within[group] = true;
            } else {
                outside[group] = true;
            }
        }
        for g in 0..f.types.len() {
            if within[g] && outside[g] {
                back[g] = true;
            }
        }
    }
    (0..f.types.len())
        .map(|v| !constant[v] && back[groups.find(v)])
        .collect()
}

/// Which memory of a function is kept in the region of a handler's state, and where
/// the frame of that region is at each place of the function.
pub struct Regioned {
    /// For each value, the region it is built in, by how many handlers that have a
    /// region it is inside, when it is an address that is in a group with a value
    /// that a `RegionKeep` keeps, so that it is read after the function, and the
    /// child that runs it, has ended. A group that is kept in two regions is built
    /// in the outer, which is given back last.
    of: Vec<Option<u64>>,
    /// For each region, the names of its frame that the function has, as the places
    /// they are made: the block and how many instructions come before them in it.
    frames: std::collections::HashMap<u64, Vec<(Value, usize, usize)>>,
    /// The block that immediately dominates each block.
    idom: Vec<usize>,
}

impl Regioned {
    /// Whether the value is kept by a region, wherever in the function it is built.
    pub fn kept(&self, v: Value) -> bool {
        self.of[v.0].is_some()
    }

    /// Whether the value is built in a region at the place, which is the
    /// instruction `at` of the block.
    pub fn holds(&self, v: Value, block: usize, at: usize) -> bool {
        self.frame_at(v, block, at).is_some()
    }

    /// The frame of the region the value is built in at the place, which is the
    /// instruction `at` of the block, as a name that is made before the place.
    /// A value that a region keeps has no frame at a place that no frame of the
    /// region is made before, as the places of a function that installs one
    /// handler after the other, such as the state of a handler that was made before
    /// the region of the next was, and is built there as a value that no region
    /// keeps is.
    pub fn frame_at(&self, v: Value, block: usize, at: usize) -> Option<Value> {
        let depth = self.of[v.0]?;
        let dominates = |db: usize, di: usize| {
            if db == block {
                return di <= at;
            }
            let mut b = block;
            while b != 0 && self.idom[b] != usize::MAX {
                b = self.idom[b];
                if b == db {
                    return true;
                }
            }
            false
        };
        self.frames
            .get(&depth)?
            .iter()
            .find(|(_, db, di)| dominates(*db, *di))
            .map(|(frame, _, _)| *frame)
    }
}

/// `regioned`, where the program starts tasks, and none of it is in a region where
/// it does not.
pub fn regioned_if(f: &Function, tasks: bool) -> Regioned {
    if tasks {
        regioned(f)
    } else {
        Regioned {
            of: vec![None; f.types.len()],
            frames: Default::default(),
            idom: Vec::new(),
        }
    }
}

/// For each value of the function, whether its memory is built in the region of a
/// handler's state: it is in a group with a value that a `RegionKeep` keeps.
pub fn regioned(f: &Function) -> Regioned {
    let (mut groups, constant) = grouped(f);
    let n = f.types.len();
    let mut numbers: std::collections::HashMap<usize, u64> = std::collections::HashMap::new();
    let mut at_of: Vec<Option<(usize, usize)>> = vec![None; n];
    for (b, block) in f.blocks.iter().enumerate() {
        for p in &block.params {
            at_of[p.0] = Some((b, 0));
        }
        for (k, i) in block.instructions.iter().enumerate() {
            for r in &i.results {
                at_of[r.0] = Some((b, k + 1));
            }
            if let Op::Constant(Constant::Int(c)) = &i.op {
                numbers.insert(i.results[0].0, *c);
            }
        }
    }
    let mut frames: std::collections::HashMap<u64, Vec<(Value, usize, usize)>> = Default::default();
    let mut kept_in: std::collections::HashMap<usize, u64> = std::collections::HashMap::new();
    for i in f.blocks.iter().flat_map(|b| &b.instructions) {
        if let Op::System {
            call: System::RegionKeep,
            args,
        } = &i.op
        {
            let depth = numbers[&args[1].0];
            let (b, k) = at_of[args[0].0].expect("a frame that is made");
            frames.entry(depth).or_default().push((args[0], b, k));
            for v in args[2..]
                .iter()
                .filter(|v| f.types[v.0] == Type::Address && !constant[v.0])
            {
                let g = groups.find(v.0);
                let now = kept_in.entry(g).or_insert(depth);
                *now = (*now).min(depth);
            }
        }
    }
    let of = (0..n)
        .map(|v| {
            (f.types[v] == Type::Address && !constant[v])
                .then(|| kept_in.get(&groups.find(v)).copied())
                .flatten()
        })
        .collect();
    let mut idom = vec![usize::MAX; f.blocks.len()];
    for (b, children) in dominated(f).iter().enumerate() {
        for &c in children {
            idom[c] = b;
        }
    }
    Regioned { of, frames, idom }
}

/// For each arena the function opens, the values that are made between its
/// opening and its closing: those of the instructions that can be reached from
/// the opening without passing the closing that takes its token, and the
/// parameters of the blocks entered on the way.
fn arenas(f: &Function) -> Vec<Vec<bool>> {
    let mut found = Vec::new();
    for (b, block) in f.blocks.iter().enumerate() {
        for (k, i) in block.instructions.iter().enumerate() {
            if !matches!(
                i.op,
                Op::System {
                    call: System::ArenaOpen,
                    ..
                }
            ) {
                continue;
            }
            let token = i.results[0];
            let mut inside = vec![false; f.types.len()];
            let mut entered = vec![false; f.blocks.len()];
            let mut work = vec![(b, k + 1)];
            while let Some((b, from)) = work.pop() {
                if from == 0 {
                    if entered[b] {
                        continue;
                    }
                    entered[b] = true;
                    for p in &f.blocks[b].params {
                        inside[p.0] = true;
                    }
                }
                let mut closed = false;
                for i in &f.blocks[b].instructions[from..] {
                    if matches!(&i.op, Op::System { call: System::ArenaClose, args } if args[0] == token)
                    {
                        closed = true;
                        break;
                    }
                    for r in &i.results {
                        inside[r.0] = true;
                    }
                }
                if !closed {
                    for jump in jumps(&f.blocks[b].end) {
                        work.push((jump.block, 0));
                    }
                }
            }
            found.push(inside);
        }
    }
    found
}

/// The groups of the function's values, and which values are constants.
fn grouped(f: &Function) -> (Groups, Vec<bool>) {
    let mut groups = Groups::new(f.types.len());
    let holds = |v: &Value| f.types[v.0] == Type::Address || f.types[v.0] == Type::Int(Bits::B64);
    let mut constant = vec![false; f.types.len()];
    for i in f.blocks.iter().flat_map(|b| &b.instructions) {
        if let Op::Constant(Constant::Int(_) | Constant::Null | Constant::Data(_)) = i.op {
            constant[i.results[0].0] = true;
        }
    }
    let mut join = |a: Value, b: Value| {
        if !constant[a.0] && !constant[b.0] {
            groups.join(a.0, b.0);
        }
    };
    for block in &f.blocks {
        for i in &block.instructions {
            match &i.op {
                Op::Binary { a, .. } if f.types[i.results[0].0] == Type::Address => {
                    join(i.results[0], *a)
                }
                Op::Choose {
                    then, otherwise, ..
                } if holds(&i.results[0]) => {
                    join(i.results[0], *then);
                    join(i.results[0], *otherwise);
                }
                Op::Load { address, .. } if holds(&i.results[0]) => join(i.results[0], *address),
                Op::Store { address, value, .. } if holds(value) => join(*value, *address),
                Op::Call { args, .. } => {
                    let held: Vec<Value> = (args.iter().chain(&i.results))
                        .filter(|v| f.types[v.0] == Type::Address && !constant[v.0])
                        .copied()
                        .collect();
                    // The first is the one the others are put with, so a
                    // constant is left out: it joins nothing.
                    for v in held.iter().skip(1) {
                        join(held[0], *v);
                    }
                }
                _ => {}
            }
        }
        for jump in jumps(&block.end) {
            for (arg, param) in jump.args.iter().zip(&f.blocks[jump.block].params) {
                if holds(param) {
                    join(*arg, *param);
                }
            }
        }
    }
    (groups, constant)
}

/// The rounds of the function's loops that end by releasing what they took.
pub struct Rounds {
    /// For each block, for each jump it ends with, whether the jump goes back
    /// to the start of a round that hands nothing on, so that what the round
    /// took from the stack for temporaries is released by it.
    pub release: Vec<Vec<bool>>,
    /// For each block, whether a jump of it enters a loop of those.
    pub enter: Vec<bool>,
}

/// Which loops of the function hand nothing they take from the stack for
/// temporaries on to their next round, and where those rounds end.
///
/// A loop is the blocks from which a jump back to its first block can be
/// reached without passing through that block. A round hands on what it makes
/// when an address it makes is, or is held with, a value that is made outside
/// the loop or that the first block takes as a parameter, which is what the
/// next round or the code after it can read. Whether an address is held with
/// another is what the groups above say, which err towards holding: a loop
/// that is in doubt releases nothing. `takes` says of an instruction whether
/// the memory it gives is taken from the stack for temporaries.
pub fn rounds(f: &Function, takes: &dyn Fn(&Instruction) -> bool) -> Rounds {
    let n = f.blocks.len();
    let mut rounds = Rounds {
        release: f
            .blocks
            .iter()
            .map(|b| vec![false; jumps(&b.end).len()])
            .collect(),
        enter: vec![false; n],
    };
    let tree = dominated(f);
    let mut parent = vec![usize::MAX; n];
    for (b, children) in tree.iter().enumerate() {
        for c in children {
            parent[*c] = b;
        }
    }
    // The blocks the tree holds are the ones that can be reached.
    let reached = {
        let mut done = vec![false; n];
        let mut stack = vec![0];
        while let Some(b) = stack.pop() {
            done[b] = true;
            stack.extend(tree[b].iter().copied());
        }
        done
    };
    let dominates = |h: usize, mut b: usize| {
        loop {
            if b == h {
                return true;
            }
            if b == 0 || parent[b] == usize::MAX {
                return false;
            }
            b = parent[b];
        }
    };
    let mut headers: Vec<usize> = Vec::new();
    let mut back: Vec<(usize, usize, usize)> = Vec::new();
    for (b, block) in f.blocks.iter().enumerate() {
        if !reached[b] {
            continue;
        }
        for (j, jump) in jumps(&block.end).iter().enumerate() {
            if reached[jump.block] && dominates(jump.block, b) {
                back.push((b, j, jump.block));
                if !headers.contains(&jump.block) {
                    headers.push(jump.block);
                }
            }
        }
    }
    if headers.is_empty() {
        return rounds;
    }
    let mut before: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (b, block) in f.blocks.iter().enumerate() {
        if reached[b] {
            for jump in jumps(&block.end) {
                before[jump.block].push(b);
            }
        }
    }
    let (mut groups, _) = grouped(f);
    for header in headers {
        let mut inside = vec![false; n];
        inside[header] = true;
        let mut stack: Vec<usize> = Vec::new();
        for (b, _, h) in &back {
            if *h == header && !inside[*b] {
                inside[*b] = true;
                stack.push(*b);
            }
        }
        while let Some(b) = stack.pop() {
            for p in &before[b] {
                if !inside[*p] {
                    inside[*p] = true;
                    stack.push(*p);
                }
            }
        }
        // The values that are made in the loop, but not the first block's
        // parameters, which the next round is given.
        let mut made = vec![false; f.types.len()];
        for (b, block) in f.blocks.iter().enumerate() {
            if !inside[b] {
                continue;
            }
            if b != header {
                for p in &block.params {
                    made[p.0] = true;
                }
            }
            for i in &block.instructions {
                for r in &i.results {
                    made[r.0] = true;
                }
            }
        }
        let mut handed_on = vec![false; f.types.len()];
        for v in 0..f.types.len() {
            if !made[v] {
                handed_on[groups.find(v)] = true;
            }
        }
        let mut any = false;
        let mut takes_one = true;
        for (b, block) in f.blocks.iter().enumerate() {
            if !inside[b] {
                continue;
            }
            for i in &block.instructions {
                if takes(i) {
                    any = true;
                    for r in i.results.iter().filter(|r| f.types[r.0] == Type::Address) {
                        if handed_on[groups.find(r.0)] {
                            takes_one = false;
                        }
                    }
                }
            }
        }
        if !any || !takes_one {
            continue;
        }
        for (b, j, h) in &back {
            if *h == header {
                rounds.release[*b][*j] = true;
            }
        }
        for (b, block) in f.blocks.iter().enumerate() {
            if !inside[b] && reached[b] && jumps(&block.end).iter().any(|j| j.block == header) {
                rounds.enter[b] = true;
            }
        }
    }
    rounds
}

/// Groups of values, each by the value that stands for it.
struct Groups(Vec<usize>);

impl Groups {
    fn new(n: usize) -> Groups {
        Groups((0..n).collect())
    }

    fn find(&mut self, mut v: usize) -> usize {
        while self.0[v] != v {
            self.0[v] = self.0[self.0[v]];
            v = self.0[v];
        }
        v
    }

    fn join(&mut self, a: usize, b: usize) {
        let (a, b) = (self.find(a), self.find(b));
        self.0[a] = b;
    }
}
