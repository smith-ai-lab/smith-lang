#!/bin/sh
# Says which tests of a change take many more steps than they do on `main`, and
# fails when there are any and the description of the change does not say why.
# Usage: check-work.sh <main steps> <change steps> <description>
# The first two are what the harness writes with `--steps=<file>`, a line
# `name,steps` for each test that passed, and the third is the file holding the
# description of the pull request. A test does much more work when its steps
# are more than 1.5 times what they are on main, and more than them by more than
# 100000 steps, so that a small test that grows by a few steps fails nothing. A
# test that only the change has is new, and is not compared. The description
# says why with a line that starts `Does more work.` and goes on with the reason.
# The summary is printed as Markdown, to be added to the summary of the run.
set -eu

if [ "$#" -ne 3 ]; then
  echo "usage: check-work.sh <main steps> <change steps> <description>" >&2
  exit 2
fi

FLOOR=100000

more=$(awk -F, -v floor="$FLOOR" '
  NR == FNR { was[$1] = $2; next }
  ($1 in was) && 2 * $2 > 3 * was[$1] && $2 - was[$1] > floor {
    print "- " $1 ": " $2 " steps, where main takes " was[$1]
  }
' "$1" "$2" | sort)

if [ -z "$more" ]; then
  echo "No test takes more than 1.5 times its steps on main and $FLOOR more."
  exit 0
fi

echo "These tests take more than 1.5 times their steps on main and $FLOOR more:"
echo
echo "$more"
echo

if tr -d '\r' < "$3" | grep -Eq '^Does more work\. +[^ ]'; then
  echo "The description says why the change does more work."
else
  echo "The description does not say why. Add a line that starts \`Does more work.\` and gives the reason, if the work is meant, or make the test do less if it is not."
  exit 1
fi
