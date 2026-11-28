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

pub const ONCE: &[Test] = &[
    (
        "a_test_that_takes_a_parameter_or_gives_another_type_is_refused_with_one_error_naming_the_form",
        a_test_that_takes_a_parameter_or_gives_another_type_is_refused_with_one_error_naming_the_form,
    ),
    (
        "a_run_of_many_tests_asks_the_system_for_the_memory_of_a_run_of_a_few",
        a_run_of_many_tests_asks_the_system_for_the_memory_of_a_run_of_a_few,
    ),
];

pub const MACHINE: &[Test] = &[
    (
        "a_file_or_a_name_runs_only_its_tests_and_says_them_as_a_full_run_does",
        a_file_or_a_name_runs_only_its_tests_and_says_them_as_a_full_run_does,
    ),
    (
        "every_test_gives_alone_what_it_gives_in_a_run_of_every_test",
        every_test_gives_alone_what_it_gives_in_a_run_of_every_test,
    ),
    (
        "a_run_of_one_file_builds_only_what_it_reaches_and_a_mistake_elsewhere_is_not_said",
        a_run_of_one_file_builds_only_what_it_reaches_and_a_mistake_elsewhere_is_not_said,
    ),
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
    // A test that waits, so that the tests of a wave run in turns, which is
    // not made on Windows, where the clock is not built yet.
    let kinds = if cfg!(windows) { 3 } else { 4 };
    let (effects, body) = match g.below(kinds) {
        0 => ("", vec![check]),
        3 => (" <time>", vec!["sleep(ms(1))".to_string(), check]),
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
        "use {module} (double)\nuse std.console (console, input, print)\nuse std.instant (ms)\nuse std.test (Failed, expect)\nuse std.time (sleep, time)\n\n\
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

/// A file of `count` tests, each of which builds a list of its own, as the
/// module `many_test`. Every other test fails, comparing its list with one
/// that is one longer, so that what its failure says, with both lists, is
/// long.
fn building(count: usize) -> String {
    let mut text = "use std.bits (u64)\nuse std.list (push)\nuse std.test (Failed, expect)\n".to_string();
    for k in 0..count {
        let longer = 200 + k % 2;
        text.push_str(&format!(
            "\nfn test_{k}() -> Result[(), Failed] <alloc> {{\n  mut xs: List[u64] = []\n  loop i in 0..200 {{ xs = push(take xs, i) }}\n  mut ys: List[u64] = []\n  loop i in 0..{longer} {{ ys = push(take ys, i) }}\n  expect {{ xs == ys }}\n}}\n"
        ));
    }
    text
}

/// The program `smith test` builds of `many_test` with `count` tests, run in
/// the evaluator, `width` at a time: what it prints, and the bytes it asks the
/// system for.
fn run_of_building(count: usize, width: usize) -> (String, usize) {
    let source = building(count);
    let tests = smith::testing::read(&source).unwrap_or_else(|e| panic!("{e:?}"));
    let chosen: Vec<(usize, &str)> = tests.tests.iter().enumerate().map(|(k, t)| (k, t.name.as_str())).collect();
    let module = tests.module(&source, &chosen);
    let file = smith::testing::File { module: "many_test", tests: &tests, chosen: &chosen };
    let program = smith::testing::program(&[file], width, 0);
    let lowered = smith::lower_project(program.as_bytes(), &[("many_test", &module)])
        .unwrap_or_else(|e| panic!("{e:?}\n{program}"));
    let (outcome, requests) = smith::scalar::run_counting(&smith::scalar::passes(&lowered), usize::MAX);
    assert_eq!(outcome.status, 0, "{}", String::from_utf8_lossy(&outcome.stdout));
    (String::from_utf8_lossy(&outcome.stdout).into_owned(), requests.iter().sum())
}

/// Each test runs as a task with an arena of its own, which is given back when
/// it ends, and what a wave of tests gave is given back when the wave ends, so
/// a run of 24 tests that each build lists, half of which fail and say both
/// lists, asks the system for the memory that a run of 2 does: the memory of a
/// test that has ended is not held.
fn a_run_of_many_tests_asks_the_system_for_the_memory_of_a_run_of_a_few() {
    let (few_said, few) = run_of_building(2, 2);
    let (many_said, many) = run_of_building(24, 2);
    for (said, count) in [(&few_said, 2), (&many_said, 24)] {
        assert_eq!(said.matches("@smith-test passed").count(), count / 2, "{said}");
        assert_eq!(said.matches("@smith-test failed").count(), count / 2, "{said}");
    }
    assert!(
        many.saturating_sub(few) <= smith::scalar::pool::CHUNK as usize,
        "{few} bytes for 2 tests and {many} for 24"
    );
}

/// What a run says of each test: its file and name, and whether it passed.
fn results(said: &str) -> Vec<(String, bool)> {
    said.lines()
        .filter_map(|l| {
            l.strip_prefix("passed ")
                .map(|t| (t.to_string(), true))
                .or_else(|| l.strip_prefix("failed ").map(|t| (t.to_string(), false)))
        })
        .collect()
}

/// That a run that said `ran` of its tests ends as every run does: with how
/// many passed and failed, then its slowest tests, at most five, each a test it
/// ran with how long it took, the slowest first, and with status 1 when one
/// failed.
fn ends_as_a_run(output: &std::process::Output, ran: &[(String, bool)]) {
    let said = stdout(output);
    let passed = ran.iter().filter(|(_, p)| *p).count();
    let failed = ran.len() - passed;
    let lines: Vec<&str> = said.lines().collect();
    let summary = (lines.iter())
        .position(|l| *l == format!("{passed} passed, {failed} failed"))
        .unwrap_or_else(|| panic!("{said}"));
    let slowest = &lines[summary + 1..];
    if ran.is_empty() {
        assert!(slowest.is_empty(), "{said}");
    } else {
        assert_eq!(slowest[0], "slowest:", "{said}");
        let listed: Vec<(u64, &str)> = (slowest[1..].iter())
            .map(|l| {
                let (ms, test) = l.trim_start().split_once(" ms ").unwrap_or_else(|| panic!("{said}"));
                (ms.parse().unwrap_or_else(|_| panic!("{said}")), test)
            })
            .collect();
        assert_eq!(listed.len(), ran.len().min(5), "{said}");
        assert!(listed.windows(2).all(|w| w[0].0 >= w[1].0), "{said}");
        for (_, test) in &listed {
            assert!(ran.iter().any(|(t, _)| t == test), "{said}");
        }
    }
    let status = if failed > 0 { 1 } else { 0 };
    assert_eq!(output.status.code(), Some(status), "{said}{}", stderr(output));
}

/// Each test of the project, as a run says it, and whether it passes.
fn expected(p: &Project) -> Vec<(String, bool)> {
    (p.tests.iter())
        .flat_map(|(path, cases)| cases.iter().map(move |c| (format!("{path} {}", c.name), !c.fails)))
        .collect()
}

/// For every project, `smith test <file>` runs the tests of that file and
/// `smith test --name <text>` the tests whose name holds the text, and each
/// says them and ends as a run of every test does.
fn a_file_or_a_name_runs_only_its_tests_and_says_them_as_a_full_run_does() {
    let generate = |g: &mut Gen| {
        let p = a_project(g);
        let file = g.below(p.tests.len());
        let all = expected(&p);
        let name = all[g.below(all.len())].0.split(' ').nth(1).unwrap().to_string();
        // A part of a name, which other names may hold too.
        let text = name[..name.len() - g.below(2)].to_string();
        (p, file, text)
    };
    generate::check(12481, 3, generate, |(p, file, text)| {
        let dir = project("chosen", &p.files);
        let every = smith(&dir, &["test"]);
        let all = expected(p);
        assert_eq!(results(&stdout(&every)), all, "{}{}", stdout(&every), stderr(&every));
        ends_as_a_run(&every, &all);
        let path = &p.tests[*file].0;
        let of_file = smith(&dir, &["test", path]);
        let wanted: Vec<(String, bool)> = (all.iter())
            .filter(|(t, _)| t.starts_with(&format!("{path} ")))
            .cloned()
            .collect();
        assert_eq!(results(&stdout(&of_file)), wanted, "{}", stdout(&of_file));
        ends_as_a_run(&of_file, &wanted);
        let named = smith(&dir, &["test", "--name", text]);
        let wanted: Vec<(String, bool)> = (all.iter())
            .filter(|(t, _)| t.split(' ').nth(1).unwrap().contains(text.as_str()))
            .cloned()
            .collect();
        assert_eq!(results(&stdout(&named)), wanted, "{}", stdout(&named));
        ends_as_a_run(&named, &wanted);
    });
}

/// For every project, a run of every test, where several tasks run at once,
/// gives each test the result it gives in a run of that test alone.
fn every_test_gives_alone_what_it_gives_in_a_run_of_every_test() {
    generate::check(12482, 2, a_project, |p| {
        let dir = project("alone", &p.files);
        let every = smith(&dir, &["test"]);
        let together = results(&stdout(&every));
        assert_eq!(together.len(), expected(p).len(), "{}", stdout(&every));
        for (test, passed) in &together {
            let (path, name) = test.split_once(' ').unwrap();
            let alone = smith(&dir, &["test", path, "--name", name]);
            assert_eq!(
                results(&stdout(&alone)),
                [(test.clone(), *passed)],
                "{}",
                stdout(&alone)
            );
        }
    });
}

/// A mistake in a module, which a file of tests imports.
const MISTAKES: [&str; 3] = [
    "pub fn broken() -> u64 { true }\n",
    "pub fn broken( {\n",
    "pub fn broken() -> u64 { missing() }\n",
];

/// For every mistake in a module that only one file of tests imports, a run of
/// another file does not build that module, so it says nothing of the mistake
/// and passes, and a run of every test says it.
fn a_run_of_one_file_builds_only_what_it_reaches_and_a_mistake_elsewhere_is_not_said() {
    let generate = |g: &mut Gen| *g.pick(&MISTAKES);
    generate::check(12483, 3, generate, |mistake| {
        let good = "use std.test (Failed, expect)\n\nfn test_fine() -> Result[(), Failed] {\n  expect { 1 + 1 == 2 }\n}\n";
        let bad = "use lib.broken (broken)\nuse std.test (Failed, expect)\n\nfn test_broken() -> Result[(), Failed] {\n  expect { broken() == 1 }\n}\n";
        let dir = project(
            "reaches",
            &[
                ("lib/broken.smith".to_string(), format!("use std.bits (u64)\n\n{mistake}")),
                ("good_test.smith".to_string(), good.to_string()),
                ("bad_test.smith".to_string(), bad.to_string()),
            ],
        );
        let one = smith(&dir, &["test", "good_test.smith"]);
        assert!(!stderr(&one).contains("broken"), "{}", stderr(&one));
        ends_as_a_run(&one, &[("good_test.smith test_fine".to_string(), true)]);
        let every = smith(&dir, &["test"]);
        assert!(stderr(&every).contains("lib/broken.smith:"), "{}", stderr(&every));
        assert_eq!(every.status.code(), Some(1), "{}", stderr(&every));
    });
}
