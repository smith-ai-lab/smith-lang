//! An `impl` of a trait written in a module is checked and lowered as one
//! written in a program is, whether the module is the library's or a
//! project's, and a call of its function gives what the `impl` gives, however
//! the program reaches the types. A handler of an effect is given values of the
//! types its operations write, so the program need not name them, and the
//! `impl`s for them are reached with the operations.

use smith::lower_project;

use crate::Test;
use crate::generate::{self, Gen};

pub const ONCE: &[Test] = &[
    (
        "a_handler_that_ends_each_value_of_net_with_the_release_of_std_net_is_checked_and_runs",
        a_handler_that_ends_each_value_of_net_with_the_release_of_std_net_is_checked_and_runs,
    ),
    (
        "a_handler_that_calls_a_trait_function_of_a_type_the_program_never_names_gets_the_impl",
        a_handler_that_calls_a_trait_function_of_a_type_the_program_never_names_gets_the_impl,
    ),
    (
        "a_call_of_a_trait_function_whose_impls_are_in_a_module_gives_what_the_impl_gives",
        a_call_of_a_trait_function_whose_impls_are_in_a_module_gives_what_the_impl_gives,
    ),
];

/// What a program of a module is: the number of types the module has, each
/// with an `impl` of one trait, the number each weighs, and how the program
/// reaches them.
#[derive(Debug)]
struct Case {
    weights: Vec<u32>,
    /// Whether the program calls the functions that make the values or hands
    /// them to a handler of the effect that the module declares.
    handled: bool,
}

fn case(g: &mut Gen) -> Case {
    let types = g.below(3) + 1;
    Case {
        weights: (0..types).map(|_| g.below(90) as u32 + 1).collect(),
        handled: g.one_in(2),
    }
}

/// The module: a trait, and for each type a struct, an `impl`, a function that
/// makes one, and the operations of an effect that hold it. A module of the
/// library has types that are used exactly once, as `std.net`'s are, and ends
/// each with `__consume`, which only the library may call.
fn module(c: &Case, library: bool) -> String {
    let take = if library { "take " } else { "" };
    let mut text = format!("pub trait Weight[T] {{\n  fn weight(x: {take}T) -> Nat\n}}\n\n");
    for i in 0..c.weights.len() {
        let (field, given, body) = if library {
            ("", "without Copy for T{i}\nwithout Discard for T{i}\n\n", "n = x.n\n    __consume(x)\n    n")
        } else {
            ("pub ", "", "x.n")
        };
        let given = given.replace("{i}", &i.to_string());
        text += &format!(
            "pub struct T{i} {{\n  {field}n: Nat\n}}\n\n{given}impl Weight[T{i}] {{\n  fn weight(x: {take}T{i}) -> Nat {{\n    {body}\n  }}\n}}\n\npub fn make_{i}(n: Nat) -> T{i} {{ T{i}(n: n) }}\n\n"
        );
    }
    text += "pub effect scale {\n";
    for i in 0..c.weights.len() {
        text += &format!("  pub fn open_{i}(n: Nat) -> Result[T{i}, Nat]\n  pub fn drop_{i}(b: {take}T{i})\n");
    }
    text + "}\n"
}

/// The program, which writes no type of the module, and what it prints.
fn program(c: &Case, home: &str) -> (String, String) {
    let n = c.weights.len();
    let ops = |f: &dyn Fn(usize) -> String| (0..n).map(f).collect::<Vec<_>>().join(", ");
    if c.handled {
        let handlers = ops(&|i| format!("open_{i}: fn(n) {{ .err(n) }}, drop_{i}: fn(b) {{ h = weight(b) }}"));
        // Only the first type is reached by a call, the others only by the
        // handler of the operations that write them.
        let uses = "open_0, drop_0";
        let calls: String = (0..1)
            .map(|i| format!("    match open_{i}({}) {{\n      .ok(b) {{ drop_{i}(b) }}\n      .err(e) {{ print(\"{{e}}\") }}\n    }}\n", c.weights[i]))
            .collect();
        let source = format!(
            "use {home} (scale, weight, {uses})\n\nfn main() <console, alloc> {{\n  scale({handlers}) {{\n{calls}  }}\n}}\n"
        );
        return (source, format!("{}\n", c.weights[0]));
    }
    let uses = ops(&|i| format!("make_{i}"));
    let sum = ops(&|i| format!("{{weight(make_{i}({}))}}", c.weights[i])).replace(", ", " ");
    let source = format!("use {home} (weight, {uses})\n\nfn main() <console, alloc> {{\n  print(\"{sum}\")\n}}\n");
    let said: Vec<String> = c.weights.iter().map(|w| w.to_string()).collect();
    (source, said.join(" ") + "\n")
}

/// The program, built with the module as a module of the library and as one of
/// a project, prints what the `impl`s give.
fn prints_what_the_impls_give(c: &Case) {
    let text = module(c, true);
    let (source, said) = program(c, "std.gen");
    let source = crate::numbers::with_header(source);
    let console = smith::library_text("console");
    let library = [("gen", text.as_str()), ("console", console)];
    let built = smith::lower_in(source.as_bytes(), &library)
        .unwrap_or_else(|e| panic!("{e:?} for\n{source}\n{text}"));
    assert_eq!(String::from_utf8(smith::scalar::run(&built).stdout).unwrap(), said);
    let text = module(c, false);
    let (source, said) = program(c, "gen");
    let source = crate::numbers::with_header(source).replace("use std.gen", "use gen");
    let built = lower_project(source.as_bytes(), &[("gen", text.as_str())])
        .unwrap_or_else(|e| panic!("{e:?} for\n{source}\n{text}"));
    assert_eq!(String::from_utf8(smith::scalar::run(&built).stdout).unwrap(), said);
}

fn a_handler_that_calls_a_trait_function_of_a_type_the_program_never_names_gets_the_impl() {
    prints_what_the_impls_give(&Case { weights: vec![3, 4, 5], handled: true });
}

fn a_call_of_a_trait_function_whose_impls_are_in_a_module_gives_what_the_impl_gives() {
    generate::check(30, 20, case, prints_what_the_impls_give);
}

/// A handler of `net` that ends the `Listener` and the `Socket` it is given
/// with `release`, whose `impl`s are in `std.net`, a program that never names
/// either type. The `std` modules the program reaches are given as a test
/// gives them.
fn a_handler_that_ends_each_value_of_net_with_the_release_of_std_net_is_checked_and_runs() {
    let source = crate::numbers::with_header(
        "use std.net (Got, NetError, close, connect, net, read, release, write)

fn ask(question: str) -> Result[Got, NetError] <net, alloc> {
  with c = connect(\"example.test\", 7)
  try write(c, question.bytes)
  read(c, 16)
}

fn main() <console, alloc> {
  net(
    listen_on: fn(host, port) { .err(.refused(host)) },
    accept: fn(l) { .err(.refused(l.host)) },
    stop: fn(l) { n = release(l) },
    connect_to: fn(host, port) { .err(.refused(host)) },
    read: fn(c, most) { .ok(.bytes([104, 105])) },
    write: fn(c, bytes) { .ok(()) },
    close: fn(c) { n = release(c) },
    bind_to: fn(host, port) { .err(.refused(host)) },
    send: fn(s, to, bytes) { .ok(()) },
    receive: fn(s, most) { .err(.refused(s.host)) },
    unbind: fn(s) { n = release(s) }
  ) {
    match ask(\"x\") {
      .ok(bytes) { print(\"answered\") }
      .err(e) { print(\"another\") }
    }
  }
}
"
        .to_string(),
    );
    let library: Vec<(&str, &str)> = ["bits", "console", "dns", "instant", "int", "list", "net", "task", "text", "time"]
        .into_iter()
        .map(|module| (module, smith::library_text(module)))
        .collect();
    let built = smith::lower_in(source.as_bytes(), &library).unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(smith::scalar::run(&built).stdout, b"another\n");
}
