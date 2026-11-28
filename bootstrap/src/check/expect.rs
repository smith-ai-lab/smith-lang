//! `expect`, the check of a test, which `std.test` declares as a function that
//! takes the check as a lambda. Each call of it with the check written in
//! braces after it is written out in its place, before anything is checked, as
//! the Smith that a function over code would give: each side of a comparison is
//! worked out once, the comparison is made of what they were, and a failure is
//! a `Failed` that says where the call is, what the check is as written, and
//! what each side was, as `repr` writes it. The failure is built under a
//! handler of `alloc` of its own, so a test that lists no `alloc` can fail.
//!
//! Every other stage sees only that Smith, so nothing past here knows of
//! `expect`.

use crate::diag::{Error, Pos};
use crate::parser::{
    Arg, Arm, Call, Expr, Form, Function, Lambda, Loop, Match, Pattern, Stmt, Type,
};

/// What the file calls the items of `std.test` the written code names.
pub(super) struct Names {
    /// The names `expect` is written as, as the module lists it.
    pub(super) expect: Vec<String>,
    pub(super) failed: String,
    pub(super) sides: String,
}

/// Writes out each call of `expect` in the function. `path` is the file the
/// function is in, as the failure names it, and `source` is its text. `made`
/// counts the places given to what is written, which are past the end of the
/// line of the call, so that no two are alike.
pub(super) fn expand(
    f: &mut Function,
    names: &Names,
    path: &str,
    source: &str,
    made: &mut usize,
    errors: &mut Vec<Error>,
) {
    let mut writer = Writer {
        names,
        path,
        source,
        made,
        errors,
    };
    for stmt in &mut f.body {
        writer.stmt(stmt);
    }
}

fn is_expect(c: &Call, names: &Names) -> bool {
    names.expect.contains(&c.callee) && c.qualifier.is_none() && c.through.is_none()
}

/// The column past which the places of what is written are, beyond any line
/// a file holds.
const MADE: usize = 1 << 24;

struct Writer<'a> {
    names: &'a Names,
    path: &'a str,
    source: &'a str,
    made: &'a mut usize,
    errors: &'a mut Vec<Error>,
}

impl Writer<'_> {
    fn stmt(&mut self, stmt: &mut Stmt) {
        match stmt {
            Stmt::Define { value, .. } | Stmt::Destructure { value, .. } => self.expr(value),
            Stmt::Assign { at, value, .. } => {
                if let Some(at) = at {
                    self.expr(&mut at.0);
                    self.expr(&mut at.1);
                }
                self.expr(value);
            }
            Stmt::Expr(e) => self.expr(e),
        }
    }

    fn block(&mut self, body: &mut [Stmt]) {
        for stmt in body {
            self.stmt(stmt);
        }
    }

    fn expr(&mut self, e: &mut Expr) {
        if let Expr::Call(c) = e
            && is_expect(c, self.names)
        {
            if let Some(written) = self.written(c) {
                *e = written;
            }
            return;
        }
        match e {
            Expr::Unbuilt { .. }
            | Expr::Text { .. }
            | Expr::Number { .. }
            | Expr::Float { .. }
            | Expr::Name { .. }
            | Expr::Break { .. } => {}
            Expr::Interp { parts, .. } => {
                for part in parts {
                    if let crate::parser::Part::Expr(e) = part {
                        self.expr(e);
                    }
                }
            }
            Expr::Call(c) => {
                if let Some(through) = &mut c.through {
                    self.expr(through);
                }
                for a in &mut c.type_args {
                    self.expr(a);
                }
                for a in &mut c.args {
                    self.expr(&mut a.value);
                }
            }
            Expr::Field { of, .. } => self.expr(of),
            Expr::Case { payload, .. } => {
                if let Some(p) = payload {
                    self.expr(p);
                }
            }
            Expr::Match(m) => {
                self.expr(&mut m.of);
                for arm in &mut m.arms {
                    self.block(&mut arm.body);
                }
            }
            Expr::Arith { left, right, .. } => {
                self.expr(left);
                self.expr(right);
            }
            Expr::Tuple { items, .. } | Expr::List { items, .. } => {
                for item in items {
                    self.expr(item);
                }
            }
            Expr::Lambda(l) => self.block(&mut l.body),
            Expr::Index { of, index, .. } => {
                self.expr(of);
                self.expr(index);
            }
            Expr::Slice { of, from, to, .. } => {
                self.expr(of);
                for e in [from, to].into_iter().flatten() {
                    self.expr(e);
                }
            }
            Expr::Loop(l) => self.round(l),
            Expr::For(f) => self.round(&mut f.round),
            Expr::Return { value, .. } => {
                if let Some(v) = value {
                    self.expr(v);
                }
            }
        }
    }

    fn round(&mut self, l: &mut Loop) {
        if let Some(r) = &mut l.range {
            self.expr(&mut r.from);
            self.expr(&mut r.to);
        }
        if let Some(o) = &mut l.over {
            self.expr(&mut o.value);
        }
        if let Some(d) = &mut l.decreases {
            self.expr(d);
        }
        for i in &mut l.invariants {
            self.expr(i);
        }
        self.block(&mut l.body);
    }

    /// A place of its own on the line of `at`, for a part of what is written.
    fn place(&mut self, at: Pos) -> Pos {
        *self.made += 1;
        Pos {
            line: at.line,
            col: MADE + *self.made,
        }
    }

    /// The code a call of `expect` stands for, or `None`, with the error said,
    /// when the check is not written in braces after it as one expression.
    fn written(&mut self, call: &Call) -> Option<Expr> {
        let check = match call.args.as_slice() {
            [
                Arg {
                    value: Expr::Lambda(l),
                    name: None,
                    ..
                },
            ] if l.params.is_empty() => match l.body.as_slice() {
                [Stmt::Expr(check)] => Some((check.clone(), l)),
                _ => None,
            },
            _ => None,
        };
        let Some((mut check, lambda)) = check else {
            self.errors.push(
                Error::new(
                    call.pos,
                    "`expect` is given its check in braces after it, as one expression",
                )
                .with_fix("write `expect { a == b }`"),
            );
            return None;
        };
        let at = call.pos;
        let line = at.line % super::STEP;
        let mut bound: Vec<(String, Expr)> = Vec::new();
        let sides = match self.compared(&mut check) {
            Some(args) => {
                let mut shown = Vec::new();
                for (k, arg) in args.iter_mut().enumerate() {
                    let side = if in_place(&arg.value) {
                        arg.value.clone()
                    } else {
                        let name = format!("expect@{}@{}@{k}", line, at.col);
                        let pos = arg.value.pos();
                        let value = std::mem::replace(
                            &mut arg.value,
                            Expr::Name {
                                name: name.clone(),
                                pos,
                            },
                        );
                        bound.push((name.clone(), value));
                        Expr::Name { name, pos }
                    };
                    let pos = side.pos();
                    shown.push(self.call(None, "repr", pos, vec![plain(side)]));
                }
                let right = shown.pop().expect("a comparison has two sides");
                let left = shown.pop().expect("a comparison has two sides");
                let p = self.place(at);
                let sides = self.call(
                    None,
                    &self.names.sides.clone(),
                    p,
                    vec![named("left", left, p), named("right", right, p)],
                );
                let p = self.place(at);
                Expr::Case {
                    name: "some".to_string(),
                    pos: p,
                    payload: Some(Box::new(sides)),
                }
            }
            None => Expr::Case {
                name: "none".to_string(),
                pos: self.place(at),
                payload: None,
            },
        };
        let p = self.place(at);
        let text = |text: String| Expr::Text {
            text: text.into_bytes(),
            pos: p,
        };
        let number = |n: usize| Expr::Number {
            text: n.to_string(),
            magnitude: n as u128,
            negative: false,
            pos: p,
        };
        let failed = self.call(
            None,
            &self.names.failed.clone(),
            p,
            vec![
                named("path", text(self.path.to_string()), p),
                named("line", number(line), p),
                named("column", number(at.col), p),
                named("check", text(self.check_text(lambda)), p),
                named("sides", sides, p),
            ],
        );
        // The failure is made under a handler of `alloc` of its own, and is
        // built in the memory of the caller, as the value of the block.
        let p = self.place(at);
        let made = Expr::Lambda(Box::new(Lambda {
            pos: p,
            generics: Vec::new(),
            params: Vec::new(),
            types: Vec::new(),
            mutable: Vec::new(),
            result: None,
            body: vec![Stmt::Expr(failed)],
            end: p,
        }));
        let p = self.place(at);
        let failure = self.call(None, "alloc", p, vec![named("body", made, p)]);
        let err = self.case("err", at, failure);
        let p = self.place(at);
        let unit = Expr::Tuple {
            pos: p,
            items: Vec::new(),
        };
        let ok = self.case("ok", at, unit);
        // The check is given a name, and what it gives a type, which a `try`
        // before the call does not give.
        let holds = format!("expect@{line}@{}", at.col);
        let result = format!("{holds}@result");
        let p = self.place(at);
        let chosen = Expr::Match(Box::new(Match {
            pos: p,
            of: Expr::Name {
                name: holds.clone(),
                pos: p,
            },
            arms: vec![self.arm("true", at, ok), self.arm("false", at, err)],
            form: Form::If { otherwise: true },
            checked: None,
        }));
        let p = self.place(at);
        let ty = |name: &str, args: Vec<Type>| Type {
            name: name.to_string(),
            pos: p,
            args,
            function: false,
            range: None,
            statement: None,
            sizes: Vec::new(),
        };
        let body = vec![
            Stmt::Define {
                name: result.clone(),
                pos: p,
                ty: Some(ty(
                    "Result",
                    vec![ty("", Vec::new()), ty(&self.names.failed, Vec::new())],
                )),
                var: false,
                value: chosen,
            },
            Stmt::Expr(Expr::Name {
                name: result,
                pos: self.place(at),
            }),
        ];
        bound.push((holds, check));
        // Each side that is worked out is given a name first, the left one
        // outermost, so that the sides are worked out in the order written,
        // and the check is named last.
        let mut body = body;
        while let Some((name, value)) = bound.pop() {
            let p = self.place(at);
            let pos = value.pos();
            body = vec![Stmt::Expr(Expr::Match(Box::new(Match {
                pos,
                of: value,
                arms: vec![Arm {
                    pattern: Pattern::Bind {
                        name,
                        pos: p,
                        var: None,
                    },
                    body,
                    end: p,
                }],
                form: Form::Match,
                checked: None,
            })))];
        }
        match body.pop() {
            Some(Stmt::Expr(written)) => Some(written),
            _ => unreachable!("the check is always named"),
        }
    }

    /// A case of `Result` that holds the value, which the binding it is
    /// given to says the type of.
    fn case(&mut self, name: &str, at: Pos, value: Expr) -> Expr {
        Expr::Case {
            name: name.to_string(),
            pos: self.place(at),
            payload: Some(Box::new(value)),
        }
    }

    /// The arm of an `if` for `case`, which gives `value`.
    fn arm(&mut self, case: &str, at: Pos, value: Expr) -> Arm {
        let p = self.place(at);
        Arm {
            pattern: Pattern::Case {
                name: case.to_string(),
                pos: p,
                inner: None,
            },
            body: vec![Stmt::Expr(value)],
            end: p,
        }
    }

    fn call(&mut self, qualifier: Option<&str>, callee: &str, pos: Pos, args: Vec<Arg>) -> Expr {
        Expr::Call(Call {
            through: None,
            qualifier: qualifier.map(|q| (q.to_string(), pos)),
            type_args: Vec::new(),
            callee: callee.to_string(),
            pos,
            name_pos: pos,
            args,
            end: pos,
        })
    }

    /// The two arguments of the call a comparison is, when the check is a
    /// comparison written with one of the six operators.
    fn compared<'e>(&self, check: &'e mut Expr) -> Option<&'e mut Vec<Arg>> {
        // `a != b` is the `not` of `a == b`, and `a < b` a `match` on how
        // they stand; a `not` that is written is no comparison.
        let (call, ops): (_, &[&str]) = match check {
            Expr::Call(c) => (c, &["=="]),
            Expr::Match(m) => match (m.form, &mut m.of) {
                (Form::Not, Expr::Call(c)) => (c, &["!="]),
                (Form::Match, Expr::Call(c)) => (c, &["<", "<=", ">", ">="]),
                _ => return None,
            },
            _ => return None,
        };
        let called = match (call.qualifier.as_ref(), call.callee.as_str()) {
            (Some((q, _)), "eq") => q == "Eq",
            (Some((q, _)), "compare") => q == "Ord",
            _ => false,
        };
        let is = called && ops.contains(&self.at(call.name_pos)) && call.args.len() == 2;
        is.then_some(&mut call.args)
    }

    /// The operator written at the place, if one of the six comparisons is.
    fn at(&self, pos: Pos) -> &'static str {
        let line = pos.line % super::STEP;
        let Some(text) = self.source.split('\n').nth(line.wrapping_sub(1)) else {
            return "";
        };
        let rest: String = text.chars().skip(pos.col - 1).take(2).collect();
        ["==", "!=", "<=", ">=", "<", ">"]
            .into_iter()
            .find(|op| rest.starts_with(op))
            .unwrap_or("")
    }

    /// The check as it is written between the braces of the lambda, with its
    /// lines joined by a space.
    fn check_text(&self, lambda: &Lambda) -> String {
        let (from, to) = (lambda.pos, lambda.end);
        let (first, last) = (from.line % super::STEP, to.line % super::STEP);
        let mut parts = Vec::new();
        for (n, text) in self.source.split('\n').enumerate() {
            let n = n + 1;
            if n < first || n > last {
                continue;
            }
            let chars: Vec<char> = text.chars().collect();
            let start = if n == first { from.col } else { 0 };
            let end = if n == last {
                (to.col - 1).min(chars.len())
            } else {
                chars.len()
            };
            let part: String = chars[start.min(end)..end].iter().collect();
            let part = part.trim();
            if !part.is_empty() {
                parts.push(part.to_string());
            }
        }
        parts.join(" ")
    }
}

/// Whether a side is read again where it is shown, with nothing worked out:
/// a name, a field of one, a literal, or a case that holds nothing. Any other
/// side is worked out once and named.
fn in_place(e: &Expr) -> bool {
    match e {
        Expr::Name { .. } | Expr::Text { .. } | Expr::Number { .. } | Expr::Float { .. } => true,
        Expr::Field { of, .. } => in_place(of),
        Expr::Case { payload: None, .. } => true,
        _ => false,
    }
}

fn plain(value: Expr) -> Arg {
    Arg {
        name: None,
        value,
        after: false,
        mutable: None,
        given_back: false,
        keeps: false,
        take: None,
        written: false,
    }
}

fn named(name: &str, value: Expr, pos: Pos) -> Arg {
    Arg {
        name: Some((name.to_string(), pos)),
        ..plain(value)
    }
}
