//! The compiler's tests, and the harness that runs them.
//!
//! The tests run in parallel, in this one process. A run prints each test
//! that failed, with the message it failed with, and ends with the tests that
//! did the most work and how much each did. Arguments that do not start with
//! `-` keep only the tests whose names hold one of them.
//!
//! The work of a test is the steps that the compiler and the evaluator took for
//! it, which `smith::work` counts on the thread that ran it and which are the
//! same on every machine and every run. `--steps=<file>` writes, for each test
//! that passed, a line `name,steps`, in the order of the names, which
//! `.github/check-work.sh` compares with what main wrote. `--times=<file>` writes
//! `name,milliseconds` for each test that passed, the time it took on the wall,
//! which `.github/check-times.sh` compares with the test's own history. A test starts no
//! thread, as the steps of a thread it starts are not its own. A test that has made no
//! progress for `clock::hang` and a little more on the wall fails the run, and
//! the run names it, so that a program that never ends is a failing test and
//! not a run that never ends. Progress is the test beginning, or a program it
//! started through `clock::output_within` beginning or ending, so a test that
//! starts many programs, each of which ends, is never taken for one that hangs.
//!
//! `--hang=<ms>` sets that limit, which the tests of the limit itself make
//! small, and `--group=hang` runs the tests they run. `--wait=<ms>` runs
//! nothing and ends after that long, as a program for them to start.
//!
//! `--fingerprint` runs nothing. It prints, for each target, a hash of
//! everything the group that depends on the machine would run for it, as
//! `.github/needs-machines.sh` reads it.
//!
//! The tests are in two groups. `--group=machine` runs only the tests whose
//! result depends on the kind of machine they run on, and `--group=once`
//! runs only the others. A run that includes the first group ends by
//! naming the targets this machine could not run.
//!
//! Every `.rs` file in `tests/` but this one is a file of tests, found by
//! `build.rs`, and each says which group its tests are in by declaring
//! `pub const ONCE`, `MACHINE`, or `RUNS`, a list of `Test`. A file that
//! declares none is a build error. What tests share and is itself no test
//! is in `tests/support/`. A new file of tests is added to no list here.

// Every file of tests, and the function that gives the tests of each group,
// are found by `build.rs`.
include!(concat!(env!("OUT_DIR"), "/tests.rs"));

use std::cell::RefCell;
use std::panic::{self, AssertUnwindSafe};
use std::process::ExitCode;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// A test: its name and what it runs. It fails by panicking.
pub type Test = (&'static str, fn());

thread_local! {
    /// The name of the test this thread runs.
    static CURRENT: std::cell::Cell<&'static str> = const { std::cell::Cell::new("") };
    /// What the test this thread runs said when it panicked.
    static MESSAGE: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// What a failing test said: the name and the message of each, kept to be
/// printed at the end of the run, where a log of a long run shows them.
type Failed = Vec<(&'static str, String)>;

/// The files a run writes, when it is asked to.
#[derive(Clone, Copy)]
struct Files<'a> {
    steps: Option<&'a str>,
    times: Option<&'a str>,
}

/// How many of the tests that did the most work a run ends with.
const LARGEST: usize = 70;

/// What a test is given beyond `clock::hang`, a fifth of it, so that a program
/// it starts that does not end is stopped, and fails the test saying so,
/// before the run gives up on the test.
fn slack() -> Duration {
    clock::hang() / 5
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(wait) = args.iter().find_map(|a| a.strip_prefix("--wait=")) {
        std::thread::sleep(Duration::from_millis(wait.parse().unwrap()));
        return ExitCode::SUCCESS;
    }
    if let Some(limit) = args.iter().find_map(|a| a.strip_prefix("--hang=")) {
        clock::set_hang(Duration::from_millis(limit.parse().unwrap()));
    }
    let steps_file = args.iter().find_map(|a| a.strip_prefix("--steps="));
    let times_file = args.iter().find_map(|a| a.strip_prefix("--times="));
    if args.iter().any(|a| a == "--fingerprint") {
        // The executables are the same for every commit, so that a change
        // that leaves them as they were needs no machine.
        generate::set_run_seed(0);
        for (target, hash) in targets::fingerprint(&machine_tests(), smith::compile) {
            println!("{} {hash}", target.name());
        }
        return ExitCode::SUCCESS;
    }
    valid_modules::install();
    let filters: Vec<&String> = args.iter().filter(|a| !a.starts_with('-')).collect();
    let once = once_tests();
    let machine = machine_tests();
    let hang = hang_tests();
    let groups: Vec<&[Test]> = match args.iter().find_map(|a| a.strip_prefix("--group=")) {
        None => vec![&once, &machine],
        Some("once") => vec![&once],
        Some("machine") => vec![&machine],
        Some("hang") => vec![&hang],
        Some(other) => {
            eprintln!("unknown group `{other}`; the groups are once and machine");
            return ExitCode::FAILURE;
        }
    };
    let tests: Vec<Test> = groups
        .concat()
        .into_iter()
        .filter(|(name, _)| filters.is_empty() || filters.iter().any(|f| name.contains(f.as_str())))
        .collect();
    let on_machine = tests
        .iter()
        .any(|(name, _)| machine.iter().any(|(m, _)| m == name));
    let next = AtomicUsize::new(0);
    let results = Mutex::new(Vec::new());
    let failures: Mutex<Failed> = Mutex::new(Vec::new());
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get());
    let workers = workers.min(tests.len());
    // What each worker is running, and when it last made progress.
    let running: Vec<Mutex<Option<(&'static str, clock::Progress)>>> =
        (0..workers).map(|_| Mutex::new(None)).collect();
    panic::set_hook(Box::new(|info| {
        let place = info
            .location()
            .map_or(String::new(), |l| format!("\nat {l}"));
        let message = format!("{}{place}", info.payload_as_str().unwrap_or("(no message)"));
        MESSAGE.with(|m| *m.borrow_mut() = Some(message));
    }));
    println!("running {} tests", tests.len());
    println!(
        "seed {} (SMITH_SEED=<it> runs the same cases again)",
        generate::run_seed()
    );
    let mut lost = 0;
    std::thread::scope(|s| {
        for slot in &running {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some((name, test)) = tests.get(i) else {
                        break;
                    };
                    let progress = clock::Progress::new(Mutex::new(Instant::now()));
                    clock::watch(Some(progress.clone()));
                    *slot.lock().unwrap() = Some((name, progress));
                    let (passed, steps, time) = measure(name, *test, &failures);
                    // A test the run gave up on has its result already.
                    if slot.lock().unwrap().take().is_none() {
                        return;
                    }
                    println!("test {name} ... {}", if passed { "ok" } else { "FAILED" });
                    results.lock().unwrap().push((*name, passed, steps, time));
                }
            });
        }
        // A test that does not end cannot be stopped. When one has made no
        // progress for the limit it fails, and its worker is let go while the others go on.
        // The run ends when every test has a result, or every worker is held
        // by a test that does not end.
        while results.lock().unwrap().len() < tests.len() && lost < workers {
            std::thread::sleep(Duration::from_millis(20));
            for slot in &running {
                let mut slot = slot.lock().unwrap();
                if let Some((name, progress)) = &*slot
                    && progress.lock().unwrap().elapsed() > clock::hang() + slack()
                {
                    let name = *name;
                    *slot = None;
                    lost += 1;
                    println!("test {name} ... FAILED");
                    let message = format!(
                        "hung: {name} did not end in {}, so it was stopped without an end",
                        milliseconds(clock::hang() + slack())
                    );
                    println!("{message}");
                    failures.lock().unwrap().push((name, message));
                    results
                        .lock()
                        .unwrap()
                        .push((name, false, 0, Duration::ZERO));
                }
            }
        }
        if lost > 0 {
            // The threads of the tests that did not end cannot be joined,
            // so the report is made here and the process ends with it.
            let ok = report(
                results.lock().unwrap().clone(),
                &failures.lock().unwrap(),
                on_machine,
                Files {
                    steps: steps_file,
                    times: times_file,
                },
            );
            scratch::remove();
            std::process::exit(if ok { 0 } else { 1 });
        }
    });
    let passed = report(
        results.into_inner().unwrap(),
        &failures.into_inner().unwrap(),
        on_machine,
        Files {
            steps: steps_file,
            times: times_file,
        },
    );
    scratch::remove();
    if passed {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Whether the test this thread runs is one of the group that depends on the
/// machine, which is the group a fingerprint is made of.
pub fn in_a_machine_test() -> bool {
    static NAMES: std::sync::OnceLock<std::collections::HashSet<&'static str>> =
        std::sync::OnceLock::new();
    let names = NAMES.get_or_init(|| machine_tests().iter().map(|(n, _)| *n).collect());
    names.contains(CURRENT.get())
}

/// Runs the test on this thread and gives whether it passed, the steps it
/// took, and the time it took on the wall, keeping what a failing test said.
fn measure(name: &'static str, test: fn(), failures: &Mutex<Failed>) -> (bool, u64, Duration) {
    CURRENT.set(name);
    let begin = shared::steps();
    let waited = shared::waited();
    let started = Instant::now();
    MESSAGE.with(|m| *m.borrow_mut() = None);
    let passed = panic::catch_unwind(AssertUnwindSafe(test)).is_ok();
    if !passed {
        let message = MESSAGE.with(|m| m.borrow_mut().take());
        failures
            .lock()
            .unwrap()
            .push((name, message.unwrap_or_default()));
    }
    // The time spent making or waiting for shared work is no test's own, as
    // `shared` says.
    let took = started.elapsed().saturating_sub(shared::waited() - waited);
    (passed, shared::steps() - begin, took)
}

/// Prints how the run went, and gives whether it passed.
fn report(
    mut results: Vec<(&'static str, bool, u64, Duration)>,
    failures: &Failed,
    on_machine: bool,
    files: Files,
) -> bool {
    let failed: Vec<&str> = results
        .iter()
        .filter(|(_, passed, _, _)| !passed)
        .map(|(name, _, _, _)| *name)
        .collect();
    println!();
    for name in &failed {
        println!("failed: {name}");
        for (_, message) in failures.iter().filter(|(n, _)| n == name) {
            for line in message.lines() {
                println!("    {line}");
            }
        }
    }
    println!(
        "{} passed, {} failed",
        results.len() - failed.len(),
        failed.len()
    );
    if on_machine {
        println!();
        for line in targets::not_run() {
            println!("{line}");
        }
    }
    results.sort_by_key(|(name, ..)| *name);
    if let Some(path) = files.steps {
        let lines: String = (results.iter())
            .filter(|(_, passed, _, _)| *passed)
            .map(|(name, _, steps, _)| format!("{name},{steps}\n"))
            .collect();
        std::fs::write(path, lines).unwrap_or_else(|e| panic!("{path}: {e}"));
    }
    if let Some(path) = files.times {
        let lines: String = (results.iter())
            .filter(|(_, passed, _, _)| *passed)
            .map(|(name, _, _, time)| format!("{name},{}\n", time.as_millis()))
            .collect();
        std::fs::write(path, lines).unwrap_or_else(|e| panic!("{path}: {e}"));
    }
    results.sort_by_key(|(_, _, steps, _)| std::cmp::Reverse(*steps));
    println!("\nthe tests that did the most work, in the steps they took:");
    for (name, _, steps, _) in results.iter().take(LARGEST) {
        println!("{steps:>12}  {name}");
    }
    failed.is_empty()
}

fn milliseconds(time: Duration) -> String {
    format!("{:.1} ms", time.as_secs_f64() * 1000.0)
}
