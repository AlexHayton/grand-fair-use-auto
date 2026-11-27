# Grand Fair Use Auto

A Rust reimplementation of the 1997 *Grand Theft Auto* engine that plays the
original game from **your own copy of its data**, natively on macOS (Metal)
and in the browser (WebGL2), with optional HD art.

This repository contains **no game data**, and nothing derived from it. You
need an original copy, such as the freeware "classic" release or a Director's
Cut install, containing `GTADATA/`.

Status: early. See [IMPLEMENTATION_PLAN.md](IMPLEMENTATION_PLAN.md) for the
roadmap, decision log and progress log.

<a href="https://www.buymeacoffee.com/alexhaytong"><img src="https://cdn.buymeacoffee.com/buttons/v2/default-yellow.png" alt="Buy me a coffee?" height="50"></a>

## Setup

1. Install Rust (stable) with `rustup`. The `rust-toolchain.toml` adds the
   wasm target.
2. Get the game: search for **GTA1 Director's Cut** and download it. It
   comes as a `.rar` or `.zip`; there is no need to unpack it.
3. Point the engine at it. The desktop program takes the first of:
   - `--data <archive or folder>`;
   - `GFA_DATA=<archive or folder>`;
   - `data = "..."` in `gfa.local.toml` (`cp gfa.local.toml.example gfa.local.toml`;
     the file is gitignored);
   - a `.rar` or `.zip` of the game in the same folder as the program (for an
     app bundle, the folder holding it);
   - the archive you chose last time;
   - otherwise choose an archive or folder on the setup screen, or drop the archive there. The app remembers the location
     (`~/Library/Application Support/GrandFairUseAuto/game.toml` on macOS).

   A folder is the one that **contains** `GTADATA/`. An archive may hold it
   at any depth. RAR 5 and ZIP archives are read in place and each file is
   checked against its checksum. Solid RAR archives, RAR 4 and archives
   split into parts are not read; `cargo run -p gfa-tool -- archive <file>`
   says whether an archive will do.

### Windows native builds

If `rustup show active-toolchain` reports `x86_64-pc-windows-gnu`, install
MinGW-w64 binutils (for example, `pacman -S mingw-w64-x86_64-binutils` in the
MSYS2 MINGW64 shell) and add MSYS2's `mingw64\bin` directory to `PATH`.
Cargo's GNU Windows linker needs `dlltool.exe` from that directory; `cargo
check` may pass without it, but `cargo build` will fail while linking
dependencies. Alternatively, use the `x86_64-pc-windows-msvc` Rust toolchain
with Visual Studio C++ Build Tools installed.

## Running

```sh
cargo run --release -p gfa-native                         # title, player and chapter selection
cargo run --release -p gfa-native -- --level 102          # San Andreas
cargo run --release -p gfa-native -- --level 202          # Vice City
cargo run --release -p gfa-native -- --game london69 --level 2
cargo run -p gfa-tool -- info                             # what's in your game data
cargo run --release -p gfa-tool -- archive game.rar --all # will this archive do?
```

You start on foot at the level's starting point. Traffic drives the streets
and people walk the pavements. Any car can be taken: the ones parked around
the city, or one out of the traffic, whose driver you drag out. Your car rams
traffic and traffic shunts back, and runs people over. Weapons, police and
missions arrive in later milestones.

### Choosing a city

`--level` takes a section number from `GTADATA/MISSION.INI`. Each city has two
sections, one per chapter. Both play on the same map, and for now they differ
only in where you start. The map names its style, which is loaded from beside
it: `.G24` if present, otherwise `.GRY`.

| City | `--level` | Map | Style |
|---|---|---|---|
| Liberty City | `1` (default), `2` | `NYC.CMP` | `STYLE001` |
| San Andreas | `102`, `103` | `SANB.CMP` | `STYLE002` |
| Vice City | `202`, `203` | `MIAMI.CMP` | `STYLE003` |
| London 1969 (`--game london69`) | `2`, `3`, `4` | `UK/UK.CMP` | `UK/STYLE001` |

### Playing Vice City

```sh
cargo run --release -p gfa-native -- --level 202     # Vice City, first chapter
cargo run --release -p gfa-native -- --level 203     # Vice City, second chapter
./target/release/gfa --level 202                     # the same, once built
```

- **Web:** run `trunk serve` and open `http://127.0.0.1:8080/?level=202`.
- **Data:** add `--data <archive or folder>` if you have not set
  `gfa.local.toml` or `GFA_DATA`, kept the archive beside the program or
  chosen one before.
- **Checking it loaded:** the log should show `GTADATA/miami.cmp: … faces`
  and Vice City's trains (2 trains, 3 stations).
- **Without a window:** `--screenshot vc.png` saves the first frame and quits.

Testing on Vice City:

```sh
cargo run -p gfa-tool -- run tools/scripts/walk_basic.txt --section 202   # any script, on Vice City's map
cargo run -p gfa-tool -- render GTADATA/MIAMI.CMP --at 9,19               # an offscreen picture of the city
```

A script runs on Vice City if its header says `# section: 202`.

| Key | Does |
|---|---|
| Arrows or WASD | on foot: run forward or back, turn. In a car: accelerate, reverse, steer |
| Shift | walk instead of run |
| Return | get into the nearest car, or out of yours |
| Space | handbrake; on foot, jump: over a low car, under a tall one |
| Scroll wheel, `+` / `-` | zoom. Zooming out stops where the picture would show the edge of the living city, where people and cars come and go; the camera also pulls back by itself as you speed up |
| F | free look: drag with the right mouse button to tilt and turn |
| R | look straight down again |
| C | detach the camera and fly over the city with the arrows (Shift: faster); C again returns to the player |
| Ctrl | fire: a punch with no gun, else the gun you hold (held, it keeps firing) |
| Tab | the special key: the horn, or in a police car, ambulance or fire engine the siren |
| Z / X | previous and next weapon, skipping any with no rounds |
| Keypad `*` | every gun with 99 rounds: the original's own key for it, which needed a cheat; always on until the cheats come (M8) |
| F6 | pause |
| `` ` `` | debug menu (hidden at start): the frame counter, frame time and rate, the simulation's tick and counts (cars, people, shots, fires, explosions), the renderer's counts, the player's block, health, gun, score and wanted level with a **Top up ammo** button (as keypad `*`), the camera and view mode, and the block under the cursor |
| Esc | quit |

The original's keys (its default layout) do what they did there; the debug keys keep off them. F5 (the radio), F7 and F10 (repeat a message), F9 (the area's name), F8 (the frame limiter) and F11 (the screen mode) are kept for when those arrive.

`--screenshot shot.png` saves the first frame and quits; `--size 1600x900` sets the window's size. Windows wider than 21:9 get black bars at the sides.

Inspecting the data (`gfa-tool --help` lists everything):

```sh
cargo run -p gfa-tool -- map GTADATA/NYC.CMP --at 25,18     # a map, or one column of it
cargo run -p gfa-tool -- style GTADATA/STYLE001.G24 --cars  # a style and its vehicles
cargo run -p gfa-tool -- mission GTADATA/MISSION.INI        # the level scripts
cargo run -p gfa-tool -- text GTADATA/ENGLISH.FXT --grep gouranga
cargo run -p gfa-tool -- export GTADATA/STYLE001.G24        # contact sheets in export/ (gitignored)
cargo run -p gfa-tool -- render GTADATA/NYC.CMP --at 64,64  # the city drawn offscreen, to export/render/
cargo run -p gfa-tool -- holecheck GTADATA/NYC.CMP          # undrawn pixels at 8 viewpoints (expect 0)
cargo run -p gfa-tool -- run tools/scripts/walk_ramp.txt     # play a test script and check it
```

`gfa-tool run` plays a script from `tools/scripts/` headless and checks its
`# expect:` and `# reject:` lines. `--hash-every 50` prints state hashes,
`--trace` the player's state every tick, `--out shots/f` a picture for each
`dump`, and `--record input.txt` the input as a replay.

Web (needs `cargo install trunk`):

```sh
cd crates/gfa-web && trunk serve     # http://127.0.0.1:8080
sh tools/web/build.sh                # the release bundle, in crates/gfa-web/dist
```

The page tells the player to search for the game and to **Select game
archive**. It reads only the files a level needs out of the chosen `.rar` or
`.zip`, a slice at a time (about 41 MB of a 215 MB archive), and keeps them in
the browser's storage (IndexedDB), so later visits start straight away.
Nothing is uploaded, and the page's content security policy lets it talk
only to its own site. An unpacked folder works too. "Game data" (top right)
chooses another or forgets the saved files. `?level=102` or
`?game=london69&level=2` picks the level.

On a touch screen the keys appear as buttons along the bottom: a pad for
moving and turning on the left; on the right, Fire, Car (in and out), Jump
(and handbrake), Horn and the weapon buttons. `?touch` shows them anywhere.
"Full screen" (top right) fills the screen, and on a phone asks to turn to
landscape. iPhones give pages no full screen; there the button is hidden, and
adding the page to the home screen opens it without the browser's bars.

### Hosting the web app

The bundle is static and holds no game data. `tools/web/build.sh` builds it
the same way everywhere. Two ways to host it:

- **GitHub Pages:** `.github/workflows/pages.yml` builds and publishes it on
  each push to `main`. Turn it on once under Settings > Pages > Source:
  GitHub Actions. Pages needs a public repository, or a paid plan for a
  private one.
- **Vercel:** `vercel.json` builds it on Vercel (installing Rust and Trunk
  first) and serves `crates/gfa-web/dist`. Import the GitHub repository at
  vercel.com/new, or from a checkout: `npx vercel link`, then `npx vercel`
  for a preview and `npx vercel --prod` for production.

## Checks

```sh
cargo test          # no data needed: unit tests, and the simulation on small maps built in memory
cargo data-test     # against your game data: formats, every level walkable, every script in
                    # tools/scripts/, the holecheck on six maps, the player and cars drawn
cargo oracle-test   # against AmiGTA1 and across platforms (needs more, see below)
cargo test-all      # all of the above
cargo wasm-check    # everything that ships to the web builds for wasm32
cargo clippy --workspace --all-targets
```

`cargo oracle-test` builds AmiGTA1's own walking, driving, train and traffic
code from a sibling `../AmiGTA1` checkout (or `$AMIGTA1`) with clang, runs the
scripts in `tools/scripts/` marked `# oracle: walk` or `# oracle: drive`, 48
random walks and drives over the three cities (with the traffic, trains and
crowd around the player, every car and every ped compared), 12,000 ticks of every city's
trains, and AmiGTA1's traffic test (four Liberty City sites, three seeds,
3,000 ticks each; `GFA_TRAFFIC_TICKS` changes the length) through both, and
compares every tick. Nothing of AmiGTA1 is copied into this
repo. It also runs the scripts natively and as wasm in Node and compares their
logs and state hashes; that needs Node and the wasm-bindgen CLI matching
`Cargo.lock` (Trunk's cached copy is used if present).

## Credits

Grand Fair Use Auto ports knowledge and code from
[Carnage3D](https://github.com/codenamecpp/carnage3d) and AmiGTA1, both MIT.
See [THIRD-PARTY.md](THIRD-PARTY.md) and [docs/REFERENCE_MAP.md](docs/REFERENCE_MAP.md).
Grand Theft Auto is a trademark of Take-Two Interactive. This project is not
affiliated with or endorsed by Rockstar Games or Take-Two Interactive.
