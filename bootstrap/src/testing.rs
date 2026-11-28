//! The tests of a file `x_test.smith`, which are its functions named `test_`,
//! and the program that runs the tests chosen as tasks in one process.
//!
//! Each file of tests is read as the module its path names, with a public
//! function written after it for each test chosen, `smith_test_<k>`, which
//! calls the test, as no other module could, and gives what its failure says
//! as text. The program is a `main` that imports them, and runs them in waves
//! of tasks of a group, each with an arena of its own: a wave spawns its tests
//! and then joins each, in order, with `outcome`, so that a test that ends by
//! a failure of its own is said and the others go on, and a wave's memory is
//! given back before the next starts. The program prints records of each
//! test, as it begins and ends and what became of it, which `smith test`
//! reads, times the test by, and says as it should be said.

use crate::diag::Error;
use crate::lexer;
use crate::parser::{self, Function};

/// What begins each line the program prints about a test, after which come
/// what became of it and the number of the test.
pub const RECORD: &str = "@smith-test ";

/// Whether a file's name is that of a file of tests: `x_test.smith`.
pub fn is_test_file(name: &str) -> bool {
    name.strip_suffix("_test.smith")
        .is_some_and(|stem| !stem.is_empty() && !stem.contains('.'))
}

/// A test of a file: its name and the effects it lists.
pub struct Test {
    pub name: String,
    effects: Vec<String>,
}

/// The tests of a file, and the `use` lines of the effects they list.
pub struct Tests {
    pub tests: Vec<Test>,
    /// Whether the file parses. One that does not has no tests here, and is
    /// built all the same, so that its errors are said.
    pub parses: bool,
    /// The names each `use` of the file lists that are effects a test lists,
    /// by the module, each as it is written: `name` or `name as alias`.
    imports: Vec<(String, Vec<(String, String)>)>,
}

/// The tests of a file of tests, or the errors that refuse a function named
/// `test_` that is not a test. A file that does not parse has no tests here:
/// its errors are said where it is built.
pub fn read(source: &str) -> Result<Tests, Vec<Error>> {
    let Ok(file) = lexer::lex(source.as_bytes()).and_then(|t| parser::parse_library(&t)) else {
        return Ok(Tests {
            tests: Vec::new(),
            parses: false,
            imports: Vec::new(),
        });
    };
    let failed = failed_name(&file);
    let mut tests = Vec::new();
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
        tests.push(Test {
            name: f.name.clone(),
            effects: f.effects.iter().map(|(e, _)| e.clone()).collect(),
        });
    }
    let listed: Vec<&String> = tests.iter().flat_map(|t| &t.effects).collect();
    let mut imports = Vec::new();
    for u in &file.uses {
        let names: Vec<(String, String)> = (u.names.iter())
            .map(|n| {
                let alias = n.alias.as_ref().map_or(&n.name, |(a, _)| a);
                (n.name.clone(), alias.clone())
            })
            .filter(|(_, alias)| listed.contains(&alias))
            .collect();
        if !names.is_empty() {
            let module: Vec<&str> = u.module.iter().map(|(n, _)| n.as_str()).collect();
            imports.push((module.join("."), names));
        }
    }
    if errors.is_empty() {
        Ok(Tests {
            tests,
            parses: true,
            imports,
        })
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

/// The effects of a test and `alloc`, which its failure is written with, as
/// a signature lists them.
fn with_alloc(effects: &[String]) -> Vec<String> {
    let mut all = vec!["alloc".to_string()];
    all.extend(effects.iter().filter(|e| *e != "alloc").cloned());
    all
}

impl Tests {
    /// The test with the name.
    fn test(&self, name: &str) -> &Test {
        self.tests
            .iter()
            .find(|t| t.name == name)
            .expect("a test of the file")
    }

    /// The file with a public function after it for each test chosen, which
    /// gives `.ok(())` when it passes and what its failure says when it
    /// fails. The test numbered `k` is `smith_test_<k>`. The lines of the file
    /// keep their places.
    pub fn module(&self, source: &str, chosen: &[(usize, &str)]) -> String {
        let mut t = source.to_string();
        if !t.ends_with('\n') {
            t.push('\n');
        }
        for (k, name) in chosen {
            let effects = with_alloc(&self.test(name).effects);
            t.push_str(&format!(
                "\npub fn smith_test_{k}() -> Result[(), str] <{}> {{\n  match {name}() {{\n    .ok(smith_test_passed) {{ .ok(()) }}\n    .err(smith_test_failed) {{\n      smith_test_sides = match smith_test_failed.sides {{\n        .some(smith_test_two) {{ \"\\n  left:  {{smith_test_two.left}}\\n  right: {{smith_test_two.right}}\" }}\n        .none {{ \"\" }}\n      }}\n      .err(\"{{smith_test_failed.path}}:{{smith_test_failed.line}}:{{smith_test_failed.column}}: expected {{smith_test_failed.check}}{{smith_test_sides}}\")\n    }}\n  }}\n}}\n",
                effects.join(", ")
            ));
        }
        t
    }
}

/// A file of tests that the program runs: the module it is, its tests, and
/// the tests chosen of it, each with its number.
pub struct File<'a> {
    pub module: &'a str,
    pub tests: &'a Tests,
    pub chosen: &'a [(usize, &'a str)],
}

/// The names the program itself uses, by the module.
const OWN: &[(&str, &[&str])] = &[
    ("std.bits", &["u64"]),
    ("std.console", &["console", "print"]),
    ("std.task", &["Failure", "outcome", "spawn", "tasks"]),
];

/// The program that runs the tests chosen of the files, `width` at a time,
/// as tasks of a group, and prints records of each, which say:
///
/// - `@smith-test began <k>`, from the task of the test as it begins, and
///   `@smith-test done <k>` as it ends, so that what reads them times the
///   test, as not every target builds the clock
/// - `@smith-test passed <k>`
/// - `@smith-test failed <k> <bytes>`, then what its failure says, of that
///   many bytes, and a line break
/// - `@smith-test ended <k> <failure>`, when the task ended by a failure of
///   its own, which is `out_of_memory` or `refused_write`
pub fn program(files: &[File], width: usize) -> String {
    let width = width.max(1);
    // One `use` for each module, which the program's own names and the
    // effects of the tests are listed in together.
    let mut uses: Vec<(String, Vec<String>)> = Vec::new();
    let mut add = |module: &str, listed: String| {
        let at = match uses.iter().position(|(m, _)| m == module) {
            Some(at) => at,
            None => {
                uses.push((module.to_string(), Vec::new()));
                uses.len() - 1
            }
        };
        if !uses[at].1.contains(&listed) {
            uses[at].1.push(listed);
        }
    };
    for (module, names) in OWN {
        for name in *names {
            add(module, (*name).to_string());
        }
    }
    let mut effects = vec!["alloc".to_string(), "console".to_string()];
    for file in files {
        let ks: Vec<String> = (file.chosen.iter())
            .map(|(k, _)| format!("smith_test_{k}"))
            .collect();
        if ks.is_empty() {
            // A file that does not parse is imported for its errors alone.
            add(file.module, String::new());
        }
        for k in ks {
            add(file.module, k);
        }
        for (module, names) in &file.tests.imports {
            for (name, alias) in names {
                if !file
                    .chosen
                    .iter()
                    .any(|(_, t)| file.tests.test(t).effects.contains(alias))
                {
                    continue;
                }
                let listed = if name == alias {
                    name.clone()
                } else {
                    format!("{name} as {alias}")
                };
                add(module, listed);
            }
        }
        for (_, name) in file.chosen {
            for e in &file.tests.test(name).effects {
                if !effects.contains(e) {
                    effects.push(e.clone());
                }
            }
        }
    }
    let mut t = String::new();
    uses.sort();
    for (module, names) in &uses {
        let names: Vec<&str> = names
            .iter()
            .map(String::as_str)
            .filter(|n| !n.is_empty())
            .collect();
        t.push_str(&format!("use {module} ({})\n", names.join(", ")));
    }
    let effects = effects.join(", ");
    t.push_str(SAID);
    let all: Vec<usize> = files
        .iter()
        .flat_map(|f| f.chosen.iter().map(|(k, _)| *k))
        .collect();
    let waves: Vec<&[usize]> = all.chunks(width).collect();
    for (w, wave) in waves.iter().enumerate() {
        t.push_str(&format!(
            "\nfn smith_test_wave_{w}() <{effects}> {{\n  with tasks()\n"
        ));
        for k in *wave {
            t.push_str(&format!(
                "  smith_test_task_{k} = spawn {{ smith_test_done(smith_test_began({k}), smith_test_{k}()) }}\n"
            ));
        }
        for k in *wave {
            t.push_str(&format!(
                "  smith_test_report({k}, outcome(take smith_test_task_{k}))\n"
            ));
        }
        t.push_str("}\n");
    }
    t.push_str(&format!("\nfn main() <{effects}> {{\n"));
    for w in 0..waves.len() {
        t.push_str(&format!("  smith_test_wave_{w}()\n"));
    }
    t.push_str("}\n");
    t
}

/// What the program prints of each test.
const SAID: &str = "
fn smith_test_began(smith_test_at: u64) -> u64 <console, alloc> {
  print(\"@smith-test began {smith_test_at}\")
  smith_test_at
}

fn smith_test_done(smith_test_at: u64, smith_test_result: Result[(), str]) -> Result[(), str] <console, alloc> {
  print(\"@smith-test done {smith_test_at}\")
  smith_test_result
}

fn smith_test_report(smith_test_at: u64, smith_test_outcome: Result[Result[(), str], Failure]) <console, alloc> {
  match smith_test_outcome {
    .ok(smith_test_result) {
      match smith_test_result {
        .ok(smith_test_passed) { print(\"@smith-test passed {smith_test_at}\") }
        .err(smith_test_text) { print(\"@smith-test failed {smith_test_at} {smith_test_text.size}\\n{smith_test_text}\") }
      }
    }
    .err(smith_test_failure) {
      smith_test_kind = match smith_test_failure {
        .out_of_memory { \"out_of_memory\" }
        .refused_write { \"refused_write\" }
      }
      print(\"@smith-test ended {smith_test_at} {smith_test_kind}\")
    }
  }
}
";
