#!/usr/bin/env python3
"""Analyze a tools/dosprobe `burst` capture from probe.log + PPM frames."""

import argparse
import collections
import pathlib
import re
import sys

import numpy as np
from PIL import Image


BURST_RE = re.compile(r"^probe: burst (\S+) (\d+)x(\d+) bpp=(\d+) tick=(\d+)$")


def load_burst(out_dir: pathlib.Path, prefix: str) -> list[tuple[int, pathlib.Path]]:
    log_path = out_dir / "probe.log"
    if not log_path.is_file():
        raise SystemExit(f"missing {log_path}")
    want_prefix = f"{prefix}_"
    frames: list[tuple[int, pathlib.Path]] = []
    for line in log_path.read_text(encoding="utf-8", errors="replace").splitlines():
        m = BURST_RE.match(line.strip())
        if not m:
            continue
        fname, _w, _h, _bpp, tick_s = m.groups()
        if fname.startswith(want_prefix):
            p = out_dir / fname
            if not p.is_file():
                raise SystemExit(f"missing frame {p}")
            frames.append((int(tick_s), p))
    if not frames:
        raise SystemExit(f"no burst frames matching '{want_prefix}*' in {log_path}")
    return frames


def measure_dx(prev_arr: np.ndarray, cur_arr: np.ndarray, y0: int, y1: int, max_dx: int = 80) -> tuple[int, float]:
    a = prev_arr[y0:y1, :, :]
    b = cur_arr[y0:y1, :, :]
    w = a.shape[1]
    best_dx, best_err = 0, 1e18
    for dx in range(-max_dx, max_dx + 1):
        x0a = max(0, dx)
        x1a = min(w, w + dx)
        x0b = max(0, -dx)
        x1b = min(w, w - dx)
        err = float(np.abs(a[:, x0a:x1a, :] - b[:, x0b:x1b, :]).mean())
        if err < best_err:
            best_err, best_dx = err, dx
    return best_dx, best_err


def main() -> int:
    ap = argparse.ArgumentParser(description="Analyze a dosprobe burst")
    ap.add_argument("out_dir", type=pathlib.Path, help="Probe output directory")
    ap.add_argument("--prefix", default="walk", help="Burst filename prefix (default: walk)")
    ap.add_argument("--shift", action="store_true", help="Estimate horizontal background shift per unique frame")
    ap.add_argument("--roi-y", default="260:380", help="Y slice y0:y1 for --shift (default: 260:380)")
    args = ap.parse_args()

    frames = load_burst(args.out_dir, args.prefix)
    ticks = [t for t, _ in frames]
    vga_dts = [b - a for a, b in zip(ticks, ticks[1:])]
    vga_span = ticks[-1] - ticks[0]
    vga_hz = ((len(ticks) - 1) * 1000.0 / vga_span) if vga_span > 0 else 0.0

    arrays = [np.array(Image.open(p), dtype=np.int16) for _, p in frames]
    unique_idx = [0]
    for i in range(1, len(arrays)):
        if not np.array_equal(arrays[i], arrays[i - 1]):
            unique_idx.append(i)

    u_ticks = [ticks[i] for i in unique_idx]
    game_dts = [b - a for a, b in zip(u_ticks, u_ticks[1:])]
    game_span = (u_ticks[-1] - u_ticks[0]) if len(u_ticks) >= 2 else 0
    game_fps = ((len(u_ticks) - 1) * 1000.0 / game_span) if game_span > 0 else 0.0

    print(f"{args.out_dir} [{args.prefix}]:")
    print(f"  VGA frames : {len(frames)} ticks {ticks[0]}..{ticks[-1]} (span={vga_span} ms, {vga_hz:.3f} Hz)")
    print(f"  VGA dt_ms  : {dict(sorted(collections.Counter(vga_dts).items()))}")
    print(f"  Game frames: {len(unique_idx)} unique (first={u_ticks[0]} last={u_ticks[-1]}, {game_fps:.3f} FPS)")
    print(f"  Game dt_ms : {dict(sorted(collections.Counter(game_dts).items()))}")
    vga_gaps = [b - a for a, b in zip(unique_idx, unique_idx[1:])]
    print(f"  VGA step   : {dict(sorted(collections.Counter(vga_gaps).items()))}")

    if args.shift and len(unique_idx) >= 2:
        y0_s, y1_s = args.roi_y.split(":")
        y0, y1 = int(y0_s), int(y1_s)
        dxs = []
        for idx_a, idx_b in zip(unique_idx, unique_idx[1:]):
            dx, err = measure_dx(arrays[idx_a], arrays[idx_b], y0, y1)
            dxs.append((frames[idx_b][1].name, ticks[idx_b] - ticks[idx_a], dx, round(err, 2)))
        total_dx = sum(item[2] for item in dxs)
        print(f"  Shift sum  : {total_dx} px over {len(dxs)} unique intervals")
        print(f"  Per frame  : {dxs[:15]}{' ...' if len(dxs) > 15 else ''}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
