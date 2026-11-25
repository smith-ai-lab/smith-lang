//! `std.sha2`: `sha256` gives the 32 bytes that FIPS 180-4 gives for any bytes,
//! asks for no memory, and carries no effect.

use smith::scalar::{self, Op, System};
use smith::{check, lower};

use crate::Test;
use crate::generate::{self, Gen};
use crate::numbers::with_header;
use crate::sha256;
use crate::targets;

pub const ONCE: &[Test] = &[
    (
        "sha256_gives_the_digests_of_fips_180_4_for_its_messages",
        sha256_gives_the_digests_of_fips_180_4_for_its_messages,
    ),
    (
        "sha256_gives_what_a_reference_gives_for_the_lengths_around_a_block",
        sha256_gives_what_a_reference_gives_for_the_lengths_around_a_block,
    ),
    (
        "sha256_gives_what_a_reference_gives_for_any_bytes",
        sha256_gives_what_a_reference_gives_for_any_bytes,
    ),
    (
        "sha256_carries_no_effect_and_its_program_never_calls_the_allocator",
        sha256_carries_no_effect_and_its_program_never_calls_the_allocator,
    ),
];

/// The lengths where the padding of a message changes: those that leave room
/// for the length in the last block, and those that do not.
const BOUNDARIES: [usize; 5] = [55, 56, 63, 64, 65];

pub const MACHINE: &[Test] = &[(
    "every_executable_gives_the_digests_the_reference_gives",
    every_executable_gives_the_digests_the_reference_gives,
)];

/// The bytes the program hashes, written out as a `Slice`, and what it prints:
/// each byte of the digest, a line each.
fn hashing(bytes: &[u8]) -> String {
    let items: Vec<String> = bytes.iter().map(u8::to_string).collect();
    let bytes = if bytes.is_empty() {
        "\"\".bytes".to_string()
    } else {
        format!("[{}]", items.join(", "))
    };
    with_header(format!(
        "use std.sha2 (sha256)\n\nfn main() <console, alloc> {{\n  bytes: Slice[Byte] = {bytes}\n  digest = sha256(bytes)\n  loop x in digest {{ print(\"{{x}}\") }}\n}}\n"
    ))
}

fn digest_of(bytes: &[u8]) -> Vec<u8> {
    let source = hashing(bytes);
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for\n{source}"));
    let out = scalar::run(&program);
    assert_eq!(out.status, 0, "for\n{source}");
    String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|l| l.parse().unwrap())
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The empty message, `abc`, and the two messages of FIPS 180-4 that fill one
/// block and two.
fn sha256_gives_the_digests_of_fips_180_4_for_its_messages() {
    for (message, digest) in [
        (
            "",
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        ),
        (
            "abc",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        ),
        (
            "abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq",
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
        ),
        (
            "abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu",
            "cf5b16a778af8380036ce59e7b0492370b249b11e8f07a51afac45037afee9d1",
        ),
    ] {
        assert_eq!(hex(&digest_of(message.as_bytes())), digest, "for {message:?}");
    }
}

/// The lengths around a block, which are where the padding changes, give what
/// the reference gives.
fn sha256_gives_what_a_reference_gives_for_the_lengths_around_a_block() {
    for length in BOUNDARIES {
        let bytes: Vec<u8> = (0..length).map(|i| (i * 7 + 3) as u8).collect();
        assert_eq!(hex(&digest_of(&bytes)), sha256::hex(&bytes), "for {length} bytes");
    }
}

/// Any bytes of up to a thousand give what the reference gives.
fn sha256_gives_what_a_reference_gives_for_any_bytes() {
    generate::check(
        4201,
        2,
        |g: &mut Gen| {
            let length = g.below(1000);
            (0..length).map(|_| g.below(256) as u8).collect::<Vec<u8>>()
        },
        |bytes| {
            assert_eq!(hex(&digest_of(bytes)), sha256::hex(bytes), "for {} bytes", bytes.len());
        },
    );
}

/// The program that prints the digest of each message, a byte a line, and what it
/// prints, worked out by the reference.
fn hashing_all(messages: &[Vec<u8>]) -> (String, Vec<u8>) {
    let mut body = String::new();
    let mut said = String::new();
    for (k, bytes) in messages.iter().enumerate() {
        let items: Vec<String> = bytes.iter().map(u8::to_string).collect();
        let written = if bytes.is_empty() {
            "\"\".bytes".to_string()
        } else {
            format!("[{}]", items.join(", "))
        };
        body += &format!(
            "  m{k}: Slice[Byte] = {written}\n  d{k} = sha256(m{k})\n  loop x in d{k} {{ print(\"{{x}}\") }}\n"
        );
        let reference = sha256::hex(bytes);
        for at in (0..64).step_by(2) {
            said += &format!("{}\n", u8::from_str_radix(&reference[at..at + 2], 16).unwrap());
        }
    }
    let source = with_header(format!(
        "use std.sha2 (sha256)\n\nfn main() <console, alloc> {{\n{body}}}\n"
    ));
    (source, said.into_bytes())
}

/// Every executable that this machine runs gives the digests that the
/// reference gives, for the lengths around a block, the longest of them a few
/// thousand bytes, and for every length from 0 to 130.
fn every_executable_gives_the_digests_the_reference_gives() {
    let mut messages: Vec<Vec<u8>> = (0..=130)
        .map(|length| (0..length).map(|i| (i * 7 + 3) as u8).collect())
        .collect();
    for length in [1000, 2999, 3000, 4097] {
        messages.push((0..length).map(|i| (i * 31 + i / 7) as u8).collect());
    }
    let (source, said) = hashing_all(&messages);
    let said = scalar::Outcome {
        status: 0,
        stdout: said,
    };
    for target in targets::runnable() {
        targets::runs_as(source.as_bytes(), target, &said);
    }
}

/// `sha256` is a function with no effect, so a function that calls it and
/// declares none compiles, and the program that holds it never asks the system
/// for memory.
fn sha256_carries_no_effect_and_its_program_never_calls_the_allocator() {
    let source = with_header(
        "use std.sha2 (sha256)\n\nfn pure(bytes: Slice[Byte]) -> [32]Byte { sha256(bytes) }\n\nfn main() <console> {\n  bytes: Slice[Byte] = [1, 2, 3]\n  d = pure(bytes)\n  print(\"done\")\n}\n".to_string(),
    );
    assert!(check(source.as_bytes()).is_ok(), "for\n{source}");
    let program = lower(source.as_bytes()).unwrap();
    let allocates = program.functions.iter().any(|f| {
        f.blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(i.op, Op::System { call: System::Allocate, .. }))
    });
    assert!(!allocates, "for\n{source}");
}
