//! What a program needs to work on a range of more than 64 bits, which is two
//! words, the low one first: dividing one by another, and writing one in
//! decimal. They are written once in the scalar form, as the rest of the
//! runtime is, and a program holds only those it reaches.

use super::runtime::{BYTE, TEXT};
use super::{Binary, Bits, Builder, Compare, Function, Type, Value};

const WORD: Type = Type::Int(Bits::B64);

fn word(f: &mut Builder, n: u64) -> Value {
    f.int(Bits::B64, n)
}

/// `a` shifted left by one place, with `carry`, a word that is 0 or 1, as its
/// lowest bit, for the two words `(low, high)`, and the bit that went out of
/// the top.
fn shifted(f: &mut Builder, (low, high): (Value, Value), carry: Value) -> ((Value, Value), Value) {
    let one = word(f, 1);
    let top = word(f, 63);
    let out = f.binary(Binary::ShiftRightU, high, top);
    let high_moved = f.binary(Binary::ShiftLeft, high, one);
    let across = f.binary(Binary::ShiftRightU, low, top);
    let high_new = f.binary(Binary::Or, high_moved, across);
    let low_moved = f.binary(Binary::ShiftLeft, low, one);
    let low_new = f.binary(Binary::Or, low_moved, carry);
    ((low_new, high_new), out)
}

/// Whether `(a_low, a_high)` is below `(b_low, b_high)`, as numbers that are not
/// negative, as a byte.
fn below(
    f: &mut Builder,
    (a_low, a_high): (Value, Value),
    (b_low, b_high): (Value, Value),
) -> Value {
    let high_below = f.compare(Compare::LessU, a_high, b_high);
    let high_same = f.compare(Compare::Equal, a_high, b_high);
    let low_below = f.compare(Compare::LessU, a_low, b_low);
    let decided = f.binary(Binary::And, high_same, low_below);
    f.binary(Binary::Or, high_below, decided)
}

/// Takes two numbers of two words, `a` and `b`, which are not negative and `b`
/// is not 0, as the words `a_low`, `a_high`, `b_low`, `b_high`, and gives the
/// words of `a / b` and then those of `a % b`, each the low word first. It
/// takes the bits of `a` from the top one at a time, keeps what is left of
/// the bits taken so far, and takes `b` away from it each time it can: 128
/// rounds, each of a few instructions, which is slow beside every other
/// operation, and is the same on every target.
pub(super) fn divide() -> Function {
    let mut f = Builder::new();
    let p = f.params(&[WORD, WORD, WORD, WORD]);
    let b = (p[2], p[3]);
    let zero = word(&mut f, 0);
    let count = word(&mut f, 128);
    // The bits of `a` not yet taken, what is left over, the quotient so far, and
    // how many rounds are left.
    let (round, held) = f.block(&[WORD, WORD, WORD, WORD, WORD, WORD, WORD]);
    let (finish, done) = f.block(&[WORD, WORD, WORD, WORD]);
    f.jump(round, vec![p[0], p[1], zero, zero, zero, zero, count]);
    f.at(round);
    let (a, r, q, left) = (
        (held[0], held[1]),
        (held[2], held[3]),
        (held[4], held[5]),
        held[6],
    );
    // The top bit of what is left of `a` joins the lowest of the remainder.
    let (a_next, bit) = shifted(&mut f, a, zero);
    let (r_next, lost) = shifted(&mut f, r, bit);
    // `b` goes in when the remainder is not below it, which it is not when a bit
    // went out of the top, as `b` has no such bit.
    let under = below(&mut f, r_next, b);
    let one = word(&mut f, 1);
    let one_byte = f.int(Bits::B8, 1);
    let not_under = f.binary(Binary::Xor, under, one_byte);
    let lost_byte = f.narrow(lost, Bits::B8);
    let goes = f.binary(Binary::Or, lost_byte, not_under);
    let low = f.binary(Binary::Sub, r_next.0, b.0);
    let borrowed = f.compare(Compare::LessU, r_next.0, b.0);
    let borrow = f.widen(false, borrowed, Bits::B64);
    let high = f.binary(Binary::Sub, r_next.1, b.1);
    let high = f.binary(Binary::Sub, high, borrow);
    let r_low = f.choose(goes, low, r_next.0);
    let r_high = f.choose(goes, high, r_next.1);
    let goes_word = f.widen(false, goes, Bits::B64);
    let (q_next, _) = shifted(&mut f, q, goes_word);
    let left_less = f.binary(Binary::Sub, left, one);
    let finished = f.compare(Compare::Equal, left_less, zero);
    f.switch(
        finished,
        vec![
            (
                round,
                vec![
                    a_next.0, a_next.1, r_low, r_high, q_next.0, q_next.1, left_less,
                ],
            ),
            (finish, vec![q_next.0, q_next.1, r_low, r_high]),
        ],
    );
    f.at(finish);
    f.ret(vec![done[0], done[1], done[2], done[3]]);
    f.finish(&[WORD, WORD, WORD, WORD])
}

/// Takes a number of two words, the low one first, and a byte that is not 0
/// when it is to be read as a signed number, and gives a new text that is its
/// digits in decimal, after a minus sign when it is negative. `divide` is the
/// function that takes a number by 10.
pub(super) fn decimal(divide: usize) -> Function {
    let mut f = Builder::new();
    let p = f.params(&[WORD, WORD, BYTE]);
    let zero = word(&mut f, 0);
    let one = word(&mut f, 1);
    let top = word(&mut f, 63);
    // A signed number is negative when its top bit is set, and then it is
    // taken from 0 to be the magnitude.
    let sign_bit = f.binary(Binary::ShiftRightU, p[1], top);
    let signed = f.widen(false, p[2], Bits::B64);
    let negative_word = f.binary(Binary::And, sign_bit, signed);
    let negative = f.narrow(negative_word, Bits::B8);
    let off = f.binary(Binary::Sub, zero, p[0]);
    let borrowed = f.compare(Compare::LessU, zero, p[0]);
    let borrow = f.widen(false, borrowed, Bits::B64);
    let off_high = f.binary(Binary::Sub, zero, p[1]);
    let off_high = f.binary(Binary::Sub, off_high, borrow);
    let low = f.choose(negative, off, p[0]);
    let high = f.choose(negative, off_high, p[1]);
    let ten = word(&mut f, 10);
    // The digits of the magnitude counted: at least one.
    let (count_head, counting) = f.block(&[WORD, WORD, WORD]);
    let (sized, size) = f.block(&[WORD]);
    f.jump(count_head, vec![low, high, zero]);
    f.at(count_head);
    let more = f.binary(Binary::Add, counting[2], one);
    let quotient = f.call(
        divide,
        vec![counting[0], counting[1], ten, zero],
        &[WORD; 4],
    );
    let both = f.binary(Binary::Or, quotient[0], quotient[1]);
    let none_left = f.compare(Compare::Equal, both, zero);
    f.switch(
        none_left,
        vec![
            (count_head, vec![quotient[0], quotient[1], more]),
            (sized, vec![more]),
        ],
    );
    f.at(sized);
    let sign = f.widen(false, negative, Bits::B64);
    let total = f.binary(Binary::Add, size[0], sign);
    let start = f.allocate(total);
    // The digits are written from the last, which is what is left over by 10.
    let (write_head, writing) = f.block(&[WORD, WORD, WORD]);
    let (finish, _) = f.block(&[]);
    let (minus, _) = f.block(&[]);
    let (done, _) = f.block(&[]);
    let last = f.binary(Binary::Sub, total, one);
    f.jump(write_head, vec![low, high, last]);
    f.at(write_head);
    let quotient = f.call(divide, vec![writing[0], writing[1], ten, zero], &[WORD; 4]);
    let digit = f.narrow(quotient[2], Bits::B8);
    let zero_char = f.int(Bits::B8, u64::from(b'0'));
    let character = f.binary(Binary::Add, digit, zero_char);
    let at = f.binary(Binary::Add, start, writing[2]);
    f.store(at, character);
    let before = f.binary(Binary::Sub, writing[2], one);
    let both = f.binary(Binary::Or, quotient[0], quotient[1]);
    let none_left = f.compare(Compare::Equal, both, zero);
    f.switch(
        none_left,
        vec![
            (write_head, vec![quotient[0], quotient[1], before]),
            (finish, vec![]),
        ],
    );
    f.at(finish);
    f.switch(negative, vec![(done, vec![]), (minus, vec![])]);
    f.at(minus);
    let sign_char = f.int(Bits::B8, u64::from(b'-'));
    f.store(start, sign_char);
    f.ret(vec![start, total]);
    f.at(done);
    f.ret(vec![start, total]);
    f.finish(&TEXT)
}
