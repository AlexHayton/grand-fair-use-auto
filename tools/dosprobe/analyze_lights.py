#!/usr/bin/env python3
"""Read the two signal lenses in traffic_lights.probe's 640x400 captures."""

import argparse
import re
from pathlib import Path


FRAME = re.compile(r"^probe: burst (lights_\d+\.ppm) 640x400 bpp=\d+ tick=(\d+)$", re.M)
LENSES = {"east–west": (107, 157), "north–south": (386, 376)}
COLOURS = {b"\xff\x41\x41": "red", b"\x08\xff\x00": "green", b"\xff\x8a\x20": "amber"}


def transitions(captures: list[tuple[int, str | None]]) -> list[tuple[int, int, str]]:
    changes = []
    last_tick, last_colour = None, None
    for tick, colour in captures:
        if colour is None:  # A sprite may cover the lens.
            continue
        if last_tick is not None and colour != last_colour:
            changes.append((last_tick, tick, colour))
        last_tick, last_colour = tick, colour
    return changes


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("out_dir", type=Path)
    args = parser.parse_args()
    captures = {axis: [] for axis in LENSES}
    for name, timestamp in FRAME.findall((args.out_dir / "probe.log").read_text()):
        path = args.out_dir / name
        magic, size, depth, rgb = path.read_bytes().split(b"\n", 3)
        if (magic, size, depth, len(rgb)) != (b"P6", b"640 400", b"255", 640 * 400 * 3):
            parser.error(f"{path}: expected a 640x400 RGB capture")
        for axis, (x, y) in LENSES.items():
            offset = (y * 640 + x) * 3
            captures[axis].append((int(timestamp), COLOURS.get(rgb[offset:offset + 3])))

    for axis, frames in captures.items():
        changes = transitions(frames)
        if len(changes) < 2:
            parser.error(f"{axis}: insufficient visible signal transitions")
        print(axis)
        print("    Change window (PIC ms)   New state   Previous state (DOS game frames)")
        start = None
        for before, after, colour in changes:
            duration = ""
            if start is not None:
                start_before, start_after, previous_colour = start
                # A transition falls between its last old and first new
                # observation. Allow one VGA refresh of variation in the
                # display delay. Each DOS game frame lasts 300/7 PIC ms.
                shortest = max(1, ((before - start_after - 15) * 7 + 299) // 300)
                longest = ((after - start_before + 15) * 7) // 300
                counts = str(shortest) if shortest == longest else f"{shortest}–{longest}"
                duration = f"{previous_colour} {counts}"
            print(f"    {before:6}–{after:<6}          {colour:<10}  {duration}")
            start = before, after, colour


if __name__ == "__main__":
    main()
