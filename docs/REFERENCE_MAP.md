# Reference map

Which Carnage3D and AmiGTA1 sources each of our modules is ported from. Each
ported module also credits its sources in its header comment. Paths are
relative to sibling checkouts `../carnage3d` and `../AmiGTA1`.

| Our module | Carnage3D | AmiGTA1 | Status |
|---|---|---|---|
| `gfa-assets::dataset` | `src/CarnageGame.cpp` (`DetectGameVersion`) | `native/gta_main.c:167` (levels) | done |
| `gfa-formats::cmp` | `src/GameMapManager.cpp`, `src/GameDefs.h` | `native/gta_map.c`, `gta_nav.c` | done |
| `gfa-formats::style` (G24) | `src/StyleData.cpp`, `src/GameDefs.h` (`eSpriteType`) | `native/gta_car.h` (car fields) | done |
| `gfa-formats::style` (GRY) | `src/StyleData.cpp` (shared sections) | `native/gta_style.c`, `gta_style.h`, `gta_car.h` | done |
| `gfa-formats::fxt` | `src/GameTextsManager.cpp` | `native/gta_text.c` | done |
| `gfa-formats::fon` | `src/Font.cpp` (accent table) | `native/gta_font.c` | done |
| `gfa-formats::sdt` | `src/AudioSampleArchive.cpp` | `native/gta_sfx.h` | done |
| `gfa-formats::screen` | | `native/gta_front.h` (screen split) | done |
| `gfa-formats::mission` | | `native/gta_script.c`, `gta_script_run.c` (layout) | done |
| `gfa-formats::career` | | `native/gta_career.c`, `gta_career.h` | done |
| `gfa-render::city_mesh` | `src/GameMapHelpers.cpp` (`PutBlockFace`), `src/MapRenderer.cpp` (chunks) | `native/gta_render.c` (`draw_block`: flat blocks, lids), `tools/gtabake.c` (lid rotation) | done |
| `gfa-render::style_tables` | `src/SpriteManager.cpp` (palette tables, `InitBlocksAnimations`) | `native/gta_main.c` (25 Hz frame, door note) | done |
| `gfa-render::city`, `shaders/city.wgsl` | `gamedata/shaders/city_mesh.glsl` | | done |
| `gfa-render::sprites`, `shaders/sprites.wgsl` | `src/SpriteBatch.cpp`, `gamedata/shaders/sprites.glsl`, `src/StyleData.cpp` (`GetSpritePaletteIndex`), `src/GameObjectsManager.cpp` (`CreateStartupObjects`) | `native/gta_player.h` (`GTA_SPRITE_ART_SOUTH`), `native/gta_render.c` (sprite rotation) | done |
| `gfa-render::camera` | `src/FollowCameraController.cpp` | | done |
| `gfa-assets::level` | `src/GameMapManager.cpp` (`GetStyleFileName`) | | done |
| `gfa-tool holecheck` | | `tools/bin/holecheck.sh` | done |
| `gfa-fixed` | | `native/gta_trig.c` | M3 |
| `gfa-sim` vehicles | | `native/gta_vehphys.c`, `gta_car.c` | M4 |
| `gfa-sim` traffic, police | | `native/gta_traffic.c`, `gta_route.c` | M5–M6 |
| `gfa-sim` peds | | `native/gta_peds.c`, `gta_player.c` | M3–M5 |
| `gfa-sim` mission VM | | `native/gta_script*.c`, `gta_trig.c` | M7 |
