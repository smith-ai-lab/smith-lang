//! A task of `project.smith`, a public function of it, is built and run by its
//! name, its parameters are its flags, and a deployment that installs what
//! answers the effects of `main` is where an executable starts.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use smith::target::Target;

use crate::Test;
use crate::cli::{folder, smith, stderr};
use crate::generate::{self, Gen};
use crate::standard::{Json, parse};

pub const ONCE: &[Test] = &[
    (
        "a_task_built_with_its_name_answers_a_client_that_starts_it_and_a_build_with_no_name_is_as_it_was",
        a_task_built_with_its_name_answers_a_client_that_starts_it_and_a_build_with_no_name_is_as_it_was,
    ),
    (
        "main_is_listed_by_project_smith_only_when_it_is_pub_and_a_pub_main_is_built_without_a_name",
        main_is_listed_by_project_smith_only_when_it_is_pub_and_a_pub_main_is_built_without_a_name,
    ),
    (
        "an_error_of_a_task_is_at_a_line_of_project_smith",
        an_error_of_a_task_is_at_a_line_of_project_smith,
    ),
    (
        "each_flag_is_spelled_as_its_parameter_and_gives_it_its_value",
        each_flag_is_spelled_as_its_parameter_and_gives_it_its_value,
    ),
    (
        "a_command_line_that_does_not_fit_is_one_error_that_names_the_flag",
        a_command_line_that_does_not_fit_is_one_error_that_names_the_flag,
    ),
    (
        "a_task_is_run_by_its_name_and_one_that_gives_an_error_prints_it_and_fails",
        a_task_is_run_by_its_name_and_one_that_gives_an_error_prints_it_and_fails,
    ),
    (
        "a_deployment_that_leaves_an_effect_of_main_unanswered_is_refused_for_a_native_target",
        a_deployment_that_leaves_an_effect_of_main_unanswered_is_refused_for_a_native_target,
    ),
    (
        "a_task_named_as_a_command_is_refused_where_it_is_written",
        a_task_named_as_a_command_is_refused_where_it_is_written,
    ),
];

/// The first block of the page of `std.mcp` that holds the text, so that the
/// page cannot drift from what builds.
fn from_the_page(holds: &str) -> String {
    let page = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../knowledge/library/mcp.md")).unwrap();
    page.split("```")
        .skip(1)
        .step_by(2)
        .find(|block| block.contains(holds))
        .unwrap_or_else(|| panic!("no block of the page holds {holds}"))
        .trim_start_matches('\n')
        .to_string()
}

const META: &str = "\"_meta\":{\"io.modelcontextprotocol/protocolVersion\":\"2026-07-28\",\"io.modelcontextprotocol/clientCapabilities\":{}}";

/// A project with the files, in a folder of its own.
fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = folder(name);
    for (file, text) in files {
        std::fs::write(dir.join(file), text).unwrap();
    }
    dir
}

/// Whether this machine is a target, which a test that runs a task needs.
fn runs_here() -> bool {
    Target::host().is_some()
}

fn run(executable: &Path, words: &[String], input: &[u8]) -> Output {
    crate::clock::output_within_given(Command::new(executable).args(words), input, true).unwrap()
}

fn a_task_built_with_its_name_answers_a_client_that_starts_it_and_a_build_with_no_name_is_as_it_was() {
    let (main, project_text) = (from_the_page("pub fn main"), from_the_page("pub fn local"));
    let dir = project("calc", &[("main.smith", &main), ("project.smith", &project_text)]);
    let built = smith(&dir, &["build", "local"]);
    assert!(built.status.success(), "{}", stderr(&built));
    let target = Target::host();
    let executable = dir.join(target.map_or("calc", |t| if t.name().contains("windows") { "calc.exe" } else { "calc" }));
    assert!(executable.is_file(), "the executable is named after the package");
    if runs_here() {
        let list = format!("{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/list\",\"params\":{{{META}}}}}\n");
        let call = format!("{{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{{\"name\":\"divide\",\"arguments\":{{\"a\":6,\"b\":3}},{META}}}}}\n");
        let said = run(&executable, &[], format!("{list}{call}").as_bytes());
        assert!(said.status.success());
        let out = String::from_utf8(said.stdout).unwrap();
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 2, "{out}");
        let tools = parse(lines[0]).get("result").and_then(|r| r.get("tools")).cloned();
        assert!(matches!(tools, Some(Json::Array(t)) if t.len() == 1), "{}", lines[0]);
        let answered = parse(lines[1]).get("result").unwrap().clone();
        assert_eq!(answered.get("structuredContent"), Some(&Json::Number("2".into())));
    }

    // A build with no name starts at `main`, whether or not a `project.smith`
    // is beside it, and writes the same bytes.
    let program = "use std.console (console, print)\n\nfn main() <console> {\n  print(\"hello\")\n}\n";
    let with = project("start-with", &[("main.smith", program), ("project.smith", "use std.project (Package)\n\npackage = Package(name: \"hi\")\n")]);
    let without = project("start-without", &[("main.smith", program)]);
    for dir in [&with, &without] {
        let built = smith(dir, &["build"]);
        assert!(built.status.success(), "{}", stderr(&built));
    }
    let first = std::fs::read(with.join("start-with")).or_else(|_| std::fs::read(with.join("hi")));
    let second = std::fs::read(without.join("start-without"));
    assert_eq!(first.unwrap(), second.unwrap());
}

const TASKS_MAIN: &str = "use std.console (console, print)\n\npub fn main() <console> {\n  print(\"main\")\n}\n";

const TASKS: &str = "use std.console (console, print)\nuse std.project (Package)\n\npackage = Package(name: \"tasks\")\n\npub fn deploy(env: str, verbose: bool, note: Option[str], region: str = \"eu\", count: 1..100 = 3) -> Result[(), str] <console, alloc> {\n  print(\"deploy {env} {verbose} {region} {count}\")\n  match note {\n    .some(n) { print(\"note {n}\") }\n    .none { }\n  }\n  .ok(())\n}\n\npub fn fail() -> Result[(), str] <console> {\n  .err(\"it broke\")\n}\n";

/// The executable of the task `deploy`, built once for a test.
fn deploy() -> PathBuf {
    let dir = project("tasks", &[("main.smith", TASKS_MAIN), ("project.smith", TASKS)]);
    let built = smith(&dir, &["build", "deploy"]);
    assert!(built.status.success(), "{}", stderr(&built));
    dir.join(if Target::host().is_some_and(|t| t.name().contains("windows")) { "tasks.exe" } else { "tasks" })
}

/// A command line for `deploy` that fits, and what it must print.
#[derive(Debug)]
struct Fits {
    words: Vec<String>,
    printed: Vec<String>,
}

fn a_command_line(g: &mut Gen) -> Fits {
    let env = format!("e{}", g.below(1000));
    let verbose = g.one_in(2);
    let mut words = vec!["--env".to_string(), env.clone()];
    let mut printed = String::new();
    if verbose {
        words.push("--verbose".to_string());
    }
    let region = if g.one_in(2) {
        let r = format!("r{}", g.below(100));
        words.push("--region".to_string());
        words.push(r.clone());
        r
    } else {
        "eu".to_string()
    };
    let count = if g.one_in(2) {
        let c = 1 + g.below(99);
        words.extend(["--count".to_string(), c.to_string()]);
        c
    } else {
        3
    };
    printed.push_str(&format!("deploy {env} {verbose} {region} {count}"));
    let mut lines = vec![printed];
    if g.one_in(2) {
        let n = format!("n{}", g.below(100));
        words.extend(["--note".to_string(), n.clone()]);
        lines.push(format!("note {n}"));
    }
    // The order the flags are given in does not matter.
    if g.one_in(2) {
        let pairs: Vec<Vec<String>> = {
            let mut pairs = Vec::new();
            let mut i = 0;
            while i < words.len() {
                let takes = !matches!(words[i].as_str(), "--verbose");
                let end = if takes { i + 2 } else { i + 1 };
                pairs.push(words[i..end].to_vec());
                i = end;
            }
            pairs
        };
        words = pairs.into_iter().rev().flatten().collect();
    }
    Fits { words, printed: lines }
}

fn each_flag_is_spelled_as_its_parameter_and_gives_it_its_value() {
    if !runs_here() {
        return;
    }
    let executable = deploy();
    generate::check(9501, 12, a_command_line, |fits: &Fits| {
        let said = run(&executable, &fits.words, b"");
        assert!(said.status.success(), "{}", String::from_utf8_lossy(&said.stderr));
        let out = String::from_utf8(said.stdout).unwrap();
        assert_eq!(out.lines().collect::<Vec<_>>(), fits.printed, "for {:?}", fits.words);
    });
}

/// A command line that does not fit, and the flag the error must name.
#[derive(Debug)]
struct Misfit {
    words: Vec<String>,
    flag: String,
}

fn a_misfit(g: &mut Gen) -> Misfit {
    let value = format!("v{}", g.below(100));
    let (words, flag): (Vec<&str>, &str) = match g.below(7) {
        0 => (vec!["--batch-size", "3"], "--batch-size"),
        1 => (vec!["--Env", "a"], "--Env"),
        2 => (vec!["--env", "a", "--region", "x", "--region", "y"], "--region"),
        3 => (vec!["--env", "a", "--count", "x"], "--count"),
        4 => (vec!["--env", "a", "--count", "500"], "--count"),
        5 => (vec!["--region", "eu"], "--env"),
        _ => (vec!["--env", "a", "--verbose", "--verbose"], "--verbose"),
    };
    let mut words: Vec<String> = words.into_iter().map(str::to_string).collect();
    words.push(String::from("--note"));
    words.push(value);
    Misfit {
        words,
        flag: flag.to_string(),
    }
}

fn a_command_line_that_does_not_fit_is_one_error_that_names_the_flag() {
    if !runs_here() {
        return;
    }
    let executable = deploy();
    generate::check(9502, 14, a_misfit, |misfit: &Misfit| {
        let said = run(&executable, &misfit.words, b"");
        assert!(!said.status.success(), "{:?} was accepted", misfit.words);
        assert!(said.stdout.is_empty(), "the task ran for {:?}", misfit.words);
        let err = String::from_utf8_lossy(&said.stderr).into_owned();
        assert_eq!(err.trim_end().lines().count(), 1, "one error for {:?}: {err}", misfit.words);
        assert!(err.contains(&misfit.flag), "{err} does not name {}", misfit.flag);
    });
    // A flag with no value is the flag that is named, too.
    let said = run(&executable, &["--env".to_string()], b"");
    assert!(!said.status.success());
    assert!(String::from_utf8_lossy(&said.stderr).contains("--env"));
}

fn a_task_is_run_by_its_name_and_one_that_gives_an_error_prints_it_and_fails() {
    if !runs_here() {
        return;
    }
    let dir = project("run-tasks", &[("main.smith", TASKS_MAIN), ("project.smith", TASKS)]);
    let said = smith(&dir, &["deploy", "--env", "staging", "--verbose"]);
    assert!(said.status.success(), "{}", stderr(&said));
    assert_eq!(String::from_utf8_lossy(&said.stdout), "deploy staging true eu 3\n");
    let failed = smith(&dir, &["fail"]);
    assert!(!failed.status.success());
    assert!(stderr(&failed).contains("it broke"), "{}", stderr(&failed));
    // The command is found from a folder below the project.
    let below = dir.join("below");
    std::fs::create_dir_all(&below).unwrap();
    let said = smith(&below, &["deploy", "--env", "a"]);
    assert!(said.status.success(), "{}", stderr(&said));
}

const STORE: &str = "pub effect store {\n  pub fn get(key: str) -> Option[str]\n}\n";

const STORE_MAIN: &str = "use std.console (console, print)\nuse store (get, store)\n\npub fn main() <store, console> {\n  match get(\"a\") {\n    .some(v) { print(v) }\n    .none { print(\"none\") }\n  }\n}\n";

fn a_deployment_that_leaves_an_effect_of_main_unanswered_is_refused_for_a_native_target() {
    let project_text = "use main (main)\nuse store (store)\nuse std.console (console)\n\npub fn forgot() <console> {\n  main()\n}\n\npub fn carries() <store, console> {\n  main()\n}\n";
    let dir = project(
        "store",
        &[("store.smith", STORE), ("main.smith", STORE_MAIN), ("project.smith", project_text)],
    );
    for task in ["forgot", "carries"] {
        let built = smith(&dir, &["build", task]);
        assert!(!built.status.success(), "{task} was built");
        let err = stderr(&built);
        assert!(err.contains("`store`"), "{task}: {err}");
        assert!(err.contains("answer"), "{task}: {err}");
    }
}

fn a_task_named_as_a_command_is_refused_where_it_is_written() {
    let main = "use std.console (console, print)\n\npub fn main() <console> {\n  print(\"main\")\n}\n";
    for name in ["build", "run", "check"] {
        let text = format!("use std.console (console)\n\npub fn {name}() <console> {{\n}}\n");
        let dir = project(&format!("reserved-{name}"), &[("main.smith", main), ("project.smith", &text)]);
        let built = smith(&dir, &["build", name]);
        assert!(!built.status.success());
        let err = stderr(&built);
        assert!(err.contains(&format!("`{name}`")), "{err}");
        let (line, col) = (text.lines().position(|l| l.contains("pub fn")).unwrap() + 1, "pub fn ".len() + 1);
        assert!(err.contains(&format!("project.smith:{line}:{col}")), "the place it is written: {err}");
    }
}

fn main_is_listed_by_project_smith_only_when_it_is_pub_and_a_pub_main_is_built_without_a_name() {
    let project_text = "use main (main)\nuse std.console (console)\n\npub fn local() <console> {\n  main()\n}\n";
    let private = project(
        "main-private",
        &[("main.smith", "use std.console (console, print)\n\nfn main() <console> {\n  print(\"main\")\n}\n"), ("project.smith", project_text)],
    );
    let refused = smith(&private, &["build", "local"]);
    assert!(!refused.status.success());
    assert!(stderr(&refused).contains("`main` is private to `main`"), "{}", stderr(&refused));

    let public = project("main-public", &[("main.smith", TASKS_MAIN), ("project.smith", project_text)]);
    for words in [&["build"][..], &["build", "local"]] {
        let built = smith(&public, words);
        assert!(built.status.success(), "{words:?}: {}", stderr(&built));
    }
}

fn an_error_of_a_task_is_at_a_line_of_project_smith() {
    // `console` is listed by the task and not imported.
    let text = "use main (main)\n\npub fn hi() <console> {\n  main()\n}\n";
    let dir = project("unimported", &[("main.smith", TASKS_MAIN), ("project.smith", text)]);
    let built = smith(&dir, &["build", "hi"]);
    assert!(!built.status.success());
    let err = stderr(&built);
    let mut shown = 0;
    for line in err.lines() {
        if let Some((number, code)) = line.trim_start().split_once(" | ")
            && number.chars().all(|c| c.is_ascii_digit())
        {
            shown += 1;
            assert!(text.lines().any(|l| l == code), "`{code}` is not a line of project.smith: {err}");
        } else if line.contains(".smith:") {
            assert!(line.starts_with("project.smith:"), "{line}");
        }
    }
    assert!(shown > 0, "{err}");
    // The missing import is said once, where the task lists the effect, and not
    // again at the task's name.
    assert_eq!(err.matches("does not import it").count(), 1, "{err}");
    let (line, col) = (3, "pub fn hi() <".len() + 1);
    assert!(err.contains(&format!("project.smith:{line}:{col}")), "{err}");
}
