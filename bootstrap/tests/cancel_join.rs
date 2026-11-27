//! A task that is asked to end while it waits for a child, because its own parent
//! failed, ends the child before itself, and every connection the chain of tasks
//! opened is closed once, those of the child before those of the task: under the
//! scheduler of a test and under the group of a program. A chain of four tasks,
//! each of which holds the connection its parent gave it and one of its own, in
//! which the second runs out of memory while the third waits for the fourth, which
//! never stops waiting. Under a scheduler that gave the third task the frame of
//! a fourth that had not ended, the fourth was never ended, and its two connections,
//! 7 and 8, stayed open.

use std::collections::HashSet;

use smith::{lower, scalar};

use crate::Test;
use crate::targets;

pub const ONCE: &[Test] = &[(
    "a_task_asked_to_end_while_it_waits_for_a_child_ends_the_child_first_and_every_connection_is_closed_once_under_the_scheduler_of_a_test",
    a_task_asked_to_end_while_it_waits_for_a_child_ends_the_child_first_and_every_connection_is_closed_once_under_the_scheduler_of_a_test,
)];

pub const MACHINE: &[Test] = &[(
    "a_task_asked_to_end_while_it_waits_for_a_child_ends_the_child_first_and_every_connection_is_closed_once_under_the_group_of_a_program",
    a_task_asked_to_end_while_it_waits_for_a_child_ends_the_child_first_and_every_connection_is_closed_once_under_the_group_of_a_program,
)];

const TEST_SCHEDULER: &str = include_str!("cancel_join.smith");
const GROUP: &str = include_str!("cancel_join_group.smith");

/// Says that each connection opened was closed once, and that the two of the
/// fourth task, 7 and 8, were closed before the two of the third, 5 and 6.
fn closed_once_children_first(said: &str, source: &str) {
    let lines: Vec<&str> = said.lines().collect();
    let numbers = |word: &str| -> Vec<u64> {
        lines
            .iter()
            .filter_map(|l| l.strip_prefix(word)?.trim().parse().ok())
            .collect()
    };
    let (opened, closed) = (numbers("opened "), numbers("closed "));
    let once: HashSet<u64> = closed.iter().copied().collect();
    assert_eq!(once.len(), closed.len(), "a connection was closed twice: {said} for {source}");
    assert_eq!(
        opened.iter().copied().collect::<HashSet<u64>>(),
        once,
        "the connections opened are not those closed: {said} for {source}"
    );
    let at = |n: u64| closed.iter().position(|c| *c == n).unwrap();
    for child in [7, 8] {
        for parent in [5, 6] {
            assert!(at(child) < at(parent), "{child} was closed after {parent}: {said} for {source}");
        }
    }
}

fn a_task_asked_to_end_while_it_waits_for_a_child_ends_the_child_first_and_every_connection_is_closed_once_under_the_scheduler_of_a_test()
 {
    for seed in [22, 0, 1, 2, 3] {
        let source = TEST_SCHEDULER.replace("test_scheduler_with_io(22,", &format!("test_scheduler_with_io({seed},"));
        let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for {source}"));
        assert_eq!(scalar::check(&program), Ok(()), "for {source}");
        let (outcome, _) = scalar::run_poisoning_freed_frames(&scalar::passes(&program), 1 << 20);
        let said = String::from_utf8(outcome.stdout).expect("text");
        assert_eq!(outcome.status, 0, "{said} for {source}");
        closed_once_children_first(&said, &source);
    }
}

fn a_task_asked_to_end_while_it_waits_for_a_child_ends_the_child_first_and_every_connection_is_closed_once_under_the_group_of_a_program()
 {
    // The memory the second task runs out of is limited by `ulimit -v`, which
    // is Linux's, as the unwinder of the native executables is.
    for target in targets::runnable()
        .into_iter()
        .filter(|t| t.name().starts_with("linux"))
    {
        let bytes = targets::build(GROUP.as_bytes(), target).unwrap_or_else(|e| panic!("{e:?}"));
        targets::record(target, &[&bytes]);
        if targets::fingerprinting() {
            continue;
        }
        let dir = crate::scratch::path("cancel_join");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("program");
        crate::executable::write(&path, &bytes);
        let output = crate::clock::output_within(
            std::process::Command::new("sh")
                .arg("-c")
                .arg("ulimit -v 60000; exec \"$0\"")
                .arg(&path),
            true,
        )
        .unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        let said = String::from_utf8_lossy(&output.stdout).to_string();
        assert_eq!(output.status.code(), Some(0), "{} said {said}", target.name());
        closed_once_children_first(&said, target.name());
    }
}
