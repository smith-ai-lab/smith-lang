//! `type NAME = TYPE` names a type, and the name is the same type as what it
//! names: a program written with aliases builds the bytes of the program with
//! each alias written out, an alias that names itself is refused with its
//! chain, a use with the wrong number of type arguments is refused with the
//! count, and an alias is exported and imported as a struct is.

use smith::{Project, compile, compile_project_timed};

use crate::Test;
use crate::generate::{self, Gen};
use crate::programs::with_imports;
use crate::targets::any;

pub const ONCE: &[Test] = &[
    (
        "a_program_written_with_aliases_builds_the_bytes_of_the_same_program_written_out",
        a_program_written_with_aliases_builds_the_bytes_of_the_same_program_written_out,
    ),
    (
        "an_alias_that_names_itself_is_refused_at_its_declaration_with_the_chain",
        an_alias_that_names_itself_is_refused_at_its_declaration_with_the_chain,
    ),
    (
        "a_use_with_the_wrong_number_of_type_arguments_is_refused_with_the_count",
        a_use_with_the_wrong_number_of_type_arguments_is_refused_with_the_count,
    ),
    (
        "an_alias_a_module_makes_pub_is_imported_and_stands_as_what_it_names",
        an_alias_a_module_makes_pub_is_imported_and_stands_as_what_it_names,
    ),
    (
        "an_alias_has_no_impl_of_its_own_and_an_error_names_the_alias",
        an_alias_has_no_impl_of_its_own_and_an_error_names_the_alias,
    ),
];

/// The types an alias may name, with a value of each, and the number of type
/// arguments the alias takes.
const TARGETS: &[(&str, &str)] = &[
    ("0..65536", "80"),
    ("Int", "7"),
    ("(Int, Int)", "(1, 2)"),
    ("List[Int]", "[1, 2, 3]"),
    ("str", "\"hi\""),
];

/// A program with aliases of some of `TARGETS` and a function of each.
#[derive(Debug)]
struct Case {
    /// The target each alias names.
    names: Vec<usize>,
    /// Whether the alias is `Pair[T]` and written `Pair[Int]`.
    generic: bool,
}

fn case(g: &mut Gen) -> Case {
    let count = 1 + g.below(3);
    Case {
        names: (0..count).map(|_| g.below(TARGETS.len())).collect(),
        generic: g.below(2) == 0,
    }
}

impl Case {
    /// The program, with each type written as an alias, or out.
    fn source(&self, aliased: bool) -> String {
        let mut text = String::from("use std.console (console, print)\n\n");
        if aliased {
            for (i, &t) in self.names.iter().enumerate() {
                text += &format!("type A{i} = {}\n", TARGETS[t].0);
            }
            if self.generic {
                text += "type Pair[T] = (T, T)\n";
            }
            text += "\n";
        }
        let ty = |i: usize| {
            if aliased {
                format!("A{i}")
            } else {
                TARGETS[self.names[i]].0.to_string()
            }
        };
        for i in 0..self.names.len() {
            text += &format!("fn pass{i}(x: {}) -> {} {{ x }}\n", ty(i), ty(i));
        }
        let pair = if aliased { "Pair[Int]" } else { "(Int, Int)" };
        text += &format!("fn twice(p: {pair}) -> {pair} {{ p }}\n\n");
        text += "fn main() <console, alloc> {\n";
        for (i, &t) in self.names.iter().enumerate() {
            // A value of the alias is taken where the type is, and back.
            let other = if aliased { TARGETS[t].0.to_string() } else { ty(i) };
            text += &format!("  v{i}: {other} = {}\n  w{i}: {} = pass{i}(v{i})\n", TARGETS[t].1, ty(i));
        }
        text += "  (a, b) = twice((1, 2))\n  print(\"{a} {b}\")\n}\n";
        with_imports(&text)
    }
}

fn a_program_written_with_aliases_builds_the_bytes_of_the_same_program_written_out() {
    generate::check(91, 8, case, |case| {
        let with = compile(case.source(true).as_bytes(), any())
            .unwrap_or_else(|e| panic!("{e:?}\n{}", case.source(true)));
        let out = compile(case.source(false).as_bytes(), any())
            .unwrap_or_else(|e| panic!("{e:?}\n{}", case.source(false)));
        assert!(with == out, "the aliases change the bytes:\n{}", case.source(true));
    });
}

fn messages(source: &str) -> Vec<String> {
    match smith::check(with_imports(source).as_bytes()) {
        Ok(warnings) => warnings.into_iter().map(|w| w.message).collect(),
        Err(errors) => errors.into_iter().map(|e| e.message).collect(),
    }
}

fn an_alias_that_names_itself_is_refused_at_its_declaration_with_the_chain() {
    generate::check(92, 6, |g| (1 + g.below(3), g.below(2) == 0), |&(length, listed)| {
        let mut text = String::from("use std.console (console, print)\n\n");
        for i in 0..length {
            let next = format!("A{}", (i + 1) % length);
            // A `List` of it does not make it a type that is held out of line.
            text += &if listed {
                format!("type A{i} = List[{next}]\n")
            } else {
                format!("type A{i} = ({next}, Int)\n")
            };
        }
        text += "\nfn f(a: A0) {}\n\nfn main() <console> {\n  print(\"x\")\n}\n";
        let found = messages(&text);
        let chain: Vec<String> = (0..=length).map(|i| format!("A{}", i % length)).collect();
        assert!(
            found.iter().any(|m| m.contains(&chain.join(" -> "))),
            "{found:?}"
        );
    });
}

fn a_use_with_the_wrong_number_of_type_arguments_is_refused_with_the_count() {
    generate::check(93, 8, |g| (g.below(4), 1 + g.below(3)), |&(given, takes)| {
        let params: Vec<String> = (0..takes).map(|i| format!("T{i}")).collect();
        let held: Vec<String> = params.clone();
        let args: Vec<&str> = vec!["Int"; given];
        let use_ = if given == 0 { "Many".to_string() } else { format!("Many[{}]", args.join(", ")) };
        let text = format!(
            "use std.console (console, print)\n\ntype Many[{}] = ({}, Int)\n\nfn f(x: {use_}) {{}}\n\nfn main() <console> {{\n  print(\"x\")\n}}\n",
            params.join(", "),
            held.join(", "),
        );
        let found = messages(&text);
        let wrong = given != takes;
        let said = found.iter().any(|m| m.contains(&format!("`Many` takes {takes} type argument")) && m.contains(&format!("this gives {given}")));
        assert_eq!(said, wrong, "{found:?}");
    });
}

fn an_alias_a_module_makes_pub_is_imported_and_stands_as_what_it_names() {
    let module = "pub type Port = 0..65536\npub type Pair[T] = (T, T)\n";
    let files = |name: &str| (name == "m").then(|| module.to_string());
    let project = Project { files: &files, entry: None };
    let with = "use std.console (console, print)\nuse m (Pair, Port as P)\n\nfn show(p: P, q: Pair[Int]) -> (P, Pair[Int]) { (p, q) }\n\nfn main() <console, alloc> {\n  (a, (b, c)) = show(80, (1, 2))\n  print(\"{a} {b} {c}\")\n}\n";
    let out = "use std.console (console, print)\n\nfn show(p: 0..65536, q: (Int, Int)) -> (0..65536, (Int, Int)) { (p, q) }\n\nfn main() <console, alloc> {\n  (a, (b, c)) = show(80, (1, 2))\n  print(\"{a} {b} {c}\")\n}\n";
    let built = compile_project_timed(
        with.as_bytes(),
        &project,
        any(),
        &mut smith::timings::Timings::new(),
    )
    .unwrap_or_else(|e| panic!("{e:?}"));
    let plain = compile(out.as_bytes(), any()).expect("the program builds");
    assert!(built == plain, "an imported alias builds other bytes than what it names");
    // A name the module keeps private is not imported.
    let private = |name: &str| (name == "m").then(|| "type Port = 0..65536\n".to_string());
    let project = Project { files: &private, entry: None };
    let refused = compile_project_timed(
        "use std.console (console, print)\nuse m (Port)\n\nfn main() <console> {\n  print(\"x\")\n}\n".as_bytes(),
        &project,
        any(),
        &mut smith::timings::Timings::new(),
    );
    assert!(refused.is_err(), "a private alias is imported");
}

fn an_alias_has_no_impl_of_its_own_and_an_error_names_the_alias() {
    // An `impl` for the alias is an `impl` for what it names, which has one.
    let twice = messages(
        "use std.console (console, print)\n\nstruct Dog { n: Int }\ntype Pet = Dog\n\nimpl Fmt[Dog] {\n  fn fmt(value: Dog) -> str <alloc> { \"dog\" }\n}\nimpl Fmt[Pet] {\n  fn fmt(value: Pet) -> str <alloc> { \"pet\" }\n}\n\nfn main() <console, alloc> {\n  p: Pet = Dog(n: 1)\n  print(\"{p}\")\n}\n",
    );
    assert!(twice.iter().any(|m| m.contains("`Fmt` is already implemented for")), "{twice:?}");
    // The alias is named where the program wrote it.
    let named = messages(
        "use std.console (console, print)\n\ntype Port = 0..65536\n\nfn main() <console> {\n  a: Port = 70000\n  print(\"x\")\n}\n",
    );
    assert!(named.iter().any(|m| m.contains("`Port`")), "{named:?}");
    assert!(named.iter().all(|m| !m.contains("u16")), "{named:?}");
}
