#!/usr/bin/env python3
"""Measure a driven vehicle's motion from a tools/dosprobe burst capture.

Built for the synthetic `tank_drive` / `tank_turn` arenas (`gfa-tool
probe-overlay`): every cell is the same road block, whose lid shows one
bright yellow lane line per block, so

* the yellow lines' spacing is the screen scale (px per block; 108 at rest,
  less as the camera zooms out with speed),
* the lines' phase gives the camera's world x (block units, unwrapped from
  frame to frame), and
* the road texture repeats every block, so a pixel that differs from the
  pixels one block above *and* below it (a block is 5/6 as tall as it is
  wide on screen) belongs to the vehicle (or a ped).
  The mask's centroid gives the vehicle's screen position and its principal
  axis the hull's heading (mod 180 degrees).

The camera is taken to look at screen x = 320. World x of the vehicle =
camera x + (centroid x - 320) / spacing. There are no horizontal lines, so
world y is not measured; the heading is.

    analyze_drive.py OUT_DIR --prefix accel --prefix coast --csv out.csv

Grand Fair Use Auto, MIT.
"""

import argparse
import math
import pathlib
import re
import sys

import numpy as np
from PIL import Image
from scipy import ndimage

BURST_RE = re.compile(r"^probe: burst (\S+) (\d+)x(\d+) bpp=(\d+) tick=(\d+)$")
CENTRE_X = 320.0
GAME_FRAME_MS = 3000.0 / 70.081  # 42.81 ms: 3 VGA frames


def frames_for(out_dir: pathlib.Path, prefix: str) -> list[tuple[int, pathlib.Path]]:
    found = []
    for line in (out_dir / "probe.log").read_text(errors="replace").splitlines():
        m = BURST_RE.match(line.strip())
        if m and m.group(1).startswith(prefix + "_"):
            ppm = out_dir / m.group(1)
            png = ppm.with_suffix(".png")
            found.append((int(m.group(5)), ppm if ppm.is_file() else png))
    if not found:
        raise SystemExit(f"no burst frames '{prefix}_*' in {out_dir / 'probe.log'}")
    return found


def yellow_lines(a: np.ndarray) -> tuple[float, float] | None:
    """(spacing px, x of the line nearest the centre) from the yellow cores."""
    r, g, b = a[..., 0], a[..., 1], a[..., 2]
    yellow = (r > 120) & (g > 120) & (b < 25)
    yellow[:30, :] = False  # HUD
    cols = yellow.sum(axis=0).astype(float)
    on = cols > 0.25 * cols.max() if cols.max() > 0 else cols > 0
    centres = []
    x = 0
    while x < len(on):
        if on[x]:
            x0 = x
            while x < len(on) and on[x]:
                x += 1
            w = cols[x0:x]
            centres.append(float((np.arange(x0, x) * w).sum() / w.sum()))
        else:
            x += 1
    if len(centres) < 3:
        return None
    c = np.array(centres)
    d = np.diff(c)
    sp0 = float(np.median(d))
    # Number the lines from the first, then fit x_k = x0 + k * sp.
    k = np.round((c - c[0]) / sp0)
    sp, x0 = np.polyfit(k, c, 1)
    near = x0 + sp * round((CENTRE_X - x0) / sp)
    return float(sp), float(near)


# A block is 54.33 x 45.33 native VGA px (the 6:5 pixel aspect), so a block's
# height on screen is its width times this.
BLOCK_ASPECT = 45.333 / 54.333


def vehicle_mask(a: np.ndarray, sp: float) -> np.ndarray:
    g = a.astype(int)
    h = g.shape[0]
    s = int(round(sp * BLOCK_ASPECT))
    mask = np.zeros(g.shape[:2], bool)
    if s < 10 or 2 * s >= h:
        return mask
    mid = g[s : h - s]
    up = np.abs(mid - g[: h - 2 * s]).sum(axis=2) > 40
    dn = np.abs(mid - g[2 * s :]).sum(axis=2) > 40
    mask[s : h - s] = up & dn
    mask[:30, :] = False
    # Zoomed out, the rescaled texture is only nearly periodic and leaves a
    # sparse speckle; the vehicle is solid. Keep dense pixels, then the
    # largest blob.
    dense = ndimage.uniform_filter(mask.astype(float), size=9) > 0.55
    labels, n = ndimage.label(dense)
    if n == 0:
        return dense
    sizes = ndimage.sum(dense, labels, range(1, n + 1))
    return labels == (1 + int(np.argmax(sizes)))


def centroid_heading(mask: np.ndarray) -> tuple[float, float, float, int] | None:
    ys, xs = np.nonzero(mask)
    if len(xs) < 50:
        return None
    cx, cy = xs.mean(), ys.mean()
    # Undo the 6:5 aspect so the angle is the world heading.
    cov = np.cov(np.vstack([xs - cx, (ys - cy) / BLOCK_ASPECT]))
    w, v = np.linalg.eigh(cov)
    ax = v[:, int(np.argmax(w))]
    # Screen angle of the long axis, 0 = east, clockwise positive (y down).
    ang = math.degrees(math.atan2(ax[1], ax[0])) % 180.0
    return float(cx), float(cy), ang, len(xs)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("out_dir", type=pathlib.Path)
    ap.add_argument("--prefix", action="append", required=True)
    ap.add_argument("--csv", type=pathlib.Path)
    args = ap.parse_args()

    rows = []
    cam = None
    prev_near = prev_sp = None
    hdg_prev = None
    hdg_unwrapped = 0.0
    for prefix in args.prefix:
        for tick, path in frames_for(args.out_dir, prefix):
            a = np.asarray(Image.open(path).convert("RGB"))
            lines = yellow_lines(a)
            if lines is None:
                print(f"{path.name}: no lane lines", file=sys.stderr)
                continue
            sp, near = lines
            # World x (blocks) of the screen centre, up to a whole block:
            # the line nearest the centre sits on a block boundary.
            frac = -(near - CENTRE_X) / sp
            if cam is None:
                cam = frac
            else:
                step = (frac - cam) % 1.0
                if step > 0.5:
                    step -= 1.0
                cam += step
            m = vehicle_mask(a, sp)
            ch = centroid_heading(m)
            if ch is None:
                continue
            cx, cy, ang, n = ch
            if hdg_prev is None:
                hdg_unwrapped = ang
            else:
                d = (ang - hdg_prev + 90.0) % 180.0 - 90.0
                hdg_unwrapped += d
            hdg_prev = ang
            veh_x = cam + (cx - CENTRE_X) / sp
            rows.append((prefix, path.stem, tick, sp, cam, cx, cy, veh_x, hdg_unwrapped, n))

    if not rows:
        raise SystemExit("nothing measured")
    out = sys.stdout
    header = "prefix,frame,tick_ms,px_per_block,camera_x_blocks,veh_screen_x,veh_screen_y,veh_x_blocks,heading_deg,mask_px,speed_blocks_s"
    lines_out = [header]
    for i, r in enumerate(rows):
        if 0 < i < len(rows) - 1:
            dt = (rows[i + 1][2] - rows[i - 1][2]) / 1000.0
            v = (rows[i + 1][7] - rows[i - 1][7]) / dt if dt > 0 else float("nan")
        else:
            v = float("nan")
        lines_out.append(
            f"{r[0]},{r[1]},{r[2]},{r[3]:.2f},{r[4]:.4f},{r[5]:.1f},{r[6]:.1f},{r[7]:.4f},{r[8]:.1f},{r[9]},{v:.3f}"
        )
    text = "\n".join(lines_out) + "\n"
    if args.csv:
        args.csv.write_text(text)
    else:
        out.write(text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
