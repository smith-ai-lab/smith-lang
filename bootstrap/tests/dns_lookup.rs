//! `connect` with a host name on Linux looks the name up itself: `/etc/hosts`,
//! and then the nameservers of `/etc/resolv.conf` over UDP, as the page
//! "Looking up a name" of `net.md` says. The program the machine tests run is
//! `dns_program.smith`, which gives `resolve` the text of the two files and the
//! port of a nameserver, and prints the addresses it finds. The test is the
//! nameserver: it starts the program, answers what it is asked on a port of
//! 127.0.0.1 from the main thread, as a test starts no thread, and reads what
//! the program printed when it has ended, with the names it was asked in order.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, UdpSocket};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use smith::target::Target;

use crate::Test;
use crate::targets;

pub const MACHINE: &[Test] = &[
    (
        "a_name_in_the_hosts_file_is_found_with_no_question_asked_and_with_no_regard_for_case",
        a_name_in_the_hosts_file_is_found_with_no_question_asked_and_with_no_regard_for_case,
    ),
    (
        "a_name_answered_for_both_kinds_gives_the_addresses_of_the_first_and_then_of_the_second_whatever_came_first",
        a_name_answered_for_both_kinds_gives_the_addresses_of_the_first_and_then_of_the_second_whatever_came_first,
    ),
    (
        "a_cname_is_followed_to_its_address_and_an_address_of_another_owner_is_not_taken",
        a_cname_is_followed_to_its_address_and_an_address_of_another_owner_is_not_taken,
    ),
    (
        "an_answer_with_the_wrong_id_or_the_wrong_question_or_from_the_wrong_address_is_ignored",
        an_answer_with_the_wrong_id_or_the_wrong_question_or_from_the_wrong_address_is_ignored,
    ),
    (
        "an_answer_that_was_cut_short_is_asked_again_over_tcp",
        an_answer_that_was_cut_short_is_asked_again_over_tcp,
    ),
    (
        "a_nameserver_that_does_not_answer_is_asked_again_and_the_lookup_fails_when_the_time_is_gone",
        a_nameserver_that_does_not_answer_is_asked_again_and_the_lookup_fails_when_the_time_is_gone,
    ),
    (
        "a_name_that_does_not_exist_is_unknown_and_a_failing_nameserver_fails_the_lookup",
        a_name_that_does_not_exist_is_unknown_and_a_failing_nameserver_fails_the_lookup,
    ),
    (
        "a_lookup_in_a_task_lets_the_other_tasks_run_while_it_waits_and_one_with_no_group_blocks_as_it_did",
        a_lookup_in_a_task_lets_the_other_tasks_run_while_it_waits_and_one_with_no_group_blocks_as_it_did,
    ),
    (
        "the_names_of_the_search_list_are_asked_in_the_order_the_dots_say",
        the_names_of_the_search_list_are_asked_in_the_order_the_dots_say,
    ),
];

/// The targets whose lookup is in Smith: those that talk to the kernel of Linux.
fn linux() -> Vec<Target> {
    targets::runnable().into_iter().filter(|t| t.name().starts_with("linux")).collect()
}

/// The program, built once for `target`.
fn program(target: Target) -> &'static [u8] {
    static X64: OnceLock<Vec<u8>> = OnceLock::new();
    static ARM64: OnceLock<Vec<u8>> = OnceLock::new();
    let cell = if target.name().ends_with("arm64") { &ARM64 } else { &X64 };
    cell.get_or_init(|| {
        let source = include_str!("dns_program.smith");
        targets::build(source.as_bytes(), target).unwrap_or_else(|e| panic!("{e:?}"))
    })
}

/// A question that was asked: its message, its name in lower case, and its kind.
pub struct Question {
    pub message: Vec<u8>,
    pub name: String,
    pub kind: u16,
}

impl Question {
    /// The question for the name, as a client sends it with the id `0x1234`.
    pub fn asking(name: &str, kind: u16) -> Question {
        let mut message = vec![0x12, 0x34, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0];
        message.extend(labels(name));
        message.extend_from_slice(&kind.to_be_bytes());
        message.extend_from_slice(&[0, 1]);
        Question { message, name: name.to_string(), kind }
    }

    fn read(message: &[u8]) -> Question {
        let mut at = 12;
        let mut labels = Vec::new();
        while message[at] != 0 {
            let n = message[at] as usize;
            labels.push(String::from_utf8_lossy(&message[at + 1..at + 1 + n]).to_lowercase());
            at += n + 1;
        }
        let kind = u16::from_be_bytes([message[at + 1], message[at + 2]]);
        Question { message: message.to_vec(), name: labels.join("."), kind }
    }
}

/// What a record of an answer holds.
pub enum Data {
    A([u8; 4]),
    Aaaa([u8; 16]),
    Cname(&'static str),
}

/// A record, and the name that owns it: the name that was asked, or another.
pub struct Record {
    pub owner: Option<&'static str>,
    pub data: Data,
}

pub fn labels(name: &str) -> Vec<u8> {
    let mut made = Vec::new();
    for label in name.split('.') {
        made.push(label.len() as u8);
        made.extend_from_slice(label.as_bytes());
    }
    made.push(0);
    made
}

/// The answer to the question, with the code, the bit that says it was cut
/// short, and the records.
pub fn answer(q: &Question, code: u8, cut: bool, records: &[Record]) -> Vec<u8> {
    let mut made = q.message[..2].to_vec();
    made.push(0x81 | if cut { 2 } else { 0 });
    made.push(0x80 | code);
    made.extend_from_slice(&[0, 1]);
    made.extend_from_slice(&(records.len() as u16).to_be_bytes());
    made.extend_from_slice(&[0, 0, 0, 0]);
    made.extend_from_slice(&q.message[12..]);
    for r in records {
        match r.owner {
            None => made.extend_from_slice(&[0xc0, 12]),
            Some(name) => made.extend(labels(name)),
        }
        let (kind, data) = match &r.data {
            Data::A(a) => (1u16, a.to_vec()),
            Data::Aaaa(a) => (28, a.to_vec()),
            Data::Cname(name) => (5, labels(name)),
        };
        made.extend_from_slice(&kind.to_be_bytes());
        made.extend_from_slice(&[0, 1, 0, 0, 0, 60]);
        made.extend_from_slice(&(data.len() as u16).to_be_bytes());
        made.extend(data);
    }
    made
}

/// What a nameserver has to answer with and over what: the sockets the test
/// owns, and the questions that have come.
struct Server {
    udp: UdpSocket,
    other: UdpSocket,
    tcp: TcpListener,
    asked: Vec<Question>,
}

/// What a run of the program gave: the lines it printed, the names it asked for
/// in order of first asking, and the questions that came.
struct Run {
    lines: Vec<String>,
    names: Vec<String>,
    questions: usize,
}

/// Runs the program for the name, with the text of `/etc/hosts` and the lines of
/// `/etc/resolv.conf` after the nameserver's, and answers each question with
/// what `answers` gives for it, from the server's socket, until it has ended.
/// `answers` is given the server, the question, and where it came from.
fn run(
    target: Target,
    name: &str,
    hosts: &str,
    options: &str,
    answers: impl FnMut(&mut Server, &Question, SocketAddr),
) -> Run {
    run_in("group", target, name, hosts, options, answers)
}

/// `run` for a program that does the lookup as `mode` says: in a task of a group,
/// in a task of a group beside a task that ticks, or with no group at all.
fn run_in(
    mode: &str,
    target: Target,
    name: &str,
    hosts: &str,
    options: &str,
    mut answers: impl FnMut(&mut Server, &Question, SocketAddr),
) -> Run {
    targets::record(target, &[program(target)]);
    let udp = UdpSocket::bind("127.0.0.1:0").unwrap();
    let port = udp.local_addr().unwrap().port();
    udp.set_read_timeout(Some(Duration::from_millis(10))).unwrap();
    let tcp = TcpListener::bind(("127.0.0.1", port)).unwrap();
    tcp.set_nonblocking(true).unwrap();
    let mut server = Server { udp, other: UdpSocket::bind("127.0.0.1:0").unwrap(), tcp, asked: Vec::new() };
    let dir = crate::scratch::path("dns");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("program");
    crate::executable::write(&path, program(target));
    let resolv = format!("nameserver 127.0.0.1\n{options}");
    let mut child = Command::new(&path)
        .args([&port.to_string(), name, &resolv, hosts, mode])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    std::fs::remove_dir_all(&dir).ok();
    let started = Instant::now();
    let mut buffer = [0u8; 512];
    while child.try_wait().unwrap().is_none() {
        assert!(started.elapsed() < Duration::from_secs(20), "the program did not end, {}", target.name());
        if let Ok((n, from)) = server.udp.recv_from(&mut buffer) {
            let q = Question::read(&buffer[..n]);
            answers(&mut server, &q, from);
            server.asked.push(q);
        }
    }
    let mut said = String::new();
    child.stdout.take().unwrap().read_to_string(&mut said).unwrap();
    let mut names: Vec<String> = Vec::new();
    for q in &server.asked {
        if !names.contains(&q.name) {
            names.push(q.name.clone());
        }
    }
    Run { lines: said.lines().map(str::to_string).collect(), names, questions: server.asked.len() }
}

const FAST: &str = "options timeout:1 attempts:2\n";

fn a_name_in_the_hosts_file_is_found_with_no_question_asked_and_with_no_regard_for_case() {
    for target in linux() {
        let hosts = "# a comment\n10.1.2.3 foo.test other # more\n::1 FOO.test\n10.9.9.9 elsewhere.test\n";
        let said = run(target, "foo.test.", hosts, "", |_, _, _| {});
        if targets::fingerprinting() {
            continue;
        }
        assert_eq!(said.lines, ["10.1.2.3", "0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.1"], "{}", target.name());
        assert_eq!(said.questions, 0, "{}", target.name());
    }
}

fn a_name_answered_for_both_kinds_gives_the_addresses_of_the_first_and_then_of_the_second_whatever_came_first() {
    for target in linux() {
        let mut held: Option<(Question, SocketAddr)> = None;
        let said = run(target, "a.test", "", FAST, |s, q, from| {
            // The answer for the second kind is sent before the one for the
            // first, which is held until it has come.
            if q.kind == 1 {
                held = Some((Question { message: q.message.clone(), name: q.name.clone(), kind: 1 }, from));
            } else {
                let a = answer(q, 0, false, &[Record { owner: None, data: Data::Aaaa([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2]) }]);
                s.udp.send_to(&a, from).unwrap();
                if let Some((first, to)) = held.take() {
                    let a = answer(&first, 0, false, &[Record { owner: None, data: Data::A([10, 0, 0, 7]) }]);
                    s.udp.send_to(&a, to).unwrap();
                }
            }
        });
        if targets::fingerprinting() {
            continue;
        }
        assert_eq!(said.lines, ["10.0.0.7", "0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.2"], "{}", target.name());
    }
}

fn a_cname_is_followed_to_its_address_and_an_address_of_another_owner_is_not_taken() {
    for target in linux() {
        let said = run(target, "a.test", "", FAST, |s, q, from| {
            let records = if q.kind == 1 {
                vec![
                    Record { owner: Some("other.test"), data: Data::A([9, 9, 9, 9]) },
                    Record { owner: Some("b.test"), data: Data::A([1, 2, 3, 4]) },
                    Record { owner: None, data: Data::Cname("b.test") },
                ]
            } else {
                vec![]
            };
            s.udp.send_to(&answer(q, 0, false, &records), from).unwrap();
        });
        if targets::fingerprinting() {
            continue;
        }
        assert_eq!(said.lines, ["1.2.3.4"], "{}", target.name());
    }
}

fn an_answer_with_the_wrong_id_or_the_wrong_question_or_from_the_wrong_address_is_ignored() {
    for target in linux() {
        let said = run(target, "a.test", "", FAST, |s, q, from| {
            let liar = [Record { owner: None, data: Data::A([6, 6, 6, 6]) }];
            // From an address that was not asked, and right in everything else.
            s.other.send_to(&answer(q, 0, false, &liar), from).unwrap();
            // With an id that is not the question's.
            let mut wrong = answer(q, 0, false, &liar);
            wrong[1] = wrong[1].wrapping_add(1);
            s.udp.send_to(&wrong, from).unwrap();
            // To another name, with the id of the question.
            let mut elsewhere = Question { message: q.message.clone(), name: String::new(), kind: q.kind };
            elsewhere.message.truncate(12);
            elsewhere.message.extend(labels("b.test"));
            elsewhere.message.extend_from_slice(&q.kind.to_be_bytes());
            elsewhere.message.extend_from_slice(&[0, 1]);
            s.udp.send_to(&answer(&elsewhere, 0, false, &liar), from).unwrap();
            // Of the kind that was not asked.
            let mut kind = Question { message: q.message.clone(), name: q.name.clone(), kind: q.kind };
            let at = kind.message.len() - 4;
            kind.message[at + 1] = if q.kind == 1 { 28 } else { 1 };
            s.udp.send_to(&answer(&kind, 0, false, &liar), from).unwrap();
            let record = [Record { owner: None, data: Data::A([10, 0, 0, 7]) }];
            let right: &[Record] = if q.kind == 1 { &record } else { &[] };
            s.udp.send_to(&answer(q, 0, false, right), from).unwrap();
        });
        if targets::fingerprinting() {
            continue;
        }
        assert_eq!(said.lines, ["10.0.0.7"], "{}", target.name());
    }
}

fn an_answer_that_was_cut_short_is_asked_again_over_tcp() {
    for target in linux() {
        let said = run(target, "a.test", "", FAST, |s, q, from| {
            if q.kind == 1 {
                s.udp.send_to(&answer(q, 0, true, &[]), from).unwrap();
                // The same question over TCP, which is answered whole.
                for _ in 0..200 {
                    if let Ok((mut stream, _)) = s.tcp.accept() {
                        stream.set_nonblocking(false).unwrap();
                        let mut length = [0u8; 2];
                        stream.read_exact(&mut length).unwrap();
                        let mut message = vec![0u8; u16::from_be_bytes(length) as usize];
                        stream.read_exact(&mut message).unwrap();
                        let asked = Question::read(&message);
                        let whole = answer(&asked, 0, false, &[Record { owner: None, data: Data::A([10, 0, 0, 8]) }]);
                        stream.write_all(&(whole.len() as u16).to_be_bytes()).unwrap();
                        stream.write_all(&whole).unwrap();
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
            } else {
                s.udp.send_to(&answer(q, 0, false, &[]), from).unwrap();
            }
        });
        if targets::fingerprinting() {
            continue;
        }
        assert_eq!(said.lines, ["10.0.0.8"], "{}", target.name());
    }
}

fn a_nameserver_that_does_not_answer_is_asked_again_and_the_lookup_fails_when_the_time_is_gone() {
    for target in linux() {
        let before = Instant::now();
        let said = run(target, "a.test", "", FAST, |_, _, _| {});
        if targets::fingerprinting() {
            continue;
        }
        assert_eq!(said.lines, ["failed lookup of a.test: no nameserver answered in time"], "{}", target.name());
        // Both kinds, sent twice, and not again after the second second.
        assert_eq!(said.questions, 4, "{}", target.name());
        assert!(before.elapsed() >= Duration::from_millis(1000), "{}", target.name());
    }
}

fn a_name_that_does_not_exist_is_unknown_and_a_failing_nameserver_fails_the_lookup() {
    for target in linux() {
        let said = run(target, "a.test", "", FAST, |s, q, from| {
            s.udp.send_to(&answer(q, 3, false, &[]), from).unwrap();
        });
        if targets::fingerprinting() {
            continue;
        }
        assert_eq!(said.lines, ["unknown_host a.test"], "{}", target.name());
        let said = run(target, "a.test", "", FAST, |s, q, from| {
            s.udp.send_to(&answer(q, 2, false, &[]), from).unwrap();
        });
        assert_eq!(said.lines, ["failed lookup of a.test: the nameserver failed"], "{}", target.name());
    }
}

fn the_names_of_the_search_list_are_asked_in_the_order_the_dots_say() {
    for target in linux() {
        let options = format!("search x.test y.test\n{FAST}");
        // A name with no dot is asked with each domain added and then as it is.
        let said = run(target, "a", "", &options, |s, q, from| {
            let records = [Record { owner: None, data: Data::A([10, 0, 0, 9]) }];
            let a = match q.name.as_str() {
                "a" if q.kind == 1 => answer(q, 0, false, &records),
                "a.x.test" => answer(q, 3, false, &[]),
                _ => answer(q, 0, false, &[]),
            };
            s.udp.send_to(&a, from).unwrap();
        });
        if targets::fingerprinting() {
            continue;
        }
        assert_eq!(said.lines, ["10.0.0.9"], "{}", target.name());
        assert_eq!(said.names, ["a.x.test", "a.y.test", "a"], "{}", target.name());
        // A name with a dot is asked as it is first, and then with each domain.
        let said = run(target, "a.b", "", &options, |s, q, from| {
            let records = [Record { owner: None, data: Data::A([10, 0, 0, 9]) }];
            let a = match q.name.as_str() {
                "a.b.x.test" if q.kind == 1 => answer(q, 0, false, &records),
                "a.b" => answer(q, 3, false, &[]),
                _ => answer(q, 0, false, &[]),
            };
            s.udp.send_to(&a, from).unwrap();
        });
        assert_eq!(said.lines, ["10.0.0.9"], "{}", target.name());
        assert_eq!(said.names, ["a.b", "a.b.x.test"], "{}", target.name());
    }
}

fn a_lookup_in_a_task_lets_the_other_tasks_run_while_it_waits_and_one_with_no_group_blocks_as_it_did() {
    for target in linux() {
        // A nameserver that does not answer: the lookup waits its second, and a
        // task that ticks every tenth of a second ticks five times meanwhile.
        let said = run_in("overlap", target, "a.test", "", FAST, |_, _, _| {});
        if targets::fingerprinting() {
            continue;
        }
        let mut expected = vec!["tick"; 5];
        expected.push("failed lookup of a.test: no nameserver answered in time");
        assert_eq!(said.lines, expected, "{}", target.name());
        // With no group the same lookup answers as it always did.
        let said = run_in("plain", target, "a.test", "", FAST, |s, q, from| {
            let record = [Record { owner: None, data: Data::A([10, 0, 0, 7]) }];
            let right: &[Record] = if q.kind == 1 { &record } else { &[] };
            s.udp.send_to(&answer(q, 0, false, right), from).unwrap();
        });
        assert_eq!(said.lines, ["10.0.0.7"], "{}", target.name());
    }
}
