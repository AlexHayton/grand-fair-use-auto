# M6b: Hare Krishnas and GOURANGA! Manual Test Plan

This plan covers manual checks in `GTA8.EXE` under DOSBox-X for the special ped groups entry in `docs/M6B_PROBES.md`. Most of the rules were read from the game's own code and checked headlessly. What is left needs eyes: how the line walks, what the score popups show, and what the others do when their leader dies.

## What the code says (to confirm)

1. **One kind of group: Hare Krishnas.** A line of **7** in orange robes: a leader and 6 followers, each following the one in front. There are no Elvises: no text, no remap and no code for them. Only Krishnas appear, in all three cities.
2. **When the line appears.** The game counts every passer-by it creates. The **100th** since the level began, if no line exists, becomes the leader, and the 6 followers appear on top of him. The count starts again when the **leader** dies, so the next line comes 100 passers-by later. There is never more than one line, and it is not removed when off screen.
   - It counts passers-by, not seconds, so the wait depends on how you move. New passers-by are made as pavement comes into view, so driving round brings a line sooner and standing still much later. Walking in the arena, the game made between 27 and 86 in each 10 s.
   - Headless timings, on foot: Liberty City 52 s, San Andreas and Vice City 50–60 s. In the synthetic `arena` it took under 20 s, because passers-by spawn faster there.
3. **What a Krishna pays.** Each is worth 100, times a base for how he died, doubling along a streak:
   - run over (base 1): 100, 200, 400, 800, 1600, 3200, 6400
   - shot (base 2): 200, 400, 800 …
   - blown up (base 3): 300, 600, 1200 …
   - burnt (base 10): 1000, 2000, 4000 …
   - An ordinary passer-by does not double: he pays 100, 200, 300 … when run over.
4. **GOURANGA!** The 7th Krishna in one streak shows **GOURANGA!**, plays a sound and adds **12,300**. A streak breaks when more than about 0.6 s (13–14 game frames) passes between two kills, or when something else is killed in between.
   - So a line run over in one pass pays 12,700 + 12,300 = **25,000**.
5. **When the leader dies,** the others stop following: their brain switches to what AmiGTA1 calls "flee". They still count as Krishnas for the score.

Points 3 and 4 run against gfa-sim and AmiGTA1. Both pay a run-over streak of anything but passers-by as 100, 200, 300 …, and gfa-sim gives GOURANGA (+500) for any 4 people run over in a streak. The popups in K2 settle which is right.

## Results so far (owner, 2026-11-27)

- **K1.** A line appeared after a while of driving. They walk slowly forward, following the leader.
- **K2.** The popups went 100, 200, 400, 800, 1600 …, so the doubling is right.
- **K3.** The streak went up, and there was no GOURANGA!. The leader, killed later on his own, gave no GOURANGA! either. It only shows when all 7 go in one streak.
  - **The streak breaks after a gap of a couple of seconds** (owner), so the popups start again at the base. That fits the code's window of 13 game frames, about 0.56 s.
- **K4.** With the leader dead first, the rest scattered at once. That matches the code: they become type 42 and flee.
- **K5.** The next line came quickly once, and then not for a long time. That looked random. The code has no chance roll: every passer-by created counts, with no path past the check. The spread comes from how fast passers-by are made (see point 2), and a line can start out of view in a 46-block plaza. Also, no new line comes while the last one's leader is alive.
  - **The timing waits for M8**, with the passer-by spawner it depends on (K5 and K7's timing).
- **K6.** Each weapon's first popup gave its base, and every one doubled. K6c (the score) was skipped.
  - **Machine gun:** 200, 400, 800 …, and GOURANGA! appeared. A gun reports "shot" (base 2), and GOURANGA! comes from any weapon, as the code says.
  - **Rocket:** 300, 600, 1200 …. A rocket's victims count as blown up (base 3), not burnt, as gfa-sim already has it. No GOURANGA!, because one rocket could not catch all 7.
  - **Flamethrower:** 1000, 2000, 4000 …: burnt (base 10). Not all 7 could be caught in one go, so GOURANGA! by fire is untested.
  - **GOURANGA! by rocket or by fire** was not tested; it is taken as an educated guess (owner, 2026-11-27). Both give it, as the machine gun did: the code counts only the Krishnas in the streak, not how they died. At ×1 that is 50,400 by rocket and 139,300 by fire (the table in K6).
- **K7.** Starting from 0 at ×1, a whole line run over in one pass gave exactly **25,000**. That confirms the 12,300 bonus.
- **K9.** The followers trace the leader's path:
  - they stay on the pavement and turn where he turned;
  - they don't bump into the buildings;
  - the gaps stay the same through a corner.
- **Seen in K9: the leader follows the player.** When the owner stood near the leader, he started to follow the player. The code bears it out (`0xbb2e6`): each tick the leader looks within half a block of himself, and if the first person he finds is the player, he follows him.
- **Ported to gfa-sim (2026-11-27)** with all of the above (`docs/M6B_PROBES.md`, the decision log).
- **From the owner's play of the game:** traffic stops when a line crosses the road, and a Krishna killed by traffic pays the player nothing.

## Setup

**The arena** (most tests): a walled 46×46-block plaza of pavement crossed by two roads, with no traffic.
- You start on foot at its centre. A Bulldog is parked 2 blocks to your right (east), facing south.
- The overlay is already built in `tools/dosprobe/build/overlays/arena`. If it is missing, rebuild it with:

```
cargo run -p gfa-tool -- probe-overlay arena --out tools/dosprobe/build/overlays/arena
```

Start it with:

```
python3 tools/dosprobe/run.py --interactive --cycles 150000 \
  --overlay tools/dosprobe/build/overlays/arena
```

**Liberty City** (K7): the plain game, no overlay:

```
python3 tools/dosprobe/run.py --interactive --cycles 150000
```

**Notes:**
- **Menus:** press Enter through them as usual. On the default name you have no weapons. For K6, delete the name and type `suckmyrocket` for all of them.
- **Score:** the score is top right. The multiplier (×1 at the start) is under it. Each kill shows a popup with its points where it happened.
- **Timing:** a stopwatch, or counting seconds, is fine.

---

## K1. The line appears (arena)

**Steps:**
1. Get in the Bulldog and drive slowly round the plaza. Keep off the walls.
2. Note roughly how long after the start the line first appears.
3. Follow it for half a minute without touching it.

**Report:**
- **K1a.** Seconds from the start until you first saw it.
- **K1b.** How many there were, and what they looked like (orange robes? all the same?).
- **K1c.** How they walk: single file, side by side or bunched? About how far apart (a block is about a lane wide)?
- **K1d.** Where they walk: pavement only, or across the roads too? Do they stop, or turn at corners?

## K2. GOURANGA! (arena)

**Steps:**
1. Line the car up with the line, from behind or head on.
2. Drive through all 7 in one pass, without stopping.
3. Watch the popups and the score.

**Report:**
- **K2a.** The popup for each Krishna, in order. Expected: 100, 200, 400, 800, 1600, 3200, 6400. The other reading would give 100, 200, 300 … 700.
- **K2b.** Did GOURANGA! appear? Was there a sound or voice?
- **K2c.** The score before and after. Expected: +25,000.
- **K2d.** If you missed some, what happened (popups, and whether GOURANGA! still showed).

## K3. A broken streak (arena)

A new line should appear about 20 s after the last leader died.

**Steps:**
1. Run over 2 or 3 from the back of the line, not the leader.
2. Drive away for 2–3 seconds.
3. Come back and run over the rest.

**Report:**
- **K3a.** The popups in each pass. Expected: 100, 200, (400), then starting again at 100, 200, 400 …
- **K3b.** Did GOURANGA! appear? Expected: no.
- **K3c.** After the first pass, did the rest close up and carry on in line?

## K4. The leader first (arena)

**Steps:**
1. Run over only the leader, the one at the front.
2. Stop and watch the others for 10 s.
3. Then run over the rest, as many as you can in one pass.

**Report:**
- **K4a.** What the others did when he died:
  - kept walking in line
  - scattered or ran away (from where he died, or from you?)
  - came at you
  - stood still
- **K4b.** The popups for the rest. Doubling (100, 200, 400 …) means they still count as Krishnas. 100, 200, 300 … means they became ordinary passers-by.

## K5. The next line (arena)

**Steps:**
1. After K4, with the leader dead, time how long until a new line appears.
2. Also, at any point while a line is alive and whole, keep an eye out for a second line.

**Report:**
- **K5a.** Seconds until the new line appeared. Expected: about 20 s.
- **K5b.** Did you ever see two lines at once? Expected: no.

## K6. Weapons (arena, name `suckmyrocket`)

**What this checks.** K2 settled the doubling for a run-over. A kill's points are 100 × a base × the doubling, and the base comes from *how* he died: the game passes a "reason" with each death. The code maps reasons to bases (shot 2, blown up 3, burnt 10), but not which reason each weapon reports. gfa-sim guesses from AmiGTA1: the guns report "shot", a rocket "blown up" and the flamethrower "burnt". K6 checks three things:
- **Which reason each weapon reports.** The first popup gives the base directly: 200 means shot, 300 blown up, 1000 burnt. A rocket's victims might count as burnt, since its blast sets people alight. If so, gfa-sim pays every rocket kill wrong, Krishnas or not.
- **That several deaths at once are separate steps.** A rocket that kills 3 at once should show 300, 600 and 1200 together, not three 300s.
- **That GOURANGA! comes from any weapon.** The bonus checks only the count of Krishnas in the streak, not how they died. Each kill has to come within about half a second of the one before; how long the whole streak takes doesn't matter. A burning man runs about before he dies, so fire can spread the deaths out and break the streak.

**Steps:**
1. Fire a rocket into the middle of a line, catching as many as you can at once.
2. On the next line, sweep the machine gun along it from one end, without stopping.
3. On a later line, use the flamethrower on as many as you can.
4. If a blast also kills an ordinary passer-by, note where in the popups it came. It should break the streak, so the next Krishna starts again at the base.

**Report, for each weapon:**
- **K6a.** The popups, in order.
- **K6b.** Whether GOURANGA! appeared.
- **K6c.** The score before and after, and the multiplier (×1 unless you've picked one up).

**Expected at ×1**, for all 7 in one streak:

| Weapon | Popups | With GOURANGA! |
|---|---|---|
| Machine gun (shot) | 200, 400 … 12,800 | 25,400 + 12,300 = **37,700** |
| Rocket (blown up) | 300, 600 … 19,200 | 38,100 + 12,300 = **50,400** |
| Rocket (if burnt) or flamethrower | 1000, 2000 … 64,000 | 127,000 + 12,300 = **139,300** |

## K7. Liberty City (settled)

What K7 was for is now covered elsewhere:
- **Traffic** is settled from the owner's play: cars stop for a line crossing the road, and a Krishna killed by traffic pays nothing.
- **Timing** waits for M8.
- **Corners** moved to K9, in an arena built for them.

What is left is the total for a whole line run over in one pass. Expected: 100 … 6400, then GOURANGA!, **+25,000** at ×1. That confirms the 12,300 bonus. It can be done in K9's arena instead, at the end.

## K9. Corners (the `corners` arena)

**What this checks.** The leader walks the pavement like any passer-by, and each follower walks after the one in front. At a corner, does a follower trace the leader's path round it, or cut across the inside? The port needs to know which: either a follower heads straight for where the one in front is now, or it walks his trail.

**The arena:** 1-block pavement paths in staircases, turning every 3 blocks, left and right in turn. 3 blocks of road part one staircase from the next, too far for a passer-by to cross, so the leader has to take every corner.
- **North of the start:** road fills the inside of each corner. A follower who cuts the corner is seen on the road.
- **South of the start:** a building (red roof) fills the inside of each corner, as at a city block. A follower who cuts it snags on the building.
- **The start:** you begin on foot on a 3-block road running east–west across the middle, with a Bulldog beside you, as in the `arena`. The north half is all road and pavement, so you can drive anywhere there. The south half has the buildings.

Build the overlay, if it is missing, and start it:

```
cargo run -p gfa-tool -- probe-overlay corners --out tools/dosprobe/build/overlays/corners
python3 tools/dosprobe/run.py --interactive --cycles 150000 \
  --overlay tools/dosprobe/build/overlays/corners
```

**Steps:**
1. Move about, on foot or in the car, until a line appears. Less of the ground is pavement here than in the `arena`, so it may take longer.
2. Follow the line on foot, without touching it, through several corners. North or south of the start, whichever it is in, and both if you can.
3. Optionally, finish by running the whole line over in one pass and noting the score before and after (K7).

**Report:**
- **K9a.** North half (road inside): did the followers keep to the pavement and turn where the leader turned, or step onto the road on the inside of the corner? If they cut, did the back of the line cut more than the front?
- **K9b.** South half (building inside): did they turn cleanly, or bump into the building and slide along it or get stuck?
- **K9c.** Did the gaps between them change at corners: bunching up as the leader turned, then stretching out again?
- **K9d.** Anything else: the leader at a dead end (where a staircase meets the middle road or the outer wall), a follower left behind, the line reversing.

## Optional: Vice City's look

The remap is the same number in every city, but each city has its own palette. To see Vice City's Krishnas, stage it in the level-1 slot:

```
python3 tools/dosprobe/level_overlay.py --section 202 \
  --out tools/dosprobe/build/overlays/krishna_vc
```

Then start it with `--overlay tools/dosprobe/build/overlays/krishna_vc` and look around for a minute.

**Report:**
- **K8.** What they look like.

---

## Anything else

Note anything odd too, with the test it happened in. For example:
- a Krishna left behind by the line
- the line walking into the road or into traffic
- a crash or an error message
- a different message from GOURANGA!

## Headless checks

`tools/dosprobe/scripts/krishna_walk.probe` walks a level for 2 minutes and dumps guest RAM every 10 s. Then `python3 tools/dosprobe/gta8.py krishna OUT/r*.bin` prints, for each dump:
- the spawn count and the number of groups
- the score and multiplier
- each Krishna's place in the line and position.

`gta8.py refs` and `gta8.py dis` show the code behind the rules above. `dis` needs `pip install capstone`.
