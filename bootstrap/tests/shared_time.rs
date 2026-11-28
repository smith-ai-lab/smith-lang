//! The time a test records with `--times` is its own: the time it spends
//! making work that tests share through `shared`, or waiting while another
//! test makes it, is counted in no test, as `shared` says. Otherwise the test
//! that asks first for a server that `smith build` builds takes the build, and
//! so does each test that asks while it is built, and which test that is
//! changes from run to run.
//!
//! The run is started again, as a program, with the two tests of `RUNS` alone
//! and `--times`, so that both ask at once for one value that takes a second
//! to make, as a run of only the tests that were slow does.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use crate::Test;
use crate::clock;
use crate::shared;

pub const ONCE: &[Test] = &[(
    "a_test_records_none_of_the_time_shared_work_takes_whether_it_makes_it_or_waits_for_it",
    a_test_records_none_of_the_time_shared_work_takes_whether_it_makes_it_or_waits_for_it,
)];

/// The tests the run is started to run, and not part of any other group.
pub const RUNS: &[Test] = &[
    ("demo_shared_first", demo_shared),
    ("demo_shared_second", demo_shared),
];

/// How long the shared value takes to make, on the wall.
const MAKING: u64 = 1000;

/// How many of the tests have begun.
static BEGUN: AtomicUsize = AtomicUsize::new(0);

/// Asks for a value whose making runs a program for `MAKING` ms, as a build
/// does, once both tests have begun, so that one makes it and the other
/// waits for it.
fn demo_shared() {
    static VALUE: shared::Once<u32> = shared::Once::new();
    BEGUN.fetch_add(1, Ordering::SeqCst);
    // A run with one thread runs the tests one after the other, and the second
    // finds the value made.
    let together = std::thread::available_parallelism().map_or(1, |n| n.get()) > 1;
    while together && BEGUN.load(Ordering::SeqCst) < RUNS.len() {
        std::thread::yield_now();
    }
    VALUE.get(|| {
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command.arg(format!("--wait={MAKING}"));
        clock::output_within(&mut command, false).unwrap();
        1
    });
}

fn a_test_records_none_of_the_time_shared_work_takes_whether_it_makes_it_or_waits_for_it() {
    let times = crate::scratch::path("shared_time").with_extension("csv");
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command.arg("--group=hang").arg(format!("--times={}", times.display())).arg("demo_shared_");
    let output = clock::output_within(&mut command, true).unwrap();
    let printed = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(output.status.success(), "{printed}");
    let written = std::fs::read_to_string(&times).unwrap();
    std::fs::remove_file(&times).unwrap();
    let recorded: Vec<(&str, u64)> = written
        .lines()
        .map(|line| {
            let (name, ms) = line.split_once(',').unwrap();
            (name, ms.parse().unwrap())
        })
        .collect();
    assert_eq!(recorded.len(), RUNS.len(), "{written}");
    for (name, ms) in recorded {
        // A test that took the making, or the wait for it, took the whole second.
        assert!(
            Duration::from_millis(ms) < Duration::from_millis(MAKING / 2),
            "{name} recorded {ms} ms, of which {MAKING} ms are the shared value's: {written}"
        );
    }
}
