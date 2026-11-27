//! A struct or a function with a whole number as a type parameter, as
//! `Box[most: Nat]` and `make[most: Nat]`, is defined in one module and used in
//! another as it is in the file that defines it: the program prints what it
//! prints with the items in its file, and builds to the same executable.

use smith::scalar;

use crate::Test;
use crate::generate::{self, Gen};
use crate::numbers;
use crate::targets::any;

pub const ONCE: &[Test] = &[
    (
        "the_items_of_a_module_with_a_whole_number_parameter_are_used_from_another_as_in_one_file",
        the_items_of_a_module_with_a_whole_number_parameter_are_used_from_another_as_in_one_file,
    ),
    (
        "a_program_that_uses_items_with_whole_number_parameters_of_a_module_builds_to_what_it_builds_with_them_in_its_file",
        a_program_that_uses_items_with_whole_number_parameters_of_a_module_builds_to_what_it_builds_with_them_in_its_file,
    ),
    (
        "a_module_of_a_project_with_items_with_whole_number_parameters_is_used_by_the_file_that_is_built",
        a_module_of_a_project_with_items_with_whole_number_parameters_is_used_by_the_file_that_is_built,
    ),
    (
        "a_use_of_an_item_of_a_module_that_writes_the_numbers_wrong_is_refused_at_its_place_in_the_file_that_uses_it",
        a_use_of_an_item_of_a_module_that_writes_the_numbers_wrong_is_refused_at_its_place_in_the_file_that_uses_it,
    ),
    (
        "an_error_in_the_copy_of_an_item_of_a_module_is_at_its_place_in_the_module_and_says_the_numbers",
        an_error_in_the_copy_of_an_item_of_a_module_is_at_its_place_in_the_module_and_says_the_numbers,
    ),
];

/// The modules given, and those of the standard library that the programs here
/// import beside them.
fn with_std<'a>(library: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
    let mut all = library.to_vec();
    for module in ["bits", "console", "int"] {
        if !all.iter().any(|(name, _)| *name == module) {
            all.push((module, smith::library_text(module)));
        }
    }
    all
}

fn built(source: &str, library: &[(&str, &str)]) -> Result<scalar::Program, Vec<smith::diag::Error>> {
    smith::lower_in(source.as_bytes(), &with_std(library))
}

fn executable(source: &str, library: &[(&str, &str)]) -> Vec<u8> {
    smith::compile_in(source.as_bytes(), &with_std(library), any())
        .unwrap_or_else(|e| panic!("{e:?} for\n{source}\n{library:?}"))
}

fn printed(source: &str, library: &[(&str, &str)]) -> Vec<u8> {
    let program = built(source, library).unwrap_or_else(|e| panic!("{e:?} for\n{source}\n{library:?}"));
    scalar::run(&program).stdout
}

/// The items of the module, with `pub` where the program takes them.
const ITEMS: &str = "{p}struct Box[most: Nat] {\n  {p}n: 0..most\n}\n\n\
{p}fn make[most: Nat](n: 0..most) -> Box[most] {\n  Box[most](n: n)\n}\n\n\
{p}fn width[most: Nat](b: Box[most]) -> Nat {\n  most\n}\n\n\
{p}fn top[most: Nat = {default}]() -> Box[most] {\n  Box(n: most - 1)\n}\n";

fn items(public: bool, default: u64) -> String {
    ITEMS.replace("{p}", if public { "pub " } else { "" }).replace("{default}", &default.to_string())
}

/// The example of the issue: a module's `Box` and `make`, and a function of the
/// program that is given a number and passes it on.
fn the_items_of_a_module_with_a_whole_number_parameter_are_used_from_another_as_in_one_file() {
    let module = "pub struct Box[most: Nat] {\n  pub n: 0..most\n}\n\npub fn make[most: Nat](n: 0..most) -> Box[most] {\n  Box[most](n: n)\n}\n";
    let main = "fn get[limit: Nat](x: 0..limit) -> Box[limit] {\n  make[limit](x)\n}\n\nfn main() <console, alloc> {\n  b = get[16](3)\n  print(\"{b.n}\")\n}\n";
    let split = numbers::with_header(format!("use std.gen (Box, make)\n\n{main}"));
    let one = numbers::with_header(format!("{}\n{main}", module.replace("pub ", "")));
    assert_eq!(printed(&split, &[("gen", module)]), b"3\n");
    assert_eq!(printed(&one, &[]), b"3\n");
}

/// What a program does with the items, written once for the module and once for
/// the file, and what it must print.
#[derive(Debug)]
struct Case {
    /// The numbers of the items, below 40.
    sizes: Vec<u64>,
    default: u64,
    /// Which use of the items each line of the program is.
    forms: Vec<u64>,
}

fn case(g: &mut Gen) -> Case {
    let count = 1 + g.below(5);
    Case {
        sizes: (0..count).map(|_| 1 + g.below(39) as u64).collect(),
        default: 1 + g.below(39) as u64,
        forms: (0..count).map(|_| g.below(5) as u64).collect(),
    }
}

/// The program's lines and what each prints. A use that writes the number, one
/// that takes it from the type the use must give, one that takes it from an
/// argument, one that passes a number on from a function of the program, and one
/// that leaves it to a default.
fn program(c: &Case) -> (String, String) {
    let mut body = String::new();
    let mut said = String::new();
    for (i, (k, form)) in c.sizes.iter().zip(&c.forms).enumerate() {
        let v = k - 1;
        match form {
            0 => {
                body += &format!("  a{i} = make[{k}]({v})\n  print(\"{{a{i}.n}}\")\n");
                said += &format!("{v}\n");
            }
            1 => {
                body += &format!("  b{i}: Box[{k}] = make({v})\n  print(\"{{b{i}.n}}\")\n");
                said += &format!("{v}\n");
            }
            2 => {
                body += &format!("  c{i} = make[{k}]({v})\n  print(\"{{width(c{i})}}\")\n");
                said += &format!("{k}\n");
            }
            3 => {
                body += &format!("  d{i} = pass[{k}]({v})\n  print(\"{{d{i}.n}}\")\n");
                said += &format!("{v}\n");
            }
            _ => {
                body += &format!("  e{i} = top()\n  print(\"{{width(e{i})}} {{e{i}.n}}\")\n");
                said += &format!("{} {}\n", c.default, c.default - 1);
            }
        }
    }
    let main = format!(
        "fn pass[limit: Nat](x: 0..limit) -> Box[limit] {{\n  make[limit](x)\n}}\n\nfn main() <console, alloc> {{\n{body}}}\n"
    );
    (main, said)
}

/// A program that takes the items from a module prints, and is built to, what
/// the program with the items in its file is.
fn a_program_that_uses_items_with_whole_number_parameters_of_a_module_builds_to_what_it_builds_with_them_in_its_file()
{
    generate::check(1230, 10, case, |c| {
        let (main, said) = program(c);
        let module = items(true, c.default);
        let split = numbers::with_header(format!("use std.gen (Box, make, top, width)\n\n{main}"));
        let one = numbers::with_header(format!("{}\n{main}", items(false, c.default)));
        let got = printed(&split, &[("gen", &module)]);
        assert_eq!(String::from_utf8_lossy(&got), said, "for\n{split}\n{module}");
        assert_eq!(printed(&one, &[]), got, "for\n{one}");
        assert!(
            executable(&split, &[("gen", &module)]) == executable(&one, &[]),
            "the executables differ for\n{split}\n{module}"
        );
    });
}

/// A use that writes the wrong number of numbers, or a name where a number must
/// stand, is refused at the place it is written in the file that uses it.
fn a_use_of_an_item_of_a_module_that_writes_the_numbers_wrong_is_refused_at_its_place_in_the_file_that_uses_it()
{
    generate::check(1231, 12, |g| (g.below(3), g.below(4)), |&(wrong, lines)| {
        let module = items(true, 8);
        let pad = "\n".repeat(lines);
        let (call, mention) = match wrong {
            0 => ("make[1, 2](0)", "takes"),
            1 => ("make[x](0)", "written out"),
            _ => ("make[-1](0)", "not a `Nat`"),
        };
        let source = numbers::with_header(format!(
            "use std.gen (make)\n\n{pad}fn main() <console, alloc> {{\n  a = {call}\n  print(\"{{a.n}}\")\n}}\n"
        ));
        let errors = built(&source, &[("gen", &module)]).unwrap_err();
        let found: Vec<_> = errors.iter().filter(|e| e.message.contains(mention)).collect();
        assert!(!found.is_empty(), "{errors:?} for\n{source}");
        let at = source.lines().position(|l| l.contains(call)).unwrap() + 1;
        assert!(found.iter().all(|e| e.pos.line == at && e.module.is_none()), "{found:?} for\n{source}");
    });
}

/// A copy of an item that is wrong for the numbers it was made for is reported at
/// the item's place in the module, with the module's name and the numbers.
fn an_error_in_the_copy_of_an_item_of_a_module_is_at_its_place_in_the_module_and_says_the_numbers() {
    generate::check(1232, 12, |g| (g.below(4), 1 + g.below(9) as u64), |&(lines, k)| {
        let pad = "\n".repeat(lines);
        let module = format!("{pad}pub fn below[most: Nat](n: 0..most) -> Nat {{\n  most - {}\n}}\n", k + 1);
        let source = numbers::with_header(format!(
            "use std.gen (below)\n\nfn main() <console, alloc> {{\n  print(\"{{below[{k}](0)}}\")\n}}\n"
        ));
        let errors = built(&source, &[("gen", &module)]).unwrap_err();
        let at = module.lines().position(|l| l.contains("most - ")).unwrap() + 1;
        let found: Vec<_> = errors.iter().filter(|e| e.module.as_deref() == Some("gen.smith") || e.module.as_deref() == Some("gen")).collect();
        assert!(!found.is_empty(), "{errors:?} for\n{source}\n{module}");
        assert!(found.iter().all(|e| e.pos.line == at), "{found:?} for\n{module}");
        assert!(found.iter().any(|e| e.message.contains(&format!("`most` as {k}"))), "{found:?}");
    });
}

/// A module that is a file beside the one that is built, and not one of the
/// library, is read the same way: its items with whole number parameters, one
/// that leaves its number to a default among them, are used by the file.
fn a_module_of_a_project_with_items_with_whole_number_parameters_is_used_by_the_file_that_is_built() {
    let dir = crate::cli::folder("whole-numbers-in-a-module");
    std::fs::write(dir.join("boxes.smith"), items(true, 7)).unwrap();
    std::fs::write(
        dir.join("main.smith"),
        "use boxes (Box, make, top, width)\nuse std.console (console, print)\n\nfn main() <console, alloc> {\n  a = make[16](3)\n  b: Box[5] = make(4)\n  c = top()\n  print(\"{a.n} {b.n} {width(b)} {width(c)}\")\n}\n",
    )
    .unwrap();
    let ran = crate::cli::smith(&dir, &["run", "main.smith"]);
    assert!(ran.status.success(), "{}", crate::cli::stderr(&ran));
    assert_eq!(String::from_utf8_lossy(&ran.stdout), "3 4 5 7\n");
}
