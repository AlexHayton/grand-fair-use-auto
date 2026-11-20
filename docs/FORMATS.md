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
