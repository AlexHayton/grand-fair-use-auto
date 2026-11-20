# Grand Fair Use Auto: implementation plan

A living document. Tick boxes as work lands, add a dated line to the
**progress log**, and record every non-obvious choice in the **decision log**
(newest first). When a milestone's exit criterion is met, say so in the log.

**Status:** M0 to M3 complete (2026-11-20), except a Safari check of the web build. Next: M4, vehicles.

## Goal

An "HD remaster" of the 1997 Grand Theft Auto, played from the user's own
original data, natively on Apple Silicon (Metal) and in the browser (WebGL2).
It ports **Carnage3D**'s data and rendering knowledge and **AmiGTA1**'s
deterministic simulation, mission VM and HUD to Rust and wgpu. It adds what
neither project has: CD-audio radio, saves, HD art and video, and the whole
game rather than its first jobs.

In scope: the base game (Liberty City, San Andreas, Vice City), London 1969
and 1961, the FMV intro, HD visual polish, and a port of the hdtex upscaling
pipeline. Out of scope: multiplayer, though the deterministic sim keeps the
door open.

### Setting up the data

Copy `gfa.local.toml.example` to `gfa.local.toml` and point `data` at the
folder that contains `GTADATA/`. Alternatively set `GFA_DATA` or pass
`--data`. Never commit the path or the data.

## Reference projects and what they give us

**Carnage3D** (sibling checkout `../carnage3d`, ~39k lines C++, MIT © 2019 jericho)
- Strengths:
  - Data loaders: `src/GameMapManager.cpp` (CMP), `src/StyleData.cpp` (G24 only), `src/GameTextsManager.cpp` (FXT), `src/Font.cpp`, `src/AudioSampleArchive.cpp`. Structs are in `src/GameDefs.h`.
  - Palette-indexed renderer: `src/GameMapHelpers.cpp` (`BuildMapMesh`, slopes 1–44, lid rotation and flips), `src/MapRenderer.cpp` (22×22-block chunks), `gamedata/shaders/*.glsl`.
  - Perspective follow camera: `src/FollowCameraController.cpp`.
  - Hand-authored entity metadata: `gamedata/entities/{gta_objects,ped_animations,pedestrians,weapons}.json`.
- Gaps:
  - No `MISSION.INI` parsing and no missions.
  - Route data is skipped; AI driving is stubbed.
  - Car handling ignores the style file's physics.
  - Box2D multi-level collision is a hack.
  - No police, no trains, no front end, no radio.
- Its `web/*.data` bundles demo game data. Do not copy it.

**AmiGTA1** (sibling checkout `../AmiGTA1`, ~38k lines of portable game-logic C, MIT)
- What it covers:
  - Behaviour derived from the original.
  - `.GRY` v325 only (`native/gta_style.c`).
  - Every gameplay system: traffic, police, peds, weapons, trains, services and the mission VM (`gta_script.c`, `gta_script_run.c`).
- Sim units:
  - 16.16 positions at **32 px per block** (the original's lengths halved, inertia quartered).
  - 256-step angles, 0 = north, clockwise.
  - Q14 cosine table (`gta_trig.c`).
- Timing:
  - Base tick `SIM_HZ 50`; the script runs at 25 Hz.
  - Vehicle physics runs at 23.33 Hz via a 7/15 accumulator.
  - Ped timers use `FRAMES(n)=n*3/2`.
  - LCGs with fixed seeds (traffic 12345, peds 777, trains 0x5eed).
- Determinism leaks we must fix:
  - Spawn rings and `GENERAL_ONSCREEN` follow the camera view and zoom.
  - Ticks per frame come from the wall clock.
  - `long` overflow behaves differently between m68k and the host.
- Mission VM:
  - 32 process slots; one command per process per 25 Hz tick.
  - `p2`/`p3` success and failure jumps; `p5` pays the score.
  - The ~70-callback `gta_script_world` table is a clean seam to copy as a trait.
- Mission VM caveats:
  - Unknown commands silently succeed.
  - Only the first job of each level has been played through.
  - The vocabulary docs are private, not in the repo.
  - Bug: `parse_line` drops the 5th number, which breaks SPECIFIC_DOOR remaps.
- `gta_main.c` is 11k lines: game logic (arrest, cranes, doors and garages, bombs, the jump arc, the arrow, HUD text, the script callbacks) tangled with the platform layer. It needs careful extraction.
- Test harness:
  - `tools/gtadump.c` has about 60 host subcommands: `drive`, `walk`, `scriptrun`, `view` and others.
  - `tools/bin/holecheck.sh` is the render regression.
  - The 117 `tools/scripts/*.txt` input scripts are consumed one order per tick; each header lists the log lines it expects.

**hdtex** (`../in_pursuit_of_greed/tools/hdtex`, the author's own project)
- Real-ESRGAN 4× using the vendored `rrdbnet.py` (MPS on Mac).
- `upscale.py` handles wrap-aware tiling: tiles are upscaled as a 3×3 mosaic and the centre is cropped back out.
- Sprites: transparency is filled before upscaling, then remasked.
- `palette.py` requantizes in Oklab with no dithering and holds back semantic indices.
- `pixelart.py` uses EPX for fonts and index-semantic art.
- `pack.py` writes a split sidecar where any subset loads.
- `roundtrip.py` proves the codecs before anything else is trusted.
- Video uses NGX VSR (NVIDIA only), with ESRGAN as the macOS fallback.

**Game data:** a Director's Cut install (complete). Its location is machine-local (`gfa.local.toml` / `GFA_DATA`).
- `GTADATA/`:
  - Maps and styles: NYC, SANB and MIAMI `.CMP`; STYLE001-003 in both `.GRY` and `.G24`.
  - Script and text: `MISSION.INI` (sections [1],[2],[102],[103],[202],[203] plus multiplayer); FXTs; `.FON`.
  - Front end and briefings: `F_*.RAW/.RAT`, `F_PAL.RAW`; `CUT0-5.RAW/.ACT` with `AUDIO/0-5.WAV` voice-overs.
  - Sound: `AUDIO/LEVEL00x`, `VOCALCOM` and `MISBRIEF` `.SDT/.RAW`.
  - Video, replays and saves: `MOVIE.SMK` (SMK2); `1/102/202.REP` replays; `PLAYER_A.DAT`.
- `GTADATA/UK/`: London (`UK.CMP`, `MC.CMP`, `uke.cmp`, `Style001.g24/.gry`, `MISSUK.INI`, `missuke.ini`/`misse8.ini`, FXTs, `UK.MAP`, `MC.MAP`, `AUDIO/*.PK`, `MOVIE.SMK`).
- `MUSIC/GTA/Track02-11.ogg` and `MUSIC/LONDON/Track02-16.ogg`, each with a `CDAUDIO.DAT` cue sheet.


Toolchain: Rust stable (1.100 at the start), the `wasm32-unknown-unknown` target, Trunk 0.22, Node 24 (for running wasm tests), Apple clang (for the AmiGTA1 oracle).


## Principles
1. **Behaviour comes from AmiGTA1; data and geometry from Carnage3D.** On gameplay, AmiGTA1 wins.
2. **The sim is pure and deterministic.** `gfa-sim` has no floats (`#![deny(clippy::float_arithmetic)]`), no I/O, no clock and no platform crates. It is a function `(state, tick inputs) -> state`. Rendering interpolates between ticks with floats.
3. **Use AmiGTA1's sim units exactly**: 16.16 at 32 px/block, 256-step angles, the Q14 table, the 50 Hz base and its subsystem dividers.
   - The ~38k lines then translate mechanically, with their tuned constants intact.
   - AmiGTA1's host harness can serve as a differential oracle.
   - Conversion to original and render units happens only at the boundaries.
   - Use `i32` with explicit wrapping where AmiGTA1 relies on 32-bit overflow; `i64` for intermediates.
4. **Close AmiGTA1's determinism leaks.**
   - Spawn and despawn rings and on-screen checks use a fixed **logical sim view** sized for the widest supported aspect. They never use the window or render zoom. Widescreen, Retina and HD therefore cannot change gameplay.
   - Wall-clock catch-up lives outside the sim.
   - Replays record `(tick, input)` only.
5. **One codebase, two targets.** Every crate except `gfa-native` compiles for `wasm32-unknown-unknown`. wgpu always uses `Limits::downlevel_webgl2_defaults()`, even on Metal.
6. **Ship no game data, or anything derived from it.** HD packs are generated locally and gitignored, never published. The `.gitignore` denies data extensions, as AmiGTA1's does.
7. **Prove before trusting.**
   - Parse every file.
   - Round-trip the codecs.
   - Headless render holecheck.
   - Replay hashes, native vs wasm.
   - Differential traces against AmiGTA1.
   - Mission opcode coverage.
8. **AmiGTA1 is the floor, not the ceiling.** The target is the whole original game, so anything AmiGTA1 lacks or leaves partial is scheduled work (see *Gaps beyond AmiGTA1*). Where AmiGTA1 is silent, establish the original's behaviour before building it, and log the evidence in the decision log. Evidence sources, in order:
   - the game's own data (FXT strings, MISSION.INI, style tables);
   - Carnage3D's docs;
   - community format and mission docs;
   - watching the original `GTADOS` executables run under DOSBox-X.

   Never invent features. For example, AmiGTA1 notes that the original's police send no army or SWAT (`gta_traffic.h:2622`), so we add none unless evidence says otherwise.

## Workspace layout
```
grand-fair-use-auto/
  Cargo.toml  rust-toolchain.toml  .cargo/config.toml
  IMPLEMENTATION_PLAN.md  README.md  LICENSE (MIT)  THIRD-PARTY.md
  crates/
    gfa-fixed/    Fx16 (16.16), Angle256/ang16, Q14 trig, gta_dir16 atan, LCG RNG
    gfa-formats/  pure parsers (&[u8] -> structs): CMP (+routes, nav), GRY v325 + G24 v336,
                  FXT, FON, SDT/RAW, F_*.RAW/RAT, F_PAL, CUT*.RAW/ACT, MISSION.INI,
                  PLAYER_A.DAT, CDAUDIO.DAT, SMK2, REP (probe)
    gfa-assets/   async AssetSource; case-insensitive resolver; dataset detection
                  (GTA / London 69 / London 61 / demo); FsSource + WebSource
    gfa-sim/      map/nav queries, player, peds, vehphys, traffic, police, weapons,
                  pickups, trains, services, marks, score/heat, career, mission VM
                  (+ ScriptWorld trait = AmiGTA1's gta_script_world)
    gfa-render/   wgpu: city mesh chunks, sprite batcher, palette/CLUT/remap in WGSL,
                  HUD/fonts, camera, lights/post, HD textures, video frames
    gfa-audio/    kira: SFX banks, positional mix, engine pitch, radio, voice
    gfa-app/      winit shell + states (boot/frontend/briefing/game), input map,
                  config, SaveStore, egui debug overlay
    gfa-native/   bin (pollster, std::fs, CLI: --data --level --nohd)
    gfa-web/      cdylib + Trunk index.html (folder picker, IndexedDB cache, audio unlock)
    gfa-tool/     CLI: info/dump, export PNG/manifest, headless render + holecheck,
                  run <script> (replay + hashes + expectations), mission coverage, hd pack
  data/entities/  object/ped-anim/ped/weapon metadata adapted from Carnage3D JSON (MIT)
  tools/hdtex/    Python upscaling pipeline adapted from in_pursuit_of_greed
  tools/oracle/   out-of-tree build of AmiGTA1's gtadump + trace patch (differential tests)
  tools/scripts/  input scripts (AmiGTA1 order format) with expected-log headers
  docs/           REFERENCE_MAP.md, FORMATS.md, MISSION_OPCODES.md,
                  PARITY.md (feature ledger: original / AmiGTA1 / GFA status + evidence)
```
Main crates:
- wgpu, winit, glam (render side only), egui + egui-wgpu + egui-winit
- kira (ogg playback)
- web-time, wasm-bindgen(-futures), web-sys, pollster
- serde, toml, serde_json; anyhow, thiserror
- zstd (pure-Rust ruzstd on the web)
- image (tool only)

## Milestones
Each milestone has tasks with checkboxes and an **exit criterion**. Named references are the files to read while porting.

**M0: Scaffold and guardrails**
- [x] git init. `.gitignore`: `target/`, data extensions (`*.cmp *.gry *.g24 *.fxt *.raw *.rat *.sdt *.smk *.rep *.fon *.ogg *.wav`, matched case-insensitively), `gfa.local.toml`, `hd/`, `tools/hdtex/.venv`, `*.pth`, `tools/oracle/build/`.
- [x] LICENSE (MIT). THIRD-PARTY.md: the Carnage3D and AmiGTA1 MIT notices; Real-ESRGAN weights (BSD-3) are downloaded, never committed. README.
- [x] Workspace and crate skeletons. Cargo aliases: `wasm-check`, `data-test`.
- [x] Data path resolution: `--data` flag, then `GFA_DATA`, then `gfa.local.toml`.
- [x] `docs/PARITY.md` seeded from *Gaps beyond AmiGTA1*.
- [x] `gfa-native` opens a winit + wgpu window that clears the screen. `gfa-web` builds with Trunk.
- Exit: `cargo build`, `cargo test` and `cargo wasm-check` are green; a window opens natively and in the browser.

**M1: Formats** (`gfa-formats`, `gfa-tool`)
- [x] CMP v331. Ref: Carnage3D `GameMapManager.cpp`, AmiGTA1 `gta_map.c`. (`gfa_formats::cmp`, notes in `docs/FORMATS.md`)
  - Header; base offsets and column-compressed blocks, validated on load; lookups by `(x, y, z)`.
  - Startup objects, routes (police patrols and roadblock lists, which Carnage3D skips), service bases, districts.
  - All six Director's Cut maps parse. Column (25,18) of NYC matches AmiGTA1's recorded dump.
  - The nav byte grid (`gta_nav.c`) is derived from the map at runtime, not stored, so it moves to M3.
- [x] Style. Ref: AmiGTA1 `gta_style.c` (GRY v325), Carnage3D `StyleData.cpp` (G24 v336). (`gfa_formats::style`, notes in `docs/FORMATS.md`)
  - Tiles, animations, CLUTs and palette indices, remaps.
  - Objects, car info (raw 16.16 physics), sprites, sprite numbers, deltas, HUD "arrow" sprites.
  - All nine style files parse (GTADATA and UK, both formats). Each map names only tiles its style has. Contact sheets of tiles, sprites, remaps and deltas reviewed for STYLE001.
  - GRY lid remaps (AmiGTA1's "unknown_b") are inferred, not yet proven; M2's renderer confirms them.
- [x] FXT (0xBF/0xA6 keys, 195 escape), FON (with its 768-byte palette), SDT/RAW (12-byte LE entries; 8-bit unsigned). (`gfa_formats::{fxt, fon, sdt}`)
  - All 22 FXTs, 191 FONs and 12 sound banks parse. Front-end bank `LEVEL000` is 16-bit, the rest 8-bit (measured).
  - Mono or stereo for LEVEL000, London's two odd front-end samples, and `crimes` duplicate use stay open for M8.
- [x] Front-end images (`F_*`, `F_PAL`, `CUT*.RAW/.ACT`), `PLAYER_A.DAT` (`gta_career.c`), `CDAUDIO.DAT`. Header probes for SMK2 and REP. (`gfa_formats::{screen, career, cue}`)
  - All 140 front-end and briefing files parse; sizes found from the data and confirmed by eye.
  - REP files look like `{tick, input}` change logs (attract mode, M8). London's saves have another layout (M11).
- [x] `MISSION.INI`: sections; typed declarations (5 trailing numbers, fixing AmiGTA1's bug); the command block. Ref: `gta_script.c`, `gta_script_run.c:244`, Carnage3D `docs/ped.txt`. (`gfa_formats::mission`)
  - All four mission files parse, including the `[1003]` header AmiGTA1's reader misses. [1] has 575 declarations and 2116 commands.
  - Names stay strings here; the typed declaration and command tables belong to the VM (M7).
- [x] `gfa-tool info | export {tiles,sprites,fonts,frontend,cuts} | mission {list,dump}`. As built: `info`, `map [--at X,Y]`, `style [--cars]`, `text [--key] [--grep]`, `mission [--section N] [--dump]`, `export <style|font|picture>`.
- [x] Data tests, gated on `GFA_DATA`: parse every file in GTADATA and UK, assert counts (e.g. [1] has 575 declarations and 2116 commands), and write contact-sheet PNGs for review.
  - 11 data tests. Not yet parsed, by design: `UK.MAP`/`MC.MAP`, `UK/AUDIO/*.PK` and London's saves (M11); `.REP` input bits (M8); `MOVIE.SMK` (M9); the `.WAV` voice-overs (standard RIFF, read by the audio crate).
- Exit: the whole Director's Cut set parses, and the exported art looks right. GRY and G24 agree on structure (they do, except where `docs/FORMATS.md` records the differences).

**M2: Static city renderer (native + web)**
- [x] City mesh. Ref: Carnage3D `GameMapHelpers::BuildMapMesh`, `MapRenderer`. (`gfa_render::city_mesh`)
  - Faces, lid rotation and flips, slopes 1–44, flat/transparent faces.
  - 16×16-block chunks (256 / 16 divides evenly; Carnage3D uses 22×22) with frustum culling; neighbouring visible chunks merge into one draw.
  - 12-byte vertices in map pixels. NYC: 138,327 faces, 553,308 vertices. Flat blocks: 11,041, exactly AmiGTA1's count.
- [x] Palette-indexed textures in WGSL via `textureLoad`. (`gfa_render::{style_tables, city}`, `shaders/city.wgsl`)
  - One 2048-wide R8Uint tile atlas instead of a texture array (WebGL2 allows 256 layers; styles have ~400 tiles).
  - RGBA palette texture (8 palettes per row); per-tile palette table for the four lid remaps (Rgba16Uint); animated-tile table (R16Uint), rewritten when a frame changes.
  - GRY and G24 share one path: GRY's palettes are its remap tables applied to its palette.
- [x] Follow camera: perspective, FOV 55°, height zoom, lead-ahead. Free-look debug camera. The egui overlay shows fps, camera position and a block inspector. (`gfa_render::camera`, `gfa_app::{controls, overlay}`)
  - Until M3 brings a player, the camera follows a point panned with the keys. F1 toggles the overlay; F toggles free look.
  - `gfa --screenshot <png>` saves the first frame and quits: a smoke test of the app itself.
- [x] Sprite batcher: instanced quads, height/draw-order sort. (`gfa_render::sprites`, `shaders/sprites.wgsl`)
  - Shelf-packed sprite atlas (NYC's style: 2048×594); car remaps from G24 remap CLUTs or GRY tables.
  - Draws the map's startup objects (render-side until M3 spawns them): all 3,015 in NYC have a sprite.
  - Particles come with effects in M6; they will be instances of the same batcher.
- [x] `gfa-tool render`: headless offscreen PNG. Holecheck: clear to a sentinel colour and count leftover pixels at 8 fixed viewpoints (AmiGTA1 `holecheck.sh`).
  - The sentinel is alpha 0, which no face writes. Options: `--at`, `--height`, `--pitch`, `--yaw`, `--debug faces|flat`, `--time`, `--style`.
- [x] Web: `<input webkitdirectory>` folder picker into a `MemSource`; cache in IndexedDB.
  - The page's script owns the picker and the cache and hands files to Rust (`gfa_add_file`, `gfa_files_done`); `?game=` and `?level=` choose the level.
  - Tested in Chrome: 180 files (41 MB) restored from IndexedDB on reload; the city at 2560×1600, 120 fps.
- [ ] Safari check of the web build (needs a person at a Mac; Chrome is tested).
- Exit: NYC, SANB and MIAMI render at Retina resolution on macOS and in Chrome and Safari; holecheck = 0.
  - Met except Safari: native and Chrome render at 2560×1600; holecheck is 0 on all six maps (NYC, SANB, MIAMI, UK, MC, UKE).

**M3: Deterministic sim foundation**
- [x] `gfa-fixed` ports AmiGTA1's formats, trig and RNGs (`gta_trig.c`).
  - `trig`: the Q14 cosine table and the `dir16` arctangent, both generated from their formulas (a test rebuilds them with floats); `cos16`/`sin16` interpolate fractional angles.
  - `rng`: the traffic/train (Numerical Recipes), ped (ANSI C) and voice generators in 32-bit arithmetic; `fx`: 16.16 helpers at 32 px a block.
- [x] Tick scheduler: the 50 Hz base with exact dividers (script 25 Hz, vehphys 7/15, peds 3/2, marks/trains/objects at ½); render interpolation.
  - `gfa_sim::clock`: `Half` (both of AmiGTA1's phases), `Ratio` (vehicle physics 7/15 fires on the third tick, as AmiGTA1's accumulator), `ped_frames`.
  - The app's `Pacer` turns wall time into ticks (at most 6 a frame, AmiGTA1's catch-up cap) and the player is drawn between the last two ticks.
- [x] Logical sim view (Principle 4). Map and height queries, including the nav byte grid (`gta_nav.c`) and ground types (`gta_map_ground_type`). `ScriptWorld` trait skeleton.
  - `SimView::GAME` is 10×6 half-blocks (a 21:9 window at the default camera height); `SimView::AMIGTA1` (5×4) is for oracle runs.
  - `gfa_sim::map`: ground types with the missing-pavement rule, the nav byte (railway blocks zeroed, dirt roads as road), thin walls from the solid flat-face tile lists, ramps (`slope_up_dir`, `slope_is_top`), `wall_cross`, and `ground_z` for the height on a ramp.
  - `ScriptWorld` holds the M3 callbacks and a table of which AmiGTA1 callbacks each later milestone adds; unsupported calls return `Unsupported` instead of succeeding.
- [x] Input becomes a recorded command stream. Parse the AmiGTA1 order format (`wait/run/enter/goto/park/…`, `gta_main.c:6875-7084`).
  - `TickInput` is the buttons held on a tick; `InputLog` records the ticks where it changes (`gfa-input 1` text format) and plays them back.
  - `Script` reads both AmiGTA1 formats in one file: orders (`autodrive.txt`) and walk legs (`gtadump walk`), with `start`, and `# section:`, `# expect:` and `# reject:` headers. Every order is recognised; fixtures for later milestones log "not supported yet".
- [x] `gfa-tool run --section N --script f --ticks T [--hash-every K]` checks the expected-log lines from the script header.
  - Also `--trace` (per-tick player state, the oracle's format), `--out` (PNGs for `dump`/`film`, with the player drawn), `--record` (save the input) and `--oracle` (AmiGTA1's view size and no parked cars, as its harnesses run).
  - Scripts live in `tools/scripts/`.
- [x] Player on foot. Ref: `gta_player.c`.
  - Run by default, Shift walks; turning, backing up at half pace, sliding along walls, fences, ramps between layers (NYC's bridge at 19,44 lands on layer 3), the walk and run cycles.
  - In the app: arrows/WASD drive him, the follow camera trails him, `C` detaches the camera to fly.
- [x] Oracle: build AmiGTA1's `gtadump` with clang, directly from a sibling `../AmiGTA1` checkout into `tools/oracle/build`. Bake `.til` from the user's GRY with `gtabake`. A small trace patch lives in our repo.
  - Instead of patching `gtadump`, our own harness (`tools/oracle/walktrace.c`) links AmiGTA1's `gta_map.c`, `gta_nav.c`, `gta_player.c` and `gta_trig.c` from the sibling checkout and prints a per-tick trace. `tools/oracle/compare.sh` diffs it against `gfa-tool run --trace`; `tools/oracle/fuzz.py` does so for random walks on all three cities.
  - Baking `.til` is not needed for walking (the player needs no style), so it waits for the first oracle check that reads car data (M4).
- Exit:
  - You can walk every level.
  - The same script gives identical hashes on native and wasm (run in node).
  - `walk` traces match the oracle.
  - Met:
    - Every mission section of all three games (28 levels, multiplayer included) starts the player on walkable ground, and he runs freely from there (data test `every_level_is_walkable`). Native app tested on Liberty City; the web bundle builds with the sim in it.
    - `tools/wasmcheck/compare.sh` runs scripts natively and as wasm in Node 24: 16 scripts, 2,223 hashes and every log line identical.
    - The three walk scripts and 24 random walks (about 43,000 ticks over NYC, San Andreas and Vice City) match AmiGTA1 tick for tick: position, layer, angle, animation frame and blocked flags.

**M4: Vehicles.** Ref: `gta_vehphys.c/.h` (essay in the header), `gta_car.c`, `gta_train.c`, `gta_main.c:790-805` and around line 8960 (jump arc).
- [x] Two-tyre rigid body with the style's raw 16.16 values. Rear grip ×1.5; handbrake 0.6; road-snap assist.
  - `gfa_sim::vehicle`: AmiGTA1's `gta_vehphys.c` in `i64`, which is its 64-bit host build's arithmetic (the oracle's). G24 and GRY car tables are checked equal by a data test.
- [x] Wall resolve with a 10-point outline and safe-position revert.
  - With fences and posts (the style's post tiles stop cars only), the wedge push-out after half a second, and wall damage (impact speed less a grace pixel).
- [x] Ramps and layers; jumps.
  - Layers change at the nose; the jump from AmiGTA1's main loop (lift, gravity, five-block cap, landing on the first surface at or below). `tools/scripts/drive_jump.txt`: the car leaves (91,105) and lands at (91,116) on layer 3.
- [ ] Damage via sprite deltas; burning, exploding, wrecks.
  - Done: a wall hit dents the panel it struck (AmiGTA1 `gta_car_panel_delta`); the sprite atlas also holds every sprite delta as its own image, drawn over the car; at 100 points the car is a write-off, shows every damage delta, burns 120 ticks with the fire object's frames, then blows up, throwing the player clear and leaving a wreck nobody can enter. Doors open and close on the art during get-in and get-out. The `damage` order.
  - The blast's effect on what is around it, wrecks' burnt look and chain explosions moved to M6.
- [ ] Door-by-door enter and exit; vault the car; drag out the driver; bikes and convertibles.
  - Done: parked cars from the map's startup objects (trains and trams excepted) are sim-owned; RETURN walks to the nearest car's door within 48 px and runs the ten-step get-in; the get-out refuses above speed 5 and plays nine steps while the car rolls; bikes mount and dismount on their own frames; a walker cannot walk through a car.
  - Also: a convertible shows its driver, a bike its rider.
  - To do: the vault over the roof (a blocked door, or the far side). Dragging out a driver moved to M5.
- [ ] Trains.
- [ ] Use `tools/vehruler.c` as the measurement reference.
- [ ] Close the M4 items in *Gaps beyond AmiGTA1*: tank, boats and water, specials, trains. (Pull-over, the siren, bombs and chain explosions moved to M5 and M6.)
- Exit: every class drives, including the tank and the boats; the (91,105..108) jump lands; vehicle traces match the oracle.
  - So far: the jump lands; `tools/oracle/drivetrace.c` (AmiGTA1's physics, wall and layer code, with its main loop's glue restated) matches `gfa-tool run --trace` on `drive_oracle`, `drive_jump`, `drive_wall` and 24 random drives over the three cities with random vehicles (`fuzz.py --drive`).

**M5: Living city.** Ref: `gta_traffic.c` (10.6k lines), `gta_route.c` (BFS: 48-block window, 700 nodes), `gta_nav.c`, `gta_peds.c`.
- [ ] Fleet spawn and despawn on the logical view.
  - Nothing may pop in or out where the player can see it. The camera's lead and any speed zoom become sim state (from position and speed only), the spawn and despawn rings follow them with AmiGTA1's margins (+2..+6 blocks spawn, +9 despawn), and in normal play the renderer never shows more ground than that view: zoom-out is capped and windows wider than 21:9 are pillarboxed. Free look, fly and extreme zoom are debug views where pop-in is accepted.
  - A pop-in check, like the holecheck: run traffic and fail if anything spawns or despawns inside the visible ground at the largest allowed zoom and aspect.
- [ ] AI driver: lanes, junctions, give way, queues, S-curve lane change, U-turn.
- [ ] Traffic lights.
- [ ] SAT `box_mtv` collisions with impulse and spin (`fleet_collide`, around `gta_traffic.c:8067`).
- [ ] Peds on pavements, crossings, knockdowns, panic.
- [ ] Close the M5 items in *Gaps beyond AmiGTA1*: counter-punch, gang roles, special ped groups, junction fidelity, pull-over, the siren, dragging out drivers.
- Exit: the `drive` traces from `fullgate`/`litegate` (4 sites × 3 seeds × 12,000 ticks) match the oracle, with no deadlocks.

**M6: Combat, police, services.** Ref: `gta_weapon.c`, `gta_pickup.c`, `gta_score.c` (heat thresholds such as LC 151/251/351/501), `gta_traffic.c:9681-10500` (cop dispatch, roadblocks), `gta_peds.c:1912-2066` (arrest, shooting), `gta_service.c`, `gta_marks.c`, plus the `gta_main.c` busted/wasted/siren code.
- [ ] Five weapons, rockets and flames, fires and explosions; crates.
- [ ] Wanted levels, chases and rams, roadblocks, arrest; BUSTED and WASTED, lives, hospital.
- [ ] Ambulance and fire engine. Spray and bomb shops.
- [ ] Tyre marks, blood and oil for all cars. AmiGTA1 only marks the player's car.
- [ ] Close the M6 items in *Gaps beyond AmiGTA1*: crates, armour, friendly fire, wall fires, power-ups, bonuses, police reasons, bust tally, bombs, chain explosions, blast damage.
- Exit: the bust, death and spray-escape loops work; the AmiGTA1 `car_*`, `bankblast` and `blood*` scripts pass.

**M7: Mission VM, HUD, text.** Ref: `gta_script.c`, `gta_script_run.c` (`cmd_names[]` at lines 35-172; `step()` at 645; `gta_script_tick` at 2691), `gta_script.h:203-220` and `:459-663`, the `gta_main.c` callbacks (doors, cranes, bombs, the arrow), `gta_hud.c`, `gta_font.c`, `gta_text.c`, and `gta_main.c:2615-2830` (text displays).
- [ ] Port the VM: declarations, the command table, 32 processes, triggers (TRIGGER/MPHONES/CARTRIGGER/DUM_MISSION_TRIG), phones, arrows, garages and doors, cranes, kill frenzies, bank robbery.
- [ ] **Unknown commands log section and line and count toward coverage. They do not silently succeed.**
- [ ] `docs/MISSION_OPCODES.md` lists every command used in GTA's `MISSION.INI` with its status. Sources: AmiGTA1's table, Carnage3D `docs/ped.txt`, community mission docs, and observed behaviour.
- [ ] `gfa-tool mission coverage` produces the coverage report.
- [ ] Original HUD: cop heads, weapon and ammo, armour, jail-free card, the pager scroll, the brief box, the big two-line card, score and multiplier fonts.
- [ ] Close the M7 items in *Gaps beyond AmiGTA1*: every job, a playthrough script per job, mission-only vehicles.
- Exit, in order:
  1. LC jobs 1–2 pass AmiGTA1's `threejobs`/`phonering`/`lc2_job1` scripts.
  2. Each level's first job passes (`sa1`, `sa2`, `vc1`, `vc2`).
  3. All of chapter 1 is completable. This goes beyond AmiGTA1.
  4. All 6 chapters are completable.
  5. Every used opcode is implemented, or stubbed with a documented reason.

**M8: Front end, audio, saves.** Ref: `gta_front.c`, `gta_career.c`, `gta_audio.c` (radio and engine-pitch logic), `gta_sfx.c`, `gta_snd.h`.
- [ ] Title and menus in the original fonts; player and chapter select; high scores.
- [ ] Briefing stills `CUT0-5` with `0-5.WAV`.
- [ ] SFX banks with positional mix; engine pitch; VOCALCOM; MISBRIEF.
- [ ] Radio: clock-based stations `(offset+elapsed) mod length` mapped to `Track02-11.ogg` through `CDAUDIO.DAT`; the police band only in police cars; on-foot ambience; a station key.
- [ ] Web audio starts on the first gesture.
- [ ] Close the M8 items in *Gaps beyond AmiGTA1*: cheats, language, high scores, pause and options, attract demos, complete speech and SFX.
- [ ] Saves: read and write the original `PLAYER_A.DAT` format behind a `SaveStore` trait.
  - Native: `~/Library/Application Support/GrandFairUseAuto`.
  - Web: IndexedDB.
  - A versioned sidecar holds settings.
- Exit: boot, title, chapter, play, complete, save, relaunch, resume.

**M9: FMV**
- [ ] SMK2 decoder in Rust, written from the public format description rather than LGPL FFmpeg or libsmacker code. Video and audio. Skippable. Same code on native and web. Covers `MOVIE.SMK` and `UK/MOVIE.SMK`.
- Exit: the intro plays in sync on macOS and in the browser.

**M10: HD remaster layer** (port of hdtex)
- [ ] `gfa-tool export-art` writes PNG indices plus a JSON manifest per asset: class (tile/sprite/font/hud/frontend/cut), CLUT, wrap flag, semantic indices, sprite span masks.
- [ ] `tools/hdtex`:
  - Reuse `rrdbnet.py`, `upscale.py`, `palette.py`, `pixelart.py`, `contact_sheet.py`, the `pack.py` split logic, `setup.sh` and `fetch_weights.py`.
  - New code: a manifest-driven `assets.py`, `extract`/`process` stages, and `make_hd.py`.
- [ ] Rules carried over:
  - 64×64 tiles: 3×3 wrap upscaling, so roads have no seams.
  - Sprites: fill, upscale, remask.
  - **Requantize in Oklab to each asset's own CLUT, no dithering**, so car and ped remaps and palette shading keep working.
  - Fonts and HUD art with semantic indices: EPX.
  - Front-end and CUT stills: 4×.
- [ ] Round-trip gate first: export, re-import and pack must be byte-identical at 1×.
- [ ] Engine: optional `GFA_HD.00N.pak` sidecar.
  - Split parts; any subset loads; missing entries fall back.
  - ORIGINAL/HD runtime toggle; `--nohd`.
  - On the web, loaded from the user's local folder.
  - Stays within WebGL2 limits (2048² textures, 256 array layers) by splitting 256² tiles across several arrays or atlas pages.
- [ ] Video: `MOVIE.SMK` frames 2×, with ESRGAN on Mac or NGX VSR on NVIDIA (`nvvsr.py`), requantized per frame.
  - Stored in a tiny `.gfv` container: paletted frames, delta + zstd, original PCM.
  - Decoded by the same Rust code everywhere.
- [ ] Polish:
  - Additive dynamic lights: headlights, siren strobes, muzzle flash, explosions.
  - Smooth speed-based zoom (render only, never the sim view).
  - Optional xBRZ-style shader when no pack is present; optional CRT shader.
- Exit:
  - The toggle works on native and web.
  - No tile seams.
  - Remaps are correct in HD.
  - Contact sheets have been reviewed.

**M11: London 1969 and 1961**
- [ ] Detection: `MISSUK.INI` means 69, `missuke.ini` means 61.
- [ ] `Style001`/`Sty001` styles; London FXT key; `Track02-16`.
- [ ] Investigate `UK.MAP`, `MC.MAP` (CMP-like headers), `AUDIO/*.PK` and `misse8.ini`.
- [ ] Left-hand traffic.
- Exit: London 69 is playable; London 61 loads and plays.

**M12: Polish and release**
- [ ] Settings: render scale, HD toggle, key rebinding, gamepad (gilrs).
- [ ] macOS `.app` bundle.
- [ ] Static web deploy with no data in it.
- [ ] Performance: 60 fps on M-series and in Safari.
- [ ] Docs.

## Gaps beyond AmiGTA1 (full-game parity)
Sources for this list:
- AmiGTA1's own status notes: the README "WHAT IS NOT IN v0.7.1" (`tools/bin/readme_gen.py:620`) and `gta_weapon.h:34` "Not built yet".
- Its in-code `NOT PORTED` and `not yet` markers.
- An audit of what it never models.

Each item is a checkbox in the milestone named, and is tracked in `docs/PARITY.md`. **(verify)** means: settle the original's behaviour first (Principle 8), because AmiGTA1 gives no answer.

**M4 Vehicles**
- Moved on 2026-11-20 because they need later systems: the horn and pull-over and the siren toggle to M5 (traffic), dragging a driver out to M5 (peds); bombs, chain explosions and blast damage to M6 (weapons).

- [ ] **Tank** (model 0x25). AmiGTA1 has only its handling: the engine cut at top speed (`gta_vehphys.c:395`) and its turn rate. Add:
  - the rotating turret, aimed independently of the hull;
  - cannon shells on the fire key: projectile, explosion, wall and car damage;
  - its armour and resistance to damage;
  - pushing and crushing cars;
  - where it spawns, and how it is reached. **(verify)**
- [ ] **Boats and water.**
  - AmiGTA1 places the boat as a map object with no driving (`gta_traffic.c:1152`, `gta_car.c:6`). Add water-only boat handling: ground type 5 is water (`gta_vehphys.c:612`), boarding at the shore, and the boats that missions use.
  - Cars entering water sink and are lost.
  - Finish drowning for the player and peds. The water ticks at `gta_main.c:2253` are only partial.
- [ ] **Map-placed specials**: the boat, tank, helicopter and RC car are placed from the map's object list (`gta_traffic.c:1152`).
  - The helicopter (model 88) is filtered out of traffic (`gta_traffic.c:1186`). Model it as a script-driven vehicle: the escape helicopter's destination (`gta_script.h:82`), takeoff and flight.
  - Drivable or not? **(verify)**
- [ ] **Trains**: follow the bogie path table instead of cutting corners on the chord.
  - Trams stay still, as in the original.
  - Make train boarding, riding and IS_PLAYER_ON_TRAIN robust across all three cities.

**M5 Living city**
- [ ] **Horn and pull-over**: the original's look-ahead of up to five blocks for a car (`gta_main.c:3957`, NOT PORTED).
- [ ] **Emergency vehicles the player drives**: the siren and light toggle in cop cars, ambulances and fire engines, and how traffic reacts to it.
- [ ] **Dragging the driver out** when the player takes an occupied car, and the carjack's score (moved from M4: needs peds).
- [ ] The victim's ped counter-punch (`gta_peds.c:1675`).
- [ ] Settle the gang-role state machine (`gta_peds.c:1111`).
- [ ] **Special ped groups** that the bonuses need: Hare Krishna lines; Elvis impersonators **(verify which levels)**. Spawn rules come from the style and ped tables.
- [ ] Traffic fidelity issues AmiGTA1 lists as open: junction wobble (`gta_traffic.c:5871`) and corner landing error (`gta_traffic.c:5666`).

**M6 Combat, police, services**
- [ ] **Bombs**: the type-4 tanker bomb, whose wreck blows five times (`gta_main.c:4272`, not ported). Find the global that triggers it.
- [ ] **Chain explosions**: a burning wreck sets off its neighbours (`gta_weapon.h:34`).
- [ ] **Blast damage**: a car blowing up hurts the cars and people around it (`gta_weapons_wreck_car`); burnt-out wrecks look burnt (moved from M4).
- [ ] Re-audit `gta_weapon.h:34`, since that list is partly stale, and finish whatever is still missing:
  - weapon crates;
  - friendly-fire groups;
  - armour absorbing hits (the pickup sets `player_armour`; confirm it is read);
  - free-standing fires that rockets leave on walls.
- [ ] **Power-ups that "nothing reads"** (`gta_main.c:10291`). Enumerate every pickup kind from the style's object table, then implement each one's effect and timer: speed-up, the multiplier, the kill-frenzy crate handed to the script, extra life, and the rest.
- [ ] **In-game bonuses AmiGTA1 lacks** (`gta_score.h:17`: "GOURANGA!" not built).
  - GOURANGA, the Elvis bonus, cop-kill bonuses, and any others. **Build the list from the FXT bonus strings**, so nothing is invented.
  - Each pays its points and multiplier and shows its card and voice.
- [ ] The crime counters the pager prints on a bust (`gta_score.h:16`).
- [ ] The police AI's reason for the siren (`gta_main.c:9976`) and the cop's 0x13 call (`gta_snd.h:219`). Check bribes and jail-free cards end to end.
- [ ] Police escalation stays cop cars only (`gta_traffic.h:2622`) unless evidence says otherwise.

**M7 Missions**
- [ ] **Every job of every chapter.** AmiGTA1 played through only the first job of each level.
  - Add one `tools/scripts` playthrough per job as each job is finished.
  - Each job's script must pass in CI. Where the oracle has no answer, record a replay hash instead.
- [ ] Mission-only vehicles and objects: tanker, escape helicopter, train jobs, boat jobs, crane and garage set pieces. They come from the opcode coverage report.

**M8 Front end, audio, saves**
- [ ] **Cheats**: port AmiGTA1's name-entry cheat table (`gta_career.c:223`; flags `GTA_CHEAT_*` in `gta_career.h:69-78`). Also check for Director's Cut and London cheats. **(verify)**
- [ ] **Language select** across the four FXTs, plus the SPECIAL.FXT toggle (`GTA_CHEAT_LANG`).
- [ ] **High-score table**: layout from the research notes in `gta_career.h`; display, entry and save.
- [ ] **Pause menu and in-game options** (audio, controls, HD/original). These are new.
- [ ] **Attract mode**: decode `1/102/202.REP` and play them as front-end demos. If the format resists decoding, record our own demos. This links to the `.REP` stretch oracle.
- [ ] Complete speech and SFX: every VOCALCOM line and its trigger, the MISBRIEF briefings, and the cop 0x13 call. The stolen-channel fade becomes a kira mixer bus.

**M11 London**: all of it is beyond AmiGTA1, whose level table covers sections [1],[2],[102],[103],[202],[203] only (`gta_main.c:167`).

## Cross-cutting
- **Attribution:** `docs/REFERENCE_MAP.md` maps each module to its Carnage3D and AmiGTA1 sources, and module headers credit them, as AmiGTA1 credits Carnage3D.
- **Testing layers:** unit tests; data tests gated on `GFA_DATA`; golden renders and holecheck; replay hashes, native vs wasm; oracle traces; script expectations; mission coverage. Stretch goal: decode the original `.REP` replays as a fidelity oracle.
- **Risks and open questions** (answers go in the decision log):
  - ~~G24 vs GRY as the default~~: settled, G24 (see the decision log, 2026-11-20).
  - Extracting `gta_main.c` cleanly.
  - Whether the oracle needs trace hooks beyond a small patch.
  - WebGL2 texture budget for HD; Safari performance.
  - London's unknown files and left-hand traffic.
  - Features where AmiGTA1 is silent (tank turret, boats, bonuses, attract demos). These need evidence from the original, possibly by observing it under DOSBox-X, which costs time per item.
  - Smacker licensing hygiene.
  - HD pack size on the web.

## Decision log

Newest first. Each entry: date, decision, why.

- **2026-11-20. M4 keeps only what vehicles need on their own.** Pull-over and the siren need traffic (to M5), dragging out a driver needs peds (to M5), and bombs, chain explosions and blast damage need weapons (to M6). Agreed with the owner, so M4 can close on trains, the vault, boats and water, the tank and the specials.

- **2026-11-20. The map's parked vehicles belong to the simulation**, built from the startup objects with a remap byte of 128 or more (Carnage3D's `CreateStartupObjects` does the same); trains and trams stay render-side until their milestone. The renderer leaves those objects out of its static sprites and draws the simulation's cars instead. AmiGTA1 parks its own fleet cars near the start and ignores the map's; its oracle runs therefore use `--oracle`, which clears them.
- **2026-11-20. Vehicle arithmetic is `i64`**, the arithmetic of AmiGTA1's 64-bit host build that serves as the oracle. AmiGTA1's fixed-point helpers keep intermediates inside 32 bits, so the Amiga agrees wherever AmiGTA1 has no overflow bug.
- **2026-11-20. A door blocked by a building or a car refuses the exit for now.** AmiGTA1 climbs out over the roof; that waits for the vault.
- **2026-11-20. `drive record bx by face` is our own script order**, putting the player straight into a car; it lasts a tick like AmiGTA1's fixture orders. A column with nowhere to stand puts the car on the street layer (2), as placing the player on foot does.

- **2026-11-20. The oracle is our own small C harness linking AmiGTA1's modules, not a patched `gtadump`.** `gtadump walk` never builds the nav grid, so fences are off in it; our `walktrace.c` builds the grid and prints a per-tick trace. Nothing of AmiGTA1 is copied into this repo: the harness compiles its sources where they are, in the sibling checkout.
- **2026-11-20. Fences on London's maps stop no one yet.** The solid flat-face tile lists are AmiGTA1's research into GTA's three styles; London's styles have none. Recorded in `docs/PARITY.md` until someone settles them.
- **2026-11-20. The logical sim view is a fixed 10×6 half-blocks** (`SimView::GAME`), covering a 21:9 window at the follow camera's default height. AmiGTA1 derives it from window width and zoom, which makes spawning depend on the display. Oracle runs use AmiGTA1's 5×4. A later milestone may tie it to sim state (the original zooms out with speed), never to the renderer.
- **2026-11-20. A script's `start` faces the player south; a section start uses the `PLAYER` declaration's heading.** Both as AmiGTA1: `gta_player_init` faces south (the art's own facing), and the game then sets the declaration's angle (`b / 4`, 1024 steps to 256).
- **2026-11-20. Test scripts run orders for their tick count and release the keys when the script ends.** AmiGTA1's harness also drops the keys on the last tick of the last order, a quirk of where it advances its queue; that is not game behaviour and is not copied.
- **2026-11-20. Trig tables are generated from their formulas, not copied.** `round(cos(2πi/256)·16384)` and `round(atan(i/64)·256/2π·65536)` reproduce AmiGTA1's tables exactly; a unit test rebuilds them with floats.
- **2026-11-20. Random number generators use 32-bit arithmetic**, as on the Amiga. AmiGTA1's host build keeps 64-bit seeds; masked draws agree, but its train generator's `seed >> 16` does not, so host-oracle train traces (M4) must allow for it.
- **2026-11-20. Colour 0 is see-through only on flat blocks** (Carnage3D), not on every side face (AmiGTA1). Drawn in 3D, AmiGTA1's rule opens holes through solid walls, whose windows use colour 0 for black: the first NYC render showed them at block 64,64. AmiGTA1's 2D painter drew the ground first, so the holes never showed there.
- **2026-11-20. Sloped side faces lose their top rows rather than squashing the tile.** The face keeps the tile's scale, as a wall cut by a ramp would. Carnage3D squashes; AmiGTA1 draws full-height walls. To check against the original if it ever looks wrong.
- **2026-11-20. Flat blocks are plates whose two sides run the same way in the world**, and a missing side borrows the other's tile (AmiGTA1). Back-face culling decides which side shows, as the original's straight-down camera did. Signs read correctly; a flat block nudges half a pixel towards the eye to win against the wall it shares a plane with.
- **2026-11-20. Sprites are drawn at heading + half a turn.** The art faces south (AmiGTA1's `GTA_SPRITE_ART_SOUTH`). The map's startup rotation is clockwise from north (Carnage3D), which puts parked cars facing along the traffic.
- **2026-11-20. Tile animations count the original's 25 Hz frames** (AmiGTA1's reading; Carnage3D uses 24). Render-side, so it cannot affect the sim. Each animation shows its own tile, then its aux frames (Carnage3D).
- **2026-11-20. One 2048-wide atlas per kind of art, plus small table textures**, read with `textureLoad` and coloured in the shader. It keeps to WebGL2's limits (256 array layers, 2048² textures) and needs no samplers. HD packs (M10) will split atlases when they outgrow 2048².
- **2026-11-20. The city camera works in map pixels with `x` east, `y` south, `z` up**, flipping `y` only inside the view matrix. Map, sim and renderer then share one frame.
- **2026-11-20. London 1961 uses `STY00n` styles; GTA and London 1969 use `STYLE00n`** (Carnage3D `GetStyleFileName`). A map's game is the one whose mission file names it.
- **2026-11-20. The web page owns the folder picker and IndexedDB cache; Rust receives files through two exported functions.** DOM and IndexedDB are far simpler in the page's script than through `web-sys`, and the engine still sees a synchronous `MemSource`.
- **2026-11-20. This is a source port: it runs the original game from the player's own copy, and recreates none of its content.**
  - What it is: our own engine code, plus knowledge from the MIT-licensed Carnage3D and AmiGTA1, reading the 1997 game's `GTADATA` files at runtime. The art, maps, missions, text, sound and video all come from the player's files.
  - What it never does: put game files or anything derived from them in the repo (Principle 6), redraw the game's art, rewrite its levels or missions, or copy its text into code. HD packs are generated locally from the player's files and stay local.
  - Why: a port that needs the original data adds a way to play a game people already own without copying it, like Carnage3D, OpenMW and OpenRCT2. A clone that rebuilt the game's content would copy it instead. Midway through M2 work stopped over this question; the owner confirmed the source-port intent and work resumed.
  - Target data: the 1997 game's `GTADATA` folder, as in a Director's Cut install. The 2021 "Definitive Edition" is the 3D trilogy, with other formats, and is out of scope.
- **2026-11-20. The app is called Grand Fair Use Auto; the exe is `gfa`.** The window title is "Grand Fair Use Auto", plus the level's name once one is loaded.
  - Never titled "Grand Theft Auto": that is Take-Two's trademark, and the app must not look official. Compatibility is described by naming what it runs ("plays Grand Theft Auto (1997) from your own copy"), with the non-affiliation notice in the README.
  - The name riffs on the trademark. If the project is ever published widely, consider a more distinct name before then.

- **2026-11-20. Front-end and briefing pictures come from the 24-bit `.RAW`.** It is the master; the `.RAT` is a quantised copy. This matches the G24 decision and gives HD upscaling the best source.
- **2026-11-20. G24 is the default style format, for rendering and for the sim's tables.**
  - The G24 is the richer file: 25 more HUD sprites, longer tile animations, and per-sprite CLUTs that HD requantization can target.
  - The base game's car and object tables are identical in both formats, so the sim loses nothing.
  - London's GRY carries the base game's car table, not London's, so only the G24 is correct there.
  - The GRY is still parsed, for oracle parity with AmiGTA1 (which reads only GRY) and for cross-checks.
- **2026-11-20. Parsers keep raw records; game rules decide later.** The CMP parser keeps the map column-compressed as in the file, keeps duplicate startup objects, and keeps ground and direction bits where the file puts them (one block up). The sim chooses Carnage3D's or AmiGTA1's interpretation, with AmiGTA1 winning on gameplay (Principle 1).
- **2026-11-20. Unused-dependency warnings are kept on.** Cargo 1.100 warns on unused (workspace) dependencies. Crates gain dependencies when they first use them, rather than declaring them all up front.
- **2026-11-20. Drawing pauses while the window is hidden.** On macOS an occluded window makes every acquire return `Occluded`. Without the pause, the loop spun at about 5,000 iterations per second. While hidden, the app probes four times a second and resumes on `WindowEvent::Occluded(false)`.
- **2026-11-20. Non-sRGB surface formats are preferred** (`Bgra8Unorm` on Metal, `Rgba8Unorm` on WebGL2). Palette bytes then reach the screen exactly as the original drew them. Any lighting or HD blending that wants linear space will convert in the shader.
- **2026-11-20. WebGL2 limits everywhere; only the texture size follows the adapter.** The device uses `Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits())`, so a Retina surface (2560x1600 and up) can be configured. Game art must still keep to `gfa_render::ASSET_TEXTURE_MAX` (2048).
- **2026-11-20. Web: WebGL2 only; the WebGPU backend is not enabled.** One code path and one set of limits. We can revisit once the renderer is complete.
- **2026-11-20. Web data access is synchronous after load.** `AssetSource` is a sync trait (`FsSource` natively). The web build will read the picked folder into a `MemSource` (cached in IndexedDB) before the game starts. This replaces the "async AssetSource" idea in the layout above.
- **2026-11-20. Toolchain is Rust stable (1.100) + wgpu 30 + winit 0.30.** Trunk 0.22.2 required rustc ≥ 1.96.1, so the toolchain was upgraded. winit 0.31 is still in beta.
- **2026-11-20. Data folder = the folder containing `GTADATA/`.** Resolution order: `--data`, then `GFA_DATA`, then `gfa.local.toml` found by walking up from the working directory. A path to `GTADATA/` itself is lifted to its parent. London lives in `GTADATA/UK/` (Director's Cut layout).
- **2026-11-20. Game data and derived art are never committed.** Case-insensitive `.gitignore` patterns cover every original data extension; HD packs and exports stay local.
- **2026-11-20. Extras in scope: HD polish, London 69/61, FMV intro, hdtex port.** Multiplayer is out.
- **2026-11-20. Deterministic fixed-point sim in AmiGTA1's exact units** (16.16 at 32 px/block, 256-step angles, Q14 trig, 50 Hz base). The ported C keeps its tuned constants, and AmiGTA1 can act as a differential oracle. Gemini's rapier2d suggestion was rejected.
- **2026-11-20. Rust + wgpu (Metal native, WebGL2 web), one codebase.**

## Progress log

Newest first.

- **2026-11-20. M4 stage 2: damage and wrecks.**
  - Dented panels from wall hits, sprite deltas in the atlas (still within 2048 rows on every map), door art on the way in and out, write-offs that burn and blow up, convertible drivers, and a fix: the car being got into was not drawn during the get-in.
  - Oracle and native-vs-wasm checks unchanged and green (nine scripts).
- **2026-11-20. M4 stage 1: the driven car.**
  - `gfa_sim::vehicle` (physics, wall resolve, layer rule), `gfa_sim::cars` (parked cars, door geometry, the per-tick drive with jumps), `World` state machine (on foot, entering, driving, exiting), `park` and `drive` orders.
  - Rendering: parked and driven cars drawn from the simulation, interpolated between ticks; the camera follows the car.
  - Oracle: `drivetrace.c`, `compare.sh` for driving scripts, `fuzz.py --drive`; native and wasm agree on all eight scripts.
  - Totals: 59 unit tests, 14 data tests.
- **2026-11-20. M3 done.**
  - `gfa-fixed`: trig, generators and 16.16 helpers. `gfa-sim`: `clock`, `map`, `player`, `world`, `view`, `input`, `script`, `replay`, `script_world`, `hash` (FNV-1a over a fixed encoding).
  - `gfa-app`: the player runs in the window at 50 Hz, drawn between ticks; the camera follows him; the overlay shows his tick, block, layer and ground.
  - `gfa-tool run`, `tools/scripts/` (four scripts), `tools/oracle/` (walk harness, compare, fuzz), `tools/wasmcheck/` (native vs Node), and the `gfa-replay-wasm` crate behind it.
  - `LevelFiles` now carries its mission section; `Dataset` parses from `gta`/`london69`/`london61`.
  - Totals: 57 unit tests, 13 data tests; fmt, clippy, wasm-check and a release Trunk build (6.8 MB wasm) green.
- **2026-11-20. M2 done, except Safari.**
  - `gfa_render`: `city_mesh` (faces, slopes, lid rotation, flips, flat blocks, 16×16 chunks), `style_tables` (tile atlas, palettes, remap and animation tables for GRY and G24), `city` and `city.wgsl`, `sprites` and `sprites.wgsl`, `camera` (perspective, frustum, follow), `offscreen`.
  - `gfa_assets::level`: finds a section's map and the style beside it, by game.
  - `gfa-app`: loads `--game`/`--level`, flies the debug camera, draws the city and startup sprites, egui overlay with a block inspector, `--screenshot`. Web: folder picker, IndexedDB cache, `?game=&level=`.
  - `gfa-tool render` and `gfa-tool holecheck`: 0 uncovered pixels on all six maps.
  - Settled from the data: GRY lid remaps (FORMATS.md), startup object height and rotation, sprite art facing south, London 1961's style name.
  - Midway through, work stopped over whether a source port counts as recreating the game; the owner confirmed the source-port intent (decision log), and work resumed.
  - Totals: 38 unit tests, 12 data tests; clippy, wasm-check, test, data-test and a release Trunk build (6.8 MB wasm) all green.
- **2026-11-20. M1 done: MISSION.INI.**
  - `gfa_formats::mission`: sections, declarations with every number, commands. Handles the glued `-1[1003]` header, a flag touching its bracket, a full stop typed for a comma, backslash map paths and the DOS end-of-file byte. 2 unit tests.
  - Data test over MISSION.INI, MISSUK.INI, missuke.ini and misse8.ini, matching AmiGTA1's counts for [1] and [203].
  - Findings: the race texts belong to the multiplayer "Canonball Run" sections; MC.CMP is London 1961's deathmatch map.
  - `gfa-tool mission`.
  - Totals for M1: 11 parser modules, 24 unit tests, 11 data tests; clippy, wasm-check, test and data-test all green.
- **2026-11-20. M1: pictures, saves, CD map, probes.**
  - `gfa_formats::screen` (palettes, `.RAW`/`.RAT` with sizes found from the data), `career` (PLAYER_A.DAT, raw bytes and accessors), `cue` (CDAUDIO.DAT). 4 unit tests.
  - Data tests: 140 pictures and palettes, the save, both cue sheets (every file they name exists).
  - Probes: both MOVIE.SMK are 320×200 SMK2; the `.REP` demos are `{tick, input}` logs.
  - `gfa-tool export` handles `.RAW`/`.RAT` pictures.
- **2026-11-20. M1: texts, fonts, sound banks.**
  - `gfa_formats::fxt` (cipher, NUL padding, `[]` end marker, first-wins duplicates, Windows-1252), `fon` (glyphs, own palette, Carnage3D's accent table) and `sdt` (index, bounds, contiguity). 6 unit tests.
  - Data tests: every FXT, FON and SDT in GTADATA, GTADATA/UK and GTADOS/DATA parses; every bank tiles its RAW exactly.
  - Glyph sheets confirm the accent mapping: 140-glyph fonts have both cases, 118-glyph fonts upper case only.
  - The FXT is now an evidence source for PARITY.md: GOURANGA (8787), the big cards (4000–4005), respray refusals including the tank's, and race checkpoint lines.
  - `gfa-tool text` and font export.
- **2026-11-20. M1: style files.**
  - `gfa_formats::style` reads GRY v325 and G24 v336: tiles, animations, colour (GRY palette, remap tables and lid remaps; G24 CLUTs and palette index), objects, cars, sprites with deltas, and sprite category counts. 3 unit tests.
  - Data test: all nine style files parse, each map's tiles exist in its style, the base GRY/G24 pairs agree on objects and cars, and G24 CLUT counts match Carnage3D's integrity checks.
  - Findings: STYLE003.GRY declares one animation more than it holds; London's GRY has the base game's car table; the G24s add 25 HUD sprites.
  - `gfa-tool style <path> [--cars]` and `gfa-tool export <style>` (contact sheets of tiles, sprites, remaps and deltas, written to the gitignored `export/`).
- **2026-11-20. M1: CMP parser.**
  - `gfa_formats::cmp` with 2 unit tests, plus data tests that parse all six Director's Cut maps (NYC, SANB, MIAMI, UK, MC, UKE) and validate every column.
  - `gfa-tool map <path> [--at X,Y]`.
  - Found that `UK.MAP` and `MC.MAP` are CMP-headed, uncompressed editor maps (`docs/FORMATS.md`).
- **2026-11-20. M0 done.**
  - Workspace with 10 crates (`gfa-fixed`, `gfa-formats`, `gfa-assets`, `gfa-sim`, `gfa-render`, `gfa-audio`, `gfa-app`, `gfa-native`, `gfa-web`, `gfa-tool`).
  - Case-insensitive data guard in `.gitignore`, checked with `git check-ignore` against sample names.
  - `gfa-assets`: data-path resolution, case-insensitive `FsSource`, `MemSource`, dataset detection. 7 unit tests, plus 1 data test that finds GTA, London 69 and London 61, all with music.
  - Native `gfa`: Metal window on the Apple M1 Max at 2560x1600, about 120 fps with vsync.
  - Web: Trunk build; Chrome runs WebGL2 through ANGLE on Metal, and the surface tracks the canvas (2560x1600).
  - `gfa-tool info` prints the data folder and the games found.
  - `cargo build`, `cargo test`, `cargo wasm-check`, `cargo data-test` and clippy are all green.
