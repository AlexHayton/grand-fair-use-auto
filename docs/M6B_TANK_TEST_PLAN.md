# M6b: Tank Cannon Manual Test Plan

This plan covers manual checks in `GTA8.EXE` under DOSBox-X for what the scripted probes could not settle (see the tank entry in `docs/M6B_PROBES.md`). The port needs four answers about the frenzy tank's cannon:

1. How far a shell flies.
2. What it bursts on.
3. What it does to vehicles and walls.
4. Whether the turret aims itself.

## How to read each test

`gfa-sim` already has a rocket, and the scripted probes found the shell flies much like one: it accelerates to about the rocket's speed and makes a similar blast. So each check compares the shell with **the rocket's rules in `gfa-sim`** (taken from AmiGTA1):

| Rocket rule in gfa-sim | Where it is tested |
|---|---|
| No range limit: it flies until it hits something | A |
| Bursts on the first car it touches, and writes that car off at once, whatever the vehicle | B1, D |
| Bursts on a person | B4 |
| Bursts on a building wall, and leaves three fires on that side of the wall | B3 |
| Bursts on a fence or railing (a "flat" face) | C1 |
| Bursts on water | C2 |
| Its blast writes off other cars within 1 block (`GTA8.EXE`'s code suggests 2) | D3, D4 |

For every check, report **"same as a rocket"** or **what happened instead**. A short sentence is fine. If you are not sure, say so, and say what you saw.

## Setup

Each test is Vice City (section [202]) staged in the level-1 slot. Its kill-frenzy tank is moved to a test spot, and you start on foot beside it. Build or rebuild the overlays (rebuild if you built them before the afternoon of 2026-11-26, when the range test moved to a longer road and the test cars changed to ones Vice City has):

```
tools/dosprobe/tank_scenarios.sh
```

Then start a test (replace `<name>` with the test's overlay):

```
python3 tools/dosprobe/run.py --interactive --cycles 150000 \
  --overlay tools/dosprobe/build/overlays/<name>
```

- Press Enter through the menus as usual. On the default name you get no weapons.
- Press Enter beside the tank to get in. "KILL FRENZY!" should appear; the cannon only fires during the frenzy.
- The frenzy has been made to last 20 minutes and cannot be won, so it will not end mid-test.
- The top-left counter is the shells left (99 at the start, 1 per shot). It is a handy way to count shots.
- Controls:
  - **Fire:** Ctrl. Tap it once per shot. Held, it fires about once a second.
  - **Turn the turret:** hold Tab, then press Left or Right. It turns fast (about 150° in 0.6 s), so tap it.
- Vice City's normal traffic and peds are still about, so wait for a clear line before each shot.
- In every test except E, the frenzy's eight soldiers are left as ordinary peds, so nothing attacks you.

The test tanks face north, except E's. A block is about one lane wide. The test cars are Itali GTBs, because Vice City has no Bulldog. Liberty City's Bulldog crashed `GTA8.EXE` with "create car failed on type 31".

At rest, the screen shows only about 2 blocks ahead of the tank. Anything further away is off screen: the bang, the score and driving up to look are how you tell a hit.

---

## A. Range (`tank_range`)

**What we're validating:** whether the shell has a range limit.

**So far:** in an earlier layout, a shot from 18 blocks blew up the target, after passing under a footbridge. So the range is at least 18 blocks, and an overhead walkway does not stop the shell.

- **Rocket:** in gfa-sim it has none.
- **Bullet:** dies after 20 game frames, about 5 blocks, measured in M6b.

**What the port needs:**
- **Range:** "unlimited", or a distance in blocks. We turn the distance into a lifetime from the measured speed (about 6 blocks a second).
- **End of range:** what happens when it runs out. It either bursts in mid-air (an explosion where it stops) or just disappears.

**Layout:**
- **The road:** a long, straight four-lane road in the west of the city, flat for 80 blocks with nothing overhead but signs. It is the longest such road in Vice City.
- **The tank:** in the second lane from the left, facing north.
- **No traffic:** in a patched copy of the map, the four lanes from just behind you to just beyond the target are field. Cars do not spawn there or drive in. It still looks like road. Cars on the cross streets stop at its edges, and people may still wander across.
- **The target:** a car parked in the same lane, **68 blocks** ahead.
- **Marker cars:** parked on the left pavement every 5 blocks. The crossroads stands in for the 50-block marker, and the last marker is at 6 blocks (a side road is at 5).
- With the tank level with a marker, it is this far from the target:

  | Marker (counting from the first one ahead of you) | 1 | 2 | 3 | crossroads | 4 | 5 | 6 | 7 | 8 | 9 | 10 | 11 | 12 |
  |---|---|---|---|---|---|---|---|---|---|---|---|---|---|
  | Blocks to the target | 65 | 60 | 55 | about 50 | 45 | 40 | 35 | 30 | 25 | 20 | 15 | 10 | 6 |

**Steps:**
1. Get in. Without turning the turret, fire one shell straight up the lane from where you start (68 blocks).
   - Listen for a bang.
   - Watch the score. It goes up when the target blows up.
   - If unsure, drive up the lane and look at the target, then come back.
2. If the target survived, drive forward and stop level with marker 1, then fire one shell. Repeat at each marker (and the crossroads) until the target blows up.
3. After any shot that falls short, drive straight up the lane before firing again. Look for fire or a scorch mark on the road where the shell ended.
4. If anyone is in the lane in front of you, wait for them to leave before you fire.

**Report:**
- **A1.** Did the first shot, from 68 blocks, blow the target up? (yes/no)
- **A2.** If not: the first marker from which a shot blew it up, and the last one that fell short. A marker number or the blocks from the table is fine.
- **A3.** When a shot fell short: a bang, an explosion, fire on the road, or nothing at all?
- **A4.** Anything that spoils the count, for example "a car drove into the line on marker 4".

**Results (2026-11-27):** a shell from marker 1 (**65 blocks**) destroyed the target. So the range is at least 65 blocks; in practice, like the rocket, it has none. That shot paid **no points**, but in D a target a few blocks away paid 100.

### A5. Score against distance

**What we're validating:** whether `GTA8.EXE` pays for a wreck only near the player or on screen, and if so, from how far. gfa-sim pays however far away it is.

**Steps:** each try uses up the target, so restart `tank_range` for each one.
1. Get in, drive to a marker, stop, and note the score.
2. Fire one shell, wait about 5 s, and note the score again.
3. Try marker 9 (20 blocks) first. If it pays, try further out (marker 6, 35); if not, closer (marker 11, 10, then 12, 6).

**Report**, for each try:
- **A5.** The distance, the points it paid (0 or 100), and whether you heard the bang.

**Results (2026-11-27):** the target was destroyed every time. It paid **100 from 6 blocks** (marker 12) and **nothing from 10 blocks** (marker 11) **or further**. Taken as a rough cut-off of **about 8 blocks**.

### A6. A chain, headless (`tank_chain`)

**Layout:** the range test's road and tank, with 11 cars nose to tail in the tank's lane, 4 to 14 blocks ahead.

**Check:** `tools/dosprobe/scripts/tank_chain_check.probe` gets in, fires one shell at the nearest car and captures the screen every 0.5 s for 30 s.

**Result:**
- **Score:** 0 → 100 within 0.5 s, then **1500** by 2 s, and no more. With the doubling streak found in A7, 1500 = 100+200+400+800: the first 4 cars, 4 to 7 blocks away. The car 8 blocks away paid nothing, so the cut-off is **between 7 and 8 blocks**.

### A7. The chain, frame by frame, headless (`tank_chain_view`)

**Layout:** the same road and tank, with 6 cars side by side, 1 block apart, across the screen 1.6 blocks ahead.

**Check:** `tools/dosprobe/scripts/tank_chain_frames.probe` fires one shell and captures the screen every 21 ms, about two captures per game frame.

**Result** (game frames of 42.86 ms after the key):
- **f3:** the shell bursts on the car.
- **f4:** that car blows up, paying 100.
- **f14:** both neighbours blow up, paying 200 and 400.
- **f24 and f25:** the next pair, paying 800 and 1600.
- **f34:** the next car, paying 3200.
- **In short:** a car a blast writes off blows up 10 frames (≈430 ms) after the blast, and each further car in the streak pays double.

---

## B. What the shell bursts on (`tank_burst`)

**What we're validating:** how the shell meets each kind of thing in its way. For each one, did it burst there, pass through or over it, or stop with no explosion?

- These are the shell's collision rules, which the port codes one by one.
- The question in each case is "does it behave like the rocket?".

**Layout:**
- The tank is in lane 221, facing north.
- **Ahead:** a car, 4 blocks (off screen until you drive a little forward).
- **Behind:** a ped held still, 3 blocks.
- **To the right (east):** the solid wall of a five-storey building, 2.5 blocks.

**Steps and what to look for:**

| Check | Do this | Rocket rule (the expectation) | Report |
|---|---|---|---|
| **B1. A car** | Fire straight ahead at the car. | Bursts on the car, which is written off at once: it burns, then blows up. | Did it burst on the car? Was the car written off by that single shell, or only damaged (dents, smoke)? If only damaged, how many shells did it take? |
| **B2. A burnt-out wreck** | When the car is a wreck, fire at the wreck. | Not settled for the rocket either. | Did the shell burst on the wreck, or fly on through it? |
| **B3. A building wall** | Turn the turret right (east) to face the building, then fire. | Bursts on the wall; three small fires burn on this side of the wall for a few seconds. | Did it burst on the wall? How many fires, if any? Did the wall itself change (it should not: walls cannot be destroyed in GTA1)? |
| **B4. A person** | Turn the turret round (south) to face the ped standing behind, then fire. | Bursts on the person, killing them. | Was there an explosion at the ped? Or was the ped killed with no explosion, the shell carrying on? Or did the shell pass through without hurting them? |

**Report:** one line for each of B1–B4 that answers the "Report" column.

**Results (2026-11-26/27):**
- **B1, B2 and B4:** same as a rocket.
- **B3:** one fire, lasting about 22 s, against gfa-sim's three lasting 5 s.
- **B5:** a *rocket* at the same wall also leaves one fire for 22 s. gfa-sim now does the same.

---

## C. Railings and water (`tank_fence`)

**What we're validating:** two more collision rules.

- **Railings:** whether railings (and fences) stop the shell. In GTA1 these are "flat" faces, which bullets and rockets cannot pass. A tank shell might be different.
- **Water:** whether the shell bursts on water.

**Layout:**
- The tank is on the bridge deck in the east lane, facing north.
- **To the right (east):** the bridge railing, 1.5 blocks, then open water.
- **To the left (west):** the other railing, 5.5 blocks (off screen).

**Steps:**
1. Turn the turret right (east), towards the near railing, and fire.
2. Turn the turret left (west), across the lanes, and fire. Listen.

**Report:**
- **C1. The near railing:** did the shell burst on the railing (an explosion at the railing), or fly over it?
- **C2. If it flew over:** did it burst when it reached the water, fly on and burst further out, or just disappear?
- **C3. The far railing (west):** did you hear or see a bang?

**Results (2026-11-27):**
- **C1:** the near (east) edge is a wall-typed block. The shell bursts there and leaves the 22 s fire.
- **C2 and C3:** the shell flew over the west railing (tile 102, on gfa-sim's shot-through list) with no bang.
- **Water:** still untested.

---

## D. Damage to vehicles (`tank_damage`)

**What we're validating:** how many shells each kind of vehicle takes, and how far the blast reaches.

- **Rocket:** one hit writes off any vehicle, from a car to a tanker.
- **Shell:** may be weaker, or stronger, and heavy vehicles may take more than one.
- **Blast reach:** gfa-sim's rocket blast writes off other cars within 1 block. `GTA8.EXE`'s code suggests 2. The target car's neighbours are spaced to tell the two apart.
- **Walls:** damage to walls is covered by B3. GTA1 walls take none, apart from fires.

**Layout** (all parked facing north, on flat road, about 5 to 7 blocks ahead, so off screen until you drive forward):
- **Straight ahead:** a Coach, about 5 blocks to its tail.
- **About 22° to the left:** a target car, with two neighbours on the verge to its left:
  - neighbour 1, 1 block away
  - neighbour 2, 2 blocks away.
- **About 30° to the right:** a Tanker, on the verge (not in a lane, so traffic does not queue in front of it).

If you cannot see a target, drive forward until it is on screen, stop, then aim and fire.

**Steps:**
1. **Coach:** fire straight ahead, one shell at a time, until the Coach is written off. That means it catches fire and then blows up. Count the shells: the top-left counter helps.
2. **Car:** turn the turret slightly left until the barrel points at the target car. Fire one shell at a time until it is written off.
3. **Neighbours:** watch them when the target blows up.
4. **Tanker:** turn the turret right, towards the Tanker. Fire until it is written off.
5. **Score:** note the score before and after each vehicle blows up.

**Report:**
- **D1. Coach:** shells to write it off.
- **D2. Target car:** shells to write it off.
- **D3. Neighbour 1 (1 block):** did it also catch fire or blow up when the target blew up?
- **D4. Neighbour 2 (2 blocks):** same question.
- **D5. Tanker:** shells to write it off, and how many explosions it went up in. A plain Tanker wreck was 5 in a line.
- **D6.** The score gained for each vehicle.

**Results (2026-11-27):**
- **D1, D2 and D5:** one shell each. The Tanker also goes up in its usual 5 blasts in a line, after the shell's own burst where it hit.
- **D3 and D4:** neighbour 1 blew up shortly after the target, and neighbour 2 slightly after neighbour 1. That is a chain of 1-block reaches, as in gfa-sim.
- **D6:** 100 for each vehicle.
- **Optional check:** deferred.

**Optional, very last** (deferred; it may crash the game): a rocket at a tank crashed `GTA8.EXE` with "ADDING TWICE". If you want to know whether a shell does the same, drive to Vice City's other tank and shoot it.

---

## E. The turret aiming itself (`tank_turret`)

**What we're validating:** whether the original turret really aims itself, and if so, at what and when.

- In the first Vice City run, the turret swung towards the attacking soldiers with no key pressed.
- **If it does aim itself,** the port needs turret AI:
  - what it targets
  - how fast it turns
  - whether it also fires by itself
  - whether your Tab+Left/Right overrides it.
- **If it doesn't,** the turret only turns on Tab+Left/Right, and the swing we saw had another cause.

**Layout:**
- The tank is where the level puts it, in the army base, facing west.
- ONE of the frenzy's eight soldiers is left hostile, so the tank lasts long enough to watch. In the original setup all eight wrote it off in about 2.5 s.

**Steps:**
1. Get in and touch nothing for 20 seconds. Watch the turret and the soldier.
2. If the turret moved: let the soldier walk about. Does the turret follow him?
3. Drive slowly round the base without Tab. Watch where the turret points.
4. Turn the turret well away from him with Tab+Left/Right, then let go. Watch for 10 seconds.
5. **Comparison, no attacker:** run `vc_tank_calm`, which has no hostile soldiers. Get in and sit still for 20 seconds, with ordinary peds and traffic around.
6. **Comparison, no frenzy:** run `combat_tank`, a tank in a test arena with no frenzy. Get in and sit still for 20 seconds.

**Report:**
- **E1.** In step 1, did the turret turn with no key pressed? (yes/no) If yes, towards the soldier, or somewhere else?
- **E2.** Did it follow the soldier as he moved? Roughly how quickly: snaps to him, or turns slowly?
- **E3.** Did the turret ever fire by itself?
- **E4.** While driving (step 3): did it keep pointing at the soldier, or line up with the hull?
- **E5.** After you turned it away (step 4): did it swing back towards him? After how long?
- **E6.** `vc_tank_calm`: did the turret move at all? If so, towards what?
- **E7.** `combat_tank`: did the turret move at all?

**Result (2026-11-27):** the turret does not aim itself. It turns only on Tab+Left/Right, so the swing in the first Vice City run had another cause.

**Turn rate (headless, `tank_turret_rate.probe` on `tank_range`):** 8 angle units a game frame either way, a full turn in 32 frames (1.37 s; six turns in 8.26 s). It starts and reverses at once, and the hull does not steer while Tab is held. Ported with the cannon (`docs/M6B_PROBES.md`). It turns with the hull as the tank steers (E4, owner), as gfa-sim does.

---

## Anything else

Note anything odd too, with the test it happened in. Examples:
- the turret coming off the hull
- a crash or error message
- the shell going somewhere other than where the barrel points
- the tank taking damage from your own shells at close range.
