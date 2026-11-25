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
use smith::{check, compile};

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
        || builds_to_the_bytes_of_the_call_written_directly(&[Hold::Made]),
    ),
    (
        "a_function_that_is_returned_and_passed_builds_to_the_bytes_of_the_call_written_directly",
        || builds_to_the_bytes_of_the_call_written_directly(&[Hold::CallTheReturn]),
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
}

#[derive(Debug)]
struct Program {
    /// The function that is held, among those the file defines.
    held: usize,
    /// What each function adds.
    adds: [u8; 3],
    argument: u8,
    hold: Hold,
}

fn program(g: &mut Gen, holds: &[Hold]) -> Program {
    Program {
        held: g.below(3),
        adds: [g.below(200) as u8, g.below(200) as u8, g.below(200) as u8],
        argument: g.below(50) as u8,
        hold: *g.pick(holds),
    }
}

impl Program {
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
        let (k, x) = (self.held, self.argument);
        let mut text = self.head();
        match self.hold {
            Hold::Bind => text += &Self::main(&format!("  f = f{k}\n  r = f({x})\n")),
            Hold::BindThenPass => text += &Self::main(&format!("  f = f{k}\n  r = twice({x}, f)\n")),
            Hold::Field => {
                text += "struct Held {\n  run: fn(Int) -> Int\n}\n\n";
                text += &Self::main(&format!("  h = Held(run: f{k})\n  r = h.run({x})\n"));
            }
            Hold::Return => {
                text += &format!("fn chosen() -> fn(Int) -> Int {{ f{k} }}\n\n");
                text += &Self::main(&format!("  r = chosen()({x})\n"));
            }
            Hold::BindTheReturn => {
                text += &format!("fn chosen() -> fn(Int) -> Int {{ f{k} }}\n\n");
                text += &Self::main(&format!("  f = chosen()\n  r = f({x})\n"));
            }
            Hold::CallTheReturn => {
                text += &format!("fn chosen() -> fn(Int) -> Int {{ f{k} }}\n\n");
                text += &Self::main(&format!("  r = twice({x}, chosen())\n"));
            }
            Hold::Takes => {
                text += "struct Held {\n  run: fn(Int) -> Int\n}\n\n";
                text += "fn priced(h: Held, x: Int) -> Int { h.run(x) }\n\n";
                text += &Self::main(&format!("  a = Held(run: f{k})\n  r = priced(a, {x})\n"));
            }
            Hold::Nested => {
                text += "struct Held {\n  run: fn(Int) -> Int\n}\n\n";
                text += "struct Shop {\n  held: Held\n}\n\n";
                text += "fn sold(s: Shop, x: Int) -> Int {\n  h = s.held\n  h.run(x)\n}\n\n";
                text += &Self::main(&format!(
                    "  s = Shop(held: Held(run: f{k}))\n  r = sold(s, {x})\n"
                ));
            }
            Hold::Made => {
                text += "struct Held {\n  run: fn(Int) -> Int\n}\n\n";
                text += &format!("fn made() -> Held {{ Held(run: f{k}) }}\n\n");
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
    generate::check(4301, 3, |g: &mut Gen| program(g, holds), |p| {
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
