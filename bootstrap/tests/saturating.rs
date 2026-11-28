//! `saturating_add`, `saturating_sub`, and `saturating_mul` are functions of
//! `std.int` that a file imports, and nothing in Smith is a `checked_`
//! function. What the saturating functions cost, and that they give what they
//! gave: the cost is the operation and a choice, with no call and no jump.
//! Every function answers what Rust's arithmetic on 128 bits says, at the
//! edges of every type.
//!
//! What a function costs is counted in the machine code of each kind of chip,
//! beside the same program with the function a `wrapping_` one, which is
//! the add or the subtraction alone, and the values are taken from a list, so
//! that the compiler cannot know them.

use std::path::Path;

use smith::scalar;
use smith::target::Target;
use smith::lower;

use crate::Test;
use crate::aarch64;
use crate::generate::{self, Gen};
use crate::machine_code::{self, Kind};
use crate::numbers::{INTS, Int, arithmetic, value, with_header};
use crate::targets::{runnable, scalar_runs_as};

pub const ONCE: &[Test] = &[
    (
        "a_saturating_function_is_imported_from_std_int_and_refused_where_it_is_called_without_the_import",
        a_saturating_function_is_imported_from_std_int_and_refused_where_it_is_called_without_the_import,
    ),
    (
        "the_prelude_has_no_saturating_or_checked_function_and_nothing_in_smith_is_a_checked_function",
        the_prelude_has_no_saturating_or_checked_function_and_nothing_in_smith_is_a_checked_function,
    ),
    (
        "a_saturating_function_costs_no_call_and_no_jump_and_the_instructions_extra_gives_part_1_of_3",
        || a_saturating_function_costs_no_call_and_no_jump_and_the_instructions_extra_gives("add"),
    ),
    (
        "a_saturating_function_costs_no_call_and_no_jump_and_the_instructions_extra_gives_part_2_of_3",
        || a_saturating_function_costs_no_call_and_no_jump_and_the_instructions_extra_gives("sub"),
    ),
    (
        "a_saturating_function_costs_no_call_and_no_jump_and_the_instructions_extra_gives_part_3_of_3",
        || a_saturating_function_costs_no_call_and_no_jump_and_the_instructions_extra_gives("mul"),
    ),
    (
        "the_saturating_functions_answer_at_the_edges_of_every_type_in_the_evaluator_and_on_arm64_part_1_of_3",
        || the_saturating_functions_answer_at_the_edges_of_every_type_in_the_evaluator_and_on_arm64(0),
    ),
    (
        "the_saturating_functions_answer_at_the_edges_of_every_type_in_the_evaluator_and_on_arm64_part_2_of_3",
        || the_saturating_functions_answer_at_the_edges_of_every_type_in_the_evaluator_and_on_arm64(1),
    ),
    (
        "the_saturating_functions_answer_at_the_edges_of_every_type_in_the_evaluator_and_on_arm64_part_3_of_3",
        || the_saturating_functions_answer_at_the_edges_of_every_type_in_the_evaluator_and_on_arm64(2),
    ),
];

pub const MACHINE: &[Test] = &[(
    "the_saturating_functions_answer_at_the_edges_of_every_type_in_every_executable",
    the_saturating_functions_answer_at_the_edges_of_every_type_in_every_executable,
)];

/// The program of the issue, with the line that imports the two functions.
const IMPORTED: &str = "use std.console (console, print)
use std.bits (u8)
use std.int (saturating_add, saturating_sub)

fn main() <console, alloc> {
  a: u8 = 250
  b: u8 = 3
  print(\"{saturating_add(a, 10)} {saturating_sub(b, 5)}\")
}
";

/// The program prints the nearest values, and without its `use` of `std.int`
/// it is refused where `saturating_add` is first called, with a fix that
/// imports it from there.
fn a_saturating_function_is_imported_from_std_int_and_refused_where_it_is_called_without_the_import() {
    let program = lower(IMPORTED.as_bytes()).unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(scalar::run(&program).stdout, b"255 0\n");
    let without = IMPORTED.replace("use std.int (saturating_add, saturating_sub)\n", "");
    let errors = lower(without.as_bytes()).expect_err("no import of `saturating_add`");
    let (line, text) = (without.lines().enumerate())
        .find(|(_, l)| l.contains("saturating_add("))
        .expect("a call");
    let col = text.find("saturating_add(").expect("a call") + 1;
    let at = (errors.iter())
        .find(|e| (e.pos.line, e.pos.col) == (line + 1, col))
        .unwrap_or_else(|| panic!("no error where `saturating_add` is called: {errors:?}"));
    let fix = at.fix.as_deref().unwrap_or_default();
    assert!(fix.contains("use std.int (saturating_add"), "{at:?}");
}

/// The words of a text of Smith: its names, and what is not a name between.
fn words(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !(c.is_alphanumeric() || c == '_'))
}

/// The prelude names no `Saturating` or `Checked` trait and no `saturating_`
/// or `checked_` function, no file of the library or of the compiler written
/// in Smith names a `checked_` function, and a program that calls one is
/// refused, whether or not it imports it.
fn the_prelude_has_no_saturating_or_checked_function_and_nothing_in_smith_is_a_checked_function() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let prelude = std::fs::read_to_string(root.join("std/prelude.smith")).expect("the prelude");
    for word in words(&prelude) {
        assert!(
            !matches!(word, "Saturating" | "Checked")
                && !word.starts_with("saturating_")
                && !word.starts_with("checked_"),
            "the prelude names `{word}`"
        );
    }
    let mut read = 0;
    for folder in ["std", "compiler"] {
        for entry in std::fs::read_dir(root.join(folder)).expect("a folder") {
            let path = entry.expect("a file").path();
            if path.extension().is_some_and(|e| e == "smith") {
                read += 1;
                let text = std::fs::read_to_string(&path).expect("a file");
                let named = words(&text).find(|w| w.starts_with("checked_"));
                assert_eq!(named, None, "{}", path.display());
            }
        }
    }
    assert!(read > 0, "no file of Smith");
    for f in ["checked_add", "checked_sub", "checked_mul", "checked_div", "checked_rem"] {
        for import in ["", "use std.int (narrow)\n"] {
            let source = format!(
                "use std.console (console, print)\n{import}\nfn main() <console, alloc> {{\n  a: Byte = 7\n  print(\"{{{f}(a, 2)}}\")\n}}\n"
            );
            let errors = lower(source.as_bytes()).expect_err("a `checked_` function");
            assert!(errors.iter().any(|e| e.message.contains(f)), "{errors:?}");
        }
        let source = format!("use std.int ({f})\n\nfn main() {{}}\n");
        let errors = lower(source.as_bytes()).expect_err("an import of a `checked_` function");
        assert!(errors.iter().any(|e| e.message.contains(f)), "{errors:?}");
    }
}

/// The types of the whole numbers, and `Nat`, which is lowered to the `u64`
/// functions.
const TYPES: [&str; 7] = ["Byte", "u32", "u64", "Nat", "i8", "i32", "Int"];

/// What a use of a saturating function may cost, in instructions, over the
/// `wrapping_` function of the same operation, on every chip. These are what
/// the code is now, and the fewest the compiler gives: a change that makes one
/// larger fails the test, and one that makes one smaller is written here.
///
/// A number of 64 bits, or an unsigned addition or subtraction of fewer, is
/// worked out with a comparison, or by the flag of the operation, and chosen
/// by it. A signed number's end is worked out from the sign of an operand,
/// and a product needs the high half, or the numbers widened and looked at.
fn extra(ty: &str, op: &str) -> usize {
    match (ty, op) {
        ("u64" | "Nat", "add") => 3,
        ("u64" | "Nat", "sub") => 2,
        ("u64" | "Nat", _) => 4,
        ("Byte" | "u16" | "u32", "mul") => 5,
        ("Byte" | "u16" | "u32", _) => 3,
        ("Int", "sub") => 7,
        ("Int", _) => 8,
        (_, "mul") => 12,
        _ => 11,
    }
}

/// The type of the values that the program makes from a list, and what the
/// wrapping function that stands beside a `Nat` is, which has none.
fn wrapping_type(ty: &str) -> &str {
    if ty == "Nat" { "u64" } else { ty }
}

/// A program that prints what `body` writes of two values of `ty` that the
/// compiler cannot know.
fn with_values(ty: &str, body: &str) -> String {
    with_header(format!("fn main() <console, alloc> {{\n  xs: List[{ty}] = [3, 4]\n  x = xs[0]\n  y = xs[1]\n{body}}}\n"))
}

/// For each target whose executable the tests read, how many instructions it
/// holds, how many are calls, and how many jump.
fn counted(source: &str) -> Vec<(Target, (usize, usize, usize))> {
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for\n{source}"));
    let program = scalar::passes(&program);
    Target::ALL
        .iter()
        .filter_map(|&target| {
            let (_, found) = machine_code::instructions(&target.emit(&program))?;
            let calls = found.iter().filter(|i| i.kind == Kind::Call).count();
            let jumps = (found.iter())
                .filter(|i| matches!(i.kind, Kind::Jump(_) | Kind::Branch(_)))
                .count();
            Some((target, (found.len(), calls, jumps)))
        })
        .collect()
}

/// For each type and each operation, on every chip, with the function used
/// twice so that it is not the one call that is replaced for being the only
/// one, for the operation that the part is given: the function has no call and no jump that the program with the
/// `wrapping_` function has not, and costs no more than `extra` instructions
/// beyond it for each use.
fn a_saturating_function_costs_no_call_and_no_jump_and_the_instructions_extra_gives(op: &str) {
    let mut read = 0;
    for ty in TYPES {
        let program = |f: String, ty: &str| {
            with_values(ty, &format!("  print(\"{{{f}(x, y)}}\")\n  print(\"{{{f}(y, x)}}\")\n"))
        };
        let saturating = program(format!("saturating_{op}"), ty);
        let wrapping = program(format!("wrapping_{op}"), wrapping_type(ty));
        let (saturating, wrapping) = (counted(&saturating), counted(&wrapping));
        for ((target, s), (_, w)) in saturating.iter().zip(&wrapping) {
            read += 1;
            assert_eq!(
                (s.1, s.2),
                (w.1, w.2),
                "`saturating_{op}` of `{ty}` makes a call or a jump on {target:?}"
            );
            assert!(
                s.0 <= w.0 + 2 * extra(ty, op),
                "`saturating_{op}` of `{ty}` is {} instructions, and `wrapping_{op}` is {}, on {target:?}",
                s.0,
                w.0
            );
        }
    }
    assert!(read > 0, "no target is an ELF file");
}

/// The three functions, as they are written.
const FUNCTIONS: [(&str, &str, &str); 3] = [
    ("saturating", "add", "saturating_add"),
    ("saturating", "sub", "saturating_sub"),
    ("saturating", "mul", "saturating_mul"),
];

/// The values a type is tried with: its least and greatest, those next to
/// them, 0, 1, and -1 where the type has them, and a few the seed picks.
fn tried(ty: Int, g: &mut Gen) -> Vec<i128> {
    let mut values = vec![ty.min(), ty.max(), ty.min() + 1, ty.max() - 1, 0, 1, 2];
    if ty.signed {
        values.push(-1);
    }
    for _ in 0..3 {
        values.push(value(g, ty));
    }
    values.retain(|v| (ty.min()..=ty.max()).contains(v));
    values.sort();
    values.dedup();
    values
}

/// A program that prints, for every two of the values, what each function
/// gives, and what it must print.
fn answers(name: &str, ty: Int, g: &mut Gen) -> (String, String) {
    let values = tried(ty, g);
    let list: Vec<String> = values.iter().map(|v| v.to_string()).collect();
    let mut source = format!(
        "fn main() <console, alloc, diverge> {{\n  xs: List[{name}] = [{}]\n  loop i in 0..xs.size {{\n    loop j in 0..xs.size {{\n      x = xs[i]\n      y = xs[j]\n",
        list.join(", ")
    );
    for (_, _, f) in FUNCTIONS {
        source += &format!("      print(\"{{{f}(x, y)}}\")\n");
    }
    source += "    }\n  }\n}\n";
    let mut said = String::new();
    for &a in &values {
        for &b in &values {
            for (kind, op, _) in FUNCTIONS {
                said += &arithmetic(kind, op, ty, a, b);
                said += "\n";
            }
        }
    }
    (with_header(source), said)
}

/// Every type, `Nat` as a `u64` whose name is `Nat`, with the program that
/// tries it and what it prints.
fn programs(g: &mut Gen) -> Vec<(String, String, String)> {
    let mut all: Vec<(String, String, String)> = INTS
        .iter()
        .map(|ty| {
            let (source, said) = answers(ty.name, *ty, g);
            (ty.name.to_string(), source, said)
        })
        .collect();
    let u64 = *INTS.iter().find(|t| t.name == "u64").unwrap();
    let (source, said) = answers("Nat", u64, g);
    all.push(("Nat".to_string(), source, said));
    all
}

/// Every function answers what Rust says at the edges, in the evaluator and in
/// each executable that the machine of the tests runs for arm64.
fn the_saturating_functions_answer_at_the_edges_of_every_type_in_the_evaluator_and_on_arm64(part: usize) {
    generate::check(4401, 1, programs, |all| {
        // The types are in three parts, one in three of them to each.
        for (name, source, said) in all.iter().skip(part).step_by(3) {
            let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for\n{source}"));
            let out = scalar::run(&program);
            let printed = String::from_utf8(out.stdout).expect("text");
            assert_eq!(&printed, said, "the evaluator, for `{name}`:\n{source}");
            let built = scalar::passes(&program);
            let mut ran_arm64 = false;
            for target in Target::ALL {
                let bytes = target.emit(&built);
                if aarch64::is_executable(&bytes) {
                    ran_arm64 = true;
                    let printed = String::from_utf8(aarch64::run(&bytes).stdout).expect("text");
                    assert_eq!(&printed, said, "arm64 {target:?}, for `{name}`:\n{source}");
                }
            }
            assert!(ran_arm64, "an arm64 executable");
        }
    });
}

/// The same, in the executables that this machine runs.
fn the_saturating_functions_answer_at_the_edges_of_every_type_in_every_executable() {
    let targets = runnable();
    generate::check(4402, 1, programs, |all| {
        for (name, source, said) in all {
            let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for\n{source}"));
            let out = scalar::run(&program);
            assert_eq!(&String::from_utf8(out.stdout.clone()).expect("text"), said, "`{name}`");
            for &target in &targets {
                scalar_runs_as(&scalar::passes(&program), target, &out);
            }
        }
    });
}
