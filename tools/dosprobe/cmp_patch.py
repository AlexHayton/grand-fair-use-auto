#!/usr/bin/env python3
"""Turn a stretch of road into field in a copy of a city map, for a probe.

Reads the owner's own GTADATA/<MAP>.CMP and writes a patched copy to
<out>/GTADATA/<MAP>.CMP (an overlay for tools/dosprobe/run.py; nothing of the
game's is committed). In every column of the given rectangle, each block of
ground type Road becomes Field, with its traffic arrows and traffic-light
hint cleared. Traffic neither spawns on nor drives into field, and the lids
are unchanged, so it still looks like road.

Columns and block records are shared across the map, so the patched ones are
appended (an identical block record is reused if there is one) and only the
rectangle's cells are pointed at them; everything after the block section is
copied byte for byte. Layout as in crates/gfa-formats/src/cmp.rs:

    header (28 bytes: version, style, sample, 2, then route, object, column,
    block and navigation sizes) | base 256x256 u32 byte offsets into the
    columns | columns (u16: empty layers, then block indices top-down) |
    blocks (8 bytes: u16 type, u8 ext, faces W E N S lid) | objects | routes
    | locations | navigation

    cmp_patch.py --map MIAMI.CMP --field 96-99,26-98 --out build/overlays/tank_range

Grand Fair Use Auto, MIT.
"""

import argparse
import pathlib
import struct
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from run import resolve_data_dir  # noqa: E402

HEADER = 28
DIM = 256
LAYERS = 6
ROAD, FIELD = 2, 4


def span(spec: str) -> tuple[int, int]:
    a, _, b = spec.partition("-")
    return int(a), int(b or a)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--data", help="Game folder (default: GFA_DATA or gfa.local.toml)")
    ap.add_argument("--map", required=True, help="Map file in GTADATA, e.g. MIAMI.CMP (any case)")
    ap.add_argument("--field", action="append", default=[],
                    help="Rectangle 'x0-x1,y0-y1' (blocks, inclusive) whose road becomes field")
    ap.add_argument("--lower", action="append", default=[],
                    help="Block 'x,y' whose ground is lowered one layer, to the height of the "
                         "water beside it (a quay becomes a pier level with the boats)")
    ap.add_argument("--out", required=True, type=pathlib.Path)
    args = ap.parse_args()

    data = resolve_data_dir(args.data)
    gtadata = next(p for p in data.iterdir() if p.name.upper() == "GTADATA")
    src = next((p for p in gtadata.iterdir() if p.name.upper() == args.map.upper()), None)
    if src is None:
        raise SystemExit(f"no {args.map} in {gtadata}")
    raw = src.read_bytes()

    version, = struct.unpack_from("<I", raw, 0)
    if version != 331:
        raise SystemExit(f"{src.name}: CMP version {version}, not 331")
    route_size, object_size, column_size, block_size, nav_size = struct.unpack_from("<5I", raw, 8)
    base_at = HEADER
    col_at = base_at + DIM * DIM * 4
    blk_at = col_at + column_size
    rest_at = blk_at + block_size
    base = list(struct.unpack_from(f"<{DIM * DIM}I", raw, base_at))
    cols = list(struct.unpack_from(f"<{column_size // 2}H", raw, col_at))
    blocks = [raw[blk_at + i: blk_at + i + 8] for i in range(0, block_size, 8)]
    index = {b: i for i, b in reversed(list(enumerate(blocks)))}

    def as_field(i: int) -> int:
        b = blocks[i]
        t, ext = struct.unpack_from("<HB", b, 0)
        if (t >> 4) & 7 != ROAD:
            return i
        # Bits 0-3 are the arrows, 4-6 the ground type: Field, no arrows.
        t = (t & ~0x7F & 0xFFFF) | (FIELD << 4)
        ext &= ~7 & 0xFF  # no traffic-light hint
        nb = struct.pack("<HB", t, ext) + b[3:]
        if nb not in index:
            index[nb] = len(blocks)
            blocks.append(nb)
        return index[nb]

    cells = changed = 0
    for spec in args.field:
        xs, ys = spec.split(",")
        (x0, x1), (y0, y1) = span(xs), span(ys)
        for y in range(y0, y1 + 1):
            for x in range(x0, x1 + 1):
                start = base[y * DIM + x] // 2
                height = LAYERS - cols[start]
                col = cols[start: start + 1 + height]
                new = [col[0]] + [as_field(i) for i in col[1:]]
                cells += 1
                if new != col:
                    base[y * DIM + x] = len(cols) * 2
                    cols.extend(new)
                    changed += 1
    for spec in args.lower:
        x, y = (int(v) for v in spec.split(","))
        start = base[y * DIM + x] // 2
        height = LAYERS - cols[start]
        col = cols[start: start + 1 + height]
        if height < 3:
            raise SystemExit(f"({x},{y}) is only {height} layers high; nothing to lower")
        # Top-down: [type marker, the block carrying the lid, ...]. The marker
        # moves down a layer and the lid block becomes an air block of the same
        # lid, so the walkable surface sits where the water's does.
        lid = blocks[col[2]][7]
        air = struct.pack("<HB", 0, 0) + bytes([0, 0, 0, 0, lid])
        if air not in index:
            index[air] = len(blocks)
            blocks.append(air)
        base[y * DIM + x] = len(cols) * 2
        cols.extend([col[0] + 1, col[1], index[air]])
        changed += 1
        cells += 1
    if len(blocks) > 0xFFFF:
        raise SystemExit("too many block records")

    out = bytearray(raw[:8])
    out += struct.pack("<5I", route_size, object_size, len(cols) * 2, len(blocks) * 8, nav_size)
    out += struct.pack(f"<{DIM * DIM}I", *base)
    out += struct.pack(f"<{len(cols)}H", *cols)
    out += b"".join(blocks)
    out += raw[rest_at:]
    dst = args.out / "GTADATA"
    dst.mkdir(parents=True, exist_ok=True)
    (dst / src.name).write_bytes(bytes(out))
    print(f"cmp_patch: {src.name}: {changed} of {cells} columns changed, "
          f"{len(blocks) - block_size // 8} new block records, in {dst / src.name}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
