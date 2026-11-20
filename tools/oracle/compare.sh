#!/bin/sh
# Play a script through the AmiGTA1 oracle and through gfa-tool, and diff
# the per-tick traces: the player on foot (walktrace) or, for a script with
# a `drive` line, his car (drivetrace). Exit 0 when every tick matches.
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
[ -x "$here/build/drivetrace" ] || "$here/build.sh" >/dev/null
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
if grep -q '^drive ' "$script"; then
    # Driving: the GRY style beside the map, which AmiGTA1 reads (the G24
    # gfa-tool reads has the same car table; see the data tests).
    case $(basename "$map" | tr '[:upper:]' '[:lower:]') in
        sanb.cmp) n=2 ;;
        miami.cmp) n=3 ;;
        *) n=1 ;;
    esac
    gry=$(find "$(dirname "$map")" -maxdepth 1 -iname "style00$n.gry" | head -1)
    "$here/build/drivetrace" "$map" "$gry" "$script" | grep '^c ' >"$out/oracle"
    pat='^c '
else
    "$here/build/walktrace" "$map" "$script" | grep '^t ' >"$out/oracle"
    pat='^t '
fi
(cd "$root" && cargo run -q --release -p gfa-tool -- run "$script" --trace --oracle) \
    | grep "$pat" >"$out/gfa"
ticks=$(wc -l <"$out/oracle" | tr -d ' ')
if diff -u "$out/oracle" "$out/gfa" >"$out/diff"; then
    echo "match: $ticks ticks ($(basename "$script"))"
else
    head -20 "$out/diff"
    echo "MISMATCH ($(basename "$script"))"
    exit 1
fi
