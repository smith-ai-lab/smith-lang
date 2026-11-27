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

use crate::check::{Checked, Library, STEP};
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
    /// The number a use that leaves the parameter out and that nothing settles
    /// gets, written after `=`.
    pub default: Option<u64>,
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
    /// The file that has the item: 0 for the file that is built, and the place
    /// of the module among those that have items of this kind otherwise.
    pub file: usize,
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

/// A file the numbers are put into: the one that is built, or a module that
/// has an item with whole number parameters or uses one of another module.
#[derive(Default)]
struct FileState {
    /// The name of the module as a `use` writes it, and none for the file that
    /// is built.
    module: Option<String>,
    /// Its tokens, without the items that have whole number parameters, and
    /// with the lines the lexer gave them.
    base: Vec<Token>,
    /// The items it has, by name.
    templates: HashMap<String, Rc<Template>>,
    /// The names it takes from another file that are items with whole number
    /// parameters there, by the name it gives them: the file, and the name the
    /// item has.
    imports: HashMap<String, (usize, String)>,
    /// What the items that are not built say.
    errors: Vec<Error>,
    /// How many copies of its items the tokens it was last given have, when it
    /// was given any in this round.
    emitted: Option<usize>,
}

#[derive(Default)]
struct State {
    /// The file that is built, and the modules.
    files: Vec<FileState>,
    /// The place of each module that is a file here, by its name.
    modules: HashMap<String, usize>,
    /// The modules that have no item of this kind and use none.
    plain: std::collections::HashSet<String>,
    /// The place of the file that the module with a number among those read
    /// is.
    placed: HashMap<usize, usize>,
    /// The copies made so far, in the order they were first asked for, each by
    /// the file it is in, its template, and its numbers. A copy's place here
    /// says where its lines are, so it never changes.
    copies: Vec<(usize, String, Vec<u64>)>,
    /// What the checker said a name is, by where the name is written: the
    /// file of the template, the template, and the numbers.
    said: HashMap<Pos, (usize, String, Vec<u64>)>,
    /// Whether the checker said something it had not said before.
    grew: bool,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// The function or struct with whole number parameters that the name is where
/// it is written at `at`: one of the file that holds the place, or one that the
/// file takes from another.
pub(crate) fn template(name: &str, at: Pos) -> Option<Rc<Template>> {
    STATE.with(|s| {
        let s = s.borrow();
        let file = if at.line < STEP {
            0
        } else {
            *s.placed.get(&(at.line / STEP))?
        };
        s.find(file, name)
    })
}

/// Says that the name written at `at` is the copy of `template` for these
/// numbers.
pub(crate) fn say(at: Pos, template: &Template, numbers: Vec<u64>) {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if let std::collections::hash_map::Entry::Vacant(slot) = s.said.entry(at) {
            slot.insert((template.file, template.name.clone(), numbers));
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
    let mut params: Vec<Param> = Vec::new();
    let mut others = 0;
    let mut broken = false;
    for generic in split_commas(&inside) {
        let (n, kind, rest) = match generic.as_slice() {
            [n, colon, kind, rest @ ..]
                if colon.tok == Tok::Punct(":")
                    && matches!(&n.tok, Tok::Ident(_))
                    && matches!(&kind.tok, Tok::Ident(k) if k == "Nat" || k == "Pos") =>
            {
                (n, kind, rest)
            }
            _ => {
                others += 1;
                continue;
            }
        };
        let (Tok::Ident(n_name), Tok::Ident(k)) = (&n.tok, &kind.tok) else {
            unreachable!("matched above")
        };
        let kind = if k == "Pos" { Kind::Pos } else { Kind::Nat };
        let pos = tokens[open + 1..close]
            .iter()
            .find(|t| t.tok == Tok::Ident(n_name.clone()))
            .map_or(tokens[open].pos, |t| t.pos);
        let mut default = None;
        match rest {
            [] => {}
            [eq, value] if eq.tok == Tok::Punct("=") => match default_of(n_name, kind, value) {
                Ok(number) => default = Some(number),
                Err(error) => {
                    errors.push(error);
                    broken = true;
                }
            },
            [eq, minus, value]
                if eq.tok == Tok::Punct("=")
                    && minus.tok == Tok::Punct("-")
                    && matches!(&value.tok, Tok::Number(_)) =>
            {
                let Tok::Number(text) = &value.tok else {
                    unreachable!("matched above")
                };
                errors.push(
                    Error::new(
                        minus.pos,
                        format!("`-{text}` is not a `Nat`, which `{n_name}` is"),
                    )
                    .with_fix("write a number that is not below zero"),
                );
                broken = true;
            }
            _ => {
                errors.push(
                    Error::new(
                        rest[0].pos,
                        format!(
                            "the default of `{n_name}` is a whole number that is known when the program is built, so it is written out"
                        ),
                    )
                    .with_fix("write the number, as `= 4`"),
                );
                broken = true;
            }
        }
        if default.is_none()
            && rest.is_empty()
            && let Some(before) = params.iter().find(|q| q.default.is_some())
        {
            errors.push(
                Error::new(
                    pos,
                    format!(
                        "`{n_name}` has no default, and comes after `{}`, which has one",
                        before.name
                    ),
                )
                .with_fix(format!(
                    "give `{n_name}` a default, or write it before `{}`",
                    before.name
                )),
            );
            broken = true;
        }
        params.push(Param {
            name: n_name.clone(),
            pos,
            kind,
            default,
        });
    }
    if params.is_empty() {
        return None;
    }
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
        file: 0,
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

/// The number a default is, which its parameter's kind must hold.
fn default_of(name: &str, kind: Kind, value: &Token) -> Result<u64, Error> {
    let Tok::Number(text) = &value.tok else {
        return Err(Error::new(
            value.pos,
            format!(
                "the default of `{name}` is a whole number that is known when the program is built, so it is written out"
            ),
        )
        .with_fix("write the number, as `= 4`"));
    };
    let Some(number) = value_of(text).and_then(|v| u64::try_from(v).ok()) else {
        return Err(Error::new(
            value.pos,
            format!("this does not fit a `Nat`, which `{name}` is"),
        ));
    };
    if kind == Kind::Pos && number == 0 {
        return Err(
            Error::new(value.pos, format!("`0` is not a `Pos`, which `{name}` is"))
                .with_fix("write a number above zero"),
        );
    }
    Ok(number)
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

/// Writes the numbers where the body of a copy gives a parameter to another
/// item in brackets after its name, as `make[limit]` and `Box[limit]` do, since
/// what is in those brackets is a number written out and not a name.
fn put_in_brackets(tokens: &mut [Token], params: &[Param], numbers: &[u64]) {
    // Whether each bracket that is open follows a name.
    let mut after_name: Vec<bool> = Vec::new();
    for i in 0..tokens.len() {
        match &tokens[i].tok {
            Tok::Punct("[") => {
                let named = i > 0 && matches!(tokens[i - 1].tok, Tok::Ident(_));
                after_name.push(named);
            }
            Tok::Punct("(" | "{") => after_name.push(false),
            Tok::Punct("]" | ")" | "}") => {
                after_name.pop();
            }
            Tok::Ident(name) if after_name.last() == Some(&true) && i > 0 => {
                let before = matches!(tokens[i - 1].tok, Tok::Punct("[" | ","));
                let next = matches!(
                    tokens.get(i + 1).map(|t| &t.tok),
                    Some(Tok::Punct("]" | ","))
                );
                if before
                    && next
                    && let Some(k) = params.iter().position(|p| p.name == *name)
                {
                    tokens[i].tok = Tok::Number(numbers[k].to_string());
                }
            }
            Tok::Interp(_) => {}
            _ => {}
        }
        if let Tok::Interp(pieces) = &mut tokens[i].tok {
            for piece in pieces {
                if let Piece::Expr(inner) = piece {
                    put_in_brackets(inner, params, numbers);
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
        let key = (template.file, template.name.clone(), numbers.to_vec());
        if !self.copies.contains(&key) {
            self.copies.push(key);
        }
        copy_name(&template.name, numbers)
    }

    /// The item with whole number parameters that a name is in a file: one it
    /// has, or one it takes from another file.
    fn find(&self, file: usize, name: &str) -> Option<Rc<Template>> {
        let file = self.files.get(file)?;
        if let Some(template) = file.templates.get(name) {
            return Some(template.clone());
        }
        let (from, original) = file.imports.get(name)?;
        self.files[*from].templates.get(original).cloned()
    }

    /// Rewrites the uses of templates among the tokens: those that say where
    /// the checker said what a name is, and those that write the numbers.
    fn rewrite(
        &mut self,
        file: usize,
        tokens: Vec<Token>,
        errors: &mut Vec<Error>,
        needed: &mut Vec<(usize, String)>,
    ) -> Vec<Token> {
        let mut out = Vec::with_capacity(tokens.len());
        let mut i = 0;
        while i < tokens.len() {
            let t = &tokens[i];
            if let Tok::Interp(pieces) = &t.tok {
                let pieces = pieces
                    .iter()
                    .map(|piece| match piece {
                        Piece::Expr(inner) => {
                            Piece::Expr(self.rewrite(file, inner.clone(), errors, needed))
                        }
                        text => text.clone(),
                    })
                    .collect();
                out.push(Token::new(Tok::Interp(pieces), t.pos));
                i += 1;
                continue;
            }
            let Tok::Ident(name) = &t.tok else {
                out.push(t.clone());
                i += 1;
                continue;
            };
            if let Some((from, of, numbers)) = self.said.get(&t.pos).cloned() {
                if let Some(template) = self.files[from].templates.get(&of).cloned()
                    && self.allowed(&template, &numbers, t.pos, errors)
                {
                    let name = self.copy(&template, &numbers);
                    if template.file != file {
                        needed.push((template.file, name.clone()));
                    }
                    out.push(Token::new(Tok::Ident(name), t.pos));
                    i += 1;
                    continue;
                }
            } else if let Some(template) = self.find(file, name)
                && template.is_struct
                && !template.broken
                && template.params.iter().all(|p| p.default.is_some())
                && i > 0
                && matches!(tokens[i - 1].tok, Tok::Punct(":" | "->" | "[" | ","))
                && !matches!(
                    tokens.get(i + 1).map(|t| &t.tok),
                    Some(Tok::Punct("(" | "[" | "."))
                )
            {
                // A type written with every number left out is the one that
                // writes the defaults.
                let numbers: Vec<u64> = template.params.iter().filter_map(|p| p.default).collect();
                let name = self.copy(&template, &numbers);
                if template.file != file {
                    needed.push((template.file, name.clone()));
                }
                out.push(Token::new(Tok::Ident(name), t.pos));
                i += 1;
                continue;
            } else if let Some(template) = self.find(file, name)
                && tokens.get(i + 1).map(|t| &t.tok) == Some(&Tok::Punct("["))
                && let Some(close) = closing(&tokens, i + 1)
            {
                if template.broken {
                    out.push(t.clone());
                } else if let Some(numbers) = self.written(&template, &tokens[i..=close], errors) {
                    let name = self.copy(&template, &numbers);
                    if template.file != file {
                        needed.push((template.file, name.clone()));
                    }
                    out.push(Token::new(Tok::Ident(name), t.pos));
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
        let required = template
            .params
            .iter()
            .filter(|p| p.default.is_none())
            .count();
        if given.len() < required || given.len() > template.params.len() {
            let takes = if required == template.params.len() {
                plural(required, "whole number")
            } else {
                format!(
                    "{required} to {}",
                    plural(template.params.len(), "whole number")
                )
            };
            errors.push(
                Error::new(
                    at,
                    format!(
                        "`{}` takes {takes}, and {} written",
                        template.name,
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
            if let [minus, t] = g.as_slice()
                && minus.tok == Tok::Punct("-")
                && let Tok::Number(text) = &t.tok
            {
                errors.push(
                    Error::new(
                        minus.pos,
                        format!(
                            "`-{text}` is not a `Nat`, which `{}` of `{}` is",
                            p.name, template.name
                        ),
                    )
                    .with_fix("write a number that is not below zero"),
                );
                return None;
            }
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
        // What a use leaves out is its default.
        for p in &template.params[numbers.len()..] {
            numbers.push(
                p.default
                    .expect("a parameter after those written has a default"),
            );
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
        let (from, name, numbers) = &self.copies[id];
        let template = &self.files[*from].templates[name];
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
            let mut rest = rest.to_vec();
            put_in_brackets(&mut rest, &template.params, numbers);
            let rest = &rest[..];
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
    let at = |tok| Token::new(tok, p.pos);
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
fn split(
    tokens: &[Token],
    file: usize,
    errors: &mut Vec<Error>,
) -> Option<(Vec<Token>, Vec<Template>)> {
    let starts = item_starts(tokens);
    let end = tokens.len() - 1;
    let mut found = Vec::new();
    for (k, &from) in starts.iter().enumerate() {
        let to = starts.get(k + 1).copied().unwrap_or(end);
        if let Some(mut template) = template_of(&tokens[from..to], errors) {
            template.file = file;
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

/// The tokens with each place moved by `by` lines.
fn shift(tokens: Vec<Token>, by: usize) -> Vec<Token> {
    let mut tokens = tokens;
    if by > 0 {
        move_lines(&mut tokens, by);
    }
    tokens
}

/// What a `use` line takes: the module it is from, written as it is, and the
/// names it lists, each with the name it is given and where it stands among the
/// tokens.
struct UseLine {
    module: String,
    /// The `(` that opens the names and the `)` that closes them, if it lists any.
    list: Option<(usize, usize)>,
    /// Each name listed, by the name it is, the name it is given, and the tokens
    /// of the entry from its first to its last.
    names: Vec<(String, String, usize, usize)>,
}

/// The `use` lines at the start of a file, which stand before everything else.
fn use_lines(tokens: &[Token]) -> Vec<UseLine> {
    let mut found = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        match &tokens[i].tok {
            Tok::Newline => {
                i += 1;
                continue;
            }
            Tok::Keyword("use") => {}
            _ => break,
        }
        i += 1;
        let mut module = String::new();
        while let Some(t) = tokens.get(i) {
            match &t.tok {
                Tok::Ident(name) => module.push_str(name),
                Tok::Punct(".") => module.push('.'),
                _ => break,
            }
            i += 1;
        }
        if tokens.get(i).map(|t| &t.tok) == Some(&Tok::Keyword("as")) {
            i += 2;
        }
        let mut list = None;
        let mut names = Vec::new();
        if tokens.get(i).map(|t| &t.tok) == Some(&Tok::Punct("("))
            && let Some(close) = closing(tokens, i)
        {
            list = Some((i, close));
            let mut k = i + 1;
            while k < close {
                if let Tok::Ident(name) = &tokens[k].tok {
                    let start = k;
                    let mut local = name.clone();
                    k += 1;
                    if tokens.get(k).map(|t| &t.tok) == Some(&Tok::Keyword("as"))
                        && let Some(Tok::Ident(given)) = tokens.get(k + 1).map(|t| &t.tok)
                    {
                        local = given.clone();
                        k += 2;
                    }
                    names.push((name.clone(), local, start, k - 1));
                } else {
                    k += 1;
                }
            }
            i = close + 1;
        }
        found.push(UseLine {
            module,
            list,
            names,
        });
        while i < tokens.len() && tokens[i].tok != Tok::Newline {
            i += 1;
        }
    }
    found
}

/// The modules a source says it takes from, read off the lines that begin with
/// `use`, so that nothing is lexed to know them.
fn used_modules(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| line.strip_prefix("use "))
        .map(|rest| {
            let end = rest
                .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.'))
                .unwrap_or(rest.len());
            rest[..end].to_string()
        })
        .collect()
}

/// Whether a source writes an item that has a whole number as a parameter: a
/// line that starts a function or a struct, with the name followed by brackets
/// that hold `: Nat` or `: Pos`.
fn defines_templates(text: &str) -> bool {
    text.lines().any(|line| {
        let line = line.strip_prefix("pub ").unwrap_or(line);
        let Some(rest) = line
            .strip_prefix("fn ")
            .or_else(|| line.strip_prefix("struct "))
        else {
            return false;
        };
        let end = rest
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
        let Some(inside) = rest[end..].strip_prefix('[') else {
            return false;
        };
        let inside = inside.split(']').next().unwrap_or(inside);
        inside.contains(": Nat") || inside.contains(": Pos")
    })
}

/// The file of a module that has items with whole number parameters, or uses
/// those of another module, made if it is new, and none for a module that has
/// none and uses none.
fn module_file(state: &mut State, library: &Library, name: &str) -> Option<usize> {
    if let Some(&file) = state.modules.get(name) {
        return Some(file);
    }
    if state.plain.contains(name) {
        return None;
    }
    let Some(text) = library.source(name) else {
        state.plain.insert(name.to_string());
        return None;
    };
    // Modules that import each other in a ring are refused by the checker, and
    // here the module is taken for one that has nothing until it is done.
    state.plain.insert(name.to_string());
    let mut uses_one = false;
    for used in used_modules(&text) {
        uses_one |= module_file(state, library, &used).is_some();
    }
    if !uses_one && !defines_templates(&text) {
        return None;
    }
    let Ok(tokens) = crate::lexer::lex(text.as_bytes()) else {
        return None;
    };
    state.plain.remove(name);
    let file = state.files.len();
    let mut errors = Vec::new();
    let (base, templates) =
        split(&tokens, file, &mut errors).unwrap_or((tokens.clone(), Vec::new()));
    state.files.push(FileState::default());
    state.modules.insert(name.to_string(), file);
    take_imports(state, file, &base);
    let at = &mut state.files[file];
    at.module = Some(name.to_string());
    at.base = base;
    at.templates = templates
        .into_iter()
        .map(|t| (t.name.clone(), Rc::new(t)))
        .collect();
    at.errors = errors;
    Some(file)
}

/// Records the names a file takes from modules that are items with whole
/// number parameters there.
fn take_imports(state: &mut State, file: usize, base: &[Token]) {
    let mut taken = HashMap::new();
    for line in use_lines(base) {
        let Some(&from) = state.modules.get(&line.module) else {
            continue;
        };
        for (name, local, ..) in &line.names {
            if state.files[from].templates.contains_key(name) {
                taken.insert(local.clone(), (from, name.clone()));
            }
        }
    }
    state.files[file].imports = taken;
}

impl State {
    /// The tokens of a file for this round: its own, then the copies of its
    /// items that have been asked for, with each use that writes numbers or has
    /// them said made one of a copy, and the `use` lines that take items of other
    /// files taking the copies this file asks of them. `at` is where the lines
    /// of the file are, which is 0 for the file that is built.
    fn file_tokens(&mut self, file: usize, at: usize, errors: &mut Vec<Error>) -> Vec<Token> {
        let base = self.files[file].base.clone();
        let end = base.last().cloned().expect("the end of the file");
        let mut needed = Vec::new();
        let body = shift(base[..base.len() - 1].to_vec(), at);
        let mut tokens = self.rewrite(file, body, errors, &mut needed);
        let mut id = 0;
        while id < self.copies.len() {
            if self.copies[id].0 == file {
                let made = shift(self.made(id), at);
                let made = self.rewrite(file, made, errors, &mut needed);
                tokens.push(Token::new(Tok::Newline, end.pos));
                tokens.extend(made);
            }
            id += 1;
        }
        tokens.push(Token::new(
            end.tok,
            Pos {
                line: end.pos.line + at,
                ..end.pos
            },
        ));
        self.files[file].emitted = Some(self.copies.iter().filter(|c| c.0 == file).count());
        self.take_copies(tokens, &needed)
    }

    /// The tokens with the `use` lines that take items of other files taking the
    /// copies of them that `needed` says, in place of the items.
    fn take_copies(&self, tokens: Vec<Token>, needed: &[(usize, String)]) -> Vec<Token> {
        let mut out = Vec::with_capacity(tokens.len() + needed.len() * 3);
        let mut copied_to = 0;
        for line in use_lines(&tokens) {
            let Some((open, close)) = line.list else {
                continue;
            };
            let from_file = self.modules.get(&line.module).copied();
            let mut new_list: Vec<Token> = Vec::new();
            let mut changed = false;
            for (name, _, first, last) in &line.names {
                // The copies of this item that the file asks of the module, each once.
                let mut copies: Vec<&String> = Vec::new();
                for (f, copy) in needed {
                    if Some(*f) == from_file
                        && copy_of(copy).is_some_and(|(of, _)| of == name)
                        && !copies.contains(&copy)
                    {
                        copies.push(copy);
                    }
                }
                let at = tokens[*first].pos;
                if copies.is_empty() {
                    if !new_list.is_empty() {
                        new_list.push(Token::new(Tok::Punct(","), at));
                    }
                    new_list.extend_from_slice(&tokens[*first..=*last]);
                } else {
                    changed = true;
                    for copy in copies {
                        if !new_list.is_empty() {
                            new_list.push(Token::new(Tok::Punct(","), at));
                        }
                        new_list.push(Token::new(Tok::Ident(copy.clone()), at));
                    }
                }
            }
            if changed {
                out.extend_from_slice(&tokens[copied_to..=open]);
                out.extend(new_list);
                copied_to = close;
            }
        }
        out.extend_from_slice(&tokens[copied_to..]);
        out
    }
}

/// The names of the items with whole number parameters that the module with this
/// name exports, which a file may list in a `use` though no item has that name
/// left in the module, so that a name that is listed and not used is said so as
/// any other is.
pub(crate) fn exported_items(name: &str) -> Vec<String> {
    STATE.with(|s| {
        let s = s.borrow();
        let Some(&file) = s.modules.get(name) else {
            return Vec::new();
        };
        let public = |t: &&Rc<Template>| {
            t.tokens
                .first()
                .is_some_and(|t| t.tok == Tok::Keyword("pub"))
        };
        s.files[file]
            .templates
            .values()
            .filter(public)
            .map(|t| t.name.clone())
            .collect()
    })
}

/// The tokens of a module read by itself, with no program that uses it: the items
/// with whole number parameters are left out, as no copy of them is asked for,
/// and the names they have are those it exports. None when it has none.
pub(crate) fn module_alone(text: &str, at: usize) -> Option<(Vec<Token>, Vec<Error>, Vec<String>)> {
    if !defines_templates(text) {
        return None;
    }
    let tokens = crate::lexer::lex(text.as_bytes()).ok()?;
    let mut errors = Vec::new();
    let (base, templates) = split(&tokens, 0, &mut errors)?;
    let names = templates
        .iter()
        .filter(|t| {
            t.tokens
                .first()
                .is_some_and(|t| t.tok == Tok::Keyword("pub"))
        })
        .map(|t| t.name.clone())
        .collect();
    Some((shift(base, at), errors, names))
}

/// The tokens of the module with this name, as the checker reads it, which is at
/// `at` lines from the start, when the module has items with whole number
/// parameters or uses those of another module, and what is not built in it.
pub(crate) fn module_tokens(name: &str, at: usize) -> Option<(Vec<Token>, Vec<Error>)> {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        let file = *s.modules.get(name)?;
        s.placed.insert(at / STEP, file);
        let mut errors = s.files[file].errors.clone();
        let tokens = s.file_tokens(file, at, &mut errors);
        Some((tokens, errors))
    })
}

/// Finds the copies the file asks for and checks it with them, again for each
/// thing the checker says about a use that leaves the numbers out, until it
/// says no more. `attempt` checks the tokens of a file, with what the items in
/// it that are not built say, which the checker reports with its other errors.
/// The modules the file takes from are looked for in `library`, and those that
/// have items with whole number parameters get their copies as the file does.
pub(crate) fn run(
    tokens: Vec<Token>,
    library: &Library,
    mut attempt: impl FnMut(Vec<Token>, Vec<Error>) -> Result<Checked, Vec<Error>>,
) -> Result<Checked, Vec<Error>> {
    let mut pre = Vec::new();
    let split = split(&tokens, 0, &mut pre);
    let mut state = State::default();
    let (base, taken) = match split {
        Some(split) => split,
        None => (tokens.clone(), Vec::new()),
    };
    state.files.push(FileState::default());
    for line in use_lines(&base) {
        module_file(&mut state, library, &line.module);
    }
    if taken.is_empty() && state.modules.is_empty() {
        STATE.with(|s| *s.borrow_mut() = State::default());
        return attempt(tokens, Vec::new());
    }
    drop(tokens);
    take_imports(&mut state, 0, &base);
    state.files[0].base = base;
    state.files[0].templates = taken
        .into_iter()
        .map(|t| (t.name.clone(), Rc::new(t)))
        .collect();
    STATE.with(|s| *s.borrow_mut() = state);
    let mut last = None;
    for _ in 0..ROUNDS {
        let (tokens, found) = STATE.with(|s| {
            let mut s = s.borrow_mut();
            s.grew = false;
            s.placed.clear();
            for file in &mut s.files {
                file.emitted = None;
            }
            let mut errors = Vec::new();
            let tokens = s.file_tokens(0, 0, &mut errors);
            (tokens, errors)
        });
        if !found.is_empty() {
            last = Some(Err(found.into_iter().chain(pre.iter().cloned()).collect()));
            break;
        }
        let result = attempt(tokens, pre.clone());
        // A module that was read before another asked it for a copy is read
        // again, with the copy.
        let grew = STATE.with(|s| {
            let s = s.borrow();
            s.grew
                || s.files.iter().enumerate().any(|(file, f)| {
                    f.emitted
                        .is_some_and(|had| s.copies.iter().filter(|c| c.0 == file).count() > had)
                })
        });
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
                // A place in a module is the module's lines, and a place in a
                // copy is in lines after those of the module.
                let within = e.pos.line % STEP;
                if within >= LINES {
                    let id = within / LINES - 1;
                    e.pos.line = e.pos.line - within + within % LINES;
                    if let Some((file, name, numbers)) = state.copies.get(id)
                        && let Some(template) = state.files[*file].templates.get(name)
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
            for template in state.files[0].templates.values() {
                if !template.broken
                    && !state
                        .copies
                        .iter()
                        .any(|(file, name, _)| *file == 0 && *name == template.name)
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
