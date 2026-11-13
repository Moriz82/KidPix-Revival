# Kid Pix Deluxe 3 data formats (reverse-engineering notes)

All notes below were derived by inspecting the Windows CD (2000, v1.0). Everything is
**big-endian** unless stated otherwise. Nothing from the CD is stored in this repository;
the engine reads the user's own copy at runtime.

## The CD

* Hybrid ISO9660 (Win + Mac). Many files appear **twice** in the ISO9660 directory: the
  first record is the Mac *resource fork* (ISO "associated file", file-flags bit `0x04`;
  these are all 4990 bytes for the `.MHK`/`.klb` files) and the second is the real data
  fork. Extractors that write both to one path (bsdtar) clobber the real file. Always skip
  associated-file records.
* Interesting directories:
  * `Data/*.MHK`, `Data/Tools/*.MHK`, `Data/Soundfx/*.MHK` – UI and tool resources.
  * `Libraries/{Backgrounds,Stamps,Stickers,Sounds,Toons}/*.klb` – content libraries
    (also Mohawk archives).
  * `Idea Machine/*.kpx` – sample pictures (Mohawk archives, see below).
  * `INSTALL/Fonts/*.TTF` – fonts installed by the original setup.
  * `QTmovies/*.mov` – QuickTime movies: Sorenson Video 1 + IMA4 (see "QuickTime movies").
  * `Data/Quicktimefx/*.qfx` – QuickTime transition effects (not Mohawk).
  * `KpInstall/KPD3.exe` – the original Win32 program (reference only).

## Mohawk archive (`MHWK`)

```
0x00 'MHWK'
0x04 u32 size-8
0x08 'RSRC'
0x0C u16 version (0x0100)
0x0E u16 compaction
0x10 u32 total file size
0x14 u32 resource directory offset (absolute)          = D
0x18 u16 file table offset (relative to D)
0x1A u16 file table size
D+0  u16 name list offset (relative to D)
D+2  u16 type count
D+4  type count x { tag[4], u16 resource table off (rel D), u16 name table off (rel D) }
resource table: u16 count, count x { u16 id, u16 file-table index (1-based) }
name table:     u16 count, count x { u16 name offset (rel D + name list off), u16 file index }
file table (D + file table offset):
  u32 count, count x { u32 offset, u16 size_lo, u8 size_hi, u8 flags, u16 unknown }
  size = size_lo | size_hi<<16 | (flags & 7)<<24
```

## Resource types

| Tag    | Meaning |
|--------|---------|
| `tBMP` | single bitmap (see *Bitmaps*) |
| `tBMH` | bitmap set: `u16 count, u16 ?, u16 ?, u16 setFormat, count x u32 offset`; each entry is a bitmap. If `setFormat & 0xf00 == 0x100` everything after the 8-byte header is LZ-packed and the offsets are relative to (header + unpacked data). The original program only accepts packing 0x000, 0x100, 0x200 (LZ with a 14-byte header) and 0xf00 (plug-in codecs such as JPEG); anything else — including the `0x400` sets in the 1999 `MainTools.MHK` and `Icons.MHK` 2350 — makes it return error 0x2a63, so those two sets are unused leftovers. |
| `tPAL` | palette: `u16 first, u16 count, count x {r,g,b,pad}` |
| `tCNT` | `u16 baseId, u16 count` (e.g. 1000/238: the tBMH 1000 has 237 cels, REGS have 238 entries). The extra cel `count` is the standalone `tBMP baseId` (splash still, easel backdrop); it has no registration point and is drawn at the script's top-left. |
| `REGS` | registration points. Two REGS per tBMH: `REGS id` = x array, `REGS id+1` = y array; `i16` per cel, index = cel number (1-based, entry 0 unused) |
| `tSCR` | animation script (see *Scripts*) |
| `MBTN` | button table (see *Buttons*) |
| `tWAV` | sound (see *Sounds*) |
| `tCUR` | Mac `CURS`: 32 bytes 16x16 1bpp image, 32 bytes mask, `i16 hotY, i16 hotX` |
| `STRL` | string list |
| `MRGN` | Mac region (cut shapes) |
| `grad` | gradient table (TP.MHK) |
| `tSWF` | Flash SWF movie (uncompressed `FWS`, version 3) – stickers and toons |
| `tTHM` | thumbnail bitmap (48x48 JPEG bitmap, format 0x0f03) |
| `tATX` | item title text (raw Latin-1, no terminator) |
| `rLHR` | library header: tag (`tSWF`/`tBMP`/`tWAV`) followed by an item list |

## Bitmaps

Header: `u16 width (&0x3fff), u16 height (&0x3fff), u16 bytesPerRow, u16 format`.

`format` bits: `bpp = fmt & 7` (2 = 8bpp, 3 = 16bpp, 4 = 24bpp), `0x08` has CLUT,
`draw = fmt & 0xf0`, `pack = fmt & 0xf00`.

Formats seen on the CD:

| fmt     | meaning |
|---------|---------|
| `0x0002`| 8bpp raw, rows of `bytesPerRow` |
| `0x0012`| 8bpp RLE8: per row `u16 rowBytes`, then codes: `c&0x80` → run of `(c&0x7f)+1` copies of next byte, else `(c&0x7f)+1` literal bytes |
| `0x0003`| 16bpp raw RGB555 (stamps; `0x7fff` = white) |
| `0x0033`| 16bpp **sprite RLE** (see below), transparent where skipped |
| `0x0133`| LZ-packed (`0x100`) 16bpp sprite RLE |
| `0x0f03`, `0x0f04` | JPEG: after the 8-byte header comes `'jpeg'`, 10 bytes (`u32 0`, `u16 ?`, `u32 jpegLen`?) then a JFIF stream starting `FF D8` |

16bpp pixels are RGB555 (`0RRRRRGGGGGBBBBB`).

### 16bpp sprite RLE (`draw = 0x30`)

Per row:

```
u16 rowDwords   – length of the row body in 32-bit words
u16 xStart      – transparent pixels before the first command
then commands until rowDwords*4 bytes are consumed:
  u16 cmd, u16 skipAfter
  if cmd & 0x8000:  run of (cmd & 0x7fff) pixels; value is a u32 holding two
                    RGB555 pixels (A,B) – alternate A,B,A,B… (usually A==B)
  else:             cmd literal RGB555 pixels, padded to a 4-byte boundary
  then skipAfter transparent pixels
```

Rows may be missing at the end of the data (treat as transparent).

### LZ (`pack = 0x100`)

Mohawk LZSS, same as ScummVM's `MohawkBitmap::unpackLZ`: header `u32 uncompressed,
u32 compressed, u16 dictSize` (0x100..0x1000 select 8..12 position bits; pack 0x200 adds 4
more header bytes); flag byte LSB-first, `1` = literal byte,
`0` = `u16 offLen`: `len = (offLen >> 10) + 3`, `pos = (offLen + 66) & 0x3ff`,
copy from `out[-((insertPos - pos) & 0x3ff)]`.

## Scripts (`tSCR`)

```
0x00 u16 0x003e
0x02 u16 tBMH id the cels come from (also REGS id pair)
0x04 u16 cel count
0x06 u16 0x20, u16 0, u16 0x20, u16 0, u16 1
0x10 i16 left, top, right, bottom      – bounds (also the hit rect for buttons)
0x18 u16 0
0x1A ops: u8 op, u8 len (len = total bytes incl. the 2 header bytes), payload
  0x04 len 4  : u16 tempo (always 100)
  0x03 len 6  : u16 sound index, u16 flag  – plays a tWAV of the same archive
  0x10 n      : FRAME – here the 2nd byte is a COUNT n, followed by n x
                { u16 cel [| 0x8000], i16 x, i16 y }   (bit 15 is NOT a visibility flag: cels
                                                       without it are drawn too, e.g. Splash 1000, RFlattenDlg labels)
  0x01 len 2  : show frame / wait one tick
  0x00 len 2  : end
```

Cel draw position: `screen = bounds.topLeft + (x, y) - REGS[cel]`.

## Buttons (`MBTN`)

Array of 32-byte records (16 x u16):
`[0] rest script, [1] rollover script, [2]/[3] usually = [1], [4] 1, [5] 0,
[6] command id, [7] 1, [8] 0, [9] 0, [10] press script, [11] 1, [12] 0, [13] 0,
[14] radio group / kind, [15] 0`.
The hit rect is the rest script's bounds. Rest scripts are mostly a single frame; the
rollover script carries the rollover sound (op 0x03) and the press script the click
animation (e.g. the dynamite's explosion). Sound op index `i` plays `tWAV 1000 + i` of the
same archive (Easel).

Easel (`Easel.MHK` MBTN 75) command ids, identified from their artwork:

| cmd | scripts (idle/hover/click) | function |
|-----|-----------|----------|
| 224 | 1000/1001/1002 | oval 1 – backgrounds |
| 222 | 1003/1004/1005 | oval 2 |
| 223 | 1006/1007/1008 | oval 3 |
| 225 | 1009/1010/1011 | oval 4 |
| 232 | 1012/1013/1014 | Pencil |
| 233 | 1015/1016/1017 | Paint brush (Wacky Brush) |
| 234 | 1018/1019/1020 | Paint can (fill) |
| 237 | 1021/1022/1023 | Electric mixer (effects) |
| 236 | 1024/1025/1026 | Stamper |
| 229 | 1027/1028/1029 | ABC text |
| 227 | 1030/1031/1032 | Moving van (select/move) |
| 228 | 1033/1034/1035 | Eraser |
| 238 | 1052/1053/1054 | (lower right sail) |
| 239 | 1079/1080/1081 | Help |
| 242 | 1049/1050/1051 | small button at (751,436) |
| 243 | 1041/1039/1040 | Play |
| 244 | 1045/1043/1044 | Stop |
| 270 | 1082/1083/1084 | Slide show |
| 281 | 1038/1036/1037 | Save |
| 266 | 1089/1090/1091 | Undo guy (right side) |
| 267 | 1086/1087/1088 | Door / back to picker |

Other Easel scripts: 1092/1093 (bottom-left color controls), 1094-1116 (face
animations), 1117 canvas frame (bounds 111,47-713,501 → canvas is **600x452 at 112,48**),
1118 color picker area (0,491-85,600), 1056-1070 option slot frames, 1071-1077 option
scroll arrows.

## Sounds (`tWAV`)

```
'MHWK' u32 size 'WAVE'
optional 'Cue#' u32 size ...
'Data' u32 size
  u16 sampleRate, u32 sampleCount, u8 bitsPerSample, u8 channels,
  u16 encoding (0 = PCM, 1 = IMA ADPCM, 2 = MPEG-2 layer II),
  u16 loopCount, u32 loopStart, u32 loopEnd, then samples
```
16-bit PCM is big-endian signed; 8-bit PCM is unsigned.

## Library files (`*.klb`)

* Backgrounds: `tBMP` (600x452 JPEG, fmt 0x0f04) + `tTHM` + `tATX`.
* Stamps: 112 `tBMP` 32x32 16bpp raw (ids 8000+).
* Stickers / Toons: `tSWF` + `tTHM` + `tATX`.
* Sounds: `tWAV` + `tTHM` + `tATX`.

## Saved pictures (`*.kpx`)

Mohawk archive; the first resource is a 600x452 JPEG bitmap (fmt 0x0f03).

## Program configuration (`KpInstall/KPD3.cfg`, `KPD3.ini`)

Plain INI text shipped on the CD; the engine parses them at runtime.

* `KPD3.cfg` maps each subsystem to its archives (`[Easel] File_0 = "data/MainTools.mhk"`,
  `File_1 = "data/Easel.mhk"`, … `[BrushPads]` → `Brushes.mhk`, `[Deleters]`,
  `[Cyclic]`, `[RSprayPanel]` → `Anims.mhk`, `[RealMediaSounds]` → `FeltPenSnds.mhk`).
  Paths are relative to the CD root, case-insensitive.
* It also holds one section per brush class (`[Airbrush]`, `[Marker]`, `[Crayon]`,
  `[Chalk]`, `[SmearBrush]`, `[BlurBrush]`, `[MosaicBrush]`, `[Drippy]`, `[CustomBrush1..16]`,
  `[CustomVoiceBrush1..10]`, `[Touchless1..10]`, transitions `[Blend]`…). Keys:
  `brushID` (tBMP id in `Brushes.MHK`), `spacing` (distance between pads, 0 = continuous
  spray), `spatter` (pad disintegration), `densityBias` (larger = more transparent),
  `dryOutRate`/`dryOutThreshold`, `oscillation` (pad jitter from centre), `shadowOffset`/
  `shadowBias`, `glintOffset`/`glintBias`, and for hands-free brushes `wrap`,
  `amplitudeBias`, `frequencyBias`, `gravityBias`, `gravityTime`, `gravityRange`,
  `acceleratonBias` (sic), `dampBias`. (`[RealMediaSettings]` in the file documents these.)
  Values may carry a trailing `;` and `//` comments.
* `KPD3.ini` `[CanvasSettings]`: `easelLeft 112, easelTop 48, easelWidth 600,
  easelHeight 452`; `[PageObjectLimits] SWFMax 15` (max stickers per page).

## QuickTime movies (`QTmovies/*.mov`)

12 movies, 320×240, 15 fps, Sorenson Video 1 (`SVQ1`) and, in 9 of them, IMA 4:1 ADPCM
stereo 44.1 kHz (`ima4`). Reader: `crates/kp-qt` (verified bit-exact against ffmpeg).

* **Container:** `moov/trak/mdia` with `mdhd` (time scale), `hdlr` (`vide`/`soun`),
  `minf/stbl`: `stsd` (format, size / channels / rate), `stco`, `stsc`, `stsz`, `stts`.
  Video samples are frames; sound chunks hold `stsc` sample frames each.
* **IMA4:** per channel per 64 samples a 34-byte packet: big-endian header (top 9 bits
  predictor, low 7 bits step index), 32 bytes of nibbles (low first); channels alternate.
  Apple's decoder keeps its running predictor when the header's index matches and its
  predictor is within 0x7f of it (the header is only 9-bit precise).
* **SVQ1 tables** (read at runtime from `INSTALL/QT4/QuickTime/QuickTimeAuthoring.qtx`,
  the Sorenson Video R2.0 *compressor*; VA `0x66ebd444`): 16 serialised code tables
  (`i32 count, i32 first symbol, count × u32 (length << 27 | code)`) in this order: inter
  mean (512, −256..255), intra mean (256), motion difference (64, value + 32: magnitude
  code then sign bit), block type (4: `01` inter, `001` four vectors, `000` intra, `1`
  skip), intra stage count per level 5..0, inter stage count per level 5..0 (symbol =
  stages + 1, 0 = no vector). Directly after (`@0x66e03800`): codebooks for levels 3, 2,
  1, 0 (8×8, 8×4, 4×4, 4×2), each an intra then an inter book of 6 stages × 16 vectors,
  values stored `+128`, every vector as 4×4 tiles left→right, top→bottom, each tile row by
  row. Frame-size table at VA `0x66ebd248`.
* **Frame:** 22-bit picture code (0x20; other codes scramble words 1–4 and add a
  checksum / message), 8-bit temporal reference, 2-bit type (0 key, 1 delta, 2 droppable
  delta); key frames: 5 unknown bits, 3-bit size code (7 = 12-bit width and height); two
  optional flag groups. Then Y, U, V planes (U/V a quarter size each way, padded to 16) in
  16×16 macroblocks.
* **Vectors:** a breadth-first binary tree from 16×16 (level 5) down to 4×2 (level 0),
  odd levels split top/bottom, even ones left/right; one split bit per node above level 0.
  A leaf: stage-count code (−1 = leave as is / zero), mean code, `4 × stages` bits of
  codebook indices (stage j uses vectors 16j…16j+15); pixel = clamp(mean + Σ vectors)
  (delta: added to the prediction). Levels 4 and 5 carry means only.
* **Delta macroblocks:** skip (copy), inter (one half-pel vector), four vectors (8×8
  each), intra. Vectors are horizontal then vertical difference added to the median of
  three predictors (left, above, above-right in a row array; TR uses TL; BL uses TL, TR
  and the left block's; BR uses BL, TR, TL), wrapped to 6 bits, then clipped to the
  plane; half-pel by rounding averages.

## Spelling lexicon (`KpInstall/usa86.lex` + `engphon.env`)

Franklin / Proximity Technology word list used by *Check Spelling* (decoded from the
spell engine in KPD3.exe; reader: `crates/kp-lex`). 101,996 entries.

**`engphon.env`** (phonetic rule set). Byte 0 = version (1 → rule outputs start at 10,
else 16). 16 u16 LE at offset 4: `h0` bins (170), `h1` letter codes (58), `h2` first bin
byte (12), `h3` bins per first byte (46), `h5` trie size (159), `h6` offset of the
3-letter rules (35), `h7` bin-string bytes (919), `h8` letter table bytes (116), `h15`
rule bytes (151). From offset 36: `h7` bytes of NUL-separated, sorted phonetic class
strings (the bins); `h8` bytes of (letter, class) pairs, one per letter code (letter 9 =
double the previous letter, 11 = silent); 3×256 character tables; the double-array trie
(`base`, `check`, `output`, `h5` bytes each, symbols = char − 88, `output` 127 = no
rule); `h15` bytes of rule outputs (literal strings when ≥ 0x41, else class bytes).

A word's **bin** is found by transcribing `Y` + word + `Z` with longest trie matches into
(letter, class) pairs, collecting classes 17–31 (dropping repeats and classes 24/30/31,
keeping every 25), and taking the last bin string ≤ that class string: bin index *k* →
bytes (`k / h3 + h2`, `k % h3 + h2`).

**`usa86.lex`**. 7 u16 LE at 0x102: version, flags, sub, `ntok`, `ndig`, `nblk`, `slen`.
At 0x110: `ntok` drop counts, `ndig` 2-byte digraphs, then `slen` bytes of
NUL-separated strings: `ntok` suffix tokens followed by `nblk` block keys. Block *i* is
1024 bytes at (5 + *i*) × 1024; its first entry is its key, the rest start at byte 7 and
are front-coded: a byte ≥ `h1 + 16` appends token *k* (after dropping `drop[k]` bytes),
a byte in `h1 … h1 + 15` drops that many − `h1` trailing bytes (`h1 + 15` takes a second
count byte), then the tail follows: letter codes < `h1`, digraph codes, `4` + literal
spelling (up to the next 0xFF), and `0xFF flag` pairs; 0 ends the block. Every entry =
2 bin bytes + letter codes. Flags: 1 known misspelling (literal = correction), 2 proper
noun, 3 acronym, 5/6/9/0x14 abbreviations, 7 single letter, 0x10 exact spelling in the
literal. Literal bytes ≥ 0x80 are code page 437 (accented letters).

## KPD3.exe structure (from RTTI, REA `inspect_pe_resources` + r2 strings)

The program is built on an in-house C++ framework ("Rembrandt"). RTTI class names give
the tool architecture: `RToolMgr`, `RTool`, `RToolPanel` with panels `RPencilPanel`,
`RWackyToolPanel`, `RFillPanel`, `RMixerPanel`, `RStampPanel`, `REraserPanel`,
`RRealMediaToolPanel`, `RScrollToolPanel`; brushes `RAirbrush`, `RBlurBrush`,
`RBristleBrush`, `RColorizeBrush`, `REdgeBrush`, `REmbossBrush`, `RExtrusionBrush`,
`RGreyscaleBrush`, `RHueShiftBrush`, `RMosaicBrush`, `RNuclearBrush`, `RPixelFXBrush`,
`RPosterizeBrush`, `RSmearBrush`, `RSolarizeBrush`, `RXRayBrush`, `RVoiceBrush`,
`RHandsFreeBrush`, `RCustomBrush1..16`, `RWackyEffectBrush`; fills `RSolidFill`,
`RPatternFill`, `RGradientFill`; erasers `REraserTool`, `RFullCutEraser`,
`RQuickTimeEraser`; `RStampEditor`, `RColorPicker`, `UndoManager`. Win32 cursor resources:
`IDCRSRFILLBUCKETCURSOR`, `IDCRSRMAGNETCURSOR`, `IDCRSRPAINTBRUSHCURSOR`,
`IDCRSRPENCILCURSOR`.

## More resource notes

* `STRL` string list: `u8 count`, then `count` NUL-terminated Latin-1 strings
  (e.g. `GFXTransitions.MHK` STRL 7500 = 30 transition names, `TransitionSounds.MHK`
  STRL 8500 = 31 sound names for tWAV 12000+i).
* `GFXTransitions.MHK` tBMP 7500+i: 23x21 transition icons (the effects themselves were
  QuickTime `.qfx` files).
* `Data/Tools/TP.MHK` option artwork: tBMP 21300–21362 gradient fills, 21400–21448
  pattern fills, 21500–21541 greyscale textures (8-bit), 22000–22081 alphabet-stamp letters
  with matching voice clips tWAV 22000+; 18000+ = tool panel buttons (MBTN 777…18900,
  scripts 18000–18274) for the bottom panels; `grad` 21300 = gradient stop lists
  (`u16 type, u16 n, n x {u16 pos 0..100, r,g,b,a}`).
* `Data/General.MHK` tCUR 10000+n cursors: 0 crayon, 1 pencil, 2 marker, 3 eraser,
  5/15 paint brush, 6 airbrush, 8/9 closed/open glove, 11 push pin, 12 mixer,
  13 scissors, 14 pointing glove, 17 paint can, 18 clock, 19 text, 20 speaker,
  21 microphone, 22/23 help, 24 stamp, 26 save, 28 arrow.
* `Data/Tools/Cyclic.MHK` (15 sets) and `Anims.MHK` (20 sets + scripts + sounds): sprite
  sets used for animated brush trails/sprays.
* Easel: the 7-slot green tray (scripts 1055–1077) holds 48x48 library thumbnails
  (`tTHM`); the Easel's "face" (scripts 1094–1116) is the undo character, whose talking
  animations play voice clips through script sound ops.
