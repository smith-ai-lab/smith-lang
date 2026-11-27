//! A `Payload` of `std.message` is made only by `json`, `form`, `text`, `html`,
//! and `bytes`, and its content type and its length come only from how it was
//! made: the type each maker says, and the number of bytes it holds.

use smith::scalar::{self, Outcome};
use smith::lower;

use crate::Test;
use crate::generate::{self, Gen};

pub const ONCE: &[Test] = &[
    (
        "each_maker_of_a_payload_gives_its_content_type_and_the_length_of_its_bytes",
        each_maker_of_a_payload_gives_its_content_type_and_the_length_of_its_bytes,
    ),
    (
        "a_form_writes_a_space_as_a_plus_and_every_byte_that_is_not_plain_as_a_percent_sign_and_two_digits",
        a_form_writes_a_space_as_a_plus_and_every_byte_that_is_not_plain_as_a_percent_sign_and_two_digits,
    ),
    (
        "a_header_named_content_type_is_refused_in_any_case_where_it_is_written_and_by_check_name",
        a_header_named_content_type_is_refused_in_any_case_where_it_is_written_and_by_check_name,
    ),
];

fn outcome(source: &str) -> Outcome {
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for\n{source}"));
    scalar::run(&program)
}

/// Words with the marks that a form writes in its own way, and that need no
/// escape in a text of Smith.
const WORDS: &[&str] = &["Ana", "a b", "x=y", "a&b", "caf\u{e9}", "100%", "~*-._", "", "+", "/?#"];

/// What the program prints for a payload: its type, its length, and its bytes as text.
const SHOW: &str = "fn show(p: Payload) <console, alloc> {\n  print(\"{content_type(p)}|{size(p)}\")\n  match utf8(freeze(content(p))) {\n    .ok(t) { print(t) }\n    .err(e) { print(\"not text\") }\n  }\n}\n\n";

const USES: &str = "use std.console (console, print)\nuse std.list (freeze)\nuse std.utf8 (utf8)\n";

fn each_maker_of_a_payload_gives_its_content_type_and_the_length_of_its_bytes() {
    let words = |g: &mut Gen| (0..2).map(|_| *g.pick(WORDS)).collect::<Vec<&str>>().concat();
    generate::check(9401, 6, words, |word| {
        let source = format!(
            "{USES}use std.bits (u64)\nuse std.html (render, text as inside)\nuse std.message (Payload, bytes, content, content_type, form, header_value, html, json, size, text)\n\nstruct User {{\n  id: u64\n  name: str\n}}\n\n{SHOW}fn main() <console, alloc> {{\n  \
show(json(User(id: 7, name: \"{word}\")))\n  \
show(text(\"{word}\"))\n  \
show(html(render() {{ inside(\"{word}\") }}))\n  \
show(bytes([104, 105], header_value(\"image/png\")))\n  \
show(form([(\"{word}\", \"v\")]))\n}}\n"
        );
        let said = String::from_utf8(outcome(&source).stdout).expect("text");
        let page = word.replace('&', "&amp;");
        let json = format!("{{\"id\":7,\"name\":\"{word}\"}}");
        let form = format!("{}=v", form_part(word));
        let expected = [
            ("application/json", json),
            ("text/plain; charset=utf-8", word.to_string()),
            ("text/html; charset=utf-8", page),
            ("image/png", "hi".to_string()),
            ("application/x-www-form-urlencoded", form),
        ];
        let mut want = String::new();
        for (kind, body) in expected {
            want += &format!("{kind}|{}\n{body}\n", body.len());
        }
        assert_eq!(said, want, "for\n{source}");
    });
}

/// A part of a form, as the rule says it: a letter, a digit, or one of `*-._`
/// stands for itself, a space is `+`, and any other byte is `%` and two capital
/// hex digits.
fn form_part(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'*' | b'-' | b'.' | b'_' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out += &format!("%{b:02X}"),
        }
    }
    out
}

fn a_form_writes_a_space_as_a_plus_and_every_byte_that_is_not_plain_as_a_percent_sign_and_two_digits() {
    let pairs = |g: &mut Gen| {
        let n = g.below(4);
        (0..n).map(|_| (*g.pick(WORDS), *g.pick(WORDS))).collect::<Vec<_>>()
    };
    generate::check(9402, 8, pairs, |pairs| {
        let list: Vec<String> = pairs.iter().map(|(a, b)| format!("(\"{a}\", \"{b}\")")).collect();
        let source = format!(
            "{USES}use std.message (Payload, content, content_type, form, size)\n\n{SHOW}fn main() <console, alloc> {{\n  show(form([{}]))\n}}\n",
            list.join(", ")
        );
        let body: Vec<String> = pairs.iter().map(|(a, b)| format!("{}={}", form_part(a), form_part(b))).collect();
        let body = body.join("&");
        let said = String::from_utf8(outcome(&source).stdout).expect("text");
        assert_eq!(said, format!("application/x-www-form-urlencoded|{}\n{body}\n", body.len()), "for\n{source}");
    });
}

fn a_header_named_content_type_is_refused_in_any_case_where_it_is_written_and_by_check_name() {
    let names = |g: &mut Gen| {
        "content-type"
            .chars()
            .map(|c| if g.below(2) == 0 { c.to_ascii_uppercase() } else { c })
            .collect::<String>()
    };
    generate::check(9403, 8, names, |name| {
        let source = format!("use std.message (header_name)\n\nfn main() <alloc> {{\n  x = header_name(\"{name}\")\n}}\n");
        let said: Vec<String> = smith::check(source.as_bytes())
            .err()
            .unwrap_or_default()
            .into_iter()
            .map(|e| e.message)
            .collect();
        assert!(said.iter().any(|m| m.contains("not_written_by_the_library") && m.contains(name.as_str())), "{said:?}");
        let source = format!(
            "use std.console (console, print)\nuse std.message (check_name)\n\nfn main() <console, alloc> {{\n  match check_name(\"{name}\") {{\n    .ok(n) {{ print(\"made\") }}\n    .err(.written_by_the_library(t)) {{ print(\"written by the library: {{t}}\") }}\n    .err(e) {{ print(\"another\") }}\n  }}\n}}\n"
        );
        let said = String::from_utf8(outcome(&source).stdout).expect("text");
        assert_eq!(said, format!("written by the library: {name}\n"), "for\n{source}");
    });
}
