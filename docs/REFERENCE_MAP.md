# Reference map

Which Carnage3D and AmiGTA1 sources each of our modules is ported from. Each
ported module also credits its sources in its header comment. Paths are
relative to sibling checkouts `../carnage3d` and `../AmiGTA1`.

| Our module | Carnage3D | AmiGTA1 | Status |
|---|---|---|---|
| `gfa-assets::dataset` | `src/CarnageGame.cpp` (`DetectGameVersion`) | `native/gta_main.c:167` (levels) | done |
| `gfa-formats::cmp` | `src/GameMapManager.cpp`, `src/GameDefs.h` | `native/gta_map.c`, `gta_nav.c` | M1 |
| `gfa-formats::style` (G24) | `src/StyleData.cpp` | | M1 |
| `gfa-formats::style` (GRY) | | `native/gta_style.c` | M1 |
| `gfa-formats::fxt` | `src/GameTextsManager.cpp` | `native/gta_text.c` | M1 |
| `gfa-formats::fon` | `src/Font.cpp` | `native/gta_font.c` | M1 |
| `gfa-formats::sdt` | `src/AudioSampleArchive.cpp` | `native/gta_sfx.c` | M1 |
| `gfa-formats::mission` | `docs/ped.txt` | `native/gta_script.c`, `gta_script_run.c` | M1 |
| `gfa-formats::career` | | `native/gta_career.c` | M1 |
| `gfa-render` city mesh | `src/GameMapHelpers.cpp`, `src/MapRenderer.cpp` | `native/gta_render.c` | M2 |
| `gfa-render` shaders | `gamedata/shaders/*.glsl` | | M2 |
| `gfa-fixed` | | `native/gta_trig.c` | M3 |
| `gfa-sim` vehicles | | `native/gta_vehphys.c`, `gta_car.c` | M4 |
| `gfa-sim` traffic, police | | `native/gta_traffic.c`, `gta_route.c` | M5–M6 |
| `gfa-sim` peds | | `native/gta_peds.c`, `gta_player.c` | M3–M5 |
| `gfa-sim` mission VM | | `native/gta_script*.c`, `gta_trig.c` | M7 |
