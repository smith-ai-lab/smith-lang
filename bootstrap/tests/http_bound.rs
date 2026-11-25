//! `get` and the other methods read the parameters of a function from a
//! request only when each is read from a text, or is the one that is read
//! from the body: otherwise the program is refused when it is built, and the
//! error names the parameter.

use smith::check;

use crate::Test;
use crate::generate::{self, Gen};

pub const ONCE: &[Test] = &[
    (
        "a_function_whose_parameters_are_not_all_read_from_a_text_but_one_is_refused_naming_the_parameter",
        a_function_whose_parameters_are_not_all_read_from_a_text_but_one_is_refused_naming_the_parameter,
    ),
    (
        "a_parameter_of_a_type_with_its_own_from_text_is_refused_as_not_read_by_a_route_yet",
        a_parameter_of_a_type_with_its_own_from_text_is_refused_as_not_read_by_a_route_yet,
    ),
];

/// The kinds of parameter a generated function has: what each is read from.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Number,
    Truth,
    Words,
    Maybe,
    Defaulted,
    Struct,
    List,
    Nothing,
}

const KINDS: &[Kind] = &[
    Kind::Number,
    Kind::Truth,
    Kind::Words,
    Kind::Maybe,
    Kind::Defaulted,
    Kind::Struct,
    Kind::List,
    Kind::Nothing,
];

fn parameters(g: &mut Gen) -> Vec<Kind> {
    (0..g.below(5)).map(|_| *g.pick(KINDS)).collect()
}

fn written(kind: Kind) -> &'static str {
    match kind {
        Kind::Number => "u64",
        Kind::Truth => "bool",
        Kind::Words => "str",
        Kind::Maybe => "Option[u8]",
        Kind::Defaulted => "u8 = 3",
        Kind::Struct => "Note",
        Kind::List => "List[str]",
        Kind::Nothing => "Nope",
    }
}

fn source(all: &[Kind]) -> String {
    let params: Vec<String> = all
        .iter()
        .enumerate()
        .map(|(i, k)| format!("p{i}: {}", written(*k)))
        .collect();
    format!(
        "use std.bits (u8, u64)\nuse std.deserialize (Deserialize)\nuse std.http (Request, Response, get, http, routes)\n\n\
struct Note {{\n  text: str\n}}\n\nstruct Nope {{\n  n: u8\n}}\n\nwithout Deserialize for Nope\n\n\
fn f({}) -> u64 {{ 1 }}\n\n\
fn app() <http, alloc> {{\n  with routes()\n  get(\"/f\", f)\n}}\n\n\
fn once[T](body: fn() -> T <http>) -> T <alloc> {{\n  http(request: {{ Request(method: \"GET\", path: \"/f\", query: \"\", headers: [], body: \"\") }}, send: fn(r) {{ }}, body: body)\n}}\n\n\
fn main() <alloc> {{\n  with once()\n  app()\n}}\n",
        params.join(", ")
    )
}

fn a_function_whose_parameters_are_not_all_read_from_a_text_but_one_is_refused_naming_the_parameter() {
    generate::check(9001, 2, parameters, |all| {
        let source = source(all);
        let errors = match check(source.as_bytes()) {
            Ok(_) => Vec::new(),
            Err(errors) => errors,
        };
        let bound: Vec<&smith::diag::Error> = errors
            .iter()
            .filter(|e| e.message.contains("`FromRequest`"))
            .collect();
        let bodies: Vec<usize> = (all.iter().enumerate())
            .filter(|(_, k)| matches!(k, Kind::Struct | Kind::List))
            .map(|(i, _)| i)
            .collect();
        let first_nothing = all.iter().position(|k| *k == Kind::Nothing);
        match (first_nothing, bodies.len()) {
            (Some(i), _) => {
                assert_eq!(bound.len(), 1, "{errors:?} for\n{source}");
                assert!(
                    bound[0].message.contains(&format!("`p{i}` has no `FromText`")),
                    "{} for\n{source}",
                    bound[0].message
                );
            }
            (None, n) if n >= 2 => {
                assert_eq!(bound.len(), 1, "{errors:?} for\n{source}");
                let message = &bound[0].message;
                assert!(
                    bodies.iter().all(|i| message.contains(&format!("`p{i}`")))
                        && message.contains("read from the body"),
                    "{message} for\n{source}"
                );
            }
            (None, _) => assert!(bound.is_empty(), "{errors:?} for\n{source}"),
        }
    });
}

fn a_parameter_of_a_type_with_its_own_from_text_is_refused_as_not_read_by_a_route_yet() {
    let source = "use std.bits (u64)\nuse std.fromtext (FromText)\nuse std.http (Request, Response, get, http, routes)\n\n\
struct Day {\n  n: u64\n}\n\nimpl FromText[Day] {\n  fn parse(text: str) -> Option[Day] { Option.some(Day(n: 1)) }\n  fn expects() -> str { \"a day\" }\n}\n\n\
fn f(day: Day) -> u64 { day.n }\n\n\
fn app() <http, alloc> {\n  with routes()\n  get(\"/f\", f)\n}\n\n\
fn once[T](body: fn() -> T <http>) -> T <alloc> {\n  http(request: { Request(method: \"GET\", path: \"/f\", query: \"\", headers: [], body: \"\") }, send: fn(r) { }, body: body)\n}\n\n\
fn main() <alloc> {\n  with once()\n  app()\n}\n";
    let errors = check(source.as_bytes()).err().unwrap_or_default();
    assert!(
        errors.iter().any(|e| e.message.contains("`day` is read from a text by its own `FromText`")),
        "{errors:?}"
    );
}
