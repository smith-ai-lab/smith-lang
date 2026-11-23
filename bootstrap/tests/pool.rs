//! The pool of pieces that tasks take their memory from, `scalar::pool`, which
//! the code of a program that starts a task has: a piece given back is the next
//! of its class that is taken, a request is given the smallest class that holds
//! it and its header, pieces are carved from chunks so that the system is asked
//! for a chunk and not for each piece, and a request no class holds is asked of
//! the system as it is. The functions are run in the evaluator in programs that
//! call them, which count what the system is asked for.

use smith::scalar::{self, Binary, Bits, Builder, Compare, Function, Program, Type, Value, pool};

use crate::Test;

pub const ONCE: &[Test] = &[
    (
        "a_piece_given_back_is_the_next_taken_of_its_class",
        a_piece_given_back_is_the_next_taken_of_its_class,
    ),
    (
        "a_request_is_given_the_smallest_class_that_holds_it_and_its_header",
        a_request_is_given_the_smallest_class_that_holds_it_and_its_header,
    ),
    (
        "pieces_are_carved_from_chunks_and_the_system_is_asked_for_a_chunk_not_a_piece",
        pieces_are_carved_from_chunks_and_the_system_is_asked_for_a_chunk_not_a_piece,
    ),
    (
        "pieces_given_back_are_taken_again_without_asking_the_system",
        pieces_given_back_are_taken_again_without_asking_the_system,
    ),
    (
        "a_request_no_class_holds_is_asked_of_the_system_as_it_is",
        a_request_no_class_holds_is_asked_of_the_system_as_it_is,
    ),
    (
        "the_pieces_taken_at_once_do_not_overlap",
        the_pieces_taken_at_once_do_not_overlap,
    ),
];

const WORD: Type = Type::Int(Bits::B64);

/// A program whose functions are `new`, `take`, `give`, and the main that
/// `build` writes, which is given the means to call them.
fn program(build: impl FnOnce(&mut Builder, &mut Calls)) -> Program {
    let mut b = Builder::new();
    let mut calls = Calls;
    build(&mut b, &mut calls);
    let main: Function = b.finish(&[]);
    let program = Program {
        data: Vec::new(),
        functions: vec![pool::new(), pool::take_sized(), pool::give(), main],
        main: 3,
    };
    assert_eq!(scalar::check(&program), Ok(()));
    program
}

struct Calls;

impl Calls {
    fn make(&self, b: &mut Builder) -> Value {
        b.call(0, vec![], &[Type::Address])[0]
    }

    fn take(&self, b: &mut Builder, pool: Value, bytes: u64) -> Value {
        let bytes = b.int(Bits::B64, bytes);
        b.call(1, vec![pool, bytes], &[Type::Address, WORD])[0]
    }

    fn give(&self, b: &mut Builder, pool: Value, address: Value) {
        b.call(2, vec![pool, address], &[]);
    }

    /// The address as a number, which is kept in a place of its own.
    fn number(&self, b: &mut Builder, address: Value) -> Value {
        let eight = b.int(Bits::B64, 8);
        let place = b.allocate(eight);
        b.store(place, address);
        b.load(WORD, place)
    }
}

fn a_piece_given_back_is_the_next_taken_of_its_class() {
    let program = program(|b, c| {
        let pool = c.make(b);
        let first = c.take(b, pool, 100);
        let second = c.take(b, pool, 100);
        c.give(b, pool, first);
        let again = c.take(b, pool, 100);
        let (first, second, again) = (c.number(b, first), c.number(b, second), c.number(b, again));
        let same = b.compare(Compare::Equal, first, again);
        let other = b.compare(Compare::Equal, first, second);
        // 1 when the piece given back is taken, and 2 more when two pieces
        // that are in use are the same.
        let two = b.int(Bits::B8, 2);
        let twice = b.binary(Binary::Mul, other, two);
        let status = b.binary(Binary::Add, same, twice);
        b.exit(status);
        b.ret(vec![]);
    });
    assert_eq!(scalar::run(&program).status, 1);
}

fn a_request_is_given_the_smallest_class_that_holds_it_and_its_header() {
    let apart = |bytes: u64| {
        let program = program(|b, c| {
            let pool = c.make(b);
            let first = c.take(b, pool, bytes);
            let second = c.take(b, pool, bytes);
            let (first, second) = (c.number(b, first), c.number(b, second));
            let gap = b.binary(Binary::Sub, second, first);
            let unit = b.int(Bits::B64, pool::FIRST);
            let classes = b.binary(Binary::DivU, gap, unit);
            let status = b.narrow(classes, Bits::B8);
            b.exit(status);
            b.ret(vec![]);
        });
        scalar::run(&program).status
    };
    // 64 bytes hold 48 and a header, 128 hold 112, and so on.
    assert_eq!(apart(1), 1);
    assert_eq!(apart(pool::FIRST - pool::HEADER), 1);
    assert_eq!(apart(pool::FIRST - pool::HEADER + 1), 2);
    assert_eq!(apart(2 * pool::FIRST - pool::HEADER), 2);
    assert_eq!(apart(2 * pool::FIRST - pool::HEADER + 1), 4);
    assert_eq!(apart(8 * pool::FIRST), 16);
}

/// The program takes `count` pieces of `bytes`, gives them all back, and takes
/// as many again.
fn churn(count: u64, bytes: u64, again: bool) -> Program {
    program(|b, c| {
        let pool = c.make(b);
        let table_size = b.int(Bits::B64, 8 * count);
        let table = b.allocate(table_size);
        let eight = b.int(Bits::B64, 8);
        let mut at = table;
        for _ in 0..count {
            let piece = c.take(b, pool, bytes);
            b.store(at, piece);
            at = b.binary(Binary::Add, at, eight);
        }
        if again {
            let mut at = table;
            for _ in 0..count {
                let piece = b.load(Type::Address, at);
                c.give(b, pool, piece);
                at = b.binary(Binary::Add, at, eight);
            }
            for _ in 0..count {
                c.take(b, pool, bytes);
            }
        }
        b.ret(vec![]);
    })
}

fn pieces_are_carved_from_chunks_and_the_system_is_asked_for_a_chunk_not_a_piece() {
    // 60 pieces of 4096 are 245,760 bytes, which a chunk of 262,144 holds.
    let (_, asked) = scalar::run_counting(&churn(60, 100, false), usize::MAX);
    assert_eq!(
        asked.len(),
        3,
        "the pool, the table, and one chunk: {asked:?}"
    );
    assert!(asked.contains(&(pool::CHUNK as usize)));
    // 130 of 8192 are 1,064,960 bytes, which five chunks of 32 pieces hold.
    let (_, asked) = scalar::run_counting(&churn(130, 5000, false), usize::MAX);
    let chunks = asked.iter().filter(|a| **a == pool::CHUNK as usize).count();
    assert_eq!(chunks, 5, "{asked:?}");
}

fn pieces_given_back_are_taken_again_without_asking_the_system() {
    let (_, once) = scalar::run_counting(&churn(60, 100, false), usize::MAX);
    let (_, twice) = scalar::run_counting(&churn(60, 100, true), usize::MAX);
    assert_eq!(once, twice);
}

fn a_request_no_class_holds_is_asked_of_the_system_as_it_is() {
    let bytes = 5_000_000u64;
    let program = program(|b, c| {
        let pool = c.make(b);
        let piece = c.take(b, pool, bytes);
        c.give(b, pool, piece);
        b.ret(vec![]);
    });
    let (outcome, asked) = scalar::run_counting(&program, usize::MAX);
    assert_eq!(outcome.status, 0);
    assert!(asked.contains(&((bytes + pool::HEADER) as usize)), "{asked:?}");
}

/// Pieces that are in use at once do not overlap: a number is written at the
/// start and at the end of what each holds, for each of several sizes, and every
/// number is read back as it was written.
fn the_pieces_taken_at_once_do_not_overlap() {
    let sizes = [100u64, 5000, 100, 70_000, 4000, 9000, 100, 300_000, 20, 8100];
    let program = program(|b, c| {
        let pool = c.make(b);
        let mut pieces = Vec::new();
        for (k, bytes) in sizes.iter().enumerate() {
            let piece = c.take(b, pool, *bytes);
            let last = b.int(Bits::B64, bytes - 8);
            let end = b.binary(Binary::Add, piece, last);
            let value = b.int(Bits::B64, 1000 + k as u64);
            b.store(piece, value);
            b.store(end, value);
            pieces.push((piece, end, 1000 + k as u64));
        }
        let mut wrong = b.int(Bits::B8, 0);
        for (piece, end, value) in pieces {
            for at in [piece, end] {
                let read = b.load(WORD, at);
                let wanted = b.int(Bits::B64, value);
                let same = b.compare(Compare::Equal, read, wanted);
                let one = b.int(Bits::B8, 1);
                let differs = b.binary(Binary::Xor, same, one);
                wrong = b.binary(Binary::Or, wrong, differs);
            }
        }
        b.exit(wrong);
        b.ret(vec![]);
    });
    assert_eq!(scalar::run(&program).status, 0);
}
