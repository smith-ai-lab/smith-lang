//! A value at the top level of a file states its type, as every other
//! definition there does, and one that does not is refused at its name, with
//! the fix that writes a type there. The compiler does not build a top-level
//! value yet, so one that states its type is said not to be built, and what is
//! read from it is not checked.

use smith::check;
use smith::diag::Error;

use crate::Test;
use crate::generate::{self, Gen};
use crate::programs::pos_after;

pub const ONCE: &[Test] = &[
    (
        "a_value_at_the_top_level_that_states_no_type_is_refused_at_its_name_with_the_fix_that_writes_one",
        a_value_at_the_top_level_that_states_no_type_is_refused_at_its_name_with_the_fix_that_writes_one,
    ),
    (
        "a_value_at_the_top_level_that_states_its_type_is_not_refused_for_stating_none",
        a_value_at_the_top_level_that_states_its_type_is_not_refused_for_stating_none,
    ),
];

fn errors(source: &str) -> Vec<Error> {
    match check(source.as_bytes()) {
        Ok(_) => Vec::new(),
        Err(errors) => errors,
    }
}

/// What a top-level value may be given, and the type it has.
const VALUES: &[(&str, &str)] = &[
    ("100", "u64"),
    ("\"text\"", "str"),
    ("Rect(w: 1, h: 1)", "Rect"),
    ("[\"fn\", \"if\"]", "List[str]"),
    ("area(Rect(w: 1, h: 1))", "u64"),
];

/// A file with one top-level value, written with or without its type, and a
/// function that does not read it, and the place of its name.
fn file(name: &str, value: &str, ty: Option<&str>, above: usize) -> (String, smith::diag::Pos) {
    let mut out = String::from("use std.bits (u64)\n\nstruct Rect {\n  w: u64\n  h: u64\n}\n\n");
    out += "fn area(r: Rect) -> u64 { r.w * r.h }\n\n";
    for i in 0..above {
        out += &format!("fn above{i}() -> u64 {{ {i} }}\n\n");
    }
    let at = pos_after(&out);
    out += name;
    if let Some(ty) = ty {
        out += &format!(": {ty}");
    }
    out += &format!(" = {value}\n\nfn main() {{}}\n");
    (out, at)
}

/// A value at the top level that states no type is one error, at its name, which
/// says that a type is needed, names the value, and whose fix writes the type
/// after the name, whatever the value is and wherever it is in the file.
fn a_value_at_the_top_level_that_states_no_type_is_refused_at_its_name_with_the_fix_that_writes_one()
{
    generate::check(
        7301,
        40,
        |g: &mut Gen| (g.below(VALUES.len()), g.below(3), g.below(2) == 0),
        |(value, above, long)| {
            let name = if *long { "unit_limit" } else { "limit" };
            let (source, at) = file(name, VALUES[*value].0, None, *above);
            let found = errors(&source);
            let refused: Vec<&Error> = found.iter().filter(|e| e.message.contains("states its type")).collect();
            assert_eq!(refused.len(), 1, "{found:?}\n{source}");
            let e = refused[0];
            assert_eq!(e.pos, at, "{e}\n{source}");
            assert!(!e.unbuilt, "{e}");
            assert!(e.message.contains(&format!("`{name}`")), "{e}");
            let fix = e.fix.as_deref().expect("a fix");
            assert!(fix.contains(&format!("{name}: ")), "{fix}");
        },
    );
}

/// A value at the top level that states its type is not refused for stating none.
fn a_value_at_the_top_level_that_states_its_type_is_not_refused_for_stating_none() {
    generate::check(
        7302,
        40,
        |g: &mut Gen| (g.below(VALUES.len()), g.below(3)),
        |(value, above)| {
            let (source, _) = file("limit", VALUES[*value].0, Some(VALUES[*value].1), *above);
            let found = errors(&source);
            assert!(found.iter().all(|e| !e.message.contains("states its type")), "{found:?}\n{source}");
        },
    );
}
