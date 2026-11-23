#!/bin/sh
# Build the patched DOSBox-X probe binary into tools/dosprobe/build/dosbox-x.
# Upstream DOSBox-X commit pinned for reproducibility:
#   https://github.com/joncampbell123/dosbox-x.git @ 322a07a2eae3f1253ac41440becea94229c5b6ff
set -eu

here=$(cd "$(dirname "$0")" && pwd)
build="$here/build"
src="$build/src"

if [ ! -f "$src/configure.ac" ]; then
    echo "build.sh: missing DOSBox-X checkout at $src" >&2
    echo "run: git clone --depth 1 https://github.com/joncampbell123/dosbox-x.git $src" >&2
    exit 1
fi

export PATH="/opt/homebrew/bin:$PATH"
export PKG_CONFIG_PATH="/opt/homebrew/lib/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
export CPPFLAGS="-I/opt/homebrew/include${CPPFLAGS:+ $CPPFLAGS}"
export LDFLAGS="-L/opt/homebrew/lib${LDFLAGS:+ $LDFLAGS}"

# Copy our probe module into the build tree and re-apply surgical hooks cleanly.
cp "$here/gfa_probe.h" "$src/include/gfa_probe.h"
cp "$here/gfa_probe.cpp" "$src/src/misc/gfa_probe.cpp"
git -C "$src" checkout -- \
    src/misc/Makefile.am \
    src/hardware/pic.cpp \
    src/gui/render.cpp \
    src/hardware/hardware.cpp \
    src/dosbox.cpp \
    src/hardware/cmos.cpp \
    src/gui/sdlmain.cpp \
    src/dos/dos_files.cpp
perl "$here/probe_patch.pl" "$src"

cd "$src"
if [ ! -x ./configure ] || ! grep -q "mkdir_p\.cpp gfa_probe\.cpp" src/misc/Makefile.in 2>/dev/null; then
    ./autogen.sh
    rm -f Makefile
fi

CONFIG_FLAGS="--enable-sdl2 --disable-x11 --disable-opengl --disable-freetype --disable-printer --disable-mt32 --disable-libslirp --disable-libfluidsynth --disable-avcodec --disable-sdlnet --disable-xbrz"
STAMP="$build/.configure-flags"
NEED_CFG=0
if [ ! -f Makefile ] || [ ! -f "$STAMP" ] || [ "$(cat "$STAMP")" != "$CONFIG_FLAGS" ]; then
    NEED_CFG=1
fi
if [ "$NEED_CFG" = "1" ]; then
    # shellcheck disable=SC2086
    ./configure $CONFIG_FLAGS
    printf '%s' "$CONFIG_FLAGS" > "$STAMP"
fi

jobs=$(sysctl -n hw.ncpu 2>/dev/null || echo 4)
make -j"$jobs"
cp "$src/src/dosbox-x" "$build/dosbox-x"
chmod +x "$build/dosbox-x"
echo "built $build/dosbox-x (upstream $(git -C "$src" rev-parse --short HEAD 2>/dev/null || echo unknown))"
