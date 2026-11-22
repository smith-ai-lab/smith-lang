//! The pool of pieces that tasks take their memory from, as the functions that
//! a program that starts a task has: the pool is a block of memory that its
//! address stands for, passed as a value and never kept anywhere else, so a
//! program has the pool of the code that runs it and no other. It holds a piece
//! at a time for each size, in classes that start at 4 KB and double up to 2 MB,
//! carved from chunks of 256 KB that the system gives, so that a task that holds
//! little holds a few KB, and a task that holds more holds at most twice what it
//! uses and a header of 16 bytes. A piece that is given back is the next that
//! is taken of its class. A request that no class holds is asked of the system
//! as it is, and is not kept when it is given back.
//!
//! The pool is a block of words: where the chunk being carved is, how many bytes
//! of it are left, and the first free piece of each class. A piece has a header
//! of two words, its class and, while it is free, the next free piece of its
//! class, and the memory a task is given follows it.

use super::{Binary, Bits, Builder, Compare, Function, Type, runtime};

const NEXT: Type = Type::Address;
const LEFT: Type = Type::Int(Bits::B64);

/// How many classes of pieces there are.
pub const CLASSES: u64 = 10;
/// How many bytes a piece of the first class has, header included.
pub const FIRST: u64 = 4096;
/// The bytes a piece spends on its header.
pub const HEADER: u64 = 16;
/// How many bytes the system is asked for at a time, at least.
pub const CHUNK: u64 = 256 * 1024;
/// The bytes of the block that is the pool.
pub const BLOCK: u64 = 16 + 8 * CLASSES;

/// The function that makes a pool: it takes nothing and gives the address of the
/// pool, which has no piece.
pub fn new() -> Function {
    let mut f = Builder::new();
    let bytes = runtime::int(&mut f, BLOCK);
    let pool = f.allocate(bytes);
    let zero = runtime::int(&mut f, 0);
    let eight = runtime::int(&mut f, 8);
    let mut at = pool;
    for _ in 0..BLOCK / 8 {
        f.store(at, zero);
        at = f.binary(Binary::Add, at, eight);
    }
    f.ret(vec![pool]);
    f.finish(&[NEXT])
}

/// The function that takes a pool and a number of bytes and gives the address of
/// that many bytes, which are in a piece of the smallest class that holds them
/// and their header, from the pieces that were given back when there is one,
/// and from the chunk being carved when there is not.
pub fn take() -> Function {
    let mut f = Builder::new();
    let p = f.params(&[NEXT, LEFT]);
    let (pool, bytes) = (p[0], p[1]);
    let (search, s) = f.block(&[LEFT, LEFT]);
    let (found, c) = f.block(&[LEFT, LEFT]);
    let (more, m) = f.block(&[LEFT, LEFT]);
    let (reuse, r) = f.block(&[NEXT, NEXT]);
    let (carve, k) = f.block(&[LEFT, LEFT]);
    let (refill, i) = f.block(&[LEFT, LEFT]);
    let (have, h) = f.block(&[LEFT, LEFT]);
    let (large, _) = f.block(&[]);
    let zero = runtime::int(&mut f, 0);
    let one = runtime::int(&mut f, 1);
    let eight = runtime::int(&mut f, 8);
    let header = runtime::int(&mut f, HEADER);
    let total = f.binary(Binary::Add, bytes, header);
    let first = runtime::int(&mut f, FIRST);
    f.jump(search, vec![zero, first]);
    // The smallest class whose piece holds the request and its header.
    f.at(search);
    let (class, size) = (s[0], s[1]);
    let short = f.compare(Compare::LessU, size, total);
    f.switch(
        short,
        vec![(found, vec![class, size]), (more, vec![class, size])],
    );
    f.at(more);
    let last = runtime::int(&mut f, CLASSES - 1);
    let none_left = f.compare(Compare::Equal, m[0], last);
    let next_class = f.binary(Binary::Add, m[0], one);
    let doubled = f.binary(Binary::Add, m[1], m[1]);
    f.switch(
        none_left,
        vec![(search, vec![next_class, doubled]), (large, vec![])],
    );
    // A class that has a piece given back gives it.
    f.at(found);
    let (class, size) = (c[0], c[1]);
    let sixteen = runtime::int(&mut f, 16);
    let by = f.binary(Binary::Mul, class, eight);
    let skip = f.binary(Binary::Add, by, sixteen);
    let slot = f.binary(Binary::Add, pool, skip);
    let head = f.load(NEXT, slot);
    let head_word = f.load(LEFT, slot);
    let empty = f.compare(Compare::Equal, head_word, zero);
    f.switch(
        empty,
        vec![(reuse, vec![head, slot]), (carve, vec![class, size])],
    );
    f.at(reuse);
    let (piece, slot) = (r[0], r[1]);
    let after_at = f.binary(Binary::Add, piece, eight);
    let after = f.load(NEXT, after_at);
    f.store(slot, after);
    let given = f.binary(Binary::Add, piece, header);
    f.ret(vec![given]);
    // The chunk being carved gives the piece when it has the room, and a new
    // chunk is asked of the system when it has not, with room for the piece.
    f.at(carve);
    let (class, size) = (k[0], k[1]);
    let left_at = f.binary(Binary::Add, pool, eight);
    let left = f.load(LEFT, left_at);
    let lacks = f.compare(Compare::LessU, left, size);
    f.switch(
        lacks,
        vec![(have, vec![class, size]), (refill, vec![class, size])],
    );
    f.at(refill);
    let (class, size) = (i[0], i[1]);
    let chunk_size = runtime::int(&mut f, CHUNK);
    let big = f.compare(Compare::LessU, chunk_size, size);
    let ask = f.choose(big, size, chunk_size);
    let chunk = f.allocate(ask);
    let left_at = f.binary(Binary::Add, pool, eight);
    f.store(pool, chunk);
    f.store(left_at, ask);
    f.jump(have, vec![class, size]);
    f.at(have);
    let (class, size) = (h[0], h[1]);
    let left_at = f.binary(Binary::Add, pool, eight);
    let piece = f.load(NEXT, pool);
    let left = f.load(LEFT, left_at);
    let past = f.binary(Binary::Add, piece, size);
    let rest = f.binary(Binary::Sub, left, size);
    f.store(pool, past);
    f.store(left_at, rest);
    f.store(piece, class);
    let given = f.binary(Binary::Add, piece, header);
    f.ret(vec![given]);
    // A request no class holds is asked of the system as it is.
    f.at(large);
    let piece = f.allocate(total);
    let marker = runtime::int(&mut f, CLASSES);
    f.store(piece, marker);
    let given = f.binary(Binary::Add, piece, header);
    f.ret(vec![given]);
    f.finish(&[NEXT])
}

/// The function that takes a pool and an address that `take` gave, and puts the
/// piece it is in among the free pieces of its class, so that the next request
/// of the class is given it. What the request of no class gave is not kept.
pub fn give() -> Function {
    let mut f = Builder::new();
    let p = f.params(&[NEXT, NEXT]);
    let (pool, given) = (p[0], p[1]);
    let (keep, _) = f.block(&[]);
    let (drop, _) = f.block(&[]);
    let zero = runtime::int(&mut f, 0);
    let eight = runtime::int(&mut f, 8);
    let header = runtime::int(&mut f, HEADER);
    let back = f.binary(Binary::Sub, zero, header);
    let piece = f.binary(Binary::Add, given, back);
    let class = f.load(LEFT, piece);
    let large = runtime::int(&mut f, CLASSES);
    let is_large = f.compare(Compare::Equal, class, large);
    f.switch(is_large, vec![(keep, vec![]), (drop, vec![])]);
    f.at(keep);
    let sixteen = runtime::int(&mut f, 16);
    let by = f.binary(Binary::Mul, class, eight);
    let skip = f.binary(Binary::Add, by, sixteen);
    let slot = f.binary(Binary::Add, pool, skip);
    let head = f.load(NEXT, slot);
    let next_at = f.binary(Binary::Add, piece, eight);
    f.store(next_at, head);
    f.store(slot, piece);
    f.ret(Vec::new());
    f.at(drop);
    f.ret(Vec::new());
    f.finish(&[])
}
