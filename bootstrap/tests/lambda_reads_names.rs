//! A lambda that reads names of the function it is written in, bound to a name
//! and called in the same body, holds the values of those names and costs
//! nothing: it builds to the bytes of the program that writes its body where it
//! is called. One that reads a value that is lent, or assigns a name of its
//! function, is refused where it is bound.
//!
//! The generator builds the program that binds the lambda and the program that
//! writes the body where it is called from the same choices, so the model of
//! each is the other.

use smith::target::Target;
use smith::{check, compile, lower, scalar};

use crate::Test;
use crate::generate::{self, Gen};

pub const ONCE: &[Test] = &[
    (
        "a_lambda_that_reads_names_and_is_called_builds_to_the_bytes_of_its_body_written_where_it_is_called",
        a_lambda_that_reads_names_and_is_called_builds_to_the_bytes_of_its_body_written_where_it_is_called,
    ),
    (
        "a_lambda_that_reads_names_gives_what_its_body_gives_for_the_values_they_had_where_it_is_bound",
        a_lambda_that_reads_names_gives_what_its_body_gives_for_the_values_they_had_where_it_is_bound,
    ),
    (
        "a_lambda_that_reads_a_name_of_a_function_with_type_parameters_is_called_with_each_type",
        a_lambda_that_reads_a_name_of_a_function_with_type_parameters_is_called_with_each_type,
    ),
    (
        "a_lambda_that_reads_a_value_of_a_type_parameter_is_built_for_each_type",
        a_lambda_that_reads_a_value_of_a_type_parameter_is_built_for_each_type,
    ),
    (
        "a_name_that_a_bound_lambda_holds_is_not_given_away_before_the_last_call_of_the_lambda",
        a_name_that_a_bound_lambda_holds_is_not_given_away_before_the_last_call_of_the_lambda,
    ),
    (
        "a_lambda_that_reads_a_lent_value_or_assigns_a_name_of_its_function_is_refused_where_it_is_bound",
        a_lambda_that_reads_a_lent_value_or_assigns_a_name_of_its_function_is_refused_where_it_is_bound,
    ),
];

/// What the generator chose: the names the lambda reads, what each holds, and
/// what the lambda is called with.
#[derive(Debug)]
struct Program {
    reads: Vec<u8>,
    argument: u8,
    calls: usize,
}

fn program_of(g: &mut Gen) -> Program {
    let names = 1 + g.below(3);
    Program {
        reads: (0..names).map(|_| g.below(100) as u8).collect(),
        argument: g.below(100) as u8,
        calls: 1 + g.below(2),
    }
}

impl Program {
    fn head(&self) -> String {
        let mut text = String::from("use std.console (console, print)\nuse std.int (wrapping_add)\n\n");
        text += "fn main() <console, alloc> {\n";
        for (i, value) in self.reads.iter().enumerate() {
            text += &format!("  k{i}: Int = {value}\n");
        }
        text
    }

    /// The body of the lambda, for the name it calls its parameter.
    fn body(&self, x: &str) -> String {
        let mut sum = x.to_string();
        for i in 0..self.reads.len() {
            sum = format!("wrapping_add({sum}, k{i})");
        }
        sum
    }

    fn bound(&self) -> String {
        let mut text = self.head();
        text += &format!("  f = fn(x: Int) -> Int {{ {} }}\n", self.body("x"));
        for _ in 0..self.calls {
            text += &format!("  print(\"{{f({})}}\")\n", self.argument);
        }
        text + "}\n"
    }

    /// The same, with the body written where the lambda is called.
    fn written_where_called(&self) -> String {
        let mut text = self.head();
        for _ in 0..self.calls {
            text += &format!("  print(\"{{{}}}\")\n", self.body(&self.argument.to_string()));
        }
        text + "}\n"
    }

    fn says(&self) -> String {
        let sum: u64 = self.reads.iter().map(|r| u64::from(*r)).sum::<u64>() + u64::from(self.argument);
        format!("{sum}\n").repeat(self.calls)
    }
}

fn a_lambda_that_reads_names_and_is_called_builds_to_the_bytes_of_its_body_written_where_it_is_called() {
    generate::check(4411, 4, program_of, |p| {
        let (bound, written) = (p.bound(), p.written_where_called());
        if let Err(found) = check(bound.as_bytes()) {
            panic!("{found:?} for\n{bound}");
        }
        for &target in Target::ALL {
            let held = compile(bound.as_bytes(), target).unwrap_or_else(|e| panic!("{e:?}\n{bound}"));
            let said = compile(written.as_bytes(), target).expect("the body written in place builds");
            assert_eq!(held, said, "for\n{bound}\nand\n{written}");
        }
    });
}

fn a_lambda_that_reads_names_gives_what_its_body_gives_for_the_values_they_had_where_it_is_bound() {
    generate::check(4412, 4, program_of, |p| {
        let bound = p.bound();
        let ran = scalar::run(&lower(bound.as_bytes()).unwrap_or_else(|e| panic!("{e:?}\n{bound}")));
        assert_eq!(ran.stdout, p.says().as_bytes(), "for\n{bound}");
    });
    // A `var` that is assigned after the lambda is bound is not what it reads.
    let source = "use std.console (console, print)\n\nfn main() <console, alloc> {\n  var k: f32 = 3.0\n  \
                  f = fn(x: f32) -> f32 { x * k }\n  k = 10.0\n  print(\"{f(2.0)} {k}\")\n}\n";
    let ran = scalar::run(&lower(source.as_bytes()).expect("builds"));
    assert_eq!(ran.stdout, b"6.0 10.0\n");
}

fn a_lambda_that_reads_a_name_of_a_function_with_type_parameters_is_called_with_each_type() {
    let source = "use std.console (console, print)\n\n\
                  fn first[T: Copy](a: T, b: T) -> T {\n  g = fn(x: T) -> T { a }\n  g(b)\n}\n\n\
                  fn main() <console, alloc> {\n  print(\"{first(1, 2)} {first(1.5, 2.5)}\")\n}\n";
    let ran = scalar::run(&lower(source.as_bytes()).expect("builds"));
    assert_eq!(ran.stdout, b"1 1.5\n");
}

/// A lambda that reads a name whose type is a type parameter of its function, or
/// holds one, is called at each type the function is called at.
fn a_lambda_that_reads_a_value_of_a_type_parameter_is_built_for_each_type() {
    let source = "use std.console (console, print)\n\n\
                  fn count[T](xs: List[T]) -> Nat {\n  size = fn(i: Nat) -> Nat { xs.size }\n  size(0)\n}\n\n\
                  fn main() <console, alloc> {\n  a: List[Nat] = [1, 2, 3]\n  b: List[str] = [\"a\", \"b\"]\n  \
                  print(\"{count(a)} {count(b)}\")\n}\n";
    let ran = scalar::run(&lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?}")));
    assert_eq!(ran.stdout, b"3 2\n");
}

fn a_lambda_that_reads_a_lent_value_or_assigns_a_name_of_its_function_is_refused_where_it_is_bound() {
    let head = "use std.console (console, print)\nuse std.bits (u8)\n\n";
    let refused = [
        "fn kept(rate: f32) -> f32 {\n  var xs: List[f32] = [rate]\n  g = fn(i: u8) -> f32 { xs[0] }\n  xs.push(1.0)\n  g(0)\n}\n",
        "fn kept(rate: f32) -> f32 {\n  var n: f32 = rate\n  g = fn(x: f32) -> f32 {\n    n = n + x\n    n\n  }\n  g(1.0)\n}\n",
    ];
    for item in refused {
        let source = format!("{head}{item}\nfn main() <console> {{\n  print(\"{{kept([1.0])}}\")\n}}\n");
        let found = check(source.as_bytes()).expect_err("refused");
        assert!(
            found.iter().any(|e| e.message.starts_with("a lambda cannot be bound to a name")),
            "{found:?} for\n{source}"
        );
    }
}

/// A name that a bound lambda holds is lent to it for as long as it can be
/// called, so giving the name away before a call of the lambda, on one path or
/// on all, is a use after it was given away, and giving it away after the last
/// call is not.
fn a_name_that_a_bound_lambda_holds_is_not_given_away_before_the_last_call_of_the_lambda() {
    let head = "use std.console (console, print)\n\nfn eat(xs: take List[Nat]) -> Nat { xs.size }\n\n\
                fn main() <console, alloc> {\n  xs: List[Nat] = [1, 2, 3]\n  \
                f = fn(i: 0..10) -> Nat { xs.size + i }\n";
    let early = format!("{head}  n = eat(xs)\n  print(\"{{f(0)}} {{n}}\")\n}}\n");
    let on_a_path =
        format!("{head}  n = if true {{ eat(xs) }} else {{ 0 }}\n  print(\"{{f(0)}} {{n}}\")\n}}\n");
    for source in [early, on_a_path] {
        let found = check(source.as_bytes()).expect_err("refused");
        assert!(
            found.iter().any(|e| e.message.contains("after it was given away") && e.message.contains("`f`")),
            "{found:?} for\n{source}"
        );
    }
    let late = format!("{head}  m = f(0)\n  n = eat(xs)\n  print(\"{{m}} {{n}}\")\n}}\n");
    let ran = scalar::run(&lower(late.as_bytes()).unwrap_or_else(|e| panic!("{e:?}\n{late}")));
    assert_eq!(ran.stdout, b"3 3\n");
}
