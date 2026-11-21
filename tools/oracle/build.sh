#!/bin/sh
# Build the AmiGTA1 oracle harnesses (walktrace, drivetrace, traintrace,
# traffictrace) into
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
# The walk and drive harnesses run the fleet and the trains too (fleet.h).
# AmiGTA1's drive_one reads two locals before it sets them (gta_traffic.c,
# the crossing check); zero them so every run agrees.
street="$src/gta_traffic.c $src/gta_route.c $src/gta_car.c $src/gta_train.c"
$cc -std=c89 -O1 -w -ftrivial-auto-var-init=zero -I"$src" -I"$here" \
    -o "$here/build/walktrace" "$here/walktrace.c" "$src/gta_map.c" \
    "$src/gta_nav.c" "$src/gta_player.c" "$src/gta_trig.c" \
    "$src/gta_style.c" "$src/gta_vehphys.c" $street
$cc -std=c89 -O1 -w -ftrivial-auto-var-init=zero -I"$src" -I"$here" \
    -o "$here/build/drivetrace" "$here/drivetrace.c" "$src/gta_map.c" \
    "$src/gta_nav.c" "$src/gta_vehphys.c" "$src/gta_style.c" \
    "$src/gta_trig.c" $street
$cc -std=c89 -O1 -w -I"$src" -o "$here/build/traintrace" \
    "$here/traintrace.c" "$src/gta_map.c" "$src/gta_train.c" "$src/gta_trig.c"
# AmiGTA1's drive_one reads two locals before it sets them
# (gta_traffic.c, the crossing check); zero them so every run agrees.
$cc -std=c89 -O1 -w -ftrivial-auto-var-init=zero -I"$src" \
    -o "$here/build/traffictrace" "$here/traffictrace.c" \
    "$src/gta_traffic.c" "$src/gta_route.c" "$src/gta_nav.c" "$src/gta_map.c" \
    "$src/gta_car.c" "$src/gta_style.c" "$src/gta_trig.c" "$src/gta_vehphys.c"
echo "built $here/build/walktrace, drivetrace, traintrace and traffictrace from $ami ($(git -C "$ami" rev-parse --short HEAD 2>/dev/null || echo 'not a git checkout'))"
