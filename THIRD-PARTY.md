# Third-party notices

Grand Fair Use Auto is an original Rust program. It ships **no game data**: no
maps, styles, text, sound, music, video or anything upscaled from them. Users
supply their own copy of the 1997 game.

Parts of its design, data-format knowledge and game logic are ported from the
two open-source projects below. Both are MIT-licensed. Each ported module names
its sources in its header, and `docs/REFERENCE_MAP.md` lists the mapping.

## Carnage3D

<https://github.com/codenamecpp/carnage3d>. File-format readers (CMP, G24
style, FXT, fonts, sound banks), map-mesh construction, the palette-indexed
rendering approach, and entity metadata tables (`data/entities/`).

```
MIT License

Copyright (c) 2019 jericho

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## AmiGTA1

The deterministic fixed-point simulation (vehicle physics, traffic, peds,
police, weapons, trains, services), the mission script VM, HUD and career
logic, the GRY v325 style reader, and the input-script test format.

```
MIT License

Copyright (c) 2026 angree

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

The AmiGTA1 oracle (`tools/oracle/`) compiles AmiGTA1's own modules from a
separate checkout, linked with our harness (`walktrace.c`), for differential
testing. No AmiGTA1 source is vendored. The Q14 cosine and arctangent tables
in `gfa-fixed` are generated from their formulas, which reproduce AmiGTA1's.

## Archive and dialog crates

Reading the game from the archive it came in uses these crates as ordinary
dependencies (nothing of them is copied into this repository):

- [`rars`](https://github.com/bitplane/rars) (Apache-2.0): RAR 5's LZ
  decoder. Only the decoder is used; the archive directory is read by our own
  code (`crates/gfa-assets/src/archive/`). Its licence ships in the crate as
  `COPYING`; binaries that include it carry that licence with them.
- [`miniz_oxide`](https://github.com/Frommi/miniz_oxide) (MIT, Zlib or
  Apache-2.0): ZIP's deflate.
- [`crc32fast`](https://github.com/srijs/rust-crc32fast) (MIT or Apache-2.0):
  the checksums.
- [`rfd`](https://github.com/PolyMeilex/rfd) (MIT): the desktop's file
  dialog.

## Real-ESRGAN model weights

The optional HD pipeline (`tools/hdtex/`) downloads Real-ESRGAN weights
(BSD-3-Clause, © 2021 Xintao Wang) at setup time. They are never committed.
The network definition in `tools/hdtex/rrdbnet.py` follows the published
RRDBNet architecture.

## Smacker video

The SMK2 decoder (milestone M9) is written from the public format description.
It does not use FFmpeg or libsmacker code.
