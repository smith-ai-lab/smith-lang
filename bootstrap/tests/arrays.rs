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

/// `a[i]` compiles when `arith` proves `i` below the size, and is refused
/// otherwise, the elements of a `var`
/// are assigned the same way, and the array that was passed is unchanged.
fn an_index_is_proven_below_the_size_or_refused_and_an_element_is_assigned_where_it_is_proven() {
    generate::check(4104, 12, |g: &mut Gen| (1 + g.below(5), g.below(7)), |(n, i)| {
        let items: Vec<String> = (0..*n).map(|k| literal("u8", k)).collect();
        let read = header(format!(
            "fn main() ! console, alloc {{\n  a: [{n}]u8 = [{}]\n  print(\"{{a[{i}]}}\")\n}}\n",
            items.join(", ")
        ));
        assert_eq!(compiles(&read), i < n, "for\n{read}");
        let unproven = header(format!(
            "fn tested(a: [{n}]u8, i: Nat) -> u8 {{ a[i] }}\n\nfn main() ! console {{\n  a: [{n}]u8 = [{}]\n  y = tested(a, 0)\n}}\n",
            items.join(", ")
        ));
        assert!(!compiles(&unproven), "for\n{unproven}");
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
