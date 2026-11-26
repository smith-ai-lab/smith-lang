//! A parameter list is split by `;`: the parameters before it are given by
//! position only, and those after it by name only. The expectation of each
//! program is what the generator that wrote its calls says they print or
//! refuse.

use smith::diag::{Error, Pos};
use smith::scalar::{self, Outcome};
use smith::{check, lower};

use crate::Test;
use crate::generate::{self, Gen};

pub const ONCE: &[Test] = &[
    (
        "a_call_that_gives_each_parameter_as_its_place_in_the_list_says_is_built_and_evaluated_as_it_is_written",
        a_call_that_gives_each_parameter_as_its_place_in_the_list_says_is_built_and_evaluated_as_it_is_written,
    ),
    (
        "a_parameter_before_the_semicolon_that_a_call_names_is_refused_at_its_name",
        a_parameter_before_the_semicolon_that_a_call_names_is_refused_at_its_name,
    ),
    (
        "an_argument_by_position_for_a_parameter_after_the_semicolon_is_refused_naming_the_parameter",
        an_argument_by_position_for_a_parameter_after_the_semicolon_is_refused_naming_the_parameter,
    ),
    (
        "a_parameter_after_the_semicolon_with_no_default_is_needed_at_every_call",
        a_parameter_after_the_semicolon_with_no_default_is_needed_at_every_call,
    ),
    (
        "a_list_with_no_semicolon_is_all_by_position",
        a_list_with_no_semicolon_is_all_by_position,
    ),
    (
        "a_semicolon_is_written_in_a_parameter_list_and_nowhere_else",
        a_semicolon_is_written_in_a_parameter_list_and_nowhere_else,
    ),
    (
        "the_last_parameter_before_the_semicolon_is_the_block_of_a_call",
        the_last_parameter_before_the_semicolon_is_the_block_of_a_call,
    ),
    (
        "a_struct_is_built_with_its_fields_named_and_a_function_type_has_no_semicolon",
        a_struct_is_built_with_its_fields_named_and_a_function_type_has_no_semicolon,
    ),
    (
        "the_library_gives_its_options_by_name_and_the_rest_by_position",
        the_library_gives_its_options_by_name_and_the_rest_by_position,
    ),
    (
        "the_fix_of_a_call_given_the_other_way_is_the_whole_call",
        the_fix_of_a_call_given_the_other_way_is_the_whole_call,
    ),
    (
        "an_implementation_has_the_semicolon_of_the_function_it_implements",
        an_implementation_has_the_semicolon_of_the_function_it_implements,
    ),
];

fn errors(source: &str) -> Vec<Error> {
    check(source.as_bytes()).err().unwrap_or_default()
}

fn outcome(source: &str) -> Outcome {
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for\n{source}"));
    scalar::run(&program)
}

/// A function `f` of up to four parameters `a0`, `a1`, ...: the first `split`
/// are given by position and the rest by name, those that have a default are
/// the last of those by position and any of those by name.
#[derive(Clone, Debug)]
struct Shape {
    count: usize,
    split: usize,
    /// How many of the last parameters by position have a default.
    positional_defaults: usize,
    /// Whether each parameter by name has a default.
    named_defaults: Vec<bool>,
}

impl Shape {
    fn has_default(&self, i: usize) -> bool {
        if i < self.split {
            i + self.positional_defaults >= self.split
        } else {
            self.named_defaults[i - self.split]
        }
    }

    /// The default of a parameter, which is a number of its own.
    fn default(i: usize) -> usize {
        100 + i
    }

    /// The signature of `f`, which takes a `Byte` for each parameter and prints
    /// them in one line.
    fn text(&self) -> String {
        let param = |i: usize| match self.has_default(i) {
            true => format!("a{i}: Byte = {}", Self::default(i)),
            false => format!("a{i}: Byte"),
        };
        let before: Vec<String> = (0..self.split).map(param).collect();
        let after: Vec<String> = (self.split..self.count).map(param).collect();
        let list = if after.is_empty() {
            before.join(", ")
        } else {
            format!("{}; {}", before.join(", "), after.join(", "))
        };
        let shown: Vec<String> = (0..self.count).map(|i| format!("{{a{i}}}")).collect();
        format!(
            "use std.console (console, print)\n\n\
             fn noisy(n: Byte) -> Byte <console, alloc> {{\n  print(\"{{n}}\")\n  n\n}}\n\n\
             fn f({list}) <console, alloc> {{\n  print(\"{}\")\n}}\n\n",
            shown.join(" ")
        )
    }
}

fn shape(g: &mut Gen) -> Shape {
    let count = g.below(5);
    let split = g.below(count + 1);
    Shape {
        count,
        split,
        positional_defaults: g.below(split + 1),
        named_defaults: (split..count).map(|_| g.one_in(2)).collect(),
    }
}

/// A call that is built: which parameters it gives, and in what order those
/// by name are written.
#[derive(Clone, Debug)]
struct Call {
    /// How many of the parameters by position the call gives.
    positional: usize,
    /// The parameters by name that the call gives, in the order it writes them.
    named: Vec<usize>,
}

fn call(g: &mut Gen, shape: &Shape) -> Call {
    let trailing = shape.positional_defaults;
    // A default left out before an argument by name is filled in after it, as
    // the compiler's order of arguments says, which only prints the same.
    let positional = shape.split - g.below(trailing + 1);
    let mut rest: Vec<usize> = (shape.split..shape.count)
        .filter(|&i| !shape.has_default(i) || g.one_in(2))
        .collect();
    let mut named = Vec::new();
    while !rest.is_empty() {
        named.push(rest.remove(g.below(rest.len())));
    }
    Call { positional, named }
}

/// The value the argument number `k` that a call writes is.
fn value(k: usize) -> usize {
    10 + k
}

/// The text of the call, with each argument a call of `noisy`, which prints
/// its value when it is worked out.
fn written(call: &Call) -> String {
    let mut args = Vec::new();
    let mut k = 0;
    for _ in 0..call.positional {
        args.push(format!("noisy({})", value(k)));
        k += 1;
    }
    for &i in &call.named {
        args.push(format!("a{i}: noisy({})", value(k)));
        k += 1;
    }
    format!("f({})", args.join(", "))
}

/// What the call prints: the value of each argument as it is worked out, in
/// the order they are written, then the line of the function.
fn prints(shape: &Shape, call: &Call) -> Vec<u8> {
    let mut said = String::new();
    let mut given = vec![None; shape.count];
    for (k, slot) in given.iter_mut().enumerate().take(call.positional) {
        *slot = Some(value(k));
        said += &format!("{}\n", value(k));
    }
    for (n, &i) in call.named.iter().enumerate() {
        let k = call.positional + n;
        given[i] = Some(value(k));
        said += &format!("{}\n", value(k));
    }
    let line: Vec<String> = (0..shape.count)
        .map(|i| given[i].unwrap_or(Shape::default(i)).to_string())
        .collect();
    said += &line.join(" ");
    said += "\n";
    said.into_bytes()
}

fn main_with(call: &str) -> String {
    format!("fn main() <console, alloc> {{\n  {call}\n}}\n")
}

/// The call that a fix says to write, from the text `write `call``.
fn suggested(fix: &str) -> &str {
    (fix.strip_prefix("write `").and_then(|f| f.strip_suffix('`')))
        .unwrap_or_else(|| panic!("a fix that writes a call: {fix}"))
}

/// Where `needle` is in the source: its line and its column.
fn place(source: &str, needle: &str) -> Pos {
    let at = source.find(needle).expect("the text is in the source");
    let before = &source[..at];
    Pos {
        line: before.matches('\n').count() + 1,
        col: at - before.rfind('\n').map_or(0, |n| n + 1) + 1,
    }
}

/// A call that gives each parameter the way its place in the list says, with
/// those by name in any order, is built, works out its arguments in the order
/// they are written, and gives each parameter its argument, or its default
/// when it leaves the argument out.
fn a_call_that_gives_each_parameter_as_its_place_in_the_list_says_is_built_and_evaluated_as_it_is_written()
 {
    generate::check(
        8801,
        60,
        |g| {
            let shape = shape(g);
            let call = call(g, &shape);
            (shape, call)
        },
        |(shape, call)| {
            let source = shape.text() + &main_with(&written(call));
            assert_eq!(outcome(&source).stdout, prints(shape, call), "for\n{source}");
        },
    );
}

/// A parameter before `;` that a call writes with its name is refused at the
/// name, which says the parameter is given by position.
fn a_parameter_before_the_semicolon_that_a_call_names_is_refused_at_its_name() {
    generate::check(
        8802,
        60,
        |g| {
            let mut shape = shape(g);
            if shape.split == 0 {
                shape.count += 1;
                shape.split = 1;
                shape.named_defaults.clear();
                shape.positional_defaults = 0;
                shape.named_defaults = (shape.split..shape.count).map(|_| true).collect();
            }
            let at = g.below(shape.split);
            (shape, at)
        },
        |(shape, at)| {
            // Every parameter is given, the one at `at` with its name.
            let args: Vec<String> = (0..shape.count)
                .map(|i| match i {
                    i if i == *at => format!("a{i}: {}", value(i)),
                    i if i >= shape.split => format!("a{i}: {}", value(i)),
                    i => value(i).to_string(),
                })
                .collect();
            let call = format!("f({})", args.join(", "));
            let source = shape.text() + &main_with(&call);
            let name = format!("a{at}: ");
            let found = errors(&source);
            let wanted = place(&source[source.find("fn main").expect("main")..], &name);
            let main_line = place(&source, "fn main").line;
            let wanted = Pos {
                line: main_line + wanted.line - 1,
                col: wanted.col,
            };
            assert!(
                found.iter().any(|e| e.pos == wanted
                    && e.message.contains(&format!("`a{at}`"))
                    && e.message.contains("`f`")
                    && e.message.contains("by position")),
                "{found:?} for\n{source}"
            );
            // The fix is the call with that name taken off and every other
            // argument as it was, which builds and gives what the call that
            // names the parameters gives.
            let fix = found
                .iter()
                .find(|e| e.pos == wanted)
                .and_then(|e| e.fix.as_deref())
                .unwrap_or_else(|| panic!("a fix in {found:?}"));
            let fixed = shape.text() + &main_with(suggested(fix));
            let plain: Vec<String> = (0..shape.count)
                .map(|i| match i >= shape.split {
                    true => format!("a{i}: {}", value(i)),
                    false => value(i).to_string(),
                })
                .collect();
            let reference = shape.text() + &main_with(&format!("f({})", plain.join(", ")));
            assert!(errors(&fixed).is_empty(), "{:?} for\n{fixed}", errors(&fixed));
            assert_eq!(outcome(&fixed).stdout, outcome(&reference).stdout, "for\n{fixed}");
        },
    );
}

/// An argument by position for a parameter after `;` is refused, and the
/// message names that parameter and says it is given by name.
fn an_argument_by_position_for_a_parameter_after_the_semicolon_is_refused_naming_the_parameter() {
    generate::check(
        8803,
        60,
        |g| {
            let mut shape = shape(g);
            if shape.split == shape.count {
                shape.count += 1;
                shape.named_defaults = vec![g.one_in(2)];
            }
            shape
        },
        |shape| {
            // Every parameter by position is given, then one more by position,
            // and the parameters by name that are needed after it are named.
            let mut args: Vec<String> = (0..shape.split)
                .map(|i| format!("noisy({})", value(i)))
                .collect();
            args.push("noisy(77)".to_string());
            let others: Vec<String> = (shape.split + 1..shape.count)
                .filter(|&i| !shape.has_default(i))
                .map(|i| format!("a{i}: noisy({})", value(i)))
                .collect();
            let call = format!("f({})", [args.clone(), others.clone()].concat().join(", "));
            let source = shape.text() + &main_with(&call);
            let found = errors(&source);
            let name = format!("`a{}`", shape.split);
            let wanted = place(&source, "noisy(77)");
            assert!(
                found.iter().any(|e| e.pos == wanted
                    && e.message.contains(&name)
                    && e.message.contains("`f`")
                    && e.message.contains("by name")),
                "{found:?} for\n{source}"
            );
            // The fix is the whole call with that argument named and every
            // other as it was, which builds and gives what the call written with
            // the names gives.
            let fix = found
                .iter()
                .find(|e| e.pos == wanted)
                .and_then(|e| e.fix.as_deref())
                .unwrap_or_else(|| panic!("a fix in {found:?}"));
            let named = format!("a{}: noisy(77)", shape.split);
            let reference = format!(
                "f({})",
                [args[..args.len() - 1].to_vec(), vec![named], others].concat().join(", ")
            );
            assert_eq!(suggested(fix), reference, "for\n{source}");
            let fixed = shape.text() + &main_with(suggested(fix));
            let reference = shape.text() + &main_with(&reference);
            assert!(errors(&fixed).is_empty(), "{:?} for\n{fixed}", errors(&fixed));
            assert_eq!(outcome(&fixed).stdout, outcome(&reference).stdout, "for\n{fixed}");
        },
    );
}

/// A parameter after `;` with no default is named at every call, and a call
/// that leaves it out is refused naming the function and the parameter.
fn a_parameter_after_the_semicolon_with_no_default_is_needed_at_every_call() {
    generate::check(
        8804,
        60,
        |g| {
            let mut shape = shape(g);
            if shape.split == shape.count {
                shape.count += 1;
                shape.named_defaults = vec![false];
            }
            let required: Vec<usize> = (shape.split..shape.count)
                .filter(|&i| !shape.has_default(i))
                .collect();
            let left_out = if required.is_empty() {
                shape.named_defaults[0] = false;
                shape.split
            } else {
                required[g.below(required.len())]
            };
            (shape, left_out)
        },
        |(shape, left_out)| {
            let call = Call {
                positional: shape.split,
                named: (shape.split..shape.count)
                    .filter(|&i| i != *left_out && !shape.has_default(i))
                    .collect(),
            };
            let source = shape.text() + &main_with(&written(&call));
            let found = errors(&source);
            assert!(
                found.iter().any(|e| e.message.contains("`f`")
                    && e.message.contains(&format!("`a{left_out}`"))
                    && e.message.contains("needs")),
                "{found:?} for\n{source}"
            );
        },
    );
}

/// A list with no `;` is all by position: a call that names any of its
/// parameters is refused, and a call that gives them by position is built.
fn a_list_with_no_semicolon_is_all_by_position() {
    generate::check(
        8805,
        40,
        |g| {
            let count = 1 + g.below(4);
            (count, g.below(count))
        },
        |(count, named)| {
            let shape = Shape {
                count: *count,
                split: *count,
                positional_defaults: 0,
                named_defaults: Vec::new(),
            };
            let plain = Call {
                positional: *count,
                named: Vec::new(),
            };
            let source = shape.text() + &main_with(&written(&plain));
            assert_eq!(outcome(&source).stdout, prints(&shape, &plain), "for\n{source}");
            let args: Vec<String> = (0..*count)
                .map(|i| match i == *named {
                    true => format!("a{i}: {}", value(i)),
                    false => value(i).to_string(),
                })
                .collect();
            let source = shape.text() + &main_with(&format!("f({})", args.join(", ")));
            let found = errors(&source);
            assert!(
                found.iter().any(|e| e.message.contains(&format!("`a{named}`"))
                    && e.message.contains("by position")),
                "{found:?} for\n{source}"
            );
        },
    );
}

/// `;` is written in a parameter list, once, and is refused in a call, between
/// statements, and a second time in a list.
fn a_semicolon_is_written_in_a_parameter_list_and_nowhere_else() {
    let header = "use std.console (console, print)\n\nfn f(a: Byte; b: Byte = 2) { }\n\n";
    for (body, at) in [
        ("fn main() <console> {\n  f(1; 2)\n}\n", "; 2"),
        ("fn main() <console> {\n  f(1)\n  x = 1; y = 2\n}\n", "; y"),
        ("fn main() <console> {\n  f(1);\n}\n", ");"),
    ] {
        let source = format!("{header}{body}");
        let found = errors(&source);
        let wanted = place(&source, at);
        let wanted = Pos {
            col: wanted.col + at.find(';').expect("a semicolon"),
            ..wanted
        };
        assert!(
            found.iter().any(|e| e.pos == wanted && e.message.contains("`;`")),
            "{found:?} for\n{source}"
        );
    }
    for list in ["a: Byte; b: Byte; c: Byte", "a: Byte;; b: Byte", "a: Byte;", ";"] {
        let source = format!("fn f({list}) {{ }}\n\nfn main() {{\n}}\n");
        assert!(
            errors(&source).iter().any(|e| e.message.contains("`;`")),
            "built: {source}"
        );
    }
    // A parameter written as a pattern has no name to give, so it is before `;`.
    let source = "fn f(a: Byte; (x, y): (Byte, Byte)) { }\n\nfn main() {\n}\n";
    assert!(
        errors(source).iter().any(|e| e.message.contains("pattern") && e.message.contains("`;`")),
        "{:?}",
        errors(source)
    );
    for list in ["; a: Byte", "a: Byte; b: Byte"] {
        let source = format!("fn f({list}) {{ }}\n\nfn main() {{\n}}\n");
        assert!(errors(&source).iter().all(|e| !e.message.contains("`;`")), "{source}");
    }
}

/// The last parameter before `;` that takes a function is the block of a call,
/// and the others are given inside the parentheses: by name, those after `;`.
fn the_last_parameter_before_the_semicolon_is_the_block_of_a_call() {
    generate::check(
        8806,
        40,
        |g| {
            let class = (b'a' + g.below(26) as u8) as char;
            let inside = (b'a' + g.below(26) as u8) as char;
            (class, inside, g.one_in(2))
        },
        |(class, inside, left_out)| {
            let program = |call: &str| {
                format!(
                    "use std.console (console, print)\n\n\
                     fn p(block: fn() <console>; class: str = \"c\", id: str = \"i\") <console> {{\n  \
                     print(class)\n  print(id)\n  block()\n}}\n\n\
                     fn a(block: fn() <console>; href: str, class: str = \"c\") <console> {{\n  \
                     print(href)\n  block()\n}}\n\n\
                     fn main() <console> {{\n  {call}\n}}\n"
                )
            };
            let by_name = if *left_out {
                "p() { print(\"in\") }".to_string()
            } else {
                format!("p(class: \"{class}\", id: \"{inside}\") {{ print(\"in\") }}")
            };
            let said = outcome(&program(&by_name)).stdout;
            let (printed_class, printed_id) = if *left_out {
                ("c".to_string(), "i".to_string())
            } else {
                (class.to_string(), inside.to_string())
            };
            assert_eq!(said, format!("{printed_class}\n{printed_id}\nin\n").into_bytes());
            // Given by position, the block is refused as the parameter by name that
            // the text goes to, with the fix that names it.
            let source = program(&format!("p(\"{class}\") {{ print(\"in\") }}"));
            let found = errors(&source);
            let call_at = place(&source, "p(\"");
            let wanted = Pos {
                col: call_at.col + 2,
                ..call_at
            };
            let refused = found.iter().find(|e| e.pos == wanted);
            let refused = refused.unwrap_or_else(|| panic!("{found:?} for\n{source}"));
            assert!(
                refused.message.contains("`class`")
                    && refused.message.contains("`p`")
                    && refused.message.contains("by name"),
                "{refused:?}"
            );
            assert!(
                refused.fix.as_deref().is_some_and(|f| f.contains(&format!("class: \"{class}\""))),
                "{refused:?}"
            );
            // A parameter by name with no default is needed.
            let source = program("a { print(\"in\") }");
            let found = errors(&source);
            assert!(
                found.iter().any(|e| e.message.contains("`a`") && e.message.contains("`href`")),
                "{found:?}"
            );
        },
    );
}

/// A struct is built with its fields named, and a function type has no `;`, so
/// the arguments of a function that a name holds are by position.
fn a_struct_is_built_with_its_fields_named_and_a_function_type_has_no_semicolon() {
    let source = "use std.console (console, print)\n\nstruct Cursor {\n  at: Nat\n  to: Nat = 0\n}\n\n\
                  fn twice(f: fn(Nat) -> Nat, n: Nat) -> Nat { f(f(n)) }\n\n\
                  fn next(n: Nat) -> Nat { n }\n\n\
                  fn main() <console, alloc> {\n  c = Cursor(at: 4)\n  print(\"{c.at} {twice(next, 1)}\")\n}\n";
    assert_eq!(outcome(source).stdout, b"4 1\n");
    let by_position = source.replace("Cursor(at: 4)", "Cursor(4)");
    assert!(!errors(&by_position).is_empty());
    let named = source.replace("f(f(n))", "f(x: f(n))");
    assert!(
        errors(&named).iter().any(|e| e.message.contains("by position")),
        "{:?}",
        errors(&named)
    );
}

/// The options of the library are given by name and the rest by position: the
/// attributes of `std.html`, `width` and `upper` of `std.fmt`, and `end` and
/// `to` of `print`.
fn the_library_gives_its_options_by_name_and_the_rest_by_position() {
    let program = |body: &str| {
        format!(
            "use std.console (console, print)\nuse std.fmt (binary, hex)\n\n\
             fn main() <console, alloc> {{\n  {body}\n}}\n"
        )
    };
    let built = |body: &str| errors(&program(body)).is_empty();
    assert!(built("print(hex(233, width: 4, upper: false))"));
    assert!(built("print(hex(233, upper: false, width: 4))"));
    assert!(built("print(binary(5, width: 8))"));
    assert!(built("print(\"a\", end: \"\", to: .stderr)"));
    for body in [
        "print(hex(233, 4))",
        "print(binary(5, 8))",
        "print(\"a\", \"\")",
    ] {
        let found = errors(&program(body));
        assert!(
            found.iter().any(|e| e.message.contains("by name")),
            "{found:?} for {body}"
        );
    }
    let found = errors(&program("print(hex(n: 233))"));
    assert!(
        found.iter().any(|e| e.message.contains("`n`") && e.message.contains("`hex`") && e.message.contains("by position")),
        "{found:?}"
    );
    let found = errors(&program("print(text: \"a\")"));
    assert!(
        found.iter().any(|e| e.message.contains("`text`") && e.message.contains("by position")),
        "{found:?}"
    );
    // The attributes of an element are by name, and its block is the one
    // argument it takes by position.
    let page = |call: &str| {
        format!(
            "use std.html (a, link, p, render, text)\n\n\
             fn main() <alloc> {{\n  page = render() {{\n    {call}\n  }}\n  same = page\n}}\n"
        )
    };
    assert!(errors(&page("p(class: \"note\") { text(\"x\") }")).is_empty());
    assert!(errors(&page("a(href: link(\"/\")) { text(\"Home\") }")).is_empty());
    let found = errors(&page("p(\"Hello\") { text(\"x\") }"));
    assert!(
        found.iter().any(|e| e.message.contains("`class`")
            && e.message.contains("`p`")
            && e.message.contains("by name")
            && e.fix.as_deref().is_some_and(|f| f.contains("class: \"Hello\""))),
        "{found:?}"
    );
    let found = errors(&page("a { text(\"Home\") }"));
    assert!(
        found.iter().any(|e| e.message.contains("`a`") && e.message.contains("`href`")),
        "{found:?}"
    );
}

/// The fix of a call given the other way is the whole call, written again with
/// every other argument as it was written, a block among them.
fn the_fix_of_a_call_given_the_other_way_is_the_whole_call() {
    let program = |call: &str| {
        format!(
            "use std.console (console, print)\nuse std.html (p, render, text)\n\n\
             fn show(label: str; n: str = \"x\") <console> {{ print(label) }}\n\n\
             fn main() <console, alloc> {{\n  {call}\n}}\n"
        )
    };
    let fix_of = |call: &str, message: &str| -> String {
        let found = errors(&program(call));
        let found = found.iter().find(|e| e.message.contains(message));
        let found = found.unwrap_or_else(|| panic!("{message} for {call}"));
        found.fix.clone().expect("a fix")
    };
    assert_eq!(
        fix_of("show(\"a\", \"N\")", "is given by name"),
        "write `show(\"a\", n: \"N\")`"
    );
    assert_eq!(
        fix_of("render { p({ text(\"a\") }, \"note\") }", "is given by name"),
        "write `p({ text(\"a\") }, class: \"note\")`"
    );
    assert_eq!(
        fix_of("render { p(\"note\") { text(\"a\") } }", "is given by name"),
        "write `p(class: \"note\") { text(\"a\") }`"
    );
    assert_eq!(
        fix_of("show(label: \"a\", n: \"N\")", "is given by position"),
        "write `show(\"a\", n: \"N\")`"
    );
}

/// An implementation writes the function it implements as the trait does,
/// with the same parameters after `;`, and one that differs is refused.
fn an_implementation_has_the_semicolon_of_the_function_it_implements() {
    let program = |in_trait: &str, in_impl: &str| {
        format!(
            "use std.console (console, print)\n\nstruct Cat {{\n  name: str\n}}\n\n\
             trait Named[T] {{\n  fn name_of({in_trait}) -> str\n}}\n\n\
             impl Named[Cat] {{\n  fn name_of({in_impl}) -> str {{ x.name }}\n}}\n\n\
             fn main() <console> {{\n  print(name_of(Cat(name: \"Tom\"), title: \"Mr\"))\n}}\n"
        )
    };
    let same = "x: T; title: str = \"Dr\"";
    let cat = "x: Cat; title: str";
    assert!(errors(&program(same, cat)).is_empty(), "{:?}", errors(&program(same, cat)));
    let by_position = "x: Cat, title: str";
    let found = errors(&program(same, by_position));
    assert!(
        found.iter().any(|e| e.message.contains("differs from its signature")),
        "{found:?}"
    );
}
