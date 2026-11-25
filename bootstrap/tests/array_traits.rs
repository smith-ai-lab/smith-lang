//! An array has `Fmt` and `Repr` when its element has them, and writes what a
//! list of the same elements writes, by `{}` and by `repr`. An array whose
//! element has none is refused naming the element, and `without` takes them
//! away.

use smith::{check, lower, scalar};

use crate::Test;
use crate::numbers::with_header;

pub const ONCE: &[Test] = &[
    (
        "an_array_is_written_as_a_list_of_the_same_elements_is_by_fmt_and_by_repr",
        an_array_is_written_as_a_list_of_the_same_elements_is_by_fmt_and_by_repr,
    ),
    (
        "an_array_written_with_no_type_prints_and_an_empty_array_prints_nothing_between_brackets",
        an_array_written_with_no_type_prints_and_an_empty_array_prints_nothing_between_brackets,
    ),
    (
        "an_array_of_what_lacks_fmt_or_repr_is_refused_naming_the_element",
        an_array_of_what_lacks_fmt_or_repr_is_refused_naming_the_element,
    ),
];

fn stdout(source: &str) -> String {
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for\n{source}"));
    let out = scalar::run(&program);
    assert_eq!(out.status, 0, "for\n{source}");
    String::from_utf8(out.stdout).unwrap()
}

const DECLARATIONS: &str = "struct Record {\n  number: Nat\n  text: str\n}\n\n";

/// An element type, and the elements an array and a list of it are written with.
const KINDS: &[(&str, &[&str])] = &[
    ("Nat", &["1", "2", "3"]),
    ("str", &["\"a\"", "\"\"", "\"b c\""]),
    ("bool", &["true", "false"]),
    ("(Nat, str)", &["(1, \"a\")", "(2, \"b\")"]),
    ("Record", &["Record(number: 1, text: \"a\")", "Record(number: 2, text: \"b\")"]),
    ("[2]Nat", &["[1, 2]", "[3, 4]"]),
];

fn an_array_is_written_as_a_list_of_the_same_elements_is_by_fmt_and_by_repr() {
    for (ty, items) in KINDS {
        let items = items.join(", ");
        let n = items.matches(", ").count() + 1;
        // A tuple or an array element has commas of its own, so the size is counted
        // from the list the elements are written in.
        let program = with_header(format!(
            "{DECLARATIONS}fn main() <console, alloc> {{\n  \
             a: [{size}]{ty} = [{items}]\n  \
             l: List[{ty}] = [{items}]\n  \
             print(\"{{a}}\")\n  print(\"{{l}}\")\n  \
             print(\"{{repr(a)}}\")\n  print(\"{{repr(l)}}\")\n}}\n",
            size = size_of(ty, n),
        ));
        let lines: Vec<String> = stdout(&program).lines().map(String::from).collect();
        assert_eq!(lines.len(), 4, "for\n{program}");
        assert!(lines.iter().all(|l| *l == lines[0]), "{lines:?} for\n{program}");
    }
}

/// How many elements the written ones are, which the commas of a tuple, a
/// record, or an array element do not change.
fn size_of(ty: &str, naive: usize) -> usize {
    match ty {
        "(Nat, str)" => 2,
        "Record" => 2,
        "[2]Nat" => 2,
        "bool" => 2,
        "str" | "Nat" => naive,
        _ => unreachable!(),
    }
}

fn an_array_written_with_no_type_prints_and_an_empty_array_prints_nothing_between_brackets() {
    let program = with_header(
        "fn main() <console, alloc> {\n  xs = [1, 2, 3]\n  print(\"{xs}\")\n  \
         e: [0]Nat = []\n  print(\"{e}\")\n  print(\"{repr(e)}\")\n}\n"
            .to_string(),
    );
    assert_eq!(stdout(&program), "[1, 2, 3]\n[]\n[]\n");
}

fn an_array_of_what_lacks_fmt_or_repr_is_refused_naming_the_element() {
    for (taken, used) in [("Fmt", "{keys}"), ("Repr", "{repr(keys)}")] {
        let program = with_header(format!(
            "struct Key {{\n  secret: str\n}}\n\nwithout {taken} for Key\n\n\
             fn main() <console, alloc> {{\n  keys = [Key(secret: \"a\")]\n  print(\"{used}\")\n}}\n"
        ));
        let errors = check(program.as_bytes()).expect_err("a mistake");
        assert!(
            errors.iter().any(|e| e.message.contains("Key") && e.message.contains(taken)),
            "{errors:?} for\n{program}"
        );
    }
}
