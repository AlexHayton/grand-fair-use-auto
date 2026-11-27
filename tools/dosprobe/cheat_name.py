#!/usr/bin/env python3
"""Generate a name-entry cheat probe for one DOS edition."""

import argparse
from pathlib import Path


def make_probe(edition: str, name: str) -> str:
    menu_wait = 2000 if edition == "dc" else 7000
    events = [
        "3500 tap esc 80",
        f"+{menu_wait} tap enter 80",
    ]
    if edition == "london61":
        events.append("+4500 tap enter 80")
    events.append("+2000 shot character")
    if name:
        events.append("+100 tap delete 80")
        for char in name:
            if char.isupper():
                events.append("+100 down lshift")
            key = "space" if char == " " else char.lower()
            events.append(f"+100 tap {key} 80")
            if char.isupper():
                events.append("+100 up lshift")
        events.append("+200 tap enter 80")

    events.extend([
        "+500 shot entered",
        "+1500 tap enter 80",
        "+750 shot levels",
        "+100 memdump levels 0 a00000",
        "+1200 tap enter 80",
        "+11000 shot before",
        "+100 memdump before 0 a00000",
        "+100 tap kpmultiply 80",
        "+750 shot after",
        "+100 memdump after 0 a00000",
        "+100 quit",
    ])
    return "\n".join(events) + "\n"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("edition", choices=("dc", "london69", "london61"))
    parser.add_argument("output", type=Path)
    parser.add_argument("--name", default="", help="omit to keep the default player name")
    args = parser.parse_args()
    if len(args.name) > 14 or any(
        char != " " and not (char.isascii() and char.isalnum()) for char in args.name
    ):
        parser.error("use up to 14 ASCII letters, digits or spaces")
    args.output.write_text(make_probe(args.edition, args.name), encoding="ascii")


if __name__ == "__main__":
    main()
