//! A write to the console that the system refuses, as when standard output goes
//! to a program that has stopped reading, ends the task that made it and no more:
//! the child that writes ends, and its parent, which waits, ends the program by
//! returning, with the status 0. A program that ended at the refused write would
//! have the status 1.

use std::process::{Command, Stdio};

use crate::Test;
use crate::targets;

pub const MACHINE: &[Test] = &[(
    "a_write_the_system_refuses_ends_its_task_and_the_program_goes_on",
    a_write_the_system_refuses_ends_its_task_and_the_program_goes_on,
)];

const PROGRAM: &str = include_str!("fail_write.smith");

fn a_write_the_system_refuses_ends_its_task_and_the_program_goes_on() {
    for target in targets::runnable()
        .into_iter()
        .filter(|t| t.name().starts_with("linux"))
    {
        let bytes = targets::build(PROGRAM.as_bytes(), target).unwrap_or_else(|e| panic!("{e:?}"));
        targets::record(target, &[&bytes]);
        if targets::fingerprinting() {
            continue;
        }
        let dir = crate::scratch::path("fail_write");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("program");
        crate::executable::write(&path, &bytes);
        // The signal for a write to a pipe nobody reads is ignored, so that the
        // write is refused and not the program ended.
        let mut child = Command::new("sh")
            .arg("-c")
            .arg("trap '' PIPE; exec \"$0\"")
            .arg(&path)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        drop(child.stdout.take());
        let status = child.wait().unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(status.code(), Some(0), "{}", target.name());
    }
}
