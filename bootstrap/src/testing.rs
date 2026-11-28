//! The tests of a file `x_test.smith`, which are its functions named `test_`,
//! and the program that runs one of them.
//!
//! The program is the file with a `main` written after it, so that it calls
//! the file's own functions, which no other module could list. `main` is
//! started with the name of a test, runs that test under the effects it lists,
//! which the system answers as it answers any `main`, and ends with the failure
//! the test gives, written out, or with status 0 when it passes.

use crate::diag::Error;
use crate::lexer;
use crate::parser::{self, Function};

/// Whether a file's name is that of a file of tests: `x_test.smith`.
pub fn is_test_file(name: &str) -> bool {
    name.strip_suffix("_test.smith")
        .is_some_and(|stem| !stem.is_empty() && !stem.contains('.'))
}

/// The tests of a file, by their names, and the effects they list.
pub struct Tests {
    pub names: Vec<String>,
    effects: Vec<String>,
}

/// The tests of a file of tests, or the errors that refuse a function named
/// `test_` that is not a test. A file that does not parse has no tests here:
/// its errors are said where it is built.
pub fn read(source: &str) -> Result<Tests, Vec<Error>> {
    let Ok(file) = lexer::lex(source.as_bytes()).and_then(|t| parser::parse_library(&t)) else {
        return Ok(Tests {
            names: Vec::new(),
            effects: Vec::new(),
        });
    };
    let failed = failed_name(&file);
    let mut names = Vec::new();
    let mut effects: Vec<String> = Vec::new();
    let mut errors = Vec::new();
    for f in file
        .functions
        .iter()
        .filter(|f| f.name.starts_with("test_"))
    {
        if !f.params.is_empty() || !gives_result(f, &failed) {
            errors.push(
                Error::new(
                    f.pos,
                    format!(
                        "`{}` is named as a test, and a test takes nothing and gives `Result[(), Failed]`",
                        f.name
                    ),
                )
                .with_fix(format!("write `fn {}() -> Result[(), Failed]`", f.name)),
            );
            continue;
        }
        names.push(f.name.clone());
        for (e, _) in &f.effects {
            if !effects.contains(e) {
                effects.push(e.clone());
            }
        }
    }
    if errors.is_empty() {
        Ok(Tests { names, effects })
    } else {
        Err(errors)
    }
}

/// The name the file gives `Failed` of `std.test`, which is `Failed` when it
/// does not list it, as the checker then says.
fn failed_name(file: &parser::File) -> String {
    listed_failed(file).unwrap_or_else(|| "Failed".to_string())
}

fn listed_failed(file: &parser::File) -> Option<String> {
    let u = file.uses.iter().find(|u| {
        let module: Vec<&str> = u.module.iter().map(|(n, _)| n.as_str()).collect();
        module == ["std", "test"]
    })?;
    let listed = u.names.iter().find(|n| n.name == "Failed")?;
    Some(
        listed
            .alias
            .as_ref()
            .map_or(&listed.name, |(a, _)| a)
            .clone(),
    )
}

/// Whether the function's result is written `Result[(), Failed]`.
fn gives_result(f: &Function, failed: &str) -> bool {
    let Some(t) = &f.result else {
        return false;
    };
    let is_unit = |t: &parser::Type| t.name.is_empty() && t.args.is_empty() && !t.function;
    t.name == "Result"
        && t.args.len() == 2
        && is_unit(&t.args[0])
        && t.args[1].name == failed
        && t.args[1].args.is_empty()
}

impl Tests {
    /// The file with a `main` after it that runs the test it is started with.
    /// The lines of the file keep their places.
    pub fn program(&self, source: &str) -> String {
        let mut effects = vec!["alloc".to_string()];
        effects.extend(self.effects.iter().filter(|e| *e != "alloc").cloned());
        let mut t = source.to_string();
        if !t.ends_with('\n') {
            t.push('\n');
        }
        t.push_str(&format!(
            "\nfn main(smith_test_words: List[str]) -> Result[(), str] <{}> {{\n",
            effects.join(", ")
        ));
        t.push_str("  mut smith_test_chosen = \"\"\n");
        t.push_str(
            "  loop smith_test_word in smith_test_words { smith_test_chosen = smith_test_word }\n",
        );
        let mut branches = Vec::new();
        for name in &self.names {
            branches.push(format!(
                "if smith_test_chosen == \"{name}\" {{\n    match {name}() {{\n      .ok(smith_test_passed) {{ .ok(()) }}\n      .err(smith_test_failed) {{\n        smith_test_sides = match smith_test_failed.sides {{\n          .some(smith_test_two) {{ \"\\n  left:  {{smith_test_two.left}}\\n  right: {{smith_test_two.right}}\" }}\n          .none {{ \"\" }}\n        }}\n        .err(\"{{smith_test_failed.path}}:{{smith_test_failed.line}}:{{smith_test_failed.column}}: expected {{smith_test_failed.check}}{{smith_test_sides}}\")\n      }}\n    }}\n  }}"
            ));
        }
        branches.push("{\n    .err(\"no test is named `{smith_test_chosen}`\")\n  }".to_string());
        t.push_str(&format!("  {}\n}}\n", branches.join(" else ")));
        t
    }
}
