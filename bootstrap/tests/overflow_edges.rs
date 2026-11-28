//! Each place of the library that checks with an `if` that a whole number fits
//! gives, at the edge, what the arithmetic of Rust on 128 bits says it must:
//! the greatest value that fits is taken as it is, and the next is refused, as
//! when the library called a `checked_` function there. The values are drawn
//! near the edge, the edge itself and the value after it always among them.

use smith::lower;
use smith::scalar;

use crate::Test;
use crate::generate::{self, Gen};
use crate::targets::{runnable, runs_as};

pub const ONCE: &[Test] = &[
    (
        "a_whole_number_of_json_near_the_greatest_u64_is_read_exactly_when_it_fits",
        a_whole_number_of_json_near_the_greatest_u64_is_read_exactly_when_it_fits,
    ),
    (
        "an_option_of_dns_near_the_greatest_nat_is_taken_exactly_when_it_fits",
        an_option_of_dns_near_the_greatest_nat_is_taken_exactly_when_it_fits,
    ),
    (
        "a_content_length_of_http1_near_the_greatest_nat_is_read_exactly_when_it_fits",
        a_content_length_of_http1_near_the_greatest_nat_is_read_exactly_when_it_fits,
    ),
    (
        "a_chunk_of_http1_whose_end_is_near_the_greatest_nat_is_refused_exactly_when_it_does_not_fit",
        a_chunk_of_http1_whose_end_is_near_the_greatest_nat_is_refused_exactly_when_it_does_not_fit,
    ),
];

pub const MACHINE: &[Test] = &[(
    "every_executable_reads_the_numbers_near_the_greatest_nat_as_the_evaluator_does",
    every_executable_reads_the_numbers_near_the_greatest_nat_as_the_evaluator_does,
)];

/// The greatest `Nat`, and `u64`.
const GREATEST: u128 = u64::MAX as u128;

/// Values near `edge`: the edge, the value after it, and some the seed picks
/// up to 300 on either side.
fn near(g: &mut Gen, edge: u128) -> Vec<u128> {
    let mut values = vec![edge, edge + 1];
    for _ in 0..g.below(6) + 2 {
        let by = g.below(301) as u128;
        values.push(if g.one_in(2) { edge - by } else { edge + by });
    }
    values
}

/// What the evaluator prints for the program.
fn printed(source: &str) -> String {
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for\n{source}"));
    String::from_utf8(scalar::run(&program).stdout).expect("text")
}

/// A program that reads each value as a `u64` of JSON, and what it prints.
fn json(values: &[u128]) -> (String, String) {
    let mut source = "use std.bits (u64)\nuse std.console (console, print)\nuse std.deserialize (ReadError)\nuse std.json (describe, from_json)\n\nfn whole(s: str) <console, alloc> {\n  r: Result[u64, ReadError] = from_json(s)\n  match r {\n    .ok(n) { print(\"{n}\") }\n    .err(e) { print(describe(e)) }\n  }\n}\n\nfn main() <console, alloc> {\n".to_string();
    let mut said = String::new();
    for v in values {
        source += &format!("  whole(\"{v}\")\n");
        said += &if *v <= GREATEST {
            format!("{v}\n")
        } else {
            "1:1: a number that does not fit u64\n".to_string()
        };
    }
    source += "}\n";
    (source, said)
}

fn a_whole_number_of_json_near_the_greatest_u64_is_read_exactly_when_it_fits() {
    generate::check(5701, 3, |g| near(g, GREATEST), |values| {
        let (source, said) = json(values);
        assert_eq!(printed(&source), said, "{source}");
    });
}

/// A program that reads each value as `ndots:` of a configuration of DNS, and
/// what it prints: a number up to the greatest `Nat` is taken, and 15 is the
/// most dots there may be, and a number that does not fit is no option, which
/// leaves the 1 there is when nothing is said.
fn dns(values: &[u128]) -> (String, String) {
    let mut source = "use std.console (console, print)\nuse std.dns (config)\n\nfn dots(s: str) <console, alloc> {\n  print(\"{config(s.bytes).dots}\")\n}\n\nfn main() <console, alloc> {\n".to_string();
    let mut said = String::new();
    for v in values {
        source += &format!("  dots(\"options ndots:{v}\\n\")\n");
        said += if *v <= GREATEST { "15\n" } else { "1\n" };
    }
    source += "}\n";
    (source, said)
}

fn an_option_of_dns_near_the_greatest_nat_is_taken_exactly_when_it_fits() {
    generate::check(5702, 3, |g| near(g, GREATEST), |values| {
        let (source, said) = dns(values);
        assert_eq!(printed(&source), said, "{source}");
    });
}

/// The bytes of a text with each `~` a carriage return, which a text of Smith
/// has no escape for, and the functions that show what `std.http1` reads.
const HTTP1: &str = "use std.console (console, print)\nuse std.http1 (Body, Read, body_of, chunk, request_head)\nuse std.list (freeze, push)\n\nfn wire(s: str) -> List[Byte] <alloc> {\n  mut made: List[Byte] = []\n  loop i in 0..s.size {\n    b = s[i]\n    made = push(take made, if b == '~' { 13 } else { b })\n  }\n  made\n}\n\nfn length(s: str) <console, alloc> {\n  match request_head(freeze(wire(s))) {\n    .complete((h, used)) {\n      match body_of(h.headers) {\n        .ok(.length(n)) { print(\"{n}\") }\n        .ok(_) { print(\"no length\") }\n        .err(why) { print(why) }\n      }\n    }\n    .incomplete { print(\"incomplete\") }\n    .refused(why) { print(why) }\n  }\n}\n\nfn piece(s: str) <console, alloc> {\n  match chunk(freeze(wire(s)), limit: 18446744073709551615) {\n    .complete((c, used)) { print(\"complete\") }\n    .incomplete { print(\"incomplete\") }\n    .refused(why) { print(why) }\n  }\n}\n\n";

/// A program that reads each value as the `content-length` of a request, and
/// what it prints.
fn lengths(values: &[u128]) -> (String, String) {
    let mut source = format!("{HTTP1}fn main() <console, alloc> {{\n");
    let mut said = String::new();
    for v in values {
        source += &format!("  length(\"GET / HTTP/1.1~\\nHost: a~\\nContent-Length: {v}~\\n~\\n\")\n");
        said += &if *v <= GREATEST {
            format!("{v}\n")
        } else {
            "a content-length that is not a number\n".to_string()
        };
    }
    source += "}\n";
    (source, said)
}

fn a_content_length_of_http1_near_the_greatest_nat_is_read_exactly_when_it_fits() {
    generate::check(5703, 3, |g| near(g, GREATEST), |values| {
        let (source, said) = lengths(values);
        assert_eq!(printed(&source), said, "{source}");
    });
}

/// A program that reads each value as the size of a chunk with no limit, and
/// what it prints. The data starts after the size and its line break, so a
/// chunk whose end, that place and the size, is above the greatest `Nat` is
/// too long, and one whose end fits waits for more bytes. A size that is no
/// `Nat` is too large.
fn chunks(values: &[u128]) -> (String, String) {
    let mut source = format!("{HTTP1}fn main() <console, alloc> {{\n");
    let mut said = String::new();
    for v in values {
        let hex = format!("{v:x}");
        let from = hex.len() as u128 + 2;
        source += &format!("  piece(\"{hex}~\\n\")\n");
        said += if *v > GREATEST {
            "a chunk size that is too large\n"
        } else if from + v > GREATEST {
            "a chunk longer than the limit\n"
        } else {
            "incomplete\n"
        };
    }
    source += "}\n";
    (source, said)
}

/// The size whose chunk ends at the greatest `Nat`: 16 hexadecimal digits and
/// a line break come before its data.
const LAST_CHUNK: u128 = GREATEST - 18;

fn a_chunk_of_http1_whose_end_is_near_the_greatest_nat_is_refused_exactly_when_it_does_not_fit() {
    generate::check(
        5704,
        3,
        |g| {
            let mut values = near(g, LAST_CHUNK);
            values.extend([GREATEST, GREATEST + 1]);
            values
        },
        |values| {
            let (source, said) = chunks(values);
            assert_eq!(printed(&source), said, "{source}");
        },
    );
}

/// Each of the programs above, in each executable this machine runs.
fn every_executable_reads_the_numbers_near_the_greatest_nat_as_the_evaluator_does() {
    let targets = runnable();
    generate::check(
        5705,
        1,
        |g| (near(g, GREATEST), near(g, LAST_CHUNK)),
        |(values, ends)| {
            for (source, said) in [json(values), dns(values), lengths(values), chunks(ends)] {
                let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?}"));
                let out = scalar::run(&scalar::passes(&program));
                assert_eq!(String::from_utf8(out.stdout.clone()).expect("text"), said, "{source}");
                for &target in &targets {
                    runs_as(source.as_bytes(), target, &out);
                }
            }
        },
    );
}
