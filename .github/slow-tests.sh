#!/bin/sh
# Runs on a pull request, in `bootstrap/`, after a run wrote the time of each
# test to <times>: compares them with the history of the kind of runner on the
# `times` branch as `check-times.sh` says, runs the slow tests once more, and
# fails when a test is slow in both. The summary is printed as Markdown.
# Usage: slow-tests.sh <runner> <times> <cargo test arguments>...
# The arguments are those of the run that made <times>, so that the tests run
# again are given what they were given the first time, with the names of the
# slow tests added.
set -eu

runner=$1
times=$2
shift 2

here=$(cd "$(dirname "$0")" && pwd)
temp=${RUNNER_TEMP:-${TMPDIR:-/tmp}}

if ! git fetch --quiet --depth=1 origin times 2>/dev/null; then
  echo "No times are recorded yet, so none are compared."
  exit 0
fi
if ! git show "FETCH_HEAD:$runner.csv" > "$temp/history" 2>/dev/null; then
  echo "No times are recorded yet for $runner, so none are compared."
  exit 0
fi

names=$(sh "$here/check-times.sh" "$temp/history" "$times")
if [ -z "$names" ]; then
  echo "No test takes more than twice the median of its last times on $runner and 100 ms more."
  exit 0
fi

echo "Running again, to see whether it was the machine: $names" >&2
rm -f "$temp/again"
# shellcheck disable=SC2086
cargo test --locked -- "$@" --times="$temp/again" $names >&2 || true
if [ ! -f "$temp/again" ]; then
  : > "$temp/again"
fi
sh "$here/check-times.sh" "$temp/history" "$times" "$temp/again"
