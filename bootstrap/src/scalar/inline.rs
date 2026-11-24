//! The first pass over the scalar form: a call of a small function becomes
//! the function's own instructions.
//!
//! A call is replaced when the function is called from one place only, or
//! when the function is no larger than `BOUND`, or when it calls nothing and
//! is no larger than `LEAF_BOUND`, and never when the function is cold.
//! Nothing measured from a run is used. Functions are taken callees first, so each is already as small
//! as it will be when a call of it is looked at, and a copy of it is never
//! looked at again. The pass is one walk over the program it gives, and
//! gives the same program every time.

use super::{Block, End, Function, Instruction, Jump, Op, Program, System, Value};

/// The most a function may hold, as `size` counts, for a call of it to be
/// replaced when it is called from more than one place.
pub const BOUND: usize = 4;

/// The most a function that calls nothing may hold, as `size` counts, for a
/// call of it to be replaced. A copy of it holds no call, so what the later
/// stages make of the copy does not grow with the calls it makes.
pub const LEAF_BOUND: usize = 40;

/// How large a function is: its instructions, and one for how each block
/// ends.
pub fn size(f: &Function) -> usize {
    f.blocks.iter().map(|b| b.instructions.len() + 1).sum()
}

/// The program with every call that the rule replaces replaced, and every
/// function that nothing calls left out. A function with no call that is
/// replaced is the one it was, not a copy.
pub fn inline(program: Program) -> Program {
    let order = callees_first(&program);
    let mut sites = vec![0usize; program.functions.len()];
    for &f in &order {
        for callee in calls(&program.functions[f]) {
            sites[callee] += 1;
        }
    }
    let (data, main) = (program.data, program.main);
    let mut pending: Vec<Option<Function>> = program.functions.into_iter().map(Some).collect();
    let mut done: Vec<Option<Function>> = pending.iter().map(|_| None).collect();
    for &f in &order {
        let function = inlined(pending[f].take().unwrap(), &mut done, &sites);
        done[f] = Some(function);
    }
    // What is left is what `main` still calls, in the order of the program.
    let mut kept = vec![false; done.len()];
    kept[main] = true;
    let mut stack = vec![main];
    while let Some(f) = stack.pop() {
        for callee in calls(done[f].as_ref().unwrap()) {
            if !kept[callee] {
                kept[callee] = true;
                stack.push(callee);
            }
        }
    }
    let mut place = vec![0usize; kept.len()];
    let mut count = 0;
    for (f, keep) in kept.iter().enumerate() {
        place[f] = count;
        count += usize::from(*keep);
    }
    let functions = (done.into_iter().enumerate())
        .filter(|(f, _)| kept[*f])
        .map(|(_, f)| {
            let mut f = f.unwrap();
            for block in &mut f.blocks {
                for i in &mut block.instructions {
                    if let Op::Call { function, .. } = &mut i.op {
                        *function = place[*function];
                    }
                }
            }
            f
        })
        .collect();
    Program {
        data,
        functions,
        main: place[main],
    }
}

/// The functions `main` reaches, each after the functions it calls.
fn callees_first(program: &Program) -> Vec<usize> {
    let mut seen = vec![false; program.functions.len()];
    let mut order = Vec::new();
    seen[program.main] = true;
    let mut stack = vec![(program.main, 0, calls(&program.functions[program.main]))];
    while let Some((f, next, callees)) = stack.last_mut() {
        if let Some(&callee) = callees.get(*next) {
            *next += 1;
            if !seen[callee] {
                seen[callee] = true;
                let inner = calls(&program.functions[callee]);
                stack.push((callee, 0, inner));
            }
        } else {
            order.push(*f);
            stack.pop();
        }
    }
    order
}

/// Whether a call of the function is replaced when it is called from more
/// than one place.
fn is_small(f: &Function) -> bool {
    let size = size(f);
    size <= BOUND || (size <= LEAF_BOUND && calls(f).is_empty())
}

/// The function of each call in the function, in order.
pub(super) fn calls(f: &Function) -> Vec<usize> {
    (f.blocks.iter())
        .flat_map(|b| &b.instructions)
        .filter_map(|i| match i.op {
            Op::Call { function, .. } => Some(function),
            _ => None,
        })
        .collect()
}

/// The function with each call that the rule replaces replaced by the
/// callee's blocks. A block that holds such a call is cut where the call
/// was: what came before jumps to a copy of the callee's blocks, which
/// return by jumping to a block that takes the call's results and holds
/// what came after.
fn inlined(f: Function, done: &mut [Option<Function>], sites: &[usize]) -> Function {
    // Whether a call of the function is replaced.
    let replaced = |done: &[Option<Function>], function: usize| {
        let callee = done[function].as_ref().unwrap();
        !(callee.cold || (sites[function] != 1 && !is_small(callee)))
    };
    if !(f.blocks.iter().flat_map(|b| &b.instructions))
        .any(|i| matches!(&i.op, Op::Call { function, .. } if replaced(done, *function)))
    {
        return f;
    }
    let Function {
        mut types,
        results,
        blocks: old,
        cold,
        name,
    } = f;
    let mut blocks: Vec<Block> = Vec::new();
    // Where each block of `f` starts in `blocks`, and which blocks end as a
    // block of `f` does, with jumps to blocks of `f`.
    let mut first = Vec::with_capacity(old.len());
    let mut original_ends = Vec::with_capacity(old.len());
    // The jumps, as a block and the place of the jump in its end, that go to a
    // block of `f`, which is not placed until all of `f` is.
    let mut later: Vec<(usize, usize, usize)> = Vec::new();
    for block in old {
        let Block {
            params,
            instructions,
            end,
        } = block;
        let mut at = blocks.len();
        first.push(at);
        blocks.push(Block {
            params,
            instructions: Vec::new(),
            end: End::Return(Vec::new()),
        });
        // The block of `f` that is the landing of the call that ends this block's
        // instructions, when the call has one: the first jump of the choice on
        // `Unwinding` that follows the call.
        let outer = outer_landing(&instructions, &end);
        for (index, instruction) in instructions.into_iter().enumerate() {
            let Instruction {
                results: call_results,
                op,
                at: place,
            } = instruction;
            let (function, args) = match op {
                // A call that a landing follows is not replaced when its function fails
                // itself: the failure of an `Allocate`, a `Write` or a `Fail` goes up from
                // the frame that holds it, to the landings of the calls it went up
                // through, and would go past this one if the code were in this function.
                Op::Call { function, args }
                    if replaced(done, function)
                        && !(outer.as_ref().is_some_and(|(_, call, _)| *call == index)
                            && fails_itself(done[function].as_ref().unwrap())) =>
                {
                    (function, args)
                }
                op => {
                    blocks[at].instructions.push(Instruction {
                        results: call_results,
                        op,
                        at: place,
                    });
                    continue;
                }
            };
            // The blocks of the callee for this call: its own when this is the only
            // call of it, and a copy when it is not.
            let base = types.len();
            let copied = if sites[function] == 1 {
                let callee = done[function].take().unwrap();
                types.extend(callee.types);
                callee.blocks
            } else {
                let callee = done[function].as_ref().unwrap();
                types.extend_from_slice(&callee.types);
                callee.blocks.clone()
            };
            let rename = |v: Value| Value(v.0 + base);
            let entry = blocks.len();
            let after = entry + copied.len();
            blocks[at].end = End::Jump(Jump { block: entry, args });
            for mut block in copied {
                block.params.iter_mut().for_each(|v| *v = rename(*v));
                for i in &mut block.instructions {
                    i.results.iter_mut().for_each(|v| *v = rename(*v));
                    i.op.rename(&rename);
                }
                block.end = match block.end {
                    End::Return(mut values) => {
                        values.iter_mut().for_each(|v| *v = rename(*v));
                        End::Jump(Jump {
                            block: after,
                            args: values,
                        })
                    }
                    mut end => {
                        if let End::Switch { on, .. } = &mut end {
                            *on = rename(*on);
                        }
                        for j in super::graph::jumps_mut(&mut end) {
                            j.block += entry;
                            j.args.iter_mut().for_each(|v| *v = rename(*v));
                        }
                        end
                    }
                };
                blocks.push(block);
            }
            at = blocks.len();
            blocks.push(Block {
                params: call_results,
                instructions: Vec::new(),
                end: End::Return(Vec::new()),
            });
            if let Some((landing, _, args)) = outer.as_ref().filter(|(_, call, _)| *call == index) {
                chained(
                    &mut blocks,
                    &mut types,
                    entry..after,
                    (*landing, args),
                    place,
                    &mut later,
                );
            }
        }
        blocks[at].end = end;
        original_ends.push(at);
    }
    for (block, jump, to) in later {
        match &mut blocks[block].end {
            End::Jump(j) => j.block = first[to],
            End::Switch { jumps, .. } => jumps[jump].block = first[to],
            End::Return(_) => {}
        }
    }
    for at in original_ends {
        let to = |j: &mut Jump| j.block = first[j.block];
        match &mut blocks[at].end {
            End::Jump(j) => to(j),
            End::Switch { jumps, .. } => jumps.iter_mut().for_each(to),
            End::Return(_) => {}
        }
    }
    Function {
        types,
        results,
        blocks,
        cold,
        name,
    }
}

/// The instruction with each value it reads given by `rename`.
pub(super) fn renamed(op: &Op, rename: &impl Fn(Value) -> Value) -> Op {
    let mut op = op.clone();
    op.rename(rename);
    op
}

/// The block that is the landing of the call that is the last of the
/// instructions, which the choice on `Unwinding` in `end` goes to first.
fn outer_landing(instructions: &[Instruction], end: &End) -> Option<(usize, usize, Vec<Value>)> {
    let unwinding = instructions.iter().position(|i| {
        matches!(
            i.op,
            Op::System {
                call: System::Unwinding,
                ..
            }
        )
    })?;
    let call = unwinding.checked_sub(1)?;
    if !matches!(instructions[call].op, Op::Call { .. }) {
        return None;
    }
    match end {
        End::Switch { jumps, .. } => jumps.first().map(|j| (j.block, call, j.args.clone())),
        _ => None,
    }
}

/// Makes the blocks that a call was replaced by end what the call's own
/// function owned, and then what the function that holds them owns across the
/// call, as a failure that went up through the call did: each landing among
/// them goes on to `landing` where it went on up from the function, and each
/// call among them that has no landing is given a choice on `Unwinding` whose
/// first jump is `landing`. The landing is a block of the function, which is
/// placed later, and `later` is told of the jumps to it.
fn chained(
    blocks: &mut Vec<Block>,
    types: &mut Vec<super::Type>,
    copy: std::ops::Range<usize>,
    (landing, landing_args): (usize, &[Value]),
    place: super::Place,
    later: &mut Vec<(usize, usize, usize)>,
) {
    // The blocks that the landings of the copy are made of: a call in one of them
    // is a failure inside a landing, which ends the program, and is left as it is.
    let mut in_landing = vec![false; blocks.len()];
    let mut stack: Vec<usize> = Vec::new();
    for b in copy.clone() {
        let has_unwinding = blocks[b].instructions.iter().any(|i| {
            matches!(
                i.op,
                Op::System {
                    call: System::Unwinding,
                    ..
                }
            )
        });
        if let (true, End::Switch { jumps, .. }) = (has_unwinding, &blocks[b].end) {
            stack.push(jumps[0].block);
        }
    }
    while let Some(b) = stack.pop() {
        if !in_landing[b] {
            in_landing[b] = true;
            stack.extend(super::graph::jumps(&blocks[b].end).iter().map(|j| j.block));
        }
    }
    for b in copy {
        if in_landing[b] {
            if let Some(r) = blocks[b].instructions.iter().position(|i| {
                matches!(
                    i.op,
                    Op::System {
                        call: System::Resume,
                        ..
                    }
                )
            }) {
                blocks[b].instructions.truncate(r);
                blocks[b].end = End::Jump(Jump {
                    block: 0,
                    args: landing_args.to_vec(),
                });
                later.push((b, 0, landing));
            }
            continue;
        }
        let mut b = b;
        // A call whose next instruction is not `Unwinding` has no landing.
        while let Some(c) = (0..blocks[b].instructions.len()).find(|&c| {
            matches!(blocks[b].instructions[c].op, Op::Call { .. })
                && !matches!(
                    blocks[b].instructions.get(c + 1).map(|i| &i.op),
                    Some(Op::System {
                        call: System::Unwinding,
                        ..
                    })
                )
        }) {
            let rest = blocks[b].instructions.split_off(c + 1);
            let end = std::mem::replace(&mut blocks[b].end, End::Return(Vec::new()));
            let int = super::Type::Int(super::Bits::B64);
            let mut fresh = |ty: super::Type| {
                types.push(ty);
                Value(types.len() - 1)
            };
            let (asked, none, equal) = (
                fresh(int),
                fresh(int),
                fresh(super::Type::Int(super::Bits::B8)),
            );
            let (call, constant, compare) = (
                Op::System {
                    call: System::Unwinding,
                    args: Vec::new(),
                },
                Op::Constant(super::Constant::Int(0)),
                Op::Compare {
                    op: super::Compare::Equal,
                    a: asked,
                    b: none,
                },
            );
            for (results, op) in [(asked, call), (none, constant), (equal, compare)] {
                blocks[b].instructions.push(Instruction {
                    results: vec![results],
                    op,
                    at: place,
                });
            }
            let next = blocks.len();
            blocks.push(Block {
                params: Vec::new(),
                instructions: rest,
                end,
            });
            blocks[b].end = End::Switch {
                on: equal,
                jumps: vec![
                    Jump {
                        block: 0,
                        args: landing_args.to_vec(),
                    },
                    Jump {
                        block: next,
                        args: Vec::new(),
                    },
                ],
            };
            later.push((b, 0, landing));
            b = next;
        }
    }
}

/// Whether the function holds an instruction that the system may refuse, which
/// ends the task by a failure that starts in the frame of the function.
fn fails_itself(f: &Function) -> bool {
    (f.blocks.iter().flat_map(|b| &b.instructions)).any(|i| {
        matches!(
            i.op,
            Op::System {
                call: System::Allocate | System::Write(_) | System::Fail,
                ..
            }
        )
    })
}
