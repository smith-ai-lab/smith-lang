//! A function that lists an effect and has nothing in it that needs the effect
//! is warned at the effect, and still builds. A function whose name is used
//! as a value is not, as the type it is given may be the reason it lists the
//! effect.

use smith::diag::Error;

use crate::Test;
use crate::generate::{self, Gen};
use crate::programs::with_imports;

pub const ONCE: &[Test] = &[
    (
        "an_effect_a_function_lists_and_nothing_in_it_needs_is_a_warning_at_the_effect",
        an_effect_a_function_lists_and_nothing_in_it_needs_is_a_warning_at_the_effect,
    ),
    (
        "an_effect_needed_only_inside_a_lambda_of_the_function_is_needed",
        an_effect_needed_only_inside_a_lambda_of_the_function_is_needed,
    ),
    (
        "a_function_of_an_impl_is_not_warned_for_the_effects_it_lists",
        a_function_of_an_impl_is_not_warned_for_the_effects_it_lists,
    ),
    (
        "a_function_used_as_a_value_is_not_warned_for_the_effects_it_lists",
        a_function_used_as_a_value_is_not_warned_for_the_effects_it_lists,
    ),
];

/// The effects a function may list.
const EFFECTS: [&str; 2] = ["console", "alloc"];

#[derive(Debug)]
struct Case {
    /// For each effect, whether the function lists it and whether the body
    /// needs it, which is only so for an effect listed.
    listed: [bool; 2],
    needed: [bool; 2],
    /// The line of the function.
    line: usize,
    source: String,
}

fn case(g: &mut Gen) -> Case {
    let listed = [g.below(2) == 1, g.below(2) == 1];
    let needed = [listed[0] && g.below(2) == 1, listed[1] && g.below(2) == 1];
    let list = |on: [bool; 2]| {
        let names: Vec<&str> = (0..2).filter(|&i| on[i]).map(|i| EFFECTS[i]).collect();
        if names.is_empty() {
            String::new()
        } else {
            format!(" ! {}", names.join(", "))
        }
    };
    let mut body = String::new();
    if needed[0] {
        body += "  print(\"hello\")\n";
    }
    if needed[1] {
        body += "  xs: List[Int] = [1, 2]\n";
    }
    let source = with_imports(&format!(
        "use std.console (console, print)

fn work(){} {{
{body}}}

fn main(){} {{
  work()
}}
",
        list(listed),
        list(listed),
    ));
    let line = source
        .lines()
        .position(|l| l.starts_with("fn work"))
        .expect("the function is written")
        + 1;
    Case {
        listed,
        needed,
        line,
        source,
    }
}

fn at(errors: &[Error], line: usize) -> Vec<&Error> {
    errors.iter().filter(|e| e.pos.line == line).collect()
}

fn an_effect_a_function_lists_and_nothing_in_it_needs_is_a_warning_at_the_effect() {
    generate::check(41, 40, case, |case| {
        let source = case.source.as_bytes();
        let warnings = smith::check(source).expect("the program builds");
        let unused: Vec<&str> = (0..2)
            .filter(|&i| case.listed[i] && !case.needed[i])
            .map(|i| EFFECTS[i])
            .collect();
        let found = at(&warnings, case.line);
        let said: Vec<&&str> = unused
            .iter()
            .filter(|effect| {
                found
                    .iter()
                    .any(|w| w.message.contains(&format!("lists the `{effect}` effect")))
            })
            .collect();
        assert_eq!(said.len(), unused.len(), "one warning for each effect unused: {found:?}");
        assert_eq!(found.len(), unused.len(), "and no other at the function: {found:?}");
        for w in &found {
            let fix = w.fix.as_deref().unwrap_or("");
            let kept: Vec<&str> = (0..2)
                .filter(|&i| case.listed[i] && case.needed[i])
                .map(|i| EFFECTS[i])
                .collect();
            for effect in EFFECTS {
                assert_eq!(fix.contains(effect), kept.contains(&effect), "{fix}");
            }
        }
        assert!(smith::lower(source).is_ok(), "a warning changes nothing");
    });
}

fn a_function_used_as_a_value_is_not_warned_for_the_effects_it_lists() {
    let source = with_imports(
        "use std.console (console, print)

fn quiet() ! console {
}

fn takes(body: fn() ! console) {
  body()
}

fn main() ! console {
  takes(quiet)
  print(\"done\")
}
",
    );
    let warnings = smith::check(source.as_bytes()).expect("the program builds");
    assert!(
        warnings.iter().all(|w| !w.message.contains("lists the")),
        "{warnings:?}"
    );
}

fn an_effect_needed_only_inside_a_lambda_of_the_function_is_needed() {
    let source = with_imports(
        "use std.console (console, print)

fn run[T](body: fn() -> T) -> T { body() }

fn work() ! console {
  run { print(\"hi\") }
}

fn main() ! console {
  work()
}
",
    );
    let warnings = smith::check(source.as_bytes()).expect("the program builds");
    assert!(warnings.iter().all(|w| !w.message.contains("lists the")), "{warnings:?}");
}

fn a_function_of_an_impl_is_not_warned_for_the_effects_it_lists() {
    let source = with_imports(
        "use std.console (console, print)

struct Quiet {
  n: Int
}

trait Greet[T] {
  fn greet(x: T) ! console
}

impl Greet[Quiet] {
  fn greet(x: Quiet) ! console {
  }
}

fn main() ! console {
  greet(Quiet(n: 1))
  print(\"done\")
}
",
    );
    let warnings = smith::check(source.as_bytes()).expect("the program builds");
    assert!(warnings.iter().all(|w| !w.message.contains("lists the")), "{warnings:?}");
}
