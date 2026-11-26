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
8. **AmiGTA1 is the floor, not the ceiling.** The target is the whole original game, so anything AmiGTA1 lacks or leaves partial is scheduled work (see _Gaps beyond AmiGTA1_). Where AmiGTA1 is silent, establish the original's behaviour before building it, and log the evidence in the decision log. Evidence sources, in order:
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
- [x] `docs/PARITY.md` seeded from _Gaps beyond AmiGTA1_.
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
  - `SimView::GAME` is 10×6 half-blocks (a 21:9 window at the default camera height; 12×7 since M5 stage 4); `SimView::AMIGTA1` (5×4) is for oracle runs.
  - `gfa_sim::map`: ground types with the missing-pavement rule, the nav byte (railway blocks zeroed, dirt roads as road), thin walls from the solid flat-face tile lists, ramps (`slope_up_dir`, `slope_is_top`), `wall_cross`, and `ground_z` for the height on a ramp.
  - `ScriptWorld` holds the M3 callbacks and a table of which AmiGTA1 callbacks each later milestone adds; unsupported calls return `Unsupported` instead of succeeding.
- [x] Input becomes a recorded command stream. Parse the AmiGTA1 order format (`wait/run/enter/goto/park/…`, `gta_main.c:6875-7084`).
  - `TickInput` is the buttons held on a tick; `InputLog` records the ticks where it changes (`gfa-input 1` text format) and plays them back.
  - `Script` reads both AmiGTA1 formats in one file: orders (`autodrive.txt`) and walk legs (`gtadump walk`), with `start`, and `# section:`, `# expect:` and `# reject:` headers. Every order is recognised; fixtures for later milestones log "not supported yet".
- [x] `gfa-tool run --section N --script f --ticks T [--hash-every K]` checks the expected-log lines from the script header.
  - Also `--trace` (per-tick player state, the oracle's format), `--trace-traffic` (the fleet every tick, traffictrace's format), `--out` (PNGs for `dump`/`film`, with the player drawn), `--record` (save the input) and `--oracle` (AmiGTA1's view size and no parked cars, as its harnesses run).
  - Scripts live in `tools/scripts/`.
- [x] Player on foot. Ref: `gta_player.c`.
  - Run by default, Shift walks; turning, backing up at half pace, sliding along walls, fences, ramps between layers (NYC's bridge at 19,44 lands on layer 3), the walk and run cycles.
  - In the app: arrows/WASD drive him, the follow camera trails him, `C` detaches the camera to fly.
- [x] Oracle: build AmiGTA1's `gtadump` with clang, directly from a sibling `../AmiGTA1` checkout into `tools/oracle/build`. Bake `.til` from the user's GRY with `gtabake`. A small trace patch lives in our repo.
  - Instead of patching `gtadump`, our own harness (`tools/oracle/walktrace.c`) links AmiGTA1's `gta_map.c`, `gta_nav.c`, `gta_player.c` and `gta_trig.c` from the sibling checkout and prints a per-tick trace. The `oracle_*` tests (`cargo oracle-test`) compare it tick by tick with gfa-sim, on the walk scripts and random walks over all three cities.
  - Baking `.til` is not needed for walking (the player needs no style), so it waits for the first oracle check that reads car data (M4).
- Exit:
  - You can walk every level.
  - The same script gives identical hashes on native and wasm (run in node).
  - `walk` traces match the oracle.
  - Met:
    - Every mission section of all three games (28 levels, multiplayer included) starts the player on walkable ground, and he runs freely from there (data test `every_level_is_walkable`). Native app tested on Liberty City; the web bundle builds with the sim in it.
    - Scripts run natively and as wasm in Node 24 give identical logs and hashes (now the `wasm_matches_native` test).
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
- [x] Door-by-door enter and exit; vault the car; drag out the driver; bikes and convertibles.
  - Done: parked cars from the map's startup objects (trains and trams excepted) are sim-owned; RETURN walks to the nearest car's door within 48 px and runs the ten-step get-in; the get-out refuses above speed 5 and plays nine steps while the car rolls; bikes mount and dismount on their own frames; a walker cannot walk through a car.
  - Also: a convertible shows its driver, a bike its rider.
  - The vault (`gfa_sim::world::Vault`, AmiGTA1's states 0x73.., sprites 91..94): getting in from the far flank runs at the near flank and goes over the roof to the door; a door blocked by a car or a building puts him out over the roof the other way; SPACE while running at a low car goes over it, at a tall one (`vert` 20 or more) slides him under. `tools/scripts/vault.txt` does all three on Liberty City. Dragging out a driver moved to M5.
- [x] Trains.
  - `gfa_sim::trains`: AmiGTA1's `gta_train.c`. The rail walked into loops, the bogie path of straights and quarter circles, stations and spawn points, stops, doors, waits and the block signal, crushing cars, wrecking on collision, running over the player; boarding a stopped train, riding, getting off beside pavement, taking the controls with FIRE. Trams (San Andreas) stand still, as in the original.
  - Liberty City: 3 trains on a 662-block loop with 5 stations; Vice City: 2 trains, 3 stations; San Andreas: 6 trams. Drawn as AmiGTA1 does, three train sprites a carriage.
  - `tools/oracle/traintrace.c` runs AmiGTA1's own `gta_train.c`; `oracle_trains` matches 12,000 ticks of every city's trains. `tools/scripts/train_ride.txt` boards, rides, hijacks, hands back and gets off.
- [ ] Use `tools/vehruler.c` as the measurement reference.
- [ ] Close the M4 items in _Gaps beyond AmiGTA1_: trains; cars sinking and drowning. (Pull-over, the siren, bombs and chain explosions moved to M5 and M6; the tank, boats, helicopter and RC car to M6b.)
  - Water done on 2026-11-21. Trains wait on a comparison with the original's path table.
- Exit: every class AmiGTA1 drives (cars, bikes, buses, trains) drives; the (91,105..108) jump lands; vehicle traces match the oracle.
  - So far: the jump lands; `tools/oracle/drivetrace.c` (AmiGTA1's physics, wall and layer code, with its main loop's glue restated) matches gfa-sim on `drive_oracle`, `drive_jump`, `drive_wall`, `drive_sink` and 24 random drives over the three cities with random vehicles (`oracle_drive_scripts`, `oracle_random_drives`).

**M5: Living city.** Ref: `gta_traffic.c` (10.6k lines), `gta_route.c` (BFS: 48-block window, 700 nodes), `gta_nav.c`, `gta_peds.c`.

- [x] Fleet spawn and despawn on the logical view.
  - Nothing may pop in or out where the player can see it. The camera's lead and any speed zoom become sim state (from position and speed only), the spawn and despawn rings follow them with AmiGTA1's margins (+2..+6 blocks spawn, +9 despawn), and in normal play the renderer never shows more ground than that view: zoom-out is capped and windows wider than 21:9 are pillarboxed. Free look, fly and extreme zoom are debug views where pop-in is accepted.
  - A pop-in check, like the holecheck: run traffic and fail if anything spawns or despawns inside the visible ground at the largest allowed zoom and aspect.
  - Stage 1 (2026-11-21): the fleet's rings follow `SimView` (half-width 10 blocks in the game), centred on the player or his car, never on the window.
  - Stage 4 (2026-11-21): `gfa_sim::view::SimCamera` leads a moving player by 0.35 s of travel (at most a third of the view) and widens the view up to 1.5× with his speed; the fleet's rings and the crowd's edges follow it. `SimView::GAME` is 12 by 7, measured on layer 1, the lowest ground anything stands on. The follow camera centres on the simulation's lead, caps its height so it never shows more than the view (`max_depth`, a block spare), and pillarboxes windows wider than 21:9. The pop-in check (`nothing_pops_in_or_out_in_sight`) drives the fastest car and runs on foot in all three cities at 21:9, 16:9 and 4:3: about 17,600 comings and goings, none in sight; its control without the cap sees 2,369 of 2,445.
- [x] AI driver: lanes, junctions, give way, queues, S-curve lane change, U-turn.
  - `gfa_sim::traffic` (with `route`): AmiGTA1's `gta_traffic.c` with its switches at their defaults. Routes by breadth-first search over the arrows, pure-pursuit lane keeping, turns on circular arcs, junction bookings and convoys, the occupancy table, following gaps, lane changes and U-turns when blocked, head-on backing off, escapes out of sight, cruising on clear straights, far cars driving at half or quarter rate.
  - `tools/oracle/traffictrace.c` runs AmiGTA1's own traffic; `oracle_traffic` matches it car for car at the four `fullgate` sites, seeds 1 to 3, 3,000 ticks each.
- [x] Traffic lights.
  - The lights' clock and the stop line (the block's traffic hint). Drawing the lights waits for the renderer's sprite work.
  - Stage 5 (2026-11-21): drawn. `gfa_sim::traffic::roads::lamps` finds AmiGTA1's lamps (two on each lit corner, one for each road beside it) with each lamp's crossing and facing; the app and `gfa-tool` pictures draw the style's lamp sprites lit red, green or amber, under the crowd and the cars, the lens toward the drivers coming in. Each crossing now has one light: AmiGTA1 timed a light by the 8×8 neighbourhood of the block asking, which split 59 of the three cities' 79 lit crossings (decision log). Liberty City has 148 lamps at 24 crossings, San Andreas 150 at 28, Vice City 156 at 27.
- [x] SAT `box_mtv` collisions with impulse and spin (`fleet_collide`, around `gta_traffic.c:8067`).
  - Within the fleet, and (stage 2) the player's car against it: stopped at the contact by eight halvings of the move (`gta_traffic_sweep_box`), then the ram (`gta_traffic_ram`): both pushed apart by mass, the struck car knocked loose with a spin or leant on, bodywork charged on a real impact.
- [x] The player and the fleet (stage 2, 2026-11-21).
  - Every car but his is in the fleet: the traffic, the map's parked cars (kept all level), the cars he leaves (AmiGTA1's `gta_traffic_abandon`, under their old serial), wrecks, and train wrecks. The fleet has 192 slots, not AmiGTA1's 60, so Liberty City's 106 parked cars fit; AmiGTA1's own runs never reach 60, so the oracle is unchanged.
  - RETURN takes the nearest car within reach out of the fleet, moving or standing (`gta_traffic_grab_car`). A moving car's driver is not thrown out yet: that needs the peds.
  - On foot he is held off fleet cars (`ped_car_block`); the vault, the slide and the blocked door ask the fleet. Trains crush fleet cars (20 points), and write-offs in the fleet burn and burn out (`gta_weapon.c`'s sweep; the blast is M6).
  - `walktrace.c` and `drivetrace.c` now run the fleet and the trains (`tools/oracle/fleet.h`), and the walk and drive oracles compare the whole fleet on every tick: 71 runs match, 11 of them with rams (46 in all).
- [x] Peds on pavements, crossings, knockdowns, panic (stage 3, 2026-11-21).
  - `gfa_sim::peds`: AmiGTA1's `gta_peds.c` crowd. People come on at the edge of the view ahead of the player and go once they have been out of sight a while; they wander the pavements, pick a gait, step round each other, cross at lit crossings, run for the pavement when caught in the road, dodge a car coming at them, lie down when knocked over, and leave bodies that stay for 30 seconds.
  - In the world, in AmiGTA1's order: the crowd learns where the player is, people on their feet are walkers the fleet stops for, the fleet and trains tick (trains now kill peds on the line), then the crowd ticks, then the player's car runs over whoever is in its box.
  - A taken car's driver is dragged out once the door is fully open, or knocked off a bike (AmiGTA1's `gta_peds_pull` and `_knock_off`); he lies in the road for a while, then gets up and walks off. The carjack's score and a cop car's driver coming after him are M6's.
  - The pool is sized to the view: 12 at AmiGTA1's view (the oracle), 33 at the game's. Ours: a ped who ends up in the water drowns (decision log).
  - Drawn in the app between ticks and in `gfa-tool` pictures, in his own dress (`PED_REMAP_TABLES`, measured).
  - The walk and drive oracles compare every living ped on every tick (`tools/oracle/fleet.h` runs AmiGTA1's crowd): all 71 runs match.
  - Not yet (M6/M7): shooting and punching people, panic at gunfire, fire, cops on foot, the script's own people.
- [x] Close the M5 items in _Gaps beyond AmiGTA1_: junction fidelity, pull-over, the siren. (The counter-punch and special ped groups moved to M6, gang roles to M7, on 2026-11-21.)
  - 2026-11-21: the siren and the horn done; the pull-over built with provisional numbers, which M6b measures; junction fidelity measured and closed (decision log).
- Exit: the `drive` traces from `fullgate`/`litegate` (4 sites × 3 seeds × 12,000 ticks) match the oracle, with no deadlocks.
  - Met for the traffic (2026-11-21): `oracle_traffic` runs `fullgate`'s twelve runs in `gtadump drive`'s setup and every trace matches AmiGTA1 (patched as `tools/oracle/build.sh` lists); `traffic_gate` passes all twelve on AmiGTA1's own checks (on the road, none inside another, none vanishing in view, the fleet moving) and ours (no car standing a minute). `litegate`'s lite mode is not in this AmiGTA1. AmiGTA1 unpatched passes 3 of the 12. The gap items below remain.

**M6: Combat, police, services.** Ref: `gta_weapon.c`, `gta_pickup.c`, `gta_score.c` (heat thresholds such as LC 151/251/351/501), `gta_traffic.c:9681-10500` (cop dispatch, roadblocks), `gta_peds.c:1912-2066` (arrest, shooting), `gta_service.c`, `gta_marks.c`, plus the `gta_main.c` busted/wasted/siren code.

- [x] Five weapons, rockets and flames, fires and explosions; crates.
  - Stage 1 (2026-11-21), the weapons: `gfa_sim::weapons`, AmiGTA1's `gta_weapon.c`. The fist (the standing punch, its blow on the third state, ten of thirteen landing), the pistol, the machine gun, the rocket launcher and the flamethrower; the shot pool of 40 with its muzzles, speeds, ranges and grace ticks; hits in the original's order (flat faces, then the player, people, cars, the block); bullet damage by body class; the panic a shot starts; explosions (kill, burn and write-off rings), the three fires a rocket leaves on a wall, the script's face explosion for M7, and cars blowing up in one, three or five blasts, which write off their neighbours: the chain. The player is a target too (bullets, his car's bodywork, blasts, burning) down to WASTED, which takes his guns and the heat.
  - The score with it: `gfa_sim::score`, AmiGTA1's `gta_score.c` (awards by type, reason and streak; the heat and each city's thresholds; the crime counters), wired to the run-over, the carjack, the shunt, touching a police car, the hijacked train and every hit above.
  - The walk oracle runs AmiGTA1's weapons and score with the fleet and the crowd, and compares every shot, explosion, fire, ped state, car's damage and the score on every tick: 6 new scripts (`punch`, `shoot_ped`, `rocket`, `rocket_wall`, `rocket_close`, `flame`) match, and so do all 71 earlier runs.
  - Drawn in the app and in `gfa-tool` pictures (the shots, the splats, the fires and the explosion's four quarters; the punching and armed player).
  - Stage 2 (2026-11-22), the crates and power-ups: `gfa_sim::pickups` (`gta_pickup.c` and `gta_main.c:10151-10296`). Closed/open crates (`0x54`/`0x55`) opened by shots, cars or blasts; all eleven item kinds (`0x4e..0x51`, `0x5f..0x65`) including speed-up (`375` ticks double-step), bribe, armour, multiplier, get-out-of-jail-free key, extra life, and kill-frenzy endless gun.
- [x] Wanted levels, chases and rams, roadblocks, arrest; BUSTED and WASTED, lives, hospital.
- [x] Ambulance and fire engine. Spray and bomb shops.
- [x] Tyre marks, blood and oil for all cars. AmiGTA1 only marks the player's car.
- [x] Close the M6 items in _Gaps beyond AmiGTA1_: crates, armour, friendly fire, wall fires, power-ups, bonuses, police reasons, bust tally, bombs, chain explosions, blast damage. (The `(verify)` items — the counter-punch, special ped groups, and type-4 tanker bomb global — wait for M6b's DOSBox-X probe.)
- Exit: the bust, death and spray-escape loops work; the AmiGTA1 `car_*`, `bankblast` and `blood*` scripts pass.
  - Met (2026-11-22): all M6 synthetic tests (`synthetic.rs`) and scripts (`car_*`, `bankblast`, `blood`) pass.

**M6b: Measured from the original.** The vehicles AmiGTA1 does not drive, measured from the DOS original running under DOSBox-X (Principle 8). After M6, because the tank's cannon uses its explosions.

- [ ] Probe harness, `tools/dosprobe`. DOSBox-X runs `GTADOS/GTA8.EXE` from the owner's data folder with a generated config.
  - Key presses come from a script timed in emulated time, not wall-clock time, so a run repeats exactly.
  - Readouts are screenshots, with sprite position and angle measured from the pixels, and memory dumps once the player's vehicle is found in memory.
  - Nothing it captures is committed. The input scripts and the measured numbers are.
  - The harness also serves the later **(verify)** items: special ped groups, the counter-punch, gang roles, bonuses, cheats.
  - Overlay test maps: `run.py`'s DOSBox-X `-t overlay` layer can inject either synthetic `CmpBuilder` `.CMP` + custom `MISSION.INI` arenas (run side-by-side in both `GTA8.EXE` and `gfa-tool`) or `MISSION.INI`-only overrides over real cities without touching the owner's game data; see `docs/M6B_PROBES.md` for the per-probe checklist and results.
- [ ] Boats: handling on water, getting on and off, what happens at the shore.
- [ ] Tank: its handling against AmiGTA1's; the turret; the cannon; its armour; pushing cars; where it is found.
- [ ] RC car and helicopter: whether either can be driven, and how. The escape helicopter's scripted flight is M7's.
- [x] The counter-punch: what the victim's punch back does to the player (the tenth blow that lands: he goes down about two seconds, no harm done; the wheel resets, so it is 11 long). Done 2026-11-26.
- [ ] Gang roles: the state machine of the original's gang AI types (`gta_peds.c:1111`: AmiGTA1 has read the police ones only). For M7.
- [ ] Special ped groups: where Hare Krishnas and Elvises appear, in which cities, how often and how they walk. For M6.
- [ ] The weapons where AmiGTA1 chose for itself: weapon fire rates/cooldowns and projectile speeds (the owner notes guns fire too fast at 50 Hz); how long the original's bullet splat stays on the map; whether every car blowing up pays the player 100, whoever wrote it off; whether a drowning man can fire (fire-key latch across `WASTED` settled in AmiGTA1's favour without DOS probe, 2026-11-25).
- [ ] The pull-over: how far to the side the look reaches, how slowly the car moves over, whether it stops and for how long, whether the siren left on does it too. (Unlisted FXT police-toggle and three-zoom-level hotkeys dropped per owner decision, 2026-11-25.)
- [x] Getting into a car, frame by frame against the original. The owner reports that ours looks as if he goes in through the wrong side of the door (2026-11-21). Check the door point, the walk-in path, the sprite frames and the door's opening delta (`docs/M6B_PROBES.md` Section 2; also settled slide-under from outside, far-side tall-vehicle detour, flush-wall vault turn & entry refusal, and forward blocked-door exit).
- Exit: each drives as measured; the numbers and the probe scripts that produced them are in the decision log.

**M7: Mission VM, HUD, text.** Ref: `gta_script.c`, `gta_script_run.c` (`cmd_names[]` at lines 35-172; `step()` at 645; `gta_script_tick` at 2691), `gta_script.h:203-220` and `:459-663`, the `gta_main.c` callbacks (doors, cranes, bombs, the arrow), `gta_hud.c`, `gta_font.c`, `gta_text.c`, and `gta_main.c:2615-2830` (text displays).

- [ ] Port the VM: declarations, the command table, 32 processes, triggers (TRIGGER/MPHONES/CARTRIGGER/DUM_MISSION_TRIG), phones, arrows, garages and doors, cranes, kill frenzies, bank robbery.
- [ ] **Unknown commands log section and line and count toward coverage. They do not silently succeed.**
- [ ] `docs/MISSION_OPCODES.md` lists every command used in GTA's `MISSION.INI` with its status. Sources: AmiGTA1's table, Carnage3D `docs/ped.txt`, community mission docs, and observed behaviour.
- [ ] `gfa-tool mission coverage` produces the coverage report.
- [ ] Original HUD: cop heads, weapon and ammo, armour, jail-free card, the pager scroll, the brief box, the big two-line card, score and multiplier fonts.
- [ ] Close the M7 items in _Gaps beyond AmiGTA1_: every job, a playthrough script per job, mission-only vehicles, gang roles.
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
- [ ] Close the M8 items in _Gaps beyond AmiGTA1_: cheats, language, high scores, pause and options, attract demos, complete speech and SFX.
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
  - Smooth speed-based zoom: done in M5 as simulation state (`SimCamera`), so the view it shows is the view that spawns.
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
- [x] Static web deploy with no data in it. (2026-11-21: GitHub Pages, from `.github/workflows/pages.yml` and `tools/web/build.sh`; `vercel.json` also builds it on Vercel.)
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

- Moved on 2026-11-20 to M6b, to be measured from the original under DOSBox-X: the tank, boat handling, the helicopter and the RC car.

- [x] **Water.**
  - Cars entering water sink and are lost.
  - Finish drowning for the player and peds. The water ticks at `gta_main.c:2253` are only partial.
  - 2026-11-21, done for the player.
    - On foot, with UP held, water 10 px ahead with only air between takes him in.
    - A jump that falls short over water loses the car (`car: sank`).
    - Under water he cannot move and loses 3 health every other tick. At none he is wasted, and until M6 he starts again where the level put him (`World::health`, `Doing::Drowning`).
    - Both harnesses restate the rule, and `walk_water.txt` and `drive_sink.txt` match AmiGTA1 tick for tick.
  - Peds drowning moved to M5, which brings the peds.
- [ ] **Trains**: follow the bogie path table instead of cutting corners on the chord.
  - Trams stay still, as in the original.
  - Make train boarding, riding and IS_PLAYER_ON_TRAIN robust across all three cities.
  - 2026-11-20: AmiGTA1 has since replaced the chord with straights and quarter circles, which is ported; the original's nine-shape path table itself is still to compare against. Boarding, riding, getting off and driving work on Liberty City (`train_ride.txt`); IS_PLAYER_ON_TRAIN comes with M7.

**M5 Living city**

- [ ] **Horn and pull-over**: the original's look-ahead of up to five blocks for a car (`gta_main.c:3957`, NOT PORTED). The horn is done; the pull-over is built with provisional numbers, to measure in M6b.
- [x] **Emergency vehicles the player drives**: the siren and light toggle in cop cars, ambulances and fire engines, and how traffic reacts to it (the pull-over, above). The siren's sound is M8's.
- [x] **Dragging the driver out** when the player takes an occupied car (moved from M4: needs peds). The carjack's score is M6's.
- [x] **Peds in the water** drown as the player does (moved from M4 on 2026-11-21: needs peds).
- [x] Traffic fidelity issues AmiGTA1 lists as open: junction wobble (`gta_traffic.c:5871`) and corner landing error (`gta_traffic.c:5666`). Measured and closed (decision log, 2026-11-21).

**M6 Combat, police, services**

- [x] The victim's ped counter-punch (`gta_peds.c:1675`; moved from M5 on 2026-11-21: needs the player's punch). Measured in M6b and built 2026-11-26.
- [ ] **Special ped groups** that the bonuses need: Hare Krishna lines; Elvis impersonators **(verify which levels)** in M6b. Spawn rules come from the style and ped tables. (Moved from M5 on 2026-11-21: the bonuses are here.)
- [x] **Bombs**: bomb shop rigs car for 5,000 points (`kind 1`), armed by fire key (`kind 2`, 250-tick fuse), plus trigger (`kind 3`), damage (`kind 4`), and speed (`kind 5`) bombs (`World::bombs_tick`). The type-4 tanker bomb global awaits **(verify)** in M6b.
- [x] **Chain explosions**: a burning wreck sets off its neighbours (`gta_weapon.h:34`). 2026-11-21: AmiGTA1 already writes off every car in a blast's reach, whose own fuse then runs; ported with the weapons (`rocket.txt`).
- [x] **Blast damage**: a car blowing up hurts the cars and people around it (`gta_weapons_wreck_car`); burnt-out wrecks look burnt (moved from M4). 2026-11-21: the blast is done (people killed or set alight, cars written off, the player too); 2026-11-22: burnt-out wrecks draw with `gfa_render::sprites::wreck_sprite` (`WBus` / `WreckedCar`).
- [x] Re-audit `gta_weapon.h:34`, since that list is partly stale, and finish whatever is still missing:
  - weapon crates (`gfa_sim::pickups`);
  - friendly-fire groups (`Peds::hit_segment`: cops do not hit cops; same-gang gunmen do not hit each other);
  - armour absorbing hits (`World::armour` set by pickup and read on bullet hits);
  - free-standing fires that rockets leave on walls. (Done 2026-11-21: `rocket_wall.txt`.)
- [x] **Power-ups that "nothing reads"** (`gta_main.c:10291`). Enumerate every pickup kind from the style's object table, then implement each one's effect and timer: speed-up (`375` ticks double-step), the multiplier (`+1`), the kill-frenzy crate handed to the script (plus endless weapon ammo), extra life (`+1` up to `99`), armour (`3` hits), bribe (clears heat and wanted level), and jail-free key (`World::jail_free`).
- [x] **In-game bonuses AmiGTA1 lacks** (`gta_score.h:17`: "GOURANGA!" not built).
  - GOURANGA bonus triggered on a 4-ped run-over streak (`World::run_over`). The remaining special-group bonuses (Elvis, Hare Krishna) await the M6b special ped group **(verify)**.
- [x] The crime counters the pager prints on a bust (`gta_score.h:16`): tracked in `Score::crimes` (`[u16; 11]`) and cleared on a new life (`Score::new_life`).
- [x] The police AI's reason for the siren (`gta_main.c:9976`: `Traffic::police_siren` returns `Siren::Sounding` when a chasing cop has `lead_kind != 0`, else `Siren::On` within 512 px) and the cop's arrest call. Bribes and jail-free cards verified end to end in synthetic tests.
- [x] Police escalation stays cop cars only (`gta_traffic.h:2622`) unless evidence says otherwise.

**M6b Measured from the original**

- [ ] **Tank** (model 0x25). AmiGTA1 has only its handling: the engine cut at top speed (`gta_vehphys.c:395`) and its turn rate. Add:
  - the rotating turret, aimed independently of the hull;
  - cannon shells on the fire key: projectile, explosion, wall and car damage;
  - its armour and resistance to damage;
  - pushing and crushing cars;
  - where it spawns, and how it is reached. **(verify)**
- [ ] **Boats.** AmiGTA1 places the boat as a map object with no driving (`gta_traffic.c:1152`, `gta_car.c:6`), and the boat has no doors in the car table. Add water-only boat handling (ground type 5 is water, `gta_vehphys.c:612`), getting on and off, and the boats that missions use. **(verify)**
- [ ] **Map-placed specials**: the boat, tank, helicopter and RC car are placed from the map's object list (`gta_traffic.c:1152`).
  - The helicopter (model 88) is filtered out of traffic (`gta_traffic.c:1186`). Model it as a script-driven vehicle: the escape helicopter's destination (`gta_script.h:82`), takeoff and flight.
  - The RC car (model 47) has no doors either. Drivable or not, and how? **(verify)**

**M7 Missions**

- [ ] Settle the gang-role state machine (`gta_peds.c:1111`; moved from M5 on 2026-11-21: only the script makes gangs). The original's states: **(verify)** in M6b.
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
- **Testing layers** (`cargo test`, `cargo data-test`, `cargo oracle-test`, `cargo test-all`; see the README): unit tests; data tests gated on `GFA_DATA`; golden renders and holecheck; replay hashes, native vs wasm; oracle traces; script expectations; mission coverage. Stretch goal: decode the original `.REP` replays as a fidelity oracle.
- **Risks and open questions** (answers go in the decision log):
  - ~~G24 vs GRY as the default~~: settled, G24 (see the decision log, 2026-11-20).
  - Extracting `gta_main.c` cleanly.
  - Whether the oracle needs trace hooks beyond a small patch.
  - WebGL2 texture budget for HD; Safari performance.
  - London's unknown files and left-hand traffic.
  - Features where AmiGTA1 is silent (tank turret, boats, bonuses, attract demos). These need evidence from the original: M6b's DOSBox-X probe harness, built once and reused per item.
  - Smacker licensing hygiene.
  - HD pack size on the web.

## Decision log

Newest first. 
- **2026-11-26. M6b: the ped counter-punch.**
  - On the tenth punch that lands on a ped, the ped turns to face him and hits back: the player is down for about two seconds (`World::knocked`, 100 ticks; no walking, firing or boarding), with no health or armour lost. The wheel resets at once, so it has 11 steps; AmiGTA1's has 13, whose last two are no-ops, so the oracle takes `tools/oracle/ped_counterpunch.pl` (wheel of 11, the ped turns, `walktrace.c` knocks him down) and `tools/scripts/counter_punch.txt` covers it (`cargo oracle-test`: 90,077 ticks match).
  - Not modelled: the ped's punch animation (`0xa9`) and the player's exact fall frame (`0xaf`): the knocked player shows the lying-on-his-back frame.

- **2026-11-26. M6b: explosion score attribution.**
  - A car blowing up pays the player 100 only when he caused it (`Car::player_kill`, set by his bullets, flames, rockets, blasts, ramming and bombs; wreck blasts inherit the owner). Test `a_car_blowing_up_pays_only_when_the_player_caused_it`; `cargo oracle-test` unchanged.
  - Left for later: a lone rocket kill pays 100 in the original and 200 here, and the chain streak (`+200`) and pistol kill (`+150` then `+100`) are not modelled.

- **2026-11-26. M6b: the type-4 tanker bomb.**
  - New `combat_tanker_bomb` overlay (`gfa-tool probe-overlay combat_tanker_bomb`): unmanned `SETBOMB 4` Tanker and Bulldog and a plain Tanker to rocket, and a driven `SETBOMB 4` Tanker and Bulldog with a runway to a wall. Owner-observed in DOSBox-X: a bomb goes off as four corner blasts then one centre blast, on the Tanker and the Bulldog, driven or parked; a plain Tanker blows up as five blasts in a line.
  - `gfa-sim`: type-4 bomb fires above damage 10 (`BOMB4_DAMAGE`), bomb detonations use `Weapons::bomb_blasts`. Details in `docs/M6B_PROBES.md`.
Each entry: date, decision, why.

- **2026-11-25. Unlisted FXT hotkeys (police toggle & three zoom levels) are not pursued.** Per owner decision after the initial `traffic_controls.probe` sweep (`docs/M6B_PROBES.md`), original DOS key-binding discovery for these unlisted FXT toggles is skipped; GFA keeps the 1997 README key layout and its existing zoom and debug bindings (`docs/PARITY.md`).

- **2026-11-25. Original DOS parity for the fire-key latch across `WASTED` is not pursued; GFA adopts AmiGTA1's latch-clearing behaviour.** Agreed with the owner after synthetic probe `combat_wasted_latch3` hit `Error 174.219` at respawn and before running the NYC-overlay follow-up.
  - `World::step` (`crates/gfa-sim/src/world.rs`) event-latches `arms.fire_held` against raw unmasked `Buttons::FIRE` (`prev_fire`, included in `World::hash`), so `arms.disarm()` on `WASTED` keeps `fire_held` cleared across both 150-tick hospital respawns and immediate `!live_city` respawns until `FIRE` is released and pressed again.
  - `tools/oracle/walktrace.c` clears `fire_held = 0` on `WASTED` (`health <= 0`) to match `gta_main.c` and `gfa-sim` on `rocket_close.txt`, covered by `fire_key_latch_clears_across_wasted_until_released` in `crates/gfa-sim/tests/synthetic.rs`.

- **2026-11-24. Car door axes (`rpy` longitudinal, `rpx` lateral), car-over-ped layering, forward blocked-door exit, flush-wall vault turn, and outside slide-under match `GTA8.EXE`.**
  - Under DOSBox-X (`docs/M6B_PROBES.md` Section 2: `getin_door`, `getin_far`, `getin_cop`, `getin_bike`, `vault_wall`, `vault_bus`, `vault_enter_wall`, `vault_bus_far2`, `vault_blocked_{car,bus,north,corner}`):
    - `CarDoor` stores `rpy` first (longitudinal along the body) and `rpx` second (lateral). `cars::{door_point, door_pos}` and `peds::pull_place` had used `d.rpx / 2` as `along`, putting the player forward beside the wing (in front of the open door blade) on saloons and mid-body on Coach (`+25` vs `+50`).
    - `GTA8.EXE` draws vehicles and open door deltas (`6..9`) over entering/exiting/sliding players (`World::player_under_car` in `gfa-app/src/scene.rs` and `gfa-tool/src/run.rs`), while vaulting (`91..94`) and bikes (`81..88`) draw over vehicles; `ENTER_SEQ = [26,26,27,27,25,29..33]` turns `face - 32` on frame `27` without sliding along the body; `EXIT_FRAMES = 8` (`016..023`); bikes mount (`81..83`) and dismount (`86..88`) immediately facing `face - 64`.
    - Pressing `Space` flush outside a Coach (`vert = 24`) slides underneath (`SLIDE_UNDER = 41`); far-side `Enter` on a Coach detours round the nearest end (`cars::far_side_around`); `Enter` with a blocked door is refused; `Space` vaulting onto a wall turns 90° off the roof onto clear road rather than dropping into `BUILDING`; and blocked-door exit vaults forward along `+car.face` across both saloons and buses (`VAULT_HOLD_MAX = 30`). Because none of `tools/oracle/`'s harnesses (`walktrace.c`, `drivetrace.c`) invoke player `try_enter`/`try_exit`/`vault`/`slide`, `cargo oracle-test` stays unchanged and green.

- **2026-11-23. Weapons cooldowns, projectile flight and bullet splats run on the original's `23.333 Hz` clock (`Ratio(7, 15)`), with a five-frame splat.**
  - Under DOSBox-X (`docs/M6B_PROBES.md`: `fps_cal.probe`, `weapons_rate.probe`, `weapons_open.probe`, `weapons_ammo.probe`), `GTA8.EXE` runs on `70.081 Hz` VGA frame-limited to `23.333 Hz` (`3` VGA frames = `42.86 ms`/game frame). Pistol (`10` frames = `428 ms`, `2.333/s`), machine gun (`1`/frame, `20`-frame `BULLET_LIFE`), rocket launcher (`20` frames = `856.5 ms`, `1.167/s`), flamethrower (`9` visible frames / gone frame `10`), bullet splat (`5` frames ≈ `214 ms`), projectile speeds (`~7.6`, `~6.5..8.1`, `~3.6 wpx/frame`), and non-cheat `AMMO_PER_UNIT = 5` (MG and flamethrower) all follow `23.333 Hz`. AmiGTA1 left gun cooldowns (`10/1/20/1`) and projectile tables unscaled at `50 Hz`, while already scaling punches (`PUNCH_TICKS = 4`), explosions (`BLAST_TICKS = 4`), ground fires (`FIRE_TICKS = 250`, `FIRE_FRAME_TICKS = 6`) and car fuses (`CAR_FUSE = 120`) for `50 Hz`.
  - `crates/gfa-sim/src/clock.rs` adds `clock::weapons() = Ratio::new(7, 15)` (stepped in `World::step`, included in `World::hash`). `Arms::tick` decrements cooldowns and triggers guns only on `gun_step`, latching presses and holds in `fire_queued` between steps so 1-tick taps are not dropped while leaving `punch_left` at `50 Hz`. `Weapons::tick` gates splat countdowns (`SPLAT_TICKS = 5`) and bullet flight on `gun_step`, keeping `burn_fires`, `blasts` and `wreck_sweep` at `50 Hz`.
  - `tools/oracle/weapons_clock.pl` patches AmiGTA1's `gta_weapon.c` with `s->ticks = 5` and `gfa_weapons_set_step` (`fleet.h`, `walktrace.c`, `drivetrace.c`), keeping all walk/drive oracle traces and `wasm_matches_native` tick-exact.

- **2026-11-21. The game is read from the archive it came in; the web app is a static bundle hosted on GitHub Pages or Vercel.** Asked by the owner: players select their `.rar` or `.zip` instead of unpacking it.
  - The archive directory (RAR 5 from RARLAB's technical note, ZIP from PKWARE's APPNOTE) is our own code, behind an async read-at-offset trait, so the browser reads only the slices it needs from the chosen `File`. About 41 MB of the 215 MB Director's Cut archive, without loading the archive into memory. RAR 5's LZ comes from the `rars` crate's decoder (Apache-2.0, `default-features = false`: no writer, crypto, recovery or threads). Of the pure-Rust RAR crates on offer it was the only one with real use behind it, and we read its source before taking it. ZIP's deflate comes from `miniz_oxide`. Every member is checked against its CRC-32.
  - Each member must unpack alone. Solid RAR, RAR 4, volume sets and passwords are refused with a message. The owner's archive is non-solid RAR 5; RAR 4 is a possible later addition (`rars` decodes it too).
  - Desktop order: `--data`, `GFA_DATA`, `gfa.local.toml`, an archive beside the program (or its app bundle), the archive chosen last time, then a file dialog (`rfd`). The winit event loop is made before the dialog, because on macOS winit must own the application object.
  - Web: a start screen telling the player to search for the game, then **Select game archive**; the unpacked level files are cached in IndexedDB as before. A content security policy limits the page to its own site.
  - Hosting: the bundle is static, so the suggested multi-service Vercel layout (one service per native crate) does not apply. There is nothing to serve but files, and no service calls another. One build script (`tools/web/build.sh`, Trunk pinned by SHA-256) serves a GitHub Pages workflow and a plain static `vercel.json`.
  - Touch screens get buttons along the bottom for the keys. They are held in the controls like keys, so they reach the simulation through the recorded input.

- **2026-11-21. The weapons and the score are AmiGTA1's, measured against it; where it chose for itself, so do we for now.**
  - Ported from `gta_weapon.c` and `gta_score.c`, whose rules AmiGTA1 read out of the original (its header notes: muzzle offsets, speeds, ranges, hit order, blast rings, bullet damage by class, award types and reasons, heat per crime, each city's thresholds). The walk oracle compares every shot, explosion, fire and score on every tick, and matches.
  - A crate holds the original's rounds, 20/20/5/10. AmiGTA1 triples them because it has no crates; ours come in the next stage.
  - Kept from AmiGTA1, **(verify)** in M6b: the bullet's splat shows for 8 ticks (the original leaves an object on the map); every car that blows up pays the player 100, whoever wrote it off.
  - Ours: the fire key is read from each tick's buttons. AmiGTA1 latches it on the key's events and clears the latch on WASTED, so a key still held does nothing until pressed again; ours punches again at once. A drowning man cannot fire (AmiGTA1's test lets him). Both **(verify)**.
  - The player is a target the weapons see where he was at the end of the last tick, which is AmiGTA1's order. So a driver who stays in his car until it blows up is inside its blast and dies, as in the original.
  - Keypad `*` gives every gun 99 rounds. That is the original's key for it, which needs a name-entry cheat; until the cheats come (M8) it is always on. The original's keys keep their actions (the decision below).
  - The traces now give the player's animation in AmiGTA1's numbers (vault 7, slide 8, punch 9), where they had our own order; no compared trace had shown a vault or a slide.

- **2026-11-21. The counter-punch and special ped groups move to M6, gang roles to M7.** Agreed with the owner.
  - Each needs a system that comes later. The counter-punch needs the player's punch (M6). The special groups exist for the bonuses (M6). Gangs are made only by the mission script (M7).
  - None has enough evidence to build from. AmiGTA1 has only the punch counter's last two steps, an unread state machine for the gang AI types, and no Hare Krishnas or Elvises. Carnage3D's mission-writing notes name ped type 143 as the Hare Krishnas, and its 60-second spawn timer is its own guess.
  - So each goes to its milestone with its facts to gather in M6b, rather than being invented now. That closes M5's gap list.

- **2026-11-21. The junction wobble and the corner landing are closed on measurement; cars still do not steer inside a crossing.**
  - Measured over the gate's twelve runs (`cars_keep_their_lanes_through_junctions`). Of 3,351 crossings, no car turned twice in one. The wobble AmiGTA1 describes came from cars with no route re-rolling their heading every tick; with our routes (the earlier gate fixes), cars on that fallback are 0.2% of moving car-ticks.
  - Of 2,126 straight crossings, 14 end more than a pixel off their line. Of 2,282 turns, 15 are more than a pixel off 40 ticks later (AmiGTA1's `stat_slide`). The test holds both under 1%.
  - The four worst landings are not landing errors. In three, a bus standing mid-turn swings its tail into the next lane and shoves a car that has already landed. The fourth is a bike whose U-turn was interrupted.
  - The worst crossing: a car set off from the kerb a block before a crossing, entered it 24° off square, and crossed diagonally into the next lane (27 px), because a car holds its heading inside a junction box. AmiGTA1 chose that rule on measurement: steering inside crossings made cars book diagonal footprints, box waiting rose from 6,851 to 8,292 car-ticks and flow fell from 93.5% to 92.25%. One crossing in 2,126 does not outweigh that, so the rule stays.

- **2026-11-21. The pull-over is built from AmiGTA1's note, with our numbers where it is silent.**
  - Evidence: AmiGTA1's `special_key` note (`gta_main.c:3957`): the original's routine looks up to five blocks ahead for a car and makes it pull over, lane offset 7 or 0x37 of 0x40 instead of the middle's 0x1f, in its own state (`car+0x11f == 1`; `gta_traffic.h`). The routine handles the key in a car, so both the horn and the siren do it, while the key is down.
  - Ours: the look takes the nearest car whose centre is within half a block of his line. It must be ordinary traffic driving along its lane, with its block and the next two plain road, so it never stops in a junction. It moves to the kerb on its right (16 ± 12 px, from the original's offsets) at 2 speed units, and stops within 2 px of it. It stays three seconds after the key last found it, then drives on. Standing there counts as no jam, so none of the stuck rules fire. A turn, a lane change or a booking ends it.
  - Every number in that list is **(verify)** in M6b. Precedent: the camera's zoom curve, built provisional for the same reason. Nothing in the oracle presses TAB, so every oracle run is unchanged.
  - London drives on the left (M11), where the kerb is on the left.

- **2026-11-21. The player's siren and horn, from AmiGTA1's `special_key`.** Models 4, 5 and 0x2a (the police car, the ambulance, the fire engine) have a siren. The first press puts it on and it stays on. A press after that sounds it (the yelp) for as long as the key is held, and letting go goes back to on. Getting out puts it off. Any other car sounds its horn while the key is held. On foot a press is a shout, whose sample is the sound's to pick (M8).
  - AmiGTA1 drives this from key events. Ours takes the presses and releases from each tick's buttons, which is the same thing. Its script order `tab N` holds the key across waits, as AmiGTA1's harness does.
  - Ours: his car shows its lights, lighting deltas 15 and 16 turn about every ten ticks. In the original the same byte (`car+0x113`) switches the siren and the lights. AmiGTA1 flashes them only on its dispatched cops.

- **2026-11-21. The original's keys do what they did; debug keys keep off them.** Asked by the owner.
  - Evidence: the 1997 DOS README's key list (pieter.com mirror of the demo's `README.TXT`). The default layout uses the arrows, Space (brake, jump), Enter (in and out), Ctrl (fire), Z and X (previous and next weapon), Tab (horn), F1 to F4 (network messages), F5 (retune the radio), F6 (pause), F7 (repeat the pager), F9 (the area's name), F10 (repeat the last message), F11 (screen mode) and Esc (quit). AmiGTA1 agrees on every in-game key it ports.
  - F8 is not in the README. The game's text has strings for a frame-rate limiter going on and off, and the Windows re-release puts that toggle on F8, so F8 counts as the original's too.
  - The game's text also has toggles for the police and for three zoom levels whose keys the README does not give: **(verify)** in M6b before binding anything new.
  - Changed: the debug overlay moves from F1 to the backtick key, and F6 now pauses (no time reaches the simulation). Our free look (F), straight down (R), fly (C) and zoom (`+`/`-`, PageUp/PageDown, the wheel) are on keys neither of the original's layouts uses, so they stay.
  - Shift (walk) and the WASD aliases are ours too. They sit on keys of the README's second, "Classic" layout (Z/X turn, U/I forward and back, P brake, W enter, 9 fire, Tab and Left Shift weapons, S horn). If we offer that layout (M12, key rebinding), those aliases go while it is chosen.
  - Kept for later: F5 with the radio (M8), F7, F9 and F10 with the HUD's messages (M7), F8 and F11 with the settings (M12), Esc's confirmation with the front end (M8).

- **2026-11-21. The last of the gate: spawning clear of bodies, following round a turn, stopping by the nose; the oracle carries them.**
  - Spawning: four of the five bumps left in the gate were new cars put down on another's body, the nose of a car waiting at a corner or one changing into the lane. AmiGTA1 asks only whether the lane has room. Ours (`Traffic::body_in_way`): no car's body may touch the box the new car would take (at the block's middle, wide enough for the kerb lane, 2 px spare at the ends). `tools/oracle/traffic_spawn.pl`.
  - Following round a turn: the fifth was two cars turning through one corner. Halfway round, the nose points across the corner and the car ahead in the street being turned into falls outside the strip a car watches. Ours: while turning, it looks along that street too. Added to `traffic_follow.pl`.
  - A shoved car's lane: a bus pushed past the middle of the pavement block beside its lane found only more pavement on the nearer side. `lane_block` now tries the other side too (`traffic_rejoin.pl`).
  - Stopping for a light by the nose: what pushed that bus was the traffic across the crossing clipping its nose. AmiGTA1 holds a car at a red light once its middle is on the stop line, which leaves a bus half a block into the crossing. Ours: the light also holds a car once its nose would reach the box (within its stopping reach, the box gate's own test), where the block before the box is a stop line. The longest waits at the crossing by (61,52) fell from 48 to 58 s to 16 to 35 s. `tools/oracle/traffic_stopline.pl`.
  - Built without either new patch, `oracle_traffic` differs (from line 96,243 and 9,469 of a (64,64) run).

- **2026-11-21. A car follows only what is on its way; the oracle carries the change.**
  - The loop by (106,114) still jammed in one seed once its corners were fixed. Traced at the jam: every car stood behind another, and round the loop each wait was for a car the waiter would never reach. A car follows whatever is straight ahead of it within four blocks, and a northbound car about to turn east queued behind a westbound car beyond its corner, a southbound one about to turn west behind a car turning out of the next street. Round a closed loop of one-way streets those waits joined into a ring.
  - Ours (`Traffic::turn_ahead`): where a car's route turns within four blocks, a car straight on past that corner is not one it follows. Cars before the corner and in it still are. The loop keeps 57 to 59% of its fleet moving, its longest wait 15 s (from 8 to 25% and up to two minutes). `tools/oracle/traffic_follow.pl`; built without it, `oracle_traffic` differs from line 681 of its first run.

- **2026-11-21. Long vehicles take their corners; the oracle carries the change.**
  - A bus came to a corner of a one-way street whose far side was a building or grass, and stopped there for good with its street queued behind it (a 198 s wait at (98,228) in the gate). AmiGTA1 asks whether a vehicle fits lying along the way it is leaving, in the block it is in and in the next: so the bus was refused the move into the corner, since its nose would reach the building if it drove straight on, which at a corner it does not; and once it had turned in the corner, its tail still over the verge, it was refused the way out.
  - Ours (`roads::fits_entering`): a move is judged by the block it enters, as the car comes in. Lying along its way where it may go straight on; at a corner, along the street it turns into. The car is already where it is.
  - Two follow-on changes: a turn the car must make at a corner with no way on starts though its nose is not yet straight (a bus pushed askew by the car beside it entered a junction box, where nothing steers, crabbing; its turn waited to straighten and it left by the far side onto the pavement), and a car refused its turn at such a corner because the street beyond is full waits until it has room, rather than AmiGTA1's half second before forcing in, which put it into the back of the queue.
  - `tools/oracle/traffic_fits.pl` and `traffic_corner.pl`; built without `traffic_fits.pl`, `oracle_traffic` differs from line 8,444 of a (61,52) run.
  - The gate's overlap check now tests the two cars' bodies as boxes on their headings (`geom::box_hit`, the fleet's own collision test), allowing 2 px at the ends and 1 at the sides. AmiGTA1's measures along the first car's nose, which halfway round a turn counts a car passing in the next lane as one in front: most of the corner "overlaps" were that.

- **2026-11-21. A car shoved out of its lane drives by its own lane; the oracle carries the change.**
  - Of the dead-end stops left after the corner fix, most were cars a collision had pushed a pixel or half a block over a lane line, into the lane beside going the other way or onto the kerb. AmiGTA1 drives a car by the block under its middle, so such a car had nowhere legal to go and stood until out of sight. AmiGTA1 does pick the lane beside when a knocked car comes to rest, but the lane keeper resets the line to the block under it the next tick, and a car only pushed gets nothing.
  - Ours (`Traffic::lane_block`): a car driving normally (not turning, changing lanes, reversing, crossing a junction or on police business) whose block does not go its way, while the block beside it on the nearer side does, drives by that block. Its route, junctions and lane line are its own lane's, and the lane keeper steers it back. Tried first as a lane change started from the dead end; that waited for the block beside to be empty, which never happens to a car queued in its own lane with its middle a pixel over the line.
  - In the gate's twelve runs, cars stranded ten seconds out of their lane with nowhere to go fell from 4 to none, and ticks with a car off the road from 2,326 to none. `tools/oracle/traffic_rejoin.pl` makes the same change to the C; every oracle run matches, and built without it `oracle_traffic` differs from line 127,492 of a (108,228) run.

- **2026-11-21. A car turns its corner or waits for it; the oracle carries the change.**
  - The traffic gate found Liberty City's loop of one-way streets by (106,114) jammed solid. Traced: a car on its route came to a corner where the street turns and there is no way on, its turn was refused because the street round the corner was busy, and it drove straight on at full speed onto the pavement, where with no arrows under it it stopped for good. Two faults, both AmiGTA1's: a turn begun a block early asked whether the block beside the car had room, which is the far lane of the street it turns into, so a car was refused for traffic coming the other way; and a refused car carried on as if straight on were open, which only a junction makes true. AmiGTA1 has only a car on a long route wait. Its in-view give-up then deleted the car six seconds later, which hid the fault; ours waits until out of sight, so the car stayed and the loop jammed.
  - Ours: the turn asks about the block beside the turning block, and a car refused its turn where its block's arrows or the road do not go straight on waits at the corner with the give-way brake. In the gate's twelve runs, cars that ran past a corner fell from 44 to none, and stops at a dead end from 47 to 12 (the rest are cars shoved out of their lane in collisions, and the off-road car at (97..101,228)). `tools/oracle/traffic_corner.pl` makes the same change to the C; every oracle run matches, and built without it `oracle_traffic` differs from line 7,426 of its first run.

- **2026-11-21. One light per crossing; the oracle carries the change.**
  - AmiGTA1 times a light from the 8×8-block neighbourhood of the block that asks, to keep neighbouring crossings half a cycle apart, and its comment says one crossing's stop lines always agree. They lie either side of the junction box, though, and on these maps they mostly fall in different neighbourhoods: 20 of Liberty City's 24 lit crossings, 21 of San Andreas's 28 and 18 of Vice City's 27 had approaches on both roads green at once, and the junction bookings sorted out who went. Invisible until the lamps were drawn; drawn faithfully, they would have looked broken.
  - Ours: a light is named by its crossing, the root of its junction box (`Boxes::crossing`: the first junction block within two blocks). A car at a stop line asks about the box ahead of it, so a second crossing just behind cannot answer; a lamp about the crossing at its corner; a person waiting to cross about where he stands. The phase still alternates by the root's 8×8 neighbourhood. The cycle itself (5 s a road, 0.6 s amber) is AmiGTA1's own, not the original's.
  - `tools/oracle/traffic_lights.pl` makes the same change to the copy of `gta_traffic.c` under `build/`. Every oracle run matches; built without it, `oracle_traffic` differs from line 246 of its first run.
  - Lamps: where they stand and which road each is for is AmiGTA1's scan of the map's hint. AmiGTA1 draws them all facing south; ours face the drivers coming in (the lens is the sprite's front, as a car's bonnet is). Where a kerb lane runs past a T-junction's box rather than into it (23 lamps in San Andreas), the lamp faces the box beside it. Cars in such a lane never stop, as in AmiGTA1, since no box lies ahead of them.

- **2026-11-21. The camera's lead and speed zoom are simulation state; the camera never shows more than the simulation's view.**
  - Before, the lead and zoom were the renderer's, and the rings followed a fixed view on the player. Zoomed out, or led ahead at speed, the picture showed ground where people and cars come and go. Measured: the control run without the cap sees 2,369 of 2,445 comings and goings in sight.
  - `SimCamera` (from his position and velocity only): the view's centre leads him by 0.35 s of travel, Carnage3D's figure, eased over about a sixth of a second and capped at a third of the view's half-extent. The old render lead was uncapped: 5.5 blocks at a sports car's top speed, more than the 4.2 blocks a 16:9 window shows above and below. The view widens with speed up to 1.5× at 10 px a tick, eased over about two thirds of a second. The original zooms out with speed; **(verify)** its curve under DOSBox-X (M6b), as nothing in AmiGTA1 or Carnage3D gives it. AmiGTA1's view (`SimView::AMIGTA1`) neither leads nor zooms, so every oracle run is unchanged.
  - `SimView::GAME` grows from 10×6 to 12×7, measured on layer 1. All three cities have walkable ground there, and lower ground shows more of itself through a perspective camera. The default camera (8 blocks above a layer-2 street) shows 10.9 by 4.7 blocks there at 21:9, plus a block spare: the car is drawn up to a physics step (0.8 of a block) ahead of its last tick. The height leaves a 16:9 window room to zoom out as far as a 21:9 one. The crowd's pool scales with the view's area: 45, from 33.
  - The renderer caps the eye at the lowest ground plus `max_depth` for the view now and the window's shape, pillarboxes past 21:9, and follows the simulation's lead without smoothing of its own (a lag would show ground behind the view). Only the height eases. Free look and flying are debug views and not capped.
- **2026-11-21. A car stuck at a dead end gives up only out of sight; the oracle carries the change.** AmiGTA1 deletes a car with nowhere legal to go after 300 ticks wherever it is, its one exception to never deleting a car in view (a long vehicle in a street it cannot leave). The pop-in check saw one vanish in San Andreas. Ours waits until the car is out of sight, as AmiGTA1 already does for a car abandoned in a queue. `tools/oracle/traffic_giveup.pl` makes the same change to a copy of `gta_traffic.c` under `build/`; every oracle run matches.

- **2026-11-21. The sidestep no longer spins people; the oracle carries the same fix.**
  - Found in play: near a car and a wall, people spun on the spot. A crowded ped's sidestep bends his heading a quarter turn for 24 ticks, and AmiGTA1 bends whatever heading he has. Its comment assumes every walking mode re-aims him each tick, but a wanderer with nowhere to be keeps his heading, and a fleeing one re-aims only every seventh tick, so the bend piled up a quarter turn a tick. A second effect: a side found walled was tried again the next tick, so a man along a kerb turned a quarter and back on alternate ticks.
  - Measured on Liberty City (eight random drives of 6,000 ticks): the longest run of hard turns, a quarter or more a tick, was 48 to 121 ticks in every drive; after the fix it is 4 to 6, all of them a man turning off a wall.
  - Fix: the bend is remembered (`Ped::bent`) and taken off before his next tick's aim, forgotten when the step itself turns him, and given up for the rest of the sidestep once that side is walled.
  - The bug is AmiGTA1's, so the faithful port had it and the oracle could not see it. The oracle now runs AmiGTA1's crowd with the same change, made by `tools/oracle/ped_sidestep.pl` on a copy under `build/` (each of its edits must apply exactly once). It keeps comparing everything else tick for tick, and it compares the crowd the game runs, not a spin nobody plays. `nobody_spins_on_the_spot` fails with AmiGTA1's rule (144 ticks of turning) and passes with ours.

- **2026-11-21. The crowd is sized to the view.** AmiGTA1 keeps 12 people, enough for its narrow screen; at the game's view (20 by 12 blocks against AmiGTA1's 10 by 8) twelve leave the streets empty. The pool is 12 × the view's area over AmiGTA1's, so 33 in the game and exactly 12 at AmiGTA1's view, where the oracle runs are unchanged. The fleet's walker list grew from 16 to 64 to take them all.
- **2026-11-21. Peds in the water drown.** AmiGTA1 lets a ped stand on water (only the player drowns). Ours goes under where he stands and is gone after 67 ticks, as the original's do. Nothing in AmiGTA1's crowd walks into water by itself, so it only differs when a shove, a fall or a drop puts somebody there; no oracle run does.
- **2026-11-21. AmiGTA1's four departures in the crowd are kept**: corner targets spread across the pavement, separation between people and the relax pass after it, no stopping to rest near a corner, and a coin per ped at a T-junction (the original sends everybody the same way). Each is AmiGTA1's fix for people piling up where the original's rule bunches them. Recorded in `docs/PARITY.md`; for M6b's probe to compare with the original.
- **2026-11-21. Ped colours in the G24 style are measured.** The simulation dresses people in GRY remap tables, as AmiGTA1 does; the G24 has 64 ped remap CLUTs with no stated order. Every ped sprite through each CLUT against the GRY palette through every table, in all four styles, pairs them one to one (`PED_REMAP_TABLES`): each best match is within the sprites' own requantising error, the next best at least 60 times worse. `people_wear_the_same_in_either_style` draws all 22 of AmiGTA1's dresses in both styles; they agree to within 2.6 of 255.

- **2026-11-21. Traffic is AmiGTA1's, with its switches at their defaults.** AmiGTA1 carries about twenty experimental switches (`opt_*`), most of them off after its author measured them; the port keeps what the defaults run and leaves out the rest, so there is one behaviour to match and to maintain.
- **2026-11-21. Traffic keeps AmiGTA1's fixed array of 60 cars.** A new car is written into the next slot with only the fields AmiGTA1 sets, and keeps the rest from whatever car last used that slot (a lane change under way, a coast, a collision latch). Clearing them would be tidier but would part from the oracle in ways nobody could see the cause of.
- **2026-11-21. The traffic generator's seed is 64 bits**, as in AmiGTA1's host build. The route wander draws `(seed >> 16) % n` without a mask, which a 32-bit seed answers differently, and the host build is the oracle we can run. (The trains keep 32 bits; their draws are masked.)
- **2026-11-21. AmiGTA1 reads two locals before setting them** (`gta_traffic.c:5316`, the check that keeps a crossing's booking while a car waits just short of the junction). The result depends on whatever is on the stack. The oracle is built with them zeroed (`-ftrivial-auto-var-init=zero`) and the port treats them as zero, which turns that one test off. Worth reporting to AmiGTA1.
- **2026-11-21. The map's parked cars live in the fleet for the whole level** (replacing the stage-1 entry that kept them outside it). AmiGTA1 has none; the original parks some. In the fleet they are what AmiGTA1's cars left by the player are: traffic stops for them, the player rams them, takes them, and is held off them on foot, and trains crush them. They are never despawned, until he takes one; left again, it is an ordinary abandoned car. A vehicle with no door (a boat, the RC car) is placed locked. Two of them at rest do not push each other: the map parks some rows a few pixels into each other, and the fleet's collision would slide them apart at the start of the level.
- **2026-11-21. The fleet has 192 slots, not AmiGTA1's 60.** Liberty City parks 106 cars. AmiGTA1's 60 was memory on an Amiga; its own runs never come near it, so the oracle runs are identical (checked), and the stale-field reuse of a slot is unchanged below 60.
- **2026-11-21. A car the player leaves keeps its dents.** AmiGTA1 keeps its damage points but clears its dented panels when it goes back into the fleet; the original shows the same car. No trace prints the panels.
- **2026-11-21. The player's car is drawn on its physics clock.** It steps on 7 ticks in 15 (23.3 Hz); drawn between ticks it moved on one and stood on the next, which the owner saw as stutter. The app now notes when each step happened (from the 7/15 accumulator) and carries the car on at its last step's velocity and turn between steps, and gives the camera that velocity. Render only: the simulation never reads it. Measured at 60 Hz on a straight: a frame's move went from 0..35 render px to 7.6..8.2.

- **2026-11-20. The vault is AmiGTA1's, including where it is thin.** The vault moves him without asking the map, so a car parked against a wall can drop him into it; and the slide under a tall vehicle only carries him while one is already over him, so SPACE at a bus approached on foot lies him down for four ticks and stands him up again (AmiGTA1 ports the original's state 0x92, which is a shove). Both are for M6b's probe to settle against the original before anything changes.

- **2026-11-20. The tank, boats, helicopter and RC car leave M4 for a new M6b, measured from the original.** AmiGTA1 drives none of them beyond the tank's engine cut, so their behaviour has to come from the DOS original. The owner proposed running it under DOSBox-X with scripted key presses. The harness is built once and reused for every later **(verify)** item. M6b follows M6 because the tank's cannon needs its explosions. M4 keeps what AmiGTA1 has: the vault, cars sinking, drowning.
- **2026-11-20. Every check runs through cargo.** Plain `cargo test` needs no data (unit tests, and the simulation on small maps built with `CmpBuilder`); `cargo data-test` runs what needs the game data; `cargo oracle-test` the AmiGTA1 comparisons and the native-vs-wasm check, which need `../AmiGTA1`, clang, Node and wasm-bindgen; `cargo test-all` runs everything. They replace the shell scripts (`compare.sh`, `fuzz.py`, `compare_trains.sh`, `wasmcheck/compare.sh`). Asked for by the owner.
- **2026-11-20. Trains draw their random numbers from a generator seeded per level.** AmiGTA1 keeps one static seed for the whole session, so its trains depend on what was played before; a replay must not.
- **2026-11-20. A train's crush only touches parked cars, as in AmiGTA1** (its fleet); the player's own car is not crushed yet, and being run over on foot only logs until M6 brings death.

- **2026-11-20. M4 keeps only what vehicles need on their own.** Pull-over and the siren need traffic (to M5), dragging out a driver needs peds (to M5), and bombs, chain explosions and blast damage need weapons (to M6). Agreed with the owner, so M4 can close on trains, the vault, boats and water, the tank and the specials.

- **2026-11-20. The map's parked vehicles belong to the simulation**, built from the startup objects with a remap byte of 128 or more (Carnage3D's `CreateStartupObjects` does the same); trains and trams stay render-side until their milestone. The renderer leaves those objects out of its static sprites and draws the simulation's cars instead. AmiGTA1 parks its own fleet cars near the start and ignores the map's; its oracle runs therefore use `--oracle`, which clears them.
- **2026-11-20. Vehicle arithmetic is `i64`**, the arithmetic of AmiGTA1's 64-bit host build that serves as the oracle. AmiGTA1's fixed-point helpers keep intermediates inside 32 bits, so the Amiga agrees wherever AmiGTA1 has no overflow bug.
- **2026-11-20. A door blocked by a building or a car refuses the exit for now.** AmiGTA1 climbs out over the roof; that waits for the vault. (Superseded the same day: the vault is in.)
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

- **2026-11-24. M6b Section 2: car get-in/exit, bike mount/dismount, slide-under and vault edge cases.**
  - Added the `blocked_exit` preset in `crates/gfa-tool/src/probe.rs` and ran all Section 2 DOSBox-X probes (`getin_door`, `getin_far`, `getin_cop`, `getin_bike`, `vault_wall`, `vault_bus`, `vault_enter_wall`, `vault_bus_far`, `vault_bus_far2`, `vault_blocked_car`, `vault_blocked_bus`, `vault_blocked_north`, `vault_blocked_corner`), documenting all findings in `docs/M6B_PROBES.md` and `docs/PARITY.md`.
  - Fixed `CarDoor` axis order (`d.rpy / 2` longitudinal, `d.rpx` lateral) in `cars::{door_point, door_pos}` and `peds::pull_place`; updated `ENTER_SEQ` (`26,26,27,27,25,29..33`), `door_delta`, `EXIT_FRAMES` (`8`: `016..023`), `exit_door_delta`, bike `81..83` / `86..88` (`face - 64`), `World::player_under_car()` painter order (`gfa-app/src/scene.rs`, `gfa-tool/src/run.rs`), cab-level lateral slide-in, `cars::far_side_around` for tall vehicles, blocked-door refusal on `try_enter`, `VAULT_HOLD_MAX = 30` + forward `+car.face` blocked-door exit (`try_exit`), wall-turning `vaulting()`, and outside `sliding()` in `crates/gfa-sim` (`cars.rs`, `peds.rs`, `traffic/contact.rs`, `world.rs`).
  - Updated `tools/scripts/vault.txt` and `a_blocked_door_puts_him_out_over_the_roof` and added five synthetic tests (`door_geometry_and_enter_exit_sequences`, `far_side_enter_on_bus_walks_around_body`, `jump_at_bus_from_outside_slides_under`, `flush_wall_enter_refused_and_vault_turns_off_roof`, `blocked_door_exit_forward_on_north_car_bus_and_l_corner`) in `crates/gfa-sim/tests/synthetic.rs`. Verified `cargo test --workspace` (41 synthetic + unit tests), `cargo wasm-check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check`, `cargo data-test`, and `cargo oracle-test`.

- **2026-11-23. M6b DOSBox-X probe harness and weapon fire-rate / projectile / splat parity.**
  - Built a patched deterministic DOSBox-X (`322a07a` + `gfa_probe.{h,cpp}`) into gitignored `tools/dosprobe/build/dosbox-x`, with `tools/dosprobe/run.py` supporting headless RGB/RAM capture, `--cycles 150000`, and repeatable `--overlay` staging (`smoke1` vs `smoke2` and `arena1` vs `arena2` bit-identical across `.ppm`/`.png` frames, RAM dumps and `probe.log`).
  - Added block/column deduplication and map header builders (`district`, `sample_number`, `route`, police/hospital/fire stations) to `gfa_formats::cmp::CmpBuilder` and `gfa-tool probe-overlay <preset> --out <dir>` (`arena`, `shooting`) so unmodified `GTA8.EXE` boots directly into controlled Level 1 arenas via `-t overlay`.
  - Measured `GTA8.EXE` simulation timing (`23.333 Hz` on `70.081 Hz` VGA), block ruler scale (`108 screen px/block`), weapon fire rates, projectile speeds, `BULLET_LIFE = 20`, non-cheat `AMMO_PER_UNIT = 5` (`fl_crate` `10 -> 04`, `mg_crate` `20 -> 14`), and bullet splat lifetime (`5` game frames ≈ `214 ms`) across `fps_cal.probe`, `weapons_rate.probe`, `weapons_open.probe` and `weapons_ammo.probe` (`docs/M6B_PROBES.md`).
  - Updated `crates/gfa-sim` (`clock::weapons()`, `Arms::fire_queued`, `SPLAT_TICKS = 5`, `World::weapons_clock`), added `weapon_fire_rates_projectile_speed_and_splat_lifetime_match_original` in `crates/gfa-sim/tests/synthetic.rs`, and added `tools/oracle/weapons_clock.pl` (with `fleet.h`, `walktrace.c`, `drivetrace.c`, `build.sh`). Verified `cargo test --workspace`, `cargo wasm-check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check`, `cargo data-test`, and `cargo oracle-test`.

- **2026-11-22. M6 stage 2: crates, power-ups, police, services, shops, marks and burnt wrecks.**
  - `gfa_sim::pickups` (AmiGTA1's `gta_pickup.c` and `gta_main.c:9370-9398, 10151-10296`): `POWERUP` mission declarations, closed/open crates (`0x54`, `0x55`) opened by bullets, cars or blasts, and all eleven item kinds (`0x4e..0x51`, `0x5f..0x65`), including the power-ups AmiGTA1 left unread (375-tick double-step speed, extra life, jail-free key, kill-frenzy endless gun).
  - `gfa_sim::marks` (`gta_marks.c`): blood pools (`0x3f`), ped splats (`0x40`) and footprints (`0x4d`), jump sparks (`0x0d`), water splashes (`0x36`), explosion clouds (`0x33`), building rubble (`0x2b..0x2d`, `0x05`), and tyre marks (`0x0b`), bloody tyre marks (`0x0c`) and wheel smoke (`0x0a`) for both the player's car and knocked fleet cars. Burnt-out wrecks draw with `gfa_render::sprites::wreck_sprite` (`WBus` / `WreckedCar`).
  - Police (`gfa_sim::traffic`, `gfa_sim::peds`, `gfa_sim::world`): map `0xff` patrol routes, station/ring dispatch and reinforcement, speed scaling, `Router::find_chase`, U-turns, junction push-through, cut-off lane offset, roadblocks at wanted level 3+ with barriers (`0x18`) and posted cops, cop out on foot, door pull-out, on-foot arrest (`14` ticks), shooting at level 3+ or when the player is armed, level-4 point-blank execution, police carjacking driver becoming a cop, and `Traffic::police_siren`.
  - BUSTED and WASTED cards (`150` ticks), lives (`4`), `Card::GameOver`, hospital and police station respawn, crime counters reset on new life (`Score::new_life`), and jail-free key preservation.
  - `gfa_sim::service` (`gta_service.c`) and `route::field_build`/`field_follow`: 64 KB BFS long-route field, ambulances dispatching from hospitals and deploying medics (`75`-tick revive), fire engines dispatching to burning wrecks and spraying water cannon jets (`0x30`, `0x31`) that put out car/ground fires and knock down peds.
  - Spray shops (`SPRAY`), bomb shops (`BOMBSHOP`, 5,000 points) and car bombs (`World::shops_tick`, `World::bombs_tick`); friendly-fire groups (`Peds::hit_segment`); GOURANGA bonus (`World::run_over`); script fixtures (`crate`, `copcar`, `lives`, `pool`, `killped`, `wreck`, `score`, `script`) and scripts (`blood.txt`, `bankblast.txt`).
  - Seven new synthetic M6 tests in `crates/gfa-sim/tests/synthetic.rs`; all tiers green (`cargo test`, `cargo data-test`, `cargo oracle-test`, `clippy`, `fmt`, `wasm-check`).

- **2026-11-21. The web app fills the screen on phones; full screen.** The site went live on GitHub Pages. On the owner's phone the game looked zoomed in, with the player off-screen. Cause: on the web winit wrote the desktop window size (1280x800) onto the canvas as an inline style, overriding the page's full-window sizing, so a phone showed only the canvas's top-left corner. The size now goes to desktop windows only. Measured in Chrome as an 844x390 phone: the canvas fills the viewport, and the view matches the desktop build at that size (about 20 blocks across, the player centred). The surface's cap at the GPU's largest texture now keeps the picture's shape (`a_capped_surface_keeps_its_shape`). A "Full screen" button fills the screen and asks a phone for landscape. The automated Chrome refuses full screen to scripted clicks, so it awaits a try on a real phone.
- **2026-11-21. The game from its archive; hosting; touch buttons.**
  - `gfa_assets::archive` reads RAR 5 and ZIP in place. The owner's `GTA1-Directors.rar` (RAR 5, 483 members): the directory reads in 42 ms; the 180 level files (41 MB) unpack in 0.2 s; all 471 files (300 MB) at 210 MB/s with every checksum matching (`gfa-tool archive --all`). The data test `the_archive_serves_the_folders_bytes` finds all 180 level files byte-identical to the unpacked folder's.
  - Desktop: found beside the program, and remembered after a dialog (both run headless with `--screenshot`). Liberty City loads from the archive in 40 ms.
  - Web, in Chrome: the archive selected, 180 files unpacked from `File` slices, the level running; on reload, straight from IndexedDB. As a landscape phone with touch, the buttons show and holding Forward walks the player.
  - `tools/web/build.sh` builds the bundle (7.2 MB wasm) and fails if anything but the page, script and wasm lands in it. `.github/workflows/pages.yml` and `vercel.json` use it; neither has been run yet.
  - Unit tests build ZIP and RAR 5 archives in memory: damage, refusals, the `__MACOSX` shadow, the game's root at any depth. `.gitignore` now keeps `.rar`, `.zip` and `.7z` out.
- **2026-11-21. The debug menu.** The overlay (backtick) is now a menu: a frame counter with the frame time, rate and simulation tick; the simulation's counts (cars, people, shots, fires, explosions) and the renderer's; the player's state with a **Top up ammo** button; the camera and view mode in a fold; the block inspector. It starts hidden; backtick shows it. The button is a press of keypad `*` queued in the controls until a tick runs with it, so it goes into the recording like any key. Tests: `a_menu_press_is_held_until_a_tick_has_run`, `the_top_up_gives_every_gun_99_rounds_once_a_press`.

- **2026-11-21. M6 stage 1: the weapons, explosions and the score.**
  - `gfa_sim::weapons` and `gfa_sim::score` (AmiGTA1's `gta_weapon.c`, `gta_score.c`), the ped hit functions (shot, punched, panicked, set alight), shot-stopping flat faces in the map, and the player's guns, punch, armour and burning in the world. Scripts orders `weapon`, `hurt` and `shootped`. Keys: Ctrl fires, Z and X change weapon, keypad `*` gives every gun (decision log).
  - The walk oracle now runs AmiGTA1's weapons and score in the street (`fleet.h`), and the harness restates the fire key and what hits the player. Six new scripts match tick for tick: the punch, the pistol and machine gun, rockets on cars and the chain they set off, a rocket on a wall and its fires, a rocket fired too close, and the flamethrower. The trace lines carry each car's damage, each ped's shot, fall and burn, every shot, explosion and fire, and the score.
  - Every tier green: unit, data (the gate unchanged), oracle (77 runs and the traffic gate), native against wasm, clippy, fmt, wasm-check.
  - Next: the crates and power-ups, then the police.

- **2026-11-21. M5 gap items: the siren, the horn and the pull-over; the junctions measured; the original's keys.**
  - The special key (TAB): the siren in a police car, ambulance or fire engine, with its lights; the horn in any other car; a shout on foot. Scripts `siren.txt` and `horn.txt` (both pass on Liberty City), synthetic tests for each state.
  - The pull-over: the car ahead moves to the kerb and stops while the key is held. Data test `the_car_ahead_pulls_over_for_the_horn`. Its numbers are provisional, **(verify)** in M6b (decision log).
  - The junction wobble and corner landing: measured over the gate's runs and closed. No car turns twice in a crossing, and over 99% of crossings and turns end within a pixel of the line. Data test `cars_keep_their_lanes_through_junctions`.
  - Keys: the original's default layout, from its 1997 README, does what it did there. F6 pauses. The debug overlay moves from F1 (the original's network messages) to the backtick key.
  - The counter-punch and special ped groups moved to M6, gang roles to M7, each with its facts to gather in M6b (decision log). M5's gap list is closed. Every M5 item is now ticked, and its exit is met for the traffic.

- **2026-11-21. M5: the traffic gate passes.**
  - All twelve of `fullgate`'s runs pass `traffic_gate` and match AmiGTA1 in `oracle_traffic` (the M5 exit for the traffic). Fixes, all ours and all in the oracle (decision log): new cars clear of other bodies, following along the exit street mid-turn, a shoved car's lane found on either side, and the light holding a car by its nose. Eight patches to AmiGTA1's traffic now (`build.sh`), each shown to matter: built without it, `oracle_traffic` differs.
  - Every tier green, the gate included: unit, data (28), oracle (all 71 walk and drive runs, traffic, trains), native against wasm, clippy, fmt, wasm-check.
  - Next: the M5 gap items (horn and pull-over, the siren toggle, the counter-punch, gang roles, special ped groups, junction wobble and corner landing).

- **2026-11-21. M5: the queue ring and the buses' corners fixed.**
  - `roads::fits_entering`, the off-straight turn at a corner with no way on, the corner's patience, `Traffic::turn_ahead` (decision log). `tools/oracle/traffic_fits.pl` and `traffic_follow.pl` (six patches in all, `build.sh` chains them); `traffic_corner.pl` widened. Every oracle run matches; built without either new patch, `oracle_traffic` differs.
  - The gate's overlap check is now the fleet's own box test (decision log), and it names the car that stood longest and what held it.
  - New tests: `the_one_way_loop_keeps_moving` (fails without the following rule: a car stands 55 s), `no_car_stops_at_a_dead_end`; both sim, data. `film_gate` films of the loop and the bus corner after the change.
  - The gate: 6 of 12 pass, from 3. No car stands a minute and none is off the road but one. What is left: a single tick of two cars inside each other in five runs, the moment of a bump; a bus stopped partway round the corner by (97,228) with its tail over the verge for 125 ticks in one run; and the longest waits at the lit crossing by (61,52) have grown to 48 to 58 s (the light, then the box on green), under the minute but worth a look.
  - Next: the bumps, the bus's tail, the crossing by (61,52).

- **2026-11-21. M5: a shoved car steers back into its lane; films of the gate.**
  - `Traffic::lane_block` (decision log); `tools/oracle/traffic_rejoin.pl`, the fifth patch.
  - New test `no_car_stands_in_the_wrong_lane` (sim, data: 4 stranded before, none now).
  - `film_gate` (sim, data): with `FILM="bx by seed x0 y0 blocks from to step"` and `OUT`, writes a schematic film of a gate run: ground types, arrows, lights, and every car as a box on its heading, coloured by state (moving, standing, 3 s, 10 s; off the road rimmed). Made of the queue ring at (106,114), the bus at the corner by (98,228), and the bus off the road at (101,228) before this change.
  - The gate: no tick with a car off the road in any run (from 2,326); 3 of 12 pass. What is left: the queue ring at (106,114) seed 1; long vehicles refused corners by `roads::fits` at (98,228) and (101,228) (a bus stands up to 198 s); brief overlaps; one car vanishing in view at (61,50).
  - Next: the queue ring, and the long vehicles' corners.

- **2026-11-21. M5: cars turn the one-way corners.**
  - `gfa_sim::traffic` (drive): a turn checks the street it leads into, and a car refused its turn at a corner with no way on waits for it (decision log). `tools/oracle/traffic_corner.pl`, the fourth patch.
  - New test `no_car_runs_past_a_corner` (sim, data: the gate's twelve runs; 44 cars ran past before, none now).
  - The gate after it: the loop by (106,114) flows (24 to 64% of the fleet moving, from 8 to 19%), but seed 1 still has a car standing a minute. That is a second fault: a car queues behind whatever is straight ahead of it, even a car beyond the corner its route turns at, and round a closed loop those waits join up into a ring. Across the twelve runs the brief overlaps have moved about, as any change to the fleet reshuffles every run, and one run now has a car vanish in view at (61,50), not yet looked at. 3 of 12 pass; `traffic_gate` is still red.
  - Next: the queue that looks past its own turn; what a car with nowhere to go does in view; then the overlaps, the vanished car and the off-road car.

- **2026-11-21. M5: the traffic gate run at 12,000 ticks; it fails as AmiGTA1's does.**
  - `oracle_traffic` is now the gate's own setup (four sites by seeds 1 to 3, 12,000 ticks, the fleet's view at 6 blocks as `gtadump drive` gives it): all 12 runs match AmiGTA1 car for car, about 238,000 trace lines each, in 8 s.
  - `traffic_gate` (sim, data) checks what those runs do, as AmiGTA1's `gtadump drive` does: every car on the road (its rule: the centre line on road or pavement, the flanks out of buildings and water), none inside another, none vanishing from the camera's picture, the fleet moving, and none standing a minute or more. It passes 4 of the 12, and gives the same verdict as AmiGTA1's own gate on every run, built from the same patched sources (the flow figures and the longest waits agree too). Unpatched, AmiGTA1 passes 3 of 12.
  - What fails: (1) brief overlaps, 4 to 8 ticks in a run, at five of the runs, AmiGTA1's own; (2) at (106,114), a loop of one-way streets with sharp corners, long cars run straight past a corner onto the pavement and stop there with no arrows under them. AmiGTA1 deletes them after six seconds in view (13 to 20 vanish a run); since stage 4 we wait until out of sight, and with the gate's camera standing still the whole loop jams (8 to 19% of the fleet moving, a car standing up to two minutes). (3) At (108,228), seeds 2 and 3, a car off the road at (97,228), once for a minute.
  - Next: fix the corner overshoot (the jam's cause) and decide what a car with nowhere to go does in view; then the overlaps and the off-road car. The data tier has one red test until then: `traffic_gate`, the exit criterion.

- **2026-11-21. M5 stage 5: the traffic lights drawn; one light per crossing.**
  - `gfa_sim::traffic::roads`: `Light` (red, green, amber), `light`, `Lamp` and `lamps` (AmiGTA1's `gta_traffic_light_state` and `gta_traffic_lights_scan`), `Boxes::crossing`; `Traffic::lamps` and `light_tick`. Lights are timed by the crossing, and a car at a stop line asks about the box ahead (decision log); `tools/oracle/traffic_lights.pl`.
  - Drawn in the app (`Game::lamps`, near the simulation's view) and in `gfa-tool` pictures, before the crowd and the cars.
  - Tests: `one_axis_at_a_time_with_amber_before_red`; `the_lamps_face_their_crossings` (sim, data: every lamp in the three cities faces a crossing and shows what its cars obey, and no crossing has both roads green, over two cycles with the city running); `the_lights_stand_on_the_corners` (render, data: a Liberty City crossing's eight lamps drawn on their corners, lit their light's colour, lenses toward the drivers coming in). Every tier green: unit, data (22), oracle (all 71 walk and drive runs, traffic, trains), native against wasm, clippy, fmt, wasm-check. The oracle built without the new patch fails, as it should.
  - Next: the traffic gate at 12,000 ticks (run; see above).

- **2026-11-21. M5 stage 4: the camera and the view; no pop-in.**
  - `gfa_sim::view::SimCamera` (the lead and speed zoom, in the world and its hash), `World::view_now` and `view_centre` for the fleet and the crowd; `SimView::GAME` 12×7; `SimMap::floor_layer`.
  - `gfa_render::camera`: `max_depth`, `pillarbox`, `MAX_ASPECT`, `VIEW_MARGIN_BLOCKS`; `FollowCamera` follows the simulation's lead, eases only its height, and takes a ceiling. The app caps the zoom and draws the scene in the pillarbox (the inspector too). `gfa --size WxH` for smoke tests: a 2912×1000 window draws a 2,333-pixel band, as computed.
  - The fleet's dead-end give-up waits until out of sight (decision log); `tools/oracle/traffic_giveup.pl`.
  - Tests: `nothing_pops_in_or_out_in_sight` and its control `without_the_cap_things_pop_in` (gfa-app, data); `the_default_camera_fits_the_view`; the camera's cap, pillarbox and follow; `SimCamera`'s lead, zoom and AmiGTA1's view at rest. Every tier green: unit, data (20), oracle (all 71 walk and drive runs, traffic, trains), native against wasm, clippy, fmt, wasm-check.
  - Next: drawing the lights, then the traffic gate at 12,000 ticks (lights done in stage 5).

- **2026-11-21. Fix: the rider trailing his bike's seat.**
  - Reported from play: riding forwards, the rider was drawn behind the seat. The app drew the bike on its physics clock but the rider between ticks, up to a quarter of a block apart at speed, and left him on the ground under a jump. A seated rider (and a driver seen in a convertible) is now drawn with the car's own pose. Render only; the simulation is unchanged.
  - New test `the_rider_sits_on_his_bike_on_every_frame` (gfa-app): the rider on the seat on every frame of a 60 Hz display; drawn the old way, 16 render units off.

- **2026-11-21. Fix: people spinning on the spot.**
  - Reported from play: near the player's car and a wall, people spun rapidly. Caused by AmiGTA1's sidestep piling its quarter-turn bend onto a heading that nothing re-aims (decision log). Fixed in `gfa_sim::peds`; the oracle runs the same fix (`tools/oracle/ped_sidestep.pl`), and all 71 walk and drive runs match again.
  - New test `nobody_spins_on_the_spot` (a crowd between a road and a wall for a minute). Every tier green.

- **2026-11-21. M5 stage 3: the crowd.**
  - `gfa_sim::peds` (AmiGTA1's `gta_peds.c`, the crowd's part: spawning, the brain, gaits, crossings, separation, dodging, knockdowns, bodies, the pull-out and knock-off, the ram, the drop). Ped RNG seed 777, AmiGTA1's generator.
  - In `World`: the crowd's tick in AmiGTA1's order, walkers to the fleet, trains killing peds, the player's car running people over, the driver of a taken car dragged out or knocked off his bike, the state hash. The `ped dx dy angle` test order (AmiGTA1's fixture 11). `tools/scripts/ped_runover.txt`.
  - Drawn: the app interpolates each ped between ticks; `gfa-tool` pictures show the crowd; `gfa_formats::style::PED_REMAP_TABLES` and `G24Colour::ped_palette` give each dress its G24 palette (decision log). `gfa-tool run --trace-peds`.
  - Oracle: `fleet.h` runs AmiGTA1's crowd too, and the walk and drive oracles compare every living ped on every tick as well as every car. All 71 runs match.
  - Tests: four synthetic ones (the crowd comes on at the pavements; run over, standing and lying; the driver dragged out, lying, then up; a ped drowning) and the two-style colour test.
  - Speed: Liberty City, 5,000 ticks standing at the start with the full crowd and fleet, 0.22 s (release), 44 µs a tick.
  - Next: the pop-in check and the camera lead and zoom as sim state, drawing the lights, then the traffic gate at 12,000 ticks.

- **2026-11-21. M5 stage 2: the player and the fleet; smooth driving.**
  - `gfa_sim::traffic::contact` (AmiGTA1's `car_place`, `abandon`, `leave_wreck`, `grab_car`, `rename_car`, `box_free`, `sweep_box`, `ram`, `gta_main.c`'s `fleet_car_at` and `gta_weapon.c`'s wreck sweep). `World` keeps no list of its own parked cars: every car but his is in the fleet. The fleet grows to 192 slots.
  - In `World`: RETURN takes the nearest car out of the fleet, standing or moving; getting out leaves the car in the fleet under its old serial, with its dents; his car stops at fleet cars and rams them; walkers, the vault, the slide and the blocked door ask the fleet; trains crush fleet cars, and wrecked trains are left as write-offs that burn out; his write-off burns after the traffic and trains tick, as in AmiGTA1.
  - Oracle: `tools/oracle/fleet.h` runs AmiGTA1's traffic and trains in `walktrace.c` and `drivetrace.c` (the drive harness also restates the contact stop and the ram), and the walk and drive oracles compare every fleet car on every tick. All 71 runs match; 11 drives ram traffic, 46 rams in all. `oracle_traffic` unchanged at 192 slots.
  - The app draws the driven car on its physics clock (decision log); `the_car_moves_on_every_frame` measures it. `gfa-tool run --trace-traffic` prints the fleet every tick.
  - Speed: Liberty City with all 106 parked cars in the fleet runs 5,000 ticks in 0.20 s (release), 40 µs a tick.
  - Next: the peds, then the driver thrown out of a taken car, the pop-in check and camera lead, and drawing the lights.

- **2026-11-21. M5 stage 1: traffic.**
  - `gfa_sim::route` (AmiGTA1's `gta_route.c`) and `gfa_sim::traffic` (`gta_traffic.c`): the fleet is placed around the level start, tops itself up in a ring outside the view, drives its routes, books junctions, stops at red lights, follows, overtakes by changing lanes, turns round, and knocks other cars about when it hits them.
  - In the world: ticked after the player, with the view on him or his car; he is an obstacle the fleet stops for. In the state hash. Drawn in the app between ticks and in `gfa-tool run` pictures.
  - `tools/oracle/traffictrace.c` and `oracle_traffic`: AmiGTA1's traffic gate sites on Liberty City, seeds 1 to 3, 3,000 ticks each, every car's position, layer, heading and speed equal on every tick.
  - Found in AmiGTA1: two locals read before they are set (decision log).
  - Next: the player's car against the fleet and taking cars from it, then peds (done in stage 2).

- **2026-11-21. M4 stage 5: water; Vice City in the README.**
  - On foot he walks into water and drowns; a car that falls short of a jump over water sinks. `World::health` (100, the drowning drains it), `Doing::Drowning`, and a stopgap wasted that starts him again at the level's start until M6.
  - The probe asks for the UP key as AmiGTA1 does, not the stick.
  - `walktrace.c` restates the walk into water and the drowning, and `drivetrace.c` ends its trace when the car sinks. New scripts `walk_water.txt` (151 ticks) and `drive_sink.txt` (75 car ticks) match AmiGTA1. New synthetic test: `walking_into_water_drowns_him`. Every tier green, with clippy and wasm-check.
  - README: how to choose a city, and how to play Vice City (sections 202 and 203, `MIAMI.CMP`, `STYLE003`).
  - A known issue, from the owner: the get-in looks as if he goes in through the wrong side of the door. Logged for M6b, to compare frame by frame with the original.

- **2026-11-20. M4 stage 4: the vault; tank, boats and specials moved to M6b.**
  - `Doing::Vaulting`, `Doing::Sliding`, the far-side get-in (`cars::far_side`), the exit over the roof, SPACE on foot as jump; ped frames 91..94 and 41. `drive_basic.txt` now parks its car door-side; `vault.txt` goes over the roof three ways.
  - Tests: four synthetic (far side, blocked door, jump over a saloon, slide out from under a bus) and a render check that the vaulting player is drawn on his car. Every tier green; native and wasm agree on 20 scripts.
  - The tank, boat handling, the helicopter and the RC car moved to a new M6b, measured from the original under DOSBox-X (decision log).
- **2026-11-20. M4 stage 3: trains, and every check in cargo.**
  - `gfa_sim::trains`, `Doing::Riding`, train sprites; `stand` order; train stops in the run log.
  - Tests: `CmpBuilder` (gfa-formats) and 14 simulation tests on maps built in memory; `every_script_passes`; the Rust oracle tests (`oracle_walk_scripts`, `oracle_drive_scripts`, `oracle_random_walks`, `oracle_random_drives`, `oracle_trains`); `wasm_matches_native`; the render tests (`holecheck_every_map`, `the_simulation_is_drawn`); `gfa-tool` gains a library part for them.
  - Totals: `cargo test` 74 tests in about a second; `cargo data-test` 17; `cargo oracle-test` 6 (48 random runs, 36,000 train ticks, 19 wasm scripts).
- **2026-11-20. M4 stage 2: damage and wrecks.**
  - Dented panels from wall hits, sprite deltas in the atlas (still within 2048 rows on every map), door art on the way in and out, write-offs that burn and blow up, convertible drivers, and a fix: the car being got into was not drawn during the get-in.
  - Oracle and native-vs-wasm checks unchanged and green (nine scripts).
- **2026-11-20. M4 stage 1: the driven car.**
  - `gfa_sim::vehicle` (physics, wall resolve, layer rule), `gfa_sim::cars` (parked cars, door geometry, the per-tick drive with jumps), `World` state machine (on foot, entering, driving, exiting), `park` and `drive` orders.
  - Rendering: parked and driven cars drawn from the simulation, interpolated between ticks; the camera follows the car.
  - Oracle: `drivetrace.c` and driving comparisons; native and wasm agree on all eight scripts.
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
