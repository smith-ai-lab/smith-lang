//! WebAssembly: a module of 32-bit WebAssembly, written by the compiler, and
//! the file of JavaScript that loads it.
//!
//! A value is a local. A function that has more than one block is a loop
//! that reads the number of the block to run in a local and goes to its code
//! through one `br_table`, so a jump is that number and a `br`, and no
//! block's place in the function decides what the form means. An integer of
//! 8, 16, or 32 bits is an `i32` with zeros above its size, one of 64 bits is
//! an `i64`, and an address is an `i32`, held in memory as the 8 bytes the
//! form says, so a list is laid out as it is for every target.

use super::wasm_code::*;
use super::wasm_module::{Body, Export, Global, Module, Signature};
use super::wasm_structure::{self as structure, Shape};
use super::{Named, Part};
use crate::scalar::{
    Binary, Bits, Builder, Compare, Constant, End, FloatBinary, FloatBits, FloatCompare, Function,
    Instruction, Jump, Op, Place, Program, Stream, System, Type, Value,
};

/// Where the first of the program's texts is. Nothing is below it, so no text
/// is at the address that holds nothing.
const DATA_START: u32 = 16;

/// The global that holds where the memory that is free begins, and the one that
/// holds the status the program ended with, which is below 0 until it has.
const HEAP: u32 = 0;
const STATUS: u32 = 1;
/// The global that holds where the free bytes of the stack of frames begin, which
/// a module has only when a function of the program has a `Frame`.
const STACK: u32 = 2;

/// How many bytes the stack of frames has.
const STACK_BYTES: u32 = 1 << 20;

/// How many bytes a page of the memory has.
const PAGE: i64 = 1 << 16;

/// What the module asks of its host.
struct Imports {
    /// Whether the program writes to a stream: the operation `print` of the
    /// effect `console`.
    print: bool,
    /// Whether the program asks for random bytes: the operation `fill` of the
    /// effect `entropy`.
    entropy: bool,
}

/// What a module of the program asks of its host, by the effect and the
/// operation, in the order of the imports.
pub fn imports(program: &Program) -> Vec<(&'static str, &'static str)> {
    let mut found = Vec::new();
    if imports_of(program).print {
        found.push(("console", "print"));
    }
    if imports_of(program).entropy {
        found.push(("entropy", "fill"));
    }
    found
}

fn imports_of(program: &Program) -> Imports {
    let instructions = program.functions.iter().flat_map(|f| &f.blocks);
    let print = instructions.flat_map(|b| &b.instructions).any(|i| {
        matches!(
            i.op,
            Op::System {
                call: System::Write(_),
                ..
            }
        )
    });
    Imports {
        print,
        entropy: program.asks_for_entropy(),
    }
}

/// Whether an instruction needs the high half of a product of 64 bits.
fn needs_high(f: &Function, i: &Instruction) -> bool {
    match &i.op {
        Op::Binary {
            op: Binary::MulHighU | Binary::MulHighS,
            ..
        } => f.types[i.results[0].0] == Type::Int(Bits::B64),
        Op::Overflows {
            op: Binary::Mul, a, ..
        } => f.types[a.0] == Type::Int(Bits::B64),
        _ => false,
    }
}

/// A check of a module, which a host that has a validator may give once, and
/// which each module is made to pass before `emit` gives it back. The compiler
/// has none of its own, so its tests give it the validator of an engine.
static VALID: std::sync::OnceLock<Check> = std::sync::OnceLock::new();

/// What says whether a module is valid, or why it is not.
type Check = fn(&[u8]) -> Result<(), String>;

/// Gives the check that every module `emit` writes passes before it is given
/// back, which ends the build with the reason when it is not valid.
pub fn check_modules_with(check: Check) {
    let _ = VALID.set(check);
}

/// The module for the program.
pub fn emit(program: &Program) -> (Vec<u8>, Vec<Part>) {
    let imports = imports_of(program);
    let mut module = Module::default();
    let mut print = None;
    if imports.print {
        // The stream, as the number a file has for it, and the bytes.
        print = Some(module.import(
            "console",
            "print",
            Signature {
                params: vec![Kind::I32; 3],
                results: vec![],
            },
        ));
    }
    let mut entropy = None;
    if imports.entropy {
        // The address and the count, and whether the bytes were filled.
        entropy = Some(module.import(
            "entropy",
            "fill",
            Signature {
                params: vec![Kind::I32; 2],
                results: vec![Kind::I32],
            },
        ));
    }
    let frames = program
        .functions
        .iter()
        .flat_map(|f| &f.blocks)
        .flat_map(|b| &b.instructions)
        .any(|i| matches!(i.op, Op::Frame { .. }));
    let image = image(program, frames);

    let needs_high = (program.functions.iter()).any(|f| {
        f.blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .any(|i| needs_high(f, i))
    });
    let mut functions: Vec<&Function> = program.functions.iter().collect();
    let helpers = if needs_high {
        let first = program.functions.len();
        vec![high_unsigned(), high_signed(first)]
    } else {
        Vec::new()
    };
    functions.extend(&helpers);

    let first = u32::from(imports.print) + u32::from(imports.entropy);
    let ends = first + functions.len() as u32;
    let context = Context {
        starts: &image.starts,
        print,
        entropy,
        high_unsigned: first + program.functions.len() as u32,
        high_signed: first + program.functions.len() as u32 + 1,
        allocate: ends,
        exit: ends + 1,
        stack_end: image.stack.map(|from| from + STACK_BYTES),
        first,
    };
    for f in &functions {
        let (signature, body) = lower(f, &context);
        module.function(signature, body);
    }
    module.function(
        Signature {
            params: vec![Kind::I64],
            results: vec![Kind::I32],
        },
        allocate(),
    );
    module.function(
        Signature {
            params: vec![Kind::I32],
            results: vec![],
        },
        exit(),
    );
    let start = module.function(
        Signature {
            params: vec![],
            results: vec![],
        },
        start(program, &image, first + program.main as u32),
    );
    let status = module.function(
        Signature {
            params: vec![],
            results: vec![Kind::I32],
        },
        status(),
    );
    module.pages = (image.heap as usize).div_ceil(PAGE as usize).max(1) as u32;
    module.globals = vec![
        Global {
            initial: image.heap as i32,
        },
        Global { initial: -1 },
    ];
    if let Some(from) = image.stack {
        module.globals.push(Global {
            initial: from as i32,
        });
    }
    module.export("memory", Export::Memory);
    module.export("main", Export::Function(start));
    module.export("status", Export::Function(status));
    if !image.bytes.is_empty() {
        module.data.push((DATA_START, image.bytes));
    }
    let (bytes, bodies) = module.finish();
    // The bodies are in the order the functions were defined: the program's,
    // then the helpers of the module, then the four that the module always has.
    let pieces = (helpers.iter().map(|_| ()).enumerate())
        .map(|(k, ())| ["<high_unsigned>", "<high_signed>"][k])
        .chain(["<allocate>", "<exit>", "<start>", "<status>"]);
    let named = (0..program.functions.len())
        .map(Named::Function)
        .chain(pieces.map(Named::Piece));
    let parts = (named.zip(bodies))
        .map(|(named, body)| Part {
            named: named.clone(),
            address: body.at as u64,
            size: body.size as u64,
            frame: 0,
            at: super::runs(
                &body.places,
                body.at,
                body.at + body.size,
                match &named {
                    Named::Function(k) => Place::of(&program.functions[*k].name),
                    Named::Piece(_) => Place::NONE,
                },
            ),
        })
        .collect();
    if let Some(check) = VALID.get()
        && let Err(why) = check(&bytes)
    {
        panic!("the module is not valid: {why}");
    }
    (bytes, parts)
}

/// What is in memory when the program starts.
struct Image {
    /// Where each of the program's texts is.
    starts: Vec<u32>,
    bytes: Vec<u8>,
    /// Where the words the program was started with are laid out, as Linux
    /// lays them out, when `main` takes them: the program's name and no more.
    arguments: Option<u32>,
    /// Where the memory the program asks for begins.
    heap: u32,
    /// Where the stack of frames begins, when a function of the program has a
    /// `Frame`. It is `STACK_BYTES` long and ends where the heap begins.
    stack: Option<u32>,
}

fn image(program: &Program, frames: bool) -> Image {
    let mut bytes = Vec::new();
    let mut starts = Vec::new();
    for text in &program.data {
        starts.push(DATA_START + bytes.len() as u32);
        bytes.extend_from_slice(text);
    }
    let mut arguments = None;
    if program.takes_arguments() {
        bytes.resize(bytes.len().next_multiple_of(8), 0);
        let name = DATA_START + bytes.len() as u32;
        bytes.extend_from_slice(b"program\0");
        arguments = Some(DATA_START + bytes.len() as u32);
        for word in [u64::from(name), 0] {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
    }
    let stack = frames.then(|| (DATA_START + bytes.len() as u32).next_multiple_of(16));
    let heap = match stack {
        Some(from) => from + STACK_BYTES,
        None => (DATA_START + bytes.len() as u32).next_multiple_of(16),
    };
    Image {
        starts,
        bytes,
        arguments,
        heap,
        stack,
    }
}

struct Context<'a> {
    starts: &'a [u32],
    print: Option<u32>,
    entropy: Option<u32>,
    high_unsigned: u32,
    high_signed: u32,
    allocate: u32,
    exit: u32,
    /// Where the stack of frames ends, when the module has one.
    stack_end: Option<u32>,
    /// The number of the first function the program defines.
    first: u32,
}

fn kind(ty: Type) -> Kind {
    match ty {
        Type::Int(Bits::B64) => Kind::I64,
        Type::Float(FloatBits::F32) => Kind::F32,
        Type::Float(FloatBits::F64) => Kind::F64,
        _ => Kind::I32,
    }
}

/// The high half of a product of two unsigned numbers of 64 bits, from the
/// halves of each: `u1 * v1` and the carries of the three products that
/// are in the middle.
fn high_unsigned() -> Function {
    let mut f = Builder::new();
    let [a, b] = f.params(&[Type::Int(Bits::B64); 2])[..] else {
        unreachable!()
    };
    let low = f.int(Bits::B64, 0xFFFF_FFFF);
    let half = f.int(Bits::B64, 32);
    let u0 = f.binary(Binary::And, a, low);
    let u1 = f.binary(Binary::ShiftRightU, a, half);
    let v0 = f.binary(Binary::And, b, low);
    let v1 = f.binary(Binary::ShiftRightU, b, half);
    let w0 = f.binary(Binary::Mul, u0, v0);
    let carry = f.binary(Binary::ShiftRightU, w0, half);
    let u1v0 = f.binary(Binary::Mul, u1, v0);
    let t = f.binary(Binary::Add, u1v0, carry);
    let w1 = f.binary(Binary::And, t, low);
    let w2 = f.binary(Binary::ShiftRightU, t, half);
    let u0v1 = f.binary(Binary::Mul, u0, v1);
    let w1 = f.binary(Binary::Add, u0v1, w1);
    let carry = f.binary(Binary::ShiftRightU, w1, half);
    let u1v1 = f.binary(Binary::Mul, u1, v1);
    let sum = f.binary(Binary::Add, u1v1, w2);
    let high = f.binary(Binary::Add, sum, carry);
    f.ret(vec![high]);
    f.finish(&[Type::Int(Bits::B64)])
}

/// The high half of a product of two signed numbers of 64 bits: that of the
/// unsigned numbers less each number that is negative's other.
fn high_signed(unsigned: usize) -> Function {
    let mut f = Builder::new();
    let [a, b] = f.params(&[Type::Int(Bits::B64); 2])[..] else {
        unreachable!()
    };
    let high = f.call(unsigned, vec![a, b], &[Type::Int(Bits::B64)])[0];
    let zero = f.int(Bits::B64, 0);
    let a_below = f.compare(Compare::LessS, a, zero);
    let b_below = f.compare(Compare::LessS, b, zero);
    let from_a = f.choose(a_below, b, zero);
    let from_b = f.choose(b_below, a, zero);
    let high = f.binary(Binary::Sub, high, from_a);
    let high = f.binary(Binary::Sub, high, from_b);
    f.ret(vec![high]);
    f.finish(&[Type::Int(Bits::B64)])
}

/// Ends the program with a failing status, as a request the host refuses does.
fn fail(code: &mut Code) {
    code.i32_const(1);
    code.global_set(STATUS);
    code.op(UNREACHABLE);
}

/// The function that gives the address of as many bytes as a 64-bit number
/// says, taken from the memory after the last that was given, which it grows
/// by the pages the request needs. A request that cannot be met ends the
/// program with status 1.
fn allocate() -> Body {
    let mut code = Code::default();
    // A request of 2^32 bytes or more cannot be met.
    code.local_get(0);
    code.i64_const(32);
    code.op(I64_SHR_U);
    code.op(I64_EQZ);
    code.op(I32_EQZ);
    code.open(IF);
    fail(&mut code);
    code.op(END);
    // Where the request would end, and where the memory does.
    code.global_get(HEAP);
    code.op(I64_EXTEND_I32_U);
    code.local_get(0);
    code.op(I64_ADD);
    code.local_set(1);
    code.memory_size();
    code.op(I64_EXTEND_I32_U);
    code.i64_const(16);
    code.op(I64_SHL);
    code.local_set(2);
    code.local_get(1);
    code.local_get(2);
    code.op(I64_GT_U);
    code.open(IF);
    // Past the greatest address, no page can be had.
    code.local_get(1);
    code.i64_const(0xFFFF_FFFF);
    code.op(I64_GT_U);
    code.open(IF);
    fail(&mut code);
    code.op(END);
    // The pages that hold the bytes that are missing.
    code.local_get(1);
    code.local_get(2);
    code.op(I64_SUB);
    code.i64_const(PAGE - 1);
    code.op(I64_ADD);
    code.i64_const(16);
    code.op(I64_SHR_U);
    code.op(I32_WRAP_I64);
    code.memory_grow();
    code.i32_const(-1);
    code.op(I32_EQ);
    code.open(IF);
    fail(&mut code);
    code.op(END);
    code.op(END);
    code.global_get(HEAP);
    code.local_get(1);
    code.op(I32_WRAP_I64);
    code.global_set(HEAP);
    code.op(END);
    Body {
        locals: vec![(2, Kind::I64)],
        code: code.bytes,
        places: Vec::new(),
    }
}

/// The function that ends the program with the status, a byte: it keeps the
/// status and traps, and the host reads the status back.
fn exit() -> Body {
    let mut code = Code::default();
    code.local_get(0);
    code.i32_const(255);
    code.op(I32_AND);
    code.global_set(STATUS);
    code.op(UNREACHABLE);
    code.op(END);
    Body {
        locals: vec![],
        code: code.bytes,
        places: Vec::new(),
    }
}

/// The function the host calls: it runs `main`, which takes the words the
/// program was started with when it asks, and there are none but the
/// program's name.
fn start(program: &Program, image: &Image, main: u32) -> Body {
    let mut code = Code::default();
    if program.takes_arguments() {
        code.i64_const(1);
        code.i32_const(image.arguments.unwrap_or(0) as i32);
        code.i32_const(0);
    }
    code.call(main);
    code.op(END);
    Body {
        locals: vec![],
        code: code.bytes,
        places: Vec::new(),
    }
}

/// The function that gives the status the program ended with, or -1.
fn status() -> Body {
    let mut code = Code::default();
    code.global_get(STATUS);
    code.op(END);
    Body {
        locals: vec![],
        code: code.bytes,
        places: Vec::new(),
    }
}

/// How many locals, parameters among them, a function may have: the most that
/// engines take, as V8 does.
const LOCALS: u32 = 50_000;

fn kind_number(kind: Kind) -> usize {
    match kind {
        Kind::I32 => 0,
        Kind::I64 => 1,
        Kind::F32 => 2,
        Kind::F64 => 3,
    }
}

/// Which local, among those of its kind that the function needs, each class of
/// values has, and how many of each kind that is. A class is the values whose
/// place is one, as `same` says. Two classes have the same local when neither
/// is alive where the other is made or read, by the places where each is alive,
/// taken from the first to the last, which is more than it is, and so never
/// too little. The parameters of the function keep the places they have.
fn reuse(f: &Function, same: &[usize], fixed: &[u32]) -> (Vec<u32>, [u32; 4]) {
    let values = f.types.len();
    // The place of the start of each block, of each of its instructions, and
    // of its end, in the order of the blocks.
    let mut start = Vec::with_capacity(f.blocks.len());
    let mut at = 0;
    for block in &f.blocks {
        start.push(at);
        at += block.instructions.len() + 2;
    }
    let end = |b: usize| start[b] + f.blocks[b].instructions.len() + 1;
    let (mut first, mut last) = (vec![usize::MAX; values], vec![0; values]);
    let mut touch = |v: Value, place: usize| {
        let class = same[v.0];
        first[class] = first[class].min(place);
        last[class] = last[class].max(place);
    };
    // The classes a block reads before it makes them, which are alive when it
    // begins, and those it makes, which are not alive before.
    let mut read = vec![Vec::new(); f.blocks.len()];
    let mut made = vec![Vec::new(); f.blocks.len()];
    let mut seen = vec![usize::MAX; values];
    for (b, block) in f.blocks.iter().enumerate() {
        let mut defined = std::collections::HashSet::new();
        for p in &block.params {
            touch(*p, start[b]);
            defined.insert(same[p.0]);
            made[b].push(same[p.0]);
        }
        let mut reads = |v: Value, place: usize, defined: &std::collections::HashSet<usize>| {
            if !defined.contains(&same[v.0]) && seen[same[v.0]] != b {
                seen[same[v.0]] = b;
                read[b].push(same[v.0]);
            }
            let _ = place;
        };
        for (i, instruction) in block.instructions.iter().enumerate() {
            let place = start[b] + 1 + i;
            for v in instruction.op.operands() {
                touch(v, place);
                reads(v, place, &defined);
            }
            for r in &instruction.results {
                touch(*r, place);
                defined.insert(same[r.0]);
                made[b].push(same[r.0]);
            }
        }
        let place = end(b);
        for v in block.end.operands() {
            touch(v, place);
            reads(v, place, &defined);
        }
        // A jump writes the parameters of the block it goes to, where it ends.
        for jump in block.end.jumps() {
            for p in &f.blocks[jump.block].params {
                touch(*p, place);
            }
        }
    }
    // The classes alive at the start and at the end of each block: those it
    // reads before it makes them, and those a block it goes to has alive at its
    // start that it does not make there.
    let alive_in = liveness(f, same, &read, &made);
    for (b, (inside, outside)) in alive_in.iter().enumerate() {
        for &class in inside {
            first[class] = first[class].min(start[b]);
            last[class] = last[class].max(start[b]);
        }
        for &class in outside {
            first[class] = first[class].min(end(b));
            last[class] = last[class].max(end(b));
        }
    }
    let mut slot = vec![u32::MAX; values];
    let mut counts = [0u32; 4];
    for here in [Kind::I32, Kind::I64, Kind::F32, Kind::F64] {
        let mut classes: Vec<usize> = (0..values)
            .filter(|&v| {
                same[v] == v
                    && fixed[v] == u32::MAX
                    && first[v] != usize::MAX
                    && kind(f.types[v]) == here
            })
            .collect();
        classes.sort_by_key(|&v| (first[v], v));
        let mut busy = std::collections::BinaryHeap::new();
        let mut free: Vec<u32> = Vec::new();
        let count = &mut counts[kind_number(here)];
        for v in classes {
            while let Some(&std::cmp::Reverse((over, freed))) = busy.peek() {
                if over >= first[v] {
                    break;
                }
                busy.pop();
                free.push(freed);
            }
            let chosen = free.pop().unwrap_or_else(|| {
                *count += 1;
                *count - 1
            });
            slot[v] = chosen;
            busy.push(std::cmp::Reverse((last[v], chosen)));
        }
    }
    (slot, counts)
}

/// For each block, the classes alive when it begins and those alive when it
/// ends, by what each block reads before it makes it and what it makes. A class
/// is alive at the end of a block when a block it goes to has it alive at its
/// start, and at its start when the block reads it before it makes it or
/// when it is alive at its end and the block does not make it.
fn liveness(
    f: &Function,
    _same: &[usize],
    read: &[Vec<usize>],
    made: &[Vec<usize>],
) -> Vec<(Vec<usize>, Vec<usize>)> {
    let blocks = f.blocks.len();
    let mut inside: Vec<std::collections::BTreeSet<usize>> =
        read.iter().map(|r| r.iter().copied().collect()).collect();
    let made: Vec<std::collections::HashSet<usize>> =
        made.iter().map(|m| m.iter().copied().collect()).collect();
    let mut outside: Vec<std::collections::BTreeSet<usize>> = vec![Default::default(); blocks];
    loop {
        let mut changed = false;
        for b in (0..blocks).rev() {
            for jump in f.blocks[b].end.jumps() {
                let wanted: Vec<usize> = inside[jump.block].iter().copied().collect();
                for class in wanted {
                    if outside[b].insert(class) && !made[b].contains(&class) {
                        changed |= inside[b].insert(class);
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
    inside
        .into_iter()
        .zip(outside)
        .map(|(i, o)| (i.into_iter().collect(), o.into_iter().collect()))
        .collect()
}

/// The signature and the body of a function of the form.
fn lower(f: &Function, context: &Context) -> (Signature, Body) {
    let shape = structure::analyze(f);
    let params = &f.blocks[0].params;
    // The value whose place a value has. A parameter of a block that one jump
    // reaches is the value that jump gives it, which is alive wherever the
    // block is, and needs no copy.
    let mut same: Vec<usize> = (0..f.types.len()).collect();
    if let Some(shape) = &shape {
        let mut entering: Vec<Option<&Jump>> = vec![None; f.blocks.len()];
        for (from, block) in f.blocks.iter().enumerate() {
            if shape.order[from] == usize::MAX {
                continue;
            }
            for jump in jumps(&block.end) {
                if !shape.back(from, jump.block) {
                    entering[jump.block] = Some(jump);
                }
            }
        }
        let mut blocks: Vec<usize> = (1..f.blocks.len())
            .filter(|&b| shape.order[b] != usize::MAX)
            .collect();
        blocks.sort_by_key(|&b| shape.order[b]);
        for b in blocks {
            if let (Some(jump), 1, false) = (entering[b], shape.entries[b], shape.header[b]) {
                for (p, a) in f.blocks[b].params.iter().zip(&jump.args) {
                    same[p.0] = same[a.0];
                }
            }
        }
    }
    let mut local = vec![u32::MAX; f.types.len()];
    for (place, p) in params.iter().enumerate() {
        local[p.0] = place as u32;
    }
    // The values that are alive at once are the ones that need a local each, so
    // a value gets the local of one that is dead by the time it is made.
    let (slot, counts) = reuse(f, &same, &local);
    // A function whose blocks are not written as loops keeps the number of the
    // block to run in a local, which comes first of the locals of 32 bits,
    // which come before those of 64.
    let mut next = params.len() as u32;
    let pc = next;
    let numbered = shape.is_none() && f.blocks.len() > 1;
    let mut locals = Vec::new();
    for here in [Kind::I32, Kind::I64, Kind::F32, Kind::F64] {
        let before = next;
        if here == Kind::I32 && numbered {
            next += 1;
        }
        let first = next;
        next += counts[kind_number(here)];
        for (v, ty) in f.types.iter().enumerate() {
            if local[v] == u32::MAX && same[v] == v && slot[v] != u32::MAX && kind(*ty) == here {
                local[v] = first + slot[v];
            }
        }
        if next > before {
            locals.push((next - before, here));
        }
    }
    for v in 0..local.len() {
        local[v] = local[same[v]];
    }
    // The bytes of each `Frame` are taken from the stack of frames when the
    // function begins, and given back when it returns, and a local holds where
    // they begin.
    let mut offsets: Vec<Option<u32>> = vec![None; f.types.len()];
    let mut total = 0;
    for i in f.blocks.iter().flat_map(|b| &b.instructions) {
        if let Op::Frame { bytes } = i.op {
            offsets[i.results[0].0] = Some(total);
            total += (bytes as u32).next_multiple_of(16);
        }
    }
    let frame = (total > 0).then(|| {
        let sp = next;
        next += 1;
        locals.push((1, Kind::I32));
        Taken { sp, total, offsets }
    });
    if next > LOCALS {
        let name = if f.name.is_empty() {
            "<unnamed>"
        } else {
            &f.name
        };
        panic!(
            "wasm does not build function {name}: it needs {next} locals, as many values are alive at once, and an engine takes {LOCALS}"
        );
    }
    let mut lowering = Lowering {
        f,
        context,
        code: Code::default(),
        places: Vec::new(),
        local,
        pc,
        block: 0,
        shape,
        frames: Vec::new(),
        taken: frame,
    };
    lowering.function();
    let signature = Signature {
        params: params.iter().map(|p| kind(f.types[p.0])).collect(),
        results: f.results.iter().map(|t| kind(*t)).collect(),
    };
    (
        signature,
        Body {
            locals,
            code: lowering.code.bytes,
            places: lowering.places,
        },
    )
}

/// The jumps a block's end makes, in order.
fn jumps(end: &End) -> Vec<&Jump> {
    match end {
        End::Jump(j) => vec![j],
        End::Switch { jumps, .. } => jumps.iter().collect(),
        End::Return(_) => Vec::new(),
    }
}

/// What is open at a place in a function that is written as loops: the
/// structures that `br` can leave.
#[derive(Clone, Copy, PartialEq)]
enum Frame {
    /// The loop that starts with a block, which a jump back goes to.
    Loop(usize),
    /// The `block` that ends where a block is written, which every jump that
    /// is not a jump back goes to.
    Before(usize),
    /// An `if`, or a `block` that a choice between many ends.
    Other,
}

/// The bytes a function takes from the stack of frames.
struct Taken {
    /// The local that holds where they begin.
    sp: u32,
    total: u32,
    /// Where each `Frame` is, from where they begin, by its value.
    offsets: Vec<Option<u32>>,
}

struct Lowering<'a> {
    taken: Option<Taken>,
    f: &'a Function,
    context: &'a Context<'a>,
    code: Code,
    /// Where in `code` the code of an instruction with a new place begins, with
    /// that place.
    places: Vec<(usize, Place)>,
    /// The local of each value.
    local: Vec<u32>,
    /// The local that holds the number of the block to run, when the function
    /// has no shape.
    pc: u32,
    /// The block being written, when the function has no shape.
    block: usize,
    /// How the function's blocks are written as loops, or `None` when they
    /// cannot be.
    shape: Option<Shape>,
    frames: Vec<Frame>,
}

impl Lowering<'_> {
    fn function(&mut self) {
        if let Some(taken) = &self.taken {
            let (sp, total) = (taken.sp, taken.total);
            let end = self.context.stack_end.expect("a module with a stack");
            self.code.global_get(STACK);
            self.code.local_set(sp);
            self.code.local_get(sp);
            self.code.i32_const(total as i32);
            self.code.op(I32_ADD);
            self.code.global_set(STACK);
            // A function that needs more than the stack has ends the program.
            self.code.global_get(STACK);
            self.code.i32_const(end as i32);
            self.code.op(I32_GT_U);
            self.code.open(IF);
            self.code.i32_const(1);
            self.code.call(self.context.exit);
            self.code.op(END);
        }
        if self.shape.is_some() {
            self.tree(0);
            self.code.op(UNREACHABLE);
            self.code.op(END);
        } else {
            self.numbered();
        }
    }

    /// Gives the bytes the function took from the stack of frames back.
    fn give_back(&mut self) {
        if let Some(taken) = &self.taken {
            self.code.local_get(taken.sp);
            self.code.global_set(STACK);
        }
    }

    /// Writes a block, and the blocks it dominates, in a `loop` when it starts
    /// one.
    fn tree(&mut self, x: usize) {
        let shape = self.shape.as_ref().unwrap();
        let (header, merges) = (shape.header[x], shape.merges[x].clone());
        if header {
            self.code.open(LOOP);
            self.frames.push(Frame::Loop(x));
        }
        self.within(x, &merges);
        if header {
            self.code.op(END);
            self.frames.pop();
        }
    }

    /// Writes a block after the blocks of `merges` that it dominates and that
    /// more than one jump reaches, each of which is written where the `block`
    /// that its jumps leave ends.
    fn within(&mut self, x: usize, merges: &[usize]) {
        match merges.split_last() {
            Some((&y, rest)) => {
                self.code.open(BLOCK);
                self.frames.push(Frame::Before(y));
                self.within(x, rest);
                self.code.op(END);
                self.frames.pop();
                self.tree(y);
            }
            None => self.node(x),
        }
    }

    /// Notes that the code written from here on is for an instruction that is
    /// written at `at`, when that is not the place of the code before it.
    fn note_place(&mut self, at: Place) {
        if self.places.last().is_none_or(|(_, last)| *last != at) {
            self.places.push((self.code.bytes.len(), at));
        }
    }

    /// The instructions of a block, and how it ends.
    fn node(&mut self, x: usize) {
        let f = self.f;
        let block = &f.blocks[x];
        for instruction in &block.instructions {
            self.note_place(instruction.at);
            self.instruction(instruction);
        }
        match &block.end {
            End::Return(values) => {
                // What ends the function is its own, named for it, and no line wrote it.
                self.note_place(Place::of(&f.name));
                for v in values {
                    self.get(*v);
                }
                self.give_back();
                self.code.op(RETURN);
            }
            End::Jump(jump) => self.branch(x, jump),
            End::Switch { on, jumps } => self.choice(x, *on, jumps),
        }
    }

    /// How far out the structure that a jump leaves is, or `None` when the
    /// block is written where the jump is.
    fn depth(&self, from: usize, jump: &Jump) -> Option<u32> {
        let shape = self.shape.as_ref().unwrap();
        let wanted = if shape.back(from, jump.block) {
            Frame::Loop(jump.block)
        } else if shape.merge(jump.block) {
            Frame::Before(jump.block)
        } else {
            return None;
        };
        let place = self.frames.iter().rposition(|&frame| frame == wanted);
        Some((self.frames.len() - 1 - place.expect("the block is open")) as u32)
    }

    /// The jump from a block: the values it gives, and then the way there.
    fn branch(&mut self, from: usize, jump: &Jump) {
        self.give(jump);
        match self.depth(from, jump) {
            Some(depth) => self.code.br(depth),
            None => self.tree(jump.block),
        }
    }

    fn choice(&mut self, from: usize, on: Value, jumps: &[Jump]) {
        match jumps {
            [only] => self.branch(from, only),
            [zero, one] => {
                // A jump that gives nothing and leaves a structure is a
                // `br_if`, and the other is what follows it.
                let bare = |me: &Self, jump: &Jump| {
                    me.given(jump).is_empty() && me.depth(from, jump).is_some()
                };
                if bare(self, one) {
                    self.get_wrapped(on);
                    self.code.br_if(self.depth(from, one).unwrap());
                    self.branch(from, zero);
                } else if bare(self, zero) {
                    self.get_wrapped(on);
                    self.code.op(I32_EQZ);
                    self.code.br_if(self.depth(from, zero).unwrap());
                    self.branch(from, one);
                } else {
                    self.get_wrapped(on);
                    self.code.open(IF);
                    self.frames.push(Frame::Other);
                    self.branch(from, one);
                    self.code.op(ELSE);
                    self.branch(from, zero);
                    self.code.op(END);
                    self.frames.pop();
                }
            }
            _ => {
                let m = jumps.len();
                for _ in 0..m {
                    self.code.open(BLOCK);
                    self.frames.push(Frame::Other);
                }
                self.get_wrapped(on);
                self.code.br_table(m as u32);
                for jump in jumps {
                    self.code.op(END);
                    self.frames.pop();
                    self.branch(from, jump);
                }
            }
        }
    }

    /// The parameters of the block a jump goes to that are not already the
    /// values it gives, as the value each is given.
    fn given(&self, jump: &Jump) -> Vec<(Value, Value)> {
        let params = &self.f.blocks[jump.block].params;
        let pairs = params.iter().zip(&jump.args);
        pairs
            .filter(|(p, a)| self.local[p.0] != self.local[a.0])
            .map(|(p, a)| (*p, *a))
            .collect()
    }

    /// Puts the values a jump gives in the parameters of its block, as if all
    /// were copied at once.
    fn give(&mut self, jump: &Jump) {
        let given = self.given(jump);
        for (_, a) in &given {
            self.get(*a);
        }
        for (p, _) in given.iter().rev() {
            self.set(*p);
        }
    }

    /// The function with the number of the block to run in a local: a loop
    /// that goes to the code of each block through one `br_table`, so a jump
    /// is that number and a `br`.
    fn numbered(&mut self) {
        let n = self.f.blocks.len();
        self.code.open(LOOP);
        for _ in 0..n {
            self.code.open(BLOCK);
        }
        if n > 1 {
            self.code.local_get(self.pc);
            self.code.br_table(n as u32);
        }
        for block in 0..n {
            self.code.op(END);
            self.block = block;
            self.write_block();
        }
        self.code.op(END);
        self.code.op(UNREACHABLE);
        self.code.op(END);
    }

    fn write_block(&mut self) {
        let f = self.f;
        let block = &f.blocks[self.block];
        for instruction in &block.instructions {
            self.note_place(instruction.at);
            self.instruction(instruction);
        }
        match &block.end {
            End::Return(values) => {
                // What ends the function is its own, named for it, and no line wrote it.
                self.note_place(Place::of(&f.name));
                for v in values {
                    self.get(*v);
                }
                self.give_back();
                self.code.op(RETURN);
            }
            End::Jump(jump) => self.jump(jump, 0),
            End::Switch { on, jumps } => self.switch(*on, jumps),
        }
    }

    /// Makes the jump of a function with the number of the block: the values
    /// it gives are put in the parameters of its block, and the loop of the
    /// function goes on from there. `open` is how many blocks and choices are
    /// open here besides those that make the function's own.
    fn jump(&mut self, jump: &Jump, open: u32) {
        self.give(jump);
        let n = self.f.blocks.len();
        if n > 1 {
            self.code.i32_const(jump.block as i32);
            self.code.local_set(self.pc);
        }
        self.code.br((n - 1 - self.block) as u32 + open);
    }

    fn switch(&mut self, on: Value, jumps: &[Jump]) {
        let m = jumps.len() as u32;
        match jumps {
            [only] => self.jump(only, 0),
            [zero, one] => {
                self.get_wrapped(on);
                self.code.open(IF);
                self.jump(one, 1);
                self.code.op(ELSE);
                self.jump(zero, 1);
                self.code.op(END);
            }
            _ => {
                for _ in 0..m {
                    self.code.open(BLOCK);
                }
                self.get_wrapped(on);
                self.code.br_table(m);
                for (place, jump) in jumps.iter().enumerate() {
                    self.code.op(END);
                    self.jump(jump, m - 1 - place as u32);
                }
            }
        }
    }

    fn get(&mut self, v: Value) {
        self.code.local_get(self.local[v.0]);
    }

    fn set(&mut self, v: Value) {
        self.code.local_set(self.local[v.0]);
    }

    fn kind(&self, v: Value) -> Kind {
        kind(self.f.types[v.0])
    }

    /// Whether the value is a float of 64 bits, and not one of 32.
    fn double(&self, v: Value) -> bool {
        match self.f.types[v.0] {
            Type::Float(bits) => bits == FloatBits::F64,
            _ => panic!("an integer where a float is needed"),
        }
    }

    fn bits(&self, v: Value) -> Bits {
        match self.f.types[v.0] {
            Type::Int(bits) => bits,
            Type::Float(_) => panic!("a float where an integer is needed"),
            Type::Vector(_) => unreachable!("Target::emit refuses a function with a vector"),
            Type::Address => panic!("an address where an integer is needed"),
        }
    }

    /// Gives a number of 64 bits, or an address, a place in 32.
    fn get_wrapped(&mut self, v: Value) {
        self.get(v);
        if self.kind(v) == Kind::I64 {
            self.code.op(I32_WRAP_I64);
        }
    }

    /// Writes the integer of `bits` bits that is on the stack with the bits
    /// above its size cleared, which only a size below 32 bits has.
    fn clear_above(&mut self, bits: Bits) {
        match bits {
            Bits::B8 => {
                self.code.i32_const(0xFF);
                self.code.op(I32_AND);
            }
            Bits::B16 => {
                self.code.i32_const(0xFFFF);
                self.code.op(I32_AND);
            }
            Bits::B32 | Bits::B64 => {}
        }
    }

    /// Makes the integer of `bits` bits that is on the stack, in 32, the
    /// number it stands for as a signed number.
    fn sign_extend(&mut self, bits: Bits) {
        match bits {
            Bits::B8 => self.code.op(I32_EXTEND8_S),
            Bits::B16 => self.code.op(I32_EXTEND16_S),
            Bits::B32 | Bits::B64 => {}
        }
    }

    /// Puts a value of up to 32 bits on the stack as a number of 64, read as a
    /// signed or an unsigned number.
    fn get_extended(&mut self, v: Value, signed: bool) {
        let bits = self.bits(v);
        self.get(v);
        if signed {
            self.sign_extend(bits);
            self.code.op(I64_EXTEND_I32_S);
        } else {
            self.code.op(I64_EXTEND_I32_U);
        }
    }

    /// The number of bits a shift moves by, in 32: the second value, modulo 64.
    fn shift_count(&mut self, b: Value) {
        self.get_wrapped(b);
        self.code.i32_const(63);
        self.code.op(I32_AND);
    }

    fn instruction(&mut self, instruction: &Instruction) {
        let f = self.f;
        let results = &instruction.results;
        match &instruction.op {
            Op::Frame { .. } => {
                let taken = self.taken.as_ref().expect("a function that took bytes");
                self.code.local_get(taken.sp);
                self.code
                    .i32_const(taken.offsets[results[0].0].expect("a frame laid out") as i32);
                self.code.op(I32_ADD);
                self.set(results[0]);
            }
            Op::FloatFused { .. } => {
                unreachable!("Target::emit gives a fused product as a call")
            }
            Op::FloatBinary { op, a, b } => {
                let double = self.double(*a);
                self.get(*a);
                self.get(*b);
                self.code.op(match (op, double) {
                    (FloatBinary::Add, false) => F32_ADD,
                    (FloatBinary::Sub, false) => F32_SUB,
                    (FloatBinary::Mul, false) => F32_MUL,
                    (FloatBinary::Div, false) => F32_DIV,
                    (FloatBinary::Add, true) => F64_ADD,
                    (FloatBinary::Sub, true) => F64_SUB,
                    (FloatBinary::Mul, true) => F64_MUL,
                    (FloatBinary::Div, true) => F64_DIV,
                });
                self.set(results[0]);
            }
            Op::FloatNegate(value) => {
                let double = self.double(*value);
                self.get(*value);
                self.code.op(if double { F64_NEG } else { F32_NEG });
                self.set(results[0]);
            }
            Op::FloatSqrt(value) => {
                let double = self.double(*value);
                self.get(*value);
                self.code.op(if double { F64_SQRT } else { F32_SQRT });
                self.set(results[0]);
            }
            Op::FloatBits(value) => {
                let double = self.double(*value);
                self.get(*value);
                self.code.op(if double {
                    I64_REINTERPRET_F64
                } else {
                    I32_REINTERPRET_F32
                });
                self.set(results[0]);
            }
            Op::FloatFromBits(value) => {
                let double = self.double(results[0]);
                self.get(*value);
                self.code.op(if double {
                    F64_REINTERPRET_I64
                } else {
                    F32_REINTERPRET_I32
                });
                self.set(results[0]);
            }
            Op::FloatCompare { op, a, b } => {
                let double = self.double(*a);
                self.get(*a);
                self.get(*b);
                self.code.op(match (op, double) {
                    (FloatCompare::Equal, false) => F32_EQ,
                    (FloatCompare::NotEqual, false) => F32_NE,
                    (FloatCompare::Less, false) => F32_LT,
                    (FloatCompare::LessEqual, false) => F32_LE,
                    (FloatCompare::Greater, false) => F32_GT,
                    (FloatCompare::GreaterEqual, false) => F32_GE,
                    (FloatCompare::Equal, true) => F64_EQ,
                    (FloatCompare::NotEqual, true) => F64_NE,
                    (FloatCompare::Less, true) => F64_LT,
                    (FloatCompare::LessEqual, true) => F64_LE,
                    (FloatCompare::Greater, true) => F64_GT,
                    (FloatCompare::GreaterEqual, true) => F64_GE,
                });
                self.set(results[0]);
            }
            Op::FloatFromInt { signed, value } => {
                let double = self.double(results[0]);
                let from = self.bits(*value);
                self.get(*value);
                let op = match (from, signed, double) {
                    (Bits::B64, true, false) => F32_CONVERT_I64_S,
                    (Bits::B64, false, false) => F32_CONVERT_I64_U,
                    (Bits::B64, true, true) => F64_CONVERT_I64_S,
                    (Bits::B64, false, true) => F64_CONVERT_I64_U,
                    (_, true, false) => {
                        self.sign_extend(from);
                        F32_CONVERT_I32_S
                    }
                    (_, false, false) => F32_CONVERT_I32_U,
                    (_, true, true) => {
                        self.sign_extend(from);
                        F64_CONVERT_I32_S
                    }
                    (_, false, true) => F64_CONVERT_I32_U,
                };
                self.code.op(op);
                self.set(results[0]);
            }
            Op::IntFromFloat { signed, value } => {
                let double = self.double(*value);
                let to = self.bits(results[0]);
                self.get(*value);
                // The truncation that saturates, and gives 0 for a float that
                // is not a number: `i32.trunc_sat_f32_s` is 0, `_u` 1, the
                // same of an `f64` 2 and 3, and of an `i64` 4 to 7.
                let which = match (to == Bits::B64, double, signed) {
                    (false, false, true) => 0,
                    (false, false, false) => 1,
                    (false, true, true) => 2,
                    (false, true, false) => 3,
                    (true, false, true) => 4,
                    (true, false, false) => 5,
                    (true, true, true) => 6,
                    (true, true, false) => 7,
                };
                self.code.saturating(which);
                self.set(results[0]);
                // A size below 32 bits is held to the least and the greatest
                // number it has.
                if matches!(to, Bits::B8 | Bits::B16) {
                    let result = results[0];
                    let bits = to.count();
                    let (low, high) = if *signed {
                        (-(1i32 << (bits - 1)), (1i32 << (bits - 1)) - 1)
                    } else {
                        (0, (1i32 << bits) - 1)
                    };
                    if *signed {
                        self.code.i32_const(low);
                        self.get(result);
                        self.get(result);
                        self.code.i32_const(low);
                        self.code.op(I32_LT_S);
                        self.code.op(SELECT);
                        self.set(result);
                    }
                    self.code.i32_const(high);
                    self.get(result);
                    self.get(result);
                    self.code.i32_const(high);
                    self.code.op(if *signed { I32_GT_S } else { I32_GT_U });
                    self.code.op(SELECT);
                    self.set(result);
                    self.get(result);
                    self.clear_above(to);
                    self.set(result);
                }
            }
            Op::FloatResize(value) => {
                let from = self.double(*value);
                let to = self.double(results[0]);
                self.get(*value);
                if from && !to {
                    self.code.op(F32_DEMOTE_F64);
                } else if !from && to {
                    self.code.op(F64_PROMOTE_F32);
                }
                self.set(results[0]);
            }
            Op::VectorBinary { .. }
            | Op::VectorFloatBinary { .. }
            | Op::VectorFloatFused { .. }
            | Op::VectorCompare { .. }
            | Op::VectorFloatCompare { .. }
            | Op::VectorChoose { .. }
            | Op::VectorSplat(_)
            | Op::VectorExtract { .. }
            | Op::VectorInsert { .. }
            | Op::VectorSum(_)
            | Op::VectorFirstSet(_) => {
                unreachable!("Target::emit refuses a function with a vector")
            }
            Op::Constant(constant) => {
                let ty = f.types[results[0].0];
                match (constant, ty) {
                    (Constant::Float(n) | Constant::Int(n), Type::Float(FloatBits::F32)) => {
                        self.code.f32_const(*n as u32)
                    }
                    (Constant::Float(n) | Constant::Int(n), Type::Float(FloatBits::F64)) => {
                        self.code.f64_const(*n)
                    }
                    (Constant::Float(_), _) => unreachable!("a float of a type that is not one"),
                    (Constant::Vector(..), _) | (_, Type::Vector(_)) => {
                        unreachable!("Target::emit refuses a function with a vector")
                    }
                    (Constant::Int(n), Type::Int(Bits::B64)) => self.code.i64_const(*n as i64),
                    (Constant::Int(n), Type::Int(bits)) => {
                        let mask = if bits == Bits::B32 {
                            u64::from(u32::MAX)
                        } else {
                            (1u64 << bits.count()) - 1
                        };
                        self.code.i32_const((*n & mask) as u32 as i32);
                    }
                    (Constant::Null, _) | (Constant::Int(_), Type::Address) => {
                        self.code.i32_const(0)
                    }
                    (Constant::Data(text), _) => {
                        self.code.i32_const(self.context.starts[*text] as i32)
                    }
                }
                self.set(results[0]);
            }
            Op::Binary { op, a, b } => {
                match f.types[results[0].0] {
                    Type::Float(_) => unreachable!("an integer instruction gives an integer"),
                    Type::Vector(_) => {
                        unreachable!("Target::emit refuses a function with a vector")
                    }
                    Type::Address => {
                        self.get(*a);
                        self.get_wrapped(*b);
                        self.code.op(I32_ADD);
                    }
                    Type::Int(bits) => self.binary(*op, bits, *a, *b),
                }
                self.set(results[0]);
            }
            Op::Compare { op, a, b } => {
                self.compare(*op, *a, *b);
                self.set(results[0]);
            }
            Op::Overflows { op, signed, a, b } => {
                let bits = self.bits(*a);
                self.binary(*op, bits, *a, *b);
                self.set(results[0]);
                self.overflowed(*op, *signed, bits, *a, *b, results[0]);
                self.set(results[1]);
            }
            Op::Widen { signed, value } => {
                let from = self.bits(*value);
                let to = self.bits(results[0]);
                self.get(*value);
                if to == Bits::B64 {
                    if *signed {
                        self.sign_extend(from);
                        self.code.op(I64_EXTEND_I32_S);
                    } else {
                        self.code.op(I64_EXTEND_I32_U);
                    }
                } else if *signed {
                    self.sign_extend(from);
                    self.clear_above(to);
                }
                self.set(results[0]);
            }
            Op::Narrow(value) => {
                let to = self.bits(results[0]);
                self.get_wrapped(*value);
                self.clear_above(to);
                self.set(results[0]);
            }
            Op::Choose {
                on,
                then,
                otherwise,
            } => {
                self.get(*then);
                self.get(*otherwise);
                self.get(*on);
                self.code.op(SELECT);
                self.set(results[0]);
            }
            Op::Load { ty, address } => {
                self.get(*address);
                match ty {
                    Type::Int(Bits::B8) => self.code.memory(I32_LOAD8_U),
                    Type::Int(Bits::B16) => self.code.memory(I32_LOAD16_U),
                    Type::Int(Bits::B32) => self.code.memory(I32_LOAD),
                    Type::Int(Bits::B64) => self.code.memory(I64_LOAD),
                    Type::Float(FloatBits::F32) => self.code.memory(F32_LOAD),
                    Type::Float(FloatBits::F64) => self.code.memory(F64_LOAD),
                    Type::Vector(_) => {
                        unreachable!("Target::emit refuses a function with a vector")
                    }
                    Type::Address => {
                        self.code.memory(I64_LOAD);
                        self.code.op(I32_WRAP_I64);
                    }
                }
                self.set(results[0]);
            }
            Op::Store { ty, address, value } => {
                self.get(*address);
                self.get(*value);
                match ty {
                    Type::Int(Bits::B8) => self.code.memory(I32_STORE8),
                    Type::Int(Bits::B16) => self.code.memory(I32_STORE16),
                    Type::Int(Bits::B32) => self.code.memory(I32_STORE),
                    Type::Int(Bits::B64) => self.code.memory(I64_STORE),
                    Type::Float(FloatBits::F32) => self.code.memory(F32_STORE),
                    Type::Float(FloatBits::F64) => self.code.memory(F64_STORE),
                    Type::Vector(_) => {
                        unreachable!("Target::emit refuses a function with a vector")
                    }
                    Type::Address => {
                        self.code.op(I64_EXTEND_I32_U);
                        self.code.memory(I64_STORE);
                    }
                }
            }
            Op::Call { function, args } => {
                for a in args {
                    self.get(*a);
                }
                self.code.call(self.context.first + *function as u32);
                for r in results.iter().rev() {
                    self.set(*r);
                }
            }
            Op::System { call, args } => self.system(*call, args, results),
        }
    }

    fn system(&mut self, call: System, args: &[Value], results: &[Value]) {
        match call {
            System::Write(stream) => {
                let print = self.context.print.expect("a write is an import");
                self.code.i32_const(match stream {
                    Stream::Stdout => 1,
                    Stream::Stderr => 2,
                });
                self.get(args[0]);
                self.get_wrapped(args[1]);
                self.code.call(print);
            }
            System::Allocate => {
                self.get(args[0]);
                self.code.call(self.context.allocate);
                self.set(results[0]);
            }
            System::Entropy => {
                let fill = self.context.entropy.expect("a fill is an import");
                self.get(args[0]);
                self.get_wrapped(args[1]);
                self.code.call(fill);
                self.set(results[0]);
            }
            System::Read => unreachable!("a module that reads is refused before it is built"),
            // The calls only Linux has are built into the scheduler for every
            // machine and never run here: they give minus the error for a call
            // the kernel does not have.
            System::Raw(crate::scalar::Raw::Kind) => {
                self.code.i64_const(0);
                self.set(results[0]);
            }
            System::Raw(
                crate::scalar::Raw::EpollCreate1
                | crate::scalar::Raw::EpollCtl
                | crate::scalar::Raw::EpollWait
                | crate::scalar::Raw::Kqueue
                | crate::scalar::Raw::Kevent,
            ) => {
                self.code.i64_const(-38);
                self.set(results[0]);
            }
            System::Raw(_) => {
                unreachable!("a module that asks the kernel is refused before it is built")
            }
            System::ArenaOpen
            | System::ArenaClose
            | System::TaskMark
            | System::TaskRelease
            | System::TaskFrame
            | System::TaskFree
            | System::TaskMove
            | System::RegionNew
            | System::RegionFree
            | System::RegionMove
            | System::RegionKeep => {
                unreachable!("an arena is gone once the stacks of `alloc` are passed")
            }
            System::Suspend => {
                unreachable!("a suspension is gone once the state machines are made")
            }
            System::Exit => {
                self.get_wrapped(args[0]);
                self.code.call(self.context.exit);
            }
        }
    }

    fn compare(&mut self, op: Compare, a: Value, b: Value) {
        let bits = self.bits(a);
        let wide = bits == Bits::B64;
        let signed = op == Compare::LessS;
        self.get(a);
        if !wide && signed {
            self.sign_extend(bits);
        }
        self.get(b);
        if !wide && signed {
            self.sign_extend(bits);
        }
        self.code.op(match (op, wide) {
            (Compare::Equal, false) => I32_EQ,
            (Compare::Equal, true) => I64_EQ,
            (Compare::LessU, false) => I32_LT_U,
            (Compare::LessU, true) => I64_LT_U,
            (Compare::LessS, false) => I32_LT_S,
            (Compare::LessS, true) => I64_LT_S,
        });
    }

    /// Puts what `op` gives of the two integers of `bits` bits on the stack,
    /// with the bits that do not fit left off.
    fn binary(&mut self, op: Binary, bits: Bits, a: Value, b: Value) {
        if bits == Bits::B64 {
            return self.binary_64(op, a, b);
        }
        let size = bits.count() as i32;
        match op {
            Binary::Add | Binary::Sub | Binary::Mul => {
                self.get(a);
                self.get(b);
                self.code.op(match op {
                    Binary::Add => I32_ADD,
                    Binary::Sub => I32_SUB,
                    _ => I32_MUL,
                });
                self.clear_above(bits);
            }
            Binary::And | Binary::Or | Binary::Xor => {
                self.get(a);
                self.get(b);
                self.code.op(match op {
                    Binary::And => I32_AND,
                    Binary::Or => I32_OR,
                    _ => I32_XOR,
                });
            }
            // A number of fewer than 32 bits is held as the number, so the
            // engine divides it as it is; a signed one is extended first, and
            // the answer has its bits above the size cleared.
            Binary::DivU | Binary::RemU => {
                self.get(a);
                self.get(b);
                self.code.op(if op == Binary::DivU {
                    I32_DIV_U
                } else {
                    I32_REM_U
                });
            }
            Binary::DivS | Binary::RemS => {
                self.get(a);
                self.sign_extend(bits);
                self.get(b);
                self.sign_extend(bits);
                self.code.op(if op == Binary::DivS {
                    I32_DIV_S
                } else {
                    I32_REM_S
                });
                self.clear_above(bits);
            }
            Binary::MulHighU if bits == Bits::B32 => {
                self.get_extended(a, false);
                self.get_extended(b, false);
                self.code.op(I64_MUL);
                self.code.i64_const(32);
                self.code.op(I64_SHR_U);
                self.code.op(I32_WRAP_I64);
            }
            Binary::MulHighU => {
                self.get(a);
                self.get(b);
                self.code.op(I32_MUL);
                self.code.i32_const(size);
                self.code.op(I32_SHR_U);
            }
            Binary::MulHighS if bits == Bits::B32 => {
                self.get_extended(a, true);
                self.get_extended(b, true);
                self.code.op(I64_MUL);
                self.code.i64_const(32);
                self.code.op(I64_SHR_S);
                self.code.op(I32_WRAP_I64);
            }
            Binary::MulHighS => {
                self.get(a);
                self.sign_extend(bits);
                self.get(b);
                self.sign_extend(bits);
                self.code.op(I32_MUL);
                self.code.i32_const(size);
                self.code.op(I32_SHR_S);
                self.clear_above(bits);
            }
            // A shift by as many bits as the integer has, or more, leaves
            // none of them, and the engine takes the count modulo 32.
            Binary::ShiftLeft => {
                self.get(a);
                self.shift_count(b);
                self.code.op(I32_SHL);
                self.clear_above(bits);
                self.code.i32_const(0);
                self.shift_count(b);
                self.code.i32_const(size);
                self.code.op(I32_LT_U);
                self.code.op(SELECT);
            }
            Binary::ShiftRightU => {
                self.get(a);
                self.shift_count(b);
                self.code.op(I32_SHR_U);
                self.code.i32_const(0);
                self.shift_count(b);
                self.code.i32_const(32);
                self.code.op(I32_LT_U);
                self.code.op(SELECT);
            }
            Binary::ShiftRightS => {
                self.get(a);
                self.sign_extend(bits);
                // A count of 31 or more gives the sign alone, as 31 does.
                self.shift_count(b);
                self.code.i32_const(31);
                self.shift_count(b);
                self.code.i32_const(31);
                self.code.op(I32_LT_U);
                self.code.op(SELECT);
                self.code.op(I32_SHR_S);
                self.clear_above(bits);
            }
        }
    }

    fn binary_64(&mut self, op: Binary, a: Value, b: Value) {
        match op {
            Binary::MulHighU | Binary::MulHighS => {
                self.get(a);
                self.get(b);
                self.code.call(match op {
                    Binary::MulHighU => self.context.high_unsigned,
                    _ => self.context.high_signed,
                });
            }
            Binary::ShiftLeft | Binary::ShiftRightU | Binary::ShiftRightS => {
                self.get(a);
                self.get(b);
                if self.kind(b) == Kind::I32 {
                    self.code.op(I64_EXTEND_I32_U);
                }
                // The engine takes the count modulo 64.
                self.code.op(match op {
                    Binary::ShiftLeft => I64_SHL,
                    Binary::ShiftRightU => I64_SHR_U,
                    _ => I64_SHR_S,
                });
            }
            Binary::DivU | Binary::DivS | Binary::RemU | Binary::RemS => {
                self.get(a);
                self.get(b);
                self.code.op(match op {
                    Binary::DivU => I64_DIV_U,
                    Binary::DivS => I64_DIV_S,
                    Binary::RemU => I64_REM_U,
                    _ => I64_REM_S,
                });
            }
            _ => {
                self.get(a);
                self.get(b);
                self.code.op(match op {
                    Binary::Add => I64_ADD,
                    Binary::Sub => I64_SUB,
                    Binary::Mul => I64_MUL,
                    Binary::And => I64_AND,
                    Binary::Or => I64_OR,
                    _ => I64_XOR,
                });
            }
        }
    }

    /// Puts on the stack whether the number `result`, which `op` gave of `a`
    /// and `b` as `bits`-bit numbers read as signed or as unsigned, is not what
    /// `op` gives of them.
    fn overflowed(
        &mut self,
        op: Binary,
        signed: bool,
        bits: Bits,
        a: Value,
        b: Value,
        result: Value,
    ) {
        if bits == Bits::B64 {
            return self.overflowed_64(op, signed, a, b, result);
        }
        if op == Binary::Sub && !signed {
            self.get(a);
            self.get(b);
            self.code.op(I32_LT_U);
            return;
        }
        // Worked out in 64 bits, which holds every number of these sizes, a
        // product too.
        self.get_extended(a, signed);
        self.get_extended(b, signed);
        self.code.op(match op {
            Binary::Add => I64_ADD,
            Binary::Sub => I64_SUB,
            _ => I64_MUL,
        });
        if signed {
            self.get_extended(result, true);
            self.code.op(I64_NE);
        } else {
            self.code.i64_const(i64::from(bits.count()));
            self.code.op(I64_SHR_U);
            self.code.op(I64_EQZ);
            self.code.op(I32_EQZ);
        }
    }

    fn overflowed_64(&mut self, op: Binary, signed: bool, a: Value, b: Value, result: Value) {
        match (op, signed) {
            (Binary::Add, false) => {
                self.get(result);
                self.get(a);
                self.code.op(I64_LT_U);
            }
            (Binary::Sub, false) => {
                self.get(a);
                self.get(b);
                self.code.op(I64_LT_U);
            }
            // A sum overflows when its sign is not that of either operand, which
            // have the same, and a difference when its sign is not that of the
            // first, and the operands have different signs.
            (Binary::Add, true) => {
                self.get(a);
                self.get(result);
                self.code.op(I64_XOR);
                self.get(b);
                self.get(result);
                self.code.op(I64_XOR);
                self.code.op(I64_AND);
                self.code.i64_const(0);
                self.code.op(I64_LT_S);
            }
            (Binary::Sub, true) => {
                self.get(a);
                self.get(b);
                self.code.op(I64_XOR);
                self.get(a);
                self.get(result);
                self.code.op(I64_XOR);
                self.code.op(I64_AND);
                self.code.i64_const(0);
                self.code.op(I64_LT_S);
            }
            (_, false) => {
                self.get(a);
                self.get(b);
                self.code.call(self.context.high_unsigned);
                self.code.i64_const(0);
                self.code.op(I64_NE);
            }
            (_, true) => {
                self.get(a);
                self.get(b);
                self.code.call(self.context.high_signed);
                self.get(result);
                self.code.i64_const(63);
                self.code.op(I64_SHR_S);
                self.code.op(I64_NE);
            }
        }
    }
}
