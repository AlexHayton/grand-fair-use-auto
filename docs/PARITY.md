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

## Vehicles (M4)

| Feature | Original | AmiGTA1 | GFA | M | Evidence / notes |
|---|---|---|---|---|---|
| Car handling (two-tyre body, style physics) | ✓ | full | — | M4 | `gta_vehphys.c` |
| Wall resolve, ramps, layers, jumps | ✓ | full | — | M4 | `gta_vehphys.c`, `gta_main.c:790-805` |
| Damage deltas, fire, explosions, wrecks | ✓ | full | — | M4 | |
| Enter/exit by door, drag out driver, bikes | ✓ | full | — | M4 | |
| Tank: handling | ✓ | full | — | M4 | engine cut, `gta_vehphys.c:395` |
| Tank: turret and cannon | ✓? | none | — | M4 | not modelled anywhere in AmiGTA1 |
| Boats: driving on water | ✓? | none | — | M4 | placed as an object only, `gta_traffic.c:1152` |
| Cars sinking; drowning | ✓ | partial | — | M4 | `gta_main.c:2253` |
| Helicopter (scripted escape) | ✓ | none | — | M4/M7 | filtered from traffic, `gta_traffic.c:1186`; drivable? |
| RC car / map-placed specials | ? | partial | — | M4 | `gta_traffic.c:1152` |
| Trains: routes, stops, boarding, crush | ✓ | partial | — | M4 | cuts corners on the chord, not the bogie path |
| Trams | ✓ (static) | full | — | M4 | the original's trams stand still |
| Horn / pull-over look-ahead | ✓ | none | — | M4 | `gta_main.c:3957` NOT PORTED |
| Bombs: timed, speed | ✓ | full | — | M4 | `gta_main.c:4266` |
| Bombs: tanker (type 4, five blasts) | ✓ | none | — | M4 | `gta_main.c:4272` |
| Chain explosions of wrecks | ✓? | none | — | M4 | `gta_weapon.h:34` |
| Siren/lights toggle in emergency vehicles | ? | partial | — | M4 | |

## Living city (M5)

| Feature | Original | AmiGTA1 | GFA | M | Evidence / notes |
|---|---|---|---|---|---|
| Traffic spawn/despawn, lanes, junctions, lights | ✓ | full | — | M5 | view-dependent spawn ring to fix |
| Junction wobble, corner landing error | ✓ | partial | — | M5 | `gta_traffic.c:5871`, `:5666` |
| Collisions between cars (SAT + impulse) | ✓ | full | — | M5 | `gta_traffic.c:8067` |
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
