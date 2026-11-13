# Kid Pix Revival

A new, cross-platform (Windows, Linux, macOS) engine written in Rust that runs the
pictures, sounds, stamps, backgrounds, stickers and tools from **your own copy** of the
Kid Pix Deluxe 3 CD (2000).

It works like ScummVM: this repository contains only new code. No art, sound, fonts or
program code from the original product is included or redistributed — everything is read
at runtime from the CD (or CD image) you own.

> Unofficial fan project. Not affiliated with or endorsed by the owners of Kid Pix.

## Running it

Build (Rust 1.80+; Linux also needs `libasound2-dev libxkbcommon-dev` or your distro's
equivalents):

```sh
cargo build --release -p kidpix
```

### One-time import — after that the CD / ISO isn't needed

The first time it starts, Kid Pix Revival asks for your Kid Pix Deluxe 3 CD: pick the
`.zip` or `.iso` image, or a folder (the CD itself, a copy of it, or an installed Kid Pix
Deluxe 3). It copies the files it needs (about 320 MB) into a `game` folder next to the
program — or, if that folder isn't writable, into your user data folder
(`~/.local/share/KidPixRevival`, `%APPDATA%\KidPixRevival`,
`~/Library/Application Support/KidPixRevival`). From then on it starts straight away and
you can delete the CD image.

From the command line:

```sh
kidpix-revival --import "Kid-Pix-Deluxe-3_Win_EN_ISO-Version.zip"   # once
kidpix-revival                                                       # every time after
```

Inside this repository you can import into `./game` (git-ignored) and just `cargo run`:

```sh
cargo run --release -p kp-import --example import -- <your .zip/.iso> game
cargo run --release -p kidpix
```

Other options: `--data <folder>` or `KIDPIX_DATA=<folder>` use a folder in place.

The hybrid CD image contains Mac resource forks next to the Windows files; the built-in
importer skips them. (If you extract the `.iso` by hand, use `7z x -aoa`; `bsdtar`
overwrites the real files with 4990-byte resource forks.)

### Portable folder

`scripts/package.sh [--windows] [--with-game]` builds `dist/KidPixRevival-<target>/` with
the program (and, with `--with-game`, your imported `game` folder so the folder runs on its
own). A folder made with `--with-game` contains your CD's files: keep it for yourself.

## What works

* The original easel: background, all buttons with their rollover/press animations and
  sounds, the undo character, play/stop, save, dynamite (erase all), new picture.
* Pencil (sizes) and real-media brushes (crayon, chalk, marker, airbrush) whose pads and
  parameters come from the CD's `Brushes.MHK` and `KPD3.cfg`.
* Wacky Brush: rainbow, drippy, bubbles, splatter, zig-zag, dots, pine, echo, 3-D tube,
  smear, blur, mosaic, x-ray, grey, old photo, posterize, solarize, color shift, emboss,
  outline, the CD's 16 "custom" glint brushes, its animated picture trails (`Cyclic.MHK`)
  and magic sprays (`Anims.MHK`).
* Paint can: solid plus the CD's 63 gradient fills, 49 pattern fills and 41 textures.
* Electric Mixer: invert, kaleidoscope, flips, ripple, swirl, shatter, mosaic and more.
* Stamper with all stamp libraries and three sizes; Letters: type with the CD's fonts
  or use the talking alphabet stamps (each letter says its name);
  Moving Van (move/copy a piece, drag stickers); Eraser sizes and whole-page erasers.
* Backgrounds, sounds, stickers and animations from the CD libraries (library drawer with
  thumbnails and titles). Stickers and animations are the CD's Flash movies, rendered by
  `kp-swf`; animations play on the page. Drag them with the Moving Van, resize with the
  mouse wheel, remove with Delete or the Eraser.
* Slide Show: collect pictures, choose one of 30 transitions and a sound per slide, play
  them full-screen.
* Open pictures (`.png`, and the CD's `Idea Machine` `.kpx` files), save as PNG.
* Menus: File / Edit / Add / Help; shortcuts Ctrl+Z/Y/S/O/N.

## Layout

| crate | purpose |
|-------|---------|
| `crates/mohawk` | Reader for Broderbund Mohawk archives and every resource type the program uses (bitmaps incl. the 16-bit sprite RLE and LZ, JPEG bitmaps, palettes, sprite sets, registration points, animation scripts, button tables, sounds, cursors). |
| `crates/kp-import` | Finds or imports the user's CD from a folder, `.iso` (ISO 9660 + Joliet, skipping Mac resource forks) or `.zip`. |
| `crates/kp-swf` | Renders the CD's Flash stickers and animations. |
| `crates/kidpix` | The application (`kidpix-revival`): winit + softbuffer window, cpal audio, the easel, tools and libraries. |
| `docs/FORMATS.md` | Reverse-engineering notes for the file formats. |

## Not done yet

Printing (the printer button saves a PNG), microphone recording, text-to-speech, the
user picker/login screen and the QuickTime movies on the CD.

## Testing

`cargo run -p mohawk --release --example census -- <cd-root>` decodes every resource on the
CD and reports anything it can't handle.
`kidpix-revival --data <cd-root> --render-test <out-dir>` drives the easel with scripted
input and writes screenshots (no window needed).
