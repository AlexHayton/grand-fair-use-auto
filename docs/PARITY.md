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
| Drowning, punching, jumping, vaulting cars | ✓ | partial | wip | M4–M6 | `gta_main.c` (water ticks, punch, vault) |

## Vehicles (M4)

| Feature | Original | AmiGTA1 | GFA | M | Evidence / notes |
|---|---|---|---|---|---|
| Car handling (two-tyre body, style physics) | ✓ | full | done | M4 | `gta_vehphys.c`; oracle traces match |
| Wall resolve, ramps, layers, jumps | ✓ | full | done | M4 | `gta_vehphys.c`, `gta_main.c:790-805`; NYC jump at x 91 lands |
| Damage deltas, burning, write-offs | ✓ | full | done | M4 | `gta_car.c` (`gta_car_panel_delta`), `gta_main.c` (fuse) |
| Explosions harming what is near, burnt wrecks | ✓ | full | — | M4/M6 | `gta_weapon.c` (`gta_weapons_wreck_car`) |
| Enter/exit by door, bikes | ✓ | full | wip | M4/M6b | `gta_main.c` (door point, enter and exit steps). Known issue (user report, 2026-11-21): the get-in animation does not match the original. He seems to enter the door from the wrong side. Compare frame by frame under DOSBox-X |
| Vault over a car | ✓ | full | done | M4 | `gta_main.c` (vault states, far-side entry, blocked-door exit); `tools/scripts/vault.txt` |
| Slide under a tall vehicle | ✓ | partial | done | M4/M6b | only while one is over him, as AmiGTA1; the original's approach from outside to verify |
| Drag out the driver | ✓ | full | — | M5 | the driver needs M5's peds |
| Tank: handling | ✓ | full | — | M6b | engine cut, `gta_vehphys.c:395`, is in the physics; check against the original |
| Tank: turret and cannon | ✓? | none | — | M6b | not modelled anywhere in AmiGTA1; measure under DOSBox-X |
| Boats: driving on water | ✓? | none | — | M6b | placed as an object only, `gta_traffic.c:1152`; no doors in the car table; measure under DOSBox-X |
| Cars sinking; drowning | ✓ | partial | done (player) | M4/M5 | `gta_main.c:2253`, the walk into water and the jump falling short; `walk_water.txt` and `drive_sink.txt` match the oracle. Peds in the water: M5 |
| Helicopter (scripted escape) | ✓ | none | — | M6b/M7 | filtered from traffic, `gta_traffic.c:1186`; drivable? measure under DOSBox-X |
| RC car / map-placed specials | ? | partial | — | M6b | `gta_traffic.c:1152`; model 47, no doors; measure under DOSBox-X |
| Trains: routes, stops, boarding, crush | ✓ | partial | done | M4 | `gta_train.c`; straights and quarter circles, not yet the original's path table; oracle matches |
| Trams | ✓ (static) | full | done | M4 | the original's trams stand still |
| Horn / pull-over look-ahead | ✓ | none | — | M5 | `gta_main.c:3957` NOT PORTED |
| Bombs: timed, speed | ✓ | full | — | M6 | `gta_main.c:4266` |
| Bombs: tanker (type 4, five blasts) | ✓ | none | — | M6 | `gta_main.c:4272` |
| Chain explosions of wrecks | ✓? | none | — | M6 | `gta_weapon.h:34` |
| Siren/lights toggle in emergency vehicles | ? | partial | — | M5 | |

## Living city (M5)

| Feature | Original | AmiGTA1 | GFA | M | Evidence / notes |
|---|---|---|---|---|---|
| Traffic spawn/despawn, lanes, junctions, lights | ✓ | full | wip | M5 | `gfa_sim::traffic` matches AmiGTA1 car for car (`oracle_traffic`: 4 sites × 3 seeds × 3,000 ticks). The rings follow the fixed logical view, not the window; the pop-in check and the camera lead are still to do |
| Junction wobble, corner landing error | ✓ | partial | — | M5 | `gta_traffic.c:5871`, `:5666` |
| Collisions between cars (SAT + impulse) | ✓ | full | done | M5 | `gta_traffic.c:8067`, `gta_traffic_ram`, `gta_traffic_sweep_box`; within the fleet and the player's car against it. Walk and drive oracles compare the fleet every tick (46 rams across the random drives) |
| Traffic and the player: rams, taking a driven car | ✓ | full | wip | M5 | `gta_traffic_ram`, `gta_traffic_grab_car`, `gta_traffic_abandon`: done. The driver of a taken car is not thrown out until the peds arrive |
| Traffic and the map's parked cars | ✓ | n/a | done | M5 | AmiGTA1 has no map-placed cars. Ours stand in the fleet all level (decision log); at rest they do not push each other |
| Write-offs in the fleet burn out | ✓ | full | wip | M5/M6 | `gta_weapon.c` `wreck_sweep`; the blast's harm to others is M6 |
| Peds: pavements, crossings, panic, knockdown | ✓ | full | — | M5 | |
| Ped counter-punch | ✓ | none | — | M5 | `gta_peds.c:1675` |
| Gang roles | ✓ | partial | — | M5 | `gta_peds.c:1111` |
| Hare Krishna / Elvis groups | ✓? | none | — | M5 | needed for bonuses |

## Combat, police, services (M6)

| Feature | Original | AmiGTA1 | GFA | M | Evidence / notes |
|---|---|---|---|---|---|
| Pistol, machine gun, rocket, flamethrower | ✓ | full | — | M6 | `gta_weapon.c` |
| Respray refusals (emergency, RC car, tank, no cash) | ✓ | ? | — | M6 | ENGLISH.FXT `no_emerg_respray`, `no_modelcar_respray`, `no_tank_respray`, `no_respray` |
| Weapon crates | ✓ | ? | — | M6 | `gta_weapon.h:34` list is partly stale |
| Friendly-fire groups | ✓? | none | — | M6 | `gta_weapon.h:34` |
| Armour absorbing hits | ✓ | partial | — | M6 | pickup sets it; reader unconfirmed |
| Fires left on walls by rockets | ✓? | none | — | M6 | `gta_weapon.h:34` |
| Power-ups (speed, multiplier, frenzy…) | ✓ | partial | — | M6 | "nothing reads them", `gta_main.c:10291` |
| Kill frenzies | ✓ | partial | — | M6/M7 | crate handed to the script |
| Bonuses: GOURANGA, Elvis, cop kills | ✓ (GOURANGA) / ? | none | — | M6 | `gta_score.h:17`. ENGLISH.FXT has `[8787] GOURANGA!`, cards `4000`–`4005` (YOU GOT IT!, FRENZY PASSED!, BONUS LOST!, BUSTED!, WASTED!, GO! GO! GO!), `frenzy`, `multiplier+`. No Elvis or cop-kill card text found yet |
| Wanted levels, chases, roadblocks, arrest | ✓ | full | — | M6 | `gta_traffic.c:9681-10500` |
| Police: army/SWAT/FBI escalation | ✗ | n/a | n/a | — | `gta_traffic.h:2622`: cop cars only |
| Siren reason in police AI | ✓ | partial | — | M6 | `gta_main.c:9976` |
| Cop 0x13 call | ✓ | none | — | M6/M8 | `gta_snd.h:219` |
| Crime counters on the bust pager | ✓ | none | — | M6 | `gta_score.h:16` |
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
| Pause menu, options | ✓ | none | — | M8 | |
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
