//! An array of a size written as a number is a type, held in place: two arrays
//! are one type exactly when their sizes and elements are, a literal of
//! another length is refused with both numbers, an element is read and
//! assigned where `arith` proves the index, and an array asks for no memory.

use smith::scalar::{self, Op, System};
use smith::{check, lower};

use crate::Test;
use crate::generate::{self, Gen};
use crate::numbers::with_header;

pub const ONCE: &[Test] = &[
    (
        "an_array_is_held_by_a_parameter_a_result_a_binding_a_var_a_field_a_case_and_a_list_and_reads_what_it_was_given",
        an_array_is_held_by_a_parameter_a_result_a_binding_a_var_a_field_a_case_and_a_list_and_reads_what_it_was_given,
    ),
    (
        "two_arrays_are_one_type_exactly_when_their_sizes_and_elements_are",
        two_arrays_are_one_type_exactly_when_their_sizes_and_elements_are,
    ),
    (
        "a_literal_of_the_wrong_number_of_elements_is_refused_and_the_error_gives_both_numbers",
        a_literal_of_the_wrong_number_of_elements_is_refused_and_the_error_gives_both_numbers,
    ),
    (
        "an_index_is_proven_below_the_size_or_refused_and_an_element_is_assigned_where_it_is_proven",
        an_index_is_proven_below_the_size_or_refused_and_an_element_is_assigned_where_it_is_proven,
    ),
    (
        "an_element_is_read_and_assigned_at_an_index_the_program_works_out_as_a_vector_does",
        an_element_is_read_and_assigned_at_an_index_the_program_works_out_as_a_vector_does,
    ),
    (
        "an_array_that_is_indexed_is_written_once_and_reading_it_again_stores_nothing",
        an_array_that_is_indexed_is_written_once_and_reading_it_again_stores_nothing,
    ),
    (
        "an_array_written_with_no_type_is_as_long_as_written_and_is_taken_as_a_list_of_a_wider_element",
        an_array_written_with_no_type_is_as_long_as_written_and_is_taken_as_a_list_of_a_wider_element,
    ),
    (
        "an_array_written_with_no_type_whose_elements_no_one_type_holds_or_that_has_none_is_refused",
        an_array_written_with_no_type_whose_elements_no_one_type_holds_or_that_has_none_is_refused,
    ),
    (
        "a_function_that_makes_reads_and_returns_arrays_has_no_alloc_and_calls_no_allocator",
        a_function_that_makes_reads_and_returns_arrays_has_no_alloc_and_calls_no_allocator,
    ),
];

/// The types an element may be written as, and a number of each that the
/// generator picks from.
const ELEMS: &[&str] = &["u8", "u32", "i64", "Byte", "bool"];

fn literal(ty: &str, k: usize) -> String {
    match ty {
        "bool" => k.is_multiple_of(2).to_string(),
        _ => (k + 1).to_string(),
    }
}

/// The program with the imports it needs, and the whole number types an
/// element may be, which a program that does not write one leaves out.
fn header(program: String) -> String {
    let uses = ["u8", "u32", "i64"]
        .into_iter()
        .filter(|t| program.contains(t))
        .collect::<Vec<_>>();
    let imports = if uses.is_empty() {
        String::new()
    } else {
        format!("use std.bits ({})\n", uses.join(", "))
    };
    with_header(imports + &program)
}

fn lowered(source: &str) -> scalar::Program {
    lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for\n{source}"))
}

fn stdout(source: &str) -> String {
    let out = scalar::run(&lowered(source));
    assert_eq!(out.status, 0, "for\n{source}");
    String::from_utf8(out.stdout).unwrap()
}

fn compiles(source: &str) -> bool {
    check(source.as_bytes()).is_ok()
}

/// Every place an array may be held holds it, and every element is read as it
/// was written, for each element type and each size.
fn an_array_is_held_by_a_parameter_a_result_a_binding_a_var_a_field_a_case_and_a_list_and_reads_what_it_was_given() {
    generate::check(4101, 8, |g: &mut Gen| (*g.pick(ELEMS), 1 + g.below(5)), |(ty, n)| {
        let items: Vec<String> = (0..*n).map(|k| literal(ty, k)).collect();
        let written = format!("[{}]", items.join(", "));
        let reads = |name: &str| {
            (0..*n)
                .map(|k| format!("  print(\"{{{name}[{k}]}}\")\n"))
                .collect::<String>()
        };
        let source = header(format!(
            "struct Holder {{\n  a: [{n}]{ty}\n  n: u8\n}}\n\nenum Either {{\n  array: [{n}]{ty}\n  nothing\n}}\n\nfn pass_on(a: [{n}]{ty}) -> [{n}]{ty} {{ a }}\n\nfn from_case(e: Either) -> [{n}]{ty} {{\n  match e {{\n    .array(a) {{ a }}\n    .nothing {{ {written} }}\n  }}\n}}\n\nfn main() ! console, alloc {{\n  bound = pass_on({written})\n  var held: [{n}]{ty} = bound\n  holder = Holder(a: held, n: 1)\n  inside = from_case(Either.array(holder.a))\n  listed: List[[{n}]{ty}] = [inside]\n  last = listed[0]\n{}}}\n",
            reads("last")
        ));
        let expected: String = items.iter().map(|i| format!("{i}\n")).collect();
        assert_eq!(stdout(&source), expected, "for\n{source}");
    });
}

/// `[n]T` and `[m]U` are one type exactly when `n` is `m` and `T` is `U`.
fn two_arrays_are_one_type_exactly_when_their_sizes_and_elements_are() {
    generate::check(4102, 12, |g: &mut Gen| {
        (*g.pick(ELEMS), *g.pick(ELEMS), 1 + g.below(4), 1 + g.below(4))
    }, |(t, u, n, m)| {
        let items: Vec<String> = (0..*n).map(|k| literal(t, k)).collect();
        let source = header(format!(
            "fn hold(a: [{m}]{u}) -> [{m}]{u} {{ a }}\n\nfn main() ! console {{\n  a: [{n}]{t} = [{}]\n  b = hold(a)\n}}\n",
            items.join(", ")
        ));
        // `Byte` is `u8` written another way.
        fn one(t: &str) -> &str {
            if t == "Byte" { "u8" } else { t }
        }
        assert_eq!(
            compiles(&source),
            one(t) == one(u) && n == m,
            "for\n{source}"
        );
    });
}

/// A literal that has other than the number of elements the type has is
/// refused, and the error says how many it has and how many its type has.
fn a_literal_of_the_wrong_number_of_elements_is_refused_and_the_error_gives_both_numbers() {
    generate::check(4103, 12, |g: &mut Gen| (1 + g.below(6), g.below(8)), |(n, written)| {
        let items = vec!["1"; *written].join(", ");
        let source = header(format!(
            "fn main() ! console {{\n  a: [{n}]u8 = [{items}]\n}}\n"
        ));
        match check(source.as_bytes()) {
            Ok(_) => assert_eq!(n, written, "for\n{source}"),
            Err(errors) => {
                assert_ne!(n, written, "for\n{source}");
                let message = &errors[0].message;
                assert!(
                    message.contains(&n.to_string()) && message.contains(&written.to_string()),
                    "{message} for\n{source}"
                );
            }
        }
    });
}

/// `a[i]` compiles when `arith` proves `i` below the size, from the type of
/// the index or an `if`, and is refused otherwise, the elements of a `var`
/// are assigned the same way, and the array that was passed is unchanged.
fn an_index_is_proven_below_the_size_or_refused_and_an_element_is_assigned_where_it_is_proven() {
    generate::check(4104, 12, |g: &mut Gen| (1 + g.below(5), g.below(7)), |(n, i)| {
        let items: Vec<String> = (0..*n).map(|k| literal("u8", k)).collect();
        let read = header(format!(
            "fn main() ! console, alloc {{\n  a: [{n}]u8 = [{}]\n  print(\"{{a[{i}]}}\")\n}}\n",
            items.join(", ")
        ));
        assert_eq!(compiles(&read), i < n, "for\n{read}");
        let typed = header(format!(
            "fn at(a: [{n}]u8, i: 0..{n}) -> u8 {{ a[i] }}\n\nfn tested(a: [{n}]u8, i: Nat) -> u8 {{\n  if i < {n} {{ a[i] }} else {{ 0 }}\n}}\n\nfn main() ! console {{\n  a: [{n}]u8 = [{}]\n  x = at(a, 0)\n  y = tested(a, {i})\n}}\n",
            items.join(", ")
        ));
        assert!(compiles(&typed), "for\n{typed}");
        let unproven = header(format!(
            "fn tested(a: [{n}]u8, i: Nat) -> u8 {{ a[i] }}\n\nfn main() ! console {{\n  a: [{n}]u8 = [{}]\n  y = tested(a, 0)\n}}\n",
            items.join(", ")
        ));
        // `arith` refuses it, saying what it could not prove of the size.
        let errors = check(unproven.as_bytes()).expect_err(&unproven);
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains(&format!("cannot prove that `i < {n}`"))),
            "{errors:?} for\n{unproven}"
        );
        let assigned = header(format!(
            "fn bump(a: [{n}]u8) -> [{n}]u8 {{\n  var b = a\n  b[{i}] = 100\n  b\n}}\n\nfn main() ! console, alloc {{\n  a: [{n}]u8 = [{}]\n  b = bump(a)\n  print(\"{{a[{i}]}} {{b[{i}]}}\")\n}}\n",
            items.join(", ")
        ));
        assert_eq!(compiles(&assigned), i < n, "for\n{assigned}");
        if i < n {
            assert_eq!(stdout(&assigned), format!("{} 100\n", items[*i]), "for\n{assigned}");
        }
    });
}

/// A function that makes, reads, and returns arrays has no `alloc`, and the
/// program it is part of never calls the allocator.
fn a_function_that_makes_reads_and_returns_arrays_has_no_alloc_and_calls_no_allocator() {
    generate::check(4105, 6, |g: &mut Gen| (*g.pick(ELEMS), 1 + g.below(6)), |(ty, n)| {
        let items: Vec<String> = (0..*n).map(|k| literal(ty, k)).collect();
        let source = header(format!(
            "fn make() -> [{n}]{ty} {{ [{}] }}\n\nfn last(a: [{n}]{ty}) -> {ty} {{ a[{}] }}\n\nfn main() ! console {{\n  a = make()\n  b = last(a)\n  print(\"done\")\n}}\n",
            items.join(", "),
            n - 1
        ));
        let program = lowered(&source);
        let allocates = program.functions.iter().any(|f| {
            f.blocks.iter().any(|b| {
                b.instructions
                    .iter()
                    .any(|i| matches!(i.op, Op::System { call: System::Allocate, .. }))
            })
        });
        assert!(!allocates, "for\n{source}");
    });
}

/// `set` and `get` take the index as a parameter, so it is a number the program
/// works out and not one written out. Whatever order they are called in, every
/// element is what a vector given the same writes holds.
fn an_element_is_read_and_assigned_at_an_index_the_program_works_out_as_a_vector_does() {
    generate::check(
        4106,
        10,
        |g: &mut Gen| {
            let n = 1 + g.below(9);
            let writes: Vec<(usize, u32)> = (0..g.below(12))
                .map(|_| (g.below(n), g.below(1000) as u32))
                .collect();
            (n, writes)
        },
        |(n, writes)| {
            let start: Vec<String> = (0..*n).map(|k| (k * 7).to_string()).collect();
            let mut model: Vec<u32> = (0..*n as u32).map(|k| k * 7).collect();
            let mut calls = String::new();
            for (at, x) in writes {
                calls += &format!("  w = set(w, {at}, {x})\n");
                model[*at] = *x;
            }
            let reads: String = (0..*n)
                .map(|k| format!("  print(\"{{get(w, {k})}}\")\n"))
                .collect();
            let source = header(format!(
                "fn set(a: [{n}]u32, at: Nat, x: u32) -> [{n}]u32 {{\n  var b = a\n  if at < {n} {{ b[at] = x }}\n  b\n}}\n\nfn get(a: [{n}]u32, at: Nat) -> u32 {{ if at < {n} {{ a[at] }} else {{ 0 }} }}\n\nfn main() ! console, alloc {{\n  var w: [{n}]u32 = [{}]\n{calls}{reads}  loop x in w {{ print(\"{{x}}\") }}\n}}\n",
                start.join(", ")
            ));
            let expected: String = model
                .iter()
                .chain(model.iter())
                .map(|x| format!("{x}\n"))
                .collect();
            assert_eq!(stdout(&source), expected, "for\n{source}");
            let program = lowered(&source);
            let allocates = program.functions.iter().any(|f| {
                f.blocks.iter().flat_map(|b| &b.instructions).any(|i| {
                    matches!(i.op, Op::System { call: System::Allocate, .. })
                })
            });
            // Only the text built to print asks the allocator, and the program
            // reads and writes the array through the function's own frame.
            let frames = program.functions.iter().any(|f| {
                f.blocks
                    .iter()
                    .flat_map(|b| &b.instructions)
                    .any(|i| matches!(i.op, Op::Frame { .. }))
            });
            assert!(frames || !allocates, "for\n{source}");
        },
    );
}

/// How many stores the function `name` holds, written out.
fn stores_in(program: &scalar::Program, name: &str) -> usize {
    let f = program
        .functions
        .iter()
        .find(|f| f.name.ends_with(name))
        .unwrap_or_else(|| panic!("no function {name}"));
    f.blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .filter(|i| matches!(i.op, Op::Store { .. }))
        .count()
}

/// An array that is read at an index the program works out is kept in the
/// frame, where it is written when it is made and when an element is assigned,
/// so a read of it again is a load and no store: the stores of a function do
/// not grow with the reads.
fn an_array_that_is_indexed_is_written_once_and_reading_it_again_stores_nothing() {
    generate::check(4107, 8, |g: &mut Gen| (2 + g.below(7), 1 + g.below(6)), |(n, reads)| {
        let items: Vec<String> = (0..*n).map(|k| k.to_string()).collect();
        let program = |reads: usize| {
            let body: String = (0..reads)
                .map(|_| "  sum = wrapping_add(sum, a[at])\n".to_string())
                .collect();
            header(format!(
                "use std.int (wrapping_add)\n\nfn total(a: [{n}]u32, at: 0..{n}) -> u32 {{\n  var sum: u32 = 0\n{body}  sum\n}}\n\nfn main() ! console, alloc {{\n  a: [{n}]u32 = [{}]\n  x = total(a, 1)\n  print(\"{{x}}\")\n}}\n",
                items.join(", ")
            ))
        };
        let (one, more) = (lowered(&program(1)), lowered(&program(1 + reads)));
        assert_eq!(
            stores_in(&one, "total"),
            stores_in(&more, "total"),
            "for\n{}",
            program(1 + reads)
        );
    });
}

/// An array written in brackets where nothing says its type is as long as it is
/// written, of the smallest range that holds its elements, and it is taken
/// where a `List` of a wider element is needed, which has every element.
fn an_array_written_with_no_type_is_as_long_as_written_and_is_taken_as_a_list_of_a_wider_element() {
    generate::check(
        4108,
        10,
        |g: &mut Gen| (0..1 + g.below(8)).map(|_| g.below(300) as u64).collect::<Vec<u64>>(),
        |items| {
            let written: Vec<String> = items.iter().map(u64::to_string).collect();
            let source = header(format!(
                "use std.int (wrapping_add)\n\nfn total(xs: List[Nat]) -> Nat {{\n  var sum: Nat = 0\n  loop x in xs {{ sum = wrapping_add(sum, x) }}\n  sum\n}}\n\nfn main() ! console, alloc {{\n  xs = [{}]\n  print(\"{{total(xs)}} {{xs[0]}}\")\n}}\n",
                written.join(", ")
            ));
            let sum: u64 = items.iter().sum();
            assert_eq!(stdout(&source), format!("{sum} {}\n", items[0]), "for\n{source}");
        },
    );
}

/// Elements that no one type holds, as `[1, "a"]`, are refused, and so is an
/// array of nothing, whose error says `List[Nat]` in its fix.
fn an_array_written_with_no_type_whose_elements_no_one_type_holds_or_that_has_none_is_refused() {
    let mixed = header("fn main() ! console {\n  xs = [1, \"a\"]\n}\n".to_string());
    assert!(!compiles(&mixed), "for\n{mixed}");
    let empty = header("fn main() ! console {\n  xs = []\n}\n".to_string());
    let errors = check(empty.as_bytes()).unwrap_err();
    assert!(
        errors.iter().any(|e| e.fix.as_deref().is_some_and(|f| f.contains("List[Nat]"))),
        "for\n{empty}: {errors:?}"
    );
}
