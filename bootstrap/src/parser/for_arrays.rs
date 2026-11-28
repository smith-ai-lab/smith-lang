//! Parses `for i { e }`, the array whose element at each index `i` is what
//! the block gives, and `for i, j { e }`, the matrix whose element at each
//! pair of indices is, with its node of the syntax tree.

use super::*;

/// `for i { e }` or `for i, j { e }`: the array, or the matrix, whose element
/// at the indices is what the block gives for them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct For {
    /// Where `for` is.
    pub pos: Pos,
    /// The name of each index, first the one of the first axis, and where it
    /// is.
    pub indices: Vec<(String, Pos)>,
    /// The `loop` that builds the array, `loop i in 0..0` with the block of
    /// the `for`, where `i` is the first index. It counts every element, and
    /// its end, which the checker finds from the sizes of the indices, is not
    /// read, so the `0` written for it is not either.
    pub round: Loop,
}

impl Parser<'_> {
    /// Parses `for`, through the block that ends it.
    pub(super) fn for_expr(&mut self) -> Result<For, Error> {
        let pos = self.pos();
        self.bump();
        let mut indices = vec![self.ident("the name of an index")?];
        while self.tok() == &Tok::Punct(",") {
            self.bump();
            indices.push(self.ident("the name of an index")?);
        }
        self.expect_punct("{", "`{` to open the block of the `for`")?;
        let (body, end) = self.inner_block(Vec::new())?;
        let (name, at) = indices[0].clone();
        let zero = || Expr::Number {
            text: "0".to_string(),
            magnitude: 0,
            negative: false,
            pos: at,
        };
        let round = Loop {
            pos,
            range: Some(Range {
                name,
                pos: at,
                from: zero(),
                to: zero(),
            }),
            over: None,
            decreases: None,
            invariants: Vec::new(),
            body,
            end,
        };
        Ok(For {
            pos,
            indices,
            round,
        })
    }
}
