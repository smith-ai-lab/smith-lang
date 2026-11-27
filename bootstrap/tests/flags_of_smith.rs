//! The flags of `smith` are written before the command, a command's flags
//! after it, and every word after the file of `smith run` is the program's.

use std::path::Path;
use std::process::Command;

use smith::target::Target;

use crate::Test;
use crate::cli::{folder, smith, stderr, timing_lines};
use crate::generate::{self, Gen};

pub const ONCE: &[Test] = &[
    (
        "a_flag_of_smith_after_the_command_is_refused_with_where_it_goes",
        a_flag_of_smith_after_the_command_is_refused_with_where_it_goes,
    ),
    (
        "a_flag_of_a_command_before_it_is_refused_and_watch_is_not_built_yet",
        a_flag_of_a_command_before_it_is_refused_and_watch_is_not_built_yet,
    ),
    (
        "a_flag_of_smith_given_twice_or_a_word_that_is_no_flag_before_the_command_is_refused",
        a_flag_of_smith_given_twice_or_a_word_that_is_no_flag_before_the_command_is_refused,
    ),
    (
        "a_task_is_timed_by_the_flag_before_it_and_a_parameter_of_the_name_is_given_its_value",
        a_task_is_timed_by_the_flag_before_it_and_a_parameter_of_the_name_is_given_its_value,
    ),
    (
        "every_word_after_the_file_of_run_is_given_to_the_program_unchanged",
        every_word_after_the_file_of_run_is_given_to_the_program_unchanged,
    ),
];

const HELLO: &str = "use std.console (console, print)\n\nfn main() <console> {\n  print(\"hello world\")\n}\n";

const EACH: &str = "use std.console (console, print)\n\nfn main(args: List[str]) <console, alloc> {\n  loop i in 0..args.size {\n    print(args[i])\n  }\n}\n";

const TASKS_MAIN: &str = "use std.console (console, print)\n\npub fn main() <console> {\n  print(\"main\")\n}\n";

fn runs_here() -> bool {
    Target::host().is_some()
}

/// A project with the files, in a folder of its own.
fn project(name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
    let dir = folder(name);
    for (file, text) in files {
        std::fs::write(dir.join(file), text).unwrap();
    }
    dir
}

const FLAGS_OF_SMITH: [&str; 2] = ["--timings", "--watch"];

/// A project with the task `probe`, which has the parameters of the names.
fn probe_project(name: &str, parameters: &[&str]) -> std::path::PathBuf {
    let declared: Vec<String> = parameters.iter().map(|p| format!("{p}: str = \"-\"")).collect();
    let parameters_text = if declared.is_empty() { String::new() } else { format!("; {}", declared.join(", ")) };
    let prints: String = parameters.iter().map(|p| format!("  print({p})\n")).collect();
    let text = format!(
        "use std.console (console, print)\n\npub fn probe({}) <console> {{\n{prints}  print(\"ran\")\n}}\n",
        parameters_text
    );
    project(name, &[("main.smith", TASKS_MAIN), ("project.smith", &text)])
}

fn a_flag_of_smith_after_the_command_is_refused_with_where_it_goes() {
    let dir = probe_project("flags-after", &[]);
    std::fs::write(dir.join("hello.smith"), HELLO).unwrap();
    // `run` takes every word after the file as the program's, so only the
    // words before the file are flags of `run`.
    let places: [(&str, &[&str]); 7] = [
        ("build", &["build", "FLAG", "hello.smith"]),
        ("build", &["build", "hello.smith", "FLAG"]),
        ("build", &["build", "FLAG"]),
        ("check", &["check", "FLAG", "hello.smith"]),
        ("check", &["check", "hello.smith", "FLAG"]),
        ("run", &["run", "FLAG", "hello.smith"]),
        ("probe", &["probe", "FLAG"]),
    ];
    generate::check(
        9511,
        8,
        |g: &mut Gen| (*g.pick(&FLAGS_OF_SMITH), *g.pick(&places)),
        |(flag, (command, words))| {
            let given: Vec<&str> = words.iter().map(|w| if *w == "FLAG" { *flag } else { *w }).collect();
            let said = smith(&dir, &given);
            let err = stderr(&said);
            assert_eq!(said.status.code(), Some(1), "{given:?}: {err}");
            assert!(
                err.contains(&format!("`{flag}` is a flag of `smith`, written before the command: smith {flag} {command}")),
                "{given:?}: {err}"
            );
            assert!(!dir.join("hello").exists() && !dir.join("probe").exists(), "{given:?}");
        },
    );
}

fn a_flag_of_a_command_before_it_is_refused_and_watch_is_not_built_yet() {
    let dir = probe_project("flags-before", &[]);
    std::fs::write(dir.join("hello.smith"), HELLO).unwrap();
    let target = crate::targets::any().name();
    for command in ["build", "run"] {
        let said = smith(&dir, &["--target", target, command, "hello.smith"]);
        let err = stderr(&said);
        assert_eq!(said.status.code(), Some(1), "{command}: {err}");
        assert!(err.contains("`--target` is a flag of `build` and `run`, written after the command"), "{command}: {err}");
        assert!(!dir.join("hello").exists(), "{command}");
    }
    for command in [&["build", "hello.smith"][..], &["run", "hello.smith"], &["check", "hello.smith"], &["probe"]] {
        let mut words = vec!["--watch"];
        words.extend_from_slice(command);
        let said = smith(&dir, &words);
        let err = stderr(&said);
        assert_eq!(said.status.code(), Some(1), "{words:?}: {err}");
        assert!(err.contains("--watch") && err.contains("not built yet"), "{words:?}: {err}");
        assert!(!dir.join("hello").exists() && !dir.join("probe").exists(), "{words:?}");
    }
}

/// What `smith` prints with both streams in one, so that the order is seen.
fn together(dir: &Path, words: &[&str]) -> (Option<i32>, String) {
    let ran = crate::clock::output_within(
        Command::new("sh")
            .args(["-c", "\"$0\" \"$@\" 2>&1"])
            .arg(env!("CARGO_BIN_EXE_smith"))
            .args(words)
            .current_dir(dir),
        true,
    )
    .unwrap();
    (ran.status.code(), String::from_utf8(ran.stdout).unwrap())
}

fn a_task_is_timed_by_the_flag_before_it_and_a_parameter_of_the_name_is_given_its_value() {
    if !runs_here() {
        return;
    }
    // Which of `timings` and `watch` the task has as parameters, and a value
    // for each that can look like a flag of `smith`.
    let values = ["x", "a b", "--watch", "--timings", "--target"];
    generate::check(
        9512,
        6,
        |g: &mut Gen| {
            let has: Vec<&str> = FLAGS_OF_SMITH.iter().map(|f| &f[2..]).filter(|_| g.one_in(2)).collect();
            (has, *g.pick(&FLAGS_OF_SMITH), *g.pick(&values), g.one_in(2))
        },
        |(has, flag, value, timed)| {
            let dir = probe_project("flags-task", has);
            let name = &flag[2..];
            let prefix: Vec<&str> = if *timed { vec!["--timings"] } else { vec![] };
            let mut words = prefix.clone();
            words.extend(["probe", flag, value]);
            let (code, text) = together(&dir, &words);
            if has.contains(&name) {
                // The parameter is given the value, and the other is as it was.
                assert_eq!(code, Some(0), "{words:?}: {text}");
                let printed: Vec<&str> = has.iter().map(|p| if *p == name { *value } else { "-" }).collect();
                let expected = format!("{}ran\n", printed.iter().map(|l| format!("{l}\n")).collect::<String>());
                let program = text.strip_suffix(&expected).unwrap_or_else(|| panic!("{words:?}: {text}"));
                assert_eq!(program.lines().any(|l| l.starts_with("lex ")), *timed, "{words:?}: {text}");
            } else {
                assert_eq!(code, Some(1), "{words:?}: {text}");
                let command = "probe";
                assert!(
                    text.contains(&format!("`{flag}` is a flag of `smith`, written before the command: smith {flag} {command}")),
                    "{words:?}: {text}"
                );
                assert!(!text.contains("ran"), "{words:?}: {text}");
            }

            // The flag before the command times the build of the task, which
            // is done before the task runs.
            let (code, text) = together(&dir, &["--timings", "probe"]);
            assert_eq!(code, Some(0), "{text}");
            let program = text.strip_suffix(&format!("{}ran\n", "-\n".repeat(has.len()))).unwrap_or_else(|| panic!("{text}"));
            let names: Vec<String> = timing_lines(program).into_iter().map(|(n, _)| n).collect();
            assert_eq!(names.last().map(String::as_str), Some("total"), "{text}");
            assert!(names.contains(&"emit".to_string()), "{text}");
        },
    );
}

fn every_word_after_the_file_of_run_is_given_to_the_program_unchanged() {
    if !runs_here() {
        return;
    }
    let dir = folder("flags-words");
    std::fs::write(dir.join("each.smith"), EACH).unwrap();
    let pool = ["--timings", "--watch", "--target", "--map", "--nope", "-x", "", "a b", "build", "each.smith"];
    generate::check(
        9513,
        6,
        |g: &mut Gen| {
            let count = g.below(5);
            (0..count).map(|_| *g.pick(&pool)).collect::<Vec<&str>>()
        },
        |words| {
            let mut given = vec!["run", "each.smith"];
            given.extend(words);
            let ran = smith(&dir, &given);
            assert_eq!(ran.status.code(), Some(0), "{words:?}: {}", stderr(&ran));
            let expected: String = words.iter().map(|w| format!("{w}\n")).collect();
            assert_eq!(String::from_utf8_lossy(&ran.stdout), expected, "{words:?}");
            assert!(ran.stderr.is_empty(), "{words:?}: {}", stderr(&ran));
        },
    );
}

fn a_flag_of_smith_given_twice_or_a_word_that_is_no_flag_before_the_command_is_refused() {
    let dir = project("flags-twice", &[("hello.smith", HELLO)]);
    let said = smith(&dir, &["--timings", "--timings", "build", "hello.smith"]);
    let err = stderr(&said);
    assert_eq!(said.status.code(), Some(1), "{err}");
    assert!(err.contains("`--timings` is given twice"), "{err}");
    assert!(!dir.join("hello").exists());

    generate::check(
        9514,
        6,
        |g: &mut Gen| {
            let length = 1 + g.below(6);
            let name: String = (0..length).map(|_| *g.pick(b"abxyz") as char).collect();
            format!("{}{name}", *g.pick(&["-", "--"]))
        },
        |flag| {
            let said = smith(&dir, &[flag, "build", "hello.smith"]);
            let err = stderr(&said);
            assert_eq!(said.status.code(), Some(1), "{flag}: {err}");
            assert!(err.contains(&format!("unknown flag `{flag}`")) && err.contains("--timings") && err.contains("--watch"), "{flag}: {err}");
            assert!(!err.contains("unknown command"), "{flag}: {err}");
            assert!(!dir.join("hello").exists(), "{flag}");
        },
    );
}
