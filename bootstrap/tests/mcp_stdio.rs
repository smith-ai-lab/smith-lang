//! A program is a server of the Model Context Protocol over the standard input
//! and output: `mcp` answers the one message `stdio` gives it with what the
//! `tool` lines say, `stdio` runs them once for each line of the input, and a
//! plain function is a tool that a client lists and calls.
//!
//! The generator builds a tool of parameters of the kinds a tool may take, and
//! the arguments of a call, and keeps what it has put in them. The expectation
//! of the schema is what `Describe` gives for a struct of the same fields, run
//! by a program of its own, and the expectation of a call is the arguments, which
//! the tool gives back as a struct of the same fields.

use smith::{lower, scalar};

use crate::Test;
use crate::generate::{self, Gen};
use crate::standard::{Json, lit, parse};
use crate::targets;

pub const ONCE: &[Test] = &[
    (
        "divide_is_listed_with_its_comments_and_a_call_of_it_is_answered_and_a_notification_is_given_no_line",
        divide_is_listed_with_its_comments_and_a_call_of_it_is_answered_and_a_notification_is_given_no_line,
    ),
    (
        "a_tool_that_fails_is_a_result_marked_as_an_error_and_a_tool_the_server_has_not_is_invalid_params",
        a_tool_that_fails_is_a_result_marked_as_an_error_and_a_tool_the_server_has_not_is_invalid_params,
    ),
    (
        "arguments_that_do_not_fit_are_invalid_params_with_the_parameter_named",
        arguments_that_do_not_fit_are_invalid_params_with_the_parameter_named,
    ),
    (
        "a_tool_is_listed_with_the_schema_describe_gives_and_a_call_reaches_it_with_the_values_it_holds_part_1_of_4",
        || a_tool_is_listed_with_the_schema_describe_gives_and_a_call_reaches_it_with_the_values_it_holds(0),
    ),
    (
        "a_tool_is_listed_with_the_schema_describe_gives_and_a_call_reaches_it_with_the_values_it_holds_part_2_of_4",
        || a_tool_is_listed_with_the_schema_describe_gives_and_a_call_reaches_it_with_the_values_it_holds(1),
    ),
    (
        "a_tool_is_listed_with_the_schema_describe_gives_and_a_call_reaches_it_with_the_values_it_holds_part_3_of_4",
        || a_tool_is_listed_with_the_schema_describe_gives_and_a_call_reaches_it_with_the_values_it_holds(2),
    ),
    (
        "a_tool_is_listed_with_the_schema_describe_gives_and_a_call_reaches_it_with_the_values_it_holds_part_4_of_4",
        || a_tool_is_listed_with_the_schema_describe_gives_and_a_call_reaches_it_with_the_values_it_holds(3),
    ),
    (
        "what_a_tool_prints_goes_to_the_standard_error_and_the_server_ends_with_the_input",
        what_a_tool_prints_goes_to_the_standard_error_and_the_server_ends_with_the_input,
    ),
    (
        "a_comment_is_in_the_executable_only_for_a_function_given_to_tool_and_the_types_it_takes",
        a_comment_is_in_the_executable_only_for_a_function_given_to_tool_and_the_types_it_takes,
    ),
];

/// Every executable answers what the evaluator answers.
pub const MACHINE: &[Test] = &[(
    "every_executable_of_a_server_answers_as_the_evaluator_does",
    every_executable_of_a_server_answers_as_the_evaluator_does,
)];

const META: &str = "\"_meta\":{\"io.modelcontextprotocol/protocolVersion\":\"2026-07-28\",\"io.modelcontextprotocol/clientCapabilities\":{}}";

/// The server of the page, with the entry that installs `stdio`, and `divide`
/// written so that the quotient is proven to fit.
const DIVIDE: &str = "use std.console (console)\nuse std.mcp (mcp, messages, stdio, tool)\n\n// Divides one whole number by another.\nfn divide(\n  // The number that is divided.\n  a: Int,\n  // The number it is divided by.\n  b: Int,\n) -> Result[Int, str] {\n  if b == 0 {\n    .err(\"b is zero\")\n  } else if a == -9223372036854775808 and b == -1 {\n    .err(\"the quotient does not fit\")\n  } else {\n    .ok(a / b)\n  }\n}\n\nfn serve() <messages, alloc> {\n  with mcp(name: \"calc\", version: \"1\")\n  tool(divide)\n}\n\nfn main() <console, alloc> {\n  with stdio()\n  serve()\n}\n";

fn request(id: u32, method: &str, params: &str) -> String {
    let sep = if params.is_empty() { "" } else { "," };
    format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"method\":\"{method}\",\"params\":{{{params}{sep}{META}}}}}")
}

fn call(id: u32, name: &str, arguments: &str) -> String {
    request(id, "tools/call", &format!("\"name\":{},\"arguments\":{arguments}", lit(name)))
}

/// The lines the program writes when it is given these messages, one to a line.
fn serve(source: &str, messages: &[String]) -> Vec<String> {
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for\n{source}"));
    serve_program(&program, messages)
}

fn serve_program(program: &scalar::Program, messages: &[String]) -> Vec<String> {
    let mut input = String::new();
    for m in messages {
        input += m;
        input += "\n";
    }
    let said = scalar::run_with_input(program, input.as_bytes());
    assert_eq!(said.status, 0, "the server ends with a passing status when its input ends");
    String::from_utf8(said.stdout).unwrap().lines().map(str::to_string).collect()
}

fn result_of(line: &str) -> Json {
    let reply = parse(line);
    reply.get("result").unwrap_or_else(|| panic!("no result in {line}")).clone()
}

fn error_of(line: &str) -> (String, String) {
    let reply = parse(line);
    let e = reply.get("error").unwrap_or_else(|| panic!("no error in {line}"));
    let code = match e.get("code") {
        Some(Json::Number(n)) => n.clone(),
        other => panic!("no code: {other:?}"),
    };
    (code, e.get("message").and_then(Json::text).unwrap_or_default().to_string())
}

fn divide_is_listed_with_its_comments_and_a_call_of_it_is_answered_and_a_notification_is_given_no_line() {
    let messages = [
        request(1, "server/discover", ""),
        request(2, "tools/list", ""),
        "{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}".to_string(),
        call(3, "divide", "{\"a\":6,\"b\":3}"),
    ];
    let lines = serve(DIVIDE, &messages);
    assert_eq!(lines.len(), 3, "one line for each request and none for the notification: {lines:?}");
    let found = result_of(&lines[0]);
    assert!(found.get("capabilities").and_then(|c| c.get("tools")).is_some(), "{}", lines[0]);
    let listed = result_of(&lines[1]);
    let Some(Json::Array(tools)) = listed.get("tools") else { panic!("{}", lines[1]) };
    assert_eq!(tools.len(), 1);
    let divide = &tools[0];
    assert_eq!(divide.get("name").and_then(Json::text), Some("divide"));
    assert_eq!(divide.get("description").and_then(Json::text), Some("Divides one whole number by another."));
    let schema = divide.get("inputSchema").unwrap();
    assert_eq!(schema.get("type").and_then(Json::text), Some("object"));
    let properties = schema.get("properties").unwrap();
    for (name, comment) in [("a", "The number that is divided."), ("b", "The number it is divided by.")] {
        let one = properties.get(name).unwrap_or_else(|| panic!("no {name}: {}", lines[1]));
        assert_eq!(one.get("type").and_then(Json::text), Some("integer"), "{name}");
        assert_eq!(one.get("description").and_then(Json::text), Some(comment), "{name}");
    }
    let answered = result_of(&lines[2]);
    assert_eq!(answered.get("structuredContent"), Some(&Json::Number("2".into())), "{}", lines[2]);
    let Some(Json::Array(content)) = answered.get("content") else { panic!("{}", lines[2]) };
    assert_eq!(content[0].get("text").and_then(Json::text), Some("2"));
    assert_eq!(answered.get("isError"), None);
}

fn a_tool_that_fails_is_a_result_marked_as_an_error_and_a_tool_the_server_has_not_is_invalid_params() {
    let messages = [
        call(1, "divide", "{\"a\":6,\"b\":0}"),
        call(2, "multiply", "{\"a\":6,\"b\":0}"),
        request(3, "tools/call", "\"arguments\":{}"),
        call(4, "divide", "{\"a\":-9223372036854775808,\"b\":-1}"),
    ];
    let lines = serve(DIVIDE, &messages);
    assert_eq!(lines.len(), 4);
    let failed = result_of(&lines[0]);
    assert_eq!(failed.get("isError"), Some(&Json::Bool(true)), "{}", lines[0]);
    assert_eq!(failed.get("structuredContent").and_then(Json::text), Some("b is zero"));
    let Some(Json::Array(content)) = failed.get("content") else { panic!("{}", lines[0]) };
    assert!(content[0].get("text").and_then(Json::text).unwrap().contains("b is zero"));
    assert_eq!(error_of(&lines[1]).0, "-32602", "{}", lines[1]);
    assert!(error_of(&lines[1]).1.contains("multiply"));
    assert_eq!(error_of(&lines[2]).0, "-32602", "{}", lines[2]);
    // "b is zero" is said only of a `b` that is zero.
    let overflowed = result_of(&lines[3]);
    assert_eq!(overflowed.get("isError"), Some(&Json::Bool(true)), "{}", lines[3]);
    assert_ne!(overflowed.get("structuredContent").and_then(Json::text), Some("b is zero"), "{}", lines[3]);
}

/// A tool of a number in a range and a text, and a call whose arguments miss
/// one of them, give one that is of another kind, or give a number outside the
/// range: the parameter is named by the failure.
fn arguments_that_do_not_fit_are_invalid_params_with_the_parameter_named() {
    const SOURCE: &str = "use std.console (console)\nuse std.mcp (mcp, messages, stdio, tool)\n\nfn place(\n  // How many.\n  count: 1..6,\n  // What they are called.\n  label: str,\n) -> str {\n  label\n}\n\nfn serve() <messages, alloc> {\n  with mcp(name: \"p\", version: \"1\")\n  tool(place)\n}\n\nfn main() <console, alloc> {\n  with stdio()\n  serve()\n}\n";
    let program = lower(SOURCE.as_bytes()).unwrap_or_else(|e| panic!("{e:?}"));
    generate::check(
        7710,
        6,
        |g: &mut Gen| {
            let which = *g.pick(&["count", "label"]);
            let how = g.below(3);
            let count = 1 + g.below(5);
            let (arguments, named) = match (which, how) {
                ("count", 0) => ("{\"label\":\"x\"}".to_string(), "count"),
                ("count", 1) => ("{\"count\":\"x\",\"label\":\"x\"}".to_string(), "count"),
                ("count", _) => (format!("{{\"count\":{},\"label\":\"x\"}}", 6 + g.below(100)), "count"),
                ("label", 0) => (format!("{{\"count\":{count}}}"), "label"),
                (_, 1) => (format!("{{\"count\":{count},\"label\":7}}"), "label"),
                (_, _) => (format!("{{\"count\":{count},\"label\":null}}"), "label"),
            };
            (arguments, named)
        },
        |(arguments, named)| {
            let lines = serve_program(&program, &[call(1, "place", arguments)]);
            assert_eq!(lines.len(), 1);
            let (code, message) = error_of(&lines[0]);
            assert_eq!(code, "-32602", "{}", lines[0]);
            assert!(message.contains(named), "{message} for {arguments}");
        },
    );
    // The ones that fit are called.
    let lines = serve_program(&program, &[call(1, "place", "{\"count\":3,\"label\":\"ok\"}")]);
    assert_eq!(result_of(&lines[0]).get("structuredContent").and_then(Json::text), Some("ok"));
}

/// The kinds of parameter a tool takes.
#[derive(Clone, Debug)]
enum Kind {
    Int,
    Small,
    Text,
    Bool,
    Point,
    Option(Box<Kind>),
    List(Box<Kind>),
}

fn kind(g: &mut Gen, depth: usize) -> Kind {
    match g.below(if depth == 0 { 7 } else { 5 }) {
        0 => Kind::Int,
        1 => Kind::Small,
        2 => Kind::Text,
        3 => Kind::Bool,
        4 => Kind::Point,
        5 => Kind::Option(Box::new(kind(g, depth + 1))),
        _ => Kind::List(Box::new(kind(g, depth + 1))),
    }
}

fn ty(k: &Kind) -> String {
    match k {
        Kind::Int => "Int".into(),
        Kind::Small => "1..6".into(),
        Kind::Text => "str".into(),
        Kind::Bool => "bool".into(),
        Kind::Point => "Point".into(),
        Kind::Option(of) => format!("Option[{}]", ty(of)),
        Kind::List(of) => format!("List[{}]", ty(of)),
    }
}

fn word(g: &mut Gen) -> String {
    let letters = ['a', 'Z', '0', ' ', '.', '-', '/', 'é', '語', '"', '\\'];
    (0..g.below(5)).map(|_| *g.pick(&letters)).collect()
}

/// The text of a value of the kind, and what the tool gives back for it.
fn value(g: &mut Gen, k: &Kind) -> String {
    match k {
        Kind::Int => (g.below(2000) as i64 - 1000).to_string(),
        Kind::Small => (1 + g.below(5)).to_string(),
        Kind::Text => lit(&word(g)),
        Kind::Bool => g.pick(&["true", "false"]).to_string(),
        Kind::Point => format!("{{\"x\":{},\"label\":{}}}", g.below(100), lit(&word(g))),
        Kind::Option(of) => value(g, of),
        Kind::List(of) => {
            let items: Vec<String> = (0..g.below(4)).map(|_| value(g, of)).collect();
            format!("[{}]", items.join(","))
        }
    }
}

/// A tool of these parameters, each with a comment or none, and a comment on
/// the function, and the arguments of a call.
#[derive(Clone, Debug)]
struct Tool {
    about: String,
    params: Vec<(Kind, Option<String>)>,
    arguments: Vec<String>,
}

fn tool(g: &mut Gen) -> Tool {
    let params: Vec<(Kind, Option<String>)> = (0..1 + g.below(4))
        .map(|i| (kind(g, 0), g.one_in(2).then(|| format!("What p{i} is."))))
        .collect();
    let arguments = params.iter().map(|(k, _)| value(g, k)).collect();
    Tool { about: "Gives its arguments back.".into(), params, arguments }
}

const POINT: &str = "// A place.\nstruct Point {\n  // Across.\n  x: 0..100\n  label: str\n}\n\n";

fn comment_lines(about: &Option<String>, indent: &str) -> String {
    about.as_ref().map(|a| format!("{indent}// {a}\n")).unwrap_or_default()
}

fn server_source(t: &Tool) -> String {
    let mut s = String::from("use std.console (console)\nuse std.mcp (mcp, messages, stdio, tool)\n\n");
    s += POINT;
    s += "struct Echo {\n";
    for (i, (k, _)) in t.params.iter().enumerate() {
        s += &format!("  p{i}: {}\n", ty(k));
    }
    s += &format!("}}\n\n// {}\nfn echo(\n", t.about);
    for (i, (k, c)) in t.params.iter().enumerate() {
        s += &comment_lines(c, "  ");
        s += &format!("  p{i}: take {},\n", ty(k));
    }
    let fields: Vec<String> = (0..t.params.len()).map(|i| format!("p{i}: p{i}")).collect();
    s += &format!(") -> Echo {{\n  Echo({})\n}}\n\n", fields.join(", "));
    s += "fn serve() <messages, alloc> {\n  with mcp(name: \"echo\", version: \"1\")\n  tool(echo)\n}\n\nfn main() <console, alloc> {\n  with stdio()\n  serve()\n}\n";
    s
}

/// The program that prints what `Describe` gives for a struct of the fields.
fn schema_source(t: &Tool) -> String {
    let mut s = String::from("use std.console (console, print)\nuse std.describe (Describe)\nuse std.json (Json)\nuse std.jsonschema (json_schema, schema_text)\n\n");
    s += POINT;
    s += &format!("// {}\nstruct Args {{\n", t.about);
    for (i, (k, c)) in t.params.iter().enumerate() {
        s += &comment_lines(c, "  ");
        s += &format!("  p{i}: {}\n", ty(k));
    }
    s += "}\n\nfn main() <console, alloc> {\n  print(schema_text(Describe[Args].describe(json_schema(Json(cases: .key, names: .written)))))\n}\n";
    s
}

fn a_tool_is_listed_with_the_schema_describe_gives_and_a_call_reaches_it_with_the_values_it_holds(part: u64) {
    generate::check_part(7711, 4, part, 4, tool, |t| {
        let described = {
            let source = schema_source(t);
            let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for\n{source}"));
            let said = scalar::run(&program);
            String::from_utf8(said.stdout).unwrap().trim_end().to_string()
        };
        let members: Vec<String> = t.arguments.iter().enumerate().map(|(i, v)| format!("\"p{i}\":{v}")).collect();
        let arguments = format!("{{{}}}", members.join(","));
        let source = server_source(t);
        let lines = serve(&source, &[request(1, "tools/list", ""), call(2, "echo", &arguments)]);
        assert_eq!(lines.len(), 2, "{lines:?}\n{source}");
        let listed = result_of(&lines[0]);
        let Some(Json::Array(tools)) = listed.get("tools") else { panic!("{}", lines[0]) };
        assert_eq!(tools[0].get("inputSchema"), Some(&parse(&described)), "{}\n{source}", lines[0]);
        let answered = result_of(&lines[1]);
        assert_eq!(answered.get("structuredContent"), Some(&parse(&arguments)), "{}\n{source}", lines[1]);
    });
}

fn what_a_tool_prints_goes_to_the_standard_error_and_the_server_ends_with_the_input() {
    let source = "use std.console (console, print)\nuse std.mcp (mcp, messages, stdio, tool)\n\n// Says hello.\nfn hello(\n  // Who.\n  name: str,\n) -> str <console, alloc> {\n  print(\"loud {name}\")\n  \"hello {name}\"\n}\n\nfn serve() <messages, console, alloc> {\n  with mcp(name: \"h\", version: \"1\")\n  tool(hello)\n}\n\nfn main() <console, alloc> {\n  with stdio()\n  serve()\n}\n";
    let program = lower(source.as_bytes()).unwrap_or_else(|e| panic!("{e:?} for\n{source}"));
    let lines = serve_program(&program, &[call(1, "hello", "{\"name\":\"ada\"}"), call(2, "hello", "{\"name\":\"bo\"}")]);
    assert_eq!(lines.len(), 2, "only messages are written to the standard output: {lines:?}");
    assert_eq!(result_of(&lines[1]).get("structuredContent").and_then(Json::text), Some("hello bo"));
    // No input is no output, and the end of the input ends the server well.
    assert!(serve_program(&program, &[]).is_empty());
}

/// A comment of a function that is not a tool, or of a type it does not take
/// or give, is not in the executable, and the ones of the tool are.
fn a_comment_is_in_the_executable_only_for_a_function_given_to_tool_and_the_types_it_takes() {
    let source = "use std.console (console)\nuse std.mcp (mcp, messages, stdio, tool)\n\n// A shape that nothing takes: the-comment-of-an-unused-struct.\nstruct Unused {\n  // the-comment-of-an-unused-field\n  a: Int\n}\n\n// A shape that the tool takes: the-comment-of-a-used-struct.\nstruct Used {\n  // the-comment-of-a-used-field\n  a: Int\n}\n\n// the-comment-of-a-plain-function\nfn plain(a: Int) -> Int { a }\n\n// the-comment-of-the-tool\nfn tell(used: Used) -> Int { used.a }\n\nfn serve() <messages, alloc> {\n  with mcp(name: \"s\", version: \"1\")\n  tool(tell)\n}\n\nfn main() <console, alloc> {\n  with stdio()\n  serve()\n}\n";
    let bytes = targets::build(source.as_bytes(), targets::any()).unwrap_or_else(|e| panic!("{e:?}"));
    let has = |needle: &str| bytes.windows(needle.len()).any(|w| w == needle.as_bytes());
    for held in ["the-comment-of-the-tool", "the-comment-of-a-used-struct", "the-comment-of-a-used-field"] {
        assert!(has(held), "{held} is in the executable");
    }
    for left in ["the-comment-of-an-unused-struct", "the-comment-of-an-unused-field", "the-comment-of-a-plain-function"] {
        assert!(!has(left), "{left} is not in the executable");
    }
}

fn every_executable_of_a_server_answers_as_the_evaluator_does() {
    let messages = [
        request(1, "server/discover", ""),
        request(2, "tools/list", ""),
        call(3, "divide", "{\"a\":6,\"b\":3}"),
        call(4, "divide", "{\"a\":6,\"b\":0}"),
        call(5, "divide", "{\"a\":6}"),
        call(6, "nope", "{}"),
        "{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}".to_string(),
    ];
    let mut input = String::new();
    for m in &messages {
        input += m;
        input += "\n";
    }
    let program = lower(DIVIDE.as_bytes()).unwrap_or_else(|e| panic!("{e:?}"));
    let said = scalar::run_with_input(&program, input.as_bytes());
    let cases: Vec<(&[u8], scalar::Outcome)> = vec![(input.as_bytes(), said)];
    for target in targets::runnable().into_iter().filter(|t| t.is_machine_code()) {
        targets::runs_as_each_given(DIVIDE.as_bytes(), target, &cases);
    }
}
