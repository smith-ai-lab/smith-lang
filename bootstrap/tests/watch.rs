//! `smith --watch` runs a command again each time a file it read changes, as
//! `command-line.md` and `project-tasks.md` say. The tests start the compiler's
//! executable, change files in a project of their own, and read what it
//! writes and what the program it started answers.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use smith::target::Target;

use crate::Test;
use crate::cli::{folder, smith, stderr};
use crate::clock;
use crate::generate::{self, Gen};
use crate::ports;
use crate::targets;

pub const MACHINE: &[Test] = &[
    (
        "a_served_page_is_the_new_one_after_the_file_is_rewritten",
        a_served_page_is_the_new_one_after_the_file_is_rewritten,
    ),
    (
        "requests_sent_across_changes_are_answered_whole_by_the_old_program_or_the_new",
        requests_sent_across_changes_are_answered_whole_by_the_old_program_or_the_new,
    ),
    (
        "a_task_without_diverge_is_run_again_and_a_run_still_going_is_let_end",
        a_task_without_diverge_is_run_again_and_a_run_still_going_is_let_end,
    ),
    (
        "check_and_run_do_again_what_they_did_after_each_save",
        check_and_run_do_again_what_they_did_after_each_save,
    ),
    (
        "a_file_the_build_did_not_read_does_not_start_a_build",
        a_file_the_build_did_not_read_does_not_start_a_build,
    ),
    (
        "interrupting_smith_watch_ends_the_run_and_leaves_no_process",
        interrupting_smith_watch_ends_the_run_and_leaves_no_process,
    ),
];

/// How long a test waits for what `smith --watch` does before it fails.
const PATIENCE: Duration = Duration::from_secs(60);

/// Whether this machine runs the tests: it listens on sockets and starts
/// programs, and a fingerprint is made of the executables that are run, which
/// the program of `http_listen.rs` is for these.
fn runs_here() -> bool {
    Target::host().is_some() && !targets::fingerprinting()
}

/// The task that serves the page. It says that it started, so that a test sees
/// each run begin.
const SERVE: &str = "use main (main)\nuse std.console (console, print)\nuse std.http (listen)\nuse std.net (net)\nuse std.project (Package)\n\npackage = Package(name: \"site\")\n\npub fn serve(; port: 1..65536 = 8080) <console, net, alloc, diverge> {\n  print(\"started\")\n  with listen(port: port)\n  main()\n}\n";

/// A `main.smith` whose only route gives the text.
fn page(text: &str) -> String {
    format!(
        "use std.http (Text, get, http, routes)\n\nfn index() -> Text {{\n  Text(text: \"{text}\")\n}}\n\npub fn main() <http, alloc> {{\n  with routes()\n  get(\"/\", index)\n}}\n"
    )
}

/// What a person breaks a page with: each is a file that does not build.
const BREAKS: [&str; 3] = [
    "fn index( {\n",
    "pub fn main() <http, alloc> {\n  nowhere()\n}\n",
    "pub fn main() <http, alloc> {\n  with routes()\n  get(\"/\", index)\n}\n",
];

/// A `smith --watch` that has been started, which is ended when the test ends,
/// however it ends.
struct Watch {
    child: Child,
    out: PathBuf,
    err: PathBuf,
    input: Option<ChildStdin>,
}

impl Watch {
    /// Starts `smith` with the words in `dir`, with what it writes in files.
    fn start(dir: &Path, words: &[&str], input: bool) -> Watch {
        let out = dir.join("watch.out");
        let err = dir.join("watch.err");
        let mut command = Command::new(env!("CARGO_BIN_EXE_smith"));
        command
            .args(words)
            .current_dir(dir)
            .env("TMPDIR", crate::scratch::for_programs())
            .stdout(std::fs::File::create(&out).unwrap())
            .stderr(std::fs::File::create(&err).unwrap())
            .stdin(if input { Stdio::piped() } else { Stdio::null() });
        let mut child = command.spawn().unwrap();
        let input = child.stdin.take();
        clock::progressed();
        Watch { child, out, err, input }
    }

    /// What the command and the program it runs wrote to standard output.
    fn said(&self) -> String {
        String::from_utf8_lossy(&std::fs::read(&self.out).unwrap_or_default()).into_owned()
    }

    /// What the command wrote to standard error.
    fn errors(&self) -> String {
        String::from_utf8_lossy(&std::fs::read(&self.err).unwrap_or_default()).into_owned()
    }

    /// How many times the command has said how long it took, with `--timings`.
    fn builds(&self) -> usize {
        self.errors().lines().filter(|l| l.starts_with("total ")).count()
    }

    /// Writes to the standard input of the program.
    fn type_line(&mut self) {
        let input = self.input.as_mut().expect("started with input");
        input.write_all(b"\n").unwrap();
        input.flush().unwrap();
    }

    /// Interrupts `smith --watch` as a terminal does, and says how it ended.
    fn interrupt(&mut self) -> ExitStatus {
        let sent = Command::new("kill").args(["-INT", &self.child.id().to_string()]).status().unwrap();
        assert!(sent.success());
        let end = Instant::now() + PATIENCE;
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                clock::progressed();
                return status;
            }
            assert!(Instant::now() < end, "smith --watch did not end when it was interrupted");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        // Ending it as a terminal does ends the program it started too.
        let _ = Command::new("kill").args(["-TERM", &self.child.id().to_string()]).stderr(Stdio::null()).status();
        let end = Instant::now() + Duration::from_secs(5);
        while Instant::now() < end && self.child.try_wait().ok().flatten().is_none() {
            std::thread::sleep(Duration::from_millis(5));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Waits until `done` is true, and fails the test saying what it waited for.
fn wait_for(what: &str, done: impl FnMut() -> bool) {
    wait_for_within(PATIENCE, what, done);
}

fn wait_for_within(limit: Duration, what: &str, mut done: impl FnMut() -> bool) {
    let end = Instant::now() + limit;
    while !done() {
        clock::progressed();
        assert!(Instant::now() < end, "nothing came of: {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// What a response said, which is whole: its head, and as many bytes of body as
/// `content-length` says.
#[derive(Debug)]
struct Answer {
    status: u16,
    body: String,
}

/// Asks the server at the port for `/`, as a client that closes the connection
/// does, and gives the response, or why there is none.
fn ask(port: u16) -> Result<Answer, String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).map_err(|e| format!("not connected: {e}"))?;
    stream.set_read_timeout(Some(Duration::from_secs(30))).unwrap();
    stream
        .write_all(b"GET / HTTP/1.1\r\nhost: localhost\r\nconnection: close\r\n\r\n")
        .map_err(|e| format!("not written: {e}"))?;
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).map_err(|e| format!("not read: {e}"))?;
    let text = String::from_utf8(bytes).map_err(|e| format!("not text: {e}"))?;
    let (head, body) = text.split_once("\r\n\r\n").ok_or_else(|| format!("no head: {text:?}"))?;
    let status = head
        .strip_prefix("HTTP/1.1 ")
        .and_then(|rest| rest.get(..3))
        .and_then(|code| code.parse().ok())
        .ok_or_else(|| format!("no status: {head:?}"))?;
    let length: usize = head
        .lines()
        .find_map(|l| l.strip_prefix("content-length: "))
        .and_then(|n| n.parse().ok())
        .ok_or_else(|| format!("no length: {head:?}"))?;
    if body.len() != length {
        return Err(format!("a response of {} bytes of {length}: {text:?}", body.len()));
    }
    Ok(Answer { status, body: body.to_string() })
}

/// How long a run that is told to stop has before `smith --watch` kills it,
/// which a swap of runs that goes right is far under.
const SWAP: Duration = Duration::from_secs(8);

/// A project that serves the page, and the port of its server.
fn served(name: &str, text: &str) -> (PathBuf, u16) {
    let dir = folder(name);
    std::fs::write(dir.join("project.smith"), SERVE).unwrap();
    std::fs::write(dir.join("main.smith"), page(text)).unwrap();
    (dir, ports::next())
}

/// Waits until the server answers with the text, as it does once it is built
/// and run.
fn until_it_says(port: u16, text: &str) {
    wait_for(&format!("the server to say {text}"), || {
        matches!(ask(port), Ok(a) if a.status == 200 && a.body == text)
    });
}

fn a_served_page_is_the_new_one_after_the_file_is_rewritten() {
    if !runs_here() {
        return;
    }
    let (dir, port) = served("watch_page", "one");
    let watch = Watch::start(&dir, &["--watch", "serve", "--port", &port.to_string()], false);
    until_it_says(port, "one");
    // The run before is told to stop and ends with no request to wake it, and
    // the next run begins.
    std::fs::write(dir.join("main.smith"), page("three")).unwrap();
    wait_for_within(SWAP, "the second run to begin", || watch.said() == "started\nstarted\n");
    std::fs::write(dir.join("main.smith"), page("two")).unwrap();
    wait_for_within(SWAP, "the server to say two", || matches!(ask(port), Ok(a) if a.body == "two"));
    // Once it says the new page it never says the old again.
    for _ in 0..20 {
        let said = ask(port).unwrap();
        assert_eq!(said.body, "two");
    }
}

/// One change made to the page.
#[derive(Debug, Clone)]
enum Change {
    /// The page says another text.
    Text,
    /// The page does not build, in one of the ways of `BREAKS`.
    Break(usize),
}

fn requests_sent_across_changes_are_answered_whole_by_the_old_program_or_the_new() {
    if !runs_here() {
        return;
    }
    generate::check(
        9620,
        1,
        |g: &mut Gen| {
            let count = 2 + g.below(3);
            (0..count)
                .map(|_| if g.one_in(3) { Change::Break(g.below(BREAKS.len())) } else { Change::Text })
                .collect::<Vec<Change>>()
        },
        |changes| {
            let (dir, port) = served("watch_changes", "v0");
            let watch = Watch::start(&dir, &["--timings", "--watch", "serve", "--port", &port.to_string()], false);
            until_it_says(port, "v0");
            // The texts the server has been built to say, in the order they were.
            let mut built = vec!["v0".to_string()];
            let mut latest = 0;
            for (step, change) in changes.iter().enumerate() {
                let builds = watch.builds();
                let expected = match change {
                    Change::Text => {
                        let text = format!("v{}", step + 1);
                        std::fs::write(dir.join("main.smith"), page(&text)).unwrap();
                        built.push(text.clone());
                        Some(text)
                    }
                    Change::Break(which) => {
                        std::fs::write(dir.join("main.smith"), BREAKS[*which]).unwrap();
                        None
                    }
                };
                // Requests are sent from the save until the build has finished
                // and the page, if it builds, is the one served, and then for
                // a while after. None is refused, reset, or cut short, and each
                // says a text that was built, never an older one than the last
                // it said.
                let mut after = 0;
                let mut done = false;
                let end = Instant::now() + PATIENCE;
                while !done || after < 10 {
                    clock::progressed();
                    std::thread::sleep(Duration::from_millis(2));
                    assert!(Instant::now() < end, "the change {step} {change:?} never took");
                    let said = ask(port).unwrap_or_else(|why| panic!("a request after the change {step}: {why}"));
                    assert_eq!(said.status, 200, "{said:?}");
                    let at = built
                        .iter()
                        .position(|t| *t == said.body)
                        .unwrap_or_else(|| panic!("a text that was never built: {said:?} of {built:?}"));
                    assert!(at >= latest, "{said:?} is older than the page said before, {built:?}");
                    latest = at;
                    done = watch.builds() > builds && expected.as_ref().is_none_or(|t| *t == said.body);
                    if done {
                        after += 1;
                    }
                }
                if expected.is_none() {
                    // What the command printed is what `smith build serve` prints.
                    let printed = smith(&dir, &["build", "serve"]);
                    assert!(!printed.status.success());
                    let message = stderr(&printed);
                    assert!(!message.is_empty());
                    assert!(
                        watch.errors().contains(message.trim_end()),
                        "the errors of the build, {message:?}, are not in {:?}",
                        watch.errors()
                    );
                }
            }
            // The last page that built is the one that is served.
            assert_eq!(ask(port).unwrap().body, *built.last().unwrap());
        },
    );
}

/// A task that writes a line when it starts, reads a line, and writes another.
fn waiting(text: &str) -> String {
    format!(
        "use std.console (console, input, print)\n\npub fn main() <console, alloc> {{\n  print(\"begin {text}\")\n  _ = input()\n  print(\"end {text}\")\n}}\n"
    )
}

fn a_task_without_diverge_is_run_again_and_a_run_still_going_is_let_end() {
    if !runs_here() {
        return;
    }
    let dir = folder("watch_task");
    let project = "use main (main)\nuse std.console (console)\n\npub fn tick() <console, alloc> {\n  main()\n}\n";
    std::fs::write(dir.join("project.smith"), project).unwrap();
    std::fs::write(dir.join("main.smith"), waiting("v1")).unwrap();
    let mut watch = Watch::start(&dir, &["--timings", "--watch", "tick"], true);
    wait_for("the first run to begin", || watch.said().contains("begin v1"));
    // A change while the run is going is built, and the run is let end.
    std::fs::write(dir.join("main.smith"), waiting("v2")).unwrap();
    wait_for("the change to be built", || watch.builds() == 2);
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(watch.said(), "begin v1\n", "the run before was ended or another was started");
    watch.type_line();
    wait_for("the second run to begin", || watch.said().contains("begin v2"));
    assert_eq!(watch.said(), "begin v1\nend v1\nbegin v2\n");
    // Another change after it ended is run again.
    watch.type_line();
    wait_for("the second run to end", || watch.said().ends_with("end v2\n"));
    std::fs::write(dir.join("main.smith"), waiting("v3")).unwrap();
    wait_for("the third run to begin", || watch.said().ends_with("begin v3\n"));
    assert_eq!(watch.said(), "begin v1\nend v1\nbegin v2\nend v2\nbegin v3\n");
}

fn check_and_run_do_again_what_they_did_after_each_save() {
    if !runs_here() {
        return;
    }
    let dir = folder("watch_commands");
    let hello = |who: &str| format!("use std.console (console, print)\n\nfn main() <console> {{\n  print(\"hello {who}\")\n}}\n");
    std::fs::write(dir.join("hello.smith"), hello("one")).unwrap();
    let running = Watch::start(&dir, &["--watch", "run", "hello.smith"], false);
    wait_for("the first run", || running.said() == "hello one\n");
    std::fs::write(dir.join("hello.smith"), hello("two")).unwrap();
    wait_for("the second run", || running.said() == "hello one\nhello two\n");

    // The report of `check` is the same each time it is made, and says what is
    // wrong with the file as it is.
    let checked = folder("watch_check");
    let broken = |name: &str| format!("use std.console (console, print)\n\nfn main() <console> {{\n  {name}()\n}}\n");
    std::fs::write(checked.join("main.smith"), broken("first_nowhere")).unwrap();
    let watch = Watch::start(&checked, &["--watch", "check"], false);
    wait_for("the first report", || watch.errors().contains("first_nowhere"));
    std::fs::write(checked.join("main.smith"), broken("second_nowhere")).unwrap();
    wait_for("the second report", || watch.errors().contains("second_nowhere"));
    let direct = smith(&checked, &["check"]);
    assert!(watch.errors().ends_with(&stderr(&direct)), "{:?} does not end as {:?}", watch.errors(), stderr(&direct));
}

fn a_file_the_build_did_not_read_does_not_start_a_build() {
    if !runs_here() {
        return;
    }
    let (dir, port) = served("watch_unread", "one");
    std::fs::write(dir.join("unused.smith"), "pub fn unused() -> u64 {\n  1\n}\n").unwrap();
    std::fs::write(dir.join("notes.txt"), "a\n").unwrap();
    let watch = Watch::start(&dir, &["--timings", "--watch", "serve", "--port", &port.to_string()], false);
    until_it_says(port, "one");
    assert_eq!(watch.builds(), 1);
    std::fs::write(dir.join("unused.smith"), "pub fn unused() -> u64 {\n  2\n}\n").unwrap();
    std::fs::write(dir.join("notes.txt"), "b\n").unwrap();
    std::fs::write(dir.join("other.smith"), "pub fn other() -> u64 {\n  3\n}\n").unwrap();
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(watch.builds(), 1, "{}", watch.errors());
    assert_eq!(ask(port).unwrap().body, "one");
    // A file that was read does start one.
    std::fs::write(dir.join("project.smith"), format!("{SERVE}\n")).unwrap();
    wait_for("the build of a file that was read", || watch.builds() == 2);
}

fn interrupting_smith_watch_ends_the_run_and_leaves_no_process() {
    if !runs_here() {
        return;
    }
    let (dir, port) = served("watch_interrupt", "one");
    let mut watch = Watch::start(&dir, &["--watch", "serve", "--port", &port.to_string()], false);
    until_it_says(port, "one");
    // A second run is going, as after a change.
    std::fs::write(dir.join("main.smith"), page("two")).unwrap();
    until_it_says(port, "two");
    let status = watch.interrupt();
    assert!(!status.success(), "{status:?}");
    // Nothing holds the port: the run is gone, and so is the socket `smith
    // --watch` kept.
    assert!(TcpStream::connect(("127.0.0.1", port)).is_err(), "something still listens at the port");
}
