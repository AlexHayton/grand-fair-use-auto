#!/bin/sh
# Build the AmiGTA1 oracle harnesses (walktrace, drivetrace, traintrace,
# traffictrace) into
# tools/oracle/build/.
#
# AmiGTA1's sources are compiled where they are, in a sibling checkout
# (../AmiGTA1 from the repo root, or $AMIGTA1). Nothing of AmiGTA1 is copied
# into this repo; only our own harness lives here, and the patches it
# applies to copies under build/ (ped_sidestep.pl, traffic_giveup.pl,
# traffic_lights.pl, traffic_corner.pl, traffic_rejoin.pl, traffic_fits.pl,
# traffic_follow.pl, traffic_spawn.pl, traffic_stopline.pl).
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
# The walk and drive harnesses run the fleet, the trains and the crowd too
# (fleet.h).
# AmiGTA1's drive_one reads two locals before it sets them (gta_traffic.c,
# the crossing check); zero them so every run agrees.
# The crowd is AmiGTA1's with gfa-sim's sidestep fix (ped_sidestep.pl), and
# the fleet AmiGTA1's with gfa-sim's dead-end rule (traffic_giveup.pl), its
# lights timed by the crossing (traffic_lights.pl), its corners
# (traffic_corner.pl), a shoved car's lane (traffic_rejoin.pl), long
# vehicles at corners (traffic_fits.pl), following (traffic_follow.pl),
# spawning (traffic_spawn.pl) and stopping for a light (traffic_stopline.pl),
# applied to copies in build/: the game's rules, not AmiGTA1's spin, a bus
# vanishing in view, both roads of a crossing green at once, a car driving
# off the road past a corner, one stranded a pixel over a lane line, a bus
# stuck at a corner, a ring of queues round a one-way loop, a car put down
# inside another or a bus waiting with its nose in the crossing.
patched=$here/build/patched
mkdir -p "$patched"
perl "$here/ped_sidestep.pl" < "$src/gta_peds.c" > "$patched/gta_peds.c"
perl "$here/traffic_giveup.pl" < "$src/gta_traffic.c" |
    perl "$here/traffic_lights.pl" |
    perl "$here/traffic_corner.pl" |
    perl "$here/traffic_rejoin.pl" |
    perl "$here/traffic_fits.pl" |
    perl "$here/traffic_follow.pl" |
    perl "$here/traffic_spawn.pl" |
    perl "$here/traffic_stopline.pl" > "$patched/gta_traffic.c"
street="$patched/gta_traffic.c $src/gta_route.c $src/gta_car.c $src/gta_train.c $patched/gta_peds.c"
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
    "$patched/gta_traffic.c" "$src/gta_route.c" "$src/gta_nav.c" "$src/gta_map.c" \
    "$src/gta_car.c" "$src/gta_style.c" "$src/gta_trig.c" "$src/gta_vehphys.c"
echo "built $here/build/walktrace, drivetrace, traintrace and traffictrace from $ami ($(git -C "$ami" rev-parse --short HEAD 2>/dev/null || echo 'not a git checkout'))"
