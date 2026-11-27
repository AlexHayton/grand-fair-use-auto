# Milestone 6b: DOSBox-X Measurement & Verification Checklist

Tracks every measurement and verification probe run against the 1997 DOS original (`GTADOS/GTA8.EXE`) under `tools/dosprobe`. Tick boxes as each item is measured, record the probe script path and observed numbers in place, and summarize the resulting changes in `IMPLEMENTATION_PLAN.md` and `docs/PARITY.md`.

### Probe Strategy: Overlay Test Maps

Because `tools/dosprobe/run.py` mounts the owner's game folder under DOSBox-X with a scratch `-t overlay` layer on top, placing generated files into `<overlay>/GTADATA/` lets unmodified `GTA8.EXE` boot directly into controlled probes on Level 1 without touching original game data or committing copyrighted assets:

1. **Synthetic `CmpBuilder` `.CMP` + custom `MISSION.INI`:** Generate tiny, noise-free test arenas (shooting range with crates/wall/ped/car; straight runway + one lit crossing; water basin + shoreline + dock; tank/RC/helicopter/`SPRAY` arena) using `gfa_formats::cmp::CmpBuilder` (adding minimal `locations`, `District`, valid style lid tile, and block-dedup helpers). The exact same `.CMP` file can be run in both `GTA8.EXE` and `gfa-tool` side-by-side.
2. **`MISSION.INI`-only overlay on real city maps (`NYC.CMP` / `SANB.CMP` / `MIAMI.CMP`):** Override only `GTADATA/MISSION.INI` when full city traffic, real intersections, or level-specific ped tables (Hare Krishnas / Elvises) are needed, placing `[1]`'s `PLAYER` spawn and probe vehicles/peds right beside each other.
   `tools/dosprobe/level_overlay.py --section N --player x,y,z [--donowt labels]` copies a real level's section (its own map, cars, peds and script) into the level-1 slot, moves the player's start, and can blank chosen commands; e.g. `--section 202 --player 248,132,4` starts beside Vice City's kill-frenzy tank. `tools/dosprobe/cmp_patch.py --map MIAMI.CMP --field x0-x1,y0-y1 --out DIR` adds a copy of the city map to the same overlay in which a rectangle's road is field, with no traffic arrows. Traffic does not spawn on it or drive into it, and it still looks like road. Patched columns and blocks are appended, so the rest of the map is untouched.
3. **Declaration gotchas found in M6b:** `PLAYER`'s first number is the id of his start car (`85 1 (6,9,1) PLAYER 84 256` names the `PARKED` car 84 beside him). With a `PLAYER 1` that names no car and is declared *before* the `PARKED` cars, the car he gets into boards but will not drive, so the synthetic arenas declare the player after their cars. A `CARTRIGGER`'s car id sits in the trigger's radius slot around (0,0), so it needs a large id (the levels use 2500, 3000) to cover the map. A declaration's `z` counts layers down from the top of the six-layer map: `z = 5 −` the map layer of the lid the thing stands on. Ground lids at layer `1` give `z=4`; Vice City's own telephone at `(159,36,3)` stands on a lid at layer `2`. In `gfa-tool map --at`, the ground type shows in the air block *above* the lid, so read the layer from the lid, not the type. A vehicle model must be in the level's style: `PARKED 31` (Liberty City's Bulldog) on Vice City stops `GTA8.EXE` with "Error 78.83: create car failed on type 31" while the map loads (`gfa-tool style … --cars` lists a style's models). A declaration's angle runs `0` south, `256` east, `512` north and `768` west. Seen on Vice City's tanks: `512` points the barrel up the screen and the level's own `768` points it west. An angle of `0` faces south, not north.

**Frame-Rate & Scale Calibration (`tools/dosprobe/scripts/fps_cal.probe`, `analyze_burst.py`):**
- VGA refresh runs at `70.081 Hz` (`14–15 ms`/vsync); `GTA8.EXE`'s game simulation is frame-limited to **`23.333 Hz` (`3` VGA frames = `42.86 ms`/game frame)**.
- Swept DOSBox-X `--cycles` across `60000` (`20.408 FPS`, occasional 6–7 VGA frame drops), `150000` (`22.237 FPS` incl. initial key delay; `28/33` intervals exact 3 VGA frames, steady `23.35–23.79 FPS` in `p_auto`/`fl_single`/`fl_auto`), and `250000` (`21.784 FPS`, already plateaued): **use `--cycles 150000`** as the calibrated plateau.
- Block ruler scale (`640×400`, ground layer `z=1`): `2` blocks = `216 screen px` -> **`1 block (32 world px) = 108.00 screen px`** (**`3.375 screen px / world px`**; independently confirmed by player-to-wall distance `2.5 blocks = 271.5 screen px`). Raw `640×400` VGA captures are exact `2×` pixel-doubled `320×200` (`54.33 × 45.33` native px/block across `3` tiles: `163 × 136`, exact `6:5` CRT pixel aspect ratio).

---

## 0. Harness Setup (`tools/dosprobe`)

- [x] **Patched DOSBox-X build (`tools/dosprobe/build/dosbox-x`)**
  - Upstream DOSBox-X (`322a07a`) built from source on macOS ARM64 with SDL2 and `gfa_probe.{h,cpp}` (`PIC_Ticks` scripted input, frame capture, RAM `memdump`, `savestate`/`loadstate`, clean `quit`).
  - **Probe script:** `tools/dosprobe/build.sh`
  - **Measured:** Compiled cleanly (`DOSBox-X version 2026.10.01 SDL2`).
  - **Follow-up / status:** Done.
  - **Windows continuation:** Built the pinned source with MSYS2 MinGW and the probe hooks. `windows_smoke3` completed four frame captures and a 1 MiB RAM dump with clean scripted exit. Windows builds use `dosbox-x.exe`; the original macOS determinism results above have not yet been rechecked on Windows.

- [x] **Headless `GTADOS/GTA8.EXE` boot & capture check**
  - Verify `SDL_VIDEODRIVER=dummy` + `output=surface` boots `GTA8.EXE` via overlay mount, writes `.ppm`/`.png` captures with correct RGB channel order, and dumps guest physical RAM without mutating `GFA_DATA`.
  - **Probe script:** `tools/dosprobe/scripts/boot_smoke.probe`
  - **Measured:** `smoke1` captured `boot_2500` (720×400), `boot_4500` (720×400), `boot_7500` (640×480), `boot_10500` (640×480 GTA character-select screen, accurate colours), and `ram_sample.bin` (`1,048,576` bytes, `261,349` non-zero) at `tick=10800`. Added `ffmpeg`/`sips` fallback in `tools/dosprobe/run.py` for `--png`.
  - **Follow-up / status:** Done.

- [x] **Cross-run determinism verification (`smoke1` vs `smoke2`)**
  - Re-run `boot_smoke.probe` cleanly into `tools/dosprobe/out/smoke1` and `smoke2` with `--png` and compare `shasum -a 256` across all `.ppm`, `.png`, and `.bin` outputs.
  - **Probe script:** `tools/dosprobe/scripts/boot_smoke.probe`
  - **Measured:** Pinned `synchronize time=false`, `fastbioslogo=true`, `startbanner=false`, and `-fastlaunch` in `tools/dosprobe/run.py`, plus `gfa_probe_init_time()` (CMOS/RTC + host `srand`) and `gfa_probe_seed_rand()` (`DOS_CreateTempFile` `initRand()`, eliminating the 8-byte DOS/4GW swapfile diff). Across `smoke1` and `smoke2` (`--run-name smoke`): all 4 `.ppm` frames, all 4 `.png` frames (`boot_10500.png` SHA-256 `ef78675b1f3bb099c11e65b56488c7f23c362edfa84332d7aa24a056beb2b661`), `ram_sample.bin` (`1,048,576` bytes, SHA-256 `619e4678172c6ecb827d8a1902a2c8ffa0fd923fba5f9be63c74ef02f92cf1a6`), and every `probe.log` tick (`2501`, `4513`, `7512`, `10512`, `10600`, `10800`) and CPU cycle count (`217364353`) match bit-for-bit.
  - **Follow-up / status:** Done.

- [x] **Overlay test-map boot (`--overlay` + `CmpBuilder` `.CMP` + custom `MISSION.INI`)**
  - Add `--overlay <dir>` support to `tools/dosprobe/run.py` and add minimal `locations`, `District`, and block-dedup helpers to `gfa_formats::cmp::CmpBuilder`, then verify unmodified `GTA8.EXE` boots straight into a synthetic Level 1 test arena via `-t overlay`.
  - **Probe script:** `tools/dosprobe/scripts/arena_smoke.probe` (generated with `cargo run -p gfa-tool -- probe-overlay arena --out tools/dosprobe/build/overlays/arena`)
  - **Measured:** Added repeatable `--overlay DIR` staging in `tools/dosprobe/run.py` (with pinned deterministic mtimes) plus block/column deduplication and `sample_number`/`route`/`police_station`/`hospital`/`fire_station`/`district` builders on `gfa_formats::cmp::CmpBuilder`. Unmodified `GTADOS/GTA8.EXE` booted `[1]` onto `TEST.CMP` (`262,389` bytes, `6` deduplicated blocks) + custom `MISSION.INI` (`288` bytes) with player at `(127,128)` facing east toward parked model-31 car at `(129,128)`; holding `up` for `800 ms` shifted the background viewport `14` px (`mean abs err=0.00`). Re-running into `arena1` and `arena2` (`--run-name arena --png`) produced bit-identical SHA-256 across all 7 `.ppm`/`.png` frames (`arena_start.png` `ca6432708a35ff5874721325acf524241fadc7329d368a833f1ffbf320a1b880`, `arena_moved.png` `e0e2c4807ddfbc7922e08054e699010d1822e8e121140d2e14a79854eb8acc7f`), `ram_sample.bin` (`e0a691a8b13584097afd29054237735642426df4dce850764bdd52fd409cedf0`), and `probe.log` ticks, leaving the source overlay directory and `GFA_DATA` untouched.
  - **Follow-up / status:** Done. Ready to probe Section 1 (weapon fire rates & projectile speed) on synthetic/overlay arenas.

---

## 1. Weapons & Combat

- [x] **Weapon fire rates & projectile speed** *(user observation: weapons fire too fast)*
  - Measure shots/sec and frame/ms spacing while holding Fire for **pistol**, **machine gun**, **rocket launcher**, **flamethrower**, and **fist punch**, plus projectile travel speed and flame puff lifetime.
  - Previous code (`crates/gfa-sim/src/weapons.rs`): `Arms::tick` ran at 50 Hz and decremented `cool` before checking `self.cool == 0`, so `cooldown = 1` (machine gun & flamethrower) fired on **every 50 Hz tick = 50 shots/sec**; pistol (`cooldown = 10`) fired at **5.0 shots/sec**; rocket (`cooldown = 20`) fired at **2.5 shots/sec**; `Weapons::tick` also advanced shots at 50 Hz (`BULLET_SPEED = 15`, `BULLET_LIFE = 20`, `ROCKET_SPEED = 10..15`, `FLAME_FRAMES = 10`).
  - **Probe script:** `tools/dosprobe/scripts/weapons_rate.probe` (on `shooting` preset via `gfa-tool probe-overlay shooting`), `tools/dosprobe/scripts/weapons_open.probe`, `tools/dosprobe/scripts/weapons_ammo.probe` (`--cycles 150000`; determinism verified bit-for-bit across `weapons_det1` vs `weapons_det2`)
  - **Measured:** All game-frame timings step at **`23.333 Hz` (`42.86 ms`/game frame)**, NOT `50 Hz`:
    - **Fist punch:** `6` states (`5` active sprite phases + `1` idle) × `2` game frames = **`12` game frames (`514 ms` cycle, `1.94 punches/sec`)**.
    - **Pistol:** `1` shot every **`10` game frames (`428 ms`, `2.333 shots/sec`)**; HUD decremented `99 -> 92` (`1` unit/shot). Bullet travels **`25.71 screen px/frame` = `7.62 world px/frame` (~`15` half-px per `23.333 Hz` frame, `~170 wpx/s`)**.
    - **Machine gun:** Fires **`1` bullet every `23.333 Hz` game frame (`42.86 ms`)** at **`~7.6 wpx/frame`** (`15` half-px), pausing **`1` game frame** when shot #1 frees its slot/expires (observed at frame `11`/`22`/`33` against wall `2.5` blocks east, and at frame **`20` (`t=24370`, `+813 ms` after first bullet)** in open west corridor, confirming **`BULLET_LIFE = 20` game frames (~`857 ms`)**; effective sustained open-air rate `~22.2 shots/sec`, or `23.333 Hz` per frame vs `gfa-sim`'s prior `50 Hz`). Non-cheat `AMMO_PER_UNIT = 5` verified in `weapons_ammo.probe` (`weapons_ammo4`, cycling `x` twice from Flame to MG): firing `mg_crate` decremented the HUD `20 -> 14` (`-6` units) at `5`-game-frame intervals (`t = 27147, 27375, 27575, 27803, 28017, 28231`, mean `216.8 ms`/unit).
    - **Rocket launcher:** `1` rocket every **`20` game frames (`856.5 ms`, `1.167 rockets/sec`)**; HUD decremented `99 -> 95`. Flight step **`+22` screen px** on frame 1 (`~6.5 wpx`, `13` half-px), then **`+26..+28` screen px/frame** (`~8.1 wpx/frame`, `~15–16` half-px) until wall burst/fires.
    - **Flamethrower:** Single puff (`fl_single`) visible for **`9` game frames (`385 ms`, gone frame `10`)**, moving **`~12.2 screen px/frame` (`3.6` world px/frame = `~7` half-px)**. Continuous fire (`fl_auto`) emits **`9` consecutive game frames ON + `1` game frame OFF** (`428 ms` period = **`21.0 puffs/sec`** vs `gfa-sim`'s prior `50 puffs/sec`). Non-cheat `AMMO_PER_UNIT = 5` verified in `weapons_ammo.probe` (`weapons_ammo4`): firing `fl_crate` decremented the HUD `10 -> 04` (`-6` units) at exact `5`-game-frame intervals (`gf = 5, 10, 15, 20, 25, 30`; `t = 24493, 24707, 24936, 25135, 25349, 25563`, mean `214.0 ms`/unit = `5 × 42.86 ms`).
  - **Follow-up / status:** Done. Added `clock::weapons() = Ratio(7, 15)` (`23.333 Hz` on `50 Hz` sim) + `Ratio::phase()` in `crates/gfa-sim/src/clock.rs`; updated `Arms::tick` (`fire_queued` latch + `gun_step`-clocked cooldowns/firing, keeping `punch_left` at `50 Hz`) and `Weapons::tick` (gating projectile `fly` and splat countdowns on `gun_step`, keeping `burn_fires`/`blasts`/`wreck_sweep` at `50 Hz`) in `crates/gfa-sim/src/weapons.rs`; stepped and hashed `weapons_clock` in `crates/gfa-sim/src/world.rs`; added `weapon_fire_rates_projectile_speed_and_splat_lifetime_match_original` in `crates/gfa-sim/tests/synthetic.rs`; patched AmiGTA1 oracle via `tools/oracle/weapons_clock.pl`, `tools/oracle/build.sh`, `tools/oracle/fleet.h`, `tools/oracle/walktrace.c`, and `tools/oracle/drivetrace.c`. Verified with `cargo test --workspace`, `cargo wasm-check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check`, `cargo data-test`, and `cargo oracle-test`.

- [x] **Bullet splat lifetime**
  - Measure how long bullet impact splats remain visible on the map (`SPLAT_TICKS = 8` provisional from AmiGTA1 in `crates/gfa-sim/src/weapons.rs`; `IMPLEMENTATION_PLAN.md` decision log, `docs/PARITY.md`).
  - **Probe script:** `tools/dosprobe/scripts/weapons_rate.probe` (`p_single` vs wall) and `tools/dosprobe/scripts/weapons_open.probe` (`car_splat` vs unobstructed Bulldog car)
  - **Measured:** Against an unobstructed parked car (`car_splat_0026..0039`, gone at `0042`), the blue/cyan impact spark stays visible for **`5` game frames (`t=20675..20861`, gone `t=20903` = `~214–228 ms`)**. Against an east building wall (`x=130`), the first 3 frames are occluded behind the 3D wall overhang and the last `2` spark/smoke frames (`p_single_0043..0046`, gone `0049` = `~86 ms`) spill west past `x=592`.
  - **Follow-up / status:** Done. Set `SPLAT_TICKS = 5` in `crates/gfa-sim/src/weapons.rs` and gated splat countdown in `Weapons::tick` on `gun_step` (`23.333 Hz` ≈ `214 ms`), mirrored in `tools/oracle/weapons_clock.pl` and covered in `crates/gfa-sim/tests/synthetic.rs` (`weapon_fire_rates_projectile_speed_and_splat_lifetime_match_original`) and `cargo oracle-test`.

- [x] **Explosion score attribution**
  - Verify whether *every* car blowing up pays the player `100` points regardless of who caused it, or only player-caused / nearby blasts (`IMPLEMENTATION_PLAN.md` M6b & decision log).
  - **Probe script:** `tools/dosprobe/scripts/combat_expl_score1.probe`, `tools/dosprobe/scripts/combat_expl_score1b.probe`, `tools/dosprobe/scripts/combat_expl_score1c.probe` (`combat_score` overlay); `tools/dosprobe/scripts/combat_expl_score2.probe`, `tools/dosprobe/scripts/combat_expl_score2c.probe` (`combat_score_nonplayer` overlay); owner-run interactive `combat_tanker_npc` overlay (`--cycles 150000`)
  - **Measured:**
    - **Player-caused car kills & chain explosions (settled in DOSBox-X, consistent across `PARKED` & `StartupObject` Bulldogs `model 31`, `dmg=100`):**
      - `1` Rocket kill pays **`+100`** only (`combat_expl_score1c` `after_r1_parked`: `0 -> 100`; `combat_expl_score1` `rocket_hit_0024`: `400 -> 500`, popup `100`).
      - Adjacent car chain-exploding within the 13-frame streak window pays **`+200`** (`streak 2`, confirmed in `combat_expl_score1` `rocket_hit_0054` `500 -> 700` popup `200`, and `combat_expl_score1c` `startup_p16_0065` popup `200`).
      - `16` Pistol shots to wreck + explode a Bulldog: shots `1..15` pay `+10` each (`+150`; `1b` `after_15 = 150`, `1c` `startup_after_15 = 250`); lethal shot `16` awards **`+150`** (`startup_p16_0023` popup `150`), followed `1` game frame later by **`+100`** (`startup_p16_0026` explosion popup `100`) = **`+250`** single car (`150 -> 400` in `1b`) or **`+450`** with an adjacent chain (`250 -> 700` in `1c`).
    - **NPC-caused vehicle explosion (visually confirmed by owner in DOSBox-X):** In the `combat_tanker_npc` arena, the player drove the normal `StartupObject` Tanker toward a rocket-armed hostile ped, got out, and the NPC's rocket destroyed the tanker. The HUD score did not increase. This confirms that an NPC-destroyed vehicle awards the player **0 explosion score**. The controlled setup also avoids the invulnerable script-spawned parked cars that blocked the earlier attempts.
    - **Original-code cross-check:** Both explosion-hit scoring (`0x7d85e` in `0x7d514`) and car-wreck scoring (`0x547cf` in `0x547b0..0x5490d`) guard score/heat with `cmp attacker, -1; jle skip` and propagate `owner_id` through `car+0x132` for chain blasts, consistent with the observed zero award for a non-player attacker.
    - Earlier `combat_expl_score2c` and `combat_expl_score_script.probe` runs did not confirm a car explosion: the target survived or did not visibly spawn. They are superseded by the successful `combat_tanker_npc` observation.
  - **Follow-up / status:** Done. Fleet cars carry `player_kill`, set when the player's bullet, flame, rocket, blast or ramming harms them (and when his own bomb goes off); `Weapons::wreck_sweep` pays the `100` only for such a car and passes the owner on to the blasts it makes, so a chain he started stays his (`car+0x132`). A wreck somebody else caused pays nothing. Covered by `a_car_blowing_up_pays_only_when_the_player_caused_it` (the oracle's player-caused cases are unchanged). The existing direct `+100` on a rocket hit is kept to match AmiGTA1; a lone rocket kill pays `+100` in `GTA8.EXE` but `+200` in `gfa-sim` (hit plus wreck), and the streak (`+200` for a chain car) and the `+150`/`+100` pistol-kill split are not modelled yet. Nor is the far-wreck rule found with the tank, that a wreck 8 or more blocks from the player pays nothing. Measured since: a streak doubles, 100, 200, 400, 800 … (tank entry, below).

- [x] **Fire-key latch across `WASTED`**
  - Verify whether holding Ctrl continuously through `WASTED` and hospital respawn immediately fires/punches on respawn or requires releasing and re-pressing Ctrl (AmiGTA1 latches on key events and clears on `WASTED`; `gfa-sim` previously read held `Buttons`; `IMPLEMENTATION_PLAN.md` decision log).
  - **Probe script:** `tools/dosprobe/scripts/combat_wasted_latch.probe` (`combat_water` overlay, `--cycles 150000`; partial synthetic run only)
  - **Measured (not pursued for DOS parity):** Synthetic `combat_wasted_latch3` exited at respawn (`t=22767`, `respawn_held_0123.png`) with `Error 174.219: No available hospital release location : 0`. Per owner decision, original DOS parity is intentionally not sought for this item (planned `combat_wasted_nyc` probe dropped), adopting AmiGTA1's key-event latch clearing on `WASTED`.
  - **Follow-up / status:** Done. Event-latched `arms.fire_held` via `World::prev_fire` in `crates/gfa-sim/src/world.rs` so `WASTED` clears `fire_held` and a continuously held `Buttons::FIRE` stays disarmed across hospital and immediate respawns until released and re-pressed; cleared `fire_held = 0` on `WASTED` in `tools/oracle/walktrace.c` (`rocket_close.txt` parity); covered by `fire_key_latch_clears_across_wasted_until_released` in `crates/gfa-sim/tests/synthetic.rs`.

- [x] **Firing while drowning**
  - Verify whether a drowning player in water can fire weapons (`IMPLEMENTATION_PLAN.md` decision log).
  - **Probe script:** `tools/dosprobe/scripts/combat_drown.probe` (`combat_drown2`), `tools/dosprobe/scripts/combat_drown_held.probe` (`combat_drown_held2`, `combat_water` overlay, `--cycles 150000`)
  - **Measured:** **Allowed while drowning underwater.** `combat_drown2` fired Rocket `99 -> 98` while submerged (`t=22858`); `combat_drown_held2` fired continuous MG bullets southward underwater (`drown_mg_held_0057..0129`) right up until `WASTED!` appeared at `drown_mg_held_0133` (`t=23101`), stopping new projectile spawns once `WASTED!` began (`0135..0150` only show in-flight bullets clearing).
  - **Follow-up / status:** Done. `GTA8.EXE` allows weapon fire while drowning and stops new shots when `WASTED!` begins. `gfa-sim` allows firing in `Doing::Drowning` through `World::trigger`; `drowning_player_can_fire` covers this behavior.

- [x] **Ped counter-punch**
  - Measure what a punched ped's counter-punch (last two steps of AmiGTA1's 13-step counter in `gta_peds.c:1675`) does to the player: damage, knockdown/stun, and animation (`crates/gfa-sim/src/peds.rs`, `docs/PARITY.md`).
  - **Probe script:** `tools/dosprobe/scripts/combat_counterpunch.probe`, `tools/dosprobe/scripts/combat_counterpunch2.probe`, `tools/dosprobe/scripts/combat_counterpunch3.probe`, `tools/dosprobe/scripts/combat_counterpunch3_armor.probe` (`combat_punch` overlay, `--cycles 150000`)
  - **Measured:** In the stationary `combat_punch` arena, the owner manually confirmed that the NPC's counter-punch knocks the player down for about **2 seconds**. Health stays unchanged without armor; with the `suckmyrocket` armor setup, the armor display stays at **3**. DOSBox-X captures and RAM samples also leave health at `100` before and after the interaction.
    - **`GTA8.EXE` read-only inspection (`0xf82ec..0xf84b6`):** Global `punch_wheel` (`0x28a43d`) increments per punch check (`0xf8306`) and wraps `> 12 -> 0` on contact (`0xf839a`). Punch box is `6×6` at `8` units ahead (`0x9ccb4`/`0x9dc94`). Steps `0..9`: target ped knocked down (`anim = 0xaf`, `speed = 0`, punch sfx `0x10`). Step `10`: unconditional counter-punch — target ped turns `(player.angle + 512) & 1023`, plays punch anim `0xa9` (`state 0x14`), knocks the player down with `anim = 0xaf`, `speed = 0`, `player+0x41 = 0`, plays sfx `0x10`, and resets `punch_wheel = 0` immediately; **zero HP damage, zero armor loss, zero lives lost**. Steps `11..12` (reachable via whiffed swings advancing `punch_wheel` past `10`) whiff on normal peds.
  - **Follow-up / status:** Done. `Peds::punch` returns `Punch::{Nobody, Down, Counter}`; the wheel is 11 long (counter on step 10, reset at once). On a counter the ped faces the puncher and `World::knocked = KNOCKED_TICKS (100 ticks, 2 s)` stops him walking, firing, boarding and jumping, drawn as the lying frame (`Anim::SlideUnder`; the ped's punch animation `0xa9` and the fall frame `0xaf` are not modelled). Covered by `the_tenth_punch_that_lands_is_countered_and_puts_the_player_down_two_seconds` and `tools/scripts/counter_punch.txt`; the oracle runs `tools/oracle/ped_counterpunch.pl` (`cargo oracle-test` matches).

- [x] **Type-4 tanker bomb global**
  - Verify behaviour of the tanker's type-4 damage bomb global (`gta_main.c:4272`, `IMPLEMENTATION_PLAN.md`).
  - **Probe script:** `tools/dosprobe/scripts/combat_tanker_unmanned.probe`, `combat_tanker_unmanned2.probe`, `combat_tanker_unmanned3.probe` (`combat_tanker` overlay); `combat_tanker_drive_plain.probe`, `combat_tanker_drive_bomb.probe`, `combat_tanker_drive_bomb2.probe`, `combat_tanker_drive_bulldog.probe` (`combat_tanker_drive` overlay); owner-run interactive `combat_tanker_bomb` overlay (`gfa-tool probe-overlay combat_tanker_bomb`, `--cycles 150000`; `combat_tanker_bomb_board.probe` checks boarding), all `--cycles 150000`.
  - **Measured (owner-observed in DOSBox-X, `combat_tanker_bomb`):**
    - **Unmanned `SETBOMB 4` Bulldog, rocketed:** four blasts first (the body's corners), then one at the centre = **5 blasts** (the bomb's pattern), not the 3 of a normal Bulldog wreck.
    - **Unmanned `SETBOMB 4` Tanker, rocketed:** the same four corner blasts, then one at the centre.
    - **Plain Tanker (no bomb), rocketed:** **5 blasts in a straight line along the tanker** (centre, front, rear, and the two quarter-length ones).
    - **Driven `SETBOMB 4` Tanker B and Bulldog C, driven into the wall:** both go off with the same large bomb blast (corners, then centre), whatever the model.
  - **Read-only inspection (`GTA8.EXE`, earlier):** global `0x54e96` = `10` set once at `0x52012`; `car+0xf7 > 10` at `0x571bc` calls `0x7cad4` (5 blasts: centre + 4 corners) and clears `car+0x9a`; a minor wall hit adds `+1` to `car+0xf7` (`0x52eb4`). Correction: the probe shows the bomb also goes off in an unmanned parked car once rocket damage passes the threshold, so it is not limited to driven cars.
  - **Not tested:** whether a gentle nudge alone passes the threshold (a wall crash did); MG fire on an unmanned bombed car left it intact (`53` hits) and is unchanged.
  - **Follow-up / status:** Done. `gfa-sim` now fires a type-4 bomb above `damage > 10` (`BOMB4_DAMAGE`, was `> 0`) in driven and unmanned cars, and bomb detonations (types 2, 4, 6) blow up as `Weapons::bomb_blasts` (4 corners, then centre; no wreck blasts) instead of the wreck pattern; the plain Tanker keeps its 5-in-a-line wreck.

---

## 2. Entering/Exiting & On-Foot Interactions

- [x] **Car get-in animation** *(known bug — owner reports entering through the wrong side of the door)*
  - Frame-by-frame capture against `GTA8.EXE`: door target point, walk-in path, ped sprite frames (`ENTER_SEQ` and `EXIT_FIRST..+EXIT_FRAMES`), bike mount/dismount, and door-opening delta (`DOOR_TICKS = 40`, `DOOR_OPEN = 20`).
  - **Probe script:** `tools/dosprobe/scripts/getin_door.probe`, `tools/dosprobe/scripts/getin_far.probe`, `tools/dosprobe/scripts/getin_cop.probe`, `tools/dosprobe/scripts/getin_bike.probe`, `tools/dosprobe/scripts/vault_bus.probe` (`getin` and `vault` overlays via `gfa-tool probe-overlay`, `--cycles 150000`)
  - **Measured:**
    - **Root cause (`CarDoor` axis order):** Across all 38 `STYLE001.G24` vehicles, `CarDoor` stores `rpy` first (longitudinal along body, `+` forward) and `rpx` second (lateral, `+` driver's left) — e.g. Bulldog `car[23]` `[(rpx=+25, rpy=-1, obj=9, delta=7)]`, Cop `car[04]` `[(+23, +1), (+26, -8)]`, Coach `car[08]` `[(+27, +50), (-9, 0, 0, 13)]`, and Bikes `car[03]`/`car[29]` `[(+8, +2, 0, 0)]` (not empty doors). `gfa-sim` (`cars::door_point`, `cars::door_pos`, `peds::pull_place`) mistakenly read `d.rpx / 2` as `along`, placing the player `+12.5` GFA-wpx (`+25` GTA1-spx) forward beside the front fender in front of the open door blade instead of in the doorway (and mid-body instead of the front cab on Coach).
    - **Draw order & baked canvases:** `GTA8.EXE` draws the car sprite + open door delta (`6..9`) **over** the entering/exiting ped (and over a player sliding underneath), whereas vaulting (`091..094`) and bike mount/ride/dismount (`080..088`) draw the ped **over** the vehicle. Door-entry (`016..033`) and bike (`080..088`) canvases already bake the lateral offset (`+2..+5` px), so raw entity `can` remains at the doorway (`d.rpy / 2`) rather than dragging toward `(cx, cy)`.
    - **Bulldog enter (`enter_east`, `face = 128` SOUTH at `~23.333 Hz`):** `026` (`g06..g10`, `h = face`, `can = (-2..-4, +22..+24) spx`, door `closed -> d1 -> d2`) -> `027` (`g11..g14`, door-pull frame, **`h = face - 32`**, `can = (-6, +22) spx`, door `d3 -> d4`) -> `025` (`g15..g16`, `h = face - 32`, `d4`) -> `029..033` (`g17..g24`, `2` gf each, `h = face - 32`, closing `d3 -> d2 -> d1`) -> seated (`g25`, `closed`). Door schedule: `d1(2gf) -> d2(2gf) -> d3(2gf) -> d4(6gf) -> d3(2gf) -> d2(2gf) -> d1(2gf) -> closed` (`18 gf ≈ 771 ms`, matching `DOOR_TICKS = 40` at `50 Hz` with `DOOR_OPEN = 20` centered in `d4`). Same `face - 32` turn confirmed on Cop (`enter_cop`, `64 -> 34`); Coach (`bus_enter`) enters the front-east cab at `can = (+48..+49, +24.7) spx`.
    - **Bulldog exit (`exit_near`, `face = 128` SOUTH):** `8` two-frame steps `016..023` (`~600 ms`, door `d1, d2, d3, d4, d4, d4, d3, d2->d1 -> closed`, `can` shifting `(-1,+21) -> (-5,+23) spx`), finishing at `098` Stand facing **`face - 32`** clear of the body (`(-6, +25) spx`).
    - **Bike mount & dismount (`enter_bike` / `exit_bike`, `face = 64` EAST):** Mounts from driver's left (`NORTH`) in `3` frames `081 -> 082 -> 083` (`2` gf each ≈ `257 ms`) -> `084` (`BIKE_SIT`); dismounts on driver's left in `3` frames `086 -> 087 -> 088` facing **`face - 64` (`0` NORTH) immediately** (`can = (+2.9, +9.2..+13.4) spx`) -> `098` Stand at `(+2.9, +14) spx`.
  - **Follow-up / status:** Done. Fixed `d.rpy / 2` longitudinal and `d.rpx` lateral door geometry in `crates/gfa-sim/src/cars.rs` (`door_point`, `door_pos`) and `crates/gfa-sim/src/peds.rs` (`pull_place`); updated `ENTER_SEQ = [26,26,27,27,25,29..33]`, `door_delta` schedule, `EXIT_FRAMES = 8` (`016..023`) + `exit_door_delta`, and `BIKE_ENTER_FIRST = 81` / `BIKE_EXIT_FIRST = 86` / `BIKE_FRAMES = 3` in `crates/gfa-sim/src/cars.rs`; updated cab-level lateral slide, `face - 32` / `face - 64` headings, and `World::player_under_car()` in `crates/gfa-sim/src/world.rs`; updated vehicle/player painter order in `crates/gfa-app/src/scene.rs` and `crates/gfa-tool/src/run.rs`. Covered by `door_geometry_and_enter_exit_sequences` in `crates/gfa-sim/tests/synthetic.rs`.

- [x] **Slide under tall vehicles & vault edge cases**
  - Verify whether pressing `Space` while running at a bus/tall vehicle from outside slides under it, far-side `Enter` on a tall vehicle, `Space` vaulting / `Enter` on a flush-wall car, and blocked-door exit (`Bulldog` SOUTH, `Coach` SOUTH, `Bulldog` NORTH, L-corner `Bulldog`) (`IMPLEMENTATION_PLAN.md` decision log, `docs/PARITY.md`).
  - **Probe script:** `tools/dosprobe/scripts/vault_wall.probe`, `tools/dosprobe/scripts/vault_bus.probe`, `tools/dosprobe/scripts/vault_enter_wall.probe`, `tools/dosprobe/scripts/vault_bus_far.probe`, `tools/dosprobe/scripts/vault_bus_far2.probe`, `tools/dosprobe/scripts/vault_blocked_car.probe`, `tools/dosprobe/scripts/vault_blocked_bus.probe`, `tools/dosprobe/scripts/vault_blocked_north.probe`, `tools/dosprobe/scripts/vault_blocked_corner.probe` (`vault` and `blocked_exit` overlays via `gfa-tool probe-overlay`, `--cycles 150000`)
  - **Measured:**
    - **Slide under tall vehicles (`bus_space_touch`, Coach `vert = 24`):** Pressing `Space` flush outside the Coach's flank dives/slides straight underneath (`SLIDE_UNDER = 41` at `g03..g04`, hidden under roof `g05..g10`, emerging clear on the west flank `g12..g13`, total `~470 ms`). (`slide_bus` on open road also dives forward briefly.)
    - **Far-side `Enter` on tall vehicles (`vault_bus_far2`):** Does NOT vault over `vert = 24` Coach; ped walks north around the nose to the front-east cab door (`doors[0]`) and gets in.
    - **Flush-wall `Space` vault & `Enter` (`vault_wall`, `vault_enter_wall`):** Running east + `Space` onto an east-flush Bulldog climbs onto the roof (`091..094` EAST), stops at the wall edge, turns north in crouched hold `094` (`g21..g29`, `h = 60 -> 30 -> 0`), and steps off north onto clear road (`g30..g35`) without entering `BUILDING`. Pressing `Enter` on the clear flank when the door is blocked against the wall is refused.
    - **Blocked-door exit (`blocked_car`, `blocked_bus`, `blocked_north`, `blocked_corner`):** Contrary to AmiGTA1's opposite-flank exit (`a - door_side * 64`), `GTA8.EXE` keeps the door shut and vaults (`091 -> 092 -> 093 -> 094`) **straight forward along `+car.face`** over the hood/front bumper on **all** vehicles: SOUTH Bulldog exits south in `~500 ms` (`along = +37..+40 spx` ahead of bumper), NORTH Bulldog exits north (`h = 0`) in `7` gf (`~371 ms`), and SOUTH Coach (`vert = 24`) also vaults forward (`h = 128`) gliding along `094` (`g08..g46`, `~1.95 s`, `along = +34.6 -> +65.0 spx`, never aborted early mid-roof). In L-corner (`blocked_corner`), driven impact against `(129,132)` rebounds `~30–40` px north before `Enter` vaults south into the post-rebound gap.
  - **Follow-up / status:** Done. Updated `cars::far_side_around`, `VAULT_HOLD_MAX = 30`, and `SLIDE_MAX_TICKS = 80` in `crates/gfa-sim/src/cars.rs`; added `Traffic::nearest_enterable`, `take_car`, `car_at_except` in `crates/gfa-sim/src/traffic/contact.rs`; updated `World::try_enter` (refuse blocked door; nose/tail detour around `!vaultable`), `World::try_exit` (blocked door vaults forward along `+car.face`), `World::vaulting` (`vault_blocked` turns off flush/L-corner walls rather than entering `BUILDING`), and `World::sliding` (`Slide` carries outside entry + wall bounce-back) in `crates/gfa-sim/src/world.rs`; updated `tools/scripts/vault.txt`. Covered by `a_blocked_door_puts_him_out_over_the_roof`, `far_side_enter_on_bus_walks_around_body`, `jump_at_bus_from_outside_slides_under`, `flush_wall_enter_refused_and_vault_turns_off_roof`, and `blocked_door_exit_forward_on_north_car_bus_and_l_corner` in `crates/gfa-sim/tests/synthetic.rs`.

---

## 3. Vehicles AmiGTA1 Doesn't Drive

- [ ] **Tank (`model 0x25`)**
  - Measure handling and turn rate vs `crates/gfa-sim/src/vehicle.rs` (`TANK_MODEL = 0x25` engine cut), independent rotating turret aiming, cannon shells on Fire (projectile speed, explosion, reload cadence, wall/car damage), armour and damage resistance, pushing/crushing cars, and where/how it spawns (`IMPLEMENTATION_PLAN.md`, `docs/PARITY.md`).
  - **Probe script:** `tools/dosprobe/scripts/combat_tank_board.probe`, `combat_tank_keys.probe` (`combat_tank` overlay); `tank_straight.probe`, `tank_fire.probe`, `tank_fire2.probe`, `tank_fire_cheat.probe` (`tank_drive` overlay); `tank_turn.probe` (`tank_turn` overlay); all `--cycles 150000`, overlays from `gfa-tool probe-overlay`. Handling read from the bursts with `tools/dosprobe/analyze_drive.py` (lane-line spacing = scale, lane-line phase = camera x, block-periodic texture masks the tank for its position and heading). Owner-run interactive `combat_tank`.
  - **Placement:** a tank from the map's object list (`StartupObject` kind `37`) or a script `PARKED 37` both board and drive. (An earlier note that a `PARKED` tank was inert was an artifact of the arena's declaration order; see the gotchas above.) The levels place tanks as `PARKED 37` / `PARKED_PIXELS 37`: three plain ones in Liberty City [2]; with a `CARTRIGGER` that starts a kill frenzy in San Andreas [102] (tanks 2500, 2530) and Vice City [202] (tank 3000) and [203].
  - **Handling (`tank_drive` / `tank_turn`, measured; gfa-sim run on an open synthetic street with the style's tank for comparison):**

    | | `GTA8.EXE` | gfa-sim (unchanged) |
    |---|---|---|
    | Top speed | `2.64` blocks/s (`84.3` wpx/s, `3.61` wpx per game frame) | `2.60` blocks/s |
    | Distance from rest after 1.5 / 2.5 / 3.5 / 4.5 s | `1.6` / `3.7` / `6.1` / `8.55` blocks | `1.64` / `3.67` / `5.99` / `8.55` blocks |
    | Coast from top speed, keys released | `3.3` blocks in `3.9` s, still creeping (`0.19` b/s) | `~3.1` blocks |
    | Reverse (Down from a near stop) | top `~1.25` b/s after `~3.5` s | top `~1.30` b/s, reached `~0.5` s sooner |
    | Left/Right at rest | no turn at all (`0.0`° in 8 s) | no turn |
    | Up+Left from rest, 8 s | `~40–48`°/s once moving, `308`° total, circle `~4.4` blocks across | `~42`°/s, `~300`° total, circle `~4.35` blocks across |

    gfa-sim's tank (AmiGTA1's physics with the engine cut) is within a few percent everywhere; no change made.
  - **Camera (bonus for Section 4):** the view zooms out in steps as the tank speeds up: `108` px/block at rest, then `106`, `101`, `96.5`, `94.7`, `89.4`, `84.9` at top speed (`×1.27`). It zooms back to `108` within `~1.7` s of releasing the throttle. At top speed the camera leads the tank by `~0.7` blocks; reversing at `1.25` b/s it zooms only to `106` with no lead. Faster vehicles are still needed for the full curve.
  - **Turret:** rotates independently of the hull: hold `Tab`, then `Left`/`Right` (owner, interactive).
    - **Rate (headless, `tank_turret_rate.probe` on `tank_range`):** `8` angle units a game frame either way, a full turn in `32` frames (`1.37` s; six turns in `8.26` s). It starts and reverses at once, and the hull does not steer while `Tab` is held.
    - **Drawing:** style objects `0x21` (the dome, its art facing south) over `0x22` (the barrel, `26` px long, pointing north), the barrel's base on the pivot and its tip `26` px out: `92` screen px with the barrel pointing east. The tank's two car sprites are its hull, the second upside down.
    - **The screen is not square:** a block is about `110` screen px wide but `91` high (the lane dashes repeat every `182` px down the screen; a `95`-pixel hull shows `129` px tall). A distance read up or down the screen at `108` px/block is `1.2` times short. The shell speeds above were read across the screen (Vice City's tank faces west) and stand.
  - **Cannon: only during a kill frenzy** (owner's lead, confirmed). Outside a frenzy no key fired anything from a boarded tank (`tank_fire.probe`, `tank_fire2.probe`, `tank_fire_cheat.probe`: `LCtrl`/`RCtrl`/`Space`/`Alt`/`Shift`/`Tab` combinations, weapon keys, keypad; with or without the `suckmyrocket` weapons). In Vice City [202] staged with `level_overlay.py` (`vc_tank`, `vc_tank_calm` overlays; `vc_tank_fire.probe`, `vc_tank_shell.probe`):
    - **The frenzy:** getting into tank 3000 fires `CARTRIGGER 7100 → proc 600`: `KILL FRENZY!` and `YOU GOT 100 SECONDS TO DO $5,000 OF DAMAGE. GET GOING!`, `FRENZY_CHECK 5000`, `KF_BRIEF_TIMED 100`, `IS_GOAL_DEAD 3000` (losing the tank fails it). Proc 600 also turns eight peds into armed attackers (`CHANGE_PED_TYPE … 22 209`): left alone they shoot the parked tank to a write-off in `~2.5` s (`FRENZY FAILED!`), and the turret swung round towards them without input. `vc_tank_calm` blanks those eight lines (`--donowt 603-610`).
    - **Fire:** `LCtrl` fires a shell along the turret (`Tab` + `Left`/`Right` aims it). The top-left counter shows `99` shells at the start of the frenzy and loses `1` per shell. Held, it fires about once a second (`97→96→95→94→93` at `~1.04`, `1.03`, `~1.15–1.2` s).
    - **Shell flight (identical on two shells):** first seen `2` game frames after the key, then per game frame `11`, `17`, `22`, `27`, `28–29` screen px at `108` px/block = `3.3`, `5.0`, `6.5`, `8.0`, `8.4` wpx/frame (`~7, 10, 13, 16, 17` half-px): a `+3` half-px/frame speed-up to `~16–17`, like the rocket's.
    - **Blast:** a large explosion like a rocket's; it kills peds, scores (`0 → 1500` on the first held burst) and raises the wanted level. Car/wall damage still to measure (the shells left the screen before bursting in `vc_tank_shell`).
    - **Range (owner, manual, `tank_range`'s first layout):** a shell fired from `18` blocks blew up the target car, flying level under a footbridge slab (Vice City `(217–222,91)`, its lid one layer above the road) on the way. So the range is at least `18` blocks, and an overhead slab does not stop the shell. A longer test (`68` blocks, on the four-lane road `x=96–99`, made field in a patched map copy so no traffic gets in the way) found more:
      - **Range:** a shell from `65` blocks destroyed the target. In practice it has no range limit, like gfa-sim's rocket.
      - **Score: a far wreck pays nothing.** In `GTA8.EXE`, a car the player wrecks pays only if it is close to him. gfa-sim pays at any distance.
        - **Owner, manual, test plan A5:** each try was a fresh `tank_range`, with the tank stopped level with a marker and one shell fired straight up the lane. The target car was destroyed every time. It paid **100 from 6 blocks** (marker 12), and **nothing from 10, 15, 20 … 65 blocks**. A wreck on screen or a few blocks off paid 100 in `tank_damage`.
        - **Cut-off: between 7 and 8 blocks.** The owner's shots bracket it between 6 and 10 blocks. The `tank_chain` total, decoded with the streak measured below, narrows it: a wreck 7 blocks away pays, one 8 away does not. Measured only straight ahead, from the tank's centre to the wreck's, with the tank standing still. gfa-sim's view half-height at rest happens to be 7 blocks (`SimView::GAME`); the two have not been compared.
        - **Taken as settled (owner, 2026-11-27):** the cut-off is a square that applies across the screen as well as up it, and a rocket's wreck follows it too. gfa-sim already does both. Still unknown: whether it counts from the player or from the camera, which leads a moving player.
        - **The rule is probably the wreck's distance, not the shell's flight time.** If the shell stopped counting as the player's after flying for a while, it would explain the same misses.
        - **Headless chain check** (`tank_chain`, `tank_chain_check.probe`): 11 cars nose to tail, 4 to 14 blocks ahead, set off by one shell at the nearest. The score reached **1500** and no more. With the doubling streak measured below, 1500 = 100+200+400+800 is exactly the first **4** cars, at 4 to 7 blocks. The car at 8 blocks would have paid 1600 and did not. The chain ran on (its later cars blew up), so the wreck's distance decides it, not the shell's flight.
        - **Ported:** a wreck pays only within 7 blocks of the player on both axes (`PAY_REACH_PX = 256`, a square, from the player's centre).
      - **Chain timing and the streak, frame by frame** (headless, `tank_chain_view`, `tank_chain_frames.probe`):
        - **Layout:** a row of 6 cars side by side, 1 block apart, across the screen 1.6 blocks ahead of the tank, captured every 21 ms (73 distinct game frames). The shell hits the middle-left car, and the score popups show each car's pay as it blows up.
        - **The hit car:** the shell bursts on it at game frame 3 after the key, and it blows up at **frame 4, one frame later**, paying 100. A car the pistol wrecks also blew up one frame after the lethal shot (`combat_expl_score1c`, above).
        - **Cars a blast writes off** blow up **10 game frames (≈430 ms) after** the car whose blast caught them. Both neighbours went at frame 14. The next pair went at frames 24 (east) and 25 (west); the one-frame split is not explained. The next went at frame 34.
        - **The streak doubles:** popups read **100, 200, 400, 800, 1600, 3200**, in the order the cars blew up, two in the same frame included. The final score was 6300. Each step (10 frames) is inside the 13-frame streak window, so the streak runs the length of the chain.
        - **Ported:** a car written off where it stands blows up on the next game frame (`cars::HIT_FUSE`), a car a blast writes off 10 frames later (`cars::BLAST_FUSE`). Fuses burn first, then the cars blow up, so the step does not depend on the fleet's order. Each wreck pays `Score::wreck`: 100, doubling along a streak whose window is now 13 game frames (`score::STREAK_FRAMES`). The oracle took `weapons_chain.pl` and `score_streak.pl` until the weapons and the score were retired from it (2026-11-27). Known gap: gfa-sim's square 32 px reach catches the hit car's side neighbours in the shell's own burst (frame 3), so they go at frame 13, not 14. The 24/25 split is not modelled.
    - **What it bursts on (owner, manual, `tank_burst`):** like the rocket in every case.
      - **Car:** the shell bursts on contact. The car's own wreck explosion follows, and that one shell writes it off.
      - **Burnt-out wreck:** the shell bursts on it, and so does a rocket.
      - **Building wall:** the shell bursts on it and leaves **one** small fire on that side of the wall. Remeasured, it burns about **22 s**. A **rocket** does the same (B5): one fire, 22 s. AmiGTA1, and gfa-sim until now, lit **three** fires for 250 ticks (**5 s** at 50 Hz). **Ported:** `WALL_FIRE_TICKS = 1100`, one fire a step back along the line. The oracle took `tools/oracle/weapons_wallfire.pl` until the weapons were retired from it (2026-11-27); `synthetic.rs` checks the fire now. The script explosion's two fires keep AmiGTA1's 250 ticks, unmeasured.
      - **Person:** the shell bursts on them and kills them at once, and the corpse does not burn. This matches gfa-sim: a direct hit is inside `BLAST_KILL_PX`, which kills, rather than the outer `BLAST_BURN_PX` ring, which sets alight.
    - **Railings and water (owner, manual, `tank_fence`, on Vice City's bridge deck at `(222,122)`):**
      - **East edge, `x=224`:** the shell bursts and leaves the 22 s fire. At deck height that block is **Building**-typed with flat sides, so it counts as a wall. gfa-sim's rocket bursts on block `(224,122,3)` as well.
      - **West railing, `x=216`:** a pavement block with flat faces of side tile `102`. The shell flies over it with no bang and on over the land and water beyond. Tile `102` is on Vice City's shot-through list (`WallTiles::gta`, `SHOT3`, from AmiGTA1), so gfa-sim agrees. This confirms that entry in `GTA8.EXE`, as the footbridge earlier confirmed tile `101`.
      - **Water:** not measured; neither shot met water at its own height. **Taken by the owner's call (2026-11-27):** a shell, like a bullet or a rocket, does not burst on water. It bursts only when it hits something. **Ported (2026-11-27):** AmiGTA1, and gfa-sim until now, stopped all three at water on their own layer, as at a wall. That only happens off a beach, where the water is level with the shooter. Now they fly on over it (the oracle took `tools/oracle/weapons_water.pl` until the weapons were retired from it, 2026-11-27). Synthetic test: the water check in `weapon_fire_rates_projectile_speed_and_splat_lifetime_match_original`.
    - **Damage (owner, manual, `tank_damage`):** like the rocket in gfa-sim, apart from the score.
      - **One shell writes off anything:** a Coach, a car (Itali GTB) and a Tanker each took **1** shell.
      - **Tanker:** the shell bursts at the point of contact, then the Tanker goes up in its usual 5 blasts in a line.
      - **Blast reach:** the target's neighbour 1 block away blew up shortly after it, and the one 2 blocks away slightly after that: a chain. Cars one blast writes off go up together, in gfa-sim too (`cars::BLAST_FUSE`). One after the other means each wreck's blast reached only the next car, 1 block away, as gfa-sim's `BLAST_CAR_PX = 32` does. A 2-block reach would have caught both neighbours at once. The reading `GTA8.EXE`'s code suggested (2) is not borne out.
        - **Settled headless (`tank_blast_reach`, `tank_blast_reach_15`, test plan D7):** a lone car 2 blocks from the target, and another 1.5 blocks off, stood untouched 12.5 s after the target blew up. A control 1 block on its other side blew up about half a second after it. So the reach is between 1 and 1.5 blocks, which fits gfa-sim's `BLAST_CAR_PX = 32` (1 block).
      - **Score:** **100** for each vehicle, whether car, Coach or Tanker. **Ported:** the rocket's or shell's direct hit no longer pays the 100 AmiGTA1 adds, so a kill pays the wreck's 100.
      - **Deferred (owner, 2026-11-27):** whether a shell at another tank crashes `GTA8.EXE` with "ADDING TWICE", as a rocket did.
  - **Armour:** a rocket at the tank makes it disappear, then `GTA8.EXE` stops with `Error 126.94 ADDING TWICE inform Ian 99` (an original-game crash; not to be reproduced).
  - **Crush:** the tank drives straight through cars, the Bus and peds: cars and the Bus blow up on touch, peds are run over. Nothing stops or pushes it (owner, interactive; in `combat_tank_keys` the tank drove into a parked Bulldog and wrote it off, score `100`).
  - **Glitch:** running over a ped can leave the turret drawn detached from the hull (original-game visual bug; not to be reproduced).
  - **Manual test plan:** `docs/M6B_TANK_TEST_PLAN.md` (overlays from `tools/dosprobe/tank_scenarios.sh`) for the shell's range, what makes it burst, damage to cars and walls, and the turret aiming itself.
  - **Follow-up / status:** Ported (2026-11-27).
    - **Done:** handling (matches gfa-sim); the cannon's trigger (kill frenzy only), fire cadence and shell flight; what the shell bursts on, railings and damage (`docs/M6B_TANK_TEST_PLAN.md` B–D: like the rocket); the one 22 s wall fire, ported to the rocket too.
    - **Turret (owner, 2026-11-27):** it does not aim itself; it turns only on Tab+Left/Right.
    - **Ported (2026-11-27):** the frenzy-gated cannon (`weapons::Cannon`: 99 shells, one every 24 game frames; `Kind::Shell`, from the muzzle 26 px out, 7 then +3 a frame to 17), the turret (`Driven::turret`, 8 a frame, drawn in the app and `gfa-tool run`), the chain timing, the far-wreck rule and the doubling streak. The script's frenzy is M7; `World::start_frenzy` and the test scripts' `frenzy` order load the cannon meanwhile, as does the frenzy crate. Synthetic tests: `the_tank_fires_only_in_a_frenzy_a_shell_every_24_frames`, `a_shell_leaves_the_muzzle_and_speeds_up_by_three_to_seventeen`, `tab_with_left_or_right_turns_the_turret_eight_a_frame_and_not_the_hull`, `a_shot_car_blows_up_a_frame_later_and_one_in_its_blast_ten_frames_after`, `a_wreck_pays_only_within_seven_blocks_of_the_player`.
    - **Measured (owner, 2026-11-27, under DOSBox-X):** the turret turns with the hull as it steers (gfa-sim holds it relative to the hull); `Tab` in a tank sounds no horn; the shell is drawn with the rocket's sprite.
    - **Assumed, not measured:** no car pulls over for a tank's `Tab` (it follows from there being no horn); the frenzy crate loads the cannon too.
    - **Settled (2026-11-27):**
      - **The blast reach**, headless (D7: 1 to 1.5 blocks, as gfa-sim).
      - **By the owner's call:** the far-wreck rule is a square, applies sideways, and covers a rocket's wreck. No projectile bursts on water, only on what it hits (ported).
    - **Open, not blocking:**
      - whether the far-wreck rule counts from the player or the camera
      - a shell at a tank (deferred).

- [ ] **Boats** *(deferred to M7, 2026-11-27)*
  - Measure water-only handling (nav ground type `5`), getting on and off (no door records in style car table), shoreline/beaching behaviour, and mission boats (`IMPLEMENTATION_PLAN.md`, `docs/PARITY.md`).
  - **Probe script:** — (step 1, finding where boats exist, needed none: `gfa-tool map GTADATA/<MAP>.CMP --objects [--kind N]` tallies a map's startup objects by kind, and a search of `GTADATA/MISSION.INI`)
  - **Measured (where boats exist):**
    - **The boat is style car `10`, model `13`, class boat** in all three styles: no doors, mass `0x640000`, thrust `0x141999`, max `5`; sprite `49`, `32x62` (styles 1, 2) or `34x54` (style 3). Scripts and maps name a vehicle by the table's *model* column (the tank is model `37`, car `28`), so the boat is `13`.
    - **No map places one.** `NYC.CMP`, `SANB.CMP` and `MIAMI.CMP` have no startup object of kind `13` (nor does any other kind stand for it), and no declaration is `PARKED 13`/`CAR 13`.
    - **Exactly three boats in the whole game, all `FUTURECAR 13` at `z=5` (water level), all mission set-pieces:**
      - **Liberty City [2], id `200` at `(151,37,5)`** (script `4225..4330`): `CAR_ON 200` and `DUMMYON 200` appear with a `THROW`, then `KILL_CAR 200` at the end.
      - **Liberty City [2], id `242` at `(203,77,5)`** (script `31160..31218`): `CAR_ON 242`, `THROW 240`, `DUMMYON 242`, then `KILL_CAR 242`.
      - **San Andreas [102], id `3720` at `(123,253,5)`, angle `768`** (script `17460..17500`): `PARKED_ON 3720`, `ARROWCAR`, `DUMMYON 3720`, `SENDTO 3720 -> 3730` (a point at `(64,253,4)`), then `DESTROY 3720`.
    - **Vice City has none.** Every boat is a `DUMMY` car: the script drives it (`SENDTO`) and ends it (`KILL_CAR`/`DESTROY`), so the game seems never to let the player drive one.
  - **Measured (owner, manual, `boat_board`: Liberty City [2] with boat 200 as `PARKED 13` and a second parked boat):**
    - **Parked boats appear** on the water as ordinary parked boats.
    - **Boarding cannot be done.** Enter from the quay did nothing. With the quay lowered to water height (`cmp_patch.py --lower 150,32`, `boat_board` overlay) Enter does try to board, but the player drowns first. The player never stands level with a boat in the original, so boarding does not arise: boats are scripted props only.
    - **A boat is destroyed like a car:** bullets hit and destroy it (about a standard car's health), and a rocket destroys it in one hit. A wreck is **three blasts in a line**. gfa-sim's `Weapons::wreck_car` already does this (boats are in its heavy class: centre and both ends).
    - **No flat land stands at water height** in `NYC.CMP` (a scan of every column for a non-water top block at the water's layer, next to water, finds only three sloped railway blocks at `(43,170..172)`). `SANB.CMP` and `MIAMI.CMP` have beaches, but all slope down into the water.
  - **Still to measure:** what a `DUMMY` boat does (speed and path of `SENDTO`; what `KILL_CAR`/`DESTROY` look like on water); Plan: stage Liberty City [2] (`level_overlay.py --section 2`) and the San Andreas [102] mission, or a `combat_water` basin with a `FUTURECAR 13` and a `PARKED_ON`.
  - **Follow-up / status:** Deferred to M7 (owner, 2026-11-27): the only boats are three mission props, so what is left, a `DUMMY` boat's `SENDTO` run and its `KILL_CAR`/`DESTROY` look, needs the mission VM. To resume: stage San Andreas [102] with `level_overlay.py --section 102` and a start trigger running `PARKED_ON 3720`, `DUMMYON 3720`, `SENDTO 3720 -1 0 3730 0`, `DESTROY 3720` (script `17460..17500`). Done so far: the boat is only a scripted mission prop, so the port is a script-driven vehicle on water that can be shot, not a player-handled one. Boarding and handling are not needed.

- [ ] **RC car (`model 47`) & Helicopter (`model 88`)**
  - Verify whether either can be driven by the player and how; record scripted escape helicopter takeoff/flight behaviour for M7 (`IMPLEMENTATION_PLAN.md`, `docs/PARITY.md`).
  - **Probe script:** — (static survey of the owner's data and AmiGTA1's notes; no DOSBox run yet)
  - **Measured (2026-11-27, from the data):**
    - **No map and no script places either vehicle.** No `NYC`/`SANB`/`MIAMI` object list holds a car of model 47 or 88, and no `FUTURECAR`, `PARKED` or `CAR_ON` in `MISSION.INI` names them (the declared kinds run 0–87, none 47 or 88).
    - **RC car = the "model car", a mission mechanic.** `START_MODEL` (24 of them: sections 2, 102, 103, 202, 203) waits until the player is driving, then swaps them into a bomb car (model 47, text `car47` "Model Car"). `DO_MODEL` supervises the session: it ends when the player gets out or the last model car is spent, a spent one is replaced after 100 ticks, and the target's wreck ends it about 100 ticks later. No score on any path. Pressing fire sets the car off. The player never meets it outside those missions. The bomb shop refuses it (`no_modelcar_respray`).
    - **Helicopter = scripted only.** The only trace in the data is `CHOPPER_ENDPOINT` (a section-wide destination: `20010 … (4800,128,0)` in [202] and `(5632,16256,0)` in [203], both Vice City). Nothing declares a model 88 car, so how it is spawned and flown is internal to `GTA8.EXE`. Traffic filters it out (`gta_traffic.c:1186`).
  - **Still to measure:** the model car's handling (speed, turning, fire's blast radius and damage, the camera while driving it) and the helicopter's flight to `CHOPPER_ENDPOINT`. Both can only be staged through a mission section, so they need a `level_overlay.py --section 2` run (Liberty City [2] has one `START_MODEL`, `7300`) or the Vice City [202] run for the helicopter.
  - **Follow-up / status:** Neither can be driven by the player outside a mission, so the port's work is the mission VM's (M7): `START_MODEL`/`DO_MODEL` and a script-flown helicopter. AmiGTA1 already implements the model-car session (`gta_main.c:4805`), which is the starting point. Deferred to M7 (owner, 2026-11-27), with the boats. A probe through Liberty City [2] (`level_overlay.py --section 2 --player 45,64,4 --cmd "0=0 KICKSTART 17010 0 -1 0 0"`, skipping the phone call) did get the player into the RC car and drivable, but the level exited as soon as the first mission text line cleared, and blanking the mission's `RESET`/`MISSION_END` commands (`--donowt`) did not stop it. Own commands with labels above the section's range page-faulted. To resume: run the mission from its start in the mission VM, where `START_MODEL`/`DO_MODEL` end properly, then measure speed, turning, the fire blast and the camera. The helicopter's flight needs Vice [202].

- [x] **Respray refusals**
  - Verify exact trigger conditions and behaviour for emergency vehicles (`no_emerg_respray`), RC car (`no_modelcar_respray`), tank (`no_tank_respray`), and insufficient cash (`no_respray`) (`docs/PARITY.md`).
  - **Probe script:** `tools/dosprobe/scripts/shop_drive.probe` with `tools/dosprobe/shop_scenarios.py`. Build the base with `cargo run -p gfa-tool -- probe-overlay arena --out tools/dosprobe/build/overlays/arena`. For example, stage `python3 tools/dosprobe/shop_scenarios.py SPRAY /tmp/wanted-spray --wanted --score 10000`, then run `python3 tools/dosprobe/run.py --overlay /tmp/wanted-spray --script tools/dosprobe/scripts/shop_drive.probe --out tools/dosprobe/out/wanted_spray --cycles 150000 --fast`. Use `BOMBSHOP`, `--model 5`, `--model 37`, `--model 42` and `--rc` to repeat the vehicle cases. The RC case uses `START_MODEL` after boarding. The wanted case uses `BANK_ROBBERY` to set four cop heads.
  - **Measured — bomb shop:** Ambulance (5) and fire engine (42) show `no_emerg_respray`, model car (47) shows `no_modelcar_respray`, and tank (37) shows `no_tank_respray`. Funded controls kept their full balance. A police car (4) can buy a bomb. A Bulldog at 4,999 points gets `no_respray`; at 5,000 it gets a bomb and the balance goes to zero. With `BOMBSHOP_COST 1000`, the corresponding boundary is 999/1,000. Moving the shop a block north suppressed the prompt. Changing its declaration from z=4 to z=3 still triggered it on the same road.
  - **Measured — spray shop:** A clean Bulldog drove through with score and colour unchanged. With four cop heads it got `no_respray` at 8,079 points and kept its wanted level. At 8,080 it got `clean_done`, kept its colour, lost all four cop heads and finished with zero. The 10,000-point control finished with 1,920. A wanted tank and remote model car also got the plate change for 8,080. Wanted police car (4), ambulance and fire engine passed silently with their cash and wanted level intact. Parking model 1 through the script with `--model 1 --color 3` exercised the paint branch at zero wanted level. The shop refused at 499 and charged 500 from a 1,000-point balance.
  - **Original-code cross-check:** `GTA8.EXE` shop branches at `0xd9660` (spray) and `0xd9a0c` (bomb), with price calculation at `0xe5f30` (LE code offsets). Spray skips models 4/5/15/16/32/42. Its paint branch requires a car linked to a script declaration (`car+0x12b != -1`) and a supported, different colour. That branch can emit `no_modelcar_respray`. Otherwise the shop only offers the plate change when wanted. There is no tank-specific spray refusal. A clean, undamaged paint job costs 500. Other prices are `(damage × 10 × style value[0] + police fee) × (100 + multiplier) / 100` with integer truncation. The police fee is 0 at level 0 and 1,000/2,000/4,000/8,000 for levels 1–4. The bomb shop allows models 4 and 32, refuses 5/15/16/42, 37 and 47 before checking cash, and skips cars already carrying a bomb. Models 15, 16 and 32 were checked in the executable.
  - **Follow-up / status:** Implemented both shops' eligibility, charges and refusal keys in `World::shops_tick`. Refusals preserve money, damage, colour and wanted level. The sim logs the FXT keys once per visit. In-game delivery through the original pager and script-created RC sessions are M7.

---

## 4. Traffic, Pull-Over, Camera & Controls

- [x] **Horn / siren pull-over (`Traffic::pull_over`)**
  - Measure lateral look width (provisional half-block in `crates/gfa-sim/src/traffic/mod.rs`), forward reach (`5` blocks?), pull-over speed (provisional `2` speed units), kerb offset (offsets `7` and `0x37` of `0x40`), whether the car stops completely and for how long (provisional `3` s), and whether an active (un-held) siren also triggers pull-over (`IMPLEMENTATION_PLAN.md`, `docs/PARITY.md`, `docs/REFERENCE_MAP.md`).
  - **Probe script:** `tools/dosprobe/scripts/traffic_pull_over.probe`; no-siren control: `traffic_pull_over_control.probe`. Both use `tools/dosprobe/overlays/traffic/GTADATA/MISSION.INI`.
  - **Measured:** The treatment/control scripts run the same timed approach with and without Tab, but captures do not confirm that the player is seated in the cop car; no pull-over response can be attributed.
  - **Measured by the owner (2026-11-27, interactive, `overlays/traffic`):**
    - **P1:** in the police car with Tab held, the car ahead moves only very slightly within its lane and **does not stop**. Taxis seem to speed up, if anything.
    - **P2:** it works from about **2 blocks** behind at most.
    - **P3:** a car in the next lane, a little ahead, reacts too.
    - **P4:** no stopping was seen, so no stop time.
    - **P5:** the siren switched on alone does nothing; cars react only while Tab is held.
    - **P6:** the horn in an ordinary car does it too.
  - **Read from `GTA8.EXE`'s code (2026-11-27, `gta8.py dis`):**
    - **The look** (`0x18ec0`, the special key in a car, horn or siren): points 1 to 5 blocks ahead of his car's centre (the sine table is 65536 to 1, read from a RAM dump). At each, a search of the map's 2-block cells (`0x65640`) for the first car whose shape meets a box of his own car's (`car+0x1ce`). That car goes to `0x1abb0`, and the look stops there.
      - The box is not read. It is what limits the reach to the owner's 2 blocks and takes in the next lane.
    - **The pull-over** (`0x1abb0`): only a car in ordinary traffic (`car+0x11f == 1`). Its lane target (`car+0x107`) goes from the middle, `0x1f`, to 7 or `0x37` of the block's `0x40`, picked by its direction. The target flips to the other side where the block on that side is road. The dead band (`car+0xbf`) becomes 4; the state becomes 2.
    - **Driving pulled over** (`0x1e3a5`): each frame its speed (`car+0x1c`) goes up one, then down one with the throttle (`car+0xa`) cut. It keeps its speed and **never stops**.
    - **The end:** when a probe at its side meets a building, state 3 (`0x30cd0` and two like it) sets the middle again (`0x1f`). The next frame it is back to state 1 (`0x1e3b0`).
  - **Follow-up / status:** **Ported (2026-11-27).** `Traffic::pull_over`:
    - the nearest car up to 2 blocks ahead of his nose, 48 px either side (his lane and the next);
    - in ordinary traffic, it moves 12 px to the kerb on its right, or its left where the right is road;
    - it drives on at the speed it had, no faster, and only what is in its way stops it.
    - Ours: it goes back to its lane at its next turn, lane change or crossing, or 3 s after the look last found it. The original waits for a building at its side.
    - Data test `the_car_ahead_pulls_over_for_the_horn`.
    - Not explained: the taxis seeming to speed up (P1).

- [x] **Unlisted hotkeys (police toggle & three zoom levels)**
  - Identify the DOS key bindings for the police toggle and the three zoom levels present in the FXT strings but missing from the 1997 README (`IMPLEMENTATION_PLAN.md` M6b & decision log).
  - **Probe script:** `tools/dosprobe/scripts/traffic_controls.probe` on the traffic overlay (initial sweep only).
  - **Measured (not pursued):** The initial sweep checked F1–F12, keypad/main-row zoom candidates, and `p`/`1`–`3`/`l` (F6 pauses; no tested key confirmed the police toggle or zoom bindings). Per owner decision, discovering/emulating these unlisted FXT hotkeys is intentionally not pursued (`IMPLEMENTATION_PLAN.md` decision log).
  - **Follow-up / status:** Done (dropped). Keep the 1997 README key layout and GFA's existing zoom/debug bindings (`docs/PARITY.md`).

- [x] **Camera speed-zoom & lead-ahead**
  - **Probe script:** `tools/dosprobe/scripts/camera_runway.probe` boards an east-facing police car, accelerates for 8 s, coasts for 3 s, then brakes and reverses. The lane stripes give ground scale, and the car's horizontal displacement gives lead. To reproduce:
    ```sh
    cargo run -p gfa-tool -- probe-overlay camera_runway --out /tmp/camera-overlay
    python3 tools/dosprobe/run.py --overlay /tmp/camera-overlay --script tools/dosprobe/scripts/camera_runway.probe --cycles 150000 --fast --run-name camera_runway --out /tmp/camera-run
    python3 tools/dosprobe/analyze_camera.py /tmp/camera-run
    ```
  - **Measured:** Ground spacing goes from **108.0 to 52.68 screen px/block** (roughly **2.05×** the resting footprint). The first zoom change is visible about 0.23 s after throttle, and full zoom at about 1.34 s. Full zoom holds for about 1.20 s after releasing throttle as the car coasts through the highest speed band. The exact camera offset reaches **257 half-pixels** (**4.02 blocks**) about 1.54 s after throttle. Its initial rise is 0, 2, 4, 6 and then 8 half-pixels per frame. In reverse, the original still leads toward the bonnet while the car moves west. On foot the striped ground stays at the resting scale.
  - **Zoom settings:** Sampled both settings and car position every DOS frame during acceleration, coast and straight reverse. The settings matched `lens = clamp(48 + 12 × floor(abs(speed)), 80, 240)` and `plane = 324 − lens/2` as targets, with speed in **half-pixels per 23.333 Hz frame**. Each setting moves using the previous frame's ramp speed, capped at 8 units per frame. When decreasing it uses the full ramp speed, and when increasing it uses half rounded up. The ramp gains one unit per frame and resets on reaching the target. The ground footprint ratio fits `(256 + lens) × 284 / (336 × plane)`. On this DOSBox-X/macOS run, the representative dumps start at guest physical `0x3bd4a8`: `u16` lens at +0, `i32` 16.16 camera x and y at +2/+6, `u16` plane at +12, and the tracked car's `i32` 24.8 x and y at +36/+40. Positions are in half-pixels and fields are little-endian. The physical addresses are build-specific.
  - **Follow-up / status:** Implemented the zoom settings and ramp in `SimCamera`, and fitted the lead from speed and hull direction using the measured cap and initial ramp. The lead transient through turns and changes of direction is an approximation. The renderer applies the measured footprint change at its existing field of view, and the traffic view uses the same lead and zoom. The three-city pop-in check passed at full zoom.

- [x] **Traffic light cycle**
  - **Probe script:** `tools/dosprobe/scripts/traffic_lights.probe` parks the camera at Liberty City block (161,83) with both signal axes visible. It captures 1,960 VGA frames (about 1.5 GB). The analyzer reads the two lenses and bounds each transition between their last old and first new readings.
    ```sh
    python3 tools/dosprobe/run.py --overlay tools/dosprobe/overlays/traffic_lights --script tools/dosprobe/scripts/traffic_lights.probe --cycles 150000 --fast --run-name light_cycle --out /tmp/light-cycle
    python3 tools/dosprobe/analyze_lights.py /tmp/light-cycle
    ```
  - **Measured:** Each approach shows green for **96 game frames** (4.11 s), amber for **40** (1.71 s), red for **176** (7.54 s), and amber again for **40** (1.71 s). The full cycle is **352 frames** (15.09 s) at 23.333 Hz. As one axis changes from amber to red, the other changes from red to amber in the same capture. One axis stays red through every phase.
  - **Phase:** The level starts with north–south green. Repeating the probe with the player's x coordinate changed to 61 showed the same phase and transition times within one game frame, even though AmiGTA1's neighbourhood offset puts the two crossings in opposite phases.
  - **Follow-up / status:** The game and the AmiGTA1 comparison harnesses use one DOS cycle for all crossings. Lamps, drivers and pedestrians read the same signal.
  - **Visibility check:** The new timing exposed a case in Vice City where roadblock officers could reuse pedestrian slots that were still visible. Slot reuse now requires the old occupant to be out of view.
  - **Traffic stress check:** The DOS timings revealed brief vehicle overlaps in two of the twelve fixed-camera runs. Fleet collision correction now separates cars along the shallowest overlap and uses the closing axis for the impact impulse. The gate checks car serials for disappearances, so a position jump at the start of a turn does not count as a despawn.

---

## 5. Pedestrians, Special Groups, Gangs & Later-Milestone Verifies

- [ ] **Special ped groups & bonuses (Hare Krishnas / Elvises)**
  - Verify which cities/levels spawn Hare Krishna lines and Elvis impersonators, spawn frequency/rules, walking formation, and exact bonus triggers/points (`GOURANGA!`, Elvis, cop kills) (`IMPLEMENTATION_PLAN.md`, `docs/PARITY.md`).
  - **Probe scripts:** `tools/dosprobe/scripts/special_groups.probe` and `special_groups_walk.probe` (Director's Cut); `special_groups_london.probe` (London). The walking run records one frame per second for 100 seconds. `krishna_walk.probe` walks the same way and dumps guest RAM every 10 s for `tools/dosprobe/gta8.py krishna`.
  - **Measured:** The walking run on the original Liberty City map shows a seven-person orange-robed line, visually matching the Hare Krishna group, in frames `0054`–`0067` (`73,112`–`86,110` DOSBox-X PIC ticks; about 13 seconds). The clearest frame (`0065`) shows seven members in a single-file sidewalk formation. The earlier stationary Liberty City and London captures did not identify special groups.
  - **Read from `GTA8.EXE`'s code (2026-11-27, `tools/dosprobe/gta8.py`; addresses as the LE objects load, code at `0x10000`, data at `0x130000`):**
    - **Only Hare Krishnas.** There are no Elvises: no text, no remap and no code. `ENGLISH.FXT` has `8787` `GOURANGA!` and no Elvis text; Carnage3D's remap list has none.
    - **The score event** (`0xca4c0`):
      - The reason picks the base, as in AmiGTA1: 2 → 3, 3/7/8 → 10, 4 → 7, 5/6 → 2, else 1.
      - The same type again extends the streak, and the timer restarts at 13. The timer counts down once a game frame and clears the streak the frame after it reaches 0. It runs inline in the per-player frame update (`0xd17d0`, in `0xd1410`). `0xcd0c0` is an identical copy that nothing calls.
      - **Type 1 (a passer-by) pays count × base. Every other type pays base doubled (count − 1) times, even with a base of 1.** AmiGTA1, and so gfa-sim's `Score::event`, read a base of 1 as the bare count. In the code the `<= 1` test comes after the doubling.
      - Each award is × 100 × the multiplier. Only type 7 has a bonus: when its streak count reaches **7**, the game shows `8787` (`0xcaddd`) and adds **12,300** × the multiplier (`0x300c`).
    - **Which peds are type 7** (the ped-death scoring, `0xf2540`): a ped of kind 0 scores by its AI type (`ped+0x62`).
      - Types **21, 22 and 42 → type 7.** 23 → 8, 24/26–30 → 9, 31–33/39/40/50/51/55 → 2.
      - 41/43–47/49 pay through `0xca230` with 10000. 25 (a plain passer-by) and the rest → 1.
      - The script cannot make them: `CHANGE_PED_TYPE` translates its numbers (21 → 24, 22 → 26, 42 → 44, measured), and `PED_ON` gives 25. So types 21, 22 and 42 are only ever Krishnas.
    - **The line** (`0xb69b0`, the passer-by spawner):
      - Every passer-by it makes counts (`0x3ba424`). The **100th**, while no group exists (`0x3971c0`), becomes the **leader**: AI type 21, mode 2 (walking), remap **143**.
      - **6 followers** (type 22, mode 4) spawn on his spot (`0xb7240`), each following the one made before it (`ped+0x7c`). They form a group record of up to 13 (`0x3971aa`).
      - The count is cleared when the level starts and when the leader dies.
      - Every passer-by placed is counted (`0xb6f9d`), whether a line exists or not, and every one reaches the `== 100` check (`0xb706d`): no branch skips it. While a leader lives the count runs past 100, so no second line can start until he dies and the count restarts.
    - **A Krishna's death** (`0xb1dc0`, from every death path):
      - **A follower:** leaves the group, and the one behind follows the one in front.
      - **The leader:** each one left becomes type 42, mode 1 (AmiGTA1's "flee"), aimed at the ped being updated at that moment. The group is gone and the count starts again.
  - **Measured headless** (`krishna_walk.probe`, RAM read with `gta8.py krishna`):
    - **Liberty City (no overlay):** the count reached 100 at 52 s on foot, and the line of 7 appeared on screen beside the player.
      - **Spacing:** they spawn on one spot and peel off one at a time, settling to about **0.3 blocks** apart in single file (frames `f053`, `f060`).
      - **Persistence:** all 7 were still there 52 s later, though off screen at times, and no second line appeared.
    - **San Andreas [102] and Vice City [202]** (staged with `level_overlay.py`): the same line, with the same types and remap, between 50 and 60 s.
    - **Synthetic `arena`:** passers-by spawn about 4 times faster there (31 at the start, 82 at 10 s), and the line appeared by 20 s.
      - The rate swings, between 27 and 86 passers-by per 10 s while walking (`arena_walk`, 12 dumps). The count reached 632 by the last dump with the line alive.
  - **Measured by the owner** (`docs/M6B_KRISHNA_TEST_PLAN.md`, K1–K5, 2026-11-27, arena):
    - **K1:** the line walks slowly, single file, behind the leader.
    - **K2:** run over, the popups went 100, 200, 400, 800, 1600 …. **The doubling is confirmed**, and gfa-sim's linear base-1 rule is wrong.
    - **K3:** GOURANGA! shows only when all 7 go in one streak, and the streak breaks after a gap of a couple of seconds, as the 13-frame window (about 0.56 s) says.
    - **K4:** with the leader dead first, the others scattered at once (type 42, flee).
    - **K5:** the gap before the next line varied a lot. It looked random, but the code has no chance roll: the spread is the passer-by rate above, a line starting out of view, and a living leader holding the next line back.
    - **K6 (weapons):** every weapon doubles, and its first popup gives the base its kills report.
      - Machine gun: 200, 400, 800 …, shot. GOURANGA! appeared, so it comes from any weapon.
      - Rocket: 300, 600, 1200 …, blown up, not burnt. gfa-sim already reports a rocket so.
      - Flamethrower: 1000, 2000, 4000 …, burnt.
      - Not tested: GOURANGA! by rocket or by fire, since neither could catch all 7 at once.
      - **Taken as an educated guess** (with the owner, 2026-11-27): both give it, as the machine gun did. The check (`0xcad25`) counts the type-7 streak and never reads the reason. Each death must come within the window of the one before; how long the whole streak takes doesn't matter. gfa-sim does the same (`a_line_of_krishnas_run_over_pays_25000_with_gouranga`: 50,400 by rocket, 139,300 by fire, at ×1).
      - **Someone a rocket's blast sets alight** without killing him pays as burnt (base 10) when he dies (the owner's play of the game, 2026-11-27). Ported: `Peds::burn`'s `owed`, paid by `World::run_peds` (`Peds::take_owed`), a Krishna as type 7. Synthetic check in `weapon_fire_rates_projectile_speed_and_splat_lifetime_match_original`: nothing when he catches fire, 1000 when he dies 100 ticks later.
    - **K7 (the total):** from 0 at ×1, a line run over in one pass gave exactly **25,000**, confirming the 12,300 bonus.
    - **K9 (corners, synthetic `corners` arena):** the followers trace the leader's path. They keep to the pavement, turn where he turned, don't snag on buildings, and keep the same gap through a corner.
    - **Seen in K9:** when the owner stood near the leader, he started to follow the player.
      - **Read from the code** (`0xbb2e6`, the AI types' switch at `0xbb260`, type 21 only): each tick the leader searches a box `0x20` of `0x40` (half a block) either side of himself for people (`0x63f50`, `0x64f30`). If the first he finds is the player's ped (`0x3ba420`), he takes mode 4 (follow) with the player as his target (`ped+0x7c`).
      - A follower drops a target that has gone (`0xbb527`, `0xbb876`: target `+0x46 < 0`) and walks again (mode 2).
  - **From the owner's play of the game:** traffic stops when a line crosses the road, and a Krishna killed by traffic pays the player nothing.
  - **Manual test plan:** `docs/M6B_KRISHNA_TEST_PLAN.md` (the `arena` and `corners` overlays). All its checks have been run.
  - **Deferred to M8:** the line's timing, which comes from the passer-by spawner (K5, and K7's timing).
  - **Follow-up / status:** **Ported (2026-11-27).**
    - **The line** (`gfa_sim::peds`, `Peds::make_line`):
      - the 100th passer-by made (`Peds::made`) becomes the leader, remap 143, with 6 followers on his spot;
      - one line at a time, never let go while the leader lives;
      - the followers take free slots or new ones beyond the crowd's, so nobody vanishes for them.
    - **Walking** (`mode::FOLLOW`): each follower walks the trail of the one in front, 10 px behind, onto the road too if that is where it goes.
    - **The player:** the leader takes up after him when the player is the nearest person within 16 px of him, on foot. Ours: he lets the player go when the player gets into a car.
    - **Deaths:** a follower's gap closes. The leader's death scatters the rest (`line::LEADERLESS`, flee) and clears the count.
    - **The score:** type 7 (`score::kind::KRISHNA`) for all of them, by any weapon. Every type but the passer-by doubles from a base of 1 too, rams and cars included, in gfa-sim (and in the oracle's `score_streak.pl`, until the score was retired from it the same day). GOURANGA! is the 7th in a streak, +12,300 × the multiplier, logged by `World::gouranga`.
    - **Removed:** gfa-sim's +500 for 4 people run over.
    - **The oracle:** AmiGTA1 has no Krishnas, so the oracle's runs turn the line off (`Peds::krishnas`, `replay::level_world`).
    - **Tests:**
      - unit test `a_line_of_krishnas_run_over_pays_25000_with_gouranga`;
      - synthetic tests `a_hare_krishna_line_run_over_pays_25000_with_gouranga`, `the_hundredth_passer_by_starts_a_hare_krishna_line`, `a_hare_krishna_line_keeps_to_its_leaders_path` (a staircase like the `corners` arena) and `the_krishna_leader_follows_the_player_who_stands_by_him`.
    - **In Liberty City** (`gfa-tool run`, on foot): the line came at the 100th passer-by, at 51 s. The original's headless walk had one at 52 s; the timing is M8's.
    - **Left for M8:** the GOURANGA! text and sound, and the score popups.
    - **Left, not read:** traffic stopping for the line. gfa-sim's fleet already stops for anyone on foot in its way.
  - **Bonus-trigger introspection (before the code was read):** `ENGLISH.FXT` key `8787` is `GOURANGA!`. The GFA port's `World::run_over` currently awards a multiplier-scaled `500` points when a generic civilian run-over streak reaches four; its synthetic test verifies that port behavior, not the original game. Original mission `[1]` separately declares seven `FUTUREPED`s (`382`–`388`), enables them at command labels `1281`–`1293`, then checks their deaths; text key `1049` says `You were meant to kill 'em all`. No explicit `GOURANGA` command appears in this mission script. A DOSBox drive attempt entered a Bulldog but got stuck against a building without hitting pedestrians, so the original bonus trigger remains unverified.

- [ ] **Gang roles state machine (for M7)**
  - Document the original state machine for script-spawned gang AI types (`gta_peds.c:1111`; AmiGTA1 only decoded police roles) (`IMPLEMENTATION_PLAN.md`, `docs/PARITY.md`).
  - **Probe script:** `tools/dosprobe/scripts/gang_roles.probe`
  - **Measured:** Captured the default Liberty City campaign at `19,006`, `24,000`, `29,008`, `34,002` and `39,010` DOSBox-X PIC ticks, plus a 1 MiB RAM sample. No scripted gunman role or state transition was visible in the sampled frames.
  - **Follow-up / status:** Keep pending. Drive a mission that spawns gang peds and decode the corresponding actor state from RAM.

- [x] **Ped anti-pileup departures comparison**
  - Compare AmiGTA1's four anti-bunching departures against `GTA8.EXE` street behaviour: spread corner targets across pavement, ped-ped separation/relax pass, no resting near corners, and per-ped coin flip at T-junctions (`IMPLEMENTATION_PLAN.md` decision log, `docs/PARITY.md`).
  - **Probe:** `tools/dosprobe/scripts/ped_anti_pileup.probe` and `tools/dosprobe/analyze_peds.py`. The player stands at `(159,66,4)` on the original Liberty City map. A mission overlay changes only the spawn. At 150,000 cycles, the analyzer reads the first 15 actor slots every 43 ms from DOSBox-X physical page `0x69a000` and filters for ordinary on-foot civilians. Coordinates are 22-bit fractions of a block. The page address is specific to this DOSBox-X and Director's Cut setup.
  - **Corner targets.** DOS places the target 32 world pixels to the chosen side and adds a forward offset from a shared four-step cycle (5, 13, 20, 0 px, `0xb14ec..0xb18cc`, selector `0x28a43b`). The 1,500-sample run recorded four corner assignments along the sidewalk at y=62, all 32 px sideways, with 5 or 20 px forward. The earlier run independently captured the same offsets. AmiGTA1 draws a forward offset from 0–20 px and an additional lateral offset from −10 to +10 px.
  - **Separation.** The DOS run recorded 28 same-height pavement pair samples closer than 4 px, with a minimum of 0.99 px at `ped_1471` (slots 1 and 5). DOS has local contact checks in the individual update (`0xb03e3..0xb0529`) and advances the actor loop directly to spawning (`0xb28ae..0xb2a1a`). AmiGTA1 adds crowd steering and a post-move pass that pushes overlapping pairs apart.
  - **Resting.** The original's gait wheel selects a timed rest on step 20 (`0xabb9b → 0xabb5d`) without checking pavement ahead. Its corner check skips walkers already resting (`0xb0e52..0xb0e58`). The street captures logged ordinary rests but none with non-pavement 24 px ahead, so this distinction comes from read-only code inspection. AmiGTA1 substitutes a short walk when the pavement ends within its 24 px look-ahead.
  - **Two open exits.** DOS alternates the side on successive choices using a shared level-wide toggle (`0xb1656..0xb1697`, byte `0x28a43c`). The y=62 capture shows three left choices and one right when both lateral probes land on pavement. The capture samples nearby actors, while the shared toggle also advances at other corners. AmiGTA1 uses an independent coin flip for each decision.
  - **Status:** Compared all four. Keep the port's current crowd behaviour. The synthetic `nobody_spins_on_the_spot` check and `oracle_walk_scripts` pass.

  Reproduce the 1,500-sample run from the repository root.

  ```sh
  python3 tools/dosprobe/level_overlay.py --section 1 --player 159,66,4 --out /tmp/ped-corner-overlay
  python3 tools/dosprobe/analyze_peds.py prepare /tmp/ped-corner.probe
  python3 tools/dosprobe/run.py --cycles 150000 --timeout 240 --overlay /tmp/ped-corner-overlay --script /tmp/ped-corner.probe --out /tmp/ped-corner --run-name ped_corner
  python3 tools/dosprobe/analyze_peds.py measure /tmp/ped-corner
  ```

- [x] **Cheats (for M8)**
  - **Method.** Compared AmiGTA1's `gta_career.c:223` with the name comparisons and their success branches in Director's Cut `GTA8.EXE` and London `gta8_uk.exe` (1969) and `Gta24_61.exe` (1961). Each executable stores the typed name in the even bytes of an interleaved string. The comparisons require an exact, case-sensitive match including spaces. The name tables start at loaded addresses `0x1324e4`, `0x1327e0` and `0x14162c` respectively. Representative names were entered through the character menu under DOSBox-X and checked against default-name controls using the chapter menu, HUD and RAM.
  - **Director's Cut.** All 16 AmiGTA1 names are present. London 1969 and 1961 share a 21-name table. Of the 16 Director's Cut names, only `super well` is also in the London table.

  | Effect | Director's Cut | London 1969 and 1961 |
  | --- | --- | --- |
  | Unlock chapters for the selected character | `nineinarow`, `super well`, `itsgallus` | `flashmotor`, `super well`, `travelcard` |
  | Endless lives (signed byte `-1` instead of `4`) | `6031769`, `itstantrum` | `6661970`, `haveagoflower` |
  | Disable police | `iamthelaw`, `stevesmates` | `iamfilth`, `tithead` |
  | Keypad `*` gives 99 rounds for each of the four guns | `callmenigel`, `buckfast` | `deathtoall`, `asawindow` |
  | Enable debug keys, including the keypad ammo shortcut | `porkcharsui` | `pieandmash` |
  | Mark cutscenes as seen, enabling Space at chapter select | `Heart of Gold` | `ohmatron` |
  | Set the special language override to 99 or clear it to `-1` | `iamgarypenn`, `iamnotgarypenn` | `tourettes`, `notourettes` |
  | Start with multiplier ×10 | `hate machine` | `iamgod` |
  | Start with 999,999,999 points | `itcouldbeyou` | `averyrichman` |
  | Start with 99 rounds per gun, armor 3 and a jail card | `suckmyrocket` | `uaintnuffin` |

  - **London extras from code.** `psychadelic` cycles vehicle remaps through 0–12. `silence` sets one digital sample channel's initial volume to 0 (normally 90, London 1969 `0xe4881`). `driveby` sets the drive-by flag used by the two side-fire paths. `rommel` combines drive-by, cycling remaps, debug keys, chapter unlock and cutscenes seen. `herc` adds silence, endless lives, no police, ×10 and 999,999,999 points to that bundle. Its debug flag makes keypad `*` work. The direct starting loadout remains a separate cheat. The 1961 mission pack exposes only one single-player chapter.
  - **Language assets.** `iamgarypenn` selects `SPECIAL.FXT`. London 1969's `tourettes` selects `UK/SPEUK.FXT`, byte-identical to `SPECIAL.FXT` in the local archive. The native 1961 executable requests `UK/SPEUKE.FXT`, which is absent from that archive. Implement the language override with the actual edition assets.
  - **Observed checks.** In Director's Cut, `buckfast` and `callmenigel` filled all four ammo slots after keypad `*`. The default name, `BUCKFAST` and `buckfastx` left them empty. The chapter screen showed all three cities with `nineinarow` and the Space cutscene prompt with `Heart of Gold`. HUD and RAM confirmed the start values above. In both London packs, `asawindow` filled all four ammo slots with keypad `*`, the default name and `buckfast` left them empty, and `uaintnuffin` granted the starting loadout. In 1969, `flashmotor` exposed all four chapters, `ohmatron` added the Space prompt, and RAM confirmed the combination flags in `rommel` and `herc`. The individual debug shortcuts beyond keypad `*` and the London audio, remap and side-fire effects were traced in code.
  - **M8 scope.** The edition can select the table. The current unconditional ammo shortcut belongs behind the keypad and debug flags once name entry is wired up.

  Generate a probe at the repository root. Omit `--name` for a default-name control. Use `dc` with `GTA8.EXE` or `london61` with `Gta24_61.exe` for the other editions. The runner mounts the corresponding CD image.

  ```sh
  python3 tools/dosprobe/cheat_name.py london69 /tmp/asawindow.probe --name asawindow
  python3 tools/dosprobe/run.py --exe gta8_uk.exe --cycles 150000 --script /tmp/asawindow.probe --out /tmp/asawindow --png
  ```
