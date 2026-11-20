#!/bin/sh
# Determinism across platforms: run scripts natively (gfa-tool run) and as
# wasm in Node, and diff the run logs and state hashes.
#
#     tools/wasmcheck/compare.sh [script...]
#
# Defaults to every script in tools/scripts/. The data folder comes from
# GFA_DATA or gfa.local.toml. Needs Node and the wasm-bindgen CLI matching
# Cargo.lock (Trunk's cached copy is used if it is there).
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
cd "$root"
[ $# -gt 0 ] || set -- tools/scripts/*.txt
every=25
data=${GFA_DATA:-}
if [ -z "$data" ] && [ -f gfa.local.toml ]; then
    data=$(sed -n 's/^data *= *"\(.*\)".*/\1/p' gfa.local.toml | head -1)
fi
want=$(sed -n '/name = "wasm-bindgen"/{n;s/version = "\(.*\)"/\1/p;}' Cargo.lock | head -1)
bindgen=$HOME/Library/Caches/dev.trunkrs.trunk/wasm-bindgen-$want/wasm-bindgen
[ -x "$bindgen" ] || bindgen=$(command -v wasm-bindgen)
have=$("$bindgen" --version | awk '{print $2}')
if [ "$have" != "$want" ]; then
    echo "compare.sh: wasm-bindgen $have, but Cargo.lock has $want" >&2
    exit 1
fi
out=tools/oracle/build/wasm
cargo build -q --release --target wasm32-unknown-unknown -p gfa-replay-wasm
"$bindgen" --target nodejs --out-dir "$out" \
    target/wasm32-unknown-unknown/release/gfa_replay_wasm.wasm
cargo build -q --release -p gfa-tool

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
node "$here/replay.mjs" "$out" "$data" gta "$every" "$@" >"$tmp/wasm"
for s in "$@"; do
    echo "== $(basename "$s")"
    ./target/release/gfa-tool run "$s" --hash-every "$every" | grep -E '^(\[|hash )' || true
done >"$tmp/native"
lines=$(wc -l <"$tmp/native" | tr -d ' ')
hashes=$(grep -c '^hash ' "$tmp/native" || true)
if diff -u "$tmp/native" "$tmp/wasm" >"$tmp/diff"; then
    echo "native and wasm agree: $# scripts, $hashes hashes, $lines lines"
else
    head -30 "$tmp/diff"
    echo "MISMATCH between native and wasm"
    exit 1
fi
