# Kid Pix Deluxe 3 — shell specification (everything outside the drawing tools)

Reverse-engineering notes for a faithful re-implementation of KPD3 (Windows v1.0, 2000).
Sources: Ghidra decompilation of `KPD3.exe` (class-named vtables), its Win32 resources,
the CD data (`Data/*.MHK`, `KpInstall/KPD3.ini|.cfg`, `Idea Machine/*.kpx`,
`Data/Quicktimefx/*.qfx`). Everything is written in our own words; addresses are given
so a reader can re-check a claim in the decompilation.

Confidence tags used throughout:

* **[code]** – read directly in the decompiled code / resource bytes.
* **[data]** – observed in CD data (rendered art, script bounds, file bytes).
* **[inf]** – inferred (consistent with code+data but not proven line by line).
* **[?]** – unknown / guess.

All screen coordinates are in the 800×600 game screen. Rects are `(left, top, right,
bottom)` unless stated "Mac order".

---

## 0. Terminology correction (important)

* **"Picker" (`RPicker`, `Picker.MHK`) is the SlideShow organizer**, not a user picker.
  The menu calls it "SlideShow", help calls it "Project Zone". [code]
* **User selection** is the graphical **login dialog** `RDlgLogin` (`RDlgLogin.MHK`). [code]
* `EnterName.MHK` is the graphical "Give your page / SlideShow a name" dialog
  (`RDlgEnterName`), used in Small-Kids mode instead of the Windows Save dialog. [code]
* `EnterSnd.MHK` is "name your sound" (`RDlgEnterSnd`), used by the sound-recording panel. [code]
* The "Paint Zone" = the Easel (`REasel`). [data]

---

## 1. Architecture: modules, messages, menu modes

### 1.1 Modules [code]
`RApplication` owns one current *module*. Module switch = `FUN_004b4390(app, id)`:

| id | class | ctor | module |
|---|---|---|---|
| `0x15bf` (5567) | `RPicker` | `FUN_004265f0` (0x840 bytes) | SlideShow organizer ("Project Zone") |
| `0x15c0` (5568) | `REasel` | `FUN_0040f230` (0xc4) | Paint Zone |
| `0x15c2` (5570) | `RSlideShowModule` | `FUN_0042b460` (0x70) | slide-show player |
| `0x15c3` (5571) | `RSplash` | `FUN_0042bf20` (0xcc) | splash |

Before creating a module the menu bar is reconfigured (`FUN_004b4560`): Picker → menu
mode `0x81`, Easel → `0x80`, SlideShow player → menu bar hidden, Splash → hidden.

### 1.2 Messages [code]
Every UI action is a message object: `+0x08 u32 id`, `+0x1c u32 param/pointer`,
`+0x20 i16 short param` (`FUN_00401470`). Button/hotspot command ids (MBTN field 6) are
posted as message ids to the owner. Module-level messages used by the shell:

| id | meaning |
|---|---|
| `0x15bf` / `0x15c0` / `0x15c2` | switch to Picker / Easel / SlideShow player (`+0x20` = start slide for 0x15c2) |
| `0x15c8` | open login dialog; `+0x1c` = message to send afterwards, `+0x20` = 0 startup / 1 from menu |
| `0x15c9` | login dialog closed; dialog result in `dlg+0x2e4`: `0x6a8` logged in, `0x5de` EXIT (quit app), `0` cancelled |
| `700` (`0x2bc`) | splash finished |
| `0x19cb` | new page (canvas) |
| `0x19cd` | load page object (`+0x1c`) into easel / open slideshow in picker |
| `0x19ed` | help mode finished |
| `0xdf9/0xdfa/0xdfb` | slide show start / stop (→ picker) / advance |

### 1.3 Global prefs block `DAT_00597dc4` (i16 array) [code]
Persisted per user in `user.ini` section `[GlobalUser]` (`RGlobalSave`, see §3):

| idx | key | default | meaning |
|---|---|---|---|
| 0 | `TTS` | 1 | Text-to-speech on |
| 1 | `SmallKids` | 0 | Small Kids Mode |
| 2 | `ToolSnds` | 1 | tool sounds |
| 3 | `AttachedSnds` | 1 | play sounds attached to pictures |
| 4 | `PlayOnce` | 1 | slide show plays once (0 = loop) |
| 5 | `ManualAdvance` | 0 | slide show ignores timers, advance by click/key |
| 0x106 | `ProjectPosition` | 0 | picker project-strip scroll pos |
| +6 (string) | `ABCString` | `"Kid Pix is fun!"` | text used by the ABC/alphabet text tool |
| string | `ProjectString` | — | last slideshow |
| `[EMail]` | `Name`, `Address` | — | e-mail sender |

`KPD3.ini [UserSettings] SmallKidsMode` and `[GeneralSettings] AutoLogin` are **not read**
by this build (no string reference) [code].

---

## 2. Startup

### 2.1 Application init (`RApplication__v3` @004b2b10) [code]
1. Creates the 800×600 16-bit display (switches screen mode if needed; game area is
   centred: origin `((W-800)/2, (H-600)/2)`). `[DebugSettings] PromptForMissingCD`,
   `lowVol` read.
2. Creates the main window with menu resource 128 and **removes top-level menu #6
   ("Debug")** by position (`FUN_004329c0` → `RemoveMenu(…, 6, MF_BYPOSITION)`).
3. `[GeneralSettings] ShowSplashScreen == "true"` → module `0x15c3` (Splash).
   Otherwise: `app+0xa4 = StartInPicker=="true" ? 0x15bf : 0x15c0` and the decompiler
   shows the same `FUN_004b4390(app,0x15c3)` call followed by a redraw. Shipping INI has
   `ShowSplashScreen="true"`, so implement: splash → login → target. [code; the
   no-splash branch is unclear – treat it as "skip splash, go straight to login"]

### 2.2 Splash (`RSplash`, `Splash.MHK`, cfg `[Splash]`) [code+data]
* Loads file set "Splash", plays **script 1000** (full-screen still, rect 0,0–800,583,
  cel 71) as the background, then creates a sprite on **script 1001** (277 frames,
  314 ticks, tempo 100) which starts `tWAV 1000` (30.6 s stereo 22 kHz music) at tick 18.
  Script 1002 = final still (cel 70). 314 ticks ≈ 31.4 s ⇒ **tempo 100 = 100 ms/tick
  (10 fps)** [inf, matches the music length].
* Sprite end callback, any key (`RSplash__v3`) or mouse click (`RSplash__v5`) posts
  **700**. On 700 (once): `target = StartInPicker ? 0x15bf : 0x15c0`; post `0x15c8` (login)
  with `+0x1c = target`, `+0x20 = 0`; stop sprite and sound; clear the three screen
  buffers to black.
* `Splash.MHK` script 1001 contains two `op 9` records (`0001 0014 0000`,
  `0001 001e 0000`) – meaning unknown [?] (probably sprite/loop control).

### 2.3 Login dialog — the user picker (`RDlgLogin`, `RDlgLogin.MHK` + `Misc.MHK`)
Created by the app on `0x15c8` (`FUN_0040c3e0`, 0x6f4 bytes). Rendered over the current
screen. [code+data]

Layout (all from script bounds) [data]:

| element | scripts | rect |
|---|---|---|
| dialog frame + titles "Enter your name here:" / "Select your name here:" | 7600 | 173,62–633,457 |
| name edit box (art 7601/7603) | 7603 = text rect | 228,136–443,166 |
| name list frame (8 rows) | 7604 (list), 7602 = list rect | 229,202–460,409 |
| row *i* (0–7) normal / highlighted | 7652+i / 7644+i | 233,206+25i–442,230+25i |
| scrollbar track | 7605 | 446,220–459,391 |
| scroll-up arrow (rest/over/press/disabled) | 7606/7607/7608/7613 | 446,203–459,220 |
| scroll-down arrow | 7609/7610/7611/7612 | 446,391–459,408 |
| thumb | 7614, 7615 | 446,220–459,237 |
| buttons (rest/over/press/disabled = base+0..3) | GO 7616, DONE 7620, ADD 7624, DELETE 7628, EXIT 7632, CREDITS 7636, BROWSE… 7640, CANCEL 7660 | right column x 491–574 |

Button tables [data]: **MBTN 777** (group 1500, login mode): GO cmd **1500** (491,202),
EXIT cmd **1502** (491,380), CANCEL cmd **1395** (491,380). **MBTN 778** (group 1600, admin
mode): EXIT 1502, DONE **1603** (491,202), ADD **1604** (491,236), DELETE **1606** (491,270).
CREDITS/BROWSE art exists but no table uses it (unused) [data].
EXIT and CANCEL share a position: `+0x20==0` (startup) hides CANCEL → EXIT visible;
`+0x20==1` (menu "Log In as New User") hides EXIT → CANCEL visible [code: `FUN_0040d920`
disables the button whose rest script == 7660 resp. 7632; inf for exact visuals].

Text field [code]: font size 14, colour RGB(0,0,255) [inf: struct `00 00 FF 00`], max
0xf5 chars. Allowed characters = `KPD3.ini [LoginAlphabet] LegalAlphabet`
(`A–Z a–z 0–9 space`) plus Backspace, Delete(0x2e), Left(0x25), Right(0x27). Each accepted
key plays `tWAV 7600` (click); a rejected key plays `tWAV 31503` (Misc.MHK). PageUp/
PageDown (0x21/0x22) scroll the list by a page. Enter = ADD/GO action (`FUN_0040cf00`).
After each edit the GO (login mode) or ADD (admin mode) button is enabled iff the text is
valid and non-empty.

List [code]: filled by enumerating the **sub-directories of the Users folder** (path id
0xf, §3). Clicking a row copies the name into the field; **double-click logs in**
(message `0x800047b3`). The default/placeholder text is `"Your Name"` (ignored).

Actions [code]:
* **GO (1500) / Enter**: name := field text. If it equals (case-insensitively, the code
  lower-cases a copy) **`kidpixadmin`** → switch to *admin mode* (hide group 1500, show
  MBTN 778 group 1600, list becomes multi-select). Otherwise, if not "Your Name": select it
  in the list if present; create the user's folder tree if missing (`FUN_004816a0`), make
  it the current user (`FUN_00481950`: loads `user.ini`, user libraries, e-mail name),
  result `0x6a8`, close, notify app. → app sends the stored target (Easel/Picker).
* **ADD (1604)** (admin): as above but the user is only created and added to the list.
* **DELETE (1606)** (admin): opens the confirm dialog `Deleters.MHK` (`FUN_00401050`,
  result 0x6a5) with message art **7809** "Are you sure you want to delete this user?" (1
  selected) or **7810** "…these users?" (several); OK (1396) → delete selected users
  (`FUN_0040d860`) [inf: removes folders].
* **DONE (1603)**: back to login mode.
* **EXIT (1502)**: result `0x5de` → application quits.
* **CANCEL (1395)**: result `0` → back to where it came from (no user change).

After login the app (`RApplication__v1` case 0x15c9/0x15c0): for target Easel, creates a
new empty page object (`FUN_004826b0`), switches to the Easel and posts `0x19cd` with it.
When invoked from the Easel menu, the Easel receives `0x109` and on `0x6a8` starts a new
blank page (`0x19cb`).

---

## 3. User storage [code + cfg]

Path ids resolved by `FUN_0046aa60/0046ac30` from `KPD3.cfg [DirectoryOrganization]`;
the key name for id *n* is entry *n* (30-byte slots) of the table at `0x558a2c`:

| id | key | default value | resolved path |
|---|---|---|---|
| 0xe | `LocatedUserAssets` | `PathToRootOfUsersFolder` | install root |
| 0xf | `DirUsers` | `Users` | `<root>/Users` |
| 0x10 | (user) | – | `<root>/Users/<UserName>` |
| 0x11 | `DirPictures` | `My Pictures` | pictures (`*.kpx`) |
| 0x12 | `DirProjects` | `My Pictures/My Slide Shows` | slide shows (`*.kpp`) |
| 0x13–0x19 | `DirBackground, DirMovies, DirSounds, DirStamps, DirBmpStickers, DirStickers, DirToons` | `My Backgrounds` … `My Toons` | user content dirs |
| 0x1a–0x20 | `FileBackground … FileToons` + `[Paths2] UserLibPostfix=".klb"` | e.g. `My Sounds/My Sounds.klb` | user libraries |
| 0x21–0x28 | `DirLib*` | `Backgrounds`… `IdeaGen` | CD libraries |
| 0x29–0x2f | `Def*Lib` | `Backgrounds/ArtsCrafts` … | default library per kind |

Per-user files: `user.ini` (`[GlobalUser]`, `[EMail]`, see §1.3; panel prefs are stored by
`RPanelPrefsTmpl<…SaveData>` – tool researchers), `Words.dic` (TTS pronunciation
exceptions, §11), `user.dct` (spell-checker user dictionary).
Default dialogs dirs: `MyPixDefaultDir` (0x11), `MyProjectsDefaultDir` (0x12),
`IdeaMachineDefaultDir` (`[Paths] IdeaMachinePath="Idea Machine"` on the CD),
`SoundsDefaultDir`, `MoviesDefaultDir`. [code]
Untitled names: page `Untitled`, slide show `[Paths2] UntitledProjName = "KP1"`.

---

## 4. Menu bar (RT_MENU 128) [code]

Win32 menu, decoded byte-exactly. Items in order (`—` = separator). "Ev" = internal
message posted by `RDisplayWindow__v32` (@00433690); "E/P" = enabled in Easel (mode 0x80) /
Picker (0x81) by `FUN_00434970` (initial state; tools may enable more later [inf]).

**File**
| id | text | E/P | action |
|---|---|---|---|
| 108 | `&New...` | E P | Easel: Ev `0xf5` → new page (ask-save 7546 if dirty). Picker: `0x555` → new slide show |
| 101 | `&Open...` | E P | Easel: `0x19f4` → ask-save 7547 if dirty → Windows Open (Page filter) → load `.kpx`. Picker: `0x19cd` → open `.kpp` |
| 102 | `&Save As...` | E P | `0x19cc` → Easel: Windows Save-As loop (`FUN_0040f380`); Picker: save slide show |
| — | | | |
| 105 | `&Export...` | E P | `0x19d6` → Save dialog (type 2) → export picture as movie/image, or "SlideShow To Go" (`%s To Go`) / movie (`FUN_00466230`) |
| — | | | |
| 106 | `Print...` | E P | Easel `0x19e3` → print dialog 150; Picker `0x55c` → dialog 151 (§12) |
| 109 | `Log In as New User...` | E P | `0x15c8` → Easel asks save (7545) first, then login dialog with CANCEL |
| — | | | |
| 107 | `E&xit` | E P | Easel `0x10c` → ask-save 7544 → quit. Elsewhere: quit (`FUN_00507ec8`) |

**Edit**: 201 `Undo/Redo` (E P, undo manager; text from `[UndoStrings]`), —, 202 `Cut`,
203 `Copy`, 204 `Paste` (E: clipboard of selected page object/area `FUN_00469e90/10/40`;
P: `0x19db/0x19dc/0x19dd` cut/copy/paste slides).

**Add** (Easel only): 111 `Add Graphic...` (`0x19d5`/0x6f: Windows Open with Image filter,
image placed as floating bitmap object), 113 `Add Movie...` (0x71, tMOV object; if the page
already has one → `[FileStrings] ReplaceMovieWarning`), 116 `Load Picture...` (Picker only:
`0x19f4` → load a picture into the selected slot), —, 115 `Import Background...`
(0x73: Open + DIALOG 134 options, see §5.1), 112 `Import Sound...` (0x70: attach a sound
file to the page).

**Toolbox**: 224 `Edit ABC Text...` (E; `0x19e4` → RABCTextDlg edits `ABCString`),
210 `Check Spelling...` (DIALOG 129; enabled by text tool [inf]), 225 `Flatten Text...`,
214 `Flatten Stickers and Animations...` (both `0x19f7` with param 225/214 → RFlattenDlg),
238 `Say It This Way...` (`0x19de` to the text panel → RPronounceDlg), —, 215
`Change Effects...` (P; `0x53c` param 0xffff → transition dialog for selected slide),
216 `Pick Background Color...` (P; `0x19e6` → RPickBgColorDlg for the slide show
background), —, 239 `Idea Machine...` (E; `0x19fd` → ask-save 7548 → Open dialog in the
CD `Idea Machine` folder).

**Controls**: 218 `Turn Tool Sounds On/Off` (E; toggles pref 2), 211 `Turn Text to Speech
On/Off` (toggles pref 0; when turned off posts `0x19fc` = stop speaking; enabled by the
text tool [inf]), 220 `Turn Attached Sound On/Off` (E; pref 3, then `0x118` refresh), —,
232 `Turn Manual Advance On/Off` (P; pref 5), 222 `Clean Up` (P; `0x5af` → compacts the
slide show [inf]), —, 234 `Go to Project/Paint Mode` (E: text "Go to SlideShow" → `0x10e`;
P: "Go to Paint Zone" → `0x15bf`→easel).

**Email** (E): 212 `Set Up...` (`0x19ee` → REmailConfigDlg / RInternetConfigDlg /
RParentalDlg), 110 `Send...` (`0x19d7` → DIALOG 160 then 131; enabled only if e-mail is
configured `FUN_005136c0()==1`).

**Debug** (600–610, 700–703) – **removed at start-up**. Debug ids still handled:
602 AnimateSelected→`0x19d0`, 603 AnimateAll→`0x19d1`, 604 NextPage→`0x19d3`, 605
PrevPage→`0x19d4`, 606→`0x19ce`, 702 Blankout (toggle), 703 ShowIDs.

**Small Kids Mode**: 219 `Turn Small Kids Mode On/Off` (E; pref 1, posts `0x107` with the
new value).

**Help** (flags 0x90 = right-justified / last): 213 `Help` (E: `0xef` = help mode,
P: `0x19ec`), 100 `About Kid Pix Deluxe 3` (`RAbout`, About.MHK + version info), 237
`View User Guide` (opens `KdpxManual.pdf` via the Acrobat registry key `[AcrobatReader]`).

Dynamic item texts (set with `ModifyMenu`, initial texts from resource are ignored)
[code]: "Turn Text-To-Speech On|Off", "Turn Small Kids Mode On|Off", "Turn Tool Sounds
On|Off", "Turn Attached Sound On|Off", "Turn Manual Advance On|Off", "Go to SlideShow" /
"Go to Paint Zone" – the text always names the action the item will perform.

Accelerators (RT_ACCELERATOR 128, all `FVIRTKEY|FCONTROL`) [code]:
Ctrl+Z → 201 Undo, Ctrl+X → 202, Ctrl+C → 203, Ctrl+V → 204, Ctrl+S → 102 Save As,
Ctrl+Q → 107 Exit, Ctrl+B → 702 Blankout, Ctrl+I → 703 ShowIDs (debug).
String table: 128 = "Kid Pix Deluxe 3" (window title), 130 = "%s", 3000 = "378180".

---

## 5. Dialogs

### 5.1 Native Win32 dialog templates (MS Sans Serif 8) [code]
| id | title | controls (id: text, x,y,w,h in DLUs) | used by |
|---|---|---|---|
| 129 | Spell Checker | 3998 Done(161,237) 3999 Skip(85,79) 4000 static word 4001 "Unknown word:" 4002 "Change to:" 4003 edit 4004 "Suggestions:" 4005 list(7,121,204,100) 4006 Change(7,79) 4007 Learn(161,79); size 218×258 | Check Spelling |
| 130 | Login To KidPix | 1005 Login, 1006 Cancel, 1001 "Your Name:", 1002 edit, 1003 "Users:", 1004 list | unused fallback (graphical RDlgLogin is used) [inf] |
| 131 | KidPix Email | Send/Cancel/Subject/Message/To list/Add Address…/Remove Address/Tips and Tricks…/ radio "Send as jpg (small)" 1122, "Send as swf (animates)" 1123 | Email Send |
| 134 | (child, Open-dialog template) | "Preview:" 444 preview box; radios 996 "Shrink/Stretch to Fill Canvas", 998 "Fill Canvas without Shrinking/Scaling", 997 "Center at Original Size", 999 "Repeat Many Times"; checks 886 "Preview On", 1015 "Also Add to Library" | Import Background |
| 136 | – | "Would you like to name your drawing?" edit, Yes/No | [inf] unused |
| 137 | (child, empty 341×6) | – | Open-dialog extension |
| 138 | Get Address | "Enter Email Address:" edit, OK/Cancel | Email |
| 139 | – | "Would you like to name your recording?" | [inf] unused |
| 150 | Print This Picture | group "Print Mode": 1000 Full Page, 1004 Poster, 1006 "2 x 2 Pages", 1007 "3 x 3 Pages"; 1013 OK, 1014 Cancel | Easel print |
| 151 | Print This SlideShow | Print Mode: 1000 Full Page "* 1 slide per page", 1001 Comic Book "* 6 slides per page", 1002 Card / Booklet "* 4 slides per page", 1003 Thumbnail "* 24 slides per page"; Orientation 1008 Landscape, 1009 Portrait; Slides: 1011 Starting Slide, 1012 Ending Slide, "You can choose only enough slides to fill one page."; OK/Cancel | Picker print |
| 160 | Send an Online Greeting To Your Friends | 1001 "Send a Picture", 1000 "Send a Movie", SEND / DON'T SEND + explanatory texts | Email Send step 1 |

### 5.2 Graphical dialogs (`RDlgBase` family) [code+data]
Common: file set = cfg section of the same name, background = first script, buttons =
MBTN 777 (some have more tables), modal over the current screen, rollover/press sounds via
script sound ops. **Standard command ids: 1395 Cancel, 1396 OK, 1702 Yes, 1703 No,
1433 Preview/Replay.** Button art = 4 scripts (rest, rollover, press, disabled).

| dialog (MHK) | layout & commands | purpose |
|---|---|---|
| `Warnings.MHK` (`RDlgWarnings`) | frame 7500; message art 7533–7548 (see below); MBTN 777 OK(360,310); 778 Cancel(300,305)+OK(420,305); 779 Yes(294,308)+No(414,308); 780 Yes(249)+No(358)+Cancel(470) at y 308 | prompts |
| `Deleters.MHK` | frame 7800 (193,159–607,358); CANCEL 1395 (287,290), OK 1396 (429,290); messages 7809 user / 7810 users / 7811 page / 7812 pages / 7813 project / 7814 projects | delete confirm |
| `EnterName.MHK` | frame 7200 (193,167–607,…), edit 7201 (273,246–529,…), CANCEL 1395 (289,296), OK 1396 (431,296); title art 7211 "Give your page a name:", 7210 "Give your SlideShow a name"; typing click tWAV 7200/7201 | Small-Kids save |
| `EnterSnd.MHK` | same geometry; title 7210 "name your sound" | recorder |
| `RAlertDlg.MHK` | frame 10000 with warning icon, text drawn by code in 10005, OK 1396 (358,303) | generic alert |
| `RPickBgColorDlg.MHK` | "Pick a Background Color!" preview chip; 21 swatch hotspots cmd 1000 idx 0–20 (scripts 7713–7733, 7×3 grid, 44×25 cells from 168,222); CANCEL 1395 (206,305), OK 1396 (358,305) | slide-show bg colour (§9.6) |
| `RPickBgSndDlg.MHK` | "Pick a Sound!" list box (≈480,95–745,340 art), icon 8051 (508,183) cmd 1632, PREVIEW 1631 (497,255), OK 1396 (497,325), CANCEL 1395 (497,367) | background/attached sound |
| `RABCTextDlg.MHK` | "Alphabet Text / Enter new letters…" edit; OK 1396 (251,303), CANCEL 1395 (465,303) [and 1641 same pos.] | edit `ABCString` |
| `RPronounceDlg.MHK` | "Original Word" edit, "Pronounce As" edit; ENTER 1801, ORIGINAL 1802 (speaks original), PRONOUNCE 1803 (speaks replacement), CANCEL 1395; buttons at x 478, y 191/227/263/301 | Say It This Way |
| `RFlattenDlg.MHK` | two buttons cmd 1600 (291,248) / 1601 (416,248) (labels not decodable here [?], probably "this one" / "all"), CANCEL 1395 (361,305) | flatten |
| `RHelpDlg.MHK` | frame 9000 (170,71–630,466) "Help"; text rect 9001 (230,133–570,384); OK 1396 (419,400), Replay 1433 (306,400) | help mode (§7) |
| `RUseIMDlg.MHK` | "Are you sure you want to create a new project from the Idea Generator?" CANCEL 1395 (289,296), OK 1396 (431,296) | picker idea gen |
| `RParentalDlg.MHK` | "Parental Controls" text; checkbox "Enable Kid Pix Email" cmd 1760; CANCEL/OK | e-mail setup |
| `REmailConfigDlg`, `REmailSendDlg`, `RInternetConfigDlg` | e-mail UI (not analysed in detail) | e-mail |
| `TransSound/TimerSettings/SlideShow.MHK` | §9.4 | slide options |

Warnings message art [data]: 7533 "The page can't be copied because the destination
drive is full." 7534 same for project, 7535/7536 "page/project can't be deleted because
it is in use.", 7537/7538 "The selected item couldn't be copied/deleted.", 7539 "The
project is full. You can't add additional pages. Create a new project by clicking the
"New" button.", 7540 "…create a new project from the Idea Generator?", 7541 "This name
has already been used!", **7542** leave Paint Zone/save picture, **7543** leave Project
Zone/save project, **7544** quit/save picture, **7545** log on as new user/save picture,
**7546** create new picture/save current, **7547** open different picture/save current,
**7548** open Idea Machine picture/save current.

---

## 6. Easel shell behaviour (non-tool buttons)

### 6.1 Easel button commands (Easel.MHK MBTN 75, + code-created hotspots) [code]
Handled in `REasel__v1` @0040f5b0.

| cmd | button (rest/over/press scripts, rect) | action |
|---|---|---|
| 222–237 (except 226,231) | tool ovals/tools | select tool (tool researchers) |
| **238** | 1052/1053/1054, (712,398–748,504) "sail" | **paper texture picker** (`RTexturePicker`, `FUN_00430ee0`, help key `texture`), positioned from the button with script 1052; in Small-Kids mode the Print button (script 1049) is hidden while it is open |
| **239** | 1079/1080/1081, (739,503–792,546) "?" | **help mode** (§7) |
| **242** | 1049/1050/1051, (751,436–782,465) | **Print** (print dialog 150) |
| **243** | 1041/1039/1040, (731,50–789,111) | **Play**: start all animations/sounds/movies on the page (`0x19d1`), sets playing flag |
| **244** | 1045/1043/1044, (730,50–788,113) | **Stop** (`0x19ce`), stops TTS too |
| **266** | 1089/1090/1091, (723,243–800,329) | **Firecracker – erase whole page** (`RDFirecracker2`, placed at canvas bottom-centre (x=mid, y=bottom-50) and detonated) |
| **267** | 1086/1087/1088, (749,322–800,370) door | **Exit Kid Pix** (ask-save 7544 if dirty → quit) |
| **270** | 1082/1083/1084, (741,550–798,598) | **go to SlideShow** (ask-save 7542 → module 0x15bf) |
| **281** | 1038/1036/1037, (739,115–787,163) disk | **Save** (`FUN_0040f380`: Windows "Save Picture As" loop; Small-Kids: no dialog [inf]) |
| **285** | hotspot over script 1048 (739,186–782,…); sprite 1046, press anim 1047 | **New page** (normal: ask-save 7546 then `0x19cb`; Small-Kids: EnterName dialog with current name, then save + new page) |
| **260/261** | hotspot over script 1115 (0,238–77,…), "face" sprite 1094 | **Undo guy** (click = undo/redo; face animations 1094–1116) |

MBTN 75 field 14 (group) values: tools 1, 243 → 2, 244/281 → 3, 270 → 27, 266/267 → 0.
**MBTN 76** (one button, cmd 243, scripts 1503–1505) is never loaded and its scripts do
not exist → dead data. [code+data]

### 6.2 "Do you want to save?" flow (`0x108`) [code]
`0x108` with follow-up action → RDlgWarnings with art & result id:

| follow-up | art | result msg |
|---|---|---|
| quit (0x10d) | 7544 | 0x102 |
| new page (0xf5) | 7546 | 0x103 |
| Idea Machine (0xf6) | 7548 | 0x100 |
| open other (0xf9) | 7547 | 0xfb |
| log in as new user (0x117) | 7545 | 0x101 |
| go to SlideShow (0x15bf) | 7542 | 0x106 |

Dialog answers: **Yes `0x6a6`** → save (`0x119`) then continue; **No `0x6a7`** → continue
without saving; Cancel → nothing. "Dirty" = `FUN_0047d850(page)`.

### 6.3 Save / Open [code]
* Save = `GetSaveFileNameA` (flags `0x80826`, filter `[FileTypeFilters] Page`, default
  ext `kpx`, title `[OpenDialogTitlesWin] SavePage`, start dir = user `My Pictures`),
  looped until success/cancel; errors: `[FileStrings] VolumeFull` (code 0x2849) or
  `GeneralFileErr`. The page is kept in a temp Mohawk file (`[GeneralStrings]
  TempPageFileBasename = KidpixTempFile`) and copied/committed to the chosen path
  (`FUN_0047e630`).
* Open = Windows Open, filter Page; failure → `CantOpenPictureFile` / `CantReadPictureFile`.
* Idea Machine = same with start dir = CD `Idea Machine` (mode 3).
* Small-Kids mode: errors use `[SmallKidsUserMessages]`.

---

## 7. Help mode [code+data]

1. Clicking **?** (Easel cmd 239 / Help menu / Picker cmd 1374) creates `RHelp(owner,
   "EaselHelp"|"PickerHelp")` (`FUN_004a9550`). It loads the file set, plays group/script
   **10000**, and installs up to three **help hotspot tables** (MBTN) on top of the UI:
   the screen table (EaselHelp 777 / PickerHelp 777) plus the tables for the *current tool
   panel* (EaselHelp 787…899; e.g. pencil = 827+828) or picker mode (PickerHelp 779 normal,
   778 effects view). All hotspots get the help cursors (General.MHK tCUR 10022/10023).
2. Clicking a hotspot posts its command; `RHelp__v1` maps command → help key (table below,
   extracted from the switch at 004a9980) and opens **RHelpDlg** (`FUN_004aae50`).
3. RHelpDlg: text = `KPD3.ini [HelpStrings] <key>` (≤ 512 chars) laid out in rect of script
   **9001**; voice = `KPD3.cfg [HelpSnds] <key>` → **tWAV id in `RHelpDlg.MHK`** (9000–9179),
   played immediately; **Replay (1433)** restarts it; **OK (1396)** closes.
4. On close RHelp posts `0x19ed` to the owner, which destroys RHelp → **help mode ends
   after one explanation** (one-shot). Commands 0x4e48–0x4e52 use an alternate close path
   (`0x4e56`).
5. INI duplicates: the first occurrence of a key wins (e.g. `newpage = 9024`, later
   `newpage = 9000` ignored) [inf: reader returns first match]. `Help` → 9006.

The complete table (hotspot command → key → sound → text) is in Appendix A.

---

## 8. `.kpx` picture file — byte-level spec

Verified on all 25 `Idea Machine/*.kpx` (script: every invariant below holds for every
file). Writer = `FUN_0047c8f0` (save), reader = `FUN_0047ae20` + factory `FUN_0047f060`.

### 8.1 Container: Mohawk archive (big-endian) [code+data]
```
0x00 'MHWK'  u32 fileSize-8  'RSRC'
0x0C u16 0x0100 (version)   u16 0x0001 (compaction)
0x10 u32 fileSize           u32 dirOffset (absolute, D)
0x18 u16 fileTableOffset (rel D)   u16 fileTableSize (= 4 + 10*nFiles)
0x1C 8 bytes 00 00 FF FD 00 00 00 00   (constant in all files; writer artefact, ignore on read)
0x24 resource data, packed back to back, no alignment (odd offsets occur)
D:   u16 nameListOffset (rel D), u16 typeCount
     typeCount × { tag[4], u16 resTableOff (rel D), u16 nameTableOff (rel D) }  sorted by tag
     per type: resource table { u16 n, n × {u16 id, u16 fileIndex (1-based)} }
               name table     { u16 n, n × {u16 nameOffset (rel name list), u16 fileIndex} }
     name list: NUL-terminated ASCII names
     file table: u32 nFiles, nFiles × { u32 offset, u16 sizeLo, u8 sizeHi, u8 flags(0), u16 0 }
```
Resource names: `decimal(u32 FourCC) + decimal(id)`, e.g. tBMP 1 → `"19505022241"`,
tTHM 1 → `"19516805891"`, tRem 1 → `"19515569731"`. **tWAV resources have no name.**
Order seen: file index 1 = tRem, 2 = tBMP 1, 3 = tWAV, 4 = tTHM, 5 = tBMP 2; on-disk
order tBMP1, tWAV, tTHM, tBMP2, tRem, directory. Readers must not depend on order.

### 8.2 Resources
| type | id | content |
|---|---|---|
| `tBMP` | 1 | **page raster** (paint + background), 600×452 |
| `tBMP` | 2 | **composite** (raster with every visible object flattened) – used for slide shows, previews; ignored when loading for editing |
| `tTHM` | 1 | thumbnail **96×72** of the composite |
| `tRem` | 1 | object catalogue (§8.4) – **mandatory**, loading iterates it |
| `tSWF` | n | sticker/animation object (§8.5) |
| `tTXT` | n | text box object (§8.6) |
| `tMBM` | n | floating ("moveable") bitmap object |
| `tMOV` | n | QuickTime movie object (data copied from the source file) |
| `tWAV` | 1 | the page's sound (only one per page; replaced when another is added) |

Object ids: for `tBMP` and `tWAV` one object per page; others get `max(existing id of that
type)+1` (`FUN_0047adc0`). Max stickers: `KPD3.ini [PageObjectLimits] SWFMax = 15`
(`TooManyPageObjects` message).

### 8.3 Bitmaps `tBMP`/`tTHM` (JPEG flavour, fmt 0x0F03) [data, all 25 files]
```
u16 width  (600 | 96)      u16 height (452 | 72)
u16 bytesPerRow = 2*width  u16 format = 0x0F03
'jpeg'  u32 0  u32 width*height*2 (16-bit size)  u32 jpegLength (= resSize-24)
JFIF baseline JPEG (YCbCr, JFIF 1.01, 72 dpi)
```
The engine writes it from a 16-bit DIB (`FUN_00456120`, `HBITMAP 0x10`).

### 8.4 `tRem` catalogue (`RemHeader`, writer `FUN_00483b90`, reader `FUN_004838a0`) [code+data]
```
u16 count
i16 version = -3 (0xFFFD)        ; constant set in RemHeader ctor (FUN_00483590)
u32 0
count × 22-byte entry:
   'kRem'  tag[4]  u16 id  i16 top  i16 left  i16 bottom  i16 right  u32 extra
padding: resource size is count*28+8; the last count*6 bytes are uninitialised
         (0xCD fill in all samples) – write zeros or 0xCD.
```
Reader compatibility: if the 4 bytes after `count` are `'kRem'` the file is the old
format v10 (entries start at offset 2, no `extra` field, 18-byte entries). [code]
Rect is the object's bounds on the 600×452 page in **Mac order** (top,left,bottom,right);
background tBMP → (0,0,452,600), tTHM → (0,0,72,96), tWAV with no icon → 0,0,0,0.
`extra`: 0 for bitmaps/sounds; for tSWF the packed placement word (§8.5).
Load order = catalogue order = **z-order** (later entries drawn on top). Types other than
tBMP/tSWF/tMBM/tMOV/tTXT/tWAV (e.g. tTHM) are skipped by the factory.

### 8.5 `tSWF` sticker/animation object [code]
* Data: a complete uncompressed **SWF ('FWS') file** – the movie re-serialised from memory
  (`FUN_004bad60`), so colour fills applied to the sticker are baked in.
* tRem rect = displayed bounds on the page.
* `extra` (u32, `FUN_0047f880` / `FUN_0047f840`):
  `bits 31–21 = width, bits 20–10 = height` (displayed size, 11 bits each),
  `bits 9–8 = 2-bit state` (from object+0x98 [?]), `bits 7–0 = byte` (float at +0x94
  truncated – likely current frame/phase [?]). On load, if width or height ≠ 0 the sticker
  is shown at that size.

### 8.6 `tTXT` text box object (writer `FUN_004808d0`, reader `FUN_00480af0`) [code]
Big-endian, 0x38-byte header + text:
```
0x00 u32 textLength
0x04 i16 top, i16 left, i16 bottom, i16 right   ; text box rect (Mac order)
0x0C char[32] fontFaceName (Windows face name, NUL padded)
0x2C u16 fontSize                (from editor +0x7d68)
0x2E u16 styleFlags              (from editor +0x7d98; bold/italic/underline [inf])
0x30 u16 version = 2             (stored as bytes 00 02)
0x32 4 bytes colour, native order R,G,B,flag (COLORREF-like; flag 0xFF = default) [inf]
0x36 u16 justification           (left/centre/right [inf] 0/1/2)
0x38 text bytes (Latin-1, textLength bytes, no terminator)
```
The per-box TTS voice is **not** stored in this blob [code: not in writer] – default voice
on reload [inf].

### 8.7 `tWAV` (page sound) [data]
Standard Mohawk WAVE (`'MHWK' size 'WAVE'`), here **IMA ADPCM** (encoding 1), 22050 Hz,
16-bit, mono, preceded by an `'ADPC'` chunk of 12 bytes `00 01 00 01 00 00 00 00 00 40 00 00`
(ADPCM checkpoint table: 1 item, 1 channel, sample 0, state `0x0040,0x0000`) [data]. The
sound plays when the page is opened/shown if "Attached Sound" is on (`FUN_0047dbd0`).

### 8.8 Writing a page (algorithm of `FUN_0047c8f0`) [code]
1. Ensure a background tBMP 1 object exists; write `tBMP 1` = page raster (JPEG).
2. Render the composite (raster + objects) into an offscreen; write `tTHM 1` = 96×72
   scaled composite; write `tBMP 2` = 600×452 composite.
3. For every other object not flagged "flattened": tSWF → `tSWF id` + extra word;
   tMBM → raw bitmap; tTXT → blob §8.6; tWAV/tMOV → copy resource data.
4. Write `tRem 1` (catalogue of everything written incl. tBMP 1/2 and tTHM 1), flush.
No other version field exists; `[FileStrings] WrongGameFileVersion` is not referenced by
the exe.

### 8.9 Byte-exact writer recipe (verified: rebuilds all 25 samples byte-identically) [data]
1. Header (0x1C bytes, patched last) + `00 00 FF FD 00 00 00 00`.
2. Resource bodies back to back in disk order: tBMP 1, tWAV 1, tTHM 1, tBMP 2, tRem 1
   (for pages with objects, append object resources before tRem [inf]).
3. Directory at D: 4-byte header, type entries **sorted by tag**; then per-type tables in
   order **tRem, tBMP, tWAV, tTHM** (each: resource table then name table, ids ascending);
   then the name list (names in file-index order, tWAV skipped); then the file table.
4. File-table index order: tRem=1, tBMP1=2, tWAV=3, tTHM=4, tBMP2=5; `flags`=size bits
   24–26, last u16 = 0.
5. Header: `'MHWK', total-8, 'RSRC', 0x0100, 0x0001, total, D, fileTableOff,
   4+10*nFiles`.
A reader only needs the generic Mohawk parser; the order only matters for byte-identity. Load failure → `LoadFailPrompt` ("Couldn't open this Kid Pix picture. Starting new
picture.").

### 8.10 Corrections found while implementing (`crates/kp-kpx`) [data]
Nothing above contradicted the bytes (crate rebuilds all 25 samples byte-identically). Additions:
* **Ordering rule, generalised:** disk order = tRem catalogue order, then tRem; file index 1 =
  tRem, then catalogue order; per-type tables ordered by each type's first file index. This
  reproduces the fixed lists of §8.9 and is what kp-kpx uses for pages with objects
  (default catalogue order: tBMP 1, tWAV 1, objects in z-order, tTHM 1, tBMP 2) [inf].
* **tWAV size quirks:** outer `MHWK` size = 4 + `Data` chunk size (ignores the 20-byte `ADPC`
  chunk); the `Data` chunk size *includes* its own 8-byte header; sample bytes = count/2.
* **JPEG details:** 4:4:4 (no chroma subsampling), JFIF 1.01 at 72 dpi, near-lossless quant
  tables (both: DC 2, every AC 1 ≈ quality 100), plus a `COM` segment `"AppleMark\n"` – so the shipped
  samples were encoded with Mac QuickTime, not by the game's own writer.
* In all samples `tBMP 2` is byte-identical to `tBMP 1` (no objects to flatten).

---

## 9. Slide Show ("Project Zone")

### 9.1 Picker screen (`Picker.MHK`, file set `[Picker]` = Picker + Thumbinf +
TimerSettings + Soundfx/TransitionSounds) [data+code]
Background `tBMP 5000` (800×600). Button table **MBTN 888**:

| cmd | scripts | rect | function |
|---|---|---|---|
| 5567 (`0x15bf`) | 5043/5044/5045 | 746,550–797,592 | go to Paint Zone (Easel) |
| 1363 (+1364) | 5005/5006/5004 (+5002–5007 anims) | 595,490–745,600 | trash (drag slides/projects onto it; confirm Deleters 7811–7814) |
| 1365 | 5025/5026/5027 (lever) | 29,471–74,515 | NEW slide show |
| 1370 | 5028/5029/5030 | 580,361–670,416 | PLAY slide show (plays once) |
| 1371 | 5031/5032/5033 (camera) | 579,424–638,480 | toggle **effects view** (shows per-slot transition / timing / sound icons) |
| 1372 | 5034/5035/5036 | 634,430–690,490 | Print (dialog 151) |
| 1373 | 5037/5038/5039 | 685,423–747,481 | Save slide show |
| 1374 | 5040/5041/5042 | 743,494–783,537 | Help mode |
| 1375 ×12 | hotspots, idx 0–11, rect scripts 5046–5057, 2nd cmd 1381–1392 | slot grid | select / drag slide *i*; 1381+i = per-slot "Load" |

Slot grid: 12 slides, 4 × 3, thumbnails **96×72** at x = 80,195,310,425 and y = 142,242,342
(frames 5080–5091; selected frame 5068 green with "load" icon; red frame 5092 = drop
target [inf]; empty placeholders 5058). Scroll ▲ 5060–5062 (269,110–332,125), ▼ 5063–5066
(269,436–332,451). Title (slide-show name) 5225 (154,68–430,86); selected slide name 5227
(102,463–512,484). Saved slide shows strip 5226 (104,504–556,600) with 4 project icons
(5009–5020, red = current), arrows 5217–5219 / 5220–5222. Effects-view icons per slot:
transition button 5132.. (62,159+100r … 41×40), timing label 5106–5117 (80,215+100r,
71×19), sound icon 5119–5130 (151,215+100r, 27×19); "Load" buttons 5193–5216.
Three small icons 5265/5268/5271 (top of slide area) – correspond to help keys
`newpage`/`dupepage`/`cleanup` whose sounds are placeholders (9000) → probably unused [?].
Idle animations: timer `'TOut'`, random 3000–7000 ms; picks randomly one of three
characters and a random script in [5245..5247] (dog), [5249..5256], [5258..5264]. [code]
Max slides per show: `[PortfolioSettings] MaxProjectSize = 96`.
Slot thumbnails are the pages' **`tTHM 1`** (96×72, read with `FUN_0047c8a0`); an empty
slot shows "Empty Slide". `Thumbinf.MHK` (loaded with the Picker) holds slot decorations:
tBMP 6500 (96×72), 6501 (108×79 frame), 6502–6509 small labels/icons, scripts 6500–6541
(per-slot overlays on the 4×3 grid at x 62/177/292/407, y 161/261/361) and STRL 12000
"This is a note!" [data; exact use inf].

Picker message handling (`RPicker__v1` @004269a0) [code]: 0x555 new; 0x55a play;
0x55b effects toggle; 0x55c print; 0x55d save; 0x55e help; 0x565–0x570 per-slot;
0x53c/0x53d/0x53e open Transition / Timer / Audio dialog for a slot (param = slot or
0xffff = selected); 0x58a/0x58c/0x58e results; 0x5af Clean Up; 0x19cc save-as; 0x19cd
open; 0x19d5 add; 0x19d6 export; 0x19db–0x19dd clipboard; 0x19e6 bg colour; 0x19f4 load
picture into slot.

### 9.2 `.kpp` slide-show file (`ProjectConfigFile`, `FUN_0046e2f0`) [code]
**Plain INI text** (Broderbund INI reader/writer), extension `kpp`
(`[FileTypeFilters] Project`), default dir `Users/<u>/My Pictures/My Slide Shows`.
```
[Pages]
Page_highnumber=<N>                ; "%s_highnumber"
Page1=<path>,<f0>,<f2>,<f8>,<f9>,<f4>,<f6>,<fA>
...                                 ; key "Page%u", 1-based slot number; empty slots absent
[SlideShow]
BackgroundColor=<0..20>             ; index into the colour table §9.6 (default 14 Black)
BackgroundSound=<n>
Looping=<0|1>
ManualAdvance=<0|1>
[PickerDisplay]
TopLeftPage=<n>
SelectedPage=<n>
```
`<path>` = the `.kpx` path **relative to the .kpp** (`FUN_00453c30`), cut at the first
`,`. Settings string format `"%u,%u,%u,%u,%u,%u,%u"`, fields in this order (defaults in
brackets, `FUN_0046e190`/`FUN_0046e2a0`):

| pos | slide field | default | meaning |
|---|---|---|---|
| 1 | +0xe u16 | 1 | Audio Options **Play Sound** |
| 2 | +0x10 u16 | 1 | Audio Options **Play Text** |
| 3 | +0x16 u8 | 7 | timer **seconds** (0–255) |
| 4 | +0x17 u8 | 0 | advance mode: 0 Seconds, 1 Mouse Click, 2 Key Down, 3 Read Text Done (≥5 → 0) |
| 5 | +0x12 u16 | 4 | **transition index** (0–29, §9.5; 4 = Fade) |
| 6 | +0x14 u16 | 4 | **transition sound index** (0–30, 30 = No Sound) |
| 7 | +0x18 u16 | 0 | "override" flag: ≠0 → don't play attached sound/text [inf] |

### 9.3 Dialog: Select Transition and Sound (`RTransDialogue`, TransSound.MHK + TransitionSounds.MHK) [data+code]
Frame 7000 (201,62–667,…). Transition name field 7001 (365,145–524,…); 2×6 icon grid
slots 7034–7045 (283+40c, 186/219, 28×?) showing tBMP `12000+i` (TransitionSounds.MHK,
23×21) with selection frames 7050–7061, scrollbar 7003–7011; sound name field 7002/7012;
4 sound icons 7046–7049 (278+61c, 305) selected 7062–7065, scrollbar 7013–7021.
CANCEL 1395 (267,384), PREVIEW 1433 (361,384) (plays transition + sound), OK 1396 (455,384).
Names: `STRL 7500` (transitions, 30) and `STRL 8500` (sounds, 31) in TransitionSounds.MHK;
sound *i* = `tWAV 12000+i`. Text colour RGB(250,250,255), size 14. OK writes f4/f6 and
updates the slot icon `tBMP 12000+transition`.

### 9.4 Dialogs: Timer Settings / Audio Options [data+code]
**Timer Settings** (`RDlgTiming`, TimerSettings.MHK MBTN 889, group 930): frame 17000
(219,83–667,…), radio rows Timer [N] Seconds 1407 (288,173), Mouse Click 1408 (288,214),
Key Down 1409 (288,254), Read Text Done 1411 (288,294); spinner ▲ 1410 (378,173) ▼ 1412
(378,186) (wraps 255↔0, selects Timer), seconds field 17006 accepts digits/keypad digits,
Backspace, ←, →, Del, clamped 0–255; CANCEL 1395 (298,356), OK 1396 (422,356). Labels in
the slot: `"<N> Seconds"` or `STRL 7000[mode]` ("Seconds","Mouse Click","Key Down","Text
Done","Music Done","Anim. Done","Page Arrow").
**Audio Options** (`RDlgToggle`, SlideShow.MHK): PLAY SOUND toggle 1415 (273,240), PLAY
TEXT 1416 (423,240), CANCEL 1395 (289,305), OK 1396 (431,305).

### 9.5 Transitions (QuickTime effects in `Data/Quicktimefx/*.qfx`) [data]
Each `.qfx` is a QuickTime atom container (`'sean'`): effect type `'what'` + parameters;
all tween 0→1 (`twnt=3`). Index = STRL 7500 order:

| # | name | QT effect | parameters |
|---|---|---|---|
| 0 | Barn Door | smpt (SMPTE wipe) | wpID 21 |
| 1 | Clock Face | smp3 | wpID 201 |
| 2 | Engulf | xplo | xcnt 0, 2nd tween (2,−2) |
| 3 | Explode | xplo | xcnt 0 ycnt 0 |
| 4 | Fade | dslv (dissolve) | – |
| 5 | Implode | mplo | xcnt 0 ycnt 0 |
| 6 | Iris Circle | smp2 | wpID 119, soft |
| 7 | Iris Heart | smp2 | wpID 130, soft |
| 8 | Iris Rectangle | smp2 | wpID 101, soft |
| 9 | Iris Star | smp2 | wpID 128, soft |
| 10 | Jaws | smpt | wpID 74 |
| 11 | Kaleidoscope | smp2 | wpID 102, hori 3 vert 3 (3×3 repeat), soft |
| 12 | Lawnmower | smp4 | wpID 301, soft |
| 13 | Open Sesame | smpt | wpID 47 |
| 14 | Page Tear | xplo | extra tweens (−2,2)/(−2,0) |
| 15 | Push | push | from 1 |
| 16 | Slide Down | slid | angle 0° |
| 17 | Slide Left | slid | 90° |
| 18 | Slide Right | slid | 270° |
| 19 | Slide Up | slid | 180° |
| 20 | Spiral | smp4 | wpID 310, soft |
| 21 | Spotlight | smp3 | wpID 231 |
| 22 | Waterfall | smp4 | wpID 350 |
| 23 | Windmill | smp3 | wpID 207 |
| 24 | Wipe Diagonal | smpt | wpID 41 |
| 25 | Wipe Down | smpt | wpID 2 |
| 26 | Wipe(r) | smp3 | wpID 223 |
| 27 | Wipe Right | smpt | wpID 1 |
| 28 | Wipe Up | slid | 180° (same as Slide Up – original data) |
| 29 | Zoom | zoom | zvar 1, zdst 5.0 |

(`hori/vert` = 1, `widt` = 0, border colour black for all wipes.) The wpIDs are the
standard SMPTE wipe codes; implement them natively. Transition duration is not in the
files (QuickTime default; ~1 s [?]).

### 9.6 Slide-show background colours (table @0x580228, `{char* name; u8 r,g,b,0}`) [code+data]
Values are **percent** (≥100 ⇒ 100 %); swatch art colours (RGB) given for exact rendering:

| idx | name | table | swatch RGB |
|---|---|---|---|
| 0 | White | 93,93,93 | 238,238,238 |
| 1 | Pink | 100,60,60 | 255,156,156 |
| 2 | Orange | 100,80,60 | 255,205,156 |
| 3 | Yellow | 255,255,100 | 255,255,156 (table anomaly) |
| 4 | Green | 60,100,60 | 156,255,156 |
| 5 | Blue | 60,100,100 | 156,255,255 |
| 6 | Purple | 80,80,100 | 205,205,255 |
| 7 | Grey | 47,47,47 | 115,115,115 |
| 8 | Red | 255,0,0 | 255,0,0 |
| 9 | Orange2 | 100,40,20 | 255,98,49 |
| 10 | Yellow2 | 255,255,0 | 255,255,0 |
| 11 | Green2 | 0,67,0 | 0,172,0 |
| 12 | Blue2 | 0,40,100 | 0,98,255 |
| 13 | Blue3 | 40,0,100 | 98,0,255 |
| 14 | **Black (default)** | 7,7,7 | 0,0,0 |
| 15 | Red | 67,0,0 | 172,0,0 |
| 16 | Brown | 60,20,0 | 131,49,0 |
| 17 | Brown2 | 50,75,0 | 123,131,0 |
| 18 | Green3 | 0,33,0 | 0,82,0 |
| 19 | Blue4 | 0,0,93 | 0,0,238 |
| 20 | Blue5 | 20,0,80 | 49,0,205 |
Invalid index (<0 or >20) → 14. Swatch hotspot *i* (cmd 1000, idx i) is the colour.

### 9.7 Playback (`RSlideShowModule` + sequencer `RTransition` @00485650) [code]
* Start: Picker PLAY (`0x55a`): only if at least one slot is filled; sets pref PlayOnce=1;
  switches to module `0x15c2`, which posts `0xdf9` (param = start slide or 0). Menu ids
  229/230 (not in the menu) would start once/looped.
* Screen: whole 800×600 filled with the background colour; each slide drawn at
  **`[CanvasSettings] slideLeft=100, slideTop=74`, 600×452**. The image shown is the
  page's **`tBMP 2` composite**; the page is also loaded so its animations run
  (`0x19d1`), its attached sound plays if Play Sound (f0) and the text boxes are read by
  TTS if Play Text (f2).
* Each step: load next filled slot (skipping empty), draw it into the back buffer, run
  the slide's transition (f4) old→new with transition sound `tWAV 12000+f6` (none if 30),
  then wait per mode (f9): 0 → `f8` seconds (timer), 1 → mouse click, 2 → any key,
  3 → TTS of the slide's text finished (only if Play Text; otherwise acts as 1).
  Menu **Manual Advance** (pref 5) disables timer/text-done advancing and makes click/key
  always advance.
* Keys: **Esc or Q** → stop, back to Picker. Other key → advance if mode 2 or manual.
  Mouse click → advance if mode 1 or manual.
* End: if the sequencer's loop flag (`+0x38`, set from PlayOnce==0 [inf]) is clear and the
  show was started from the beginning (`+0x2c == 0`) → rewind and continue from slide 1;
  otherwise stop (`0xdfa`) → Picker. Started from a specific slide → always stops at the
  end. [code for the condition, inf for the flag's origin]
* Sequencer states (`+0x30`): `0x800000` init/first slide, `0x8000` fetch next,
  `0x80000` drawing, `0x400000` waiting for event, `0x10000` waiting for timer
  (`timeGetTime() >= start + seconds*1000`), `0x200000` advance requested,
  `0x40000` stop. Advance events: 0 timer, 1 mouse, 2 key, 3 text done, 4 music done,
  5 anim done (0,3,5 ignored while Manual Advance is on). [code]

---

## 10. Sound recording (`RAudioPanel` + `RRecord`) [code]
* Recording panel = sound library panel (Easel oval 4 "audio") in "record" mode
  (help keys RecStart/RecStop/RecPlay/RecSndMeter/RecSpeedSlider/RecSaveSnd/RecTrashCan,
  help table 819). Panel details belong to the tool-panel spec.
* Format negotiation (`FUN_00491ba0`), PCM via `waveInOpen`, tried in order:
  requested (initially 0 ⇒ fails) → **11025 Hz/16-bit** → 22050/16 → 22050/8 → 44100/8 →
  11025/8 → 44100/16; channels as requested (mono). Buffer = bytes/s × **10 s max**
  (`RAudioPanel+0x3e4 = 10`), even length.
* Default name "Untitled"; save → `RDlgEnterSnd` "name your sound" → stored in the user's
  `My Sounds/My Sounds.klb` (tWAV + tTHM + tATX) [inf]; speed slider = playback rate
  (16.16 fixed, default 1.0 at `+0x2b0`) [inf].
* The sound can be dragged onto the page → becomes the page's tWAV object (§8.7,
  re-encoded to IMA ADPCM 22 kHz [inf from samples]).

---

## 11. Text-to-speech [code]
* Engine: **SAPI 4** through Broderbund's COM wrapper **`TTSServer.TTSObject`**
  (`INSTALL/Data/TTSServer.dll`, installed with `spchapi.exe`, `tv_enua.exe` (L&H
  TruVoice US English), `msttsL.exe`). Voices exposed by TTSServer: *Adult Male #1,
  Adult Female #1, Adult Female #2 (TruVoice)* (+ whatever SAPI voices exist) [data];
  missing engine → `[HardwareStrings] WindowsSpeechMissing`.
* Pronunciation exceptions: per-user **`Words.dic`** in the user folder, loaded on first
  speak; edited with **Say It This Way** (RPronounceDlg: ENTER stores
  original→pronounce-as, ORIGINAL/PRONOUNCE speak them).
* What gets spoken: a text box's text when its speaker ("Read Text", help 8779/`TTSAll`
  reads the whole page) is clicked (`FUN_00408910`); text strings via `FUN_00408970`
  (letters/words typed in the text tool [inf]); slide show "Play Text"
  (`FUN_004377c0`, completion drives mode 3). Global toggle = pref 0 (menu 211); Stop
  button / turning TTS off stops speech.
* `usa86.lex` + `engphon.env` are **not TTS**: they are the Franklin/Proximity
  spelling lexicon for *Check Spelling* (DIALOG 129, user words in `user.dct`); format in
  FORMATS.md "Spelling lexicon".

---

## 12. Printing (`RPrintDialog`, `RPrintJob` family, `FUN_004b07c0`) [code]
* Dialog 150 (Easel) or 151 (Picker), then the Windows print path (`NoPrinterInstalled`
  message if none). Job selected by print mode: 0 Full Page (`RPrintJob`), 1 Comic
  (`RComicJob`), 2 Card/Booklet (`RCardJob`), 3 Thumbnail (`RThumbnailJob`), 4 Poster
  (`RPosterJob`, n = 2 or 3). Then orientation, start, end slide are passed.
* Source = the page composite (600×452). Layouts (printable rect W×H, cell aspect kept,
  image height = cell width × 452/600, vertically centred):
  * **Full Page**: one picture per sheet.
  * **Poster n×n**: n² sheets; sheet k (1-based, row-major) prints source tile
    `(col=(k-1)%n, row=(k-1)/n)` of size (600/n × 452/n) enlarged to the page.
  * **Card/Booklet** (4 per sheet): quadrants; slot 1 = top-left, 2 = bottom-left,
    3 = bottom-right, 4 = top-right (fold order), slots 3–4 shifted by W/8 [inf meaning].
  * **Comic Book** (6): 3 columns × 2 rows, slot 1–3 top row, 4–6 bottom row, small
    gutter (`+0x26 >> 4`).
  * **Thumbnail** (24): 4 columns × 6 rows, row-major.
* Slide-show printing is limited to one sheet's worth of slides (dialog text).

---

## 13. Differences vs current engine (what to replace)
* `main.rs`: invented menu bar → use §4 exactly (8 menus, Debug removed, enable states
  per module, dynamic texts, accelerators).
* `setup.rs`/startup: implement Splash → graphical Login (§2.3) → Easel/Picker; users =
  sub-folders of `<data>/Users`.
* `slideshow.rs`: replace the invented editor with the Picker screen (§9.1), the three
  slide dialogs, `.kpp` INI format (§9.2) and the player rules (§9.7).
* `easel.rs`: correct button meanings: 266 = firecracker erase-all (not undo), 267 = exit
  app (not "back to picker"), 270 = go to SlideShow, 242 = print, 238 = paper texture,
  285 = new page, 260/261 = undo guy hotspot (left face).
* Save/load: implement §8 (round-trip verified on the 25 samples).

---

## 14. Unknowns / open points
* Exact tick unit of tSCR tempo (100 ⇒ 100 ms assumed from splash music length) and the
  meaning of script op 9.
* ~~RFlattenDlg button labels~~ — resolved: cmd 1600 = **SELECTED**, 1601 = **ALL** (title "What should be flattened?"); the art cels lack bit 15, which is not a hide flag.
* tSWF `extra` low 10 bits semantics; tTXT style/justification bit values; colour byte
  order (assumed COLORREF R,G,B).
* Transition duration and QuickTime SMPTE pattern rendering details (wpID tables).
* Whether DELETE in admin mode removes user folders from disk.
* E-mail / Internet dialogs (REmail*, RInternetConfigDlg) not analysed.
* Picker's three top icons (5265/5268/5271), "Clean Up" exact semantics, project strip
  drag/drop details, "SlideShow To Go" export format.
* Small-Kids-mode UI differences beyond New Page/Save (other researchers' tool panels).

---

## Appendix A — help hotspots → help key → voice → text
Generated from `RHelp__v1` (004a9980) + EaselHelp/PickerHelp MBTN + KPD3.cfg
`[HelpSnds]` (first occurrence) + KPD3.ini `[HelpStrings]`. tWAV ids are in
`Data/RHelpDlg.MHK`. "not in any help MBTN" = mapped in code but no hotspot uses it.

| hotspot cmd | help key | tWAV (RHelpDlg) | hotspot table(s) / rect (first) | text (KPD3.ini [HelpStrings], truncated) |
|---|---|---|---|---|
| 787 | BackgrdPreLibFolders | 9031 | EaselHelp 787 (101, 547, 256, 600) | Select a folder name to see different kinds of backgrounds. To sele... |
| 788 | BackgrdGraphicSelectIcons | 9033 | EaselHelp 788 (320, 518, 704, 583) | Click a background to select it, then drag it on to your page. |
| 797 | StickPreLibFolders | 9038 | EaselHelp 797 (101, 547, 256, 600) | Select a folder name to see different kinds of stickers. To select ... |
| 798 | StickGraphicSelectIcons | 9039 | EaselHelp 798 (320, 518, 704, 583) | Click a sticker to select it, then drag it on to your page. |
| 807 | AnimaPreLibFolders | 9035 | EaselHelp 807 (101, 547, 256, 600) | Select a folder name to see different kinds of animations. To selec... |
| 808 | AnimaGraphicSelectIcons | 9036 | EaselHelp 808 (320, 518, 704, 583) | Click an animation to select it, then drag it on to your page. |
| 817 | SoundPreLibFolders | 9041 | EaselHelp 817 (101, 547, 256, 600) | Select a folder name to see different kinds of sounds. To select a ... |
| 818 | RecTrashCan | 9055 | EaselHelp 818 (308, 519, 343, 552) | Erase recorded sounds by selecting them, then clicking on the trash... |
| 819 | RecStart | 9047 | EaselHelp 819 (360, 527, 390, 552) | Click this button to begin recording. |
| 828 | LineSize | 9100 | EaselHelp 828 (314, 515, 376, 586) | Click to change the size of the line you draw. |
| 829 | LineSize | 9100 | EaselHelp 829 (314, 515, 376, 586) | Click to change the size of the line you draw. |
| 830 | LineSize | 9100 | EaselHelp 830 (314, 515, 376, 586) | Click to change the size of the line you draw. |
| 831 | LineSize | 9100 | EaselHelp 831 (314, 515, 376, 586) | Click to change the size of the line you draw. |
| 838 | PaintBrushSizes | 9112 | EaselHelp 838 (314, 515, 376, 586) | Click to change the size of your paint brush. |
| 839 | WackyBrushSizes | 9117 | EaselHelp 839 (314, 515, 376, 586) | Click to change the size of your paint brush. |
| 840 | SpraySizes | 9122 | EaselHelp 840 (314, 515, 376, 586) | Click to change the size of your spray can. |
| 848 | FillPatternIcons | 9094 | EaselHelp 848 (320, 518, 704, 583) | Click a pattern and then click on your page to use it. |
| 849 | FillPatternIcons | 9094 | EaselHelp 849 (320, 518, 704, 583) | Click a pattern and then click on your page to use it. |
| 850 | FillPatternIcons | 9094 | EaselHelp 850 (320, 518, 704, 583) | Click a pattern and then click on your page to use it. |
| 858 | BigMixerStyles | 9128 | EaselHelp 858 (382, 519, 699, 584) | Click a Mixer to select it, and then click your picture. |
| 859 | MixerWandStyles | 9130 | EaselHelp 859 (382, 519, 699, 584) | Click a mixer effect to select it, then click on your picture to ap... |
| 860 | MixerWandSizing | 9142 | EaselHelp 859 (349, 519, 384, 586) | Click to change the size of the Mixer Wand effects. |
| 868 | StampSizing | 9145 | EaselHelp 868 (349, 519, 384, 586) | Click here to change the size of your stamp. |
| 888 | TrashCan | 9066 | EaselHelp 888 (308, 519, 343, 586) | Select something on your page and click the trash to cut it out of ... |
| 889 | TrashCan | 9066 | EaselHelp 889 (308, 519, 343, 586) | Select something on your page and click the trash to cut it out of ... |
| 898 | EraserSz | 9071 | EaselHelp 898 (314, 515, 376, 586) | Click to change the size of the Mini Eraser. |
| 899 | EraserSz | 9071 | not in any help MBTN | Click to change the size of the Mini Eraser. |
| 7872 | FolderScroller | 9178 | EaselHelp 787 (258, 511, 292, 599) | Use the arrows to see more folder names. |
| 7881 | BGFilelabltxtfield | 9034 | EaselHelp 788 (476, 587, 613, 600) | The name of the selected background is shown here. |
| 7981 | STFilelabltxtfield | 9040 | EaselHelp 798 (476, 587, 613, 600) | The name of the selected sticker is shown here. |
| 8071 | FolderLabel | 9179 | EaselHelp 807 (101, 510, 256, 544) | This is the name of the selected folder. |
| 8072 | FolderScroller | 9178 | EaselHelp 807 (258, 511, 292, 599) | Use the arrows to see more folder names. |
| 8081 | ToonFilelabltxtfield | 9037 | EaselHelp 808 (476, 587, 613, 600) | The name of the selected animation is shown here. |
| 8171 | FolderLabel | 9179 | EaselHelp 817 (101, 510, 256, 544) | This is the name of the selected folder. |
| 8181 | ShowRecSoundMenu | 9045 | EaselHelp 818 (345, 519, 380, 586) | Click to record your own sounds. |
| 8182 | SoundGraphSelectIcons | 9043 | EaselHelp 818 (384, 520, 701, 585) | To add sound to your picture, click and drag it onto your page.  Ti... |
| 8183 | RemoveSoundPage | 9177 | EaselHelp 818 (308, 553, 343, 586) | Click here to remove the selected sound from your page. |
| 8191 | RecStop | 9048 | EaselHelp 819 (392, 527, 422, 552) | Click to stop a sound or recording. |
| 8192 | RecPlay | 9049 | EaselHelp 819 (424, 527, 454, 552) | Click to play the sound you recorded. |
| 8193 | RecSndMeter | 9050 | EaselHelp 819 (360, 555, 453, 575) | This wiggly line shows how much time you have used to record a sound. |
| 8194 | RecSpeedSlider | 9052 | EaselHelp 819 (468, 552, 561, 572) | Click and drag the button to speed up or slow down your sound. |
| 8196 | HideRecSoundMenu | 9046 | EaselHelp 819 (611, 519, 646, 586) | Click to see the sound libraries. |
| 8197 | RecSaveSnd | 9053 | EaselHelp 819 (559, 552, 589, 577) | Click to save a sound into your "My Sounds" folder. |
| 8271 | PencilTool | 9096 | EaselHelp 827 (105, 504, 145, 600) | The Pencil tool draws solid lines and shapes. |
| 8272 | ChalkTool | 9097 | EaselHelp 827 (152, 504, 192, 600) | The Chalk tool draws smooth lines and shapes. |
| 8273 | CrayonTool | 9098 | EaselHelp 827 (198, 504, 238, 600) | The Crayon tool draws fuzzy lines and shapes. |
| 8274 | MarkerTool | 9099 | EaselHelp 827 (248, 504, 288, 600) | The Marker tool draws tinted lines and shapes. |
| 8281 | DrawFreehand | 9101 | EaselHelp 828 (381, 515, 443, 586) | Click to draw freely. |
| 8282 | DrawLine | 9102 | EaselHelp 828 (451, 509, 513, 547) (+3 more) | Click to draw a straight line.  Set the cursor at the starting poin... |
| 8283 | DrawArc | 9103 | EaselHelp 828 (451, 549, 513, 587) (+3 more) | Click and drag to make a line, then let go of the mouse button and ... |
| 8284 | DrawBox | 9104 | EaselHelp 828 (514, 509, 576, 547) (+3 more) | Click to draw a box. |
| 8285 | DrawBoxFilled | 9105 | EaselHelp 828 (514, 549, 576, 587) (+3 more) | Click to draw a filled box. |
| 8286 | DrawOvalOutline | 9106 | EaselHelp 828 (577, 509, 639, 547) (+3 more) | Click to draw a circle. |
| 8287 | DrawOvalFilled | 9107 | EaselHelp 828 (577, 549, 639, 587) (+3 more) | Click to draw a filled circle. |
| 8288 | DrawPolyOutline | 9108 | EaselHelp 828 (640, 509, 702, 547) (+3 more) | Click to draw a shape using lines. Each mouse click creates a corne... |
| 8289 | DrawPolyFilled | 9109 | EaselHelp 828 (640, 549, 702, 587) (+3 more) | Click to draw a filled shape. Each mouse click creates a corner for... |
| 8291 | DrawFreehand | 9101 | EaselHelp 829 (381, 515, 443, 586) | Click to draw freely. |
| 8292 | DrawLine | 9102 | not in any help MBTN | Click to draw a straight line.  Set the cursor at the starting poin... |
| 8293 | DrawArc | 9103 | not in any help MBTN | Click and drag to make a line, then let go of the mouse button and ... |
| 8294 | DrawBox | 9104 | not in any help MBTN | Click to draw a box. |
| 8295 | DrawBoxFilled | 9105 | not in any help MBTN | Click to draw a filled box. |
| 8296 | DrawOvalOutline | 9106 | not in any help MBTN | Click to draw a circle. |
| 8297 | DrawOvalFilled | 9107 | not in any help MBTN | Click to draw a filled circle. |
| 8298 | DrawPolyOutline | 9108 | not in any help MBTN | Click to draw a shape using lines. Each mouse click creates a corne... |
| 8299 | DrawPolyFilled | 9109 | not in any help MBTN | Click to draw a filled shape. Each mouse click creates a corner for... |
| 8301 | DrawFreehand | 9101 | EaselHelp 830 (381, 515, 443, 586) | Click to draw freely. |
| 8302 | DrawLine | 9102 | not in any help MBTN | Click to draw a straight line.  Set the cursor at the starting poin... |
| 8303 | DrawArc | 9103 | not in any help MBTN | Click and drag to make a line, then let go of the mouse button and ... |
| 8304 | DrawBox | 9104 | not in any help MBTN | Click to draw a box. |
| 8305 | DrawBoxFilled | 9105 | not in any help MBTN | Click to draw a filled box. |
| 8306 | DrawOvalOutline | 9106 | not in any help MBTN | Click to draw a circle. |
| 8307 | DrawOvalFilled | 9107 | not in any help MBTN | Click to draw a filled circle. |
| 8308 | DrawPolyOutline | 9108 | not in any help MBTN | Click to draw a shape using lines. Each mouse click creates a corne... |
| 8309 | DrawPolyFilled | 9109 | not in any help MBTN | Click to draw a filled shape. Each mouse click creates a corner for... |
| 8311 | DrawFreehand | 9101 | EaselHelp 831 (381, 515, 443, 586) | Click to draw freely. |
| 8312 | DrawLine | 9102 | not in any help MBTN | Click to draw a straight line.  Set the cursor at the starting poin... |
| 8313 | DrawArc | 9103 | not in any help MBTN | Click and drag to make a line, then let go of the mouse button and ... |
| 8314 | DrawBox | 9104 | not in any help MBTN | Click to draw a box. |
| 8315 | DrawBoxFilled | 9105 | not in any help MBTN | Click to draw a filled box. |
| 8316 | DrawOvalOutline | 9106 | not in any help MBTN | Click to draw a circle. |
| 8317 | DrawOvalFilled | 9107 | not in any help MBTN | Click to draw a filled circle. |
| 8318 | DrawPolyOutline | 9108 | not in any help MBTN | Click to draw a shape using lines. Each mouse click creates a corne... |
| 8319 | DrawPolyFilled | 9109 | not in any help MBTN | Click to draw a filled shape. Each mouse click creates a corner for... |
| 8371 | PaintBrush | 9111 | EaselHelp 837 (97, 520, 159, 591) | Use the Paint Brush tools to add realistic and wacky paint to your ... |
| 8372 | Microphone | 9116 | EaselHelp 837 (162, 518, 224, 589) | Use the Sound Art tools to add paint to your picture using your voice. |
| 8373 | SprayCan | 9121 | EaselHelp 837 (230, 524, 292, 595) | Use the Spray Can tools to spray shapes or moving paint onto your p... |
| 8381 | RealPaintOpts | 9113 | EaselHelp 838 (381, 513, 443, 551) | Click to see different realistic paint tool options. |
| 8382 | WackyPaint | 9114 | EaselHelp 838 (381, 552, 443, 590) | Click to see different wacky paint tool options. |
| 8383 | paint | 9110 | EaselHelp 838 (451, 519, 702, 584) | Paint with Paint Brush, Sound Art and Spray Can tools. |
| 8391 | MousePaint | 9118 | EaselHelp 839 (381, 513, 443, 551) | Click to paint with your voice and the mouse. |
| 8392 | HandsfreePaint | 9119 | EaselHelp 839 (381, 552, 443, 590) | Click to paint with your voice without using the mouse. |
| 8393 | WackyPaintBrushStyleOpts | 9120 | EaselHelp 839 (451, 519, 702, 584) | Click to choose a style of paint to add to your picture. |
| 8401 | SprayShapesPaint | 9123 | EaselHelp 840 (381, 513, 443, 551) | Click to paint with shapes. |
| 8402 | SprayMovingPaint | 9124 | EaselHelp 840 (381, 552, 443, 590) | Click to paint with moving paint. |
| 8403 | SprayPaintBrushStyleOpts | 9125 | EaselHelp 840 (451, 519, 702, 584) | Click to choose a style of paint to add to your picture. |
| 8471 | SolidFill | 9091 | EaselHelp 847 (100, 515, 162, 586) | This tool will fill an area with the selected color and pattern. |
| 8472 | GradientFill | 9092 | EaselHelp 847 (165, 523, 227, 594) | This tool will fill an area with a blend of colors.  Hold down the ... |
| 8473 | PatternFill | 9093 | EaselHelp 847 (230, 520, 292, 591) | This tool will fill an area with a multicolor pattern. |
| 8571 | BigMixer | 9127 | EaselHelp 857 (100, 511, 235, 596) | Transform your whole picture with one mouse click. |
| 8572 | MixerWand | 9129 | EaselHelp 857 (238, 512, 288, 597) | Smudge, smear, warp or twist your picture with the Mixer Wand. |
| 8581 | MixerWandSizing | 9142 | EaselHelp 858 (349, 519, 384, 586) | Click to change the size of the Mixer Wand effects. |
| 8671 | StampFolder | 9143 | EaselHelp 867 (101, 547, 256, 600) | Select a folder name to see different kinds of stamps. To select a ... |
| 8672 | FolderLabel | 9179 | EaselHelp 867 (101, 510, 256, 544) | This is the name of the selected folder. |
| 8673 | FolderScroller | 9178 | EaselHelp 867 (258, 511, 292, 599) | Use the arrows to see more folder names. |
| 8681 | StampEditTool | 9144 | EaselHelp 868 (388, 519, 423, 586) | Use this tool to edit the selected stamp. |
| 8682 | StampThumbs | 9132 | EaselHelp 868 (427, 520, 668, 585) | Click a stamp to select it, then click on your picture to add the s... |
| 8771 | FontPopUpMenu | 9076 | EaselHelp 877 (109, 526, 220, 552) | Click to see different kinds of letters. |
| 8772 | SizePopUpMenu | 9077 | EaselHelp 877 (241, 526, 268, 552) | Click to change the size of your words. |
| 8773 | TextStyles | 9089 | EaselHelp 877 (117, 564, 188, 594) | Click to make your text bold, italic or underlined. |
| 8774 | AlignSettings | 9078 | EaselHelp 877 (200, 562, 286, 593) | Click to make the words move to the left, middle or right of the box. |
| 8775 | TextCut | 9081 | EaselHelp 877 (308, 519, 343, 586) | Use the Scissor tool to cut text from a selected text box. |
| 8776 | TextCopy | 9079 | EaselHelp 877 (345, 519, 380, 552) | Select a text box and then click this button to make a copy of it. |
| 8777 | TextPaste | 9080 | EaselHelp 877 (345, 553, 380, 586) | Click Paste to stick something you cut or copied to your picture. |
| 8778 | VoicesPopUp | 9086 | EaselHelp 877 (451, 542, 588, 562) | Click to change the speaking voice for this box. |
| 8779 | TTSAll | 9083 | EaselHelp 877 (409, 536, 444, 569) | This button reads the words on your page out loud. |
| 8871 | GrabTool | 9062 | EaselHelp 887 (105, 515, 191, 600) | Use this tool to move stickers, animations and text boxes around. |
| 8872 | CutTool | 9063 | EaselHelp 887 (198, 514, 284, 599) | Use this tool to cut, copy and paste selected parts of your picture. |
| 8881 | Duplicate | 9064 | EaselHelp 888 (345, 519, 380, 552) | Select something on your page and then click this button to make a ... |
| 8882 | Merge | 9065 | EaselHelp 888 (345, 553, 380, 586) | Click Paste to stick something you cut or copied on to your picture. |
| 8891 | Duplicate | 9064 | EaselHelp 889 (345, 519, 380, 552) | Select something on your page and then click this button to make a ... |
| 8892 | Merge | 9065 | EaselHelp 889 (345, 553, 380, 586) | Click Paste to stick something you cut or copied on to your picture. |
| 8893 | GrabCutSettings | 9067 | EaselHelp 889 (384, 520, 701, 585) | Choose an option for selecting part of your picture. |
| 8971 | SmEraser | 9070 | EaselHelp 897 (111, 512, 188, 599) | Choose the Mini Eraser to erase part of your picture by hand. |
| 8972 | BigEraser | 9073 | EaselHelp 897 (194, 513, 271, 600) | The Big Eraser clears your whole picture with one click. |
| 8981 | EraserShapes | 9072 | EaselHelp 898 (381, 515, 443, 586) | Click to change the shape of the Mini Eraser. |
| 8991 | EraserShapes | 9072 | not in any help MBTN | Click to change the shape of the Mini Eraser. |
| 8992 | BigEraserSettings | 9074 | EaselHelp 899 (451, 519, 702, 584) | Choose a Big Eraser effect and click on your picture to clear the P... |
| 19002 | backgrounds | 9027 | EaselHelp 777 (8, 43, 83, 86) | Click to add a Background to your picture. |
| 19004 | stickers | 9028 | EaselHelp 777 (16, 93, 91, 136) | Click to add a sticker to your picture. |
| 19006 | toons | 9029 | EaselHelp 777 (23, 141, 98, 184) | Click to add an Animation to your picture. |
| 19008 | audio | 9030 | EaselHelp 777 (8, 188, 83, 231) | Click to add a sound to your picture, or record your own sound. |
| 19010 | pencil | 9095 | EaselHelp 777 (4, 283, 48, 328) | Use these tools to draw lines and shapes. |
| 19012 | paint | 9110 | EaselHelp 777 (53, 288, 97, 333) | Paint with Paint Brush, Sound Art and Spray Can tools. |
| 19014 | fill | 9090 | EaselHelp 777 (4, 338, 49, 383) | Use the Paint Buckets to fill areas with colors or patterns. |
| 19016 | mix | 9126 | EaselHelp 777 (54, 338, 99, 383) | Use these electric mixer tools to transform all or part of your pic... |
| 19018 | Stamps | 9131 | EaselHelp 777 (6, 390, 50, 435) | Add Rubber Stamps to your picture. |
| 19020 | text | 9075 | EaselHelp 777 (54, 391, 99, 436) | Use the Text tool to add words to your page. |
| 19022 | grab | 9061 | EaselHelp 777 (0, 444, 47, 489) | Use these tools for grabbing, cutting, and moving parts of your pic... |
| 19024 | eraser | 9069 | EaselHelp 777 (54, 442, 99, 487) | Use these tools to erase part or all of your picture. |
| 19026 | Save | 9025 | EaselHelp 777 (740, 112, 786, 157) | Click to save this picture. |
| 19032 | play | 9020 | EaselHelp 777 (734, 51, 782, 104) | Click to play the animations, sounds and movies in your picture. |
| 19034 | stop | 9021 | not in any help MBTN | Click to stop the animations, sounds and movies in your picture. |
| 19036 | newpage | 9024 | EaselHelp 777 (737, 185, 783, 222) | Click to start a new picture. |
| 19038 | print | 9023 | EaselHelp 777 (752, 434, 785, 468) | Click to print this picture. |
| 19040 | texture | 9139 | EaselHelp 777 (710, 399, 750, 504) | Click to change the paper texture.  Use Chalk, Crayon or Marker to ... |
| 19042 | OptionsTrayNum | 9056 | EaselHelp 777 (715, 542, 735, 561) | This number shows which option tray you're using. |
| 19044 | picker | 9022 | EaselHelp 777 (748, 551, 791, 593) | Click to go to SlideShow. |
| 19046 | Help | 9006 | EaselHelp 777 (741, 506, 784, 543) | Click this button, then click on any part of the screen you need he... |
| 19048 | FixedColorsToggle | 9136 | EaselHelp 777 (19, 540, 62, 588) | Use the Color Picker to choose a color.  Click on a color swatch, o... |
| 19050 | Undo | 9068 | EaselHelp 777 (29, 240, 72, 281) | Click here to undo or redo the last thing you did to your picture. |
| 19052 | FirecrackerErase | 9147 | EaselHelp 777 (729, 257, 797, 325) | Click here to erase your whole page. |
| 19053 | PaintExit | 9146 | EaselHelp 777 (728, 341, 796, 409) | Click here to exit Kid Pix. |
| 19056 | FolderList | 9057 | EaselHelp 788 (706, 512, 736, 537) (+14 more) | Click to see more choices. |
| 19058 | FolderList | 9057 | EaselHelp 788 (706, 568, 736, 593) (+14 more) | Click to see more choices. |
| 20002 | title | 9007 | PickerHelp 777 (104, 61, 483, 93) | The name of your open SlideShow is shown here. |
| 20010 | trash | 9003 | PickerHelp 777 (577, 485, 705, 600) | Erase slides and SlideShows by dragging them to the trash. |
| 20012 | ProjectFileIconScrollBar | 9012 | PickerHelp 777 (117, 499, 532, 580) | This is a saved SlideShow.  Click on it to open it. |
| 20014 | new | 9011 | PickerHelp 777 (4, 475, 64, 583) | Click this button to start a new SlideShow. |
| 20016 | pickplay | 9016 | PickerHelp 777 (577, 361, 668, 412) | Click here to Play your SlideShow. |
| 20018 | slide | 9014 | PickerHelp 777 (576, 423, 633, 476) | Click to choose your own SlideShow effects. |
| 20020 | pickprint | 9015 | PickerHelp 777 (632, 434, 682, 487) | Click to print one or more slides from this SlideShow. |
| 20022 | pickersave | 9013 | PickerHelp 777 (686, 423, 738, 475) | Click to save your SlideShow. |
| 20026 | easel | 9000 | PickerHelp 777 (739, 544, 788, 591) | Click this button to go to the Paint Zone. |
| 20028 | thumbnails | 9008 | not in any help MBTN | Click and drag slides to move them around. Click the "Load" button ... |
| 20030 | thumbup | 9001 | PickerHelp 777 (261, 104, 337, 127) | Click to see more slides in your SlideShow. |
| 20032 | thumbdn | 9002 | PickerHelp 777 (262, 430, 332, 453) | Click to see more slides in your SlideShow. |
| 20033 | thumbnails | 9008 | PickerHelp 779 (80, 142, 176, 214) (+11 more) | Click and drag slides to move them around. Click the "Load" button ... |
| 20034 | transition | 9017 | PickerHelp 778 (62, 159, 103, 199) (+11 more) | Click here to pick a sound and transition for this slide. |
| 20036 | timing | 9018 | PickerHelp 778 (80, 215, 151, 234) (+11 more) | Click here to choose Page advance options for each slide. |
| 20038 | sounds | 9019 | PickerHelp 778 (151, 215, 178, 234) (+11 more) | Click here to set audio options for each slide. |
| 20039 | clickload | 9140 | PickerHelp 779 (92, 215, 163, 234) (+11 more) | Click here to load a picture into this thumbnail. |
| 20040 | ProjectFileArrow | 9004 | PickerHelp 777 (64, 520, 108, 561) | Click to see more of your SlideShows. |
| 20042 | ProjectFileArrow | 9004 | PickerHelp 777 (526, 519, 571, 558) | Click to see more of your SlideShows. |
| 20044 | pagename | 9005 | PickerHelp 777 (87, 461, 569, 483) | The name of the currently selected slide is shown here. |
| 20046 | cleanup | 9000 | not in any help MBTN | (none) |
| 20048 | newpage | 9024 | not in any help MBTN | Click to start a new picture. |
| 20050 | dupepage | 9000 | not in any help MBTN | (none) |

Help hotspot tables in `EaselHelp.MHK` (which table is active depends on the tool panel):
777 = Easel screen (tool ovals, Save, Play/Stop, New page, Print, texture, tray number,
SlideShow, Help, colour picker, Undo, firecracker, exit); 787/797/807/817 = background /
sticker / animation / sound library folder area, 788/798/808/818 = their item trays,
819 = sound recorder; 827–831 = pencil family + line sizes / shapes; 837–840 = paint
brush / sound art / spray can; 847–850 = fill; 857–859 = mixer; 867–868 = stamps;
877 = text panel; 887–889 = grab/cut; 897–899 = eraser. `PickerHelp.MHK`: 777 = Picker
screen, 778 = effects view (transition/timing/sound per slot), 779 = normal view
(thumbnails + Load buttons).

---

## Appendix B — MBTN record layout (from loader `FUN_00405ae0` / `FUN_00405770`) [code]
32 bytes = 16 × u16 BE:

| field | meaning |
|---|---|
| 0 | state-0 script (rest) |
| 1 | state-1 script (rollover) |
| 2 | state-2 script (if ≠0: button kind 1, e.g. "on"/selected) |
| 3 | state-3 script (if [2]==0 and ≠0: kind 2) |
| 4 | hit shape: 1 = union of the state scripts' bounds, 2 = Mac region `MRGN` [5] |
| 5 | MRGN id (shape 2) |
| 6 | command id (posted as message id) |
| 7 | 1 = button; **5 = hotspot record** (no sprite; rect from script [10]) |
| 8, 9 | passed to the button init (hotspot: [8] = index, [9] = flag) |
| 10 | press script (hotspot: rect script) |
| 11, 12, 13 | hotspot params; [12] = second command (e.g. Picker per-slot 1381+i) |
| 14 | group id (show/hide/enable as a group; also cursor group) |
| 15 | 0 |

Disabled look = script `rest+3` for dialog buttons (art convention). Script sound op
index *i* plays `tWAV (archive base id + i)`.

## Appendix C — raw button tables (dumped)

**RDlgLogin.MHK**

| MBTN | f0 rest | f1 over | f2 | f3 | f4 | f6 cmd | f7 kind | f8 | f10 press | f12 | f14 group | rect of f0/f10 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 777 | 7616 | 7617 | 0 | 0 | 1 | 1500 | 1 | 0 | 7618 | 0 | 1500 | (491, 202, 574, 231) |
| 777 | 7632 | 7633 | 0 | 0 | 1 | 1502 | 1 | 0 | 7634 | 0 | 1500 | (491, 380, 574, 409) |
| 777 | 7660 | 7661 | 0 | 0 | 1 | 1395 | 1 | 0 | 7662 | 0 | 1500 | (491, 380, 574, 409) |
| 778 | 7632 | 7633 | 0 | 0 | 1 | 1502 | 1 | 0 | 7634 | 0 | 1600 | (491, 380, 574, 409) |
| 778 | 7620 | 7621 | 0 | 0 | 1 | 1603 | 1 | 0 | 7622 | 0 | 1600 | (491, 202, 574, 231) |
| 778 | 7624 | 7625 | 0 | 0 | 1 | 1604 | 1 | 0 | 7626 | 0 | 1600 | (491, 236, 574, 265) |
| 778 | 7628 | 7629 | 0 | 0 | 1 | 1606 | 1 | 0 | 7630 | 0 | 1600 | (491, 270, 574, 299) |

**Picker.MHK**

| MBTN | f0 rest | f1 over | f2 | f3 | f4 | f6 cmd | f7 kind | f8 | f10 press | f12 | f14 group | rect of f0/f10 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 888 | 5043 | 5044 | 0 | 0 | 1 | 5567 | 1 | 0 | 5045 | 0 | 1492 | (746, 550, 797, 592) |
| 888 | 5005 | 5006 | 0 | 0 | 1 | 1363 | 1 | 0 | 5004 | 1364 | 912 | (595, 490, 745, 600) |
| 888 | 5025 | 5026 | 0 | 0 | 1 | 1365 | 1 | 0 | 5027 | 0 | 913 | (29, 471, 74, 515) |
| 888 | 5028 | 5029 | 0 | 0 | 1 | 1370 | 1 | 0 | 5030 | 0 | 915 | (580, 361, 670, 416) |
| 888 | 5031 | 5032 | 5032 | 5032 | 1 | 1371 | 1 | 0 | 5033 | 0 | 916 | (579, 424, 638, 480) |
| 888 | 5034 | 5035 | 0 | 0 | 1 | 1372 | 1 | 0 | 5036 | 0 | 917 | (634, 430, 690, 490) |
| 888 | 5037 | 5038 | 0 | 0 | 1 | 1373 | 1 | 0 | 5039 | 0 | 918 | (685, 423, 747, 481) |
| 888 | 5040 | 5041 | 5041 | 5041 | 1 | 1374 | 1 | 0 | 5042 | 0 | 919 | (743, 494, 783, 537) |
| 888 | 0 | 0 | 0 | 0 | 1 | 1375 | 5 | 0 | 5046 | 1381 | 920 | (71, 142, 73, 222) |
| 888 | 0 | 0 | 0 | 0 | 1 | 1375 | 5 | 1 | 5047 | 1382 | 920 | (184, 142, 186, 222) |
| 888 | 0 | 0 | 0 | 0 | 1 | 1375 | 5 | 2 | 5048 | 1383 | 920 | (299, 142, 301, 222) |
| 888 | 0 | 0 | 0 | 0 | 1 | 1375 | 5 | 3 | 5049 | 1384 | 920 | (415, 142, 417, 222) |
| 888 | 0 | 0 | 0 | 0 | 1 | 1375 | 5 | 4 | 5050 | 1385 | 920 | (71, 243, 73, 315) |
| 888 | 0 | 0 | 0 | 0 | 1 | 1375 | 5 | 5 | 5051 | 1386 | 920 | (184, 243, 186, 315) |
| 888 | 0 | 0 | 0 | 0 | 1 | 1375 | 5 | 6 | 5052 | 1387 | 920 | (299, 243, 301, 315) |
| 888 | 0 | 0 | 0 | 0 | 1 | 1375 | 5 | 7 | 5053 | 1388 | 920 | (415, 243, 417, 315) |
| 888 | 0 | 0 | 0 | 0 | 1 | 1375 | 5 | 8 | 5054 | 1389 | 920 | (71, 342, 73, 414) |
| 888 | 0 | 0 | 0 | 0 | 1 | 1375 | 5 | 9 | 5055 | 1390 | 920 | (184, 342, 186, 414) |
| 888 | 0 | 0 | 0 | 0 | 1 | 1375 | 5 | 10 | 5056 | 1391 | 920 | (299, 342, 301, 414) |
| 888 | 0 | 0 | 0 | 0 | 1 | 1375 | 5 | 11 | 5057 | 1392 | 920 | (415, 342, 417, 414) |

**Deleters.MHK**

| MBTN | f0 rest | f1 over | f2 | f3 | f4 | f6 cmd | f7 kind | f8 | f10 press | f12 | f14 group | rect of f0/f10 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 777 | 7801 | 7802 | 0 | 0 | 1 | 1395 | 1 | 0 | 7803 | 0 | 0 | (287, 290, 370, 319) |
| 777 | 7805 | 7806 | 0 | 0 | 1 | 1396 | 1 | 0 | 7807 | 0 | 0 | (429, 290, 512, 319) |

**EnterName.MHK**

| MBTN | f0 rest | f1 over | f2 | f3 | f4 | f6 cmd | f7 kind | f8 | f10 press | f12 | f14 group | rect of f0/f10 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 777 | 7202 | 7203 | 0 | 0 | 1 | 1395 | 1 | 0 | 7204 | 0 | 0 | (289, 296, 372, 325) |
| 777 | 7206 | 7207 | 0 | 0 | 1 | 1396 | 1 | 0 | 7208 | 0 | 0 | (431, 296, 514, 325) |

**Warnings.MHK**

| MBTN | f0 rest | f1 over | f2 | f3 | f4 | f6 cmd | f7 kind | f8 | f10 press | f12 | f14 group | rect of f0/f10 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 777 | 7501 | 7502 | 0 | 0 | 1 | 1396 | 1 | 0 | 7503 | 0 | 1500 | (360, 310, 443, 339) |
| 778 | 7509 | 7510 | 0 | 0 | 1 | 1396 | 1 | 0 | 7511 | 0 | 1600 | (420, 305, 503, 334) |
| 778 | 7505 | 7506 | 0 | 0 | 1 | 1395 | 1 | 0 | 7507 | 0 | 1600 | (300, 305, 383, 334) |
| 779 | 7513 | 7514 | 0 | 0 | 1 | 1702 | 1 | 0 | 7515 | 0 | 1700 | (294, 308, 377, 337) |
| 779 | 7517 | 7518 | 0 | 0 | 1 | 1703 | 1 | 0 | 7519 | 0 | 1700 | (414, 308, 497, 337) |
| 780 | 7521 | 7522 | 0 | 0 | 1 | 1702 | 1 | 0 | 7523 | 0 | 1700 | (249, 308, 332, 337) |
| 780 | 7525 | 7526 | 0 | 0 | 1 | 1703 | 1 | 0 | 7527 | 0 | 1700 | (358, 308, 441, 337) |
| 780 | 7529 | 7530 | 0 | 0 | 1 | 1395 | 1 | 0 | 7531 | 0 | 1700 | (470, 308, 553, 337) |

**TransSound.MHK**

| MBTN | f0 rest | f1 over | f2 | f3 | f4 | f6 cmd | f7 kind | f8 | f10 press | f12 | f14 group | rect of f0/f10 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 777 | 7022 | 7023 | 0 | 0 | 1 | 1395 | 1 | 0 | 7024 | 0 | 0 | (267, 384, 350, 413) |
| 777 | 7026 | 7027 | 0 | 0 | 1 | 1396 | 1 | 0 | 7028 | 0 | 0 | (455, 384, 538, 413) |
| 777 | 7030 | 7031 | 0 | 0 | 1 | 1433 | 1 | 0 | 7032 | 0 | 0 | (361, 384, 444, 413) |

**TimerSettings.MHK**

| MBTN | f0 rest | f1 over | f2 | f3 | f4 | f6 cmd | f7 kind | f8 | f10 press | f12 | f14 group | rect of f0/f10 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 889 | 17027 | 17028 | 0 | 17028 | 1 | 1395 | 1 | 0 | 17029 | 0 | 0 | (298, 356, 381, 385) |
| 889 | 17031 | 17032 | 0 | 17032 | 1 | 1396 | 1 | 0 | 17033 | 0 | 0 | (422, 356, 505, 385) |
| 889 | 17002 | 17003 | 0 | 17003 | 1 | 1407 | 1 | 0 | 17004 | 0 | 930 | (288, 173, 506, 199) |
| 889 | 17015 | 17016 | 0 | 17016 | 1 | 1408 | 1 | 0 | 17017 | 0 | 930 | (288, 214, 414, 238) |
| 889 | 17019 | 17020 | 0 | 17020 | 1 | 1409 | 1 | 0 | 17021 | 0 | 930 | (288, 254, 399, 278) |
| 889 | 17023 | 17024 | 0 | 17024 | 1 | 1411 | 1 | 0 | 17025 | 0 | 930 | (288, 294, 447, 322) |
| 889 | 17007 | 17008 | 0 | 0 | 1 | 1410 | 1 | 0 | 17009 | 0 | 930 | (378, 173, 397, 186) |
| 889 | 17011 | 17012 | 0 | 0 | 1 | 1412 | 1 | 0 | 17013 | 0 | 930 | (378, 186, 397, 199) |

**SlideShow.MHK**

| MBTN | f0 rest | f1 over | f2 | f3 | f4 | f6 cmd | f7 kind | f8 | f10 press | f12 | f14 group | rect of f0/f10 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 777 | 7009 | 7010 | 0 | 0 | 1 | 1395 | 1 | 0 | 7011 | 0 | 0 | (289, 305, 372, 334) |
| 777 | 7013 | 7014 | 0 | 0 | 1 | 1396 | 1 | 0 | 7015 | 0 | 0 | (431, 305, 514, 334) |
| 777 | 7001 | 7002 | 7002 | 7002 | 1 | 1415 | 1 | 0 | 7003 | 0 | 0 | (273, 240, 395, 264) |
| 777 | 7005 | 7006 | 7006 | 7006 | 1 | 1416 | 1 | 0 | 7007 | 0 | 0 | (423, 240, 537, 264) |

**RPickBgColorDlg.MHK**

| MBTN | f0 rest | f1 over | f2 | f3 | f4 | f6 cmd | f7 kind | f8 | f10 press | f12 | f14 group | rect of f0/f10 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 777 | 7701 | 7702 | 0 | 0 | 1 | 1395 | 1 | 0 | 7703 | 0 | 0 | (206, 305, 289, 334) |
| 777 | 7705 | 7706 | 0 | 0 | 1 | 1396 | 1 | 0 | 7707 | 0 | 0 | (358, 305, 441, 334) |
| 777 | 0 | 0 | 0 | 0 | 1 | 1000 | 5 | 0 | 7713 | 0 | 0 | (168, 222, 214, 249) |
| 777 | 0 | 0 | 0 | 0 | 1 | 1000 | 5 | 1 | 7714 | 1 | 0 | (212, 222, 258, 249) |
| 777 | 0 | 0 | 0 | 0 | 1 | 1000 | 5 | 2 | 7715 | 2 | 0 | (256, 222, 302, 249) |
| 777 | 0 | 0 | 0 | 0 | 1 | 1000 | 5 | 3 | 7716 | 3 | 0 | (300, 222, 346, 249) |
| 777 | 0 | 0 | 0 | 0 | 1 | 1000 | 5 | 4 | 7717 | 4 | 0 | (344, 222, 390, 249) |
| 777 | 0 | 0 | 0 | 0 | 1 | 1000 | 5 | 5 | 7718 | 5 | 0 | (388, 222, 434, 249) |
| 777 | 0 | 0 | 0 | 0 | 1 | 1000 | 5 | 6 | 7719 | 6 | 0 | (432, 222, 478, 249) |
| 777 | 0 | 0 | 0 | 0 | 1 | 1000 | 5 | 7 | 7720 | 7 | 0 | (168, 247, 214, 274) |
| 777 | 0 | 0 | 0 | 0 | 1 | 1000 | 5 | 8 | 7721 | 8 | 0 | (212, 247, 258, 274) |
| 777 | 0 | 0 | 0 | 0 | 1 | 1000 | 5 | 9 | 7722 | 9 | 0 | (256, 247, 302, 274) |
| 777 | 0 | 0 | 0 | 0 | 1 | 1000 | 5 | 10 | 7723 | 10 | 0 | (300, 247, 346, 274) |
| 777 | 0 | 0 | 0 | 0 | 1 | 1000 | 5 | 11 | 7724 | 11 | 0 | (344, 247, 390, 274) |
| 777 | 0 | 0 | 0 | 0 | 1 | 1000 | 5 | 12 | 7725 | 12 | 0 | (388, 247, 434, 274) |
| 777 | 0 | 0 | 0 | 0 | 1 | 1000 | 5 | 13 | 7726 | 13 | 0 | (432, 247, 478, 274) |
| 777 | 0 | 0 | 0 | 0 | 1 | 1000 | 5 | 14 | 7727 | 14 | 0 | (168, 272, 214, 299) |
| 777 | 0 | 0 | 0 | 0 | 1 | 1000 | 5 | 15 | 7728 | 15 | 0 | (212, 272, 258, 299) |
| 777 | 0 | 0 | 0 | 0 | 1 | 1000 | 5 | 16 | 7729 | 16 | 0 | (256, 272, 302, 299) |
| 777 | 0 | 0 | 0 | 0 | 1 | 1000 | 5 | 17 | 7730 | 17 | 0 | (300, 272, 346, 299) |
| 777 | 0 | 0 | 0 | 0 | 1 | 1000 | 5 | 18 | 7731 | 18 | 0 | (344, 272, 390, 299) |
| 777 | 0 | 0 | 0 | 0 | 1 | 1000 | 5 | 19 | 7732 | 19 | 0 | (388, 272, 434, 299) |
| 777 | 0 | 0 | 0 | 0 | 1 | 1000 | 5 | 20 | 7733 | 20 | 0 | (432, 272, 478, 299) |

**RPickBgSndDlg.MHK**

| MBTN | f0 rest | f1 over | f2 | f3 | f4 | f6 cmd | f7 kind | f8 | f10 press | f12 | f14 group | rect of f0/f10 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 777 | 8063 | 8064 | 0 | 8066 | 1 | 1395 | 1 | 0 | 8065 | 0 | 1630 | (497, 367, 580, 396) |
| 777 | 8059 | 8060 | 0 | 8062 | 1 | 1396 | 1 | 0 | 8061 | 0 | 1630 | (497, 325, 580, 354) |
| 777 | 8055 | 8056 | 0 | 8058 | 1 | 1631 | 1 | 0 | 8057 | 0 | 1630 | (497, 255, 580, 284) |
| 777 | 8051 | 8052 | 0 | 8054 | 1 | 1632 | 1 | 0 | 8053 | 0 | 1630 | (508, 183, 568, 236) |

**RPronounceDlg.MHK**

| MBTN | f0 rest | f1 over | f2 | f3 | f4 | f6 cmd | f7 kind | f8 | f10 press | f12 | f14 group | rect of f0/f10 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 777 | 6501 | 6502 | 0 | 6504 | 1 | 1801 | 1 | 0 | 6503 | 0 | 1800 | (478, 191, 561, 220) |
| 777 | 6505 | 6506 | 0 | 0 | 1 | 1802 | 1 | 0 | 6507 | 0 | 1800 | (478, 227, 561, 256) |
| 777 | 6509 | 6510 | 0 | 0 | 1 | 1803 | 1 | 0 | 6511 | 0 | 1800 | (478, 263, 561, 292) |
| 777 | 6513 | 6514 | 0 | 6516 | 1 | 1395 | 1 | 0 | 6515 | 0 | 1800 | (478, 301, 561, 330) |

**RFlattenDlg.MHK**

| MBTN | f0 rest | f1 over | f2 | f3 | f4 | f6 cmd | f7 kind | f8 | f10 press | f12 | f14 group | rect of f0/f10 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 777 | 8001 | 8002 | 0 | 0 | 1 | 1395 | 1 | 0 | 8003 | 0 | 1650 | (361, 305, 444, 334) |
| 777 | 8005 | 8006 | 0 | 0 | 1 | 1600 | 1 | 0 | 8007 | 0 | 1650 | (291, 248, 388, 277) |
| 777 | 8009 | 8010 | 0 | 0 | 1 | 1601 | 1 | 0 | 8011 | 0 | 1650 | (416, 248, 499, 277) |

**RABCTextDlg.MHK**

| MBTN | f0 rest | f1 over | f2 | f3 | f4 | f6 cmd | f7 kind | f8 | f10 press | f12 | f14 group | rect of f0/f10 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 777 | 8209 | 8210 | 0 | 8212 | 1 | 1395 | 1 | 0 | 8211 | 0 | 1640 | (465, 303, 548, 332) |
| 777 | 8201 | 8202 | 0 | 8204 | 1 | 1396 | 1 | 0 | 8203 | 0 | 1640 | (251, 303, 334, 332) |
| 777 | 8209 | 8210 | 0 | 8212 | 1 | 1641 | 1 | 0 | 8211 | 0 | 1640 | (465, 303, 548, 332) |

**RHelpDlg.MHK**

| MBTN | f0 rest | f1 over | f2 | f3 | f4 | f6 cmd | f7 kind | f8 | f10 press | f12 | f14 group | rect of f0/f10 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 777 | 9002 | 9003 | 0 | 0 | 1 | 1396 | 1 | 0 | 9004 | 0 | 1700 | (419, 400, 502, 429) |
| 777 | 9006 | 9007 | 0 | 0 | 1 | 1433 | 1 | 0 | 9008 | 0 | 1700 | (306, 400, 389, 429) |

**RAlertDlg.MHK**

| MBTN | f0 rest | f1 over | f2 | f3 | f4 | f6 cmd | f7 kind | f8 | f10 press | f12 | f14 group | rect of f0/f10 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 777 | 10001 | 10002 | 0 | 0 | 1 | 1396 | 1 | 0 | 10003 | 0 | 1500 | (358, 303, 441, 332) |

**RParentalDlg.MHK**

| MBTN | f0 rest | f1 over | f2 | f3 | f4 | f6 cmd | f7 kind | f8 | f10 press | f12 | f14 group | rect of f0/f10 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 777 | 9005 | 9006 | 0 | 0 | 1 | 1395 | 1 | 0 | 9007 | 0 | 1750 | (304, 454, 387, 483) |
| 777 | 9009 | 9010 | 0 | 0 | 1 | 1396 | 1 | 0 | 9011 | 0 | 1750 | (410, 454, 493, 483) |
| 777 | 9001 | 9002 | 9002 | 9002 | 1 | 1760 | 1 | 0 | 9003 | 0 | 1750 | (300, 423, 495, 448) |

**RUseIMDlg.MHK**

| MBTN | f0 rest | f1 over | f2 | f3 | f4 | f6 cmd | f7 kind | f8 | f10 press | f12 | f14 group | rect of f0/f10 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 777 | 8005 | 8006 | 0 | 0 | 1 | 1396 | 1 | 0 | 8007 | 0 | 0 | (431, 296, 514, 325) |
| 777 | 8001 | 8002 | 0 | 0 | 1 | 1395 | 1 | 0 | 8003 | 0 | 0 | (289, 296, 372, 325) |

Easel `MBTN 75/76`: see `docs/FORMATS.md` and §6.1 (command meanings corrected there).

## Appendix D — reproducing the art
Python helpers used for this document (not part of the repo): Mohawk parser, bitmap
decoder (incl. LZ-packed `tBMH` sets, 8-bit with `tPAL`), script first-frame compositor
(`screen = bounds.topLeft + (x,y) - REGS[cel]`). Rendering the first frame of every MBTN
rest script over the first script of a dialog archive reproduces each dialog exactly as
described above.
