#!/usr/bin/env bash
# Build the tank-cannon test overlays (docs/M6B_TANK_TEST_PLAN.md).
#
# Each is Vice City [202] staged in the level-1 slot by level_overlay.py, with
# its kill-frenzy tank (decl 3000, CARTRIGGER 7100 -> proc 600) moved to a test
# spot. Getting in starts the frenzy, and only then does the cannon fire. For
# all of them the frenzy is made to last: FRENZY_CHECK 999999 (it cannot be
# won, which would end it), KF_BRIEF_TIMED 999 s and SURVIVE 30000 ticks
# (20 minutes, instead of 100 s). All but tank_turret also leave the frenzy's
# eight soldiers (proc 600, 603-610) as ordinary peds.
#
# Positions are blocks on the six-lane road west of the army base (lanes
# x=217-222) and on the bridge south of it, except A's. A declaration's z counts layers
# down from the top of the six-layer map: z = 5 - the map layer of the lid the
# thing stands on (Vice City's own telephone at (159,36,3) stands on a lid at
# map layer 2; `gfa-tool map GTADATA/MIAMI.CMP --at X,Y` shows the layers,
# with the ground type in the air block above the lid). Along the road:
#
#   y  60-65   cross street
#   y  66-90   flat road, lid at layer 1: z=4
#   y  91      a footbridge over the road (a walkway slab, railings on top)
#   y  92-108  flat road, lid at layer 1: z=4     (B, D)
#   y 109-116  ramp up to the bridge (slopes 17-24)
#   y 117-135  bridge deck, lid at layer 2: z=3   (C), over water
#
# The damage test keeps off the ramp and the footbridge, so the shell flies
# level over flat road between the tank and its targets. Lanes x=217-219 run
# south (towards the tests' tanks), x=220-222 north: a target parked in a
# northbound lane gathers a queue of traffic between it and the tank, so
# targets sit in southbound lanes or on the verge.
#
#     tools/dosprobe/tank_scenarios.sh            # all, into build/overlays/
#
# Grand Fair Use Auto, MIT.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
out="$here/build/overlays"
lo() { python3 "$here/level_overlay.py" --section 202 "$@"; }

# The frenzy, made to last.
LONG=(
  --cmd "645=645 FRENZY_CHECK 999999 654 645 0 0"
  --cmd "646=646 KF_BRIEF_TIMED 0 0 0 999 3502"
  --cmd "647=647 SURVIVE 0 0 0 30000 0"
)
CALM=(--donowt 603-610)

# Vehicle models must be in Vice City's style (STYLE003.G24), or GTA8.EXE
# stops with "create car failed on type N": Liberty City's Bulldog (31) is
# not. CAR is the Itali GTB, a plain car of the Bulldog's mass that Vice City
# parks itself; 9 is its Coach and 44 its Tanker.
CAR=82
COACH=9
TANKER=44

# Declaration angles: 0 faces south, 512 north (an angle of 0 left every
# tank facing south, the wrong way, until 2026-11-26).
NORTH=512

# The tank at block (x,y), facing north, at declaration z.
tank_at() { # x y z
  local px=$(($1 * 64 + 32)) py=$(($2 * 64 + 32)) pz=$(($3 * 64))
  echo "3000=3000 1 ($px,$py,$pz) PARKED_PIXELS 37 $NORTH"
}

# A: range. The longest straight, flat, clear road in Vice City: four lanes,
# x=96-99, y=22-102, lid at layer 1 (z=4), nothing above but signs at layers
# 3-4; x=96-97 run south, 98-99 north. Tank at (97,96) facing north up
# southbound lane 97 (traffic from the north queues beyond the target, and
# cars turning in between stop at the tank, in view); a target car in the
# same lane at (97,28), 68 blocks ahead; marker cars on the west verge (x=95)
# every 5 blocks, 65 to 10 blocks from the target, except 50, where the
# crossroads at y=75-78 stands in for it, then one at 6 (y=33 is a side
# road). The close ones are for the score: a wreck 65 blocks off paid
# nothing, one a few blocks off paid 100. Level with a marker at y, the tank
# is (y - 28) blocks from the target.
markers=()
id=28201
for y in 93 88 83 73 68 63 58 53 48 43 38 34; do
  markers+=(--add-decl "$id 1 (95,$y,4) PARKED $CAR $NORTH")
  id=$((id + 1))
done
lo --out "$out/tank_range" --player 96,96,4 --decl "$(tank_at 97 96 4)" \
  "${LONG[@]}" "${CALM[@]}" \
  --add-decl "28200 1 (97,28,4) PARKED $CAR $NORTH" "${markers[@]}"
# Cars spawned and turned in between the tank and the target, so the four
# lanes from just behind the tank to just beyond the target are field in a
# patched copy of the map: no traffic spawns on or drives into it, and it
# still looks like road. The cross streets at y=22-25 and 99-102 stay road.
python3 "$here/cmp_patch.py" --map MIAMI.CMP --field 96-99,26-98 --out "$out/tank_range"

# A6: why a wreck far off pays nothing. The range test's road and tank, and
# a chain of 11 cars nose to tail in lane 97, 4 to 14 blocks ahead (y=92 down
# to 82). One shell at the nearest sets off the chain, each wreck's blast
# writing off the next, 1 block on. If the wreck's distance from the player
# decides the score, it stops rising part way along; if the shell's flight
# (its age) does, every car pays, since only the first met the shell.
chain=()
id=28200
for y in 92 91 90 89 88 87 86 85 84 83 82; do
  chain+=(--add-decl "$id 1 (97,$y,4) PARKED $CAR $NORTH")
  id=$((id + 1))
done
lo --out "$out/tank_chain" --player 96,96,4 --decl "$(tank_at 97 96 4)" \
  "${LONG[@]}" "${CALM[@]}" "${chain[@]}"
python3 "$here/cmp_patch.py" --map MIAMI.CMP --field 96-99,26-98 --out "$out/tank_chain"

# A7: the chain, timed frame by frame on screen. The same road and tank, and
# a row of 6 cars across the screen, side by side 1 block apart (x=95-100),
# 1.6 blocks ahead of the tank (y=6074 px; the screen shows about 2.2 blocks
# up from the tank at rest). The shell hits the one at x=97, and the chain
# runs west and east along the row in view.
row=()
id=28200
for x in 95 96 97 98 99 100; do
  row+=(--add-decl "$id 1 ($((x * 64 + 32)),6074,256) PARKED_PIXELS $CAR $NORTH")
  id=$((id + 1))
done
lo --out "$out/tank_chain_view" --player 96,96,4 --decl "$(tank_at 97 96 4)" \
  "${LONG[@]}" "${CALM[@]}" "${row[@]}"
python3 "$here/cmp_patch.py" --map MIAMI.CMP --field 96-99,26-98 --out "$out/tank_chain_view"

# B: what makes it burst. Tank at (221,103) facing north. Ahead: a car at
# (221,99). Behind: a ped held still at (221,106). East (Tab+Right): the solid
# west wall of the five-storey block at x=224, 2.5 blocks away. A start
# trigger at the player's block puts the ped down.
lo --out "$out/tank_burst" --player 220,103,4 --decl "$(tank_at 221 103 4)" \
  "${LONG[@]}" "${CALM[@]}" \
  --add-decl "28200 1 (221,99,4) PARKED $CAR $NORTH" \
  --add-decl "28210 1 (14176,6816,256) FUTUREPED 0 0" \
  --add-decl "28290 (220,103,4) TRIGGER 28300 0" \
  --add-cmd "28300 DISABLE 28290 0 0 0 0" \
  --add-cmd "28301 PED_ON 28210 0 -1 136 0" \
  --add-cmd "28302 CHANGE_PED_TYPE 28210 0 -1 4 0" \
  --add-cmd "28303 SET_PED_SPEED 28210 0 0 0 0" \
  --add-cmd "28304 DONOWT 0 0 0 0 0"

# C: railings and water. Tank on the bridge deck (z=3) at (222,122) facing
# north in the east lane. East (Tab+Right): the bridge's railing at x=224, 1.5
# blocks, then open water. West (Tab+Left): across the lanes to the west
# railing at x=216, 5.5 blocks.
lo --out "$out/tank_fence" --player 221,122,3 --decl "$(tank_at 222 122 3)" \
  "${LONG[@]}" "${CALM[@]}"

# D: damage. Tank at (219,107) facing north. Straight ahead: the Coach at
# (219,100). 22 degrees left: a car at (217,102), with neighbours on the verge
# 1 block (216,102) and 2 blocks (215,102) west of it, to tell a 1-block blast
# reach (gfa-sim's) from a 2-block one (GTA8.EXE's code). 30 degrees right,
# on the east verge: the Tanker at (223,100). All parked facing north.
lo --out "$out/tank_damage" --player 218,107,4 --decl "$(tank_at 219 107 4)" \
  "${LONG[@]}" "${CALM[@]}" \
  --add-decl "28200 1 (219,100,4) PARKED $COACH $NORTH" \
  --add-decl "28201 1 (217,102,4) PARKED $CAR $NORTH" \
  --add-decl "28202 1 (216,102,4) PARKED $CAR $NORTH" \
  --add-decl "28204 1 (215,102,4) PARKED $CAR $NORTH" \
  --add-decl "28203 1 (223,100,4) PARKED $TANKER $NORTH"

# E: the turret aiming itself. The tank where the level puts it, in the army
# base, with ONE of the frenzy's eight soldiers left hostile (603: ped 5100),
# so the tank lasts long enough to watch.
lo --out "$out/tank_turret" --player 248,132,4 "${LONG[@]}" --donowt 604-610

echo "tank_scenarios: built tank_range, tank_chain, tank_chain_view, tank_burst, tank_fence, tank_damage, tank_turret in $out"
