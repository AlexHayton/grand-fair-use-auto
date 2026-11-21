# Parity ledger

Tracks every feature of the original game: what AmiGTA1 (our behavioural
source) does with it, and where Grand Fair Use Auto stands. Update a row when
its state changes, and put the evidence in the last column.

**Original**
- ✓ in the game
- ? unverified; settle it before building (IMPLEMENTATION_PLAN Principle 8)
- ✗ not in the game, so don't add it

**AmiGTA1**
- full
- partial
- none

**GFA**
- — not started
- wip
- done
- n/a

AmiGTA1 references are to `native/` in a sibling `../AmiGTA1` checkout.

## On foot (M3)

| Feature | Original | AmiGTA1 | GFA | M | Evidence / notes |
|---|---|---|---|---|---|
| Run (default), turn, back up at half pace | ✓ | full | done | M3 | `gta_player.c`; oracle traces match |
| Walk key | ✗ | full | done | M3 | Not in the original (it only runs); AmiGTA1 and we keep it on Shift as an extra |
| Collision from ground types, wall sliding | ✓ | full | done | M3 | `gta_player.c`, `gta_map_ground_type` |
| Missing pavement blocks count as pavement | ✓ | full | done | M3 | `gta_map.c` (NYC 25,17) |
| Ramps between layers | ✓ | full | done | M3 | NYC 19,44 lands on layer 3 (`tools/scripts/walk_ramp.txt`) |
| Fences (solid flat faces) on GTA's maps | ✓ | full | done | M3 | `gta_nav.c` tile lists per style |
| Fences on London's maps | ✓ | none | — | M11 | No tile lists for London's styles yet: walked through for now |
| Railway blocks closed to traffic routing | ✗ | full | done | M3 | AmiGTA1's own deviation (the original treats them as road); kept, for traffic |
| Drowning, punching, jumping, vaulting cars | ✓ | partial | done | M4–M6 | `gta_main.c` (water ticks, punch, vault). The punch: six states, the blow on the third, ten of thirteen land (`punch.txt` matches AmiGTA1); the victim's counter-punch is M6b's |

## Vehicles (M4)

| Feature | Original | AmiGTA1 | GFA | M | Evidence / notes |
|---|---|---|---|---|---|
| Car handling (two-tyre body, style physics) | ✓ | full | done | M4 | `gta_vehphys.c`; oracle traces match |
| Wall resolve, ramps, layers, jumps | ✓ | full | done | M4 | `gta_vehphys.c`, `gta_main.c:790-805`; NYC jump at x 91 lands |
| Damage deltas, burning, write-offs | ✓ | full | done | M4 | `gta_car.c` (`gta_car_panel_delta`), `gta_main.c` (fuse) |
| Explosions harming what is near, burnt wrecks | ✓ | full | wip | M4/M6 | `gta_weapon.c` (`gta_weapons_explode`, `_wreck_car`): kills within 17 px, sets alight to 30, writes off cars within 32, the player too; one, three or five blasts by body (`rocket.txt`, `rocket_close.txt` match AmiGTA1). The burnt look of a wreck remains |
| Enter/exit by door, bikes | ✓ | full | wip | M4/M6b | `gta_main.c` (door point, enter and exit steps). Known issue (user report, 2026-11-21): the get-in animation does not match the original. He seems to enter the door from the wrong side. Compare frame by frame under DOSBox-X |
| Vault over a car | ✓ | full | done | M4 | `gta_main.c` (vault states, far-side entry, blocked-door exit); `tools/scripts/vault.txt` |
| Slide under a tall vehicle | ✓ | partial | done | M4/M6b | only while one is over him, as AmiGTA1; the original's approach from outside to verify |
| Drag out the driver | ✓ | full | done | M5 | `gta_peds_pull` (door fully open, 6 states × 4 ticks, then lying 84 ticks) and `_knock_off` for a bike; `taking_a_car_drags_its_driver_out`. The carjack's score and a cop car's driver chasing him are M6 |
| Tank: handling | ✓ | full | — | M6b | engine cut, `gta_vehphys.c:395`, is in the physics; check against the original |
| Tank: turret and cannon | ✓? | none | — | M6b | not modelled anywhere in AmiGTA1; measure under DOSBox-X |
| Boats: driving on water | ✓? | none | — | M6b | placed as an object only, `gta_traffic.c:1152`; no doors in the car table; measure under DOSBox-X |
| Cars sinking; drowning | ✓ | partial | done | M4/M5 | `gta_main.c:2253`, the walk into water and the jump falling short; `walk_water.txt` and `drive_sink.txt` match the oracle. Peds in the water drown (ours: AmiGTA1's stand on it), gone after 67 ticks; `a_ped_in_the_water_drowns` |
| Helicopter (scripted escape) | ✓ | none | — | M6b/M7 | filtered from traffic, `gta_traffic.c:1186`; drivable? measure under DOSBox-X |
| RC car / map-placed specials | ? | partial | — | M6b | `gta_traffic.c:1152`; model 47, no doors; measure under DOSBox-X |
| Trains: routes, stops, boarding, crush | ✓ | partial | done | M4 | `gta_train.c`; straights and quarter circles, not yet the original's path table; oracle matches |
| Trams | ✓ (static) | full | done | M4 | the original's trams stand still |
| Horn / pull-over look-ahead | ✓ | partial | wip | M5/M6b | The horn: TAB held in a car without a siren (`special_key`), done, `horn.txt`. The pull-over (`gta_main.c:3957`, NOT PORTED there): TAB held in a car, the nearest car up to five blocks ahead moves to the kerb (the original's offsets 7 and 0x37 of 0x40) and stops; `Traffic::pull_over`, `the_car_ahead_pulls_over_for_the_horn`. **(verify)** under DOSBox-X: how far to the side the look reaches, how slowly it moves over, that it stops, for how long, and whether the siren left on does it too |
| Bombs: timed, speed | ✓ | full | — | M6 | `gta_main.c:4266` |
| Bombs: tanker (type 4, five blasts) | ✓ | none | — | M6 | `gta_main.c:4272` |
| Chain explosions of wrecks | ✓ | full | done | M6 | A blast writes off cars in reach, whose own fuses run (`rocket.txt`) |
| Siren/lights toggle in emergency vehicles | ✓ | partial | done | M5 | `special_key` (from the original's routine; `car+0x113`): models 4, 5 and 0x2a; first press on, a press after that sounds it while held, off when he gets out. AmiGTA1 plays the sound but does not light his car; ours flashes lighting deltas 15 and 16 as its dispatched cops do. `siren.txt`, `the_special_key_works_the_siren_then_sounds_it`. The sound itself is M8 |

## Living city (M5)

| Feature | Original | AmiGTA1 | GFA | M | Evidence / notes |
|---|---|---|---|---|---|
| Traffic spawn/despawn, lanes, junctions, lights | ✓ | full | wip | M5 | `gfa_sim::traffic` matches AmiGTA1 car for car (`oracle_traffic`: 4 sites × 3 seeds × 3,000 ticks). The rings follow the simulation's view (`SimCamera`: lead and speed zoom), not the window, and the camera never shows more than it: the pop-in check finds nothing coming or going in sight in all three cities. Ours: a car stuck at a dead end gives up only out of sight (AmiGTA1 deletes it in view; the oracle carries the change, `traffic_giveup.pl`). The lamps are drawn on the crossings' corners, facing the drivers coming in; each crossing has one light (AmiGTA1's timing split most crossings so both roads were green at once; `traffic_lights.pl`). The light cycle is AmiGTA1's own: **(verify)** the original's. Ours: a car refused its turn at a corner with no way on waits for it rather than driving onto the pavement (`traffic_corner.pl`), a car shoved out of its lane steers back into it rather than standing at a "dead end" (`traffic_rejoin.pl`), long vehicles take corners whose far side is a building or grass (`traffic_fits.pl`), and a car does not queue behind one past the corner it turns at (`traffic_follow.pl`). The 12,000-tick gate's traces match AmiGTA1; and all twelve pass its checks (AmiGTA1 unpatched: 3). Also ours: new cars put down clear of other bodies, a turning car watching the street it turns into, and the light holding a car by its nose (`traffic_spawn.pl`, `traffic_follow.pl`, `traffic_stopline.pl`) |
| Camera: zoom out with speed | ✓ | none | done | M5/M6b | Ours: up to 1.5× at a sports car's top speed, as simulation state. AmiGTA1 zooms only on a key. **(verify)** the original's curve under DOSBox-X |
| Camera: lead ahead of a moving car | ? | none | done | M5 | Carnage3D's 0.35 s, capped at a third of the view so the car stays on screen; **(verify)** against the original |
| Junction wobble, corner landing error | ✓ | partial | done | M5 | `gta_traffic.c:5871`, `:5666`. Measured over the gate's twelve runs: no car turns twice in a crossing (the wobble came from cars with no route, now 0.2% of car-ticks); 99.3% of straight crossings and of turns end within a pixel of the line. `cars_keep_their_lanes_through_junctions`. AmiGTA1's rule of not steering inside a crossing is kept (decision log) |
| Collisions between cars (SAT + impulse) | ✓ | full | done | M5 | `gta_traffic.c:8067`, `gta_traffic_ram`, `gta_traffic_sweep_box`; within the fleet and the player's car against it. Walk and drive oracles compare the fleet every tick (46 rams across the random drives) |
| Traffic and the player: rams, taking a driven car | ✓ | full | done | M5 | `gta_traffic_ram`, `gta_traffic_grab_car`, `gta_traffic_abandon`; the driver of a taken car is dragged out |
| Traffic and the map's parked cars | ✓ | n/a | done | M5 | AmiGTA1 has no map-placed cars. Ours stand in the fleet all level (decision log); at rest they do not push each other |
| Write-offs in the fleet burn out | ✓ | full | done | M5/M6 | `gta_weapon.c` `wreck_sweep`, with its blast inline (`Weapons::wreck_sweep`) |
| Peds: pavements, crossings, panic, knockdown | ✓ | full | done | M5/M6 | `gfa_sim::peds` matches AmiGTA1's crowd ped for ped in every walk and drive oracle run. Pool sized to the view (33 in the game, 12 at AmiGTA1's). Shot, punched, panicked by gunfire and set alight since M6 stage 1 |
| Peds: AmiGTA1's anti-pileup rules | ? | full | done | M5/M6b | AmiGTA1 departs from the original in four places so people do not bunch: corner targets spread across the pavement, separation and relax between people, no resting near corners, a coin per ped at a T-junction. Kept; compare with the original under DOSBox-X. Ours: the sidestep's bend no longer piles up into a spin or flicks along a kerb (decision log 2026-11-21; the oracle carries the fix, `ped_sidestep.pl`) |
| Peds run over, killed by trains | ✓ | full | done | M5 | `gta_peds_ram` (a man lying down is killed but not counted, as AmiGTA1), `gta_train.c`'s crush; `ped_runover.txt`, `a_car_runs_a_ped_over` |
| Ped dress (remaps) | ✓ | full | done | M5 | GRY tables, and in the G24 the ped remap CLUTs, paired by measurement (`PED_REMAP_TABLES`); `people_wear_the_same_in_either_style` |
| Ped counter-punch | ✓ | none | — | M6 | `gta_peds.c:1675`. Moved from M5: needs the player's punch. What it does is not in AmiGTA1: **(verify)** M6b |
| Gang roles | ✓ | partial | — | M7 | `gta_peds.c:1111`. Moved from M5: only the mission script makes gangs. AmiGTA1 has not read their state machine: **(verify)** M6b |
| Hare Krishna / Elvis groups | ✓? | none | — | M6 | Moved from M5: needed for the bonuses. Carnage3D's `docs/ped.txt` lists ped type 143 "Hare Krishnas" for mission scripts; where they walk, how often and which levels have Elvises: **(verify)** M6b |

## Combat, police, services (M6)

| Feature | Original | AmiGTA1 | GFA | M | Evidence / notes |
|---|---|---|---|---|---|
| Pistol, machine gun, rocket, flamethrower | ✓ | full | done | M6 | `gfa_sim::weapons` from `gta_weapon.c`; `shoot_ped.txt`, `rocket*.txt`, `flame.txt` match AmiGTA1 tick for tick. Crates hold the original's 20/20/5/10 (AmiGTA1 triples them). Splat life and a few latch details **(verify)** M6b |
| Respray refusals (emergency, RC car, tank, no cash) | ✓ | ? | — | M6 | ENGLISH.FXT `no_emerg_respray`, `no_modelcar_respray`, `no_tank_respray`, `no_respray` |
| Weapon crates | ✓ | ? | — | M6 | `gta_weapon.h:34` list is partly stale |
| Friendly-fire groups | ✓? | none | — | M6 | `gta_weapon.h:34` |
| Armour absorbing hits | ✓ | partial | wip | M6 | A point stops a bullet (`World::harm`); the pickup that gives it comes with the crates |
| Fires left on walls by rockets | ✓? | full | done | M6 | AmiGTA1 has built them since `gta_weapon.h:34` was written: three fires 10 px back from the wall, 250 ticks (`rocket_wall.txt`) |
| Power-ups (speed, multiplier, frenzy…) | ✓ | partial | — | M6 | "nothing reads them", `gta_main.c:10291` |
| Kill frenzies | ✓ | partial | — | M6/M7 | crate handed to the script |
| Bonuses: GOURANGA, Elvis, cop kills | ✓ (GOURANGA) / ? | none | — | M6 | `gta_score.h:17`. ENGLISH.FXT has `[8787] GOURANGA!`, cards `4000`–`4005` (YOU GOT IT!, FRENZY PASSED!, BONUS LOST!, BUSTED!, WASTED!, GO! GO! GO!), `frenzy`, `multiplier+`. No Elvis or cop-kill card text found yet |
| Wanted levels, chases, roadblocks, arrest | ✓ | full | wip | M6 | `gta_traffic.c:9681-10500`. The heat and the level (`gfa_sim::score`, each city's thresholds) are done; the police are next |
| Police: army/SWAT/FBI escalation | ✗ | n/a | n/a | — | `gta_traffic.h:2622`: cop cars only |
| Siren reason in police AI | ✓ | partial | — | M6 | `gta_main.c:9976` |
| Cop 0x13 call | ✓ | none | — | M6/M8 | `gta_snd.h:219` |
| Crime counters on the bust pager | ✓ | partial | wip | M6 | `gta_score.h:16`. Counted per life and per level (`Score::crimes`, `total`); the pager is M7's HUD |
| Bribes, jail-free card | ✓ | partial | — | M6 | check end to end |
| Ambulance, fire engine (water cannon) | ✓ | full | — | M6 | `gta_service.c` |
| Spray shop, bomb shop | ✓ | full | — | M6 | |
| Tyre marks, blood, oil for every car | ✓ | partial | — | M6 | player car only; `gta_marks.h:18` |

## Missions, HUD, text (M7)

| Feature | Original | AmiGTA1 | GFA | M | Evidence / notes |
|---|---|---|---|---|---|
| Mission VM (32 processes, triggers, phones) | ✓ | full | — | M7 | `gta_script_run.c` |
| Unknown opcodes reported, not silently passed | n/a | none | — | M7 | AmiGTA1 maps them to success |
| SPECIFIC_DOOR remap (5th number) | ✓ | none | — | M1 | `parse_line` keeps 4 numbers |
| First job of each level | ✓ | full | — | M7 | `threejobs`, `sa1`, `vc1`… scripts |
| Every job of every chapter | ✓ | none | — | M7 | only first jobs played through |
| Cranes, garages, doors, bank robbery | ✓ | full | — | M7 | in `gta_main.c` |
| Races (checkpoints, win/lose) | ✓ multiplayer only | n/a | n/a | — | the "Canonball Run" sections [1002]–[1004] etc. are multiplayer; FXT `win_race`, `lost_race`. Out of scope with multiplayer |
| HUD (cop heads, pager, brief box, cards) | ✓ | full | — | M7 | `gta_hud.c` |

## Front end, audio, saves (M8)

| Feature | Original | AmiGTA1 | GFA | M | Evidence / notes |
|---|---|---|---|---|---|
| Title and menus, player and chapter select | ✓ | partial | — | M8 | `gta_front.c` |
| Cheat names | ✓ | full | — | M8 | `gta_career.c:223` |
| Language select (4 FXTs, SPECIAL.FXT) | ✓ | partial | — | M8 | `GTA_CHEAT_LANG` |
| High-score table | ✓ | partial | — | M8 | `gfa_formats::career` reads the six tables; display and entry to do |
| Briefing stills CUT0-5 with voice | ✓ | none | — | M8 | |
| Attract-mode demos (.REP) | ✓ | none | — | M8 | `{u32 tick, u32 input}` change logs, one per city (FORMATS.md); input bits not decoded |
| Pause menu, options | ✓ | none | wip | M8 | F6 pauses, the original's key (its README); the menu and options are M8 |
| The original's keys | ✓ | partial | done | M5 | Default layout from the 1997 README: arrows, Space, Enter, Ctrl, Z/X, Tab, F5 radio, F6 pause, F7/F10 repeat messages, F9 area name, F11 screen mode, Esc. Our debug keys keep off them (decision log). The "Classic" layout is M12 |
| SFX banks, positional mix, engine pitch | ✓ | full | — | M8 | `gta_sfx.c`, `gta_audio.c` |
| VOCALCOM speech, MISBRIEF briefings | ✓ | partial | — | M8 | |
| CD-audio radio stations | ✓ | partial | — | M8 | clock-based, `CDAUDIO.DAT` |
| Saves (`PLAYER_A.DAT`) | ✓ | full | — | M8 | `gta_career.c` |
| FMV intro (`MOVIE.SMK`) | ✓ | none | — | M9 | |

## Datasets (M11)

| Feature | Original | AmiGTA1 | GFA | M | Evidence / notes |
|---|---|---|---|---|---|
| GTA: Liberty City, San Andreas, Vice City | ✓ | full | detected | M1+ | `Datasets::detect` |
| London 1969 | ✓ | none | detected | M11 | `GTADATA/UK/MISSUK.INI` |
| London 1961 | ✓ | none | detected | M11 | `GTADATA/UK/MISSUKE.INI` |
