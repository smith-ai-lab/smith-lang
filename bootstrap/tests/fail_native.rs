//! A task that runs out of memory ends, and the program goes on: the system
//! refuses the memory of a child, which ends by a failure that goes back up
//! its frames to the poll of the scheduler, so that its parent, which waits for
//! it, goes on and prints. The memory is limited to 60 MB of address space, which
//! the child passes at the 128 MB request of its list's growth, and a program that
//! ended the whole program at the refusal would print nothing and give the status 1.

use std::io::Read;
use std::net::TcpStream;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::Test;
use crate::ports;
use crate::targets;

pub const MACHINE: &[Test] = &[
    (
        "a_child_that_runs_out_of_memory_ends_and_its_parent_goes_on",
        a_child_that_runs_out_of_memory_ends_and_its_parent_goes_on,
    ),
    (
        "a_join_of_a_child_that_ran_out_of_memory_ends_the_program_with_the_status_1_and_gives_no_value",
        a_join_of_a_child_that_ran_out_of_memory_ends_the_program_with_the_status_1_and_gives_no_value,
    ),
    (
        "a_task_that_holds_a_connection_and_runs_out_of_memory_closes_it_and_the_peer_sees_the_end_of_the_file",
        a_task_that_holds_a_connection_and_runs_out_of_memory_closes_it_and_the_peer_sees_the_end_of_the_file,
    ),
    (
        "a_task_that_holds_a_connection_and_whose_write_the_system_refuses_closes_it_and_the_peer_sees_the_end_of_the_file",
        a_task_that_holds_a_connection_and_whose_write_the_system_refuses_closes_it_and_the_peer_sees_the_end_of_the_file,
    ),
    (
        "a_task_that_starts_a_child_and_runs_out_of_memory_in_the_same_poll_has_the_child_ended_with_the_group_and_the_peer_sees_the_end_of_the_file",
        a_task_that_starts_a_child_and_runs_out_of_memory_in_the_same_poll_has_the_child_ended_with_the_group_and_the_peer_sees_the_end_of_the_file,
    ),
    (
        "every_round_of_a_child_that_runs_out_of_memory_reaches_as_far_as_the_first_because_its_memory_goes_back",
        every_round_of_a_child_that_runs_out_of_memory_reaches_as_far_as_the_first_because_its_memory_goes_back,
    ),
    (
        "every_round_of_a_child_that_a_parent_joins_and_that_runs_out_of_memory_reaches_as_far_as_the_first",
        every_round_of_a_child_that_a_parent_joins_and_that_runs_out_of_memory_reaches_as_far_as_the_first,
    ),
];

const PROGRAM: &str = include_str!("fail_native.smith");
const ROUNDS: &str = include_str!("fail_rounds.smith");
const PEER: &str = include_str!("fail_peer.smith");
const CLOSED: &str = include_str!("fail_closed.smith");
const PRINTING: &str = include_str!("fail_printing.smith");

/// Runs the program, for each target that has the native unwinder, under a limit
/// of address space, and says what it wrote and how it ended.
fn limited(source: &str, what: &str, check: impl Fn(&str, Option<i32>)) {
    limited_to(60000, source, what, check);
}

/// The same, with `kilobytes` of address space.
fn limited_to(kilobytes: u64, source: &str, what: &str, check: impl Fn(&str, Option<i32>)) {
    // The native unwinder is that of Linux on x86-64 and arm64, which limit address space
    // by `ulimit -v`.
    for target in targets::runnable()
        .into_iter()
        .filter(|t| t.name().starts_with("linux"))
    {
        let bytes = targets::build(source.as_bytes(), target).unwrap_or_else(|e| panic!("{e:?}"));
        targets::record(target, &[&bytes]);
        if targets::fingerprinting() {
            continue;
        }
        let dir = crate::scratch::path(what);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("program");
        crate::executable::write(&path, &bytes);
        let output = crate::clock::output_within(
            Command::new("sh")
                .arg("-c")
                .arg(format!("ulimit -v {kilobytes}; exec \"$0\""))
                .arg(&path),
            true,
        )
        .unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        check(&String::from_utf8_lossy(&output.stdout), output.status.code());
    }
}

fn a_child_that_runs_out_of_memory_ends_and_its_parent_goes_on() {
    limited(PROGRAM, "fail_native", |said, status| {
        assert_eq!((said, status), ("the parent went on\n", Some(0)));
    });
}

/// The child never made the value a join gives, so the join ends the program, as
/// a failure of `main` does, and what follows it is not written.
fn a_join_of_a_child_that_ran_out_of_memory_ends_the_program_with_the_status_1_and_gives_no_value() {
    let joined = PROGRAM.replace("  sleep(ms(100))\n", "  join(child)\n");
    assert_ne!(joined, PROGRAM);
    limited(&joined, "fail_joined", |said, status| {
        assert_eq!((said, status), ("", Some(1)));
    });
}

/// A task that starts a child that holds the connection of a peer, and then runs
/// out of memory in the same poll, has ended, and the child, which the task's
/// failure took the scheduler's own record of out of the results of the poll, is
/// found by the end of the group: the peer sees the end of the file while the
/// program, which goes on for 2.5 seconds, is still running.
fn a_task_that_starts_a_child_and_runs_out_of_memory_in_the_same_poll_has_the_child_ended_with_the_group_and_the_peer_sees_the_end_of_the_file()
 {
    the_peer_sees_the_end_of_the_file(PEER, "fail_peer", false);
}

/// A task that holds the connection of a peer and runs out of memory at a call
/// closes it as it goes up, so the peer sees the end of the file while the group
/// and the program go on, and not when the group ends.
fn a_task_that_holds_a_connection_and_runs_out_of_memory_closes_it_and_the_peer_sees_the_end_of_the_file()
 {
    the_peer_sees_the_end_of_the_file(CLOSED, "fail_closed", false);
}

/// A task that holds the connection of a peer and writes through a small function
/// that is inlined, and whose write the system refuses, closes it as it goes up.
fn a_task_that_holds_a_connection_and_whose_write_the_system_refuses_closes_it_and_the_peer_sees_the_end_of_the_file()
 {
    the_peer_sees_the_end_of_the_file(PRINTING, "fail_printing", true);
}

/// Runs the program, which takes a connection of a peer and goes on for 2.5
/// seconds after its group has ended, and asks that the peer reads the end of the
/// file within 1.8 seconds, while the program is still running.
///
/// With `reader_gone`, nothing reads standard output, so that a write is refused,
/// and the signal for a write to a pipe nobody reads is ignored.
fn the_peer_sees_the_end_of_the_file(source: &str, what: &str, reader_gone: bool) {
    for target in targets::runnable()
        .into_iter()
        .filter(|t| t.name().starts_with("linux"))
    {
        let bytes = targets::build(source.as_bytes(), target).unwrap_or_else(|e| panic!("{e:?}"));
        targets::record(target, &[&bytes]);
        if targets::fingerprinting() {
            continue;
        }
        let dir = crate::scratch::path(what);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("program");
        crate::executable::write(&path, &bytes);
        let mut server = ports::serve(ports::all(), |port| {
            let mut child = Command::new("sh")
                .arg("-c")
                .arg("trap '' PIPE; ulimit -v 60000; exec \"$0\" \"$@\"")
                .arg(&path)
                .arg(port.to_string())
                .stdout(if reader_gone {
                    Stdio::piped()
                } else {
                    Stdio::null()
                })
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            if reader_gone {
                drop(child.stdout.take());
            }
            // A program that cannot bind the port ends at once, and says so.
            std::thread::sleep(Duration::from_millis(300));
            match child.try_wait().unwrap() {
                None => ports::Start::Up((child, port)),
                Some(status) => {
                    let said = ports::said(&mut child);
                    if said.contains("in use") {
                        ports::Start::Taken
                    } else {
                        ports::Start::Failed(format!("the program ended with {status}: {said:?}"))
                    }
                }
            }
        });
        let end = Instant::now() + Duration::from_secs(8);
        let mut peer = loop {
            match TcpStream::connect(("127.0.0.1", server.1)) {
                Ok(stream) => break stream,
                Err(e) => {
                    assert!(Instant::now() < end, "no connection to the program: {e}");
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        };
        peer.set_read_timeout(Some(Duration::from_millis(1800))).unwrap();
        let read = loop {
            // A signal that reaches the thread interrupts the read, which says
            // nothing of the connection, so the read is made again.
            match peer.read(&mut [0u8; 1]) {
                Ok(0) => break Ok(()),
                Ok(n) => break Err(format!("{n} bytes were read")),
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => break Err(format!("{:?}: {e}", e.kind())),
            }
        };
        // A program that ends closes its connections before the system says it
        // has ended, so it is looked at again a little later.
        std::thread::sleep(Duration::from_millis(300));
        let alive = server.0.try_wait().unwrap().is_none();
        let _ = server.0.kill();
        let _ = server.0.wait();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(read, Ok(()), "{}", target.name());
        assert!(alive, "the program ended first, {}", target.name());
    }
}

/// How far each round of `fail_rounds.smith` got, which it says every thousand
/// pushes, and whether each ended by running out of memory.
fn reaches(said: &str) -> Vec<(u64, bool)> {
    said.split("round ")
        .skip(1)
        .map(|round| {
            let last = round
                .lines()
                .skip(1)
                .filter_map(|line| line.trim().parse::<u64>().ok())
                .max()
                .unwrap_or(0);
            (last, round.contains("out of memory"))
        })
        .collect()
}

/// Rounds of a child that pushes onto a list until the system refuses, each in its
/// own group, under a limit that the first round passes at 4 million pushes: the
/// memory of a task that failed goes back, so each round gets as far as the first.
/// Without it each round got half as far as the one before, since the pieces larger
/// than 2 MB that the first round asked the system for were kept by nothing.
fn every_round_of_a_child_that_runs_out_of_memory_reaches_as_far_as_the_first_because_its_memory_goes_back()
 {
    let source = ROUNDS.replace("outcome(spawn { eaten() })", "outcome(spawn { eat() })");
    assert_ne!(source, ROUNDS);
    limited_to(200000, &source, "fail_rounds", |said, status| {
        let rounds = reaches(said);
        assert_eq!(rounds.len(), 6, "{said}");
        assert!(rounds.iter().all(|(_, failed)| *failed), "{said}");
        assert!(rounds[0].0 >= 1_000_000, "{said}");
        assert!(rounds.iter().all(|(last, _)| *last == rounds[0].0), "{rounds:?}");
        assert_eq!(status, Some(0));
    });
}

/// The same when the child is joined by a task that ends by the child's failure.
fn every_round_of_a_child_that_a_parent_joins_and_that_runs_out_of_memory_reaches_as_far_as_the_first()
 {
    let source = ROUNDS.replace(
        "fn eaten() -> u64 <alloc, console> {\n  eat()\n}",
        "fn eaten() -> u64 <alloc, console, task> {\n  join(spawn { eat() })\n}",
    );
    assert_ne!(source, ROUNDS);
    limited_to(200000, &source, "fail_rounds_joined", |said, status| {
        let rounds = reaches(said);
        assert_eq!(rounds.len(), 6, "{said}");
        assert!(rounds.iter().all(|(_, failed)| *failed), "{said}");
        assert!(rounds[0].0 >= 1_000_000, "{said}");
        assert!(rounds.iter().all(|(last, _)| *last == rounds[0].0), "{rounds:?}");
        assert_eq!(status, Some(0));
    });
}
