//! A value that a setting cannot hold is refused where it is written: `unknown`
//! is for reading and not for writing, and the mode of a file or the letters of
//! a number are cases, not booleans.

use smith::check;

use crate::Test;

pub const ONCE: &[Test] = &[
    (
        "a_setting_that_is_for_reading_is_refused_where_text_is_written",
        a_setting_that_is_for_reading_is_refused_where_text_is_written,
    ),
    (
        "a_boolean_is_refused_where_a_file_mode_or_the_letters_of_a_number_are_given",
        a_boolean_is_refused_where_a_file_mode_or_the_letters_of_a_number_are_given,
    ),
];

/// What the compiler says of a program, as one text, or nothing when it builds.
fn refusal(source: &str) -> String {
    let found = check(source.as_bytes()).err().unwrap_or_default();
    found.iter().map(|e| e.message.clone()).collect::<Vec<_>>().join("\n")
}

fn a_setting_that_is_for_reading_is_refused_where_text_is_written() {
    let program = |call: &str| {
        format!(
            "use std.json (Json, from_json_as, to_json_as)\n\nfn main() <alloc> {{\n  json = Json(cases: .key, names: .written)\n  {call}\n}}\n"
        )
    };
    // The setting of reading is given where text is read, and is no part of
    // what writes.
    assert_eq!(refusal(&program("_ = to_json_as(3, json)")), "");
    let read = "r: Result[Int, ReadError] = from_json_as(\"3\", json, unknown: .refuse)";
    let found = refusal(&program(read).replace("use std.json", "use std.deserialize (ReadError)\nuse std.json"));
    assert_eq!(found, "", "{found}");
    let wrong = refusal(&program("_ = to_json_as(3, json, unknown: .refuse)"));
    assert!(wrong.contains("unknown"), "{wrong:?}");
    let wrong = refusal(&program("_ = to_json_as(3, Json(cases: .key, names: .written, unknown: .refuse))"));
    assert!(wrong.contains("unknown"), "{wrong:?}");
}

fn a_boolean_is_refused_where_a_file_mode_or_the_letters_of_a_number_are_given() {
    let program = |uses: &str, call: &str| format!("use std.fmt (hex)\nuse std.fs (fs, write_all)\n{uses}\nfn main() <fs, alloc> {{\n  {call}\n}}\n");
    for call in ["_ = hex(233, letters: .lower)", "_ = write_all(\"x\", [], mode: .executable)"] {
        assert_eq!(refusal(&program("", call)), "", "{call}");
    }
    for (call, name) in [
        ("_ = hex(233, upper: false)", "upper"),
        ("_ = hex(233, letters: false)", "Letters"),
        ("_ = write_all(\"x\", [], executable: true)", "executable"),
        ("_ = write_all(\"x\", [], mode: true)", "Mode"),
    ] {
        let wrong = refusal(&program("", call));
        assert!(wrong.contains(name), "{wrong:?} for {call}");
    }
}
