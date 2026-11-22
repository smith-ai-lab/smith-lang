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
//! A function that suspends, and so the program it is in, cannot call itself:
//! a frame that holds the frame of a callee holds a fixed number of bytes, which
//! a call of itself would not.

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
    /// The poll: it takes the address of the frame, and gives a status that is 0
    /// when the task is done and 1 when it waits, a kind and a handle, and then
    /// the values the function gave.
    pub poll: usize,
    /// How many bytes the frame has, and so how many the one who starts the
    /// task must give it. The address is a multiple of 16.
    pub bytes: u64,
    /// The types of the values the poll gives after the wait, which are the
    /// types the function gave.
    pub results: Vec<Type>,
}

/// The program with each function that suspends made a state machine, and each
/// of those functions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Suspended {
    pub program: Program,
    pub tasks: Vec<Task>,
}

const STATUS: Type = Type::Int(Bits::B8);
const WORD: Type = Type::Int(Bits::B64);

/// The program with each function that suspends made a state machine. A program
/// with none is given back as it was.
pub fn suspend(program: Program) -> Result<Suspended, String> {
    let suspends = suspending(&program);
    if suspends.is_empty() {
        return Ok(Suspended {
            program,
            tasks: Vec::new(),
        });
    }
    if suspends.contains(&program.main) {
        return Err("`main` suspends, and nothing is there to resume it".to_string());
    }
    let order = callees_first(&program, &suspends)?;
    let mut functions = program.functions.clone();
    let mut tasks: HashMap<usize, Task> = HashMap::new();
    let mut polls = Vec::new();
    for f in order {
        let (start, poll, task) = Machine::new(&program.functions[f], f, &tasks, &suspends)
            .build(functions.len() + polls.len());
        functions[f] = start;
        polls.push(poll);
        tasks.insert(f, task);
    }
    functions.extend(polls);
    let mut list: Vec<(usize, Task)> = tasks.into_iter().collect();
    list.sort_by_key(|(f, _)| *f);
    Ok(Suspended {
        program: Program {
            data: program.data,
            functions,
            main: program.main,
        },
        tasks: list.into_iter().map(|(_, t)| t).collect(),
    })
}

/// The functions that run `System::Suspend`, and those that call one of the
/// functions that do, which is each function that suspends.
fn suspending(program: &Program) -> HashSet<usize> {
    let mut found: HashSet<usize> = (program.functions.iter().enumerate())
        .filter(|(_, f)| f.blocks.iter().flat_map(|b| &b.instructions).any(waits))
        .map(|(i, _)| i)
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
/// suspends and is reached from itself is refused.
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
                    "`{}` suspends and calls itself, and its frame would hold its own",
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
    name: String,
    /// The place in the frame of each value alive across a suspension.
    slot: HashMap<Value, u64>,
    /// The place of each parameter, which the start fills.
    param_slot: Vec<u64>,
    /// The place of the bytes of each `Frame` instruction, by block and place.
    frame_at: HashMap<(usize, usize), u64>,
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
    results: Vec<Type>,
    start_name: String,
    suspends: &'a HashSet<usize>,
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
        tasks: &'a HashMap<usize, Task>,
        suspends: &'a HashSet<usize>,
    ) -> Machine<'a> {
        let live = alive_across(original, suspends);
        let mut at = 8u64;
        let place = |at: &mut u64, ty: Type| {
            *at = align(*at, ty.bytes());
            let here = *at;
            *at += ty.bytes();
            here
        };
        let param_slot = (original.blocks[0].params.iter())
            .map(|p| place(&mut at, original.types[p.0]))
            .collect();
        let mut frame_at = HashMap::new();
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
            name: original.name.clone(),
            slot,
            param_slot,
            frame_at,
            callee_at,
            bytes: align(at, 16),
            types: original.types.clone(),
            blocks: Vec::new(),
            frame: Value(0),
            states: Vec::new(),
            results: original.results.clone(),
            start_name: format!("{}<start {index}>", original.name),
            suspends,
        }
    }

    fn fresh(&mut self, ty: Type) -> Value {
        self.types.push(ty);
        Value(self.types.len() - 1)
    }

    fn zero(&mut self, into: usize, ty: Type) -> Value {
        let c = match ty {
            Type::Int(_) => Constant::Int(0),
            Type::Float(_) => Constant::Float(0),
            Type::Address => Constant::Null,
            Type::Vector(_) => Constant::Vector(0, 0),
        };
        self.emit(into, Op::Constant(c), &[ty], Place::NONE)[0]
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
    fn give_wait(&mut self, into: usize, state: u64, kind: Value, handle: Value) {
        let n = self.word(into, state);
        self.save(into, 0, n);
        let status = self.emit(into, Op::Constant(Constant::Int(1)), &[STATUS], Place::NONE)[0];
        let mut values = vec![status, kind, handle];
        for ty in self.results.clone() {
            values.push(self.zero(into, ty));
        }
        self.blocks[into].end = End::Return(values);
    }

    fn build(mut self, poll_index: usize) -> (Function, Function, Task) {
        let original = self.original;
        // The poll's first block goes to the state, and each block of the
        // function is the block after it, so a jump goes one further.
        let frame = self.fresh(Type::Address);
        self.frame = frame;
        let dispatch = self.new_block(vec![frame]);
        let entry = self.new_block(Vec::new());
        let first = self.blocks.len();
        for block in &original.blocks {
            self.new_block(block.params.clone());
        }
        let after = |b: usize| first + b;
        // State 0 starts the function: its parameters are in the frame.
        self.states.push(entry);
        let params = original.blocks[0].params.clone();
        let mut args = Vec::new();
        for (p, offset) in params.iter().zip(self.param_slot.clone()) {
            args.push(self.load(entry, offset, original.types[p.0]));
        }
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
                    Op::System {
                        call: System::Suspend,
                        args,
                    } => {
                        let state = self.states.len() as u64;
                        let (kind, handle) = (args[0], args[1]);
                        let resume = self.new_block(Vec::new());
                        self.states.push(resume);
                        self.give_wait(at, state, kind, handle);
                        at = resume;
                    }
                    Op::Call { function, args } if self.suspends.contains(function) => {
                        let (offset, task) = self.callee_at[&(b, i)];
                        let task = task.clone();
                        let state = self.states.len() as u64;
                        let callee = self.within(at, offset);
                        let mut given = vec![callee];
                        given.extend(args.iter().copied());
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
                        let polls = self.new_block(Vec::new());
                        self.states.push(polls);
                        self.blocks[at].end = End::Jump(Jump {
                            block: polls,
                            args: Vec::new(),
                        });
                        let callee = self.within(polls, offset);
                        let mut types = vec![STATUS, WORD, WORD];
                        types.extend(task.results.iter().copied());
                        let given = self.emit(
                            polls,
                            Op::Call {
                                function: task.poll,
                                args: vec![callee],
                            },
                            &types,
                            ins.at,
                        );
                        let waits = self.new_block(Vec::new());
                        let done = self.new_block(ins.results.clone());
                        self.blocks[polls].end = End::Switch {
                            on: given[0],
                            jumps: vec![
                                Jump {
                                    block: done,
                                    args: given[3..].to_vec(),
                                },
                                Jump {
                                    block: waits,
                                    args: Vec::new(),
                                },
                            ],
                        };
                        let mut values = vec![
                            self.emit(
                                waits,
                                Op::Constant(Constant::Int(1)),
                                &[STATUS],
                                Place::NONE,
                            )[0],
                            given[1],
                            given[2],
                        ];
                        for ty in self.results.clone() {
                            values.push(self.zero(waits, ty));
                        }
                        self.blocks[waits].end = End::Return(values);
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
                    let mut all = vec![status, kind, kind];
                    all.extend(values.iter().map(|v| by(*v)));
                    End::Return(all)
                }
            };
        }
        // The state that is read from the frame says which block to go on in.
        let state = self.load(dispatch, 0, WORD);
        let jumps = (self.states.clone())
            .into_iter()
            .map(|block| Jump {
                block,
                args: Vec::new(),
            })
            .collect();
        self.blocks[dispatch].end = End::Switch { on: state, jumps };

        let mut poll_results = vec![STATUS, WORD, WORD];
        poll_results.extend(original.results.iter().copied());
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
            bytes: self.bytes,
            results: original.results.clone(),
        };
        (start, poll, task)
    }

    /// The function at the place of the original, which makes the frame ready:
    /// the state is the start of the function, and its parameters are in it.
    fn start(&self) -> Function {
        let original = self.original;
        let mut types = vec![Type::Address];
        let params: Vec<Value> = (original.blocks[0].params.iter())
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
        let mut saved = vec![(0, state, WORD)];
        saved.extend(
            (params
                .iter()
                .zip(&self.param_slot)
                .zip(&original.blocks[0].params))
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
