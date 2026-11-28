//! Each place of the library that writes a value as it is, in a range that
//! holds it, where it once kept the low bits of a wider one, gives at the edge of that range what Rust says it must: a digit of `hex`
//! at 9, 10, and 15, a character of JSON at 127 and 128, the bits of a byte of
//! the short code of HTTP/2 from the first to the last, and a handle at the
//! greatest `Descriptor`, past which a number is refused.

use smith::lower;
use smith::scalar;

use crate::Test;
use crate::generate::{self, Gen};
use crate::targets::{runnable, runs_as};

pub const ONCE: &[Test] = &[
    (
        "each_digit_of_hex_and_binary_is_what_rust_writes_at_the_edges_of_ten_and_sixteen",
        each_digit_of_hex_and_binary_is_what_rust_writes_at_the_edges_of_ten_and_sixteen,
    ),
    (
        "a_character_of_json_near_128_is_one_byte_exactly_when_it_is_below_128",
        a_character_of_json_near_128_is_one_byte_exactly_when_it_is_below_128,
    ),
    (
        "a_text_in_the_short_code_of_http2_is_read_from_the_first_bit_of_each_byte_to_the_last",
        a_text_in_the_short_code_of_http2_is_read_from_the_first_bit_of_each_byte_to_the_last,
    ),
    (
        "a_handle_at_the_greatest_descriptor_is_given_back_whole_and_the_next_number_is_refused",
        a_handle_at_the_greatest_descriptor_is_given_back_whole_and_the_next_number_is_refused,
    ),
];

pub const MACHINE: &[Test] = &[(
    "every_executable_gives_the_digits_characters_and_handles_at_their_edges_as_the_evaluator_does",
    every_executable_gives_the_digits_characters_and_handles_at_their_edges_as_the_evaluator_does,
)];

/// What the evaluator prints for the program.
fn printed(source: &str) -> String {
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for\n{source}"));
    String::from_utf8(scalar::run(&program).stdout).expect("text")
}

/// Numbers whose digits are each at an edge, 0, 9, 10, or 15, in some place,
/// and some the seed picks.
fn digits(g: &mut Gen) -> Vec<u64> {
    let mut values = vec![0, 9, 10, 15, 16, 0x9a, 0xaf, 0xf9, u64::MAX, 0x9999_aaaa_ffff_0000];
    for _ in 0..g.below(4) + 2 {
        let edges = [0u64, 9, 10, 15];
        let mut n = 0u64;
        for _ in 0..g.below(16) + 1 {
            n = n * 16 + edges[g.below(4)];
        }
        values.push(n);
    }
    values
}

/// A program that writes each number in upper and lower hexadecimal and in
/// binary, and what it prints.
fn hexes(values: &[u64]) -> (String, String) {
    let mut source = "use std.bits (u64)\nuse std.console (console, print)\nuse std.fmt (binary, hex)\n\nfn show(n: u64) <console, alloc> {\n  print(\"{hex(n)} {hex(n, upper: false)} {binary(n)}\")\n}\n\nfn main() <console, alloc> {\n".to_string();
    let mut said = String::new();
    for v in values {
        source += &format!("  show({v})\n");
        said += &format!("{v:X} {v:x} {v:b}\n");
    }
    source += "}\n";
    (source, said)
}

fn each_digit_of_hex_and_binary_is_what_rust_writes_at_the_edges_of_ten_and_sixteen() {
    generate::check(7741, 3, digits, |values| {
        let (source, said) = hexes(values);
        assert_eq!(printed(&source), said, "{source}");
    });
}

/// Characters near 128, where a character of JSON stops being one byte, and
/// near 2048, where it stops being two, the edges always among them.
fn characters(g: &mut Gen) -> Vec<u32> {
    let mut values = vec![0, 127, 128, 2047, 2048];
    for _ in 0..g.below(6) + 2 {
        let by = g.below(40) as u32;
        values.push(if g.one_in(2) { 128 - by.min(128) } else { 128 + by });
    }
    values
}

/// A program that reads each character as an escape of a text of JSON and
/// prints its bytes, and what it prints.
fn escapes(values: &[u32]) -> (String, String) {
    let mut source = "use std.console (console, print)\nuse std.deserialize (ReadError)\nuse std.json (describe, from_json)\n\nfn bytes(json: str) <console, alloc> {\n  r: Result[str, ReadError] = from_json(json)\n  match r {\n    .ok(s) {\n      loop b in s.bytes { print(\"{b}\") }\n      print(\"end\")\n    }\n    .err(e) { print(describe(e)) }\n  }\n}\n\nfn main() <console, alloc> {\n".to_string();
    let mut said = String::new();
    for v in values {
        source += &format!("  bytes(\"\\\"\\\\u{v:04x}\\\"\")\n");
        let c = char::from_u32(*v).expect("a character");
        for b in c.to_string().bytes() {
            said += &format!("{b}\n");
        }
        said += "end\n";
    }
    source += "}\n";
    (source, said)
}

fn a_character_of_json_near_128_is_one_byte_exactly_when_it_is_below_128() {
    generate::check(7742, 3, characters, |values| {
        let (source, said) = escapes(values);
        assert_eq!(printed(&source), said, "{source}");
    });
}

/// A block of headers of HTTP/2 whose one header is `name` with a value of
/// `value`, written in the short code, as RFC 7541 does in its example of
/// C.4.1: `www.example.com` is the bytes below, which use every bit of each.
const SHORT: &[u8] = &[0xf1, 0xe3, 0xc2, 0xe5, 0xf2, 0x3a, 0x6b, 0xa0, 0xab, 0x90, 0xf4, 0xff];

/// A program that reads a header whose name is the 1st entry of the table and
/// whose value is `SHORT`, and what it prints.
fn short_code() -> (String, String) {
    let mut block = vec![0x41u8, 0x80 | SHORT.len() as u8];
    block.extend(SHORT);
    let mut source = "use std.console (console, print)\nuse std.http2 (decode, decoder)\nuse std.list (freeze, push)\n\nfn main() <console, alloc> {\n  mut l: List[Byte] = []\n".to_string();
    for b in block {
        source += &format!("  l = push(take l, {b})\n");
    }
    source += "  (d, r) = decode(decoder(table: 4096), freeze(take l), largest: 65536)\n  match r {\n    .ok(headers) { loop (n, v) in headers { print(\"{n}: {v}\") } }\n    .err(why) { print(why) }\n  }\n}\n";
    (source, ":authority: www.example.com\n".to_string())
}

fn a_text_in_the_short_code_of_http2_is_read_from_the_first_bit_of_each_byte_to_the_last() {
    let (source, said) = short_code();
    assert_eq!(printed(&source), said, "{source}");
}

/// The greatest number a `Descriptor` holds, which is the greatest `i64`.
const GREATEST: u64 = i64::MAX as u64;

/// A program that makes a connection, a listener, and a socket of each handle,
/// and a file of it, and prints what `release` gives back for each, and what it
/// prints. `std.net` and `std.fs` each have a `release`, so they are two
/// programs.
fn handles(values: &[u64]) -> [(String, String); 2] {
    let mut net = "use std.console (console, print)\nuse std.net (connection, listener, release, socket)\n\nfn main() <console, alloc> {\n".to_string();
    let mut fs = "use std.console (console, print)\nuse std.fs (file, release)\n\nfn main() <console, alloc> {\n".to_string();
    let (mut net_said, mut fs_said) = (String::new(), String::new());
    for v in values {
        net += &format!("  c{v} = release(connection({v}))\n  l{v} = release(listener({v}))\n  s{v} = release(socket({v}))\n  print(\"{{c{v}}} {{l{v}}} {{s{v}}}\")\n");
        net_said += &format!("{v} {v} {v}\n");
        fs += &format!("  f{v} = release(file({v}))\n  print(\"{{f{v}}}\")\n");
        fs_said += &format!("{v}\n");
    }
    net += "}\n";
    fs += "}\n";
    [(net, net_said), (fs, fs_said)]
}

/// Handles near the greatest `Descriptor`, it among them.
fn near_greatest(g: &mut Gen) -> Vec<u64> {
    let mut values = vec![0, GREATEST];
    for _ in 0..g.below(3) + 1 {
        values.push(GREATEST - g.below(300) as u64);
    }
    values.sort();
    values.dedup();
    values
}

fn a_handle_at_the_greatest_descriptor_is_given_back_whole_and_the_next_number_is_refused() {
    generate::check(7743, 3, near_greatest, |values| {
        for (source, said) in handles(values) {
            assert_eq!(printed(&source), said, "{source}");
        }
    });
    for (module, made) in [("net", "connection"), ("net", "listener"), ("net", "socket"), ("fs", "file")] {
        let source = format!(
            "use std.{module} ({made}, release)\n\nfn main() {{\n  n = release({made}({}))\n}}\n",
            GREATEST + 1
        );
        let errors = match lower(source.as_bytes()) {
            Ok(_) => panic!("a handle past the greatest descriptor was taken in\n{source}"),
            Err(errors) => format!("{errors:?}"),
        };
        assert!(errors.contains(&(GREATEST + 1).to_string()), "{errors}");
    }
}

/// Each program above, in each executable this machine runs.
fn every_executable_gives_the_digits_characters_and_handles_at_their_edges_as_the_evaluator_does() {
    let targets = runnable();
    generate::check(
        7744,
        1,
        |g| (digits(g), characters(g), near_greatest(g)),
        |(numbers, chars, held)| {
            let [net, fs] = handles(held);
            for (source, said) in [hexes(numbers), escapes(chars), short_code(), net, fs] {
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
