//! A program that is lowered once and run with other numbers in it.
//!
//! Lowering a program costs the same whatever numbers it holds, as the standard
//! library it reaches is most of it, so a test that runs one text with many
//! rounds, counts, or seeds writes it once with `ROUNDS` and `SEED` where those
//! are read, lowers it once, and writes in the numbers of each run. What each run
//! gives is what lowering the text with the numbers written in would give.

use smith::lower;
use smith::scalar;

/// The number of rounds, or any other count, that a program written once stands
/// for, and the seed it stands for. They are not any number a program of the
/// tests holds, so that each is told apart from every other number in it.
pub const ROUNDS: u64 = 999_983;
pub const SEED: u64 = 999_979;

/// The text of a program, and what it lowers to.
pub struct Written {
    pub source: String,
    pub program: scalar::Program,
}

impl Written {
    /// Lowers the text, which is written with `ROUNDS` and `SEED`, or numbers
    /// of its own, where the numbers of a run are read.
    pub fn new(source: String) -> Written {
        let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for {source}"));
        assert_eq!(scalar::check(&program), Ok(()), "for {source}");
        Written { source, program }
    }

    /// The program with each number of `changes`, as the one it stands for and
    /// the one it is written as, read where the first was. A number that is read
    /// nowhere is a mistake of the test, and fails it.
    pub fn with(&self, changes: &[(u64, u64)]) -> scalar::Program {
        use scalar::{Constant, Op};
        let mut program = self.program.clone();
        let mut written = vec![0; changes.len()];
        for f in &mut program.functions {
            for b in &mut f.blocks {
                for i in &mut b.instructions {
                    if let Op::Constant(Constant::Int(n)) = &mut i.op {
                        for (k, (from, to)) in changes.iter().enumerate() {
                            if n == from {
                                *n = *to;
                                written[k] += 1;
                            }
                        }
                    }
                }
            }
        }
        assert!(
            written.iter().all(|&n| n > 0),
            "{changes:?} are read in {written:?} places of\n{}",
            self.source
        );
        assert_eq!(scalar::check(&program), Ok(()), "for {}", self.source);
        program
    }
}
