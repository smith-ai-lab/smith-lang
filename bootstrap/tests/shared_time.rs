//! The time and the steps a test records with `--times` and `--steps` are its
//! own: what it spends making work that tests share through `shared`, or
//! waiting while another test makes it, is counted in no test, as `shared`
//! says. Otherwise the test that asks first for a server that `smith build`
//! builds takes the build, and so does each test that asks while it is built,
//! and which test that is changes from run to run.
//!
//! The run is started again, as a program, with the tests of `RUNS` alone, so
//! that they ask at once for shared values, as a run of only the tests that
//! were slow does. Each says how long it spent asking, which the time it
//! records must leave out.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Instant;

use crate::Test;
use crate::clock;
use crate::shared;

pub const ONCE: &[Test] = &[(
    "a_test_records_none_of_the_time_and_steps_shared_work_takes_and_two_keys_are_made_at_once",
    a_test_records_none_of_the_time_and_steps_shared_work_takes_and_two_keys_are_made_at_once,
)];

/// The tests the run is started to run, and not part of any other group.
pub const RUNS: &[Test] = &[
    ("demo_shared_once_first", demo_shared_once_first),
    ("demo_shared_once_second", demo_shared_once_second),
    ("demo_shared_key_a", demo_shared_key_a),
    ("demo_shared_key_b", demo_shared_key_b),
];

/// The variable that names the folder each demo writes how long it asked in.
const ASKED: &str = "SMITH_SHARED_ASKED";

/// Whether the run has threads enough for the demos to ask at the same time.
/// A run with one runs them one after another, and none waits for another.
fn together() -> bool {
    std::thread::available_parallelism().map_or(1, |n| n.get()) > 1
}

/// Work of a known number of steps: lowering a program and running it in the
/// evaluator, as making a shared program does.
fn work() -> usize {
    let source = "use std.bits (u64)\nuse std.list (push)\n\nfn main() <alloc> {\n  mut xs: List[u64] = []\n  loop i in 0..100000 { xs = push(take xs, i) }\n}\n";
    let program = smith::lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?}"));
    smith::scalar::run(&program).stdout.len()
}

/// Asks for the value with `ask`, and writes how long the ask took, in
/// microseconds, where the test that started the run reads it.
fn asking(name: &str, ask: impl FnOnce()) {
    let begin = Instant::now();
    ask();
    let micros = begin.elapsed().as_micros();
    let dir = std::env::var(ASKED).expect("the folder the demos write in");
    std::fs::write(std::path::Path::new(&dir).join(name), micros.to_string()).unwrap();
}

/// How many of the demos of `Once` have begun.
static ARRIVED: AtomicUsize = AtomicUsize::new(0);

/// Both demos ask for one value. The one that makes it holds it until the
/// other has begun, which then asks while it is made, and waits for it.
fn demo_shared_once(name: &str) {
    static VALUE: shared::Once<usize> = shared::Once::new();
    ARRIVED.fetch_add(1, Ordering::SeqCst);
    asking(name, || {
        VALUE.get(|| {
            while together() && ARRIVED.load(Ordering::SeqCst) < 2 {
                std::thread::yield_now();
            }
            work()
        });
    });
}

/// Which keys of `KEYS` have begun to be made.
static MAKING: [AtomicBool; 2] = [AtomicBool::new(false), AtomicBool::new(false)];

/// Each demo asks for a key of its own, and the value of each is made only
/// once the other's has begun, so a `Keyed` that made one key while the other
/// waits never ends, and the run says that the demo hangs.
fn demo_shared_key(name: &str, key: usize) {
    static KEYS: shared::Keyed<usize, usize> = shared::Keyed::new();
    asking(name, || {
        KEYS.get(key, || {
            MAKING[key].store(true, Ordering::SeqCst);
            while together() && !MAKING[1 - key].load(Ordering::SeqCst) {
                std::thread::yield_now();
            }
            work()
        });
    });
}

fn demo_shared_once_first() {
    demo_shared_once("demo_shared_once_first");
}

fn demo_shared_once_second() {
    demo_shared_once("demo_shared_once_second");
}

fn demo_shared_key_a() {
    demo_shared_key("demo_shared_key_a", 0);
}

fn demo_shared_key_b() {
    demo_shared_key("demo_shared_key_b", 1);
}

/// The lines `name,number` of a file the run wrote.
fn lines(path: &std::path::Path) -> Vec<(String, u128)> {
    let written = std::fs::read_to_string(path).unwrap();
    written
        .lines()
        .map(|line| {
            let (name, n) = line.split_once(',').unwrap();
            (name.to_string(), n.parse().unwrap())
        })
        .collect()
}

fn a_test_records_none_of_the_time_and_steps_shared_work_takes_and_two_keys_are_made_at_once() {
    let dir = crate::scratch::path("shared_time");
    std::fs::create_dir_all(&dir).unwrap();
    let (times, steps) = (dir.join("times.csv"), dir.join("steps.csv"));
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command
        .env(ASKED, &dir)
        .arg("--group=hang")
        .arg("--hang=4000")
        .arg(format!("--times={}", times.display()))
        .arg(format!("--steps={}", steps.display()))
        .arg("demo_shared_");
    let output = clock::output_within(&mut command, true).unwrap();
    let printed = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    // Neither key waited for the other to be made: the run ended, and every
    // demo passed.
    assert!(output.status.success(), "{printed}");
    let times = lines(&times);
    assert_eq!(times.len(), RUNS.len(), "{times:?}");
    for (name, steps) in lines(&steps) {
        assert_eq!(steps, 0, "{name} took the steps of shared work");
    }
    let mut made = 0;
    for (name, ms) in &times {
        let asked: u128 = std::fs::read_to_string(dir.join(name)).unwrap().parse().unwrap();
        // An ask that found the value made, which a run with one thread gives
        // the second of a kind, held nothing up and is not compared.
        if asked < 2000 {
            continue;
        }
        made += 1;
        // The time is written in whole milliseconds, rounded down, so the
        // test's own time is less than it by less than one.
        assert!(
            ms * 1000 + 1000 <= asked,
            "{name} recorded {ms} ms, though it spent {asked} microseconds asking for shared work"
        );
    }
    std::fs::remove_dir_all(&dir).unwrap();
    // The value of each kind was made in some demo.
    assert!(made >= 2, "{times:?}");
}
