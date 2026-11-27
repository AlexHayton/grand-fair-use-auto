# File formats

Notes on the original game's files as our parsers read them. All
little-endian. Code lives in `crates/gfa-formats`.

## CMP: city maps (v331) (`gfa_formats::cmp`)

Sources: DMA's "CityScape Data Structure" (cds.doc v3.10) as read by
Carnage3D `GameMapManager.cpp` and AmiGTA1 `gta_map.c`.

| Section | Size | Contents |
|---|---|---|
| header | 28 | `u32` version (331); `u8` style, `u8` sample, 2 reserved; `u32` sizes of routes, objects, columns, blocks, navigation |
| base | 262,144 | 256×256 `u32` **byte** offsets into the column data, row-major `[y][x]` |
| columns | header | `u16` words. Per column: the number of empty layers `e`, then `6 − e` block indices, **top block first** |
| blocks | header | 8-byte records: `u16` type, `u8` extended type, faces W E N S lid |
| objects | header | 14-byte startup objects: `i16` x y z, `u8` type, `u8` remap, `i16` rotation pitch roll |
| routes | header | `u8` count, `u8` type (0xff police patrol, 0xfe other, else roadblock list for district *n*), `count` × `(x, y, z)` |
| locations | 108 | 6 groups × 6 × `(x, y, z)`: police, hospitals, unused, unused, fire, unused; all-zero = absent |
| navigation | header | 35-byte districts: `x y w h sample name[30]` |

- Layer `z = 0` is the ground; block for `z` = `column[start + height − z]`.
- **Block type word:**

  | bits | meaning |
  |---|---|
  | 0–3 | directions up, down, left, right |
  | 4–6 | ground: 0 air, 1 water, 2 road, 3 pavement, 4 field, 5 building |
  | 7 | flat |
  | 8–13 | slope |
  | 14–15 | lid rotation |
- **Extended byte:**

  | bits | meaning |
  |---|---|
  | 0–2 | traffic hint (1 lights, 4–7 train markers) |
  | 3–4 | lid remap |
  | 5 | flip top/bottom |
  | 6 | flip left/right |
  | 7 | railway |
- Ground and direction bits describe the block *below* the one that draws them (cds.doc; Carnage3D `FixShiftedBits`). The parser keeps the raw records; the sim decides (M3, following AmiGTA1's `gta_map_ground_type`).
- Empty district records are kept: roadblock route type *n* refers to district index *n* (AmiGTA1).
- Startup objects contain duplicate car records. Carnage3D removes them; we keep the raw list and leave that choice to the sim.
- Startup object fields (settled in M2, by rendering them):
  - `x`, `y` are map pixels (64 per block) at the object's centre.
  - `z` counts **down** from the top of the six-layer map: the height is `383 − z` pixels, so 255 is the ground at 128 (Carnage3D inverts the layer the same way). NYC: 2,863 of 3,015 objects are at 255.
  - `rotation` is 1024 steps a turn, clockwise from north: 256 is east (Carnage3D's reading). Parked cars on the south side of an east–west road have 256, so they face east, with the traffic.
  - A `remap` of 128 or more marks a car: `kind` is the car's **model** number (the car record whose `model` matches) and `remap − 128` its colour remap (0 is none). Otherwise `kind` indexes the style's object table.
- The game's car, people and object art faces **south** when unrotated, so a sprite heading `h` is drawn turned by `h` plus half a turn (AmiGTA1 `GTA_SPRITE_ART_SOUTH`, proved there with a test picture).
- The routes section is read in full. AmiGTA1 caps nodes per route at 24; we keep all of them.

What our data test sees in the Director's Cut:

| Map | Style | Blocks | Objects | Routes (police) | Districts |
|---|---|---|---|---|---|
| NYC.CMP | 1 | 5318 | 3015 | 52 (22) | 28 |
| SANB.CMP | 2 | 4022 | 2685 | 57 (36) | 17 |
| MIAMI.CMP | 3 | 3595 | 2243 | 66 (40) | 11 |
| UK/UK.CMP | 1 | 2230 | 2779 | 26 (26) | 14 |
| UK/MC.CMP | 1 | 1748 | 3126 | 0 | 1 ("0") |
| UK/UKE.CMP | 1 | 2231 | 2798 | 26 (26) | 15 |

### UK.MAP and MC.MAP (open question, M11)

Both start with a v331 CMP header whose column and block sizes are **zero**.
The `UK.MAP` object section is 38,906 bytes (= 2,779 × 14, the same count as
`UK.CMP`), with a 525-byte navigation section (15 districts). After the
navigation section come exactly 2,883,584 bytes (256 × 256 × 44). This looks
like an uncompressed editor-format map. Not needed to play; investigate in M11.

## Style files: GRY (v325) and G24 (v336) (`gfa_formats::style`)

Sources: Carnage3D `StyleData.cpp` (G24) and AmiGTA1 `gta_style.c` (GRY).

A map's header names its style by number; the file beside the map is
`STYLE00n.G24` for GTA and London 1969, but `STY00n.G24` for London 1961,
whose folder holds both (Carnage3D `GetStyleFileName`). `gfa_assets::level`
picks by game, and works out a map's game from which mission file names it.
The two formats share every section except the header, the colour data and
the sprite record. Sections follow the header in this order:

| Section | GRY | G24 |
|---|---|---|
| header | 13 × `u32` (52 bytes): version, side, lid, aux, anim, palette, remaps, lid remaps, objects, cars, sprite info, sprite graphics, sprite numbers | 16 × `u32` (64 bytes): version, side, lid, aux, anim, CLUT data, tile/sprite/remap/font CLUT sizes, palette index, objects, cars, sprite info, sprite graphics, sprite numbers |
| tiles | side, lid, aux tiles: 64×64 bytes each, in 256×256 pages of 4×4 tiles | same |
| tile padding | the last page row is completed with blank tiles; **the header sizes leave the padding out** | same |
| animations | `u8` count, then per animation `u8 tile, which (0 side, 1 lid), speed, n`, `n` aux tile numbers | same |
| colour | 768-byte palette (6-bit VGA values); 256 remap tables × 256 bytes; 256 × 4 lid remap bytes | CLUT data, rounded **up** to whole 64 KiB pages; `u16` palette index |
| objects | 20 bytes (`i32` w h depth, `u16` sprite, `i16` weight aux, `u8` status, `u8` n) + `n` × `u16` | same |
| cars | 174 bytes + 8 per door (≤ 4) | same |
| sprites | 10 bytes: `u8` w h deltas pad, `u16` size, `u8` x y, `u16` page; + 6 per delta (`u16` size, `u32` offset) | 12 bytes: adds a `u16` CLUT after size |
| sprite graphics | 256×256 pages; deltas live here too | same |
| sprite numbers | 21 × `u16` category counts | same |

The parser checks that the sizes (plus the tile padding and the CLUT rounding)
add up to the file length, that every variable-length section is consumed to
the byte, that `size == w × h` for every sprite, that the category counts sum
to the sprite count, and that every sprite and every delta run lies inside the
graphics and inside its sprite.

- **G24 colour.** CLUT pages hold 64 palettes: 256 rows (one per colour
  index), each row that colour in all 64 palettes, as BGRA. The palette index
  maps *logical* CLUTs to physical palettes: 4 per tile (one per lid remap;
  entry 3 equals entry 0 for lids), then sprite CLUTs, then remap CLUTs
  (12 per car, then 64 for pedestrians), then font CLUTs. STYLE001.G24 has
  2,229 logical CLUTs over 968 palettes.
- **GRY colour.** One palette, scaled to 8 bits by bit replication
  (`v << 2 | v >> 4`). Remap tables 1..43 are the vehicles' (a car's
  `remap8` bytes are table numbers) and 128..187 the pedestrians'. The 1,024
  bytes AmiGTA1 calls "unknown_b" are **lid remaps**: per lid tile, the remap
  table for each of the map's four lid remaps. Entry 0 is table 0, the
  identity. **Confirmed in M2 by measurement:** for every lid and remaps 1
  and 2, the change in a tile's mean colour under the GRY table points the
  same way as under the G24 palette. STYLE001: 302 of 306 agree, and
  swapping the two remaps agrees with only 2 (the two remaps differ there).
  STYLE002: 319 of 334. STYLE003: 262 of 286. In those two the swapped
  remaps agree too (303 and 213), because both remaps shift colour the same
  way, so the swap tells nothing there.
- **Deltas** are runs of `u16 skip, u8 len, len pixels`. The cursor advances
  in units of the **256-pixel page width**, not the sprite width, and
  positions are relative to the sprite's top-left. The contact sheets show
  doors opening, damage, and the siren lights of police cars and ambulances.
- **Car table.** Physics values are 16.16 fixed point, kept raw. The second
  dimension is the length along the vehicle and the third its height
  (AmiGTA1). The tank is model 37 (0x25), class 14. The helicopter, model 88,
  is filed as class 0 (bus).

What the data test sees:

| File | Tiles side/lid/aux | Anims | Objects | Cars | Sprites | Deltas | Colour |
|---|---|---|---|---|---|---|---|
| STYLE001.GRY | 195/154/31 | 5 | 102 | 38 | 1009 | 383 | palette + 256 remaps |
| STYLE001.G24 | 195/154/37 | 5 | 102 | 38 | 1034 | 383 | 968 palettes |
| STYLE002.GRY | 194/168/24 | 2 | 102 | 40 | 1012 | 414 | palette + 256 remaps |
| STYLE002.G24 | 194/168/29 | 2 | 102 | 40 | 1037 | 410 | 1528 palettes |
| STYLE003.GRY | 198/149/30 | 2 (declares 3) | 102 | 40 | 1024 | 406 | palette + 256 remaps |
| STYLE003.G24 | 198/149/37 | 2 | 102 | 40 | 1049 | 409 | 1697 palettes |
| UK/STYLE001.GRY | 195/154/31 | 5 | 102 | 38 | 1009 | 383 | palette + 256 remaps |
| UK/STYLE001.G24 | 195/154/37 | 5 | 102 | 38 | 1034 | 383 | 968 palettes |
| UK/STY001.G24 | 195/154/37 | 5 | 102 | 38 | 1034 | 383 | 968 palettes |

Differences between formats and files:

- **G24 is the richer format.** It has 25 more arrow (HUD) sprites, more aux
  tiles (its animations have more frames), and some sprites differ in size or
  delta count from the GRY. Objects and side/lid tile counts match.
- **STYLE003.GRY declares 3 animations but holds 2,** which end exactly at the
  section boundary. The parser stops at the boundary and keeps the declared
  count (`Style::animations_declared`).
- **London's GRY carries the base game's car table.** `UK/STYLE001.GRY`'s cars
  are byte-for-byte those of `STYLE001.GRY`, while `UK/STYLE001.G24` (1969) and
  `UK/STY001.G24` (1961) each have their own, and differ from each other. London
  must therefore use the G24 styles.
- Every map names only tiles its style has (NYC: lids up to 153 and sides up to
  194, against 154 and 195).

Review the art with `gfa-tool export GTADATA/STYLE001.G24`, which writes
`tiles.png`, `sprites.png`, `remaps.png` and `deltas.png` to the gitignored
`export/` folder.

## FXT: text tables (`gfa_formats::fxt`)

Sources: gta.mendelsohn.de's description as read by Carnage3D
`GameTextsManager.cpp` and AmiGTA1 `gta_text.c`.

- A run of `[key]text\0` records under a light cipher. The first byte picks the
  variant: `0xBF` means key `0x63`, offset −1; `0xA6` means key `0x67`, offset +28.
  The key is subtracted from the first eight bytes, doubling each time, and the
  offset is added to every byte. A byte that decodes to 195 is dropped and 64 is
  added to the next, which puts accented letters in Windows-1252.
- **Every FXT in the Director's Cut uses `0xBF`,** including London's and the DOS
  launcher's (`GTADOS/DATA`).
- Records can be followed by **extra NUL bytes** (London's ENGUK.FXT has 69 such gaps).
- An **empty key `[]` ends the table.** Most files then carry a few junk bytes
  (`\x1a\0[]`).
- **Keys repeat.** In ENGLISH.FXT six keys appear twice (`2500` MISSION
  COMPLETE!, `2501`…), with identical text, except `crimes`: "Criminal Record"
  first, "Crimes" later. The first record wins (AmiGTA1's behaviour). Which one
  the game shows where is open (M8).
- Text is kept as bytes, because the fonts index bytes. `cp1252_to_string`
  converts for display; FRENCH.FXT uses `0x9c` (œ), which no font has a glyph for.
- Keys are numbers for the script (1001… briefs, 2500… messages, 4000–4005 the
  big cards, 4003 BUSTED!, 4004 WASTED!, 8787 GOURANGA!) and names for the code
  (`bomb_on`, `crane0`, `no_tank_respray`, `win_race`…).

| File | Keys |
|---|---|
| ENGLISH / SPECIAL | 1792 |
| FRENCH / GERMAN / ITALIAN | 1798 / 1800 / 1799 |
| UK/ENGUK, FREUK | 911 |
| UK/GERUK, ITAUK, fixed | 935 / 933 / 933 |
| UK/enguke, enge8 (1961) | 569 |
| UK/SPEUK | 1792 |
| GTADOS/DATA/*.FXT (launcher) | 279–284 |

`gfa-tool text GTADATA/ENGLISH.FXT [--key K] [--grep S]` looks texts up.

## FON: bitmap fonts (`gfa_formats::fon`)

Sources: Carnage3D `Font.cpp`, AmiGTA1 `gta_font.c`.

- `u8` glyph count, `u8` height; per glyph `u8` width and `width × height`
  palette indices; then a 768-byte **8-bit** RGB palette of the font's own.
  Index 0 is transparent. The parser requires the file to end exactly there:
  all 191 FON files do.
- Glyph `n` is character `0x21 + n` (`!` first) up to DEL (glyph 94). Accented
  Windows-1252 letters from 0xC0 start at glyph 95, mapped by Carnage3D's table
  (`fon::ACCENTS`). The contact sheets confirm it:
  - **140-glyph fonts** (`F_MTEXT`, `STREET1`…) have the upper-case accents À…ß,
    then the lower-case à…ü.
  - **118-glyph fonts** (`PAGER1`, `BIG1`…) stop at ß, so they only have upper-case
    accents. Their lower-case ASCII glyphs are blank too, which is why the
    original prints those fonts in capitals (AmiGTA1).
- The rest are not alphabets: `SCORE*`/`EXPSCOR*` hold the 10 digits,
  `MISSMUL*` the multiplier (11 glyphs), `F_CITY*`/`F_KEY` the front-end
  pictures, and `CUT*.FON` the briefing-screen artwork in 1–10 tall glyphs.

`gfa-tool export GTADATA/PAGER1.FON` writes a glyph sheet to `export/fonts/`.

## SDT/RAW: sound banks (`gfa_formats::sdt`)

Sources: Carnage3D `AudioSampleArchive.cpp`, AmiGTA1 `gta_sfx.h`.

- The `.SDT` is only 12-byte records `{u32 offset, u32 length, u32 rate}`; the
  `.RAW` is the samples back to back. In **all 12 banks** the records are
  contiguous and the last ends exactly at the end of the `.RAW`.
- **Encoding is not in the index.** Measured on the data (high bytes smooth,
  low bytes noise means 16-bit little-endian):
  - `LEVEL000` (front end) is **16-bit signed**, all 16 samples.
  - `LEVEL001`–`003`, `VOCALCOM` and `MISBRIEF` are **8-bit unsigned**.
- Open (M8):
  - Carnage3D treats LEVEL000's first three samples as stereo. The measurement
    can't tell mono from stereo, so that waits until the samples can be heard.
  - London's `LEVEL000` samples 0 and 1 look like neither 8- nor 16-bit PCM.
  - `UK/AUDIO/*.PK` is unidentified (M11).

| Bank | Samples | Rates (most common) |
|---|---|---|
| LEVEL000 | 16 | 30000, 32000, 22050, 44100 |
| LEVEL001–003 | 131 each | 5500, 22050, 11025, 8000 |
| VOCALCOM | 71 | 11025, 22050 |
| MISBRIEF | 10 | 11025, 16000, 22050 |

## Front-end and briefing pictures (`gfa_formats::screen`)

Source: AmiGTA1 `gta_front.h` for the screen split. The other sizes were found
from the data: the width whose rows line up best, then confirmed by looking.

- No headers. Each picture comes as a **`.RAW` (24-bit RGB, the master)** and a
  **`.RAT` (8-bit indices)** into a 768-byte **8-bit** RGB palette: `F_PAL.RAW`
  for the front end, the same-named `.ACT` for a briefing still.
- The `.RAT` is a quantised copy. Through its palette it comes within 1–18
  levels (mean) of the `.RAW`, and some London files match exactly. Channel
  order is RGB. **We use the `.RAW`**; the `.RAT` stays parsed for an
  "original 8-bit" look.
- AmiGTA1 calls the components 6-bit; in this release they are 8-bit (maximum 255).

| Stem | Size | What |
|---|---|---|
| `F_LOGO0`–`7`, `F_LOGOE0`–`7` (London 61) | 640×168 | animated logo, top strip |
| `F_UPPER` | 640×168 | static top strip |
| `F_LOWER0`–`1` (London `0`–`4`) | 640×312 | background, bottom strip |
| `F_PLAY1`–`8`, London `F_PLAY1A` | 102×141 | player portraits |
| `F_PLAYN` | 180×50 | name plate |
| `F_BMG` | 100×50 (London 75×90) | BMG logo |
| `F_DMA` | 78×109 | DMA Design logo |
| London `F_DMA` | 225×38 | the 1999 Rockstar/Take 2 copyright notice |
| `CUT0`–`5`, London `CUT0E` | 640×480 | briefing stills (with `AUDIO/0–5.WAV`) |

London lacks `CUT4.RAT` and `CUT5.RAT` (it has their `.RAW` and `.ACT`).
`UK/MAINPAL.ACT` matches none of the pictures; it is probably the in-game
palette for the 8-bit mode.

`gfa-tool export GTADATA/CUT0.RAT` writes `export/screens/cut0_rat.png`.

## PLAYER_A.DAT: saves (`gfa_formats::career`)

Source: AmiGTA1 `gta_career.c`, from the original's read and write routines.
0x3f1 bytes, a raw memory dump, no header. The parser keeps the bytes and
reads fields through accessors, so saves round-trip exactly. The layout table
is in the module header. The Director's Cut ships the original's defaults:
slot 1 "Travis", only Liberty City 1 unlocked, high scores BILLY/PAUL/STEVE.
London's `PLAYER_A.DAT` (767 bytes) and `PLAYER_E.DAT` (404) differ (M11).

## CDAUDIO.DAT: CD track map (`gfa_formats::cue`)

A CUE sheet from the re-release: `FILE "TrackNN.ogg"`, `TRACK nn AUDIO`,
`INDEX 01 00:00:00`. CD track 1 (the data track) is mapped to `Track02.ogg`.
GTA has 11 entries (Track02–11), London 16 (Track02–16).

## MOVIE.SMK (probe; decoder in M9)

| File | Format | Size | Frames | Rate | Audio |
|---|---|---|---|---|---|
| `GTADATA/MOVIE.SMK` | SMK2 | 320×200 | 415 | 25 fps | 44.1 kHz, 16-bit, mono, compressed |
| `GTADATA/UK/MOVIE.SMK` | SMK2 | 320×200 | 314 | 30 fps | 44.1 kHz, 16-bit, stereo, compressed |

## REP: recorded demos (probe; M8)

`1.REP`, `102.REP`, `202.REP` (one per city) and London's `4.REP` are whole
8-byte records `{u32 tick, u32 input}`. Ticks never decrease, the files run to
about tick 1,500, and they end with the same marker records (`0x40800040`,
`0xe000040`). They look like input-change logs, which is what attract mode
needs and possibly a fidelity oracle. The input bits are not decoded yet.
`REPLAY.REP` is a single 8-byte record. London's `4.REP` starts with what may
be a header record (`tick 0, 0x4e000040`).

## MISSION.INI: level scripts (`gfa_formats::mission`)

Source: AmiGTA1 `gta_script.c` and `gta_script_run.c`. Also read:
`UK/MISSUK.INI` (1969), `UK/missuke.ini` (1961) and `UK/misse8.ini` (a copy of
`missuke.ini`).

Each `[N]` section has a title line (`name, 22, map.cmp, 0,`), a settings line
(`100 1 1 1 1 0`), the declarations up to a line starting `-`, and the commands
up to the next one.

- **Declaration:** `id [flag] (x,y,z) NAME numbers…`. Coordinates are blocks or
  pixels depending on the kind (AmiGTA1 `gta_script_is_block`).
- **Command:** `id NAME numbers…`. `p2`/`p3` are the success and failure jumps:
  0 means next, −1 means end the process, anything else is a line label. `p5`
  is the score.
- **We keep every number.** AmiGTA1's declaration reader stops at four, which
  loses the fifth (the remap) on all 19 `SPECIFIC_DOOR`s and 2 `SPECIFIC_BARR`s.
- **Quirks in the files:**
  - `[1003]` is glued to the end of a `-1` line (`-1[1003]`). A reader that
    only looks for `[` at the start of a line loses the whole section.
  - The flag can touch the bracket: `9000 1(122,155,1) POWERUP 13 0`.
  - One coordinate has a full stop for a comma: `(57,124.4)`, in a race section.
  - London writes map paths with a backslash (`uk\uk.cmp`).
  - Files end with a DOS `0x1a`.
  - One 1961 `MOBILE_BRIEF` has six numbers; everything else has at most five.

| File | Sections |
|---|---|
| MISSION.INI | [1] [2] Liberty City, [102] [103] San Andreas, [202] [203] Vice City; multiplayer [1001] deathmatch and [1002]–[1004] "Canonball Run" races per city (1001–1004, 1101–1104, 1201–1204) |
| UK/MISSUK.INI (1969) | [1] intro, [2] easy, [3] medium, [4] hard; multiplayer [1001]–[1004] |
| UK/missuke.ini (1961) | [1] campaign on `uke.cmp`; [1001] deathmatch on `mc.cmp` |

| Section | Declarations | Commands |
|---|---|---|
| [1] | 575 (matches AmiGTA1: 151 POWERUP, 128 TELEPHONE, 95 DUMMY, 55 TRIGGER…) | 2116 (matches AmiGTA1) |
| [2] | 676 | 2765 |
| [102] | 668 | 3121 |
| [103] | 767 | 2911 |
| [202] | 1006 | 4354 |
| [203] | 876 | 4863 (AmiGTA1: "Vice City 2 has 4863") |
| UK [2] / [3] / [4] | 301 / 293 / 319 | 1877 / 1828 / 1897 |
| 1961 [1] | 349 | 2257 |

The base game uses 130 distinct commands, London 1969 79 and 1961 75. MC.CMP
turns out to be the 1961 deathmatch map. `gfa-tool mission <path> [--section N] [--dump]`
lists sections and tallies kinds and commands. Without multiplayer, the base
game uses 129 commands. `gfa-tool mission coverage` reports each name against
GTA8.EXE's tables and gfa-sim (`MISSION_OPCODES.md`).
