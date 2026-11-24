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

- [ ] **Explosion score attribution**
  - Verify whether *every* car blowing up pays the player `100` points regardless of who caused it, or only player-caused / nearby blasts (`IMPLEMENTATION_PLAN.md` M6b & decision log).
  - **Probe script:** —
  - **Measured:** —
  - **Follow-up / status:** Pending.

- [ ] **Fire-key latch across `WASTED`**
  - Verify whether holding Ctrl continuously through `WASTED` and hospital respawn immediately fires/punches on respawn or requires releasing and re-pressing Ctrl (AmiGTA1 latches on key events and clears on `WASTED`; `gfa-sim` currently reads held `Buttons`; `IMPLEMENTATION_PLAN.md` decision log).
  - **Probe script:** —
  - **Measured:** —
  - **Follow-up / status:** Pending.

- [ ] **Firing while drowning**
  - Verify whether a drowning player in water can fire a weapon or punch (`gfa-sim` forbids it; AmiGTA1's test allows it; `IMPLEMENTATION_PLAN.md` decision log).
  - **Probe script:** —
  - **Measured:** —
  - **Follow-up / status:** Pending.

- [ ] **Ped counter-punch**
  - Measure what a punched ped's counter-punch (last two steps of AmiGTA1's 13-step counter in `gta_peds.c:1675`) does to the player: damage, knockdown/stun, and animation (`crates/gfa-sim/src/peds.rs`, `docs/PARITY.md`).
  - **Probe script:** —
  - **Measured:** —
  - **Follow-up / status:** Pending.

- [ ] **Type-4 tanker bomb global**
  - Verify behaviour of the tanker's type-4 damage bomb global (`gta_main.c:4272`, `IMPLEMENTATION_PLAN.md`).
  - **Probe script:** —
  - **Measured:** —
  - **Follow-up / status:** Pending.

---

## 2. Entering/Exiting & On-Foot Interactions

- [ ] **Car get-in animation** *(known bug — owner reports entering through the wrong side of the door)*
  - Frame-by-frame capture against `GTA8.EXE`: door target point, walk-in path, ped sprite frames (`ENTER_SEQ = [26, 26, 26, 25, 25, 29..33]` at `4` ticks each in `crates/gfa-sim/src/cars.rs`), and door-opening delta (`DOOR_TICKS = 40`, `DOOR_OPEN = 20`).
  - **Probe script:** —
  - **Measured:** —
  - **Follow-up / status:** Pending.

- [ ] **Slide under tall vehicles & vault edge cases**
  - Verify whether pressing Space while running at a bus/tall vehicle from outside slides under it (`gfa-sim` / AmiGTA1 only slides while already beneath one and otherwise plays state `0x92` shove), and whether vaulting a car parked flush against a wall drops the player into the wall (`IMPLEMENTATION_PLAN.md` decision log, `docs/PARITY.md`).
  - **Probe script:** —
  - **Measured:** —
  - **Follow-up / status:** Pending.

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
  - **Probe script:** —
  - **Measured:** —
  - **Follow-up / status:** Pending.

- [ ] **Unlisted hotkeys (police toggle & three zoom levels)**
  - Identify the DOS key bindings for the police toggle and the three zoom levels present in the FXT strings but missing from the 1997 README (`IMPLEMENTATION_PLAN.md` M6b & decision log).
  - **Probe script:** —
  - **Measured:** —
  - **Follow-up / status:** Pending.

- [ ] **Camera speed-zoom & lead-ahead**
  - Measure the original's speed-based zoom-out curve and easing (provisional up to `1.5×` at `10 px/tick` in `SimCamera`), and check whether/how far the camera leads a moving car (provisional Carnage3D `0.35 s` capped at 1/3 view) (`docs/PARITY.md`, `IMPLEMENTATION_PLAN.md`).
  - **Probe script:** —
  - **Measured:** —
  - **Follow-up / status:** Pending.

- [ ] **Traffic light cycle**
  - Measure exact green, amber, and red durations (provisional AmiGTA1 `5 s` green / `0.6 s` amber) (`docs/PARITY.md`).
  - **Probe script:** —
  - **Measured:** —
  - **Follow-up / status:** Pending.

---

## 5. Pedestrians, Special Groups, Gangs & Later-Milestone Verifies

- [ ] **Special ped groups & bonuses (Hare Krishnas / Elvises)**
  - Verify which cities/levels spawn Hare Krishna lines and Elvis impersonators, spawn frequency/rules, walking formation, and exact bonus triggers/points (`GOURANGA!`, Elvis, cop kills) (`IMPLEMENTATION_PLAN.md`, `docs/PARITY.md`).
  - **Probe script:** —
  - **Measured:** —
  - **Follow-up / status:** Pending.

- [ ] **Gang roles state machine (for M7)**
  - Document the original state machine for script-spawned gang AI types (`gta_peds.c:1111`; AmiGTA1 only decoded police roles) (`IMPLEMENTATION_PLAN.md`, `docs/PARITY.md`).
  - **Probe script:** —
  - **Measured:** —
  - **Follow-up / status:** Pending.

- [ ] **Ped anti-pileup departures comparison**
  - Compare AmiGTA1's four anti-bunching departures against `GTA8.EXE` street behaviour: spread corner targets across pavement, ped-ped separation/relax pass, no resting near corners, and per-ped coin flip at T-junctions (`IMPLEMENTATION_PLAN.md` decision log, `docs/PARITY.md`).
  - **Probe script:** —
  - **Measured:** —
  - **Follow-up / status:** Pending.

- [ ] **Cheats (for M8)**
  - Verify Director's Cut and London name-entry cheats against AmiGTA1's table (`gta_career.c:223`) (`IMPLEMENTATION_PLAN.md`).
  - **Probe script:** —
  - **Measured:** —
  - **Follow-up / status:** Pending.
