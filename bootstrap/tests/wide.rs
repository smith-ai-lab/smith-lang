//! A range that needs 65 to 128 bits is a type, kept in two words, so the result
//! of `+`, `-`, and `*` of numbers of 64 bits or fewer is never refused for want
//! of room: it is printed, compared, and given on exactly, and a result of more
//! than 128 bits is refused until it is narrowed.
//!
//! The generator draws numbers of every type of 64 bits or fewer, from their
//! least and greatest as often as from anywhere else, and the oracle is a sign
//! and a magnitude of 128 bits, which shares nothing with the compiler.

use smith::scalar::{self, Outcome};
use smith::{check, lower};

use crate::Test;
use crate::generate::{self, Gen};
use crate::numbers::{INTS, Int, with_header};
use crate::targets;

pub const ONCE: &[Test] = &[
    (
        "the_result_of_an_operator_on_numbers_of_64_bits_or_fewer_is_printed_and_compared_exactly",
        the_result_of_an_operator_on_numbers_of_64_bits_or_fewer_is_printed_and_compared_exactly,
    ),
    (
        "a_wide_number_is_held_by_a_parameter_a_result_a_field_a_tuple_and_a_list",
        a_wide_number_is_held_by_a_parameter_a_result_a_field_a_tuple_and_a_list,
    ),
    (
        "a_proof_is_refused_for_a_range_whose_greatest_number_is_above_what_an_i128_holds",
        a_proof_is_refused_for_a_range_whose_greatest_number_is_above_what_an_i128_holds,
    ),
    (
        "a_program_with_no_wide_range_has_no_instruction_for_one",
        a_program_with_no_wide_range_has_no_instruction_for_one,
    ),
    (
        "a_number_above_the_greatest_i128_is_above_every_negative_number_and_every_number_below_it",
        a_number_above_the_greatest_i128_is_above_every_negative_number_and_every_number_below_it,
    ),
    (
        "a_wide_number_divided_by_a_number_gives_the_quotient_and_the_remainder_exactly",
        a_wide_number_divided_by_a_number_gives_the_quotient_and_the_remainder_exactly,
    ),
    (
        "a_wide_number_divided_by_two_to_the_64_is_its_two_halves_and_needs_no_division",
        a_wide_number_divided_by_two_to_the_64_is_its_two_halves_and_needs_no_division,
    ),
];

pub const MACHINE: &[Test] = &[
    (
        "every_executable_gives_what_the_evaluator_gives_for_wide_numbers",
        every_executable_gives_what_the_evaluator_gives_for_wide_numbers,
    ),
    (
        "every_executable_gives_what_the_evaluator_gives_for_a_wide_quotient_and_remainder",
        every_executable_gives_what_the_evaluator_gives_for_a_wide_quotient_and_remainder,
    ),
];

/// A whole number as a sign and a magnitude of 128 bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Sm {
    negative: bool,
    magnitude: u128,
}

impl Sm {
    fn of(n: i128) -> Sm {
        Sm {
            negative: n < 0,
            magnitude: n.unsigned_abs(),
        }
    }

    fn add(self, other: Sm) -> Sm {
        if self.negative == other.negative {
            return Sm {
                negative: self.negative,
                magnitude: self.magnitude + other.magnitude,
            };
        }
        let (more, less) = if self.magnitude >= other.magnitude {
            (self, other)
        } else {
            (other, self)
        };
        let magnitude = more.magnitude - less.magnitude;
        Sm {
            negative: more.negative && magnitude != 0,
            magnitude,
        }
    }

    fn negated(self) -> Sm {
        Sm {
            negative: !self.negative && self.magnitude != 0,
            magnitude: self.magnitude,
        }
    }

    fn times(self, other: Sm) -> Sm {
        let magnitude = self.magnitude * other.magnitude;
        Sm {
            negative: magnitude != 0 && self.negative != other.negative,
            magnitude,
        }
    }

    fn below(self, other: Sm) -> bool {
        match (self.negative, other.negative) {
            (true, false) => !(self.magnitude == 0 && other.magnitude == 0),
            (false, true) => false,
            (false, false) => self.magnitude < other.magnitude,
            (true, true) => self.magnitude > other.magnitude,
        }
    }

    fn written(self) -> String {
        format!("{}{}", if self.negative { "-" } else { "" }, self.magnitude)
    }
}

/// An operation of two numbers and the type of each.
#[derive(Clone, Copy, Debug)]
struct Sum {
    types: (Int, Int),
    op: char,
    values: (i128, i128),
}

impl Sum {
    fn result(self) -> Sm {
        let (a, b) = (Sm::of(self.values.0), Sm::of(self.values.1));
        match self.op {
            '+' => a.add(b),
            '-' => a.add(b.negated()),
            _ => a.times(b),
        }
    }

    fn source(self, name: &str) -> String {
        format!(
            "  {name}a: {} = {}\n  {name}b: {} = {}\n  {name} = {name}a {} {name}b\n",
            self.types.0.name, self.values.0, self.types.1.name, self.values.1, self.op
        )
    }
}

fn value(g: &mut Gen, ty: Int) -> i128 {
    match g.below(4) {
        0 => ty.min(),
        1 => ty.max(),
        2 => (ty.min() + g.below(3) as i128).min(ty.max()),
        _ => (ty.max() - g.below(3) as i128).max(ty.min()),
    }
}

fn sum(g: &mut Gen) -> Sum {
    let (a, b) = (*g.pick(INTS), *g.pick(INTS));
    Sum {
        types: (a, b),
        op: *g.pick(&['+', '-', '*']),
        values: (value(g, a), value(g, b)),
    }
}

fn lowered(source: &str) -> scalar::Program {
    lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for\n{source}"))
}

fn stdout(source: &str) -> String {
    let out = scalar::run(&lowered(source));
    assert_eq!(out.status, 0, "for\n{source}");
    String::from_utf8(out.stdout).unwrap()
}

/// What the program of two sums prints: each result, and the comparisons of the
/// first with the second.
fn program_of(p: Sum, q: Sum) -> (String, String) {
    let (a, b) = (p.result(), q.result());
    let source = with_header(format!(
        "fn main() ! console, alloc {{\n{}{}  print(\"{{p}}\")\n  print(\"{{q}}\")\n  print(\"{{p < q}} {{p <= q}} {{p == q}} {{p > q}} {{p >= q}} {{p != q}}\")\n  print(\"{{Ord.compare(p, q)}}\")\n}}\n",
        p.source("p"),
        q.source("q")
    ));
    let order = if a == b {
        ".equal"
    } else if a.below(b) {
        ".less"
    } else {
        ".greater"
    };
    let (less, same) = (a.below(b), a == b);
    let expected = format!(
        "{}\n{}\n{} {} {} {} {} {}\n{order}\n",
        a.written(),
        b.written(),
        less,
        less || same,
        same,
        !less && !same,
        !less,
        !same
    );
    (source, expected)
}

/// For every two operations of numbers of 64 bits or fewer, each result is
/// printed exactly and compared exactly with the other, whether it needs 64 bits
/// or not, so a product of two numbers of 64 bits is the whole product.
fn the_result_of_an_operator_on_numbers_of_64_bits_or_fewer_is_printed_and_compared_exactly() {
    generate::check(4301, 60, |g: &mut Gen| (sum(g), sum(g)), |(p, q)| {
        let (source, expected) = program_of(*p, *q);
        assert_eq!(stdout(&source), expected, "for\n{source}");
    });
}

/// A wide number is a parameter, a result, a field of a struct, an element of a
/// tuple, and an element of a list, and is the same number in each.
fn a_wide_number_is_held_by_a_parameter_a_result_a_field_a_tuple_and_a_list() {
    const R: &str = "-73786976294838206464..73786976294838206464";
    generate::check(
        4302,
        12,
        |g: &mut Gen| {
            let mut p = sum(g);
            // A sum or a difference is below 2 to the 66.
            p.op = *g.pick(&['+', '-']);
            p
        },
        |p| {
            let source = with_header(format!(
                "struct Holder {{\n  n: {R}\n  tag: Byte\n}}\n\nfn pass(n: {R}) -> {R} {{ n }}\n\nfn first(h: Holder) -> {R} {{ h.n }}\n\nfn main() ! console, alloc {{\n{}  h = Holder(n: p, tag: 7)\n  t = (p, 7)\n  (m, k) = t\n  xs: List[{R}] = [p]\n  print(\"{{pass(p)}} {{first(h)}} {{m}} {{xs[0]}} {{h.tag}} {{k}}\")\n}}\n",
                p.source("p")
            ));
            let n = p.result().written();
            assert_eq!(stdout(&source), format!("{n} {n} {n} {n} 7 7\n"), "for\n{source}");
        },
    );
}

/// The product of two `Nat`s has numbers above the greatest `i128`, which no bound
/// of a proof can be, so `arith` gives a name that holds one no fact. Were its
/// greatest number read as the greatest `i128`, a branch that holds only numbers
/// above that would hold none, and anything would be proven in it: a value that
/// can be as great as 2 to the 128 would enter `0..2` with no mistake reported.
/// With no fact it is refused, and the error says what could not be proven.
fn a_proof_is_refused_for_a_range_whose_greatest_number_is_above_what_an_i128_holds() {
    let source = with_header(
        "fn into(a: Nat, b: Nat) -> Nat {\n  p = a * b\n  if p > 170141183460469231731687303715884105727 {\n    x: 0..2 = p\n    x\n  } else {\n    1\n  }\n}\n\nfn main() ! console, alloc {\n  print(\"{into(3, 4)}\")\n}\n".to_string(),
    );
    let errors = check(source.as_bytes()).expect_err("the product may not fit");
    assert!(
        errors.iter().any(|e| e.message.contains("cannot prove that `p <= 1`")),
        "{errors:?} for\n{source}"
    );
}

/// A program that writes no range of more than 64 bits and has no result of one
/// holds none of the functions that work on two words.
fn a_program_with_no_wide_range_has_no_instruction_for_one() {
    let source = with_header(
        "fn main() ! console, alloc {\n  a: u32 = 4000000000\n  b: u32 = 3\n  c: u64 = 1\n  print(\"{a + b} {a * b} {c + c}\")\n}\n".to_string(),
    );
    let program = lowered(&source);
    assert!(
        program.functions.iter().all(|f| !f.name.contains("wide")),
        "for\n{source}"
    );
}

/// Every executable that this machine runs gives what the evaluator gives for
/// the sums and the comparisons of wide numbers.
fn every_executable_gives_what_the_evaluator_gives_for_wide_numbers() {
    generate::check(4303, 10, |g: &mut Gen| (sum(g), sum(g)), |(p, q)| {
        let (source, expected) = program_of(*p, *q);
        let said = Outcome {
            status: 0,
            stdout: expected.into_bytes(),
        };
        for target in targets::runnable() {
            targets::runs_as(source.as_bytes(), target, &said);
        }
    });
}

/// A product of two numbers of 64 bits or fewer, a divisor written as a number,
/// and what `/` and `%` give, as the sign and the magnitude each has: a quotient
/// rounds toward zero, and what is left over has the sign of the number divided.
#[derive(Clone, Copy, Debug)]
struct Division {
    product: Sum,
    divisor: Sm,
}

impl Division {
    fn expected(self) -> String {
        let p = self.product.result();
        let q = p.magnitude / self.divisor.magnitude;
        let r = p.magnitude % self.divisor.magnitude;
        let quotient = Sm {
            negative: q != 0 && p.negative != self.divisor.negative,
            magnitude: q,
        };
        let rest = Sm {
            negative: r != 0 && p.negative,
            magnitude: r,
        };
        format!("{} {}\n", quotient.written(), rest.written())
    }

    fn source(self) -> String {
        with_header(format!(
            "fn main() ! console, alloc {{\n{}  print(\"{{p / {d}}} {{p % {d}}}\")\n}}\n",
            self.product.source("p"),
            d = self.divisor.written()
        ))
    }
}

fn division(g: &mut Gen) -> Division {
    // The range of the product is wide, as the types hold more than 64 bits
    // between them, so no divisor is a number that does not fit it.
    let wide: Vec<(Int, Int)> = INTS
        .iter()
        .flat_map(|a| INTS.iter().map(move |b| (*a, *b)))
        .filter(|(a, b)| a.bits + b.bits > 64)
        .collect();
    let (a, b) = *g.pick(&wide);
    let mut values = (value(g, a), value(g, b));
    // Where the two are one type and the values show the product fits it, the
    // product is of that type and a divisor above it does not fit; the
    // greatest of each shows no such thing.
    if a.name == b.name {
        values = (a.max(), a.max());
    }
    let product = Sum {
        types: (a, b),
        op: '*',
        values,
    };
    let magnitude = match g.below(5) {
        0 => 1u128 << (1 + g.below(100)),
        1 => (1u128 << (1 + g.below(100))) + 1,
        2 => 3 + g.below(8) as u128,
        3 => u128::from(u64::MAX) - g.below(3) as u128,
        _ => (1u128 << 63) + g.below(3) as u128,
    };
    // A divisor below 0 is taken only where both numbers can be, so that the
    // range of the product has no side that cannot be divided by it.
    let negative = a.signed && b.signed && g.below(2) == 1;
    Division {
        product,
        divisor: Sm { negative, magnitude },
    }
}

/// `p / d` and `p % d` of a product of two numbers of 64 bits or fewer, by a
/// number that is a power of two or not, below 2 to the 64 or above it, and
/// below 0 or above, are the exact quotient and what is left over, whether the
/// range of either is wide or not.
fn a_wide_number_divided_by_a_number_gives_the_quotient_and_the_remainder_exactly() {
    generate::check(4304, 30, division, |d| {
        let source = d.source();
        assert_eq!(stdout(&source), d.expected(), "for\n{source}");
    });
}

/// The halves of a product are what `/` and `%` by 2 to the 64 give, and a
/// program that takes them holds no function of the runtime that divides.
fn a_wide_number_divided_by_two_to_the_64_is_its_two_halves_and_needs_no_division() {
    let source = with_header(
        "fn times(a: Nat, b: Nat) -> (Nat, Nat) {\n  p = a * b\n  (p / 18446744073709551616, p % 18446744073709551616)\n}\n\nfn main() ! console, alloc {\n  (high, low) = times(18446744073709551615, 2)\n  print(\"{high} {low}\")\n}\n".to_string(),
    );
    assert_eq!(stdout(&source), "1 18446744073709551614\n", "for\n{source}");
    let program = lowered(&source);
    assert!(
        program.functions.iter().all(|f| !f.name.contains("divide")),
        "a division was called for\n{source}"
    );
}

/// Every executable that this machine runs gives what the evaluator gives for
/// a quotient and a remainder of a wide number.
fn every_executable_gives_what_the_evaluator_gives_for_a_wide_quotient_and_remainder() {
    generate::check(4305, 8, division, |d| {
        let said = Outcome {
            status: 0,
            stdout: d.expected().into_bytes(),
        };
        for target in targets::runnable() {
            targets::runs_as(d.source().as_bytes(), target, &said);
        }
    });
}

/// The product of two `u64`s is above the greatest `i128` and a negative number
/// is below zero, so the product is above it, though its high word has its top
/// bit set as a negative number's does: the order is by the signs first, and
/// then by the words.
fn a_number_above_the_greatest_i128_is_above_every_negative_number_and_every_number_below_it() {
    let source = with_header(
        "fn main() ! console, alloc {\n  pa: u64 = 18446744073709551615\n  pb: u64 = 18446744073709551615\n  p = pa * pb\n  qa: i32 = -2147483648\n  qb: i32 = -2147483648\n  q = qa + qb\n  print(\"{p < q} {p <= q} {p == q} {p > q} {p >= q} {p != q}\")\n  print(\"{q < p} {q <= p} {q == p} {q > p} {q >= p} {q != p}\")\n  print(\"{Ord.compare(p, q)} {Ord.compare(q, p)}\")\n}\n".to_string(),
    );
    assert_eq!(
        stdout(&source),
        "false false false true true true\ntrue true false false false true\n.greater .less\n",
        "for\n{source}"
    );
}
