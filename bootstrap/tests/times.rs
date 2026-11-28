//! The times that tests take on the wall, which `.github/check-times.sh`
//! compares with each test's own history and `.github/report-times.sh` reports,
//! tested here without GitHub.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::Test;

pub const ONCE: &[Test] = &[
    (
        "a_test_near_its_median_passes",
        a_test_near_its_median_passes,
    ),
    (
        "a_slow_test_is_run_again_and_fails_only_when_slow_both_times",
        a_slow_test_is_run_again_and_fails_only_when_slow_both_times,
    ),
    (
        "a_test_over_twice_its_median_by_100_ms_or_less_is_not_slow",
        a_test_over_twice_its_median_by_100_ms_or_less_is_not_slow,
    ),
    (
        "a_test_with_fewer_than_five_times_is_never_slow",
        a_test_with_fewer_than_five_times_is_never_slow,
    ),
    (
        "the_median_is_of_the_last_twenty_times",
        the_median_is_of_the_last_twenty_times,
    ),
    (
        "a_run_slower_than_usual_as_a_whole_makes_no_test_slow_but_one_slow_against_it",
        a_run_slower_than_usual_as_a_whole_makes_no_test_slow_but_one_slow_against_it,
    ),
    (
        "the_report_lists_the_slowest_tests_and_the_suite_for_each_week",
        the_report_lists_the_slowest_tests_and_the_suite_for_each_week,
    ),
];

fn github() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../.github")
}

/// What a script of `.github` prints and whether it passes, given the files
/// that the arguments after its name name, written as `(name, text)` first.
fn run(script: &str, files: &[(&str, String)], arguments: &[&str]) -> (bool, String) {
    let dir = crate::scratch::path("times");
    std::fs::create_dir_all(&dir).unwrap();
    for (name, text) in files {
        std::fs::write(dir.join(name), text).unwrap();
    }
    let mut command = Command::new("sh");
    command.arg(github().join(script));
    for argument in arguments {
        command.arg(if files.iter().any(|(name, _)| name == argument) {
            dir.join(argument).display().to_string()
        } else if *argument == "." {
            dir.display().to_string()
        } else {
            argument.to_string()
        });
    }
    let output = crate::clock::output_within(&mut command, false).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    (
        output.status.success(),
        String::from_utf8(output.stdout).unwrap(),
    )
}

/// A history of `merges` merges in which each test took the times it is given.
fn history(merges: usize, tests: &[(&str, u64)]) -> String {
    let mut text = String::from("commit,date,test,milliseconds\n");
    for merge in 0..merges {
        for (name, time) in tests {
            text += &format!("c{merge},2026-11-01,{name},{time}\n");
        }
    }
    text
}

fn a_test_near_its_median_passes() {
    let past = history(10, &[("a", 1000), ("b", 40)]);
    let now = "a,1900\nb,100\nnew,50000\n".to_string();
    let (passed, said) = run("check-times.sh", &[("h", past), ("t", now.clone())], &["h", "t", "t"]);
    assert!(passed, "{said}");
    assert!(said.starts_with("No test takes"), "{said}");
}

fn a_slow_test_is_run_again_and_fails_only_when_slow_both_times() {
    let past = history(10, &[("a", 1000), ("b", 40), ("c", 400)]);
    let first = "a,2500\nb,300\nc,5000\n".to_string();
    let files = |second: &str| {
        [("h", past.clone()), ("t", first.clone()), ("u", second.to_string())]
    };
    // What is run again is what is slow.
    let (passed, said) = run("check-times.sh", &files(""), &["h", "t"]);
    assert!(passed);
    assert_eq!(said, "a\nb\nc\n");
    // Slow once is not slow.
    let (passed, said) = run("check-times.sh", &files("a,1000\nb,40\nc,400\n"), &["h", "t", "u"]);
    assert!(passed, "{said}");
    // Slow twice is, and the check names the test, both times, and the median.
    let (passed, said) = run("check-times.sh", &files("a,3000\nb,50\nc,6000\n"), &["h", "t", "u"]);
    assert!(!passed, "{said}");
    assert!(said.contains("- a: 2500 ms and 3000 ms, where the median of its last times is 1000 ms"), "{said}");
    assert!(said.contains("- c: 5000 ms and 6000 ms, where the median of its last times is 400 ms"), "{said}");
    assert!(!said.contains("- b:"), "{said}");
}

fn a_test_over_twice_its_median_by_100_ms_or_less_is_not_slow() {
    let past = history(10, &[("a", 40)]);
    for (now, slow) in [("a,140\n", false), ("a,141\n", true), ("a,80\n", false)] {
        let (_, said) = run("check-times.sh", &[("h", past.clone()), ("t", now.into())], &["h", "t"]);
        assert_eq!(!said.is_empty(), slow, "{now}: {said}");
    }
    // Twice the median exactly is not more than twice.
    let past = history(10, &[("a", 1000)]);
    let (_, said) = run("check-times.sh", &[("h", past), ("t", "a,2000\n".into())], &["h", "t"]);
    assert_eq!(said, "");
}

fn a_test_with_fewer_than_five_times_is_never_slow() {
    for merges in 0..5 {
        let past = history(merges, &[("a", 10)]);
        let (_, said) = run("check-times.sh", &[("h", past), ("t", "a,100000\n".into())], &["h", "t"]);
        assert_eq!(said, "", "{merges} times");
    }
    let past = history(5, &[("a", 10)]);
    let (_, said) = run("check-times.sh", &[("h", past), ("t", "a,100000\n".into())], &["h", "t"]);
    assert_eq!(said, "a\n");
}

fn the_median_is_of_the_last_twenty_times() {
    // Thirty merges at 1000 ms and then twenty at 100 ms: the old times are
    // not the history.
    let mut past = history(30, &[("a", 1000)]);
    past += &history(20, &[("a", 100)])["commit,date,test,milliseconds\n".len()..];
    let (_, said) = run("check-times.sh", &[("h", past), ("t", "a,500\n".into())], &["h", "t"]);
    assert_eq!(said, "a\n");
}

fn a_run_slower_than_usual_as_a_whole_makes_no_test_slow_but_one_slow_against_it() {
    // Sixty tests of 100 ms to 690 ms, and a run in which every one takes
    // `factor` times as long, and the test `t7` `extra` times that again.
    let usual: Vec<(String, u64)> = (0..60).map(|k| (format!("t{k}"), 100 + 10 * k)).collect();
    let named: Vec<(&str, u64)> = usual.iter().map(|(n, t)| (n.as_str(), *t)).collect();
    let past = history(10, &named);
    let run_at = |factor: u64, extra: u64| -> String {
        (usual.iter())
            .map(|(n, t)| format!("{n},{}\n", t * factor * if n == "t7" { extra } else { 1 }))
            .collect()
    };
    for factor in [1, 3] {
        let (_, said) = run("check-times.sh", &[("h", past.clone()), ("t", run_at(factor, 1))], &["h", "t"]);
        assert_eq!(said, "", "every test {factor} times as long");
        let (_, said) = run("check-times.sh", &[("h", past.clone()), ("t", run_at(factor, 5))], &["h", "t"]);
        assert_eq!(said, "t7\n", "every test {factor} times as long, and t7 five times that");
        let files = [("h", past.clone()), ("t", run_at(factor, 5)), ("u", format!("t7,{}\n", 170 * factor * 5))];
        let (passed, said) = run("check-times.sh", &files, &["h", "t", "u"]);
        assert!(!passed, "{said}");
        assert!(said.contains(&format!("This run is {factor} times as slow as usual")), "{said}");
        assert!(said.contains("- t7: "), "{said}");
    }
}

fn the_report_lists_the_slowest_tests_and_the_suite_for_each_week() {
    let mut text = String::from("commit,date,test,milliseconds\n");
    // 2026-11-02 is a Monday, so the first two merges are a week and the next is another.
    for (commit, date, a, b) in [("c1", "2026-11-02", 1500, 50), ("c2", "2026-11-08", 1500, 70), ("c3", "2026-11-09", 1500, 90)] {
        text += &format!("{commit},{date},a,{a}\n{commit},{date},b,{b}\n");
    }
    let files = [("ubuntu-latest.csv", text)];
    let (passed, said) = run("report-times.sh", &files, &["report", "."]);
    assert!(passed, "{said}");
    assert!(said.contains("## ubuntu-latest"), "{said}");
    assert!(said.find("| a | 1500 |").unwrap() < said.find("| b | 70 |").unwrap(), "{said}");
    assert!(said.contains("| 2026-W45 | 2 | 1.6 |"), "{said}");
    assert!(said.contains("| 2026-W46 | 1 | 1.6 |"), "{said}");
    let (passed, said) = run("report-times.sh", &files, &["over", ".", "ubuntu-latest", "1000"]);
    assert!(passed);
    assert_eq!(said, "a\n");
}
