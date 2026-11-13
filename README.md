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

Everything is rebuilt from the original data and from reverse-engineering notes
(`docs/RE_PANELS.md`, `docs/RE_BRUSHES.md`, `docs/RE_SHELL.md`):

* **Start-up:** the splash screen with its music, then the graphical log-in dialog (users
  are the folders in `Users/`; type `KidPixAdmin` to add or delete users). Each user has
  their own `user.ini` preferences, `Words.dic` pronunciations and My Pictures / My Sounds /
  My Stamps folders.
* **Easel:** the original screen and buttons (rollover and press animations, sounds), the
  Undo Guy, Play/Stop, Save, New Page, the firecracker erase-all, the paper-texture tray,
  Print, Help and the exit door. The hidden Win32-style menu bar (File / Edit / Add /
  Toolbox / Controls / Email / Small Kids Mode / Help) uses the original command ids, texts,
  enable rules and Ctrl shortcuts.
* **Tool panels:** every option panel is the original TP.MHK art and button tables, with
  the shutter animation between tools, item scrollers with page numbers, and selection art.
* **Drawing and painting:** the original brush engine (`kp-brushes`): pencil, chalk,
  crayon and marker media; the 20 realistic paint brushes; all 25 Wacky Brushes (Madona,
  Drippy, Spiral, Poly, TriSpin, Particle, Garland, Bricks, Window, Door, Roof, DotToDot,
  ABC, Rainbow, 8-Way Mirror, Tree and the rest); Sound Art (10 voice brushes and 10
  hands-free brushes, driven by the microphone); the spray can's 15 shape sprays and 20
  moving-paint sprays. The tool sounds follow the original rules.
* **Shapes:** lines, curves, boxes, ovals and polygons, filled or empty.
* **Paint can:** exact 4-connected fill; solid and texture fills, 63 gradients (dragging
  moves the gradient's centre), 49 patterns.
* **Electric Mixer:** the 22 Big Mixer effects (Space selects the alternates) and the 12
  Mixer Wand brushes.
* **Stamper:** all stamp libraries (name field with popup list), sizes ×1/×2/×4, the stamp
  follows the pointer, and the Stamp Editor (edits are kept in the user's My Stamps).
* **ABC tool:** text boxes in any of the CD's fonts, sizes 7–72, bold / italic /
  underline, alignment, cut/copy/paste, "speak all text" (text-to-speech through the
  system voice, with Say It This Way exceptions). Small Kids Mode switches to the talking
  alphabet stamps.
* **Moving Van:** the glove moves stickers and text boxes; scissors with free-form,
  polygon, oval and rectangle cuts and 10 cookie cutters; Trash, Duplicate and Glue.
* **Eraser:** the little eraser (round/square, 3 sizes) and the 15 big erasers
  (Firecracker plus the QuickTime-style wipes, Vanish).
* **Colour picker:** press the wheel, drag over a colour, release; the eye dropper picks
  from the picture; the colour splat shows the current colour.
* **Libraries:** backgrounds, stickers, animations and sounds in the original bottom
  panels (folder list, scroller); drag an item onto the page. Stickers and animations are
  the CD's Flash movies, rendered by `kp-swf`, with the edit frame (resize, flip, loop,
  frame-by-frame). Sound recording (10 s) with Play, Stop and Save to My Sounds.
* **Pictures:** native `.kpx` files (open the CD's Idea Machine pictures, save your own
  with stickers, text and sound as objects); Add Graphic, Import Background (with the
  original placement options) and Import Sound.
* **Modern image saving (additions, not in the original):**
  * *Save Picture As* also offers PNG, JPEG and BMP next to the Kid Pix format; images
    contain the whole picture with its stickers and text.
  * *Controls > Turn PNG Copy On* writes a `.png` with the same name next to every saved
    `.kpx` (remembered per user).
  * *File > Export* (now also **Ctrl+E**) saves PNG, JPEG or BMP; PNG and image files can
    be opened again.
* **SlideShow:** the original Project Zone screen: 96 slides, thumbnails, drag to reorder,
  trash, the effects view with transition, timer and audio dialogs, `.kpp` slide-show files,
  saved shows strip, and the full-screen player with the 30 transitions and their sounds.
* **Dialogs:** save prompts, alerts, Edit ABC Text, Say It This Way, Flatten (selected /
  all), name-your-page and name-your-sound (Small Kids Mode), print (full page or a 2×2 /
  3×3 poster) and help mode (click ?, then anything: the original help text and voice).

## Known differences

* **Text-to-speech** offers the original three voices (Adult Male #1, Adult Female #1,
  Adult Female #2) mapped to the operating system's speech engine (SAPI on Windows, `say`
  on macOS, eSpeak NG / Speech Dispatcher on Linux), since the L&H TruVoice engine itself is
  not on the CD.
* **E-mail:** *Email > Set Up* is the original Parental Controls dialog. The Kid Pix Email web
  service no longer exists, so *Send* saves the picture (JPEG) or page (`.kpx`) into
  My Pictures/Email and opens your mail program and that folder, so you can attach the file.
* **QuickTime movies** (Add > Movie; the CD's `QTmovies`) play natively: `crates/kp-qt`
  reads the `.mov` files, decodes Sorenson Video 1 and IMA4 sound, and takes the
  Sorenson codebooks and code tables from the CD's own
  `INSTALL/QT4/QuickTime/QuickTimeAuthoring.qtx` (copied by the importer), so no codec
  data is bundled. All 12 CD movies decode identically to ffmpeg's reference decoder
  (picture planes and sound samples). Only movies in other formats, or an import made
  before the `.qtx` was copied, fall back to an `ffmpeg` executable.
* **Check Spelling** has the original Spell Checker dialog (Change / Skip / Learn / Done,
  learned words in the user's `user.dct`) and checks against the CD's own
  Franklin/Proximity lexicon (`KpInstall/usa86.lex` + `engphon.env`, ~102,000 words,
  read by `crates/kp-lex`); suggestions come from its misspelling list and its phonetic
  bins. The ranking of suggestions is the engine's own (closest spelling first), not
  Proximity's. Only if the lexicon is missing does it fall back to the SCOWL word list
  (`crates/kidpix/data`, see `SCOWL-LICENSE.txt`).
* Menus and Win32-style dialogs use a built-in bitmap font drawn in the Windows 9x
  "MS Sans Serif" style (the original font itself isn't on the CD), so they look the same
  on every operating system. Text the original drew in Windows system fonts (Arial, Times
  New Roman, … — the Picker title, text boxes set to them) uses the computer's own copy
  (Liberation fonts on Linux), so those letters differ slightly between systems; the CD's
  own fonts render identically everywhere.
* A few details are inferred where the original couldn't be traced; they are marked **[I]**
  in the docs (for example the text-box style bits, how often the original's idle loop
  ticks — it paces hands-free Sound Art and Particle —, and that palette index 255, which
  the inverting Sound Art brushes use, is white). Screens and panels were checked side by
  side against the screenshots in the CD's own User Guide (`KpInstall/KdpxManual.pdf`).

## Testing

`cargo run -p mohawk --release --example census -- <cd-root>` decodes every resource on the
CD and reports anything it can't handle.
`kidpix-revival --data <cd-root> --render-test <out-dir>` drives the whole program with
scripted input and writes about 50 screenshots (no window needed).
`cargo run -p kp-brushes --release --example brushsheet -- <cd-root> out.png` paints every
brush once. `KP_QT_REF=<dir> cargo test -p kp-qt --release` decodes every CD movie and,
with reference dumps (`<movie>.yuv` from `ffmpeg -pix_fmt yuv410p -f rawvideo`,
`<movie>.pcm` from `ffmpeg -f s16le`), requires them to match exactly. `cargo test -p kp-kpx` checks that every Idea Machine page is rewritten
byte-for-byte.
