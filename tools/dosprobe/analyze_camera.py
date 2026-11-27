#!/usr/bin/env python3
"""Measure camera_runway.probe's DOS ground scale and longitudinal lead.

The runway repeats one yellow lane line per block. Fit the lines for scale and
scale, then locate the blue ends of the police car and compare its centre
with the boarded frame.
"""

import argparse
import csv
import re
import sys
from pathlib import Path
from statistics import mean, median


WIDTH, HEIGHT = 640, 400
FRAME = re.compile(
    r"probe: (?:burst ((?:accel|coast|reverse)_\d+\.ppm)|shot (settled))"
    r" 640x400 bpp=\d+ tick=(\d+)"
)


def measure(path: Path) -> tuple[float, float]:
    magic, dimensions, depth, rgb = path.read_bytes().split(b"\n", 3)
    if (magic, dimensions, depth, len(rgb)) != (b"P6", b"640 400", b"255", WIDTH * HEIGHT * 3):
        raise ValueError(f"{path}: expected a 640x400 RGB probe capture")

    def pixel(x: int, y: int) -> tuple[int, int, int]:
        i = (y * WIDTH + x) * 3
        return rgb[i], rgb[i + 1], rgb[i + 2]

    stripes: list[list[int]] = []
    for x in range(0, WIDTH, 2):
        r, g, b = pixel(x, 100)
        if min(r, g) > 120 and b < 25:
            if not stripes or x > stripes[-1][-1] + 2:
                stripes.append([])
            stripes[-1].append(x)
    xs = [mean(stripe) + 0.5 for stripe in stripes if stripe[0] > 0 and stripe[-1] < WIDTH - 2]
    if len(xs) < 3:
        raise ValueError(f"{path}: expected at least three yellow lane lines")
    spacing = median(b - a for a, b in zip(xs, xs[1:]))
    indices = [round((x - xs[0]) / spacing) for x in xs]
    x_mean, k_mean = mean(xs), mean(indices)
    spacing = sum((x - x_mean) * (k - k_mean) for x, k in zip(xs, indices)) / sum(
        (k - k_mean) ** 2 for k in indices
    )
    blue = []
    for y in range(46, HEIGHT - 46, 2):
        for x in range(0, WIDTH, 2):
            r, g, b = pixel(x, y)
            if r == g and b - r > 55:
                blue.append(x)
    if not blue:
        raise ValueError(f"{path}: police car not found")
    car_x = (min(blue) + max(blue)) / 2 + 0.5
    return spacing, car_x


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("out_dir", type=Path)
    args = ap.parse_args()
    frames = [
        (file or f"{shot}.ppm", int(tick))
        for file, shot, tick in FRAME.findall((args.out_dir / "probe.log").read_text())
    ]
    if not frames:
        ap.error("no camera_runway bursts found")
    rest, rest_x = measure(args.out_dir / "boarded.ppm")
    out = csv.writer(sys.stdout)
    out.writerow(("frame", "tick_ms", "px_per_block", "zoom", "lead_blocks"))
    for file, tick in frames:
        spacing, car_x = measure(args.out_dir / file)
        lead = (rest_x - car_x) / spacing
        out.writerow((file, tick, round(spacing, 3), round(rest / spacing, 4), round(lead, 4)))


if __name__ == "__main__":
    main()
