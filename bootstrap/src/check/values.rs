//! A call of a function, or the construction of a struct, that has whole
//! number parameters and leaves them out: the numbers are taken from the type
//! of an argument or from the type the call must give, and the checker says
//! which copy of the item the call is, as `values` explains.

use super::*;
use crate::values::{Shape, Template};

/// What the type a name stands for is called, and the types and numbers in
/// brackets after it.
enum Part<'t> {
    Ty(&'t Ty),
    Number(u64),
}

fn head<'t>(defs: &Defs, ty: &'t Ty) -> Option<(String, Vec<Part<'t>>)> {
    let (name, args): (&str, &'t [Ty]) = match ty {
        Ty::Struct(s, args) => (&defs.file.structs[*s].name, args),
        Ty::Enum(e, args) => (&defs.file.enums[*e].name, args),
        Ty::List(args) => ("List", args),
        Ty::Slice(args) => ("Slice", args),
        _ => return None,
    };
    match crate::values::copy_of(name) {
        Some((template, numbers)) => Some((
            written_name(template).to_string(),
            numbers.into_iter().map(Part::Number).collect(),
        )),
        None => Some((
            written_name(name).to_string(),
            args.iter().map(Part::Ty).collect(),
        )),
    }
}

/// Reads the numbers the type says off it, where the shape is written with the
/// parameters of the template, and leaves the others as they are.
fn read(defs: &Defs, template: &Template, shape: &Shape, ty: &Ty, found: &mut [Option<u64>]) {
    let (name, args) = match (shape, ty) {
        (Shape::Name(name, args), _) => (name, args),
        (Shape::Array(names, of), Ty::Array(..) | Ty::Matrix(..)) => {
            let (element, sizes) = match ty {
                Ty::Matrix(element, sizes) => (element, sizes.clone()),
                Ty::Array(element, n) => (element, vec![*n]),
                _ => unreachable!("matched"),
            };
            if names.len() != sizes.len() {
                return;
            }
            for (name, n) in names.iter().zip(sizes) {
                if let Some(k) = (name.as_ref())
                    .and_then(|name| template.params.iter().position(|q| q.name == *name))
                {
                    found[k].get_or_insert(n);
                }
            }
            return read(defs, template, of, &element[0], found);
        }
        (Shape::Array(..), _) => return,
    };
    let Some((of, parts)) = head(defs, ty) else {
        return;
    };
    if of != *name || parts.len() != args.len() {
        return;
    }
    for (arg, part) in args.iter().zip(parts) {
        match (arg, part) {
            (Shape::Name(p, inner), Part::Number(n)) if inner.is_empty() => {
                if let Some(k) = template.params.iter().position(|q| q.name == *p) {
                    found[k].get_or_insert(n);
                }
            }
            (_, Part::Ty(t)) => read(defs, template, arg, t, found),
            _ => {}
        }
    }
}

impl<'a> Checker<'a> {
    /// What a call of a name that is a function or a struct with whole number
    /// parameters, which leaves the numbers out, gives: nothing is known of it
    /// until the checker has said which copy it is, which it does here when
    /// the arguments and the place of the call give the numbers, and reports
    /// when they do not.
    pub(super) fn left_out_numbers(
        &mut self,
        body: &mut Body<'a>,
        call: &Call,
        want: &Want,
    ) -> Option<Ty> {
        let template = crate::values::template(&call.callee, call.name_pos)?;
        if call.qualifier.is_some() {
            return None;
        }
        // What is not built in it was reported, and nothing more is said.
        if template.broken {
            return Some(self.unknown_call(body, call));
        }
        let defs = self.defs;
        let mut found: Vec<Option<u64>> = vec![None; template.params.len()];
        let mut unknown = false;
        if let (Want::Ty(wanted), Some(gives)) = (want, &template.gives) {
            unknown |= wanted.unknown();
            read(defs, &template, gives, wanted, &mut found);
        }
        for (i, arg) in call.args.iter().enumerate() {
            let shape = match &arg.name {
                Some((name, _)) => template.inputs.iter().find(|(p, _)| p == name),
                None => template.inputs.get(i),
            }
            .and_then(|(_, shape)| shape.as_ref());
            let Some(shape) = shape else {
                continue;
            };
            if let Shape::Name(_, args) = shape
                && args.is_empty()
            {
                continue;
            }
            let given = self.expr(body, &arg.value, &Want::Any);
            unknown |= given.unknown();
            read(defs, &template, shape, &given, &mut found);
        }
        // What neither an argument nor the type the call must give says is the
        // default of its parameter, if it has one. Where a type is not known
        // yet, a later round says it, so nothing is taken.
        if !unknown {
            for (n, p) in found.iter_mut().zip(&template.params) {
                if n.is_none() {
                    *n = p.default;
                }
            }
        }
        if let Some(numbers) = found.iter().copied().collect::<Option<Vec<u64>>>() {
            crate::values::say(call.name_pos, &template, numbers);
            // What the file is checked with now is not final, and no more of
            // it is lowered.
            self.errors.push(Error::new(
                call.name_pos,
                "which copy this is has been said, and the file is checked again",
            ));
            return Some(Ty::Unknown);
        }
        if !unknown {
            let missing: Vec<&str> = (template.params.iter().zip(&found))
                .filter(|(_, n)| n.is_none())
                .map(|(p, _)| p.name.as_str())
                .collect();
            // A number an argument or the type the call must give says is
            // written as it is, and one that nothing says as an example.
            let written: Vec<String> = (found.iter())
                .map(|n| n.map_or("4".to_string(), |n| n.to_string()))
                .collect();
            self.errors.push(
                Error::new(
                    call.pos,
                    format!(
                        "the whole number `{}` of `{}` is not known here, as no argument and no type that the call must give says it",
                        missing.join("` and `"),
                        template.name
                    ),
                )
                .with_fix(format!(
                    "write it after the name, as `{}[{}]`",
                    template.name,
                    written.join(", ")
                )),
            );
        }
        Some(self.unknown_call(body, call))
    }
}
