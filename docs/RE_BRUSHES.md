# Kid Pix Deluxe 3 — brush / Wacky Brush implementation spec

Reverse-engineered from the user's own KPD3.exe (Win32, 2000) plus the CD data
(`KpInstall/KPD3.cfg`, `Data/Tools/*.MHK`). Written for the Rust engine; algorithms are
paraphrased, constants are exact. Addresses (`@0x…`) point into KPD3.exe for anyone who
wants to re-check a detail.

Confidence tags used throughout:

* **[O]** observed directly in decompiled code or disassembly.
* **[I]** inferred (naming, data shape, or a likely reading of partially decompiled code).
* **[?]** not determined; needs more RE or a side-by-side run of the original.

Colour conventions: the canvas is RGB555. "5-bit" means a channel 0..31. The current
palette colour is a 24-bit RGB triple (`DAT_0058dc60`). Brushes turn it into 5-bit with
`c5 = (31*c8 + 127) / 255` (integer division, `@0x4597a0`) **[O]**. `trunc()` means C
`(int)` truncation toward zero (MSVC `__ftol`), `round(x)` means `trunc(x + 0.5)`.

---

## 1. Tool IDs and where each brush lives

Every drawing tool is created by one factory, `@0x49c590`, from a numeric **tool id**.
The id also indexes the name table `@0x5541b4`, and that name is the KPD3.cfg section
name (`[Name]` for parameters, `[NameSnd]` for sounds) **[O]**.

| id | name / class | id | name / class |
|---|---|---|---|
| 0 | RealBrush / RRealBrush | 45–54 | CustomVoiceBrush1–10 / RCustomVoiceBrushN |
| 1 | Airbrush / RAirbrush | 55–64 | Touchless1–10 / RTouchlessN (hands-free) |
| 2 | BristleBrush / RBristleBrush | 65–86 | Electric Mixer full-picture effects (XRay … Kaleidoscope) |
| 3 | ExtrusionBrush / RExtrusionBrush | 87 | SmearBrush / RSmearBrush |
| 4–19 | CustomBrush1–16 / RCustomBrushN | 88 | BlurBrush / RBlurBrush |
| 20 | Madona / RMadonna | 89 | GreyscaleBrush |
| 21 | Drippy / RDrippy | 90 | ColorizeBrush |
| 22 | Spiral / RSpiral | 91 | XRayBrush |
| 23 | Poly / RPoly | 92 | EmbossBrush |
| 24 | TriSpin / RTriSpin | 93 | EdgeBrush |
| 25 | WhispyCircle / RWhispyCircle | 94 | NuclearBrush |
| 26 | Particle / RParticle | 95 | SolarizeBrush |
| 27 | Garland / RGarland | 96 | HueShiftBrush |
| 28 | LeakyPen / RLeakyPen | 97 | PosterizeBrush |
| 29 | ScratchyPen / RScratchyPen | 98 | MosaicBrush |
| 30 | LoopingLine / RLoopingLine | 99 | Circle / RCircle (plain Pencil) |
| 31 | LoopingLine2 / RLoopingLine (same class) | 100 | Chalk / RChalk |
| 32 | Bricks / RBricks | 101 | Crayon / RCrayon |
| 33 | Log / RLog | 102 | Marker / RMarker |
| 34 | Window / RWindow | 103–110 | ModifierLine … ModifierFPoly (pencil shapes) |
| 35 | Door / RDoor | 111–114 | ModifierCut* (move/cut tool) |
| 36 | DoorPlain / RDoorPlain | 115 | Eraser (RCircle subclass) |
| 37 | Roof / RRoof | 116–118 | FillSolid / FillPattern / FillGradient |
| 38 | DotToDot / RDotToDot | 119 | SprayGun / RSprayGun (spray "moving paint") |
| 39 | ABC / RABC | 120 | SimpleBitmap |
| 40 | Rainbow / RRainbow | 121 | CyclicBitmap / RCyclic (spray "shapes") |
| 41 | ColorTile / RColorTile | 122 | MarchingAnts |
| 42 | 8WayMirror / R8WayMirror | | |
| 43 | Splat / RSplat | | |
| 44 | Tree / RTree | | |

Classes that exist in the binary but are never created by the factory: `RVoiceBrush`,
`RHandsFreeBrush` (base classes only), `RPixelFXBrush`, `RWackyEffectBrush`,
`RWackyToolPanel` (its init passes tray script "1", looks dead), `[MedianBrush]`,
`[SepiaBrush]`, `[SimpleVoiceBrush]` cfg sections **[O]**.

**Important for the current engine:** blur, mosaic, x-ray etc. are **not** in the Wacky
Brush list. They are the Electric Mixer's *Mixer Wand* (Mixer tool → wand mode). The real
Wacky Brush list is ids 20–44 **[O]**.

### 1.1 Panels and option lists

The Easel "paint brush" button (cmd 233) opens `RRealMediaToolPanel` ("RealMediaPanel"
prefs). Its first row (TP.MHK MBTN 18000, radio group 18000) has three modes **[O]**:

| cmd | button art (script) | mode | sub-toggle (second MBTN) |
|---|---|---|---|
| 18000 | paint brush (18072) | **Paint Brush** | MBTN 18001: 18003 = realistic (splat icon, script 18167), 18004 = **wacky** (shapes icon, 18170) |
| 18001 | microphone (18075) | **Sound Art** | MBTN 18002: 18006 = voice + mouse (mouse icon, 18173), 18005 = hands-free (crossed-out mouse, 18176) |
| 18002 | spray can (18078) | **Spray Can** | MBTN 18003: 18007 = shapes (gems icon, 18179), 18008 = moving paint (caterpillar, 18182) |

Sub-panel tool ranges (`v22`/`v23` = first id, end id) **[O]**:

| mode | sub-panel class | ids | count |
|---|---|---|---|
| Paint Brush / realistic (18003) | RBrushSubPanel | 0–19 | 20 |
| Paint Brush / wacky (18004) | RBrushSubPanel | **20–44** | **25** |
| Sound Art / mouse (18006) | RVoiceSubPanel | 45–54 | 10 |
| Sound Art / hands-free (18005) | RVoiceSubPanel | 55–64 | 10 |
| Spray / shapes (18007) | RRollerSubPanel ("Cyclic") | Cyclic.MHK sets | 15 |
| Spray / moving (18008) | RSpraySubPanel ("RSprayPanel") | Anims.MHK sets | 20 |
| Mixer / big mixer (cmd 18600) | RMixerPanel | 65–86 | 22 |
| Mixer / wand (cmd 18601) | RMixerPanel | 87–98 | 12 |

Defaults when no prefs exist: main mode 0 (Paint Brush), size index 1, sub-modes
18003 / 18006 / 18007, selected item 0 in every list **[O]** (`@0x49b100`). The panel keeps
one selected index per list (6 slots: realistic, wacky, voice, hands-free, shapes,
moving) and restores it when you switch back **[O]**.

**Option tray layout** **[O]**: the brush/voice/spray sub-panels call the tray builder with
script 18027 (`0x466b`), a 5-slot tray at x=439..695, y=525..577 (each slot 51 px). Slot
frames are scripts 18003–18007 (normal) and 18010–18014 (selected); scroll arrows are
scripts 18028–18033 (commands 18350 up / 18351 down). **5 icons are visible**, an arrow
press scrolls by a whole page (step = visible count, `@0x42b250`) and **wraps**
(`+0x1ee = 1`): past the end it returns to 0, before 0 it jumps to the last page start.
So the Wacky list shows pages starting at 0, 5, 10, 15, 20.

Each slot's icon = the tool's `v13()` id loaded as `tBMP` from the ToolPanels file set
(TP.MHK + **Icons.MHK**; the ids only exist in Icons.MHK), drawn centred in the slot with
transparent mode 8 **[O]** (`@0x49ba80`, `@0x404380`).

### 1.2 The Wacky Brush list (exact order)

Order = id order. Page = 5 items. "Sizes" = small/medium/large values selected by the
three size buttons (MBTN 18200, `0x4718` message, 1..3). Sound section = `[<name>Snd]`.

| # | id | name | Icons.MHK tBMP | class (base) | sizes | sound section |
|---|---|---|---|---|---|---|
| 1 | 20 | Madona (pyramid) | 3520 | RMadonna (drag tool) | 10/20/40 | MadonaSnd |
| 2 | 21 | Drippy | 3521 | RDrippy (pad engine) | pad 1013–1015 | DrippySnd |
| 3 | 22 | Spiral | 3522 | RSpiral (timed) | 5/10/20 (unused) | — (no section) |
| 4 | 23 | Poly | 3523 | RPoly (drag) | 5/10/20 (unused) | PolySnd |
| 5 | 24 | TriSpin | 3524 | RTriSpin (OpenGL) | 5/10/20 | TriSpinSnd |
| 6 | 25 | WhispyCircle | 3525 | RWhispyCircle (OpenGL) | see §5.6 | WhispyCircleSnd |
| 7 | 26 | Particle | 3526 | RParticle (OpenGL) | 5/10/50 | ParticleSnd |
| 8 | 27 | Garland (starburst) | 3527 | RGarland (timed) | 10/20/40 | GarlandSnd |
| 9 | 28 | LeakyPen | 3528 | RLeakyPen (timed) | 2/5/10 | LeakyPenSnd |
| 10 | 29 | ScratchyPen | 3529 | RScratchyPen | 5/10/20 | ScratchyPenSnd |
| 11 | 30 | LoopingLine | 3530 | RLoopingLine | 5/10/20 | LoopingLineSnd |
| 12 | 31 | LoopingLine2 (fern) | 3531 | RLoopingLine | 5/10/20 | LoopingLine2Snd |
| 13 | 32 | Bricks | 3532 | RBricks (aligned stamp) | — | BricksSnd |
| 14 | 33 | Log | 3533 | RLog (aligned stamp) | — | LogSnd |
| 15 | 34 | Window | 3534 | RWindow (drag) | — | WindowSnd |
| 16 | 35 | Door | 3535 | RDoor (drag) | — | DoorSnd |
| 17 | 36 | DoorPlain | 3536 | RDoorPlain (drag) | — | DoorPlainSnd |
| 18 | 37 | Roof | 3537 | RRoof (drag, OpenGL texture) | — | RoofSnd |
| 19 | 38 | DotToDot | 3538 | RDotToDot (timed) | — | DotToDotSnd |
| 20 | 39 | ABC | 3539 | RABC | font 16/24/32 | ABCSnd |
| 21 | 40 | Rainbow | 3540 | RRainbow (drag, OpenGL) | — | RainbowSnd |
| 22 | 41 | ColorTile | 3541 | RColorTile (drag, OpenGL) | — | ColorTileSnd |
| 23 | 42 | 8WayMirror | 3542 | R8WayMirror | — | 8WayMirrorSnd |
| 24 | 43 | Splat | 3543 | RSplat (timed) | 1/2/3 | SplatSnd |
| 25 | 44 | Tree | 3544 | RTree | — | TreeSnd |

Icons, sizes, class and order: **[O]**. The 3521 Drippy icon id comes from `RDrippy::v13`
(returns 0xdc1) **[O]**. Section names `[ConfettiSnd] [DotsSnd] [SolarizerSnd] [TicTacToeSnd]`
exist in the cfg but no tool uses them **[O]**.

### 1.3 Realistic Paint Brush list (ids 0–19, 4 pages)

| # | id | name | icon | class | KPD3.cfg |
|---|---|---|---|---|---|
| 1 | 0 | RealBrush | 3500 | RRealBrush | `[RealBrush] brushID=1004` |
| 2 | 1 | Airbrush | 3501 | RAirbrush | brushID 1001, spacing 0, densityBias 4, spatter 1, oscillation 2, shadowOffset 0, shadowBias 7 |
| 3 | 2 | BristleBrush | 3502 | RBristleBrush | brushID 1022, spacing 1, densityBias 1, dryOutRate 20, smearBias 7, shadowBias 10 |
| 4 | 3 | ExtrusionBrush | 3503 | RExtrusionBrush | brushID 1013 |
| 5–20 | 4–19 | CustomBrush1–16 | 3504–3519 | RCustomBrushN (= plain pad engine) | `[CustomBrushN]` (see §3.8) |

The cfg's `[Tray1.1]…[Tray4.5]` headers in front of the `…Snd` sections are just
comments: Tray p.s = page p, slot s of this list (5 per page) **[O]** (matches ids).
Icons 3500–3519 are 8-bit RLE (fmt 0x12) intensity maps (0 = background, up to 255);
Icons.MHK also has `tPAL 3500` **[O]**. How they are tinted on screen is **[?]** (they look
right rendered as grey intensity; rendering through tPAL 3500 gives noise).

### 1.4 Mixer Wand list (ids 87–98)

Smear 3800, Blur 3801, Greyscale 3802, Colorize 3803, XRay 3804, Emboss 3805, Edge 3806,
Nuclear 3807, Solarize 3808, HueShift 3809, Posterize 3810, Mosaic 3811 (Icons.MHK) **[O]**.
The Big Mixer effects (65–86) have icons 3900–3921 **[O]** (not covered here).

### 1.5 Sound Art lists

CustomVoiceBrush1–10 → icons 3547–3556; Touchless1–10 → icons 3557–3566 **[O]**.
(3545/3546 are unreferenced extra art.)

---

## 2. Common tool runtime

### 2.1 Event flow (RToolMgr) **[O]**

* **Mouse down** (`RToolMgr::v5`): stores the current colour, calls `tool.v5(button, point,
  params)` where `params` carries the canvas rect and the panel's size as a float
  (1, 2 or 3), then `tool.v18()` (start sound).
* **While the button is held**, every idle tick (`RToolMgr::v1 @0x445870`) the manager
  reads the mouse position and calls `tool.v20(point)` (sound update) and then
  `tool.v8(point)` — **even if the mouse did not move**. Tools decide what "still" means.
  The tick rate is the app's idle loop, not a fixed timer **[I]**.
* **Mouse up** (`RToolMgr::v6`): `tool.v6(point)`, then `tool.v19()` (stop sounds), then
  undo bookkeeping.
* Base `v8`: if the point changed, call `v11(point)` (default: `v14(point)`), else nothing.

Tool families **[O]**:

* **RTimedTool** (Spiral, Garland, LeakyPen, DotToDot, Splat): `v4` sets interval 50 ms
  (subclasses override); `v5` sets `next = now + interval`; `v8` runs the base move logic
  and, if `now > next`, calls `v14(current point)` and `next += interval`. So timed
  brushes keep working while the mouse is still.
* **RDragTool** (Madona, Poly, Window, Door, DoorPlain, Roof, Rainbow, ColorTile): at mouse
  down the whole canvas is copied to an off-screen *snapshot*; every move restores the
  snapshot and redraws the shape from the start point to the current point (rubber band).
  Mouse up leaves the last drawing. "Pt2Rect" = rect spanned by start and current point.
* **RGLTool** (TriSpin, WhispyCircle, Particle; Rainbow/ColorTile/Roof/Door also use GL):
  OpenGL renders into the canvas DC through a viewport of 800×600 covering the canvas
  rect (`@0x4c1060`). For the Rust port these are plain alpha-blended polygon/disk
  rasterisations.

### 2.2 Sizes **[O]**

`v0` fills a 3-entry size table, default `{5, 10, 20}`; `v5` calls `v10(trunc(size) - 1)`
which copies entry 0/1/2 to the brush size `0x2a`. Per-class tables are in §1.2.

### 2.3 Modifier keys **[O]**

`GetAsyncKeyState` wrapper `@0x507d79`: arg 1 = **Ctrl**, 2 = **Alt**, 4 = **Shift**,
0x20 = **Space**. Brushes read them live per stamp; see each brush.

### 2.4 Sounds **[O]** (`v21 @0x445140`, `v18/v19/v20`)

For tool name *N*, section `[NSnd]` in **KPD3.cfg** is read: booleans `continuous`,
`lingers`; ids `startSoundID stopSoundID loopSoundID specialFxID directionNorth
directionEast directionSouth directionWest` (+ unused `fastSound cruiseSound slowSound
bigSound mediumSound smallSound`). A value of 0 / "off" means none. Ids are `tWAV` in the
opened archives (WackyBrushSounds.MHK 7200–7432). Behaviour:

* **Mouse down** (`v18`, only if a start sound exists): stop the loop and the fx sound,
  play `start`.
* **Every tick** (`v20(point)`): stop `stop` if it is still playing. If there is a loop
  sound: wait while `start` is still playing; if `continuous=on` keep the loop playing
  regardless of movement; otherwise play the loop while the mouse moves, and stop it
  when the mouse has been still for more than **50 ms**, resuming on the next movement.
  Direction sounds: compare `dx`, `dy` with the previous point. If `|dx| > |dy|`: `dx > 0`
  plays **East**, `dx < 0` plays **West**; else `dy > 0` plays **North**, `dy < 0` plays
  **South** (yes: "North" fires when moving *down* the screen). A direction sound only
  restarts when the direction changes; starting one stops the other three.
* **Mouse up** (`v19`): unless `lingers=on`, stop all direction sounds, start, loop and fx;
  then play `stop`.
* **specialFxID** is played by the brushes themselves per event (e.g. each brick, each
  drip, each letter) if it is not already playing.
* Whether a sound loops is a property of the tWAV: loop sounds have `loopCount = 65535`
  in the `Data` header (e.g. 7256, 7263, 7279, 7343, 7362), one-shots have 0.
* KPD3.cfg contains a few duplicate sections (`[BlurBrushSnd]` twice). Which copy wins
  depends on the INI reader **[?]** (assume first).

Per-brush sound table (from KPD3.cfg, **[O]** data):

| brush | start | loop | stop | fx | N / E / S / W | continuous |
|---|---|---|---|---|---|---|
| Madona | – | 7374 | 7375 | – | – | on |
| Drippy | – | 7343 | – | 7380 (each drip) | – | off |
| Poly | – | 7318 | – | – | – | on (`pitchbend=on`, unused by code **[?]**) |
| TriSpin | – | 7369 | – | – | – | off |
| WhispyCircle | – | 7356 | – | – | – | off |
| Particle | 7361 | 7362 | 7363 | – | – | on |
| Garland | – | 7377 | – | – | – | on |
| LeakyPen | – | 7365 | – | – | – | on |
| ScratchyPen | – | 7385 | – | – | – | on |
| LoopingLine | – | 7387 | 7388 | – | – | off |
| LoopingLine2 | – | 7364 | – | – | – | off |
| Bricks | – | – | – | 7353 (each brick) | – | off |
| Log | – | – | – | 7371 (each log) | – | off |
| Window | – | 7355 | – | – | – | off |
| Door | – | 7378 | – | – | – | off |
| DoorPlain | – | 7357 | – | – | – | off |
| Roof | – | 7376 | – | – | – | off |
| DotToDot | – | – | – | 7384 (each dot) | – | off |
| ABC | – | – | – | 7431 (each letter) | – | off |
| Rainbow | – | 7432 | – | – | – | on |
| ColorTile | – | 7373 | – | – | – | off |
| 8WayMirror | – | 7382 | – | – | – | off |
| Splat | – | 7386 | – | – | – | on |
| Tree | 7379 | – | – | – | – | off |
| Spiral | (no section → silent) | | | | | |
| RealBrush | 7289 | – | – | – | 7295 / 7292 / 7296 / 7293 | off |
| Airbrush | – | 7279 | 7325 | – | – / – / 7322 / 7322 | on |
| BristleBrush | 7302 | 7305 | – | – | 7299 / 7299 / 7300 / 7300 | off |
| ExtrusionBrush | 7288 | 7343 | 7359 | – | – | off |
| CustomBrush1 | – | 7326 | 7327 | – | 7322 / – / – / 7322 | off |
| CustomBrush2 | – | 7349 | 7348 | – | – | off |
| CustomBrush3 | – | 7349 | 7350 | – | – | off |
| CustomBrush4 | – | 7281 | 7329 | – | 7322 / 7322 / – / – | off |
| CustomBrush5 | – | 7338 | 7339 | – | – | off |
| CustomBrush6 | – | 7332 | – | – | – | off |
| CustomBrush7 | – | 7334 | 7335 | – | 7322 / – / 7322 / – | off |
| CustomBrush8 | – | 7336 | 7337 | – | 7322 / 7322 / – / – | off |
| CustomBrush9 | – | 7338 | 7339 | – | 7322 / 7322 / – / – | off |
| CustomBrush10 | – | 7340 | – | – | – / – / 7322 / 7322 | on |
| CustomBrush11 | – | 7345 | 7346 | – | – / – / 7322 / – | off |
| CustomBrush12 | – | 7341 | 7342 | – | 7322 / – / – / – | off |
| CustomBrush13 | – | 7321 | 7323 | – | 7322 all four | off |
| CustomBrush14 | – | 7328 | 7329 | – | – | off |
| CustomBrush15 | – | 7324 | 7325 | – | – | off |
| CustomBrush16 | – | 7332 | – | – | – | on |
| Marker | 7273 | 7274 | – | – | 7275 / 7277 / 7276 / 7278 | off |
| Crayon | 7269 | 7271 | – | – | 7267 / 7267 / 7268 / – | off |
| Chalk | 7259 | 7263 | – | – | 7257 / – / 7258 / 7258 | off |
| Circle (pencil) | 7286 | 7287 | – | – | 7283 / 7285 / 7284 / – | off |
| SmearBrush | 7295 | – | – | – | 7295 / 7296 / 7297 / 7298 | off |
| BlurBrush | 7255 | 7256 | – | – | 7251 / 7253 / 7252 / 7254 (1st section) | – |
| Greyscale/Colorize/XRay | 7255 | 7256 | – | – | 7251 / – / – / 7252 | off |
| Emboss/Edge/Nuclear/Solarize/HueShift/Posterize/Mosaic | 7255 | 7256 | 7317 | – | 7251 / – / – / 7252 | off |

Note `[GreyScaleBrushSnd]` is spelled with a capital S while the tool name is
`GreyscaleBrush`; whether the lookup is case-insensitive is **[?]**. The voice brushes
have empty sound sections (silent).

`FeltPenSnds.MHK` (`[RealMediaSounds]`, tWAV 1000–1007) is not referenced by any brush
code I traced **[?]**.

---

## 3. The real-media pad engine (RRealMedia and subclasses)

Used by: RealBrush, Airbrush, Bristle, Extrusion, CustomBrush1–16, Chalk, Crayon, Marker,
Drippy, Smear, Blur, Greyscale, Colorize, all Wand effect brushes, voice and hands-free
brushes.

### 3.1 Configuration **[O]** (`@0x44de20`, defaults from ctor `@0x44c4c0`)

| key (field) | default | meaning in code |
|---|---|---|
| brushID (0x158) | 1001 | first pad bitmap (Brushes.MHK tBMP) |
| spacing (0x15a) | 1 | stamp distance in px; 0 = "spray" mode (§3.3) |
| spatter (0x15c) | 0 | pixel drop-out (§3.5) |
| densityBias (0x15e) | 1 | opacity divisor (§3.6) |
| textureBias (0x160) | 1 | texture influence divisor (§3.7) |
| smearBias (0x162) | 0 | colour pick-up in % (§3.7) |
| dryOutRate (0x164) | 0 | ≠0 → pad is reloaded on every mouse down (§3.4) |
| dryOutThreshold (0x166) | 0 | not read by the pad engine **[O]** |
| oscillation (0x168) | 0 | random offset radius factor (§3.5) |
| shadowOffset/shadowBias (0x16a/0x16c) | 0 | dark copy (§3.6) |
| glintOffset/glintBias (0x16e/0x170) | 0 | light copy (§3.6) |

Values are 16-bit signed; the parser accepts trailing `;` and `//` comments, keys are
looked up in `[<tool name>]`. The voice brushes add `frequencyBias`, `amplitudeBias`,
the hands-free ones `accelerationBias` (the cfg spells it `acceleratonBias`, so it is never
found and keeps its default), `gravityBias`, `gravityTime`, `gravityRange`, `dampBias`,
plus a key at `PTR_DAT_0058188c` (probably `wrap`) **[O]**/**[I]**. `maxMovement` is read
from KPD3.ini for every tool (absent → 0x7fff) **[O]**.

**BlurBrush quirk** **[O]**: its constructor passes tool id 0, so `this.id = 0` and it reads
`[RealBrush]` (brushID 1004 → pads 1004–1006) and `[RealBrushSnd]`, not `[BlurBrush]`.

### 3.2 Pads **[O]** (`v26 @0x44dc90`, `v5 @0x44d090`)

* Size index `s = trunc(panel size)` ∈ {1,2,3}. Pad id = `(brushID != 0 ? brushID : 1001)
  + s - 1`. So Brushes.MHK holds 20 triplets (small/medium/large), 1001–1060.
* The pad is drawn into a square 8-bit buffer whose side is `max(w, h)` (the bitmap at
  (0,0), rest 0). The brush size `0x2a` becomes that side length.
* Pad byte = coverage `a`, 0 = untouched. 8-bit pads are copied raw (no CLUT).
* Pads 1008, 1009, 1021 are 16-bit sprites (white square, black disc). They are drawn into
  the 8-bit buffer by GDI colour mapping; with a Mac-style palette (index 0 white) that
  gives `a ≈ 255 - luminance` **[I]**. They are only used as medium/large of brushID 1007
  (CustomBrush8) and large of 1019 (CustomBrush9).
* The pad is (re)loaded at mouse down when `dryOutRate != 0` or the size changed.

### 3.3 Stroke sampling **[O]** (`v3 @0x44cf50`, `v8 @0x44cfa0`, line walker `v15 @0x4448c0`)

* `v3`: clip rect = canvas; if cfg spacing ≠ 0, `step = spacing`, else `step` keeps the
  tool default **3**.
* Mouse down: brush rect = `size × size` centred on the point (top-left = p − size/2),
  then one stamp at offset (0,0).
* Every tick, `spacing == 0` brushes: if the point did not move → **stamp again in place**
  (continuous spray while held still, once per idle tick); if it moved → walk the line.
* `spacing != 0` brushes: ignore movement smaller than `spacing` in both axes (the last
  point is not updated); otherwise walk the line and update the last point.
* Line walk: Bresenham from last to new point along the major axis; accumulate the
  integer delta; whenever `|accumulated major delta| >= step`, stamp with the
  accumulated (dx, dy) and reset; after the loop, if anything is left, stamp once more
  (so every segment ends with a stamp at the exact mouse point).
* A stamp at offset (dx, dy) moves the brush rect by (dx, dy) first.

### 3.4 Stamp (`v14 @0x44d230`) **[O]**

```
stamp(dx, dy):
  brushRect += (dx, dy)
  (ox, oy) = oscillation_offset()                        # 3.5
  r = brushRect + (ox, oy)
  if shadowOffset != 0:  pass(r + (shadowOffset, shadowOffset), colour=(0,0,0),    mode=SHADOW)
  if glintOffset  != 0:  pass(r - (glintOffset,  glintOffset),  colour=(31,31,31), mode=GLINT)
  pass(r, colour = current colour as 5-bit, mode=NORMAL)
pass(rect, colour, mode):
  for each pad pixel (px,py) with a = pad[py][px] != 0:
     (x,y) = rect.topleft + (px,py); skip if outside canvas
     if not spatter_keep(): continue                     # 3.5
     blend(a, &canvas[y][x], texel(x,y), smearbuf?, mode) # 3.6
```

Note: shadow and glint passes ignore texture and smear; dry-out is not applied here
(`dryOutThreshold` unused) **[O]**.

### 3.5 Randomness **[O]**

* `rnd(lo, hi)` = uniform integer in [lo, hi] using MSVC `rand()` (15-bit) with rejection
  (`@0x459760`).
* **Spatter** (`v40`): keep pixel iff `rnd(0, spatter) == 0` (probability 1/(spatter+1)).
  BristleBrush overrides: keep iff `rnd(0, spatter * sizeIndex * 2) == 0`.
  ColorizeBrush overrides: always keep.
* **Oscillation** (`v41`): `m = oscillation * sizeIndex`; if `m != 0` and
  `r = rand() % m` is non-zero: `θ = rand() % 360` **used directly as radians** (no
  degree conversion), offset = `(trunc(cos θ * r), trunc(sin θ * r))`. Else (0,0).
  One offset per stamp (all passes).

### 3.6 Blend (`v42 @0x44cc00`) **[O]**

Inputs: pad coverage `a` (0..255), destination pixel `d` (5-bit RGB), optional texel `tx`,
optional smear-buffer pixel `s`, brush colour `c` (5-bit). Constant `K = 254.0`.

```
if mode == SHADOW: t = a / (shadowBias * K)
elif mode == GLINT: t = a / (glintBias  * K)
else:
    D = density()                     # base: densityBias*K, or K if densityBias == 0
    if texture active:
        tv = tx; if Space held at mouse down: tv = 255 - tv
        t = (a + tv / textureBias) / (255 + D)      # integer division tv/textureBias
    else:
        t = a / D
    if D <= 0: t = 0
if smear buffer exists (smearBias != 0, NORMAL pass only):
    new = clamp31(round(s*t + d*(1-t)))             # per channel
    if new != white:  s = clamp31(round(s*(1-k) + new*k))   with k = smearBias/100
else:
    new = trunc(c*t + d*(1-t))                       # no rounding, no clamp
d = new
```

* Density overrides: **Airbrush** `D = rnd(1, densityBias+1) * 255.0`; **Bristle**
  `D = rnd(1, densityBias+1) * 254.0` (per pixel).
* With `t > 1` (e.g. shadowBias small, big `a`) the formula extrapolates; the non-smear
  path has no clamp, results are packed with `<<5` without masking **[O]** — reproduce
  if you want bit-exact output.
* The smear buffer (`v5`, `smearBias != 0`): a `size×size` 16-bit buffer filled with the
  brush colour at mouse down; it is indexed with the same (px,py) as the pad.
* **"Space held"** is sampled once at mouse down (field 0x154) and also used by the effect
  brushes as an "invert" switch.

### 3.7 Textures **[O]** (`v43 @0x44e0f0`, `v25 @0x44db00`)

The Easel texture picker stores index `n` in `REasel+0x7c`. If `n != 0` and
`textureBias > 0`, the brush loads **Easel.MHK tBMP 17000+n** (17001–17041 exist) into an
8-bit buffer `texW × texH`. Texel for canvas (x,y) =
`tex[(y - canvasTop) % texH][(x0 - canvasLeft) % texW + (x - x0)]` where x0 is the left
edge of the pad rect (rows wrap, columns are not re-wrapped inside one pad row).
`TextureMaps.MHK` (12301–12320) is opened but the ids requested are 17000+n **[O]**.

### 3.8 Per-class pad-engine variants

* **Airbrush** — density override above; spacing 0 → sprays continuously while still.
* **Chalk / CustomBrush1–16** — pure engine. CustomBrush cfg values:

| N | brushID | spacing | spatter | dens | osc | shadow off/bias | glint off/bias | other |
|---|---|---|---|---|---|---|---|---|
| 1 | 1052 | 1 | 0 | 1 | 3 | 7/12 | 3/1 | |
| 2 | 1058 | 1 | 0 | 1 | 0 | 3/7 | 2/2 | |
| 3 | 1016 | 1 | 0 | 1 | 0 | 20/30 | 2/3 | |
| 4 | 1028 | 1 | 0 | 1 | 5 | 2/1 | 1/1 | |
| 5 | 1031 | 6 | 0 | 0 | 6 | 3/2 | – | |
| 6 | 1022 | 4 | 0 | 0 | 8 | 2/3 | – | |
| 7 | 1004 | 2 | 0 | 0 | 4 | 1/5 | 1/1 | `densitybias` lower-case **[?]** |
| 8 | 1007 | 5 | 0 | 1 | 8 | 3/3 | 2/2 | |
| 9 | 1019 | 1 | 0 | 1 | 5 | 10/15 | 2/1 | |
| 10 | 1010 | 0 | 0 | 0 | 4 | 8/7 | 3/1 | |
| 11 | 1013 | 5 | 0 | 2 | 5 | 4/3 | 1/1 | `densitybias` |
| 12 | 1055 | 2 | 0 | 5 | 8 | 4/20 | 4/3 | dryOut 10/15 |
| 13 | 1034 | 20 | 0 | 0 | 30 | – | – | |
| 14 | 1013 | 1 | 1 | 1 | 2 | 3/2 | – | |
| 15 | 1001 | 1 | 3 | 0 | 0 | 10/4 | 4/1 | |
| 16 | 1022 | 0 | 0 | 2 | 5 | – | – | `densitybias`, dryOut 2/2 |

  densityBias 0 means `D = 254` (same as 1). Key case-sensitivity of the INI reader is
  **[?]**; if it is case-sensitive the lower-case `densitybias` lines are ignored.
  Chalk: brushID 1001, densityBias 3, spatter 1, spacing 5.

* **BristleBrush** — density and spatter overrides above; smearBias 7 → smear buffer
  picks up 7 % per pixel; dryOutRate 20 → pad reloaded each stroke.
* **Marker** (`v42 @0x44bb60`) — same `t` as the base, then
  `new = round(c*t + d*(1-t))` and **per channel `d = min(d, new)`** (darken only, felt
  marker). Glint mode is not special-cased. brushID 1010, spacing 2.
* **Crayon** (`v14 @0x448090`) — replaces the stamp: for every non-zero pad pixel, with
  probability 1/4 (`rnd(0,3) == 0`): `a = 64` (or `texel + 96` with a texture),
  `t = a / 254`, `d = trunc(c*t + d*(1-t))`. Pad intensity, density, oscillation,
  shadow are ignored. brushID 1001 (spacing 1).
* **RealBrush** ("loaded brush", `v5 @0x44c410`, `v14 @0x44bdb0`) — at mouse down a paint
  buffer `size×size` is filled with the brush colour and the pixel counter is reset.
  Per stamp, `pickup = true`; for every non-zero pad pixel inside the canvas:

  ```
  if d is white (31,31,31): pickup = false          # for the rest of this stamp
  t = a / 254
  new = clamp31(round(b*t + d*(1-t)))                # b = buffer pixel
  if pickup: b = clamp31(round(new*0.1 + b*0.9))
  d = new
  if counter % 100 == 0 and a > 64: a -= 1          # pad slowly dries (persists!)
  counter += 1
  ```
  No oscillation/shadow/spatter. Because `dryOutRate` is 0 the dried pad is *not*
  reloaded on the next stroke; it recovers only when the size changes **[O]**.
* **ExtrusionBrush** ("3-D tube", `v3/v5/v14 @0x44a0d0/0x44a8b0/0x44a110`) — three passes
  per stamp (pad = brushID 1013+s−1, spheres):
  1. shadow at (+3,+3): `d = trunc(d * (1 - a/5080))` (constant 0.000196850…).
  2. highlight pad (Brushes.MHK tBMP **1030 + s**, i.e. 1031–1033, loaded into a 33×33
     buffer) at (+1,+1), or (+2,+2) when s ≥ 3, only where
     `y < top + h/2 + 2` and `x < left + w/2 + 2`:
     `k = a2/508; d = round(d*(1-k) + 31*k)`.
  3. main: `t = a/254; d = round(c*t + d*(1-t))`.
* **Drippy** — pad engine with `[Drippy] brushID=1013` (other keys commented out), plus
  drips: every tick, if `now >= nextDrip`, `nextDrip = now + rnd(100,150)` ms and a drip
  sprite starts at the current point. Drip scripts are **Icons.MHK tSCR**
  `base + rnd(0, n-1)`: size index < 3 → base 3200, n = 8; otherwise base 3208, n = 16
  (the 3224 branch is unreachable). Each animation frame is composited permanently into
  the canvas, tinted with the current colour (`@0x43dca0`, `@0x459930`); the final frame
  is stamped when the script ends. Each drip plays fx 7380. Frames of a script contain
  two cels (a black mask and a magenta layer) **[O]**; exactly how the cel pixels map to
  coverage is **[?]** (treat non-transparent cel pixels as current colour).

### 3.9 Mixer Wand brushes

All are pad-engine brushes with `[XBrush] brushID=1001` (pads 1001–1003, max coverage 82)
and spacing 1 unless noted.

* **SmearBrush** (`@0x44e2b0/0x44e390`, spacing 2) — per stamp: copy the canvas under the
  *current* rect (shifted to stay inside the canvas) into a buffer, move the rect by
  (dx,dy), then for non-zero pad pixels `t = a/127`, `d = clamp31(round(buf*t + d*(1-t)))`
  (drags the old pixels along) **[O]** (operand order **[I]**). For `a > 127` the result
  extrapolates and negative values clamp to 31.
* **BlurBrush** (`@0x447310`) — snapshot of the canvas around the rect; for non-zero pad
  pixels: `avg` = 3×3 box mean of the snapshot (R and G by integer /9, B by ×(1/9)
  truncated); `t = a/255`; `d = round(avg*t + d*(1-t))`. Reads pads 1004–1006 because of
  the id-0 quirk.
* **GreyscaleBrush** (`v42 @0x4ac0a0`, spacing 0 → continuous) — for every non-zero pad
  pixel (no alpha): `(h,s,v) = rgb2hsv(d)`, `d = hsv2rgb(h, 0, v)`.
* **ColorizeBrush** (`v42 @0x447ae0`) — `(hB,sB,vB) = rgb2hsv(brush colour, 8-bit)`,
  `(hD,sD,vD) = rgb2hsv(d)`, `c' = hsv2rgb(hB, sB, vD)`; then **as coded**
  `d = trunc(c'*a + d*(1 - a))` with the raw pad byte `a` (no /255). This overflows for
  `a > 1`; it looks like an original bug — **verify against the original before copying**
  **[O]/[?]**.
* **Effect brushes** XRay, Emboss, Edge, Nuclear, Solarize, HueShift, Posterize
  (RWackyEffectBrush family, `@0x4af2e0` …) — two-phase:
  * While dragging (`v42 @0x4af420`): every pad pixel not yet marked (bit `0x8000` of the
    RGB555 word) is replaced by its inverse `31 - c` per channel **and marked**. This
    is only the preview.
  * Mouse up (`v6` → `v45`/`v44`/`v46`): one pass over the **whole canvas rect**, reading
    the pre-stroke picture `S` (backup GWorld) and the canvas `D`:
    * Space not held: for marked `D` pixels, `D = effect(S)` with the mark cleared.
    * Space held at mouse down: for **unmarked** pixels `D = effect(S)`; for marked
      pixels `D = S` (restore). I.e. Space inverts the selection.
  * `effect()` (`v55`), using the integer HSV helpers below:
    * XRay: `31 - c` per channel.
    * Nuclear: `h = (h + 180) mod 360`.
    * HueShift: `h = (h + 30) mod 360` (Space: `+330`).
    * Solarize: `v = (v > 15 ? 31 - v : v) * 2`, clamp 0..31.
    * Posterize: `v = ((v + 8) / 8) * 8` (Space: `((v + 2) / 2) * 2`), clamp 31.
    * Emboss: neighbour `N = S(x+2, y+2)` (near the bottom rows the code uses the nearest
      row that exists, and compares the *column* with height−2 — a bug that only matters
      in the bottom 3 rows); `v = clamp(vS - vN + 15, 0, 31)`; result `hsv2rgb(hS, 0, v)`.
    * Edge: as Emboss, then `v = (v > 15 ? 31 - v : v) * 2`, clamp; result grey.
* **MosaicBrush** (`v42 @0x4ac620`) — immediate, no final pass. Cell `c = 2^s`
  (2/4/8 px). Each pad pixel at (x,y) takes the canvas pixel at
  `(floor(x/c)*c + c/2, floor(y/c)*c)`; the grid is aligned to the canvas buffer's memory
  layout, i.e. to canvas (0,0) **[I]**.

Integer HSV (`@0x447ce0`, `@0x447e10`) **[O]** — channels in whatever scale is passed
(5-bit for pixels, 8-bit for the brush colour in Colorize):

```
rgb2hsv(r,g,b): v = max; m = min; Δ = v - m
  s = v ? (31*Δ + v/2) / v : 0            # 0..31
  h = 0; if s >= 1:
     if r == v: h = (60*(g-b) + Δ/2) / Δ
     elif g == v: h = (60*(b-r) + Δ/2) / Δ + 120
     elif b == v: h = (60*(r-g) + Δ/2) / Δ + 240
     if h < 0: h += 360;  if h > 359: h -= 360
hsv2rgb(h,s,v): f = h % 60; p = ((31-s)*v + 15) / 31; q = v - p
  rise = (f*q + 30)/60 + p ;  fall = ((60-f)*q + 30)/60 + p
  sector h/60: 0:(v,rise,p) 1:(fall,v,p) 2:(p,v,rise) 3:(p,fall,v) 4:(rise,p,v) 5:(v,p,fall)
```
(`/` is C integer division; the `+x/2` terms come from the rounding helper
`(a*b + c/2)/c`.)

---

## 4. Pencil tool real media (RPencilPanel)

* Buttons MBTN 18400 (cmd 18400, param 0..3) select tool id `99 + param`:
  0 Pencil (RCircle), 1 Chalk, 2 Crayon, 3 Marker **[O]**. Shape buttons 18450–18457 select
  modifier ids `cmd - 18347` = 103 (Line) … 110 (FPoly); 18458 = freehand **[O]**.
  Modifiers drive the selected media along the shape outline **[I]**.
* **RCircle (plain pencil)** — size table {2, 10, 20}; step = clamp(size/3, 1, 3); brush
  rect `size×size` centred on the point; each step paints a filled **oval** if the shape
  field is 0x377 (round) else a filled **rect** in the current colour; a click without
  movement stamps once on mouse up **[O]**. The round/square choice comes from MBTN 18203
  (18201 sphere / 18202 square) **[I]**.
* Chalk, Crayon, Marker: §3.8.

---

## 5. Wacky brushes in detail

All use the current colour unless stated. "Shift = random colour" means each element
picks `(rnd(0,255), rnd(0,255), rnd(0,255))`. GDI pens/brushes quantise colours to
`& 0xF8` per channel **[O]**.

### 5.1 Madona — 3-D pyramid **[O]** (`@0x43fe20`, `@0x440020`)

Drag tool. At mouse down with point (x0,y0) and size `s` (10/20/40): base square
`L = x0-s, T = y0-s, R = L+2s, B = T+2s`; four faces, each a polygon holding one base
edge: face0 (L,T)-(R,T), face1 (R,T)-(R,B), face2 (R,B)-(L,B), face3 (L,B)-(L,T). Each
face gets a random colour `rnd(0,255)` per channel (fixed for the stroke). On every move:
restore snapshot; direction code `d` = 0 if `|dx|<|dy|` and dy<0, 2 if `|dx|<|dy|` and
dy≥0, 1 if dx≥0, 3 otherwise (dx,dy = apex − start); draw faces in order
`[0,1,3,2][k] XOR d` for k = 0..3, each as a filled triangle (edge + apex = current mouse
point), 1-px pen. Brush and pen colour identical.

### 5.2 Drippy — see §3.8.

### 5.3 Spiral **[O]** (`@0x4ae6f0`, `@0x4ae4b0`)

Timed (50 ms). At tool init 16 random colours are made into two pen sets (1-px and 3-px).
Mouse down: centre = point, angle α = 0, radius r = 1.5, colour index 1. Every tick draws
**20 segments**: for each, unless Space is held advance the colour index (mod 16);
pen = 3-px set if Ctrl or Shift held, else 1-px; new point = centre +
`(trunc(r*cos[i]), trunc(r*sin[i]))` with table `cos/sin[k] = cos/sin(k·π/16)`, k=0..31,
`i = trunc(α/(π/16)) - (α != 0 ? 1 : 0)`; line previous→new; `α += π/16` (wraps to 0 at
2π); `r += 0.075` (Ctrl: 0.2, Shift: 0.085). Mouse movement is ignored — the spiral keeps
growing around the click point while held.

### 5.4 Poly **[O]** (`@0x442010`, `@0x4420d0`, `@0x442130`)

Drag tool, filled polygon in the current colour. Mouse down adds the first vertex. On
movement: if more than **1000 ms** passed since the last fixed vertex, the current point
becomes a fixed vertex. Each move: restore snapshot, draw polygon (fixed vertices +
current point) filled. Mouse up keeps it.

### 5.5 TriSpin **[O]** (`@0x446b50`, `@0x446a30`, `@0x4469a0`)

Each mouse move stamps (base v8: only when moved) a Gouraud triangle with vertices
(1,1) red, (−1,−1) blue, (1,−1) yellow, scaled by size (5/10/20), rotated by a global angle
that grows **6°** per stamp (persists across strokes), alpha **0.3** (Shift: 0.085),
blend SRC_ALPHA / ONE_MINUS_SRC_ALPHA, centred at the mouse.

### 5.6 WhispyCircle **[O]** (`@0x446d40`, `@0x446e10`)

Radius `R = trunc(panelSize)*10 - 5` (5/15/25). Each mouse move draws two rings
(20-segment disks): radii [R−3, R−2] in colour `255 - c/2` per channel (pale tint), and
[R−1, R] in the current colour; alpha **60/255** (Shift: 25/255), alpha blending.

### 5.7 Particle **[O]** (`@0x440980`, `@0x440af0`, `@0x440f40`, `@0x441140`, `@0x441330`)

An "Emitter"-style particle system, pool 1000. `rnd()` = `(rand() − 0x3fff) · 0.00006103888`
(`@0x440930`, ≈ −1..1). Emitter (ctor `@0x440dd0`): position (400, 300, 150), yaw 0 ± 2π,
pitch π/2 ± pitchVar, speed 3 ± 1, `10 + trunc(rnd·15)` particles per emission (none if
≤ 0), life `60 + trunc(rnd·15)`, start colour ± 0, end colour ± 0.05, force (0, 0, −0.1).
* Mouse down (`@0x440af0`): pitchVar = size° (sizes 5/10/50), end colour = current colour
  (0..1), start colour = 1 − current, emitter x, y = mouse; emit. Every mouse move
  (`@0x440c30`) moves the emitter and emits.
* Emission (`@0x440f40`), rnd order: yaw, pitch, speed, 3 × start, 3 × end, life;
  direction `(−sin yaw · cos pitch, cos yaw · cos pitch, sin pitch) · speed` — mostly +z,
  a small x/y spread; Δcolour = (end − start) / life.
* Update (input-manager idle callback, mask 8; `@0x441440`): per particle, if life ≤ 0 or
  z ≤ 0 → paint it with `SetPixel` (one pixel at `(trunc x, trunc y)`, colour clamped to
  0..1 ·255) and free it (`@0x441330`); else pos += dir, dir += force, life −= 1,
  colour += Δcolour.
* The live particles are drawn as `GL_POINTS` at their z > 0, outside the ortho/identity
  clip volume, so nothing is visible until they land as dots.
* Deactivation (`@0x441510`) sets every life to 0 and updates once: all are painted.
* Update rate: one callback per idle input event **[I]** (port: 30 ms).

### 5.8 Garland (starburst) **[O]** (`@0x43e380`, `@0x43e1e0`)

Timed, interval **20 ms**, pen 2 px. Each tick draws **11** lines from the current point
to `point + (cos θ, sin θ) * size` with `θ = rnd(0,360)` taken as radians; colour random
per line, **Shift** = current colour. Sizes 10/20/40.

### 5.9 LeakyPen **[O]** (`@0x43f890`, `@0x43f7c0`)

Timed, **40 ms**. Radius `r` (16.16 fixed) = size at mouse down and after every movement;
each tick `r += 0.5` then a filled oval in the current colour centred at the point with
half-width `(r+0.5)>>16` and half-height `(r*1.25 + 0.5)>>16`. Held still, the blob grows
0.5 px per 40 ms. Sizes 2/5/10.

### 5.10 ScratchyPen **[O]** (`@0x442830`)

Per mouse move: new point = mouse + (rnd(−s,s), rnd(−s,s)); 1-px line from the previous
jittered point; current colour, Shift = random colour per segment. Sizes 5/10/20.

### 5.11 LoopingLine / LoopingLine2 **[O]** (`@0x43fc30`, `@0x43fa30`)

Pen 2 px. Per mouse move: point = mouse + `(trunc(cos α * s), trunc(sin α * s))`;
`α += 0.4` (LoopingLine) or `3.0` (LoopingLine2), wrapped at 2π; line from the previous
looped point (first event after mouse down only moves). Shift = random colour.

### 5.12 Bricks / Log — grid tiles **[O]** (`@0x43ae10`, `@0x43b130`, `@0x43b070`)

Tile set: Bricks = Icons.MHK tBMP **3613–3618**, Log = **3606–3612** (32×16). Grid is
anchored at the canvas top-left. Row = `(y - top) / tileH`. Bricks rows with even index
are shifted by `-tileW/2` (running bond) unless Shift is held; logs are never shifted.
Column = `floor((x - left - shift)/tileW)`. Whenever the mouse enters a different cell,
draw a random tile of the set there (transparent mode 8) and play the fx sound.

### 5.13 Window **[O]** (`@0x446ff0`, `@0x447060`)

Drag rect (Pt2Rect from start); Icons.MHK tBMP **3605** (479×479 window art) stretched
into the rect, transparent copy.

### 5.14 Door / DoorPlain **[O]** (`@0x43c720`…`@0x43d130`)

Drag rect, minimum size 3 × the first piece's width/height (in each direction from the
start point, `@0x43c7c0`). Door: OpenGL quad, smooth vertical gradient top (255,255,207) →
bottom white, 1-px black frame; window panes **3600** (15×19) tiled with 2-px gaps: columns
`(w-18)/(pw+2)`, rows `(h-40)/(ph+2)`, centred horizontally, from top+10; door knob
**3601** (7×20) at `right - knobW - 2`, vertically centred.
DoorPlain (tray icon 3536, pieces **3602–3604**, 20×20 / 20×31 / 20×39):
* background `@0x43cfc0`: `FillRect` RGB(204, 102, 51), then a black 1-px `Rectangle`
  (NULL brush);
* panels `@0x43d000`: `cols = (w − 12) / (pw + 8)`, `x0 = (w − cols·(pw+8) + 8) / 2`,
  rows from `top + 10`, each row drawn while `y + ph < bottom − 20`, transparent copy at
  `left + x0 + k·(pw+8)`, then `y += ph + 8` and the piece index steps 3602 → 3603 →
  3604 → 3602 (`@0x43d130`).

### 5.15 Roof **[O]** (`@0x442430`, `@0x442530`)

Drag rect → trapezoid: bottom edge = rect bottom, top edge inset by `w/4` on both sides.
Filled with OpenGL texture **Icons.MHK tBMP 3300** (64×32 shingles), repeat-wrapped,
texture coordinate = canvas offset / tile size, decal mode; black 1-px outline.

### 5.16 DotToDot **[O]** (`@0x43d2a0`, `@0x43d340`)

Timed, **100 ms**. Any movement sets a flag; on a tick with the flag set: append the
point to the list, paint a filled circle radius 2 at it, print the running number
(`"%d"`, counter restarts at 1 every mouse down) into the box (x−2, y−2)–(x+22, y+22)
(dot rect grown by 10, then offset by +10,+10), then redraw the 1-px
polyline through all points of this stroke. Dots and numbers are kept in a layer so the
polyline is redrawn cleanly. Play fx 7384 per dot.

### 5.17 ABC **[O]** (`@0x43a780`, `@0x43a850`)

Writes the **current user's name** (string at `prefs+0xc`, **[I]** = artist name) letter
by letter along the stroke in font **"Jester"**, size 16/24/32 (default 24), current
colour. A new letter is drawn when the mouse moved more than one character width
horizontally or one line height vertically since the last letter; the name cycles.
Each letter plays fx 7431.

### 5.18 Rainbow **[O]** (`@0x441650`, `@0x4417e0`, `@0x441950`)

Drag rect (from start to current). Half-annulus drawn with a 1-D texture of 8 RGBA texels
`(0,0,0,0) (127,0,127,80) (191,0,191,80) (0,0,255,80) (0,255,0,80) (255,255,0,80)
(255,127,0,80) (255,0,0,80)`, linear filter, replace mode; quad strip in steps of π/32,
outer vertex `((cos θ + 1)/2, sin θ)` texcoord 0.1875, inner vertex
`((r·cos θ + 1)/2, r·sin θ)` texcoord 0.9375, translated to (left, bottom) and scaled by
(w, h). Inner ratio `r` and blending: default r = 0.6 with alpha blending; Space → r =
0.85; Shift → opaque (no blending). Orientation of the arc in screen space is **[?]**
(depends on the GL projection; expect an arch standing on the rect's bottom edge).

### 5.19 ColorTile **[O]** (`@0x43b720`)

Drag rect, filled with the current colour at alpha 180 (0xB4), blend
`(ONE, ONE_MINUS_SRC_ALPHA)`; Shift → `(ONE, ONE_MINUS_SRC_COLOR)`.

### 5.20 8WayMirror **[O]** (`@0x43a620`, `@0x43a340`)

Eight pen heads start around the click point (x,y) at (x−5,y−10), (x+5,y−10),
(x+10,y−5), (x+10,y+5), (x+5,y+10), (x−5,y+10), (x−10,y+5), (x−10,y−5). For each mouse
delta (dx,dy) the heads move by (+dx,+dy), (−dx,+dy), (−dy,+dx), (−dy,−dx), (−dx,−dy),
(+dx,−dy), (+dy,−dx), (+dy,+dx) and draw a line from their previous to their new
position (current colour, 1 px; Space → 2 px). Alt held: lines go from the original click
point instead (starburst).

### 5.21 Splat **[O]** (`@0x443aa0`, `@0x4437b0`)

Timed (50 ms), sizes 1/2/3. Each tick 6 blobs, colour random (Shift: current). Normal:
`r = rand in [3s, 8s]`, oval rect = point ± r shifted by (rand[−2r,2r], rand[−2r,2r]),
filled. Space: filled random triangle with vertices point ± rand[−s,s] per axis.

### 5.22 Tree **[O]** (`@0x4463d0`, `@0x446560`)

Drawn on mouse down (and on every move while **Alt** is held, with a 50-ms busy wait).
Trunk length `L = rand[18,22]` (Shift: [21,25], Space: [25,31]); pen width
`max(1, (L*1.5)/7)`; trunk line from (x−1, y) to (x−1, y−L−2). Then
`branch(x, y−L, L, angle=270°, spread=rand[5,30], min=10, factor=0.9)`:

```
branch(x,y,len,angle,spread):
  loop:
    if Shift: random pen colour
    a1 = angle - spread; a2 = angle + spread (degrees)
    e1 = (x + trunc(cos(a1)*(len+rnd(-5,5))), y + trunc(sin(a1)*(len+rnd(-5,5))))
    e2 = same with a2 (fresh jitter per coordinate)
    pen = max(1, len/7); line (x,y)->e1; line (x,y)->e2
    len = trunc(len*0.9); if len <= 10: return
    branch(e1, len, a1 - rnd(-20,20), spread)
    (x,y,angle) = (e2, a2)            # iterate the right branch
```

---

## 6. Sound Art (voice) brushes

Transcribed from KPD3.exe (IDA 9.5 / Hex-Rays plus disassembly where the decompiler lost
track of the stack); port in `crates/kp-brushes/src/voice.rs`. Field names are the
RVoiceBrush offsets.

### 6.1 Microphone analysis **[O]** (`@0x463e80`, `@0x463d10`, `@0x462e20`, `@0x464120`)

* `waveIn` 22,050 Hz, 16-bit mono, three 8 KB buffers. A worker thread feeds every buffer
  in 512-sample chunks into a sliding **1024-point** window (oldest 512 shifted out) and
  runs a radix-2 FFT (no window function) after each chunk.
* Each tick the brush reads bins 0..511: `mag[k] = trunc(|X_k| / sqrt(1024) / 256)` (u16),
  `freq` = the first strongest bin (DC included), `amp` = its magnitude.
* Tick (`v8`, `@0x450380`), only while armed **and** the recorder runs (no microphone →
  nothing at all): clear the bin buffer, analyse. If `freq != 0 && amp != 0`: track the
  largest `freq`/`amp` since mouse down (0x57e / 0x57c), `A = amp · amplitudeBias · 0.01`,
  `F = freq · frequencyBias · 0.01`; colour = base colour if `F ≤ 10`, else
  `hsv2rgb((short)trunc(h_base + F) % 360, s_base, v_base)` (5-bit HSV, `@0x447ce0` /
  `@0x447e10`). Otherwise `A = F = 0` and the colour is left as it is.
  Then movement: `spacing ≠ 0` → walk from the last point when |dx| or |dy| ≥ spacing;
  `spacing = 0` → walk when the mouse moved, else one pad call `v14(0, 0)` in place.
* Walk (`v15`, `@0x4448c0`): the pad rect is first centred on the start point (`v16`), then
  a Bresenham walk calls `v14(dx, dy)` every `step` pixels (spacing, 3 when 0) and once
  for the remainder.
* Mouse down (`@0x450670`): base pad setup (rect = point − size/2; its own `v14(0,0)` runs
  before the brush is armed, so nothing is drawn), reset A/F/maxima, base and current
  colour = palette colour as `(127 + 31·c) / 255`, start the recorder, arm. Mouse up stops
  the recorder and disarms.
* `v41` oscillation (`@0x4501a0`): `m = (short)(s · trunc(A))`, capped at `oscillation`;
  if `m ≠ 0`: `r = rand() % m`; if `r ≠ 0`: `θ = rand() % 360` **used as radians**,
  offset `(trunc(cos θ · r), trunc(sin θ · r))`. `s` = panel size 1–3.
* Pad size (`v26`): `brushID` when 1..127, else per class: CVB1/2 → 5, CVB5 and
  Touchless5 → 2, CVB7 → 3, all other voice and Touchless classes → 1; CVB10 keeps the
  base class's Brushes.MHK pad `1001 + s − 1` (side = max(w, h)). The cfg's `brushID 1001`
  lines therefore mean "default".

GDI primitives: `@0x4f8854(rect, colour)` = `Ellipse` with a solid brush and the NULL pen
(filled, one pixel smaller right/bottom); `@0x4f8996(rect)` = `Ellipse` with the NULL brush
and the current pen (outline; pen = 1 px voice colour unless noted, width = field 0x28);
`@0x4f8633(rect)` = `PatBlt(PATINVERT)` with brush `PALETTEINDEX(255)` clipped to
`CreateEllipticRgn(rect)` — inverts the oval (index 255 is white in the canvas palette
**[I]**). "grow g" = `InsetRect(r, −g, −g)`; `R` = the pad rect after the step's move.

### 6.2 CustomVoiceBrush1–10 `v14` **[O]**

| # | addr | rule |
|---|------|------|
| 1 | 0x448a80 | if A: R − osc, grow `min(rand % trunc((F+100)·0.01) + 1, 4s)`, **filled** oval |
| 2 | 0x448f30 | if A: R − osc, grow `min(rand % trunc((A+100)·0.01) + 1, 12s)`, outline |
| 3 | 0x449140 | if F: R − osc, grow `min(rand % trunc((F+10)·0.1) + 1, 12s)`, outline |
| 4 | 0x449350 | if A and F: R − osc, grow `min(1 + trunc((F+10)·0.1), 12s)`, **invert** oval |
| 5 | 0x449550 | for every bin 0..=max freq: R, plus `osc·s` if the bin is non-zero; filled oval each |
| 6 | 0x449760 | osc; `InsetRect(R, −(trunc(A)&0xffff) % ox, … % oy)` (0 when the offset is 0), `+ osc·s`, outline |
| 7 | 0x449980 | for every bin 0..=max freq: zero bin → outline at R with the current pen; else R + osc, colour = base hue + magnitude (kept as brush colour, becomes the pen), grow `(mag/100 + 1)·s`, outline |
| 8 | 0x449cb0 | grow trunc(A), + osc, pen width `min(rand % trunc((F+10)·0.1) + 1, 2s)`, outline (draws even when silent) |
| 9 | 0x449ea0 | grow `−trunc(A / (−6 / s))`, + osc, invert oval then outline |
| 10 | 0x448cf0 | the pad rect is **not** moved; `q = rect.topLeft + (dx, dy) + osc`; line from the previous `q` (first: the click point) to `q` |

### 6.3 Hands-free (Touchless1–10, RHandsFreeBrush) **[O]**

* Starts when the brush is **selected** (`v3` `@0x44ae90`): centre of the canvas, recorder
  on, A/F/maxima/velocity cleared, colour from the palette, armed. Mouse clicks do nothing
  (`v5` is a null routine), except Touchless10 whose `v5` re-arms at the click point and
  restarts its line there. Updates come from an input-idle callback (`@0x44b170`) that
  first turns the stored update time into "ms since".
* Update (`@0x44b1f0`): analyse; proceed if there is sound **or** the time since the last
  update exceeds `gravityTime` (after one skipped update the stored value is an elapsed
  time, so the next one always proceeds); otherwise A = F = 0 and stop. Proceeding: as
  §6.1 but the colour threshold is `F ≤ 1`; then
  * push = `(trunc(cos α · |A|), trunc(sin α · |A|))`, `α = (u16)trunc(F) % 360` as radians;
  * if `gravityBias ≠ 0`: gravity = `accelerationBias · 0.1 · gravityBias · 0.001 · d²`
    towards the start centre (d = distance; 0 within `gravityRange`), kept until the next
    computation;
  * velocity += push + gravity, each axis clamped to ±50, then damped by
    `dampBias · 0.01` towards 0 (unchanged when smaller); position += velocity;
  * walk from the current point to the truncated position.
* Pad step (`v46` `@0x44b670`, called first by every Touchless `v14`): move rect and point
  by the step, then keep the rect in the canvas: `wrap` → wrap around, else pin to the
  edge (velocity not reversed); position = point. Touchless1–9 then move the rect by the
  step **again** (so the shapes run ahead of the point until the next walk re-centres it);
  Touchless10 does not.
* Defaults (`@0x44a9d0`): accelerationBias 1, gravityBias 1, gravityTime 100,
  gravityRange 1, dampBias 2, wrap 0. The cfg's `acceleratonBias` (sic) is never read, so
  every Touchless brush has accelerationBias 1.
* Every 10 moves the colour getter `v29` (`@0x44abe0`) steps the hue by 2 at full
  saturation/value; it is not used for drawing.

| # | addr | rule (R = pad rect after both moves) |
|---|------|------|
| 1 | 0x44e910 | as CVB1 |
| 2 | 0x44eb80 | if A: R − osc, grow `min(rand % trunc((A+100)·0.01) + 1, 4s)`, outline |
| 3 | 0x44edc0 | if F: grow trunc(A), + osc, pen width `min(rand % trunc((F+10)·0.1) + 1, 2s)`, outline |
| 4 | 0x44eff0 | if A and F: R − osc, `InsetRect(−ox·s, −oy·s)`, invert oval |
| 5 | 0x44f1f0 | as CVB5 |
| 6 | 0x44f440 | as CVB6 |
| 7 | 0x44f690 | as CVB7 but only bins with magnitude > 10 are scattered/recoloured |
| 8 | 0x44f9d0 | grow trunc(A), + osc·s, outline |
| 9 | 0x44fba0 | grow trunc(A·0.1), + osc·s, invert oval then outline |
| 10 | 0x44fd80 | as CVB10 (line from 0x638), rect moved only by `v46` |

Remaining **[I]**: the idle-callback rate (the port runs one update per frame) and the
colour of `PALETTEINDEX(255)` (taken as white → inversion).

---

## 7. Spray Can

* **Shapes** (RRollerSubPanel "Cyclic", RCyclic) **[O]**: list = Cyclic.MHK tBMP 24000,
  24100, … while present (15 sets; the tBMP is the icon, tBMH with the same id is the
  sprite set, REGS id/id+1 its registration points). While held (`@0x442cc0`), whenever
  the stamp time has passed — at once after mouse down, which clears it — the brush
  stamps and sets the next time to now + **100 ms**; it stamps a random cel `rnd(1, count-1)` of the set at the mouse
  point minus the cel's registration offset, transparent copy. Size table {0,1,2}; the
  RSimpleBitmap base scales ×1/×2/×4 by size, RCyclic's own stamp does not scale **[O]**.
* **Moving paint** (RSpraySubPanel "RSprayPanel", RSprayGun) **[O]/[I]**: Anims.MHK sets
  24000–25900 (tBMH + tSCR + tWAV). A repeating RTimer fires while the button is down;
  each fire (if the mouse is inside the canvas) starts a random script
  `rnd(first, first+count-1)` of the selected set at the mouse point, confined to the
  canvas. When a script ends its last frame is composited into the picture. On tool
  switch all running sprites of the set are finished. The RTimer is started at mouse
  down with **50 ms**, repeating, and the mouse down sends the first shot itself
  (`@0x443ce0`). Script→sound binding **[?]**.

---

## 8. Cursors

Not determined **[?]**. `RHotSpotList` loads all 29 `General.MHK tCUR 10000–10028` and
selects one by index per hot spot, but I did not find where the paint panels set the
canvas cursor index. Use the FORMATS.md list (5/15 paint brush, 6 airbrush,
21 microphone, 12 mixer, …) per main mode until this is traced.

---

## 9. Not determined / to verify

1. Cursor per tool (§8).
2. Colorize formula overflow (§3.9) — verify visually in the original.
3. Tinting of the 8-bit realistic-brush icons 3500–3519 (§1.3).
4. Exact pixel mapping of drip cels (§3.8) and Anims/Cyclic compositing.
5. ~~Voice brush hue formula, CVB5–10, Touchless shapes, hands-free motion~~ — done (§6).
6. Rainbow orientation (§5.18). (Particle and DoorPlain: done, §5.7 / §5.14.)
7. INI reader case sensitivity (`densitybias`, `GreyScaleBrushSnd`) and duplicate
   section precedence.
8. Idle-tick rate that drives `spacing = 0` brushes, Particle and hands-free updates
   (the spray timer is 50 ms, §7).
9. FeltPenSnds.MHK usage; TP.MHK 20400–20448 / 20500–20541 are not used by brush code
   (no references found).

Fastest way to settle 2–8: run the original under Wine/Xvfb at 16-bit colour and compare
strokes pixel by pixel with the port.

---

## Corrections found while implementing (`crates/kp-brushes`)

Settled from the decompiled code / CD data while writing the Rust port:

1. **INI reader** (`@0x4f0543`, section match `@0x4f1324`, key match `@0x4f13c1`):
   section *and* key names are compared with `_memicmp` → **case-insensitive**; the
   **first** section of a name wins; `;` lines are comments; a section ends at the next
   `[` line; the value is trimmed and one pair of matching quotes is stripped; numbers go
   through `strtol(value, &end, 0)` (base 0: `0x` hex, leading `0` octal) and must lie in
   −32768..32767, otherwise the field keeps its default. Consequences: the lower-case
   `densitybias` lines of CustomBrush7/11/16 **are** read (§3.8 [?] resolved),
   `[GreyScaleBrushSnd]` **does** match `GreyscaleBrush` (§2.4), and for the duplicate
   `[BlurBrushSnd]` the first copy wins (moot: BlurBrush reads `[RealBrushSnd]`).
2. **Texture term** in the blend (§3.6): the code adds `254.0` (`v24`), not 255:
   `t = (a + tv / textureBias) / (254 + D)`. `tv = 255 - texel` with Space is computed as
   `(u8)(-1 - texel)`.
3. **Blend packing** (§3.6): channels are stored in bytes (`(u8)(__int64)value`, so
   negatives wrap) and packed as `b | (g | r<<5) << 5` with OR and no masking; the smear
   path clamps "`> 31` as unsigned byte" (negatives become 31).
4. **Spacing test** (§3.3): a move is walked when `|dx| >= step` **or** `|dy| >= step`
   (`step` = cfg spacing, or 3 when spacing is 0).
5. **rnd(lo, hi)** (`@0x459760`): `n = hi-lo+1; limit = 0x8000 - 0x8000 % n;` reroll while
   `rand() > limit` (strictly greater, so `limit` itself is accepted and maps to `lo`);
   result `lo + r % n`.
6. **Voice brush defaults** (ctor `@0x44ffd0`): `frequencyBias` and `amplitudeBias`
   default to **1**; the voice ctor seeds `srand(timeGetTime())`. The hands-free keys
   are `accelerationBias gravityBias gravityTime gravityRange dampBias wrap`
   (`PTR_DAT_0058188c` = `"wrap"`, confirmed from the string table).
7. **Spiral sounds** (§2.4 table says "no section → silent"): the name table has
   `Spiral` twice (tool 22 and transition 129) and the lookup is by name, so the Wacky
   Spiral finds the transition section `[SpiralSnd] startSoundID = 7224` and plays it at
   mouse down (unless RSpiral overrides `v21`, not re-checked).
8. **Drip cels** (§3.8): tBMH 3200 holds 8-bit *alpha* cels (black, alpha 0xa0–0xff
   edges) for cels 1–8/17–24/… and opaque magenta 16-bit cels of the same shapes for
   9–16/25–32/…; each frame lists `[magenta, mask]` at the same registration. The port
   tints by coverage = cel alpha (so effectively solid current colour) — still [I].
9. **Script placement** (drips, moving spray): scripts carry authoring-time bounds
   (e.g. drip 3200 at (393,299)); the port anchors the first frame's first cel position
   (`bounds.topLeft + (x, y)`, i.e. its registration point) on the mouse point and keeps
   all later cels relative to it [I]. Frame period = tempo (100) ms [I].
10. **DoorPlain**, **Particle**, all Sound Art classes, the hands-free motion and the
    spray timer were re-traced in IDA 9.5 (database kept outside the repo) and are now
    transcribed (§5.7, §5.14, §6, §7). Corrections over the first port: CVB1 fills and
    CVB2 outlines (swapped before), CVB4/9 and Touchless4/9 invert, Touchless shapes
    differ from the CVB ones, no stamp at mouse down, colour unchanged on silence,
    Particle flies in z and lands as 1-px dots, spray period 50 ms.
