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
        "a_task_that_starts_a_child_and_runs_out_of_memory_in_the_same_poll_has_the_child_ended_with_the_group_and_the_peer_sees_the_end_of_the_file",
        a_task_that_starts_a_child_and_runs_out_of_memory_in_the_same_poll_has_the_child_ended_with_the_group_and_the_peer_sees_the_end_of_the_file,
    ),
];

const PROGRAM: &str = include_str!("fail_native.smith");
const PEER: &str = include_str!("fail_peer.smith");

/// Runs the program, for each target that has the native unwinder, under a limit
/// of address space, and says what it wrote and how it ended.
fn limited(source: &str, what: &str, check: impl Fn(&str, Option<i32>)) {
    // The native unwinder is that of x86-64 on Linux, which limits address space
    // by `ulimit -v`.
    for target in targets::runnable()
        .into_iter()
        .filter(|t| t.name().starts_with("linux") && !t.name().ends_with("arm64"))
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
                .arg("ulimit -v 60000; exec \"$0\"")
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
    let joined = PROGRAM.replace("  sleep(100)\n", "  join(child)\n");
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
    for target in targets::runnable()
        .into_iter()
        .filter(|t| t.name().starts_with("linux") && !t.name().ends_with("arm64"))
    {
        let bytes = targets::build(PEER.as_bytes(), target).unwrap_or_else(|e| panic!("{e:?}"));
        targets::record(target, &[&bytes]);
        if targets::fingerprinting() {
            continue;
        }
        let dir = crate::scratch::path("fail_peer");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("program");
        crate::executable::write(&path, &bytes);
        let mut server = ports::serve(ports::all(), |port| {
            let mut child = Command::new("sh")
                .arg("-c")
                .arg("ulimit -v 60000; exec \"$0\" \"$@\"")
                .arg(&path)
                .arg(port.to_string())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
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
