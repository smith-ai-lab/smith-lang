//! A piece of a text, `t[a..b]`, is the bytes of `t` from `a` up to `b` as a
//! `str`, sharing its memory, and compiles exactly when `a <= b`,
//! `b <= t.size`, and `boundary(t, a)` and `boundary(t, b)` are known.
//! `boundary` is known at 0 and at the size, on both sides of a byte below
//! 128, and from a `requires`, an `ensures`, or an `invariant`.
//!
//! The model is plain Rust: the generator keeps the bytes of the text it
//! wrote, and the positions it asks of, and says which ends the program can
//! show fall between characters, from the bytes on each side of them.

use smith::scalar::{self, Outcome};
use smith::{check, lower};

use crate::Test;
use crate::generate::{self, Gen};
use crate::targets::{runnable, runs_as};

pub const ONCE: &[Test] = &[
    (
        "a_piece_of_a_text_whose_ends_are_shown_between_characters_is_its_bytes_between_them",
        a_piece_of_a_text_whose_ends_are_shown_between_characters_is_its_bytes_between_them,
    ),
    (
        "a_piece_of_a_text_is_the_program_of_the_same_piece_of_its_bytes",
        a_piece_of_a_text_is_the_program_of_the_same_piece_of_its_bytes,
    ),
    (
        "a_piece_of_a_text_at_a_place_nothing_shows_between_characters_is_refused_naming_it",
        a_piece_of_a_text_at_a_place_nothing_shows_between_characters_is_refused_naming_it,
    ),
    (
        "a_piece_of_a_text_whose_ends_are_not_proven_in_order_is_refused_as_any_piece",
        a_piece_of_a_text_whose_ends_are_not_proven_in_order_is_refused_as_any_piece,
    ),
    (
        "a_place_that_an_ensures_promises_and_an_invariant_keeps_cuts_a_text_there",
        a_place_that_an_ensures_promises_and_an_invariant_keeps_cuts_a_text_there,
    ),
    (
        "boundary_written_where_the_program_computes_is_refused",
        boundary_written_where_the_program_computes_is_refused,
    ),
    (
        "a_piece_at_a_place_inside_a_character_is_refused_after_any_test_of_its_byte_but_one_below_128",
        a_piece_at_a_place_inside_a_character_is_refused_after_any_test_of_its_byte_but_one_below_128,
    ),
];

pub const MACHINE: &[Test] = &[(
    "every_executable_cuts_a_text_as_the_evaluator_does",
    every_executable_cuts_a_text_as_the_evaluator_does,
)];

/// One byte, two, three, and four of UTF-8, and ASCII.
const CHARACTERS: &[char] = &['a', ' ', '<', 'é', 'ж', '€', '中', '\u{1F600}'];

/// A text of generated characters, and pairs of places in it, each at most
/// one past its end, so that some places fall inside a character.
#[derive(Debug)]
struct Cuts {
    bytes: Vec<u8>,
    pairs: Vec<(usize, usize)>,
}

fn cuts(g: &mut Gen) -> Cuts {
    let mut bytes = Vec::new();
    for _ in 0..g.below(7) {
        let mut buffer = [0; 4];
        bytes.extend_from_slice(g.pick(CHARACTERS).encode_utf8(&mut buffer).as_bytes());
    }
    let pairs = (0..1 + g.below(12))
        .map(|_| (g.below(bytes.len() + 2), g.below(bytes.len() + 2)))
        .collect();
    Cuts { bytes, pairs }
}

fn written(bytes: &[u8]) -> String {
    let items: Vec<String> = bytes.iter().map(u8::to_string).collect();
    format!("[{}]", items.join(", "))
}

/// Whether the program shows that `at` falls between characters: it is 0
/// or the size, or a byte beside it is below 128.
fn shown(bytes: &[u8], at: usize) -> bool {
    at == 0
        || at == bytes.len()
        || bytes.get(at).is_some_and(|b| *b < 128)
        || (at > 0 && bytes.get(at - 1).is_some_and(|b| *b < 128))
}

/// The condition that shows `boundary(t, x)` for the name `x`.
fn shows(x: &str) -> String {
    format!(
        "({x} == 0 or {x} == t.size or ({x} < t.size and t[{x}] < 128) or ({x} > 0 and t[{x} - 1] < 128))"
    )
}

/// A program that cuts the text at each pair, with `piece` written for the
/// piece of `t` from `a` to `b`, and prints its size and the piece, or `no`
/// where an end is not in order or not shown to fall between characters.
fn program(c: &Cuts, piece: &str, show: &str) -> String {
    let calls: String = (c.pairs.iter())
        .map(|(a, b)| format!("      cut(t, {a}, {b})\n"))
        .collect();
    format!(
        "use std.console (console, print)\nuse std.list (freeze)\nuse std.utf8 (utf8)\n\nfn cut(t: str, a: Nat, b: Nat) <console, alloc> {{\n  if a <= b and b <= t.size and {} and {} {{\n    u = {piece}\n    print(\"{{u.size}}\")\n    {show}\n  }} else {{\n    print(\"no\")\n  }}\n}}\n\nfn main() <console, alloc> {{\n  match utf8(freeze({})) {{\n    .ok(t) {{\n{calls}    }}\n    .err(e) {{ print(\"bad\") }}\n  }}\n}}\n",
        shows("a"),
        shows("b"),
        written(&c.bytes)
    )
}

/// What the program prints, by the model.
fn expected(c: &Cuts) -> Vec<u8> {
    let mut out = Vec::new();
    for &(a, b) in &c.pairs {
        if a <= b && b <= c.bytes.len() && shown(&c.bytes, a) && shown(&c.bytes, b) {
            out.extend(format!("{}\n", b - a).bytes());
            out.extend(&c.bytes[a..b]);
            out.push(b'\n');
        } else {
            out.extend(b"no\n");
        }
    }
    out
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

fn a_piece_of_a_text_whose_ends_are_shown_between_characters_is_its_bytes_between_them() {
    generate::check(77801, 2, cuts, |c| {
        prints(&program(c, "t[a..b]", "print(u)"), expected(c));
    });
}

/// `t[a..b]` and `t.bytes[a..b]` are the same address and size, so the two
/// programs that differ in nothing else are the same scalar program.
fn a_piece_of_a_text_is_the_program_of_the_same_piece_of_its_bytes() {
    let c = generate::sample(77802, 0, cuts);
    let text = lower(program(&c, "t[a..b]", "").as_bytes()).expect("compiles");
    let bytes = lower(program(&c, "t.bytes[a..b]", "").as_bytes()).expect("compiles");
    assert!(text == bytes, "the piece of the text is not the piece of its bytes");
}

/// A function that is given a text and two places, which it knows are in
/// order, and of which `told` are known to fall between characters.
fn told(told: (bool, bool)) -> String {
    let fact = |known: bool, x: &str| match known {
        true => format!("\n  requires boundary(t, {x})"),
        false => String::new(),
    };
    format!(
        "use std.console (console, print)\n\nfn cut(t: str, from: Nat, to: Nat) -> str\n  requires from <= to and to <= t.size{}{}\n{{\n  t[from..to]\n}}\n\nfn main() <console> {{\n  print(cut(\"ab\", 0, 0))\n}}\n",
        fact(told.0, "from"),
        fact(told.1, "to"),
    )
}

fn a_piece_of_a_text_at_a_place_nothing_shows_between_characters_is_refused_naming_it() {
    for from in [false, true] {
        for to in [false, true] {
            let errors = refused(&told((from, to)));
            let named = |x: &str| {
                errors
                    .iter()
                    .any(|e| e.contains(&format!("`boundary(t, {x})`")))
            };
            if !from {
                assert!(named("from"), "{errors:?} for {from} {to}");
            } else if !to {
                assert!(named("to"), "{errors:?} for {from} {to}");
            }
            // The call in `main` has nothing that shows its places fall
            // between characters but 0, which is one, so a function that
            // asks for both compiles.
            assert_eq!(errors.is_empty(), from && to, "{errors:?} for {from} {to}");
        }
    }
    // A text known only when the program runs, cut at a place inside a
    // character, is refused, as nothing shows that any place but its ends
    // falls between characters.
    let c = generate::sample(77803, 0, cuts);
    let inside = (0..c.bytes.len()).find(|&at| !shown(&c.bytes, at));
    if let Some(at) = inside {
        let source = format!(
            "use std.console (console, print)\nuse std.list (freeze)\nuse std.utf8 (utf8)\n\nfn main() <console, alloc> {{\n  match utf8(freeze({})) {{\n    .ok(t) {{\n      at: Nat = {at}\n      if at <= t.size {{ print(t[..at]) }}\n    }}\n    .err(e) {{ }}\n  }}\n}}\n",
            written(&c.bytes)
        );
        let errors = refused(&source);
        assert!(
            errors.iter().any(|e| e.contains("`boundary(t, at)`")),
            "{errors:?}"
        );
    }
}

fn a_piece_of_a_text_whose_ends_are_not_proven_in_order_is_refused_as_any_piece() {
    let source = "use std.console (console, print)\n\nfn cut(t: str, from: Nat, to: Nat) -> str\n  requires to <= t.size\n  requires boundary(t, from) and boundary(t, to)\n{\n  t[from..to]\n}\n\nfn main() <console> {\n  print(cut(\"ab\", 0, 0))\n}\n";
    let errors = refused(source);
    assert!(
        errors.iter().any(|e| e.contains("`from <= to`")),
        "{errors:?}"
    );
    assert!(!errors.iter().any(|e| e.contains("boundary")), "{errors:?}");
}

/// A text of generated characters, with or without a space.
fn spaced(g: &mut Gen) -> Vec<u8> {
    let mut bytes = Vec::new();
    for _ in 0..g.below(7) {
        let mut buffer = [0; 4];
        bytes.extend_from_slice(g.pick(CHARACTERS).encode_utf8(&mut buffer).as_bytes());
    }
    bytes
}

fn a_place_that_an_ensures_promises_and_an_invariant_keeps_cuts_a_text_there() {
    generate::check(77804, 3, spaced, |bytes| {
        let source = format!(
            "use std.console (console, print)\nuse std.list (freeze)\nuse std.utf8 (utf8)\n\nfn space(s: str) -> at: Nat\n  ensures at <= s.size\n  ensures boundary(s, at)\n{{\n  var found = s.size\n  loop i in 0..s.size\n    invariant found <= s.size\n    invariant boundary(s, found)\n  {{\n    if found == s.size and s[i] == 32 {{\n      found = i\n    }}\n  }}\n  found\n}}\n\nfn main() <console, alloc> {{\n  match utf8(freeze({})) {{\n    .ok(t) {{\n      at = space(t)\n      print(t[..at])\n      print(t[at..])\n    }}\n    .err(e) {{ print(\"bad\") }}\n  }}\n}}\n",
            written(bytes)
        );
        let at = bytes.iter().position(|b| *b == b' ').unwrap_or(bytes.len());
        let mut stdout = bytes[..at].to_vec();
        stdout.push(b'\n');
        stdout.extend(&bytes[at..]);
        stdout.push(b'\n');
        prints(&source, stdout);
    });
}

/// `boundary` in an `if`, and in a `check`, which both compute what they
/// test, is refused, and the message names where it is written instead.
fn boundary_written_where_the_program_computes_is_refused() {
    let wheres = [
        "if boundary(t, 1) {\n    print(\"yes\")\n  }",
        "match check(boundary(t, 1)) {\n    .some(p) { print(\"yes\") }\n    .none { print(\"no\") }\n  }",
    ];
    for written in wheres {
        let source = format!(
            "use std.console (console, print)\n\nfn main() <console> {{\n  t = \"ab\"\n  {written}\n}}\n"
        );
        let errors = refused(&source);
        let named = errors.iter().any(|e| {
            e.contains("`boundary`")
                && e.contains("never computed")
                && ["`requires`", "`ensures`", "`invariant`"]
                    .iter()
                    .all(|w| e.contains(w))
        });
        assert!(named, "{errors:?} for {source}");
    }
}

/// A text of generated characters with at least one of several bytes.
fn wide(g: &mut Gen) -> Vec<u8> {
    let mut chars: Vec<char> = (0..g.below(6)).map(|_| *g.pick(CHARACTERS)).collect();
    let at = g.below(chars.len() + 1);
    chars.insert(at, *g.pick(&CHARACTERS[3..]));
    chars.iter().collect::<String>().into_bytes()
}

/// A program that, for each place, names it and cuts the text there where
/// its byte passes `test`.
fn cut_where(bytes: &[u8], places: &[usize], test: &str) -> String {
    let cuts: String = (places.iter())
        .map(|i| {
            format!(
                "      a{i}: Nat = {i}\n      if a{i} < t.size and t.bytes[a{i}] {test} {{ print(t[..a{i}]) }} else {{ print(\"no\") }}\n"
            )
        })
        .collect();
    format!(
        "use std.console (console, print)\nuse std.list (freeze)\nuse std.utf8 (utf8)\n\nfn main() <console, alloc> {{\n  match utf8(freeze({})) {{\n    .ok(t) {{\n{cuts}    }}\n    .err(e) {{ print(\"bad\") }}\n  }}\n}}\n",
        written(bytes)
    )
}

/// A byte below 128 is a character of its own, and no other byte is: a test
/// that passes for a byte inside a character, as `< 200` does, shows no
/// boundary, and each piece after it is refused naming its place. After
/// `< 128`, the piece is taken where the byte is ASCII and nowhere else.
fn a_piece_at_a_place_inside_a_character_is_refused_after_any_test_of_its_byte_but_one_below_128() {
    generate::check(77806, 2, wide, |bytes| {
        let inside: Vec<usize> = (0..bytes.len())
            .filter(|&i| bytes[i] & 0xC0 == 0x80)
            .collect();
        let errors = refused(&cut_where(bytes, &inside, "< 200"));
        for i in &inside {
            let place = format!("`boundary(t, a{i})`");
            assert!(
                errors.iter().any(|e| e.contains(&place)),
                "no refusal names {place} in {errors:?}"
            );
        }
        let every: Vec<usize> = (0..bytes.len()).collect();
        let mut stdout = Vec::new();
        for &i in &every {
            if bytes[i] < 128 {
                stdout.extend(&bytes[..i]);
                stdout.push(b'\n');
            } else {
                stdout.extend(b"no\n");
            }
        }
        prints(&cut_where(bytes, &every, "< 128"), stdout);
    });
}

fn every_executable_cuts_a_text_as_the_evaluator_does() {
    let c = generate::sample(77805, 0, cuts);
    let source = program(&c, "t[a..b]", "print(u)");
    let program = lower(source.as_bytes()).expect("compiles");
    let said = scalar::run(&program);
    assert_eq!(said.stdout, expected(&c), "for {source}");
    for target in runnable() {
        runs_as(source.as_bytes(), target, &said);
    }
}
