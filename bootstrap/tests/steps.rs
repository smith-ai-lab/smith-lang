//! The steps that the compiler and the evaluator count, which the harness adds
//! up for each test, and `.github/check-work.sh`, which compares what a change
//! counts with what main counts, tested here without GitHub.

use std::path::{Path, PathBuf};
use std::process::Command;

use smith::scalar;
use smith::target::Target;
use smith::work::steps;

use crate::Test;
use crate::lowered_once::{ROUNDS, Written};

pub const ONCE: &[Test] = &[
    (
        "the_same_program_takes_the_same_steps_every_time",
        the_same_program_takes_the_same_steps_every_time,
    ),
    (
        "each_line_of_a_body_adds_the_same_steps_to_checking_and_lowering_it",
        each_line_of_a_body_adds_the_same_steps_to_checking_and_lowering_it,
    ),
    (
        "the_passes_take_a_step_for_each_instruction_they_are_given",
        the_passes_take_a_step_for_each_instruction_they_are_given,
    ),
    (
        "each_round_of_a_loop_adds_the_same_steps_to_running_it",
        each_round_of_a_loop_adds_the_same_steps_to_running_it,
    ),
    (
        "tests_that_take_about_the_steps_they_take_on_main_pass",
        tests_that_take_about_the_steps_they_take_on_main_pass,
    ),
    (
        "a_test_that_takes_many_more_steps_fails_a_description_that_does_not_say_why",
        a_test_that_takes_many_more_steps_fails_a_description_that_does_not_say_why,
    ),
    (
        "a_description_that_says_why_passes_and_the_summary_names_the_tests_and_both_counts",
        a_description_that_says_why_passes_and_the_summary_names_the_tests_and_both_counts,
    ),
    (
        "a_new_test_is_not_compared",
        a_new_test_is_not_compared,
    ),
];

/// The steps that `f` takes, on this thread.
fn taken<T>(f: impl FnOnce() -> T) -> (T, u64) {
    let before = steps();
    let value = f();
    (value, steps() - before)
}

const HELLO: &str = "use std.console (console, print)\n\nfn main() <console> { print(\"hello\") }\n";

fn the_same_program_takes_the_same_steps_every_time() {
    for target in Target::ALL {
        let built = |_| taken(|| smith::compile(HELLO.as_bytes(), *target));
        let (bytes, first) = built(());
        assert!(first > 0, "{}", target.name());
        for _ in 0..2 {
            let (again, count) = built(());
            assert_eq!(again, bytes, "{}", target.name());
            assert_eq!(count, first, "{}", target.name());
        }
    }
    let program = smith::lower(HELLO.as_bytes()).unwrap();
    let ran = |_| taken(|| scalar::run(&scalar::passes(&program)));
    let (outcome, first) = ran(());
    assert_eq!(outcome.stdout, b"hello\n");
    assert_eq!(ran(()).1, first);
}

/// A program that prints a line `lines` times, each with a call of its own.
fn printing(lines: usize) -> String {
    let body = "  print(\"a\")\n".repeat(lines);
    format!("use std.console (console, print)\n\nfn main() <console> {{\n{body}}}\n")
}

fn each_line_of_a_body_adds_the_same_steps_to_checking_and_lowering_it() {
    let counts: Vec<u64> = (2..7)
        .map(|lines| {
            let source = printing(lines);
            taken(|| smith::lower(source.as_bytes()).unwrap()).1
        })
        .collect();
    let added: Vec<u64> = counts.windows(2).map(|w| w[1] - w[0]).collect();
    assert!(added[0] > 0, "{counts:?}");
    assert!(added.iter().all(|&a| a == added[0]), "{counts:?}");
}

/// The instructions of a program, the end of each block counted as one.
fn size(program: &scalar::Program) -> u64 {
    program.functions.iter().map(|f| scalar::size(f) as u64).sum()
}

fn the_passes_take_a_step_for_each_instruction_they_are_given() {
    let program = smith::lower(HELLO.as_bytes()).unwrap();
    let (after, count) = taken(|| scalar::passes(&program));
    // The first pass is given the program, and the last is given at least
    // what it gives.
    assert!(count >= size(&program) + size(&after), "{count}");
    // Each of the seven is given no more than the largest program among them,
    // which inlining makes, and which is never ten times what it was given.
    assert!(count <= 7 * 10 * size(&program), "{count}");
}

fn each_round_of_a_loop_adds_the_same_steps_to_running_it() {
    let written = Written::new(format!(
        "fn main() {{\n  mut total = 0\n  loop i in 0..{ROUNDS} {{ total = total + i }}\n}}\n"
    ));
    let counts: Vec<u64> = [10, 20, 30]
        .iter()
        .map(|&n| {
            let program = written.with(&[(ROUNDS, n)]);
            let (outcome, count) = taken(|| scalar::run(&program));
            assert_eq!(outcome.status, 0);
            assert_eq!(scalar::run_measured(&program).1, count);
            count
        })
        .collect();
    assert!(counts[1] > counts[0], "{counts:?}");
    assert_eq!(counts[2] - counts[1], counts[1] - counts[0], "{counts:?}");
}

fn github() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../.github")
}

/// What `check-work.sh` says and whether it passes, for the steps of main and
/// of a change, and the description of the change.
fn checked(main: &str, change: &str, description: &str) -> (bool, String) {
    let dir = crate::scratch::path("steps");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main"), main).unwrap();
    std::fs::write(dir.join("change"), change).unwrap();
    std::fs::write(dir.join("description"), description).unwrap();
    let output = crate::clock::output_within(
        Command::new("sh")
            .arg(github().join("check-work.sh"))
            .arg(dir.join("main"))
            .arg(dir.join("change"))
            .arg(dir.join("description")),
        false,
    )
    .unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    (
        output.status.success(),
        String::from_utf8(output.stdout).unwrap(),
    )
}

const MAIN: &str = "large,1000000\nsmall,1000\nother,500000\n";

fn tests_that_take_about_the_steps_they_take_on_main_pass() {
    for change in [
        MAIN,
        // Fewer steps.
        "large,10\nsmall,1\nother,500000\n",
        // 1.5 times exactly, which is not more.
        "large,1500000\nsmall,1000\nother,500000\n",
        // A hundred times, but by less than the floor.
        "large,1000000\nsmall,100000\nother,500000\n",
        // More than the floor, but less than 1.5 times.
        "large,1400000\nsmall,1000\nother,500000\n",
    ] {
        let (passed, said) = checked(MAIN, change, "Moves a function.\n");
        assert!(passed, "{change:?}: {said}");
        assert!(said.starts_with("No test takes more"), "{said}");
    }
}

/// `large` is over both, and `small` is over 1.5 times its steps by 100001.
const MORE: &str = "large,1500001\nsmall,101001\nother,500000\n";

fn a_test_that_takes_many_more_steps_fails_a_description_that_does_not_say_why() {
    // Saying it in other words, inside a line, or without a reason is not
    // saying why.
    for description in [
        "",
        "Moves a function.\n",
        "This change does more work.\n",
        "It Does more work. as it must\n",
        "Does more work.\n",
        "Does more work.   \n",
    ] {
        let (passed, said) = checked(MAIN, MORE, description);
        assert!(!passed, "{description:?}: {said}");
        assert!(
            said.contains("- large: 1500001 steps, where main takes 1000000"),
            "{said}"
        );
        assert!(
            said.contains("- small: 101001 steps, where main takes 1000"),
            "{said}"
        );
        assert!(!said.contains("other"), "{said}");
    }
}

fn a_description_that_says_why_passes_and_the_summary_names_the_tests_and_both_counts() {
    for description in [
        "Does more work. The checker proves every bound now.\n",
        "Moves a function.\r\n\r\nDoes more work. Each loop is checked twice.\r\n\r\nCloses #1\n",
    ] {
        let (passed, said) = checked(MAIN, MORE, description);
        assert!(passed, "{description:?}: {said}");
        assert!(
            said.contains("- large: 1500001 steps, where main takes 1000000"),
            "{said}"
        );
        assert!(said.contains("says why"), "{said}");
    }
}

fn a_new_test_is_not_compared() {
    let change = "large,1000000\nnew,90000000\nsmall,1000\nother,500000\n";
    let (passed, said) = checked(MAIN, change, "");
    assert!(passed, "{said}");
}
