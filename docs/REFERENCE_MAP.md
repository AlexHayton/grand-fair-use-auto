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
| `gfa-fixed::trig`, `gfa-fixed::rng` | | `native/gta_trig.c`; generators in `gta_traffic.c`, `gta_peds.c`, `gta_train.c`, `gta_main.c` (`voice_rand`) | done |
| `gfa-sim::clock` | | `native/gta_main.c` (`SIM_HZ`, `MAX_CATCHUP`), `gta_vehphys.c` (`TIME_NUM`/`TIME_DEN`), `gta_script_run.c`, `gta_marks.c`, `gta_train.c` (`half`), `gta_peds.c` (`FRAMES`) | done |
| `gfa-sim::map` | `src/GameDefs.h` (`eGroundType`) | `native/gta_map.c` (`gta_map_ground_type`, ramps), `gta_nav.c` (nav byte, thin walls, `seg_cross`) | done |
| `gfa-sim::player` | `gamedata/entities/ped_animations.json` (frames) | `native/gta_player.c`, `gta_player.h` | done |
| `gfa-sim::view` | | `native/gta_main.c` (`gta_peds_set_view`, `gta_traffic_set_view_blocks`) | done |
| `gfa-sim::script`, `gfa-sim::replay` | | `native/gta_main.c` (autodrive orders), `tools/gtadump.c` (`cmd_walk`) | done |
| `gfa-sim::script_world` | | `native/gta_script.h` (`gta_script_world`) | skeleton |
| `tools/oracle/walktrace.c` | | links `native/gta_map.c`, `gta_nav.c`, `gta_player.c`, `gta_trig.c`, `gta_style.c`, and the fleet (`fleet.h`) | done |
| `tools/oracle/fleet.h` | | links `native/gta_traffic.c`, `gta_route.c`, `gta_car.c`, `gta_train.c`; restates `gta_main.c`'s `fleet_car_at`/`ped_car_block` and `gta_weapon.c`'s `wreck_sweep` | done |
| `gfa-sim::vehicle` | | `native/gta_vehphys.c`, `gta_vehphys.h` | done |
| `gfa-sim::cars` | `src/GameObjectsManager.cpp` (`CreateStartupObjects`) | `native/gta_main.c` (`car_door_point`, `car_door_pos`, enter and exit, the jump, `ped_car_block`, `car_body_hit`, the vault and the slide) | wip |
| `tools/oracle/drivetrace.c` | | links `native/gta_vehphys.c`, `gta_style.c`, `gta_nav.c`, `gta_map.c`, `gta_trig.c`, and the fleet (`fleet.h`); restates the ram glue in `gta_main.c` | done |
| `gfa-sim::trains` | | `native/gta_train.c`, `gta_train.h` | done |
| `tools/oracle/traintrace.c` | | links `native/gta_train.c`, `gta_map.c`, `gta_trig.c` | done |
| `gfa-sim` boats, tank, specials | | `native/gta_car.c`, `gta_traffic.c:1152` | M4 |
| `gfa-sim::traffic` (fleet, driving, junction bookings, lights, collisions between cars) | | `native/gta_traffic.c`, `gta_traffic.h` | wip (M5) |
| `gfa-sim::traffic::contact` (cars put into the fleet, taken out of it, the ram, walkers held off, write-offs) | | `native/gta_traffic.c` (`car_place`, `gta_traffic_abandon`, `_leave_wreck`, `_grab_car`, `_rename_car`, `_box_free`, `_sweep_box`, `_ram`), `gta_main.c` (`fleet_car_at`, `ped_car_block`, `leave_car`), `gta_weapon.c` (`wreck_sweep`) | done |
| `gfa-sim::route` | | `native/gta_route.c`, `gta_route.h` | done |
| `tools/oracle/traffictrace.c` | | links `native/gta_traffic.c`, `gta_route.c`, `gta_nav.c`, `gta_map.c`, `gta_car.c`, `gta_style.c`, `gta_trig.c`, `gta_vehphys.c` | done |
| `gfa-sim` police | | `native/gta_traffic.c:9681-10500` | M6 |
| `gfa-sim` peds | | `native/gta_peds.c` | M5 |
| `gfa-sim` mission VM | | `native/gta_script*.c`, `gta_trig.c` | M7 |
