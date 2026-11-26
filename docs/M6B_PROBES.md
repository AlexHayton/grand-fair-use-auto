# Milestone 6b: DOSBox-X Measurement & Verification Checklist

Tracks every measurement and verification probe run against the 1997 DOS original (`GTADOS/GTA8.EXE`) under `tools/dosprobe`. Tick boxes as each item is measured, record the probe script path and observed numbers in place, and summarize the resulting changes in `IMPLEMENTATION_PLAN.md` and `docs/PARITY.md`.

### Probe Strategy: Overlay Test Maps

Because `tools/dosprobe/run.py` mounts the owner's game folder under DOSBox-X with a scratch `-t overlay` layer on top, placing generated files into `<overlay>/GTADATA/` lets unmodified `GTA8.EXE` boot directly into controlled probes on Level 1 without touching original game data or committing copyrighted assets:

1. **Synthetic `CmpBuilder` `.CMP` + custom `MISSION.INI`:** Generate tiny, noise-free test arenas (shooting range with crates/wall/ped/car; straight runway + one lit crossing; water basin + shoreline + dock; tank/RC/helicopter/`SPRAY` arena) using `gfa_formats::cmp::CmpBuilder` (adding minimal `locations`, `District`, valid style lid tile, and block-dedup helpers). The exact same `.CMP` file can be run in both `GTA8.EXE` and `gfa-tool` side-by-side.
2. **`MISSION.INI`-only overlay on real city maps (`NYC.CMP` / `SANB.CMP` / `MIAMI.CMP`):** Override only `GTADATA/MISSION.INI` when full city traffic, real intersections, or level-specific ped tables (Hare Krishnas / Elvises) are needed, placing `[1]`'s `PLAYER` spawn and probe vehicles/peds right beside each other.

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
  - **Follow-up / status:** Done. Fleet cars carry `player_kill`, set when the player's bullet, flame, rocket, blast or ramming harms them (and when his own bomb goes off); `Weapons::wreck_sweep` pays the `100` only for such a car and passes the owner on to the blasts it makes, so a chain he started stays his (`car+0x132`). A wreck somebody else caused pays nothing. Covered by `a_car_blowing_up_pays_only_when_the_player_caused_it` (the oracle's player-caused cases are unchanged). The existing direct `+100` on a rocket hit is kept to match AmiGTA1; a lone rocket kill pays `+100` in `GTA8.EXE` but `+200` in `gfa-sim` (hit plus wreck), and the streak (`+200` for a chain car) and the `+150`/`+100` pistol-kill split are not modelled yet.

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
  - **Probe script:** —
  - **Measured:** —
  - **Follow-up / status:** Pending.

- [ ] **Boats**
  - Measure water-only handling (nav ground type `5`), getting on and off (no door records in style car table), shoreline/beaching behaviour, and mission boats (`IMPLEMENTATION_PLAN.md`, `docs/PARITY.md`).
  - **Probe script:** —
  - **Measured:** —
  - **Follow-up / status:** Pending.

- [ ] **RC car (`model 47`) & Helicopter (`model 88`)**
  - Verify whether either can be driven by the player and how; record scripted escape helicopter takeoff/flight behaviour for M7 (`IMPLEMENTATION_PLAN.md`, `docs/PARITY.md`).
  - **Probe script:** —
  - **Measured:** —
  - **Follow-up / status:** Pending.

- [ ] **Respray refusals**
  - Verify exact trigger conditions and behaviour for emergency vehicles (`no_emerg_respray`), RC car (`no_modelcar_respray`), tank (`no_tank_respray`), and insufficient cash (`no_respray`) (`docs/PARITY.md`).
  - **Probe script:** —
  - **Measured:** —
  - **Follow-up / status:** Pending.

---

## 4. Traffic, Pull-Over, Camera & Controls

- [ ] **Horn / siren pull-over (`Traffic::pull_over`)**
  - Measure lateral look width (provisional half-block in `crates/gfa-sim/src/traffic/mod.rs`), forward reach (`5` blocks?), pull-over speed (provisional `2` speed units), kerb offset (offsets `7` and `0x37` of `0x40`), whether the car stops completely and for how long (provisional `3` s), and whether an active (un-held) siren also triggers pull-over (`IMPLEMENTATION_PLAN.md`, `docs/PARITY.md`, `docs/REFERENCE_MAP.md`).
  - **Probe script:** `tools/dosprobe/scripts/traffic_pull_over.probe`; no-siren control: `traffic_pull_over_control.probe`. Both use `tools/dosprobe/overlays/traffic/GTADATA/MISSION.INI`.
  - **Measured:** The treatment/control scripts run the same timed approach with and without Tab, but captures do not confirm that the player is seated in the cop car; no pull-over response can be attributed.
  - **Follow-up / status:** **Pending.** Confirm car entry and isolate a target before measuring pull-over geometry, stop duration, or held-vs-toggled siren behavior.

- [x] **Unlisted hotkeys (police toggle & three zoom levels)**
  - Identify the DOS key bindings for the police toggle and the three zoom levels present in the FXT strings but missing from the 1997 README (`IMPLEMENTATION_PLAN.md` M6b & decision log).
  - **Probe script:** `tools/dosprobe/scripts/traffic_controls.probe` on the traffic overlay (initial sweep only).
  - **Measured (not pursued):** The initial sweep checked F1–F12, keypad/main-row zoom candidates, and `p`/`1`–`3`/`l` (F6 pauses; no tested key confirmed the police toggle or zoom bindings). Per owner decision, discovering/emulating these unlisted FXT hotkeys is intentionally not pursued (`IMPLEMENTATION_PLAN.md` decision log).
  - **Follow-up / status:** Done (dropped). Keep the 1997 README key layout and GFA's existing zoom/debug bindings (`docs/PARITY.md`).

- [ ] **Camera speed-zoom & lead-ahead**
  - Measure the original's speed-based zoom-out curve and easing (provisional up to `1.5×` at `10 px/tick` in `SimCamera`), and check whether/how far the camera leads a moving car (provisional Carnage3D `0.35 s` capped at 1/3 view) (`docs/PARITY.md`, `IMPLEMENTATION_PLAN.md`).
  - **Probe script:** `tools/dosprobe/scripts/traffic_camera.probe`; attempts the south-flank get-in sequence and captures at start and 700 ms intervals through 5.2 s.
  - **Measured:** No speed/zoom ratio or lead distance is recorded; captures show the player is still on foot.
  - **Follow-up / status:** **Pending.** Verify seating, then repeat on a long straight road segment.

- [ ] **Traffic light cycle**
  - Measure exact green, amber, and red durations (provisional AmiGTA1 `5 s` green / `0.6 s` amber) (`docs/PARITY.md`).
  - **Probe script:** `tools/dosprobe/scripts/traffic_lights.probe` with `tools/dosprobe/overlays/traffic_lights/GTADATA/MISSION.INI`; captures one frame per second for 20 s.
  - **Measured:** The sampling run completes, but the current fixed view does not yet provide a reliable, unobstructed signal-state sequence.
  - **Follow-up / status:** **Pending.** Reposition the player/camera until the signal is clearly visible, then time phase transitions at frame-level resolution.

---

## 5. Pedestrians, Special Groups, Gangs & Later-Milestone Verifies

- [ ] **Special ped groups & bonuses (Hare Krishnas / Elvises)**
  - Verify which cities/levels spawn Hare Krishna lines and Elvis impersonators, spawn frequency/rules, walking formation, and exact bonus triggers/points (`GOURANGA!`, Elvis, cop kills) (`IMPLEMENTATION_PLAN.md`, `docs/PARITY.md`).
  - **Probe scripts:** `tools/dosprobe/scripts/special_groups.probe` and `special_groups_walk.probe` (Director's Cut); `special_groups_london.probe` (London). The walking run records one frame per second for 100 seconds.
  - **Measured:** The walking run on the original Liberty City map shows a seven-person orange-robed line, visually matching the Hare Krishna group, in frames `0054`–`0067` (`73,112`–`86,110` DOSBox-X PIC ticks; about 13 seconds). The clearest frame (`0065`) shows seven members in a single-file sidewalk formation. The earlier stationary Liberty City and London captures did not identify special groups.
  - **Follow-up / status:** Partial. Repeat to estimate spawn frequency and verify the group's actor type; check Vice City for Elvises and measure the bonus trigger/points separately.
  - **Bonus-trigger introspection:** `ENGLISH.FXT` key `8787` is `GOURANGA!`. The GFA port's `World::run_over` currently awards a multiplier-scaled `500` points when a generic civilian run-over streak reaches four; its synthetic test verifies that port behavior, not the original game. Original mission `[1]` separately declares seven `FUTUREPED`s (`382`–`388`), enables them at command labels `1281`–`1293`, then checks their deaths; text key `1049` says `You were meant to kill 'em all`. No explicit `GOURANGA` command appears in this mission script. A DOSBox drive attempt entered a Bulldog but got stuck against a building without hitting pedestrians, so the original bonus trigger remains unverified.

- [ ] **Gang roles state machine (for M7)**
  - Document the original state machine for script-spawned gang AI types (`gta_peds.c:1111`; AmiGTA1 only decoded police roles) (`IMPLEMENTATION_PLAN.md`, `docs/PARITY.md`).
  - **Probe script:** `tools/dosprobe/scripts/gang_roles.probe`
  - **Measured:** Captured the default Liberty City campaign at `19,006`, `24,000`, `29,008`, `34,002` and `39,010` DOSBox-X PIC ticks, plus a 1 MiB RAM sample. No scripted gunman role or state transition was visible in the sampled frames.
  - **Follow-up / status:** Keep pending. Drive a mission that spawns gang peds and decode the corresponding actor state from RAM.

- [ ] **Ped anti-pileup departures comparison**
  - Compare AmiGTA1's four anti-bunching departures against `GTA8.EXE` street behaviour: spread corner targets across pavement, ped-ped separation/relax pass, no resting near corners, and per-ped coin flip at T-junctions (`IMPLEMENTATION_PLAN.md` decision log, `docs/PARITY.md`).
  - **Probe script:** `tools/dosprobe/scripts/ped_anti_pileup.probe` (run on the original city map, without a synthetic overlay)
  - **Measured:** Walked the player through the default Liberty City area and captured frames at `19,006`, `28,808`, `39,010` and `44,003` DOSBox-X PIC ticks. The camera/player moved, but these captures do not isolate ped corner choices or separation.
  - **Follow-up / status:** Keep pending. Need a reproducible busy intersection and tracked pedestrian positions across repeated runs.

- [ ] **Cheats (for M8)**
  - Verify Director's Cut and London name-entry cheats against AmiGTA1's table (`gta_career.c:223`) (`IMPLEMENTATION_PLAN.md`).
  - **Probe script:** `tools/dosprobe/scripts/cheats.probe` (`buckfast`), `tools/dosprobe/scripts/cheats_control.probe` (Director's Cut default-name control), `tools/dosprobe/scripts/cheats_london.probe` (`buckfast`) and `tools/dosprobe/scripts/cheats_london_control.probe` (London default-name control)
  - **Measured:** In the Director's Cut, entering `buckfast` then pressing keypad `*` changed the visible ammunition count to `99`; the default-name control did not. In London 1969 and 1961, the name was accepted and gameplay started, but keypad `*` did not show `99`; the corresponding no-name controls were also unchanged.
  - **Follow-up / status:** Test remaining AmiGTA1 names and decode the London-specific cheat behavior. The probe runner mounts the edition's CD image automatically.
