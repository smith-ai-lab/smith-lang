//! The scalar form: the one small typed form below the kernel form that
//! every target receives. A function is a list of blocks. A block takes
//! parameters, runs its instructions in order, and ends by jumping to a
//! block with arguments, by choosing between blocks, or by returning
//! values. Every value is named once, has a type, and has a size.
//!
//! Only what the programs lowered so far need is built: whole numbers, at
//! the size of their type, memory read and written one value at a time, and
//! a call into the system that asks for memory. The rest of the form
//! `knowledge/compiler.md` describes is added with the features that need
//! it. A program is lowered to it from the checked program, by
//! `check::to_scalar`, and what it needs at run time is in `runtime`.

mod build;
mod circle;
mod common;
mod escape;
mod eval;
mod exp;
mod exp_fast;
mod fold;
mod fuse;
mod graph;
pub mod handler;
mod inline;
mod ln;
pub mod pool;
mod recur;
mod remat;
mod remove;
pub mod runtime;
mod ryu;
mod sink;
mod suspend;
mod tail;
mod thread;
mod trig;
mod trim;
mod unfuse;
mod vector;
mod wellformed;
mod wide;
mod words;

pub use build::Builder;
pub use common::common;
pub use eval::{
    Outcome, Waited, call, call_waiting, poll_cancelling, poll_through, poll_to_end, run,
    run_copying, run_counting, run_measured, run_taking, run_with_command_line,
    run_with_command_line_and_errors, run_with_input, run_with_limit, run_with_words,
    run_with_words_and_errors,
};
pub use exp_fast::table as exp_table;
pub use fold::fold;
pub use fuse::fuse;
pub use inline::{BOUND, LEAF_BOUND, inline, size};
pub use recur::recur;
pub use remat::rematerialized;
pub use remove::{WALKS, remove, remove_counting};
pub use ryu::tables as ryu_tables;
pub use suspend::{
    CANCEL, CANCEL_AT, Frames, SCHEDULER_AT, Starter, Suspended, Task, result_places, suspend,
    suspend_frames,
};
pub use tail::tail;
pub use thread::thread;
pub use trim::trim;
pub use unfuse::unfuse;
pub use wellformed::{Malformed, Why, check};

/// The passes over a program in the scalar form, in the order they run:
/// the making of a loop of a function whose last act is a call of itself,
/// inlining, the taking away of what a function is passed and gives back and
/// does not need, the sending of a jump past a block
/// that only chooses, folding of constants, the replacing of what is worked out
/// twice, and the removal of what nothing uses. None of them makes a fused
/// instruction or takes one apart: `fuse` does that, once, to a form as it is
/// made.
pub fn passes(program: &Program) -> Program {
    remove(common(fold(thread(trim(inline(tail(program.clone())))))))
}

/// How many bits an integer has.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Bits {
    B8,
    B16,
    B32,
    B64,
}

impl Bits {
    pub const ALL: [Bits; 4] = [Bits::B8, Bits::B16, Bits::B32, Bits::B64];

    pub fn count(self) -> u32 {
        match self {
            Bits::B8 => 8,
            Bits::B16 => 16,
            Bits::B32 => 32,
            Bits::B64 => 64,
        }
    }
}

/// How many bits a float has.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FloatBits {
    F32,
    F64,
}

impl FloatBits {
    pub fn count(self) -> u32 {
        match self {
            FloatBits::F32 => 32,
            FloatBits::F64 => 64,
        }
    }
}

/// What each lane of a vector is: an integer or a float. A vector is 128
/// bits whole, so the lane says how many lanes it has.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Lane {
    Int(Bits),
    Float(FloatBits),
}

impl Lane {
    /// Every lane a vector has.
    pub const ALL: [Lane; 6] = [
        Lane::Int(Bits::B8),
        Lane::Int(Bits::B16),
        Lane::Int(Bits::B32),
        Lane::Int(Bits::B64),
        Lane::Float(FloatBits::F32),
        Lane::Float(FloatBits::F64),
    ];

    /// How many bits a lane has.
    pub fn bits(self) -> u32 {
        match self {
            Lane::Int(bits) => bits.count(),
            Lane::Float(bits) => bits.count(),
        }
    }

    /// How many lanes a vector of the lane has: 2, 4, 8, or 16.
    pub fn count(self) -> u32 {
        128 / self.bits()
    }

    /// The type of a lane read alone.
    pub fn scalar(self) -> Type {
        match self {
            Lane::Int(bits) => Type::Int(bits),
            Lane::Float(bits) => Type::Float(bits),
        }
    }

    /// The type of a mask over vectors of the lane: a vector of integers as
    /// wide as the lane.
    pub fn mask(self) -> Type {
        Type::Vector(Lane::Int(match self.bits() {
            8 => Bits::B8,
            16 => Bits::B16,
            32 => Bits::B32,
            _ => Bits::B64,
        }))
    }
}

/// What a value is, which also says its size.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Type {
    /// An integer. Whether it is signed belongs to the operation that
    /// reads it, not to the value.
    Int(Bits),
    /// A float of 32 or 64 bits, as IEEE 754 says.
    Float(FloatBits),
    /// A vector of a fixed number of one of those, 128 bits in all: lane 0 is
    /// the lowest bits, and a lane is as an integer or a float of its size is.
    Vector(Lane),
    /// An address in the program's memory.
    Address,
}

impl Type {
    /// How many bytes a value of the type takes in memory.
    pub fn bytes(self) -> u64 {
        match self {
            Type::Int(bits) => u64::from(bits.count() / 8),
            Type::Float(bits) => u64::from(bits.count() / 8),
            Type::Vector(_) => 16,
            Type::Address => 8,
        }
    }
}

/// Where each scalar of an element of a list is held, and how many bytes the
/// element takes: the scalars lie one after another, each at a multiple of
/// its size, and the element takes a whole number of its largest scalar.
pub fn element(scalars: &[Type]) -> (Vec<u64>, u64) {
    let mut end: u64 = 0;
    let places = scalars
        .iter()
        .map(|t| {
            let at = end.next_multiple_of(t.bytes());
            end = at + t.bytes();
            at
        })
        .collect();
    let largest = scalars.iter().map(|t| t.bytes()).max().unwrap_or(1);
    (places, end.next_multiple_of(largest))
}

/// A value, by its place in `Function::types`. It is defined once: as a
/// parameter of a block, or as a result of an instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Value(pub usize);

/// A standard stream of the program.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Stream {
    Stdout,
    Stderr,
}

/// A constant. Its type is the type of the value the instruction gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Constant {
    /// An integer, which is the number's bits, in the value's width.
    Int(u64),
    /// A float: the bits of the number, in the value's width.
    Float(u64),
    /// The address that holds nothing.
    Null,
    /// The address of the first byte of one of the program's texts, by its
    /// place in `Program::data`.
    Data(usize),
    /// A vector: the bits of the whole, with lane 0 in the lowest, as the low
    /// 64 bits and then the high 64, which keeps a constant as small as it was.
    Vector(u64, u64),
}

impl Constant {
    /// The constant that is a vector of the bits.
    pub fn vector(bits: u128) -> Constant {
        Constant::Vector(bits as u64, (bits >> 64) as u64)
    }

    /// The bits of a constant that is a vector.
    pub fn vector_bits(self) -> Option<u128> {
        match self {
            Constant::Vector(low, high) => Some(u128::from(low) | (u128::from(high) << 64)),
            _ => None,
        }
    }
}

/// A call into the system.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum System {
    /// Writes the bytes from an address, as many as a 64-bit integer says,
    /// to the stream. It takes the address and then the count, and gives
    /// nothing. A write the system refuses ends the program with a failing
    /// status.
    Write(Stream),
    /// Asks for as many bytes as a 64-bit integer says, and gives the
    /// address of the first. The bytes are the program's until it ends. A
    /// request the system refuses ends the program with a failing status.
    Allocate,
    /// Opens an arena: what is allocated until the `ArenaClose` that takes its
    /// token, and is not handed out of the block between them, is released at
    /// that close. It takes nothing and gives the token, a byte that stands
    /// for the place. It is made by the lowering and is gone once the stacks
    /// of `alloc` are passed.
    ArenaOpen,
    /// Closes the arena that the token it takes opened.
    ArenaClose,
    /// Notes in the frame of a task, at the address it takes, where the stack
    /// for results is, which is what an arena inside a task that waits puts
    /// back, since the token of an arena is no value that a poll may keep
    /// across a wait. It gives nothing, and is gone once the stacks of `alloc`
    /// are passed.
    TaskMark,
    /// Puts the stack for results back to where the `TaskMark` at the address
    /// it takes noted it.
    TaskRelease,
    /// Gives the frame of a task: it takes the number of bytes the frame has and
    /// gives its address, in a piece of the first class of the pool that the
    /// frame shares with the stack of results the task has of its own, which
    /// starts where the frame ends. The frame is where it is until the task is
    /// freed, whoever made it. It is gone once the stacks of `alloc` are passed.
    TaskFrame,
    /// Gives the pieces of the stack of the task whose frame is at the address it
    /// takes back to the pool, where the next task that is given a stack takes
    /// them, when the task is over and what it gave holds no address into them.
    /// It gives nothing, and is gone once the stacks of `alloc` are passed.
    TaskFree,
    /// Gives the pieces of the task whose frame is at the address it takes to the
    /// stack for results of the one that takes them, with no byte moved: a record of
    /// it is made at the top of the stack, which a put-back of the stack that passes
    /// below the record gives to the pool, with the pieces of the task. It gives
    /// nothing, and is gone once the stacks of `alloc` are passed.
    TaskMove,
    /// Gives the region of a handler's state: a frame of the same make as a
    /// task's, which holds a stack of its own that nothing else builds in. It
    /// takes nothing and gives the address of the frame. A program that starts no
    /// task has no pool to take it from, so there it gives a null address and
    /// every other operation of a region does nothing. It is gone once the
    /// stacks of `alloc` are passed.
    RegionNew,
    /// Gives the pieces of the region whose frame is at the address it takes back
    /// to the pool, where the handler's block has ended and nothing it gave holds
    /// an address into them. It gives nothing.
    RegionFree,
    /// Gives the pieces of the region whose frame is at the address it takes to
    /// the stack for results, with no byte moved, as `TaskMove` gives those of a
    /// task, where what the handler's block gave holds an address into them. It
    /// gives nothing.
    RegionMove,
    /// Says that the values after the first two arguments, the address of the frame
    /// of a region and how many handlers that have a region it is inside, are kept
    /// in that region: the state of a handler is assigned them. With none after,
    /// it says that the first is a name of the frame of that region. The
    /// memory that is, or is held with, an address among them is built in the
    /// region, wherever in the function it is asked for, as the memory of a value
    /// the function returns is built where its caller keeps it. It gives nothing.
    RegionKeep,
    /// Ends the program with the status, a byte, that it takes. It gives
    /// nothing.
    Exit,
    /// Fills the bytes from an address, as many as a 64-bit integer says,
    /// with bytes nobody can predict, which the system gives. It takes the
    /// address and then the count, and gives a byte: 0 when every byte is
    /// filled, which it asks the system again for until they are, and
    /// another when the system refused, with some bytes perhaps filled.
    Entropy,
    /// Reads at most as many bytes as a 64-bit integer says, which is not 0,
    /// from standard input into the bytes from an address. It takes the
    /// address and then the count, and gives a 64-bit integer: how many bytes
    /// it read, which is 0 only when standard input has ended. A read the
    /// system refuses ends the program with a failing status.
    Read,
    /// Asks the kernel for the call, which takes at most six words, each an
    /// address or a 64-bit integer, and gives a 64-bit integer: what the kernel
    /// gives, which is minus its error number when it refuses. The call is
    /// asked for once, so a caller that retries a call that was interrupted
    /// does it itself. Only the Linux and macOS targets build it, and on macOS
    /// it is the wrapper of `libSystem` that makes it.
    Raw(Raw),
    /// Waits for what the two 64-bit integers it takes say: the kind of the
    /// wait and the handle it is on, which the library gives their meaning. It
    /// takes the state of the scheduler after them, and gives that state back,
    /// as the state the task goes on with, which the poll that resumes it is
    /// given. A task that runs it suspends and is resumed after the
    /// wait is over, with every value it held. It is made by the lowering of a
    /// function that suspends and is gone once `suspend` has made that function
    /// a state machine, so no target builds it.
    Suspend,
    /// Whether the task that runs this function has been asked to end. It takes
    /// nothing and gives a 64-bit integer, which is 1 once the task has been
    /// asked to end and 0 until then. It is read from the frame of the function,
    /// into which the poll that runs the function puts what the task's own
    /// header says, so a function that a task suspends in sees it after each
    /// wait. It is made by the lowering where a call may suspend, and is gone
    /// once `suspend` has run: in a function that is not a state machine it is
    /// the constant 0, so no target builds it.
    Cancelled,
}

/// The calls of the kernel that a program asks for with `System::Raw`, each
/// named for the call as the kernel's documentation names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Raw {
    /// `socket(domain, type, protocol)`.
    Socket,
    /// `bind(socket, address, length)`.
    Bind,
    /// `listen(socket, backlog)`.
    Listen,
    /// `accept(socket, address, length)`.
    Accept,
    /// `connect(socket, address, length)`.
    Connect,
    /// `getsockname(socket, address, length)`.
    GetSockName,
    /// `setsockopt(socket, level, name, value, length)`.
    SetSockOpt,
    /// `sendto(socket, bytes, count, flags, address, length)`.
    SendTo,
    /// `recvfrom(socket, bytes, count, flags, address, length)`.
    RecvFrom,
    /// `close(descriptor)`.
    Close,
    /// `clock_gettime(clock, time)`.
    ClockGetTime,
    /// `nanosleep(time, left)`.
    NanoSleep,
    /// `epoll_create1(flags)`, which the scheduler of Linux waits with.
    EpollCreate1,
    /// `epoll_ctl(epoll, operation, descriptor, event)`.
    EpollCtl,
    /// `epoll_pwait(epoll, events, count, milliseconds, 0, 0)`, with no mask of
    /// signals, which is how `epoll_wait` is asked on every machine.
    EpollWait,
    /// `kqueue()`, which the scheduler of macOS waits with.
    Kqueue,
    /// `kevent(queue, changes, change count, events, event count, time)`, which
    /// applies the changes and then waits for an event, until the time, which is
    /// the address of a pair of numbers, or for ever when it is 0.
    Kevent,
    /// `clock_gettime(clock, time)`, as `ClockGetTime` is, which the scheduler
    /// of a group reads the time with. It is built into the scheduler for every
    /// machine, so it asks nothing of a machine that has no clock, where it
    /// gives minus 38 and is never run, as `EpollWait` does.
    Now,
    /// `openat(directory, path, flags, mode)`, with the path written as bytes
    /// and ended by a zero byte, which the library reads files of Linux with.
    OpenAt,
    /// `read(descriptor, bytes, count)`.
    Read,
    /// Not a call: which kernel the program is built for, as the numbers its
    /// calls and its errors are the kernel's own. It gives 0 on Linux and 1 on
    /// macOS.
    Kind,
}

/// What an instruction does with two integers of the same size, except a
/// shift, whose second is an integer of any size. Every one gives an integer
/// of the size of its first, in which the bits that do not fit are left off,
/// so no overflow is ever reported. Whether a number is signed is in the
/// operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Binary {
    /// `a + b`. It is also what moves an address: `a` is an address and `b`
    /// is a 64-bit integer, the number of bytes, and it gives an address.
    Add,
    /// `a - b`.
    Sub,
    /// The low half of `a * b`.
    Mul,
    /// The high half of `a * b`, with `a` and `b` as unsigned numbers.
    MulHighU,
    /// The high half of `a * b`, with `a` and `b` as signed numbers.
    MulHighS,
    /// `a / b`, with `a` and `b` as unsigned numbers, rounded toward zero. `b`
    /// is not zero: an instruction is given a divisor that is not, and what a
    /// program does that gives one is not defined, as the processors differ.
    DivU,
    /// `a / b`, with `a` and `b` as signed numbers, rounded toward zero. `b`
    /// is not zero, and `a` is not the least number of the size when `b` is
    /// -1, whose quotient the size cannot hold.
    DivS,
    /// What is left of `a` when it is divided by `b`, with both as unsigned
    /// numbers: `a - (a / b) * b`. `b` is not zero.
    RemU,
    /// What is left of `a` when it is divided by `b`, with both as signed
    /// numbers, which has the sign of `a`: `a - (a / b) * b`. `b` is not zero,
    /// and `a` is not the least number of the size when `b` is -1.
    RemS,
    /// The bits `a` and `b` both have.
    And,
    /// The bits that only one of `a` and `b` has.
    Xor,
    /// The bits that `a` or `b` has.
    Or,
    /// `a` with its bits moved up by `b`, the bits that leave the top
    /// dropped. `b` is taken modulo 64, so a program that means a shift is
    /// by fewer bits than the size of `a`.
    ShiftLeft,
    /// `a` with its bits moved down by `b`, and zeros filled in at the top.
    /// `b` is taken modulo 64.
    ShiftRightU,
    /// `a` with its bits moved down by `b`, and the sign of `a` filled in at
    /// the top. `b` is taken modulo 64.
    ShiftRightS,
}

/// What an instruction asks of two integers of the same size. It gives an
/// integer of 8 bits that is 1 if it is so and 0 if it is not.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Compare {
    /// Whether `a` and `b` are the same.
    Equal,
    /// Whether `a` is below `b`, as unsigned numbers.
    LessU,
    /// Whether `a` is below `b`, as signed numbers.
    LessS,
}

/// What an instruction does with two floats of the same size. Every one gives
/// a float of that size, rounded to the nearest, ties to even.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FloatBinary {
    Add,
    Sub,
    Mul,
    Div,
}

/// What a fused instruction works out of three floats of one size, `a`, `b`,
/// and `c`, exactly and then rounded once, to the nearest, ties to even.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Fused {
    /// `a * b + c`.
    MulAdd,
    /// `a * b - c`.
    MulSub,
    /// `c - a * b`.
    NegMulAdd,
}

impl Fused {
    pub const ALL: [Fused; 3] = [Fused::MulAdd, Fused::MulSub, Fused::NegMulAdd];
}

/// What an instruction asks of two floats of the same size. It gives an
/// integer of 8 bits that is 1 if it is so and 0 if it is not. A float that is
/// not a number is neither below, above, nor equal to any, so every one of
/// these but `NotEqual` is 0 when either is not a number, and `NotEqual` is 1.
/// `0.0` and `-0.0` are equal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FloatCompare {
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}

impl FloatCompare {
    pub const ALL: [FloatCompare; 6] = [
        FloatCompare::Equal,
        FloatCompare::NotEqual,
        FloatCompare::Less,
        FloatCompare::LessEqual,
        FloatCompare::Greater,
        FloatCompare::GreaterEqual,
    ];
}

/// What an instruction asks of each pair of lanes of two vectors of integers.
/// For `Equal` and `NotEqual` whether the lanes are signed makes no difference.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Relation {
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}

impl Relation {
    pub const ALL: [Relation; 6] = [
        Relation::Equal,
        Relation::NotEqual,
        Relation::Less,
        Relation::LessEqual,
        Relation::Greater,
        Relation::GreaterEqual,
    ];
}

/// What an instruction does.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Op {
    Constant(Constant),
    Binary {
        op: Binary,
        a: Value,
        b: Value,
    },
    Compare {
        op: Compare,
        a: Value,
        b: Value,
    },
    /// The sum, difference, or product of two integers of one size, `Add`,
    /// `Sub`, or `Mul`, read as signed or as unsigned numbers, and whether it
    /// did not fit the size: two results, the number with the bits that do not
    /// fit left off, which is what the `Binary` instruction gives, and an
    /// integer of 8 bits that is 1 when the number was not what the operation
    /// gives of the two numbers, and 0 when it was.
    Overflows {
        op: Binary,
        signed: bool,
        a: Value,
        b: Value,
    },
    /// `a + b`, `a - b`, `a * b`, or `a / b` of two floats of one size. A
    /// product is not fused with a sum here: `FloatFused` is the instruction
    /// that does it.
    FloatBinary {
        op: FloatBinary,
        a: Value,
        b: Value,
    },
    /// What `op` says of three floats of one size, worked out exactly and
    /// rounded once. `fuse` makes it, where the form is made, of a product whose
    /// only use is one sum or difference, and no pass makes or takes one apart.
    FloatFused {
        op: Fused,
        a: Value,
        b: Value,
        c: Value,
    },
    /// The float with its sign bit flipped, whatever it is, so that the
    /// negation of zero is zero of the other sign.
    FloatNegate(Value),
    /// The square root of a float, rounded to the nearest, ties to even, as
    /// IEEE 754 says: a float below 0 has none, and gives the one float that is
    /// not a number, and `-0.0` gives itself.
    FloatSqrt(Value),
    /// The bits of a float, as an integer of its size, which keeps every bit of
    /// it, a float that is not a number included.
    FloatBits(Value),
    /// The float whose bits are the integer, which has the size of the float,
    /// with every bit kept.
    FloatFromBits(Value),
    FloatCompare {
        op: FloatCompare,
        a: Value,
        b: Value,
    },
    /// The float nearest the integer, which is read as signed or as unsigned.
    FloatFromInt {
        signed: bool,
        value: Value,
    },
    /// The integer the float is cut to, which is read as signed or as
    /// unsigned: the float without its fraction when that fits, the least or
    /// the greatest integer when it does not, and 0 when it is not a number.
    IntFromFloat {
        signed: bool,
        value: Value,
    },
    /// The float of the size of the result that is nearest the float.
    FloatResize(Value),
    /// Each lane of two vectors of integers, `a` and `b`, by `Add`, `Sub`,
    /// `Mul`, `And`, `Or`, or `Xor`: the vector of what the instruction of that
    /// operation gives for each pair of lanes.
    VectorBinary {
        op: Binary,
        a: Value,
        b: Value,
    },
    /// Each lane of two vectors of floats, by `Add`, `Sub`, or `Mul`: what the
    /// instruction of that operation gives for each pair of lanes.
    VectorFloatBinary {
        op: FloatBinary,
        a: Value,
        b: Value,
    },
    /// Each lane of three vectors of floats, `a`, `b`, and `c`: what
    /// `FloatFused` gives for each triple of lanes.
    VectorFloatFused {
        op: Fused,
        a: Value,
        b: Value,
        c: Value,
    },
    /// A mask: for each pair of lanes of two vectors of integers, a lane of
    /// the same size that is all ones when the pair is as `op` says, read as
    /// signed or as unsigned numbers, and all zeros when it is not.
    VectorCompare {
        op: Relation,
        signed: bool,
        a: Value,
        b: Value,
    },
    /// A mask: for each pair of lanes of two vectors of floats, an integer lane
    /// as wide as the float that is all ones when the comparison holds, as
    /// `FloatCompare` says it, and all zeros when it does not.
    VectorFloatCompare {
        op: FloatCompare,
        a: Value,
        b: Value,
    },
    /// Takes each bit from `then` where the bit of the mask is 1, and from
    /// `otherwise` where it is 0. For a mask that a comparison gave, each lane
    /// is `then` or `otherwise` as the scalar choice on the lane of the mask.
    VectorChoose {
        mask: Value,
        then: Value,
        otherwise: Value,
    },
    /// The vector whose every lane is the integer or the float.
    VectorSplat(Value),
    /// The lane of the vector at the place, which is below the number of lanes.
    VectorExtract {
        vector: Value,
        lane: u8,
    },
    /// The vector with the lane at the place, which is below the number of
    /// lanes, replaced by the value.
    VectorInsert {
        vector: Value,
        lane: u8,
        value: Value,
    },
    /// The sum of the lanes. Of integers it is the sum with the bits that do not
    /// fit left off. Of floats it is added in pairs of lanes that lie
    /// next to each other, and then in pairs of those sums, so that the sum of
    /// lanes 0 to 3 is `(l0 + l1) + (l2 + l3)`, and each addition rounds as
    /// that of `FloatBinary` does.
    VectorSum(Value),
    /// The place of the first lane of the mask whose top bit is 1, as an integer
    /// of 64 bits, and the number of lanes when none has. A lane of a mask that
    /// a comparison gave is all ones or all zeros, so this is its first lane that
    /// is set.
    VectorFirstSet(Value),
    /// Gives an integer with more bits that stands for the same number,
    /// read as signed or as unsigned.
    Widen {
        signed: bool,
        value: Value,
    },
    /// Gives an integer with fewer bits: the low bits of the value.
    Narrow(Value),
    /// Gives `then` if the integer of 8 bits `on` is not 0, and `otherwise`
    /// if it is. Both are values of the type the instruction gives.
    Choose {
        on: Value,
        then: Value,
        otherwise: Value,
    },
    /// The address of `bytes` bytes of the function's frame, which are its own
    /// for as long as the function runs and are the same bytes each time the
    /// instruction runs in one run of the function. The bytes hold nothing
    /// until they are stored to, and no two of these instructions share any.
    Frame {
        bytes: u64,
    },
    /// Gives the integer or address of the type that is held at the address,
    /// in as many bytes as its size, least significant first.
    Load {
        ty: Type,
        address: Value,
    },
    /// Holds the integer or address of the type at the address, in as many
    /// bytes as its size, least significant first. It gives nothing.
    Store {
        ty: Type,
        address: Value,
        value: Value,
    },
    /// Runs a function, by its place in `Program::functions`, with these
    /// values as the parameters of its first block, in order. It gives the
    /// values the function returns, in order.
    Call {
        function: usize,
        args: Vec<Value>,
    },
    System {
        call: System,
        args: Vec<Value>,
    },
}

/// The values an instruction reads: up to three held in it, or the arguments
/// of a call, in order.
pub struct Operands<'a> {
    fixed: [Value; 3],
    at: usize,
    len: usize,
    args: std::slice::Iter<'a, Value>,
}

impl Iterator for Operands<'_> {
    type Item = Value;

    fn next(&mut self) -> Option<Value> {
        if self.at < self.len {
            self.at += 1;
            Some(self.fixed[self.at - 1])
        } else {
            self.args.next().copied()
        }
    }
}

impl Op {
    /// The values the instruction reads, in order, which it gives without
    /// asking for memory.
    pub fn operands(&self) -> Operands<'_> {
        let some = |fixed: [Value; 3], len: usize| Operands {
            fixed,
            at: 0,
            len,
            args: [].iter(),
        };
        match self {
            Op::Constant(_) | Op::Frame { .. } => some([Value(0); 3], 0),
            Op::Binary { a, b, .. }
            | Op::Compare { a, b, .. }
            | Op::Overflows { a, b, .. }
            | Op::FloatBinary { a, b, .. }
            | Op::FloatCompare { a, b, .. }
            | Op::VectorBinary { a, b, .. }
            | Op::VectorFloatBinary { a, b, .. }
            | Op::VectorCompare { a, b, .. }
            | Op::VectorFloatCompare { a, b, .. } => some([*a, *b, *a], 2),
            Op::FloatFused { a, b, c, .. } | Op::VectorFloatFused { a, b, c, .. } => {
                some([*a, *b, *c], 3)
            }
            Op::Widen { value, .. }
            | Op::Narrow(value)
            | Op::VectorSplat(value)
            | Op::VectorSum(value)
            | Op::VectorFirstSet(value)
            | Op::VectorExtract { vector: value, .. }
            | Op::FloatNegate(value)
            | Op::FloatSqrt(value)
            | Op::FloatBits(value)
            | Op::FloatFromBits(value)
            | Op::FloatFromInt { value, .. }
            | Op::IntFromFloat { value, .. }
            | Op::FloatResize(value) => some([*value; 3], 1),
            Op::Choose {
                on,
                then,
                otherwise,
            }
            | Op::VectorChoose {
                mask: on,
                then,
                otherwise,
            } => some([*on, *then, *otherwise], 3),
            Op::VectorInsert { vector, value, .. } => some([*vector, *value, *vector], 2),
            Op::Load { address, .. } => some([*address; 3], 1),
            Op::Store { address, value, .. } => some([*address, *value, *address], 2),
            Op::Call { args, .. } | Op::System { args, .. } => Operands {
                fixed: [Value(0); 3],
                at: 0,
                len: 0,
                args: args.iter(),
            },
        }
    }

    /// Puts the values the instruction reads, in order, in `values`, which it
    /// empties first.
    pub fn operands_into(&self, values: &mut Vec<Value>) {
        values.clear();
        values.extend(self.operands());
    }

    /// Gives each value the instruction reads the value `by` gives for it, in
    /// the instruction, with no copy of it.
    pub fn rename(&mut self, by: &impl Fn(Value) -> Value) {
        let one = |v: &mut Value| *v = by(*v);
        match self {
            Op::Constant(_) | Op::Frame { .. } => {}
            Op::Binary { a, b, .. }
            | Op::Compare { a, b, .. }
            | Op::Overflows { a, b, .. }
            | Op::FloatBinary { a, b, .. }
            | Op::FloatCompare { a, b, .. }
            | Op::VectorBinary { a, b, .. }
            | Op::VectorFloatBinary { a, b, .. }
            | Op::VectorCompare { a, b, .. }
            | Op::VectorFloatCompare { a, b, .. } => {
                one(a);
                one(b);
            }
            Op::FloatFused { a, b, c, .. } | Op::VectorFloatFused { a, b, c, .. } => {
                one(a);
                one(b);
                one(c);
            }
            Op::Widen { value, .. }
            | Op::Narrow(value)
            | Op::VectorSplat(value)
            | Op::VectorSum(value)
            | Op::VectorFirstSet(value)
            | Op::VectorExtract { vector: value, .. }
            | Op::FloatNegate(value)
            | Op::FloatSqrt(value)
            | Op::FloatBits(value)
            | Op::FloatFromBits(value)
            | Op::FloatFromInt { value, .. }
            | Op::IntFromFloat { value, .. }
            | Op::FloatResize(value) => one(value),
            Op::Choose {
                on,
                then,
                otherwise,
            }
            | Op::VectorChoose {
                mask: on,
                then,
                otherwise,
            } => {
                one(on);
                one(then);
                one(otherwise);
            }
            Op::VectorInsert { vector, value, .. } => {
                one(vector);
                one(value);
            }
            Op::Load { address, .. } => one(address),
            Op::Store { address, value, .. } => {
                one(address);
                one(value);
            }
            Op::Call { args, .. } | Op::System { args, .. } => args.iter_mut().for_each(one),
        }
    }

    /// Whether the instruction runs a function or asks the system for
    /// something, which may change any register that is not kept for it.
    pub fn is_call(&self) -> bool {
        matches!(self, Op::Call { .. } | Op::System { .. })
    }
}

/// An instruction: what it does, the values it defines, and the place in the
/// source it was built from, which the map of an executable holds. Two
/// instructions are equal when they do the same, wherever they were written.
#[derive(Clone, Debug)]
pub struct Instruction {
    pub results: Vec<Value>,
    pub op: Op,
    pub at: Place,
}

impl PartialEq for Instruction {
    fn eq(&self, other: &Instruction) -> bool {
        self.results == other.results && self.op == other.op
    }
}

impl Eq for Instruction {}

/// A place in the source: the file and the function it is written in, each as the
/// number `names::intern` gives its name, and a line and a column of the file
/// that count from 1. A line of 0 says no line of the source wrote the code:
/// the code of the runtime and of the entry of the program has none, and holds
/// the name of the piece it is, in angle brackets, as its function. A place that
/// is all zeros says nothing of where the code comes from, as for the constants a
/// pass makes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Place {
    pub file: u32,
    pub function: u32,
    pub line: u32,
    pub col: u32,
}

impl Place {
    /// No place.
    pub const NONE: Place = Place {
        file: 0,
        function: 0,
        line: 0,
        col: 0,
    };

    /// Whether a line of the source wrote the code.
    pub fn is_source(self) -> bool {
        self.line > 0
    }

    /// Whether it says nothing of where the code comes from.
    pub fn is_none(self) -> bool {
        self.line == 0 && self.function == 0
    }

    /// The place of the code of the piece of the runtime, or the function, with
    /// the name, which no line of the source wrote.
    pub fn of(name: &str) -> Place {
        Place {
            function: names::intern(name),
            ..Place::NONE
        }
    }
}

/// The names a place holds, which are kept once for the process: a name is the
/// number it is given, and the same name is always the same number.
pub mod names {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    /// The names in the order they were given, and the number of each.
    struct Names {
        list: Vec<&'static str>,
        number: HashMap<&'static str, u32>,
    }

    fn names() -> &'static Mutex<Names> {
        static NAMES: OnceLock<Mutex<Names>> = OnceLock::new();
        NAMES.get_or_init(|| {
            Mutex::new(Names {
                list: vec![""],
                number: HashMap::from([("", 0)]),
            })
        })
    }

    /// The number of the name. The names grow with the program, a file and a
    /// function each, so a name is found by a lookup and not by a scan. The
    /// name that is nothing has the number 0, which is the number of no name.
    pub fn intern(name: &str) -> u32 {
        let mut names = names().lock().expect("the names are not left held");
        if let Some(&at) = names.number.get(name) {
            return at;
        }
        let kept: &'static str = Box::leak(name.to_string().into_boxed_str());
        let at = names.list.len() as u32;
        names.list.push(kept);
        names.number.insert(kept, at);
        at
    }

    /// The name with the number.
    pub fn name(number: u32) -> &'static str {
        match number {
            0 => "",
            n => names().lock().expect("the names are not left held").list[n as usize],
        }
    }
}

/// A jump to a block, giving it a value for each of its parameters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Jump {
    pub block: usize,
    pub args: Vec<Value>,
}

/// How a block ends.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum End {
    Jump(Jump),
    /// Makes the jump that is at the place of the integer `on`, which is
    /// below the number of jumps.
    Switch {
        on: Value,
        jumps: Vec<Jump>,
    },
    Return(Vec<Value>),
}

impl End {
    /// The values the end reads, in order, which it gives without asking for
    /// memory.
    pub fn operands(&self) -> impl Iterator<Item = Value> + '_ {
        let (on, returned): (Option<Value>, &[Value]) = match self {
            End::Switch { on, .. } => (Some(*on), &[]),
            End::Return(values) => (None, values),
            End::Jump(_) => (None, &[]),
        };
        on.into_iter()
            .chain(self.jumps().iter().flat_map(|j| j.args.iter().copied()))
            .chain(returned.iter().copied())
    }

    /// Puts the values the end reads, in order, in `values`, which it empties
    /// first.
    pub fn operands_into(&self, values: &mut Vec<Value>) {
        values.clear();
        values.extend(self.operands());
    }

    /// The jumps the end makes, in order.
    pub fn jumps(&self) -> &[Jump] {
        match self {
            End::Jump(jump) => std::slice::from_ref(jump),
            End::Switch { jumps, .. } => jumps,
            End::Return(_) => &[],
        }
    }

    /// The jumps the end makes, in order, to change.
    pub fn jumps_mut(&mut self) -> &mut [Jump] {
        match self {
            End::Jump(jump) => std::slice::from_mut(jump),
            End::Switch { jumps, .. } => jumps,
            End::Return(_) => &mut [],
        }
    }

    /// The blocks the end may go to, in order.
    pub fn successors(&self) -> impl Iterator<Item = usize> + '_ {
        self.jumps().iter().map(|j| j.block)
    }
}

/// A block: it is entered with its parameters, runs its instructions in
/// order, and ends.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub params: Vec<Value>,
    pub instructions: Vec<Instruction>,
    pub end: End,
}

/// A function. Its first block is where it starts, and the parameters of
/// that block are the function's own. Two functions are equal when they do the
/// same: the name is for the map of an executable and is not compared.
#[derive(Clone, Debug)]
pub struct Function {
    /// The type of each value, by its place.
    pub types: Vec<Type>,
    /// The types of the values it returns, in order.
    pub results: Vec<Type>,
    pub blocks: Vec<Block>,
    /// Whether the function is run rarely, so that a call of it is never
    /// replaced by its instructions, however many places call it. Its
    /// instructions then stay out of the code of a loop that calls it, and
    /// so do the values it needs.
    pub cold: bool,
    /// What the map of an executable calls the function: its module and its
    /// name as written, with the types it is made for in brackets after them,
    /// or a name in angle brackets for what no function was written for. It is
    /// empty for a function that has none, which the map calls `<function N>`.
    pub name: String,
}

impl PartialEq for Function {
    fn eq(&self, other: &Function) -> bool {
        self.types == other.types
            && self.results == other.results
            && self.blocks == other.blocks
            && self.cold == other.cold
    }
}

impl Eq for Function {}

impl Function {
    /// The function, called `name` in the map of an executable. The code of it that
    /// has no place is the code of the piece the name says.
    pub fn named(mut self, name: impl Into<String>) -> Function {
        self.name = name.into();
        let piece = Place::of(&self.name);
        for i in self.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
            if i.at.is_none() {
                i.at = piece;
            }
        }
        self
    }
}

/// A program: it runs the function `main`, which gives back nothing, and then
/// ends with status 0. `main` takes no parameters, or it takes the three that
/// `ARGUMENTS` lists, which the system gives it as `runtime::Runtime::Arguments`
/// says. No function calls itself, directly or through others.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Program {
    /// The bytes of the program's texts, which `Constant::Data` addresses.
    pub data: Vec<Vec<u8>>,
    pub functions: Vec<Function>,
    pub main: usize,
}

/// The types of the parameters of a `main` that takes the words the program was
/// started with: a number of words, the address of their addresses, and the
/// address of a command line.
pub const ARGUMENTS: [Type; 3] = [Type::Int(Bits::B64), Type::Address, Type::Address];

impl Program {
    /// Whether `main` takes what the system gives a program that is started
    /// with words, which a target must then give it.
    pub fn takes_arguments(&self) -> bool {
        self.functions[self.main].blocks[0].params.len() == ARGUMENTS.len()
    }

    /// Whether the program asks the system for random bytes.
    pub fn asks_for_entropy(&self) -> bool {
        let instructions = self.functions.iter().flat_map(|f| &f.blocks);
        instructions.flat_map(|b| &b.instructions).any(|i| {
            matches!(
                i.op,
                Op::System {
                    call: System::Entropy,
                    ..
                }
            )
        })
    }

    /// Whether the program reads standard input.
    pub fn reads(&self) -> bool {
        let instructions = self.functions.iter().flat_map(|f| &f.blocks);
        instructions.flat_map(|b| &b.instructions).any(|i| {
            matches!(
                i.op,
                Op::System {
                    call: System::Read,
                    ..
                }
            )
        })
    }

    /// Whether the program asks the kernel for a call of its own, as a program
    /// that uses the network or the clock does.
    pub fn asks_the_kernel(&self) -> bool {
        self.asks_the_network() || self.asks_the_clock()
    }

    /// Whether the program asks the kernel for a call of the network, which
    /// is a program that carries `net`.
    pub fn asks_the_network(&self) -> bool {
        let instructions = self.functions.iter().flat_map(|f| &f.blocks);
        instructions.flat_map(|b| &b.instructions).any(|i| {
            matches!(
                i.op,
                Op::System {
                    call: System::Raw(
                        Raw::Socket
                            | Raw::Bind
                            | Raw::Listen
                            | Raw::Accept
                            | Raw::Connect
                            | Raw::GetSockName
                            | Raw::SetSockOpt
                            | Raw::SendTo
                            | Raw::RecvFrom
                            | Raw::Close
                            | Raw::OpenAt
                            | Raw::Read
                    ),
                    ..
                }
            )
        })
    }

    /// Whether the program asks the kernel for the clock, which is a program
    /// that carries `time`.
    pub fn asks_the_clock(&self) -> bool {
        let instructions = self.functions.iter().flat_map(|f| &f.blocks);
        instructions.flat_map(|b| &b.instructions).any(|i| {
            matches!(
                i.op,
                Op::System {
                    call: System::Raw(Raw::ClockGetTime | Raw::NanoSleep),
                    ..
                }
            )
        })
    }

    /// Whether the program waits for the system in `kqueue`, which only macOS
    /// has, as the scheduler of a group does where it is built for every machine.
    /// It asks nothing of a machine that has none, so a target that has no network
    /// still builds it, and a macOS executable imports the wrappers when it does.
    pub fn waits_in_kqueue(&self) -> bool {
        let instructions = self.functions.iter().flat_map(|f| &f.blocks);
        instructions.flat_map(|b| &b.instructions).any(|i| {
            matches!(
                i.op,
                Op::System {
                    call: System::Raw(Raw::Kqueue | Raw::Kevent | Raw::Now),
                    ..
                }
            )
        })
    }

    /// Whether the program asks the system for memory.
    pub fn allocates(&self) -> bool {
        let instructions = self.functions.iter().flat_map(|f| &f.blocks);
        instructions.flat_map(|b| &b.instructions).any(|i| {
            matches!(
                i.op,
                Op::System {
                    call: System::Allocate,
                    ..
                }
            )
        })
    }
}
