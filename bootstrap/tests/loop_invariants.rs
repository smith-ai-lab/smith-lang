//! A `loop` over a range may state an `invariant`, a fact that holds at the
//! start of every pass: `arith` proves that it holds before the first pass and
//! that each pass keeps it, uses it inside the loop, and, where no `break` ends
//! the loop, after it.
//!
//! Every generator knows the values its loop gives the `var`, so whether an
//! invariant is true, and where it stops being, comes from there and shares
//! nothing with the compiler.

use smith::scalar::{self, Outcome};
use smith::target::Target;
use smith::{compile, lower};

use crate::Test;
use crate::generate::{self, Gen};
use crate::numbers::with_header;

pub const ONCE: &[Test] = &[
    (
        "an_invariant_that_holds_is_accepted_and_the_program_is_the_one_that_states_the_range_with_none_part_1_of_4",
        || an_invariant_that_holds_is_accepted_and_the_program_is_the_one_that_states_the_range_with_none(0),
    ),
    (
        "an_invariant_that_holds_is_accepted_and_the_program_is_the_one_that_states_the_range_with_none_part_2_of_4",
        || an_invariant_that_holds_is_accepted_and_the_program_is_the_one_that_states_the_range_with_none(1),
    ),
    (
        "an_invariant_that_holds_is_accepted_and_the_program_is_the_one_that_states_the_range_with_none_part_3_of_4",
        || an_invariant_that_holds_is_accepted_and_the_program_is_the_one_that_states_the_range_with_none(2),
    ),
    (
        "an_invariant_that_holds_is_accepted_and_the_program_is_the_one_that_states_the_range_with_none_part_4_of_4",
        || an_invariant_that_holds_is_accepted_and_the_program_is_the_one_that_states_the_range_with_none(3),
    ),
    (
        "an_invariant_that_is_false_before_the_first_pass_is_refused_at_the_invariant",
        an_invariant_that_is_false_before_the_first_pass_is_refused_at_the_invariant,
    ),
    (
        "an_invariant_that_a_pass_breaks_is_refused_where_the_pass_ends",
        an_invariant_that_a_pass_breaks_is_refused_where_the_pass_ends,
    ),
    (
        "an_invariant_gives_the_range_after_the_loop_unless_a_break_can_end_it",
        an_invariant_gives_the_range_after_the_loop_unless_a_break_can_end_it,
    ),
    (
        "the_invariant_must_hold_where_the_counter_is_the_end_of_the_range_and_before_a_loop_that_runs_no_pass",
        the_invariant_must_hold_where_the_counter_is_the_end_of_the_range_and_before_a_loop_that_runs_no_pass,
    ),
    (
        "an_invariant_of_a_loop_over_a_list_is_about_the_names_outside_it_and_holds_after_it",
        an_invariant_of_a_loop_over_a_list_is_about_the_names_outside_it_and_holds_after_it,
    ),
    (
        "an_invariant_ties_two_names_that_no_template_ties",
        an_invariant_ties_two_names_that_no_template_ties,
    ),
    (
        "an_invariant_on_its_own_line_and_two_of_them_are_read",
        an_invariant_on_its_own_line_and_two_of_them_are_read,
    ),
];

/// The messages of the errors of a program that is refused.
fn refused(source: &str) -> Vec<(usize, String)> {
    let errors = lower(source.as_bytes()).expect_err("a program that is refused");
    errors.into_iter().map(|e| (e.pos.line, e.message)).collect()
}

fn run(source: &str) -> Outcome {
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for\n{source}"));
    scalar::run(&program)
}

/// The line of a statement in the source the generator wrote, which is the
/// line of its text.
fn line_of(source: &str, text: &str) -> usize {
    source
        .lines()
        .position(|l| l.trim() == text)
        .unwrap_or_else(|| panic!("`{text}` in\n{source}"))
        + 1
}

/// A loop that gives a `var` the same step in each of its passes.
#[derive(Debug)]
struct Steps {
    start: i128,
    step: i128,
    passes: i128,
}

fn steps(g: &mut Gen) -> Steps {
    Steps {
        start: g.below(5) as i128,
        step: 1 + g.below(4) as i128,
        passes: 1 + g.below(9) as i128,
    }
}

impl Steps {
    /// What the `var` is after the last pass.
    fn last(&self) -> i128 {
        self.start + self.step * self.passes
    }

    /// The program: the `var` is given the range that holds it, or none, the
    /// head of the loop is `head`, and the result is a range that holds it.
    fn source(&self, range: bool, head: &str) -> String {
        let declared = if range {
            format!(": 0..{}", self.last() + 1)
        } else {
            String::new()
        };
        with_header(format!(
            "fn f() -> 0..{} {{\n  var acc{declared} = {}\n  loop i in 0..{}{head} {{\n    acc = acc + {}\n  }}\n  acc\n}}\n\n\
             fn main() <console, alloc> {{\n  print(\"{{f()}}\")\n}}\n",
            self.last() + 1,
            self.start,
            self.passes,
            self.step
        ))
    }
}

/// An invariant that is true is accepted where no range is written for the
/// `var` and nothing but the invariant proves each pass fits it, it prints
/// what the loop gives, and its executable is that of the program that has the
/// range inferred and no invariant, on two targets.
fn an_invariant_that_holds_is_accepted_and_the_program_is_the_one_that_states_the_range_with_none(part: u64) {
    generate::check_part(71, 12, part, 4,steps, |s| {
        let (start, step) = (s.start, s.step);
        let open = s.source(false, "");
        for form in [
            format!(" invariant acc == {start} + {step} * i"),
            format!(" invariant acc <= {start} + {step} * i"),
        ] {
            let stated = s.source(true, &form);
            assert_eq!(
                run(&stated),
                Outcome {
                    stdout: format!("{}\n", s.last()).into_bytes(),
                    status: 0
                },
                "for\n{stated}"
            );
            for target in [Target::ALL[0], Target::ALL[Target::ALL.len() - 1]] {
                let (a, b) = (compile(stated.as_bytes(), target), compile(open.as_bytes(), target));
                assert!(a.is_ok(), "{a:?} for\n{stated}");
                assert!(a == b, "the executables differ, or {b:?}, for\n{stated}\nand\n{open}");
            }
        }
    });
}

/// An invariant that is false when the loop starts is refused at the
/// invariant, before the first pass, and for nothing else.
fn an_invariant_that_is_false_before_the_first_pass_is_refused_at_the_invariant() {
    generate::check(72, 12, steps, |s| {
        let off = 1 + s.start;
        let head = format!(" invariant acc <= {} + {} * i", s.start - off, s.step);
        let source = s.source(true, &head);
        let errors = refused(&source);
        let at = line_of(&source, &format!("loop i in 0..{}{head} {{", s.passes));
        assert!(
            errors.iter().any(|(line, m)| *line == at
                && m.contains("invariant")
                && m.contains("before the first pass")),
            "{errors:?} for\n{source}"
        );
        assert!(
            errors.iter().all(|(_, m)| m.contains("invariant")),
            "{errors:?} for\n{source}"
        );
    });
}

/// An invariant that is true when the loop starts and that a pass breaks is
/// refused where the pass ends, which is the `}` of the loop.
fn an_invariant_that_a_pass_breaks_is_refused_where_the_pass_ends() {
    generate::check(73, 12, steps, |s| {
        let head = format!(" invariant acc <= {} + {} * i", s.start, s.step - 1);
        let source = s.source(true, &head);
        let errors = refused(&source);
        let at = line_of(&source, &format!("loop i in 0..{}{head} {{", s.passes)) + 2;
        assert_eq!(errors.len(), 1, "{errors:?} for\n{source}");
        let (line, message) = &errors[0];
        assert!(
            *line == at && message.contains("invariant") && message.contains("after a pass"),
            "{errors:?} for\n{source}"
        );
    });
}

/// What a loop with an invariant gives is known after it: the range it has
/// there is what the invariant says of the last pass. A `break` in the loop
/// may end it in the middle of a pass, so nothing is known after it.
fn an_invariant_gives_the_range_after_the_loop_unless_a_break_can_end_it() {
    generate::check(74, 12, steps, |s| {
        let top = s.step * 9;
        let program = |bound: i128, extra: &str| {
            with_header(format!(
                "fn f(n: 0..10) -> 0..{bound} {{\n  var acc: 0..1000 = 0\n  \
                 loop i in 0..n invariant acc == {} * i {{\n{extra}    acc = acc + {}\n  }}\n  acc\n}}\n\n\
                 fn main() <console, alloc> {{\n  print(\"{{f({})}}\")\n}}\n",
                s.step, s.step, s.passes
            ))
        };
        let tight = program(top + 1, "");
        assert_eq!(
            run(&tight),
            Outcome {
                stdout: format!("{}\n", s.step * s.passes).into_bytes(),
                status: 0
            },
            "for\n{tight}"
        );
        assert!(!refused(&program(top, "")).is_empty());
        let ended = program(top + 1, "    if i == 3 { break }\n");
        let errors = refused(&ended);
        assert!(
            errors.iter().any(|(_, m)| m.contains("acc")),
            "{errors:?} for\n{ended}"
        );
        assert!(errors.iter().all(|(_, m)| !m.contains("invariant")), "{errors:?} for\n{ended}");
        // Inside the loop it is still used: only the result is refused.
        assert!(lower(program(1000, "    if i == 3 { break }\n").as_bytes()).is_ok());
    });
}

/// The last pass has the counter one below the end of its range, so the
/// invariant must hold with the counter at the end, which a type of the
/// counter does not hold. Where the range has no pass, it must hold with the
/// counter as its first end.
fn the_invariant_must_hold_where_the_counter_is_the_end_of_the_range_and_before_a_loop_that_runs_no_pass()
 {
    let program = |head: &str| {
        with_header(format!(
            "fn f() -> 0..257 {{\n  var acc: 0..257 = 0\n  loop i in 0..256 {head} {{\n    acc = acc + 1\n  }}\n  acc\n}}\n\n\
             fn main() <console, alloc> {{\n  print(\"{{f()}}\")\n}}\n"
        ))
    };
    let kept = program("invariant acc == i");
    assert_eq!(
        run(&kept),
        Outcome {
            stdout: b"256\n".to_vec(),
            status: 0
        }
    );
    let past = refused(&program("invariant i < 256"));
    assert!(
        past.iter().any(|(_, m)| m.contains("after a pass")),
        "{past:?}"
    );
    // A range with no pass has no pass to keep the invariant, and the loop
    // runs none, so the invariant holds after it as it did before it.
    let zero = |held: i128| {
        with_header(format!(
            "fn f() -> 0..3 {{\n  var acc: 0..3 = 0\n  loop i in 2..2 invariant acc == {held} {{\n    acc = 2\n  }}\n  acc\n}}\n\n\
             fn main() <console, alloc> {{\n  print(\"{{f()}}\")\n}}\n"
        ))
    };
    assert_eq!(
        run(&zero(0)),
        Outcome {
            stdout: b"0\n".to_vec(),
            status: 0
        }
    );
    let errors = refused(&zero(1));
    assert!(
        errors.iter().any(|(_, m)| m.contains("before the first pass")),
        "{errors:?}"
    );
}

/// A loop over a list has no counter, so its invariant is about the names
/// outside it, and holds after it.
fn an_invariant_of_a_loop_over_a_list_is_about_the_names_outside_it_and_holds_after_it() {
    let program = |head: &str| {
        with_header(format!(
            "fn f(xs: List[Byte]) -> Nat {{\n  var a = 0\n  var b = 0\n  loop v in xs {head} {{\n    a = a + 1\n    b = b + 1\n  }}\n  a - b\n}}\n\n\
             fn main() <console, alloc> {{\n  print(\"{{f([1, 2, 3])}}\")\n}}\n"
        ))
    };
    assert_eq!(
        run(&program("invariant a == b")),
        Outcome {
            stdout: b"0\n".to_vec(),
            status: 0
        }
    );
    assert!(!refused(&program("")).is_empty());
    assert!(!refused(&program("invariant a <= b")).is_empty());
    let element = refused(&program("invariant v > 0"));
    assert!(!element.is_empty());
    let false_at_start = refused(&program("invariant a == b + 1"));
    assert!(
        false_at_start.iter().any(|(_, m)| m.contains("before the first pass")),
        "{false_at_start:?}"
    );
}

/// A relation between two names, which no template finds, is what proves
/// `a - b` fits a `Nat` after a loop that changes both.
fn an_invariant_ties_two_names_that_no_template_ties() {
    let program = |head: &str| {
        with_header(format!(
            "fn gap() -> Nat {{\n  var a: 0..11 = 0\n  var b: 0..11 = 0\n  loop i in 0..10 {head} {{\n    a = a + 1\n    b = b + 1\n  }}\n  a - b\n}}\n\n\
             fn main() <console, alloc> {{\n  print(\"{{gap()}}\")\n}}\n"
        ))
    };
    assert_eq!(
        run(&program("invariant a == i and b == a")),
        Outcome {
            stdout: b"0\n".to_vec(),
            status: 0
        }
    );
    assert!(!refused(&program("")).is_empty());
}

/// An `invariant` may start a line of its own, as `decreases` may, a loop may
/// have more than one, and the `{` that opens the body may follow them on a
/// line of its own.
fn an_invariant_on_its_own_line_and_two_of_them_are_read() {
    let source = with_header(
        "fn gap() -> Nat {\n  var a: 0..11 = 0\n  var b: 0..11 = 0\n  loop i in 0..10\n    invariant a == i\n    invariant b == a\n  {\n    a = a + 1\n    b = b + 1\n  }\n  a - b\n}\n\n\
         fn main() <console, alloc> {\n  print(\"{gap()}\")\n}\n"
            .to_string(),
    );
    assert_eq!(
        run(&source),
        Outcome {
            stdout: b"0\n".to_vec(),
            status: 0
        }
    );
}
