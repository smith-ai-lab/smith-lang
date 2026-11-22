//! The lexer of `compiler/lexer.smith` and the Rust lexer, each run on text
//! and printed in one plain form, so that the two are compared line by line.
//! Nothing here is a test.

use std::sync::OnceLock;

/// The lexer written in Smith, without the `main` it ships with.
fn lexer() -> &'static str {
    static LEXER: OnceLock<String> = OnceLock::new();
    LEXER.get_or_init(|| {
        let whole = include_str!("../../../compiler/lexer.smith");
        let cut = whole.find("\nfn main()").expect("lexer.smith has a main");
        whole[..cut + 1].to_string()
    })
}

/// What a program that lexes each input prints for it. Each input starts with
/// a line `input`.
const DRIVER: &str = r#"
fn show(s: str) -> str ! alloc {
  var joined: str = ""
  loop i in 0..s.size {
    if i == 0 { joined = "{s[i]}" } else { joined = "{joined},{s[i]}" }
  }
  joined
}

fn keyword_place(k: Keyword) -> Nat {
  match k {
    .kw_and { 0 }
    .kw_as { 1 }
    .kw_break { 2 }
    .kw_derive { 3 }
    .kw_effect { 4 }
    .kw_else { 5 }
    .kw_ensures { 6 }
    .kw_enum { 7 }
    .kw_fn { 8 }
    .kw_for { 9 }
    .kw_foreign { 10 }
    .kw_if { 11 }
    .kw_impl { 12 }
    .kw_in { 13 }
    .kw_loop { 14 }
    .kw_match { 15 }
    .kw_not { 16 }
    .kw_or { 17 }
    .kw_out { 18 }
    .kw_pub { 19 }
    .kw_requires { 20 }
    .kw_return { 21 }
    .kw_struct { 22 }
    .kw_take { 23 }
    .kw_trait { 24 }
    .kw_try { 25 }
    .kw_use { 26 }
    .kw_var { 27 }
    .kw_with { 28 }
    .kw_without { 29 }
  }
}

fn punct_place(p: Punct) -> Nat {
  match p {
    .dot_dot { 0 }
    .arrow { 1 }
    .plus_equal { 2 }
    .minus_equal { 3 }
    .star_equal { 4 }
    .slash_equal { 5 }
    .equal_equal { 6 }
    .bang_equal { 7 }
    .less_equal { 8 }
    .greater_equal { 9 }
    .open_paren { 10 }
    .close_paren { 11 }
    .open_bracket { 12 }
    .close_bracket { 13 }
    .open_brace { 14 }
    .close_brace { 15 }
    .comma { 16 }
    .colon { 17 }
    .dot { 18 }
    .equal { 19 }
    .less { 20 }
    .greater { 21 }
    .plus { 22 }
    .minus { 23 }
    .star { 24 }
    .slash { 25 }
    .at_sign { 26 }
    .caret { 27 }
    .bang { 28 }
    .pipe_arrow { 29 }
    .percent { 30 }
  }
}

fn show_token(t: Token, fuel: u64) ! console, alloc
  decreases fuel
{
  l = t.pos.line
  c = t.pos.col
  match t.tok {
    .ident(s) { print("ident {l} {c} {show(s)}") }
    .keyword(k) { print("keyword {l} {c} {keyword_place(k)}") }
    .number(s) { print("number {l} {c} {show(s)}") }
    .string(s) { print("string {l} {c} {show(s)}") }
    .interp(pieces) {
      print("interp_open {l} {c}")
      loop i in 0..pieces.size {
        match pieces[i] {
          .text(s) { print("text {l} {c} {show(s)}") }
          .expr(tokens) { if fuel > 0 { show_tokens(tokens, fuel - 1) } }
        }
      }
      print("interp_close {l} {c}")
    }
    .punct(p) { print("punct {l} {c} {punct_place(p)}") }
    .newline { print("newline {l} {c}") }
    .eof { print("eof {l} {c}") }
  }
}

fn show_tokens(tokens: List[Token], fuel: u64) ! console, alloc
  decreases fuel
{
  loop i in 0..tokens.size {
    if fuel > 0 { show_token(tokens[i], fuel - 1) }
  }
}

fn show_error(e: LexError) ! console, alloc {
  print("error {e.pos.line} {e.pos.col} {e.message}")
}

fn main() ! console, alloc {
  texts = inputs()
  loop k in 0..texts.size {
    print("input")
    match lex(texts[k]) {
      .ok(tokens) { show_tokens(tokens, 100000) }
      .err(errors) {
        loop i in 0..errors.size { show_error(errors[i]) }
      }
    }
  }
}
"#;

/// A program that lexes each of the texts, with the lexer written in Smith.
pub fn program(texts: &[&[u8]]) -> String {
    let mut out = String::from(lexer());
    out.push_str(DRIVER);
    out.push_str("\nfn nothing() -> List[Byte] ! alloc {\n  []\n}\n");
    out.push_str("\nfn inputs() -> List[List[Byte]] ! alloc {\n  [");
    for (i, text) in texts.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        // A list that holds no byte has no type of its own.
        if text.is_empty() {
            out.push_str("nothing()");
            continue;
        }
        let bytes: Vec<String> = text.iter().map(|b| b.to_string()).collect();
        out.push_str(&format!("[{}]", bytes.join(", ")));
    }
    out.push_str("]\n}\n");
    out
}

/// What the program of `program` prints when the lexer written in Smith
/// agrees with the Rust lexer: a line `input` and then the lines of the Rust
/// lexer, for each text.
pub fn expected(texts: &[&[u8]]) -> String {
    let mut out = String::new();
    for text in texts {
        out.push_str("input\n");
        for line in rust(text) {
            out.push_str(&line);
            out.push('\n');
        }
    }
    out
}

/// What the Rust lexer says of a text, in the same form.
pub fn rust(text: &[u8]) -> Vec<String> {
    match smith::lex_plain(text) {
        Ok(tokens) => tokens.lines().map(str::to_string).collect(),
        Err(errors) => errors
            .iter()
            .map(|e| format!("error {} {} {}", e.pos.line, e.pos.col, e.message))
            .collect(),
    }
}
