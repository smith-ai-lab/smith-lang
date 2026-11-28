//! `arith` knows the size of a text written out, as of a list written out:
//! the number of bytes the literal is, escapes and characters of several
//! bytes counted by their bytes, through a binding and an argument, and of
//! its `bytes` too. It knows `boundary(s, i)` of such a text at each place
//! that falls between its characters, and at no other.
//!
//! The model is plain Rust: the generator keeps the bytes each piece of the
//! literal stands for, beside how the piece is written.

use smith::scalar::{self, Outcome};
use smith::{check, lower};

use crate::Test;
use crate::generate::{self, Gen};

pub const ONCE: &[Test] = &[
    (
        "the_byte_at_the_last_place_of_a_text_written_out_is_read_with_no_check",
        the_byte_at_the_last_place_of_a_text_written_out_is_read_with_no_check,
    ),
    (
        "a_text_written_out_has_the_size_of_its_bytes_through_a_binding_and_an_argument",
        a_text_written_out_has_the_size_of_its_bytes_through_a_binding_and_an_argument,
    ),
    (
        "a_text_written_out_is_cut_at_each_place_between_its_characters_and_refused_at_any_other",
        a_text_written_out_is_cut_at_each_place_between_its_characters_and_refused_at_any_other,
    ),
];

/// How a piece of a literal is written, and the bytes it stands for: ASCII,
/// each escape, and characters of two, three, and four bytes.
const PIECES: &[(&str, &str)] = &[
    ("a", "a"),
    (" ", " "),
    ("\\n", "\n"),
    ("\\t", "\t"),
    ("\\\"", "\""),
    ("\\\\", "\\"),
    ("\\{", "{"),
    ("\\}", "}"),
    ("é", "é"),
    ("€", "€"),
    ("中", "中"),
    ("\u{1F600}", "\u{1F600}"),
];

/// A text literal, as it is written between its quotes, and its bytes.
#[derive(Debug)]
struct Literal {
    written: String,
    bytes: Vec<u8>,
}

fn literal(g: &mut Gen) -> Literal {
    let mut written = String::new();
    let mut bytes = Vec::new();
    for _ in 0..1 + g.below(6) {
        let (w, b) = g.pick(PIECES);
        written.push_str(w);
        bytes.extend_from_slice(b.as_bytes());
    }
    Literal { written, bytes }
}

fn prints(source: &str, stdout: Vec<u8>) {
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for {source}"));
    assert_eq!(
        scalar::run(&program),
        Outcome { stdout, status: 0 },
        "for {source}"
    );
}

fn refused(source: &str) -> Vec<String> {
    match check(source.as_bytes()) {
        Ok(_) => Vec::new(),
        Err(errors) => errors.into_iter().map(|e| e.message).collect(),
    }
}

fn the_byte_at_the_last_place_of_a_text_written_out_is_read_with_no_check() {
    let source = "use std.console (console, print)\n\nfn main() <console, alloc> {\n  s = \"hello\"\n  print(\"{s.bytes[4]}\")\n}\n";
    prints(source, b"111\n".to_vec());
}

/// A function that asks for the size of its text and of its bytes, called
/// with the literal, with a name given it, and with a name given that name,
/// which also reads its last byte and its size where it was bound.
fn sized(l: &Literal, size: usize) -> String {
    let n = l.bytes.len();
    format!(
        "use std.console (console, print)\n\nfn size(s: str) -> Nat\n  requires s.size == {size}\n  requires s.bytes.size == {size}\n{{\n  s.size\n}}\n\nfn main() <console, alloc> {{\n  s = \"{w}\"\n  t = s\n  a = size(\"{w}\")\n  b = size(s)\n  c = size(t)\n  last = t.bytes[{n} - 1]\n  m: Nat = \"{w}\".size\n  print(\"{{a}} {{b}} {{c}} {{m}} {{last}}\")\n}}\n",
        w = l.written,
    )
}

fn a_text_written_out_has_the_size_of_its_bytes_through_a_binding_and_an_argument() {
    generate::check(126001, 3, literal, |l| {
        let n = l.bytes.len();
        let last = l.bytes[n - 1];
        prints(&sized(l, n), format!("{n} {n} {n} {n} {last}\n").into_bytes());
        let errors = refused(&sized(l, n + 1));
        assert!(
            errors.iter().any(|e| e.contains(&format!("s.size == {}", n + 1))),
            "{errors:?}"
        );
    });
    // The example of the issue: `é` is two bytes.
    let source = "fn ok(s: str) -> Nat\n  requires s.size == 6\n{\n  s.size\n}\n\nfn main() {\n  n = ok(\"héllo\")\n}\n";
    assert_eq!(refused(source), Vec::<String>::new());
}

/// A program that, for each place, cuts the literal there three ways: with
/// a function that asks for a boundary, given the literal and a name of
/// it, and as a piece of the name.
fn cut_at(l: &Literal, places: &[usize]) -> String {
    let cuts: String = (places.iter())
        .map(|i| {
            format!(
                "  print(cut(\"{w}\", {i}))\n  print(cut(s, {i}))\n  print(s[{i}..])\n",
                w = l.written
            )
        })
        .collect();
    format!(
        "use std.console (console, print)\n\nfn cut(s: str, i: Nat) -> str\n  requires i <= s.size\n  requires boundary(s, i)\n{{\n  s[..i]\n}}\n\nfn main() <console, alloc> {{\n  s = \"{}\"\n{cuts}}}\n",
        l.written
    )
}

fn a_text_written_out_is_cut_at_each_place_between_its_characters_and_refused_at_any_other() {
    generate::check(126002, 3, literal, |l| {
        let n = l.bytes.len();
        let between = |i: usize| i == n || l.bytes[i] & 0xC0 != 0x80;
        let (places, inside): (Vec<usize>, Vec<usize>) = (0..=n).partition(|&i| between(i));
        let mut stdout = Vec::new();
        for &i in &places {
            for piece in [&l.bytes[..i], &l.bytes[..i], &l.bytes[i..]] {
                stdout.extend(piece);
                stdout.push(b'\n');
            }
        }
        prints(&cut_at(l, &places), stdout);
        let errors = refused(&cut_at(l, &inside));
        for i in &inside {
            let literal = format!("`boundary(\"{}\", {i})`", l.written);
            let name = format!("`boundary(s, {i})`");
            for place in [literal, name] {
                let named = errors.iter().filter(|e| e.contains(&place)).count();
                assert!(named >= 1, "no refusal names {place} in {errors:?}");
            }
        }
        // Each place inside a character is refused for the function given
        // the name, and for the piece of it.
        let of_name = "`boundary(s, ";
        let count = errors.iter().filter(|e| e.contains(of_name)).count();
        assert_eq!(count, 2 * inside.len(), "{errors:?}");
    });
}
