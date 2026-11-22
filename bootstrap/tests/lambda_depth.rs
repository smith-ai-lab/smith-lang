//! A block passed to a call that does not settle its result is looked at to
//! find what it gives, so that a `try` in it knows what the block leaves with.
//! Blocks that hold one another are looked at once each, so the time to check
//! a nest grows with its depth and not with a power of it.

use smith::check;

use crate::Test;

pub const ONCE: &[Test] = &[(
    "blocks_nested_forty_deep_are_checked_within_the_limit_of_a_test_with_a_try_and_without",
    blocks_nested_forty_deep_are_checked_within_the_limit_of_a_test_with_a_try_and_without,
)];

/// `run` gives what its block gives, whatever `T` is, which no call settles.
const RUN: &str = "use std.bits (i64)

fn run[T](body: fn() -> Result[T, str]) -> Result[T, str] { body() }

fn step() -> Result[i64, str] { .ok(1) }

";

/// A function that nests `depth` blocks, each in the call of `run` before it,
/// and, where `with_try`, a `try` in each.
fn nested(depth: usize, with_try: bool) -> String {
    let mut source = String::from(RUN);
    source.push_str("fn go() -> Result[i64, str] {\n");
    for i in 0..depth {
        source.push_str("  run({\n");
        if with_try {
            source.push_str(&format!("    a{i} = try step()\n"));
        }
    }
    source.push_str(if with_try {
        "    Result.ok(a0)\n"
    } else {
        "    step()\n"
    });
    source.push_str(&"  })\n".repeat(depth));
    source.push_str("}\n\nfn main() {\n  match go() {\n    .ok(x) { }\n    .err(e) { }\n  }\n}\n");
    source
}

fn blocks_nested_forty_deep_are_checked_within_the_limit_of_a_test_with_a_try_and_without() {
    for with_try in [false, true] {
        let source = nested(40, with_try);
        // What is said is of names that nothing reads, and no mistake.
        if let Err(errors) = check(source.as_bytes()) {
            for e in &errors {
                assert!(e.message.contains("never read"), "{e:?} for\n{source}");
            }
        }
    }
}
