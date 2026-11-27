//! `std.fetch` over `net` asks a server on this machine with each method and gets
//! its status, headers, and body, natively on Linux, macOS, and Windows. The test
//! is the server: it takes the connections of the program that asks, and checks
//! the request on the wire, which has the type and the length of the payload, and
//! answers as it chooses, with a length, in chunks, and with a body that is too
//! long. A send that is cancelled closes its connection while the program goes on,
//! and one that waits suspends its task alone.
//! The program the tests start is `fetch_client.smith`.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use smith::target::Target;

use crate::Test;
use crate::cli::{folder, smith, stderr};
use crate::targets;

pub const MACHINE: &[Test] = &[
    (
        "a_module_makes_its_requests_through_the_import_of_its_host",
        a_module_makes_its_requests_through_the_import_of_its_host,
    ),
    (
        "a_client_gives_the_status_headers_and_body_of_a_server_on_this_machine_for_each_method",
        a_client_gives_the_status_headers_and_body_of_a_server_on_this_machine_for_each_method,
    ),
    (
        "a_send_that_waits_suspends_its_task_and_no_other",
        a_send_that_waits_suspends_its_task_and_no_other,
    ),
    (
        "a_send_that_is_cancelled_closes_its_connection_while_the_program_goes_on",
        a_send_that_is_cancelled_closes_its_connection_while_the_program_goes_on,
    ),
];

const PROGRAM: &str = include_str!("fetch_client.smith");

/// The targets that talk to the kernel: those of Linux, macOS, and Windows.
fn kernel() -> Vec<Target> {
    targets::runnable()
        .into_iter()
        .filter(|t| {
            let n = t.name();
            n.starts_with("linux") || n.starts_with("macos") || n.starts_with("windows")
        })
        .collect()
}

/// The program, built once for each kind of chip, by the compiler's executable.
fn program(target: Target) -> &'static [u8] {
    static X64: OnceLock<Vec<u8>> = OnceLock::new();
    static ARM64: OnceLock<Vec<u8>> = OnceLock::new();
    let cell = if target.name().ends_with("arm64") { &ARM64 } else { &X64 };
    cell.get_or_init(|| {
        if targets::fingerprinting() {
            targets::build(PROGRAM.as_bytes(), target).unwrap_or_else(|e| panic!("{e:?}"))
        } else {
            let dir = folder(&format!("fetch_client_{}", target.name()));
            std::fs::write(dir.join("client.smith"), PROGRAM).unwrap();
            let built = smith(&dir, &["build", "--target", target.name(), "client.smith"]);
            assert!(built.status.success(), "{}", stderr(&built));
            std::fs::read(dir.join(target.executable_name("client"))).unwrap()
        }
    })
}

/// Notes the program for each target a fingerprint is made for, and says whether
/// this machine runs it.
fn runs(target: Target) -> bool {
    targets::record(target, &[program(target)]);
    !targets::fingerprinting()
}

/// The program that asks, started with its arguments, and killed when the test
/// ends, however it ends.
struct Asker {
    child: Child,
    dir: std::path::PathBuf,
}

impl Drop for Asker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        std::fs::remove_dir_all(&self.dir).ok();
    }
}

fn start(target: Target, name: &str, args: &[String]) -> Asker {
    let dir = crate::scratch::path(&format!("fetch_native_{name}"));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("program");
    crate::executable::write(&path, program(target));
    let child = Command::new(&path)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    Asker { child, dir }
}

/// A server on a port the system chose, which no other server of the run holds.
fn listening() -> (TcpListener, u16) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    (listener, port)
}

/// A request the program made: its head as text, without the blank line, and its
/// body.
struct Asked {
    stream: TcpStream,
    head: String,
    body: Vec<u8>,
}

impl Asked {
    fn line(&self) -> &str {
        self.head.lines().next().unwrap()
    }

    /// The value of a header of the request, in small letters.
    fn header(&self, name: &str) -> Option<String> {
        self.head.lines().skip(1).find_map(|l| {
            let (n, v) = l.split_once(": ")?;
            (n.to_ascii_lowercase() == name).then(|| v.to_string())
        })
    }

    fn answer(&mut self, bytes: &[u8]) {
        self.stream.write_all(bytes).unwrap();
    }
}

/// Takes the next connection and reads the request of it, whole.
fn take(listener: &TcpListener) -> Asked {
    let (mut stream, _) = listener.accept().unwrap();
    stream.set_read_timeout(Some(Duration::from_secs(8))).unwrap();
    stream.set_write_timeout(Some(Duration::from_secs(8))).unwrap();
    let mut held = Vec::new();
    let end = loop {
        let mut byte = [0u8; 1];
        let n = stream.read(&mut byte).unwrap();
        assert!(n == 1, "the request ended inside its head: {:?}", String::from_utf8_lossy(&held));
        held.push(byte[0]);
        if held.ends_with(b"\r\n\r\n") {
            break held.len();
        }
    };
    let head = String::from_utf8(held[..end - 4].to_vec()).unwrap();
    let length = head
        .lines()
        .skip(1)
        .find_map(|l| {
            let (n, v) = l.split_once(": ")?;
            n.eq_ignore_ascii_case("content-length").then(|| v.parse::<usize>().unwrap())
        })
        .unwrap_or(0);
    let mut body = vec![0u8; length];
    stream.read_exact(&mut body).unwrap();
    Asked { stream, head, body }
}

fn response(status: &str, headers: &[&str], body: &[u8]) -> Vec<u8> {
    let mut out = format!("HTTP/1.1 {status}\r\n");
    for h in headers {
        out += &format!("{h}\r\n");
    }
    out += &format!("content-length: {}\r\n\r\n", body.len());
    let mut bytes = out.into_bytes();
    bytes.extend(body);
    bytes
}

/// A program of this machine asks a server of this machine with each method and
/// gets what the server says: the status whatever it is, the headers, and a body
/// framed by its length or in chunks. The request has the host, `connection:
/// close`, the headers the program gave, and for a body the type and the length of
/// its payload, and the payload. A body longer than the limit is `.too_big` and
/// one of exactly the limit is not, a head has no body whatever its length says,
/// a redirect is given as it is, and a server that is not there is `unreachable`.
fn a_client_gives_the_status_headers_and_body_of_a_server_on_this_machine_for_each_method() {
    for target in kernel() {
        if !runs(target) {
            continue;
        }
        let (listener, port) = listening();
        let (closed, closed_port) = listening();
        drop(closed);
        let mut asker = start(target, "basic", &["basic".into(), port.to_string(), closed_port.to_string()]);
        let host = format!("127.0.0.1:{port}");
        let next = |line: &str| {
            let asked = take(&listener);
            assert_eq!(asked.line(), line);
            assert_eq!(asked.header("host").as_deref(), Some(host.as_str()));
            assert_eq!(asked.header("connection").as_deref(), Some("close"));
            asked
        };
        let mut a = next("GET /a HTTP/1.1");
        assert!(a.header("content-type").is_none() && a.header("content-length").is_none());
        a.answer(&response("200 OK", &["x-id: 5"], b"hello"));
        let mut b = next("POST /b HTTP/1.1");
        assert_eq!(b.header("content-type").as_deref(), Some("application/json"));
        assert_eq!(b.header("content-length").as_deref(), Some(b.body.len().to_string().as_str()));
        assert_eq!(b.body, br#"{"id":7,"name":"Ana"}"#);
        b.answer(&response("201 Created", &[], b"ok"));
        let mut c = next("PUT /c HTTP/1.1");
        assert_eq!(c.header("content-type").as_deref(), Some("application/x-www-form-urlencoded"));
        assert_eq!(c.body, b"a+b=c%26d");
        c.answer(&response("200 OK", &[], b""));
        let mut d = next("PATCH /d HTTP/1.1");
        assert_eq!(d.header("x-id").as_deref(), Some("9"));
        assert_eq!(d.header("content-type").as_deref(), Some("text/plain; charset=utf-8"));
        assert_eq!(d.body, b"x");
        d.answer(&response("200 OK", &[], b""));
        let mut e = next("DELETE /e HTTP/1.1");
        e.answer(&response("204 No Content", &[], b""));
        let mut f = next("HEAD /f HTTP/1.1");
        // A head says how long the body would be and sends none.
        f.answer(b"HTTP/1.1 200 OK\r\ncontent-length: 100\r\n\r\n");
        let mut big = next("GET /big HTTP/1.1");
        big.answer(&response("200 OK", &[], b"hello"));
        let mut exact = next("GET /exact HTTP/1.1");
        exact.answer(&response("200 OK", &[], b"hello"));
        let mut chunked = next("GET /chunked HTTP/1.1");
        chunked.answer(b"HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\n\r\n2\r\nwi\r\n2\r\nki\r\n0\r\n\r\n");
        let mut moved = next("GET /moved HTTP/1.1");
        moved.answer(&response("302 Found", &["location: /elsewhere"], b""));
        let mut root = next("GET / HTTP/1.1");
        root.answer(&response("200 OK", &[], b""));
        let said = String::from_utf8(output_of(&mut asker.child)).unwrap();
        let wanted = format!(
            "get 200 hello 2\npost 201 ok\nput 200\npatch 200\ndelete 204\nhead 200 1\nbig http://127.0.0.1:{port}/big: body longer than 4 bytes\nexact 200 hello\nchunked 200 wiki\nmoved 302 2\ngone http://127.0.0.1:{closed_port}/gone: server not reached\nroot 200\n"
        );
        assert_eq!(said.replace("\r\n", "\n"), wanted, "on {}", target.name());
    }
}

/// What the program wrote to its standard output, once it has ended.
fn output_of(child: &mut Child) -> Vec<u8> {
    let mut out = Vec::new();
    child.stdout.take().unwrap().read_to_end(&mut out).unwrap();
    let status = child.wait().unwrap();
    assert!(status.success(), "the program ended with {status}");
    out
}

/// A send that waits for a server that says nothing is cancelled when its group
/// ends, and closes the connection it holds: the server sees the end of the file
/// while the program is still running.
fn a_send_that_is_cancelled_closes_its_connection_while_the_program_goes_on() {
    for target in kernel() {
        if !runs(target) || target.name().starts_with("windows") {
            continue;
        }
        let (listener, port) = listening();
        let mut asker = start(target, "cancelled", &["cancelled".into(), port.to_string()]);
        let mut asked = take(&listener);
        assert_eq!(asked.line(), "GET /slow HTTP/1.1");
        let end = Instant::now() + Duration::from_secs(2);
        asked.stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let ended = loop {
            match asked.stream.read(&mut [0u8; 1]) {
                Ok(0) => break true,
                Ok(_) => break false,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => break false,
            }
        };
        assert!(ended, "the connection was not closed within the time on {}", target.name());
        assert!(Instant::now() < end, "closed too late");
        assert!(asker.child.try_wait().unwrap().is_none(), "the program ended with the connection, so its group did not");
    }
}

/// Two sends in tasks of one group: the first server takes the request and says
/// nothing until the second has answered and the program has printed its answer.
/// A send that held up the whole program would never send the second request, so
/// the server would wait for ever, and the test fails by the time it takes.
fn a_send_that_waits_suspends_its_task_and_no_other() {
    for target in kernel() {
        if !runs(target) || target.name().starts_with("windows") {
            continue;
        }
        let (first, first_port) = listening();
        let (second, second_port) = listening();
        let mut asker = start(target, "pair", &["pair".into(), first_port.to_string(), second_port.to_string()]);
        let mut late = take(&first);
        assert_eq!(late.line(), "GET /late HTTP/1.1");
        let mut early = take(&second);
        assert_eq!(early.line(), "GET /early HTTP/1.1");
        early.answer(&response("200 OK", &[], b""));
        // The program prints the second answer while the first is not given.
        let mut said = Vec::new();
        let mut stdout = asker.child.stdout.take().unwrap();
        let end = Instant::now() + Duration::from_secs(8);
        while !said.ends_with(b"early 200\n") {
            let mut byte = [0u8; 1];
            assert!(Instant::now() < end, "the second answer was not printed: {:?}", String::from_utf8_lossy(&said));
            assert!(stdout.read(&mut byte).unwrap() == 1, "the program ended before it printed the second answer");
            said.push(byte[0]);
        }
        late.answer(&response("200 OK", &[], b""));
        stdout.read_to_end(&mut said).unwrap();
        assert_eq!(String::from_utf8_lossy(&said).replace("\r\n", "\n"), "early 200\nlate 200\n", "on {}", target.name());
        assert!(asker.child.wait().unwrap().success());
    }
}

/// The program a module is built from: a request, and what it gets or why not.
const MODULE: &str = r#"use std.console (console, print)
use std.fetch (get, host)
use std.utf8 (utf8)
use std.list (freeze)

fn main() <console, alloc> {
  with host()
  match get[8]("http://127.0.0.1:PORT/a", headers: []) {
    .ok(r) {
      text = match utf8(freeze(r.body.bytes)) {
        .ok(t) { t }
        .err(e) { "not text" }
      }
      print("a {r.status} {text} {r.headers.size}")
    }
    .err(e) { print("a {e}") }
  }
  match get[3]("http://127.0.0.1:PORT/b") {
    .ok(r) { print("b {r.status}") }
    .err(e) { print("b {e}") }
  }
  match get[3]("http://127.0.0.1:CLOSED/c") {
    .ok(r) { print("c {r.status}") }
    .err(e) { print("c {e}") }
  }
  match get("https://127.0.0.1/d") {
    .ok(r) { print("d {r.status}") }
    .err(e) { print("d {e}") }
  }
}
"#;

/// A module that is built to ask its host to make a request, which `node` makes
/// with a child of its own, gets the status, headers and body of a server on this
/// machine, `.too_big` for a body over its limit, `unreachable` for a port nobody
/// holds, and refuses `https` as the native program does.
fn a_module_makes_its_requests_through_the_import_of_its_host() {
    let found = crate::clock::output_within(Command::new("node").arg("--version"), false)
        .is_ok_and(|o| o.status.success());
    if !found {
        eprintln!("skipped: node is not installed");
        return;
    }
    for target in smith::target::Target::ALL.iter().copied().filter(|t| !t.is_machine_code()) {
        let (listener, port) = listening();
        let (closed, closed_port) = listening();
        drop(closed);
        let source = MODULE.replace("CLOSED", &closed_port.to_string()).replace("PORT", &port.to_string());
        let dir = folder(&format!("fetch_module_{}", target.name()));
        std::fs::write(dir.join("m.smith"), &source).unwrap();
        let built = smith(&dir, &["build", "--target", target.name(), "m.smith"]);
        assert!(built.status.success(), "{}", stderr(&built));
        let mut node = Command::new("node")
            .arg(dir.join("m.js"))
            .current_dir(&dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut a = take(&listener);
        assert_eq!(a.line(), "GET /a HTTP/1.1");
        assert_eq!(a.header("host").as_deref(), Some(format!("127.0.0.1:{port}").as_str()));
        a.answer(&response("200 OK", &["x-id: 5"], b"hello"));
        // The request says `connection: close`, so the server closes, and the host
        // reads to the end.
        drop(a);
        let mut b = take(&listener);
        assert_eq!(b.line(), "GET /b HTTP/1.1");
        b.answer(&response("200 OK", &[], b"hello"));
        drop(b);
        let said = String::from_utf8(output_of(&mut node)).unwrap();
        let wanted = format!(
            "a 200 hello 2\nb http://127.0.0.1:{port}/b: body longer than 3 bytes\nc http://127.0.0.1:{closed_port}/c: server not reached\nd https://127.0.0.1/d: https is not built yet\n"
        );
        assert_eq!(said.replace("\r\n", "\n"), wanted, "on {}", target.name());
        std::fs::remove_dir_all(&dir).ok();
    }
}
