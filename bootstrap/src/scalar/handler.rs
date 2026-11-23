//! The state of the `alloc` handler, passed and never global: two stacks of
//! memory, each held as where its free memory begins and how many bytes of it
//! are left. One is where a function's result is built, and one is where its
//! temporaries are.
//!
//! The lowering asks the system for memory wherever the source builds a list
//! or a text, with `System::Allocate`, as if each were a request of its own.
//! `install` turns that into taking room from a stack. A function takes the
//! pair for its results and the pair for its temporaries as its first
//! parameters, each only when it uses it, and gives back the pair for its
//! results. An address it asks for comes from the pair for results when the
//! function may return memory that holds it, and from the pair for temporaries
//! when it cannot, as `escape` works out. A call whose result the function
//! returns is given the pairs as they are. A call whose result it only uses is
//! given them swapped, so that the callee's result is built where the caller
//! keeps its temporaries, and what the callee does with its own temporaries
//! lies above everything the caller has in the stack it returns. A function
//! goes on with the pair for temporaries it had before a call, which releases
//! what the call used. So does a round of a loop that hands nothing on, as
//! `escape::rounds` works out: its jump back gives the first block the pair
//! for temporaries the round began with. Each block takes the pairs from the
//! jump that enters it. The program's entry starts both stacks empty, so the system is not
//! asked before the program first needs room. A program that asks for nothing
//! is left as it is. Nothing is given back to the system.

use super::{
    Binary, Bits, Builder, Compare, Constant, End, Function, Instruction, Jump, Op, Place, Program,
    System, Type, Value, escape, runtime,
};

/// How many bytes the system gives at a time, at least.
pub const PIECE: u64 = 64 * 1024;

/// What a piece spends at its end: the number of bytes it holds for the
/// program, and the address of the end of the piece that follows it, or
/// nothing.
const TRAILER: u64 = 16;

/// What a piece spends at its end in a program that starts a task: the bytes it
/// holds, the end of the piece after it, and the head of the records of the tasks
/// moved into the stack in it.
const TRAILER_RECORDS: u64 = 24;

/// The bit of a request's count that asks only for what the piece has room for.
pub const TRY: u64 = 1 << 63;

/// The bytes of the frame of a region: the two words of a header, the words that
/// say where its stack is and where it started, and the four that a scheduler
/// keeps its queues in, which a task's frame has and a region's does not use.
const REGION_BYTES: u64 = 80;

/// Where in the frame of a region the pair of its stack is.
const REGION_STACK: u64 = 16;

const NEXT: Type = Type::Address;
const LEFT: Type = Type::Int(Bits::B64);
/// The values of one stack, in the order they are passed.
const PAIR: [Type; 2] = [NEXT, LEFT];

/// Whether the function of the program is the handler's `take`, or calls it,
/// which is what the functions that build lists and texts do.
pub fn takes_room(program: &Program, f: &Function) -> bool {
    let take = take();
    *f == take
        || (f.blocks.iter().flat_map(|b| &b.instructions)).any(
            |i| matches!(&i.op, Op::Call { function, .. } if program.functions[*function] == take),
        )
}

/// Which stacks a function takes: the one for results, and the one for
/// temporaries.
#[derive(Clone, Copy, Default, PartialEq)]
struct Uses {
    results: bool,
    temporaries: bool,
}

impl Uses {
    fn any(self) -> bool {
        self.results || self.temporaries
    }
}

/// The two stacks of a function, as they are at a place in it, for the ones it
/// takes.
#[derive(Clone, Copy)]
struct State {
    results: Option<[Value; 2]>,
    temporaries: Option<[Value; 2]>,
}

impl State {
    /// The values in the order they are passed to a function and to a block.
    fn values(&self) -> Vec<Value> {
        [self.results, self.temporaries]
            .into_iter()
            .flatten()
            .flatten()
            .collect()
    }
}

/// The program with the stacks passed to every function that needs room.
pub fn install(program: Program) -> Program {
    install_tasks(program, &[])
}

/// `install` for a program whose tasks have a stack of their own: each poll of
/// `roots`, with the place in its frame of the pair that is the stack for
/// results, runs on that stack, which its frame keeps from one poll to the
/// next, and gives back the stack of whoever polled it as it was.
pub fn install_tasks(program: Program, roots: &[(usize, u64)]) -> Program {
    // The poll of a task runs to a wait and is run again, and what the task made
    // before the wait is read after it, so everything a poll builds is built in
    // the stack for results, which its caller goes on with, and a call of a poll
    // is a call whose result is kept. A task's memory is then below where every
    // poll after it begins, and nothing a task holds across a wait is overwritten.
    let forced: Vec<bool> = program.functions.iter().map(is_poll).collect();
    let polls = forced.clone();
    // A program that starts a task has the pool its memory is taken from, a
    // value that a function takes when it asks for room, gives a task its
    // stack, or gives it back, or calls one that does.
    let tasks_program = (program
        .functions
        .iter()
        .flat_map(|f| &f.blocks)
        .flat_map(|b| &b.instructions))
    .any(|i| {
        matches!(
            i.op,
            Op::System {
                call: System::TaskFrame | System::TaskFree,
                ..
            }
        )
    });
    let pooled: Vec<bool> = if tasks_program {
        pooled_functions(&program)
    } else {
        vec![false; program.functions.len()]
    };
    let joins: Vec<bool> = if tasks_program {
        joining_functions(&program)
    } else {
        vec![false; program.functions.len()]
    };
    let given: Vec<Vec<bool>> = (program.functions.iter().enumerate())
        .map(|(k, f)| {
            if forced[k] {
                vec![true; f.types.len()]
            } else {
                escape::given_back(f, tasks_program)
            }
        })
        .collect();
    let uses = needing(&program, &given, &polls, tasks_program);
    let rounds: Vec<escape::Rounds> = (program.functions.iter().enumerate())
        .map(|(k, f)| {
            escape::rounds(f, &|i| match &i.op {
                Op::System {
                    call: System::Allocate,
                    ..
                } => !given[k][i.results[0].0],
                Op::Call { function, .. } => {
                    uses[*function].results
                        && !polls[*function]
                        && !gives_back(f, &given[k], &i.results)
                }
                _ => false,
            })
        })
        .collect();
    if !uses[program.main].any() {
        return Program {
            functions: program.functions.into_iter().map(without_arenas).collect(),
            ..program
        };
    }
    let at_take = program.functions.len();
    let main = program.functions[program.main].clone();
    let mut functions: Vec<Function> = (program.functions.into_iter().enumerate())
        .map(|(k, f)| {
            if uses[k].any() {
                with_state(
                    f,
                    &given[k],
                    &rounds[k],
                    &About {
                        uses: &uses,
                        polls: &polls,
                        pooled: &pooled,
                        joins: &joins,
                        take: at_take,
                        first: k == program.main,
                        root: roots.iter().find(|(r, _)| *r == k).map(|(_, at)| *at),
                        me_index: k,
                        tasks: tasks_program,
                    },
                )
            } else {
                without_arenas(f)
            }
        })
        .collect();
    // What a task takes from the pool, and gives back to it, is written only for
    // a program that has tasks, and every stack of that program grows from the
    // pool, in pieces that start small and double.
    let mut data = program.data;
    // What a stack that has no piece reads at its end: a piece that holds
    // nothing and has none after it.
    data.push(vec![
        0;
        if tasks_program {
            TRAILER_RECORDS
        } else {
            TRAILER
        } as usize
    ]);
    let empty = data.len() - 1;
    let main_at = if tasks_program {
        // Where the runtime functions are, after `take` and `ready`: `claim`,
        // `free` and the one that frees a task, `move_in`, `release_above`, and
        // the three of the pool.
        functions.push(take_with(Some(at_take + 8)).named("<stack_take>"));
        functions.push(ready_with(at_take, true).named("<stack_ready>"));
        functions.push(claim(at_take + 8).named("<stack_claim>"));
        functions.push(free(at_take + 9).named("<stack_free_all>"));
        functions.push(free_one(at_take + 3, at_take + 9).named("<stack_free>"));
        functions.push(move_in(at_take, at_take + 9).named("<stack_move>"));
        functions.push(release_above(at_take + 3).named("<stack_release>"));
        functions.push(super::pool::new().named("<pool_new>"));
        functions.push(super::pool::take_sized().named("<pool_take>"));
        functions.push(super::pool::give().named("<pool_give>"));
        functions.push(
            entry(
                &main,
                program.main,
                uses[program.main],
                empty,
                Some((at_take + 7, pooled[program.main])),
            )
            .named("<stack_entry>"),
        );
        at_take + 10
    } else {
        functions.push(take_with(None).named("<stack_take>"));
        functions.push(ready_with(at_take, false).named("<stack_ready>"));
        functions.push(
            entry(&main, program.main, uses[program.main], empty, None).named("<stack_entry>"),
        );
        at_take + 2
    };
    Program {
        data,
        main: main_at,
        functions,
    }
}

/// Which functions take the pool: those that ask for room, give a task its
/// stack, or give it back, and those that call one.
fn pooled_functions(program: &Program) -> Vec<bool> {
    let mut found: Vec<bool> = program
        .functions
        .iter()
        .map(|f| {
            (f.blocks.iter().flat_map(|b| &b.instructions)).any(|i| {
                matches!(
                    i.op,
                    Op::System {
                        call: System::Allocate
                            | System::TaskFrame
                            | System::TaskFree
                            | System::TaskMove
                            | System::RegionNew
                            | System::RegionFree
                            | System::RegionMove,
                        ..
                    }
                )
            })
        })
        .collect();
    // The function the program starts with makes each stack it passes have a
    // piece, which asks the pool.
    found[program.main] = true;
    loop {
        let mut changed = false;
        for (k, f) in program.functions.iter().enumerate() {
            let calls = (f.blocks.iter().flat_map(|b| &b.instructions))
                .any(|i| matches!(&i.op, Op::Call { function, .. } if found[*function]));
            if calls && !found[k] {
                found[k] = true;
                changed = true;
            }
        }
        if !changed {
            return found;
        }
    }
}

/// Whether the function is the poll of a task, or the one that polls any: the
/// pass `suspend` names them.
fn is_poll(f: &Function) -> bool {
    f.name.ends_with("<poll>") || f.name == "<poll any>"
}

/// Whether what a call gives is memory that the function returns, so that the
/// callee builds it in the stack for results.
fn gives_back(f: &Function, given: &[bool], results: &[Value]) -> bool {
    results
        .iter()
        .any(|v| f.types[v.0] == Type::Address && given[v.0])
}

/// Which stacks each function takes, which is what it needs where it asks the
/// system for memory, and where it calls a function that needs some.
fn needing(program: &Program, given: &[Vec<bool>], polls: &[bool], regions: bool) -> Vec<Uses> {
    let mut uses = vec![Uses::default(); program.functions.len()];
    loop {
        let mut changed = false;
        for (k, f) in program.functions.iter().enumerate() {
            let now = needing_of(f, &given[k], &uses, polls, regions);
            if now != uses[k] {
                uses[k] = now;
                changed = true;
            }
        }
        if !changed {
            return uses;
        }
    }
}

/// A constant of the function, made where the instructions are being kept.
fn fresh_int(f: &mut Function, kept: &mut Vec<Instruction>, n: u64) -> Value {
    f.types.push(LEFT);
    let value = Value(f.types.len() - 1);
    kept.push(Instruction {
        results: vec![value],
        op: Op::Constant(Constant::Int(n)),
        at: Place::NONE,
    });
    value
}

/// A new pair of values of a stack in the function.
fn fresh(f: &mut Function) -> [Value; 2] {
    PAIR.map(|ty| {
        f.types.push(ty);
        Value(f.types.len() - 1)
    })
}

/// What `with_state` knows of the program and of the function it is given.
struct About<'a> {
    uses: &'a [Uses],
    polls: &'a [bool],
    pooled: &'a [bool],
    /// Which functions can reach a `join` that moves a task's pieces into a stack, so
    /// that a put-back of a stack in them gives those pieces back.
    joins: &'a [bool],
    /// Where the runtime functions of the pass are, which `take` is the first of.
    take: usize,
    /// Whether the function is the one the program starts with.
    first: bool,
    /// Where in its frame the poll of a task keeps its stack, when it is one.
    root: Option<u64>,
    /// The place of the function in the program.
    me_index: usize,
    /// Whether the program starts tasks, so that a region is a frame of the pool.
    tasks: bool,
}

/// The function with the stacks it takes before its parameters, and the one
/// for results before its results, taking room where it asked the system, and
/// giving the stacks to each call that needs them, and to each block.
fn with_state(mut f: Function, given: &[bool], rounds: &escape::Rounds, about: &About) -> Function {
    let About {
        uses,
        polls,
        pooled,
        joins,
        take,
        first,
        root,
        me_index,
        tasks,
    } = *about;
    let me = needing_of(&f, given, uses, polls, tasks);
    let pool: Option<Value> = pooled[me_index].then(|| {
        f.types.push(NEXT);
        Value(f.types.len() - 1)
    });
    let start = |f: &mut Function| State {
        results: me.results.then(|| fresh(f)),
        temporaries: me.temporaries.then(|| fresh(f)),
    };
    // What the stacks are at the start of each block.
    let at_start: Vec<State> = (0..f.blocks.len()).map(|_| start(&mut f)).collect();
    for (block, start) in f.blocks.iter_mut().zip(&at_start) {
        block.params.splice(0..0, start.values());
    }
    let starts = at_start.clone();
    // What the stack for temporaries was where each arena of the function
    // opened, which its closing puts back. The blocks are taken so that the
    // block that opens an arena comes before the block that closes it.
    let mut opened: Vec<(Value, Option<[Value; 2]>)> = Vec::new();
    // What the stack for results was where each window of a region opened.
    let mut windows: Vec<(Value, Option<[Value; 2]>)> = Vec::new();
    let order = if opens_arena(&f) {
        reverse_postorder(&f)
    } else {
        (0..f.blocks.len()).collect()
    };
    // The frame of a task that has a stack of its own is the parameter after the
    // stacks, and the stack it runs on is the one the frame keeps.
    let own_stack = root.filter(|_| me.results);
    let frame = f.blocks[0].params.get(at_start[0].values().len()).copied();
    let caller = at_start[0].results;
    for k in order {
        let mut now = at_start[k];
        let instructions = std::mem::take(&mut f.blocks[k].instructions);
        let mut kept: Vec<Instruction> = Vec::with_capacity(instructions.len());
        if let (0, Some(at)) = (k, own_stack) {
            let [next, left] = load_pair(&mut f, &mut kept, frame.expect("the frame"), at);
            now.results = Some([next, left]);
        }
        for mut i in instructions {
            match &mut i.op {
                Op::System {
                    call: System::ArenaOpen,
                    ..
                } => {
                    // The function the program starts with has a piece for the
                    // stack first, as before a loop whose rounds release what
                    // they take, since what is taken in the arena is not
                    // remembered once the stack is put back.
                    if let (true, Some([at, left])) = (first, now.temporaries) {
                        let made = fresh(&mut f);
                        kept.push(Instruction {
                            results: made.to_vec(),
                            op: Op::Call {
                                function: take + 1,
                                args: with_pool(vec![at, left], pool),
                            },
                            at: Place::of("<stack_take>"),
                        });
                        now.temporaries = Some(made);
                    }
                    opened.push((i.results[0], now.temporaries));
                    continue;
                }
                Op::System {
                    call: System::ArenaClose,
                    args,
                } => {
                    let (_, saved) = opened
                        .iter()
                        .find(|(token, _)| *token == args[0])
                        .expect("an arena that was opened");
                    if now.temporaries.is_some() {
                        now.temporaries = *saved;
                        // What was moved into the stack above the place it goes
                        // back to is given back.
                        if let (true, Some(pair)) = (joins[me_index], *saved) {
                            kept.push(release_call(take, pool, pair));
                        }
                    }
                    continue;
                }
                Op::System {
                    call: System::TaskMark,
                    args,
                } => {
                    if let Some(pair) = now.results {
                        store_pair(&mut f, &mut kept, args[0], 0, pair);
                    }
                    continue;
                }
                Op::System {
                    call: System::TaskFrame,
                    args,
                } => {
                    kept.push(Instruction {
                        results: i.results.clone(),
                        op: Op::Call {
                            function: take + 2,
                            args: with_pool(vec![args[0]], pool),
                        },
                        at: Place::of("<stack_claim>"),
                    });
                    continue;
                }
                Op::System {
                    call: System::TaskFree,
                    args,
                } => {
                    kept.push(Instruction {
                        results: Vec::new(),
                        op: Op::Call {
                            function: take + 4,
                            args: with_pool(vec![args[0]], pool),
                        },
                        at: Place::of("<stack_free>"),
                    });
                    continue;
                }
                Op::System {
                    call: System::TaskMove,
                    args,
                } => {
                    let [at, left] = now.results.expect("the function takes the stack it asks");
                    let made = fresh(&mut f);
                    kept.push(Instruction {
                        results: made.to_vec(),
                        op: Op::Call {
                            function: take + 5,
                            args: with_pool(vec![at, left, args[0]], pool),
                        },
                        at: Place::of("<stack_move>"),
                    });
                    now.results = Some(made);
                    continue;
                }
                Op::System {
                    call: System::TaskRelease,
                    args,
                } => {
                    if now.results.is_some() {
                        let pair = load_pair(&mut f, &mut kept, args[0], 0);
                        now.results = Some(pair);
                        if joins[me_index] {
                            kept.push(release_call(take, pool, pair));
                        }
                    }
                    continue;
                }
                Op::System {
                    call: System::RegionNew,
                    ..
                } => {
                    if tasks {
                        // The frame of a region is the size of a task's: its
                        // header, and the words that say where its stack is.
                        let bytes = fresh_int(&mut f, &mut kept, REGION_BYTES);
                        kept.push(Instruction {
                            results: i.results.clone(),
                            op: Op::Call {
                                function: take + 2,
                                args: with_pool(vec![bytes], pool),
                            },
                            at: Place::of("<stack_claim>"),
                        });
                    } else {
                        // A program that starts no task has no pool, and no
                        // region that the operations could build in.
                        i.op = Op::Constant(Constant::Null);
                        kept.push(i);
                    }
                    continue;
                }
                Op::System {
                    call: System::RegionFree,
                    args,
                } => {
                    if tasks {
                        kept.push(Instruction {
                            results: Vec::new(),
                            op: Op::Call {
                                function: take + 4,
                                args: with_pool(vec![args[0]], pool),
                            },
                            at: Place::of("<stack_free>"),
                        });
                    }
                    continue;
                }
                Op::System {
                    call: System::RegionMove,
                    args,
                } => {
                    if tasks {
                        let [at, left] = now.results.expect("the function takes the stack it asks");
                        let made = fresh(&mut f);
                        kept.push(Instruction {
                            results: made.to_vec(),
                            op: Op::Call {
                                function: take + 5,
                                args: with_pool(vec![at, left, args[0]], pool),
                            },
                            at: Place::of("<stack_move>"),
                        });
                        now.results = Some(made);
                    }
                    continue;
                }
                Op::System {
                    call: System::RegionOpen,
                    args,
                } => {
                    if tasks {
                        // What is built for results in the window is built in
                        // the stack of the region, which its frame keeps.
                        windows.push((i.results[0], now.results));
                        let pair = load_pair(&mut f, &mut kept, args[0], REGION_STACK);
                        now.results = Some(pair);
                    }
                    continue;
                }
                Op::System {
                    call: System::RegionClose,
                    args,
                } => {
                    if tasks {
                        let at = windows
                            .iter()
                            .position(|(token, _)| *token == args[0])
                            .expect("a window that was opened");
                        let (_, saved) = windows.remove(at);
                        let pair = now.results.expect("the window has the stack of the region");
                        store_pair(&mut f, &mut kept, args[1], REGION_STACK, pair);
                        now.results = saved;
                    }
                    continue;
                }
                Op::System {
                    call: System::Allocate,
                    args,
                } => {
                    let next = fresh(&mut f);
                    let stack = if given[i.results[0].0] {
                        &mut now.results
                    } else {
                        &mut now.temporaries
                    };
                    let [at, left] = stack.expect("the function takes the stack it asks");
                    i.op = Op::Call {
                        function: take,
                        args: with_pool(vec![at, left, args[0]], pool),
                    };
                    i.results.splice(0..0, next);
                    *stack = Some(next);
                }
                Op::Call { function, args } if uses[*function].any() => {
                    let callee = uses[*function];
                    let returned = polls[*function] || gives_back(&f, given, &i.results);
                    // What the callee does with a stack it is given for its
                    // temporaries is not given back, so a stack that has no
                    // piece yet would lose the one it takes, and the callee may
                    // lend what it is given to a function that does so. The
                    // function the program starts with makes each stack it
                    // passes have a piece first.
                    if first && callee.temporaries {
                        let (own, other) = if returned {
                            (&mut now.results, &mut now.temporaries)
                        } else {
                            (&mut now.temporaries, &mut now.results)
                        };
                        let passed = [(own, callee.results), (other, true)];
                        for (stack, takes) in passed {
                            if !takes {
                                continue;
                            }
                            let [at, left] = stack.expect("the function takes what it gives");
                            let made = fresh(&mut f);
                            kept.push(Instruction {
                                results: made.to_vec(),
                                op: Op::Call {
                                    function: take + 1,
                                    args: with_pool(vec![at, left], pool),
                                },
                                at: Place::of("<stack_take>"),
                            });
                            *stack = Some(made);
                        }
                    }
                    // What the callee takes for its results and for its
                    // temporaries, from what the function has.
                    let (for_results, for_temporaries) = if returned {
                        (now.results, now.temporaries)
                    } else {
                        (now.temporaries, now.results)
                    };
                    let mut passed = Vec::new();
                    if callee.results {
                        passed.extend(for_results.expect("the function takes what it gives"));
                    }
                    if callee.temporaries {
                        passed.extend(for_temporaries.expect("the function takes what it gives"));
                    }
                    args.splice(0..0, passed);
                    if pooled[*function] {
                        args.insert(0, pool.expect("a function that takes the pool gives it"));
                    }
                    if callee.results {
                        // What the callee gives back is the stack it built
                        // its result in, moved on by it. The other goes on as
                        // it was before the call.
                        let next = fresh(&mut f);
                        i.results.splice(0..0, next);
                        if returned {
                            now.results = Some(next);
                        } else {
                            now.temporaries = Some(next);
                        }
                    }
                }
                _ => {}
            }
            kept.push(i);
        }
        // The function the program starts with has a piece for the stack before
        // a loop whose rounds release what they take, as before a call, since
        // the stack goes back to where it was and would otherwise ask the
        // system for a piece in each round.
        if let (true, Some([at, left])) = (first && rounds.enter[k], now.temporaries) {
            let made = fresh(&mut f);
            kept.push(Instruction {
                results: made.to_vec(),
                op: Op::Call {
                    function: take + 1,
                    args: with_pool(vec![at, left], pool),
                },
                at: Place::of("<stack_take>"),
            });
            now.temporaries = Some(made);
        }
        f.blocks[k].instructions = kept;
        // A jump that ends a round that hands nothing on goes on with the
        // stack for temporaries the round began with, which releases it.
        let state_for = |j: usize| {
            let mut state = now;
            if rounds.release[k][j] {
                let header = jump_of(&f.blocks[k].end, j).block;
                state.temporaries = starts[header].temporaries;
            }
            state
        };
        let by_jump: Vec<State> = (0..rounds.release[k].len()).map(state_for).collect();
        // What a round that hands nothing on had moved into the stack is given
        // back where the round ends: after the instructions of a block that has
        // one way on, and in a block of its own on the way that the round ends.
        let mut gives_back_on: Vec<(usize, usize, [Value; 2])> = Vec::new();
        for j in 0..rounds.release[k].len() {
            if let (true, true, Some(pair)) = (
                joins[me_index],
                rounds.release[k][j],
                starts[jump_of(&f.blocks[k].end, j).block].temporaries,
            ) {
                gives_back_on.push((k, j, pair));
            }
        }
        if let End::Jump(_) = &f.blocks[k].end {
            for (_, _, pair) in gives_back_on.drain(..) {
                f.blocks[k]
                    .instructions
                    .push(release_call(take, pool, pair));
            }
        }
        if let End::Return(_) = &f.blocks[k].end {
            // What the function's temporaries hold dies with it, among them the
            // tasks moved into them.
            if let (true, Some(pair)) = (joins[me_index], at_start[0].temporaries) {
                f.blocks[k]
                    .instructions
                    .push(release_call(take, pool, pair));
            }
            if let (Some(at), Some(pair)) = (own_stack, now.results) {
                // The task keeps its stack, and whoever polled it goes on with
                // the stack it gave.
                let mut tail = Vec::new();
                store_pair(&mut f, &mut tail, frame.expect("the frame"), at, pair);
                f.blocks[k].instructions.extend(tail);
            }
        }
        match &mut f.blocks[k].end {
            End::Jump(jump) => {
                jump.args.splice(0..0, by_jump[0].values());
            }
            End::Switch { jumps, .. } => {
                for (j, jump) in jumps.iter_mut().enumerate() {
                    jump.args.splice(0..0, by_jump[j].values());
                }
            }
            End::Return(values) => {
                if own_stack.is_some() && now.results.is_some() {
                    values.splice(0..0, caller.into_iter().flatten());
                } else {
                    values.splice(0..0, now.results.into_iter().flatten());
                }
            }
        }
        for (k, j, pair) in gives_back_on {
            let block = f.blocks.len();
            let End::Switch { jumps, .. } = &mut f.blocks[k].end else {
                continue;
            };
            let on = std::mem::replace(
                &mut jumps[j],
                Jump {
                    block: 0,
                    args: Vec::new(),
                },
            );
            jumps[j] = Jump {
                block,
                args: Vec::new(),
            };
            f.blocks.push(super::Block {
                params: Vec::new(),
                instructions: vec![release_call(take, pool, pair)],
                end: End::Jump(on),
            });
        }
    }
    if me.results {
        f.results.splice(0..0, PAIR);
    }
    if let Some(pool) = pool {
        f.blocks[0].params.insert(0, pool);
    }
    f
}

/// Whether the function opens an arena.
fn opens_arena(f: &Function) -> bool {
    (f.blocks.iter().flat_map(|b| &b.instructions)).any(|i| {
        matches!(
            i.op,
            Op::System {
                call: System::ArenaOpen | System::RegionOpen,
                ..
            }
        )
    })
}

/// The function without the openings and closings of arenas, for one that
/// takes no stack, so that there is nothing to put back.
fn without_arenas(mut f: Function) -> Function {
    for i in f.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        // With no task there is no region, so what asks for one gets none.
        if let Op::System {
            call: System::RegionNew,
            ..
        } = i.op
        {
            i.op = Op::Constant(Constant::Null);
        }
    }
    for block in &mut f.blocks {
        block.instructions.retain(|i| {
            !matches!(
                i.op,
                Op::System {
                    call: System::ArenaOpen
                        | System::ArenaClose
                        | System::RegionFree
                        | System::RegionMove
                        | System::RegionOpen
                        | System::RegionClose
                        | System::TaskMark
                        | System::TaskRelease
                        | System::TaskFrame
                        | System::TaskFree
                        | System::TaskMove,
                    ..
                }
            )
        });
    }
    f
}

/// The blocks that can be reached, each after the blocks that dominate it.
fn reverse_postorder(f: &Function) -> Vec<usize> {
    fn visit(f: &Function, b: usize, seen: &mut [bool], out: &mut Vec<usize>) {
        if seen[b] {
            return;
        }
        seen[b] = true;
        for j in super::graph::jumps(&f.blocks[b].end) {
            visit(f, j.block, seen, out);
        }
        out.push(b);
    }
    let mut seen = vec![false; f.blocks.len()];
    let mut out = Vec::new();
    visit(f, 0, &mut seen, &mut out);
    out.reverse();
    // A block that cannot be reached is still given the stacks.
    out.extend((0..f.blocks.len()).filter(|b| !seen[*b]));
    out
}

/// The jump at the place `j` of how a block ends.
fn jump_of(end: &End, j: usize) -> &Jump {
    match end {
        End::Jump(jump) => jump,
        End::Switch { jumps, .. } => &jumps[j],
        End::Return(_) => unreachable!("a block that returns has no jump"),
    }
}

/// Which stacks the function takes.
fn needing_of(f: &Function, given: &[bool], uses: &[Uses], polls: &[bool], regions: bool) -> Uses {
    let mut now = Uses::default();
    for i in f.blocks.iter().flat_map(|b| &b.instructions) {
        match &i.op {
            Op::System {
                call: System::Allocate,
                ..
            } => {
                if given[i.results[0].0] {
                    now.results = true;
                } else {
                    now.temporaries = true;
                }
            }
            Op::System {
                call: System::TaskFrame | System::TaskFree | System::TaskMove,
                ..
            } => now.results = true,
            // A region is made, windowed, given back, and moved only where tasks
            // are, and gives the function its stack for results to move it into
            // and to put back.
            Op::System {
                call:
                    System::RegionNew
                    | System::RegionFree
                    | System::RegionMove
                    | System::RegionOpen
                    | System::RegionClose,
                ..
            } if regions => now.results = true,
            Op::Call { function, .. } if uses[*function].any() => {
                let callee = uses[*function];
                if polls[*function] || gives_back(f, given, &i.results) {
                    now.results |= callee.results;
                    now.temporaries |= callee.temporaries;
                } else {
                    now.temporaries |= callee.results;
                    now.results |= callee.temporaries;
                }
            }
            _ => {}
        }
    }
    now
}

/// Takes the pair of a stack and a number of bytes, and gives the pair after
/// it and the address of that many bytes. The count is made a multiple of 8,
/// so that every address is. When the piece has the room, it is a compare, an
/// add, and a subtraction. When it has not, the stack goes to the piece that
/// follows when there is one that is big enough, and otherwise the system
/// gives a piece of 64 KB, or of what the count and the end of a piece need
/// rounded up to a multiple of 64 KB when that is more, and it is put between
/// the two. What was left of the old piece is not used again until the stack
/// is back at its start. A piece ends with the number of bytes it holds and the
/// address of the end of the piece after it, or nothing, so that a piece is
/// found again from where its stack ends. A count too large to round is asked
/// of the system as it is, and refused.
pub fn take() -> Function {
    take_with(None)
}

/// `take`, and for a program that has tasks `take` that asks the pool of the
/// program, which is its first parameter, for a piece of the smallest class that
/// holds what is needed and at least twice what the piece the stack is in holds,
/// so that a stack starts small and doubles, in place of asking the system for
/// 64 KB. `pool_take` is where the pool's `take_sized` is.
pub fn take_with(pool_take: Option<usize>) -> Function {
    let mut f = Builder::new();
    let pool = pool_take.map(|_| f.params(&[NEXT])[0]);
    let p = f.params(&[NEXT, LEFT, LEFT]);
    let (next, left, count) = (p[0], p[1], p[2]);
    let seven = runtime::int(&mut f, 7);
    let padded = f.binary(Binary::Add, count, seven);
    let mask = runtime::int(&mut f, !7);
    let size = f.binary(Binary::And, padded, mask);
    let huge_from = runtime::int(&mut f, 1 << 62);
    let huge = f.compare(Compare::LessU, huge_from, count);
    let short = f.compare(Compare::LessU, left, size);
    let no_room = f.binary(Binary::Or, short, huge);
    let (slow, _) = f.block(&[]);
    let (fast, _) = f.block(&[]);
    let (look, _) = f.block(&[]);
    let (have, _) = f.block(&[]);
    let (reuse, _) = f.block(&[]);
    // What the system is asked for, the end of the piece the stack was in, the
    // piece that follows it, and whether that end is a piece's to write in.
    let (ask, a) = f.block(&[LEFT, NEXT, NEXT, Type::Int(Bits::B8)]);
    let (done, _) = f.block(&[]);
    let (fix, _) = f.block(&[]);
    let (trying, _) = f.block(&[]);
    let (ask_raw, _) = f.block(&[]);
    let (attempt, _) = f.block(&[]);
    let (grant, _) = f.block(&[]);
    let (refuse, _) = f.block(&[]);
    f.switch(no_room, vec![(fast, vec![]), (slow, vec![])]);
    f.at(fast);
    let after = f.binary(Binary::Add, next, size);
    let rest = f.binary(Binary::Sub, left, size);
    f.ret(vec![after, rest, next]);
    // The end of the piece is where it keeps the bytes it holds and where the
    // piece after it ends. A stack that has no piece is at one that holds 0.
    f.at(slow);
    let end = f.binary(Binary::Add, next, left);
    let eight = runtime::int(&mut f, 8);
    let link = f.binary(Binary::Add, end, eight);
    let holds = f.load(LEFT, end);
    let zero = runtime::int(&mut f, 0);
    let bare = f.compare(Compare::Equal, holds, zero);
    let one = f.int(Bits::B8, 1);
    let patch = f.binary(Binary::Xor, bare, one);
    let linked = f.load(LEFT, link);
    let nothing = f.compare(Compare::Equal, linked, zero);
    let after_at = f.load(NEXT, link);
    // A request that no piece of the usual size has room for is given a piece
    // with room for it to double, which a list that grows does, in place.
    let fits_from = runtime::int(&mut f, PIECE - TRAILER);
    let large = f.compare(Compare::LessU, fits_from, size);
    let twice = f.binary(Binary::Add, size, size);
    let room = f.choose(large, twice, size);
    let wanted = if pool_take.is_some() {
        // What the request needs and its trailer, or twice what the piece the
        // stack is in holds and a trailer when that is more.
        let trailer = runtime::int(&mut f, TRAILER_RECORDS);
        let base = f.binary(Binary::Add, room, trailer);
        let doubled = f.binary(Binary::Add, holds, holds);
        let grown = f.binary(Binary::Add, doubled, trailer);
        let more = f.compare(Compare::LessU, base, grown);
        f.choose(more, grown, base)
    } else {
        let more = runtime::int(&mut f, TRAILER + PIECE - 1);
        let up = f.binary(Binary::Add, room, more);
        let pieces = runtime::int(&mut f, !(PIECE - 1));
        f.binary(Binary::And, up, pieces)
    };
    let none = f.null();
    f.switch(huge, vec![(look, vec![]), (trying, vec![])]);
    // A count with its top bit set asks for the rest of it only if the piece
    // has room, and is given nothing when it has not, so that a list can ask
    // for the room it lacks after it without taking a piece it will not use.
    f.at(trying);
    let flag = runtime::int(&mut f, TRY);
    let one_word = runtime::int(&mut f, 1);
    let below = f.binary(Binary::Sub, flag, one_word);
    let tried = f.compare(Compare::LessU, below, count);
    f.switch(tried, vec![(ask_raw, vec![]), (attempt, vec![])]);
    f.at(ask_raw);
    f.jump(ask, vec![count, end, none, patch]);
    f.at(attempt);
    let wanted_bytes = f.binary(Binary::And, count, below);
    let wanted_up = f.binary(Binary::Add, wanted_bytes, seven);
    let wanted_size = f.binary(Binary::And, wanted_up, mask);
    let lacks = f.compare(Compare::LessU, left, wanted_size);
    f.switch(lacks, vec![(grant, vec![]), (refuse, vec![])]);
    f.at(grant);
    let after = f.binary(Binary::Add, next, wanted_size);
    let rest = f.binary(Binary::Sub, left, wanted_size);
    f.ret(vec![after, rest, next]);
    f.at(refuse);
    f.ret(vec![next, left, none]);
    f.at(look);
    f.switch(
        nothing,
        vec![(have, vec![]), (ask, vec![wanted, end, none, patch])],
    );
    f.at(have);
    let holds_after = f.load(LEFT, after_at);
    let small = f.compare(Compare::LessU, holds_after, size);
    f.switch(
        small,
        vec![(reuse, vec![]), (ask, vec![wanted, end, after_at, patch])],
    );
    f.at(reuse);
    let back = f.binary(Binary::Sub, zero, holds_after);
    let start = f.binary(Binary::Add, after_at, back);
    let after = f.binary(Binary::Add, start, size);
    let rest = f.binary(Binary::Sub, holds_after, size);
    f.ret(vec![after, rest, start]);
    f.at(ask);
    let (asked, old_end, old_link, patch) = (a[0], a[1], a[2], a[3]);
    // What the system or the pool gives, as the order of the instructions was
    // when only the system gave it.
    let from_pool = match (pool_take, pool) {
        (Some(from_pool), Some(pool)) => Some(f.call(from_pool, vec![pool, asked], &[NEXT, LEFT])),
        _ => None,
    };
    let start = match &from_pool {
        Some(got) => got[0],
        None => f.allocate(asked),
    };
    let trailer = runtime::int(
        &mut f,
        if pool_take.is_some() {
            TRAILER_RECORDS
        } else {
            TRAILER
        },
    );
    let holds_now = match &from_pool {
        Some(got) => f.binary(Binary::Sub, got[1], trailer),
        None => f.binary(Binary::Sub, asked, trailer),
    };
    let new_end = f.binary(Binary::Add, start, holds_now);
    f.store(new_end, holds_now);
    let new_link = f.binary(Binary::Add, new_end, eight);
    f.store(new_link, old_link);
    if pool_take.is_some() {
        // No task is moved into a stack in a piece that is new.
        let zero = runtime::int(&mut f, 0);
        let sixteen = runtime::int(&mut f, 16);
        let records = f.binary(Binary::Add, new_end, sixteen);
        f.store(records, zero);
    }
    let after = f.binary(Binary::Add, start, size);
    let rest = f.binary(Binary::Sub, holds_now, size);
    f.switch(patch, vec![(done, vec![]), (fix, vec![])]);
    f.at(done);
    f.ret(vec![after, rest, start]);
    f.at(fix);
    let old_link_at = f.binary(Binary::Add, old_end, eight);
    f.store(old_link_at, new_end);
    f.ret(vec![after, rest, start]);
    f.finish(&[NEXT, LEFT, Type::Address])
}

/// Takes the pair of a stack, and gives it as it is when it has a piece, and
/// otherwise a piece the system gives, which holds 8 bytes less than it did.
/// It is what makes a stack that a callee may use for its temporaries one that
/// remembers the piece, since the callee does not give the stack back.
pub fn ready(take: usize) -> Function {
    ready_with(take, false)
}

/// `ready`, for a program that has tasks, with the pool before the pair.
pub fn ready_with(take: usize, pooled: bool) -> Function {
    let mut f = Builder::new();
    let pool = pooled.then(|| f.params(&[NEXT])[0]);
    let p = f.params(&[NEXT, LEFT]);
    let (next, left) = (p[0], p[1]);
    let (has, _) = f.block(&[]);
    let (bare, _) = f.block(&[]);
    let end = f.binary(Binary::Add, next, left);
    let holds = f.load(LEFT, end);
    let zero = runtime::int(&mut f, 0);
    let none = f.compare(Compare::Equal, holds, zero);
    f.switch(none, vec![(has, vec![]), (bare, vec![])]);
    f.at(has);
    f.ret(vec![next, left]);
    f.at(bare);
    let one = runtime::int(&mut f, 1);
    let mut args = vec![next, left, one];
    if let Some(pool) = pool {
        args.insert(0, pool);
    }
    let made = f.call(take, args, &[NEXT, LEFT, NEXT]);
    f.ret(vec![made[0], made[1]]);
    f.finish(&PAIR)
}

/// What starts the program: it calls `main`, which took what the system gives,
/// with stacks that have no bytes before them.
fn entry(
    main: &Function,
    at: usize,
    uses: Uses,
    empty: usize,
    pool: Option<(usize, bool)>,
) -> Function {
    let mut f = Builder::new();
    let types: Vec<Type> = (main.blocks[0].params.iter())
        .map(|v| main.types[v.0])
        .collect();
    let params = f.params(&types);
    let mut args = Vec::new();
    if let Some((make, true)) = pool {
        args.push(f.call(make, vec![], &[NEXT])[0]);
    }
    for _ in 0..usize::from(uses.results) + usize::from(uses.temporaries) {
        args.push(f.emit(Op::Constant(Constant::Data(empty)), &[NEXT])[0]);
        args.push(runtime::int(&mut f, 0));
    }
    args.extend(params);
    let mut results = Vec::new();
    if uses.results {
        results.extend(PAIR);
    }
    results.extend(&main.results);
    f.call(at, args, &results);
    f.ret(Vec::new());
    f.finish(&[])
}

/// Loads the pair of a stack that is at `by` bytes from `base`, and gives its
/// values.
fn load_pair(f: &mut Function, kept: &mut Vec<Instruction>, base: Value, by: u64) -> [Value; 2] {
    let make = |f: &mut Function, kept: &mut Vec<Instruction>, op: Op, ty: Type| {
        f.types.push(ty);
        let value = Value(f.types.len() - 1);
        kept.push(Instruction {
            results: vec![value],
            op,
            at: Place::NONE,
        });
        value
    };
    let at = |f: &mut Function, kept: &mut Vec<Instruction>, by: u64| {
        let offset = make(f, kept, Op::Constant(Constant::Int(by)), LEFT);
        make(
            f,
            kept,
            Op::Binary {
                op: Binary::Add,
                a: base,
                b: offset,
            },
            Type::Address,
        )
    };
    let first = at(f, kept, by);
    let second = at(f, kept, by + 8);
    let next = make(
        f,
        kept,
        Op::Load {
            ty: NEXT,
            address: first,
        },
        NEXT,
    );
    let left = make(
        f,
        kept,
        Op::Load {
            ty: LEFT,
            address: second,
        },
        LEFT,
    );
    [next, left]
}

/// Stores the pair of a stack at `by` bytes from `base`.
fn store_pair(
    f: &mut Function,
    kept: &mut Vec<Instruction>,
    base: Value,
    by: u64,
    pair: [Value; 2],
) {
    let at = |f: &mut Function, kept: &mut Vec<Instruction>, by: u64| {
        f.types.push(LEFT);
        let offset = Value(f.types.len() - 1);
        kept.push(Instruction {
            results: vec![offset],
            op: Op::Constant(Constant::Int(by)),
            at: Place::NONE,
        });
        f.types.push(Type::Address);
        let address = Value(f.types.len() - 1);
        kept.push(Instruction {
            results: vec![address],
            op: Op::Binary {
                op: Binary::Add,
                a: base,
                b: offset,
            },
            at: Place::NONE,
        });
        address
    };
    let first = at(f, kept, by);
    let second = at(f, kept, by + 8);
    kept.push(Instruction {
        results: Vec::new(),
        op: Op::Store {
            ty: NEXT,
            address: first,
            value: pair[0],
        },
        at: Place::NONE,
    });
    kept.push(Instruction {
        results: Vec::new(),
        op: Op::Store {
            ty: LEFT,
            address: second,
            value: pair[1],
        },
        at: Place::NONE,
    });
}

/// Makes the frame of a task, and gives it a stack for results. The frame is a
/// piece of the pool of its own, which `free_one` gives back when the task is
/// joined, with the result read: what is moved to the one that joins is the
/// stack, which starts as the smallest piece there is and doubles. The frame says
/// where the stack is and where it started, which `free` needs, in the words
/// after its header's two. The function takes the pool and the bytes of the frame,
/// and gives the address of the frame.
pub fn claim(pool_take: usize) -> Function {
    let mut f = Builder::new();
    let p = f.params(&[NEXT, LEFT]);
    let (pool, bytes) = (p[0], p[1]);
    let got = f.call(pool_take, vec![pool, bytes], &[NEXT, LEFT]);
    let frame = got[0];
    let one = runtime::int(&mut f, 1);
    let stack = f.call(pool_take, vec![pool, one], &[NEXT, LEFT]);
    let (first, capacity) = (stack[0], stack[1]);
    // The piece of the stack ends as the pieces of a stack do: with the bytes it
    // holds, the end of the piece after it, which is none, and the head of the
    // tasks moved into it, which is none.
    let trailer = runtime::int(&mut f, TRAILER_RECORDS);
    let holds = f.binary(Binary::Sub, capacity, trailer);
    let end = f.binary(Binary::Add, first, holds);
    f.store(end, holds);
    let eight = runtime::int(&mut f, 8);
    let link = f.binary(Binary::Add, end, eight);
    let none = f.null();
    f.store(link, none);
    let sixteen = runtime::int(&mut f, 16);
    let records = f.binary(Binary::Add, end, sixteen);
    let nothing = runtime::int(&mut f, 0);
    f.store(records, nothing);
    let words = [16u64, 24, 32, 40];
    let values = [first, holds, first, holds];
    for (by, value) in words.into_iter().zip(values) {
        let by = runtime::int(&mut f, by);
        let at = f.binary(Binary::Add, frame, by);
        f.store(at, value);
    }
    // The words a scheduler keeps its queues in start as nothing.
    let zero = runtime::int(&mut f, 0);
    for by in [48u64, 56, 64, 72] {
        let by = runtime::int(&mut f, by);
        let at = f.binary(Binary::Add, frame, by);
        f.store(at, zero);
    }
    f.ret(vec![frame]);
    f.finish(&[NEXT])
}

/// Slots to say an address as a number and a number as an address, which the
/// scalar form has no instruction between.
fn number_of(f: &mut Builder, address: Value) -> Value {
    let slot = f.emit(Op::Frame { bytes: 8 }, &[Type::Address])[0];
    f.store(slot, address);
    f.load(LEFT, slot)
}

fn address_of(f: &mut Builder, number: Value) -> Value {
    let slot = f.emit(Op::Frame { bytes: 8 }, &[Type::Address])[0];
    f.store(slot, number);
    f.load(NEXT, slot)
}

/// Gives the pieces of every stack on a list back to the pool, and of the stacks
/// that were moved into them. The list is of records, each of where a stack
/// started, the bytes its first piece holds, and the next record on the list, and
/// the function is given the first. A stack is the pieces from its first, each
/// followed by the one its end says; the records at the end of a piece are those
/// of the stacks moved into the stack in it, which are put on the list before the
/// piece is given back, each with its link in the place of the record before it,
/// which is read first. The function takes the pool and the first record of the
/// list, or none.
pub fn free(pool_give: usize) -> Function {
    let mut f = Builder::new();
    let p = f.params(&[NEXT, NEXT]);
    let (pool, first) = (p[0], p[1]);
    let (outer, o) = f.block(&[LEFT]);
    let (open, _) = f.block(&[]);
    let (done, _) = f.block(&[]);
    let (piece, q) = f.block(&[NEXT, LEFT, LEFT]);
    let (records, r) = f.block(&[LEFT, LEFT]);
    let (take_record, t) = f.block(&[LEFT, LEFT]);
    let (after, a) = f.block(&[LEFT]);
    let (onward, _) = f.block(&[]);
    let zero = runtime::int(&mut f, 0);
    let eight = runtime::int(&mut f, 8);
    let at = |f: &mut Builder, base: Value, by: u64| {
        let by = runtime::int(f, by);
        f.binary(Binary::Add, base, by)
    };
    let start = number_of(&mut f, first);
    f.jump(outer, vec![start]);
    f.at(outer);
    let pending = o[0];
    let empty = f.compare(Compare::Equal, pending, zero);
    f.switch(empty, vec![(open, vec![]), (done, vec![])]);
    f.at(done);
    f.ret(Vec::new());
    f.at(open);
    let item = address_of(&mut f, pending);
    let origin = f.load(NEXT, item);
    let held_at = f.binary(Binary::Add, item, eight);
    let holds = f.load(LEFT, held_at);
    let link_at = at(&mut f, item, 16);
    let next_pending = f.load(LEFT, link_at);
    f.jump(piece, vec![origin, holds, next_pending]);
    // A piece of the stack: the records at its end, then the piece after it.
    f.at(piece);
    let (here, held, pend) = (q[0], q[1], q[2]);
    let end = f.binary(Binary::Add, here, held);
    let link = at(&mut f, end, 8);
    let recs = at(&mut f, end, 16);
    let following = f.load(NEXT, link);
    let following_word = f.load(LEFT, link);
    let head = f.load(LEFT, recs);
    f.jump(records, vec![head, pend]);
    f.at(records);
    let (record, pending_now) = (r[0], r[1]);
    let no_more = f.compare(Compare::Equal, record, zero);
    f.switch(
        no_more,
        vec![
            (take_record, vec![record, pending_now]),
            (after, vec![pending_now]),
        ],
    );
    // A record: where the stack moved in started, the bytes its first piece holds,
    // and the record before.
    f.at(take_record);
    let (record, pending_now) = (t[0], t[1]);
    let record_at = address_of(&mut f, record);
    let prev_at = at(&mut f, record_at, 16);
    let prev = f.load(LEFT, prev_at);
    f.store(prev_at, pending_now);
    f.jump(records, vec![prev, record]);
    // The piece goes back once what it holds that matters is read.
    f.at(after);
    let pending_after = a[0];
    f.call(pool_give, vec![pool, here], &[]);
    let last = f.compare(Compare::Equal, following_word, zero);
    f.switch(last, vec![(onward, vec![]), (outer, vec![pending_after])]);
    f.at(onward);
    let next_held = f.load(LEFT, following);
    let back = f.binary(Binary::Sub, zero, next_held);
    let next_piece = f.binary(Binary::Add, following, back);
    f.jump(piece, vec![next_piece, next_held, pending_after]);
    f.finish(&[])
}

/// Gives a task's pieces back to the pool: the stack from where it started, and
/// what was moved into it, and the frame, which is a piece of its own. A task
/// whose frame says it has no stack, because it was moved, gives only the frame.
/// It takes the pool and the address of the frame.
pub fn free_one(free_all: usize, pool_give: usize) -> Function {
    let mut f = Builder::new();
    let p = f.params(&[NEXT, Type::Address]);
    let (pool, frame) = (p[0], p[1]);
    let (stack, _) = f.block(&[]);
    let (frame_only, _) = f.block(&[]);
    let zero = runtime::int(&mut f, 0);
    let at = |f: &mut Builder, by: u64| {
        let by = runtime::int(f, by);
        f.binary(Binary::Add, frame, by)
    };
    let record = at(&mut f, 32);
    let holds_at = at(&mut f, 40);
    let link_at = at(&mut f, 48);
    let holds = f.load(LEFT, holds_at);
    let moved = f.compare(Compare::Equal, holds, zero);
    f.switch(moved, vec![(stack, vec![]), (frame_only, vec![])]);
    // The words of the frame that say where the stack started and what its first
    // piece holds are a record, with the word after them for the link.
    f.at(stack);
    f.store(link_at, zero);
    f.call(free_all, vec![pool, record], &[]);
    f.call(pool_give, vec![pool, frame], &[]);
    f.ret(Vec::new());
    f.at(frame_only);
    f.call(pool_give, vec![pool, frame], &[]);
    f.ret(Vec::new());
    f.finish(&[])
}

/// Moves the stack of a task into a stack, with no byte moved: a record of where it
/// started, the bytes its first piece holds, and the record before is made at the
/// top of the stack, and the head of the records of the piece the stack is in is
/// the new one, so that a put-back of the stack that passes below the record gives
/// the stack's pieces back. The frame of the task is given back at once, since
/// what the result holds is in the stack. It takes the pool, the pair of the stack
/// and the frame of the task, and gives the pair after the record.
pub fn move_in(take: usize, pool_give: usize) -> Function {
    let mut f = Builder::new();
    let p = f.params(&[NEXT, NEXT, LEFT, Type::Address]);
    let (pool, next, left, frame) = (p[0], p[1], p[2], p[3]);
    let size = runtime::int(&mut f, 24);
    let eight = runtime::int(&mut f, 8);
    let sixteen = runtime::int(&mut f, 16);
    let zero = runtime::int(&mut f, 0);
    let made = f.call(take, vec![pool, next, left, size], &[NEXT, LEFT, NEXT]);
    let (after, rest, record) = (made[0], made[1], made[2]);
    // The record is in the piece the stack is in now, whose end is where the
    // stack's room ends.
    let end = f.binary(Binary::Add, after, rest);
    let recs = f.binary(Binary::Add, end, sixteen);
    let prev = f.load(LEFT, recs);
    let thirty_two = runtime::int(&mut f, 32);
    let forty = runtime::int(&mut f, 40);
    let origin_at = f.binary(Binary::Add, frame, thirty_two);
    let holds_at = f.binary(Binary::Add, frame, forty);
    let origin = f.load(LEFT, origin_at);
    let holds = f.load(LEFT, holds_at);
    f.store(record, origin);
    let holds_in = f.binary(Binary::Add, record, eight);
    f.store(holds_in, holds);
    let prev_in = f.binary(Binary::Add, record, sixteen);
    f.store(prev_in, prev);
    let record_number = number_of(&mut f, record);
    f.store(recs, record_number);
    f.store(holds_at, zero);
    f.call(pool_give, vec![pool, frame], &[]);
    f.ret(vec![after, rest]);
    f.finish(&PAIR)
}

/// Gives back the stacks that were moved into a stack above the place a pair says,
/// when the stack is put back to it: the records in the piece it is in that are
/// at or above the place, and all those in the pieces after it. It takes the pool
/// and the pair.
pub fn release_above(free_all: usize) -> Function {
    let mut f = Builder::new();
    let p = f.params(&[NEXT, NEXT, LEFT]);
    let (pool, next, left) = (p[0], p[1], p[2]);
    let (top, t) = f.block(&[LEFT, LEFT]);
    let (pop, pp) = f.block(&[LEFT, LEFT]);
    let (settle, s) = f.block(&[LEFT, LEFT]);
    let (rest, r) = f.block(&[LEFT, LEFT]);
    let (visit, v) = f.block(&[LEFT, LEFT]);
    let (drain, d) = f.block(&[LEFT, LEFT]);
    let (finish, fin) = f.block(&[LEFT]);
    let (call_free, _) = f.block(&[]);
    let (ret, _) = f.block(&[]);
    let zero = runtime::int(&mut f, 0);
    let eight = runtime::int(&mut f, 8);
    let sixteen = runtime::int(&mut f, 16);
    let end = f.binary(Binary::Add, next, left);
    let recs = f.binary(Binary::Add, end, sixteen);
    let link = f.binary(Binary::Add, end, eight);
    let here = number_of(&mut f, next);
    let head = f.load(LEFT, recs);
    f.jump(top, vec![head, zero]);
    // The records of this piece that are at or above the place.
    f.at(top);
    let (record, pending) = (t[0], t[1]);
    let none = f.compare(Compare::Equal, record, zero);
    let below = f.compare(Compare::LessU, record, here);
    let stop = f.binary(Binary::Or, none, below);
    f.switch(
        stop,
        vec![
            (pop, vec![record, pending]),
            (settle, vec![record, pending]),
        ],
    );
    f.at(pop);
    let (record, pending) = (pp[0], pp[1]);
    let (prev, origin) = put_on_list(&mut f, record, pending);
    f.jump(top, vec![prev, origin]);
    // The head is written back only when a record was taken, since a stack that
    // has no piece is at a trailer that nothing may write.
    f.at(settle);
    let (write_back, _) = f.block(&[]);
    let (go_on_after, _) = f.block(&[]);
    let unchanged = f.compare(Compare::Equal, s[0], head);
    f.switch(unchanged, vec![(write_back, vec![]), (go_on_after, vec![])]);
    f.at(write_back);
    f.store(recs, s[0]);
    f.jump(go_on_after, vec![]);
    f.at(go_on_after);
    let after_this = f.load(LEFT, link);
    f.jump(rest, vec![after_this, s[1]]);
    // Every record of the pieces after it.
    f.at(rest);
    let (piece, pending) = (r[0], r[1]);
    let last = f.compare(Compare::Equal, piece, zero);
    f.switch(
        last,
        vec![(visit, vec![piece, pending]), (finish, vec![pending])],
    );
    f.at(visit);
    let (piece, pending) = (v[0], v[1]);
    let piece_end = address_of(&mut f, piece);
    let piece_recs = f.binary(Binary::Add, piece_end, sixteen);
    let head = f.load(LEFT, piece_recs);
    f.store(piece_recs, zero);
    f.jump(drain, vec![head, pending]);
    f.at(drain);
    let (record, pending) = (d[0], d[1]);
    let done = f.compare(Compare::Equal, record, zero);
    let (take_it, _) = f.block(&[]);
    let (go_on, _) = f.block(&[]);
    f.switch(done, vec![(take_it, vec![]), (go_on, vec![])]);
    f.at(take_it);
    let (prev, origin) = put_on_list(&mut f, record, pending);
    f.jump(drain, vec![prev, origin]);
    f.at(go_on);
    // The next piece is the one this piece's end says, read again from the piece
    // the records were in.
    let piece_end_again = address_of(&mut f, v[0]);
    let link_again = f.binary(Binary::Add, piece_end_again, eight);
    let next_piece = f.load(LEFT, link_again);
    f.jump(rest, vec![next_piece, pending]);
    f.at(finish);
    let nothing = f.compare(Compare::Equal, fin[0], zero);
    f.switch(nothing, vec![(call_free, vec![]), (ret, vec![])]);
    f.at(call_free);
    let list = address_of(&mut f, fin[0]);
    f.call(free_all, vec![pool, list], &[]);
    f.ret(Vec::new());
    f.at(ret);
    f.ret(Vec::new());
    f.finish(&[])
}

/// Takes a record, and puts it on the list of those to be given back, whose first
/// is a number, with the record before it in the place of the link. It gives the
/// record before and the record, which is the new first of the list.
fn put_on_list(f: &mut Builder, record: Value, pending: Value) -> (Value, Value) {
    let sixteen = runtime::int(f, 16);
    let record_at = address_of(f, record);
    let prev_at = f.binary(Binary::Add, record_at, sixteen);
    let prev = f.load(LEFT, prev_at);
    f.store(prev_at, pending);
    (prev, record)
}

/// The arguments of a call of a runtime function, with the pool before them when
/// the program has one.
fn with_pool(mut args: Vec<Value>, pool: Option<Value>) -> Vec<Value> {
    if let Some(pool) = pool {
        args.insert(0, pool);
    }
    args
}

/// The call that gives back what was moved into a stack above the place its pair
/// says, for a stack that is put back to that place.
fn release_call(take: usize, pool: Option<Value>, pair: [Value; 2]) -> Instruction {
    Instruction {
        results: Vec::new(),
        op: Op::Call {
            function: take + 6,
            args: with_pool(vec![pair[0], pair[1]], pool),
        },
        at: Place::of("<stack_release>"),
    }
}

/// Which functions reach a `TaskMove`, through the calls they make.
fn joining_functions(program: &Program) -> Vec<bool> {
    let mut found: Vec<bool> = program
        .functions
        .iter()
        .map(|f| {
            (f.blocks.iter().flat_map(|b| &b.instructions)).any(|i| {
                matches!(
                    i.op,
                    Op::System {
                        call: System::TaskMove | System::RegionMove,
                        ..
                    }
                )
            })
        })
        .collect();
    loop {
        let mut changed = false;
        for (k, f) in program.functions.iter().enumerate() {
            let calls = (f.blocks.iter().flat_map(|b| &b.instructions))
                .any(|i| matches!(&i.op, Op::Call { function, .. } if found[*function]));
            if calls && !found[k] {
                found[k] = true;
                changed = true;
            }
        }
        if !changed {
            return found;
        }
    }
}
