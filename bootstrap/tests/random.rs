//! `std.random`: a `Key` is made from a seed or from `entropy`, used once, and
//! gives bits and two keys that are the `knowledge/library/random.md` says,
//! which is the generator Threefry-2x64 with 20 rounds, checked here against
//! the published vectors and against a model of it written in Rust; and
//! `uniform` and `standard_normal`, the floats drawn from a key, which are
//! checked against a model of the arithmetic the page gives, on every target,
//! and for their mean and spread over many keys.

use smith::{check, lower, scalar};

use crate::Test;
use crate::exact;
use crate::generate::{self, Gen};
use crate::targets;

pub const ONCE: &[Test] = &[
    (
        "threefry_gives_the_published_vectors",
        threefry_gives_the_published_vectors,
    ),
    (
        "bits_and_split_and_seeded_give_what_the_model_of_the_generator_gives_for_every_seed",
        bits_and_split_and_seeded_give_what_the_model_of_the_generator_gives_for_every_seed,
    ),
    (
        "a_key_is_used_once",
        a_key_is_used_once,
    ),
    (
        "uniform_and_standard_normal_give_what_the_model_gives_for_every_seed",
        uniform_and_standard_normal_give_what_the_model_gives_for_every_seed,
    ),
    (
        "uniform_is_at_least_0_and_below_1_where_the_bits_are_at_their_edges",
        uniform_is_at_least_0_and_below_1_where_the_bits_are_at_their_edges,
    ),
];

pub const MACHINE: &[Test] = &[
    (
        "every_executable_for_this_machine_that_draws_from_a_fresh_key_prints_different_bits_each_run",
        every_executable_for_this_machine_that_draws_from_a_fresh_key_prints_different_bits_each_run,
    ),
    (
        "every_executable_draws_the_floats_the_evaluator_draws_from_a_key",
        every_executable_draws_the_floats_the_evaluator_draws_from_a_key,
    ),
    (
        "draws_from_many_keys_have_the_mean_and_spread_of_their_distribution_on_every_executable",
        draws_from_many_keys_have_the_mean_and_spread_of_their_distribution_on_every_executable,
    ),
];


fn rotate(x: u64, by: u32) -> u64 {
    x.rotate_left(by)
}

/// Threefry-2x64 with 20 rounds, as Random123 gives it, for a counter and a key.
fn threefry(counter: [u64; 2], key: [u64; 2]) -> [u64; 2] {
    const ROTATIONS: [u32; 8] = [16, 42, 12, 31, 16, 32, 24, 21];
    let third = key[0] ^ key[1] ^ 0x1BD1_1BDA_A9FC_1A22;
    let keys = [key[0], key[1], third];
    let mut x = [
        counter[0].wrapping_add(key[0]),
        counter[1].wrapping_add(key[1]),
    ];
    for round in 0..20 {
        x[0] = x[0].wrapping_add(x[1]);
        x[1] = rotate(x[1], ROTATIONS[round % 8]) ^ x[0];
        if round % 4 == 3 {
            let step = round / 4 + 1;
            x[0] = x[0].wrapping_add(keys[step % 3]);
            x[1] = x[1].wrapping_add(keys[(step + 1) % 3]).wrapping_add(step as u64);
        }
    }
    x
}

/// What `random.md` says `seeded`, `bits`, and `split` give, as a model.
fn seeded(seed: u64) -> [u64; 2] {
    threefry([3, 0], [seed, 0])
}

fn bits(key: [u64; 2]) -> u64 {
    threefry([0, 0], key)[0]
}

fn split(key: [u64; 2]) -> ([u64; 2], [u64; 2]) {
    (threefry([1, 0], key), threefry([2, 0], key))
}

/// What a program prints, by the evaluator, or the errors it is refused with.
fn printed(source: &str) -> String {
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?}\n{source}"));
    String::from_utf8(scalar::run(&program).stdout).unwrap()
}

const HEAD: &str = "use std.bits (u64)\nuse std.console (console, print)\nuse std.fmt (hex)\nuse std.random (Words, bits, seeded, split, threefry)\n\n";

/// The vectors of `kat_vectors` of Random123 for `threefry2x64 20`: the
/// counter and key of all zeros, of all ones, and of the digits of pi.
const VECTORS: [([u64; 2], [u64; 2], [u64; 2]); 3] = [
    (
        [0, 0],
        [0, 0],
        [0xc2b6_e3a8_c2c6_9865, 0x6f81_ed42_f350_084d],
    ),
    (
        [u64::MAX, u64::MAX],
        [u64::MAX, u64::MAX],
        [0xe02c_b7c4_d95d_277a, 0xd066_33d0_893b_8b68],
    ),
    (
        [0x243f_6a88_85a3_08d3, 0x1319_8a2e_0370_7344],
        [0xa409_3822_299f_31d0, 0x082e_fa98_ec4e_6c89],
        [0x263c_7d30_bb0f_0af1, 0x56be_8361_d331_1526],
    ),
];

/// The generator the page gives is the one Random123 publishes, for the three
/// vectors that file holds, and the Rust model is it too.
fn threefry_gives_the_published_vectors() {
    let mut body = String::from("fn main() <console, alloc> {\n");
    for (i, (counter, key, _)) in VECTORS.iter().enumerate() {
        body += &format!(
            "  r{i} = threefry(Words(first: {}, second: {}), Words(first: {}, second: {}))\n  print(\"{{hex(r{i}.first, width: 16, letters: .lower)}} {{hex(r{i}.second, width: 16, letters: .lower)}}\")\n",
            counter[0], counter[1], key[0], key[1]
        );
    }
    body += "}\n";
    let got = printed(&format!("{HEAD}{body}"));
    let want: String = VECTORS
        .iter()
        .map(|(_, _, out)| format!("{:016x} {:016x}\n", out[0], out[1]))
        .collect();
    assert_eq!(got, want);
    for (counter, key, out) in VECTORS {
        assert_eq!(threefry(counter, key), out, "the model");
    }
}

/// Seeds, as eight numbers of 64 bits.
fn seeds(g: &mut Gen) -> Vec<u64> {
    (0..8)
        .map(|_| {
            (0..4).fold(0u64, |n, _| n << 16 | g.below(1 << 16) as u64)
        })
        .collect()
}

/// For every seed, the bits of the key `seeded` makes, and of the two keys
/// `split` makes of it, are those the model gives; and the three differ.
fn bits_and_split_and_seeded_give_what_the_model_of_the_generator_gives_for_every_seed() {
    generate::check(160, 12, seeds, |seeds| {
        let mut body = String::from("fn main() <console, alloc> {\n");
        for (i, seed) in seeds.iter().enumerate() {
            body += &format!(
                "  (a{i}, b{i}) = split(seeded({seed}))\n  print(\"{{bits(seeded({seed}))}} {{bits(take a{i})}} {{bits(take b{i})}}\")\n"
            );
        }
        body += "}\n";
        let got = printed(&format!("{HEAD}{body}"));
        let want: String = seeds
            .iter()
            .map(|&seed| {
                let key = seeded(seed);
                let (a, b) = split(key);
                let line = [bits(key), bits(a), bits(b)];
                assert!(
                    line[0] != line[1] && line[0] != line[2] && line[1] != line[2],
                    "{seed}: {line:?}"
                );
                format!("{} {} {}\n", line[0], line[1], line[2])
            })
            .collect();
        assert_eq!(got, want);
        // The same seed twice is the same key.
        for &seed in seeds {
            assert_eq!(seeded(seed), seeded(seed));
        }
    });
}

/// A key given to `bits` or `split` is given away, so a second use is refused.
fn a_key_is_used_once() {
    let twice = format!("{HEAD}fn main() <console, alloc> {{\n  key = seeded(1)\n  print(\"{{bits(take key)}} {{bits(take key)}}\")\n}}\n");
    let Err(errors) = check(twice.as_bytes()) else {
        panic!("a key was used twice");
    };
    assert!(
        errors.iter().any(|e| e.message.contains("key")),
        "{errors:?}"
    );
    let once = format!("{HEAD}fn main() <console, alloc> {{\n  key = seeded(1)\n  print(\"{{bits(take key)}}\")\n}}\n");
    assert!(check(once.as_bytes()).is_ok());
}

/// What `random.md` says `uniform` gives: the top 24 bits of `bits`, over
/// `2^24`.
fn uniform(key: [u64; 2]) -> f32 {
    (bits(key) >> 40) as f32 / 16_777_216.0
}

/// What `random.md` says `standard_normal` gives: Box-Muller, from bits 63 to
/// 40 and 39 to 16 of `bits`, each step of it an `f32` rounded once, with the
/// reference for `ln` and `cos` that no host library gives.
fn standard_normal(key: [u64; 2]) -> f32 {
    let b = bits(key);
    let first = ((b >> 40) + 1) as f32 / 16_777_216.0;
    let second = ((b >> 16) & 0xFF_FFFF) as f32 / 16_777_216.0;
    let ln = f32::from_bits(exact::ln_f32(first));
    let cos = f32::from_bits(exact::cos_f32(6.283_185_5 * second));
    (-2.0 * ln).sqrt() * cos
}

/// Seeds whose key's `bits` has, as its top 24 bits, 0, 1, `2^24 - 2`, and
/// `2^24 - 1`, two of each, found by a search with the model of the
/// generator. The low bits of those at the top are high, so that the 64 bits
/// over `2^64`, rounded to an `f32`, would be 1.
const EDGES: [(u64, u64); 8] = [
    (31_166_927, 0),
    (44_055_378, 0),
    (9_980_753, 1),
    (29_841_400, 1),
    (203_855, (1 << 24) - 2),
    (3_399_169, (1 << 24) - 2),
    (44_085_426, (1 << 24) - 1),
    (45_371_710, (1 << 24) - 1),
];

const FLOATS: &str = "use std.console (console, print)\nuse std.random (seeded, split, standard_normal, uniform)\n\n";

/// A program that prints, for each seed, what `uniform` and `standard_normal`
/// draw from its key, one line for each.
fn drawing(seeds: &[u64]) -> String {
    let mut source = format!("{FLOATS}fn main() <console, alloc> {{\n");
    for seed in seeds {
        source += &format!("  print(\"{{uniform(seeded({seed}))}} {{standard_normal(seeded({seed}))}}\")\n");
    }
    source + "}\n"
}

/// The floats each line of what a program printed holds, read back exactly.
fn floats_printed(said: &str) -> Vec<Vec<f32>> {
    said.lines()
        .map(|line| {
            line.split(' ')
                .map(|x| x.parse::<f32>().unwrap_or_else(|_| panic!("{x:?} in {line:?}")))
                .collect()
        })
        .collect()
}

/// The bits of each float, so that `-0.0` is told from `0.0`.
fn same_floats(got: &[Vec<f32>], want: &[Vec<f32>]) -> bool {
    let bits = |rows: &[Vec<f32>]| -> Vec<Vec<u32>> {
        rows.iter().map(|r| r.iter().map(|x| x.to_bits()).collect()).collect()
    };
    bits(got) == bits(want)
}

/// For every seed, `uniform` and `standard_normal` of its key are the floats
/// the model gives, in the evaluator, and the same seed gives the same floats.
fn uniform_and_standard_normal_give_what_the_model_gives_for_every_seed() {
    generate::check(40, 2, seeds, |seeds| {
        let got = floats_printed(&printed(&drawing(seeds)));
        let want: Vec<Vec<f32>> = seeds
            .iter()
            .map(|&seed| vec![uniform(seeded(seed)), standard_normal(seeded(seed))])
            .collect();
        assert!(same_floats(&got, &want), "{got:?} is not {want:?} for {seeds:?}");
    });
}

/// Where the top 24 bits are 0, 1, or the greatest two, `uniform` is
/// `n / 2^24`, which is at least 0 and below 1; and `standard_normal` is what
/// the model gives, which is 0 where the first uniform is 1 and at most the
/// `sqrt(-2 ln 2^-24)`, about 5.77, that `random.md` says ends the tail.
fn uniform_is_at_least_0_and_below_1_where_the_bits_are_at_their_edges() {
    let seeds: Vec<u64> = EDGES.iter().map(|&(seed, _)| seed).collect();
    let got = floats_printed(&printed(&drawing(&seeds)));
    for (&(seed, n), line) in EDGES.iter().zip(&got) {
        assert_eq!(bits(seeded(seed)) >> 40, n, "the search found {seed} for {n}");
        let u = line[0];
        assert!((0.0..1.0).contains(&u), "{u} for {seed}");
        assert_eq!(u, n as f32 / 16_777_216.0, "for {seed}");
        let z = line[1];
        assert_eq!(z.to_bits(), standard_normal(seeded(seed)).to_bits(), "{z} for {seed}");
        assert!(z.abs() <= 5.78, "{z} for {seed}");
    }
    assert_eq!(got[6][0], 1.0 - 1.0 / 16_777_216.0);
    assert_eq!(got[6][1].abs(), 0.0, "the first uniform is 1 here");
}

/// Every executable of a chip that builds floats prints, for the seeds and
/// the edges, the floats the evaluator prints. The seeds are fixed, as what a
/// test of the machine builds is the same in every run.
fn every_executable_draws_the_floats_the_evaluator_draws_from_a_key() {
    let mut seeds: Vec<u64> = generate::sample(0, 0, seeds);
    seeds.extend(EDGES.iter().map(|&(seed, _)| seed));
    let source = drawing(&seeds);
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?}\n{source}"));
    let outcome = scalar::run(&program);
    for target in targets::runnable().into_iter().filter(|t| t.builds_floats()) {
        targets::runs_as(source.as_bytes(), target, &outcome);
    }
}

/// How many draws of each the test of the mean and the spread makes.
const DRAWS: usize = 10_000;

/// Over `DRAWS` keys split one after another from one seed's, the mean and the
/// variance of `uniform` and of `standard_normal` are those of the uniform
/// distribution on 0 to 1 and of the normal distribution with mean 0 and
/// deviation 1, on every executable of a chip that builds floats. The draws
/// run there, as the exact `ln` and `cos` take the evaluator far more steps
/// than a test is given; the test above holds each executable to the
/// evaluator.
///
/// The bound on each statistic is 5 of its standard errors for `DRAWS` draws,
/// which a right draw passes but for about once in 1.7 million, and which a
/// draw whose mean is off by 0.015 for `uniform` or 0.05 for
/// `standard_normal`, or whose variance is off by 4.5% or 7%, fails. The
/// standard error of a mean is `sqrt(var / n)`, and of a variance
/// `sqrt((m4 - var^2) / n)`, where `m4` is the fourth central moment: for the
/// uniform distribution `var` is 1/12 and `m4` is 1/80, and for the normal one
/// `var` is 1 and `m4` is 3. What `uniform` gives is `n / 2^24` and what
/// `standard_normal` gives ends at about 5.77, which move each of these by
/// less than 10^-6. The floats are summed in `f64` here, so that rounding
/// adds nothing the bound must hold. The seed is fixed, as what a test of the
/// machine builds is the same in every run.
fn draws_from_many_keys_have_the_mean_and_spread_of_their_distribution_on_every_executable() {
    let source = format!(
        "{FLOATS}fn main() <console, alloc> {{\n  mut key = seeded(918)\n  loop i in 0..{DRAWS} {{\n    \
         (rest, here) = split(take key)\n    (one, other) = split(take here)\n    key = rest\n    \
         print(\"{{uniform(take one)}} {{standard_normal(take other)}}\")\n  }}\n}}\n"
    );
    for target in targets::runnable().into_iter().filter(|t| t.builds_floats()) {
        let bytes = targets::build(source.as_bytes(), target).unwrap_or_else(|e| panic!("{e:?}"));
        targets::record(target, &[&bytes]);
        if targets::fingerprinting() {
            continue;
        }
        let ran = targets::run(&bytes);
        assert_eq!(ran.status, 0, "{}", target.name());
        let got = floats_printed(&String::from_utf8(ran.stdout).unwrap());
        assert_eq!(got.len(), DRAWS, "{}", target.name());
        let n = DRAWS as f64;
        let moments = |column: usize| -> (f64, f64) {
            let mean = got.iter().map(|l| f64::from(l[column])).sum::<f64>() / n;
            let var = (got.iter())
                .map(|l| (f64::from(l[column]) - mean).powi(2))
                .sum::<f64>()
                / (n - 1.0);
            (mean, var)
        };
        for (column, name, mean, var, m4) in [
            (0, "uniform", 0.5, 1.0 / 12.0, 1.0 / 80.0),
            (1, "standard_normal", 0.0, 1.0, 3.0),
        ] {
            let (got_mean, got_var) = moments(column);
            let mean_bound = 5.0 * (var / n).sqrt();
            let var_bound = 5.0 * ((m4 - var * var) / n).sqrt();
            assert!(
                (got_mean - mean).abs() <= mean_bound,
                "{}: the mean of {name} is {got_mean}, not within {mean_bound} of {mean}",
                target.name()
            );
            assert!(
                (got_var - var).abs() <= var_bound,
                "{}: the variance of {name} is {got_var}, not within {var_bound} of {var}",
                target.name()
            );
        }
    }
}

/// A program that prints the bits of a fresh key twice, by two keys.
const FRESH: &str = "use std.bits (u64)\nuse std.console (console, print)\nuse std.entropy (entropy)\nuse std.random (bits, fresh)\n\n\
fn main() <console, entropy, alloc> {\n  print(\"{bits(fresh())} {bits(fresh())}\")\n}\n";

/// For every target this machine runs, a fresh key is not the same twice in one
/// run, and not the same in two runs.
fn every_executable_for_this_machine_that_draws_from_a_fresh_key_prints_different_bits_each_run() {
    for target in targets::runnable().into_iter().filter(|t| t.is_machine_code()) {
        let bytes = targets::build(FRESH.as_bytes(), target).unwrap_or_else(|e| panic!("{e:?}"));
        targets::record(target, &[&bytes]);
        if targets::fingerprinting() {
            continue;
        }
        let said = |ran: scalar::Outcome| String::from_utf8(ran.stdout).unwrap();
        let (first, second) = (said(targets::run(&bytes)), said(targets::run(&bytes)));
        for line in [&first, &second] {
            let numbers: Vec<&str> = line.split_whitespace().collect();
            assert_eq!(numbers.len(), 2, "{}: {line:?}", target.name());
            assert_ne!(numbers[0], numbers[1], "{}", target.name());
        }
        assert_ne!(first, second, "{}", target.name());
    }
}
