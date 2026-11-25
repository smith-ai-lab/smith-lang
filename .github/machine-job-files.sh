#!/bin/sh
# Prints, from the names of the files a change touches, one a line on standard
# input, those that the machine job is made of: its workflow, the workflow that
# calls it, the list of machines, and the harness it runs the tests with. The
# executables the job runs do not say that any of these changed, so a change to
# one is tested on every kind of machine.
# Usage: git diff --name-only <base> <head> | machine-job-files.sh
set -eu

grep -E '^(\.github/workflows/(machine-test|ci)\.yml|\.github/machines|bootstrap/tests/main\.rs|bootstrap/tests/support/)' || true
