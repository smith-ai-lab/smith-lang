//! A whole number as a type parameter of a function and of a struct, written
//! `[n: Nat]` or `[n: Pos]`.
//!
//! The number is known when the program is built, so each use of such an item
//! stands for a copy of it with the number written where the name is, and the
//! copy is checked and lowered as the item that is written with the number put
//! in by hand. This is done on the tokens, before the file is parsed: the
//! item is taken out, and for each choice of numbers that the file makes, a
//! copy of its tokens is added, with the name of the item made into one that
//! no source writes, `top#4`, and each name of a parameter made into the
//! number. Each copy has lines of its own, so that nothing the checker keys
//! by place is shared between two copies, and an error found in a copy is put
//! back at the place it has in the item, and says the numbers it was found
//! for.
//!
//! A use that writes the numbers, as `top[4]()` and `Box[10]`, is rewritten
//! here. A use that leaves them out, as `top()` and `Box(at: 1)`, takes them
//! from the type of an argument or from the type the use must give, which only
//! the checker knows, so the checker says which copy such a use is, and the
//! file is expanded and checked again until it says no more.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::check::Checked;
use crate::diag::{Error, Pos};
use crate::lexer::{Piece, Tok, Token};

/// How many lines apart two copies are. A file has fewer lines than this.
const LINES: usize = 1 << 24;

/// How many times the file is expanded again at most, as a use whose numbers
/// come from the copy of another needs one more time for each step.
const ROUNDS: usize = 64;

/// What a whole number parameter is: any `Nat`, or one above zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Nat,
    Pos,
}

/// A whole number parameter of an item.
#[derive(Clone, Debug)]
pub(crate) struct Param {
    pub name: String,
    pos: Pos,
    kind: Kind,
}

/// A type as it is written in the signature of an item: a name with the types
/// in brackets after it. A number written there is a name that is empty, which
/// no parameter has.
#[derive(Clone, Debug)]
pub(crate) enum Shape {
    Name(String, Vec<Shape>),
}

/// A function or a struct with whole number parameters, as it is written.
pub(crate) struct Template {
    pub name: String,
    is_struct: bool,
    /// Where its name is.
    pos: Pos,
    /// Whether it writes what is not built, which is reported, so that no copy
    /// is made of it and a use of it is said nothing of.
    pub broken: bool,
    pub params: Vec<Param>,
    /// For a function, the name of each parameter and the type it is written
    /// with, when that is a name and the types in brackets after it.
    pub inputs: Vec<(String, Option<Shape>)>,
    /// The type the item gives: that of a function's result, or the struct.
    pub gives: Option<Shape>,
    tokens: Vec<Token>,
    /// Where the `[` and the `]` of the parameters are among the tokens.
    brackets: (usize, usize),
}

#[derive(Default)]
struct State {
    templates: HashMap<String, Rc<Template>>,
    /// The copies made so far, in the order they were first asked for, each by
    /// its template and its numbers. A copy's place here says where its lines
    /// are, so it never changes.
    copies: Vec<(String, Vec<u64>)>,
    /// What the checker said a name is, by where the name is written: the
    /// template, and the numbers.
    said: HashMap<Pos, (String, Vec<u64>)>,
    /// Whether the checker said something it had not said before.
    grew: bool,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// The function or struct with whole number parameters that the name is.
pub(crate) fn template(name: &str) -> Option<Rc<Template>> {
    STATE.with(|s| s.borrow().templates.get(name).cloned())
}

/// Says that the name written at `at` is the copy of `template` for these
/// numbers.
pub(crate) fn say(at: Pos, template: &str, numbers: Vec<u64>) {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if let std::collections::hash_map::Entry::Vacant(slot) = s.said.entry(at) {
            slot.insert((template.to_string(), numbers));
            s.grew = true;
        }
    });
}

/// The name of the copy of an item for these numbers.
fn copy_name(template: &str, numbers: &[u64]) -> String {
    let numbers: Vec<String> = numbers.iter().map(u64::to_string).collect();
    format!("{template}#{}", numbers.join(","))
}

/// The template and the numbers of the copy a name is, if it is one.
pub(crate) fn copy_of(name: &str) -> Option<(&str, Vec<u64>)> {
    let (template, numbers) = name.split_once('#')?;
    let numbers: Option<Vec<u64>> = numbers.split(',').map(|n| n.parse().ok()).collect();
    Some((template, numbers?))
}

/// What the whole number a token is written as is.
fn value_of(text: &str) -> Option<u128> {
    let digits: String = text.chars().filter(|c| *c != '_').collect();
    match digits.get(..2) {
        Some("0x") => u128::from_str_radix(&digits[2..], 16).ok(),
        Some("0b") => u128::from_str_radix(&digits[2..], 2).ok(),
        _ => digits.parse().ok(),
    }
}

fn is_newline(t: &Token) -> bool {
    t.tok == Tok::Newline
}

/// The tokens without the line breaks that stand inside brackets.
fn plain(tokens: &[Token]) -> Vec<&Token> {
    tokens.iter().filter(|t| !is_newline(t)).collect()
}

/// Splits tokens at the commas that are not inside brackets.
fn split_commas<'t>(tokens: &[&'t Token]) -> Vec<Vec<&'t Token>> {
    let mut parts = vec![Vec::new()];
    let mut depth = 0usize;
    for &t in tokens {
        match &t.tok {
            Tok::Punct("(" | "[" | "{") => depth += 1,
            Tok::Punct(")" | "]" | "}") => depth = depth.saturating_sub(1),
            Tok::Punct(",") if depth == 0 => {
                parts.push(Vec::new());
                continue;
            }
            _ => {}
        }
        parts.last_mut().expect("one part").push(t);
    }
    if parts.last().is_some_and(|p| p.is_empty()) {
        parts.pop();
    }
    parts
}

/// The type that tokens write, when it is a name, with the types in brackets
/// after it, and numbers.
fn shape(tokens: &[&Token]) -> Option<Shape> {
    match tokens {
        [t] => match &t.tok {
            Tok::Ident(name) => Some(Shape::Name(name.clone(), Vec::new())),
            Tok::Number(_) => Some(Shape::Name(String::new(), Vec::new())),
            _ => None,
        },
        [t, open, inner @ .., close]
            if open.tok == Tok::Punct("[") && close.tok == Tok::Punct("]") =>
        {
            let Tok::Ident(name) = &t.tok else {
                return None;
            };
            let args: Option<Vec<Shape>> = split_commas(inner).iter().map(|a| shape(a)).collect();
            Some(Shape::Name(name.clone(), args?))
        }
        _ => None,
    }
}

/// The index of the token that closes the bracket opened at `open`.
fn closing(tokens: &[Token], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (i, t) in tokens.iter().enumerate().skip(open) {
        match &t.tok {
            Tok::Punct("(" | "[" | "{") => depth += 1,
            Tok::Punct(")" | "]" | "}") => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// What an item that starts a file's line says of itself, when it has whole
/// number parameters.
fn template_of(tokens: &[Token], errors: &mut Vec<Error>) -> Option<Template> {
    let mut at = 0;
    if tokens.first()?.tok == Tok::Keyword("pub") {
        at = 1;
    }
    let is_struct = match tokens.get(at)?.tok {
        Tok::Keyword("fn") => false,
        Tok::Keyword("struct") => true,
        _ => return None,
    };
    let Tok::Ident(name) = &tokens.get(at + 1)?.tok else {
        return None;
    };
    let open = at + 2;
    if tokens.get(open)?.tok != Tok::Punct("[") {
        return None;
    }
    let close = closing(tokens, open)?;
    let inside = plain(&tokens[open + 1..close]);
    let mut params = Vec::new();
    let mut others = 0;
    for generic in split_commas(&inside) {
        match generic.as_slice() {
            [n, colon, kind]
                if colon.tok == Tok::Punct(":")
                    && matches!(&n.tok, Tok::Ident(_))
                    && matches!(&kind.tok, Tok::Ident(k) if k == "Nat" || k == "Pos") =>
            {
                let (Tok::Ident(n), Tok::Ident(k)) = (&n.tok, &kind.tok) else {
                    unreachable!("matched above")
                };
                params.push(Param {
                    name: n.clone(),
                    pos: tokens[open + 1..close]
                        .iter()
                        .find(|t| t.tok == Tok::Ident(n.clone()))
                        .map_or(tokens[open].pos, |t| t.pos),
                    kind: if k == "Pos" { Kind::Pos } else { Kind::Nat },
                });
            }
            _ => others += 1,
        }
    }
    if params.is_empty() {
        return None;
    }
    let mut broken = false;
    if others > 0 {
        errors.push(Error::unsupported(
            tokens[open].pos,
            "a type parameter and a whole number parameter in one list",
        ));
        broken = true;
    }
    for pos in array_sizes(&tokens[close + 1..], &params) {
        errors.push(Error::unsupported(pos, "an array type"));
        broken = true;
    }
    let (inputs, gives) = if is_struct {
        let own = params
            .iter()
            .map(|p| Shape::Name(p.name.clone(), Vec::new()))
            .collect();
        (Vec::new(), Some(Shape::Name(name.clone(), own)))
    } else {
        signature(&tokens[close + 1..])
    };
    Some(Template {
        name: name.clone(),
        is_struct,
        pos: tokens[at + 1].pos,
        broken,
        params,
        inputs,
        gives,
        tokens: tokens.to_vec(),
        brackets: (open, close),
    })
}

/// Where an array type has a size that is the name of a parameter, as `[n]f32`
/// does, which is not built.
fn array_sizes(tokens: &[Token], params: &[Param]) -> Vec<Pos> {
    let mut found = Vec::new();
    for i in 1..tokens.len() {
        let after_colon = matches!(tokens[i - 1].tok, Tok::Punct(":" | "->"));
        if tokens[i].tok != Tok::Punct("[") || !after_colon {
            continue;
        }
        let Some(close) = closing(tokens, i) else {
            continue;
        };
        let inside = plain(&tokens[i + 1..close]);
        let sizes = split_commas(&inside)
            .iter()
            .all(|part| matches!(part.as_slice(), [t] if matches!(t.tok, Tok::Ident(_) | Tok::Number(_))));
        let named = inside
            .iter()
            .any(|t| matches!(&t.tok, Tok::Ident(n) if params.iter().any(|p| p.name == *n)));
        let typed = matches!(
            tokens.get(close + 1).map(|t| &t.tok),
            Some(Tok::Ident(_) | Tok::Punct("(" | "["))
        );
        if sizes && named && typed {
            found.push(tokens[i].pos);
        }
    }
    found
}

/// The parameters of a function and the type it gives, as they are written
/// after its name and its parameters in brackets, which `tokens` starts at.
fn signature(tokens: &[Token]) -> (Vec<(String, Option<Shape>)>, Option<Shape>) {
    let mut inputs = Vec::new();
    if tokens.first().map(|t| &t.tok) != Some(&Tok::Punct("(")) {
        return (inputs, None);
    }
    let Some(close) = closing(tokens, 0) else {
        return (inputs, None);
    };
    for part in split_commas(&plain(&tokens[1..close])) {
        let part: Vec<&Token> = part
            .into_iter()
            .filter(|t| t.tok != Tok::Keyword("take"))
            .collect();
        if let [name, colon, ty @ ..] = part.as_slice()
            && let Tok::Ident(name) = &name.tok
            && colon.tok == Tok::Punct(":")
        {
            let end = ty
                .iter()
                .position(|t| t.tok == Tok::Punct("="))
                .unwrap_or(ty.len());
            inputs.push((name.clone(), shape(&ty[..end])));
        } else {
            inputs.push((String::new(), None));
        }
    }
    let mut gives = None;
    if let Some(arrow) = tokens.get(close + 1)
        && arrow.tok == Tok::Punct("->")
    {
        let mut depth = 0usize;
        let mut written = Vec::new();
        for t in &tokens[close + 2..] {
            match &t.tok {
                Tok::Punct("(" | "[") => depth += 1,
                Tok::Punct(")" | "]") => depth = depth.saturating_sub(1),
                Tok::Punct("{" | "<") | Tok::Keyword("requires" | "ensures" | "decreases")
                    if depth == 0 =>
                {
                    break;
                }
                Tok::Newline => continue,
                _ => {}
            }
            written.push(t);
        }
        gives = shape(&written);
    }
    (inputs, gives)
}

/// Where each item of the file starts: a word that begins a line in the first
/// column, after a line break, which is how every item begins.
fn item_starts(tokens: &[Token]) -> Vec<usize> {
    let mut starts = Vec::new();
    for (i, t) in tokens.iter().enumerate() {
        let after_break = i == 0 || tokens[i - 1].tok == Tok::Newline;
        if after_break && t.pos.col == 1 && matches!(t.tok, Tok::Keyword(_) | Tok::Ident(_)) {
            starts.push(i);
        }
    }
    starts
}

/// Writes the numbers where the names of the parameters are, and does the same
/// inside a string that holds expressions. A name after a `.` or before a `:`
/// is a field, a case, or the name of an argument, and is left as it is.
fn put_in(tokens: &mut [Token], params: &[Param], numbers: &[u64]) {
    for i in 0..tokens.len() {
        let after_dot = i > 0 && tokens[i - 1].tok == Tok::Punct(".");
        let before_colon = tokens.get(i + 1).map(|t| &t.tok) == Some(&Tok::Punct(":"));
        let number = match &tokens[i].tok {
            Tok::Ident(name) if !after_dot && !before_colon => (params.iter())
                .position(|p| p.name == *name)
                .map(|k| numbers[k]),
            _ => None,
        };
        if let Some(n) = number {
            tokens[i].tok = Tok::Number(n.to_string());
        }
        if let Tok::Interp(pieces) = &mut tokens[i].tok {
            for piece in pieces {
                if let Piece::Expr(inner) = piece {
                    put_in(inner, params, numbers);
                }
            }
        }
    }
}

fn move_lines(tokens: &mut [Token], by: usize) {
    for t in tokens {
        t.pos.line += by;
        if let Tok::Interp(pieces) = &mut t.tok {
            for piece in pieces {
                if let Piece::Expr(inner) = piece {
                    move_lines(inner, by);
                }
            }
        }
    }
}

fn plural(n: usize, thing: &str) -> String {
    if n == 1 {
        format!("{n} {thing}")
    } else {
        format!("{n} {thing}s")
    }
}

impl State {
    /// The copy of the template for the numbers, which is made if it is new.
    fn copy(&mut self, template: &Template, numbers: &[u64]) -> String {
        let key = (template.name.clone(), numbers.to_vec());
        if !self.copies.contains(&key) {
            self.copies.push(key);
        }
        copy_name(&template.name, numbers)
    }

    /// Rewrites the uses of templates among the tokens: those that say where
    /// the checker said what a name is, and those that write the numbers.
    fn rewrite(&mut self, tokens: Vec<Token>, errors: &mut Vec<Error>) -> Vec<Token> {
        let mut out = Vec::with_capacity(tokens.len());
        let mut i = 0;
        while i < tokens.len() {
            let t = &tokens[i];
            if let Tok::Interp(pieces) = &t.tok {
                let pieces = pieces
                    .iter()
                    .map(|piece| match piece {
                        Piece::Expr(inner) => Piece::Expr(self.rewrite(inner.clone(), errors)),
                        text => text.clone(),
                    })
                    .collect();
                out.push(Token {
                    tok: Tok::Interp(pieces),
                    pos: t.pos,
                });
                i += 1;
                continue;
            }
            let Tok::Ident(name) = &t.tok else {
                out.push(t.clone());
                i += 1;
                continue;
            };
            if let Some((of, numbers)) = self.said.get(&t.pos).cloned() {
                if let Some(template) = self.templates.get(&of).cloned()
                    && self.allowed(&template, &numbers, t.pos, errors)
                {
                    let name = self.copy(&template, &numbers);
                    out.push(Token {
                        tok: Tok::Ident(name),
                        pos: t.pos,
                    });
                    i += 1;
                    continue;
                }
            } else if let Some(template) = self.templates.get(name).cloned()
                && tokens.get(i + 1).map(|t| &t.tok) == Some(&Tok::Punct("["))
                && let Some(close) = closing(&tokens, i + 1)
            {
                if template.broken {
                    out.push(t.clone());
                } else if let Some(numbers) = self.written(&template, &tokens[i..=close], errors) {
                    let name = self.copy(&template, &numbers);
                    out.push(Token {
                        tok: Tok::Ident(name),
                        pos: t.pos,
                    });
                }
                i = close + 1;
                continue;
            }
            out.push(t.clone());
            i += 1;
        }
        out
    }

    /// Whether the numbers suit the template, which is reported if they do not.
    fn allowed(
        &self,
        template: &Template,
        numbers: &[u64],
        at: Pos,
        errors: &mut Vec<Error>,
    ) -> bool {
        for (p, n) in template.params.iter().zip(numbers) {
            if p.kind == Kind::Pos && *n == 0 {
                errors.push(
                    Error::new(
                        at,
                        format!(
                            "`0` is not a `Pos`, which `{}` of `{}` is",
                            p.name, template.name
                        ),
                    )
                    .with_fix("write a number above zero"),
                );
                return false;
            }
        }
        true
    }

    /// The numbers a use writes: `top[4]`, or `Grid[3, 4]`, which are the
    /// tokens from the name to the bracket that closes the numbers.
    fn written(
        &self,
        template: &Template,
        tokens: &[Token],
        errors: &mut Vec<Error>,
    ) -> Option<Vec<u64>> {
        let at = tokens[0].pos;
        let inside = plain(&tokens[2..tokens.len() - 1]);
        let given = split_commas(&inside);
        if given.len() != template.params.len() {
            errors.push(
                Error::new(
                    at,
                    format!(
                        "`{}` takes {}, and {} written",
                        template.name,
                        plural(template.params.len(), "whole number"),
                        given.len()
                    ),
                )
                .with_fix(format!(
                    "write {}",
                    template
                        .params
                        .iter()
                        .map(|p| format!("a value for `{}`", p.name))
                        .collect::<Vec<_>>()
                        .join(" and ")
                )),
            );
            return None;
        }
        let mut numbers = Vec::new();
        for (p, g) in template.params.iter().zip(&given) {
            let number = match g.as_slice() {
                [t] => match &t.tok {
                    Tok::Number(text) => value_of(text).map(|v| (v, t.pos)),
                    _ => None,
                },
                _ => None,
            };
            let Some((value, pos)) = number else {
                let pos = g.first().map_or(at, |t| t.pos);
                errors.push(
                    Error::new(
                        pos,
                        format!(
                            "`{}` of `{}` is a whole number that is known when the program is built, so it is written out",
                            p.name, template.name
                        ),
                    )
                    .with_fix(format!("write the number, as `{}[4]`", template.name)),
                );
                return None;
            };
            let Ok(value) = u64::try_from(value) else {
                errors.push(Error::new(
                    pos,
                    format!(
                        "this does not fit a `Nat`, which `{}` of `{}` is",
                        p.name, template.name
                    ),
                ));
                return None;
            };
            numbers.push(value);
        }
        let allowed = template
            .params
            .iter()
            .zip(&numbers)
            .zip(&given)
            .all(|((p, n), g)| {
                p.kind != Kind::Pos || *n != 0 || {
                    errors.push(
                        Error::new(
                            g[0].pos,
                            format!(
                                "`0` is not a `Pos`, which `{}` of `{}` is",
                                p.name, template.name
                            ),
                        )
                        .with_fix("write a number above zero"),
                    );
                    false
                }
            });
        allowed.then_some(numbers)
    }

    /// The tokens of the copy of a template for some numbers, with lines of its
    /// own. A struct has each number written where its name is. A function has
    /// it so in what it says of itself, and in its body the name stands, as a
    /// `Nat` that the body defines first, so that it is a value there and not a
    /// number written out.
    fn made(&self, id: usize) -> Vec<Token> {
        let (name, numbers) = &self.copies[id];
        let template = &self.templates[name];
        let (open, close) = template.brackets;
        let mut tokens: Vec<Token> = template.tokens[..open]
            .iter()
            .chain(&template.tokens[close + 1..])
            .cloned()
            .collect();
        let name_at = tokens
            .iter()
            .position(|t| t.tok == Tok::Ident(template.name.clone()))
            .expect("the item is named");
        tokens[name_at].tok = Tok::Ident(copy_name(name, numbers));
        let body = if template.is_struct {
            None
        } else {
            body_start(&tokens, name_at + 1)
        };
        let at = body.unwrap_or(tokens.len());
        let (signature, body) = tokens.split_at_mut(at);
        put_in(signature, &template.params, numbers);
        let mut tokens = signature.to_vec();
        if let Some((open, rest)) = body.split_first() {
            tokens.push(open.clone());
            for (p, n) in template.params.iter().zip(numbers) {
                if mentions(rest, &p.name) {
                    tokens.extend(binding(p, *n));
                }
            }
            tokens.extend_from_slice(rest);
        }
        move_lines(&mut tokens, (id + 1) * LINES);
        tokens
    }
}

/// Where the `{` that opens the body of a function is, which is the first one
/// outside brackets after its parameters.
fn body_start(tokens: &[Token], from: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (i, t) in tokens.iter().enumerate().skip(from) {
        match &t.tok {
            Tok::Punct("(" | "[") => depth += 1,
            Tok::Punct(")" | "]") => depth = depth.saturating_sub(1),
            Tok::Punct("{") if depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

/// Whether the tokens write the name, as a value.
fn mentions(tokens: &[Token], name: &str) -> bool {
    tokens.iter().enumerate().any(|(i, t)| match &t.tok {
        Tok::Ident(n) => {
            n == name
                && !(i > 0 && tokens[i - 1].tok == Tok::Punct("."))
                && tokens.get(i + 1).map(|t| &t.tok) != Some(&Tok::Punct(":"))
        }
        Tok::Interp(pieces) => pieces.iter().any(|piece| match piece {
            Piece::Expr(inner) => mentions(inner, name),
            Piece::Text(_) => false,
        }),
        _ => false,
    })
}

/// The line that defines a parameter in the body of a copy: `n: Nat = 4`, or
/// the range from 1 for a `Pos`, which no file can write by its name yet.
fn binding(p: &Param, number: u64) -> Vec<Token> {
    let at = |tok| Token { tok, pos: p.pos };
    let mut tokens = vec![
        at(Tok::Newline),
        at(Tok::Ident(p.name.clone())),
        at(Tok::Punct(":")),
    ];
    match p.kind {
        Kind::Nat => tokens.push(at(Tok::Ident("Nat".into()))),
        Kind::Pos => tokens.extend([
            at(Tok::Number("1".into())),
            at(Tok::Punct("..")),
            at(Tok::Number("18446744073709551616".into())),
        ]),
    }
    tokens.extend([
        at(Tok::Punct("=")),
        at(Tok::Number(number.to_string())),
        at(Tok::Newline),
    ]);
    tokens
}

/// Takes out of the tokens of the file each item that has whole number
/// parameters, and gives what is left and the items taken out. A file with
/// none is not copied.
fn split(tokens: &[Token], errors: &mut Vec<Error>) -> Option<(Vec<Token>, Vec<Template>)> {
    let starts = item_starts(tokens);
    let end = tokens.len() - 1;
    let mut found = Vec::new();
    for (k, &from) in starts.iter().enumerate() {
        let to = starts.get(k + 1).copied().unwrap_or(end);
        if let Some(template) = template_of(&tokens[from..to], errors) {
            found.push((from, to, template));
        }
    }
    if found.is_empty() {
        return None;
    }
    let mut out = Vec::with_capacity(tokens.len());
    let mut copied_to = 0;
    let mut taken = Vec::new();
    for (from, to, template) in found {
        out.extend_from_slice(&tokens[copied_to..from]);
        copied_to = to;
        taken.push(template);
    }
    out.extend_from_slice(&tokens[copied_to..]);
    Some((out, taken))
}

/// Finds the copies the file asks for and checks it with them, again for each
/// thing the checker says about a use that leaves the numbers out, until it
/// says no more. `attempt` checks the tokens of a file, with what the items in
/// it that are not built say, which the checker reports with its other errors.
pub(crate) fn run(
    tokens: Vec<Token>,
    mut attempt: impl FnMut(Vec<Token>, Vec<Error>) -> Result<Checked, Vec<Error>>,
) -> Result<Checked, Vec<Error>> {
    let mut pre = Vec::new();
    let split = split(&tokens, &mut pre);
    let Some((base, taken)) = split else {
        STATE.with(|s| *s.borrow_mut() = State::default());
        return attempt(tokens, Vec::new());
    };
    drop(tokens);
    STATE.with(|s| {
        *s.borrow_mut() = State {
            templates: taken
                .into_iter()
                .map(|t| (t.name.clone(), Rc::new(t)))
                .collect(),
            ..State::default()
        };
    });
    let mut last = None;
    for _ in 0..ROUNDS {
        let (tokens, found) = STATE.with(|s| {
            let mut s = s.borrow_mut();
            s.grew = false;
            let mut errors = Vec::new();
            let end = base.last().cloned().expect("the end of the file");
            let mut tokens = s.rewrite(base[..base.len() - 1].to_vec(), &mut errors);
            let mut id = 0;
            while id < s.copies.len() {
                let made = s.made(id);
                let made = s.rewrite(made, &mut errors);
                tokens.push(Token {
                    tok: Tok::Newline,
                    pos: end.pos,
                });
                tokens.extend(made);
                id += 1;
            }
            tokens.push(end);
            (tokens, errors)
        });
        if !found.is_empty() {
            last = Some(Err(found.into_iter().chain(pre.iter().cloned()).collect()));
            break;
        }
        let result = attempt(tokens, pre.clone());
        let grew = STATE.with(|s| s.borrow().grew);
        last = Some(result);
        if !grew {
            break;
        }
    }
    let result = last.expect("one round ran");
    let result = STATE.with(|s| put_back(&s.borrow(), result));
    STATE.with(|s| *s.borrow_mut() = State::default());
    result
}

/// Puts each error and warning found in a copy at the place the item has,
/// says the numbers it was found for, and writes the name of each copy as it
/// is written in a file.
fn put_back(state: &State, result: Result<Checked, Vec<Error>>) -> Result<Checked, Vec<Error>> {
    // An error says the numbers its copy was made for. A warning does not, as
    // it is about the item, which a copy no code reaches is dead in as much as
    // the item is.
    let fix = |errors: Vec<Error>, says: bool| -> Vec<Error> {
        errors
            .into_iter()
            .map(|mut e| {
                let mut note = String::new();
                if e.module.is_none() && e.pos.line >= LINES {
                    let id = e.pos.line / LINES - 1;
                    e.pos.line %= LINES;
                    if let Some((name, numbers)) = state.copies.get(id)
                        && let Some(template) = state.templates.get(name)
                        && says
                    {
                        let given: Vec<String> = (template.params.iter().zip(numbers))
                            .map(|(p, n)| format!("`{}` as {n}", p.name))
                            .collect();
                        note = format!(", with {}", given.join(" and "));
                    }
                }
                e.message = format!("{}{note}", written(&e.message));
                e.fix = e.fix.map(|f| written(&f));
                e
            })
            .collect()
    };
    match result {
        Ok(mut checked) => {
            checked.warnings = fix(checked.warnings, false);
            // An item that no use asks a copy of is dead, as one that `main`
            // does not reach is.
            for template in state.templates.values() {
                if !template.broken && !state.copies.iter().any(|(name, _)| *name == template.name)
                {
                    checked.warnings.push(
                        Error::new(
                            template.pos,
                            format!(
                                "the {} `{}` is dead code: `main` does not reach it",
                                if template.is_struct {
                                    "struct"
                                } else {
                                    "function"
                                },
                                template.name
                            ),
                        )
                        .with_fix("delete it, or use it from code that `main` reaches"),
                    );
                }
            }
            Ok(checked)
        }
        Err(errors) => Err(fix(errors, true)),
    }
}

/// The text with the name of each copy as a file writes it: `top#4` as
/// `top[4]`.
fn written(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '#'
            && i > 0
            && (chars[i - 1].is_alphanumeric() || chars[i - 1] == '_')
            && chars.get(i + 1).is_some_and(|c| c.is_ascii_digit())
        {
            let mut j = i + 1;
            let mut numbers = String::new();
            while j < chars.len() && (chars[j].is_ascii_digit() || chars[j] == ',') {
                numbers.push(chars[j]);
                j += 1;
            }
            out.push('[');
            out.push_str(&numbers.replace(',', ", "));
            out.push(']');
            i = j;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}
