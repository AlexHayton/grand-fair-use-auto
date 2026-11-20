# Grand Fair Use Auto

A Rust reimplementation of the 1997 *Grand Theft Auto* engine that plays the
original game from **your own copy of its data**, natively on macOS (Metal)
and in the browser (WebGL2), with optional HD art.

This repository contains **no game data**, and nothing derived from it. You
need an original copy, such as the freeware "classic" release or a Director's
Cut install, containing `GTADATA/`.

Status: early. See [IMPLEMENTATION_PLAN.md](IMPLEMENTATION_PLAN.md) for the
roadmap, decision log and progress log.

## Setup

1. Install Rust (stable) with `rustup`. The `rust-toolchain.toml` adds the
   wasm target.
2. Point the engine at your data. Choose one:
   - `cp gfa.local.toml.example gfa.local.toml`, then edit `data` (this file is gitignored);
   - set `GFA_DATA=/path/to/GTA`;
   - pass `--data /path/to/GTA`.

   The folder is the one that **contains** `GTADATA/`.

## Running

```sh
cargo run --release -p gfa-native                         # Liberty City, level [1]
cargo run --release -p gfa-native -- --level 102          # San Andreas (202: Vice City)
cargo run --release -p gfa-native -- --game london69 --level 2
cargo run -p gfa-tool -- info                             # what's in your data folder
```

Until the player arrives (M3) the game is a fly-over of the city:

| Key | Does |
|---|---|
| Arrows or WASD (Shift: faster) | pan |
| Scroll wheel, `+` / `-` | zoom |
| F | free look: drag with the right mouse button to tilt and turn |
| R | look straight down again |
| F1 | debug overlay: frame rate, camera, draw counts, the block under the cursor |
| Esc | quit |

`--screenshot shot.png` saves the first frame and quits.

Inspecting the data (`gfa-tool --help` lists everything):

```sh
cargo run -p gfa-tool -- map GTADATA/NYC.CMP --at 25,18     # a map, or one column of it
cargo run -p gfa-tool -- style GTADATA/STYLE001.G24 --cars  # a style and its vehicles
cargo run -p gfa-tool -- mission GTADATA/MISSION.INI        # the level scripts
cargo run -p gfa-tool -- text GTADATA/ENGLISH.FXT --grep gouranga
cargo run -p gfa-tool -- export GTADATA/STYLE001.G24        # contact sheets in export/ (gitignored)
cargo run -p gfa-tool -- render GTADATA/NYC.CMP --at 64,64  # the city drawn offscreen, to export/render/
cargo run -p gfa-tool -- holecheck GTADATA/NYC.CMP          # undrawn pixels at 8 viewpoints (expect 0)
```

Web (needs `cargo install trunk`):

```sh
cd crates/gfa-web && trunk serve     # http://127.0.0.1:8080
```

The page asks for the folder that contains `GTADATA`. It reads the files it
needs in the browser and keeps them in the browser's storage (IndexedDB), so
later visits start straight away; nothing is uploaded. "Game data" (bottom
right) chooses another folder or forgets the saved one. `?level=102` or
`?game=london69&level=2` picks the level.

## Checks

```sh
cargo test                # unit tests (no data needed)
cargo data-test           # tests against your game data
cargo wasm-check          # everything that ships to the web builds for wasm32
cargo clippy --workspace --all-targets
```

## Credits

Grand Fair Use Auto ports knowledge and code from
[Carnage3D](https://github.com/codenamecpp/carnage3d) and AmiGTA1, both MIT.
See [THIRD-PARTY.md](THIRD-PARTY.md) and [docs/REFERENCE_MAP.md](docs/REFERENCE_MAP.md).
Grand Theft Auto is a trademark of Take-Two Interactive. This project is not
affiliated with or endorsed by Rockstar Games or Take-Two Interactive.
