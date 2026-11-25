//! `effects NAME = <e1, e2>` names a set of effects once, and the name stands
//! wherever a list of effects goes. A program written with groups builds the
//! bytes of the same program with the groups written out, a group that names
//! itself is refused, and the warning and the fix say the group's name.

use smith::{Project, compile, compile_project_timed};

use crate::Test;
use crate::generate::{self, Gen};
use crate::programs::with_imports;
use crate::targets::any;

pub const ONCE: &[Test] = &[
    (
        "a_program_written_with_groups_builds_the_bytes_of_the_same_program_written_out",
        a_program_written_with_groups_builds_the_bytes_of_the_same_program_written_out,
    ),
    (
        "a_group_that_names_itself_is_refused_at_its_declaration_with_the_chain",
        a_group_that_names_itself_is_refused_at_its_declaration_with_the_chain,
    ),
    (
        "naming_a_group_or_an_effect_twice_directly_is_refused_and_overlap_is_not",
        naming_a_group_or_an_effect_twice_directly_is_refused_and_overlap_is_not,
    ),
    (
        "a_group_in_a_signature_is_warned_of_only_when_nothing_in_it_is_used",
        a_group_in_a_signature_is_warned_of_only_when_nothing_in_it_is_used,
    ),
    (
        "the_fix_writes_the_name_of_a_group_that_holds_exactly_what_is_missing",
        the_fix_writes_the_name_of_a_group_that_holds_exactly_what_is_missing,
    ),
    (
        "every_fix_for_a_missing_effect_in_one_function_writes_the_whole_list_it_needs",
        every_fix_for_a_missing_effect_in_one_function_writes_the_whole_list_it_needs,
    ),
    (
        "a_group_a_module_makes_pub_is_imported_by_any_name_and_stands_as_its_effects",
        a_group_a_module_makes_pub_is_imported_by_any_name_and_stands_as_its_effects,
    ),
];

const EFFECTS: [&str; 2] = ["console", "alloc"];

/// The groups of a program and what each names: a group names effects and
/// the groups before it, so that no group names itself.
#[derive(Debug)]
struct Case {
    /// For each group, the names it lists.
    groups: Vec<Vec<String>>,
    /// The names a function lists, which are groups and effects.
    listed: Vec<String>,
}

fn case(g: &mut Gen) -> Case {
    let count = 1 + g.below(3);
    let mut groups: Vec<Vec<String>> = Vec::new();
    for i in 0..count {
        let mut names: Vec<String> = Vec::new();
        for _ in 0..1 + g.below(3) {
            let name = match g.below(3) {
                0 if i > 0 => format!("g{}", g.below(i)),
                n => EFFECTS[n % 2].to_string(),
            };
            if !names.contains(&name) {
                names.push(name);
            }
        }
        groups.push(names);
    }
    let mut listed: Vec<String> = Vec::new();
    for _ in 0..1 + g.below(3) {
        let name = if g.below(2) == 0 {
            format!("g{}", g.below(count))
        } else {
            EFFECTS[g.below(2)].to_string()
        };
        if !listed.contains(&name) {
            listed.push(name);
        }
    }
    Case { groups, listed }
}

impl Case {
    /// The effects a name stands for.
    fn held(&self, name: &str, out: &mut Vec<String>) {
        match name.strip_prefix('g') {
            Some(i) => {
                for n in &self.groups[i.parse::<usize>().expect("a group")] {
                    self.held(n, out);
                }
            }
            None if !out.iter().any(|o| o == name) => out.push(name.to_string()),
            None => {}
        }
    }

    /// The program, where every list is written with groups or, when `out`,
    /// with the effects they stand for in the order of `EFFECTS`.
    fn source(&self, out: bool) -> String {
        let mut held = Vec::new();
        for n in &self.listed {
            self.held(n, &mut held);
        }
        let list = |names: Vec<String>| names.join(", ");
        let written = if out {
            list(EFFECTS.iter().filter(|e| held.iter().any(|h| h == *e)).map(|e| e.to_string()).collect())
        } else {
            list(self.listed.clone())
        };
        let mut text = String::from("use std.console (console, print)\n\n");
        if !out {
            for (i, names) in self.groups.iter().enumerate() {
                text += &format!("effects g{i} = <{}>\n", list(names.clone()));
            }
            text += "\n";
        }
        // The function does what every effect it lists is for, and the caller
        // lists the same as the function does.
        text += &format!("fn work() <{written}> {{\n");
        if held.iter().any(|h| h == "console") {
            text += "  print(\"hello\")\n";
        }
        if held.iter().any(|h| h == "alloc") {
            text += "  xs: List[Int] = [1, 2]\n";
        }
        text += &format!("}}\n\nfn main() <{written}> {{\n  work()\n}}\n");
        with_imports(&text)
    }
}

fn a_program_written_with_groups_builds_the_bytes_of_the_same_program_written_out() {
    generate::check(77, 12, case, |case| {
        let with = compile(case.source(false).as_bytes(), any())
            .unwrap_or_else(|e| panic!("{e:?}\n{}", case.source(false)));
        let out = compile(case.source(true).as_bytes(), any())
            .unwrap_or_else(|e| panic!("{e:?}\n{}", case.source(true)));
        assert!(with == out, "the groups change the bytes:\n{}", case.source(false));
    });
}

fn refusals(source: &str) -> Vec<String> {
    match smith::check(with_imports(source).as_bytes()) {
        Ok(warnings) => warnings.into_iter().map(|w| w.message).collect(),
        Err(errors) => errors.into_iter().map(|e| e.message).collect(),
    }
}

fn a_group_that_names_itself_is_refused_at_its_declaration_with_the_chain() {
    generate::check(78, 6, |g| 2 + g.below(3), |&length| {
        let mut text = String::from("use std.console (console, print)\n\n");
        for i in 0..length {
            text += &format!("effects g{i} = <g{}, console>\n", (i + 1) % length);
        }
        text += "\nfn main() <g0> {\n  print(\"x\")\n}\n";
        let found = refusals(&text);
        let chain: Vec<String> = (0..=length).map(|i| format!("g{}", i % length)).collect();
        assert!(
            found.iter().any(|m| m.contains(&chain.join(" -> "))),
            "{found:?}"
        );
    });
}

fn naming_a_group_or_an_effect_twice_directly_is_refused_and_overlap_is_not() {
    let twice = refusals(
        "use std.console (console, print)\neffects both = <console, alloc>\nfn main() <both, both> {\n  print(\"x\")\n}\n",
    );
    assert!(twice.iter().any(|m| m.contains("`both` effect is already listed")), "{twice:?}");
    let inside = refusals(
        "use std.console (console, print)\neffects both = <console, console>\nfn main() <both> {\n  print(\"x\")\n}\n",
    );
    assert!(inside.iter().any(|m| m.contains("`console` effect is already listed")), "{inside:?}");
    let overlap = refusals(
        "use std.console (console, print)\neffects a = <console, alloc>\neffects b = <console>\neffects c = <a, b, console>\nfn main() <a, b, c, alloc> {\n  print(\"x{1}\")\n}\n",
    );
    assert!(overlap.is_empty(), "{overlap:?}");
}

fn a_group_in_a_signature_is_warned_of_only_when_nothing_in_it_is_used() {
    let used = refusals(
        "use std.console (console, print)\neffects both = <console, alloc>\nfn handle() <both> {\n  print(\"x\")\n}\nfn main() <both> {\n  handle()\n}\n",
    );
    assert!(used.is_empty(), "a group that is partly used is not warned of: {used:?}");
    let unused = refusals(
        "use std.console (console, print)\neffects both = <console, alloc>\nfn handle() <both> {\n}\nfn main() <both> {\n  handle()\n  print(\"x\")\n}\n",
    );
    assert!(
        unused.iter().any(|m| m.contains("`handle` lists `both`, and nothing in it needs any of its effects")),
        "{unused:?}"
    );
}

fn the_fix_writes_the_name_of_a_group_that_holds_exactly_what_is_missing() {
    let source = with_imports(
        "use std.console (console, print)\neffects both = <console, alloc>\nfn greet() {\n  print(\"x\")\n  xs: List[Int] = [1]\n}\nfn main() <both> {\n  greet()\n}\n",
    );
    let errors = smith::check(source.as_bytes()).expect_err("greet lists nothing");
    let fixes: Vec<&str> = errors.iter().filter_map(|e| e.fix.as_deref()).collect();
    assert!(fixes.iter().any(|f| f.contains("fn greet() <both>")), "{fixes:?}");
    let part = with_imports(
        "use std.console (console, print)\neffects both = <console, alloc>\nfn greet() {\n  print(\"x\")\n}\nfn main() <both> {\n  greet()\n}\n",
    );
    let errors = smith::check(part.as_bytes()).expect_err("greet lists nothing");
    assert!(
        errors.iter().filter_map(|e| e.fix.as_deref()).all(|f| !f.contains("both")),
        "a group that holds more than is missing is not written: {errors:?}"
    );
}

fn a_group_a_module_makes_pub_is_imported_by_any_name_and_stands_as_its_effects() {
    let module = "use std.console (console)\n\npub effects server = <console, alloc>\npub effects wide = <server>\n";
    let files = |name: &str| (name == "m").then(|| module.to_string());
    let project = Project { files: &files, entry: None };
    let with = "use std.console (console, print)\nuse m (wide, server as srv)\n\nfn two() <wide> {\n  print(\"hi\")\n}\n\nfn main() <srv> {\n  two()\n}\n";
    let out = "use std.console (console, print)\n\nfn two() <console, alloc> {\n  print(\"hi\")\n}\n\nfn main() <console, alloc> {\n  two()\n}\n";
    let built = compile_project_timed(
        with.as_bytes(),
        &project,
        any(),
        &mut smith::timings::Timings::new(),
    )
    .unwrap_or_else(|e| panic!("{e:?}"));
    let plain = compile(out.as_bytes(), any()).expect("the program builds");
    assert!(built == plain, "an imported group builds other bytes than its effects");
}

fn every_fix_for_a_missing_effect_in_one_function_writes_the_whole_list_it_needs() {
    let program = |group: bool| {
        with_imports(&format!(
            "use std.console (console, print)\n{}\nfn shout(n: str) <console, alloc> {{\n  print(\"hi {{n}}\")\n}}\nfn main() {{\n  shout(\"Ana\")\n}}\n",
            if group { "effects talk = <console, alloc>" } else { "" }
        ))
    };
    for (group, want) in [(true, "fn main() <talk>"), (false, "fn main() <console, alloc>")] {
        let errors = smith::check(program(group).as_bytes()).expect_err("main lists nothing");
        let fixes: Vec<&str> = errors.iter().filter_map(|e| e.fix.as_deref()).collect();
        assert_eq!(fixes.len(), 2, "one fix for each effect missing: {fixes:?}");
        for fix in fixes {
            assert!(fix.contains(want), "{fix} is not the whole list `{want}`");
        }
    }
}
