#!/usr/bin/env python3
"""Run DOSBox-X headlessly (with a .probe script) or interactively against GTADOS."""

import argparse
import os
import pathlib
import re
import shutil
import subprocess
import sys


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
DOSPROBE_DIR = pathlib.Path(__file__).resolve().parent
DEFAULT_BIN = DOSPROBE_DIR / "build" / ("dosbox-x.exe" if os.name == "nt" else "dosbox-x")
DEFAULT_STATES = DOSPROBE_DIR / "states"
PROBE_EPOCH = 883656000  # 1998-01-01 12:00:00 UTC (matches gfa_probe_init_time)


def stage_overlays(overlay_srcs: list[pathlib.Path], overlay_dst: pathlib.Path) -> int:
    staged = 0
    for src_raw in overlay_srcs:
        src_root = src_raw.expanduser().resolve()
        if not src_root.is_dir():
            raise SystemExit(f"missing overlay directory: {src_raw}")
        for dirpath, dirnames, filenames in os.walk(src_root):
            dirnames.sort()
            filenames.sort()
            rel_dir = pathlib.Path(dirpath).relative_to(src_root)
            dst_dir = overlay_dst / rel_dir
            dst_dir.mkdir(parents=True, exist_ok=True)
            for name in filenames:
                src_file = pathlib.Path(dirpath) / name
                dst_file = dst_dir / name
                shutil.copyfile(src_file, dst_file)
                os.utime(dst_file, (PROBE_EPOCH, PROBE_EPOCH))
                staged += 1
    return staged


def resolve_data_dir(explicit: str | None) -> pathlib.Path:
    candidates: list[pathlib.Path] = []
    if explicit:
        candidates.append(pathlib.Path(explicit))
    elif env := os.environ.get("GFA_DATA"):
        candidates.append(pathlib.Path(env))
    else:
        cur = pathlib.Path.cwd().resolve()
        for parent in [cur, *cur.parents]:
            cfg = parent / "gfa.local.toml"
            if cfg.is_file():
                text = cfg.read_text(encoding="utf-8")
                m = re.search(r'(?m)^\s*data\s*=\s*"([^"]+)"', text)
                if m:
                    candidates.append(pathlib.Path(m.group(1)))
                break
        if (REPO_ROOT / "gfa.local.toml").is_file() and not candidates:
            text = (REPO_ROOT / "gfa.local.toml").read_text(encoding="utf-8")
            m = re.search(r'(?m)^\s*data\s*=\s*"([^"]+)"', text)
            if m:
                candidates.append(pathlib.Path(m.group(1)))

    if not candidates:
        raise SystemExit("could not resolve game data folder (pass --data, set GFA_DATA, or configure gfa.local.toml)")

    p = candidates[0].expanduser().resolve()
    if p.name.upper() == "GTADATA":
        p = p.parent
    if not (p / "GTADOS").is_dir():
        raise SystemExit(f"expected GTADOS directory inside {p}")
    return p


def resolve_cdrom(data_dir: pathlib.Path, exe_name: str) -> pathlib.Path:
    exe = pathlib.Path(exe_name).name.lower()
    edition = "LONDON" if "_uk" in exe or "_61" in exe else "GTA"
    cdrom = data_dir / "MUSIC" / edition / "CDAUDIO.DAT"
    if not cdrom.is_file():
        raise SystemExit(f"missing CD image for {exe_name}: {cdrom}")
    return cdrom


def write_conf(
    conf_path: pathlib.Path,
    data_dir: pathlib.Path,
    cdrom_path: pathlib.Path,
    overlay_dir: pathlib.Path,
    captures_dir: pathlib.Path,
    exe_name: str,
    cycles: int,
    headless: bool,
    fast: bool,
    keep_shell: bool,
) -> None:
    nosound = "true" if headless else "false"
    turbo = "true" if fast else "false"
    exit_line = "" if keep_shell else "EXIT\n"
    conf = f"""[sdl]
output=surface
autolock=false
usescancodes=true
windowresolution=640x480
showmenu=false

[dosbox]
machine=svga_s3
memsize=32
synchronize time=false
fastbioslogo=true
startbanner=false
captures={captures_dir}

[cpu]
core=normal
cputype=pentium
cycles=fixed {cycles}
turbo={turbo}

[render]
frameskip=0
scaler=none

[mixer]
nosound={nosound}

[autoexec]
@ECHO OFF
MOUNT C "{data_dir}"
MOUNT C "{overlay_dir}" -t overlay
IMGMOUNT D "{cdrom_path}" -t cdrom -fs iso
C:
CD \\GTADOS
{exe_name}
{exit_line}"""
    conf_path.write_text(conf, encoding="utf-8")


def convert_ppms_to_png(out_dir: pathlib.Path) -> list[pathlib.Path]:
    converted: list[pathlib.Path] = []
    ppms = sorted(out_dir.glob("*.ppm"))
    if not ppms:
        return converted
    try:
        from PIL import Image
    except ImportError:
        ffmpeg = shutil.which("ffmpeg")
        sips = shutil.which("sips")
        for ppm in ppms:
            png = ppm.with_suffix(".png")
            if ffmpeg:
                subprocess.run([ffmpeg, "-y", "-loglevel", "error", "-i", str(ppm), str(png)], check=True)
            elif sips:
                subprocess.run([sips, "-s", "format", "png", str(ppm), "--out", str(png)], check=True, stdout=subprocess.DEVNULL)
            else:
                raise SystemExit("cannot convert .ppm to .png: install Pillow, ffmpeg, or sips")
            converted.append(png)
        return converted

    for ppm in ppms:
        png = ppm.with_suffix(".png")
        with Image.open(ppm) as im:
            im.save(png, format="PNG")
        converted.append(png)
    return converted


def main() -> int:
    ap = argparse.ArgumentParser(description="Run DOSBox-X probe harness against GTADOS")
    ap.add_argument("--data", help="Path to game folder containing GTADOS/")
    ap.add_argument(
        "--overlay",
        action="append",
        default=[],
        metavar="DIR",
        help=(
            "Directory tree (e.g. containing GTADATA/MISSION.INI and GTADATA/TEST.CMP) "
            "to stage into DOSBox-X's C:\\ -t overlay mount without mutating DIR or game data"
        ),
    )
    ap.add_argument("--exe", default="GTA8.EXE", help="Executable inside GTADOS/ (default: GTA8.EXE)")
    ap.add_argument("--script", help="Path to .probe script (enables headless mode by default)")
    ap.add_argument("--out", help="Output directory for shots, memdumps, and probe.log")
    ap.add_argument("--run-name", help="Scratch directory name under tools/dosprobe/build/run/")
    ap.add_argument("--headless", action="store_true", help="Force dummy SDL video/audio drivers")
    ap.add_argument("--windowed", action="store_true", help="Show SDL window even when --script is used")
    ap.add_argument("--interactive", action="store_true", help="Run interactively with window & sound")
    ap.add_argument("--png", action="store_true", help="Convert captured .ppm frames to .png")
    ap.add_argument("--cycles", type=int, default=60000, help="Fixed DOSBox-X CPU cycles (default: 60000)")
    ap.add_argument("--timeout", type=float, help="Wall-clock timeout in seconds (default: 120 when headless)")
    ap.add_argument("--savedir", default=str(DEFAULT_STATES), help="Directory for DOSBox-X savestate slots")
    ap.add_argument("--fast", action="store_true", help="Enable cpu turbo mode")
    ap.add_argument("--keep-shell", action="store_true", help="Keep DOS shell open after exe exits")
    args = ap.parse_args()

    if not DEFAULT_BIN.is_file():
        raise SystemExit(f"missing {DEFAULT_BIN}; run tools/dosprobe/build.sh first")

    data_dir = resolve_data_dir(args.data)
    cdrom_path = resolve_cdrom(data_dir, args.exe)
    headless = (bool(args.script) and not args.windowed and not args.interactive) or args.headless
    if args.interactive:
        headless = False

    out_dir = pathlib.Path(args.out).resolve() if args.out else (DOSPROBE_DIR / "out" / "default").resolve()
    out_dir.mkdir(parents=True, exist_ok=True)
    savedir = pathlib.Path(args.savedir).resolve()
    savedir.mkdir(parents=True, exist_ok=True)

    run_name = args.run_name or (out_dir.name if args.out else "interactive")
    run_dir = (DOSPROBE_DIR / "build" / "run" / run_name).resolve()
    overlay_dir = run_dir / "overlay"
    if run_dir.exists():
        shutil.rmtree(run_dir)
    overlay_dir.mkdir(parents=True, exist_ok=True)
    if args.overlay:
        staged = stage_overlays([pathlib.Path(p) for p in args.overlay], overlay_dir)
        print(f"run.py: staged {staged} file(s) into overlay")

    conf_path = run_dir / "dosbox-x.conf"
    write_conf(
        conf_path=conf_path,
        data_dir=data_dir,
        cdrom_path=cdrom_path,
        overlay_dir=overlay_dir,
        captures_dir=out_dir,
        exe_name=args.exe,
        cycles=args.cycles,
        headless=headless,
        fast=args.fast,
        keep_shell=args.keep_shell or (args.interactive and not args.script),
    )

    env = os.environ.copy()
    if args.script:
        script_path = pathlib.Path(args.script).resolve()
        if not script_path.is_file():
            raise SystemExit(f"missing probe script: {script_path}")
        env["GFA_PROBE_SCRIPT"] = str(script_path)
        env["GFA_PROBE_OUT"] = str(out_dir)
    else:
        env.pop("GFA_PROBE_SCRIPT", None)
        env.pop("GFA_PROBE_OUT", None)

    if headless:
        env["SDL_VIDEODRIVER"] = "dummy"
        env["SDL_AUDIODRIVER"] = "dummy"

    cmd = [
        str(DEFAULT_BIN),
        "-conf",
        str(conf_path),
        "-savedir",
        str(savedir),
    ]
    if headless:
        cmd.append("-nomenu")
    if headless or args.script:
        cmd.append("-fastlaunch")

    timeout = args.timeout if args.timeout is not None else (120.0 if headless else None)
    try:
        proc = subprocess.run(cmd, env=env, timeout=timeout, check=False)
    except subprocess.TimeoutExpired:
        print(f"run.py: timed out after {timeout}s", file=sys.stderr)
        return 124

    if args.png and out_dir.is_dir():
        pngs = convert_ppms_to_png(out_dir)
        print(f"run.py: converted {len(pngs)} ppm frame(s) to png in {out_dir}")

    return proc.returncode


if __name__ == "__main__":
    raise SystemExit(main())
