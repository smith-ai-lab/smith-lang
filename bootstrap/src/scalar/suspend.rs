//! The pass that makes a function that suspends a state machine, so that a task
//! needs no stack of its own: what it holds while it waits is in a frame, in
//! memory that its task owns.
//!
//! A function suspends when it runs `System::Suspend`, or calls a function that
//! suspends. Each such function `f` becomes two. `f` itself, at the same place in
//! the program, is the start: it takes the address of its frame and then what it
//! took, saves them in the frame, and gives nothing. The poll, a function made
//! after the others, takes the address of the frame and runs the task to its next
//! wait or to its end, and gives a status, the kind and handle of the wait, and
//! then what `f` gave: the status is 0 when the task is done, and the rest is
//! its values, or 1 when it waits, and the rest is zeros. A task that waits is
//! resumed by polling its frame again, and the function that called it, which
//! polled the frame of the callee, goes down the chain again.
//!
//! The frame has a layout of its own for each function: the number of the state,
//! the values `f` took, the bytes of each `Frame` instruction, one place for each
//! value that is alive across a suspension, and room for the frame of each
//! suspending function it calls, so a frame holds the frames of everything the
//! task may be in the middle of and nothing is asked of the system while the
//! task runs. A value that is alive across a suspension is written to its place
//! where it is made and read from it where it is used, so no path that resumes
//! needs the value made again. The poll starts by going to the state the frame
//! says, and each suspension is a state of its own: a wait gives the wait and
//! says where to go on, and a call of a function that suspends starts the callee
//! in its place, polls it, goes on when it is done, and gives its wait when it is
//! not, with the state of the call so that the next poll goes to the callee.
//!
//! The pass runs after `recur`, so no function reaches itself: a function that
//! called itself has a stack of its own in memory, and the pointers to that stack
//! are values like any other, saved in the frame across a wait. A frame therefore
//! holds a fixed number of bytes however deep a recursion goes. A program that
//! still has a function that reaches itself is not one the pass is given.

use std::collections::{HashMap, HashSet};

use super::{
    Binary, Bits, Block, Constant, End, Function, Instruction, Jump, Op, Place, Program, System,
    Type, Value,
};

/// A function that suspends, as the program that `suspend` gives has it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Task {
    /// The start, which is where the function was: it takes the address of the
    /// frame and then the parameters the function took.
    pub start: usize,
    /// The poll: it takes the address of the frame and the state of the
    /// scheduler, and gives that state, and a status that is 0
    /// when the task is done and 1 when it waits, and the kind and the handle of
    /// the wait. The values the function gave are in the frame when it is done,
    /// at `result_at`.
    pub poll: usize,
    /// The number in the frame's header that says which poll runs it, so that a
    /// frame is polled without knowing which function it is of.
    pub id: u64,
    /// Where in the frame each value the function gave is, once it is done, in
    /// the order the function gave them.
    pub result_at: Vec<u64>,
    /// How many bytes the frame has, and so how many the one who starts the
    /// task must give it. The address is a multiple of 16.
    pub bytes: u64,
    /// Where in the frame the task keeps its own stack for results, when the
    /// task is one that a frame is made of: the poll runs on it, and not on the
    /// stack of whoever polls, so that what the task builds is its own, and
    /// an arena of the task puts back no memory that another task built.
    pub stack_at: Option<u64>,
    /// The types of the values the poll gives after the wait, which are the
    /// types the function gave.
    pub results: Vec<Type>,
}

/// What the lowering asks of `suspend` for the frames of tasks that a program
/// makes: a frame is made by a function that the lowering placed, whose body is
/// the pass's to write, because only the pass knows how many bytes the frame has.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Frames {
    /// The place of the function that polls any frame, where the pass puts it,
    /// when the program polls one.
    pub poll_any: Option<usize>,
    /// The functions that make a frame.
    pub starters: Vec<Starter>,
    /// The types of the state of the scheduler that polls the frames, which is
    /// given to each poll and given back by it, and which no frame saves. Each
    /// function that suspends takes these as its last parameters and gives them
    /// as its last results, and `System::Suspend` takes them after the kind and
    /// the handle and gives them as the state the task goes on with. They are
    /// last, after what the function takes and gives of its own, as the state of
    /// any handler is.
    pub through: Vec<Type>,
}

/// A function that makes the frame of another and starts it. The lowering gives
/// it the types of the parameters of the function the frame is of, and the types
/// of what it gives, an address and a length; the pass writes its body: it asks
/// the arena in force for the bytes of the frame, starts the function in them
/// with the parameters, and gives the address of the frame and how many bytes
/// it has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Starter {
    /// The place of the function that makes the frame.
    pub place: usize,
    /// The function the frame is of, which is a task whether it waits or not.
    pub function: usize,
}

/// The program with each function that suspends made a state machine, and each
/// of those functions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Suspended {
    pub program: Program,
    pub tasks: Vec<Task>,
    /// The function that takes the address of any frame, reads which poll runs
    /// it from the frame's header, and polls it. It is there when a function
    /// suspends.
    pub poll_any: Option<usize>,
}

/// The bytes at the start of a frame: the number of the state, the number of
/// the poll, and the stack for results of the task, as it is now and as it was
/// when the task began, each a pair of the address and the bytes left.
pub const HEADER: u64 = 80;

/// Where in the header the four words are that a scheduler keeps its queues in.
pub const SCHEDULER_AT: u64 = 48;

/// Where in the header the pair of the stack is.
pub const STACK_AT: u64 = 16;

/// The bit of the state word of a frame, which is the second of the four a
/// scheduler keeps its queues in, that says the task has been asked to end. The
/// scheduler sets it in the header of the task's own frame, and the poll of each
/// function that the task is in the middle of puts it in the header of the frame
/// of the function it polls, so each function reads it from its own frame.
pub const CANCEL: u64 = 16;

/// Where the word that holds the bit is, in any frame.
pub const CANCEL_AT: u64 = SCHEDULER_AT + 8;

const STATUS: Type = Type::Int(Bits::B8);
const WORD: Type = Type::Int(Bits::B64);

/// The program with each function that suspends made a state machine. A program
/// with none is given back as it was.
pub fn suspend(program: Program) -> Result<Suspended, String> {
    suspend_frames(program, &Frames::default())
}

/// `suspend` for a program that makes frames of its own: each function of
/// `frames.starters` is a task even when it never waits, its starter is given
/// its body, and the function that polls any frame is put where it says.
pub fn suspend_frames(program: Program, frames: &Frames) -> Result<Suspended, String> {
    let suspends = suspending(&program, frames.starters.iter().map(|s| s.function));
    if suspends.is_empty() {
        return Ok(Suspended {
            program: never_cancelled(program),
            tasks: Vec::new(),
            poll_any: None,
        });
    }
    if suspends.contains(&program.main) {
        return Err("`main` suspends, and nothing is there to resume it".to_string());
    }
    let order = callees_first(&program, &suspends)?;
    let mut ranked: Vec<usize> = suspends.iter().copied().collect();
    ranked.sort_unstable();
    let ids: HashMap<usize, u64> = (ranked.iter().enumerate())
        .map(|(id, f)| (*f, id as u64))
        .collect();
    let mut functions = program.functions.clone();
    let mut tasks: HashMap<usize, Task> = HashMap::new();
    let mut polls = Vec::new();
    for f in order {
        let (start, poll, task) = Machine::new(
            &program.functions[f],
            f,
            ids[&f],
            &tasks,
            &suspends,
            &frames.through,
        )
        .build(functions.len() + polls.len());
        functions[f] = start;
        polls.push(poll);
        tasks.insert(f, task);
    }
    functions.extend(polls);
    let mut list: Vec<(usize, Task)> = tasks.into_iter().collect();
    list.sort_by_key(|(f, _)| *f);
    // The frame of a task that is made as a frame has room, after all else in
    // it, for the stack for results that the task builds in.
    for starter in &frames.starters {
        let at = list
            .binary_search_by_key(&starter.function, |(f, _)| *f)
            .expect("a task");
        let task = &mut list[at].1;
        task.stack_at = Some(STACK_AT);
    }
    for starter in &frames.starters {
        let task = &list[list
            .binary_search_by_key(&starter.function, |(f, _)| *f)
            .expect("a task")]
        .1;
        functions[starter.place] = make_frame(&program.functions[starter.place], task);
    }
    let list: Vec<Task> = list.into_iter().map(|(_, t)| t).collect();
    let poll_any = match frames.poll_any {
        Some(place) => {
            functions[place] = any_poll(&list, &frames.through);
            Some(place)
        }
        None => {
            functions.push(any_poll(&list, &frames.through));
            Some(functions.len() - 1)
        }
    };
    Ok(Suspended {
        program: never_cancelled(Program {
            data: program.data,
            functions,
            main: program.main,
        }),
        tasks: list,
        poll_any,
    })
}

/// The program with every `System::Cancelled` that is left, which is each one in
/// a function that is not a state machine, made the constant 0: nothing asks
/// such a function to end, so a program that starts no task is as it was.
fn never_cancelled(mut program: Program) -> Program {
    for f in &mut program.functions {
        for block in &mut f.blocks {
            for ins in &mut block.instructions {
                if matches!(
                    ins.op,
                    Op::System {
                        call: System::Cancelled,
                        ..
                    }
                ) {
                    ins.op = Op::Constant(Constant::Int(0));
                }
            }
        }
    }
    program
}

/// Where the values a function gives are in its frame, in the order it gives
/// them, and how many bytes the frame holds before what follows them. They come
/// first, so the same types are at the same places in the frame of every
/// function that gives them, and a frame is read for what it gave without
/// knowing which function it is of.
pub fn result_places(types: &[Type]) -> (Vec<u64>, u64) {
    let mut at = HEADER;
    let places = types
        .iter()
        .map(|ty| {
            at = align(at, ty.bytes());
            let here = at;
            at += ty.bytes();
            here
        })
        .collect();
    (places, at)
}

/// The function that makes the frame of a task and starts it, in the place of
/// `stub`, which has the parameters the task takes: it asks for the bytes of the
/// frame, starts the task in them, and gives the address and the number of bytes.
fn make_frame(stub: &Function, task: &Task) -> Function {
    let params = stub.blocks[0].params.clone();
    let mut types = stub.types.clone();
    let mut fresh = |ty: Type| {
        types.push(ty);
        Value(types.len() - 1)
    };
    let (bytes, frame) = (fresh(WORD), fresh(Type::Address));
    let instructions = vec![
        Instruction {
            results: vec![bytes],
            op: Op::Constant(Constant::Int(task.bytes)),
            at: Place::NONE,
        },
        // The frame is in the pool, with the stack of the task, and is where it
        // is until the task is freed, so that the memory of whoever makes it
        // may be released and the frame goes on.
        Instruction {
            results: vec![frame],
            op: Op::System {
                call: System::TaskFrame,
                args: vec![bytes],
            },
            at: Place::NONE,
        },
    ];
    let mut instructions = instructions;
    let mut args = vec![frame];
    args.extend(params.iter().copied());
    instructions.push(Instruction {
        results: Vec::new(),
        op: Op::Call {
            function: task.start,
            args,
        },
        at: Place::NONE,
    });
    Function {
        types,
        results: vec![Type::Address, WORD],
        blocks: vec![Block {
            params,
            instructions,
            end: End::Return(vec![frame, bytes]),
        }],
        cold: stub.cold,
        name: stub.name.clone(),
    }
    .named(stub.name.clone())
}

/// The functions that run `System::Suspend`, those the lowering made frames of,
/// and those that call one of them, which is each function that suspends.
fn suspending(program: &Program, forced: impl Iterator<Item = usize>) -> HashSet<usize> {
    let mut found: HashSet<usize> = (program.functions.iter().enumerate())
        .filter(|(_, f)| f.blocks.iter().flat_map(|b| &b.instructions).any(waits))
        .map(|(i, _)| i)
        .chain(forced)
        .collect();
    loop {
        let before = found.len();
        for (i, f) in program.functions.iter().enumerate() {
            let calls = (f.blocks.iter().flat_map(|b| &b.instructions)).any(
                |ins| matches!(&ins.op, Op::Call { function, .. } if found.contains(function)),
            );
            if calls {
                found.insert(i);
            }
        }
        if found.len() == before {
            return found;
        }
    }
}

fn waits(ins: &Instruction) -> bool {
    matches!(
        ins.op,
        Op::System {
            call: System::Suspend,
            ..
        }
    )
}

/// The functions that suspend, each after the ones it calls. A function that
/// suspends and is reached from itself is not a program the pass is given, since
/// `recur` runs first, and is refused.
fn callees_first(program: &Program, suspends: &HashSet<usize>) -> Result<Vec<usize>, String> {
    #[derive(Clone, Copy, PartialEq)]
    enum Mark {
        Open,
        Done,
    }
    fn visit(
        program: &Program,
        suspends: &HashSet<usize>,
        f: usize,
        marks: &mut HashMap<usize, Mark>,
        order: &mut Vec<usize>,
    ) -> Result<(), String> {
        match marks.get(&f) {
            Some(Mark::Done) => return Ok(()),
            Some(Mark::Open) => {
                return Err(format!(
                    "`{}` suspends and reaches itself, and `recur` has not run",
                    program.functions[f].name
                ));
            }
            None => {}
        }
        marks.insert(f, Mark::Open);
        let mut callees: Vec<usize> = (program.functions[f].blocks.iter())
            .flat_map(|b| &b.instructions)
            .filter_map(|i| match &i.op {
                Op::Call { function, .. } if suspends.contains(function) => Some(*function),
                _ => None,
            })
            .collect();
        callees.sort_unstable();
        callees.dedup();
        for c in callees {
            visit(program, suspends, c, marks, order)?;
        }
        marks.insert(f, Mark::Done);
        order.push(f);
        Ok(())
    }
    let mut marks = HashMap::new();
    let mut order = Vec::new();
    let mut all: Vec<usize> = suspends.iter().copied().collect();
    all.sort_unstable();
    for f in all {
        visit(program, suspends, f, &mut marks, &mut order)?;
    }
    Ok(order)
}

/// How a function is made a state machine: the function, where in its frame
/// each thing is, and the function being made.
struct Machine<'a> {
    original: &'a Function,
    index: usize,
    id: u64,
    result_at: Vec<u64>,
    name: String,
    /// The place in the frame of each value alive across a suspension.
    slot: HashMap<Value, u64>,
    /// The place of each parameter, which the start fills.
    param_slot: Vec<u64>,
    /// The place of the bytes of each `Frame` instruction, by block and place.
    frame_at: HashMap<(usize, usize), u64>,
    /// Where in the frame each arena the function opens notes the stack for
    /// results, by the token it gives.
    arena_at: HashMap<Value, u64>,
    /// The place of the frame of the function each call of one that suspends
    /// starts, by block and place, and what that function is.
    callee_at: HashMap<(usize, usize), (u64, &'a Task)>,
    bytes: u64,
    /// The function being made: its values, and its blocks, of which the first
    /// is the one that goes to the state.
    types: Vec<Type>,
    blocks: Vec<Block>,
    /// The address of the frame, which the poll is given.
    frame: Value,
    /// The place in the poll of each state: where it goes on.
    states: Vec<usize>,
    start_name: String,
    suspends: &'a HashSet<usize>,
    /// The types of the state the poll is given and gives back.
    through: &'a [Type],
}

fn align(n: u64, to: u64) -> u64 {
    n.next_multiple_of(to)
}

fn is_suspension(ins: &Instruction, suspends: &HashSet<usize>) -> bool {
    waits(ins) || matches!(&ins.op, Op::Call { function, .. } if suspends.contains(function))
}

impl<'a> Machine<'a> {
    fn new(
        original: &'a Function,
        index: usize,
        id: u64,
        tasks: &'a HashMap<usize, Task>,
        suspends: &'a HashSet<usize>,
        through: &'a [Type],
    ) -> Machine<'a> {
        let live = alive_across(original, suspends);
        let place = |at: &mut u64, ty: Type| {
            *at = align(*at, ty.bytes());
            let here = *at;
            *at += ty.bytes();
            here
        };
        let (result_at, mut at) =
            result_places(&original.results[..original.results.len() - through.len()]);
        let param_slot = (original.blocks[0].params.iter())
            .take(original.blocks[0].params.len() - through.len())
            .map(|p| place(&mut at, original.types[p.0]))
            .collect();
        let mut frame_at = HashMap::new();
        let mut arena_at = HashMap::new();
        let mut callee_at = HashMap::new();
        let mut slot = HashMap::new();
        let mut values: Vec<Value> = live.into_iter().collect();
        values.sort();
        for v in values {
            slot.insert(v, place(&mut at, original.types[v.0]));
        }
        for (b, block) in original.blocks.iter().enumerate() {
            for (i, ins) in block.instructions.iter().enumerate() {
                match &ins.op {
                    Op::Frame { bytes } => {
                        at = align(at, 16);
                        frame_at.insert((b, i), at);
                        at += bytes;
                    }
                    Op::System {
                        call: System::ArenaOpen,
                        ..
                    } => {
                        at = align(at, 16);
                        arena_at.insert(ins.results[0], at);
                        at += 16;
                    }
                    Op::Call { function, .. } if suspends.contains(function) => {
                        at = align(at, 16);
                        let task = &tasks[function];
                        callee_at.insert((b, i), (at, task));
                        at += task.bytes;
                    }
                    _ => {}
                }
            }
        }
        Machine {
            original,
            index,
            id,
            result_at,
            name: original.name.clone(),
            slot,
            param_slot,
            frame_at,
            arena_at,
            callee_at,
            bytes: align(at, 16),
            types: original.types.clone(),
            blocks: Vec::new(),
            frame: Value(0),
            states: Vec::new(),
            start_name: format!("{}<start {index}>", original.name),
            suspends,
            through,
        }
    }

    fn fresh(&mut self, ty: Type) -> Value {
        self.types.push(ty);
        Value(self.types.len() - 1)
    }

    fn emit(&mut self, into: usize, op: Op, results: &[Type], at: Place) -> Vec<Value> {
        let results: Vec<Value> = results.iter().map(|t| self.fresh(*t)).collect();
        self.blocks[into].instructions.push(Instruction {
            results: results.clone(),
            op,
            at,
        });
        results
    }

    fn new_block(&mut self, params: Vec<Value>) -> usize {
        self.blocks.push(Block {
            params,
            instructions: Vec::new(),
            end: End::Return(Vec::new()),
        });
        self.blocks.len() - 1
    }

    fn word(&mut self, into: usize, n: u64) -> Value {
        self.emit(into, Op::Constant(Constant::Int(n)), &[WORD], Place::NONE)[0]
    }

    /// The address `offset` bytes into the frame.
    fn within(&mut self, into: usize, offset: u64) -> Value {
        if offset == 0 {
            return self.frame;
        }
        let by = self.word(into, offset);
        let frame = self.frame;
        self.emit(
            into,
            Op::Binary {
                op: Binary::Add,
                a: frame,
                b: by,
            },
            &[Type::Address],
            Place::NONE,
        )[0]
    }

    fn save(&mut self, into: usize, offset: u64, value: Value) {
        let ty = self.types[value.0];
        let address = self.within(into, offset);
        self.emit(into, Op::Store { ty, address, value }, &[], Place::NONE);
    }

    fn load(&mut self, into: usize, offset: u64, ty: Type) -> Value {
        let address = self.within(into, offset);
        self.emit(into, Op::Load { ty, address }, &[ty], Place::NONE)[0]
    }

    /// The values the operands name, each that lives in the frame read from it.
    fn read(&mut self, into: usize, operands: Vec<Value>) -> HashMap<Value, Value> {
        let mut read = HashMap::new();
        for v in operands {
            let Some(&offset) = self.slot.get(&v) else {
                continue;
            };
            if let std::collections::hash_map::Entry::Vacant(e) = read.entry(v) {
                let ty = self.types[v.0];
                let value = self.load(into, offset, ty);
                e.insert(value);
            }
        }
        read
    }

    /// Writes each of the values that lives in the frame.
    fn keep(&mut self, into: usize, values: &[Value]) {
        for v in values {
            if let Some(&offset) = self.slot.get(v) {
                self.save(into, offset, *v);
            }
        }
    }

    /// Ends the block by giving the wait: the state is where the poll goes on.
    fn give_wait(
        &mut self,
        into: usize,
        state: u64,
        kind: Value,
        handle: Value,
        through: &[Value],
    ) {
        let n = self.word(into, state);
        self.save(into, 0, n);
        let status = self.emit(into, Op::Constant(Constant::Int(1)), &[STATUS], Place::NONE)[0];
        let mut given = through.to_vec();
        given.extend([status, kind, handle]);
        self.blocks[into].end = End::Return(given);
    }

    /// Values of the types of the state the poll is given.
    fn state_values(&mut self) -> Vec<Value> {
        let types = self.through.to_vec();
        types.into_iter().map(|t| self.fresh(t)).collect()
    }

    fn build(mut self, poll_index: usize) -> (Function, Function, Task) {
        let original = self.original;
        // The poll's first block goes to the state, and each block of the
        // function is the block after it, so a jump goes one further.
        let frame = self.fresh(Type::Address);
        self.frame = frame;
        let given = self.state_values();
        let mut dispatch_params = vec![frame];
        dispatch_params.extend(given.iter().copied());
        let dispatch = self.new_block(dispatch_params);
        let entered = self.state_values();
        let entry = self.new_block(entered.clone());
        let first = self.blocks.len();
        for block in &original.blocks {
            self.new_block(block.params.clone());
        }
        let after = |b: usize| first + b;
        // State 0 starts the function: its parameters are in the frame.
        self.states.push(entry);
        let own = original.blocks[0].params.len() - self.through.len();
        let params = original.blocks[0].params[..own].to_vec();
        let mut args = Vec::new();
        for (p, offset) in params.iter().zip(self.param_slot.clone()) {
            args.push(self.load(entry, offset, original.types[p.0]));
        }
        args.extend(entered);
        self.blocks[entry].end = End::Jump(Jump {
            block: after(0),
            args,
        });
        for (b, block) in original.blocks.iter().enumerate() {
            let mut at = after(b);
            let params = block.params.clone();
            self.keep(at, &params);
            for (i, ins) in block.instructions.iter().enumerate() {
                let mut operands = Vec::new();
                ins.op.operands_into(&mut operands);
                let read = self.read(at, operands);
                let mut op = ins.op.clone();
                op.rename(&|v| read.get(&v).copied().unwrap_or(v));
                match &op {
                    Op::Frame { .. } => {
                        let offset = self.frame_at[&(b, i)];
                        let by = self.word(at, offset);
                        let frame = self.frame;
                        self.blocks[at].instructions.push(Instruction {
                            results: ins.results.clone(),
                            op: Op::Binary {
                                op: Binary::Add,
                                a: frame,
                                b: by,
                            },
                            at: ins.at,
                        });
                        self.keep(at, &ins.results);
                    }
                    // An arena in a task puts the stack for results back at its
                    // end, as in code that never waits: the place of the stack
                    // is noted in the frame, since the token of an arena is no
                    // value a poll that is run again could keep, and what is
                    // built in the arena is built in the task's own stack.
                    Op::System {
                        call: System::ArenaOpen,
                        ..
                    } => {
                        // The token is a value that is kept across a wait like
                        // any other, and says nothing: the place of the stack is
                        // in the frame.
                        self.blocks[at].instructions.push(Instruction {
                            results: ins.results.clone(),
                            op: Op::Constant(Constant::Int(0)),
                            at: ins.at,
                        });
                        self.keep(at, &ins.results);
                        let offset = self.arena_at[&ins.results[0]];
                        let slot = self.within(at, offset);
                        self.blocks[at].instructions.push(Instruction {
                            results: Vec::new(),
                            op: Op::System {
                                call: System::TaskMark,
                                args: vec![slot],
                            },
                            at: ins.at,
                        });
                    }
                    Op::System {
                        call: System::ArenaClose,
                        ..
                    } => {
                        let Op::System { args: opened, .. } = &ins.op else {
                            unreachable!("a close")
                        };
                        let offset = self.arena_at[&opened[0]];
                        let slot = self.within(at, offset);
                        self.blocks[at].instructions.push(Instruction {
                            results: Vec::new(),
                            op: Op::System {
                                call: System::TaskRelease,
                                args: vec![slot],
                            },
                            at: ins.at,
                        });
                    }
                    // Whether the task has been asked to end is the bit of the
                    // frame's own header, as the poll that runs the function left
                    // it.
                    Op::System {
                        call: System::Cancelled,
                        ..
                    } => {
                        let word = self.load(at, CANCEL_AT, WORD);
                        let mask = self.word(at, CANCEL);
                        let bit = self.emit(
                            at,
                            Op::Binary {
                                op: Binary::And,
                                a: word,
                                b: mask,
                            },
                            &[WORD],
                            ins.at,
                        )[0];
                        let four = self.word(at, CANCEL.trailing_zeros() as u64);
                        self.blocks[at].instructions.push(Instruction {
                            results: ins.results.clone(),
                            op: Op::Binary {
                                op: Binary::ShiftRightU,
                                a: bit,
                                b: four,
                            },
                            at: ins.at,
                        });
                        self.keep(at, &ins.results);
                    }
                    Op::System {
                        call: System::Suspend,
                        args,
                    } => {
                        let state = self.states.len() as u64;
                        let (kind, handle) = (args[0], args[1]);
                        let through = args[2..].to_vec();
                        // The state the task goes on with is what the poll that
                        // resumes it is given.
                        let resume = self.new_block(ins.results.clone());
                        self.states.push(resume);
                        self.give_wait(at, state, kind, handle, &through);
                        at = resume;
                    }
                    Op::Call { function, args } if self.suspends.contains(function) => {
                        let (offset, task) = self.callee_at[&(b, i)];
                        let task = task.clone();
                        let state = self.states.len() as u64;
                        let k = self.through.len();
                        let own = args.len() - k;
                        let callee = self.within(at, offset);
                        let mut given = vec![callee];
                        given.extend(args[..own].iter().copied());
                        self.blocks[at].instructions.push(Instruction {
                            results: Vec::new(),
                            op: Op::Call {
                                function: task.start,
                                args: given,
                            },
                            at: ins.at,
                        });
                        let n = self.word(at, state);
                        self.save(at, 0, n);
                        // The poll of the callee, where the state comes back to.
                        let polled = self.state_values();
                        let polls = self.new_block(polled.clone());
                        self.states.push(polls);
                        self.blocks[at].end = End::Jump(Jump {
                            block: polls,
                            args: args[own..].to_vec(),
                        });
                        // The callee is asked to end when this function is.
                        let word = self.load(polls, CANCEL_AT, WORD);
                        let mask = self.word(polls, CANCEL);
                        let bit = self.emit(
                            polls,
                            Op::Binary {
                                op: Binary::And,
                                a: word,
                                b: mask,
                            },
                            &[WORD],
                            ins.at,
                        )[0];
                        self.save(polls, offset + CANCEL_AT, bit);
                        let callee = self.within(polls, offset);
                        let mut types = self.through.to_vec();
                        types.extend([STATUS, WORD, WORD]);
                        let mut passed = vec![callee];
                        passed.extend(polled);
                        let given = self.emit(
                            polls,
                            Op::Call {
                                function: task.poll,
                                args: passed,
                            },
                            &types,
                            ins.at,
                        );
                        let (state, rest) = given.split_at(k);
                        let waiting = self.state_values();
                        let waits = self.new_block(waiting.clone());
                        // What the callee gave of the state is what the call
                        // gave of it, whether the callee is done or not.
                        let done = self.new_block(ins.results[ins.results.len() - k..].to_vec());
                        self.blocks[polls].end = End::Switch {
                            on: rest[0],
                            jumps: vec![
                                Jump {
                                    block: done,
                                    args: state.to_vec(),
                                },
                                Jump {
                                    block: waits,
                                    args: state.to_vec(),
                                },
                            ],
                        };
                        let mut values = waiting;
                        values.push(
                            self.emit(
                                waits,
                                Op::Constant(Constant::Int(1)),
                                &[STATUS],
                                Place::NONE,
                            )[0],
                        );
                        values.extend([rest[1], rest[2]]);
                        self.blocks[waits].end = End::Return(values);
                        // What the callee gave is in its frame, and is read as the
                        // values the call gave.
                        for (r, place) in ins.results[..ins.results.len() - k]
                            .iter()
                            .zip(&task.result_at)
                        {
                            let address = self.within(done, offset + place);
                            let ty = self.types[r.0];
                            self.blocks[done].instructions.push(Instruction {
                                results: vec![*r],
                                op: Op::Load { ty, address },
                                at: ins.at,
                            });
                        }
                        self.keep(done, &ins.results);
                        at = done;
                    }
                    _ => {
                        self.blocks[at].instructions.push(Instruction {
                            results: ins.results.clone(),
                            op,
                            at: ins.at,
                        });
                        self.keep(at, &ins.results);
                    }
                }
            }
            // The end of the block, with what it reads read from the frame.
            let mut used: Vec<Value> = block.end.operands().collect();
            used.dedup();
            let read = self.read(at, used);
            let by = |v: Value| read.get(&v).copied().unwrap_or(v);
            let jump = |j: &Jump| Jump {
                block: after(j.block),
                args: j.args.iter().map(|v| by(*v)).collect(),
            };
            self.blocks[at].end = match &block.end {
                End::Jump(j) => End::Jump(jump(j)),
                End::Switch { on, jumps } => End::Switch {
                    on: by(*on),
                    jumps: jumps.iter().map(jump).collect(),
                },
                End::Return(values) => {
                    let status =
                        self.emit(at, Op::Constant(Constant::Int(0)), &[STATUS], Place::NONE)[0];
                    let kind = self.word(at, 0);
                    let own = values.len() - self.through.len();
                    for (v, place) in values[..own].iter().zip(self.result_at.clone()) {
                        self.save(at, place, by(*v));
                    }
                    let mut given: Vec<Value> = values[own..].iter().map(|v| by(*v)).collect();
                    given.extend([status, kind, kind]);
                    End::Return(given)
                }
            };
        }
        // The state that is read from the frame says which block to go on in.
        let state = self.load(dispatch, 0, WORD);
        let jumps = (self.states.clone())
            .into_iter()
            .map(|block| Jump {
                block,
                args: given.clone(),
            })
            .collect();
        self.blocks[dispatch].end = End::Switch { on: state, jumps };

        let mut poll_results = self.through.to_vec();
        poll_results.extend([STATUS, WORD, WORD]);
        let poll = Function {
            types: std::mem::take(&mut self.types),
            results: poll_results,
            blocks: std::mem::take(&mut self.blocks),
            cold: original.cold,
            name: format!("{}<poll>", self.name),
        }
        .named(format!("{}<poll>", self.name));
        let start = self.start();
        let task = Task {
            start: self.index,
            poll: poll_index,
            id: self.id,
            result_at: self.result_at.clone(),
            bytes: self.bytes,
            stack_at: None,
            results: original.results[..original.results.len() - self.through.len()].to_vec(),
        };
        (start, poll, task)
    }

    /// The function at the place of the original, which makes the frame ready:
    /// the state is the start of the function, and its parameters are in it.
    fn start(&self) -> Function {
        let original = self.original;
        let mut types = vec![Type::Address];
        let own = original.blocks[0].params.len() - self.through.len();
        let params: Vec<Value> = (original.blocks[0].params[..own].iter())
            .map(|p| {
                types.push(original.types[p.0]);
                Value(types.len() - 1)
            })
            .collect();
        let mut instructions: Vec<Instruction> = Vec::new();
        let mut make = |types: &mut Vec<Type>, op: Op, result: Option<Type>| -> Value {
            let results: Vec<Value> = result
                .into_iter()
                .map(|t| {
                    types.push(t);
                    Value(types.len() - 1)
                })
                .collect();
            let value = results.first().copied().unwrap_or(Value(0));
            instructions.push(Instruction {
                results,
                op,
                at: Place::NONE,
            });
            value
        };
        let frame = Value(0);
        let state = make(&mut types, Op::Constant(Constant::Int(0)), Some(WORD));
        let id = make(&mut types, Op::Constant(Constant::Int(self.id)), Some(WORD));
        let mut saved = vec![(0, state, WORD), (8, id, WORD)];
        saved.extend(
            (params
                .iter()
                .zip(&self.param_slot)
                .zip(original.blocks[0].params[..own].iter()))
            .map(|((p, at), o)| (*at, *p, original.types[o.0])),
        );
        for (offset, value, ty) in saved {
            let address = if offset == 0 {
                frame
            } else {
                let by = make(&mut types, Op::Constant(Constant::Int(offset)), Some(WORD));
                let add = Op::Binary {
                    op: Binary::Add,
                    a: frame,
                    b: by,
                };
                make(&mut types, add, Some(Type::Address))
            };
            make(&mut types, Op::Store { ty, address, value }, None);
        }
        Function {
            types,
            results: Vec::new(),
            blocks: vec![Block {
                params: std::iter::once(frame).chain(params).collect(),
                instructions,
                end: End::Return(Vec::new()),
            }],
            cold: original.cold,
            name: self.start_name.clone(),
        }
        .named(self.start_name.clone())
    }
}

/// The values that are alive after a suspension: each is read after the task
/// is resumed, so it is kept in the frame.
fn alive_across(f: &Function, suspends: &HashSet<usize>) -> HashSet<Value> {
    let n = f.types.len();
    let blocks = f.blocks.len();
    // For each block: what it reads before it makes it, and what it makes.
    let mut uses = vec![vec![false; n]; blocks];
    let mut defs = vec![vec![false; n]; blocks];
    for (b, block) in f.blocks.iter().enumerate() {
        for p in &block.params {
            defs[b][p.0] = true;
        }
        for ins in &block.instructions {
            for v in ins.op.operands() {
                if !defs[b][v.0] {
                    uses[b][v.0] = true;
                }
            }
            for r in &ins.results {
                defs[b][r.0] = true;
            }
        }
        for v in block.end.operands() {
            if !defs[b][v.0] {
                uses[b][v.0] = true;
            }
        }
    }
    let mut live_in = vec![vec![false; n]; blocks];
    let mut live_out = vec![vec![false; n]; blocks];
    loop {
        let mut changed = false;
        for b in (0..blocks).rev() {
            for next in f.blocks[b].end.successors() {
                for v in 0..n {
                    if live_in[next][v] && !live_out[b][v] {
                        live_out[b][v] = true;
                        changed = true;
                    }
                }
            }
            for v in 0..n {
                let now = uses[b][v] || (live_out[b][v] && !defs[b][v]);
                if now && !live_in[b][v] {
                    live_in[b][v] = true;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    let mut alive = HashSet::new();
    for (b, block) in f.blocks.iter().enumerate() {
        let mut live: Vec<bool> = live_out[b].clone();
        for v in block.end.operands() {
            live[v.0] = true;
        }
        for ins in block.instructions.iter().rev() {
            for r in &ins.results {
                live[r.0] = false;
            }
            if is_suspension(ins, suspends) {
                for (v, l) in live.iter().enumerate() {
                    if *l {
                        alive.insert(Value(v));
                    }
                }
            }
            for v in ins.op.operands() {
                live[v.0] = true;
            }
        }
    }
    alive
}

/// The function that takes the address of a frame, reads from its header which
/// poll runs it, and polls it, giving what that poll gives. A frame is polled
/// without knowing which function it is of, as a scheduler that holds frames of
/// many functions does.
fn any_poll(tasks: &[Task], through: &[Type]) -> Function {
    let mut types = vec![Type::Address];
    types.extend(through.iter().copied());
    let frame = Value(0);
    let given: Vec<Value> = (1..=through.len()).map(Value).collect();
    let mut blocks = vec![Block {
        params: std::iter::once(frame)
            .chain(given.iter().copied())
            .collect(),
        instructions: Vec::new(),
        end: End::Return(Vec::new()),
    }];
    let fresh = |types: &mut Vec<Type>, ty: Type| {
        types.push(ty);
        Value(types.len() - 1)
    };
    let eight = fresh(&mut types, WORD);
    let at = fresh(&mut types, Type::Address);
    let id = fresh(&mut types, WORD);
    blocks[0].instructions.push(Instruction {
        results: vec![eight],
        op: Op::Constant(Constant::Int(8)),
        at: Place::NONE,
    });
    blocks[0].instructions.push(Instruction {
        results: vec![at],
        op: Op::Binary {
            op: Binary::Add,
            a: frame,
            b: eight,
        },
        at: Place::NONE,
    });
    blocks[0].instructions.push(Instruction {
        results: vec![id],
        op: Op::Load {
            ty: WORD,
            address: at,
        },
        at: Place::NONE,
    });
    let mut jumps = Vec::new();
    for task in tasks {
        let mut results: Vec<Value> = through.iter().map(|t| fresh(&mut types, *t)).collect();
        results.extend([
            fresh(&mut types, STATUS),
            fresh(&mut types, WORD),
            fresh(&mut types, WORD),
        ]);
        blocks.push(Block {
            params: Vec::new(),
            instructions: vec![Instruction {
                results: results.clone(),
                op: Op::Call {
                    function: task.poll,
                    args: std::iter::once(frame)
                        .chain(given.iter().copied())
                        .collect(),
                },
                at: Place::NONE,
            }],
            end: End::Return(results),
        });
        jumps.push(Jump {
            block: blocks.len() - 1,
            args: Vec::new(),
        });
    }
    blocks[0].end = End::Switch { on: id, jumps };
    let mut results = through.to_vec();
    results.extend([STATUS, WORD, WORD]);
    Function {
        types,
        results,
        blocks,
        cold: false,
        name: "<poll any>".to_string(),
    }
    .named("<poll any>")
}
