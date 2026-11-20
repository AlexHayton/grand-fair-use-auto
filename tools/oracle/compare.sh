#!/bin/sh
# Walk a script through the AmiGTA1 oracle and through gfa-tool, and diff
# the per-tick player traces. Exit 0 when every tick matches.
#
#     tools/oracle/compare.sh tools/scripts/walk_ramp.txt [map]
#
# The map is a path, or @name to find GTADATA/name in the data folder
# (GFA_DATA, or data = "..." in gfa.local.toml); it defaults to @nyc.cmp. The script's `# section:` picks the level
# for gfa-tool, so the map given here must be that level's.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
script=$1
[ -x "$here/build/walktrace" ] || "$here/build.sh" >/dev/null
data=${GFA_DATA:-}
if [ -z "$data" ] && [ -f "$root/gfa.local.toml" ]; then
    data=$(sed -n 's/^data *= *"\(.*\)".*/\1/p' "$root/gfa.local.toml" | head -1)
fi
want=${2:-@nyc.cmp}
case $want in
    @*) map=$(find "$data" -ipath "*/gtadata/${want#@}" | head -1) ;;
    *) map=$want ;;
esac
out=$(mktemp -d)
trap 'rm -rf "$out"' EXIT
"$here/build/walktrace" "$map" "$script" | grep '^t ' >"$out/oracle"
(cd "$root" && cargo run -q --release -p gfa-tool -- run "$script" --trace --amigta1-view) \
    | grep '^t ' >"$out/gfa"
ticks=$(wc -l <"$out/oracle" | tr -d ' ')
if diff -u "$out/oracle" "$out/gfa" >"$out/diff"; then
    echo "match: $ticks ticks ($(basename "$script"))"
else
    head -20 "$out/diff"
    echo "MISMATCH ($(basename "$script"))"
    exit 1
fi
