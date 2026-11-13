# Kid Pix Deluxe 3 – tool option panels and the non-brush tools

This is an implementation spec, worked out by reverse-engineering `KPD3.exe` (Win32, 2000) and the
data on the CD. It covers the easel's bottom option panels, the paint can, electric mixer, eraser,
stamper, ABC/text tool, moving van and color picker. Brush algorithms (the Wacky Brush, real-media
brushes and the mixer *wand* brushes) are only listed here, because they're covered elsewhere.
Behaviour is described in my own words and pseudocode. No CD-derived art is reproduced.

Confidence tags: **[O]** = seen in code or data (constants copied exactly), **[I]** = inferred
(plausible, not proven; check it against the original if possible).

Conventions: screen coordinates are absolute, in the 800×600 easel window. The canvas is at
(112,48), size 600×452. "Script N" means `tSCR N`, drawn with the usual
`screen = bounds.topLeft + (x,y) − REGS[cel]`. Unless another archive is named, scripts, MBTN
tables, bitmaps and sounds below are in `Data/Tools/TP.MHK`. A script sound op with index *i*
plays the `tWAV` *base+i* of the script's own archive, where base = the archive's resource block
(TP 18000, Easel 1000, BigPicker 1500, StampEd 14000). For example, TP op `0x45` plays TP
tWAV 18069. **[O]** for TP (the indices line up with the 18000–18070 tWAV block).

---

## 1. Shared panel framework

### 1.1 Which panel each easel tool opens [O]

`Easel.MHK` MBTN 75 sends the command id. The easel's panel switcher creates the panel:

| Easel cmd | Tool | Panel class | Notes |
|---|---|---|---|
| 232 | Pencil | `RPencilPanel` | |
| 233 | Paint brush | `RRealMediaToolPanel` | real paint, microphone and spray sub-panels; the brush researcher's area |
| 234 | Paint can | `RFillPanel` | |
| 237 | Electric mixer | `RMixerPanel` | |
| 236 | Stamper | `RStampPanel` | |
| 229 | ABC | `RTextPanel`, or `RAlphaPanel` when **Small Kids Mode** is on | the global-prefs short at offset +2 (`user.ini` / GlobalSave). `RAlphaPanel` reads `[SmallKidsAlphabet]` |
| 227 | Moving van | `RCutMovePanel` | |
| 228 | Eraser | `REraserPanel` | |
| 230 | — | `RWackyToolPanel` | The switcher handles it, but Easel MBTN 75 has no 230 button (only the old `MainTools.MHK` MBTN 75 does). Unreachable in KPD3 **[O]**. |
| 222/223/224 | ovals | `RLibPanel` | library drawer (not covered here) |
| 225 | sounds oval | `RAudioPanel` | uses TP MBTN 777 (record, play, stop, trash…), not covered here |

Tool switch animation [O/I]: the old panel is closed and the new one built; the panel strip plays
script 18036 (4 frames, sound idx 2 → tWAV 18002; shutter lines converging) and script 18035
(4 frames, sound idx 1 → tWAV 18001; lines opening). Both cover (96,510)–(726,600). My reading of
the order is close = 18036, then open = 18035 over the new panel **[I]**.

### 1.2 Geometry

* The bottom strip is drawn by one **panel background script** per panel. It covers
  (93..96, 504..510)–(726,600): a left "sockets" panel (≈ x 93–305) and a long right panel
  (≈ x 310–726) with the scroll-arrow column at x 698–740. **[O]**

| Script | Used by | Look |
|---|---|---|
| 18039 | `RAlphaPanel` | both halves empty (dark) |
| 18044 | Pencil | 4 sockets left, light right area |
| 18045 | Paint brush | 3 sockets left, 2 stacked slots at x≈310 |
| 18046 | Paint can | 3 bucket sockets |
| 18047 | Mixer | 2 sockets (mixer, wand) |
| 18048 | Stamper | dark left panel with a text field; right area with an extra slot |
| 18049 | Text | left font/style area; right area with the voice field |
| 18050 | Moving van | 2 sockets (glove, scissors) |
| 18051 | Eraser | 2 sockets (pencil eraser, block eraser) + 2 stacked slots |
| 18040/41/42/43/52 | unused here or by other panels | |

* Draw order: background script → MBTN rest frames → scroller slot frames and icons → arrows.
  **[I]**

### 1.3 MBTN buttons in panels [O]

A record has the usual 16 u16 fields. The ones that matter here:
`[0]` rest script, `[1]` rollover script, `[10]` press script, `[6]` command id, `[7]` kind
(1 = button, 3 = button that sends a value, 4 = static text field with no press script),
`[8]` value sent with the command (sizes 1..3, pencil item index 0..3); the kind meanings are **[I]**, `[14]` radio group.
The hit rect is the rest script's bounds. The rollover script usually has 2 frames plus a sound;
the press script has 1 frame.

Left-panel tool buttons (pencil types, buckets, mixer/wand, erasers, glove/scissors, brush types)
have **animated** scripts. Each of rest, rollover and press has 4–13 frames and its own sound
(full table in the appendix). Behaviour [I]: rollover plays the hover animation and sound; press
plays the "pick up" animation, then the button stays in the press script's last frame while it is
selected. Deselecting plays the rest script.

**8-bit cels:** 131 of the 741 cels in TP tBMH 18000 are 8-bit RLE (fmt 0x12/0x02, no CLUT).
They are soft drop-shadow alpha maps. 0 = transparent, other values = coverage. Draw them as black
with alpha = value/255 (`dst = dst*(255−a)/256`) **[I]**. Rendering them through a palette gives
black boxes. The same blend appears in code as the "tint" routine (§3.3 *Tint*).

### 1.4 Item scroller (RBitmapScroller) [O]

Most right panels hold one row of N square slots that page through a list of 48×48 icons.

* **Slot-row scripts:** every slot is 52×52, pitch 51 px, y 525–577, and the row is right-aligned
  to x = 695. A row script uses cel 739 (empty slot frame) for each slot.
  * 7 slots: 18000/18015–18018 (x = 337, 388, 439, 490, 541, 592, 643)
  * 6 slots: 18019–18021 (x from 388)
  * 5 slots: 18022, 18024, 18027 (x from 439)
  * 4 slots: 18023 (x from 490)
* **Selected frame** (green, cel 740) **[I: green = selected, red = hover; the hover set is the one the mouse-over code uses]**: per-slot scripts 18001–18007 (slot x = 337…643). A row
  with N slots uses scripts `18008−N … 18007`.
* **Hover frame** (red, cel 741): 18008–18014, or `18015−N …` for N slots. It is shown while the
  mouse is over a slot.
* **Icons** are drawn centred in the slot: `x = slot.left + (52/2 − w/2)`, likewise for y, as a
  transparent blit.
* **Arrows:** up = scripts 18028 (rest) / 18029 (hover, sound idx 0 → tWAV 18000) / 18030
  (press), cmd 18350. Down = 18031 / 18032 / 18033, cmd 18351. Both sit in the arrow column at
  (705,515)–(738,542) and (701,568)–(738,600). Script **18034** is the "arrows disabled" art;
  panels with no list (pencil, text, small eraser, grab mode) loop it.
* **Paging:** one click moves a whole page (step = visible slot count). Wrap is on: next on the
  last page goes to page 0, previous on page 0 goes to the last page. Arrows are enabled only when
  item count > visible count.
* **Page number:** Easel script 1078 at (717,545)–(733,558) shows two digit cels, the current page
  number (1-based, capped below 100) **[O]** (help id "OptionsTrayNum").
* Clicking slot *k* selects item `first + k` and sends `'scht'` to the panel with the item index.

| Panel | Slot row | Visible |
|---|---|---|
| Paint can | 18018 | 7 |
| Mixer | 18021 | 6 |
| Moving van | 18020 | 6 |
| Stamper | 18022 | 5 |
| Eraser (big) | 18024 | 5 |
| ABC (Small Kids) | 18000 | 7 |

### 1.5 Size buttons [O]

| MBTN | Look | Buttons (cmd 18200, value 1/2/3) |
|---|---|---|
| 18200 | 3 balls | (342,520)–(366,536), (342,536)–(368,555), (335,555)–(373,588); hover sounds 18052/18053/18054 |
| 18201 | 3 squares, same column | (346,522)–(359,536), (343,535)–(365,557), (335,555)–(373,588); sounds 18055/18056/18054 |
| 18202 | 3 balls (stamper) | same rects as 18200; sounds 18062/18063/18064 |

The panel turns value *v* into tool size index *v−1* and broadcasts `'chng'` with the float *v*.
The tool sets `size_px = sizeTable[v−1]`. Size tables [O]:

* `RCircle` (pencil "Pencil" and the eraser): {2, 10, 20}
* default tool: {5, 10, 20}
* stamp: {0, 1, 2}, i.e. scale ×1 / ×2 / ×4
* `RSplat` {1, 2, 3}, `RParticle` {5, 10, 50}

### 1.6 Panel prefs [O]

Each panel saves its state under a name in the user prefs: `PencilPanel`, `FillPanel`,
`MixerPanel`, `EraserPanel`, `StampPanel`, `CutPanel`, `TextPanel`, `RealMediaPanel`. Defaults
when nothing is saved are listed per tool below.

---

## 2. Tool id table [O]

The tool factory switch (vtable slot 17) maps ids to classes. Names come from the exe's name table
(used for `KPD3.cfg` sections `<Name>` and `<Name>Snd`). Icons are `Data/Tools/Icons.MHK` tBMP ids.

| id | name / class | icon | | id | name / class | icon |
|---|---|---|---|---|---|---|
| 65 | XRay `RXRay` | 3900 | | 87 | SmearBrush | 3800 |
| 66 | Nuclear | 3901 | | 88 | BlurBrush | 3801 |
| 67 | Emboss | 3902 | | 89 | GreyscaleBrush | 3802 |
| 68 | Lighten | 3903 | | 90 | ColorizeBrush | 3803 |
| 69 | Sepia | 3904 | | 91 | XRayBrush | 3804 |
| 70 | Colorize | 3905 | | 92 | EmbossBrush | 3805 |
| 71 | ColorCycle `RHueShift` | 3906 | | 93 | EdgeBrush | 3806 |
| 72 | StripColor | 3907 | | 94 | NuclearBrush | 3807 |
| 73 | Pointilize | 3908 | | 95 | SolarizeBrush | 3808 |
| 74 | Posterize | 3909 | | 96 | HueShiftBrush | 3809 |
| 75 | Solarize | 3910 | | 97 | PosterizeBrush | 3810 |
| 76 | Pinch | 3911 | | 98 | MosaicBrush | 3811 |
| 77 | Spherize | 3912 | | 99 | Circle `RCircle` (= Pencil) | – |
| 78 | Ripple | 3913 | | 100 | Chalk | 3500 |
| 79 | Bubbles | 3914 | | 101 | Crayon | – |
| 80 | Burlap | 3915 | | 102 | Marker | 3500 |
| 81 | Whirl | 3916 | | 103–110 | ModifierLine, Curve, OSquare, FSquare, OCircle, FCircle, OPoly, FPoly | |
| 82 | Pulse | 3917 | | 111–114 | ModifierCutFree, CutCircle, CutSquare, CutPoly | |
| 83 | Wavelet | 3918 | | 115 | Eraser `REraserTool` (subclass of RCircle) | |
| 84 | Shear | 3919 | | 116 | FillSolid `RSolidFill` | |
| 85 | Flip | 3920 | | 117 | FillPattern `RPatternFill` | |
| 86 | Kaleidoscope | 3921 | | 118 | FillGradient `RGradientFill` | |
| | | | | 119 | SprayGun · 120 SimpleBitmap · 121 CyclicBitmap · 122 MarchingAnts | |
| | | | | 123–137 | big erasers (§3.8) | 3700+(id−123) |

Ids 0–64 are brushes and Wacky Brush effects (RealBrush, Airbrush, Bristle, Extrusion,
CustomBrush1–16, Madona, Drippy, Spiral, Poly, TriSpin, WhispyCircle, Particle, Garland,
LeakyPen, ScratchyPen, LoopingLine(2), Bricks, Log, Window, Door, DoorPlain, Roof, DotToDot, ABC,
Rainbow, ColorTile, 8WayMirror, Splat, Tree, CustomVoiceBrush1–10, Touchless1–10). They belong to
the brush researcher. Icons.MHK 3922/3923 exist but no mixer effect uses them.

---

## 3. The tools

### 3.1 Pencil (232) – `RPencilPanel` [O]

* Background 18044. Script 18034 (arrows disabled) loops; there is no scroller.
* **Left, MBTN 18400 #0–3** (cmd 18400, value = index, group 18400): selects tool `99 + index`.

  | # | Tool | Rest rect | Sounds |
  |---|---|---|---|
  | 0 | Pencil (`RCircle`) | (90,514)–(157,599) | 18004/18005/18006 |
  | 1 | Chalk | (135,500)–(204,599) | 18007/18008/18009 |
  | 2 | Crayon | (157,512)–(243,599) | 18010/18011/18012 |
  | 3 | Marker | (232,515)–(301,600) | 18013/18014/18015 |

  Order confirmed by the help ids PencilTool/ChalkTool/CrayonTool/MarkerTool.
* **Sizes:** MBTN 18200 (balls), see §1.5. The pencil's sizes are {2, 10, 20} px.
* **Shapes, MBTN 18400 #4–12** (group 18401, hover sound 18061). cmd → modifier id:

  | cmd | Shape | Id | Rect |
  |---|---|---|---|
  | 18458 | freehand | −1 | (387,532)–(439,578) |
  | 18450 | line | 103 | (450,526)–(505,551) |
  | 18451 | arc/curve | 104 | (450,557)–(507,574) |
  | 18452 | box | 105 | (517,526)–(565,549) |
  | 18453 | filled box | 106 | (518,557)–(564,579) |
  | 18454 | oval | 107 | (578,523)–(629,549) |
  | 18455 | filled oval | 108 | (579,556)–(629,579) |
  | 18456 | polygon | 109 | (639,527)–(697,550) |
  | 18457 | filled polygon | 110 | (640,557)–(695,579) |

  The modifier id is computed as `cmd − 18347`. Shapes are drawn with the selected pencil medium.
* **Defaults:** Pencil, size button 2 (10 px), freehand.
* **Sounds** (KPD3.cfg): `[CircleSnd]` start 7286, loop 7287, directional N/S/E = 7283/7284/7285.
  `[ModifierLineSnd]` loop 7314, stop 7315; `[ModifierCurveSnd]` 7316/7317; the other modifiers
  in the same pattern. These sound ids are in `Data/Tools/WackyBrushSounds.MHK` (tWAV 7200–7430).
* The pencil tool is `RCircle`: a round dab of diameter = size (or a square, see eraser) stamped
  along the mouse path. Chalk, Crayon and Marker are real-media brushes (other researcher).

### 3.2 Paint brush (233) – `RRealMediaToolPanel` (layout only) [O]

* Background 18045. **Left, MBTN 18000:**

  | cmd | Item | Rect | Sounds |
  |---|---|---|---|
  | 18000 | paint brush | (93,499)–(165,600) | 18016/17/18 |
  | 18001 | microphone (voice brush) | (162,507)–(236,600) | 18019/20/21 |
  | 18002 | spray can | (223,512)–(304,600) | 18022/23/24 |

* **Mode toggle** (stacked pair at x 379–432). Each left item swaps in its own MBTN:
  * brush → 18001 (18003 "real paint" splat (379,517)–(432,553), 18004 "wacky" shapes (379,552)–(432,585))
  * mic → 18002 (18006 mouse-driven, 18005 hands-free)
  * spray → 18003 (18007 shape spray, 18008 moving/"caterpillar" spray)
* Sizes 18200 are shown for brush and mic; the spray variant changes their state. The brush
  "tray" selector is MBTN 18900: field 18801 at (114,545)–(249,571) plus arrows 18802/18803,
  backed by the cfg `[TrayN.M]` sections. Sub-panels: `RBrushSubPanel`, `RVoiceSubPanel`,
  `RSpraySubPanel`.

### 3.3 Paint can (234) – `RFillPanel` + fills

**Panel [O]**

* Background 18046. Scroller of 7 slots (18018).
* **Left, MBTN 18300** (kind 3, group 3):

  | cmd | Bucket | Rect | Sounds | Tool | Slot icons | Fill data |
  |---|---|---|---|---|---|---|
  | 18300 | solid / texture | (81,497)–(174,600) | 18025/26/27 | 116 FillSolid | tBMP 21500+i (42 items) | 20500+i |
  | 18301 | gradient | (165,519)–(233,600) | 18028/29/30 | 118 FillGradient | tBMP 21300+i (63 items) | `grad` 21300 entry i |
  | 18302 | pattern | (216,513)–(297,600) | 18025/26/27 | 117 FillPattern | tBMP 21400+i (49 items) | 20400+i |

  The icon list is built by counting up from the base id until a tBMP is missing.
* Choosing slot *i* calls `tool.setResource(base + i)`: 20500+i, 21300+i or 20400+i. The panel
  remembers one selected index per bucket.
* Defaults: bucket 0, index 0 for every bucket.
* Sounds: `[FillSolidSnd]` 7217, `[FillPatternSnd]` 7216, `[FillGradientSnd]` 7215, all with
  `lingers = on`.

**Region finding (all three fills) [O]**

The canvas is a 16-bit RGB555 bitmap. To fill at click point P:

1. Set bit 15 of every canvas pixel (mark).
2. Flood fill with GDI `ExtFloodFill(P, colour = pixel at P, FLOODFILLSURFACE)` using a red
   brush. That replaces exactly the connected pixels whose colour equals P's colour (exact match
   on the 15-bit value, zero tolerance) and clears their bit 15. GDI flood fill is 4-connected
   **[I]**.
3. Composite: for each pixel, if bit 15 is clear (filled) take the source pixel at the same
   coordinates; otherwise clear bit 15 again (original kept).

The current engine's tolerance-24 flood is **not** faithful. Use exact match, 4-connected.

**Solid / texture fill (116) [O]**

* Source = a canvas-sized buffer. Start with a copy of the current picture, then tile the 8-bit
  texture `tBMP 20500+i` from canvas (0,0), stepping by the bitmap's width and height. Each tile
  is drawn with *Tint* in blend mode, using the current colour.
* **Tint** (`FUN_00459930`):
  * `C5 = current RGB888 >> 3` per channel.
  * For each 8-bit texel `g`: if `g == 0` skip (keep).
  * Multiply mode (mode 0): `dst = RGB555(C5.r*g>>8, C5.g*g>>8, C5.b*g>>8)`.
  * Blend mode (≠0, used here): per channel `dst = (dst*(255−g) + C5*g) >> 8`.
* 20500 is 48×48, all 255, so index 0 is a plain solid fill. 20501–20541 are 160×160 greyscale
  textures. Tiles align to the canvas origin.

**Pattern fill (117) [O]**

* Same buffer approach, but tile the 16-bit sprite `tBMP 20400+i` (160×160) with a plain copy.
  Its transparent pixels (a few patterns have them) keep the original picture **[I]**.

**Gradient fill (118) [O]**

* Each gradient is one entry of `grad` 21300, a packed list of
  `{u16 type, u16 n, n × {u16 pos 0..100, u8 r, g, b, a}}` with 63 entries:
  * entries 0–20: type 2, horizontal (colour varies along x)
  * entries 21–41: type 1, vertical
  * entries 42–62: type 0, radial
  * Icon 21300+i is entry i. All alpha bytes are 255. At most 8 stops are honoured.
* After the region is found, take the region's **bounding box** B (canvas coordinates).
  * Type 2: colour at `x` = piecewise-linear interpolation of the stops at `t = (x − B.left)/B.w`
    (pos/100). Rows are identical.
  * Type 1: same along y.
  * Type 0 (radial): centre `c` = centre of B, `R` = half the diagonal of B
    (`sqrt((w/2)² + (h/2)²)`).
    * Stop k is a circle of radius `pos_k/100 · R`, centred at `c + (F − c)·(1 − pos_k/100)`,
      where F is the focus point passed in (the click point **[I]**). The last stop is centred
      exactly on c.
    * Colour is interpolated between consecutive circles; each circle is a 20-gon (OpenGL ring
      strips, smooth shading). A simpler equivalent: for each pixel, find the stop pair whose
      circles bracket it and lerp.
* The result is rendered to an offscreen bitmap (OpenGL, alpha blending, all alphas 255) and
  copied into the region only.
* The exe also converts the gradient into a SWF gradient record (`0x10` linear / `0x12` radial,
  ratio = pos·255/100); probably for the vector/undo path. Not needed for drawing.

### 3.4 Electric mixer (237) – `RMixerPanel` + effects

**Panel [O]**

* Background 18047, scroller 6 slots (18021), sizes MBTN 18200 (values 1/2/3, default 2).
* **Left, MBTN 18600:**
  * 18600 **Big Mixer** (98,506)–(243,600), sounds 18031/32/33 → effects 65–86 (22 items, icons
    Icons 3900–3921)
  * 18601 **Mixer Wand** (239,507)–(301,600), sounds 18034/35/36 → brushes 87–98 (12 items,
    icons 3800–3811; brush algorithms are the other researcher's)
* Item i → tool `base + i` (base 65 or 87).
* Defaults: Big Mixer, size 2, item 0 (XRay).

**Effect order (slot index = id − 65) [O]**

XRay, Nuclear, Emboss, Lighten, Sepia, Colorize, ColorCycle, StripColor, Pointilize, Posterize,
Solarize, Pinch, Spherize, Ripple, Bubbles, Burlap, Whirl, Pulse, Wavelet, Shear, Flip,
Kaleidoscope.

**When it runs [O]**

* On mouse-down on the canvas the effect runs **once, over the whole 600×452 picture**. The
  click point is the centre for the geometric effects. There is no animation. Repeated clicks
  stack.
* `size` = the panel size value (1..3, float → int).
* `alt` = whether key code 0x20 is down at click time. The key helper maps 1→Ctrl, 2→Alt,
  4→Shift and anything else to a raw virtual key, so 0x20 is **Space** **[I]**.
* Start sound: cfg `[<Name>Snd] startSoundID`. XRay 7411, Nuclear 7398, Emboss 7391, Lighten 7413,
  Sepia 7404, Colorize 7401, ColorCycle 7394, StripColor 7414, Pointilize 7417, Posterize 7408,
  Solarize 7402, Pinch 7422, Spherize 7399, Ripple 7405, Bubbles 7400, Burlap 7419, Whirl 7403,
  Pulse 7410, Wavelet 7426, Shear 7392, Flip 7425, Kaleidoscope: see Unknowns (the cfg has two
  `[KaleidoscopeSnd]` sections, 7237 and 7406).

**Colour effects** work per pixel on 5-bit channels (0..31) with the exe's integer HSV.

* `muldiv(a,b,c) = (a*b + c/2)/c`.
* **RGB→HSV:**
  * `V = max(r,g,b)`, `d = V − min`, `S = V ? muldiv(31,d,V) : 0`.
  * H: if `r==V`: `muldiv(60,g−b,d)`; elif `g==V`: `muldiv(60,b−r,d)+120`; else
    `muldiv(60,r−g,d)+240`. Then wrap to 0..359. With d = 0, H = 0 (guard the division).
* **HSV→RGB:**
  * `p = muldiv(31−S, V, 31)`, `f = H%60`, sector `H/60`.
  * For each sector the (r,g,b) are 0: (V, p+muldiv(f,V−p,60), p); 1: (p+muldiv(60−f,V−p,60), V, p);
    2: (p, V, p+muldiv(f,V−p,60)); 3: (p, p+muldiv(60−f,V−p,60), V);
    4: (p+muldiv(f,V−p,60), p, V); 5: (V, p, p+muldiv(60−f,V−p,60)).
* Clamp the results to 0..31 **[I]**.

| Effect | Normal | alt |
|---|---|---|
| XRay | invert each channel `c = 31−c` | hue +180 first, then invert |
| Nuclear | hue +180 | hue +180, then invert |
| Emboss | `V' = clamp(V − V(x+2,y+2) + 15, 0, 31)`, S = 0, H kept; near the right/bottom edge the neighbour offset shrinks to stay inside | same |
| Lighten | r,g,b each +1 (cap 31) | each −1 (floor 0), i.e. darken |
| Sepia | H = 27, S = 15, V += 2 if V < 30 | same |
| Colorize | H,S from the current colour (RGB888 → 5-bit → HSV), V kept | same |
| ColorCycle | hue +30 | hue +330 |
| StripColor | S = 0 (greyscale) | S −= 1 |
| Pointilize | H += rand[−60,60], V += rand[0,15] | H += rand[−30,30], V += rand[0,31] |
| Posterize | `V = min(31, ((V+8)/8)*8)` | `V = min(31, ((V+2)/2)*2)` |
| Solarize | `if V>15: V=31−V; V=min(31,2V)` | same |

`rand[a,b]` is `rand()` from the C runtime with rejection to make it uniform.

**Geometric effects** (polar remap about the click point C) [O]

```
W,H = canvas size; R = max(W,H)/2; out = white (0x7FFF) canvas
for each dest (x,y):
    dx = x − C.x ; dy = y − C.y            (doubles)
    if not (dx == 0 and dy == 0): remap(dx,dy)   // per effect, below
    sx = trunc(dx) + C.x ; sy = trunc(dy) + C.y
    out(x,y) = (0 < sx < W and 0 < sy < H) ? src(sx,sy) : 0x7FFF
```

Pixels whose source lands outside become white. Note the strict `> 0` test. In the formulas
below, r = |(dx,dy)|, θ = atan2(dy,dx), and s = size (1..3).

| Effect | remap(dx,dy) |
|---|---|
| Pinch | `r' = sqrt(r·R)`; if `r' < R`: (dx,dy) = r'·(cos θ, sin θ) |
| Spherize | if `r < R`: `r' = r²/R`; (dx,dy) = r'·(cos θ, sin θ) |
| Whirl | `Δ = (sqrt(r·R)/r)·π / (6/s)`; θ' = θ + Δ (alt: θ − Δ); keep r |
| Wavelet | if `r < R`: θ' = θ ± s·sin(2π·r/R)/(2π) (+ normal, − alt); keep r |
| Kaleidoscope | θ' = 2·s·θ. Normal: r' = r/(s·π·0.5), dx = (sin θ'+cos θ')·r', dy = (sin θ'−cos θ')·r'. Alt: r' = r/(s·π/3), dx = (cos θ'−sin θ')·r', dy = (cos θ'+sin θ')·r' |
| Ripple | Normal: k = 8/s; dx += π·cos(dy/(kπ)); dy += π·sin(dx_new/(kπ)). Alt: f = 5s; `t = sin(dx/f)+cos(dx/f)+cos(dy/f)+sin(dy/f)`; dx += f·t; then dy += f·(sin(dx/f)+cos(dx/f)+cos(dy/f)+sin(dy/f)) using the new dx and the old dy terms |
| Bubbles | Normal: dy += 4s·cos(dx/s)·sin(dy/s). Alt: f = 4s; dy += 4f·cos(dx/f)·sin(dy/f) |
| Pulse | (skip if dx = 0) f = 10s (alt 100s); dx += π·cos(dy/(π·dx/f)); dy += π·sin(dx/(π·dy/f)) (new dx) |
| Burlap | s' = max(s,1); m = 6s' (alt 24s'); a = trunc(dx) % m; dx += cos(a)·a; b = trunc(dy) % m; dy += sin(b)·b |

The **row effects** are not polar:

* **Shear** [O]: each row is shifted horizontally by `2·rand[−3s, 3s]` px (alt `2·rand[−12s, 12s]`).
  Pixels written outside the row are dropped.
* **Flip** [O/I]: normal mirrors left↔right; alt mirrors top↔bottom. The decompile of the address
  arithmetic is garbled, so the axis assignment is **[I]**.

### 3.5 Stamper (236) – `RStampPanel`, `RStampSimpleBitmap`, `RStampEditor`

**Panel [O]**

* Background 18048. MBTN 18700 sits mostly in the **left** panel:

  | cmd | Item | Rect | Notes |
  |---|---|---|---|
  | 18702 | library name field | (114,545)–(249,571) | kind 4; drawn in font "Vogel" 14 pt |
  | 18703 | previous library | (248,545)–(280,558) | hover sound 18003; no wrap |
  | 18704 | next library | (248,558)–(280,571) | hover sound 18003; no wrap |
  | 18701 | stamp editor pencil | (384,525)–(433,588) | hover sound 18065 |

* Clicking the name field opens a popup list of all stamp libraries (`RPopup`). The chosen
  library comes back as cmd 8002 with the index **[O]**. cmd 8001 (re)creates the stamp tool.
* Sizes: MBTN **18202** (balls) → stamp scale ×1 / ×2 / ×4 (32 / 64 / 128 px).
* Scroller: 5 slots (18022, x = 439…643). The icons are the library's own 32×32 stamps, centred
  in the slots. 112 stamps → 23 pages.
* Libraries are the `Libraries/Stamps/*.klb` files (20 on the CD, each 112 tBMP 8000–8111, 32×32
  raw RGB555), enumerated under the library type tag `tSTM`, plus the user stamps library
  `UserStampsLib`. A `[DebugSettings] UseLibsFromCD` flag picks CD vs installed path.
* `LittlePicker.MHK` / `BigPicker.MHK` are **not** stamp pickers; they are the colour pickers (§4).

**Stamp tool [O/I]**

* While the stamper is active, the stamp follows the cursor as a ghost sprite (animation channel),
  centred on the hotspot (w/2, h/2).
* A click blits the stamp, nearest-neighbour scaled by 1/2/4. Transparent colour: white `0x7FFF`.
  Stamp corners are practically always 0x7FFF; the blit mode is the transparent mode **[I]**.
* Sound: `[SimpleBitmapSnd]` 7200 (another sound id 7320 is set up in one state).
* No flip/rotate controls found in the panel.

**Stamp editor (`StampEd.MHK`, `RStampEditor`) [O]**

* Dialog background script 14000, bounds (154,99)–(670,444).
* Edit grid script 14001 at (320,124)–(577,381): 256×256, i.e. 32×32 at 8×.
* Actual-size preview script 14002 at (611,124)–(646,159).
* Palette script 14003 at (186,250)–(313,413). tBMP 14000 (117×165) is its index map.
* Buttons (MBTN 14000, sounds StampEd tWAV 14000+idx):

  | cmd | Rect | Function |
  |---|---|---|
  | 19000 | (215,124) | pencil (mode 3) |
  | 19001 | (258,122) | paint can (mode 7, flood within the stamp) |
  | 19002 | (212,164) | face icon: plays a sound and calls a helper. Purpose unknown; maybe eyedropper |
  | 19003 | (258,162) | firecracker: clear |
  | 19004 | (210,205) | ↔ flip horizontal |
  | 19005 | (256,203) | undo |
  | 19006 | (324,392)–(452,416) | RESTORE ORIGINAL |
  | 19007 | (467,392)–(537,416) | CANCEL |
  | 19008 | (547,392)–(617,416) | OK: saves if changed |

  Grid clicks and drags arrive as cmd 19010 and paint one cell at a time. Palette clicks (19009)
  look up the colour index under the cursor. 19011 clears to white 0x7FFF.

### 3.6 ABC (229) – `RTextPanel` (normal) / `RAlphaPanel` (Small Kids Mode)

**RTextPanel [O]**

* Background 18049, MBTN **9383**. Script 18034 loops. Panel UI font "vogel" 12 pt.
* Left panel:

  | cmd | Item | Rect | Notes |
  |---|---|---|---|
  | 210 | font-name field | (109,526)–(220,552) | |
  | 200 / 201 | font up / down | (219,526)–(238,539) / (219,539)–(238,552) | sound 18003 |
  | 211 | size field | (241,526)–(268,552) | |
  | 202 / 203 | size up / down | (267,526)–(286,539) / (267,539)–(286,552) | sound 18003 |
  | 207 / 208 / 209 | **B / I / U** toggles | (118,565)–(136,588) / (147,565)–(157,588) / (170,565)–(187,593) | groups 5/6/7, independent; rest 2-frame with sound 18037, hover 18038 |
  | 204 / 205 / 206 | align left / center / right | (200,562) / (230,562) / (260,562), each 26×31 | radio group 2, hover sound 18039 |

* Right panel:

  | cmd | Item | Rect | Sound |
  |---|---|---|---|
  | 240 | cut | (316,527)–(352,588) | 18068 |
  | 241 | copy | (349,523)–(383,554) | 18059 |
  | 242 | paste | (347,553)–(384,588) | 18060 |
  | 232 | speak all text (lips) | (392,524)–(438,588) | 18066 |
  | 231 | voice-name field | (458,540)–(569,566) | |
  | 236 / 237 | voice up / down | (568,540)–(587,553) / (568,553)–(587,566) | 18067 |

* Size list [O]: 7, 9, 10, 12, 14, 16, 18, 20, 22, 24, 28, 32, 48, 72.
* Font list: the exe enumerates installed font families with `EnumFontFamilies`. KPD3 installs
  the 22 TTFs in `INSTALL/Fonts`; the revival should list those families **[I]**.
* Text is typed into a text page-object (`RTextPageObject`/`RTextEditFrame`). It can be moved
  later with the moving van's glove until merged.
* Speech uses SAPI (`ASpeechHandler`, voice popup). On other platforms use the system TTS or
  hide it **[I]**.

**RAlphaPanel (Small Kids Mode) [O]**

* Background 18039. A 7-slot scroller (row 18000, arrows 18028, selected 18001+) of letters.
* Each letter is rendered at 48×48 from `user.ini [SmallKidsAlphabet]`:
  * `AlphabetSet` default `ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxzy1234567890,.;?!'@#$%&*/+-=<>()`
    (note the original "xzy" order)
  * `FontName` default `vogel`, `FontSize` default 36
* Picking letter *i* arms a stamp tool (`RStampSimpleBitmap`) with that glyph bitmap and plays the
  letter's voice:

  | Index range | Characters | Sound |
  |---|---|---|
  | i < 26 | A–Z | tWAV 22000+i |
  | 26 ≤ i < 68 | a–z (same voice as the capital), digits 1–9,0, then `, . ; ? ! '` | tWAV i+21974 |
  | i = 68 | `@` | 22092 |
  | 69–71 | `# $ %` | i+22021 (22090–22092) |
  | other | | 22042 |

  Port this mapping verbatim; it was written for that exact string.
* TP tBMP 22000–22081 (48×48 letter tiles: A–Z, 0–9, punctuation, plus a Spanish set with CH, LL,
  Ñ, ¿, ¡) are not referenced by any immediate in the exe. The only `22000` immediate is the
  sound mapping above. They look like leftovers or Spanish-build assets **[I]**.

### 3.7 Moving van (227) – `RCutMovePanel`

**Panel [O]**

* Background 18050, MBTN **926**.
  * Left: 22301 **Grab** glove (91,506)–(188,600), sounds 18040/41/42. 22300 **Cut** scissors
    (194,507)–(292,594), sounds 18043/44/45.
  * Right: 22352 **Trash** (318,526)–(352,588), sound 18068. 22350 **Duplicate** (two smileys)
    (349,523)–(383,554), sound 18059. 22351 **Glue / Merge** (347,553)–(384,588), sound 18060.
    Help ids: TrashCan, Duplicate, Merge.
* Scroller: 6 slots (18020, x from 388), filled only in Cut mode. Grab mode empties it and loops
  18034.
* Default mode: Grab if the page has any movable objects (stickers SWF/`tSWA`, `tSWF`, text
  `tTXT`, movies `tMOV`), otherwise Cut. Scroller index 0.

**Cut items** (`Data/Tools/CutRgns.MHK`, icons tBMP 13000–13013) [O]

| Item | Icon | Mode | Class |
|---|---|---|---|
| 0 | 13000 | free-form lasso | `RCutFree` |
| 1 | 13001 | polygon | `RCutPoly` |
| 2 | 13002 | oval drag | `ROCircleCut` |
| 3 | 13003 | rectangle drag | `RCutSquare` |
| 4–13 | 13004–13013 | fixed shapes from **MRGN** of the same id: square 64×64, rectangle 64×128, circle 128×128, oval 64×128, heart 113×103, house 76×103, pear 107×140, car 221×73, tree 83×103, leaf 82×103 | |

* Picking a shape (item ≥ 4) creates a floating cutter of that shape that follows the cursor; a
  click cuts that shape out at the cursor **[I]**.
* After any cut the piece becomes a floating selection with marching ants (tool 122). It is
  dragged with the glove; Trash deletes it, Duplicate copies it, Glue merges it (and other
  objects) into the picture.
* Copy/paste between pictures uses the clipboard format `"Kidpix Internal Format"` **[O]**.
* No flip in this panel (flip is a mixer effect).
* Sounds: `[ModifierCutFreeSnd]` loop 7313; Circle/Square loop 7314; Poly loop 7314, stop 7315.
  `[CutShapePosSnd]` 7427, `ActiveSnd` 7428, `DragSnd` 7429, `EndSnd` 7430.

**MRGN format [O]**

Big-endian:

```
'rgnR'
u16 left, top, right, bottom     (bounds; shapes start at 0,0)
u16 n, u16 n                     (same count twice)
n × {u16 left, top, right, bottom}   rects, half-open
```

The region is the union of the rects; they are horizontal spans, sometimes several rows high.

### 3.8 Eraser (228) – `REraserPanel`, `REraserTool`, big erasers

**Panel [O]**

* Background 18051. **Left, MBTN 18500:**
  * 18501 small eraser (pencil-top) (93,505)–(188,600), sounds 18046/47/48
  * 18500 big eraser (block) (187,516)–(269,595), sounds 18049/50/51
* **Small eraser** (tool 115):
  * Right panel shows the shape toggle MBTN 18203: 18201 round (382,525)–(408,553), 18202 square
    (405,524)–(431,554), sound 18057.
  * Sizes: MBTN 18200 (balls) when round, 18201 (squares) when square.
  * Loops 18034. Help ctx "EraserSz".
* **Big eraser:** scroller of 5 slots (18024, x from 439) with 15 items, icons Icons.MHK
  tBMP 3700–3714 (3 pages). Selecting slot i arms tool 123+i. Help ctx 899.
* **Defaults:** small eraser, size value 3, round, big-eraser index 0.
* Note: **`Data/Deleters.MHK` has nothing to do with the eraser.** It holds the "Are you sure you
  want to delete this user / these projects?" confirm dialog (`RDeleter`: scripts 7800–7814,
  MBTN 777 = Cancel 1395 / OK 1396, tWAV 7800). That belongs to the picker/dialog spec.

**Small eraser (115) [O]**

* `REraserTool` is `RCircle` with the colour forced to white (255,255,255). It paints white dabs
  of diameter = size along the drag: sizes {2, 10, 20} px.
* Shape 887 = filled oval, 888 = filled rect.
* `[EraserSnd]` has no sound.

**Big erasers [O]**

`Firecracker, ClockFace, IrisHeart, IrisStar, Windmill, PageTear, Spiral, Implode, Engulf, Fade,
Explode, Kaleidoscope, Waterfall, WipeRight, Vanish` = ids 123–137. They all clear the whole
picture to white, with a show:

* **123 Firecracker** (`RDFirecracker`):
  * On click at P, start a timer (period ≈ 120 ms **[I]**).
  * Each tick: grow a rect centred on P by 20 px per side, and paint a white filled oval inscribed
    in it, clipped to the canvas.
  * Once the oval's rect covers the whole canvas, count 7 more ticks, then fill the canvas white
    and finish.
  * Sound `[FirecrackerSnd]` 7219.
* **124–136 QuickTime erasers** (`RQuickTimeEraser`/`RQuickTimeFX`):
  * Run a QuickTime transition from the current picture to an all-white picture, using the effect
    description file `Data/Quicktimefx/<name>.qfx`. Directory = `[Paths] EfxDir`.
  * id → file: 124 "Clock Face", 125 "Iris Heart", 126 "Iris Star", 127 "Windmill",
    128 "Page Tear", 129 "Spiral", 130 "Implode", 131 "Engulf", 132 "Fade", 133 "Explode",
    134 "Kaleidoscope", 135 "Waterfall", 136 "Wipe Right".
  * The other `.qfx` files are used by slide-show transitions.
  * Sounds `[<Name>Snd]`: ClockFace 7227, IrisHeart 7234, IrisStar 7218, Windmill 7244,
    PageTear 7243, Spiral 7224, Implode 7232, Engulf 7228, Fade 7230, Explode 7229,
    Kaleidoscope 7237, Waterfall 7226, WipeRight 7247.
* **137 Vanish** (`RFullCutEraser`): fill the canvas white instantly. No cfg sound section.

**`.qfx` format [O]**

* 12 zero bytes, then a QuickTime atom container. Each atom is
  `u32 size, 4cc type, u32 id, u16 0, u16 childCount, u32 0` followed by data or children.
* Root `'sean'` atom with these children:
  * `'what'` = effect: `smpt`/`smp2`/`smp3`/`smp4` = SMPTE wipe families, `dslv` = dissolve,
    `xplo` = explode, `mplo` = implode, `push`, `slid`, `zoom`
  * `'wpID'` = SMPTE wipe code
  * `'pcnt'` = percentage tween `twnt 3`, from 0.0 to 1.0 (16.16 fixed)
  * `'hori'`/`'vert'` = repeat counts
  * `'soft'` = soft edge
  * `'xcnt'`/`'ycnt'` = explode/implode centre (16.16, can be a tween)
  * `'from'`, `'angl'`, `'zvar'`, `'zdst'`
* Values used by the 13 erasers:

  | File | what | Parameters |
  |---|---|---|
  | Clock Face | smp3 | wpID 201 |
  | Iris Heart | smp2 | wpID 130, soft |
  | Iris Star | smp2 | wpID 128, soft |
  | Windmill | smp3 | wpID 207 |
  | Page Tear | xplo | xcnt tween −2→2, ycnt tween −2→0 |
  | Spiral | smp4 | wpID 310, soft |
  | Implode | mplo | |
  | Engulf | xplo | ycnt tween 2→−2 |
  | Fade | dslv | |
  | Explode | xplo | |
  | Kaleidoscope | smp2 | wpID 102, hori = vert = 3, soft |
  | Waterfall | smp4 | wpID 350 |
  | Wipe Right | smpt | wpID 1 |

* Implement them as SMPTE-style masks (standard SMPTE 258M wipe codes) interpolating 0→1 over
  the effect duration **[I]**. The duration is a field of the eraser object (not decoded) **[I]**.

---

## 4. Colour picker and current-colour splat

**Normal mode [O]**

* The easel draws Easel script **1118**, the wheel, at (0,491)–(85,600). An `RColorPicker` is
  attached to it.
* The **splat**, Easel script **1092** at (17,540)–(63,589), is an animation channel with a custom
  draw callback. It draws Easel **tBMP 7000** (8-bit greyscale splat) through *Tint*
  **multiply mode** with the current colour at the channel's position (§3.3). Small Kids Mode uses
  tBMP 7001.
* Easel 1093 at (74,554)–(93,600) is also drawn; its purpose is unknown (help id
  "FixedColorsToggle" exists **[I]**).

**Opening the big picker (`BigPicker.MHK`, cfg `[ColorPicker]`) [O]**

* Clicking the wheel plays script 1500 (open: 5 frames, sound idx 1 → tWAV 1501). The open state
  is script 1502, a palette panel at (39,381)–(205,600).
* While the button is held or dragged inside the panel:
  * if the mask bitmap `tBMP 1500` (182×553, 8-bit, aligned to screen (0,0)) is 0xFF under the
    cursor, the colour is the **screen pixel under the cursor**, i.e. the cell art itself
  * on every colour change, play `Misc.MHK` tWAV 31504 and broadcast cmd 17647 (0x44EF) with the
    new RGB
* Releasing closes the picker: script 1501, 6 frames, sound idx 0 → tWAV 1500.
* **Eyedropper button:** scripts 1503 rest / 1504 hover (sound idx 2) / 1505 pressed at
  (103,563)–(150,597). It switches to pick-from-canvas mode: the next click or drag takes the
  screen pixel anywhere.
* The current colour is a global RGB888 value. Tools convert it with `>>3` per channel.

**Big picker palette** (cell centres sampled through the mask; rows top→bottom) [O]

* Cell pitch is 14 px. Column x ranges: 63–75, 81–93, 95–107, 109–121, 123–135, 137–149,
  151–163, 169–181.
* Row y ranges: 396–408, 410–423, 425–437, 439–452, 454–466, 468–480, 482–495, 497–509,
  511–524, 526–538, 540–552.
* Cells marked "–" are not colour cells.

```
col:  grey    1      2      3      4      5      6     brown
r0  5a4252 e7ffb5 c6ff63 9cff00 5a9c00 294a00   –      –
r1  ceb5b5 b5ffce 63ff9c 00ff00 00b539 006321 004210   –
r2  527b8c b5efff 63deff 00bdff 008cbd 005a84 002939 211818
r3  a5bdc6 b5ceff 639cff 0052ff 0039bd 002984 00184a 422921
r4  000000 c6b5ff 8c63ff 5218ff 2900bd 180084 100042 7b5242
r5  525252 e7b5ff ce5aff ad00ff 7b00bd 520084 290042 bd7b63
r6  949494 ffb5d6 ff63a5 ff006b bd0052 840031 420018 de846b
r7  cecece ffb5b5 ff6363 ff0000 bd0000 840000 420000 ffad94
r8  fffff7 ffd6b5 ffa563 ff6b00 bd5200 843100 421800 ffcead
r9    –    ffefb5 ffd663 ffbd00 ce9400 846300 523900 ffdece
r10   –    ffffb5 ffff63 ffff00 bdbd00 7b8400 394200 fff7de
```

These are the screen colours of the art. Picking the art pixel keeps them exact; the mask also
admits cell borders that are 0xFF in the mask.

**Small Kids Mode (`LittlePicker.MHK`, `RSmallColorPicker`) [O]**

* Script 1500 replaces the wheel at (0,491)–(85,600). Scripts 1501 (23,500)–(61,508) and
  1502 (12,510)–(32,522) are small parts whose purpose is unknown.
* Mask `tBMP 1500` (71×593). tBMP 1501 is an 800×600 alternate easel background.
* Palette: 3 columns (x 13–30, 33–50, 53–70) × rows at y 511–520 … 583–592.
  * row 1: efefef / 737373 / 000000
  * then pastel / pure / dark triples: red ff9c9c/ff0000/ad0000, orange ffce9c/ff6331/843100,
    yellow ffff9c/ffff00/7b8400, green (light)/00ad00/005200, blue 9cffff/0063ff/0000ef,
    purple ceceff/6300ff/3100ce
  * plus a top white bar at y 500–507 **[O, sampled]**

---

## 5. Corrections to the current engine and docs

* The option panel is not a free-form area at (322,527). Each tool has its own background script
  (§1.2), MBTN tables and one N-slot scroller with fixed positions (§1.4).
* Flood fill: exact colour match, 4-connected (§3.3), not tolerance 24.
* Gradients come from `grad` 21300 (63 stop lists, 3 geometries) fitted to the region's bounding
  box. The 21300 tBMPs are only icons; a fill is not a stretched picture.
* Texture fill blends the current colour over the existing pixel by the texel value (`g == 0`
  keeps the pixel). 20500 = plain solid.
* `Deleters.MHK` is the delete-confirm dialog, not eraser animations.
* `LittlePicker`/`BigPicker` are the colour pickers, not stamp/sticker pickers.
* 8-bit cels in TP tBMH 18000 are shadow alpha maps (§1.3).
* The ABC tool depends on Small Kids Mode. Normal mode is the typed-text panel (no alphabet
  stamps); Small Kids Mode is the alphabet panel.

## 6. Unknowns / open questions

1. Which `[KaleidoscopeSnd]` wins (7237 at cfg line 2362 or 7406 at line 2627)? The exe's INI
   reader (`FUN_004f07b4`) wasn't traced. Both tool 86 (mixer) and tool 134 (eraser) look up the
   same section name. First match is probable.
2. Exact semantics of the MBTN `kind` field (1/3/4), and the precise rest/rollover/press playback
   rules for the animated left-panel buttons (§1.3 is inferred).
3. The modifier key the exe calls 0x20 (taken as Space) that switches mixer effects to their
   alternate variant. Check against the original by pressing Space, Ctrl or Alt during a click.
4. Flip axis assignment (normal = horizontal is inferred).
5. Firecracker timer period, and whether a firecracker sprite also plays (none found in code; the
   effect seen is an expanding white oval).
6. QuickTime eraser duration, and how strictly the SMPTE patterns must match the QuickTime
   originals.
7. Easel script 1093, LittlePicker scripts 1501/1502, and stamp editor button 19002 (face).
8. How text-mode typing interacts with speech beyond "speak all" (no per-letter talking found in
   `RTextPanel`).
9. The radial gradient focus point F is assumed to be the click point.
10. Stamp drag behaviour: one stamp per click, or repeated while dragging (`RCyclic` base)? Not
    traced.
11. TP 18037/18038 (a small dark tab at (433,575)–(599,600), 5-frame slide): owner not identified.

## Appendix A – TP.MHK MBTN tables (generated from the CD)

`val` = field [8], `kind` = [7], `grp` = [14]. Sounds are absolute TP tWAV ids.
"5f" = number of frames.

**MBTN 777**

| # | cmd | val | kind | grp | rest (rect L,T,R,B; frames; snd) | rollover (frames; snd) | press (frames; snd) |
|---|---|---|---|---|---|---|---|
| 0 | 201 | 0 | 1 | 2 | 18265: 362,526,397,560; 1f | 18266: 2f; 18069 | 18267: 1f |
| 1 | 202 | 0 | 1 | 2 | 18262: 389,526,423,557; 1f | 18263: 2f; 18069 | 18264: 1f |
| 2 | 203 | 0 | 1 | 2 | 18268: 422,528,457,558; 1f | 18269: 2f; 18069 | 18270: 1f |
| 3 | 200 | 0 | 1 | 1 | 18248: 348,519,383,588; 1f | 18249: 2f; 18058 | 18250: 1f |
| 4 | 200 | 0 | 1 | 2 | 18251: 529,519,561,588; 1f | 18252: 2f; 18058 | 18253: 1f |
| 5 | 208 | 0 | 1 | 1 | 18254: 316,515,351,552; 1f | 18255: 2f; 18068 | 18256: 1f |
| 6 | 207 | 0 | 1 | 2 | 18271: 474,532,518,579; 1f | 18272: 2f; 18065 | 18273: 1f |
| 7 | 211 | 0 | 1 | 1 | 18258: 316,552,352,588; 1f | 18259: 2f; 18068 | 18260: 1f |

**MBTN 926**

| # | cmd | val | kind | grp | rest (rect L,T,R,B; frames; snd) | rollover (frames; snd) | press (frames; snd) |
|---|---|---|---|---|---|---|---|
| 0 | 22300 | 0 | 1 | 153 | 18131: 194,507,292,594; 7f; 18043 | 18132: 10f; 18044 | 18133: 5f; 18045 |
| 1 | 22301 | 0 | 1 | 153 | 18128: 91,506,188,600; 9f; 18040 | 18129: 9f; 18041 | 18130: 7f; 18042 |
| 2 | 22352 | 0 | 1 | 152 | 18194: 318,526,352,588; 1f | 18195: 2f; 18068 | 18196: 1f |
| 3 | 22350 | 0 | 1 | 152 | 18185: 349,523,383,554; 1f | 18186: 2f; 18059 | 18187: 1f |
| 4 | 22351 | 0 | 1 | 152 | 18188: 347,553,384,588; 1f | 18189: 2f; 18060 | 18190: 1f |

**MBTN 9383**

| # | cmd | val | kind | grp | rest (rect L,T,R,B; frames; snd) | rollover (frames; snd) | press (frames; snd) |
|---|---|---|---|---|---|---|---|
| 0 | 210 | 1 | 4 | 1 | 18096: 109,526,220,552; 1f | 18096: 1f | 0: - |
| 1 | 211 | 1 | 4 | 1 | 18097: 241,526,268,552; 1f | 18097: 1f | 0: - |
| 2 | 200 | 1 | 3 | 1 | 18098: 219,526,238,539; 1f | 18099: 2f; 18003 | 18100: 1f |
| 3 | 201 | 1 | 3 | 1 | 18101: 219,539,238,552; 1f | 18102: 2f; 18003 | 18103: 1f |
| 4 | 202 | 1 | 3 | 1 | 18104: 267,526,286,539; 1f | 18105: 2f; 18003 | 18106: 1f |
| 5 | 203 | 1 | 3 | 1 | 18107: 267,539,286,552; 1f | 18108: 2f; 18003 | 18109: 1f |
| 6 | 204 | 0 | 1 | 2 | 18119: 200,562,226,593; 1f | 18120: 2f; 18039 | 18121: 1f |
| 7 | 206 | 0 | 1 | 2 | 18125: 260,562,286,593; 1f | 18126: 2f; 18039 | 18127: 1f |
| 8 | 205 | 0 | 1 | 2 | 18122: 230,562,256,593; 1f | 18123: 2f; 18039 | 18124: 1f |
| 9 | 207 | 0 | 1 | 5 | 18110: 118,565,136,588; 2f; 18037 | 18111: 2f; 18038 | 18112: 1f |
| 10 | 208 | 0 | 1 | 6 | 18113: 147,565,157,588; 2f; 18037 | 18114: 2f; 18038 | 18115: 1f |
| 11 | 209 | 0 | 1 | 7 | 18116: 170,565,187,593; 2f; 18037 | 18117: 2f; 18038 | 18118: 1f |
| 12 | 240 | 0 | 1 | 3 | 18191: 316,527,352,588; 1f | 18192: 2f; 18068 | 18193: 1f |
| 13 | 241 | 0 | 1 | 3 | 18185: 349,523,383,554; 1f | 18186: 2f; 18059 | 18187: 1f |
| 14 | 242 | 0 | 1 | 3 | 18188: 347,553,384,588; 1f | 18189: 2f; 18060 | 18190: 1f |
| 15 | 232 | 0 | 1 | 3 | 18238: 392,524,438,588; 1f | 18239: 2f; 18066 | 18240: 1f |
| 16 | 231 | 0 | 4 | 3 | 18241: 458,540,569,566; 1f | 18241: 1f | 0: - |
| 17 | 236 | 1 | 3 | 3 | 18242: 568,540,587,553; 1f | 18243: 2f; 18067 | 18244: 1f |
| 18 | 237 | 1 | 3 | 3 | 18245: 568,553,587,566; 1f | 18246: 2f; 18067 | 18247: 1f |

**MBTN 18000**

| # | cmd | val | kind | grp | rest (rect L,T,R,B; frames; snd) | rollover (frames; snd) | press (frames; snd) |
|---|---|---|---|---|---|---|---|
| 0 | 18000 | 0 | 1 | 18000 | 18072: 93,499,165,600; 5f; 18016 | 18073: 11f; 18017 | 18074: 4f; 18018 |
| 1 | 18001 | 0 | 1 | 18000 | 18075: 162,507,236,600; 9f; 18019 | 18076: 12f; 18020 | 18077: 8f; 18021 |
| 2 | 18002 | 0 | 1 | 18000 | 18078: 223,512,304,600; 9f; 18022 | 18079: 10f; 18023 | 18080: 4f; 18024 |

**MBTN 18001**

| # | cmd | val | kind | grp | rest (rect L,T,R,B; frames; snd) | rollover (frames; snd) | press (frames; snd) |
|---|---|---|---|---|---|---|---|
| 0 | 18003 | 0 | 1 | 18001 | 18167: 379,517,432,553; 1f | 18168: 2f; 18058 | 18169: 1f |
| 1 | 18004 | 0 | 1 | 18001 | 18170: 379,552,432,585; 1f | 18171: 2f; 18058 | 18172: 1f |

**MBTN 18002**

| # | cmd | val | kind | grp | rest (rect L,T,R,B; frames; snd) | rollover (frames; snd) | press (frames; snd) |
|---|---|---|---|---|---|---|---|
| 0 | 18006 | 0 | 1 | 18002 | 18173: 380,518,432,553; 1f | 18174: 2f; 18058 | 18175: 1f |
| 1 | 18005 | 0 | 1 | 18002 | 18176: 380,552,432,585; 1f | 18177: 2f; 18058 | 18178: 1f |

**MBTN 18003**

| # | cmd | val | kind | grp | rest (rect L,T,R,B; frames; snd) | rollover (frames; snd) | press (frames; snd) |
|---|---|---|---|---|---|---|---|
| 0 | 18007 | 0 | 1 | 18003 | 18179: 380,518,432,552; 1f | 18180: 2f; 18058 | 18181: 1f |
| 1 | 18008 | 0 | 1 | 18003 | 18182: 380,552,432,585; 1f | 18183: 2f; 18058 | 18184: 1f |

**MBTN 18200**

| # | cmd | val | kind | grp | rest (rect L,T,R,B; frames; snd) | rollover (frames; snd) | press (frames; snd) |
|---|---|---|---|---|---|---|---|
| 0 | 18200 | 1 | 1 | 18200 | 18140: 342,520,366,536; 1f | 18141: 2f; 18052 | 18142: 1f |
| 1 | 18200 | 2 | 1 | 18200 | 18143: 342,536,368,555; 1f | 18144: 2f; 18053 | 18145: 1f |
| 2 | 18200 | 3 | 1 | 18200 | 18146: 335,555,373,588; 1f | 18147: 2f; 18054 | 18148: 1f |

**MBTN 18201**

| # | cmd | val | kind | grp | rest (rect L,T,R,B; frames; snd) | rollover (frames; snd) | press (frames; snd) |
|---|---|---|---|---|---|---|---|
| 0 | 18200 | 1 | 1 | 18201 | 18150: 346,522,359,536; 1f | 18151: 2f; 18055 | 18152: 1f |
| 1 | 18200 | 2 | 1 | 18201 | 18153: 343,535,365,557; 1f | 18154: 2f; 18056 | 18155: 1f |
| 2 | 18200 | 3 | 1 | 18201 | 18156: 335,555,373,588; 1f | 18157: 2f; 18054 | 18158: 1f |

**MBTN 18202**

| # | cmd | val | kind | grp | rest (rect L,T,R,B; frames; snd) | rollover (frames; snd) | press (frames; snd) |
|---|---|---|---|---|---|---|---|
| 0 | 18200 | 1 | 1 | 18202 | 18226: 342,520,366,536; 1f | 18227: 2f; 18062 | 18228: 1f |
| 1 | 18200 | 2 | 1 | 18202 | 18229: 342,536,368,555; 1f | 18230: 2f; 18063 | 18231: 1f |
| 2 | 18200 | 3 | 1 | 18202 | 18232: 335,555,373,588; 1f | 18233: 2f; 18064 | 18234: 1f |

**MBTN 18203**

| # | cmd | val | kind | grp | rest (rect L,T,R,B; frames; snd) | rollover (frames; snd) | press (frames; snd) |
|---|---|---|---|---|---|---|---|
| 0 | 18201 | 0 | 1 | 18203 | 18160: 382,525,408,553; 1f | 18161: 2f; 18057 | 18162: 1f |
| 1 | 18202 | 0 | 1 | 18203 | 18163: 405,524,431,554; 1f | 18164: 2f; 18057 | 18165: 1f |

**MBTN 18300**

| # | cmd | val | kind | grp | rest (rect L,T,R,B; frames; snd) | rollover (frames; snd) | press (frames; snd) |
|---|---|---|---|---|---|---|---|
| 0 | 18300 | 1 | 3 | 3 | 18081: 81,497,174,600; 10f; 18025 | 18082: 9f; 18026 | 18083: 6f; 18027 |
| 1 | 18301 | 1 | 3 | 3 | 18084: 165,519,233,600; 7f; 18028 | 18085: 9f; 18029 | 18086: 4f; 18030 |
| 2 | 18302 | 1 | 3 | 3 | 18087: 216,513,297,600; 9f; 18025 | 18088: 6f; 18026 | 18089: 4f; 18027 |

**MBTN 18400**

| # | cmd | val | kind | grp | rest (rect L,T,R,B; frames; snd) | rollover (frames; snd) | press (frames; snd) |
|---|---|---|---|---|---|---|---|
| 0 | 18400 | 0 | 1 | 18400 | 18060: 90,514,157,599; 10f; 18004 | 18061: 11f; 18005 | 18062: 6f; 18006 |
| 1 | 18400 | 1 | 1 | 18400 | 18063: 135,500,204,599; 8f; 18007 | 18064: 13f; 18008 | 18065: 6f; 18009 |
| 2 | 18400 | 2 | 1 | 18400 | 18066: 157,512,243,599; 9f; 18010 | 18067: 8f; 18011 | 18068: 5f; 18012 |
| 3 | 18400 | 3 | 1 | 18400 | 18069: 232,515,301,600; 10f; 18013 | 18070: 9f; 18014 | 18071: 6f; 18015 |
| 4 | 18450 | 0 | 1 | 18401 | 18201: 450,526,505,551; 1f | 18202: 2f; 18061 | 18203: 1f |
| 5 | 18451 | 0 | 1 | 18401 | 18204: 450,557,507,574; 1f | 18205: 2f; 18061 | 18206: 1f |
| 6 | 18452 | 0 | 1 | 18401 | 18207: 517,526,565,549; 1f | 18208: 2f; 18061 | 18209: 1f |
| 7 | 18453 | 0 | 1 | 18401 | 18210: 518,557,564,579; 1f | 18211: 2f; 18061 | 18212: 1f |
| 8 | 18454 | 0 | 1 | 18401 | 18213: 578,523,629,549; 1f | 18214: 2f; 18061 | 18215: 1f |
| 9 | 18455 | 0 | 1 | 18401 | 18216: 579,556,629,579; 1f | 18217: 2f; 18061 | 18218: 1f |
| 10 | 18456 | 0 | 1 | 18401 | 18219: 639,527,697,550; 1f | 18220: 2f; 18061 | 18221: 1f |
| 11 | 18457 | 0 | 1 | 18401 | 18222: 640,557,695,579; 1f | 18223: 2f; 18061 | 18224: 1f |
| 12 | 18458 | 0 | 1 | 18401 | 18198: 387,532,439,578; 1f | 18199: 2f; 18061 | 18200: 1f |

**MBTN 18500**

| # | cmd | val | kind | grp | rest (rect L,T,R,B; frames; snd) | rollover (frames; snd) | press (frames; snd) |
|---|---|---|---|---|---|---|---|
| 0 | 18500 | 0 | 1 | 18500 | 18137: 187,516,269,595; 4f; 18049 | 18138: 4f; 18050 | 18139: 6f; 18051 |
| 1 | 18501 | 0 | 1 | 18500 | 18134: 93,505,188,600; 8f; 18046 | 18135: 7f; 18047 | 18136: 5f; 18048 |

**MBTN 18600**

| # | cmd | val | kind | grp | rest (rect L,T,R,B; frames; snd) | rollover (frames; snd) | press (frames; snd) |
|---|---|---|---|---|---|---|---|
| 0 | 18600 | 0 | 1 | 18600 | 18090: 98,506,243,600; 7f; 18031 | 18091: 8f; 18032 | 18092: 8f; 18033 |
| 1 | 18601 | 0 | 1 | 18600 | 18093: 239,507,301,600; 5f; 18034 | 18094: 10f; 18035 | 18095: 7f; 18036 |

**MBTN 18700**

| # | cmd | val | kind | grp | rest (rect L,T,R,B; frames; snd) | rollover (frames; snd) | press (frames; snd) |
|---|---|---|---|---|---|---|---|
| 0 | 18702 | 1 | 4 | 18700 | 18053: 114,545,249,571; 1f | 18053: 1f | 0: - |
| 1 | 18703 | 1 | 1 | 18700 | 18054: 248,545,280,558; 1f | 18055: 2f; 18003 | 18056: 1f |
| 2 | 18704 | 1 | 1 | 18700 | 18057: 248,558,280,571; 1f | 18058: 2f; 18003 | 18059: 1f |
| 3 | 18701 | 0 | 1 | 18700 | 18235: 384,525,433,588; 1f | 18236: 2f; 18065 | 18237: 1f |

**MBTN 18900**

| # | cmd | val | kind | grp | rest (rect L,T,R,B; frames; snd) | rollover (frames; snd) | press (frames; snd) |
|---|---|---|---|---|---|---|---|
| 0 | 18801 | 1 | 4 | 20 | 18053: 114,545,249,571; 1f | 18053: 1f | 0: - |
| 1 | 18802 | 1 | 1 | 20 | 18054: 248,545,280,558; 1f | 18055: 2f; 18003 | 18056: 1f |
| 2 | 18803 | 1 | 1 | 20 | 18057: 248,558,280,571; 1f | 18058: 2f; 18003 | 18059: 1f |

## Appendix B – how these facts were found (for re-checking)

* Panel switcher: `FUN_00411b90` (switch on easel cmd 0xDE–0xED). Tool factory:
  `Rembrandt_RAlphaPanel__v17` (switch on tool id). Tool-name table at VA 0x5541B4 (138 entries).
* Panel init (`…Panel__v6`): background script via `FUN_0048f5c0(id)`, MBTN via vtable +0x30,
  sizes/extra MBTN via +0x34, scroller via `RBrushSubPanel__v21(slotScript)`. Ghidra drops the
  immediate arguments of virtual calls, so they were read from the disassembly (r2).
* Fill: `RPatternFill__v27/28`, `RFill__v25/26`, `FUN_00441e40` (ExtFloodFill), tint
  `FUN_00459930`, gradient `RGradientFill__v4/v11`, `FUN_0043e7a0`, `FUN_0043e950`,
  `FUN_0043ebd0`, `FUN_0043efc0`, `FUN_0043ef40`.
* Mixer: per-pixel `R<Effect>__v37`, polar remaps `R<Effect>__v34`, loop `RBubbles__v35`, HSV
  `FUN_00447ce0`/`FUN_00447e10`, start `RWackyEffect__v5`. Constants: 0x58DD80 = 2π (set at
  startup), 0x554168 = π.
* Eraser: `REraserPanel__v1/v14`, `FUN_00498300` (big eraser factory), QT table at VA 0x583AF8
  (13 × {i16 id+1, i16, char* name}), `RDFirecracker` `FUN_0043c180`.
* Colour picker: `RColorPicker__v1/v2/v5/v6/v7`, `FUN_0040ae00` (set colour + broadcast 0x44EF),
  splat draw callback at 0x412A00.
