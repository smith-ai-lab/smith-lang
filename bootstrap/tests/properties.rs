//! A test that takes a `Key` is a property: `smith test` runs it many times,
//! each with a key of its own, and shrinks a failure to the least choices
//! that still fail, as `knowledge/language/properties.md` says.
//!
//! The properties are generated, and each prints the choices it draws, so
//! that a test reads every input a run gave the property and what the
//! property made of it.

use std::path::PathBuf;

use crate::Test;
use crate::cli::{smith, stderr};
use crate::generate::{self, Gen};

pub const MACHINE: &[Test] = &[
    (
        "the_example_fails_on_10_and_its_seed_fails_on_10_again",
        the_example_fails_on_10_and_its_seed_fails_on_10_again,
    ),
    (
        "every_property_runs_100_times_with_a_key_each_and_one_seed_gives_one_run",
        every_property_runs_100_times_with_a_key_each_and_one_seed_gives_one_run,
    ),
    (
        "every_failure_is_shrunk_to_choices_that_no_smaller_try_fails_with",
        every_failure_is_shrunk_to_choices_that_no_smaller_try_fails_with,
    ),
    (
        "every_choice_is_shrunk_to_the_least_that_fails_in_a_bounded_number_of_replays",
        every_choice_is_shrunk_to_the_least_that_fails_in_a_bounded_number_of_replays,
    ),
    (
        "a_pair_of_choices_is_shrunk_to_the_least_sum_that_fails",
        a_pair_of_choices_is_shrunk_to_the_least_sum_that_fails,
    ),
];

/// A folder holding one file of tests, `x_test.smith`, with the text.
fn project(label: &str, text: &str) -> PathBuf {
    let dir = crate::cli::folder(label);
    std::fs::write(dir.join("x_test.smith"), text).unwrap();
    dir
}

fn stdout(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// The seed a failure says it ran with.
fn seed_of(said: &str) -> u64 {
    let line = (said.lines())
        .find_map(|l| l.trim_start().strip_prefix("seed: "))
        .unwrap_or_else(|| panic!("no seed in {said}"));
    line.parse().unwrap_or_else(|_| panic!("{said}"))
}

/// The left side of the comparison a failure says.
fn left_of(said: &str) -> u64 {
    let line = (said.lines())
        .find_map(|l| l.trim_start().strip_prefix("left:"))
        .unwrap_or_else(|| panic!("no left side in {said}"));
    line.trim().parse().unwrap_or_else(|_| panic!("{said}"))
}

const EXAMPLE: &str = "use std.bits (u64)\nuse std.random (Key, bits)\nuse std.test (Failed, expect)\n\nfn test_small(key: take Key) -> Result[(), Failed] {\n  n: u64 = bits(take key) % 64\n  expect { n < 10 }\n}\n";

/// The example of the issue: `n` below 10 fails for most keys, and the least
/// choice that fails is 10, which the failure says with the seed of the run.
/// The run with that seed says the same failure.
fn the_example_fails_on_10_and_its_seed_fails_on_10_again() {
    let dir = project("example", EXAMPLE);
    let ran = smith(&dir, &["test"]);
    assert_eq!(ran.status.code(), Some(1), "{}", stderr(&ran));
    assert!(stdout(&ran).contains("failed x_test.smith test_small\n"), "{}", stdout(&ran));
    let failure = stderr(&ran);
    assert_eq!(left_of(&failure), 10, "{failure}");
    let seed = seed_of(&failure).to_string();
    let again = smith(&dir, &["test", "--seed", &seed]);
    assert_eq!(again.status.code(), Some(1), "{}", stderr(&again));
    assert_eq!(stderr(&again), failure);
}

/// A property that draws one choice, or two from the keys `split` gives,
/// prints them, and checks that what they are modulo `modulus` adds to less
/// than `below`, which every choice passes when `below` is past the most they
/// can add to. `below` is given by a function, so that a check that always
/// holds is not refused as one.
#[derive(Debug)]
struct Property {
    draws: usize,
    modulus: u64,
    below: u64,
    seed: u64,
}

fn a_property(g: &mut Gen) -> Property {
    let draws = 1 + g.below(2);
    let modulus = 2 + g.below(150) as u64;
    let most = draws as u64 * (modulus - 1);
    // A property that never fails is drawn as often as one that may.
    let below = if g.one_in(2) {
        most + 1
    } else {
        1 + g.below(most as usize) as u64
    };
    Property {
        draws,
        modulus,
        below,
        seed: g.below(1 << 30) as u64,
    }
}

fn text_of(p: &Property) -> String {
    let (m, t) = (p.modulus, p.below);
    let body = if p.draws == 1 {
        format!("  a = bits(take key)\n  print(\"drawn {{a}}\")\n  expect {{ a % {m} < below() }}\n")
    } else {
        format!(
            "  (ka, kb) = split(take key)\n  a = bits(take ka)\n  b = bits(take kb)\n  print(\"drawn {{a}} {{b}}\")\n  expect {{ a % {m} + b % {m} < below() }}\n"
        )
    };
    format!(
        "use std.bits (u64)\nuse std.console (console, print)\nuse std.random (Key, bits, split)\nuse std.test (Failed, expect)\n\nfn below() -> u64 {{ {t} }}\n\nfn test_drawn(key: take Key) -> Result[(), Failed] <console, alloc> {{\n{body}}}\n"
    )
}

/// The choices of each run of the property, in the order the runs were made.
fn runs_of(said: &str) -> Vec<Vec<u64>> {
    (said.lines())
        .filter_map(|l| l.strip_prefix("drawn "))
        .map(|l| l.split(' ').map(|n| n.parse().unwrap()).collect())
        .collect()
}

/// Whether the property passes on the choices.
fn passes(p: &Property, choices: &[u64]) -> bool {
    choices.iter().map(|c| c % p.modulus).sum::<u64>() < p.below
}

/// For every property, a run of `smith test --seed` gives the property 100
/// keys while it passes, each giving other choices, and says it passed with
/// the number of its runs; a run with the same seed gives the same choices,
/// and the same result, and a run with another seed other choices.
fn every_property_runs_100_times_with_a_key_each_and_one_seed_gives_one_run() {
    generate::check(12491, 4, a_property, |p| {
        let dir = project("runs", &text_of(p));
        let seed = p.seed.to_string();
        let ran = smith(&dir, &["test", "--seed", &seed]);
        let said = stdout(&ran);
        let runs = runs_of(&said);
        let explored = (runs.iter())
            .position(|c| !passes(p, c))
            .map_or(runs.len(), |at| at + 1);
        if runs.iter().all(|c| passes(p, c)) {
            assert_eq!(runs.len(), 100, "{said}");
            assert!(said.contains("passed x_test.smith test_drawn (100 runs)\n"), "{said}");
            assert_eq!(ran.status.code(), Some(0), "{said}{}", stderr(&ran));
        } else {
            assert!(explored <= 100, "{said}");
            assert!(said.contains("failed x_test.smith test_drawn\n"), "{said}");
            assert_eq!(ran.status.code(), Some(1), "{said}{}", stderr(&ran));
        }
        let first = &runs[..explored];
        for (i, c) in first.iter().enumerate() {
            assert!(!first[..i].contains(c), "{said}");
        }
        let again = smith(&dir, &["test", "--seed", &seed]);
        assert_eq!(runs_of(&stdout(&again)), runs, "{said}");
        assert_eq!(stderr(&again), stderr(&ran));
        let other = (p.seed + 1).to_string();
        let elsewhere = smith(&dir, &["test", "--seed", &other]);
        assert_ne!(runs_of(&stdout(&elsewhere))[0], runs[0], "{said}");
    });
}

/// For every property that fails, the input the failure says is the one the
/// last run built from the choices it was given and fails on, every run
/// whose choices come before those, fewer of them or as many with the first
/// that differs less, passed, and the left side the failure says is the
/// least that fails, which a run of one choice reaches through the choice and
/// a run of two through the second.
fn every_failure_is_shrunk_to_choices_that_no_smaller_try_fails_with() {
    let failing = |g: &mut Gen| {
        let mut p = a_property(g);
        let most = p.draws as u64 * (p.modulus - 1);
        // One choice in four or more fails, so that a run finds one.
        p.below = 1 + g.below((most - most / 4) as usize) as u64;
        p
    };
    generate::check(12492, 4, failing, |p| {
        let dir = project("shrunk", &text_of(p));
        let ran = smith(&dir, &["test", "--seed", &p.seed.to_string()]);
        let said = stdout(&ran);
        let failure = stderr(&ran);
        assert_eq!(ran.status.code(), Some(1), "{said}{failure}");
        let runs = runs_of(&said);
        let reported = runs.last().unwrap_or_else(|| panic!("{said}"));
        assert!(!passes(p, reported), "{said}");
        let sum: u64 = reported.iter().map(|c| c % p.modulus).sum();
        assert_eq!(left_of(&failure), sum, "{said}{failure}");
        for c in &runs {
            let before = c.len() < reported.len() || (c.len() == reported.len() && c < reported);
            if before {
                assert!(passes(p, c), "{c:?} fails and comes before {reported:?}\n{said}");
            }
        }
        assert_eq!(sum, p.below, "{said}{failure}");
    });
}

/// The most replays a property of one choice is shrunk in: 64 passes of 64.
const REPLAYS: usize = 64 * 64;

/// A property of one choice that fails when the choice is no less than
/// `least`, or when what it is modulo `modulus` is no less, where at least a
/// tenth of every `modulus` choices fail.
#[derive(Debug)]
struct Least {
    least: u64,
    modulus: Option<u64>,
    seed: u64,
}

/// A number of up to 64 bits, of a width drawn first, so that numbers of
/// every size are drawn.
fn wide(g: &mut Gen) -> u64 {
    let width = g.below(65) as u32;
    let high = g.below(1 << 30) as u64;
    let low = g.below(1 << 30) as u64;
    let top = g.below(16) as u64;
    let n = (top << 60) | (high << 30) | low;
    if width == 64 { n } else { n & ((1u64 << width) - 1) }
}

fn a_least(g: &mut Gen) -> Least {
    let modulus = (!g.one_in(3)).then(|| wide(g).max(2));
    let least = match modulus {
        // Half of the time fewer than half of the choices fail.
        Some(m) if g.one_in(2) => m / 2 + wide(g) % (m / 2 - m.div_ceil(10) + 1),
        Some(m) => wide(g) % (m / 2 + 1),
        // Half of the choices or more fail, so that a run finds one.
        None => wide(g) >> 1,
    };
    Least { least, modulus, seed: g.below(1 << 30) as u64 }
}

fn least_text(p: &Least) -> String {
    let check = match p.modulus {
        Some(m) => format!("a % {m} < least()"),
        None => "a < least()".to_string(),
    };
    format!(
        "use std.bits (u64)\nuse std.console (console, print)\nuse std.random (Key, bits)\nuse std.test (Failed, expect)\n\nfn least() -> u64 {{ {} }}\n\nfn test_least(key: take Key) -> Result[(), Failed] <console, alloc> {{\n  a = bits(take key)\n  print(\"drawn {{a}}\")\n  expect {{ {check} }}\n}}\n",
        p.least
    )
}

/// For every property of one choice that fails from `least` on, spread over
/// every width of 64 bits, with `%` and without, the failure is shrunk in no
/// more than `REPLAYS` replays after the run that first failed, to a choice
/// that is `least` modulo the modulus, and to `least` itself when there is no
/// modulus or at least half of every `modulus` choices fail.
fn every_choice_is_shrunk_to_the_least_that_fails_in_a_bounded_number_of_replays() {
    generate::check(12493, 12, a_least, |p| {
        let dir = project("least", &least_text(p));
        let ran = smith(&dir, &["test", "--seed", &p.seed.to_string()]);
        let said = stdout(&ran);
        assert_eq!(ran.status.code(), Some(1), "{said}{}", stderr(&ran));
        let runs = runs_of(&said);
        let fails = |c: u64| match p.modulus {
            Some(m) => c % m >= p.least,
            None => c >= p.least,
        };
        let first = runs.iter().position(|c| fails(c[0])).unwrap_or_else(|| panic!("{said}"));
        let replays = runs.len() - first - 1;
        assert!(replays <= REPLAYS, "{replays} replays\n{said}");
        let reported = runs.last().unwrap()[0];
        match p.modulus {
            Some(m) if p.least > m / 2 => assert_eq!(reported % m, p.least, "{said}"),
            _ => assert_eq!(reported, p.least, "{said}"),
        }
    });
}

const PAIR: &str = "use std.bits (u64)\nuse std.random (Key, bits, split)\nuse std.test (Failed, expect)\n\nfn test_pair(key: take Key) -> Result[(), Failed] {\n  (ka, kb) = split(take key)\n  a: u64 = bits(take ka) % 1000\n  b: u64 = bits(take kb) % 1000\n  expect { a + b < 1500 }\n}\n";

/// Two choices from `split`, modulo 1000, that fail when they add to 1500 or
/// more, are shrunk to two that add to 1500 exactly, for every seed.
fn a_pair_of_choices_is_shrunk_to_the_least_sum_that_fails() {
    let dir = project("pair", PAIR);
    for seed in 1..=4 {
        let ran = smith(&dir, &["test", "--seed", &seed.to_string()]);
        let failure = stderr(&ran);
        assert_eq!(ran.status.code(), Some(1), "{failure}");
        assert_eq!(left_of(&failure), 1500, "{failure}");
    }
}
