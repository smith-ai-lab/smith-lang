//! An argument of a wrapping function, beside another of a wider size, is
//! checked by the range it gives, whatever its form: a name, an `if`, a
//! `match`, or a call is refused exactly as the same value bound to a name
//! first is, and where the other settled the type first, at the argument with
//! the error the name gets. A number written out takes the size the other
//! settled.

use smith::check;

use crate::Test;
use crate::generate::{self, Gen};

pub const ONCE: &[Test] = &[
    (
        "an_argument_of_a_wrapping_function_of_any_form_is_refused_beside_a_wider_size_as_its_name_is",
        an_argument_of_a_wrapping_function_of_any_form_is_refused_beside_a_wider_size_as_its_name_is,
    ),
    (
        "a_number_written_out_takes_the_size_the_other_argument_of_a_wrapping_function_settled",
        a_number_written_out_takes_the_size_the_other_argument_of_a_wrapping_function_settled,
    ),
];

/// The sizes, each with its bits and whether it is signed.
const SIZES: [(&str, u32, bool); 8] = [
    ("u8", 8, false),
    ("u16", 16, false),
    ("u32", 32, false),
    ("u64", 64, false),
    ("i8", 8, true),
    ("i16", 16, true),
    ("i32", 32, true),
    ("i64", 64, true),
];

const FUNCTIONS: [&str; 3] = ["wrapping_add", "wrapping_sub", "wrapping_mul"];

/// The forms of an argument that gives the value of `y`.
const FORMS: [&str; 4] = [
    "y",
    "if b { 1 } else { y }",
    "match b {\n    true { 1 }\n    false { y }\n  }",
    "id(y)",
];

#[derive(Debug)]
struct Case {
    narrow: usize,
    wide: usize,
    function: usize,
    form: usize,
    first: bool,
}

/// Whether every value of size `a` is a value of size `b`, where the two
/// differ.
fn inside(a: usize, b: usize) -> bool {
    let ((_, x, xs), (_, y, ys)) = (SIZES[a], SIZES[b]);
    x < y && (xs == ys || !xs)
}

fn case(g: &mut Gen) -> Case {
    let wides: Vec<usize> = (0..SIZES.len())
        .filter(|&w| (0..SIZES.len()).any(|n| inside(n, w)))
        .collect();
    let wide = *g.pick(&wides);
    let held: Vec<usize> = (0..SIZES.len()).filter(|&n| inside(n, wide)).collect();
    Case {
        narrow: *g.pick(&held),
        wide,
        function: g.below(FUNCTIONS.len()),
        form: g.below(FORMS.len()),
        first: g.one_in(2),
    }
}

/// The program with `argument` beside `s`, after `before`, and the line and
/// column where the argument starts.
fn program(c: &Case, before: &str, argument: &str) -> (String, usize, usize) {
    let (narrow, wide) = (SIZES[c.narrow].0, SIZES[c.wide].0);
    let function = FUNCTIONS[c.function];
    let head = format!(
        "use std.bits ({narrow}, {wide})\nuse std.console (console, print)\nuse std.int ({function})\n\n\
         fn id(v: {narrow}) -> {narrow} {{ v }}\n\n\
         fn f(b: bool, y: {narrow}) -> {wide} {{\n  s: {wide} = 7\n{before}"
    );
    let call = format!("  {function}(");
    let args = if c.first {
        format!("{argument}, s")
    } else {
        format!("s, {argument}")
    };
    let line = head.lines().count() + 1;
    let column = call.len() + 1 + if c.first { 0 } else { "s, ".len() };
    let source = format!(
        "{head}{call}{args})\n}}\n\n\
         fn main() <console, alloc> {{\n  print(\"{{f(true, id(1))}}\")\n}}\n"
    );
    (source, line, column)
}

fn an_argument_of_a_wrapping_function_of_any_form_is_refused_beside_a_wider_size_as_its_name_is()
{
    generate::check(1319, 40, case, |c| {
        let form = FORMS[c.form];
        let (written, line, column) = program(c, "", form);
        let (bound, ..) = program(c, &format!("  x = {form}\n"), "x");
        let written_errors = check(written.as_bytes()).expect_err(&written);
        let bound_errors = check(bound.as_bytes()).expect_err(&bound);
        // Where `s` comes first and settles the type, the argument gets the
        // error its name gets, at its place. Where it comes first, both are
        // refused, as a call waits for the other argument to be checked.
        if !c.first {
            let messages = |errors: &[smith::diag::Error]| -> Vec<String> {
                errors.iter().map(|e| e.message.clone()).collect()
            };
            assert_eq!(messages(&written_errors), messages(&bound_errors), "{written}\n{bound}");
            let at = &written_errors[0].pos;
            assert_eq!((at.line, at.col), (line, column), "{written}");
        }
    });
}

fn a_number_written_out_takes_the_size_the_other_argument_of_a_wrapping_function_settled() {
    generate::check(13190, 12, case, |c| {
        let (source, ..) = program(c, "", "1");
        if let Err(errors) = check(source.as_bytes()) {
            panic!("{errors:?} for {source}");
        }
    });
}
