//! `listen` of `std.http` answers `http` from a socket, one connection at a
//! time, as `http.md` says. The program the tests start is `http_listen.smith`,
//! which is `serve` of the page's deployment at the port it is started with: a
//! test starts it, connects as a client does, and reads the responses off the
//! connection. What each response must be comes from the same routes run under
//! a handler of `http` that a test writes, with no network.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::time::Duration;

use smith::diag::Error;
use smith::scalar;
use smith::target::Target;
use smith::lower;

use crate::Test;
use crate::cli::{folder, smith, stderr};
use crate::generate::{self, Gen};
use crate::ports;
use crate::targets;

pub const ONCE: &[Test] = &[
    (
        "the_routes_under_a_handler_of_http_answer_as_the_page_says",
        the_routes_under_a_handler_of_http_answer_as_the_page_says,
    ),
    (
        "a_target_that_has_no_network_refuses_a_server_naming_itself",
        a_target_that_has_no_network_refuses_a_server_naming_itself,
    ),
];

pub const MACHINE: &[Test] = &[
    (
        "a_server_whose_first_port_is_held_by_another_socket_is_started_again_on_another_and_serves",
        a_server_whose_first_port_is_held_by_another_socket_is_started_again_on_another_and_serves,
    ),
    (
        "a_server_answers_json_a_404_and_a_400_each_with_its_length_and_a_second_request_on_the_connection",
        a_server_answers_json_a_404_and_a_400_each_with_its_length_and_a_second_request_on_the_connection,
    ),
    (
        "a_request_the_reader_refuses_is_answered_400_and_the_connection_closed_and_the_next_is_served",
        a_request_the_reader_refuses_is_answered_400_and_the_connection_closed_and_the_next_is_served,
    ),
    (
        "a_body_in_chunks_or_in_pieces_is_read_whole_and_the_response_is_written_with_its_length",
        a_body_in_chunks_or_in_pieces_is_read_whole_and_the_response_is_written_with_its_length,
    ),
    (
        "a_server_answers_the_requests_as_the_routes_do_under_a_handler",
        a_server_answers_the_requests_as_the_routes_do_under_a_handler,
    ),
    (
        "a_second_server_on_a_port_in_use_ends_with_a_failing_status_and_the_error_on_standard_error",
        a_second_server_on_a_port_in_use_ends_with_a_failing_status_and_the_error_on_standard_error,
    ),
    (
        "a_connection_that_ends_before_a_request_does_not_stop_the_server",
        a_connection_that_ends_before_a_request_does_not_stop_the_server,
    ),
    (
        "a_request_sent_with_the_next_is_answered_and_the_connection_is_then_closed",
        a_request_sent_with_the_next_is_answered_and_the_connection_is_then_closed,
    ),
    (
        "serving_requests_does_not_grow_what_the_system_is_asked_for",
        serving_requests_does_not_grow_what_the_system_is_asked_for,
    ),
    (
        "a_thousand_connections_open_at_once_are_each_answered",
        a_thousand_connections_open_at_once_are_each_answered,
    ),
    (
        "the_deployment_of_the_page_built_by_its_name_answers_on_the_port_it_is_given",
        the_deployment_of_the_page_built_by_its_name_answers_on_the_port_it_is_given,
    ),
];

const PROGRAM: &str = include_str!("http_listen.smith");

/// The routes of the program, which are everything before the `main` that
/// serves them.
fn routes() -> &'static str {
    PROGRAM.split("// What `serve` of `project.smith` does").next().unwrap()
}

/// The targets that build the program: those that talk to the kernel.
fn kernel() -> Vec<Target> {
    targets::runnable()
        .into_iter()
        .filter(|t| t.name().starts_with("linux") || t.name().starts_with("macos"))
        .collect()
}

/// The program, built once for `target`.
fn program(target: Target) -> &'static [u8] {
    static X64: OnceLock<Vec<u8>> = OnceLock::new();
    static ARM64: OnceLock<Vec<u8>> = OnceLock::new();
    let cell = if target.name().ends_with("arm64") { &ARM64 } else { &X64 };
    cell.get_or_init(|| {
        targets::build(PROGRAM.as_bytes(), target).unwrap_or_else(|e| panic!("{e:?}"))
    })
}

/// Notes the program for each target a fingerprint is made for, and says
/// whether this machine runs it.
fn runs(target: Target) -> bool {
    targets::record(target, &[program(target)]);
    !targets::fingerprinting()
}

/// A program that has been started, which is killed when the test ends,
/// however it ends.
struct Started {
    child: Child,
    port: u16,
}

impl Drop for Started {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Started {
    /// Starts the server for `target` on a port of `ports.rs`, and waits until
    /// it takes a connection.
    fn new(target: Target) -> Started {
        Started::on(target, ports::all())
    }

    /// Starts the server for `target` on the first of `candidates` it can bind,
    /// as `ports.rs` says.
    fn on(target: Target, candidates: impl IntoIterator<Item = u16>) -> Started {
        let dir = crate::scratch::path("http_listen");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("program");
        crate::executable::write(&path, program(target));
        let server = ports::serve(candidates, |port| {
            Started::at(Command::new(&path).arg(port.to_string()), port)
        });
        std::fs::remove_dir_all(&dir).ok();
        server
    }

    /// Starts `command`, which serves at `port`, and says how it came out.
    fn at(command: &mut Command, port: u16) -> ports::Start<Started> {
        let child = command.stdout(Stdio::null()).stderr(Stdio::piped()).spawn().unwrap();
        let mut server = Started { child, port };
        match server.wait_for_it() {
            Ok(()) => ports::Start::Up(server),
            Err((_, said)) if said.contains("in use") => ports::Start::Taken,
            Err((status, said)) => ports::Start::Failed(format!(
                "the server ended with {status} before it listened, and wrote {said:?} to standard error"
            )),
        }
    }

    /// Waits until the program answers a request, and gives how it ended and
    /// what it wrote to standard error if it ends first. The program says
    /// nothing when it is listening, and a connection is taken by the system
    /// for whatever holds the port, so only an answer is the program's own.
    fn wait_for_it(&mut self) -> Result<(), (std::process::ExitStatus, String)> {
        let end = std::time::Instant::now() + Duration::from_secs(8);
        let mut asked: Option<TcpStream> = None;
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return Err((status, ports::said(&mut self.child)));
            }
            assert!(std::time::Instant::now() < end, "the server never listened");
            match &mut asked {
                None => {
                    if let Ok(mut stream) = TcpStream::connect(("127.0.0.1", self.port)) {
                        stream.set_read_timeout(Some(Duration::from_millis(20))).unwrap();
                        stream.write_all(get("/users/1").as_bytes()).unwrap();
                        asked = Some(stream);
                    }
                }
                Some(stream) => match stream.read(&mut [0u8; 1]) {
                    Ok(0) => asked = None,
                    Ok(_) => return Ok(()),
                    Err(_) => {}
                },
            }
            std::thread::yield_now();
        }
    }

    fn connect(&self) -> Client {
        let stream = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(8))).unwrap();
        Client { stream: BufReader::new(stream) }
    }
}

/// What a response says.
#[derive(Debug, PartialEq)]
struct Reply {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

struct Client {
    stream: BufReader<TcpStream>,
}

impl Client {
    fn send(&mut self, bytes: &[u8]) {
        self.stream.get_mut().write_all(bytes).unwrap();
    }

    /// The next response, whose body is as long as its `content-length` says,
    /// which every response must have, and none else.
    fn reply(&mut self) -> Reply {
        let mut line = String::new();
        self.stream.read_line(&mut line).unwrap();
        assert!(line.starts_with("HTTP/1.1 "), "the status line: {line:?}");
        let status: u16 = line[9..12].parse().unwrap();
        let mut headers = Vec::new();
        loop {
            let mut line = String::new();
            self.stream.read_line(&mut line).unwrap();
            let line = line.strip_suffix("\r\n").unwrap_or_else(|| panic!("a line of a head ends with a line break: {line:?}"));
            if line.is_empty() {
                break;
            }
            let (name, value) = line.split_once(": ").unwrap();
            headers.push((name.to_string(), value.to_string()));
        }
        let lengths: Vec<&(String, String)> = headers.iter().filter(|(n, _)| n == "content-length").collect();
        assert_eq!(lengths.len(), 1, "one content-length: {headers:?}");
        let length: usize = lengths[0].1.parse().unwrap();
        let mut body = vec![0u8; length];
        self.stream.read_exact(&mut body).unwrap();
        Reply { status, headers, body: String::from_utf8(body).unwrap() }
    }

    /// Whether the server has closed the connection: a read gives no bytes.
    fn closed(&mut self) -> bool {
        let mut byte = [0u8; 1];
        matches!(self.stream.read(&mut byte), Ok(0))
    }
}

fn get(path: &str) -> String {
    format!("GET {path} HTTP/1.1\r\nhost: localhost\r\n\r\n")
}

fn post(path: &str, body: &str) -> String {
    format!("POST {path} HTTP/1.1\r\nhost: localhost\r\ncontent-length: {}\r\n\r\n{body}", body.len())
}

fn a_server_answers_json_a_404_and_a_400_each_with_its_length_and_a_second_request_on_the_connection() {
    for target in kernel() {
        if !runs(target) {
            continue;
        }
        let server = Started::new(target);
        let mut client = server.connect();
        client.send(get("/users/7").as_bytes());
        let found = client.reply();
        assert_eq!(found.status, 200, "{}", target.name());
        assert_eq!(found.body, "{\"id\":7,\"name\":\"Ana\"}");
        assert!(found.headers.contains(&("content-type".into(), "application/json".into())));
        // The same connection carries the next requests.
        client.send(get("/nowhere").as_bytes());
        let missing = client.reply();
        assert_eq!((missing.status, missing.body.as_str()), (404, ""), "{}", target.name());
        client.send(post("/users", "{\"nope\":1}").as_bytes());
        let refused = client.reply();
        assert_eq!(refused.status, 400, "{}", target.name());
        assert!(refused.body.contains("name"), "{}", refused.body);
        client.send(get("/users/8").as_bytes());
        assert_eq!(client.reply().body, "{\"id\":8,\"name\":\"Ana\"}");
    }
}

/// A request that `std.http1` refuses, each by a rule of RFC 9112 that the
/// page names.
fn refused_requests() -> Vec<(&'static str, String)> {
    let head = |lines: &str| format!("{lines}\r\n\r\n");
    vec![
        ("not a request", head("garbage")),
        ("a line that ends with a line feed alone", "GET / HTTP/1.1\nhost: a\n\n".to_string()),
        ("a space before the colon of a header", head("GET / HTTP/1.1\r\nhost: a\r\nx : 1")),
        ("two content-length headers", head("POST / HTTP/1.1\r\nhost: a\r\ncontent-length: 1\r\ncontent-length: 1")),
        ("content-length with transfer-encoding", head("POST / HTTP/1.1\r\nhost: a\r\ncontent-length: 1\r\ntransfer-encoding: chunked")),
        ("no host in HTTP/1.1", head("GET / HTTP/1.1")),
        ("another version", head("GET / HTTP/2.0\r\nhost: a")),
        ("a content-length that is no number", head("POST / HTTP/1.1\r\nhost: a\r\ncontent-length: x")),
    ]
}

fn a_request_the_reader_refuses_is_answered_400_and_the_connection_closed_and_the_next_is_served() {
    for target in kernel() {
        if !runs(target) {
            continue;
        }
        let server = Started::new(target);
        for (what, bytes) in refused_requests() {
            let mut client = server.connect();
            client.send(bytes.as_bytes());
            let reply = client.reply();
            assert_eq!(reply.status, 400, "{what}, {}", target.name());
            assert!(client.closed(), "{what} is answered and the connection closed, {}", target.name());
            // The next connection is served.
            let mut next = server.connect();
            next.send(get("/users/1").as_bytes());
            assert_eq!(next.reply().status, 200, "after {what}, {}", target.name());
        }
    }
}

/// A body of the name that a person has, and how it is cut to be sent: the
/// size of each piece, which may be larger than the body.
fn bodies(g: &mut Gen) -> (usize, Vec<usize>) {
    let sizes = [0, 1, 2, 5, 100, 4095, 4096, 4097, 70000];
    let length = *g.pick(&sizes);
    let pieces = (0..4).map(|_| 1 + g.below(5000)).collect();
    (length, pieces)
}

fn a_body_in_chunks_or_in_pieces_is_read_whole_and_the_response_is_written_with_its_length() {
    for target in kernel() {
        if !runs(target) {
            continue;
        }
        let server = Started::new(target);
        generate::check_cases(
            31,
            0,
            1,
            bodies,
            |(length, pieces)| {
                let mut client = server.connect();
                let name = "a".repeat(*length);
                let json = format!("{{\"name\":\"{name}\"}}");
                let expected = format!("{{\"id\":{length},\"name\":\"{name}\"}}");
                // With its length, sent in pieces.
                let head = format!("POST /users HTTP/1.1\r\nhost: a\r\ncontent-length: {}\r\n\r\n", json.len());
                let mut bytes = head.into_bytes();
                bytes.extend_from_slice(json.as_bytes());
                let mut at = 0;
                for piece in pieces.iter().cycle() {
                    if at >= bytes.len() {
                        break;
                    }
                    let end = (at + piece).min(bytes.len());
                    client.send(&bytes[at..end]);
                    at = end;
                }
                let reply = client.reply();
                assert_eq!((reply.status, reply.body.as_str()), (200, expected.as_str()), "{}", target.name());
                // In chunks of those sizes.
                let mut chunked = b"POST /users HTTP/1.1\r\nhost: a\r\ntransfer-encoding: chunked\r\n\r\n".to_vec();
                let data = json.as_bytes();
                let mut at = 0;
                for piece in pieces.iter().cycle() {
                    if at >= data.len() {
                        break;
                    }
                    let end = (at + piece).min(data.len());
                    chunked.extend_from_slice(format!("{:x}\r\n", end - at).as_bytes());
                    chunked.extend_from_slice(&data[at..end]);
                    chunked.extend_from_slice(b"\r\n");
                    at = end;
                }
                chunked.extend_from_slice(b"0\r\n\r\n");
                client.send(&chunked);
                let reply = client.reply();
                assert_eq!((reply.status, reply.body.as_str()), (200, expected.as_str()), "{}", target.name());
            },
        );
    }
}

/// The requests the server and the handler both answer, as the method, the
/// path, and the body.
const REQUESTS: &[(&str, &str, &str)] = &[
    ("GET", "/users/7", ""),
    ("GET", "/users/x", ""),
    ("GET", "/nowhere", ""),
    ("POST", "/users", "{\"name\":\"Bo\"}"),
    ("POST", "/users", "{\"nope\":1}"),
    ("POST", "/users", "not json"),
    ("DELETE", "/users/7", ""),
];

/// The status and the body of each of the requests, as the routes answer them
/// under a handler of `http` that gives the request and prints the response,
/// with no network.
fn under_a_handler() -> Vec<String> {
    let quoted = |s: &str| {
        s.replace('\\', "\\\\").replace('"', "\\\"").replace('{', "\\{").replace('}', "\\}")
    };
    let mut main = String::from(
        "fn copy_of(r: Request) -> Request ! alloc {\n  Request(method: r.method, path: r.path, query: r.query, headers: [], body: r.body)\n}\n\n\
         fn once[T](asked: Request, sink: fn(Response), body: fn() -> T ! http) -> T ! alloc {\n  http(request: { copy_of(asked) }, send: fn(r) { sink(r) }, body: body)\n}\n\n\
         fn ask(method: str, path: str, body: str) ! console, alloc {\n  with once(Request(method: method, path: path, query: \"\", headers: [], body: body), fn(r) { print(\"{r.status} {r.body}\") })\n  app()\n}\n\n\
         fn main() ! console, alloc {\n",
    );
    for (method, path, body) in REQUESTS {
        main.push_str(&format!("  ask(\"{method}\", \"{path}\", \"{}\")\n", quoted(body)));
    }
    main.push_str("}\n");
    let routes = routes()
        .replace("use std.http (get, http, listen, post, routes)", "use std.console (console, print)\nuse std.http (Request, Response, get, http, post, routes)")
        .replace("use std.net (net)\n", "")
        .replace("use std.deserialize (ReadError)\nuse std.fromtext (from_text)\n", "");
    let source = format!("{routes}\n{main}");
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?}\n{source}"));
    let outcome = scalar::run(&program);
    String::from_utf8(outcome.stdout).unwrap().lines().map(str::to_string).collect()
}

fn the_routes_under_a_handler_of_http_answer_as_the_page_says() {
    // What the handler gives is what the routes say, for every request.
    let said = under_a_handler();
    assert_eq!(said.len(), REQUESTS.len(), "{said:?}");
    assert_eq!(said[0], "200 {\"id\":7,\"name\":\"Ana\"}");
    assert_eq!(said[2], "404 ");
    assert!(said[1].starts_with("400 "), "{}", said[1]);
}

fn a_server_answers_the_requests_as_the_routes_do_under_a_handler() {
    let wanted = under_a_handler();
    for target in kernel() {
        if !runs(target) {
            continue;
        }
        let server = Started::new(target);
        let mut client = server.connect();
        for ((method, path, body), wanted) in REQUESTS.iter().zip(&wanted) {
            let request = if body.is_empty() {
                format!("{method} {path} HTTP/1.1\r\nhost: a\r\n\r\n")
            } else {
                format!("{method} {path} HTTP/1.1\r\nhost: a\r\ncontent-length: {}\r\n\r\n{body}", body.len())
            };
            client.send(request.as_bytes());
            let reply = client.reply();
            assert_eq!(&format!("{} {}", reply.status, reply.body), wanted, "{method} {path}, {}", target.name());
        }
    }
}

fn a_second_server_on_a_port_in_use_ends_with_a_failing_status_and_the_error_on_standard_error() {
    for target in kernel() {
        if !runs(target) {
            continue;
        }
        let first = Started::new(target);
        let dir = crate::scratch::path("http_listen_second");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("program");
        crate::executable::write(&path, program(target));
        let output = crate::clock::output_within(
            Command::new(&path).arg(first.port.to_string()),
            true,
        )
        .unwrap();
        std::fs::remove_dir_all(&dir).ok();
        assert!(!output.status.success(), "{}", target.name());
        let said = String::from_utf8_lossy(&output.stderr);
        assert!(said.contains("in use"), "{said:?}, {}", target.name());
        // The first is still serving.
        let mut client = first.connect();
        client.send(get("/users/1").as_bytes());
        assert_eq!(client.reply().status, 200);
    }
}

fn a_server_whose_first_port_is_held_by_another_socket_is_started_again_on_another_and_serves() {
    for target in kernel() {
        if !runs(target) {
            continue;
        }
        // The server listens on every address, as macOS lets it beside a socket
        // that holds one address only, so the socket holds them all too.
        let held = TcpListener::bind("[::]:0").or_else(|_| TcpListener::bind("0.0.0.0:0")).unwrap();
        let taken = held.local_addr().unwrap().port();
        let server = Started::on(target, std::iter::once(taken).chain(ports::all()));
        assert_ne!(server.port, taken, "{}", target.name());
        let mut client = server.connect();
        client.send(get("/users/1").as_bytes());
        assert_eq!(client.reply().status, 200, "{}", target.name());
    }
}

fn a_connection_that_ends_before_a_request_does_not_stop_the_server() {
    for target in kernel() {
        if !runs(target) {
            continue;
        }
        let server = Started::new(target);
        // Nothing is sent, and a head that is cut off is sent.
        drop(TcpStream::connect(("127.0.0.1", server.port)).unwrap());
        let mut cut = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
        cut.write_all(b"GET /users/1 HTTP/1.1\r\nho").unwrap();
        cut.shutdown(Shutdown::Write).unwrap();
        let mut rest = Vec::new();
        cut.set_read_timeout(Some(Duration::from_secs(8))).unwrap();
        let _ = cut.read_to_end(&mut rest);
        let mut client = server.connect();
        client.send(get("/users/2").as_bytes());
        assert_eq!(client.reply().body, "{\"id\":2,\"name\":\"Ana\"}", "{}", target.name());
    }
}

fn a_request_sent_with_the_next_is_answered_and_the_connection_is_then_closed() {
    for target in kernel() {
        if !runs(target) {
            continue;
        }
        let server = Started::new(target);
        let mut client = server.connect();
        client.send(format!("{}{}", get("/users/1"), get("/users/2")).as_bytes());
        let reply = client.reply();
        assert_eq!((reply.status, reply.body.as_str()), (200, "{\"id\":1,\"name\":\"Ana\"}"), "{}", target.name());
        assert!(reply.headers.contains(&("connection".into(), "close".into())), "{:?}", reply.headers);
        assert!(client.closed(), "{}", target.name());
        // The next connection is served.
        let mut next = server.connect();
        next.send(get("/users/3").as_bytes());
        assert_eq!(next.reply().status, 200);
    }
}

/// How much address space the program has, which grows when it asks the system
/// for memory that it does not give back. Linux says it; macOS does not here.
fn address_space(pid: u32) -> Option<u64> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let line = status.lines().find(|l| l.starts_with("VmSize:"))?;
    line.split_whitespace().nth(1)?.parse().ok()
}

fn serving_requests_does_not_grow_what_the_system_is_asked_for() {
    for target in kernel() {
        if !runs(target) || !target.name().starts_with("linux") {
            continue;
        }
        let server = Started::new(target);
        let mut client = server.connect();
        let mut ask = |count: usize| {
            for _ in 0..count {
                client.send(get("/users/7").as_bytes());
                client.reply();
            }
        };
        // The first requests may ask for memory that the rest then use.
        ask(50);
        let before = address_space(server.child.id()).unwrap();
        ask(2000);
        let after = address_space(server.child.id()).unwrap();
        assert!(after <= before + 1024, "{target:?}: {before} KB to {after} KB", target = target.name());
    }
}

/// A wave of connections that are all open at once, each asked for a user and
/// each answered, and then closed, as 1,000 clients of a server are.
fn a_wave(server: &Started, count: usize) {
    let mut clients: Vec<Client> = (0..count).map(|_| server.connect()).collect();
    for (i, client) in clients.iter_mut().enumerate() {
        client.send(get(&format!("/users/{}", 1 + i % 7)).as_bytes());
    }
    for client in clients.iter_mut() {
        assert_eq!(client.reply().status, 200);
    }
}

fn a_thousand_connections_open_at_once_are_each_answered() {
    for target in kernel() {
        if !runs(target) || !target.name().starts_with("linux") {
            continue;
        }
        let server = Started::new(target);
        a_wave(&server, 1000);
        // The server still answers on a connection of its own after the wave.
        let mut client = server.connect();
        client.send(get("/users/7").as_bytes());
        assert_eq!(client.reply().status, 200);
    }
}

/// The deployment of `deployments.md` in a project of its own: the routes are
/// `main`, and `serve` installs `listen` and calls it. It is built by its name
/// for the machine the test runs on, and started with the port as its flag.
fn the_deployment_of_the_page_built_by_its_name_answers_on_the_port_it_is_given() {
    let Some(target) = Target::host() else { return };
    if !kernel().contains(&target) || targets::fingerprinting() {
        return;
    }
    let dir = folder("http_listen_project");
    let main = routes()
        .replace("use std.http (get, http, listen, post, routes)", "use std.http (get, http, post, routes)")
        .replace("use std.net (net)\n", "")
        .replace("use std.deserialize (ReadError)\nuse std.fromtext (from_text)\n", "")
        .replace("fn app()", "pub fn main()");
    let project = "use main (main)\nuse std.http (listen)\nuse std.net (net)\nuse std.project (Package)\n\npackage = Package(name: \"users\")\n\npub fn serve(port: Nat = 8080) ! net, alloc, diverge {\n  with listen(port: port)\n  main()\n}\n";
    std::fs::write(dir.join("main.smith"), main).unwrap();
    std::fs::write(dir.join("project.smith"), project).unwrap();
    let built = smith(&dir, &["build", "serve"]);
    assert!(built.status.success(), "{}", stderr(&built));
    let executable: PathBuf = dir.join("users");
    let server = ports::serve(ports::all(), |port| {
        Started::at(Command::new(&executable).args(["--port", &port.to_string()]), port)
    });
    let mut client = server.connect();
    client.send(get("/users/7").as_bytes());
    let reply = client.reply();
    assert_eq!((reply.status, reply.body.as_str()), (200, "{\"id\":7,\"name\":\"Ana\"}"));
}

/// A target that has no network refuses a program that carries `net`, as
/// `net.md` says, and a server carries it.
fn a_target_that_has_no_network_refuses_a_server_naming_itself() {
    let program = lower(PROGRAM.as_bytes()).unwrap_or_else(|e: Vec<Error>| panic!("{e:?}"));
    assert!(program.asks_the_kernel());
    for &target in Target::ALL.iter() {
        if target.name().starts_with("linux") || target.name().starts_with("macos") {
            continue;
        }
        let refused = std::panic::catch_unwind(|| target.emit(&program));
        let message = match refused {
            Ok(_) => panic!("{} built a server", target.name()),
            Err(why) => why.downcast_ref::<String>().cloned().unwrap_or_default(),
        };
        assert!(message.contains(target.name()), "{message}");
    }
}
