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
    (
        "a_reply_of_mcp_holds_what_it_says_and_a_refusal_has_a_status_that_is_one",
        a_reply_of_mcp_holds_what_it_says_and_a_refusal_has_a_status_that_is_one,
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

fn a_reply_of_mcp_holds_what_it_says_and_a_refusal_has_a_status_that_is_one() {
    let program = |call: &str| format!("use std.mcp (Reply)\n\nfn main() {{\n  _ = {call}\n}}\n");
    for call in ["Reply.answer(\"x\")", "Reply.nothing", "Reply.refused((403, \"x\"))", "Reply.refused((499, \"x\"))"] {
        assert_eq!(refusal(&program(call)), "", "{call}");
    }
    // A refusal is a 4xx, a text with no answer is not a state, and the status
    // beside a text is no member of a reply.
    for (call, name) in [
        ("Reply.refused((200, \"x\"))", "400"),
        ("Reply.refused((202, \"x\"))", "400"),
        ("Reply.refused((500, \"x\"))", "500"),
        ("Reply.refused((404))", "400..500"),
        ("Reply.answer(.none)", "text"),
        ("Reply(text: .none, status: 202)", "Reply"),
    ] {
        let wrong = refusal(&program(call));
        assert!(wrong.contains(name), "{wrong:?} for {call}");
    }
}
