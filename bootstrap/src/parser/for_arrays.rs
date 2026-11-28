//! Parses `for i { e }`, the array whose element at each index `i` is what
//! the block gives, with its node of the syntax tree. A `for` over several
//! indices is not built.

use super::*;

/// `for i { e }`: the array whose element at the index `i` is what the block
/// gives for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct For {
    /// Where `for` is.
    pub pos: Pos,
    /// The `loop` over the index that builds the array, `loop i in 0..0`
    /// with the block of the `for`. Its end is the size of the index, which
    /// the checker finds, so the `0` written for it is not read.
    pub round: Loop,
}

impl For {
    /// The name of the index, and where it is.
    pub fn index(&self) -> (&str, Pos) {
        let r = self.round.range.as_ref().expect("a `for` has an index");
        (&r.name, r.pos)
    }
}

impl Parser<'_> {
    /// Parses `for`, through the block that ends it.
    pub(super) fn for_expr(&mut self) -> Result<For, Error> {
        let pos = self.pos();
        self.bump();
        let (name, at) = self.ident("the name of an index")?;
        if self.tok() == &Tok::Punct(",") {
            return Err(Error::unsupported(pos, "a `for` over several indices"));
        }
        self.expect_punct("{", "`{` to open the block of the `for`")?;
        let (body, end) = self.inner_block(Vec::new())?;
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
        Ok(For { pos, round })
    }
}
