//! Parses functions, calls, and lambdas: a function with its signature and
//! parameters, a function type, a call with its arguments, and a lambda.

use super::*;

impl Parser<'_> {
    /// Parses a function that starts at the token `from`. The errors of its
    /// statements are recorded, and a function with any gives `None`.
    ///
    /// A mistake in the signature of a function inside an implementation
    /// is given back, for the implementation to report. Elsewhere it is
    /// recorded, and the function is skipped.
    pub(super) fn function(
        &mut self,
        from: usize,
        nested: bool,
    ) -> Result<Option<Function>, Error> {
        let (
            name,
            pos,
            generics,
            params,
            result,
            mut result_name,
            effects,
            requires,
            ensures,
            decreases,
            notes,
        ) = match self.body_signature() {
            Ok(signature) => signature,
            Err(e) if nested => return Err(e),
            Err(e) => {
                self.errors.push(e);
                self.skip_item(from);
                return Ok(None);
            }
        };
        let mut block = Block::default();
        block.inner.append(&mut self.binds);
        let mut ok = true;
        let mut requires = requires;
        let mut ensures = ensures;
        let owed = self.bounds_owed(&params);
        ok &= owed.is_some();
        if let Some(mut owed) = owed {
            owed.append(&mut requires);
            requires = owed;
        }
        let given = self.result_owed(&params, result.as_ref(), &mut result_name);
        ok &= given.is_some();
        if let Some(mut given) = given {
            given.append(&mut ensures);
            ensures = given;
        }
        let end = loop {
            self.skip_newlines();
            let start = self.at;
            match self.tok() {
                Tok::Punct("}") => {
                    let end = self.pos();
                    self.bump();
                    break end;
                }
                Tok::Eof => {
                    let error = self.expected("`}`", &Tok::Eof);
                    if nested {
                        return Err(error);
                    }
                    self.errors.push(error);
                    return Ok(None);
                }
                _ => match self.statement_into(&mut block) {
                    Ok(()) => {}
                    Err(e) if e.unbuilt => {
                        if !self.read_past_statement(start, e, &mut block) {
                            return Ok(None);
                        }
                    }
                    Err(e) => {
                        self.errors.push(e);
                        ok = false;
                        if !self.skip_statement(start) {
                            return Ok(None);
                        }
                    }
                },
            }
        };
        match self.tok() {
            Tok::Newline | Tok::Eof => {}
            Tok::Punct("}") if nested => {}
            tok => {
                let error = self.expected("a line break", tok);
                if nested {
                    return Err(error);
                }
                self.errors.push(error);
                let at = self.at;
                self.skip_item(at);
                return Ok(None);
            }
        }
        Ok(ok.then_some(Function {
            name,
            pos,
            generics,
            params,
            result,
            result_name,
            effects,
            requires,
            needs: Vec::new(),
            notes,
            ensures,
            decreases,
            body: block.finish(end),
            end,
        }))
    }

    /// Parses a function's signature, through the `{` that opens its body.
    pub(super) fn body_signature(&mut self) -> Result<BodySignature, Error> {
        self.bound_names_allowed = true;
        let signature = self.signature();
        self.bound_names_allowed = false;
        let (name, pos, generics, params, result, result_name, effects) = signature?;
        let (mut requires, mut ensures) = (Vec::new(), Vec::new());
        let mut decreases: Vec<Expr> = Vec::new();
        let mut notes = Vec::new();
        loop {
            self.skip_newlines();
            let keyword = match self.tok() {
                Tok::Keyword(k @ ("requires" | "ensures")) => *k,
                Tok::Ident(name) if name == "decreases" => "decreases",
                _ => break,
            };
            let at = self.pos();
            let note = self.note();
            self.bump();
            let no_lambda = std::mem::replace(&mut self.no_lambda, true);
            let statement = self.expr();
            self.no_lambda = no_lambda;
            match keyword {
                "requires" => {
                    let statement = statement?;
                    if let Some(note) = note {
                        notes.push((statement.pos(), note));
                    }
                    requires.push(statement);
                }
                "decreases" => {
                    if !decreases.is_empty() {
                        return Err(Error::new(
                            at,
                            "a function has one `decreases`, which may name more than one part, as `decreases a, b`",
                        ));
                    }
                    decreases.push(statement?);
                    // The parts after the first, each after a comma.
                    while self.tok() == &Tok::Punct(",") {
                        self.bump();
                        self.skip_newlines();
                        let no_lambda = std::mem::replace(&mut self.no_lambda, true);
                        let part = self.expr();
                        self.no_lambda = no_lambda;
                        decreases.push(part?);
                    }
                }
                _ => ensures.push(statement?),
            }
        }
        match self.tok() {
            Tok::Punct("{") => self.bump(),
            tok => return Err(self.expected("`{` to open the function's body", tok)),
        }
        // What a proof parameter says is owed at every call, as a `requires`
        // says it, so the statement of each is one before the others.
        let owed: Vec<Expr> = (params.iter())
            .filter_map(|p| p.ty.statement.as_deref().cloned())
            .collect();
        requires.splice(0..0, owed);
        Ok((
            name,
            pos,
            generics,
            params,
            result,
            result_name,
            effects,
            requires,
            ensures,
            decreases,
            notes,
        ))
    }

    /// The comment above the `requires` read next, if it has one.
    pub(super) fn note(&self) -> Option<String> {
        self.tokens[self.at].note.as_deref().map(str::to_string)
    }

    /// Parses a function's signature, through its list of effects.
    pub(super) fn signature(&mut self) -> Result<Signature, Error> {
        self.binds.clear();
        self.bump();
        let (name, pos) = self.defined("the function's name")?;
        let generics = self.generics()?;
        self.expect_punct("(", "`(`")?;
        let mut params: Vec<Param> = Vec::new();
        self.skip_newlines();
        // Whether a `;` has been read: the parameters after it are given by
        // name, and those before it by position.
        let mut by_name = false;
        if self.tok() == &Tok::Punct(";") {
            self.bump();
            self.skip_newlines();
            by_name = true;
            if self.tok() == &Tok::Punct(")") {
                return Err(self.after_semicolon());
            }
        }
        loop {
            match self.tok() {
                Tok::Punct(")") => break,
                Tok::Ident(_) => params.push(Param {
                    by_name,
                    ..self.param()?
                }),
                tok if starts_pattern(tok) => {
                    let at = self.pos();
                    if by_name {
                        return Err(Error::new(
                            at,
                            "a parameter written as a pattern has no name to give at a call, so it is written before the `;`",
                        )
                        .with_fix("write it before the `;`, or give it a name"));
                    }
                    params.push(self.param()?);
                }
                tok => return Err(self.expected("a parameter or `)`", tok)),
            }
            self.skip_newlines();
            match self.tok() {
                Tok::Punct(",") => {
                    self.bump();
                    self.skip_newlines();
                }
                Tok::Punct(";") if !by_name => {
                    self.bump();
                    self.skip_newlines();
                    by_name = true;
                    if self.tok() == &Tok::Punct(")") {
                        return Err(self.after_semicolon());
                    }
                }
                Tok::Punct(";") => {
                    return Err(Error::new(
                        self.pos(),
                        "a parameter list has one `;`, and the parameters after it are all given by name",
                    )
                    .with_fix("take out this `;`"));
                }
                Tok::Punct(")") => break,
                tok => return Err(self.expected("`,` or `)`", tok)),
            }
        }
        self.bump();
        let mut result = None;
        let mut result_name = None;
        if self.tok() == &Tok::Punct("->") {
            self.bump();
            if let Tok::Ident(n) = self.tok()
                && self.next_tok() == &Tok::Punct(":")
            {
                result_name = Some((n.to_string(), self.pos()));
                self.bump();
                self.bump();
            }
            if let Some(at) = self.effects_after_statement() {
                return Err(Error::new(
                    at,
                    "a function that gives a proof carries no effect, as a proof is erased and the code that gives it is never run",
                )
                .with_fix("take the effects off the function, and give the proof from steps of a proof"));
            }
            if let Some(statement) = self.statement_type() {
                result = Some(statement);
            } else {
                self.name_bound_ok = self.bound_names_allowed && self.tok() != &Tok::Keyword("fn");
                let ty = self.ty();
                self.name_bound_ok = false;
                result = Some(ty?);
            }
        }
        let effects = self.effects()?;
        self.skip_to_contract();
        Ok((name, pos, generics, params, result, result_name, effects))
    }

    /// The mistake of a `;` that no parameter follows.
    fn after_semicolon(&self) -> Error {
        Error::new(
            self.pos(),
            "a `;` is followed by the parameters that are given by name",
        )
        .with_fix("write those parameters after the `;`, or take it out")
    }

    /// Skips the line breaks, and the comments between them, that lie before a
    /// `requires` or an `ensures` on a line of its own.
    pub(super) fn skip_to_contract(&mut self) {
        let mut ahead = self.at;
        while self.tokens[ahead].tok == Tok::Newline {
            ahead += 1;
        }
        if ahead > self.at && matches!(self.tokens[ahead].tok, Tok::Keyword("requires" | "ensures"))
        {
            self.at = ahead;
        }
    }

    /// Reads the type of a parameter when it is a statement, which ends at the
    /// `,` or `)` that ends the parameter, or at the `=` of its default.
    pub(super) fn param_statement_type(&mut self) -> Option<Type> {
        let mut depth = 0usize;
        let mut found = false;
        for (i, token) in self.tokens[self.at..].iter().enumerate() {
            match &token.tok {
                Tok::Punct("<") if self.effects_at(self.at + i) => return None,
                Tok::Punct("(" | "[" | "{") => depth += 1,
                Tok::Punct(")" | "]" | "}") if depth == 0 => break,
                Tok::Punct(")" | "]" | "}") => depth -= 1,
                Tok::Punct("," | "=") if depth == 0 => break,
                Tok::Punct("==" | "!=" | "<" | "<=" | ">" | ">=")
                | Tok::Keyword("and" | "or" | "not") => found = true,
                Tok::Eof => break,
                _ => {}
            }
        }
        if !found {
            return None;
        }
        self.statement_type()
    }

    /// Reads a type that is a statement: a comparison of values, or comparisons
    /// joined with `and`, `or`, and `not`, as `a < 10` is. Gives none, and
    /// leaves what follows as it was, when what follows is not one, so that it
    /// is read as the type it is.
    pub(super) fn statement_type(&mut self) -> Option<Type> {
        if !self.comparison_ahead() {
            return None;
        }
        let (at, errors, unbuilt) = (self.at, self.errors.len(), self.unbuilt.len());
        let pos = self.pos();
        // The `{` after it opens the body, so no block follows a name in it.
        let no_lambda = std::mem::replace(&mut self.no_lambda, true);
        let read = self.expr();
        self.no_lambda = no_lambda;
        let statement = match read {
            Ok(e) if is_statement(&e) => Some(e),
            _ => None,
        };
        let Some(statement) = statement else {
            self.at = at;
            self.errors.truncate(errors);
            self.unbuilt.truncate(unbuilt);
            return None;
        };
        Some(Type {
            name: STATEMENT.to_string(),
            pos,
            args: Vec::new(),
            function: false,
            range: None,
            statement: Some(Box::new(statement)),
            sizes: Vec::new(),
        })
    }

    /// Where the effects are written after a result that is a statement, as in
    /// `-> a > 0 <console>`, which a proof does not carry.
    fn effects_after_statement(&self) -> Option<Pos> {
        let mut compared = false;
        for (i, token) in self.tokens[self.at..].iter().enumerate() {
            match &token.tok {
                Tok::Punct("<") if compared && self.effects_at(self.at + i) => {
                    return Some(token.pos);
                }
                Tok::Punct("==" | "!=" | "<" | "<=" | ">" | ">=")
                | Tok::Keyword("and" | "or" | "not") => compared = true,
                Tok::Punct("{")
                | Tok::Keyword("requires" | "ensures")
                | Tok::Newline
                | Tok::Eof => return None,
                _ => {}
            }
        }
        None
    }

    /// Whether a comparison, or `and`, `or`, or `not`, is written before the
    /// `{` or the line break that ends what follows, which only a statement
    /// has: no type is written with one.
    fn comparison_ahead(&self) -> bool {
        for (i, token) in self.tokens[self.at..].iter().enumerate() {
            match &token.tok {
                Tok::Punct("<") if self.effects_at(self.at + i) => return false,
                Tok::Punct("==" | "!=" | "<" | "<=" | ">" | ">=")
                | Tok::Keyword("and" | "or" | "not") => return true,
                Tok::Punct("{")
                | Tok::Keyword("requires" | "ensures")
                | Tok::Newline
                | Tok::Eof => return false,
                _ => {}
            }
        }
        false
    }

    /// Whether the `<` at token `at` opens a list of effects: names between
    /// commas, then `>`. A comparison has no such form, as `>` does not
    /// follow a comparison.
    fn effects_at(&self, at: usize) -> bool {
        let mut i = at + 1;
        loop {
            match self.tokens.get(i).map(|t| &t.tok) {
                Some(Tok::Ident(_)) => i += 1,
                _ => return false,
            }
            match self.tokens.get(i).map(|t| &t.tok) {
                Some(Tok::Punct(">")) => return true,
                Some(Tok::Punct(",")) => {
                    i += 1;
                    if self.tokens.get(i).map(|t| &t.tok) == Some(&Tok::Punct(">")) {
                        return true;
                    }
                }
                _ => return false,
            }
        }
    }

    /// Parses `name: type`, and `= default` after it if it has one.
    pub(super) fn param(&mut self) -> Result<Param, Error> {
        let pattern = !matches!(self.tok(), Tok::Ident(_));
        let (name, pos, pattern) = if pattern {
            let pattern = self.binder()?;
            let (name, pos) = hidden(&pattern);
            (name, pos, Some(pattern))
        } else {
            let (name, pos) = self.defined("a parameter")?;
            (name, pos, None)
        };
        self.expect_punct(":", "`:` and the parameter's type")?;
        let mutable = self.tok() == &Tok::Keyword("mut");
        if mutable {
            self.bump();
        }
        let take = !mutable && self.taking();
        // A function with a body takes a proof; a signature that has none, as a
        // trait's or an effect's, does not.
        let proof = if self.bound_names_allowed {
            self.param_statement_type()
        } else {
            None
        };
        let (ty, takes, effects) = match proof {
            Some(statement) => (statement, Vec::new(), Vec::new()),
            None => {
                self.name_bound_ok = self.bound_names_allowed && self.tok() != &Tok::Keyword("fn");
                let read = self.ty_taking();
                self.name_bound_ok = false;
                read?
            }
        };
        let mut default = None;
        if self.tok() == &Tok::Punct("=") {
            if pattern.is_some() {
                return Err(Error::new(
                    self.pos(),
                    "a parameter written as a pattern has no name to leave out at a call, so it takes no default",
                )
                .with_fix("give the parameter a name and take it apart in the body"));
            }
            self.bump();
            self.skip_newlines();
            let at = self.pos();
            let value = self.default()?;
            if ty.statement.is_some()
                && !matches!(&value, Expr::Name { name, .. } if name == "arith")
            {
                return Err(
                    Error::new(at, "the one default a proof parameter has is `arith`")
                        .with_fix("write `= arith`, or leave the default out and pass the proof"),
                );
            }
            default = Some(value);
        }
        if let Some(pattern) = pattern {
            self.binds.push(taken_apart(pattern, &name, pos));
        }
        Ok(Param {
            name,
            pos,
            ty,
            default,
            take,
            mutable,
            takes,
            effects,
            by_name: false,
        })
    }

    /// Parses a function type, from its `fn`: the types of its parameters in
    /// parentheses, each marked `take` if the function owns it, then
    /// `-> R` if it gives a result, and the effects it carries. It gives
    /// whether each parameter is marked `take`, and the effects it carries,
    /// which only the type of a parameter keeps: the effects of any other
    /// function type are not built.
    pub(super) fn function_type(
        &mut self,
        kept: bool,
    ) -> Result<(Type, Vec<bool>, Effects), Error> {
        let pos = self.pos();
        self.bump();
        if self.tok() != &Tok::Punct("(") {
            return Err(self.expected("`(` and the function type's parameters", self.tok()));
        }
        let marked = self.list(|p| {
            if p.tok() == &Tok::Keyword("mut") {
                p.unbuilt
                    .push(Error::unsupported(p.pos(), "`mut` in a function type"));
                p.bump();
            }
            let take = p.taking();
            Ok((take, p.ty()?))
        })?;
        let (takes, mut args): (Vec<bool>, Vec<Type>) = marked.into_iter().unzip();
        if self.tok() == &Tok::Punct("->") {
            self.bump();
            args.push(self.ty()?);
        } else {
            args.push(Type {
                name: String::new(),
                pos,
                args: Vec::new(),
                function: false,
                range: None,
                statement: None,
                sizes: Vec::new(),
            });
        }
        let open = self.pos();
        let mut effects = self.effects()?;
        if !kept && !effects.is_empty() {
            self.unbuilt
                .push(Error::unsupported(open, "an effect on a function type"));
            effects.clear();
        }
        let function = Type {
            name: String::new(),
            pos,
            args,
            function: true,
            range: None,
            statement: None,
            sizes: Vec::new(),
        };
        Ok((function, takes, effects))
    }

    /// Parses what follows a name, which is at `pos` and after the name
    /// before the `.` if one is written: a call, if parentheses or a lambda
    /// follow it, and otherwise the name, or the field of the name before
    /// the `.`.
    pub(super) fn call_or_name(
        &mut self,
        qualifier: Option<(String, Pos)>,
        name: String,
        pos: Pos,
    ) -> Result<Expr, Error> {
        if self.tok() == &Tok::Punct("(") {
            return Ok(Expr::Call(self.call_rest(qualifier, name, pos)?));
        }
        if let Some(lambda) = self.lambda_after()? {
            let end = lambda.value.pos();
            return Ok(Expr::Call(Call {
                through: None,
                pos: qualifier.as_ref().map_or(pos, |(_, at)| *at),
                qualifier,
                type_args: Vec::new(),
                callee: name,
                name_pos: pos,
                args: vec![lambda],
                end,
            }));
        }
        Ok(match qualifier {
            None => Expr::Name { name, pos },
            Some((q, at)) => Expr::Field {
                of: Box::new(Expr::Name { name: q, pos: at }),
                name,
                pos,
            },
        })
    }

    /// Parses a lambda written after a call, if one is, as the call's last
    /// argument.
    pub(super) fn lambda_after(&mut self) -> Result<Option<Arg>, Error> {
        if self.no_lambda || !matches!(self.tok(), Tok::Punct("{") | Tok::Keyword("fn")) {
            return Ok(None);
        }
        Ok(Some(Arg {
            name: None,
            value: Expr::Lambda(Box::new(self.lambda()?)),
            after: true,
            mutable: None,
            given_back: false,
            keeps: false,
            take: None,
            written: false,
        }))
    }

    /// Parses a lambda, from its `fn` or its `{` through the `}` that closes
    /// it.
    pub(super) fn lambda(&mut self) -> Result<Lambda, Error> {
        let pos = self.pos();
        let mut params = Vec::new();
        let mut types = Vec::new();
        let mut result = None;
        let mut binds = Vec::new();
        let mut generics = Vec::new();
        if self.tok() == &Tok::Keyword("fn") {
            self.bump();
            generics = self.generics()?;
            self.expect_punct("(", "`(` and the lambda's parameters")?;
            self.skip_newlines();
            if self.tok() == &Tok::Punct(")") {
                return Err(Error::new(
                    pos,
                    "a lambda that takes no parameters is a block alone, with no `fn()`",
                )
                .with_fix("leave out `fn()`"));
            }
            loop {
                if starts_pattern(self.tok()) {
                    let pattern = self.binder()?;
                    let (name, pos) = hidden(&pattern);
                    binds.push(taken_apart(pattern, &name, pos));
                    params.push((name, pos));
                } else {
                    params.push(self.defined("a parameter's name")?);
                }
                types.push(if self.tok() == &Tok::Punct(":") {
                    self.bump();
                    Some(self.ty()?)
                } else {
                    None
                });
                self.skip_newlines();
                match self.tok() {
                    Tok::Punct(",") => {
                        self.bump();
                        self.skip_newlines();
                    }
                    Tok::Punct(")") => break,
                    tok => return Err(self.expected("`,` or `)`", tok)),
                }
            }
            self.bump();
            if self.tok() == &Tok::Punct("->") {
                self.bump();
                result = Some(self.ty()?);
            }
        }
        self.expect_punct("{", "`{` to open the lambda's body")?;
        let no_lambda = std::mem::replace(&mut self.no_lambda, false);
        let body = self.block_rest(binds);
        self.no_lambda = no_lambda;
        let (body, end) = body?;
        Ok(Lambda {
            pos,
            generics,
            params,
            types,
            result,
            body,
            end,
        })
    }

    /// Parses the arguments of a call, from its `(`.
    pub(super) fn call_rest(
        &mut self,
        qualifier: Option<(String, Pos)>,
        callee: String,
        name_pos: Pos,
    ) -> Result<Call, Error> {
        let pos = qualifier.as_ref().map_or(name_pos, |(_, at)| *at);
        self.bump();
        let mut args = Vec::new();
        self.skip_newlines();
        let no_lambda = std::mem::replace(&mut self.no_lambda, false);
        let inside = self.args(&mut args);
        self.no_lambda = no_lambda;
        inside?;
        let end = self.pos();
        self.bump();
        args.extend(self.lambda_after()?);
        Ok(Call {
            through: None,
            qualifier,
            type_args: Vec::new(),
            callee,
            pos,
            name_pos,
            args,
            end,
        })
    }

    /// Parses the arguments inside the parentheses of a call, up to the
    /// `)`.
    pub(super) fn args(&mut self, args: &mut Vec<Arg>) -> Result<(), Error> {
        loop {
            if self.tok() == &Tok::Punct(")") {
                break;
            }
            args.push(self.arg()?);
            self.skip_newlines();
            match self.tok() {
                Tok::Punct(",") => {
                    self.bump();
                    self.skip_newlines();
                }
                Tok::Punct(")") => break,
                tok => return Err(self.expected("`,` or `)`", tok)),
            }
        }
        Ok(())
    }

    pub(super) fn arg(&mut self) -> Result<Arg, Error> {
        let mut name = None;
        if let Tok::Ident(n) = self.tok()
            && self.next_tok() == &Tok::Punct(":")
        {
            name = Some((n.clone(), self.pos()));
            self.bump();
            self.bump();
            self.skip_newlines();
        }
        let mutable = (self.tok() == &Tok::Keyword("mut")).then(|| self.pos());
        if mutable.is_some() {
            self.bump();
        }
        let take = (mutable.is_none() && self.tok() == &Tok::Keyword("take")).then(|| self.pos());
        if take.is_some() {
            self.bump();
        }
        let value = self.expr()?;
        Ok(Arg {
            name,
            value,
            after: false,
            mutable,
            given_back: false,
            keeps: false,
            take,
            written: true,
        })
    }
}

/// Whether an expression is a statement: a comparison of values, or one of
/// `and`, `or`, and `not`.
fn is_statement(e: &Expr) -> bool {
    match e {
        Expr::Match(m) => match m.form {
            Form::Not | Form::And | Form::Or => true,
            Form::Match => matches!(&m.of, Expr::Call(c)
                if c.qualifier.as_ref().is_some_and(|(q, _)| q == "Ord") && c.callee == "compare"),
            Form::If { .. } | Form::Try => false,
        },
        Expr::Call(c) => c.qualifier.as_ref().is_some_and(|(q, _)| q == "Eq") && c.callee == "eq",
        _ => false,
    }
}
