#!/bin/sh
# Reports the times that the `times` workflow recorded.
# Usage: report-times.sh report <folder>
#        report-times.sh over <folder> <runner> <milliseconds>
# <folder> holds a file `<runner>.csv` for each kind of runner, a line
# `commit,date,test,milliseconds` for each test of each merge, oldest first,
# after its header. `report` prints Markdown: for each kind of runner, the 20
# tests with the greatest median over their last 20 times, and the time of the
# whole suite for each week, the mean over the merges of the week of the sum of
# the times of every test. `over` prints the name of each test whose median is
# above the milliseconds on that kind of runner, one on each line.
set -eu

LAST=20
SLOWEST=20

# Prints `test,median` for each test of the history in $1.
medians() {
  awk -F, -v last="$LAST" '
    FNR > 1 { count[$3]++; time[$3, count[$3]] = $4 }
    END {
      for (name in count) {
        n = count[name]
        if (n > last) n = last
        for (i = 1; i <= n; i++) v[i] = time[name, count[name] - n + i]
        for (i = 2; i <= n; i++) {
          x = v[i]
          for (j = i - 1; j >= 1 && v[j] > x; j--) v[j + 1] = v[j]
          v[j + 1] = x
        }
        median = (n % 2) ? v[(n + 1) / 2] : (v[n / 2] + v[n / 2 + 1]) / 2
        printf "%s,%g\n", name, median
      }
    }
  ' "$1"
}

# Prints `week,merges,mean total in milliseconds` for each week of the history in $1.
weeks() {
  awk -F, '
    FNR > 1 { total[$1] += $4; day[$1] = $2 }
    END { for (c in total) print day[c] "," total[c] }
  ' "$1" | sort | while IFS=, read -r day total; do
    echo "$(date -u -d "$day" +%G-W%V),$total"
  done | awk -F, '
    { n[$1]++; sum[$1] += $2 }
    END { for (w in n) printf "%s,%d,%d\n", w, n[w], sum[w] / n[w] }
  ' | sort
}

case "${1:-}" in
  report)
    folder=$2
    found=
    for file in "$folder"/*.csv; do
      [ -f "$file" ] || continue
      found=1
      runner=$(basename "$file" .csv)
      echo "## $runner"
      echo
      echo "The $SLOWEST slowest tests, by the median of their last $LAST times:"
      echo
      echo "| Test | Median (ms) |"
      echo "| --- | ---: |"
      medians "$file" | sort -t, -k2,2nr -k1,1 | head -n "$SLOWEST" | awk -F, '{ printf "| %s | %g |\n", $1, $2 }'
      echo
      echo "The time of the whole suite for each week:"
      echo
      echo "| Week | Merges | Mean total (s) |"
      echo "| --- | ---: | ---: |"
      weeks "$file" | awk -F, '{ printf "| %s | %d | %.1f |\n", $1, $2, $3 / 1000 }'
      echo
    done
    if [ -z "$found" ]; then
      echo "No times are recorded yet."
    fi
    ;;
  over)
    medians "$2/$3.csv" | awk -F, -v limit="$4" '$2 > limit { print $1 }' | sort
    ;;
  *)
    echo "usage: report-times.sh report <folder> | over <folder> <runner> <milliseconds>" >&2
    exit 2
    ;;
esac
