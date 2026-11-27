#!/usr/bin/env python3
"""Sample and compare ambient pedestrians in the Director's Cut DOS build."""

import argparse
from collections import Counter
from dataclasses import dataclass
from itertools import combinations
from math import hypot
from pathlib import Path
import struct

from run import resolve_data_dir


TEMPLATE = Path(__file__).with_name("scripts") / "ped_anti_pileup.probe"
# GTA8.EXE's actor array has 232-byte records. The first 15 fit on one
# guest physical page in this DOSBox-X build and boot configuration.
ACTOR_OFFSET = 0x1C4
ACTOR_SIZE = 0xE8
SLOTS = 15
UNITS_PER_PIXEL = 1 << 17
UNITS_PER_BLOCK = 1 << 22
PAVEMENT = 3
# DOS heading 0 is south. A positive quarter turn from south faces east.
CARDINAL = ((0, 1), (1, 0), (0, -1), (-1, 0))


@dataclass(frozen=True)
class Ped:
    x: int
    y: int
    z: int
    target: tuple[int, int]
    heading: int
    speed: int
    gait: int
    timer: int
    target_kind: int

    @property
    def position(self) -> tuple[float, float]:
        return self.x / UNITS_PER_BLOCK, self.y / UNITS_PER_BLOCK


def ambient_civilians(raw: bytes) -> dict[int, Ped]:
    peds = {}
    for slot in range(SLOTS):
        offset = ACTOR_OFFSET + slot * ACTOR_SIZE
        actor = raw[offset:offset + ACTOR_SIZE]
        if len(actor) != ACTOR_SIZE or struct.unpack_from("<h", actor)[0] != slot:
            raise ValueError("unexpected actor page layout for this DOSBox-X run")
        active, speed = struct.unpack_from("<hh", actor, 4)
        animation, = struct.unpack_from("<h", actor, 0x17)
        kind, mode = struct.unpack_from("<ii", actor, 0x62)
        on_foot = 0 < animation < 0x11 or animation == 0x88
        if active != 1 or not on_foot or (kind, mode) != (0x19, 2):
            continue
        x, y, z = struct.unpack_from("<iii", actor, 0x83)
        target = struct.unpack_from("<ii", actor, 0x33)
        heading, = struct.unpack_from("<h", actor, 0x96)
        gait, = struct.unpack_from("<i", actor, 0x72)
        timer, = struct.unpack_from("<h", actor, 0x7A)
        target_kind, = struct.unpack_from("<h", actor, 0x3D)
        peds[slot] = Ped(x, y, z, target, heading, speed, gait, timer, target_kind)
    return peds


class CityMap:
    def __init__(self, data: Path):
        gtadata = next(path for path in data.iterdir() if path.name.upper() == "GTADATA")
        cmp_file = next(path for path in gtadata.iterdir() if path.name.upper() == "NYC.CMP")
        self.raw = cmp_file.read_bytes()
        version, = struct.unpack_from("<I", self.raw)
        if version != 331:
            raise ValueError(f"unexpected NYC.CMP version {version}")
        column_size, = struct.unpack_from("<I", self.raw, 16)
        self.columns = 28 + 256 * 256 * 4
        self.blocks = self.columns + column_size

    def pavement_at(self, ped: Ped, dx: int = 0, dy: int = 0) -> bool:
        x = (ped.x + dx * UNITS_PER_PIXEL) // UNITS_PER_BLOCK
        y = (ped.y + dy * UNITS_PER_PIXEL) // UNITS_PER_BLOCK
        # DOS z counts down. The CMP ground marker is above the walking lid.
        layer = 5 - ped.z // UNITS_PER_BLOCK
        if not (0 <= x < 256 and 0 <= y < 256 and 0 <= layer < 6):
            return False
        column, = struct.unpack_from("<I", self.raw, 28 + 4 * (y * 256 + x))
        offset = self.columns + column
        empty, = struct.unpack_from("<H", self.raw, offset)
        height = 6 - empty
        if layer >= height:
            return False
        block, = struct.unpack_from("<H", self.raw, offset + 2 * (height - layer))
        flags, = struct.unpack_from("<H", self.raw, self.blocks + 8 * block)
        return (flags >> 4) & 7 == PAVEMENT


def prepare(path: Path, samples: int) -> None:
    lines = TEMPLATE.read_text().splitlines()
    start, command, _, address, size = lines[-2].split()
    first_tick = int(start)
    lines = lines[:-2]
    for index in range(samples):
        tick = first_tick + 43 * index
        lines.append(f"{tick} {command} ped_{index:04d} {address} {size}")
    lines.append(f"{first_tick + 43 * (samples - 1) + 200} quit")
    path.write_text("\n".join(lines) + "\n")


def measure(path: Path, city: CityMap) -> None:
    samples = sorted(path.glob("ped_*.bin"), key=lambda entry: int(entry.stem[4:]))
    if not samples:
        raise ValueError(f"no ped_*.bin samples in {path}")

    previous = {}
    tracks = set()
    corner_offsets = Counter()
    tee_choices = Counter()
    rests = near_edge_rests = contacts = 0
    closest = None
    for sample in samples:
        peds = ambient_civilians(sample.read_bytes())
        pavement = {slot: ped for slot, ped in peds.items() if city.pavement_at(ped)}
        tracks.update(pavement)
        for slot, ped in pavement.items():
            before = previous.get(slot)
            # Skip slot reuse and spawned peds repositioned between samples.
            if before is None or hypot(ped.x - before.x, ped.y - before.y) > 4 * UNITS_PER_PIXEL:
                continue
            if ped.gait == 8 and ped.speed == 0 and ped.timer > 0 and before.gait != 8:
                rests += 1
                fx, fy = CARDINAL[((before.heading + 128) // 256) % 4]
                if not city.pavement_at(before, fx * 24, fy * 24):
                    near_edge_rests += 1
            if ped.target_kind != 0x3C or not ped.target[0] or ped.target == before.target:
                continue
            # The ped may already have re-aimed at the new target in this sample.
            fx, fy = CARDINAL[((before.heading + 128) // 256) % 4]
            tx, ty = ped.target[0] - before.x, ped.target[1] - before.y
            lateral = (tx * fy - ty * fx) / UNITS_PER_PIXEL
            forward = (tx * fx + ty * fy) / UNITS_PER_PIXEL
            if abs(abs(lateral) - 32) > 1:
                continue
            corner_offsets[round(forward, 2)] += 1
            if city.pavement_at(before, fy * 32, -fx * 32) and city.pavement_at(before, -fy * 32, fx * 32):
                tee_choices["left" if lateral > 0 else "right"] += 1
            x, y = before.position
            print(f"{sample.stem} slot {slot}, ({x:.3f}, {y:.3f}): "
                  f"target {lateral:+.2f} px lateral, {forward:+.2f} px forward")
        for (a_slot, a), (b_slot, b) in combinations(pavement.items(), 2):
            if a.z == b.z:
                distance = hypot(a.x - b.x, a.y - b.y) / UNITS_PER_PIXEL
                if distance < 4:
                    contacts += 1
                    if closest is None or distance < closest[0]:
                        closest = distance, sample.stem, a_slot, b_slot
        previous = peds

    print(f"{len(samples)} samples, ambient slots seen on pavement: {sorted(tracks)}")
    print(f"Corner forward offsets (px): {dict(sorted(corner_offsets.items()))}")
    print(f"Corner choices with pavement on both sides: {dict(tee_choices)}")
    print(f"Rests started on pavement: {rests}, with non-pavement 24 px ahead: {near_edge_rests}")
    print(f"Same-height pavement pair samples within 4 px: {contacts}")
    if closest is not None:
        distance, sample_name, a_slot, b_slot = closest
        print(f"Closest sampled pair: {distance:.2f} px at {sample_name}, slots {a_slot} and {b_slot}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    generator = commands.add_parser("prepare", help="generate a dense probe script")
    generator.add_argument("path", type=Path)
    generator.add_argument("--samples", type=int, default=1500)
    analyzer = commands.add_parser("measure", help="analyze a capture on the NYC map")
    analyzer.add_argument("path", type=Path)
    analyzer.add_argument("--data", help="game folder (default: GFA_DATA or gfa.local.toml)")
    args = parser.parse_args()
    try:
        if args.command == "prepare":
            if args.samples < 1:
                parser.error("--samples must be positive")
            prepare(args.path, args.samples)
        else:
            measure(args.path, CityMap(resolve_data_dir(args.data)))
    except (OSError, ValueError, struct.error) as error:
        parser.exit(1, f"{error}\n")


if __name__ == "__main__":
    main()
