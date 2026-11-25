//! A block with a `try` that is passed to a call whose result nothing settles
//! gets the same look as one whose result is partly known, so the `try` is
//! refused at the `try`, wherever the block is written, with the type the block
//! gives, and the checker never panics on it.

use smith::diag::Error;
use smith::lower;

use crate::Test;

pub const ONCE: &[Test] = &[
    (
        "a_try_in_a_block_whose_result_nothing_settles_is_refused_at_the_try_for_every_opener_and_place",
        a_try_in_a_block_whose_result_nothing_settles_is_refused_at_the_try_for_every_opener_and_place,
    ),
];

/// A call that takes the block, as written where it is called, with the
/// `use` line it needs and the effects of the function it is written in.
struct Opener {
    uses: &'static str,
    call: &'static str,
    effects: &'static str,
}

const OPENERS: &[Opener] = &[
    Opener {
        uses: "",
        call: "around()",
        effects: "",
    },
    Opener {
        uses: "use std.task (tasks)\n",
        call: "tasks()",
        effects: "",
    },
    Opener {
        uses: "use std.alloc (arena)\n",
        call: "arena()",
        effects: "",
    },
];

/// Where the block is written.
#[derive(Clone, Copy, Debug)]
enum Place {
    /// `r = call { ... }`, a statement of the function.
    Statement,
    /// `with call`, in a `loop`.
    WithInLoop,
    /// `call { ... }`, in a `loop`.
    BlockInLoop,
}

const PLACES: &[Place] = &[
    Place::Statement,
    Place::WithInLoop,
    Place::BlockInLoop,
];

const HEAD: &str = "use std.bits (u64)\n";

const FUNCTIONS: &str = "fn around[T](body: fn() -> T) -> T { body() }\n\n\
                         fn step(n: u64) -> Result[u64, str] { .ok(n) }\n\n";

/// The program and the line of its `try`, which the generator that writes the
/// lines knows.
fn program(opener: &Opener, place: Place) -> (String, usize) {
    let Opener { uses, call, effects } = opener;
    let lines: Vec<String> = match place {
        Place::Statement => vec![
            format!("  r = {call} {{"),
            "    x = try step(i)".into(),
            "    x".into(),
            "  }".into(),
            "  .ok(0)".into(),
        ],
        Place::WithInLoop => vec![
            "  loop j in 0..3 {".into(),
            format!("    with {call}"),
            "    x = try step(i)".into(),
            "    x".into(),
            "  }".into(),
            "  .ok(0)".into(),
        ],
        Place::BlockInLoop => vec![
            "  loop j in 0..3 {".into(),
            format!("    {call} {{"),
            "      x = try step(i)".into(),
            "      x".into(),
            "    }".into(),
            "  }".into(),
            "  .ok(0)".into(),
        ],
    };
    let header = format!("{HEAD}{uses}\n{FUNCTIONS}fn rounds(i: u64) -> Result[u64, str]{effects} {{\n");
    let try_line = header.lines().count() + lines.iter().position(|l| l.contains("try")).unwrap() + 1;
    let source = format!("{header}{}\n}}\n\nfn main() {{\n  r = rounds(1)\n}}\n", lines.join("\n"));
    (source, try_line)
}

fn errors(source: &str) -> Vec<Error> {
    lower(source.as_bytes()).expect_err("an error")
}

fn refused_at_the_try(source: &str, try_line: usize) {
    let found = errors(source);
    assert!(
        found.iter().any(|e| e.pos.line == try_line
                && e.message.contains("this block gives `u64`")
                && e.message.contains("`try` cannot return a failure")),
        "{found:?} for\n{source}"
    );
}

fn a_try_in_a_block_whose_result_nothing_settles_is_refused_at_the_try_for_every_opener_and_place() {
    for opener in OPENERS {
        for place in PLACES {
            let (source, try_line) = program(opener, *place);
            refused_at_the_try(&source, try_line);
        }
    }
}
