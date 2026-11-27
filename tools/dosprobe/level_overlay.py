#!/usr/bin/env python3
"""Stage a real level in the level-1 slot, for probing it under DOSBox-X.

Copies one section of the owner's own GTADATA/MISSION.INI (its map, cars,
peds and script, unchanged) into section [1], so a new game starts straight
into it, then applies small, named edits:

* `--player x,y,z` moves the player's start;
* `--decl 'ID=LINE'` replaces the declaration labelled ID with LINE;
* `--cmd 'LABEL=LINE'` replaces the command labelled LABEL with LINE;
* `--donowt 603-610,700` turns commands into DONOWT (keeping their labels,
  so jumps still land);
* `--add-decl LINE` / `--add-cmd LINE` append a declaration / command.

Declarations and commands are told apart by position (the first `-1` ends
the declarations, the second the commands), since a number can label one of
each. Nothing of the game's is committed: the overlay is written under the
given directory.

    level_overlay.py --section 202 --player 248,132,4 --out build/overlays/vc_tank
    level_overlay.py --section 202 --player 248,132,4 --donowt 603-610 \\
        --out build/overlays/vc_tank_calm

Grand Fair Use Auto, MIT.
"""

import argparse
import pathlib
import re
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from run import resolve_data_dir  # noqa: E402


def find_ini(data: pathlib.Path) -> pathlib.Path:
    gtadata = next((p for p in data.iterdir() if p.name.upper() == "GTADATA"), None)
    if gtadata is None:
        raise SystemExit(f"no GTADATA in {data}")
    ini = next((p for p in gtadata.iterdir() if p.name.upper() == "MISSION.INI"), None)
    if ini is None:
        raise SystemExit(f"no MISSION.INI in {gtadata}")
    return ini


def label_ranges(spec: str) -> set[int]:
    out: set[int] = set()
    for part in spec.split(","):
        if "-" in part:
            a, b = part.split("-")
            out.update(range(int(a), int(b) + 1))
        elif part:
            out.add(int(part))
    return out


def label_of(line: str) -> int | None:
    m = re.match(r"^\s*(\d+)\s", line)
    return int(m.group(1)) if m else None


def keyed(specs: list[str], what: str) -> dict[int, str]:
    out = {}
    for spec in specs:
        key, sep, line = spec.partition("=")
        if not sep or not key.strip().isdigit():
            raise SystemExit(f"--{what} wants 'LABEL=LINE', got {spec!r}")
        out[int(key)] = line.strip()
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--data", help="Game folder (default: GFA_DATA or gfa.local.toml)")
    ap.add_argument("--section", required=True, help="Section to stage, e.g. 202")
    ap.add_argument("--player", help="New player start block x,y,z")
    ap.add_argument("--decl", action="append", default=[], help="'ID=LINE': replace a declaration")
    ap.add_argument("--cmd", action="append", default=[], help="'LABEL=LINE': replace a command")
    ap.add_argument("--donowt", default="", help="Command labels to blank, e.g. 603-610,700")
    ap.add_argument("--add-decl", action="append", default=[], help="Append a declaration line")
    ap.add_argument("--add-cmd", action="append", default=[], help="Append a command line")
    ap.add_argument("--out", required=True, type=pathlib.Path)
    args = ap.parse_args()

    ini = find_ini(resolve_data_dir(args.data))
    lines = ini.read_bytes().decode("latin-1").split("\n")
    eol = "\r" if lines and lines[0].endswith("\r") else ""
    heads = [i for i, l in enumerate(lines) if re.match(r"^\[\d+\]\s*$", l.strip())]

    def span(name: str) -> tuple[int, int]:
        for k, i in enumerate(heads):
            if lines[i].strip() == f"[{name}]":
                return i, heads[k + 1] if k + 1 < len(heads) else len(lines)
        raise SystemExit(f"no section [{name}] in {ini}")

    s0, s1 = span(args.section)
    sec = lines[s0:s1]
    sec[0] = sec[0].replace(f"[{args.section}]", "[1]")
    ends = [i for i, l in enumerate(sec) if l.strip() == "-1"]
    if len(ends) < 2:
        raise SystemExit(f"[{args.section}] has no declaration/command terminators")
    decl_end, cmd_end = ends[0], ends[1]

    if args.player:
        x, y, z = (int(v) for v in args.player.split(","))
        hits = 0
        for i in range(decl_end):
            m = re.match(r"^(\s*\d+\s+\d+\s+)\([^)]*\)(\s+PLAYER\b.*)$", sec[i])
            if m:
                sec[i] = f"{m.group(1)}({x},{y},{z}){m.group(2)}"
                hits += 1
        if hits != 1:
            raise SystemExit(f"expected one PLAYER line in [{args.section}], found {hits}")

    decls = keyed(args.decl, "decl")
    cmds = keyed(args.cmd, "cmd")
    for label in label_ranges(args.donowt):
        cmds.setdefault(label, f"{label} DONOWT 0 0 0 0 0")
    done_d, done_c = set(), set()
    for i in range(decl_end):
        k = label_of(sec[i])
        if k in decls:
            sec[i] = decls[k] + eol
            done_d.add(k)
    for i in range(decl_end + 1, cmd_end):
        k = label_of(sec[i])
        if k in cmds:
            sec[i] = cmds[k] + eol
            done_c.add(k)
    if set(decls) - done_d:
        raise SystemExit(f"declarations not found in [{args.section}]: {sorted(set(decls) - done_d)}")
    if set(cmds) - done_c:
        raise SystemExit(f"commands not found in [{args.section}]: {sorted(set(cmds) - done_c)}")

    taken_d = {label_of(l) for l in sec[:decl_end]} - {None}
    taken_c = {label_of(l) for l in sec[decl_end + 1 : cmd_end]} - {None}
    for line in args.add_decl:
        if label_of(line) in taken_d:
            raise SystemExit(f"declaration label already used: {line!r}")
    for line in args.add_cmd:
        if label_of(line) in taken_c:
            raise SystemExit(f"command label already used: {line!r}")
    sec = (
        sec[:decl_end]
        + [l + eol for l in args.add_decl]
        + sec[decl_end:cmd_end]
        + [l + eol for l in args.add_cmd]
        + sec[cmd_end:]
    )

    a0, a1 = span("1")
    out = lines[:a0] + sec + lines[a1:]
    dst = args.out / "GTADATA"
    dst.mkdir(parents=True, exist_ok=True)
    (dst / "MISSION.INI").write_bytes("\n".join(out).encode("latin-1"))
    edits = len(done_d) + len(done_c) + len(args.add_decl) + len(args.add_cmd)
    print(
        f"level_overlay: [{args.section}] staged as [1] in {dst / 'MISSION.INI'}"
        f"{' (player ' + args.player + ')' if args.player else ''} ({edits} edits)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
