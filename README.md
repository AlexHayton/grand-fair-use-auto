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
cargo run -p gfa-native              # the game (window opens; levels arrive in M2)
cargo run -p gfa-tool -- info        # what's in your data folder
```

Web (needs `cargo install trunk`):

```sh
cd crates/gfa-web && trunk serve     # http://127.0.0.1:8080
```

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
