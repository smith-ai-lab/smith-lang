//! Runs a program in the scalar form, in-process. What it gives is what the
//! program does.

use std::collections::HashMap;

use super::vector::{
    lane_of, lane_type, vector_binary, vector_choose, vector_compare, vector_first_set,
    vector_float_binary, vector_float_compare, vector_float_fused, vector_splat, vector_sum,
    with_lane,
};
use super::{
    Binary, Bits, Compare, Constant, End, FloatBinary, FloatBits, FloatCompare, Fused, Jump, Lane,
    Op, Program, Stream, System, Type, Value,
};

/// What a program did: what it wrote to standard output, and the status it
/// ended with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    pub stdout: Vec<u8>,
    pub status: i32,
}

/// What the system gives a program that takes the words it was started with.
#[derive(Clone, Copy)]
enum Given<'a> {
    /// The program's name and then its words, as bytes, which the system lays
    /// out as Linux and macOS do.
    Words(&'a [Vec<u8>]),
    /// A command line, as the units of 16 bits that Windows gives.
    Line(&'a [u16]),
}

/// Where the program's texts start, so that no text is at the address that
/// holds nothing.
const BASE: usize = 0x1000;

/// What a value holds.
#[derive(Clone, Copy, Debug)]
enum Word {
    Int(u64),
    Address(usize),
    /// The bits of a vector, lane 0 in the lowest.
    Vector(u128),
}

struct Machine<'a> {
    program: &'a Program,
    /// The program's texts one after another, which is memory from `BASE`.
    memory: Vec<u8>,
    /// Where each text starts, as an address.
    starts: Vec<usize>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    /// What standard input holds, and how much of it has been read.
    stdin: Vec<u8>,
    read: usize,
    /// How many bytes the system will still give.
    left: usize,
    /// How many bytes each request to the system for memory has asked for.
    requests: Vec<usize>,
    /// The place of the handler's `take`, when the program has it, and how many
    /// bytes each call of it asked for.
    taking: Option<usize>,
    taken: Vec<usize>,
    /// The place of the runtime's copy, when the program has it, and how many
    /// bytes each call of it moved.
    copying: Option<usize>,
    copied: Vec<usize>,
    /// How many blocks the program may still enter.
    blocks: u64,
    /// How many instructions and ends of blocks have run.
    ran: u64,
    /// How many calls are open.
    depth: usize,
    /// What says whether a wait is over at once, and each wait so far.
    ready: Option<&'a mut (dyn FnMut(u64, u64) -> bool + 'a)>,
    waited: Vec<Waited>,
}

/// A wait a program made: its kind and handle, and whether what it waited for
/// was there at once, so that the task went on, or it was not, so that the
/// task was suspended and resumed once it was.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Waited {
    pub kind: u64,
    pub handle: u64,
    pub ready: bool,
}

/// The program ends, with the status.
struct Ended(u8);

/// How many blocks a program may enter before it is taken to never end. A
/// program that has not ended by then has not ended in a time any test can
/// give it, so the evaluator stops it with a panic that says so.
const BLOCKS: u64 = 3_000_000;

/// How many calls may be open at once before the program is taken to never
/// end, as a call that never returns takes the memory of the evaluator, which
/// keeps the open calls in a list. A call is a block or two, so a program that
/// goes this deep and does no more than call has entered `BLOCKS` blocks long
/// before.
const DEPTH: usize = 10_000_000;

/// Runs the program, with standard output going to memory, where no write
/// is refused, and no request for memory is either.
pub fn run(program: &Program) -> Outcome {
    run_with_limit(program, usize::MAX)
}

/// Runs the program as `run` does, except that the system gives no more
/// than `limit` bytes in all, and a request for more ends the program with
/// status 1.
pub fn run_with_limit(program: &Program, limit: usize) -> Outcome {
    run_counting(program, limit).0
}

/// Runs the program as `run_with_limit` does, and also gives how many bytes
/// each request to the system for memory asked for, in order.
pub fn run_counting(program: &Program, limit: usize) -> (Outcome, Vec<usize>) {
    let (outcome, _, requests, _, _, _) = start(program, limit, Given::Words(&[]));
    (outcome, requests)
}

/// Runs the program as `run_counting` does, and gives how many bytes each call
/// of the handler's `take` asked for, in order, as the program asked, before
/// they are rounded and taken from a piece. A program that has no handler asks
/// for none.
pub fn run_taking(program: &Program, limit: usize) -> (Outcome, Vec<usize>) {
    let (outcome, _, _, taken, _, _) = start(program, limit, Given::Words(&[]));
    (outcome, taken)
}

/// Runs the program as `run_counting` does, and gives how many bytes each call
/// of the runtime's copy moved, in order, which is what a list that grows moves
/// into its new memory.
pub fn run_copying(program: &Program, limit: usize) -> (Outcome, Vec<usize>, Vec<usize>) {
    let (outcome, _, requests, _, _, copied) = start(program, limit, Given::Words(&[]));
    (outcome, requests, copied)
}

/// Runs the program as `run` does, and also gives how many instructions ran,
/// counting the end of each block entered as one.
pub fn run_measured(program: &Program) -> (Outcome, u64) {
    let (outcome, _, _, _, ran, _) = start(program, usize::MAX, Given::Words(&[]));
    (outcome, ran)
}

/// Runs the program as `run` does, with the bytes as what standard input holds.
pub fn run_with_input(program: &Program, input: &[u8]) -> Outcome {
    start_with(program, usize::MAX, Given::Words(&[]), input).0
}

/// Runs a program that takes the words it was started with, as Linux and
/// macOS give them: its name, which is not one of them, and then the words.
pub fn run_with_words(program: &Program, words: &[Vec<u8>]) -> Outcome {
    run_with_words_and_errors(program, words).0
}

/// Runs a program as `run_with_words` does, and also gives what it wrote to
/// standard error.
pub fn run_with_words_and_errors(program: &Program, words: &[Vec<u8>]) -> (Outcome, Vec<u8>) {
    let (outcome, errors, _, _, _, _) = start(program, usize::MAX, Given::Words(words));
    (outcome, errors)
}

/// Runs a program that takes the words it was started with, as Windows gives
/// them: one command line, whose first word is the program's name.
pub fn run_with_command_line(program: &Program, line: &[u16]) -> Outcome {
    run_with_command_line_and_errors(program, line).0
}

/// Runs a program as `run_with_command_line` does, and also gives what it
/// wrote to standard error.
pub fn run_with_command_line_and_errors(program: &Program, line: &[u16]) -> (Outcome, Vec<u8>) {
    let (outcome, errors, _, _, _, _) = start(program, usize::MAX, Given::Line(line));
    (outcome, errors)
}

/// What a run gave: the outcome, what was written to standard error, each
/// request for memory, each call of `take`, how many instructions and block
/// ends ran, and each call of the runtime's copy.
type Run = (Outcome, Vec<u8>, Vec<usize>, Vec<usize>, u64, Vec<usize>);

fn start(program: &Program, limit: usize, given: Given) -> Run {
    start_with(program, limit, given, &[])
}

fn start_with(program: &Program, limit: usize, given: Given, input: &[u8]) -> Run {
    let mut memory = Vec::new();
    let mut starts = Vec::new();
    for text in &program.data {
        starts.push(BASE + memory.len());
        memory.extend_from_slice(text);
    }
    let args = if program.takes_arguments() {
        arguments(&mut memory, given)
    } else {
        Vec::new()
    };
    let mut machine = Machine {
        program,
        memory,
        starts,
        stdout: Vec::new(),
        stderr: Vec::new(),
        stdin: input.to_vec(),
        read: 0,
        left: limit,
        requests: Vec::new(),
        taking: (program.functions.iter()).position(|f| *f == super::handler::take()),
        taken: Vec::new(),
        copying: (program.functions.iter()).position(|f| *f == super::runtime::copy()),
        copied: Vec::new(),
        blocks: BLOCKS,
        ran: 0,
        depth: 0,
        ready: None,
        waited: Vec::new(),
    };
    let status = match machine.call(program.main, args) {
        Ok(_) => 0,
        Err(Ended(status)) => i32::from(status),
    };
    (
        Outcome {
            stdout: machine.stdout,
            status,
        },
        machine.stderr,
        machine.requests,
        machine.taken,
        machine.ran,
        machine.copied,
    )
}

/// Puts what the system gives in memory after the program's texts, and gives
/// the values `main` starts with.
fn arguments(memory: &mut Vec<u8>, given: Given) -> Vec<Word> {
    match given {
        Given::Words(words) => {
            // The program's name comes first, then the words, each ending
            // with a byte 0, and then the addresses of them, and a last that
            // holds nothing.
            let mut places = Vec::new();
            for text in std::iter::once(&b"program"[..]).chain(words.iter().map(Vec::as_slice)) {
                places.push(BASE + memory.len());
                memory.extend_from_slice(text);
                memory.push(0);
            }
            memory.resize(memory.len().next_multiple_of(8), 0);
            let table = BASE + memory.len();
            for place in places.iter().chain(std::iter::once(&0)) {
                memory.extend_from_slice(&(*place as u64).to_le_bytes());
            }
            vec![
                Word::Int(places.len() as u64),
                Word::Address(table),
                Word::Address(0),
            ]
        }
        Given::Line(line) => {
            memory.resize(memory.len().next_multiple_of(2), 0);
            let at = BASE + memory.len();
            for unit in line.iter().chain(std::iter::once(&0)) {
                memory.extend_from_slice(&unit.to_le_bytes());
            }
            vec![Word::Int(u64::MAX), Word::Address(0), Word::Address(at)]
        }
    }
}

/// Runs one function of the program on integers, and gives the integers it
/// returns, each as its bits in the size of its type. A function that takes
/// or gives an address cannot be run so.
pub fn call(program: &Program, function: usize, args: &[u64]) -> Vec<u64> {
    let mut machine = Machine {
        program,
        memory: Vec::new(),
        starts: Vec::new(),
        stdout: Vec::new(),
        stderr: Vec::new(),
        stdin: Vec::new(),
        read: 0,
        left: usize::MAX,
        requests: Vec::new(),
        taking: None,
        taken: Vec::new(),
        copying: None,
        copied: Vec::new(),
        blocks: BLOCKS,
        ran: 0,
        depth: 0,
        ready: None,
        waited: Vec::new(),
    };
    let words = args.iter().map(|n| Word::Int(*n)).collect();
    integers(machine.call(function, words).ok().expect("memory"))
}

fn integers(words: Vec<Word>) -> Vec<u64> {
    words
        .into_iter()
        .map(|w| match w {
            Word::Int(n) => n,
            Word::Address(_) => panic!("a function that gives an address"),
            Word::Vector(_) => panic!("a function that gives a vector"),
        })
        .collect()
}

/// A machine with no texts and no input, for a function that is run alone.
fn alone<'a>(
    program: &'a Program,
    ready: &'a mut (dyn FnMut(u64, u64) -> bool + 'a),
) -> Machine<'a> {
    Machine {
        program,
        memory: Vec::new(),
        starts: Vec::new(),
        stdout: Vec::new(),
        stderr: Vec::new(),
        stdin: Vec::new(),
        read: 0,
        left: usize::MAX,
        requests: Vec::new(),
        taking: None,
        taken: Vec::new(),
        copying: None,
        copied: Vec::new(),
        blocks: BLOCKS,
        ran: 0,
        depth: 0,
        ready: Some(ready),
        waited: Vec::new(),
    }
}

/// Runs one function on integers as `call` does, where each wait it makes is
/// answered by `ready`, which is given its kind and handle, and gives the
/// integers it returns and each wait in order.
pub fn call_waiting(
    program: &Program,
    function: usize,
    args: &[u64],
    ready: &mut dyn FnMut(u64, u64) -> bool,
) -> (Vec<u64>, Vec<Waited>) {
    let mut machine = alone(program, ready);
    let words = args.iter().map(|n| Word::Int(*n)).collect();
    let given = machine.call(function, words).ok().expect("memory");
    (integers(given), machine.waited)
}

/// Runs a function that `suspend` made a state machine, as a scheduler of one
/// task does: it makes a frame of `bytes`, starts the task with `args`, and
/// polls it until it is done, and each time it is not, it asks `ready` whether
/// the wait it gives is over, as `call_waiting` does of the same function
/// before it was split, and polls again. It gives what the function returns,
/// each wait in order, and how many times the poll said the task was waiting.
pub fn poll_to_end(
    program: &Program,
    start: usize,
    poll: usize,
    bytes: u64,
    args: &[u64],
    ready: &mut dyn FnMut(u64, u64) -> bool,
) -> (Vec<u64>, Vec<Waited>, usize) {
    let mut machine = alone(program, ready);
    machine.memory.resize(16, 0);
    let frame = BASE + machine.memory.len();
    machine
        .memory
        .resize(machine.memory.len() + bytes as usize, 0);
    // A frame that held the last task's values is not what a new task starts
    // with: the start gives the frame what it needs.
    machine.memory[16..].fill(0xA5);
    let mut words = vec![Word::Address(frame)];
    words.extend(args.iter().map(|n| Word::Int(*n)));
    machine.call(start, words).ok().expect("memory");
    let mut waiting = 0;
    loop {
        let given = machine
            .call(poll, vec![Word::Address(frame)])
            .ok()
            .expect("memory");
        let mut given = integers(given).into_iter();
        let status = given.next().expect("a status");
        let (kind, handle) = (
            given.next().expect("a kind"),
            given.next().expect("a handle"),
        );
        if status == 0 {
            return (given.collect(), machine.waited, waiting);
        }
        waiting += 1;
        machine.wait(kind, handle);
    }
}

/// The bits of an integer of the size, in a word.
pub(super) fn mask(bits: Bits) -> u64 {
    match bits {
        Bits::B64 => u64::MAX,
        _ => (1u64 << bits.count()) - 1,
    }
}

/// The number the bits stand for as a signed integer of the size.
fn signed(n: u64, bits: Bits) -> i128 {
    let up = 64 - bits.count();
    (((n << up) as i64) >> up) as i128
}

/// Whether `op` is a division or a remainder that is not given what it may be
/// given: a divisor of zero, or, for the signed ones, the least number of the
/// size divided by -1, whose quotient the size cannot hold. A processor faults
/// or gives a number that depends on which it is, so the form does not say
/// what such an instruction does.
pub(super) fn divides_wrongly(op: Binary, bits: Bits, x: u64, y: u64) -> bool {
    match op {
        Binary::DivU | Binary::RemU => y == 0,
        Binary::DivS | Binary::RemS => {
            y == 0 || (signed(y, bits) == -1 && signed(x, bits) == -(1i128 << (bits.count() - 1)))
        }
        _ => false,
    }
}

/// What `op` gives for two integers of the size, as bits of that size. The
/// evaluator and the pass that folds constants both work it out here.
pub(super) fn binary(op: Binary, bits: Bits, x: u64, y: u64) -> u64 {
    let size = bits.count();
    let n = match op {
        Binary::Add => x.wrapping_add(y),
        Binary::Sub => x.wrapping_sub(y),
        Binary::Mul => x.wrapping_mul(y),
        Binary::MulHighU => ((x as u128 * y as u128) >> size) as u64,
        Binary::MulHighS => ((signed(x, bits) * signed(y, bits)) >> size) as u64,
        Binary::DivU | Binary::DivS | Binary::RemU | Binary::RemS => {
            assert!(
                !divides_wrongly(op, bits, x, y),
                "{op:?} of {x} and {y} at {size} bits is given a divisor that it may not be"
            );
            match op {
                Binary::DivU => x / y,
                Binary::RemU => x % y,
                Binary::DivS => (signed(x, bits) / signed(y, bits)) as u64,
                _ => (signed(x, bits) % signed(y, bits)) as u64,
            }
        }
        Binary::And => x & y,
        Binary::Xor => x ^ y,
        Binary::Or => x | y,
        Binary::ShiftLeft => x << (y & 63),
        Binary::ShiftRightU => x >> (y & 63),
        Binary::ShiftRightS => (signed(x, bits) as i64 >> (y & 63)) as u64,
    };
    n & mask(bits)
}

/// 1 if the two integers of the size are as `op` asks, and 0 if not.
/// Whether the number `op` gives of `x` and `y`, read as signed or as unsigned
/// numbers of `bits` bits, does not fit them. This is what the checked
/// functions are, so every target is held to it.
pub(super) fn overflows(op: Binary, signed: bool, bits: Bits, x: u64, y: u64) -> bool {
    let n = bits.count();
    let (least, greatest): (i128, i128) = if signed {
        (-(1i128 << (n - 1)), (1i128 << (n - 1)) - 1)
    } else {
        (0, (1i128 << n) - 1)
    };
    let value = |v: u64| -> i128 {
        if signed {
            i128::from(widen(true, bits, Bits::B64, v) as i64)
        } else {
            i128::from(v & mask(bits))
        }
    };
    let (a, b) = (value(x), value(y));
    let exact = match op {
        Binary::Add => a.checked_add(b),
        Binary::Sub => a.checked_sub(b),
        Binary::Mul => a.checked_mul(b),
        _ => panic!("an operation that is not an addition, a subtraction, or a product"),
    };
    // A product that does not fit 127 bits does not fit 64.
    exact.is_none_or(|e| e < least || e > greatest)
}

pub(super) fn compare(op: Compare, bits: Bits, x: u64, y: u64) -> u64 {
    (match op {
        Compare::Equal => x == y,
        Compare::LessU => x < y,
        Compare::LessS => signed(x, bits) < signed(y, bits),
    }) as u64
}

/// The integer of `from` bits widened to `to` bits, read as signed or not.
pub(super) fn widen(read_signed: bool, from: Bits, to: Bits, n: u64) -> u64 {
    let n = if read_signed {
        signed(n, from) as u64
    } else {
        n
    };
    n & mask(to)
}

/// The low bits of the integer, as many as `to` has.
pub(super) fn narrow(to: Bits, n: u64) -> u64 {
    n & mask(to)
}

/// How many bits an integer type has.
pub(super) fn bits_of(ty: Type) -> Bits {
    match ty {
        Type::Int(bits) => bits,
        Type::Float(_) => panic!("a float where an integer is needed"),
        Type::Vector(_) => panic!("a vector where an integer is needed"),
        Type::Address => panic!("an address where an integer is needed"),
    }
}

/// How many bits a float type has.
pub(super) fn float_bits_of(ty: Type) -> FloatBits {
    match ty {
        Type::Float(bits) => bits,
        _ => panic!("what is not a float where a float is needed"),
    }
}

/// The one float that is not a number which every result that is not a number
/// is: positive, quiet, with no payload. The sign and the payload of such a
/// result are the chip's, so no result holds more than that.
fn not_a_number(bits: FloatBits) -> u64 {
    match bits {
        FloatBits::F32 => 0x7fc0_0000,
        FloatBits::F64 => 0x7ff8_0000_0000_0000,
    }
}

/// The bits of a float of the size, in a word.
pub(super) fn float_mask(bits: FloatBits) -> u64 {
    match bits {
        FloatBits::F32 => u64::from(u32::MAX),
        FloatBits::F64 => u64::MAX,
    }
}

/// The float the bits stand for, as a number of 64 bits, which holds a float
/// of 32 bits exactly.
fn float_value(bits: FloatBits, n: u64) -> f64 {
    match bits {
        FloatBits::F32 => f64::from(f32::from_bits(n as u32)),
        FloatBits::F64 => f64::from_bits(n),
    }
}

/// The bits of a float of the size that is a result: the one float that is not
/// a number when it is not.
fn float_result_f32(x: f32) -> u64 {
    if x.is_nan() {
        not_a_number(FloatBits::F32)
    } else {
        u64::from(x.to_bits())
    }
}

fn float_result_f64(x: f64) -> u64 {
    if x.is_nan() {
        not_a_number(FloatBits::F64)
    } else {
        x.to_bits()
    }
}

/// The square root of a float of the size, rounded once.
pub(super) fn float_sqrt(bits: FloatBits, x: u64) -> u64 {
    match bits {
        FloatBits::F32 => float_result_f32(f32::from_bits(x as u32).sqrt()),
        FloatBits::F64 => float_result_f64(f64::from_bits(x).sqrt()),
    }
}

/// What `op` gives for two floats of the size, as the bits of a float of that
/// size.
pub(super) fn float_binary(op: FloatBinary, bits: FloatBits, x: u64, y: u64) -> u64 {
    match bits {
        FloatBits::F32 => {
            let (a, b) = (f32::from_bits(x as u32), f32::from_bits(y as u32));
            float_result_f32(match op {
                FloatBinary::Add => a + b,
                FloatBinary::Sub => a - b,
                FloatBinary::Mul => a * b,
                FloatBinary::Div => a / b,
            })
        }
        FloatBits::F64 => {
            let (a, b) = (f64::from_bits(x), f64::from_bits(y));
            float_result_f64(match op {
                FloatBinary::Add => a + b,
                FloatBinary::Sub => a - b,
                FloatBinary::Mul => a * b,
                FloatBinary::Div => a / b,
            })
        }
    }
}

/// What `op` gives for three floats of the size, worked out exactly and
/// rounded once, as the bits of a float of that size.
pub(super) fn float_fused(op: Fused, bits: FloatBits, x: u64, y: u64, z: u64) -> u64 {
    match bits {
        FloatBits::F32 => {
            let (a, b, c) = (
                f32::from_bits(x as u32),
                f32::from_bits(y as u32),
                f32::from_bits(z as u32),
            );
            float_result_f32(match op {
                Fused::MulAdd => a.mul_add(b, c),
                Fused::MulSub => a.mul_add(b, -c),
                Fused::NegMulAdd => (-a).mul_add(b, c),
            })
        }
        FloatBits::F64 => {
            let (a, b, c) = (f64::from_bits(x), f64::from_bits(y), f64::from_bits(z));
            float_result_f64(match op {
                Fused::MulAdd => a.mul_add(b, c),
                Fused::MulSub => a.mul_add(b, -c),
                Fused::NegMulAdd => (-a).mul_add(b, c),
            })
        }
    }
}

/// The float with its sign bit flipped.
pub(super) fn float_negate(bits: FloatBits, x: u64) -> u64 {
    (x ^ (1 << (bits.count() - 1))) & float_mask(bits)
}

/// 1 if the two floats of the size are as `op` asks, and 0 if not.
pub(super) fn float_compare(op: FloatCompare, bits: FloatBits, x: u64, y: u64) -> u64 {
    let (a, b) = (float_value(bits, x), float_value(bits, y));
    (match op {
        FloatCompare::Equal => a == b,
        FloatCompare::NotEqual => a != b,
        FloatCompare::Less => a < b,
        FloatCompare::LessEqual => a <= b,
        FloatCompare::Greater => a > b,
        FloatCompare::GreaterEqual => a >= b,
    }) as u64
}

/// The float of the size nearest the integer of `from` bits, read as signed or
/// as unsigned.
pub(super) fn float_from_int(read_signed: bool, from: Bits, to: FloatBits, n: u64) -> u64 {
    match (to, read_signed) {
        (FloatBits::F32, true) => u64::from((signed(n, from) as i64 as f32).to_bits()),
        (FloatBits::F32, false) => u64::from((n as f32).to_bits()),
        (FloatBits::F64, true) => (signed(n, from) as i64 as f64).to_bits(),
        (FloatBits::F64, false) => (n as f64).to_bits(),
    }
}

/// The integer of `to` bits the float of the size is cut to, read as signed or
/// as unsigned, as bits: the least or the greatest number that fits when it
/// does not, and 0 when it is not a number.
pub(super) fn int_from_float(read_signed: bool, from: FloatBits, to: Bits, x: u64) -> u64 {
    let f = float_value(from, x);
    if read_signed {
        let size = to.count();
        let least = -(1i128 << (size - 1));
        let greatest = (1i128 << (size - 1)) - 1;
        // The cast gives 0 for a float that is not a number, and saturates.
        ((i128::from(f as i64)).clamp(least, greatest) as u64) & mask(to)
    } else {
        (f as u64).min(mask(to))
    }
}

/// The float of the size `to` that is nearest the float of the size `from`.
pub(super) fn float_resize(from: FloatBits, to: FloatBits, x: u64) -> u64 {
    match (from, to) {
        (FloatBits::F32, FloatBits::F64) => float_result_f64(f64::from(f32::from_bits(x as u32))),
        (FloatBits::F64, FloatBits::F32) => float_result_f32(f64::from_bits(x) as f32),
        _ => x,
    }
}

/// How many bytes a value of the type takes in memory.
fn width(ty: Type) -> usize {
    match ty {
        Type::Float(bits) => bits.count() as usize / 8,
        Type::Int(bits) => bits.count() as usize / 8,
        Type::Vector(_) => 16,
        Type::Address => 8,
    }
}

/// A call that is open: where it is in its function, and what its values are.
struct Frame {
    function: usize,
    values: Vec<Option<Word>>,
    /// The block it is in, what the jump that entered it gave, and whether
    /// the block has been entered, which it has when the frame goes on after
    /// a call inside it.
    block: usize,
    args: Vec<Word>,
    entered: bool,
    /// The place of the instruction to run next in the block.
    at: usize,
    /// The address of each `Frame` of the call that has run, by its value.
    homes: HashMap<usize, usize>,
}

/// Why a frame stopped: it calls a function, or it returns.
enum Stop {
    Call { function: usize, args: Vec<Word> },
    Return(Vec<Word>),
}

impl Machine<'_> {
    /// Runs a call of the function to its end. The calls it makes, and the calls
    /// they make, are open frames in a list and not calls of this function, so
    /// that the depth of the calls is bounded by memory and not by the stack of
    /// the evaluator.
    fn call(&mut self, function: usize, args: Vec<Word>) -> Result<Vec<Word>, Ended> {
        let mut open: Vec<Frame> = Vec::new();
        let mut frame = self.enter(function, args);
        loop {
            match self.run(&mut frame)? {
                Stop::Call { function, args } => {
                    let next = self.enter(function, args);
                    open.push(std::mem::replace(&mut frame, next));
                }
                Stop::Return(words) => match open.pop() {
                    None => return Ok(words),
                    Some(mut caller) => {
                        let program = self.program;
                        let call = &program.functions[caller.function].blocks[caller.block]
                            .instructions[caller.at - 1];
                        for (result, word) in call.results.iter().zip(words) {
                            caller.values[result.0] = Some(word);
                        }
                        frame = caller;
                    }
                },
            }
        }
    }

    /// Notes a wait, and whether it is over, as the one who runs the program
    /// says.
    fn wait(&mut self, kind: u64, handle: u64) {
        let ready = match self.ready.as_mut() {
            Some(ready) => ready(kind, handle),
            None => true,
        };
        self.waited.push(Waited {
            kind,
            handle,
            ready,
        });
    }

    /// A frame for a call of the function that is about to begin.
    fn enter(&mut self, function: usize, args: Vec<Word>) -> Frame {
        self.depth += 1;
        assert!(
            self.depth <= DEPTH,
            "the program never ends: its calls never return"
        );
        Frame {
            function,
            values: vec![None; self.program.functions[function].types.len()],
            block: 0,
            args,
            entered: false,
            at: 0,
            homes: HashMap::new(),
        }
    }

    /// Runs the frame until it makes a call or returns.
    fn run(&mut self, frame: &mut Frame) -> Result<Stop, Ended> {
        let program = self.program;
        let f = &program.functions[frame.function];
        let Frame {
            values,
            block,
            args,
            entered,
            at,
            homes: frame_homes,
            ..
        } = frame;
        loop {
            let b = &f.blocks[*block];
            if !*entered {
                self.blocks = self.blocks.checked_sub(1).expect(
                    "the program never ends: it entered more blocks than any test gives it time for",
                );
                self.ran += b.instructions.len() as u64 + 1;
                for (param, word) in b.params.iter().zip(std::mem::take(args)) {
                    values[param.0] = Some(word);
                }
                *entered = true;
            }
            for (k, instruction) in b.instructions.iter().enumerate().skip(*at) {
                let get = |v: &Value| values[v.0].expect("a value that is defined");
                let given = match &instruction.op {
                    Op::Constant(Constant::Int(n)) => {
                        let bits = bits_of(f.types[instruction.results[0].0]);
                        vec![Word::Int(n & mask(bits))]
                    }
                    Op::Binary { a, b, .. }
                        if f.types[instruction.results[0].0] == Type::Address =>
                    {
                        let (Word::Address(x), Word::Int(y)) = (get(a), get(b)) else {
                            panic!("a move of what is not an address by a number")
                        };
                        vec![Word::Address(x.wrapping_add(y as usize))]
                    }
                    Op::Binary { op, a, b } => {
                        let bits = bits_of(f.types[instruction.results[0].0]);
                        let (Word::Int(x), Word::Int(y)) = (get(a), get(b)) else {
                            panic!("arithmetic on what is not an integer")
                        };
                        vec![Word::Int(binary(*op, bits, x, y))]
                    }
                    Op::Constant(Constant::Float(n)) => {
                        let bits = float_bits_of(f.types[instruction.results[0].0]);
                        vec![Word::Int(n & float_mask(bits))]
                    }
                    Op::FloatBinary { op, a, b } => {
                        let bits = float_bits_of(f.types[a.0]);
                        let (Word::Int(x), Word::Int(y)) = (get(a), get(b)) else {
                            panic!("arithmetic on what is not a float")
                        };
                        vec![Word::Int(float_binary(*op, bits, x, y))]
                    }
                    Op::FloatFused { op, a, b, c } => {
                        let bits = float_bits_of(f.types[a.0]);
                        let (Word::Int(x), Word::Int(y), Word::Int(z)) = (get(a), get(b), get(c))
                        else {
                            panic!("arithmetic on what is not a float")
                        };
                        vec![Word::Int(float_fused(*op, bits, x, y, z))]
                    }
                    Op::FloatNegate(value) => {
                        let bits = float_bits_of(f.types[value.0]);
                        let Word::Int(x) = get(value) else {
                            panic!("a negation of what is not a float")
                        };
                        vec![Word::Int(float_negate(bits, x))]
                    }
                    Op::FloatSqrt(value) => {
                        let bits = float_bits_of(f.types[value.0]);
                        let Word::Int(x) = get(value) else {
                            panic!("a square root of what is not a float")
                        };
                        vec![Word::Int(float_sqrt(bits, x))]
                    }
                    Op::FloatBits(value) | Op::FloatFromBits(value) => {
                        let Word::Int(x) = get(value) else {
                            panic!("the bits of what is not a float")
                        };
                        vec![Word::Int(x)]
                    }
                    Op::FloatCompare { op, a, b } => {
                        let bits = float_bits_of(f.types[a.0]);
                        let (Word::Int(x), Word::Int(y)) = (get(a), get(b)) else {
                            panic!("a comparison of what is not floats")
                        };
                        vec![Word::Int(float_compare(*op, bits, x, y))]
                    }
                    Op::FloatFromInt {
                        signed: read,
                        value,
                    } => {
                        let from = bits_of(f.types[value.0]);
                        let to = float_bits_of(f.types[instruction.results[0].0]);
                        let Word::Int(n) = get(value) else {
                            panic!("a float made of what is not an integer")
                        };
                        vec![Word::Int(float_from_int(*read, from, to, n))]
                    }
                    Op::IntFromFloat {
                        signed: read,
                        value,
                    } => {
                        let from = float_bits_of(f.types[value.0]);
                        let to = bits_of(f.types[instruction.results[0].0]);
                        let Word::Int(x) = get(value) else {
                            panic!("an integer made of what is not a float")
                        };
                        vec![Word::Int(int_from_float(*read, from, to, x))]
                    }
                    Op::FloatResize(value) => {
                        let from = float_bits_of(f.types[value.0]);
                        let to = float_bits_of(f.types[instruction.results[0].0]);
                        let Word::Int(x) = get(value) else {
                            panic!("a resizing of what is not a float")
                        };
                        vec![Word::Int(float_resize(from, to, x))]
                    }
                    Op::Constant(c @ Constant::Vector(..)) => {
                        vec![Word::Vector(c.vector_bits().expect("a vector"))]
                    }
                    Op::VectorBinary { op, a, b } => {
                        let Lane::Int(bits) = lane_type(f.types[a.0]) else {
                            panic!("arithmetic on lanes that are not integers")
                        };
                        let (Word::Vector(x), Word::Vector(y)) = (get(a), get(b)) else {
                            panic!("arithmetic on what is not a vector")
                        };
                        vec![Word::Vector(vector_binary(*op, bits, x, y))]
                    }
                    Op::VectorFloatBinary { op, a, b } => {
                        let Lane::Float(bits) = lane_type(f.types[a.0]) else {
                            panic!("arithmetic on lanes that are not floats")
                        };
                        let (Word::Vector(x), Word::Vector(y)) = (get(a), get(b)) else {
                            panic!("arithmetic on what is not a vector")
                        };
                        vec![Word::Vector(vector_float_binary(*op, bits, x, y))]
                    }
                    Op::VectorFloatFused { op, a, b, c } => {
                        let Lane::Float(bits) = lane_type(f.types[a.0]) else {
                            panic!("arithmetic on lanes that are not floats")
                        };
                        let (Word::Vector(x), Word::Vector(y), Word::Vector(z)) =
                            (get(a), get(b), get(c))
                        else {
                            panic!("arithmetic on what is not a vector")
                        };
                        vec![Word::Vector(vector_float_fused(*op, bits, x, y, z))]
                    }
                    Op::VectorCompare { op, signed, a, b } => {
                        let Lane::Int(bits) = lane_type(f.types[a.0]) else {
                            panic!("a comparison of lanes that are not integers")
                        };
                        let (Word::Vector(x), Word::Vector(y)) = (get(a), get(b)) else {
                            panic!("a comparison of what is not vectors")
                        };
                        vec![Word::Vector(vector_compare(*op, *signed, bits, x, y))]
                    }
                    Op::VectorFloatCompare { op, a, b } => {
                        let Lane::Float(bits) = lane_type(f.types[a.0]) else {
                            panic!("a comparison of lanes that are not floats")
                        };
                        let (Word::Vector(x), Word::Vector(y)) = (get(a), get(b)) else {
                            panic!("a comparison of what is not vectors")
                        };
                        vec![Word::Vector(vector_float_compare(*op, bits, x, y))]
                    }
                    Op::VectorChoose {
                        mask,
                        then,
                        otherwise,
                    } => {
                        let (Word::Vector(m), Word::Vector(t), Word::Vector(o)) =
                            (get(mask), get(then), get(otherwise))
                        else {
                            panic!("a choice of what is not vectors")
                        };
                        vec![Word::Vector(vector_choose(m, t, o))]
                    }
                    Op::VectorSplat(value) => {
                        let lane = lane_type(f.types[instruction.results[0].0]);
                        let Word::Int(n) = get(value) else {
                            panic!("a vector of what is not a number")
                        };
                        vec![Word::Vector(vector_splat(lane, n))]
                    }
                    Op::VectorExtract { vector, lane: at } => {
                        let lane = lane_type(f.types[vector.0]);
                        let Word::Vector(v) = get(vector) else {
                            panic!("a lane of what is not a vector")
                        };
                        vec![Word::Int(lane_of(lane, v, u32::from(*at)))]
                    }
                    Op::VectorInsert {
                        vector,
                        lane: at,
                        value,
                    } => {
                        let lane = lane_type(f.types[vector.0]);
                        let (Word::Vector(v), Word::Int(n)) = (get(vector), get(value)) else {
                            panic!("a lane put in what is not a vector")
                        };
                        vec![Word::Vector(with_lane(lane, v, u32::from(*at), n))]
                    }
                    Op::VectorSum(vector) => {
                        let lane = lane_type(f.types[vector.0]);
                        let Word::Vector(v) = get(vector) else {
                            panic!("the sum of what is not a vector")
                        };
                        vec![Word::Int(vector_sum(lane, v))]
                    }
                    Op::VectorFirstSet(mask) => {
                        let lane = lane_type(f.types[mask.0]);
                        let Word::Vector(m) = get(mask) else {
                            panic!("the first lane set of what is not a vector")
                        };
                        vec![Word::Int(vector_first_set(lane, m))]
                    }
                    Op::Overflows { op, signed, a, b } => {
                        let bits = bits_of(f.types[a.0]);
                        let (Word::Int(x), Word::Int(y)) = (get(a), get(b)) else {
                            panic!("arithmetic on what is not an integer")
                        };
                        vec![
                            Word::Int(binary(*op, bits, x, y)),
                            Word::Int(u64::from(overflows(*op, *signed, bits, x, y))),
                        ]
                    }
                    Op::Compare { op, a, b } => {
                        let bits = bits_of(f.types[a.0]);
                        let (Word::Int(x), Word::Int(y)) = (get(a), get(b)) else {
                            panic!("a comparison of what is not integers")
                        };
                        vec![Word::Int(compare(*op, bits, x, y))]
                    }
                    Op::Widen {
                        signed: read,
                        value,
                    } => {
                        let from = bits_of(f.types[value.0]);
                        let to = bits_of(f.types[instruction.results[0].0]);
                        let Word::Int(n) = get(value) else {
                            panic!("a widening of what is not an integer")
                        };
                        vec![Word::Int(widen(*read, from, to, n))]
                    }
                    Op::Narrow(value) => {
                        let to = bits_of(f.types[instruction.results[0].0]);
                        let Word::Int(n) = get(value) else {
                            panic!("a narrowing of what is not an integer")
                        };
                        vec![Word::Int(narrow(to, n))]
                    }
                    Op::Choose {
                        on,
                        then,
                        otherwise,
                    } => {
                        let Word::Int(on) = get(on) else {
                            panic!("a choice on what is not an integer")
                        };
                        vec![if on != 0 { get(then) } else { get(otherwise) }]
                    }
                    Op::Constant(Constant::Null) => vec![Word::Address(0)],
                    Op::Constant(Constant::Data(t)) => vec![Word::Address(self.starts[*t])],
                    Op::Call { function, args } => {
                        let args: Vec<Word> = args.iter().map(get).collect();
                        if self.copying == Some(*function) {
                            let Word::Int(count) = args[2] else {
                                panic!("a copy of what is not a number of bytes")
                            };
                            self.copied.push(count as usize);
                        }
                        if self.taking == Some(*function) {
                            let Word::Int(count) = args[2] else {
                                panic!("a request for room that is not a number")
                            };
                            self.taken.push(count as usize);
                        }
                        *at = k + 1;
                        return Ok(Stop::Call {
                            function: *function,
                            args,
                        });
                    }
                    // The bytes of a frame are those of the function's first run of
                    // this instruction, and are the same for each run after it.
                    Op::Frame { bytes } => {
                        let result = instruction.results[0].0;
                        let at = *frame_homes.entry(result).or_insert_with(|| {
                            let at = BASE + self.memory.len();
                            self.memory.resize(self.memory.len() + *bytes as usize, 0);
                            at
                        });
                        vec![Word::Address(at)]
                    }
                    Op::Load { ty, address } => {
                        let Word::Address(at) = get(address) else {
                            panic!("a read through what is not an address")
                        };
                        let size = width(*ty);
                        let from = at - BASE;
                        let mut n = 0u128;
                        for (k, byte) in self.memory[from..from + size].iter().enumerate() {
                            n |= (*byte as u128) << (8 * k);
                        }
                        vec![match ty {
                            Type::Address => Word::Address(n as usize),
                            Type::Int(_) | Type::Float(_) => Word::Int(n as u64),
                            Type::Vector(_) => Word::Vector(n),
                        }]
                    }
                    Op::Store { ty, address, value } => {
                        let Word::Address(at) = get(address) else {
                            panic!("a write through what is not an address")
                        };
                        let n = match get(value) {
                            Word::Int(n) => u128::from(n),
                            Word::Address(a) => a as u128,
                            Word::Vector(v) => v,
                        };
                        let size = width(*ty);
                        let from = at - BASE;
                        for k in 0..size {
                            self.memory[from + k] = (n >> (8 * k)) as u8;
                        }
                        Vec::new()
                    }
                    Op::System {
                        call: System::Allocate,
                        args,
                    } => {
                        let Word::Int(count) = get(&args[0]) else {
                            panic!("a request for memory that is not a number")
                        };
                        self.requests.push(count as usize);
                        if count > self.left as u64 {
                            return Err(Ended(1));
                        }
                        self.left -= count as usize;
                        let at = BASE + self.memory.len();
                        self.memory.resize(self.memory.len() + count as usize, 0);
                        vec![Word::Address(at)]
                    }
                    // Bytes nobody can predict are not what an evaluator that
                    // says the same every time can give: it fills each byte
                    // with its place, which is what no system gives.
                    Op::System {
                        call: System::Entropy,
                        args,
                    } => {
                        let (Word::Address(at), Word::Int(count)) = (get(&args[0]), get(&args[1]))
                        else {
                            panic!("a fill that is not of an address and a count")
                        };
                        let from = at - BASE;
                        for k in 0..count as usize {
                            self.memory[from + k] = k as u8;
                        }
                        vec![Word::Int(0)]
                    }
                    // Only the bytes asked for are taken, so what a program
                    // has not asked for is left for its next read.
                    Op::System {
                        call: System::Read,
                        args,
                    } => {
                        let (Word::Address(at), Word::Int(count)) = (get(&args[0]), get(&args[1]))
                        else {
                            panic!("a read that is not of an address and a count")
                        };
                        let n = (count as usize).min(self.stdin.len() - self.read);
                        let from = at - BASE;
                        self.memory[from..from + n]
                            .copy_from_slice(&self.stdin[self.read..self.read + n]);
                        self.read += n;
                        vec![Word::Int(n as u64)]
                    }
                    Op::System {
                        call: System::Raw(_),
                        ..
                    } => panic!("the evaluator has no network and no clock of its own"),
                    // A wait is answered by what the test says is ready, and is
                    // noted. Whether it is ready at once or the task is
                    // suspended and resumed, the program goes on after it with
                    // every value it held, so the run is the same.
                    Op::System {
                        call: System::Suspend,
                        args,
                    } => {
                        let (Word::Int(kind), Word::Int(handle)) = (get(&args[0]), get(&args[1]))
                        else {
                            panic!("a wait that is not of two numbers")
                        };
                        self.wait(kind, handle);
                        Vec::new()
                    }
                    Op::System {
                        call: System::ArenaOpen | System::ArenaClose,
                        ..
                    } => unreachable!("an arena is gone once the stacks of `alloc` are passed"),
                    Op::System {
                        call: System::Exit,
                        args,
                    } => {
                        let Word::Int(status) = get(&args[0]) else {
                            panic!("a status that is not a number")
                        };
                        return Err(Ended(status as u8));
                    }
                    Op::System {
                        call: System::Write(stream),
                        args,
                    } => {
                        let (Word::Address(at), Word::Int(count)) = (get(&args[0]), get(&args[1]))
                        else {
                            panic!("a write that is not of an address and a count")
                        };
                        if count > 0 {
                            let from = at - BASE;
                            let to = match stream {
                                Stream::Stdout => &mut self.stdout,
                                Stream::Stderr => &mut self.stderr,
                            };
                            to.extend_from_slice(&self.memory[from..from + count as usize]);
                        }
                        Vec::new()
                    }
                };
                for (result, word) in instruction.results.iter().zip(given) {
                    values[result.0] = Some(word);
                }
            }
            *at = 0;
            *entered = false;
            let jump = |j: &Jump| -> (usize, Vec<Word>) {
                let args = j.args.iter().map(|v| values[v.0].unwrap()).collect();
                (j.block, args)
            };
            match &b.end {
                End::Return(returned) => {
                    self.depth -= 1;
                    return Ok(Stop::Return(
                        returned.iter().map(|v| values[v.0].unwrap()).collect(),
                    ));
                }
                End::Jump(j) => (*block, *args) = jump(j),
                End::Switch { on, jumps } => {
                    let Some(Word::Int(n)) = values[on.0] else {
                        panic!("a choice on what is not an integer")
                    };
                    (*block, *args) = jump(&jumps[n as usize]);
                }
            }
        }
    }
}
