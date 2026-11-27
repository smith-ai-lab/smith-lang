//! A whole number as a type parameter of a function and of a struct, written
//! `[n: Nat]` or `[n: Pos]`: a type names it, as `Box[10]` and `0..n`, it is
//! a `Nat` in the body, a call may leave it out when an argument or the type
//! the call must give says it, and the executable is the one written with
//! each number put in by hand.

use smith::{check, compile, lower, scalar};

use crate::Test;
use crate::generate::{self, Gen};
use crate::numbers::with_header;
use crate::targets::any;

pub const ONCE: &[Test] = &[
    (
        "the_top_of_a_box_is_one_below_its_size_and_a_box_of_nothing_is_refused_with_the_number",
        the_top_of_a_box_is_one_below_its_size_and_a_box_of_nothing_is_refused_with_the_number,
    ),
    (
        "a_function_of_a_number_is_checked_as_if_the_number_were_written_where_the_name_is",
        a_function_of_a_number_is_checked_as_if_the_number_were_written_where_the_name_is,
    ),
    (
        "a_struct_of_one_number_is_the_type_of_another_exactly_when_the_numbers_are_equal",
        a_struct_of_one_number_is_the_type_of_another_exactly_when_the_numbers_are_equal,
    ),
    (
        "a_number_left_out_of_a_call_is_taken_from_an_argument_or_from_the_type_the_call_gives",
        a_number_left_out_of_a_call_is_taken_from_an_argument_or_from_the_type_the_call_gives,
    ),
    (
        "a_number_that_nothing_says_is_refused_naming_the_parameter",
        a_number_that_nothing_says_is_refused_naming_the_parameter,
    ),
    (
        "the_name_of_a_number_is_a_nat_in_the_body_and_a_bound_in_the_type_of_a_field",
        the_name_of_a_number_is_a_nat_in_the_body_and_a_bound_in_the_type_of_a_field,
    ),
    (
        "a_program_with_numbers_as_parameters_builds_the_bytes_of_the_program_written_with_them_put_in",
        a_program_with_numbers_as_parameters_builds_the_bytes_of_the_program_written_with_them_put_in,
    ),
    (
        "a_pos_is_a_number_above_zero_and_zero_is_refused_where_it_is_written",
        a_pos_is_a_number_above_zero_and_zero_is_refused_where_it_is_written,
    ),
    (
        "a_list_of_parameters_that_has_a_type_beside_a_number_is_refused_as_not_built",
        a_list_of_parameters_that_has_a_type_beside_a_number_is_refused_as_not_built,
    ),
    (
        "a_number_below_zero_is_refused_as_not_a_nat_and_not_as_one_that_is_not_written_out",
        a_number_below_zero_is_refused_as_not_a_nat_and_not_as_one_that_is_not_written_out,
    ),
    (
        "a_copy_that_is_refused_says_which_numbers_it_was_made_for_where_the_item_is_written",
        a_copy_that_is_refused_says_which_numbers_it_was_made_for_where_the_item_is_written,
    ),
    (
        "a_use_that_leaves_a_number_out_and_that_nothing_settles_is_the_use_written_with_its_default_part_1_of_2",
        || a_use_that_leaves_a_number_out_and_that_nothing_settles_is_the_use_written_with_its_default(0),
    ),
    (
        "a_use_that_leaves_a_number_out_and_that_nothing_settles_is_the_use_written_with_its_default_part_2_of_2",
        || a_use_that_leaves_a_number_out_and_that_nothing_settles_is_the_use_written_with_its_default(1),
    ),
    (
        "a_number_that_an_argument_or_the_wanted_type_settles_is_taken_over_the_default",
        a_number_that_an_argument_or_the_wanted_type_settles_is_taken_over_the_default,
    ),
    (
        "a_default_that_its_kind_does_not_hold_is_refused_where_it_is_written",
        a_default_that_its_kind_does_not_hold_is_refused_where_it_is_written,
    ),
    (
        "a_number_with_no_default_after_one_with_a_default_is_refused_at_the_number",
        a_number_with_no_default_after_one_with_a_default_is_refused_at_the_number,
    ),
    (
        "a_use_may_write_the_numbers_before_those_with_a_default_and_no_fewer",
        a_use_may_write_the_numbers_before_those_with_a_default_and_no_fewer,
    ),
];

/// A number a parameter may be: the least and the greatest as often as another.
fn number(g: &mut Gen) -> u64 {
    match g.below(4) {
        0 => 1,
        1 => u64::MAX,
        2 => 1 + g.below(100) as u64,
        _ => (1 + g.below(1 << 30) as u64) << g.below(30),
    }
}

fn stdout(source: &str) -> String {
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for\n{source}"));
    let out = scalar::run(&program);
    assert_eq!(out.status, 0, "for\n{source}");
    String::from_utf8(out.stdout).unwrap()
}

fn compiles(source: &str) -> bool {
    check(source.as_bytes()).is_ok()
}

fn messages(source: &str) -> Vec<String> {
    check(source.as_bytes())
        .err()
        .unwrap_or_default()
        .into_iter()
        .map(|e| e.message)
        .collect()
}

/// The box and the function that makes the top of one, as the issue has them.
const BOX: &str = "struct Box[most: Nat] { at: 0..most }\n\nfn top[most: Nat]() -> Box[most] { Box(at: most - 1) }\n";

/// The program of the issue prints one below each number it is given, and the
/// top of a box that holds nothing is refused, with the number said.
fn the_top_of_a_box_is_one_below_its_size_and_a_box_of_nothing_is_refused_with_the_number() {
    generate::check(4100, 20, |g: &mut Gen| (number(g), number(g)), |(k, j)| {
        let source = with_header(format!(
            "{BOX}\nfn main() <console, alloc> {{\n  print(\"{{top[{k}]().at}}\")\n  b: Box[{j}] = top[{j}]()\n  print(\"{{b.at}}\")\n}}\n"
        ));
        assert_eq!(stdout(&source), format!("{}\n{}\n", k - 1, j - 1), "for\n{source}");
    });
    let source = with_header(format!(
        "{BOX}\nfn main() <console, alloc> {{\n  b = top[0]()\n  print(\"{{b.at}}\")\n}}\n"
    ));
    let found = messages(&source);
    assert!(!found.is_empty(), "for\n{source}");
    assert!(
        found.iter().all(|m| m.contains("`most` as 0")),
        "the error says which number it is for: {found:?}"
    );
}

/// `f[k]` has the body of `f` with `k` where `n` is: a function that gives `n`
/// as a number below a bound compiles for exactly the `k` below the bound.
fn a_function_of_a_number_is_checked_as_if_the_number_were_written_where_the_name_is() {
    generate::check(4101, 30, |g: &mut Gen| (number(g), number(g)), |(k, bound)| {
        let source = with_header(format!(
            "fn under[n: Nat]() -> 0..{bound} {{ n }}\n\nfn main() <console, alloc> {{\n  print(\"{{under[{k}]()}}\")\n}}\n"
        ));
        assert_eq!(compiles(&source), k < bound, "for\n{source}");
        if k < bound {
            assert_eq!(stdout(&source), format!("{k}\n"), "for\n{source}");
        }
    });
}

/// A value of `Box[k]` has the type `Box[j]` exactly when `k == j`.
fn a_struct_of_one_number_is_the_type_of_another_exactly_when_the_numbers_are_equal() {
    generate::check(4102, 30, |g: &mut Gen| {
        let k = number(g);
        (k, if g.one_in(3) { number(g) } else { k })
    }, |(k, j)| {
        let source = with_header(format!(
            "{BOX}\nfn main() <console, alloc> {{\n  a: Box[{k}] = top[{k}]()\n  b: Box[{j}] = a\n  print(\"{{b.at}}\")\n}}\n"
        ));
        assert_eq!(compiles(&source), k == j, "for\n{source}");
    });
}

/// A call that leaves the number out takes it from the type of an argument,
/// from the type it must give, or from a construction that is given a type.
fn a_number_left_out_of_a_call_is_taken_from_an_argument_or_from_the_type_the_call_gives() {
    generate::check(4103, 20, |g: &mut Gen| (number(g), number(g)), |(k, j)| {
        let source = with_header(format!(
            "{BOX}\nfn size[n: Nat](b: Box[n]) -> Nat {{ n }}\n\nfn main() <console, alloc> {{\n  a: Box[{k}] = top()\n  b = Box[{j}](at: 0)\n  c: Box[{j}] = Box(at: 0)\n  print(\"{{size(a)}} {{size(b)}} {{size(c)}} {{size(top[{j}]())}} {{a.at}}\")\n}}\n"
        ));
        assert_eq!(
            stdout(&source),
            format!("{k} {j} {j} {j} {}\n", k - 1),
            "for\n{source}"
        );
    });
}

/// A call whose number no argument and no wanted type gives is refused, and the
/// error names the parameter that is not known.
fn a_number_that_nothing_says_is_refused_naming_the_parameter() {
    generate::check(4104, 6, |g: &mut Gen| g.below(3), |which| {
        let call = ["x = top()", "x = Box(at: 0)", "x = size(top())"][*which];
        let source = with_header(format!(
            "{BOX}\nfn size[n: Nat](b: Box[n]) -> Nat {{ n }}\n\nfn main() <console, alloc> {{\n  {call}\n}}\n"
        ));
        let found = messages(&source);
        assert!(!found.is_empty(), "for\n{source}");
        assert!(
            found.iter().any(|m| m.contains("`most`") || m.contains("`n`")),
            "the error names the parameter: {found:?}\nfor\n{source}"
        );
    });
}

/// The name of a number is a `Nat` in the body, so it is read, added to, and
/// shown, and in the type of a field it is the bound of a range.
fn the_name_of_a_number_is_a_nat_in_the_body_and_a_bound_in_the_type_of_a_field() {
    generate::check(4105, 20, |g: &mut Gen| (number(g), g.below(40)), |(k, below)| {
        let source = with_header(format!(
            "struct Cell[n: Nat] {{ at: 0..n }}\n\nfn shown[n: Nat]() -> Nat {{ n }}\n\nfn main() <console, alloc> {{\n  print(\"{{shown[{k}]()}}\")\n  c: Cell[{k}] = Cell(at: {below})\n  print(\"{{c.at}}\")\n}}\n"
        ));
        assert_eq!(compiles(&source), (*below as u64) < *k, "for\n{source}");
        if (*below as u64) < *k {
            assert_eq!(stdout(&source), format!("{k}\n{below}\n"), "for\n{source}");
        }
    });
}

/// The executable of a program that uses numbers as parameters is the
/// executable of the program written with each number put in by hand.
fn a_program_with_numbers_as_parameters_builds_the_bytes_of_the_program_written_with_them_put_in() {
    generate::check(4106, 8, |g: &mut Gen| (1 + g.below(50), 1 + g.below(50)), |(k, j)| {
        let with = with_header(format!(
            "{BOX}\nfn size[n: Nat](b: Box[n]) -> Nat {{ n }}\n\nfn main() <console, alloc> {{\n  print(\"{{top[{k}]().at}}\")\n  b: Box[{j}] = top()\n  print(\"{{size(b)}}\")\n}}\n"
        ));
        let by_hand = with_header(format!(
            "struct Box{k} {{ at: 0..{k} }}\n\nstruct Box{j} {{ at: 0..{j} }}\n\nfn top{k}() -> Box{k} {{ Box{k}(at: {}) }}\n\nfn top{j}() -> Box{j} {{ Box{j}(at: {}) }}\n\nfn size{j}(b: Box{j}) -> Nat {{ {j} }}\n\nfn main() <console, alloc> {{\n  print(\"{{top{k}().at}}\")\n  b: Box{j} = top{j}()\n  print(\"{{size{j}(b)}}\")\n}}\n",
            k - 1,
            j - 1
        ));
        // Two copies of one function with the same name written twice are one
        // function, so the by-hand program is only written out when they differ.
        if k == j {
            return;
        }
        let a = compile(with.as_bytes(), any()).unwrap_or_else(|e| panic!("{e:?}\n{with}"));
        let b = compile(by_hand.as_bytes(), any()).unwrap_or_else(|e| panic!("{e:?}\n{by_hand}"));
        assert!(a == b, "the executables differ for\n{with}\nand\n{by_hand}");
    });
}

/// `[n: Pos]` is `[n: Nat]` with `0` refused, where it is written and where a
/// call takes it from a type.
fn a_pos_is_a_number_above_zero_and_zero_is_refused_where_it_is_written() {
    generate::check(4107, 20, |g: &mut Gen| number(g) - 1, |k| {
        let source = with_header(format!(
            "fn same[n: Pos]() -> Nat {{ n }}\n\nfn main() <console, alloc> {{\n  print(\"{{same[{k}]()}}\")\n}}\n"
        ));
        assert_eq!(compiles(&source), *k >= 1, "for\n{source}");
        let boxed = with_header(format!(
            "struct Row[n: Pos] {{ at: 0..n }}\n\nfn main() <console, alloc> {{\n  r: Row[{k}] = Row(at: 0)\n  print(\"{{r.at}}\")\n}}\n"
        ));
        assert_eq!(compiles(&boxed), *k >= 1, "for\n{boxed}");
    });
}

/// A list that holds a type and a number is not built, and is refused with
/// that said.
fn a_list_of_parameters_that_has_a_type_beside_a_number_is_refused_as_not_built() {
    let source = with_header(
        "fn pick[T, n: Nat](x: T) -> T { x }\n\nfn main() <console, alloc> {\n  print(\"{pick(1)}\")\n}\n"
            .to_string(),
    );
    let errors = check(source.as_bytes()).expect_err("a type and a number in one list");
    assert!(errors.iter().any(|e| e.unbuilt), "{errors:?}");
}

/// An error found in the copy of an item is at the place the item is written,
/// and says the numbers the copy was made for.
fn a_copy_that_is_refused_says_which_numbers_it_was_made_for_where_the_item_is_written() {
    generate::check(4108, 10, |g: &mut Gen| 1 + g.below(30), |k| {
        let source = with_header(format!(
            "fn bad[n: Nat]() -> 0..{k} {{ n }}\n\nfn main() <console, alloc> {{\n  print(\"{{bad[{k}]()}}\")\n}}\n"
        ));
        let line = source
            .lines()
            .position(|l| l.starts_with("fn bad"))
            .expect("the item is written")
            + 1;
        let errors = check(source.as_bytes()).expect_err("a number that does not fit");
        assert!(
            errors.iter().all(|e| e.pos.line == line && e.message.contains(&format!("`n` as {k}"))),
            "{errors:?}\nfor\n{source}"
        );
    });
}

/// A number below zero is written out, and is refused for what it is: not a
/// `Nat`, where a use of a function or a struct writes it.
fn a_number_below_zero_is_refused_as_not_a_nat_and_not_as_one_that_is_not_written_out() {
    generate::check(4109, 10, |g: &mut Gen| 1 + g.below(1000), |k| {
        for call in [format!("x = twice[-{k}]()"), format!("x: Box[-{k}] = twice[1]()")] {
            let source = with_header(format!(
                "struct Box[n: Nat] {{ at: 0..n }}\n\nfn twice[n: Nat]() -> Nat {{ n }}\n\nfn main() <console, alloc> {{\n  {call}\n}}\n"
            ));
            let found = messages(&source);
            assert!(
                found.iter().any(|m| m.contains(&format!("`-{k}` is not a `Nat`"))),
                "{found:?}\nfor\n{source}"
            );
            assert!(
                found.iter().all(|m| !m.contains("written out")),
                "{found:?}\nfor\n{source}"
            );
        }
    });
}

/// Where `needle` first is in the text, as the place the checker names.
fn place(source: &str, needle: &str) -> smith::diag::Pos {
    let at = source.find(needle).expect("the text is there");
    let before = &source[..at];
    smith::diag::Pos {
        line: before.matches('\n').count() + 1,
        col: before.rsplit('\n').next().unwrap_or("").chars().count() + 1,
    }
}

/// The box and the function of the issue, with a default for the number.
fn defaulted(k: u64) -> String {
    format!(
        "struct Box[most: Nat = {k}] {{ at: 0..most }}\n\nfn top[most: Nat = {k}]() -> Box[most] {{ Box(at: most - 1) }}\n\nfn size[n: Nat = {k}](b: Box[n]) -> Nat {{ n }}\n"
    )
}

/// A function or a struct that a use leaves the number out of, where nothing
/// settles it, is the one written with the default.
fn a_use_that_leaves_a_number_out_and_that_nothing_settles_is_the_use_written_with_its_default(part: u64) {
    generate::check_part(4110, 20, part, 2, |g: &mut Gen| number(g), |k| {
        let left_out = with_header(format!(
            "{}\nfn main() <console, alloc> {{\n  print(\"{{top().at}}\")\n  b: Box = Box(at: 0)\n  print(\"{{size(b)}}\")\n  print(\"{{size(Box(at: 0))}}\")\n}}\n",
            defaulted(*k)
        ));
        let written = with_header(format!(
            "{}\nfn main() <console, alloc> {{\n  print(\"{{top[{k}]().at}}\")\n  b: Box[{k}] = Box[{k}](at: 0)\n  print(\"{{size(b)}}\")\n  print(\"{{size(Box[{k}](at: 0))}}\")\n}}\n",
            defaulted(*k)
        ));
        let said = stdout(&left_out);
        assert_eq!(said, format!("{}\n{k}\n{k}\n", k - 1), "for\n{left_out}");
        assert_eq!(said, stdout(&written), "for\n{left_out}");
        let a = compile(left_out.as_bytes(), any()).unwrap_or_else(|e| panic!("{e:?}\n{left_out}"));
        let b = compile(written.as_bytes(), any()).unwrap_or_else(|e| panic!("{e:?}\n{written}"));
        assert!(a == b, "the executables differ for\n{left_out}\nand\n{written}");
    });
}

/// A number that an argument or the type the use must give says is that
/// number, whatever the default is.
fn a_number_that_an_argument_or_the_wanted_type_settles_is_taken_over_the_default() {
    generate::check(4111, 20, |g: &mut Gen| (number(g), number(g)), |(k, j)| {
        let source = with_header(format!(
            "{}\nfn main() <console, alloc> {{\n  a: Box[{j}] = top()\n  b: Box[{j}] = Box(at: 0)\n  c = top[{j}]()\n  print(\"{{size(a)}} {{size(b)}} {{size(c)}} {{top[{j}]().at}} {{a.at}}\")\n}}\n",
            defaulted(*k)
        ));
        assert_eq!(
            stdout(&source),
            format!("{j} {j} {j} {} {}\n", j - 1, j - 1),
            "for\n{source}"
        );
    });
}

/// A default that is not held by its parameter's type is one error, at the
/// default, naming the number and the parameter.
fn a_default_that_its_kind_does_not_hold_is_refused_where_it_is_written() {
    generate::check(4112, 10, |g: &mut Gen| (g.one_in(2), g.below(1000) as u64), |(item_is_struct, k)| {
        let item = if *item_is_struct { "struct S[n: Pos = 0] { at: 0..n }" } else { "fn f[n: Pos = 0]() -> Nat { n }" };
        let source = with_header(format!("{item}\n\nfn main() <console, alloc> {{\n  print(\"{k}\")\n}}\n"));
        let errors = check(source.as_bytes()).expect_err("a default of zero for a `Pos`");
        let at = place(&source, "= 0");
        let at = smith::diag::Pos { line: at.line, col: at.col + 2 };
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].pos, at, "{errors:?}");
        assert!(errors[0].message.contains("`0`") && errors[0].message.contains("`n`"), "{errors:?}");
        // The same number that is above zero is held.
        let held = source.replace("= 0", &format!("= {}", k + 1));
        assert!(compiles(&held), "for\n{held}");
    });
}

/// A number with no default after one that has a default is one error, at the
/// number, naming the one before it.
fn a_number_with_no_default_after_one_with_a_default_is_refused_at_the_number() {
    generate::check(4113, 10, |g: &mut Gen| (g.one_in(2), 1 + g.below(1000) as u64), |(item_is_struct, k)| {
        let item = if *item_is_struct {
            format!("struct S[a: Nat = {k}, b: Nat] {{ at: 0..b }}")
        } else {
            format!("fn f[a: Nat = {k}, b: Nat]() -> Nat {{ b }}")
        };
        let source = with_header(format!("{item}\n\nfn main() <console, alloc> {{\n  print(\"1\")\n}}\n"));
        let errors = check(source.as_bytes()).expect_err("a number with no default after a default");
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].pos, place(&source, "b: Nat"), "{errors:?}");
        assert!(
            errors[0].message.contains("`b`") && errors[0].message.contains("`a`"),
            "{errors:?}"
        );
    });
}

/// A use writes the numbers that have no default, and may stop there: the
/// ones it leaves off are their defaults. It may not write fewer or more.
fn a_use_may_write_the_numbers_before_those_with_a_default_and_no_fewer() {
    generate::check(4114, 20, |g: &mut Gen| (number(g), number(g)), |(a, b)| {
        let head = format!("struct Grid[w: Nat, h: Nat = {b}] {{ x: 0..w\n  y: 0..h }}\n\nfn cells[w: Nat, h: Nat = {b}](g: Grid[w, h]) -> Nat {{ h }}\n");
        let program = |ty: &str, call: &str| {
            with_header(format!(
                "{head}\nfn main() <console, alloc> {{\n  g: {ty} = Grid(x: 0, y: 0)\n  print(\"{{{call}(g)}}\")\n}}\n"
            ))
        };
        let short = program(&format!("Grid[{a}]"), "cells");
        let full = program(&format!("Grid[{a}, {b}]"), "cells");
        assert_eq!(stdout(&short), format!("{b}\n"), "for\n{short}");
        assert_eq!(stdout(&short), stdout(&full), "for\n{short}");
        for ty in ["Grid[]".to_string(), format!("Grid[{a}, {b}, {a}]")] {
            let source = program(&ty, "cells");
            assert!(!compiles(&source), "for\n{source}");
        }
    });
}
