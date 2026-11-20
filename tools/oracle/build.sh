#!/bin/sh
# Build the AmiGTA1 oracle harnesses (walktrace, drivetrace) into
# tools/oracle/build/.
#
# AmiGTA1's sources are compiled where they are, in a sibling checkout
# (../AmiGTA1 from the repo root, or $AMIGTA1). Nothing of AmiGTA1 is copied
# into this repo; only our own harness lives here.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
ami=${AMIGTA1:-$root/../AmiGTA1}
src=$ami/native
if [ ! -f "$src/gta_player.c" ]; then
    echo "build.sh: no AmiGTA1 checkout at $ami (set AMIGTA1)" >&2
    exit 1
fi
mkdir -p "$here/build"
cc=${CC:-clang}
# C89 as AmiGTA1 is written; its host build relies on 64-bit long only where
# it masks the result, and the walk code never overflows 32 bits.
$cc -std=c89 -O1 -w -I"$src" -o "$here/build/walktrace" \
    "$here/walktrace.c" "$src/gta_map.c" "$src/gta_nav.c" \
    "$src/gta_player.c" "$src/gta_trig.c"
$cc -std=c89 -O1 -w -I"$src" -o "$here/build/drivetrace" \
    "$here/drivetrace.c" "$src/gta_map.c" "$src/gta_nav.c" \
    "$src/gta_vehphys.c" "$src/gta_style.c" "$src/gta_trig.c"
echo "built $here/build/walktrace and drivetrace from $ami ($(git -C "$ami" rev-parse --short HEAD 2>/dev/null || echo 'not a git checkout'))"
