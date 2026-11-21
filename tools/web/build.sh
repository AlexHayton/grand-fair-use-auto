#!/bin/sh
# Build the web app's release bundle into crates/gfa-web/dist: the page, its
# script and its wasm, made from the sources alone. No game data goes in
# (Principle 6): players select their own archive in the page.
#
# The Vercel build (vercel.json) and the GitHub Pages workflow
# (.github/workflows/pages.yml) both run this. Locally:
#
#   sh tools/web/build.sh                                   # served from /
#   PUBLIC_URL=/grand-fair-use-auto/ sh tools/web/build.sh  # a project site
#
# Rust comes from rust-toolchain.toml; where there is no cargo at all
# (Vercel's build image) rustup is installed first. Trunk is the one on PATH,
# else the release pinned below, checked against its published SHA-256.
set -eu
root=$(cd "$(dirname "$0")/../.." && pwd)
public_url=${PUBLIC_URL:-/}
trunk_version=0.22.2
# trunk-x86_64-unknown-linux-gnu.tar.gz, as GitHub lists it for the release.
trunk_sha256=26fca8a9952e46cd7d8b8b9a2e2c1786c090fc14b8e10b6678ab4d0f9f1a385f
tools=$root/target/web-tools

if ! command -v cargo >/dev/null 2>&1; then
    if [ ! -x "$HOME/.cargo/bin/cargo" ]; then
        echo "build.sh: installing Rust"
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs |
            sh -s -- -y --profile minimal --default-toolchain none
    fi
    PATH=$HOME/.cargo/bin:$PATH
    export PATH
fi
# Build scripts need a C linker; Vercel's image installs packages with dnf.
if ! command -v cc >/dev/null 2>&1 && command -v dnf >/dev/null 2>&1; then
    dnf install -y gcc
fi
# The toolchain, components and wasm target that rust-toolchain.toml names.
(cd "$root" && rustup toolchain install) ||
    (cd "$root" && rustup target add wasm32-unknown-unknown)

if command -v trunk >/dev/null 2>&1; then
    trunk=trunk
elif [ -x "$tools/trunk" ]; then
    trunk=$tools/trunk
else
    case "$(uname -s)-$(uname -m)" in
    Linux-x86_64) ;;
    *)
        echo "build.sh: install Trunk first: cargo install trunk --locked" >&2
        exit 1
        ;;
    esac
    mkdir -p "$tools"
    curl --proto '=https' -sSfL -o "$tools/trunk.tar.gz" \
        "https://github.com/trunk-rs/trunk/releases/download/v$trunk_version/trunk-x86_64-unknown-linux-gnu.tar.gz"
    echo "$trunk_sha256  $tools/trunk.tar.gz" | sha256sum -c -
    tar xzf "$tools/trunk.tar.gz" -C "$tools"
    trunk=$tools/trunk
fi

cd "$root/crates/gfa-web"
"$trunk" build --release --public-url "$public_url"

# The bundle is the page, scripts and wasm; anything else is a mistake.
# (Trunk may leave its lock file behind.)
rm -f dist/.trunk-lock
stray=$(find dist -type f ! -name '*.html' ! -name '*.js' ! -name '*.wasm' ! -name '*.css')
if [ -n "$stray" ]; then
    echo "build.sh: unexpected files in the bundle:" >&2
    echo "$stray" >&2
    exit 1
fi
ls -l dist
