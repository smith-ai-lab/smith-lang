#!/bin/sh
# Holds `listen` against `hyper` on `tokio`: requests a second, the memory of a
# connection with 1000 held open, the time to build, and the size of the
# executable. Not built by CI: this crate has its own Cargo.lock. Linux only.
# Usage: sh measure/hyper/run.sh
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root="$here/../.."
work=$(mktemp -d)
trap 'kill $(cat "$work"/pid 2>/dev/null) 2>/dev/null || true; rm -rf "$work"' EXIT

cargo build --release --locked --manifest-path "$root/bootstrap/Cargo.toml"
smith="$root/bootstrap/target/release/smith"

# The program of the page's deployment, as bootstrap/tests/http_listen.rs builds it.
mkdir "$work/project"
sed -e 's/use std.http (get, http, listen, post, routes)/use std.http (get, http, post, routes)/' \
    -e '/^use std.net (net)$/d' \
    -e '/^use std.deserialize (ReadError)$/d' -e '/^use std.fromtext (from_text)$/d' \
    -e 's/^fn app()/pub fn main()/' \
    "$root/bootstrap/tests/http_listen.smith" | sed '/^\/\/ What `serve`/,$d' > "$work/project/main.smith"
cat > "$work/project/project.smith" <<'SMITH'
use main (main)
use std.http (listen)
use std.net (net)
use std.project (Package)

package = Package(name: "users")

pub fn serve(; port: 1..65536 = 8080) <net, alloc, diverge> {
  with listen(port: port)
  main()
}
SMITH
start=$(date +%s.%N)
(cd "$work/project" && "$smith" build serve >/dev/null)
end=$(date +%s.%N)
echo "smith build: $(echo "$end - $start" | bc) s, executable $(stat -c %s "$work/project/users") bytes"

cargo clean --manifest-path "$here/Cargo.toml" 2>/dev/null
start=$(date +%s.%N)
cargo build --release --locked --manifest-path "$here/Cargo.toml" 2>&1 | tail -1
end=$(date +%s.%N)
echo "hyper build (with its dependencies): $(echo "$end - $start" | bc) s, executable $(stat -c %s "$here/target/release/server") bytes"

measure() {
  "$@" &
  echo $! > "$work/pid"
  sleep 1
  cargo run --release --quiet --locked --manifest-path "$here/Cargo.toml" --bin load -- "$PORT" "$(cat "$work/pid")"
  kill "$(cat "$work/pid")"; wait "$(cat "$work/pid")" 2>/dev/null || true
}
echo "== listen"; PORT=18081 measure "$work/project/users" --port 18081
echo "== hyper on tokio"; PORT=18082 measure "$here/target/release/server" 18082
