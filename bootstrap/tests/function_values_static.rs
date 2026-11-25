//! A function with a name is bound, stored in a field, and returned, and which
//! function a value of a function type is must be known when the program is
//! built. So each of these builds to the bytes of the call written to the
//! function directly, and a value that could be one of two functions is
//! refused.
//!
//! The generator builds the program that uses the function as a value and the
//! program that calls it directly from the same choices, so the model of each
//! is the other.

use smith::target::Target;
use smith::{check, compile, lower, scalar};

use crate::Test;
use crate::generate::{self, Gen};

pub const ONCE: &[Test] = &[
    (
        "a_function_that_is_bound_builds_to_the_bytes_of_the_call_written_directly",
        || builds_to_the_bytes_of_the_call_written_directly(&[Hold::Bind, Hold::BindThenPass]),
    ),
    (
        "a_function_that_is_stored_builds_to_the_bytes_of_the_call_written_directly",
        || builds_to_the_bytes_of_the_call_written_directly(&[Hold::Field]),
    ),
    (
        "a_function_that_is_returned_builds_to_the_bytes_of_the_call_written_directly",
        || builds_to_the_bytes_of_the_call_written_directly(&[Hold::Return, Hold::BindTheReturn]),
    ),
    (
        "a_function_that_a_function_takes_in_a_struct_builds_to_the_bytes_of_the_call_written_directly",
        || builds_to_the_bytes_of_the_call_written_directly(&[Hold::Takes]),
    ),
    (
        "a_function_that_a_struct_in_a_struct_holds_builds_to_the_bytes_of_the_call_written_directly",
        || builds_to_the_bytes_of_the_call_written_directly(&[Hold::Nested]),
    ),
    (
        "a_function_that_a_struct_a_function_gives_holds_builds_to_the_bytes_of_the_call_written_directly",
        || builds_to_the_bytes_of_the_call_written_directly(&[Hold::Made, Hold::Wraps]),
    ),
    (
        "a_function_that_is_returned_and_passed_builds_to_the_bytes_of_the_call_written_directly",
        || builds_to_the_bytes_of_the_call_written_directly(&[Hold::CallTheReturn]),
    ),
    (
        "a_lambda_that_is_bound_stored_or_returned_builds_to_the_bytes_of_the_function_with_its_body",
        || {
            a_lambda_builds_to_the_bytes_of_the_call_written_directly(&[
                Hold::Bind,
                Hold::BindThenPass,
                Hold::Field,
                Hold::Return,
                Hold::BindTheReturn,
                Hold::CallTheReturn,
                Hold::Takes,
                Hold::Nested,
                Hold::Made,
            ])
        },
    ),
    (
        "a_lambda_that_leaves_its_types_out_where_a_function_type_decides_them_builds_to_the_bytes_of_the_one_that_writes_them",
        || {
            a_lambda_that_leaves_its_types_out_builds_to_the_bytes_of_the_call_written_directly(&[
                Hold::Field,
                Hold::Return,
                Hold::BindTheReturn,
                Hold::CallTheReturn,
                Hold::Takes,
                Hold::Nested,
                Hold::Made,
            ])
        },
    ),
    (
        "a_lambda_that_leaves_a_type_out_where_nothing_decides_it_is_refused_where_to_write_it",
        a_lambda_that_leaves_a_type_out_where_nothing_decides_it_is_refused_where_to_write_it,
    ),
    (
        "a_choice_between_lambdas_bound_to_a_name_is_refused_as_a_choice_between_functions",
        a_choice_between_lambdas_bound_to_a_name_is_refused_as_a_choice_between_functions,
    ),
    (
        "a_lambda_that_holds_something_lent_or_needs_an_effect_is_refused_where_it_is_kept",
        a_lambda_that_holds_something_lent_or_needs_an_effect_is_refused_where_it_is_kept,
    ),
    (
        "which_lambda_a_value_is_is_known_when_the_program_is_built",
        which_lambda_a_value_is_is_known_when_the_program_is_built,
    ),
    (
        "a_lambda_that_writes_type_parameters_is_called_with_each_type",
        a_lambda_that_writes_type_parameters_is_called_with_each_type,
    ),
    (
        "every_accepted_program_that_holds_named_functions_prints_what_calling_them_directly_prints",
        every_accepted_program_that_holds_named_functions_prints_what_calling_them_directly_prints,
    ),
    (
        "a_function_that_needs_an_effect_is_not_stored_in_a_field_or_given_as_a_result",
        a_function_that_needs_an_effect_is_not_stored_in_a_field_or_given_as_a_result,
    ),
    (
        "a_value_that_could_be_one_of_two_functions_is_one_error_at_the_second",
        a_value_that_could_be_one_of_two_functions_is_one_error_at_the_second,
    ),
];

/// How a program holds the function it calls.
#[derive(Clone, Copy, Debug)]
enum Hold {
    Bind,
    BindThenPass,
    Field,
    Return,
    BindTheReturn,
    CallTheReturn,
    /// Given to a function that takes the struct that holds it.
    Takes,
    /// Held by a struct that a struct holds.
    Nested,
    /// Made by a function that gives the struct that holds it.
    Made,
    /// Held by a struct that a function builds from the function it is given.
    Wraps,
}

#[derive(Debug)]
struct Program {
    /// The function that is held, among those the file defines.
    held: usize,
    /// What each function adds.
    adds: [u8; 3],
    argument: u8,
    hold: Hold,
    /// Whether the function is a lambda written where it is held.
    lambda: bool,
    /// Whether that lambda leaves out its types.
    bare: bool,
}

fn program_of(g: &mut Gen, holds: &[Hold], lambda: bool, bare: bool) -> Program {
    Program {
        lambda,
        bare,
        held: g.below(3),
        adds: [g.below(200) as u8, g.below(200) as u8, g.below(200) as u8],
        argument: g.below(50) as u8,
        hold: *g.pick(holds),
    }
}

impl Program {
    /// The function that is held, as a function with a name or as the lambda
    /// that is written with the same body.
    fn function(&self) -> String {
        if self.bare {
            format!("fn(x) {{ wrapping_add(x, {}) }}", self.adds[self.held])
        } else if self.lambda {
            format!("fn(x: Int) -> Int {{ wrapping_add(x, {}) }}", self.adds[self.held])
        } else {
            format!("f{}", self.held)
        }
    }

    fn head(&self) -> String {
        let mut text = String::from("use std.console (console, print)\nuse std.int (wrapping_add)\n\n");
        for (k, add) in self.adds.iter().enumerate() {
            text += &format!("fn f{k}(x: Int) -> Int {{ wrapping_add(x, {add}) }}\n");
        }
        text += "\nfn twice(x: Int, f: fn(Int) -> Int) -> Int { f(f(x)) }\n\n";
        text
    }

    fn main(body: &str) -> String {
        format!("fn main() <console, alloc> {{\n{body}  print(\"{{r}}\")\n}}\n")
    }

    /// The program that holds the function in the way `hold` says.
    fn as_value(&self) -> String {
        let x = self.argument;
        let fv = self.function();
        let mut text = self.head();
        match self.hold {
            Hold::Bind => text += &Self::main(&format!("  f = {fv}\n  r = f({x})\n")),
            Hold::BindThenPass => text += &Self::main(&format!("  f = {fv}\n  r = twice({x}, f)\n")),
            Hold::Field => {
                text += "struct Held {\n  run: fn(Int) -> Int\n}\n\n";
                text += &Self::main(&format!("  h = Held(run: {fv})\n  r = h.run({x})\n"));
            }
            Hold::Return => {
                text += &format!("fn chosen() -> fn(Int) -> Int {{ {fv} }}\n\n");
                text += &Self::main(&format!("  r = chosen()({x})\n"));
            }
            Hold::BindTheReturn => {
                text += &format!("fn chosen() -> fn(Int) -> Int {{ {fv} }}\n\n");
                text += &Self::main(&format!("  f = chosen()\n  r = f({x})\n"));
            }
            Hold::CallTheReturn => {
                text += &format!("fn chosen() -> fn(Int) -> Int {{ {fv} }}\n\n");
                text += &Self::main(&format!("  r = twice({x}, chosen())\n"));
            }
            Hold::Takes => {
                text += "struct Held {\n  run: fn(Int) -> Int\n}\n\n";
                text += "fn priced(h: Held, x: Int) -> Int { h.run(x) }\n\n";
                text += &Self::main(&format!("  a = Held(run: {fv})\n  r = priced(a, {x})\n"));
            }
            Hold::Nested => {
                text += "struct Held {\n  run: fn(Int) -> Int\n}\n\n";
                text += "struct Shop {\n  held: Held\n}\n\n";
                text += "fn sold(s: Shop, x: Int) -> Int {\n  h = s.held\n  h.run(x)\n}\n\n";
                text += &Self::main(&format!(
                    "  s = Shop(held: Held(run: {fv}))\n  r = sold(s, {x})\n"
                ));
            }
            Hold::Wraps => {
                text += "struct Held {\n  run: fn(Int) -> Int\n}\n\n";
                text += "fn wrap(f: fn(Int) -> Int) -> Held { Held(run: f) }\n\n";
                text += &Self::main(&format!("  r = wrap({fv}).run({x})\n"));
            }
            Hold::Made => {
                text += "struct Held {\n  run: fn(Int) -> Int\n}\n\n";
                text += &format!("fn made() -> Held {{ Held(run: {fv}) }}\n\n");
                text += &Self::main(&format!("  h = made()\n  r = h.run({x})\n"));
            }
        }
        text
    }

    /// The program that calls the function by its name.
    fn called(&self) -> String {
        let (k, x) = (self.held, self.argument);
        let mut text = self.head();
        let call = match self.hold {
            Hold::BindThenPass | Hold::CallTheReturn => format!("twice({x}, f{k})"),
            _ => format!("f{k}({x})"),
        };
        text += &Self::main(&format!("  r = {call}\n"));
        text
    }
}

fn builds_to_the_bytes_of_the_call_written_directly(holds: &[Hold]) {
    builds_to_the_bytes(holds, false, false);
}

/// The same, where the function is a lambda written where it is held.
fn a_lambda_builds_to_the_bytes_of_the_call_written_directly(holds: &[Hold]) {
    builds_to_the_bytes(holds, true, false);
}

/// The same, where the lambda leaves its types out.
fn a_lambda_that_leaves_its_types_out_builds_to_the_bytes_of_the_call_written_directly(holds: &[Hold]) {
    builds_to_the_bytes(holds, true, true);
}

fn builds_to_the_bytes(holds: &[Hold], lambda: bool, bare: bool) {
    generate::check(4301, 3, |g: &mut Gen| program_of(g, holds, lambda, bare), |p| {
        let (value, direct) = (p.as_value(), p.called());
        if let Err(found) = check(value.as_bytes()) {
            panic!("{found:?} for\n{value}");
        }
        for &target in Target::ALL {
            let held = compile(value.as_bytes(), target).unwrap_or_else(|e| panic!("{e:?}\n{value}"));
            let said = compile(direct.as_bytes(), target).expect("the direct call builds");
            assert_eq!(held, said, "for\n{value}\nand\n{direct}");
        }
    });
}

/// Where a value could be one of two functions: the arms of an `if`, two
/// elements of a list, a `var` given one and then another, and a function that
/// returns one by a branch.
fn a_value_that_could_be_one_of_two_functions_is_one_error_at_the_second() {
    let head = "use std.console (console, print)\n\n\
                fn half(x: f32) -> f32 { x * 0.5 }\n\
                fn dbl(x: f32) -> f32 { x * 2.0 }\n\n\
                struct Held {\n  run: fn(f32) -> f32\n}\n\n";
    // Each case is what comes before the second value, the second value, and what follows.
    let cases = [
        ("  p = if true { Held(run: half) } else { ", "Held(run: dbl)", " }\n  r = p.run(1.0)\n"),
        ("  ps = [Held(run: half), ", "Held(run: dbl)", "]\n  r = ps.size\n"),
        ("  var f = half\n  f = ", "dbl", "\n  r = f(1.0)\n"),
    ];
    for (before, second, after) in cases {
        let source = format!(
            "{head}fn main() <console, alloc> {{\n{before}{second}{after}  print(\"{{r}}\")\n}}\n"
        );
        let at = crate::programs::pos_after(&format!("{head}fn main() <console, alloc> {{\n{before}"));
        let found = check(source.as_bytes()).expect_err("refused");
        assert_eq!(found.len(), 1, "{found:?} for\n{source}");
        assert_eq!(found[0].pos.line, at.line, "{found:?} for\n{source}");
        assert!(
            (at.col..=at.col + second.len()).contains(&found[0].pos.col),
            "{found:?} for\n{source}"
        );
        assert!(
            found[0].message.contains("must be known when the program is built"),
            "{found:?}"
        );
    }
    let branch = format!(
        "{head}fn pick(c: bool) -> fn(f32) -> f32 {{\n  if c {{ half }} else {{ dbl }}\n}}\n\n\
         fn main() <console, alloc> {{\n  f = pick(true)\n  print(\"{{f(1.0)}}\")\n}}\n"
    );
    let found = check(branch.as_bytes()).expect_err("refused");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].message.contains("must be known when the program is built"));
}

/// A variable of a generated program: what it holds, and which of the three
/// functions that is.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    /// A function, as `f = f1`.
    Function,
    /// A `Held`, which holds a function in its field `run`.
    Held,
    /// A `Shop`, which holds a `Held` in its field `held`.
    Shop,
}

/// A program that builds values that hold named functions by each way a
/// function is held, and calls each, with what the calls print.
#[derive(Debug)]
struct Generated {
    adds: [u8; 3],
    /// Whether each function prints, which needs the `console` effect.
    loud: [bool; 3],
    argument: u8,
    source: String,
    printed: Vec<u8>,
    /// Whether a function that needs an effect goes into a type that states
    /// none, which the compiler refuses.
    refused: bool,
}

fn generated(g: &mut Gen) -> Generated {
    let adds = [g.below(100) as u8, g.below(100) as u8, g.below(100) as u8];
    let argument = g.below(50) as u8;
    let loud = [g.below(3) == 0, g.below(3) == 0, g.below(3) == 0];
    let mut refused = false;
    let mut text = String::from("use std.console (console, print)\nuse std.int (wrapping_add)\n\n");
    for (k, add) in adds.iter().enumerate() {
        if loud[k] {
            text += &format!(
                "fn f{k}(x: Int) -> Int <console> {{\n  print(\"side\")\n  wrapping_add(x, {add})\n}}\n"
            );
        } else {
            text += &format!("fn f{k}(x: Int) -> Int {{ wrapping_add(x, {add}) }}\n");
        }
        text += &format!("fn chosen{k}() -> fn(Int) -> Int {{ f{k} }}\n");
        text += &format!("fn made{k}() -> Held {{ Held(run: f{k}) }}\n");
    }
    text += "\nstruct Held {\n  run: fn(Int) -> Int\n}\n\nstruct Shop {\n  held: Held\n}\n\n";
    text += "fn ident(f: fn(Int) -> Int) -> fn(Int) -> Int { f }\n";
    text += "fn same(h: Held) -> Held { h }\n";
    text += "fn wrap(f: fn(Int) -> Int) -> Held { Held(run: f) }\n";
    text += "fn priced(h: Held, x: Int) -> Int { h.run(x) }\n";
    text += "fn apply(f: fn(Int) -> Int, x: Int) -> Int { f(x) }\n";
    text += "fn via(h: Held, x: Int) -> Int { apply(h.run, x) }\n";
    text += "fn sold(s: Shop, x: Int) -> Int { s.held.run(x) }\n\n";
    text += "fn main() <console, alloc> {\n";
    // What each name holds, by its place among the names of its kind.
    let mut names: Vec<(String, Kind, usize)> = Vec::new();
    let mut calls: Vec<(String, usize)> = Vec::new();
    let of = |names: &[(String, Kind, usize)], kind: Kind, g: &mut Gen| {
        let some: Vec<&(String, Kind, usize)> = names.iter().filter(|n| n.1 == kind).collect();
        (!some.is_empty()).then(|| some[g.below(some.len())].clone())
    };
    for step in 0..2 + g.below(5) {
        let name = format!("v{step}");
        let (value, kind, held) = match g.below(10) {
            0 => {
                let k = g.below(3);
                (format!("f{k}"), Kind::Function, k)
            }
            1 => {
                let k = g.below(3);
                refused |= loud[k];
                (format!("chosen{k}()"), Kind::Function, k)
            }
            2 => match of(&names, Kind::Function, g) {
                Some((f, _, k)) => (format!("ident({f})"), Kind::Function, k),
                None => continue,
            },
            3 => match of(&names, Kind::Held, g) {
                Some((h, _, k)) => (format!("{h}.run"), Kind::Function, k),
                None => continue,
            },
            4 => match of(&names, Kind::Function, g) {
                Some((f, _, k)) => {
                    refused |= loud[k];
                    (format!("Held(run: {f})"), Kind::Held, k)
                }
                None => continue,
            },
            5 => {
                let k = g.below(3);
                refused |= loud[k];
                (format!("made{k}()"), Kind::Held, k)
            }
            6 => match of(&names, Kind::Held, g) {
                Some((h, _, k)) => (format!("same({h})"), Kind::Held, k),
                None => continue,
            },
            9 => match of(&names, Kind::Function, g) {
                Some((f, _, k)) => {
                    refused |= loud[k];
                    (format!("wrap({f})"), Kind::Held, k)
                }
                None => continue,
            },
            7 => match of(&names, Kind::Held, g) {
                Some((h, _, k)) => (format!("Shop(held: {h})"), Kind::Shop, k),
                None => continue,
            },
            _ => match of(&names, Kind::Shop, g) {
                Some((s, _, k)) => (format!("{s}.held"), Kind::Held, k),
                None => continue,
            },
        };
        text += &format!("  {name} = {value}\n");
        let x = argument;
        match kind {
            Kind::Function => calls.push((format!("{name}({x})"), held)),
            Kind::Held => {
                calls.push((format!("{name}.run({x})"), held));
                calls.push((format!("priced({name}, {x})"), held));
                calls.push((format!("via({name}, {x})"), held));
                calls.push((format!("same({name}).run({x})"), held));
            }
            Kind::Shop => {
                calls.push((format!("sold({name}, {x})"), held));
                calls.push((format!("{name}.held.run({x})"), held));
            }
        }
        names.push((name, kind, held));
    }
    let mut printed = Vec::new();
    for (call, held) in &calls {
        text += &format!("  print(\"{{{call}}}\")\n");
        if loud[*held] {
            printed.extend(b"side\n");
        }
        printed.extend(format!("{}\n", argument as u32 + adds[*held] as u32).into_bytes());
    }
    text += "}\n";
    Generated {
        adds,
        loud,
        argument,
        source: text,
        printed,
        refused,
    }
}

/// Whatever the generated program holds in each of these ways, it prints what
/// the call of the function it holds prints when it is written directly, and
/// a function that needs an effect is never held where its type states none,
/// so no program that is accepted runs an effect that a function along its
/// way did not declare.
fn every_accepted_program_that_holds_named_functions_prints_what_calling_them_directly_prints() {
    generate::check(4302, 150, generated, |p| {
        let built = lower(p.source.as_bytes());
        if p.refused {
            let found = built.expect_err("a function with an effect is held in a type");
            assert!(
                found.iter().any(|e| e.message.contains("states none")),
                "{found:?} for\n{}",
                p.source
            );
            return;
        }
        let program = built.unwrap_or_else(|e| {
            panic!("{e:?} for\n{}\n{:?} {:?} {}", p.source, p.adds, p.loud, p.argument)
        });
        assert_eq!(scalar::run(&program).stdout, p.printed, "for\n{}", p.source);
    });
}

/// A function type states no effect, so a function that needs one is refused
/// where it goes into a field or is given as a result, and is not refused where
/// it is passed to a call, which is checked at the call.
fn a_function_that_needs_an_effect_is_not_stored_in_a_field_or_given_as_a_result() {
    let head = "use std.console (console, print)\n\n\
                fn loud(x: f32) -> f32 <console> {\n  print(\"side\")\n  x\n}\n\n\
                fn twice_f(x: f32, f: fn(f32) -> f32) -> f32 { f(f(x)) }\n\n\
                struct Pricing {\n  discount: fn(f32) -> f32\n}\n\n\
                fn quiet(p: Pricing) -> f32 { p.discount(1.0) }\n\n\
                fn ret() -> fn(f32) -> f32 { loud }\n\n\
                fn made() -> Pricing { Pricing(discount: loud) }\n\n\
                fn wrap(f: fn(f32) -> f32) -> Pricing { Pricing(discount: f) }\n\n\
                fn half(x: f32) -> f32 { x * 0.5 }\n\n";
    let main = |body: &str| format!("{head}fn main() <console, alloc> {{\n{body}}}\n");
    let refused = [
        "  print(\"{quiet(Pricing(discount: loud))}\")\n",
        "  f = loud\n  print(\"{quiet(Pricing(discount: f))}\")\n",
        "  print(\"{ret()(1.0)}\")\n",
        "  print(\"{quiet(made())}\")\n",
        "  print(\"{quiet(wrap(loud))}\")\n",
    ];
    for body in refused {
        let source = main(body);
        let found = check(source.as_bytes()).expect_err("refused");
        assert!(
            found.iter().any(|e| e.message.contains("`loud` needs the `console` effect, and")
                && e.message.contains("states none")),
            "{found:?} for\n{source}"
        );
    }
    // A function that is given for a parameter and stored takes the function of
    // the argument, which is half here, and a function that needs an effect is
    // refused where the argument is, in a function that declares the effect or
    // not.
    let kept = main("  print(\"{wrap(half).discount(3.0)}\")\n");
    assert!(check(kept.as_bytes()).is_ok(), "{:?}", check(kept.as_bytes()));
    let run = main("  print(\"{wrap(half).discount(3.0)}\")\n");
    assert_eq!(scalar::run(&lower(run.as_bytes()).expect("builds")).stdout, b"1.5\n");
    // From a function that does not declare the effect, it stays refused.
    let without = main("  x = quiet(wrap(loud))\n").replace("fn main() <console, alloc>", "fn main() <alloc>");
    assert!(check(without.as_bytes()).is_err());
    // Passed to a call, it is the caller's effect, and the caller declares it.
    let passed = main("  print(\"{twice_f(1.0, loud)}\")\n");
    assert!(check(passed.as_bytes()).is_ok(), "{:?}", check(passed.as_bytes()));
    let silent = passed.replace("fn main() <console, alloc>", "fn main() <alloc>");
    assert!(check(silent.as_bytes()).is_err());
}

/// A lambda that reads or assigns a name around it holds something lent, and
/// a lambda that needs an effect is held where a type states none: each is
/// refused where the lambda is kept.
fn a_lambda_that_holds_something_lent_or_needs_an_effect_is_refused_where_it_is_kept() {
    let head = "use std.console (console, print)\n\n\
                struct Pricing {\n  discount: fn(f32) -> f32\n}\n\n";
    let lent = [
        "fn kept(rate: f32) -> Pricing {\n  Pricing(discount: fn(v: f32) -> f32 { v * rate })\n}\n",
        "fn kept(rate: f32) -> fn(f32) -> f32 {\n  fn(v: f32) -> f32 { v * rate }\n}\n",
    ];
    for item in lent {
        let source = format!("{head}{item}\nfn main() <console> {{\n  print(\"{{kept(1.0)}}\")\n}}\n");
        let found = check(source.as_bytes()).expect_err("refused");
        assert!(
            found.iter().any(|e| e.message.starts_with("a lambda cannot be")),
            "{found:?} for\n{source}"
        );
    }
    let loud = format!(
        "{head}fn kept() -> Pricing {{\n  Pricing(discount: fn(v: f32) -> f32 {{\n    print(\"x\")\n    v\n  }})\n}}\n\n\
         fn main() <console> {{\n  k = kept()\n}}\n"
    );
    let found = check(loud.as_bytes()).expect_err("refused");
    assert!(
        found.iter().any(|e| e.message.contains("`console` effect") && e.message.contains("states none")),
        "{found:?}"
    );
}

/// Two different lambdas where one type is needed are refused, as two
/// functions with names are, and each lambda is a type of its own.
fn which_lambda_a_value_is_is_known_when_the_program_is_built() {
    let head = "struct Pricing {\n  discount: fn(f32) -> f32\n}\n\n";
    let pick = format!(
        "{head}fn pick(wide: bool) -> Pricing {{\n  if wide {{ Pricing(discount: fn(v: f32) -> f32 {{ v * 2.0 }}) }} \
         else {{ Pricing(discount: fn(v: f32) -> f32 {{ v * 3.0 }}) }}\n}}\n\nfn main() {{\n  p = pick(true)\n}}\n"
    );
    let found = check(pick.as_bytes()).expect_err("refused");
    assert!(
        found.iter().any(|e| e.message.contains("must be known when the program is built")),
        "{found:?}"
    );
    // Two lambdas of the same text, written twice, are each their own type, so a
    // function that takes the struct is built once for each.
    let both = format!(
        "use std.console (console, print)\n\n{head}fn priced(p: Pricing, x: f32) -> f32 {{ p.discount(x) }}\n\n\
         fn main() <console, alloc> {{\n  a = Pricing(discount: fn(v: f32) -> f32 {{ v * 2.0 }})\n  \
         b = Pricing(discount: fn(v: f32) -> f32 {{ v * 3.0 }})\n  print(\"{{priced(a, 1.0)}} {{priced(b, 1.0)}}\")\n}}\n"
    );
    assert_eq!(scalar::run(&lower(both.as_bytes()).expect("builds")).stdout, b"2.0 3.0\n");
}

/// A lambda may write its own type parameters, and each call settles them.
fn a_lambda_that_writes_type_parameters_is_called_with_each_type() {
    let source = r#"use std.console (console, print)

fn main() <console, alloc> {
  same = fn[T](x: T) -> T { x }
  print("{same(1)} {same("a")}")
}
"#;
    assert_eq!(scalar::run(&lower(source.as_bytes()).expect("builds")).stdout, b"1 a\n");
}

/// A lambda that leaves a type out where no function type decides it is refused
/// with where to write the type, and not as one that holds something lent.
fn a_lambda_that_leaves_a_type_out_where_nothing_decides_it_is_refused_where_to_write_it() {
    for bound in ["fn(v) { v * 2.0 }", "fn(v: f32) { v * 2.0 }"] {
        let source = format!("fn main() {{\n  f = {bound}\n}}\n");
        let found = check(source.as_bytes()).expect_err("refused");
        assert!(
            found.iter().any(|e| e.message.contains("writes the type of each parameter")),
            "{found:?} for\n{source}"
        );
    }
}

/// `f = if c { a } else { b }` of lambdas is a choice between functions, and is
/// refused as one is.
fn a_choice_between_lambdas_bound_to_a_name_is_refused_as_a_choice_between_functions() {
    let source = "fn main() {\n  wide = true\n  f = if wide { fn(x: f32) -> f32 { x } } \
                  else { fn(x: f32) -> f32 { x * 2.0 } }\n}\n";
    let found = check(source.as_bytes()).expect_err("refused");
    assert!(
        found.iter().any(|e| e.message.contains("must be known when the program is built")),
        "{found:?}"
    );
    assert!(found.iter().all(|e| !e.message.contains("stands alone")), "{found:?}");
}
