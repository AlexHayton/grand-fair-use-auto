#!/usr/bin/env python3
"""Stage a repeatable SPRAY or BOMBSHOP test on the synthetic arena."""

import argparse
from pathlib import Path
import shutil


def main():
    here = Path(__file__).resolve().parent
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("shop", choices=("SPRAY", "BOMBSHOP"))
    parser.add_argument("out", type=Path)
    parser.add_argument("--arena", type=Path, default=here / "build/overlays/arena")
    parser.add_argument("--model", type=int, default=31, help="Parked car model")
    parser.add_argument("--color", type=int, default=2, help="Requested spray colour")
    parser.add_argument("--score", type=int, default=0)
    parser.add_argument("--wanted", action="store_true")
    parser.add_argument("--rc", action="store_true", help="Switch to model 47 after boarding")
    parser.add_argument("--bomb-cost", type=int, help="Override BOMBSHOP_COST")
    parser.add_argument("--shop-pos", default="130,128,4", help="Declaration x,y,z")
    args = parser.parse_args()

    declarations = [
        f"1 1 (127,128,4) PARKED {args.model} 256",
        "2 1 (127,127,4) PLAYER 1 512",
        f"3 1 ({args.shop_pos}) {args.shop} {args.color if args.shop == 'SPRAY' else 0} 0",
    ]
    if args.bomb_cost is not None:
        declarations.append(f"4 1 (0,0,0) BOMBSHOP_COST {args.bomb_cost} 0")
    commands = ["0 DONOWT 0 0 0 0 0"]
    if args.score or args.wanted or args.rc:
        declarations.append("9 (127,127,4) TRIGGER 100 0")
        commands.extend([
            "100 DISABLE 9 101 0 0 0",
            f"101 ARROW_OFF 0 102 0 0 {args.score}",
        ])
        if args.wanted:
            # At 23.333 Hz, 170 frames puts BANK_ROBBERY after boarding.
            commands.extend(["102 SURVIVE 0 103 0 170 0", "103 BANK_ROBBERY 1 104 0 0 0"])
        else:
            commands.append("102 ARROW_OFF 0 104 0 0 0")
        if args.rc:
            commands.extend(["104 START_MODEL 0 105 0 -1 5", "105 DO_MODEL 104 0 0 -1 0"])
        else:
            commands.append("104 DONOWT 0 0 0 0 0")

    lines = [
        "[1]", "", "Probe Shops, 22, test.cmp, 0,", "100 1 1 1 1 0", "",
        *declarations,
        "10 1 (0,0,0) MISSION_COUNTER 4 0",
        "11 1 (0,0,0) SECRET_MISSION_COUNTER 7 0",
        "12 1 (0,0,0) MISSION_TOTAL 11 0",
        "13 1 (0,0,0) TARGET_SCORE 1000000 0",
        "-1", *commands, "-1", "\x1a",
    ]
    source = args.arena / "GTADATA/TEST.CMP"
    dest = args.out / "GTADATA"
    dest.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, dest / "TEST.CMP")
    (dest / "MISSION.INI").write_bytes("\r\n".join(lines).encode("ascii"))


if __name__ == "__main__":
    main()
