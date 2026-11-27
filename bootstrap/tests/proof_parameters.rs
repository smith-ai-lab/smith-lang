//! A boolean expression written where a type is, as the statement that it is
//! true: a parameter of that type is a proof the caller owes, a result of it is
//! a proof the function gives, and no proof is in the executable.
//!
//! What a call owes is what `requires` already says, so the program that takes
//! the statement as a proof parameter is judged against the program that states
//! it in a `requires`, and the program with every proof taken out by hand.

use smith::{check, lower};

use crate::Test;
use crate::arith::{Condition, condition};
use crate::generate::{self, Gen};
use crate::numbers::{INTS, Int, with_header};

pub const ONCE: &[Test] = &[
    (
        "a_call_compiles_exactly_when_the_proof_passed_is_of_the_statement_with_the_arguments_put_in",
        a_call_compiles_exactly_when_the_proof_passed_is_of_the_statement_with_the_arguments_put_in,
    ),
    (
        "a_proof_left_out_where_arith_is_the_default_is_built_from_what_is_known",
        a_proof_left_out_where_arith_is_the_default_is_built_from_what_is_known,
    ),
    (
        "a_proof_a_call_gives_is_passed_on_and_the_program_is_the_one_without_proofs",
        a_proof_a_call_gives_is_passed_on_and_the_program_is_the_one_without_proofs,
    ),
    (
        "a_proof_that_is_not_passed_or_not_a_proof_or_picked_by_is_refused_with_its_name",
        a_proof_that_is_not_passed_or_not_a_proof_or_picked_by_is_refused_with_its_name,
    ),
    (
        "a_function_that_gives_a_proof_and_carries_an_effect_is_refused",
        a_function_that_gives_a_proof_and_carries_an_effect_is_refused,
    ),
    (
        "a_proof_function_that_is_refused_is_one_error_and_never_reaches_the_lowering",
        a_proof_function_that_is_refused_is_one_error_and_never_reaches_the_lowering,
    ),
    (
        "a_function_that_gives_a_proof_with_an_effect_listed_is_refused_saying_why",
        a_function_that_gives_a_proof_with_an_effect_listed_is_refused_saying_why,
    ),
    (
        "a_name_bound_to_a_proof_is_read_when_arith_may_use_it_and_not_otherwise",
        a_name_bound_to_a_proof_is_read_when_arith_may_use_it_and_not_otherwise,
    ),
    (
        "a_proof_of_a_statement_that_names_a_var_is_refused_with_the_name_of_the_var",
        a_proof_of_a_statement_that_names_a_var_is_refused_with_the_name_of_the_var,
    ),
    (
        "a_proof_of_a_check_is_a_fact_after_a_call_that_gives_it_back",
        a_proof_of_a_check_is_a_fact_after_a_call_that_gives_it_back,
    ),
    (
        "a_proof_parameter_is_a_fact_in_the_function_and_a_proof_to_pass_on",
        a_proof_parameter_is_a_fact_in_the_function_and_a_proof_to_pass_on,
    ),
];

/// A signed type with room for the numbers of the conditions.
fn room(g: &mut Gen) -> Int {
    let wide: Vec<Int> = INTS
        .iter()
        .filter(|t| t.signed && (16..=32).contains(&t.bits))
        .copied()
        .collect();
    *g.pick(&wide)
}

/// A text with `x` and `y` in it, with each the other.
fn swapped(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            'x' => 'y',
            'y' => 'x',
            c => c,
        })
        .collect()
}

/// What is owed, a callee's statement over its parameters `x` and `y`, and what
/// the caller has proven over its own, with how the second was made.
#[derive(Debug)]
pub struct Owing {
    pub ty: &'static str,
    /// The statement of the callee, over `x` and `y`.
    pub owed: String,
    /// The statement the caller tests, over its `x` and `y`, where it calls with
    /// the two swapped, so that what is owed there is `owed` with them swapped.
    pub tested: String,
    /// Whether the test says what is owed or more, which proves it.
    pub proves: bool,
}

pub fn owing(g: &mut Gen) -> Owing {
    let ty = room(g);
    let (owed, _): Condition = condition(g, 1, true);
    let (other, _): Condition = condition(g, 1, true);
    let there = swapped(&owed);
    let (tested, proves) = match g.below(4) {
        0 => (other, false),
        1 => (there.clone(), true),
        2 => (format!("({there}) and ({other})"), true),
        _ => (format!("({other}) or ({there})"), false),
    };
    Owing {
        ty: ty.name,
        owed,
        tested,
        proves,
    }
}

fn accepted(source: &str) -> bool {
    check(source.as_bytes()).is_ok()
}

fn lowered(source: &str) -> smith::scalar::Program {
    lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for\n{source}"))
}

/// The programs a call is written in: with the statement as a proof parameter
/// that the call passes the proof of a `check` for, as a `requires`, and with
/// the proof taken out by hand.
fn calls(o: &Owing) -> [String; 3] {
    let t = o.ty;
    let (owed, tested) = (&o.owed, &o.tested);
    let main = "fn main() <console, alloc> {\n  print(\"{f(1, 2)}\")\n}\n";
    [
        with_header(format!(
            "fn g(x: {t}, y: {t}, p: {owed}) -> {t} {{ x }}\n\nfn f(x: {t}, y: {t}) -> {t} {{\n  match check({tested}) {{\n    .some(q) {{ g(y, x, q) }}\n    .none {{ 0 }}\n  }}\n}}\n\n{main}"
        )),
        with_header(format!(
            "fn g(x: {t}, y: {t}) -> {t}\n  requires {owed}\n{{ x }}\n\nfn f(x: {t}, y: {t}) -> {t} {{\n  if {tested} {{ g(y, x) }} else {{ 0 }}\n}}\n\n{main}"
        )),
        with_header(format!(
            "fn g(x: {t}, y: {t}) -> {t} {{ x }}\n\nfn f(x: {t}, y: {t}) -> {t} {{\n  if {tested} {{ g(y, x) }} else {{ 0 }}\n}}\n\n{main}"
        )),
    ]
}

/// For every statement a callee takes a proof of and every statement a caller
/// tests, the call that passes the proof of the test compiles exactly when the
/// one that states the statement in a `requires` does, which is when the test
/// says what is owed with the arguments put in, and the executable is the one
/// that has no proof in it.
fn a_call_compiles_exactly_when_the_proof_passed_is_of_the_statement_with_the_arguments_put_in() {
    generate::check(131, 80, owing, |o| {
        let [proof, requires, bare] = calls(o);
        let ok = accepted(&requires);
        assert_eq!(accepted(&proof), ok, "for\n{proof}\nand\n{requires}");
        if o.proves {
            assert!(ok, "for\n{requires}");
        }
        // What the proof passed says is what counts, and not what else is known
        // where the call is: the statement owed is a fact in the outer test, and
        // is no help to a proof of another statement.
        let (t, owed, tested) = (o.ty, &o.owed, &o.tested);
        let outer = swapped(owed);
        let nested = with_header(format!(
            "fn g(x: {t}, y: {t}, p: {owed}) -> {t} {{ x }}\n\nfn f(x: {t}, y: {t}) -> {t} {{\n  if {outer} {{\n    match check({tested}) {{\n      .some(q) {{ g(y, x, q) }}\n      .none {{ 0 }}\n    }}\n  }} else {{ 0 }}\n}}\n\nfn main() <console, alloc> {{\n  print(\"{{f(1, 2)}}\")\n}}\n"
        ));
        assert_eq!(accepted(&nested), ok, "for\n{nested}\nand\n{requires}");
        if ok {
            assert_eq!(lowered(&proof), lowered(&bare), "for\n{proof}\nand\n{bare}");
        }
    });
}

/// The same with the proof left out where the parameter says `= arith`, which
/// is built from what is known where the call is written.
fn a_proof_left_out_where_arith_is_the_default_is_built_from_what_is_known() {
    generate::check(132, 80, owing, |o| {
        let t = o.ty;
        let (owed, tested) = (&o.owed, &o.tested);
        let main = "fn main() <console, alloc> {\n  print(\"{f(1, 2)}\")\n}\n";
        let program = |callee: &str, call: &str| {
            with_header(format!(
                "{callee}\n\nfn f(x: {t}, y: {t}) -> {t} {{\n  if {tested} {{ {call} }} else {{ 0 }}\n}}\n\n{main}"
            ))
        };
        let defaulted = program(
            &format!("fn g(x: {t}, y: {t}, p: {owed} = arith) -> {t} {{ x }}"),
            "g(y, x)",
        );
        let named = program(
            &format!("fn g(x: {t}, y: {t}, _: {owed} = arith) -> {t} {{ x }}"),
            "g(y, x)",
        );
        let requires = program(
            &format!("fn g(x: {t}, y: {t}) -> {t}\n  requires {owed}\n{{ x }}"),
            "g(y, x)",
        );
        let bare = program(&format!("fn g(x: {t}, y: {t}) -> {t} {{ x }}"), "g(y, x)");
        let ok = accepted(&requires);
        assert_eq!(accepted(&defaulted), ok, "for\n{defaulted}\nand\n{requires}");
        assert_eq!(accepted(&named), ok, "for\n{named}\nand\n{requires}");
        if ok {
            assert_eq!(lowered(&defaulted), lowered(&bare), "for\n{defaulted}");
            assert_eq!(lowered(&named), lowered(&bare), "for\n{named}");
        }
        // A call that writes the proof of what it tested passes it for the
        // default, and is accepted when the call that leaves it out is.
        let [written, ..] = calls(o);
        let with_default = written.replace(&format!("p: {owed})"), &format!("p: {owed} = arith)"));
        assert_eq!(accepted(&with_default), accepted(&written), "for\n{with_default}");
    });
}

/// A function that gives its proof back, and the caller that holds the proof it
/// gave, as `c = keep(y, x, q)`, and passes it on, or passes the call itself.
fn a_proof_a_call_gives_is_passed_on_and_the_program_is_the_one_without_proofs() {
    generate::check(133, 80, owing, |o| {
        let t = o.ty;
        let (owed, tested) = (&o.owed, &o.tested);
        let main = "fn main() <console, alloc> {\n  print(\"{f(1, 2)}\")\n}\n";
        let keep = format!("fn keep(x: {t}, y: {t}, p: {owed}) -> {owed} {{ p }}");
        let need = format!("fn need(x: {t}, y: {t}, p: {owed}) -> {t} {{ x }}");
        let held = with_header(format!(
            "{keep}\n\n{need}\n\nfn f(x: {t}, y: {t}) -> {t} {{\n  match check({tested}) {{\n    .some(q) {{\n      c = keep(y, x, q)\n      need(y, x, c)\n    }}\n    .none {{ 0 }}\n  }}\n}}\n\n{main}"
        ));
        let direct = with_header(format!(
            "{keep}\n\n{need}\n\nfn f(x: {t}, y: {t}) -> {t} {{\n  match check({tested}) {{\n    .some(q) {{ need(y, x, keep(y, x, q)) }}\n    .none {{ 0 }}\n  }}\n}}\n\n{main}"
        ));
        let requires = with_header(format!(
            "fn need(x: {t}, y: {t}) -> {t}\n  requires {owed}\n{{ x }}\n\nfn f(x: {t}, y: {t}) -> {t} {{\n  if {tested} {{ need(y, x) }} else {{ 0 }}\n}}\n\n{main}"
        ));
        let bare = with_header(format!(
            "fn need(x: {t}, y: {t}) -> {t} {{ x }}\n\nfn f(x: {t}, y: {t}) -> {t} {{\n  if {tested} {{ need(y, x) }} else {{ 0 }}\n}}\n\n{main}"
        ));
        let ok = accepted(&requires);
        assert_eq!(accepted(&held), ok, "for\n{held}\nand\n{requires}");
        assert_eq!(accepted(&direct), ok, "for\n{direct}\nand\n{requires}");
        if ok {
            assert_eq!(lowered(&held), lowered(&bare), "for\n{held}\nand\n{bare}");
            assert_eq!(lowered(&direct), lowered(&bare), "for\n{direct}\nand\n{bare}");
        }
    });
}

/// What a refused program must name.
fn refused(source: &str, names: &str) {
    let errors = check(source.as_bytes()).expect_err(source);
    assert!(
        errors.iter().any(|e| e.message.contains(names)),
        "{errors:?} does not name `{names}` for\n{source}"
    );
}

/// For every callee, a proof parameter with no default that a call leaves out,
/// a name that is not a proof where one is owed, and a `match` or an `if` on a
/// proof are each refused with the name of what is wrong.
fn a_proof_that_is_not_passed_or_not_a_proof_or_picked_by_is_refused_with_its_name() {
    generate::check(134, 40, owing, |o| {
        let t = o.ty;
        let owed = &o.owed;
        let head = format!("fn g(x: {t}, y: {t}, proof: {owed}) -> {t} {{ x }}\n\n");
        let main = "fn main() <console, alloc> {\n  print(\"{f(1, 2)}\")\n}\n";
        let program = |body: &str| {
            with_header(format!(
                "{head}fn f(x: {t}, y: {t}) -> {t} {{\n{body}\n}}\n\n{main}"
            ))
        };
        refused(&program("  g(x, y)"), "proof");
        // A number is no proof, which the call is refused for.
        assert!(check(program("  g(x, y, x)").as_bytes()).is_err());
        refused(
            &program(&format!(
                "  match check({}) {{\n    .some(q) {{\n      if q {{ 1 }} else {{ 0 }}\n    }}\n    .none {{ 0 }}\n  }}",
                o.tested
            )),
            "`q`",
        );
        refused(
            &program(&format!(
                "  match check({}) {{\n    .some(q) {{\n      match q {{\n        .some(r) {{ 1 }}\n        .none {{ 0 }}\n      }}\n    }}\n    .none {{ 0 }}\n  }}",
                o.tested
            )),
            "`q`",
        );
    });
}

/// A function that gives a proof carries no effect, so what its body does is
/// steps of a proof and never a call of a function that carries one.
fn a_function_that_gives_a_proof_and_carries_an_effect_is_refused() {
    let source = |step: &str| {
        with_header(format!(
            "fn noisy(n: Nat) -> Nat <console, alloc> {{\n  print(\"{{n}}\")\n  n\n}}\n\nfn keep(a: Nat, p: a > 0) -> a > 0 {{\n{step}\n}}\n\nfn main() <console, alloc> {{\n  print(\"{{1}}\")\n}}\n"
        ))
    };
    assert!(accepted(&source("  p")), "{:?}", check(source("  p").as_bytes()));
    refused(&source("  c = noisy(a)\n  p"), "not a step of a proof");
}

/// `keep` of the page: the proof a `check` bound is given to it, and what it
/// proves is a fact after the call, so the division that needs it is proven.
fn a_proof_of_a_check_is_a_fact_after_a_call_that_gives_it_back() {
    let source = with_header(
        "fn keep(a: Byte, p: a > 0) -> a > 0 { p }\n\nfn use_it(a: Byte) -> Byte {\n  match check(a > 0) {\n    .some(p) {\n      keep(a, p)\n      255 / a\n    }\n    .none { 0 }\n  }\n}\n\nfn main() <console, alloc> {\n  print(\"{use_it(5)}\")\n  print(\"{use_it(0)}\")\n}\n"
            .to_string(),
    );
    let program = lowered(&source);
    let out = smith::scalar::run(&program);
    assert_eq!(out.stdout, b"51\n0\n".to_vec(), "for\n{source}");
}

/// `first` of the page: `xs[0]` is read with no test, as the proof `xs.size > 0`
/// is a parameter, and a proof a function is given is one it passes on.
fn a_proof_parameter_is_a_fact_in_the_function_and_a_proof_to_pass_on() {
    let source = with_header(
        "fn first(xs: List[Byte], p: xs.size > 0) -> Byte { xs[0] }\n\nfn second_first(xs: List[Byte], p: xs.size > 0) -> Byte { first(xs, p) }\n\nfn lone(xs: List[Byte]) -> Byte {\n  match check(xs.size > 0) {\n    .some(q) { second_first(xs, q) }\n    .none { 0 }\n  }\n}\n\nfn main() <console, alloc> {\n  xs: List[Byte] = [7, 8]\n  print(\"{lone(xs)}\")\n  print(\"{first(xs)}\")\n}\n"
            .to_string(),
    );
    // `first(xs)` with no proof passed is refused, as `p` has no default.
    refused(&source, "`p`");
    let program = lowered(&source.replace("  print(\"{first(xs)}\")\n", ""));
    let out = smith::scalar::run(&program);
    assert_eq!(out.stdout, b"7\n".to_vec(), "for\n{source}");
}

/// For every statement of a comparison, the proof a call gives, or is passed,
/// is refused where the argument is a `var`, and accepted where it is a name
/// that cannot change.
fn a_proof_of_a_statement_that_names_a_var_is_refused_with_the_name_of_the_var() {
    fn comparison(g: &mut Gen) -> (&'static str, usize, bool) {
        (
            *g.pick(&["<", "<=", ">", ">=", "==", "!="]),
            1 + g.below(8),
            g.one_in(2),
        )
    }
    generate::check(135, 40, comparison, |&(op, n, giving)| {
        // A proof a call gives is the statement of the call, and a proof a call
        // is passed is the statement of the parameter, which the value is put in.
        let (keep, drop) = if giving {
            (format!("fn keep(a: Nat, p: a {op} {n}) -> a {op} {n} {{ p }}"), "")
        } else {
            (format!("fn keep(a: Nat, p: a {op} {n}) -> Nat {{ a }}"), "_ = ")
        };
        let source = |bound: &str, passed: &str| {
            with_header(format!(
                "{keep}\n\nfn f(m: Nat) -> Nat {{\n  {bound}\n  match check(m {op} {n}) {{\n    .some(p) {{\n      {drop}keep({passed}, p)\n      1\n    }}\n    .none {{ 0 }}\n  }}\n}}\n\nfn main() <console, alloc> {{\n  print(\"{{f(1)}}\")\n}}\n"
            ))
        };
        refused(&source("var n = m", "n"), "`n`");
        assert!(
            accepted(&source("n = m", "m")),
            "{:?}",
            check(source("n = m", "m").as_bytes())
        );
    });
}

/// A proof function whose steps are refused is reported, whether or not code
/// calls it and binds what it gives, and the program is never lowered.
fn a_proof_function_that_is_refused_is_one_error_and_never_reaches_the_lowering() {
    let source = with_header(
        "fn more(a: Nat, p: a > 3) -> a > 2 { arith(p) }\n\nfn need(a: Nat) -> Nat\n  requires a > 2\n{ a }\n\nfn f(a: Nat) -> Nat {\n  match check(a > 3) {\n    .some(p) {\n      q = more(a, p)\n      need(a)\n    }\n    .none { 0 }\n  }\n}\n\nfn main() <console, alloc> {\n  print(\"{f(5)}\")\n}\n"
            .to_string(),
    );
    refused(&source, "`arith`");
    assert!(lower(source.as_bytes()).is_err());
}

/// A function that gives a proof lists no effect, and is told why.
fn a_function_that_gives_a_proof_with_an_effect_listed_is_refused_saying_why() {
    let source = with_header(
        "fn bad(a: Nat, p: a > 0) -> a > 0 <console> { p }\n\nfn main() <console, alloc> {\n  print(\"{1}\")\n}\n"
            .to_string(),
    );
    refused(&source, "carries no effect");
}

/// A proof a call gave and bound to a name is a fact `arith` uses, so the name
/// is read where the body asks `arith` for something, and is never read in a
/// body that asks it for nothing.
fn a_name_bound_to_a_proof_is_read_when_arith_may_use_it_and_not_otherwise() {
    let source = |tail: &str| {
        with_header(format!(
            "fn more(a: Nat, p: a == 3) -> a + 1 == 4 {{ arith(p) }}\n\nfn need(a: Nat) -> Nat\n  requires a > 2\n{{ a }}\n\nfn f(a: Nat) -> Nat {{\n  match check(a == 3) {{\n    .some(p) {{\n      q = more(a, p)\n      {tail}\n    }}\n    .none {{ 0 }}\n  }}\n}}\n\nfn main() <console, alloc> {{\n  print(\"{{f(3)}}\")\n}}\n"
        ))
    };
    let used = source("need(a)");
    let warnings = check(used.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for\n{used}"));
    assert!(warnings.is_empty(), "{warnings:?} for\n{used}");
    let out = smith::scalar::run(&lowered(&used));
    assert_eq!(out.stdout, b"3\n".to_vec());
    let unused = source("7");
    let warnings = check(unused.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for\n{unused}"));
    assert!(
        warnings.iter().any(|w| w.message.contains("`q`")),
        "{warnings:?} for\n{unused}"
    );
}
