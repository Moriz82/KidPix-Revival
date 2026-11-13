#!/usr/bin/env bash
# Build a portable folder: dist/KidPixRevival-<target>/
#
#   scripts/package.sh                 # host platform
#   scripts/package.sh --windows       # also cross-build Windows (needs mingw-w64)
#   scripts/package.sh --with-game     # copy your imported ./game folder next to the program
#
# --with-game copies the files from YOUR OWN Kid Pix Deluxe 3 CD into the folder so it runs
# without the CD. That folder is for your personal use only — don't share it.
set -euo pipefail
cd "$(dirname "$0")/.."

windows=0
with_game=0
for a in "$@"; do
  case "$a" in
    --windows) windows=1 ;;
    --with-game) with_game=1 ;;
    *) echo "unknown option $a" >&2; exit 1 ;;
  esac
done

pack() {
  local target="$1" exe="$2"
  local out="dist/KidPixRevival-$target"
  rm -rf "$out"
  mkdir -p "$out"
  cp "$exe" "$out/"
  cp README.md "$out/README.md"
  if [ "$with_game" = 1 ]; then
    if [ ! -f game/Data/Easel.MHK ]; then
      echo "no ./game folder; run: cargo run --release -p kidpix -- --import <your CD zip/iso>" >&2
      exit 1
    fi
    cp -r game "$out/game"
  fi
  echo "packed $out"
}

host="$(rustc -vV | sed -n 's/^host: //p')"
cargo build --release -p kidpix
pack "$host" "target/release/kidpix-revival"

if [ "$windows" = 1 ]; then
  rustup target add x86_64-pc-windows-gnu >/dev/null
  cargo build --release -p kidpix --target x86_64-pc-windows-gnu
  pack "x86_64-pc-windows-gnu" "target/x86_64-pc-windows-gnu/release/kidpix-revival.exe"
fi
