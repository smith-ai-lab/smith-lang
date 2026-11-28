//! `smith test`, which runs every function named `test_` of every file named
//! `x_test.smith` below the folder it runs in, says of each whether it passed
//! and what each failure says, and ends with status 1 when one failed.
//!
//! The projects are generated, and the generator knows which of their tests
//! fail and what each failure says.

use std::path::PathBuf;

use crate::Test;
use crate::cli::{smith, stderr};
use crate::generate::{self, Gen};

pub const ONCE: &[Test] = &[(
    "a_test_that_takes_a_parameter_or_gives_another_type_is_refused_with_one_error_naming_the_form",
    a_test_that_takes_a_parameter_or_gives_another_type_is_refused_with_one_error_naming_the_form,
)];

pub const MACHINE: &[Test] = &[
    (
        "a_test_that_passes_and_one_that_fails_are_each_said_and_the_failure_shows_both_sides",
        a_test_that_passes_and_one_that_fails_are_each_said_and_the_failure_shows_both_sides,
    ),
    (
        "every_project_ends_with_status_0_when_its_tests_pass_and_1_when_one_fails",
        every_project_ends_with_status_0_when_its_tests_pass_and_1_when_one_fails,
    ),
];

/// A folder holding the files, each a path below it and its text.
fn project(label: &str, files: &[(String, String)]) -> PathBuf {
    let dir = crate::cli::folder(label);
    for (path, text) in files {
        let path = dir.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    dir
}

fn stdout(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

const ADD: &str = "use std.bits (u32, u64)\n\npub fn add(a: u32, b: u32) -> u64 { a + b }\n";

const ADD_TEST: &str = "use add (add)\nuse std.test (Failed, expect)\n\nfn test_one_and_one() -> Result[(), Failed] {\n  expect { add(1, 1) == 2 }\n}\n\nfn test_wrong() -> Result[(), Failed] {\n  try expect { add(1, 1) == 2 }\n  expect { add(2, 2) == 5 }\n}\n";

/// The example of `tests.md`: a test file imports what it tests, one test
/// passes, and the other fails at its second check, which names the file, the
/// line and column of the `expect`, the check as written, and both sides.
fn a_test_that_passes_and_one_that_fails_are_each_said_and_the_failure_shows_both_sides() {
    let dir = project(
        "add",
        &[
            ("add.smith".to_string(), ADD.to_string()),
            ("add_test.smith".to_string(), ADD_TEST.to_string()),
        ],
    );
    let ran = smith(&dir, &["test"]);
    assert_eq!(ran.status.code(), Some(1), "{}", stderr(&ran));
    let said = stdout(&ran);
    assert!(said.contains("passed add_test.smith test_one_and_one"), "{said}");
    assert!(said.contains("failed add_test.smith test_wrong"), "{said}");
    let line = ADD_TEST.lines().position(|l| l.contains("== 5")).unwrap() + 1;
    let col = ADD_TEST.lines().nth(line - 1).unwrap().find("expect").unwrap() + 1;
    let failure = stderr(&ran);
    let at = format!("add_test.smith:{line}:{col}: ");
    let reported = failure
        .lines()
        .position(|l| l.starts_with(&at) && l.contains("add(2, 2) == 5"))
        .unwrap_or_else(|| panic!("{failure}"));
    let sides: Vec<&str> = failure.lines().skip(reported + 1).take(2).collect();
    assert!(sides[0].trim_start().ends_with(" 4"), "{failure}");
    assert!(sides[1].trim_start().ends_with(" 5"), "{failure}");
}

/// A test: its name, the effects it lists, the lines of its body, and
/// whether it fails.
#[derive(Debug)]
struct Case {
    name: String,
    effects: &'static str,
    body: Vec<String>,
    fails: bool,
}

/// A project of one module, a file of tests for it in the root, and maybe
/// another in a folder, whose tests check a function of the module. Some
/// print with the system's `console`, and some install a handler of their
/// own that prints nothing.
#[derive(Debug)]
struct Project {
    files: Vec<(String, String)>,
    /// The tests of each file of tests, by its path.
    tests: Vec<(String, Vec<Case>)>,
}

fn a_case(g: &mut Gen, k: usize) -> Case {
    let n = g.below(5);
    let fails = g.one_in(3);
    let wanted = if fails { n + 1 } else { n };
    let check = format!("expect {{ double({n}) == {} }}", 2 * wanted);
    let (effects, body) = match g.below(3) {
        0 => ("", vec![check]),
        1 => (
            " <console>",
            vec![format!("print(\"printed by test {k}\")"), check],
        ),
        _ => (
            "",
            vec![
                "with silent()".to_string(),
                format!("print(\"silenced in test {k}\")"),
                check,
            ],
        ),
    };
    Case {
        name: format!("test_{k}"),
        effects,
        body,
        fails,
    }
}

fn test_file(cases: &[Case], module: &str) -> String {
    let mut text = format!(
        "use {module} (double)\nuse std.console (console, input, print)\nuse std.test (Failed, expect)\n\n\
         fn silent[T](body: fn() -> T <console>) -> T {{\n  console(\n    print: fn(text, end, to) {{ }},\n    input: fn(prompt) {{ .none }},\n    body: body,\n  )\n}}\n"
    );
    for case in cases {
        text.push_str(&format!(
            "\nfn {}() -> Result[(), Failed]{} {{\n",
            case.name, case.effects
        ));
        for line in &case.body {
            text.push_str(&format!("  {line}\n"));
        }
        text.push_str("}\n");
    }
    text
}

fn a_project(g: &mut Gen) -> Project {
    let module = "use std.bits (u32, u64)\n\npub fn double(n: u32) -> u64 { n + n }\n";
    let mut files = vec![("calc/double.smith".to_string(), module.to_string())];
    let mut tests = Vec::new();
    let mut k = 0;
    for path in ["calc/double_test.smith", "more_test.smith"] {
        if path == "more_test.smith" && g.one_in(2) {
            continue;
        }
        let cases: Vec<Case> = (0..1 + g.below(3))
            .map(|_| {
                k += 1;
                a_case(g, k)
            })
            .collect();
        files.push((path.to_string(), test_file(&cases, "calc.double")));
        tests.push((path.to_string(), cases));
    }
    Project { files, tests }
}

/// For every project, `smith test` says of each test that it passed or that
/// it failed, ends with status 0 when every test passes and 1 when one fails,
/// and each test runs under the effects it lists: one that lists `console`
/// prints with the system's, and one that installs a handler of `console`
/// prints through it, here nowhere.
fn every_project_ends_with_status_0_when_its_tests_pass_and_1_when_one_fails() {
    generate::check(12471, 3, a_project, |p| {
        let dir = project("tests", &p.files);
        let ran = smith(&dir, &["test"]);
        let said = stdout(&ran);
        let mut any_fails = false;
        for (path, cases) in &p.tests {
            for case in cases {
                let word = if case.fails { "failed" } else { "passed" };
                assert!(
                    said.contains(&format!("{word} {path} {}\n", case.name)),
                    "{said}{}",
                    stderr(&ran)
                );
                let k = &case.name["test_".len()..];
                assert_eq!(
                    said.contains(&format!("printed by test {k}\n")),
                    case.effects.contains("console"),
                    "{said}"
                );
                assert!(!said.contains(&format!("silenced in test {k}")), "{said}");
                any_fails |= case.fails;
            }
        }
        let status = if any_fails { 1 } else { 0 };
        assert_eq!(ran.status.code(), Some(status), "{said}{}", stderr(&ran));
    });
}

/// A wrong form of a test: what is written after its name, and what it
/// is.
const WRONG: [&str; 4] = [
    "(n: Nat) -> Result[(), Failed]",
    "() -> bool",
    "()",
    "() -> Result[Nat, Failed]",
];

/// For every function named `test_` that takes a parameter or does not give
/// `Result[(), Failed]`, `smith test` is refused with one error at its name,
/// which names the form a test has.
fn a_test_that_takes_a_parameter_or_gives_another_type_is_refused_with_one_error_naming_the_form() {
    let generate = |g: &mut Gen| {
        let form = *g.pick(&WRONG);
        let name = format!("test_{}", g.pick(&["a", "sum", "wrong_form"]));
        let body = if form.contains("bool") {
            "true"
        } else if form.ends_with("Failed]") {
            ".ok(1)"
        } else {
            ""
        };
        let first = g.one_in(2);
        let good = "fn test_good() -> Result[(), Failed] {\n  expect { true }\n}\n";
        let bad = format!("fn {name}{form} {{\n  {body}\n}}\n");
        let text = if first {
            format!("use std.test (Failed, expect)\n\n{bad}\n{good}")
        } else {
            format!("use std.test (Failed, expect)\n\n{good}\n{bad}")
        };
        (name, text)
    };
    generate::check(12472, 4, generate, |(name, text)| {
        let dir = project(
            "refused",
            &[("x_test.smith".to_string(), text.to_string())],
        );
        let ran = smith(&dir, &["test"]);
        assert_eq!(ran.status.code(), Some(1), "{}", stderr(&ran));
        let line = text.lines().position(|l| l.contains(name.as_str())).unwrap() + 1;
        let col = text.lines().nth(line - 1).unwrap().find(name.as_str()).unwrap() + 1;
        let said = stderr(&ran);
        let errors: Vec<&str> = said
            .lines()
            .filter(|l| l.starts_with("x_test.smith:"))
            .collect();
        assert_eq!(errors.len(), 1, "{said}");
        assert!(
            errors[0].starts_with(&format!("x_test.smith:{line}:{col}: ")),
            "{said}"
        );
        assert!(errors[0].contains("Result[(), Failed]"), "{said}");
    });
}
