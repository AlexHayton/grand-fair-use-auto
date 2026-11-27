#!/usr/bin/env python3
"""Read GTA8.EXE's code and data, and its state in a guest RAM dump.

The owner's own GTADOS/GTA8.EXE is read at run time; nothing of it is kept.
It is a DOS/4GW LE executable: object 1 is the code (loaded at 0x10000),
object 2 the data (0x130000). Addresses here are those load addresses, with
the fixups applied, as the notes in docs/M6B_PROBES.md quote them.

    gta8.py refs TEXT...          where code uses a data string
    gta8.py dis START END         disassemble (needs `pip install capstone`)
    gta8.py krishna DUMP...       the Hare Krishna counters in RAM dumps

At run time DOS/4GW moves the data object up by DATA_DELTA and pages it. A
dump from the probe's `memdump NAME 0 a00000` holds the page tables, so
`krishna` finds the page directory through a data page that never changes
(one of texts) and reads the addresses through it.

Grand Fair Use Auto, MIT.
"""

import argparse
import pathlib
import struct
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from run import resolve_data_dir  # noqa: E402

DATA_DELTA = 0x1006E000
TEXT_PAGE = 0x13F000  # a page of texts, the same in RAM as in the file

# The Krishna line (M6B_PROBES.md, special ped groups).
PEDS, PED_SIZE, PED_COUNT = 0x3971C4, 0xE8, 200
GROUPS = 0x3971C0  # groups alive
SPAWNED = 0x3BA424  # ambient peds spawned; the 100th starts a group
PLAYER = 0x3BA568  # score i32; multiplier u16 at +4
KRISHNA_TYPES = {21: "leader", 22: "follower", 42: "leaderless"}


def find_exe(data: str | None) -> pathlib.Path:
    root = resolve_data_dir(data)
    dos = next((p for p in root.iterdir() if p.name.upper() == "GTADOS"), None)
    exe = dos and next((p for p in dos.iterdir() if p.name.upper() == "GTA8.EXE"), None)
    if exe is None:
        raise SystemExit(f"no GTADOS/GTA8.EXE in {root}")
    return exe


def load(path: pathlib.Path):
    """The objects' images, fixed up: [(base, bytearray)]."""
    b = path.read_bytes()
    le = b.find(b"LE\x00\x00", 0x30000)
    host = next(
        j for j in range(le, -1, -1)
        if b[j:j + 2] == b"MZ" and struct.unpack_from("<I", b, j + 0x3C)[0] + j == le
    )
    u = lambda o: struct.unpack_from("<I", b, le + o)[0]  # noqa: E731
    page, npages, last = u(0x28), u(0x14), u(0x2C)
    data_pages = u(0x80) + host
    objs = [struct.unpack_from("<6I", b, le + u(0x40) + 24 * k) for k in range(u(0x44))]
    imgs = []
    for vsize, base, _flags, first, count, _ in objs:
        img = bytearray(vsize + page)
        for p in range(count):
            pg = first + p
            n = page if pg != npages else last
            off = data_pages + (pg - 1) * page
            img[p * page:p * page + n] = b[off:off + n]
        imgs.append((base, img))
    fixpg = [u(0x68) + 4 * i for i in range(npages + 1)]
    fp = [struct.unpack_from("<I", b, le + o)[0] for o in fixpg]
    rec = le + u(0x6C)
    for k, (_vsize, _base, _f, first, count, _) in enumerate(objs):
        img = imgs[k][1]
        for p in range(count):
            o, end = rec + fp[first + p - 1], rec + fp[first + p]
            while o < end:
                src, flg = b[o], b[o + 1]
                o += 2
                if src & 0x20:
                    cnt = b[o]
                    o += 1
                else:
                    srcoff = struct.unpack_from("<h", b, o)[0]
                    o += 2
                if flg & 0x40:
                    tobj = struct.unpack_from("<H", b, o)[0]
                    o += 2
                else:
                    tobj = b[o]
                    o += 1
                if src & 0xF == 2:
                    toff = 0
                elif flg & 0x10:
                    toff = struct.unpack_from("<I", b, o)[0]
                    o += 4
                else:
                    toff = struct.unpack_from("<H", b, o)[0]
                    o += 2
                if src & 0x20:
                    offs = [struct.unpack_from("<h", b, o + 2 * i)[0] for i in range(cnt)]
                    o += 2 * cnt
                else:
                    offs = [srcoff]
                if src & 0xF != 7:  # only 32-bit offsets matter here
                    continue
                target = objs[tobj - 1][1] + toff
                for so in offs:
                    a = p * page + so
                    if 0 <= a and a + 4 <= len(img):
                        img[a:a + 4] = struct.pack("<I", target)
    return imgs


def refs(imgs, texts):
    (cb, code), (db, data) = imgs
    for t in texts:
        j = data.find(t.encode() + b"\x00")
        while j >= 0 and j > 0 and data[j - 1] != 0:
            j = data.find(t.encode() + b"\x00", j + 1)
        if j < 0:
            print(f"{t!r}: not found")
            continue
        pk = struct.pack("<I", db + j)
        at, i = [], code.find(pk)
        while i >= 0:
            at.append(hex(cb + i))
            i = code.find(pk, i + 1)
        print(f"{t!r} at {db + j:#x}, used at {', '.join(at) or 'nowhere directly'}")


def dis(imgs, start, end):
    try:
        from capstone import CS_ARCH_X86, CS_MODE_32, Cs
    except ImportError:
        raise SystemExit("dis needs capstone: pip install capstone")
    (cb, code), (db, data) = imgs
    md = Cs(CS_ARCH_X86, CS_MODE_32)
    md.skipdata = True

    def text(v):
        if db <= v < db + len(data):
            s = data[v - db:data.find(b"\x00", v - db)]
            if 0 < len(s) < 60 and all(32 <= c < 127 for c in s):
                return s.decode()
        return None

    for ins in md.disasm(bytes(code[start - cb:end - cb]), start):
        note = ""
        for tok in ins.op_str.replace("[", " ").replace("]", " ").replace(",", " ").split():
            if tok.startswith("0x") and ":" not in tok and (s := text(int(tok, 16))):
                note += f'  ; "{s}"'
        print(f"{ins.address:08x}  {ins.mnemonic:6} {ins.op_str}{note}")


class Ram:
    """A guest RAM dump, read at data-object addresses."""

    def __init__(self, dump: bytes, imgs):
        self.ram = dump
        db, data = imgs[1]
        want = bytes(data[TEXT_PAGE - db:TEXT_PAGE - db + 0x1000])
        phys = dump.find(want)
        if phys < 0 or phys % 0x1000:
            raise SystemExit("the text page is not in this dump (dump from 0, at least 10 MB)")
        lin = TEXT_PAGE + DATA_DELTA
        self.dir = None
        for t in range(0, len(dump), 0x1000):
            pte = struct.unpack_from("<I", dump, t + ((lin >> 12) & 0x3FF) * 4)[0]
            if pte & 1 and pte & 0xFFFFF000 == phys:
                for d in range(0, len(dump), 0x1000):
                    pde = struct.unpack_from("<I", dump, d + (lin >> 22) * 4)[0]
                    if pde & 1 and pde & 0xFFFFF000 == t:
                        self.dir = d
                        break
            if self.dir is not None:
                break
        if self.dir is None:
            raise SystemExit("no page directory found")

    def read(self, addr: int, fmt: str = "<I"):
        lin = addr + DATA_DELTA
        pde = struct.unpack_from("<I", self.ram, self.dir + (lin >> 22) * 4)[0]
        pte = struct.unpack_from("<I", self.ram, (pde & 0xFFFFF000) + ((lin >> 12) & 0x3FF) * 4)[0]
        return struct.unpack_from(fmt, self.ram, (pte & 0xFFFFF000) | (lin & 0xFFF))[0]


def krishna(imgs, dumps):
    for path in dumps:
        r = Ram(pathlib.Path(path).read_bytes(), imgs)
        line = []
        for i in range(PED_COUNT):
            p = PEDS + i * PED_SIZE
            if r.read(p + 0x17, "<h") == 0 or r.read(p + 0x62) not in KRISHNA_TYPES:
                continue
            line.append(
                (r.read(p + 0x80, "<b"), r.read(p, "<h"), KRISHNA_TYPES[r.read(p + 0x62)],
                 r.read(p + 0x66), r.read(p + 0x83) / 0x400000, r.read(p + 0x87) / 0x400000)
            )
        print(
            f"{pathlib.Path(path).name}: spawned {r.read(SPAWNED, '<h')}, groups {r.read(GROUPS)}, "
            f"score {r.read(PLAYER)} (x{r.read(PLAYER + 4, '<H')}), krishnas {len(line)}"
        )
        for gi, pid, what, mode, x, y in sorted(line):
            print(f"    #{gi} ped {pid} {what}, mode {mode}, at block ({x:.2f}, {y:.2f})")


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--data", help="Game folder (default: GFA_DATA or gfa.local.toml)")
    sub = ap.add_subparsers(dest="cmd", required=True)
    sub.add_parser("refs").add_argument("text", nargs="+")
    d = sub.add_parser("dis")
    d.add_argument("start", type=lambda s: int(s, 16))
    d.add_argument("end", type=lambda s: int(s, 16))
    sub.add_parser("krishna").add_argument("dump", nargs="+")
    args = ap.parse_args()
    imgs = load(find_exe(args.data))
    if args.cmd == "refs":
        refs(imgs, args.text)
    elif args.cmd == "dis":
        dis(imgs, args.start, args.end)
    else:
        krishna(imgs, args.dump)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
