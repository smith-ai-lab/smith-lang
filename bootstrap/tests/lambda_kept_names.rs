//! A lambda that reads names of the function it is written in, stored in a field
//! or returned, holds the values of those names, as the fields of the value that
//! holds it, and costs nothing: a call of it is the body of the lambda, written
//! where it is called. One that holds something lent, assigns a name of its
//! function, or needs an effect is refused where it is kept.
//!
//! The generator builds the program that holds the lambda and what it prints from
//! the same choices, so the model of each is the other.

use smith::target::Target;
use smith::{check, compile, lower, scalar};

use crate::Test;
use crate::generate::{self, Gen};

pub const ONCE: &[Test] = &[
    (
        "a_lambda_that_reads_names_and_is_kept_gives_what_its_body_gives_for_the_values_it_read",
        a_lambda_that_reads_names_and_is_kept_gives_what_its_body_gives_for_the_values_it_read,
    ),
    (
        "a_lambda_that_reads_names_and_is_stored_builds_to_the_bytes_of_its_body_written_where_it_is_called",
        a_lambda_that_reads_names_and_is_stored_builds_to_the_bytes_of_its_body_written_where_it_is_called,
    ),
    (
        "a_lambda_that_reads_names_is_one_type_that_holds_the_values_each_call_of_its_function_gives_it",
        a_lambda_that_reads_names_is_one_type_that_holds_the_values_each_call_of_its_function_gives_it,
    ),
    (
        "a_lambda_that_reads_names_of_a_function_with_type_parameters_is_kept_for_each_type",
        a_lambda_that_reads_names_of_a_function_with_type_parameters_is_kept_for_each_type,
    ),
    (
        "a_lambda_that_holds_something_lent_or_assigns_a_name_is_refused_where_it_is_kept",
        a_lambda_that_holds_something_lent_or_assigns_a_name_is_refused_where_it_is_kept,
    ),
    (
        "a_lambda_that_holds_something_lent_is_not_given_by_the_name_it_is_bound_to",
        a_lambda_that_holds_something_lent_is_not_given_by_the_name_it_is_bound_to,
    ),
    (
        "a_lambda_that_reads_names_and_needs_an_effect_is_refused_where_it_is_kept",
        a_lambda_that_reads_names_and_needs_an_effect_is_refused_where_it_is_kept,
    ),
    (
        "a_lambda_that_reads_names_is_kept_as_the_function_type_that_holds_it_says",
        a_lambda_that_reads_names_is_kept_as_the_function_type_that_holds_it_says,
    ),
    (
        "which_lambda_that_reads_names_a_value_is_is_known_when_the_program_is_built",
        which_lambda_that_reads_names_a_value_is_is_known_when_the_program_is_built,
    ),
    (
        "a_lambda_that_is_returned_reads_the_parameters_of_its_function_only",
        a_lambda_that_is_returned_reads_the_parameters_of_its_function_only,
    ),
];

/// How a program holds the lambda it calls.
#[derive(Clone, Copy, Debug)]
enum Hold {
    /// In a field of a struct, built where the names are.
    Field,
    /// In a field that is a type parameter of the struct.
    Step,
    /// Returned, and called by the call that gives it.
    Return,
    /// Returned, bound to a name, and called by it.
    BindTheReturn,
    /// In a struct that a function builds, given to a function that calls it.
    Made,
    /// In a struct that a struct holds.
    Nested,
}

/// What the generator chose: how the lambda is held, the values of the names it
/// reads, and what it is called with.
#[derive(Debug)]
struct Program {
    hold: Hold,
    reads: Vec<u8>,
    argument: u8,
}

fn program_of(g: &mut Gen, holds: &[Hold]) -> Program {
    let names = 1 + g.below(3);
    Program {
        hold: *g.pick(holds),
        reads: (0..names).map(|_| g.below(100) as u8).collect(),
        argument: g.below(100) as u8,
    }
}

impl Program {
    const HEAD: &'static str = "use std.console (console, print)\nuse std.int (wrapping_add)\n\n\
                                struct Held {\n  run: fn(Int) -> Int\n}\n\n\
                                struct Step[F] {\n  run: F\n}\n\n\
                                struct Shop {\n  held: Held\n}\n\n";

    /// The parameters of the function that makes the lambda, which are the names it reads.
    fn params(&self) -> String {
        (0..self.reads.len()).map(|i| format!("k{i}: Int")).collect::<Vec<_>>().join(", ")
    }

    /// The values of those names, as the arguments of a call of that function.
    fn values(&self) -> String {
        self.reads.iter().map(u8::to_string).collect::<Vec<_>>().join(", ")
    }

    /// The body of the lambda, for the name it calls its parameter, with the
    /// names it reads written as `name`.
    fn body(&self, x: &str, name: &dyn Fn(usize) -> String) -> String {
        let mut sum = x.to_string();
        for i in 0..self.reads.len() {
            sum = format!("wrapping_add({sum}, {})", name(i));
        }
        sum
    }

    fn lambda(&self) -> String {
        format!("fn(x: Int) -> Int {{ {} }}", self.body("x", &|i| format!("k{i}")))
    }

    /// The names the lambda reads, defined where it is written.
    fn locals(&self) -> String {
        (self.reads.iter().enumerate())
            .map(|(i, v)| format!("  k{i}: Int = {v}\n"))
            .collect()
    }

    fn main(body: &str) -> String {
        format!("fn main() <console, alloc> {{\n{body}  print(\"{{r}}\")\n}}\n")
    }

    /// The program that holds the lambda in the way `hold` says.
    fn kept(&self) -> String {
        let (lambda, x) = (self.lambda(), self.argument);
        let (params, values) = (self.params(), self.values());
        let mut text = String::from(Self::HEAD);
        match self.hold {
            Hold::Field => {
                text += &Self::main(&format!("{}  h = Held(run: {lambda})\n  r = h.run({x})\n", self.locals()));
            }
            Hold::Step => {
                text += &Self::main(&format!("{}  s = Step(run: {lambda})\n  r = s.run({x})\n", self.locals()));
            }
            Hold::Return => {
                text += &format!("fn make({params}) -> fn(Int) -> Int {{ {lambda} }}\n\n");
                text += &Self::main(&format!("  r = make({values})({x})\n"));
            }
            Hold::BindTheReturn => {
                text += &format!("fn make({params}) -> fn(Int) -> Int {{ {lambda} }}\n\n");
                text += &Self::main(&format!("  f = make({values})\n  r = f({x})\n"));
            }
            Hold::Made => {
                text += &format!("fn made({params}) -> Held {{ Held(run: {lambda}) }}\n\n");
                text += "fn priced(h: Held, x: Int) -> Int { h.run(x) }\n\n";
                text += &Self::main(&format!("  h = made({values})\n  r = priced(h, {x})\n"));
            }
            Hold::Nested => {
                text += &format!("fn made({params}) -> Held {{ Held(run: {lambda}) }}\n\n");
                text += "fn sold(s: Shop, x: Int) -> Int {\n  h = s.held\n  h.run(x)\n}\n\n";
                text += &Self::main(&format!("  s = Shop(held: made({values}))\n  r = sold(s, {x})\n"));
            }
        }
        text
    }

    /// The program that writes the body of the lambda where it is called, for the
    /// holds that are written in `main`, where the names it reads are.
    fn written_where_called(&self) -> String {
        let body = self.body(&self.argument.to_string(), &|i| format!("k{i}"));
        let mut text = String::from(Self::HEAD);
        text += &Self::main(&format!("{}  r = {body}\n", self.locals()));
        text
    }

    fn says(&self) -> String {
        let sum: u64 = self.reads.iter().map(|r| u64::from(*r)).sum::<u64>() + u64::from(self.argument);
        format!("{sum}\n")
    }
}

const ALL: &[Hold] = &[
    Hold::Field,
    Hold::Step,
    Hold::Return,
    Hold::BindTheReturn,
    Hold::Made,
    Hold::Nested,
];

fn a_lambda_that_reads_names_and_is_kept_gives_what_its_body_gives_for_the_values_it_read() {
    generate::check(4421, 12, |g: &mut Gen| program_of(g, ALL), |p| {
        let kept = p.kept();
        let ran = scalar::run(&lower(kept.as_bytes()).unwrap_or_else(|e| panic!("{e:?}\n{kept}")));
        assert_eq!(ran.stdout, p.says().as_bytes(), "for\n{kept}");
    });
}

/// A lambda stored where the names it reads are is lowered where it is called,
/// and the struct that holds it has no bytes of its own.
fn a_lambda_that_reads_names_and_is_stored_builds_to_the_bytes_of_its_body_written_where_it_is_called() {
    generate::check(4422, 3, |g: &mut Gen| program_of(g, &[Hold::Field, Hold::Step]), |p| {
        let (kept, written) = (p.kept(), p.written_where_called());
        if let Err(found) = check(kept.as_bytes()) {
            panic!("{found:?} for\n{kept}");
        }
        for &target in Target::ALL {
            let held = compile(kept.as_bytes(), target).unwrap_or_else(|e| panic!("{e:?}\n{kept}"));
            let said = compile(written.as_bytes(), target).expect("the body written in place builds");
            assert_eq!(held, said, "for\n{kept}\nand\n{written}");
        }
    });
}

/// The values that a lambda holds are those of the call of its function that
/// gave it, so a function that makes one gives each call its own.
fn a_lambda_that_reads_names_is_one_type_that_holds_the_values_each_call_of_its_function_gives_it() {
    let source = "use std.console (console, print)\n\n\
                  struct Pricing {\n  discount: fn(f32) -> f32\n}\n\n\
                  fn scaler(k: f32) -> fn(f32) -> f32 {\n  fn(x: f32) -> f32 { x * k }\n}\n\n\
                  fn sale(rate: f32) -> Pricing {\n  Pricing(discount: fn(v: f32) -> f32 { v - rate })\n}\n\n\
                  fn priced(p: Pricing, x: f32) -> f32 { p.discount(x) }\n\n\
                  fn main() <console, alloc> {\n  a = sale(1.0)\n  b = sale(3.0)\n  \
                  print(\"{scaler(3.0)(2.0)} {priced(a, 5.0)} {priced(b, 5.0)} {b.discount(5.0)}\")\n}\n";
    let ran = scalar::run(&lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?}")));
    assert_eq!(ran.stdout, b"6.0 4.0 2.0 2.0\n");
}

/// A lambda that reads a value whose type is a type parameter of its function is
/// kept at each type the function is called at.
fn a_lambda_that_reads_names_of_a_function_with_type_parameters_is_kept_for_each_type() {
    let source = "use std.console (console, print)\n\n\
                  fn first[T: Copy](a: T) -> fn(T) -> T {\n  fn(b: T) -> T { a }\n}\n\n\
                  fn main() <console, alloc> {\n  print(\"{first(1)(2)} {first(1.5)(2.5)}\")\n}\n";
    let ran = scalar::run(&lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?}")));
    assert_eq!(ran.stdout, b"1 1.5\n");
}

/// The function `kept` that a test refuses a part of, and a `main` that calls it
/// as `how` says, with the call of the lambda it gives or the struct it gives.
fn refused_source(item: &str, how: &str) -> String {
    format!(
        "use std.console (console, print)\n\nstruct Pricing {{\n  discount: fn(f32) -> f32\n}}\n\n{item}\n\
         fn main() <console, alloc> {{\n  print(\"{{{how}}}\")\n}}\n"
    )
}

/// A lambda that reads a value that is lent, or assigns a name of its function,
/// is refused where it is stored or returned, as it is where it is bound.
fn a_lambda_that_holds_something_lent_or_assigns_a_name_is_refused_where_it_is_kept() {
    for (stored, call) in [(true, "kept([1.0]).discount(1.0)"), (false, "kept([1.0])(1.0)")] {
        let (open, close) = if stored { ("Pricing(discount: ", ")") } else { ("", "") };
        let result = if stored { "Pricing" } else { "fn(f32) -> f32" };
        // Lent: a list the function is lent.
        let lent = format!(
            "fn kept(xs: List[f32]) -> {result} {{\n  {open}fn(v: f32) -> f32 {{ xs.size }}{close}\n}}\n"
        );
        // Assigned: a `var` of the function.
        let assigned = format!(
            "fn kept(xs: List[f32]) -> {result} {{\n  var m: f32 = 1.0\n  {open}fn(v: f32) -> f32 {{\n    m = m + v\n    m\n  }}{close}\n}}\n"
        );
        for item in [lent, assigned] {
            let source = refused_source(&item, call);
            let found = check(source.as_bytes()).expect_err("refused");
            assert!(
                found.iter().any(|e| e.message.starts_with("a lambda cannot be")),
                "{found:?} for\n{source}"
            );
        }
    }
}

/// A lambda that is bound may hold what is lent, as it is called in the body it
/// is written in, and so it is not returned, or stored in what is returned, by
/// the name it is bound to.
fn a_lambda_that_holds_something_lent_is_not_given_by_the_name_it_is_bound_to() {
    for (result, give) in [("fn(f32) -> f32", "f"), ("Pricing", "Pricing(discount: f)")] {
        let item = format!(
            "fn kept(xs: List[f32]) -> {result} {{\n  f = fn(v: f32) -> f32 {{ v * float(xs.size) }}\n  {give}\n}}\n"
        );
        let call = if result == "Pricing" { "kept([1.0]).discount(1.0)" } else { "kept([1.0])(1.0)" };
        let source = refused_source(&item, call);
        assert!(check(source.as_bytes()).is_err(), "accepted\n{source}");
        // Called where it is bound, it holds what is lent and is accepted.
        let called = refused_source(
            "fn kept(xs: List[f32]) -> f32 {\n  f = fn(v: f32) -> f32 { v * float(xs.size) }\n  f(2.0)\n}\n",
            "kept([1.0])",
        );
        assert!(check(called.as_bytes()).is_ok(), "{:?}\n{called}", check(called.as_bytes()));
    }
}

/// A function type states no effect, so a lambda that needs one is refused where
/// it is stored or returned, and not where it is called, whose function declares it.
fn a_lambda_that_reads_names_and_needs_an_effect_is_refused_where_it_is_kept() {
    for (open, close, result, call) in [
        ("Pricing(discount: ", ")", "Pricing", "kept(1.0).discount(1.0)"),
        ("", "", "fn(f32) -> f32", "kept(1.0)(1.0)"),
    ] {
        let item = format!(
            "fn kept(rate: f32) -> {result} {{\n  {open}fn(v: f32) -> f32 {{\n    print(\"x\")\n    v - rate\n  }}{close}\n}}\n"
        );
        let source = refused_source(&item, call);
        let found = check(source.as_bytes()).expect_err("refused");
        assert!(
            found.iter().any(|e| e.message.contains("`console` effect") && e.message.contains("states none")),
            "{found:?} for\n{source}"
        );
    }
}

/// A lambda is of the function type that the field it is stored in, or the
/// result it is given as, is written with, and takes its types from it when it
/// leaves them out.
fn a_lambda_that_reads_names_is_kept_as_the_function_type_that_holds_it_says() {
    let item = "fn kept(rate: f32) -> Pricing {\n  Pricing(discount: fn(v: f32) -> bool { v > rate })\n}\n";
    let source = refused_source(item, "kept(1.0).discount(1.0)");
    let found = check(source.as_bytes()).expect_err("refused");
    assert!(found.iter().any(|e| e.message.contains("is needed")), "{found:?}");
    let bare = "fn kept(rate: f32) -> Pricing {\n  Pricing(discount: fn(v) { v - rate })\n}\n";
    let ran = scalar::run(&lower(refused_source(bare, "kept(1.0).discount(3.0)").as_bytes()).unwrap_or_else(|e| panic!("{e:?}")));
    assert_eq!(ran.stdout, b"2.0\n");
}

/// Two different lambdas where one type is needed are refused, as two functions
/// with names are, and a function that returns one of two by a branch is.
fn which_lambda_that_reads_names_a_value_is_is_known_when_the_program_is_built() {
    let head = "struct Pricing {\n  discount: fn(f32) -> f32\n}\n\n";
    let pick = format!(
        "{head}fn pick(wide: bool, rate: f32) -> Pricing {{\n  \
         if wide {{ Pricing(discount: fn(v: f32) -> f32 {{ v * rate }}) }} \
         else {{ Pricing(discount: fn(v: f32) -> f32 {{ v + rate }}) }}\n}}\n\n\
         fn main() {{\n  p = pick(true, 2.0)\n  r = p.discount(1.0)\n}}\n"
    );
    let found = check(pick.as_bytes()).expect_err("refused");
    assert!(
        found.iter().any(|e| e.message.contains("must be known when the program is built")),
        "{found:?}"
    );
}

/// What a function gives is known before its body is checked, so a lambda that
/// it returns reads the parameters of the function, and one that reads a name the
/// function defines is refused saying which.
fn a_lambda_that_is_returned_reads_the_parameters_of_its_function_only() {
    let source = "fn kept(rate: f32) -> fn(f32) -> f32 {\n  doubled = rate * 2.0\n  \
                  fn(v: f32) -> f32 { v - doubled }\n}\n\nfn main() {\n  r = kept(1.0)(2.0)\n}\n";
    let found = check(source.as_bytes()).expect_err("refused");
    assert!(found.iter().any(|e| e.message.contains("`doubled`")), "{found:?}");
}
