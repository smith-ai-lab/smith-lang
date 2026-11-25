//! A group reads the clock once in a turn, and decides from that one value which
//! tasks that wait for a time are woken and how long to wait for the others, so
//! that no task is neither: `time_left` gives 0 exactly when the time has come,
//! for every time and every moment, and a time that comes between two reads of
//! the clock is lost by the structure that read it twice and by no read of one.
//! A task that waits for a time makes the group wait in the system for that
//! long, and does not make turns that find nothing to run.

use std::process::Command;
use std::time::Duration;

use smith::lower;
use smith::scalar;
use smith::target::Target;

use crate::Test;
use crate::cli::{folder, smith, stderr};
use crate::targets;

pub const ONCE: &[Test] = &[(
    "a_time_is_come_exactly_when_none_of_it_is_left_and_a_time_between_two_reads_of_the_clock_is_lost_by_two_reads_and_by_no_one",
    a_time_is_come_exactly_when_none_of_it_is_left_and_a_time_between_two_reads_of_the_clock_is_lost_by_two_reads_and_by_no_one,
)];

pub const MACHINE: &[Test] = &[(
    "a_group_whose_only_task_sleeps_waits_in_the_system_and_makes_no_turn_that_finds_nothing_to_run",
    a_group_whose_only_task_sleeps_waits_in_the_system_and_makes_no_turn_that_finds_nothing_to_run,
)];

/// For each time `d` and each moment `a`, `time_left(d, a)` is 0 exactly when
/// `d <= a`. A group that reads the clock at `a`, wakes the tasks that have none
/// left, and then reads it again at `b` and waits for the tasks that have some
/// left loses the task with `a < d <= b`: it was not woken and is not waited
/// for. With one read, `b` is `a`, and no task is lost.
fn a_time_is_come_exactly_when_none_of_it_is_left_and_a_time_between_two_reads_of_the_clock_is_lost_by_two_reads_and_by_no_one()
{
    let source = "use std.bits (u64)
use std.console (console, print)
use std.int (wrapping_add)
use std.task (time_left)

fn main() <console, alloc> {
  var wrong: u64 = 0
  var lost_two: u64 = 0
  var lost_one: u64 = 0
  loop d in 0..8 {
    loop a in 0..8 {
      come = d <= a
      left = time_left(d, a)
      if come and left != 0 { wrong = wrapping_add(wrong, 1) }
      if not come and left == 0 { wrong = wrapping_add(wrong, 1) }
      if left != 0 and time_left(d, a) == 0 { lost_one = wrapping_add(lost_one, 1) }
      loop b in 0..8 {
        if a <= b and time_left(d, a) != 0 and time_left(d, b) == 0 { lost_two = wrapping_add(lost_two, 1) }
      }
    }
  }
  print(\"{wrong} {lost_two} {lost_one}\")
}
";
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(scalar::check(&program), Ok(()));
    let (outcome, _) = scalar::run_counting(&scalar::passes(&program), usize::MAX);
    assert_eq!(outcome.status, 0);
    // The times a < d <= b among the moments 0 to 7 and the times 0 to 7.
    let mut lost = 0;
    for d in 0..8u32 {
        for a in 0..8u32 {
            for b in a..8u32 {
                if a < d && d <= b {
                    lost += 1;
                }
            }
        }
    }
    assert!(lost > 0);
    assert_eq!(String::from_utf8(outcome.stdout).unwrap(), format!("0 {lost} 0\n"));
}

const PROGRAM: &str = "use std.console (console, print)
use std.task (join, spawn, task, tasks)
use std.time (sleep, time)

fn main() <console, alloc, time> {
  with tasks()
  t = spawn { sleep(600) }
  join(t)
  print(\"slept\")
}
";

fn kernel() -> Vec<Target> {
    targets::runnable()
        .into_iter()
        .filter(|t| t.name().starts_with("linux") || t.name().starts_with("macos"))
        .collect()
}

/// The seconds that the shell's `times` says its children used, user and system
/// together, of the last two lines it prints: the shell's own and its children's.
fn children_seconds(said: &str) -> f64 {
    let line = said.lines().rfind(|l| !l.trim().is_empty()).expect("times printed a line");
    line.split_whitespace()
        .map(|t| {
            let (m, s) = t.split_once('m').expect("minutes");
            let s = s.trim_end_matches('s');
            m.parse::<f64>().expect("minutes") * 60.0 + s.parse::<f64>().expect("seconds")
        })
        .sum()
}

/// A program of one task that sleeps 600 ms waits in the system for that time. A
/// group that made turns that found nothing to run until the time came would use
/// the processor for the 600 ms; this one uses a few.
fn a_group_whose_only_task_sleeps_waits_in_the_system_and_makes_no_turn_that_finds_nothing_to_run() {
    for target in kernel() {
        let dir = folder(&format!("group_clock_{}", target.name()));
        std::fs::write(dir.join("nap.smith"), PROGRAM).unwrap();
        let built = smith(&dir, &["build", "--target", target.name(), "nap.smith"]);
        assert!(built.status.success(), "{}", stderr(&built));
        if targets::fingerprinting() {
            continue;
        }
        let path = dir.join(target.executable_name("nap"));
        let output = crate::clock::output_within(
            Command::new("sh").arg("-c").arg("\"$0\"; times").arg(&path),
            true,
        )
        .unwrap();
        let said = String::from_utf8_lossy(&output.stdout);
        assert!(said.starts_with("slept"), "{said:?} on {}", target.name());
        let used = children_seconds(&said);
        assert!(
            Duration::from_secs_f64(used) < Duration::from_millis(250),
            "the program used {used} s of the processor to sleep 0.6 s, on {}",
            target.name()
        );
    }
}
