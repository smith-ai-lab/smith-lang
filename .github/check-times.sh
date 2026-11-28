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
# never slow. Each median is first scaled by how much slower than usual the
# whole first run is, so a slow machine does not make every test slow. With no second file it prints the names of the slow tests, one on
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
SPEED_FLOOR=20

# With "speed", prints how much slower than usual the whole run $2 is: the
# median, over its tests with a median of at least $SPEED_FLOOR ms, of each
# test's time over its median, and never below 1. A machine slower than usual
# makes every test slower, so the medians are scaled by it. With a number,
# prints `test,milliseconds,median` for each test of $2 that is slow against
# its median scaled by that number.
# Usage: measure <history> <times> speed | <speed>
measure() {
  awk -F, -v last="$LAST" -v fewest="$FEWEST" -v floor="$FLOOR" -v speed_floor="$SPEED_FLOOR" -v given="$3" '
    function median_of(a, n,    i, j, x) {
      for (i = 2; i <= n; i++) {
        x = a[i]
        for (j = i - 1; j >= 1 && a[j] > x; j--) a[j + 1] = a[j]
        a[j + 1] = x
      }
      return (n % 2) ? a[(n + 1) / 2] : (a[n / 2] + a[n / 2 + 1]) / 2
    }
    NR == FNR {
      if (FNR > 1) { count[$3]++; time[$3, count[$3]] = $4 }
      next
    }
    {
      n = count[$1]
      if (n > last) n = last
      if (n < fewest) next
      for (i = 1; i <= n; i++) v[i] = time[$1, count[$1] - n + i]
      m = median_of(v, n)
      tests++
      name[tests] = $1; took[tests] = $2; med[tests] = m
      if (m >= speed_floor) { ratios++; ratio[ratios] = $2 / m }
    }
    END {
      if (given == "speed") {
        speed = ratios ? median_of(ratio, ratios) : 1
        printf "%g\n", (speed < 1) ? 1 : speed
        exit
      }
      speed = given
      for (k = 1; k <= tests; k++) {
        usual = med[k] * speed
        if (took[k] > 2 * usual && took[k] - usual > floor) printf "%s,%d,%g\n", name[k], took[k], med[k]
      }
    }
  ' "$1" "$2"
}

# Prints the slow tests of $2 against the speed $3 of the first, full run.
slow() {
  measure "$1" "$2" "$3" | sort
}

speed=$(measure "$1" "$2" speed)

if [ "$#" -eq 2 ]; then
  slow "$1" "$2" "$speed" | cut -d, -f1
  exit 0
fi

# The second run's slow tests go through a file, as an `awk -v` value may not
# hold a line break on every awk.
again=$(mktemp)
trap 'rm -f "$again"' EXIT
slow "$1" "$3" "$speed" > "$again"
both=$(slow "$1" "$2" "$speed" | awk -F, '
  NR == FNR { second[$1] = $2; next }
  NF && ($1 in second) { printf "- %s: %d ms and %d ms, where the median of its last times is %g ms\n", $1, $2, second[$1], $3 }
' "$again" -)
note="This run is $speed times as slow as usual, so each median is scaled by $speed first."

if [ -z "$both" ]; then
  echo "No test takes more than twice the median of its last times and $FLOOR ms more in both runs."
  exit 0
fi

echo "These tests take more than twice the median of their last $LAST times on this kind of runner, and $FLOOR ms more, in two runs:"
echo
echo "$note"
echo
echo "$both"
exit 1
