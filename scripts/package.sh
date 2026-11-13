#!/usr/bin/env bash
# Build native, double-clickable apps in dist/:
#
#   macOS    dist/Kid Pix Revival.app  and  dist/KidPixRevival-macOS.dmg   (universal arm64+x86_64)
#   Windows  dist/KidPixRevival-Windows/kidpix-revival.exe (+ .zip)       (GUI program with icon)
#   Linux    dist/KidPixRevival-x86_64.AppImage  (+ a .tar.gz with a .desktop file)
#
#   scripts/package.sh                 # the host platform
#   scripts/package.sh --windows       # also cross-build Windows (needs mingw-w64)
#   scripts/package.sh --with-game     # put your imported ./game folder into the package
#
# --with-game copies the files from YOUR OWN Kid Pix Deluxe 3 CD into the package so it runs
# without the CD. That package is for your personal use only — don't share it.
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

version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
mkdir -p dist

need_game() {
  if [ ! -f game/Data/Easel.MHK ] && [ ! -f game/Data/EASEL.MHK ]; then
    echo "no ./game folder; run: cargo run --release -p kidpix -- --import <your CD zip/iso>" >&2
    exit 1
  fi
}

# ---------------------------------------------------------------- macOS .app + .dmg
package_macos() {
  local app="dist/Kid Pix Revival.app"
  rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null 2>&1 || true
  cargo build --release -p kidpix --target aarch64-apple-darwin
  cargo build --release -p kidpix --target x86_64-apple-darwin
  rm -rf "$app"
  mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
  lipo -create -output "$app/Contents/MacOS/kidpix-revival" \
    target/aarch64-apple-darwin/release/kidpix-revival \
    target/x86_64-apple-darwin/release/kidpix-revival
  sed "s/@VERSION@/$version/g" packaging/macos/Info.plist > "$app/Contents/Info.plist"
  printf 'APPL????' > "$app/Contents/PkgInfo"
  cp packaging/icons/kidpix-revival.icns "$app/Contents/Resources/"
  if [ "$with_game" = 1 ]; then need_game; cp -R game "$app/Contents/Resources/game"; fi
  # ad-hoc signature (Apple Silicon refuses unsigned code); a Developer ID can replace "-"
  codesign --force --deep --sign "${MACOS_SIGN_IDENTITY:--}" --options runtime \
    --entitlements packaging/macos/entitlements.plist "$app"
  local dmg="dist/KidPixRevival-macOS.dmg"
  rm -f "$dmg"
  local stage; stage="$(mktemp -d)"
  cp -R "$app" "$stage/"
  ln -s /Applications "$stage/Applications"
  hdiutil create -volname "Kid Pix Revival" -srcfolder "$stage" -ov -format UDZO "$dmg" >/dev/null
  rm -rf "$stage"
  echo "packed $app and $dmg"
}

# ---------------------------------------------------------------- Windows .exe
package_windows() {
  local target="$1" exe="$2"
  local out="dist/KidPixRevival-Windows"
  rm -rf "$out"
  mkdir -p "$out"
  cp "$exe" "$out/kidpix-revival.exe"
  cp README.md "$out/README.md"
  if [ "$with_game" = 1 ]; then need_game; cp -r game "$out/game"; fi
  rm -f "dist/KidPixRevival-Windows.zip"
  if command -v zip >/dev/null; then
    (cd dist && zip -qr KidPixRevival-Windows.zip KidPixRevival-Windows)
  elif command -v 7z >/dev/null; then
    (cd dist && 7z a -bd -tzip KidPixRevival-Windows.zip KidPixRevival-Windows >/dev/null)
  fi
  echo "packed $out ($target)"
}

# ---------------------------------------------------------------- Linux AppImage
package_linux() {
  cargo build --release -p kidpix
  local appdir="dist/KidPixRevival.AppDir"
  rm -rf "$appdir"
  mkdir -p "$appdir/usr/bin" "$appdir/usr/share/applications" \
    "$appdir/usr/share/icons/hicolor/256x256/apps" "$appdir/usr/share/icons/hicolor/512x512/apps"
  cp target/release/kidpix-revival "$appdir/usr/bin/"
  cp packaging/linux/AppRun "$appdir/AppRun"
  cp packaging/linux/kidpix-revival.desktop "$appdir/"
  cp packaging/linux/kidpix-revival.desktop "$appdir/usr/share/applications/"
  cp packaging/icons/icon-256.png "$appdir/kidpix-revival.png"
  cp packaging/icons/icon-256.png "$appdir/usr/share/icons/hicolor/256x256/apps/kidpix-revival.png"
  cp packaging/icons/icon-512.png "$appdir/usr/share/icons/hicolor/512x512/apps/kidpix-revival.png"
  if [ "$with_game" = 1 ]; then need_game; cp -r game "$appdir/usr/share/kidpix-revival-game"; fi
  local tool="${APPIMAGETOOL:-$(command -v appimagetool || true)}"
  if [ -n "$tool" ]; then
    ARCH=x86_64 "$tool" --no-appstream "$appdir" "dist/KidPixRevival-x86_64.AppImage" >/dev/null
    echo "packed dist/KidPixRevival-x86_64.AppImage"
  else
    echo "appimagetool not found (set APPIMAGETOOL=…); made the AppDir and a .tar.gz only"
  fi
  tar -C dist -czf dist/KidPixRevival-Linux-x86_64.tar.gz "$(basename "$appdir")"
  echo "packed dist/KidPixRevival-Linux-x86_64.tar.gz"
}

case "$(uname -s)" in
  Darwin) package_macos ;;
  Linux) package_linux ;;
  MINGW*|MSYS*|CYGWIN*)
    cargo build --release -p kidpix
    package_windows "$(rustc -vV | sed -n 's/^host: //p')" target/release/kidpix-revival.exe ;;
esac

if [ "$windows" = 1 ]; then
  rustup target add x86_64-pc-windows-gnu >/dev/null 2>&1 || true
  cargo build --release -p kidpix --target x86_64-pc-windows-gnu
  package_windows x86_64-pc-windows-gnu target/x86_64-pc-windows-gnu/release/kidpix-revival.exe
fi
