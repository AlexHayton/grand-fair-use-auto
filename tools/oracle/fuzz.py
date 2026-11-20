#!/usr/bin/env python3
"""Random walks or drives through the AmiGTA1 oracle and gfa-tool, diffed
tick by tick.

    tools/oracle/fuzz.py [--drive] [--seed N] [--count N] [--legs N]

For each of GTA's three cities, writes `count` random scripts and runs
tools/oracle/compare.sh on each. A walk is a start block and legs of random
turns, steps and runs; a drive (--drive) puts the player in a random vehicle
on a random block and holds random throttle, reverse, steering and handbrake.
The scripts are generated from the seed, so a failure can be reproduced;
failing scripts are kept in tools/oracle/build/fuzz/.
"""
import argparse
import os
import random
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
CITIES = [("nyc.cmp", 1), ("sanb.cmp", 102), ("miami.cmp", 202)]


def script(rng, section, legs):
    lines = [f"# section: {section}", f"start {rng.randrange(256)} {rng.randrange(256)}"]
    for _ in range(legs):
        turn = rng.choice([-1, 0, 0, 1])
        fwd = rng.choice([-1, 0, 1, 1, 1])
        walk = rng.choice([0, 0, 0, 1])
        lines.append(f"{turn} {fwd} {walk} {rng.randrange(1, 120)}")
    return "\n".join(lines) + "\n"


def drive_script(rng, section, legs):
    # Records 0..20 exist in every GTA style: cars, bikes, buses and odd ones.
    lines = [f"# section: {section}",
             f"drive {rng.randrange(21)} {rng.randrange(256)} {rng.randrange(256)} {rng.randrange(256)}"]
    for _ in range(legs):
        up = rng.choice([0, 1, 1, 1])
        down = 0 if up else rng.choice([0, 1])
        lines.append(f"run {rng.randrange(1, 90)} {up} {down} {rng.choice([-1, 0, 0, 1])} {rng.choice([0, 0, 0, 1])}")
    return "\n".join(lines) + "\n"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--drive", action="store_true", help="random drives instead of walks")
    ap.add_argument("--seed", type=int, default=1)
    ap.add_argument("--count", type=int, default=10)
    ap.add_argument("--legs", type=int, default=30)
    args = ap.parse_args()
    rng = random.Random(args.seed)
    out = os.path.join(HERE, "build", "fuzz")
    os.makedirs(out, exist_ok=True)
    data = os.environ.get("GFA_DATA") or ""
    bad = 0
    for mapname, section in CITIES:
        for i in range(args.count):
            path = os.path.join(out, f"{section}_{i}.txt")
            with open(path, "w") as f:
                make = drive_script if args.drive else script
                f.write(make(rng, section, args.legs))
            cmd = [os.path.join(HERE, "compare.sh"), path]
            if data:
                cmd.append(os.path.join(data, "GTADATA", mapname))
            else:
                cmd.append(f"@{mapname}")
            r = subprocess.run(cmd, capture_output=True, text=True)
            status = r.stdout.strip().splitlines()[-1] if r.stdout.strip() else r.stderr.strip()
            print(f"{mapname} #{i}: {status}")
            if r.returncode != 0:
                bad += 1
                print(r.stdout)
            else:
                os.remove(path)
    print(f"fuzz: {bad} mismatches")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
