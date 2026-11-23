//! A task that waits for a time suspends, and the others run meanwhile: the
//! group that is the scheduler of a program waits in `epoll` (or `kqueue`) with
//! the timeout of the nearest time, and the scheduler of a test goes the time of
//! the test forward to it, so a test of a timeout takes no time. A task can also
//! wait for a handle until a time, whichever comes first, and is told which.
//! The programs run, so these are tests of the kind of machine that runs them.

use std::io::{BufRead, BufReader};
use std::net::UdpSocket;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use smith::target::Target;

use crate::Test;
use crate::ports;
use crate::targets;

pub const MACHINE: &[Test] = &[
    (
        "a_task_that_sleeps_the_largest_number_is_still_waiting_when_one_that_sleeps_a_millisecond_is_done",
        a_task_that_sleeps_the_largest_number_is_still_waiting_when_one_that_sleeps_a_millisecond_is_done,
    ),
    (
        "a_task_that_sleeps_is_never_woken_before_the_time_it_asked_for",
        a_task_that_sleeps_is_never_woken_before_the_time_it_asked_for,
    ),
    (
        "two_tasks_that_sleep_wake_in_the_order_of_their_times_and_the_program_takes_the_longer_of_them",
        two_tasks_that_sleep_wake_in_the_order_of_their_times_and_the_program_takes_the_longer_of_them,
    ),
    (
        "a_thousand_tasks_that_each_sleep_wake_once_each_and_the_program_takes_the_time_of_one",
        a_thousand_tasks_that_each_sleep_wake_once_each_and_the_program_takes_the_time_of_one,
    ),
    (
        "a_task_that_waits_for_a_socket_with_a_deadline_wakes_by_its_time_with_no_data_and_by_the_data_when_it_comes",
        a_task_that_waits_for_a_socket_with_a_deadline_wakes_by_its_time_with_no_data_and_by_the_data_when_it_comes,
    ),
    (
        "under_the_scheduler_of_a_test_a_thousand_sleeps_wake_in_the_order_of_their_times_and_take_no_time",
        under_the_scheduler_of_a_test_a_thousand_sleeps_wake_in_the_order_of_their_times_and_take_no_time,
    ),
    (
        "under_the_scheduler_of_a_test_a_wait_for_a_handle_by_a_time_wakes_by_the_handle_when_the_test_says_it_is_ready_and_by_the_time_when_it_does_not",
        under_the_scheduler_of_a_test_a_wait_for_a_handle_by_a_time_wakes_by_the_handle_when_the_test_says_it_is_ready_and_by_the_time_when_it_does_not,
    ),
];

/// The targets whose scheduler waits with the system's queue.
fn kernel() -> Vec<Target> {
    targets::runnable()
        .into_iter()
        .filter(|t| t.name().starts_with("linux") || t.name().starts_with("macos"))
        .collect()
}

/// Runs the program for the target with the words and gives the lines it
/// printed, and how long it took, from outside.
fn run(target: Target, source: &str, words: &[&str], during: impl FnOnce(&mut BufReader<std::process::ChildStdout>)) -> (Vec<String>, Duration) {
    let bytes = targets::build(source.as_bytes(), target).unwrap_or_else(|e| panic!("{e:?}"));
    targets::record(target, &[&bytes]);
    if targets::fingerprinting() {
        return (Vec::new(), Duration::ZERO);
    }
    let dir = crate::scratch::path("time_waits");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("program");
    crate::executable::write(&path, &bytes);
    let started = Instant::now();
    let mut child = Command::new(&path)
        .args(words)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    std::fs::remove_dir_all(&dir).ok();
    let mut lines = BufReader::new(child.stdout.take().unwrap());
    during(&mut lines);
    let mut rest = String::new();
    let mut all: Vec<String> = Vec::new();
    while lines.read_line(&mut rest).unwrap() > 0 {
        all.push(rest.trim_end().to_string());
        rest.clear();
    }
    let status = child.wait().unwrap();
    assert!(status.success(), "{} ended with {status}", target.name());
    (all, started.elapsed())
}

/// The names the programs share: a task that sleeps and says so.
const NAP: &str = "use std.bits (u64)
use std.console (console, print)
use std.int (wrapping_sub)
use std.task (join, spawn, task, tasks)
use std.time (now, sleep, time)

fn nap(ms: u64) -> u64 ! console, time, alloc, task {
  sleep(ms)
  print(\"woke {ms}\")
  ms
}
";

/// The clock says the milliseconds that have gone whole, so a wake that is
/// rounded down comes up to a millisecond early, half of one on average: 2,000
/// sleeps of a millisecond, one after another, take at least 2,000 milliseconds
/// by the clock of the test, and about 1,000 when each is woken early.
fn a_task_that_sleeps_is_never_woken_before_the_time_it_asked_for() {
    let source = "use std.console (console, print)
use std.task (join, spawn, task, tasks)
use std.time (sleep, time)

fn main() ! console, alloc, time {
  loop i in 0..2000 {
    with tasks()
    t = spawn { sleep(1) }
    join(t)
  }
  print(\"slept\")
}
";
    for target in kernel() {
        let (lines, took) = run(target, source, &[], |_| {});
        if targets::fingerprinting() {
            continue;
        }
        assert_eq!(lines, ["slept"], "{}", target.name());
        assert!(took >= Duration::from_millis(2000), "{took:?} on {}", target.name());
        assert!(took < Duration::from_secs(20), "{took:?} on {}", target.name());
    }
}

fn two_tasks_that_sleep_wake_in_the_order_of_their_times_and_the_program_takes_the_longer_of_them() {
    let source = format!(
        "{NAP}
fn main() ! console, alloc, time {{
  with tasks()
  a = spawn {{ nap(300) }}
  b = spawn {{ nap(100) }}
  x = join(a)
  y = join(b)
  print(\"done {{x + y}}\")
}}
"
    );
    for target in kernel() {
        let (lines, took) = run(target, &source, &[], |_| {});
        if targets::fingerprinting() {
            continue;
        }
        assert_eq!(lines, ["woke 100", "woke 300", "done 400"], "{}", target.name());
        // The two sleep at once: about 300 milliseconds, and not 400.
        assert!(took >= Duration::from_millis(300), "{took:?} on {}", target.name());
        assert!(took < Duration::from_millis(390), "{took:?} on {}", target.name());
    }
}

fn a_thousand_tasks_that_each_sleep_wake_once_each_and_the_program_takes_the_time_of_one() {
    let source = format!(
        "{NAP}
fn sleeper(i: u64) -> u64 ! console, time, alloc, task {{
  sleep(100)
  print(\"{{i}}\")
  i
}}

fn main() ! console, alloc, time {{
  with tasks()
  loop i in 0..1000 {{
    t = spawn {{ sleeper(i) }}
  }}
  last = spawn {{ nap(400) }}
  join(last)
  print(\"done\")
}}
"
    );
    for target in kernel() {
        let (lines, took) = run(target, &source, &[], |_| {});
        if targets::fingerprinting() {
            continue;
        }
        let mut woke: Vec<u64> = lines.iter().filter_map(|l| l.parse().ok()).collect();
        woke.sort();
        assert_eq!(woke, (0..1000).collect::<Vec<u64>>(), "{}", target.name());
        assert_eq!(lines.last().map(String::as_str), Some("done"), "{}", target.name());
        assert_eq!(lines[lines.len() - 2], "woke 400", "{}", target.name());
        // A thousand sleeps of 100 milliseconds, one after another, are 100 seconds.
        assert!(took < Duration::from_secs(3), "{took:?} on {}", target.name());
    }
}

/// A task that waits for the datagram socket with a deadline, and says which
/// came first, once it has said that its socket is open.
const BY: &str = "use std.bits (u64)
use std.console (console, print)
use std.deserialize (ReadError)
use std.fromtext (from_text)
use std.net (NetError, bind, handle, net)
use std.task (join, spawn, task, tasks, wait_to_read_by)

fn waiting(port: 0..65536, ms: u64) -> Result[bool, NetError] ! console, net, alloc, task {
  with s = bind(\"127.0.0.1\", port)
  print(\"ready\")
  .ok(wait_to_read_by(handle(s), ms))
}

fn main(args: List[str]) ! console, alloc, net {
  if args.size < 2 { return }
  number: Result[0..65536, ReadError] = from_text(\"port\", args[0])
  limit: Result[u64, ReadError] = from_text(\"limit\", args[1])
  match (number, limit) {
    (.ok(port), .ok(ms)) {
      with tasks()
      t = spawn { waiting(port, ms) }
      match join(t) {
        .ok(data) { print(\"data {data}\") }
        .err(e) { print(\"failed\") }
      }
    }
    _ { print(\"usage\") }
  }
}
";

fn a_task_that_waits_for_a_socket_with_a_deadline_wakes_by_its_time_with_no_data_and_by_the_data_when_it_comes() {
    for target in kernel() {
        // With no datagram it is the time that wakes it, and not before it.
        let port = ports::next();
        let (lines, took) = run(target, BY, &[&port.to_string(), "200"], |_| {});
        if targets::fingerprinting() {
            continue;
        }
        assert_eq!(lines, ["ready", "data false"], "{}", target.name());
        assert!(took >= Duration::from_millis(200), "{took:?} on {}", target.name());
        // With one, the datagram wakes it long before the deadline.
        let port = ports::next();
        let (lines, took) = run(target, BY, &[&port.to_string(), "20000"], |lines| {
            let mut ready = String::new();
            lines.read_line(&mut ready).unwrap();
            assert_eq!(ready.trim_end(), "ready");
            let from = UdpSocket::bind("127.0.0.1:0").unwrap();
            from.send_to(b"here", ("127.0.0.1", port)).unwrap();
        });
        assert_eq!(lines, ["data true"], "{}", target.name());
        assert!(took < Duration::from_secs(10), "{took:?} on {}", target.name());
    }
}

/// A thousand tasks of a test's scheduler that each sleep as many milliseconds as
/// their number says, and say so when they wake, and one that sleeps longer, and
/// how long the program took by the clock that is not the test's.
const VIRTUAL: &str = "use std.bits (u64)
use std.console (console, print)
use std.int (wrapping_sub)
use std.task (join, spawn, task, test_scheduler)
use std.time (now, sleep, time)

fn sleeper(i: u64) -> u64 ! console, time, alloc, task {
  sleep(i)
  print(\"{i}\")
  i
}

fn main(args: List[str]) ! console, alloc, time, diverge {
  before = now()
  with test_scheduler(7)
  loop i in 1..1001 {
    t = spawn { sleeper(i) }
  }
  last = spawn { sleeper(5000) }
  join(last)
  print(\"done {wrapping_sub(now(), before)}\")
}
";

/// A wait of the largest number of milliseconds, begun when the clock is past 0,
/// is a moment that is past the largest number, which is the largest number and
/// not a moment that has gone: a sum that wrapped would wake the task at once,
/// before the short one.
fn a_task_that_sleeps_the_largest_number_is_still_waiting_when_one_that_sleeps_a_millisecond_is_done() {
    // Which task runs first when both may is what the seed picks, so the seeds
    // are many: a sum that wrapped wakes the long one with the short one, and
    // some seed runs it before the end.
    for seed in 0..10 {
        let source = format!(
            "use std.bits (u64)
use std.console (console, print)
use std.task (join, spawn, task, test_scheduler)
use std.time (sleep, time)

fn long() -> u64 ! console, time, alloc, task {{
  sleep(5)
  sleep(18446744073709551615)
  print(\"long woke\")
  0
}}

fn short() -> u64 ! console, time, alloc, task {{
  sleep(10)
  print(\"short\")
  1
}}

fn main() ! console, alloc, time, diverge {{
  with test_scheduler({seed})
  a = spawn {{ long() }}
  b = spawn {{ short() }}
  join(b)
  print(\"done\")
}}
"
        );
        for target in kernel() {
            let (lines, _) = run(target, &source, &[], |_| {});
            if targets::fingerprinting() {
                continue;
            }
            assert_eq!(lines, ["short", "done"], "seed {seed} on {}", target.name());
        }
    }
}

fn under_the_scheduler_of_a_test_a_thousand_sleeps_wake_in_the_order_of_their_times_and_take_no_time() {
    for target in kernel() {
        let (lines, took) = run(target, VIRTUAL, &[], |_| {});
        if targets::fingerprinting() {
            continue;
        }
        let mut expected: Vec<String> = (1..=1000).map(|i| i.to_string()).collect();
        expected.push("5000".to_string());
        assert_eq!(&lines[..1001], &expected[..], "{}", target.name());
        // Fifteen seconds of sleeping, which the clock of the test goes over.
        assert!(took < Duration::from_secs(5), "{took:?} on {}", target.name());
        let elapsed: u64 = lines[1001].strip_prefix("done ").unwrap().parse().unwrap();
        assert!(elapsed < 4000, "{elapsed} on {}", target.name());
    }
}

/// Two tasks that wait for a handle until a time, of which the test says one
/// handle is ready: the handles are two numbers that differ only above the 24th
/// bit, and which one is ready is given by the words the program is started with.
const HANDLES: &str = "use std.bits (u64)
use std.console (console, print)
use std.deserialize (ReadError)
use std.fromtext (from_text)
use std.task (join, spawn, task, test_ready_by, test_scheduler_with_io)

fn waits(handle: u64) -> bool ! task {
  test_ready_by(handle, 20)
}

fn main(args: List[str]) ! console, alloc, diverge {
  if args.size < 1 { return }
  which: Result[u64, ReadError] = from_text(\"handle\", args[0])
  match which {
    .ok(ready) {
      with test_scheduler_with_io(3, [ready])
      a = spawn { waits(1099511627781) }
      b = spawn { waits(5) }
      c = spawn { waits(9) }
      print(\"{join(a)} {join(b)} {join(c)}\")
    }
    .err(e) { print(\"usage\") }
  }
}
";

fn under_the_scheduler_of_a_test_a_wait_for_a_handle_by_a_time_wakes_by_the_handle_when_the_test_says_it_is_ready_and_by_the_time_when_it_does_not() {
    for target in kernel() {
        // The handle with a bit above the 24th set is ready: only its task is told
        // so, and the one whose handle is that number without the bit is not.
        let (lines, _) = run(target, HANDLES, &["1099511627781"], |_| {});
        if targets::fingerprinting() {
            continue;
        }
        assert_eq!(lines, ["true false false"], "{}", target.name());
        let (lines, _) = run(target, HANDLES, &["5"], |_| {});
        assert_eq!(lines, ["false true false"], "{}", target.name());
        let (lines, _) = run(target, HANDLES, &["9"], |_| {});
        assert_eq!(lines, ["false false true"], "{}", target.name());
        // A handle that nothing waits for wakes none of them, by their times.
        let (lines, _) = run(target, HANDLES, &["77"], |_| {});
        assert_eq!(lines, ["false false false"], "{}", target.name());
    }
}
