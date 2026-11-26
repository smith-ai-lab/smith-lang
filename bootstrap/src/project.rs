//! `project.smith`: the package it names and the tasks it holds, and the
//! program that starts at one of them, whose command line is the task's flags.
//!
//! A task is a public function of `project.smith`, which is read as the module
//! `project`. The program is made as text, a `main` that reads each flag from
//! the words it is started with, and calls the task with what they say. The
//! text is checked and built as any program is, so nothing here knows what a
//! function is called with beyond the flags' types.

use crate::diag::{Error, Pos};
use crate::lexer;
use crate::parser::{self, Bound, Type};

/// The file `project.smith` is, as errors name it.
const FILE: &str = "project.smith";

/// The names of the commands of `smith`, which no task is named.
const COMMANDS: &[&str] = &[
    "build", "run", "check", "new", "test", "fmt", "vendor", "hole", "find", "deps", "cost",
];

/// The most parameters with a default that a task may have. A task is called
/// once for each choice of which of them are given, so the program grows with
/// two to that power.
const MOST_DEFAULTS: usize = 6;

/// A parameter of a task, which is a flag.
pub struct Flag {
    pub name: String,
    /// The type that is read from the text of the flag: the parameter's type,
    /// or what an `Option` holds.
    value: String,
    optional: bool,
    default: bool,
    boolean: bool,
}

/// A public function of `project.smith`.
pub struct Task {
    pub name: String,
    /// Where the task's name is written in `project.smith`.
    pos: Pos,
    flags: Vec<Flag>,
    /// The `use` lines of the effects the task lists, as they are written.
    imports: Vec<String>,
    effects: Vec<String>,
    gives_result: bool,
}

/// What `project.smith` says.
pub struct Read {
    /// The name of the package, when `package` is written.
    pub package: Option<String>,
    /// The text of the module `project`, which is the file with the lines the
    /// compiler reads no further left empty.
    pub module: String,
    pub tasks: Vec<Task>,
}

/// Reads `project.smith`, or gives the errors in it.
pub fn read(source: &str) -> Result<Read, Vec<Error>> {
    let mut package = None;
    let mut module = String::new();
    for line in source.split('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with("package") && trimmed.contains('=') {
            package = trimmed
                .split_once("name:")
                .and_then(|(_, rest)| rest.split('"').nth(1))
                .map(str::to_string);
        } else if !trimmed.starts_with("use std.project") {
            module.push_str(line);
        }
        module.push('\n');
    }
    module.pop();
    let in_file = |errors: Vec<Error>| -> Vec<Error> {
        errors.into_iter().map(|e| e.in_module(FILE)).collect()
    };
    let tokens = lexer::lex(module.as_bytes()).map_err(in_file)?;
    let file = parser::parse_library(&tokens).map_err(in_file)?;
    let mut errors = Vec::new();
    let mut tasks = Vec::new();
    for f in file
        .functions
        .iter()
        .filter(|f| file.exports.contains(&f.name))
    {
        if COMMANDS.contains(&f.name.as_str()) {
            errors.push(
                Error::new(
                    f.pos,
                    format!(
                        "`{}` is the name of a command of smith, and a task cannot be named so",
                        f.name
                    ),
                )
                .with_fix("give the task another name")
                .in_module(FILE),
            );
            continue;
        }
        let mut flags = Vec::new();
        for p in &f.params {
            if !p.by_name {
                errors.push(
                    Error::new(
                        p.pos,
                        format!(
                            "`{}` of the task `{}` is a flag, which is given by name, so it is written after `;`",
                            p.name, f.name
                        ),
                    )
                    .with_fix(format!("write `fn {}(; ...)`, with every parameter after the `;`", f.name))
                    .in_module(FILE),
                );
                continue;
            }
            match flag(p) {
                Some(flag) => flags.push(flag),
                None => errors.push(
                    Error::new(
                        p.pos,
                        format!(
                            "`{}` of the task `{}` cannot be a flag, which is a text, a whole number, a bool, or an Option of one of them",
                            p.name, f.name
                        ),
                    )
                    .in_module(FILE),
                ),
            }
        }
        let defaults = flags.iter().filter(|f| f.default).count();
        if defaults > MOST_DEFAULTS {
            errors.push(
                Error::new(
                    f.pos,
                    format!(
                        "the task `{}` has more than {MOST_DEFAULTS} parameters with a default",
                        f.name
                    ),
                )
                .in_module(FILE),
            );
        }
        let effects: Vec<String> = f.effects.iter().map(|(e, _)| e.clone()).collect();
        let mut imports = Vec::new();
        for u in &file.uses {
            let listed: Vec<String> = u
                .names
                .iter()
                .filter(|n| {
                    effects.contains(&n.alias.as_ref().map_or(&n.name, |(a, _)| a).to_string())
                })
                .map(|n| match &n.alias {
                    Some((a, _)) => format!("{} as {a}", n.name),
                    None => n.name.clone(),
                })
                .collect();
            if !listed.is_empty() {
                let module: Vec<&str> = u.module.iter().map(|(n, _)| n.as_str()).collect();
                imports.push(format!(
                    "use {} ({})\n",
                    module.join("."),
                    listed.join(", ")
                ));
            }
        }
        tasks.push(Task {
            name: f.name.clone(),
            pos: f.pos,
            flags,
            imports,
            effects,
            gives_result: f.result.as_ref().is_some_and(|t| t.name == "Result"),
        });
    }
    if errors.is_empty() {
        Ok(Read {
            package,
            module,
            tasks,
        })
    } else {
        Err(errors)
    }
}

/// The type as it is written, when a flag can have it.
fn written(t: &Type) -> Option<String> {
    if t.function || t.statement.is_some() || t.length.is_some() {
        return None;
    }
    if t.name == parser::RANGE {
        return match &t.range {
            Some((Bound::Number(a), Bound::Number(b))) => Some(format!("{a}..{b}")),
            _ => None,
        };
    }
    let plain = matches!(t.name.as_str(), "str" | "bool" | "Nat" | "Int" | "Byte")
        || (t.name.len() > 1
            && matches!(t.name.as_bytes()[0], b'u' | b'i')
            && t.name[1..].bytes().all(|b| b.is_ascii_digit()));
    (plain && t.args.is_empty()).then(|| t.name.clone())
}

fn flag(p: &parser::Param) -> Option<Flag> {
    let (value, optional) = if p.ty.name == "Option" && p.ty.args.len() == 1 {
        (written(&p.ty.args[0])?, true)
    } else {
        (written(&p.ty)?, false)
    };
    Some(Flag {
        name: p.name.clone(),
        boolean: !optional && value == "bool",
        value,
        optional,
        default: p.default.is_some(),
    })
}

impl Read {
    pub fn task(&self, name: &str) -> Option<&Task> {
        self.tasks.iter().find(|t| t.name == name)
    }
}

impl Task {
    /// The errors with the ones in the text the compiler writes for the task
    /// given the place of the task's name, which is in `project.smith`, so that
    /// every error shows a line the programmer wrote.
    pub fn placed(&self, errors: Vec<Error>) -> Vec<Error> {
        let mut placed: Vec<Error> = Vec::new();
        for mut e in errors {
            if e.module.is_none() {
                // The effects of the task are listed again by the program the
                // compiler writes, and a name the task does not import is the
                // same missing import, which `project.smith` already says once
                // where the task lists it.
                if e.message.ends_with("this file does not import it") {
                    continue;
                }
                e.pos = self.pos;
                e.module = Some(FILE.to_string());
                e.fix = None;
            }
            if !placed.contains(&e) {
                placed.push(e);
            }
        }
        placed
    }

    /// The program that starts at the task, as the text of a file with a
    /// `main`. It reads the flags from the words it is started with, says the
    /// first thing that is wrong with them and ends with a status that is not
    /// 0, and otherwise calls the task.
    pub fn program(&self) -> String {
        let mut effects = self.effects.clone();
        if !effects.iter().any(|e| e == "alloc") {
            effects.push("alloc".to_string());
        }
        let mut t = String::new();
        t.push_str(&format!("use project ({})\n", self.name));
        for import in &self.imports {
            t.push_str(import);
        }
        t.push_str("use std.deserialize (ReadError)\nuse std.fromtext (from_text)\n\n");
        t.push_str(&format!(
            "fn main(args: List[str]) -> Result[(), str] <{}> {{\n",
            effects.join(", ")
        ));
        for k in 0..self.flags.len() {
            t.push_str(&format!("  var s{k}: Option[str] = .none\n"));
        }
        // Only a flag that takes a value leaves the loop waiting for it.
        let waits = self.flags.iter().any(|f| !f.boolean);
        t.push_str("  var failure: Option[str] = .none\n");
        if waits {
            t.push_str("  var waiting = 0\n");
        }
        t.push_str("  loop i in 0..args.size {\n    arg = args[i]\n");
        let mut branches: Vec<String> = Vec::new();
        if waits {
            let mut waiting = String::from("if waiting > 0 {\n");
            for (k, f) in self.flags.iter().enumerate() {
                if !f.boolean {
                    waiting.push_str(&format!(
                        "      if waiting == {} {{ s{k} = .some(arg) }}\n",
                        k + 1
                    ));
                }
            }
            waiting.push_str("      waiting = 0\n    }");
            branches.push(waiting);
        }
        for (k, f) in self.flags.iter().enumerate() {
            let take = if f.boolean {
                format!("s{k} = .some(\"true\")")
            } else {
                format!("waiting = {}", k + 1)
            };
            branches.push(format!(
                "if arg == \"--{name}\" {{\n      match s{k} {{\n        .some(x) {{ {twice} }}\n        .none {{ {take} }}\n      }}\n    }}",
                name = f.name,
                twice = fail(&format!("--{} is given twice", f.name)),
            ));
        }
        let unknown = fail_with("\"{arg} is not a flag of this task\"");
        // With no branch before it, nothing is else.
        branches.push(if branches.is_empty() {
            unknown
        } else {
            format!("{{\n      {unknown}\n    }}")
        });
        t.push_str(&format!("    {}\n  }}\n", branches.join(" else ")));
        for (k, f) in self.flags.iter().enumerate() {
            if !f.boolean {
                t.push_str(&format!(
                    "  if waiting == {} {{ {} }}\n",
                    k + 1,
                    fail(&format!("--{} needs a value", f.name))
                ));
            }
        }
        for (k, f) in self.flags.iter().enumerate() {
            if f.boolean {
                t.push_str(&format!(
                    "  b{k} = match s{k} {{\n    .some(x) {{ true }}\n    .none {{ false }}\n  }}\n"
                ));
                continue;
            }
            let needed = if f.optional || f.default {
                String::new()
            } else {
                fail(&format!("--{} is needed", f.name))
            };
            t.push_str(&format!(
                "  var v{k}: Option[{ty}] = .none\n  match s{k} {{\n    .some(text) {{\n      g{k}: Result[{ty}, ReadError] = from_text(\"{name}\", text)\n      match g{k} {{\n        .ok(x) {{ v{k} = .some(x) }}\n        .err(e) {{ {bad} }}\n      }}\n    }}\n    .none {{ {needed} }}\n  }}\n",
                ty = f.value,
                name = f.name,
                bad = fail_with(&format!("\"--{}: `{{text}}` is not read as {}\"", f.name, f.value)),
            ));
        }
        t.push_str("  match failure {\n    .some(why) { .err(why) }\n    .none {\n");
        let required: Vec<usize> = (0..self.flags.len())
            .filter(|&k| {
                let f = &self.flags[k];
                !f.boolean && !f.optional && !f.default
            })
            .collect();
        let defaulted: Vec<usize> = (0..self.flags.len())
            .filter(|&k| self.flags[k].default && !self.flags[k].boolean)
            .collect();
        let mut body = String::new();
        self.unwrap(&required, &defaulted, &mut Vec::new(), &mut body);
        t.push_str(&body);
        t.push_str("    }\n  }\n}\n");
        t
    }

    /// Takes each required flag out of its `Option`, then branches on each
    /// flag that has a default, and calls the task at the end of each branch.
    fn unwrap(
        &self,
        required: &[usize],
        defaulted: &[usize],
        given: &mut Vec<usize>,
        out: &mut String,
    ) {
        if let Some((&k, rest)) = required.split_first() {
            out.push_str(&format!(
                "match v{k} {{\n  .none {{ Result.ok(()) }}\n  .some(r{k}) {{\n"
            ));
            given.push(k);
            self.unwrap(rest, defaulted, given, out);
            given.pop();
            out.push_str("  }\n}\n");
        } else if let Some((&k, rest)) = defaulted.split_first() {
            out.push_str(&format!("match v{k} {{\n  .some(r{k}) {{\n"));
            given.push(k);
            self.unwrap(required, rest, given, out);
            given.pop();
            out.push_str("  }\n  .none {\n");
            self.unwrap(required, rest, given, out);
            out.push_str("  }\n}\n");
        } else {
            let mut args = Vec::new();
            for (k, f) in self.flags.iter().enumerate() {
                if f.boolean {
                    args.push(format!("{}: b{k}", f.name));
                } else if f.optional {
                    args.push(format!("{}: v{k}", f.name));
                } else if given.contains(&k) {
                    args.push(format!("{}: r{k}", f.name));
                }
            }
            let call = format!("{}({})", self.name, args.join(", "));
            if self.gives_result {
                out.push_str(&format!(
                    "match {call} {{\n  .ok(done) {{ Result.ok(()) }}\n  .err(e) {{ Result.err(fmt(e)) }}\n}}\n"
                ));
            } else {
                out.push_str(&format!("{call}\nResult.ok(())\n"));
            }
        }
    }
}

/// A statement that notes what is wrong, when nothing else is yet.
fn fail(message: &str) -> String {
    fail_with(&format!("\"{message}\""))
}

fn fail_with(message: &str) -> String {
    format!("match failure {{\n .none {{ failure = .some({message}) }}\n .some(f) {{ }}\n }}")
}

/// The effects a system answers, as `provided-effects.md` lists them.
const PROVIDED: &[&str] = &[
    "console", "fs", "net", "time", "entropy", "ui", "alloc", "task", "gpu", "diverge", "foreign",
    "exec",
];

/// Says of an error in `project.smith` that the task does not list an effect,
/// and that no system answers it, so that the task is known to be one that
/// nothing runs: where a task calls `main` that needs what the task neither
/// lists nor answers.
pub fn explain(errors: Vec<Error>) -> Vec<Error> {
    errors
        .into_iter()
        .map(|mut e| {
            if e.module.as_deref() == Some(FILE)
                && let Some((_, rest)) = e.message.split_once(" needs the `")
                && let Some((effect, tail)) = rest.split_once("` effect, which `")
                && tail.ends_with("` does not declare")
                && !PROVIDED.contains(&effect)
            {
                let task = tail.trim_end_matches("` does not declare");
                e.message = format!(
                    "{}, and nothing in `{task}` answers `{effect}`, which no system provides",
                    e.message
                );
            }
            e
        })
        .collect()
}
