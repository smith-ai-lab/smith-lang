//! The `smith` command: `build`, `run`, `check`, and `test`, over the
//! compiler in `lib.rs`.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;

use smith::diag;
use smith::project;
use smith::target::Target;
use smith::timings::Timings;

#[cfg(unix)]
mod watch;

const USAGE: &str =
    "usage: smith [--timings] build [--target <target>] [--map] [<file>.smith | <task>]
       smith [--timings] <task> [--<flag> <value>...]
       smith [--timings] run [--target <target>] [<file>.smith [<word>...]]
       smith [--timings] check [<file>.smith]
       smith [--timings] test [<file>_test.smith] [--name <text>] [--seed <n>]
       smith [--timings] --watch <command>
       smith --version";

/// The version a release was built as, set by the release workflow. A build
/// that is not a release has none.
const VERSION: &str = match option_env!("SMITH_VERSION") {
    Some(version) => version,
    None => "dev",
};

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    match command(&args) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

/// What a `build` or `run` command line asks for.
struct Request {
    target: Target,
    entry: PathBuf,
    /// The task of `project.smith` the executable starts at, when a name that is
    /// not a file's was given.
    task: Option<String>,
    timings: bool,
    /// Whether `smith build` writes the map of the executable beside it.
    map: bool,
    /// Whether `smith --watch` builds the executable to run it, which is the
    /// only executable that is built to keep a socket for its run.
    watched: bool,
    /// What follows the file's name in `smith run`, which are the words the
    /// program is started with.
    words: Vec<OsString>,
}

/// An argument of the compiler's own, which is text. The words a program is
/// started with are not, and are never read as text here.
fn text(arg: &OsStr) -> Result<&str, String> {
    arg.to_str().ok_or_else(|| {
        format!(
            "the argument `{}` is not valid UTF-8",
            arg.to_string_lossy()
        )
    })
}

/// The error for a flag of `smith` written after the command.
fn flag_of_smith(flag: &str, command: &str) -> String {
    format!("`{flag}` is a flag of `smith`, written before the command: smith {flag} {command}")
}

fn command(args: &[OsString]) -> Result<ExitCode, String> {
    // The flags of `smith` come right after `smith`, before the command.
    let mut timings = false;
    let mut watching = false;
    let mut args = args;
    while let Some((flag, rest)) = args.split_first() {
        let flag = text(flag)?;
        match flag {
            "--version" if rest.is_empty() && !timings => {
                println!("smith {VERSION}");
                return Ok(ExitCode::SUCCESS);
            }
            "--timings" if timings => return Err("`--timings` is given twice".to_string()),
            "--timings" => timings = true,
            "--watch" if watching => return Err("`--watch` is given twice".to_string()),
            "--watch" => watching = true,
            "--target" => {
                return Err(
                    "`--target` is a flag of `build` and `run`, written after the command: smith build --target <target>"
                        .to_string(),
                );
            }
            _ if flag.starts_with('-') => {
                return Err(format!(
                    "unknown flag `{flag}`; the flags of smith are --timings and --watch\n{USAGE}"
                ));
            }
            _ => break,
        }
        args = rest;
    }
    let Some((name, rest)) = args.split_first() else {
        return Err(USAGE.to_string());
    };
    let name = text(name)?;
    if watching {
        return watch_command(name, rest, timings);
    }
    match name {
        "build" => {
            let request = request(rest, false, timings)?;
            build(&request)?;
            Ok(ExitCode::SUCCESS)
        }
        "run" => {
            let request = request(rest, true, timings)?;
            if Target::host() != Some(request.target) {
                return Err(format!(
                    "smith run runs only programs for this machine, and {} is not its target",
                    request.target.name()
                ));
            }
            let executable = build(&request)?;
            run_to_the_end(&executable, &request.words)
        }
        "check" => check(rest, timings),
        "test" => test(rest, timings),
        "new" | "fmt" | "vendor" | "hole" | "find" | "deps" | "cost" => {
            Err(format!("smith {name} is not supported yet"))
        }
        task => run_task(task, rest, timings),
    }
}

/// What a command line asks for. When `with_words`, every word after the
/// file's name is for the program and no flag is read among them.
fn request(args: &[OsString], with_words: bool, timings: bool) -> Result<Request, String> {
    let mut target = None;
    let mut entry = None;
    let mut task = None;
    let mut map = false;
    let mut words = Vec::new();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let arg = text(arg)?;
        if arg == "--target" {
            let Some(name) = args.next() else {
                return Err("--target needs a target name".to_string());
            };
            let name = text(name)?;
            target = Some(Target::from_name(name).ok_or_else(|| {
                let names: Vec<_> = Target::ALL.iter().map(|t| t.name()).collect();
                format!(
                    "unknown target `{name}`; the targets are {}",
                    names.join(", ")
                )
            })?);
        } else if arg == "--timings" || arg == "--watch" {
            let command = if with_words { "run" } else { "build" };
            return Err(flag_of_smith(arg, command));
        } else if arg == "--map" && !with_words {
            map = true;
        } else if arg.starts_with("--") {
            return Err(format!("unknown flag `{arg}`\n{USAGE}"));
        } else if entry.is_some() || task.is_some() {
            return Err(format!("only one file can be built\n{USAGE}"));
        } else if !with_words && !arg.ends_with(".smith") {
            task = Some(arg.to_string());
        } else {
            entry = Some(PathBuf::from(arg));
            if with_words {
                words = args.by_ref().cloned().collect();
            }
        }
    }
    let target = match target {
        Some(target) => target,
        None => Target::host().ok_or_else(|| {
            format!(
                "this machine, {}, is not a target; choose one with --target",
                Target::host_description()
            )
        })?,
    };
    Ok(Request {
        target,
        entry: entry.unwrap_or_else(|| PathBuf::from("main.smith")),
        task,
        timings,
        map,
        watched: false,
        words,
    })
}

/// Checks the entry, `main.smith` unless a file is named, and reports its
/// errors, or else its warnings, writing no file. It fails if there are
/// any. With `--timings` it then prints how long each stage took.
fn check(args: &[OsString], timings: bool) -> Result<ExitCode, String> {
    check_reading(args, timings, &mut Vec::new())
}

/// `check`, which also says which files it read.
fn check_reading(
    args: &[OsString],
    timings: bool,
    read: &mut Vec<PathBuf>,
) -> Result<ExitCode, String> {
    let mut entry = None;
    for arg in args {
        let arg = text(arg)?;
        if arg == "--timings" || arg == "--watch" {
            return Err(flag_of_smith(arg, "check"));
        } else if arg.starts_with("--") {
            return Err(format!("unknown flag `{arg}`\n{USAGE}"));
        } else if entry.is_some() {
            return Err(format!("only one file can be checked\n{USAGE}"));
        } else {
            entry = Some(PathBuf::from(arg));
        }
    }
    let whole_project = entry.is_none();
    let entry = entry.unwrap_or_else(|| PathBuf::from("main.smith"));
    read.push(absolute(&entry));
    let (_, source) = read_entry(&entry)?;
    let files = Files::in_current_folder(&entry)?;
    let project = files.project();
    let mut stages = Timings::new();
    let found = match smith::check_project_timed(&source, &project, &mut stages) {
        Ok(warnings) => warnings,
        Err(errors) => errors,
    };
    let mut report = files.render(&entry.to_string_lossy(), &source, &found);
    let mut failed = !found.is_empty();
    // A file that no `use` reached is read too, and what is wrong in it said,
    // though no build reads it.
    if whole_project {
        for name in files.unreached() {
            if files.read.borrow().contains(&name) {
                continue;
            }
            let found = smith::check_module(&name, &project);
            failed |= !found.is_empty();
            report.push_str(&files.render(&entry.to_string_lossy(), &source, &found));
        }
    }
    if timings {
        report.push_str(&stages.report());
        report.push('\n');
    }
    read.extend(files.paths());
    eprint!("{report}");
    Ok(if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}

/// `smith test`: runs the tests of every file of tests below the folder,
/// `x_test.smith`, or of the file given, and of those the ones whose name
/// holds the text after `--name`, as tasks of one process. It says of each
/// test whether it passed and what each failure says, how many passed and
/// failed, and the slowest, and fails when a test fails or a file of tests is
/// refused.
fn test(args: &[OsString], timings: bool) -> Result<ExitCode, String> {
    let root = std::env::current_dir().map_err(|e| format!("cannot find this folder: {e}"))?;
    let Chosen { file, name, seed } = tests_chosen(args, &root)?;
    let seed = seed.unwrap_or_else(fresh_seed);
    let found = match file {
        Some(file) => vec![file],
        None => {
            let mut found = Vec::new();
            test_files(&root, &mut Vec::new(), &mut found);
            found.sort();
            found
        }
    };
    let folder = std::env::temp_dir().join(format!("smith-test-{}", std::process::id()));
    let ran = test_in(&found, name.as_deref(), seed, &root, &folder, timings);
    let _ = std::fs::remove_dir_all(&folder);
    let ran = ran?;
    println!("{} passed, {} failed", ran.passed, ran.failed);
    let mut slowest = ran.times;
    slowest.sort_by_key(|(ms, _)| std::cmp::Reverse(*ms));
    if !slowest.is_empty() {
        println!("slowest:");
        for (ms, test) in slowest.iter().take(SLOWEST) {
            println!("  {ms} ms {test}");
        }
    }
    Ok(if ran.failed > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}

/// How many of the slowest tests a run of `smith test` ends by listing.
const SLOWEST: usize = 5;

/// The seed of a run of `smith test` that is given none, which differs from
/// one run to the next, so that the properties are given other keys.
fn fresh_seed() -> u64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    (now as u64) ^ u64::from(std::process::id()).rotate_left(32)
}

/// What `smith test` is given, each when it is: the file of tests, by its path
/// from the folder, as a run names it, the text of `--name`, and the seed of
/// `--seed`.
struct Chosen {
    file: Option<String>,
    name: Option<String>,
    seed: Option<u64>,
}

fn tests_chosen(args: &[OsString], root: &Path) -> Result<Chosen, String> {
    let (mut file, mut name, mut seed) = (None, None, None);
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let arg = text(arg)?;
        match arg {
            "--timings" | "--watch" => return Err(flag_of_smith(arg, "test")),
            "--name" if name.is_some() => return Err("`--name` is given twice".to_string()),
            "--name" => {
                let Some(value) = args.next() else {
                    return Err(format!(
                        "`--name` is followed by the text a test's name holds\n{USAGE}"
                    ));
                };
                name = Some(text(value)?.to_string());
            }
            "--seed" if seed.is_some() => return Err("`--seed` is given twice".to_string()),
            "--seed" => {
                let value = args.next().map(|v| text(v)).transpose()?;
                let Some(n) = value.and_then(|v| v.parse().ok()) else {
                    return Err(format!(
                        "`--seed` is followed by the seed of a run, a whole number below 2^64\n{USAGE}"
                    ));
                };
                seed = Some(n);
            }
            _ if arg.starts_with('-') => {
                return Err(format!(
                    "unknown flag `{arg}`; the flags of `smith test` are --name and --seed\n{USAGE}"
                ));
            }
            _ if file.is_some() => {
                return Err(format!("`smith test` takes one file of tests\n{USAGE}"));
            }
            _ => file = Some(test_file(arg, root)?),
        }
    }
    Ok(Chosen { file, name, seed })
}

/// The path from the folder of a file of tests that `smith test` is given.
fn test_file(arg: &str, root: &Path) -> Result<String, String> {
    let given = Path::new(arg);
    let is_tests = given
        .file_name()
        .is_some_and(|n| smith::testing::is_test_file(&n.to_string_lossy()));
    if !is_tests {
        return Err(format!(
            "`{arg}` is not a file of tests, whose name ends in `_test.smith`"
        ));
    }
    if !given.is_file() {
        return Err(format!("{arg}: there is no such file"));
    }
    let whole = absolute(given);
    let relative = whole.strip_prefix(root).unwrap_or(&whole);
    let mut parts = Vec::new();
    for part in relative.components() {
        match part {
            Component::Normal(name) => parts.push(name.to_string_lossy().into_owned()),
            Component::CurDir => {}
            _ => {
                return Err(format!(
                    "{arg}: a file of tests is in the folder `smith test` runs in, or below it"
                ));
            }
        }
    }
    Ok(parts.join("/"))
}

/// What a run of tests gave: how many passed and failed, where a file that is
/// refused counts as one that failed, and how long each test that ended took,
/// in milliseconds, with its file and name.
struct Ran {
    passed: usize,
    failed: usize,
    times: Vec<(u64, String)>,
}

/// A file of tests that a run builds: its path, the module it is, its text,
/// and its tests.
struct TestFile {
    path: String,
    module: String,
    source: String,
    tests: smith::testing::Tests,
    /// The tests chosen of it, each with its number in the run.
    chosen: Vec<(usize, String)>,
}

/// Builds one program of the tests chosen of the files, the executable in
/// `folder`, and runs it. A file whose tests are refused, or that the build
/// refuses, is said and left out, and the rest are built again without it.
fn test_in(
    found: &[String],
    name: Option<&str>,
    seed: u64,
    root: &Path,
    folder: &Path,
    timings: bool,
) -> Result<Ran, String> {
    let mut ran = Ran {
        passed: 0,
        failed: 0,
        times: Vec::new(),
    };
    let mut files: Vec<TestFile> = Vec::new();
    let mut number = 0;
    for path in found {
        let (_, bytes) = read_entry(&root.join(path))?;
        let source = String::from_utf8_lossy(&bytes).into_owned();
        let tests = match smith::testing::read(&source) {
            Ok(tests) => tests,
            Err(errors) => {
                eprint!("{}", Files::at(root).render(path, &bytes, &errors));
                ran.failed += 1;
                continue;
            }
        };
        let chosen: Vec<(usize, String)> = (tests.tests.iter())
            .filter(|t| name.is_none_or(|n| t.name.contains(n)))
            .map(|t| {
                number += 1;
                (number - 1, t.name.clone())
            })
            .collect();
        if chosen.is_empty() && tests.parses {
            continue;
        }
        let module = path.trim_end_matches(".smith").replace('/', ".");
        files.push(TestFile {
            path: path.clone(),
            module,
            source,
            tests,
            chosen,
        });
    }
    if files.is_empty() {
        return Ok(ran);
    }
    let target = Target::host().ok_or_else(|| {
        format!(
            "this machine, {}, is not a target",
            Target::host_description()
        )
    })?;
    let width = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    let mut stages = Timings::new();
    let built = loop {
        let mut written = BTreeMap::new();
        let chosen: Vec<Vec<(usize, &str)>> = (files.iter())
            .map(|f| f.chosen.iter().map(|(k, n)| (*k, n.as_str())).collect())
            .collect();
        let shown: Vec<smith::testing::File> = (files.iter().zip(&chosen))
            .map(|(f, chosen)| {
                written.insert(f.module.clone(), f.tests.module(&f.source, chosen));
                smith::testing::File {
                    module: &f.module,
                    tests: &f.tests,
                    chosen,
                }
            })
            .collect();
        let program = smith::testing::program(&shown, width, seed);
        let mut each = Files::at(root);
        each.written = written;
        let built =
            smith::compile_tests_timed(program.as_bytes(), &each.project(), target, &mut stages);
        let errors = match built {
            Ok(built) => break built,
            Err(errors) => errors,
        };
        // A file of tests that an error is in is refused, and so is every
        // file when an error is in a module that is not a file of tests, or in
        // no module, which no file can then be said to be built without.
        let refused: Vec<usize> = (0..files.len())
            .filter(|&i| {
                errors
                    .iter()
                    .any(|e| e.module.as_deref() == Some(&files[i].path))
            })
            .collect();
        let elsewhere = errors.iter().any(|e| {
            e.module
                .as_ref()
                .is_some_and(|m| files.iter().all(|f| &f.path != m))
        });
        if refused.is_empty() || elsewhere {
            eprint!("{}", each.render("tests", program.as_bytes(), &errors));
            ran.failed += files.len();
            return Ok(ran);
        }
        for &i in &refused {
            let file = &files[i];
            // What is wrong in the functions written after the file is wrong
            // in the tests they call, which is said there.
            let lines = file.source.split('\n').count();
            let in_file: Vec<diag::Error> = (errors.iter())
                .filter(|e| e.module.as_deref() == Some(&file.path))
                .cloned()
                .collect();
            let theirs: Vec<diag::Error> = (in_file.iter())
                .filter(|e| e.pos.line <= lines)
                .cloned()
                .collect();
            let shown = if theirs.is_empty() { &in_file } else { &theirs };
            eprint!("{}", each.render("tests", program.as_bytes(), shown));
            ran.failed += 1;
        }
        for &i in refused.iter().rev() {
            files.remove(i);
        }
        if files.is_empty() {
            return Ok(ran);
        }
    };
    if timings {
        eprintln!("{}", stages.report());
    }
    let executable = folder.join(target.executable_name("tests"));
    std::fs::create_dir_all(folder)
        .and_then(|()| write_executable(&executable, &built.executable, true))
        .map_err(|e| format!("{}: {e}", executable.display()))?;
    let named: BTreeMap<usize, (String, bool)> = (files.iter())
        .flat_map(|f| {
            f.chosen.iter().map(|(k, n)| {
                let property = f
                    .tests
                    .tests
                    .iter()
                    .any(|t| &t.name == n && t.is_property());
                (*k, (format!("{} {n}", f.path), property))
            })
        })
        .collect();
    run_tests(&executable, &named, &mut ran)?;
    Ok(ran)
}

/// Runs the program of the tests, and says what its records say of each
/// test, by its number: what else it prints is passed on as it is. A test
/// that no record is printed for, as the program ended first, failed. A test
/// took the time from the record that it began to the one that it is done.
fn run_tests(
    executable: &Path,
    named: &BTreeMap<usize, (String, bool)>,
    ran: &mut Ran,
) -> Result<(), String> {
    use std::io::{BufRead, Read, Write};
    let failed = |e: std::io::Error| format!("{}: {e}", executable.display());
    let mut child = run(executable, &[], None, std::process::Stdio::piped()).map_err(failed)?;
    let mut said = std::io::BufReader::new(child.stdout.take().expect("piped"));
    let mut seen = BTreeSet::new();
    let mut began = BTreeMap::new();
    let mut line = Vec::new();
    loop {
        line.clear();
        if said.read_until(b'\n', &mut line).map_err(failed)? == 0 {
            break;
        }
        let text = String::from_utf8_lossy(&line);
        let Some(record) = text
            .trim_end_matches('\n')
            .strip_prefix(smith::testing::RECORD)
        else {
            let mut out = std::io::stdout();
            let _ = out.write_all(&line).and_then(|()| out.flush());
            continue;
        };
        let words: Vec<&str> = record.split(' ').collect();
        let Some((k, (test, property))) = (words.get(1))
            .and_then(|k| k.parse().ok())
            .and_then(|k: usize| named.get_key_value(&k))
        else {
            continue;
        };
        match words[0] {
            "began" => {
                began.insert(*k, std::time::Instant::now());
                continue;
            }
            "done" => {
                if let Some(at) = began.get(k) {
                    let ms = u64::try_from(at.elapsed().as_millis()).unwrap_or(u64::MAX);
                    ran.times.push((ms, test.clone()));
                }
                continue;
            }
            "passed" if *property => {
                println!("passed {test} ({} runs)", smith::testing::RUNS);
                ran.passed += 1;
            }
            "passed" => {
                println!("passed {test}");
                ran.passed += 1;
            }
            "failed" => {
                let size: usize = words.get(2).and_then(|n| n.parse().ok()).unwrap_or(0);
                let mut message = vec![0; size + 1];
                said.read_exact(&mut message).map_err(failed)?;
                message.pop();
                println!("failed {test}");
                eprintln!("{}", String::from_utf8_lossy(&message));
                ran.failed += 1;
            }
            _ => {
                let why = match words.get(2) {
                    Some(&"out_of_memory") => "it ran out of memory",
                    _ => "a write of it was refused",
                };
                println!("failed {test}");
                eprintln!("{test}: the test ended before it gave a result, as {why}");
                ran.failed += 1;
            }
        }
        seen.insert(*k);
    }
    child.wait().map_err(failed)?;
    for (k, (test, _)) in named {
        if !seen.contains(k) {
            println!("failed {test}");
            eprintln!("{test}: the test did not end, as the run of the tests ended first");
            ran.failed += 1;
        }
    }
    Ok(())
}

/// The files of tests below the folder, by their paths from the root, but
/// those in hidden folders.
fn test_files(folder: &Path, above: &mut Vec<String>, found: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            above.push(name);
            test_files(&path, above, found);
            above.pop();
        } else if smith::testing::is_test_file(&name) {
            let mut parts = above.clone();
            parts.push(name);
            found.push(parts.join("/"));
        }
    }
}

/// The files of the project a command runs in, which is the folder it runs in:
/// the module `db.transaction` is the file `db/transaction.smith` below it.
struct Files {
    root: PathBuf,
    /// The module the entry is, when its path below the root gives it one.
    entry: Option<String>,
    /// The modules a `use` has read, in the order they were asked for.
    read: RefCell<BTreeSet<String>>,
    /// The modules whose text is written here and not read from their file:
    /// `project` when a task is built, which is `project.smith` without the
    /// lines the compiler reads no further, and a file of tests with the
    /// functions after it that call its tests.
    written: BTreeMap<String, String>,
}

impl Files {
    fn in_current_folder(entry: &Path) -> Result<Files, String> {
        let root = std::env::current_dir().map_err(|e| format!("cannot find this folder: {e}"))?;
        let relative = entry.strip_prefix(&root).unwrap_or(entry);
        let mut parts: Vec<String> = Vec::new();
        for part in relative.with_extension("").components() {
            match part {
                Component::Normal(name) => parts.push(name.to_string_lossy().into_owned()),
                Component::CurDir => {}
                _ => {
                    parts.clear();
                    break;
                }
            }
        }
        let entry = (!parts.is_empty()).then(|| parts.join("."));
        Ok(Files {
            root,
            entry,
            read: RefCell::new(BTreeSet::new()),
            written: BTreeMap::new(),
        })
    }

    /// The files of the project in the folder, built from no file of it.
    fn at(root: &Path) -> Files {
        Files {
            root: root.to_path_buf(),
            entry: None,
            read: RefCell::new(BTreeSet::new()),
            written: BTreeMap::new(),
        }
    }

    /// The files of the modules that have been read.
    fn paths(&self) -> Vec<PathBuf> {
        let read = self.read.borrow();
        read.iter()
            .map(|module| {
                self.root
                    .join(format!("{}.smith", module.replace('.', "/")))
            })
            .collect()
    }

    fn project(&self) -> smith::Project<'_> {
        smith::Project {
            files: self,
            entry: self.entry.as_deref(),
        }
    }
}

impl smith::Source for Files {
    /// The text of the file of the module with the name, which is noted as
    /// read.
    fn source(&self, module: &str) -> Option<String> {
        if let Some(text) = self.written.get(module) {
            self.read.borrow_mut().insert(module.to_string());
            return Some(text.clone());
        }
        let path = self
            .root
            .join(format!("{}.smith", module.replace('.', "/")));
        let bytes = std::fs::read(path).ok()?;
        self.read.borrow_mut().insert(module.to_string());
        Some(String::from_utf8_lossy(&bytes).into_owned())
    }
}

impl Files {
    /// Writes errors, showing the line of a module from its file.
    fn render(&self, file: &str, source: &[u8], errors: &[diag::Error]) -> String {
        diag::render_with(file, source, errors, &|module| match module {
            path if path.ends_with(".smith") => std::fs::read(self.root.join(path)).map_or_else(
                |_| String::new(),
                |bytes| String::from_utf8_lossy(&bytes).into_owned(),
            ),
            std => smith::library_text(std).to_string(),
        })
    }

    /// The modules of the project's files that no `use` has read, which are
    /// all the files below the root but the entry and the hidden folders.
    fn unreached(&self) -> Vec<String> {
        let mut found = BTreeSet::new();
        self.collect(&self.root, &mut Vec::new(), &mut found);
        let read = self.read.borrow();
        found
            .into_iter()
            .filter(|m| !read.contains(m) && Some(m) != self.entry.as_ref())
            .collect()
    }

    fn collect(&self, folder: &Path, above: &mut Vec<String>, found: &mut BTreeSet<String>) {
        let Ok(entries) = std::fs::read_dir(folder) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = entry.path();
            if name.starts_with('.') {
                continue;
            }
            if path.is_dir() {
                above.push(name);
                self.collect(&path, above, found);
                above.pop();
            } else if let Some(stem) = name.strip_suffix(".smith")
                && !stem.is_empty()
                && !stem.contains('.')
            {
                let mut module = above.clone();
                module.push(stem.to_string());
                found.insert(module.join("."));
            }
        }
    }
}

/// The stem of an entry's name, and its text.
fn read_entry(entry: &Path) -> Result<(&std::ffi::OsStr, Vec<u8>), String> {
    let shown = entry.display();
    let stem = match (entry.extension(), entry.file_stem()) {
        (Some(ext), Some(stem)) if ext == "smith" && !stem.is_empty() => stem,
        _ => return Err(format!("{shown}: a Smith file's name ends in .smith")),
    };
    let source = std::fs::read(entry).map_err(|e| format!("{shown}: {e}"))?;
    Ok((stem, source))
}

/// Builds the entry, and with `--timings` prints how long each stage took:
/// after the errors when the build fails, and before returning when it
/// succeeds.
fn build(request: &Request) -> Result<PathBuf, String> {
    build_reading(request, &mut Vec::new())
}

/// `build`, which also says which files it read.
fn build_reading(request: &Request, read: &mut Vec<PathBuf>) -> Result<PathBuf, String> {
    let mut timings = Timings::new();
    let result = build_timed(request, &mut timings, read);
    match (result, request.timings) {
        (Ok(path), true) => {
            eprintln!("{}", timings.report());
            Ok(path)
        }
        (Err(message), true) => Err(format!("{message}\n{}", timings.report())),
        (result, false) => result,
    }
}

/// Builds the entry into an executable in the current folder, and returns
/// its path.
fn build_timed(
    request: &Request,
    timings: &mut Timings,
    read: &mut Vec<PathBuf>,
) -> Result<PathBuf, String> {
    if let Some(task) = &request.task {
        return build_task(task, request, timings, read);
    }
    let entry = &request.entry;
    read.push(absolute(entry));
    let (stem, source) = read_entry(entry)?;
    let files = Files::in_current_folder(entry)?;
    let compiled = smith::compile_project_files_timed(
        &source,
        &files.project(),
        request.target,
        request.map,
        request.watched,
        timings,
    );
    read.extend(files.paths());
    let built = compiled.map_err(|errors| {
        let file = entry.to_string_lossy();
        let text = files.render(&file, &source, &errors);
        text.strip_suffix('\n').unwrap_or(&text).to_string()
    })?;
    let cwd = files.root.clone();
    let name = if stem == "main" {
        cwd.file_name().unwrap_or(stem)
    } else {
        stem
    };
    let output = cwd.join(request.target.executable_name(&name.to_string_lossy()));
    timings
        .stage("write", || {
            write_executable(&output, &built.executable, request.target.is_machine_code())?;
            for (extension, bytes) in &built.beside {
                write_executable(&output.with_extension(extension), bytes, false)?;
            }
            if let Some(map) = built.map_text() {
                write_executable(&output.with_extension("map"), map.as_bytes(), false)?;
            }
            Ok(())
        })
        .map_err(|e: std::io::Error| format!("{}: {e}", output.display()))?;
    Ok(output)
}

/// Writes the executable, replacing any file already there, and lets the
/// system run it when it is a program for a chip.
fn write_executable(path: &Path, bytes: &[u8], runs: bool) -> std::io::Result<()> {
    if path.is_file() {
        std::fs::remove_file(path)?;
    }
    std::fs::write(path, bytes)?;
    #[cfg(unix)]
    if runs {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))?;
    }
    let _ = runs;
    Ok(())
}

/// Starts a built executable with the words it is started with, writing what
/// it prints to `output`. `smith --watch` gives it a socket too, which the
/// program holds at descriptor 3.
fn run(
    executable: &Path,
    words: &[OsString],
    control: Option<&Control>,
    output: std::process::Stdio,
) -> std::io::Result<std::process::Child> {
    let mut command = std::process::Command::new(executable);
    command.args(words).stdout(output);
    #[cfg(unix)]
    if let Some(socket) = control {
        use std::os::fd::AsRawFd;
        use std::os::unix::process::CommandExt;
        unsafe extern "C" {
            fn dup2(from: i32, to: i32) -> i32;
            fn fcntl(fd: i32, command: i32, argument: i32) -> i32;
        }
        const AT: i32 = 3;
        const F_SETFD: i32 = 2;
        let from = socket.as_raw_fd();
        // SAFETY: only `dup2` and `fcntl` are called between the fork and the
        // exec, which is safe there. `dup2` leaves the new descriptor open
        // across the exec, and `fcntl` does it for one that is already there.
        unsafe {
            command.pre_exec(move || {
                if from == AT {
                    fcntl(AT, F_SETFD, 0);
                } else if dup2(from, AT) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
    let _ = control;
    command.spawn()
}

/// The socket `smith --watch` talks to a program on.
#[cfg(unix)]
type Control = std::os::unix::net::UnixStream;
#[cfg(not(unix))]
type Control = ();

/// Runs a built executable with the words it is started with, and exits as it
/// does.
fn run_to_the_end(executable: &Path, words: &[OsString]) -> Result<ExitCode, String> {
    let status = run(executable, words, None, std::process::Stdio::inherit())
        .and_then(|mut child| child.wait())
        .map_err(|e| format!("{}: {e}", executable.display()))?;
    let code = status.code().unwrap_or(1);
    Ok(ExitCode::from(u8::try_from(code).unwrap_or(1)))
}

/// The nearest folder at or above this one that holds a `project.smith`.
fn find_root() -> Option<PathBuf> {
    let here = std::env::current_dir().ok()?;
    here.ancestors()
        .find(|folder| folder.join("project.smith").is_file())
        .map(Path::to_path_buf)
}

/// What the `project.smith` of the folder says, or the errors in it as they
/// are printed.
fn tasks_of(root: &Path) -> Result<project::Read, String> {
    let file = root.join("project.smith");
    let source = std::fs::read(&file).map_err(|e| format!("{}: {e}", file.display()))?;
    let text = String::from_utf8_lossy(&source).into_owned();
    project::read(&text).map_err(|errors| {
        let shown = diag::render_with("project.smith", text.as_bytes(), &errors, &|_| text.clone());
        shown.strip_suffix('\n').unwrap_or(&shown).to_string()
    })
}

/// The folder of the nearest `project.smith` and what it says, when it has a
/// task of this name. Otherwise the one reason it has not: there is no
/// `project.smith`, it does not compile, or it has no such task.
fn project_with_task(task: &str) -> Result<(PathBuf, project::Read), String> {
    let Some(root) = find_root() else {
        let here =
            std::env::current_dir().map_or_else(|_| ".".to_string(), |d| d.display().to_string());
        return Err(format!(
            "no project.smith was found in {here} or any folder above it, and `{task}` would be a task of project.smith"
        ));
    };
    let read = tasks_of(&root)?;
    if read.task(task).is_none() {
        let file = root.join("project.smith");
        let names: Vec<&str> = read.tasks.iter().map(|t| t.name.as_str()).collect();
        let has = if names.is_empty() {
            "it has no tasks".to_string()
        } else {
            format!("its tasks are: {}", names.join(", "))
        };
        return Err(format!("{} has no task `{task}`; {has}", file.display()));
    }
    Ok((root, read))
}

/// Builds the executable that starts at a task of `project.smith`, which is
/// named after the package, in the folder that holds the file.
fn build_task(
    task: &str,
    request: &Request,
    timings: &mut Timings,
    reading: &mut Vec<PathBuf>,
) -> Result<PathBuf, String> {
    if let Some(root) = find_root() {
        reading.push(root.join("project.smith"));
    }
    let (root, read) = project_with_task(task)?;
    let found = read.task(task).expect("project_with_task found it");
    let program = found.program();
    let files = Files {
        root: root.clone(),
        entry: None,
        read: RefCell::new(BTreeSet::new()),
        written: BTreeMap::from([("project".to_string(), read.module.clone())]),
    };
    let compiled = smith::compile_project_files_timed(
        program.as_bytes(),
        &files.project(),
        request.target,
        request.map,
        request.watched,
        timings,
    );
    reading.extend(files.paths());
    let built = compiled.map_err(|errors| {
        let errors = found.placed(project::explain(errors));
        let text = files.render("project.smith", program.as_bytes(), &errors);
        text.strip_suffix('\n').unwrap_or(&text).to_string()
    })?;
    let name = read.package.clone().unwrap_or_else(|| {
        root.file_name()
            .map_or_else(|| "main".to_string(), |n| n.to_string_lossy().into_owned())
    });
    let output = root.join(request.target.executable_name(&name));
    timings
        .stage("write", || {
            write_executable(&output, &built.executable, request.target.is_machine_code())?;
            for (extension, bytes) in &built.beside {
                write_executable(&output.with_extension(extension), bytes, false)?;
            }
            Ok(())
        })
        .map_err(|e: std::io::Error| format!("{}: {e}", output.display()))?;
    Ok(output)
}

/// Builds a task for this machine and runs it with the flags it is given.
fn run_task(task: &str, flags: &[OsString], timings: bool) -> Result<ExitCode, String> {
    let (_, read) = project_with_task(task)?;
    let found = read.task(task).expect("project_with_task found it");
    if let Some(flag) = found.flag_of_smith(flags) {
        return Err(flag_of_smith(&flag, task));
    }
    let target = Target::host().ok_or_else(|| {
        format!(
            "this machine, {}, is not a target",
            Target::host_description()
        )
    })?;
    let request = Request {
        target,
        entry: PathBuf::from("main.smith"),
        task: Some(task.to_string()),
        timings,
        map: false,
        watched: false,
        words: flags.to_vec(),
    };
    let executable = build(&request)?;
    run_to_the_end(&executable, flags)
}

/// The path made absolute by the folder the command runs in.
fn absolute(path: &Path) -> PathBuf {
    std::env::current_dir().map_or_else(|_| path.to_path_buf(), |folder| folder.join(path))
}

/// `smith --watch <command>`: the command again each time a file it read
/// changes.
#[cfg(unix)]
fn watch_command(name: &str, rest: &[OsString], timings: bool) -> Result<ExitCode, String> {
    let mut starter = |executable: &Path, words: &[OsString], control: &Control| {
        run(
            executable,
            words,
            Some(control),
            std::process::Stdio::inherit(),
        )
    };
    match name {
        "build" | "run" => {
            let running = name == "run";
            let mut request = request(rest, running, timings)?;
            request.watched = running;
            if running && Target::host() != Some(request.target) {
                return Err(format!(
                    "smith run runs only programs for this machine, and {} is not its target",
                    request.target.name()
                ));
            }
            let kind = if running {
                watch::Kind::Run {
                    diverge: false,
                    words: request.words.clone(),
                }
            } else {
                watch::Kind::Report
            };
            Ok(watch::watch(kind, &mut starter, || {
                let mut read = Vec::new();
                match build_reading(&request, &mut read) {
                    Ok(path) => watch::Outcome {
                        executable: running.then_some(path),
                        read,
                        ok: true,
                    },
                    Err(message) => {
                        eprintln!("{message}");
                        watch::Outcome {
                            executable: None,
                            read,
                            ok: false,
                        }
                    }
                }
            }))
        }
        "check" => Ok(watch::watch(watch::Kind::Report, &mut starter, || {
            let mut read = Vec::new();
            match check_reading(rest, timings, &mut read) {
                Ok(code) => watch::Outcome {
                    executable: None,
                    read,
                    ok: code == ExitCode::SUCCESS,
                },
                Err(message) => {
                    eprintln!("{message}");
                    watch::Outcome {
                        executable: None,
                        read,
                        ok: false,
                    }
                }
            }
        })),
        "new" | "test" | "fmt" | "vendor" | "hole" | "find" | "deps" | "cost" => {
            Err(format!("smith {name} is not supported yet"))
        }
        task => {
            let (_, read) = project_with_task(task)?;
            let found = read.task(task).expect("project_with_task found it");
            if let Some(flag) = found.flag_of_smith(rest) {
                return Err(flag_of_smith(&flag, task));
            }
            let target = Target::host().ok_or_else(|| {
                format!(
                    "this machine, {}, is not a target",
                    Target::host_description()
                )
            })?;
            let request = Request {
                target,
                entry: PathBuf::from("main.smith"),
                task: Some(task.to_string()),
                timings,
                map: false,
                watched: true,
                words: rest.to_vec(),
            };
            let kind = watch::Kind::Run {
                diverge: found.diverges(),
                words: rest.to_vec(),
            };
            Ok(watch::watch(kind, &mut starter, || {
                let mut read = Vec::new();
                match build_reading(&request, &mut read) {
                    Ok(path) => watch::Outcome {
                        executable: Some(path),
                        read,
                        ok: true,
                    },
                    Err(message) => {
                        eprintln!("{message}");
                        watch::Outcome {
                            executable: None,
                            read,
                            ok: false,
                        }
                    }
                }
            }))
        }
    }
}

#[cfg(not(unix))]
fn watch_command(_: &str, _: &[OsString], _: bool) -> Result<ExitCode, String> {
    Err("smith --watch is not built for this system yet".to_string())
}
