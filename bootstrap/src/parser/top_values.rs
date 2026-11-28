//! A value at the top level of a file, `name: T = e`, as
//! `knowledge/language/structs.md` says under "At the top level of a file".
//!
//! It is read here as a function that takes nothing and gives `e`, named
//! `value@name`, which no source can write, and each read of the name in a
//! function of the file is a call of it. So the checker checks the value as it
//! checks any body, in the scope of the file, and the lowering works the value
//! out when the program is built, as `check::to_scalar` says, and gives each
//! call what it worked out.
//!
//! A name that the file defines again where a value can be read, a value that
//! has the name of a function or of another value, an assignment to a value,
//! and a value that is defined by itself, directly or through another value or
//! a function, are refused here.

use super::mut_parameters::{each_block, each_child, each_expr};
use super::*;

/// The name of the function that gives the value `name`.
pub(crate) fn function_name(name: &str) -> String {
    format!("{VALUE}{name}")
}

/// The name of the value a function gives, when it is one.
pub(crate) fn value_name(function: &str) -> Option<&str> {
    function.strip_prefix(VALUE)
}

const VALUE: &str = "value@";

impl Parser<'_> {
    /// Parses a value at the top level and the line break after it, as the
    /// function that gives it. A value that states no type gives no type, and
    /// the checker refuses it, naming the type its value has.
    pub(super) fn top_value(&mut self) -> Result<Function, Error> {
        let (name, pos) = self.defined("the value's name")?;
        let mut result = None;
        if self.tok() == &Tok::Punct(":") {
            self.bump();
            result = Some(self.ty()?);
        }
        self.expect_punct("=", "`=`")?;
        let value = self.expr()?;
        let end = self.pos();
        match self.tok() {
            Tok::Newline | Tok::Eof => {}
            tok => return Err(self.expected("a line break", tok)),
        }
        // A value with no type is read as a binding is, which keeps what its
        // value has, as a whole number keeps its own range.
        let body = match result {
            Some(_) => vec![Stmt::Expr(value)],
            None => vec![
                Stmt::Define {
                    name: VALUE.to_string(),
                    pos,
                    ty: None,
                    var: false,
                    value,
                },
                Stmt::Expr(Expr::Name {
                    name: VALUE.to_string(),
                    pos,
                }),
            ],
        };
        Ok(Function {
            name: function_name(&name),
            pos,
            generics: Vec::new(),
            params: Vec::new(),
            result,
            result_name: None,
            effects: Vec::new(),
            requires: Vec::new(),
            needs: Vec::new(),
            notes: Vec::new(),
            ensures: Vec::new(),
            decreases: Vec::new(),
            body,
            end,
        })
    }
}

/// Makes each read of a value in the functions a call of the function that
/// gives it, and refuses what may not be done with a value's name.
pub(super) fn read_as_calls(functions: &mut [Function], errors: &mut Vec<Error>) {
    let values: Vec<(String, Pos)> = (functions.iter())
        .filter_map(|f| Some((value_name(&f.name)?.to_string(), f.pos)))
        .collect();
    if values.is_empty() {
        return;
    }
    named_twice(functions, &values, errors);
    let names: HashSet<&str> = values.iter().map(|(n, _)| n.as_str()).collect();
    for f in functions.iter_mut() {
        for p in &f.params {
            if names.contains(p.name.as_str()) {
                errors.push(defined_again(&p.name, p.pos));
            }
        }
        rewrite_block(&mut f.body, &names, errors);
    }
    circles(functions, &values, errors);
}

/// The error for a name the file defines where the value of that name can be
/// read.
fn defined_again(name: &str, pos: Pos) -> Error {
    Error::new(
        pos,
        format!("`{name}` is already defined, at the top level of the file"),
    )
}

/// Refuses a value with the name of a function or of a value written before it,
/// and a function with the name of a value written before it.
fn named_twice(functions: &[Function], values: &[(String, Pos)], errors: &mut Vec<Error>) {
    let mut defined: Vec<(Pos, &str, bool)> = values
        .iter()
        .map(|(n, pos)| (*pos, n.as_str(), true))
        .collect();
    defined.extend(
        (functions.iter())
            .filter(|f| !f.name.contains('@'))
            .map(|f| (f.pos, f.name.as_str(), false)),
    );
    defined.sort_by_key(|(pos, ..)| *pos);
    let mut first: HashMap<&str, bool> = HashMap::new();
    for (pos, name, value) in defined {
        match first.get(name) {
            // Two functions of one name are refused where functions are.
            Some(false) if !value => {}
            Some(&was) => {
                let what = if was { "a value" } else { "a function" };
                errors.push(Error::new(
                    pos,
                    format!("there is already {what} named `{name}`"),
                ));
            }
            None => {
                first.insert(name, value);
            }
        }
    }
}

fn rewrite_block(stmts: &mut [Stmt], names: &HashSet<&str>, errors: &mut Vec<Error>) {
    for stmt in stmts.iter_mut() {
        match stmt {
            Stmt::Define {
                name, pos, ty, var, ..
            } if names.contains(name.as_str()) => {
                // A bare `x = e` assigns to `x` where `x` can be read.
                errors.push(if ty.is_none() && !*var {
                    not_assigned(name, *pos)
                } else {
                    defined_again(name, *pos)
                });
            }
            Stmt::Assign { name, pos, .. } if names.contains(name.as_str()) => {
                errors.push(not_assigned(name, *pos));
            }
            Stmt::Destructure { pattern, .. } => binds(pattern, names, errors),
            _ => {}
        }
        each_expr(stmt, &mut |e| rewrite(e, names, errors));
    }
}

/// The error for assigning to the value `name`.
fn not_assigned(name: &str, pos: Pos) -> Error {
    Error::new(
        pos,
        format!(
            "`{name}` is a value at the top level, which is fixed, so it cannot be assigned to"
        ),
    )
}

/// Refuses each name the pattern binds that is the name of a value.
fn binds(pattern: &Pattern, names: &HashSet<&str>, errors: &mut Vec<Error>) {
    match pattern {
        Pattern::Bind { name, pos, .. } if names.contains(name.as_str()) => {
            errors.push(defined_again(name, *pos));
        }
        Pattern::Case {
            inner: Some(inner), ..
        } => binds(inner, names, errors),
        Pattern::Tuple { items, .. } | Pattern::List { items, .. } => {
            for item in items {
                binds(item, names, errors);
            }
        }
        _ => {}
    }
}

fn rewrite(e: &mut Expr, names: &HashSet<&str>, errors: &mut Vec<Error>) {
    if let Expr::Name { name, pos } = e
        && names.contains(name.as_str())
    {
        let pos = *pos;
        *e = Expr::Call(Call {
            through: None,
            qualifier: None,
            type_args: Vec::new(),
            callee: function_name(name),
            pos,
            name_pos: pos,
            args: Vec::new(),
            end: pos,
        });
        return;
    }
    // The names an expression binds for its blocks.
    match e {
        Expr::Match(m) => {
            for arm in &m.arms {
                binds(&arm.pattern, names, errors);
            }
        }
        Expr::Loop(l) => {
            let bound = (l.range.iter().map(|r| (&r.name, r.pos)))
                .chain(l.over.iter().map(|o| (&o.name, o.pos)));
            for (name, pos) in bound {
                if names.contains(name.as_str()) {
                    errors.push(defined_again(name, pos));
                }
            }
        }
        Expr::For(f) => {
            for (name, pos) in &f.indices {
                if names.contains(name.as_str()) {
                    errors.push(defined_again(name, *pos));
                }
            }
        }
        Expr::Lambda(l) => {
            for (name, pos) in &l.params {
                if names.contains(name.as_str()) {
                    errors.push(defined_again(name, *pos));
                }
            }
        }
        _ => {}
    }
    each_child(e, &mut |e| rewrite(e, names, errors));
    each_block(e, &mut |b| rewrite_block(b, names, errors));
}

/// Refuses each value that is defined by itself: one whose value reads it, or
/// calls a function that reads it, directly or through other values and
/// functions.
fn circles(functions: &[Function], values: &[(String, Pos)], errors: &mut Vec<Error>) {
    let index: HashMap<&str, usize> = (functions.iter().enumerate())
        .map(|(i, f)| (f.name.as_str(), i))
        .collect();
    let calls: Vec<Vec<usize>> = (functions.iter())
        .map(|f| {
            let mut found = Vec::new();
            let mut stmts: Vec<Stmt> = f.body.clone();
            for stmt in &mut stmts {
                each_expr(stmt, &mut |e| callees(e, &index, &mut found));
            }
            found
        })
        .collect();
    for (name, pos) in values {
        let start = index[function_name(name).as_str()];
        let mut seen = vec![false; functions.len()];
        let mut stack = calls[start].clone();
        let mut circle = false;
        while let Some(f) = stack.pop() {
            if f == start {
                circle = true;
                break;
            }
            if !std::mem::replace(&mut seen[f], true) {
                stack.extend(&calls[f]);
            }
        }
        if circle {
            errors.push(Error::new(
                *pos,
                format!(
                    "`{name}` is defined by itself, and a value at the top level is worked out when the program is built, from what is already known"
                ),
            ));
        }
    }
}

/// Adds the function each call in the expression names, by its place.
fn callees(e: &mut Expr, index: &HashMap<&str, usize>, found: &mut Vec<usize>) {
    if let Expr::Call(call) = e
        && call.qualifier.is_none()
        && let Some(&f) = index.get(call.callee.as_str())
    {
        found.push(f);
    }
    each_child(e, &mut |e| callees(e, index, found));
    each_block(e, &mut |b| {
        for stmt in b {
            each_expr(stmt, &mut |e| callees(e, index, found));
        }
    });
}
