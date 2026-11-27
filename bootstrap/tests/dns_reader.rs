//! The reader of DNS messages in `std.dns` is given every truncated and every
//! corrupted form of a set of answers, which are what nameservers send, and
//! ends each time, having read no byte that was not given, with an answer only
//! when it is whole: a message that is cut anywhere is not believed, and one
//! that is corrupted is believed or not, and the program that reads it goes on.

use smith::{lower, scalar};

use crate::Test;
use crate::dns_lookup::{Data, Question, Record, answer};

pub const ONCE: &[Test] = &[
    (
        "every_truncated_and_every_corrupted_form_of_an_answer_is_read_to_its_end_and_a_truncated_one_is_never_believed_part_1_of_8",
        || every_truncated_and_every_corrupted_form_of_an_answer_is_read_to_its_end_and_a_truncated_one_is_never_believed(0),
    ),
    (
        "every_truncated_and_every_corrupted_form_of_an_answer_is_read_to_its_end_and_a_truncated_one_is_never_believed_part_2_of_8",
        || every_truncated_and_every_corrupted_form_of_an_answer_is_read_to_its_end_and_a_truncated_one_is_never_believed(1),
    ),
    (
        "every_truncated_and_every_corrupted_form_of_an_answer_is_read_to_its_end_and_a_truncated_one_is_never_believed_part_3_of_8",
        || every_truncated_and_every_corrupted_form_of_an_answer_is_read_to_its_end_and_a_truncated_one_is_never_believed(2),
    ),
    (
        "every_truncated_and_every_corrupted_form_of_an_answer_is_read_to_its_end_and_a_truncated_one_is_never_believed_part_4_of_8",
        || every_truncated_and_every_corrupted_form_of_an_answer_is_read_to_its_end_and_a_truncated_one_is_never_believed(3),
    ),
    (
        "every_truncated_and_every_corrupted_form_of_an_answer_is_read_to_its_end_and_a_truncated_one_is_never_believed_part_5_of_8",
        || every_truncated_and_every_corrupted_form_of_an_answer_is_read_to_its_end_and_a_truncated_one_is_never_believed(4),
    ),
    (
        "every_truncated_and_every_corrupted_form_of_an_answer_is_read_to_its_end_and_a_truncated_one_is_never_believed_part_6_of_8",
        || every_truncated_and_every_corrupted_form_of_an_answer_is_read_to_its_end_and_a_truncated_one_is_never_believed(5),
    ),
    (
        "every_truncated_and_every_corrupted_form_of_an_answer_is_read_to_its_end_and_a_truncated_one_is_never_believed_part_7_of_8",
        || every_truncated_and_every_corrupted_form_of_an_answer_is_read_to_its_end_and_a_truncated_one_is_never_believed(6),
    ),
    (
        "every_truncated_and_every_corrupted_form_of_an_answer_is_read_to_its_end_and_a_truncated_one_is_never_believed_part_8_of_8",
        || every_truncated_and_every_corrupted_form_of_an_answer_is_read_to_its_end_and_a_truncated_one_is_never_believed(7),
    ),
];

/// What each part reads: the answer it is given, the values that a byte is
/// changed to, from the first up to but not including the last, of the five,
/// and whether it reads the answer cut at each place. Every value and every cut
/// of every answer is in one part, and the costly answers are in several, so
/// that no part takes long.
const PARTS: [(usize, usize, usize, bool); 8] = [
    (0, 0, 5, true),
    (1, 0, 3, true),
    (1, 3, 5, false),
    (2, 0, 2, true),
    (2, 2, 4, false),
    (2, 4, 5, false),
    (3, 0, 5, true),
    (4, 0, 5, true),
];

/// A set of answers to a question for `a.test`, with how many addresses each
/// holds: of one kind and the other, with a name that is a `CNAME` of a `CNAME`
/// whose labels end in a pointer to the question's, cut short by the bit that
/// says so, and of a name that does not exist.
fn answers() -> Vec<(Vec<u8>, u16, usize)> {
    let a = Question::asking("a.test", 1);
    let aaaa = Question::asking("a.test", 28);
    let one = answer(&a, 0, false, &[Record { owner: None, data: Data::A([10, 0, 0, 7]) }]);
    let two = answer(
        &aaaa,
        0,
        false,
        &[
            Record { owner: None, data: Data::Aaaa([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]) },
            Record { owner: None, data: Data::Aaaa([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2]) },
        ],
    );
    // a.test is a CNAME of b.test, which is a CNAME of c.test, which has an
    // address; the targets write their first label and then point at `test`,
    // which is at the 14th byte of the question.
    let mut chained = a.message[..2].to_vec();
    chained.extend_from_slice(&[0x81, 0x80, 0, 1, 0, 3, 0, 0, 0, 0]);
    chained.extend_from_slice(&a.message[12..]);
    for (owner, rdata) in [
        (vec![0xc0, 12], vec![1, b'b', 0xc0, 14]),
        (vec![1, b'b', 0xc0, 14], vec![1, b'c', 0xc0, 14]),
    ] {
        chained.extend(owner);
        chained.extend_from_slice(&[0, 5, 0, 1, 0, 0, 0, 60]);
        chained.extend_from_slice(&(rdata.len() as u16).to_be_bytes());
        chained.extend(rdata);
    }
    chained.extend_from_slice(&[1, b'c', 0xc0, 14, 0, 1, 0, 1, 0, 0, 0, 60, 0, 4, 1, 2, 3, 4]);
    let cut = answer(&a, 0, true, &[]);
    let missing = answer(&a, 3, false, &[]);
    vec![(one, 1, 1), (two, 28, 2), (chained, 1, 1), (cut, 1, 0), (missing, 1, 0)]
}

fn list(bytes: &[u8]) -> String {
    let numbers: Vec<String> = bytes.iter().map(|b| b.to_string()).collect();
    format!("[{}]", numbers.join(", "))
}

fn every_truncated_and_every_corrupted_form_of_an_answer_is_read_to_its_end_and_a_truncated_one_is_never_believed(part: usize) {
    let (which, from, to, cut) = PARTS[part];
    let set = vec![answers().swap_remove(which)];
    let mut calls = String::new();
    for (message, kind, _) in &set {
        calls.push_str(&format!("  survey({}, {kind})\n", list(message)));
    }
    let cuts = if cut { "m.size" } else { "0" };
    let source = format!(
        "use std.bits (u16)
use std.console (console, print)
use std.dns (read_reply)
use std.int (saturating_add)
use std.list (push)

// How many addresses the message holds, and 99 when it is not believed.
fn read(m: List[Byte], kind: u16) -> Nat <alloc> {{
  match read_reply(m, 4660, \"a.test\", kind) {{
    .some(r) {{ r.addresses.size }}
    .none {{ 99 }}
  }}
}}

// The message with the byte at `at` made `value`.
fn changed(m: List[Byte], at: Nat, value: Byte) -> List[Byte] <alloc> {{
  mut made: List[Byte] = []
  loop i in 0..m.size {{
    if i == at {{ made = push(made, value) }} else {{ made = push(made, m[i]) }}
  }}
  made
}}

// The message whose first bytes are the first `n`.
fn first(m: List[Byte], n: Nat) -> List[Byte] <alloc> {{
  mut made: List[Byte] = []
  loop i in 0..m.size {{
    if i < n {{ made = push(made, m[i]) }}
  }}
  made
}}

fn survey(m: List[Byte], kind: u16) <console, alloc> {{
  mut believed: Nat = 0
  loop n in 0..{cuts} {{
    if read(first(m, n), kind) != 99 {{ believed = saturating_add(believed, 1) }}
  }}
  mut corrupted: Nat = 0
  loop at in 0..m.size {{
    loop k in {from}..{to} {{
      mut value: Byte = 0
      if k == 1 {{ value = 255 }}
      if k == 2 {{ value = 192 }}
      if k == 3 {{ value = 1 }}
      if k == 4 {{ value = 128 }}
      if read(changed(m, at, value), kind) != 99 {{ corrupted = saturating_add(corrupted, 1) }}
    }}
  }}
  print(\"{{read(m, kind)}} {{believed}} {{corrupted}}\")
}}

fn main() <console, alloc> {{
{calls}}}
"
    );
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(scalar::check(&program), Ok(()));
    let said = scalar::run(&program);
    assert_eq!(said.status, 0, "{}", String::from_utf8_lossy(&said.stdout));
    let printed = String::from_utf8(said.stdout).unwrap();
    let lines: Vec<&str> = printed.lines().collect();
    assert_eq!(lines.len(), set.len(), "{printed}");
    for (line, (message, _, addresses)) in lines.iter().zip(&set) {
        let numbers: Vec<usize> = line.split(' ').map(|n| n.parse().unwrap()).collect();
        // The whole message gives its addresses, and no message cut anywhere
        // before its end is believed.
        assert_eq!(numbers[0], *addresses, "{} bytes: {line}", message.len());
        assert_eq!(numbers[1], 0, "{} bytes: {line}", message.len());
        // Some of the corrupted forms are not believed and some are.
        assert!(numbers[2] > 0 && numbers[2] < message.len() * (to - from), "{} bytes: {line}", message.len());
    }
}
