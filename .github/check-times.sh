#!/bin/sh
# Says which tests of a change became much slower than their own history on
# this kind of runner, and fails when a test is so in two runs.
# Usage: check-times.sh <history> <times> [<second times>]
# <history> is the file of one kind of runner on the `times` branch, a line
# `commit,date,test,milliseconds` for each test of each merge, oldest first,
# after its header. <times> is what the harness writes with `--times=<file>`,
# a line `test,milliseconds` for each test that passed. A test is slow when its
# time is more than twice its median over its last 20 recorded times, and more
# than it by more than 100 ms, and a test with fewer than 5 recorded times is
# never slow. With no second file it prints the names of the slow tests, one on
# each line, which the caller runs again. With the file of that second run it
# prints, as Markdown, the tests that are slow in both, with both times and the
# median, and fails when there are any. A test that the second run did not run
# or that passed it is not slow.
set -eu

if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: check-times.sh <history> <times> [<second times>]" >&2
  exit 2
fi

LAST=20
FEWEST=5
FLOOR=100

# Prints `test,milliseconds,median` for each test of $2 that is slow.
slow() {
  awk -F, -v last="$LAST" -v fewest="$FEWEST" -v floor="$FLOOR" '
    NR == FNR {
      if (FNR > 1) { count[$3]++; time[$3, count[$3]] = $4 }
      next
    }
    {
      n = count[$1]
      if (n > last) n = last
      if (n < fewest) next
      for (i = 1; i <= n; i++) v[i] = time[$1, count[$1] - n + i]
      for (i = 2; i <= n; i++) {
        x = v[i]
        for (j = i - 1; j >= 1 && v[j] > x; j--) v[j + 1] = v[j]
        v[j + 1] = x
      }
      median = (n % 2) ? v[(n + 1) / 2] : (v[n / 2] + v[n / 2 + 1]) / 2
      if ($2 > 2 * median && $2 - median > floor) printf "%s,%d,%g\n", $1, $2, median
    }
  ' "$1" "$2" | sort
}

if [ "$#" -eq 2 ]; then
  slow "$1" "$2" | cut -d, -f1
  exit 0
fi

# The second run's slow tests go through a file, as an `awk -v` value may not
# hold a line break on every awk.
again=$(mktemp)
trap 'rm -f "$again"' EXIT
slow "$1" "$3" > "$again"
both=$(slow "$1" "$2" | awk -F, '
  NR == FNR { second[$1] = $2; next }
  NF && ($1 in second) { printf "- %s: %d ms and %d ms, where the median of its last times is %g ms\n", $1, $2, second[$1], $3 }
' "$again" -)

if [ -z "$both" ]; then
  echo "No test takes more than twice the median of its last times and $FLOOR ms more in both runs."
  exit 0
fi

echo "These tests take more than twice the median of their last $LAST times on this kind of runner, and $FLOOR ms more, in two runs:"
echo
echo "$both"
exit 1
