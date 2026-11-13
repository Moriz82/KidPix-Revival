//! The easel screen: original UI art and button scripts read from the user's CD, plus
//! the engine's own painting tools.

use crate::anim::Player;
use crate::buttons::UiButton;
use crate::assets::{self, Assets, SpriteSet};
use crate::audio::{self, Audio};
use crate::gfx::{self, Image};
use crate::library::{LibDrawer, LibKind};
use crate::paint::{self, Canvas, MediaStroke, Rng, CANVAS_H, CANVAS_W, CANVAS_X, CANVAS_Y, WHITE};
use crate::tools::{self, FillKind, PadBank, Pen, Stroke, ToolKind};
use crate::trays::{self, CutShape, DrawTool, GrabAction, GrabTool, Item, PaintTool, Shape, Tray, G};
use crate::ui::{self, Fonts};
use mohawk::Sound;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

pub const EASEL: &str = "Data/Easel.MHK";
/// The New Picture button (star paper in the right-hand oval, scripts 1046-1048) has no
/// MBTN entry in the CD's table; the engine adds it with this private command id.
const NEW_PICTURE_CMD: u16 = 0xfff0;

/// Requests from the easel to the application shell.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Save,
    SaveAs,
    Open,
    New,
    Quit,
    AddMovie,
    Export,
    Login,
    AddGraphic,
    ImportBackground,
    ImportSound,
    EditAbcText,
    CheckSpelling,
    SayItThisWay,
    IdeaMachine,
    UserGuide,
    /// Help mode (the "?" button).
    Help,
    /// Name and keep the recording in "My Sounds" (EnterSnd dialog).
    SaveRecording,
    /// Email > Set Up: the Parental Controls dialog (RParentalDlg).
    EmailSetup,
    /// Email > Send: save the picture (JPEG) or page (.kpx) and hand it to the mail program.
    EmailSend { movie: bool },
    /// Save the picture to this path (Small Kids Mode names it with EnterName).
    SaveTo(std::path::PathBuf),
    /// Print with a mode: 1 = full page, 2 / 3 = poster of n×n sheets.
    PrintPoster(u8),
    /// Ask what to flatten (RFlattenDlg): 225 text, 214 stickers and animations.
    Flatten(u16),
    /// Go to the SlideShow (after the save prompt).
    GoSlideShow,
}

pub use crate::objects::PageObj;
use crate::objects::Handle;



enum Overlay {
    None,
    /// Picture-placement options for Add Graphic / Import Background.
    Fit(FitTarget, Image, usize),
    /// "Print This Picture" (DIALOG 150).
    Print(crate::win32::PrintDialog),
    /// The Rubber Stamp editor for stamp `id`.
    StampEditor(Box<crate::stampedit::StampEditor>, u16),
    /// Email > Send (DIALOG 160).
    Email(crate::win32::EmailDialog),
    /// Check Spelling: the dialog and the misspelt word (text box, byte range).
    Spell(Box<crate::win32::SpellDialog>, (usize, usize, usize)),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FitTarget {
    Graphic,
    Background,
}

/// DIALOG 134 radio buttons, in dialog order (996, 998, 997, 999); the index is the
/// `fit_picture` mode.
pub const FIT_CHOICES: [&str; 4] = [
    "Shrink/Stretch to Fill Canvas",
    "Fill Canvas without Shrinking/Scaling",
    "Center at Original Size",
    "Repeat Many Times",
];

pub struct Easel {
    assets: Rc<Assets>,
    audio: Rc<Audio>,
    pub canvas: Canvas,
    pub fonts: Fonts,
    bg: Option<Rc<Image>>,
    buttons: Vec<UiButton>,
    hover_btn: Option<usize>,
    decor: Vec<Player>,
    splat: Option<Player>,
    /// Play button's disabled art (Easel script 1042).
    play_disabled: Option<Player>,
    /// Easel tBMP 7000/7001: the colour splat masks.
    splat_mask: HashMap<u16, mohawk::Indexed>,
    undo_face: Option<Player>,
    undo_talk: Option<Player>,
    effects: Vec<Player>,
    drawer_frame: Option<Player>,
    slot_frames: Option<Player>,
    tool: ToolKind,
    trays: HashMap<ToolKind, Tray>,
    panel: Option<crate::panels::Panel>,
    shutter: Option<crate::panels::Shutter>,
    /// Space is held (selects the mixers' alternate effects).
    pub space: bool,
    big_erase: Option<BigErase>,
    tray_icons: Vec<Vec<Image>>,
    stamp_folder: usize,
    stamp_archive: Option<mohawk::Archive>,
    sound_art: Option<crate::record::Recorder>,
    pub color: u32,
    stroke: Stroke,
    mouse: (i32, i32),
    down: bool,
    overlay: Overlay,
    drawer: Option<LibDrawer>,
    /// The library panel (RLibPanel / RAudioPanel) in the bottom strip.
    lib_panel: Option<crate::panels::Panel>,
    lib_sel: Option<usize>,
    lib_drag: Option<usize>,
    /// Sound panel in record mode, the recorder and the last recording.
    lib_rec: bool,
    recorder: Option<crate::record::Recorder>,
    pub recording: Option<std::sync::Arc<mohawk::Sound>>,
    rec_meter: Option<Player>,
    /// The ABC/alphabet text (`user.ini [GlobalUser] ABCString`).
    pub abc_string: String,
    /// [UndoStrings] grfx / txTy: what Undo undoes.
    undo_words: (&'static str, &'static str),
    /// Say It This Way pronunciations (`Words.dic`).
    pub words: Vec<(String, String)>,
    /// The stamp-library popup list (Stamp panel name field): hovered row.
    lib_popup: Option<Option<usize>>,
    /// Small Kids alphabet (`user.ini [SmallKidsAlphabet]`): letters, font, size.
    pub alphabet: Vec<char>,
    pub alphabet_font: String,
    pub alphabet_size: f32,
    /// The logged-in user's "My Pictures" folder.
    pub user_pictures: Option<std::path::PathBuf>,
    /// The logged-in user's folder.
    pub user_dir: Option<std::path::PathBuf>,
    pub objects: Vec<PageObj>,
    drag_obj: Option<(usize, i32, i32)>,
    armed: Option<(Vec<Image>, f32, Option<Rc<[u8]>>)>,
    armed_movie: Option<Rc<crate::movie::Movie>>,
    scale_drag: Option<(usize, Handle, f32, i32, i32)>,
    handles: Option<Rc<SpriteSet>>,
    movie_time: f32,
    playing: bool,
    page_sound: Option<Arc<Sound>>,
    /// Text boxes on the page and the one being typed into: (index, caret).
    pub texts: Vec<crate::textbox::TextBox>,
    /// Check Spelling (learned words come from the artist's `user.dct`).
    spell: Option<crate::spell::Checker>,
    /// Alerts waiting for the shell's RAlertDlg.
    pub alerts: Vec<String>,
    /// The original brush engine (docs/RE_BRUSHES.md via kp-brushes).
    brush_assets: Rc<kp_brushes::BrushAssets>,
    brush_cfg: Rc<kp_brushes::BrushConfig>,
    brush: Option<(kp_brushes::BrushId, kp_brushes::BrushTool)>,
    brush_held: bool,
    brush_sounds: HashMap<(&'static str, u16), u64>,
    /// Paper texture index n (Easel tBMP 17000+n, 0 = none) for the brush engine.
    paper_index: u16,
    /// Logged-in artist's name (the ABC Wacky Brush writes it).
    pub artist_name: String,
    /// The `.kpx` page this picture came from (keeps the parts the engine doesn't edit).
    pub kpx_base: Option<kp_kpx::Page>,
    /// Text Undo: (box, text before the typing run, caret) and whether a run is open.
    text_undo: Option<(usize, String, usize)>,
    typing_run: bool,
    /// Milliseconds since start (caret blink).
    clock: f32,
    editing: Option<(usize, usize)>,
    drag_text: Option<(usize, i32, i32)>,
    text_bold: bool,
    text_italic: bool,
    text_underline: bool,
    text_align: u8,
    text_clip: String,
    text_font: usize,
    text_size: f32,
    stamp: Option<Image>,
    stamp_scale: u32,
    pads: PadBank,
    rng: Rng,
    hover_time: f32,
    pub actions: Vec<Action>,
    dynamite_clear: bool,
    menubar: crate::menubar::MenuBar,
    picker: crate::colorpicker::ColorPicker,
    /// The page's paper texture (Paper Texture button, cmd 238).
    paper: Option<Rc<Image>>,
    dropper_preview: Option<u32>,
    menu_press: bool,
    pub menu_state: crate::menubar::MenuState,
    selected: Option<usize>,
    clipboard: Option<PageObj>,
    about: Option<Player>,
}

// Engine-drawn regions, placed inside panels painted in the original background art.
const OPT_ICON: u32 = 48;
const SLOT_X0: i32 = 330;
const SLOT_Y0: i32 = 435;
const SLOT_W: i32 = 51;
const SLOT_COUNT: usize = 7;
const FACE_RECT: (i32, i32, i32, i32) = (0, 238, 77, 53);
const COLOR_RECT: (i32, i32, i32, i32) = (0, 491, 85, 109);


impl Easel {
    pub fn new(assets: Rc<Assets>, audio: Rc<Audio>) -> Easel {
        let fonts = Fonts::load(&assets.font_files());
        let menu_face = crate::ui::UI_BITMAP;
        let menubar = crate::menubar::MenuBar::new(&fonts, menu_face);
        let bg = assets.image(EASEL, 1000);
        let mut buttons = crate::buttons::load(&assets, EASEL, 75);
        {
            let mk = |s: u16| Player::new(&assets, EASEL, s);
            let idle = mk(1046);
            let rect = idle.as_ref().map(|p| p.bounds()).unwrap_or_default();
            let mut b = UiButton { cmd: NEW_PICTURE_CMD, group: 0, value: 0, idle, hover: mk(1047), click: mk(1048), on: None, rect, showing: 0 };
            b.show(0, false);
            buttons.push(b);
        }
        let held = |id: u16| {
            Player::new(&assets, EASEL, id).map(|mut p| {
                p.finish_now();
                p
            })
        };
        let decor: Vec<Player> = [1118u16, 1093].iter().filter_map(|&i| held(i)).collect();
        let splat = held(1092);
        let undo_face = held(1094);
        let drawer_frame = held(1055);
        let slot_frames = held(1056);
        let pads = PadBank::load(&assets);
        let mut e = Easel {
            canvas: Canvas::new(),
            fonts,
            bg,
            buttons,
            hover_btn: None,
            decor,
            splat,
            play_disabled: held(1042),
            splat_mask: [7000u16, 7001]
                .into_iter()
                .filter_map(|id| Some((id, assets.archive(EASEL)?.get(assets::T_BMP, id).and_then(|d| mohawk::decode_indexed(d).ok())?)))
                .collect(),
            undo_face,
            undo_talk: None,
            effects: Vec::new(),
            drawer_frame,
            slot_frames,
            tool: ToolKind::Pencil,
            trays: HashMap::new(),
            panel: None,
            shutter: None,
            space: false,
            big_erase: None,
            tray_icons: Vec::new(),
            stamp_folder: 0,
            stamp_archive: None,
            sound_art: None,
            color: gfx::rgb(0, 0, 0),
            stroke: Stroke::None,
            mouse: (0, 0),
            down: false,
            overlay: Overlay::None,
            abc_string: "Kid Pix is fun!".to_string(),
            undo_words: {
                let leak = |k: &str, d: &str| -> &'static str { Box::leak(assets.ini.get("UndoStrings", k).unwrap_or(d).to_string().into_boxed_str()) };
                (leak("grfx", "drawing"), leak("txTy", "Typing"))
            },
            words: Vec::new(),
            lib_popup: None,
            alphabet: ALPHABET.chars().collect(),
            alphabet_font: "vogel".into(),
            alphabet_size: 36.0,
            user_pictures: None,
            user_dir: None,
            drawer: None,
            lib_panel: None,
            lib_sel: None,
            lib_drag: None,
            lib_rec: false,
            recorder: None,
            recording: None,
            rec_meter: None,
            objects: Vec::new(),
            drag_obj: None,
            armed: None,
            armed_movie: None,
            scale_drag: None,
            handles: assets.sprites("Data/Buttons.MHK", 7000),
            movie_time: 0.0,
            playing: false,
            page_sound: None,
            texts: Vec::new(),
            spell: None,
            alerts: Vec::new(),
            brush_assets: Rc::new(kp_brushes::BrushAssets::new(&assets.root)),
            brush_cfg: Rc::new(kp_brushes::BrushConfig::load(&assets.root).unwrap_or_default()),
            brush: None,
            brush_held: false,
            brush_sounds: HashMap::new(),
            paper_index: 0,
            artist_name: String::new(),
            kpx_base: None,
            text_undo: None,
            typing_run: false,
            clock: 0.0,
            editing: None,
            drag_text: None,
            text_bold: false,
            text_italic: false,
            text_underline: false,
            text_align: 0,
            text_clip: String::new(),
            text_font: 0,
            text_size: 12.0,
            stamp: None,
            stamp_scale: 1,
            pads,
            rng: Rng::new(0x5eed_1234),
            hover_time: 0.0,
            actions: Vec::new(),
            dynamite_clear: false,
            menubar,
            picker: crate::colorpicker::ColorPicker::new(),
            paper: None,
            dropper_preview: None,
            menu_press: false,
            menu_state: {
                let leak = |k: &str, d: &str| -> &'static str { Box::leak(assets.ini.get("UndoStrings", k).unwrap_or(d).to_string().into_boxed_str()) };
                crate::menubar::MenuState {
                    tool_sounds: true,
                    text_to_speech: true,
                    attached_sound: true,
                    undo_word: leak("UndoMenuItem", "Undo"),
                    redo_word: leak("RedoMenuItem", "Redo"),
                    undo_what: leak("grfx", "drawing"),
                    ..Default::default()
                }
            },
            selected: None,
            clipboard: None,
            about: None,
            assets,
            audio,
        };
        // The original text tool starts in Arial 12 (or the closest installed family).
        e.text_font = e.fonts.find("Arial").or_else(|| e.fonts.find("Liberation Sans")).unwrap_or_else(|| e.fonts.ui_face());
        e.select_tool(ToolKind::Pencil, false);
        e
    }

    pub fn assets_rc(&self) -> Rc<Assets> {
        self.assets.clone()
    }

    pub fn assets_root(&self) -> std::path::PathBuf {
        self.assets.root.clone()
    }

    /// An original message from KPD3.ini (`\n` → line break), or `default`.
    pub fn ini_text(&self, section: &str, key: &str, default: &str) -> String {
        self.assets.ini.get(section, key).unwrap_or(default).replace("\\n", "\n")
    }

    /// A problem to report in the original alert dialog (RAlertDlg); the shell shows it.
    pub fn alert(&mut self, s: impl Into<String>) {
        self.alerts.push(s.into());
    }


    fn play_wav(&self, archive: &str, id: u16, ch: u8) {
        if !self.menu_state.tool_sounds && (ch == audio::CH_UI || ch == audio::CH_TOOL) {
            return;
        }
        if let Some(s) = self.assets.sound(archive, id) {
            self.audio.play(s, ch, false);
        }
    }

    fn play_script_sounds(&self, archive: &str, base: u16, idx: &[u16]) {
        for &i in idx {
            self.play_wav(archive, base + i, audio::CH_UI);
        }
    }

    // ------------------------------------------------------------------ tools & trays

    fn select_tool(&mut self, tool: ToolKind, from_click: bool) {
        self.commit_text();
        self.end_brush();
        self.stroke = Stroke::None;
        let changed = tool != self.tool || self.panel.is_none();
        self.tool = tool;
        self.rebuild_tray();
        if changed && from_click {
            self.shutter = Some(crate::panels::Shutter::new(&self.assets));
        } else {
            self.rebuild_panel();
        }
        // A tool's tray replaces any open library tray.
        self.close_library();
        // Moving Van default: Grab when the page has movable objects, otherwise Cut.
        if tool == ToolKind::Mover && changed {
            let grab = !self.objects.is_empty() || !self.texts.is_empty();
            self.set_sel(G::Variant, if grab { 0 } else { 1 });
            self.rebuild_panel();
        }
        if tool == ToolKind::Brush {
            self.update_sound_art();
        } else {
            self.sound_art = None;
        }
        for b in self.buttons.iter_mut() {
            if let Some(t) = ToolKind::from_command(b.cmd) {
                if t == tool {
                    b.show(2, from_click);
                } else if b.showing == 2 {
                    b.show(0, false);
                }
            }
        }
    }

    /// Panel art for the current tool and its state (docs/RE_PANELS.md §1.2, §3).
    fn panel_layout(&self) -> crate::panels::Layout {
        use crate::panels::Layout;
        let v = self.tray().and_then(|t| t.group(G::Variant)).map(|g| g.sel).unwrap_or(0);
        let small_kids = self.menu_state.small_kids;
        let (background, mbtns, scroller): (u16, Vec<u16>, Option<(u16, usize)>) = match self.tool {
            ToolKind::Pencil => (18044, vec![18400, 18200], None),
            ToolKind::Brush => {
                let mode = [18001, 18002, 18003][v.min(2)];
                // The User Guide shows the size balls for the spray can too.
                let sizes = vec![18200];
                // Brush / voice / spray sub-panels: 5-slot tray script 18027 (RE_BRUSHES §1.1).
                (18045, [vec![18000, mode], sizes].concat(), Some((18027, 5)))
            }
            ToolKind::Fill => (18046, vec![18300], Some((18018, 7))),
            ToolKind::Mixer => (18047, vec![18600, 18200], Some((18021, 6))),
            ToolKind::Stamp => (18048, vec![18700, 18202], Some((18022, 5))),
            ToolKind::Text if small_kids => (18039, vec![], Some((18000, 7))),
            ToolKind::Text => (18049, vec![9383], None),
            ToolKind::Mover => (18050, vec![926], if v == 1 { Some((18020, 6)) } else { None }),
            ToolKind::Eraser => {
                if v == 1 {
                    // The User Guide shows the size and shape buttons with the big erasers too.
                    let round = !matches!(self.item(G::Shape), Some(Item::EraserRound(false)));
                    (18051, vec![18500, 18203, if round { 18200 } else { 18201 }], Some((18024, 5)))
                } else {
                    let round = !matches!(self.item(G::Shape), Some(Item::EraserRound(false)));
                    (18051, vec![18500, 18203, if round { 18200 } else { 18201 }], None)
                }
            }
        };
        Layout { background, mbtns, scroller }
    }

    fn rebuild_panel(&mut self) {
        let layout = self.panel_layout();
        if self.panel.as_ref().is_some_and(|p| p.layout == layout) {
            return;
        }
        self.panel = Some(crate::panels::Panel::new(&self.assets, layout));
        let sel = self.panel_selection();
        if let Some(p) = self.panel.as_mut() {
            p.sync(&sel);
        }
    }

    /// Which panel buttons show as chosen, from the tray state.
    fn panel_selection(&self) -> Vec<bool> {
        let Some(p) = &self.panel else { return vec![] };
        let t = self.tray();
        let sel = |g: G| t.and_then(|t| t.group(g)).map(|x| x.sel).unwrap_or(usize::MAX);
        let shape = match self.item(G::Shape) {
            Some(Item::Shape(s)) => Some(s),
            _ => None,
        };
        let filled = matches!(self.item(G::FillMode), Some(Item::Filled(true)));
        p.buttons
            .iter()
            .map(|b| match (self.tool, b.cmd) {
                (ToolKind::Pencil, 18400) => sel(G::Variant) == b.value as usize,
                (ToolKind::Pencil, 18450..=18458) => pencil_shape_cmd(shape, filled) == b.cmd,
                (_, 18200) => sel(G::Size).checked_add(1) == Some(b.value as usize),
                (ToolKind::Brush, 18000..=18002) => sel(G::Variant) == (b.cmd - 18000) as usize,
                (ToolKind::Brush, 18003 | 18006 | 18007) => sel(G::Mode) == 0,
                (ToolKind::Brush, 18004 | 18005 | 18008) => sel(G::Mode) == 1,
                (ToolKind::Fill, 18300..=18302) => sel(G::Variant) == (b.cmd - 18300) as usize,
                (ToolKind::Mixer, 18600 | 18601) => sel(G::Variant) == (b.cmd - 18600) as usize,
                (ToolKind::Mover, 22301) => sel(G::Variant) == 0,
                (ToolKind::Mover, 22300) => sel(G::Variant) == 1,
                (ToolKind::Eraser, 18501) => sel(G::Variant) == 0,
                (ToolKind::Eraser, 18500) => sel(G::Variant) == 1,
                (ToolKind::Text, 207) => self.text_bold,
                (ToolKind::Text, 208) => self.text_italic,
                (ToolKind::Text, 209) => self.text_underline,
                (ToolKind::Text, 204..=206) => self.text_align as u16 == b.cmd - 204,
                (ToolKind::Eraser, 18201) => sel(G::Shape) == 0,
                (ToolKind::Eraser, 18202) => sel(G::Shape) == 1,
                _ => false,
            })
            .collect()
    }

    fn set_sel(&mut self, g: G, i: usize) {
        let restructure = trays::restructures(g, self.tool);
        if let Some(grp) = self.trays.get_mut(&self.tool).and_then(|t| t.groups.iter_mut().find(|x| x.kind == g)) {
            if i < grp.items.len() {
                grp.sel = i;
            }
        }
        if restructure {
            self.rebuild_tray();
        }
    }

    /// A panel button was pressed.
    fn panel_command(&mut self, cmd: u16, value: u16) {
        use crate::panels::{CMD_DOWN, CMD_UP};
        match (self.tool, cmd) {
            (_, CMD_UP | CMD_DOWN) => {
                let n = self.panel.as_ref().map(|p| p.slots()).unwrap_or(1).max(1);
                if let Some(g) = self.trays.get_mut(&self.tool).and_then(|t| t.groups.iter_mut().find(|g| g.kind == G::Options)) {
                    let pages = g.items.len().div_ceil(n).max(1);
                    g.page = if cmd == CMD_UP { (g.page + pages - 1) % pages } else { (g.page + 1) % pages };
                }
            }
            (ToolKind::Pencil, 18400) => self.set_sel(G::Variant, value as usize),
            (ToolKind::Pencil, 18450..=18458) => {
                let (shape, filled) = match cmd {
                    18450 => (Shape::Line, false),
                    18451 => (Shape::Curve, false),
                    18452 => (Shape::Rect, false),
                    18453 => (Shape::Rect, true),
                    18454 => (Shape::Oval, false),
                    18455 => (Shape::Oval, true),
                    18456 => (Shape::Polygon, false),
                    18457 => (Shape::Polygon, true),
                    _ => (Shape::Freehand, false),
                };
                let si = [Shape::Freehand, Shape::Curve, Shape::Line, Shape::Rect, Shape::Oval, Shape::Polygon].iter().position(|&s| s == shape).unwrap_or(0);
                self.set_sel(G::Shape, si);
                self.set_sel(G::FillMode, if filled { 0 } else { 1 });
            }
            (_, 18200) => self.set_sel(G::Size, value.saturating_sub(1) as usize),
            (ToolKind::Brush, 18000..=18002) => self.set_sel(G::Variant, (cmd - 18000) as usize),
            (ToolKind::Brush, 18003 | 18006 | 18007) => self.set_sel(G::Mode, 0),
            (ToolKind::Brush, 18004 | 18005 | 18008) => self.set_sel(G::Mode, 1),
            (ToolKind::Fill, 18300..=18302) => self.set_sel(G::Variant, (cmd - 18300) as usize),
            (ToolKind::Mixer, 18600 | 18601) => self.set_sel(G::Variant, (cmd - 18600) as usize),
            (ToolKind::Stamp, 18702) => self.lib_popup = Some(None),
            (ToolKind::Stamp, 18703 | 18704) => {
                let n = self.assets.libraries("Libraries/Stamps").len().max(1);
                self.stamp_folder = match cmd {
                    18703 => self.stamp_folder.saturating_sub(1),
                    _ => (self.stamp_folder + 1).min(n - 1),
                };
                if let Some(g) = self.trays.get_mut(&ToolKind::Stamp).and_then(|t| t.groups.iter_mut().find(|g| g.kind == G::Options)) {
                    g.sel = 0;
                    g.page = 0;
                }
                self.rebuild_tray();
            }
            (ToolKind::Stamp, 18701) => self.open_stamp_editor(),
            (ToolKind::Mover, 22301) => self.set_sel(G::Variant, 0),
            (ToolKind::Mover, 22300) => self.set_sel(G::Variant, 1),
            (ToolKind::Mover, 22352) => {
                if let Some(i) = self.selected.take().filter(|&i| i < self.objects.len()) {
                    self.objects.remove(i);
                }
            }
            (ToolKind::Mover, 22350) => {
                if let Some(mut o) = self.selected.and_then(|i| self.objects.get(i)).cloned() {
                    o.x += 12;
                    o.y += 12;
                    self.objects.push(o);
                    self.selected = Some(self.objects.len() - 1);
                }
            }
            (ToolKind::Mover, 22351) => self.flatten_objects(),
            (ToolKind::Eraser, 18501) => self.set_sel(G::Variant, 0),
            (ToolKind::Eraser, 18500) => self.set_sel(G::Variant, 1),
            (ToolKind::Eraser, 18201) => self.set_sel(G::Shape, 0),
            (ToolKind::Eraser, 18202) => self.set_sel(G::Shape, 1),
            (ToolKind::Text, c) => self.text_panel_command(c),
            _ => {}
        }
        self.rebuild_panel();
        let sel = self.panel_selection();
        if let Some(p) = self.panel.as_mut() {
            p.sync(&sel);
        }
        if self.tool == ToolKind::Brush {
            self.update_sound_art();
        }
    }

    fn text_panel_command(&mut self, cmd: u16) {
        use crate::textbox::SIZES;
        let nf = self.fonts.faces.len().max(1);
        let sys = self.fonts.faces.iter().position(|(n, _)| n == "System");
        let step_font = |f: usize, d: usize| {
            let mut f = (f + d) % nf;
            if Some(f) == sys {
                f = (f + d) % nf;
            }
            f
        };
        match cmd {
            200 => self.text_font = step_font(self.text_font, nf - 1),
            201 => self.text_font = step_font(self.text_font, 1),
            202 | 203 => {
                let i = SIZES.iter().position(|&z| z >= self.text_size).unwrap_or(5);
                let j = if cmd == 202 { (i + 1).min(SIZES.len() - 1) } else { i.saturating_sub(1) };
                self.text_size = SIZES[j];
            }
            207 => self.text_bold = !self.text_bold,
            208 => self.text_italic = !self.text_italic,
            209 => self.text_underline = !self.text_underline,
            204..=206 => self.text_align = (cmd - 204) as u8,
            240 | 241 => {
                if let Some((i, _)) = self.editing {
                    if let Some(t) = self.texts.get(i) {
                        self.text_clip = t.text.clone();
                    }
                    if cmd == 240 {
                        self.canvas.dirty = true;
                        if let Some(t) = self.texts.get_mut(i) {
                            t.text.clear();
                        }
                        self.editing = Some((i, 0));
                    }
                }
                return;
            }
            242 => {
                let clip = self.text_clip.clone();
                if let Some((i, c)) = self.editing {
                    if let Some(t) = self.texts.get_mut(i) {
                        let at = t.text.char_indices().nth(c).map(|(b, _)| b).unwrap_or(t.text.len());
                        t.text.insert_str(at, &clip);
                        self.editing = Some((i, c + clip.chars().count()));
                        self.canvas.dirty = true;
                    }
                }
                return;
            }
            // Voice field arrows: the TTS voices.
            236 => {
                let n = crate::speech::VOICES.len();
                crate::speech::set_voice(crate::speech::voice() + n - 1);
                return;
            }
            237 => {
                crate::speech::set_voice(crate::speech::voice() + 1);
                return;
            }
            232 => {
                let all: Vec<String> = self.texts.iter().map(|t| t.text.replace('\n', " ")).filter(|t| !t.trim().is_empty()).collect();
                if self.menu_state.text_to_speech && !all.is_empty() {
                    let text = self.pronounced(&all.join(". "));
                    crate::speech::speak(&text, 0);
                }
                return;
            }
            _ => return,
        }
        // Style changes apply to the box being typed into.
        if let Some((i, _)) = self.editing {
            if let Some(t) = self.texts.get_mut(i) {
                t.font = self.text_font;
                t.size = self.text_size;
                t.bold = self.text_bold;
                t.italic = self.text_italic;
                t.underline = self.text_underline;
                t.align = self.text_align;
                self.canvas.dirty = true;
            }
        }
    }

    fn tray(&self) -> Option<&Tray> {
        self.trays.get(&self.tool)
    }

    fn item(&self, g: G) -> Option<Item> {
        self.tray()?.get(g).cloned()
    }

    fn rebuild_tray(&mut self) {
        let prev = self.trays.get(&self.tool).cloned();
        let mut t = trays::build(self.tool, &self.assets, self.fonts.faces.len(), prev.as_ref());
        if self.tool == ToolKind::Stamp {
            self.load_stamp_folder(&mut t);
        }
        if self.tool == ToolKind::Text {
            if let Some(g) = t.groups.iter_mut().find(|g| g.kind == G::Options) {
                g.items = (0..self.alphabet.len() as u16).map(Item::Letter).collect();
                if g.sel >= g.items.len() {
                    g.sel = 0;
                }
            }
        }
        self.tray_icons = t
            .groups
            .iter()
            .map(|g| {
                g.items
                    .iter()
                    .map(|it| match it {
                        Item::Stamp(id) => self.stamp_icon(*id),
                        // RAlphaPanel letters are drawn in the current colour on white tiles.
                        Item::Letter(i) => {
                            let mut tile = Image::filled(48, 48, WHITE);
                            if let Some(g) = self.letter_bitmap(*i as usize, self.color) {
                                gfx::blit(&mut tile, &g, 0, 0);
                            }
                            tile
                        }
                        _ => trays::tinted_icon(it, &self.assets, self.color)
                            .or_else(|| trays::original_icon(it, &self.assets))
                            .unwrap_or_else(|| Image::new(OPT_ICON, OPT_ICON)),
                    })
                    .collect()
            })
            .collect();
        if let Some(Item::TextSize(z)) = t.get(G::Size) {
            self.text_size = *z;
        }
        if let Some(Item::TextFont(f)) = t.get(G::Size) {
            self.text_font = *f;
        }
        self.trays.insert(self.tool, t);
    }

    fn load_stamp_folder(&mut self, t: &mut Tray) {
        let libs = self.assets.libraries("Libraries/Stamps");
        if libs.is_empty() {
            return;
        }
        self.stamp_folder %= libs.len();
        self.stamp_archive = mohawk::Archive::open(&libs[self.stamp_folder]).ok();
        let ids = self.stamp_archive.as_ref().map(|a| a.ids(assets::T_BMP)).unwrap_or_default();
        if let Some(g) = t.groups.iter_mut().find(|g| g.kind == G::Options) {
            g.items = ids.into_iter().map(Item::Stamp).collect();
            g.sel = g.sel.min(g.items.len().saturating_sub(1));
            g.page = g.page.min(g.items.len() / 20);
        }
    }

    fn stamp_image_by_id(&self, id: u16) -> Option<Image> {
        if let Some(p) = self.stamp_override(id) {
            if let Some(img) = crate::stampedit::load_png(&p) {
                return Some(img);
            }
        }
        crate::library::decode_stamp(self.stamp_archive.as_ref()?.get(assets::T_BMP, id)?)
    }

    /// The user's edited copy of stamp `id` of the current library.
    fn stamp_override(&self, id: u16) -> Option<std::path::PathBuf> {
        let dir = self.user_dir.as_ref()?;
        let stamps = self.assets.cfg.get("DirectoryOrganization", "DirStamps").unwrap_or("My Stamps");
        Some(crate::stampedit::override_path(dir, stamps, &self.stamp_folder_name(), id))
    }

    fn open_stamp_editor(&mut self) {
        let Some(Item::Stamp(id)) = self.item(G::Options) else { return };
        let Some(img) = self.stamp_image_by_id(id) else { return };
        self.overlay = Overlay::StampEditor(Box::new(crate::stampedit::StampEditor::new(&self.assets, &img, self.color)), id);
    }

    fn stroke_is_idle(&self) -> bool {
        matches!(self.stroke, Stroke::None)
    }

    /// The bitmap the stamper (or a Small Kids letter) would put down, at its scale.
    fn stamp_ghost(&self) -> Option<Image> {
        match self.tool {
            ToolKind::Stamp => {
                let scale = match self.item(G::Size) {
                    Some(Item::StampSize(n)) => [1, 2, 4][(n as usize).clamp(1, 3) - 1],
                    _ => 1,
                };
                let Some(Item::Stamp(id)) = self.item(G::Options) else { return None };
                let st = self.stamp_image_by_id(id)?;
                Some(if scale == 1 { st } else { scale_nearest(&st, scale) })
            }
            ToolKind::Text if self.menu_state.small_kids => match self.item(G::Options) {
                Some(Item::Letter(i)) => self.letter_bitmap(i as usize, self.color),
                _ => None,
            },
            _ => None,
        }
    }

    /// Glyph `i` of the Small Kids alphabet rendered 48×48 in `color`.
    fn letter_bitmap(&self, i: usize, color: u32) -> Option<Image> {
        let c = *self.alphabet.get(i)?;
        let face = self.fonts.find(&self.alphabet_font).unwrap_or_else(|| self.fonts.ui_face());
        let mut img = Image::new(48, 48);
        let s = c.to_string();
        let w = self.fonts.measure(face, self.alphabet_size, &s);
        self.fonts.draw(&mut img, face, self.alphabet_size, (48.0 - w) / 2.0, 24.0 + self.alphabet_size * 0.36, color, &s);
        Some(img)
    }

    fn stamp_icon(&self, id: u16) -> Image {
        let mut out = Image::filled(OPT_ICON, OPT_ICON, WHITE);
        if let Some(s) = self.stamp_image_by_id(id) {
            gfx::blit(&mut out, &s, (OPT_ICON as i32 - s.width as i32) / 2, (OPT_ICON as i32 - s.height as i32) / 2);
        }
        out
    }

    fn stamp_folder_name(&self) -> String {
        self.assets
            .libraries("Libraries/Stamps")
            .get(self.stamp_folder)
            .and_then(|p| p.file_stem())
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default()
    }

    /// A click on tray group `gi`, item `ii`.
    fn tray_choose(&mut self, gi: usize, ii: usize) {
        self.commit_text();
        let Some(t) = self.trays.get_mut(&self.tool) else { return };
        let Some(g) = t.groups.get_mut(gi) else { return };
        let Some(item) = g.items.get(ii).cloned() else { return };
        let kind = g.kind;
        if !trays::one_shot(&item) {
            g.sel = ii;
        }
        match item {
            Item::GrabAct(GrabAction::Trash) => {
                if let Some(i) = self.selected.take().filter(|&i| i < self.objects.len()) {
                    self.objects.remove(i);
                }
            }
            Item::GrabAct(GrabAction::Copy) => self.copy(),
            Item::GrabAct(GrabAction::Paste) => {
                if self.clipboard.is_some() {
                    self.stroke = Stroke::PasteAim;
                }
            }
            Item::StampEditor => self.open_stamp_editor(),
            Item::Letter(i) => self.play_wav(tools::TP, letter_voice(i as usize), audio::CH_TOOL),
            _ => {}
        }
        if trays::restructures(kind, self.tool) {
            self.rebuild_tray();
        } else if let Some(Item::TextSize(z)) = self.item(G::Size) {
            self.text_size = z;
        }
        if let Some(Item::TextFont(f)) = self.item(G::Size) {
            self.text_font = f;
        }
        if self.tool == ToolKind::Brush {
            self.update_sound_art();
        }
    }

    /// Sound Art needs the microphone while it is the selected painting tool.
    fn update_sound_art(&mut self) {
        let want = self.tool == ToolKind::Brush && matches!(self.item(G::Variant), Some(Item::Paint(PaintTool::SoundArt)));
        if want && self.sound_art.is_none() {
            match crate::record::Recorder::start(36000.0) {
                Ok(r) => self.sound_art = Some(r),
                Err(e) => {
                    eprintln!("microphone: {e}");
                    self.alert("Sound Art needs a microphone.");
                }
            }
        } else if !want {
            self.sound_art = None;
        }
    }

    /// Icons for the panel scroller's visible slots.
    fn scroller_icons(&self) -> (Vec<Option<Image>>, Option<usize>, usize, usize) {
        let n = self.panel.as_ref().map(|p| p.slots()).unwrap_or(0);
        let Some((gi, g)) = self.tray().and_then(|t| t.groups.iter().enumerate().find(|(_, g)| g.kind == G::Options)) else {
            return (vec![], None, 0, 0);
        };
        if n == 0 {
            return (vec![], None, 0, 0);
        }
        let pages = g.items.len().div_ceil(n).max(1);
        let page = g.page.min(pages - 1);
        let start = page * n;
        let icons = (start..(start + n).min(g.items.len()))
            .map(|i| self.tray_icons.get(gi).and_then(|v| v.get(i)).cloned())
            .collect();
        let sel = (g.sel >= start && g.sel < start + n).then(|| g.sel - start);
        (icons, sel, page, pages)
    }

    /// Stamp-library popup (RPopup) over the name field: (x, y, w, row height, names).
    fn lib_popup_geom(&self) -> (i32, i32, i32, i32, Vec<String>) {
        let paths = match (&self.lib_panel, &self.drawer) {
            (Some(_), Some(d)) => d.libs.clone(),
            _ => self.assets.libraries("Libraries/Stamps"),
        };
        let names: Vec<String> = paths.iter().map(|p| p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()).collect();
        let rh = 16;
        let h = rh * names.len() as i32 + 4;
        (114, (571 - h).max(20), 136, rh, names)
    }

    fn lib_popup_row(&self, x: i32, y: i32) -> Option<usize> {
        let (px, py, w, rh, names) = self.lib_popup_geom();
        if x < px || x >= px + w || y < py + 2 {
            return None;
        }
        let i = ((y - py - 2) / rh) as usize;
        (i < names.len()).then_some(i)
    }

    fn draw_lib_popup(&self, frame: &mut Image) {
        let Some(hover) = self.lib_popup else { return };
        let (px, py, w, rh, names) = self.lib_popup_geom();
        let h = rh * names.len() as i32 + 4;
        gfx::fill_rect(frame, px, py, w, h, 0xff00_0000);
        gfx::fill_rect(frame, px + 1, py + 1, w - 2, h - 2, 0xffff_ffff);
        let face = self.fonts.find("vogel").unwrap_or_else(|| self.fonts.ui_face());
        for (i, n) in names.iter().enumerate() {
            let y = py + 2 + i as i32 * rh;
            let cur = match (&self.lib_panel, &self.drawer) {
                (Some(_), Some(d)) => d.lib,
                _ => self.stamp_folder,
            };
            let sel = hover == Some(i) || (hover.is_none() && i == cur);
            if sel {
                gfx::fill_rect(frame, px + 2, y, w - 4, rh, 0xff00_0080);
            }
            let col = if sel { 0xffff_ffff } else { 0xff00_0000 };
            self.fonts.draw(frame, face, 14.0, (px + 6) as f32, (y + rh - 4) as f32, col, n);
        }
    }

    fn panel_click(&mut self, x: i32, y: i32) -> bool {
        if self.drawer.is_some() || self.shutter.is_some() {
            return false;
        }
        let Some(p) = self.panel.as_mut() else { return false };
        if let Some(i) = p.button_at(x, y) {
            let (cmd, value) = p.press(i, &self.assets, &self.audio, self.menu_state.tool_sounds);
            self.panel_command(cmd, value);
            return true;
        }
        if let Some(k) = p.slot_at(x, y) {
            let n = p.slots();
            if let Some(gi) = self.tray().and_then(|t| t.groups.iter().position(|g| g.kind == G::Options)) {
                let page = self.tray().map(|t| t.groups[gi].page).unwrap_or(0);
                let idx = page * n + k;
                if idx < self.tray().map(|t| t.groups[gi].items.len()).unwrap_or(0) {
                    self.tray_choose(gi, idx);
                }
            }
            return true;
        }
        ui::hit(x, y, 90, 500, 650, 100)
    }

    fn draw_tray(&self, frame: &mut Image) {
        if self.lib_panel.is_some() {
            self.draw_lib_panel(frame);
            return;
        }
        if let Some(p) = &self.panel {
            if self.shutter.as_ref().is_none_or(|s| s.swapped) {
                let (icons, sel, page, pages) = self.scroller_icons();
                let refs: Vec<Option<&Image>> = icons.iter().map(|o| o.as_ref()).collect();
                p.draw(frame, &refs, sel, page, pages);
                self.draw_panel_fields(frame);
            }
        }
        if let Some(s) = &self.shutter {
            s.draw(frame);
        }
    }

    fn draw_lib_panel(&self, frame: &mut Image) {
        let (Some(p), Some(d)) = (&self.lib_panel, &self.drawer) else { return };
        let n = p.slots();
        let start = d.page * n.max(1);
        let icons: Vec<Option<&Image>> = (start..start + n).map(|i| d.items.get(i).and_then(|it| it.thumb.as_ref())).collect();
        let sel = self.lib_sel.filter(|&i| i >= start && i < start + n).map(|i| i - start);
        let pages = if n == 0 { 1 } else { d.items.len().div_ceil(n).max(1) };
        p.draw(frame, &icons, sel, d.page, pages);
        if let Some(m) = &self.rec_meter {
            m.draw(frame);
        }
        if self.fonts.is_empty() {
            return;
        }
        let vogel = self.fonts.find("vogel").unwrap_or_else(|| self.fonts.ui_face());
        let light = gfx::rgb(255, 255, 255);
        // Folder name in the MBTN 18900 field (114,545)–(249,571), like the stamp panel.
        self.fonts.draw_centered(frame, vogel, 16.0, 181.0, 564.0, 130.0, gfx::rgb(0, 0, 0), &d.library_name());
        // Item-name tab under the slots [I] (position from the User Guide screenshots).
        gfx::fill_round_rect(frame, 435, 585, 157, 17, 6, gfx::rgb(122, 128, 220));
        gfx::fill_round_rect(frame, 437, 587, 153, 15, 5, gfx::rgb(16, 0, 112));
        if let Some(it) = self.lib_sel.and_then(|i| d.items.get(i)).filter(|it| !it.title.is_empty()) {
            self.fonts.draw_centered(frame, vogel, 12.0, 513.5, 598.0, 146.0, light, &it.title);
        }
    }

    /// Text fields of the panels: stamp library name, text font/size/voice.
    fn draw_panel_fields(&self, frame: &mut Image) {
        if self.fonts.is_empty() {
            return;
        }
        let vogel = self.fonts.find("vogel").unwrap_or_else(|| self.fonts.ui_face());
        let ink = gfx::rgb(0, 0, 0);
        match self.tool {
            ToolKind::Stamp => {
                self.fonts.draw_centered(frame, vogel, 16.0, 181.0, 564.0, 130.0, ink, &self.stamp_folder_name());
            }
            ToolKind::Text if !self.menu_state.small_kids => {
                let fname = self.fonts.faces.get(self.text_font).map(|f| f.0.clone()).unwrap_or_default();
                self.fonts.draw_centered(frame, vogel, 13.0, 164.0, 544.0, 106.0, ink, &fname);
                self.fonts.draw_centered(frame, vogel, 13.0, 254.0, 544.0, 26.0, ink, &format!("{}", self.text_size as i32));
                let voice = if crate::speech::available() { crate::speech::VOICES[crate::speech::voice()] } else { "-" };
                self.fonts.draw_centered(frame, vogel, 13.0, 513.0, 558.0, 106.0, ink, voice);
            }
            _ => {}
        }
    }

    // ------------------------------------------------------------------ library drawer

    fn open_drawer(&mut self, kind: LibKind) {
        self.commit_text();
        if self.drawer.as_ref().is_some_and(|d| d.kind == kind) {
            return;
        }
        self.drawer = LibDrawer::open(&self.assets, kind);
        if kind == LibKind::Sounds {
            let sounds = self.user_dir.as_ref().map(|d| d.join(self.assets.cfg.get("DirectoryOrganization", "DirSounds").unwrap_or("My Sounds")));
            if let (Some(d), Some(dir)) = (self.drawer.as_mut(), sounds) {
                d.add_user_folder(dir);
            }
        }
        if self.drawer.is_none() {
            self.alert(format!("No {} were found in your Kid Pix files.", kind.name()));
        }
        self.lib_sel = None;
        self.lib_rec = false;
        self.build_lib_panel();
    }

    /// Library panels: TP 18040 with a 7-slot scroller for backgrounds / stickers /
    /// animations; TP 18042 + MBTN 777 (6 slots) for sounds, 18043 in record mode.
    fn build_lib_panel(&mut self) {
        use crate::panels::Layout;
        let kind = self.drawer.as_ref().map(|d| d.kind);
        let layout = match kind {
            None | Some(LibKind::Paper) | Some(LibKind::Stamps) => {
                self.lib_panel = None;
                return;
            }
            Some(LibKind::Sounds) if self.lib_rec => Layout { background: 18043, mbtns: vec![18900, 777], scroller: None },
            Some(LibKind::Sounds) => Layout { background: 18042, mbtns: vec![18900, 777], scroller: Some((18019, 6)) },
            // Folder field + arrows (MBTN 18900) on the left, 7-slot scroller on the right.
            _ => Layout { background: 18040, mbtns: vec![18900], scroller: Some((18018, 7)) },
        };
        let mut p = crate::panels::Panel::new(&self.assets, layout);
        if kind == Some(LibKind::Sounds) {
            p.set_group_hidden(if self.lib_rec { 1 } else { 2 }, true);
        }
        self.rec_meter = if self.lib_rec { Player::new(&self.assets, tools::TP, 18274) } else { None };
        self.lib_panel = Some(p);
    }

    fn close_library(&mut self) {
        self.drawer = None;
        self.lib_panel = None;
        self.lib_drag = None;
        if let Some(r) = self.recorder.take() {
            self.recording = Some(std::sync::Arc::new(r.finish()));
        }
    }

    fn lib_slots(&self) -> usize {
        self.lib_panel.as_ref().map(|p| p.slots()).unwrap_or(0)
    }

    /// Library-panel press in the bottom strip.
    fn lib_panel_down(&mut self, x: i32, y: i32) {
        let Some(p) = self.lib_panel.as_mut() else { return };
        if let Some(i) = p.button_at(x, y) {
            let (cmd, _) = p.press(i, &self.assets, &self.audio, self.menu_state.tool_sounds);
            self.lib_command(cmd);
            return;
        }
        if let Some(k) = p.slot_at(x, y) {
            let n = p.slots();
            let Some(d) = self.drawer.as_ref() else { return };
            let idx = d.page * n + k;
            if idx < d.items.len() {
                self.lib_sel = Some(idx);
                self.lib_drag = Some(idx);
                if d.kind == LibKind::Sounds {
                    if let Some(s) = d.sound(d.items[idx].id) {
                        self.audio.stop_channel(audio::CH_FX);
                        self.audio.play(s, audio::CH_FX, false);
                    }
                }
            }
        }
    }

    fn lib_command(&mut self, cmd: u16) {
        use crate::panels::{CMD_DOWN, CMD_UP};
        let n = self.lib_slots().max(1);
        match cmd {
            CMD_UP | CMD_DOWN => {
                if let Some(d) = self.drawer.as_mut() {
                    let pages = d.items.len().div_ceil(n).max(1);
                    d.page = if cmd == CMD_UP { (d.page + pages - 1) % pages } else { (d.page + 1) % pages };
                    d.ensure_thumbs(d.page * n, n);
                }
            }
            // Folder field (popup list of folders) and previous / next folder, no wrap.
            18801 => self.lib_popup = Some(None),
            18802 | 18803 => {
                if let Some(d) = self.drawer.as_mut() {
                    let n = d.libs.len();
                    if cmd == 18802 && d.lib > 0 {
                        d.switch(-1);
                    } else if cmd == 18803 && d.lib + 1 < n {
                        d.switch(1);
                    }
                    self.lib_sel = None;
                }
            }
            // Sound panel: 200 switches library / record mode.
            200 => {
                if let Some(r) = self.recorder.take() {
                    self.recording = Some(std::sync::Arc::new(r.finish()));
                }
                self.lib_rec = !self.lib_rec;
                self.build_lib_panel();
            }
            201 => {
                self.audio.stop_channel(audio::CH_FX);
                match crate::record::Recorder::start(10.0) {
                    Ok(r) => self.recorder = Some(r),
                    Err(e) => {
                        eprintln!("microphone: {e}");
                        self.alert("Recording needs a microphone.");
                    }
                }
            }
            202 => {
                if let Some(r) = self.recorder.take() {
                    self.recording = Some(std::sync::Arc::new(r.finish()));
                }
                self.audio.stop_channel(audio::CH_FX);
            }
            203 => {
                if let Some(s) = self.recording.clone() {
                    self.audio.stop_channel(audio::CH_FX);
                    self.audio.play(s, audio::CH_FX, false);
                }
            }
            207 => {
                if let Some(r) = self.recorder.take() {
                    self.recording = Some(std::sync::Arc::new(r.finish()));
                }
                if let Some(s) = self.recording.clone() {
                    self.set_page_sound(s);
                    self.actions.push(Action::SaveRecording);
                }
            }
            // 208 trash: forget the recording; 211: take the sound off the page.
            208 => self.recording = None,
            211 => {
                self.page_sound = None;
                self.canvas.dirty = true;
                self.audio.stop_channel(audio::CH_MUSIC);
            }
            _ => {}
        }
    }

    /// Drop a dragged library item onto the page at (x, y).
    fn lib_drop(&mut self, idx: usize, x: i32, y: i32) {
        let Some(d) = self.drawer.as_ref() else { return };
        let kind = d.kind;
        let Some(id) = d.items.get(idx).map(|i| i.id) else { return };
        match kind {
            LibKind::Stickers | LibKind::Toons => {
                let (frames, fps, src) = d.movie_frames(id, kind == LibKind::Toons);
                if !frames.is_empty() {
                    self.armed = Some((frames, fps, src));
                    self.canvas_down(x, y);
                    self.down = false;
                }
            }
            _ => self.drawer_pick(idx),
        }
    }

    fn drawer_pick(&mut self, idx: usize) {
        let Some(d) = self.drawer.as_mut() else { return };
        let kind = d.kind;
        let Some(id) = d.items.get(idx).map(|i| i.id) else { return };
        match kind {
            LibKind::Backgrounds => {
                if let Some(mut img) = d.full_bitmap(id) {
                    if img.width != CANVAS_W || img.height != CANVAS_H {
                        img = gfx::resize(&img, CANVAS_W, CANVAS_H);
                    }
                    self.canvas.replace(img);
                }
            }
            LibKind::Sounds => {
                if let Some(s) = d.sound(id) {
                    self.page_sound = Some(s.clone());
                    self.audio.play(s, audio::CH_MUSIC, false);
                }
            }
            LibKind::Stamps => {
                if let Some(img) = d.stamp_image(id) {
                    self.stamp = Some(img);
                    if self.tool != ToolKind::Stamp {
                        self.select_tool(ToolKind::Stamp, false);
                    }
                }
            }
            LibKind::Paper => {
                self.paper = if id == 0 { None } else { d.full_bitmap(id).map(Rc::new) };
                self.paper_index = if id >= 17000 { id - 17000 } else { 0 };
                self.canvas.dirty = true;
            }
            LibKind::Stickers | LibKind::Toons => {
                let (frames, fps, src) = d.movie_frames(id, kind == LibKind::Toons);
                if !frames.is_empty() {
                    self.armed = Some((frames, fps, src));
                }
            }
        }
    }

    // ------------------------------------------------------------------ commands

    fn command(&mut self, cmd: u16) {
        if let Some(t) = ToolKind::from_command(cmd) {
            self.select_tool(t, true);
            return;
        }
        match cmd {
            224 => self.open_drawer(LibKind::Backgrounds),
            222 => self.open_drawer(LibKind::Stickers),
            223 => self.open_drawer(LibKind::Toons),
            225 => self.open_drawer(LibKind::Sounds),
            281 => self.actions.push(Action::Save),
            242 => self.overlay = Overlay::Print(crate::win32::PrintDialog::new()),
            267 => self.actions.push(Action::Quit),
            NEW_PICTURE_CMD => self.actions.push(Action::New),
            243 => {
                self.playing = true;
                self.movie_time = 0.0;
                if let Some(s) = self.page_sound.as_ref().filter(|_| self.menu_state.attached_sound) {
                    self.audio.play(s.clone(), audio::CH_MUSIC, false);
                }
                for o in &self.objects {
                    if let Some(a) = o.movie.as_ref().and_then(|m| m.audio.clone()) {
                        self.audio.play(std::sync::Arc::new(a), audio::CH_FX, false);
                    }
                }
            }
            244 => {
                self.playing = false;
                self.audio.stop_channel(audio::CH_MUSIC);
                self.audio.stop_channel(audio::CH_FX);
            }
            266 => {
                // Firecracker erase-all (RDFirecracker2): detonated at the canvas bottom centre.
                self.objects.clear();
                self.selected = None;
                self.start_big_erase(0, (CANVAS_W as f32 / 2.0, CANVAS_H as f32 - 50.0));
            }
            238 => self.open_drawer(LibKind::Paper),
            239 => self.actions.push(Action::Help),
            270 => self.actions.push(Action::GoSlideShow),
            _ => {}
        }
    }


    /// EaselHelp.MHK hotspot tables for the open panel / drawer (docs/RE_SHELL.md §7).
    pub fn help_tables(&self) -> Vec<u16> {
        if let Some(d) = &self.drawer {
            return match d.kind {
                LibKind::Backgrounds => vec![787, 788],
                LibKind::Stickers => vec![797, 798],
                LibKind::Toons => vec![807, 808],
                LibKind::Sounds => vec![817, 818, 819],
                _ => vec![],
            };
        }
        let v = self.tray().and_then(|t| t.group(G::Variant)).map(|g| g.sel).unwrap_or(0);
        match self.tool {
            ToolKind::Pencil => vec![827, 828],
            ToolKind::Brush => vec![837, [838, 839, 840][v.min(2)]],
            ToolKind::Fill => vec![847, [848, 849, 850][v.min(2)]],
            ToolKind::Mixer => vec![857, [858, 859][v.min(1)]],
            ToolKind::Stamp => vec![867, 868],
            ToolKind::Text => vec![877],
            ToolKind::Mover => vec![887, [888, 889][v.min(1)]],
            ToolKind::Eraser => vec![897, [898, 899][v.min(1)]],
        }
    }

    pub fn undo(&mut self) {
        self.commit_text();
        if self.canvas.undo() {
            if let Some(face) = self.undo_face.as_ref() {
                let _ = face;
            }
            // A random talking face (scripts 1095..1116) with its voice clip.
            let id = 1095 + self.rng.below(22) as u16;
            if let Some(p) = Player::new(&self.assets, EASEL, id) {
                self.undo_talk = Some(p);
            }
        }
    }

    pub fn redo(&mut self) {
        self.commit_text();
        self.canvas.redo();
    }

    pub fn new_picture(&mut self) {
        self.commit_text();
        self.kpx_base = None;
        self.canvas.reset();
        self.objects.clear();
        self.texts.clear();
        self.page_sound = None;
        self.playing = false;
        self.audio.stop_channel(audio::CH_MUSIC);
    }

    /// Picture with stickers flattened (for saving).
    pub fn flattened(&mut self) -> Image {
        self.commit_text();
        let mut img = self.canvas.img.clone();
        for t in &self.texts {
            t.draw(&mut img, &self.fonts, 0, 0);
        }
        for o in &self.objects {
            gfx::blit(&mut img, &o.current(self.playing, self.movie_time), o.x, o.y);
        }
        img
    }



    // ------------------------------------------------------------------ text tool

    pub fn commit_text_now(&mut self) {
        self.commit_text();
    }

    pub fn page_sound_ref(&self) -> Option<&mohawk::Sound> {
        self.page_sound.as_deref()
    }

    /// The kp-brushes tool for the current tool / panel state, if the original engine
    /// handles it (pencil media freehand, paint brushes, Sound Art, spray, mixer wand).
    fn current_brush_id(&self) -> Option<kp_brushes::BrushId> {
        match self.tool {
            ToolKind::Pencil => {
                if !matches!(self.item(G::Shape), Some(Item::Shape(Shape::Freehand)) | None) {
                    return None;
                }
                let v = self.tray().and_then(|t| t.group(G::Variant)).map(|g| g.sel).unwrap_or(0);
                Some(kp_brushes::PENCIL_MEDIA[v.min(3)])
            }
            ToolKind::Brush => match self.item(G::Options) {
                Some(Item::KpBrush(b)) => Some(b),
                _ => None,
            },
            ToolKind::Mixer => match self.item(G::Options) {
                Some(Item::Wand(i)) => Some(kp_brushes::WAND[(i as usize).min(11)]),
                _ => None,
            },
            // The little eraser is RCircle with the colour forced to white (RE_PANELS §3.8).
            ToolKind::Eraser if !matches!(self.item(G::Variant), Some(Item::BigEraser(true))) => Some(kp_brushes::BrushId::Circle),
            _ => None,
        }
    }

    /// Mic analysis for Sound Art: the original's 1024-point FFT at 22,050 Hz over the
    /// latest input (resampled from the device rate). Not live without a microphone.
    fn brush_voice(&self) -> kp_brushes::Voice {
        match &self.sound_art {
            Some(r) => {
                let rate = r.rate();
                let n = (kp_brushes::Voice::WINDOW as u64 * rate as u64 / kp_brushes::Voice::RATE as u64) as usize + 4;
                kp_brushes::Voice::analyze_at(&r.tail(n), rate)
            }
            None => kp_brushes::Voice::default(),
        }
    }

    /// Hands-free Sound Art paints by itself from the moment it is chosen (KPD3.exe
    /// `RHandsFreeBrush::v3`): create and start the brush when it becomes the selection,
    /// stop it when something else is chosen.
    fn sync_hands_free(&mut self) {
        use kp_brushes::BrushId::Touchless;
        let want = if self.tool == ToolKind::Brush { self.current_brush_id().filter(|id| matches!(id, Touchless(_))) } else { None };
        let have = self.brush.as_ref().map(|b| b.0).filter(|id| matches!(id, Touchless(_)));
        if want == have {
            return;
        }
        if have.is_some() {
            self.end_brush();
        }
        if let Some(id) = want {
            self.end_brush();
            let seed = self.rng.next_u32();
            self.brush = Some((id, kp_brushes::BrushTool::new(id, &self.brush_cfg, seed)));
            self.canvas.checkpoint();
            self.with_brush(|t, c, ctx| t.activate(c, ctx));
        }
    }

    fn play_brush_events(&mut self, ev: Vec<kp_brushes::SoundEvent>) {
        for e in ev {
            let key = (e.archive, e.id);
            if e.stop {
                if let Some(h) = self.brush_sounds.remove(&key) {
                    self.audio.stop(h);
                }
                continue;
            }
            if !self.menu_state.tool_sounds {
                continue;
            }
            if let Some(h) = self.brush_sounds.remove(&key) {
                self.audio.stop(h);
            }
            if let Some(snd) = self.assets.sound(e.archive, e.id) {
                let h = self.audio.play(snd, audio::CH_FX, e.looping);
                self.brush_sounds.insert(key, h);
            }
        }
        self.brush_sounds.retain(|_, h| self.audio.is_playing(*h));
    }

    /// Run `f` with the brush tool, the canvas and a fresh input context.
    fn with_brush(&mut self, f: impl FnOnce(&mut kp_brushes::BrushTool, &mut Image, &kp_brushes::Ctx) -> Vec<kp_brushes::SoundEvent>) {
        let assets = self.brush_assets.clone();
        let (_, r, g, b) = gfx::split(self.color);
        let mut ctx = kp_brushes::Ctx::new(&assets);
        ctx.color = [r as u8, g as u8, b as u8];
        if self.tool == ToolKind::Eraser {
            ctx.color = [255, 255, 255];
            ctx.pencil_square = matches!(self.item(G::Shape), Some(Item::EraserRound(false)));
        }
        ctx.size = self.tray().map(|t| t.size_idx()).unwrap_or(1).min(2) as u8 + 1;
        ctx.space = self.space;
        ctx.texture = self.paper_index;
        ctx.voice = self.brush_voice();
        let name = self.artist_name.clone();
        ctx.artist_name = &name;
        let Some((_, tool)) = self.brush.as_mut() else { return };
        let ev = f(tool, &mut self.canvas.img, &ctx);
        self.play_brush_events(ev);
    }

    /// Make any running brush animation permanent (tool or brush change).
    fn end_brush(&mut self) {
        if self.brush.is_some() {
            self.with_brush(|t, c, ctx| t.deactivate(c, ctx));
            self.brush = None;
            self.brush_held = false;
            for (_, h) in self.brush_sounds.drain() {
                self.audio.stop(h);
            }
        }
    }

    /// Mouse down with a brush-engine tool.
    fn brush_down(&mut self, id: kp_brushes::BrushId, fx: f32, fy: f32) {
        if self.brush.as_ref().map(|b| b.0) != Some(id) {
            self.end_brush();
            let seed = self.rng.next_u32();
            self.brush = Some((id, kp_brushes::BrushTool::new(id, &self.brush_cfg, seed)));
        }
        self.canvas.checkpoint();
        self.brush_held = true;
        self.with_brush(|t, c, ctx| t.begin(c, fx, fy, ctx));
    }

    /// Stop typing into the current text box (an empty box is removed).
    fn commit_text(&mut self) {
        self.typing_run = false;
        if let Some((i, _)) = self.editing.take() {
            if self.texts.get(i).is_some_and(|t| t.text.trim().is_empty()) {
                self.texts.remove(i);
            }
        }
    }

    fn text_at(&self, x: i32, y: i32) -> Option<usize> {
        let (cx, cy) = (x - CANVAS_X, y - CANVAS_Y);
        (0..self.texts.len()).rev().find(|&i| self.texts[i].contains(&self.fonts, cx, cy))
    }

    fn user_dct(&self) -> Option<std::path::PathBuf> {
        self.user_dir.as_ref().map(|d| d.join("user.dct"))
    }

    /// Toolbox > Check Spelling: walk the text boxes word by word.
    fn start_spell(&mut self) {
        let learned = self.user_dct().and_then(|p| std::fs::read_to_string(p).ok()).map(|t| t.lines().map(|l| l.trim().to_lowercase()).filter(|l| !l.is_empty()).collect()).unwrap_or_default();
        self.spell = Some(crate::spell::Checker::new(&self.assets.root, learned));
        self.spell_next((0, 0));
    }

    /// Show the next unknown word at or after (box, byte), or finish.
    fn spell_next(&mut self, from: (usize, usize)) {
        let Some(ck) = &self.spell else { return };
        for bi in from.0..self.texts.len() {
            let text = &self.texts[bi].text;
            for (a, b) in crate::spell::words(text) {
                if bi == from.0 && a < from.1 {
                    continue;
                }
                let w = &text[a..b];
                if !ck.known(w) {
                    let sugg = ck.suggest(w);
                    self.overlay = Overlay::Spell(Box::new(crate::win32::SpellDialog::new(w, sugg)), (bi, a, b));
                    return;
                }
            }
        }
        self.overlay = Overlay::None;
        self.spell = None;
    }

    fn spell_result(&mut self, r: crate::win32::SpellResult) {
        use crate::win32::SpellResult as S;
        let Overlay::Spell(d, (bi, a, b)) = &self.overlay else { return };
        let (bi, a, b) = (*bi, *a, *b);
        let word = d.word.clone();
        match r {
            S::None => {}
            S::Done => {
                self.overlay = Overlay::None;
                self.spell = None;
            }
            S::Skip => {
                if let Some(c) = self.spell.as_mut() {
                    c.skip(&word);
                }
                self.spell_next((bi, b));
            }
            S::Learn => {
                let dct = self.user_dct();
                if let Some(c) = self.spell.as_mut() {
                    c.learn(&word);
                    if let Some(p) = dct {
                        let _ = std::fs::write(p, c.learned.join("\n") + "\n");
                    }
                }
                self.spell_next((bi, b));
            }
            S::Change(new) => {
                if let Some(t) = self.texts.get_mut(bi) {
                    if b <= t.text.len() {
                        t.text.replace_range(a..b, &new);
                        self.canvas.dirty = true;
                    }
                }
                self.spell_next((bi, a + new.len()));
            }
        }
    }

    /// Apply the Say It This Way exceptions to text before speaking it.
    pub fn pronounced(&self, text: &str) -> String {
        text.split_inclusive(|c: char| !c.is_alphanumeric() && c != '\'')
            .map(|tok| {
                let w: String = tok.chars().filter(|c| c.is_alphanumeric() || *c == '\'').collect();
                let rest = &tok[w.len().min(tok.len())..];
                match self.words.iter().find(|(o, _)| o.eq_ignore_ascii_case(&w)) {
                    Some((_, say)) if !w.is_empty() => format!("{say}{rest}"),
                    _ => tok.to_string(),
                }
            })
            .collect()
    }

    /// Flatten Text: SELECTED = the box being typed into, ALL = every box.
    pub fn flatten_texts_mode(&mut self, all: bool) {
        if all {
            self.flatten_texts();
            return;
        }
        let Some((i, _)) = self.editing.take() else { return };
        if i < self.texts.len() {
            self.canvas.checkpoint();
            let t = self.texts.remove(i);
            t.draw(&mut self.canvas.img, &self.fonts, 0, 0);
        }
    }

    /// Flatten Stickers and Animations: SELECTED = the selected object, ALL = every object.
    pub fn flatten_objects_mode(&mut self, all: bool) {
        if all {
            self.flatten_objects();
            return;
        }
        let Some(i) = self.selected.take().filter(|&i| i < self.objects.len()) else { return };
        self.canvas.checkpoint();
        let o = self.objects.remove(i);
        gfx::blit(&mut self.canvas.img, &o.current(self.playing, self.movie_time), o.x, o.y);
    }

    /// Toolbox > Flatten Text: draw every text box into the picture.
    pub fn flatten_texts(&mut self) {
        self.commit_text();
        if self.texts.is_empty() {
            return;
        }
        self.canvas.checkpoint();
        for t in std::mem::take(&mut self.texts) {
            t.draw(&mut self.canvas.img, &self.fonts, 0, 0);
        }
    }

    pub fn key_char(&mut self, c: char) {
        if let Overlay::Spell(d, _) = &mut self.overlay {
            let r = d.key(c);
            self.spell_result(r);
            return;
        }
        if self.tool != ToolKind::Text {
            return;
        }
        let Some((i, caret)) = self.editing else { return };
        let Some(t) = self.texts.get_mut(i) else { return };
        if !self.typing_run && !matches!(c, '\u{1c}' | '\u{1d}') {
            self.text_undo = Some((i, t.text.clone(), caret));
            self.typing_run = true;
        }
        let at = |t: &crate::textbox::TextBox, c: usize| t.text.char_indices().nth(c).map(|(b, _)| b).unwrap_or(t.text.len());
        let caret = match c {
            '\u{8}' => {
                if caret == 0 {
                    return;
                }
                let b = at(t, caret - 1);
                t.text.remove(b);
                caret - 1
            }
            '\u{7f}' => {
                if caret < t.text.chars().count() {
                    let b = at(t, caret);
                    t.text.remove(b);
                }
                caret
            }
            '\u{1c}' => caret.saturating_sub(1),
            '\u{1d}' => (caret + 1).min(t.text.chars().count()),
            '\r' | '\n' => {
                let b = at(t, caret);
                t.text.insert(b, '\n');
                caret + 1
            }
            c if !c.is_control() => {
                let b = at(t, caret);
                t.text.insert(b, c);
                caret + 1
            }
            _ => return,
        };
        self.editing = Some((i, caret));
        self.canvas.dirty = true;
    }

    /// Mouse wheel over a sticker resizes it (around its centre).
    pub fn wheel(&mut self, steps: f32) {
        let (x, y) = self.mouse;
        let Some(i) = self.object_at(x, y) else { return };
        let z = self.objects[i].zoom * 1.12f32.powf(steps);
        self.objects[i].rescale(z);
        self.canvas.dirty = true;
    }

    fn finish_scale(&mut self, i: usize, h: Handle, z0: f32, sx: i32, sy: i32, cx: i32, cy: i32) {
        let Some(o) = self.objects.get(i) else { return };
        let (w, hgt) = o.size();
        let d = match h {
            Handle::ScaleBR => ((cx - sx) + (cy - sy)) as f32 / 2.0,
            _ => ((sx - cx) + (sy - cy)) as f32 / 2.0,
        };
        let base = (w.max(hgt) as f32).max(1.0);
        let z = z0 * ((base + d) / base).max(0.1);
        self.objects[i].rescale(z);
        self.canvas.dirty = true;
    }

    /// Delete key: remove the sticker under the mouse.
    pub fn delete_under_mouse(&mut self) {
        if self.editing.is_some() {
            self.key_char('\u{7f}');
            return;
        }
        if let Some(i) = self.selected.take().or_else(|| self.object_at(self.mouse.0, self.mouse.1)) {
            if i < self.objects.len() {
                self.objects.remove(i);
                self.canvas.dirty = true;
            }
        }
    }

    pub fn key_escape(&mut self) {
        if !matches!(self.overlay, Overlay::None) {
            self.overlay = Overlay::None;
        } else if self.armed.is_some() {
            self.armed = None;
        } else if self.drawer.is_some() {
            self.close_library();
        } else {
            self.commit_text();
        }
    }

    // ------------------------------------------------------------------ input

    fn in_canvas(x: i32, y: i32) -> bool {
        ui::hit(x, y, CANVAS_X, CANVAS_Y, CANVAS_W as i32, CANVAS_H as i32)
    }

    /// The green tray over the canvas is the paper-texture picker (Easel 1055–1077).
    fn drawer_rect(&self) -> Option<(i32, i32, i32, i32)> {
        (self.drawer.as_ref()?.kind == LibKind::Paper).then_some((305, 398, 408, 102))
    }

    /// Play (243) and Stop (244) share one spot: only one is live at a time.
    fn button_visible(&self, b: &UiButton) -> bool {
        if self.menu_state.small_kids && matches!(b.cmd, 242 | 270) {
            return false; // printing and SlideShow are hidden in Small Kids Mode
        }
        match b.cmd {
            243 => !self.playing,
            244 => self.playing,
            _ => true,
        }
    }

    /// Play is live only when the page has something to play (animations, movies, sound);
    /// otherwise it shows its disabled art (MBTN 75 field 2 = script 1042).
    fn has_playable(&self) -> bool {
        self.page_sound.is_some() || self.objects.iter().any(|o| o.animated())
    }

    fn button_at(&self, x: i32, y: i32) -> Option<usize> {
        self.buttons
            .iter()
            .position(|b| self.button_visible(b) && !(b.cmd == 243 && !self.has_playable()) && b.rect.contains(x, y))
    }

    fn object_at(&self, x: i32, y: i32) -> Option<usize> {
        let (cx, cy) = (x - CANVAS_X, y - CANVAS_Y);
        self.objects.iter().rposition(|o| {
            let (w, h) = o.size();
            ui::hit(cx, cy, o.x, o.y, w, h) && o.hit(cx - o.x, cy - o.y)
        })
    }

    pub fn mouse_move(&mut self, x: i32, y: i32) {
        let prev = self.mouse;
        self.mouse = (x, y);
        if self.lib_popup.is_some() {
            self.lib_popup = Some(self.lib_popup_row(x, y));
            return;
        }
        if let Overlay::StampEditor(ed, _) = &mut self.overlay {
            ed.mouse_move(x, y, &self.assets, &self.audio);
            return;
        }
        self.menubar.motion(&self.fonts, &self.menu_state, x, y);
        if self.menu_press || self.menubar.open.is_some() {
            return;
        }
        if self.picker.captured() {
            if let Some(c) = self.picker.drag(x, y, &self.assets, &self.audio, self.menu_state.tool_sounds) {
                self.set_color(c);
            }
            return;
        }
        let sel = self.panel_selection();
        if let Some(p) = self.lib_panel.as_mut() {
            p.mouse_move(x, y, &self.assets, &self.audio, self.menu_state.tool_sounds, &[]);
        } else if let Some(p) = self.panel.as_mut() {
            p.mouse_move(x, y, &self.assets, &self.audio, self.menu_state.tool_sounds, &sel);
        }
        if self.picker.eyedropper {
            self.dropper_preview = Self::in_canvas(x, y).then(|| self.canvas.img.get(x - CANVAS_X, y - CANVAS_Y)).flatten();
        }
        // Button rollover: field 1 plays (with its sounds) on enter, rest script on leave.
        let hb = if self.down { self.hover_btn } else { self.button_at(x, y) };
        if hb != self.hover_btn {
            if let Some(i) = self.hover_btn {
                if !self.is_selected(i) {
                    self.buttons[i].show(0, false);
                }
            }
            if let Some(i) = hb {
                if !self.is_selected(i) {
                    self.buttons[i].show(1, true);
                    let s = self.button_sounds(i);
                    self.play_script_sounds(EASEL, 1000, &s);
                }
            }
            self.hover_btn = hb;
            self.hover_time = 0.0;
        }
        if !self.down {
            self.hover_stroke(x, y);
            return;
        }
        if self.brush_held {
            let (bx, by) = ((x - CANVAS_X) as f32 + 0.5, (y - CANVAS_Y) as f32 + 0.5);
            self.with_brush(|t, c, ctx| t.drag(c, bx, by, ctx));
            return;
        }
        if let Some((i, gx, gy)) = self.drag_text {
            if let Some(t) = self.texts.get_mut(i) {
                t.x = x - CANVAS_X - gx;
                t.y = y - CANVAS_Y - gy;
                self.canvas.dirty = true;
            }
            return;
        }
        if let Some((i, gx, gy)) = self.drag_obj {
            if let Some(o) = self.objects.get_mut(i) {
                o.x = x - CANVAS_X - gx;
                o.y = y - CANVAS_Y - gy;
            }
            return;
        }
        self.stroke_to(prev, (x, y));
    }

    /// Strokes that follow the mouse between clicks (curve bending, polygon corners).
    fn hover_stroke(&mut self, x: i32, y: i32) {
        let (fx, fy) = ((x - CANVAS_X) as f32 + 0.5, (y - CANVAS_Y) as f32 + 0.5);
        match std::mem::replace(&mut self.stroke, Stroke::None) {
            Stroke::CurveBend { a, b, pen, base, .. } => {
                self.canvas.img = base.clone();
                let pts = paint::curve_points(a, (fx, fy), b);
                self.stroke_path(&pts, &pen, false);
                self.stroke = Stroke::CurveBend { a, b, pen, base };
            }
            Stroke::Polygon { pts, filled, pen, base, .. } => {
                self.canvas.img = base.clone();
                let mut p = pts.clone();
                p.push((fx, fy));
                self.stroke_path(&p, &pen, false);
                self.stroke = Stroke::Polygon { pts, filled, pen, base };
            }
            Stroke::CutPolygon { pts, .. } => self.stroke = Stroke::CutPolygon { pts, cur: (fx, fy) },
            other => self.stroke = other,
        }
    }

    fn button_sounds(&mut self, i: usize) -> Vec<u16> {
        // Sound ops fire at the start of in/out scripts; collect them by a zero-length update.
        self.buttons[i].player().map(|p| p.update(0.0)).unwrap_or_default()
    }

    fn is_selected(&self, i: usize) -> bool {
        ToolKind::from_command(self.buttons[i].cmd) == Some(self.tool)
    }

    pub fn mouse_down(&mut self, x: i32, y: i32) {
        self.mouse = (x, y);
        if self.lib_popup.is_some() && self.lib_panel.is_some() {
            if let (Some(i), Some(d)) = (self.lib_popup_row(x, y), self.drawer.as_mut()) {
                let delta = i as i32 - d.lib as i32;
                if delta != 0 {
                    d.switch(delta);
                }
                self.lib_sel = None;
            }
            self.lib_popup = None;
            return;
        }
        if self.lib_popup.is_some() {
            if let Some(i) = self.lib_popup_row(x, y) {
                self.stamp_folder = i;
                if let Some(g) = self.trays.get_mut(&ToolKind::Stamp).and_then(|t| t.groups.iter_mut().find(|g| g.kind == G::Options)) {
                    g.sel = 0;
                    g.page = 0;
                }
                self.rebuild_tray();
            }
            self.lib_popup = None;
            return;
        }
        if let Some(p) = self.about.take() {
            let _ = p;
            return;
        }
        {
            if self.menubar.press(x, y) {
                self.menu_press = true;
                return;
            }
        }
        self.down = true;
        if let Overlay::Fit(..) = self.overlay {
            self.fit_click(x, y);
            self.down = false;
            return;
        }
        if let Overlay::Print(d) = &mut self.overlay {
            d.mouse_down(x, y);
            self.down = false;
            return;
        }
        if let Overlay::StampEditor(ed, _) = &mut self.overlay {
            ed.mouse_down(x, y, &self.assets, &self.audio);
            self.down = false;
            return;
        }
        if let Overlay::Spell(d, _) = &mut self.overlay {
            d.mouse_down(x, y);
            self.down = false;
            return;
        }
        if let Overlay::Email(d) = &mut self.overlay {
            d.mouse_down(x, y);
            self.down = false;
            return;
        }
        if self.picker.press(x, y, &self.assets, &self.audio, self.menu_state.tool_sounds) {
            self.down = false;
            return;
        }
        if self.picker.eyedropper {
            self.picker.eyedropper = false;
            if let Some(c) = Self::in_canvas(x, y).then(|| self.canvas.img.get(x - CANVAS_X, y - CANVAS_Y)).flatten() {
                self.set_color(c | 0xff00_0000);
            }
            self.dropper_preview = None;
            self.down = false;
            return;
        }
        // Library drawer.
        if let Some(r) = self.drawer_rect() {
            if ui::hit(x, y, r.0, r.1, r.2, r.3) {
                self.drawer_click(x, y);
                self.down = false;
                return;
            }
        }
        if self.lib_panel.is_some() && y >= 505 && x >= 90 && x < 742 {
            self.lib_panel_down(x, y);
            self.down = false;
            return;
        }
        // Tool tray (bottom panels).
        if self.panel_click(x, y) {
            self.down = false;
            return;
        }
        // The splotch area itself (the picker opens on hover).
        if ui::hit(x, y, COLOR_RECT.0, COLOR_RECT.1, COLOR_RECT.2, COLOR_RECT.3) {
            self.down = false;
            return;
        }
        // Undo face.
        if ui::hit(x, y, FACE_RECT.0, FACE_RECT.1, FACE_RECT.2, FACE_RECT.3) {
            self.undo_redo();
            self.down = false;
            return;
        }
        // Buttons.
        if let Some(i) = self.button_at(x, y) {
            let cmd = self.buttons[i].cmd;
            self.buttons[i].show(2, true);
            let s = self.button_sounds(i);
            self.play_script_sounds(EASEL, 1000, &s);
            self.command(cmd);
            self.down = false;
            return;
        }
        if Self::in_canvas(x, y) {
            self.canvas_down(x, y);
        }
    }

    pub fn mouse_up(&mut self, x: i32, y: i32) {
        if let Overlay::Spell(d, _) = &mut self.overlay {
            let r = d.mouse_up(x, y);
            self.spell_result(r);
            return;
        }
        if let Overlay::Email(d) = &mut self.overlay {
            match d.mouse_up(x, y) {
                crate::win32::EmailResult::Send { movie } => {
                    self.overlay = Overlay::None;
                    self.actions.push(Action::EmailSend { movie });
                }
                crate::win32::EmailResult::Cancel => self.overlay = Overlay::None,
                crate::win32::EmailResult::None => {}
            }
            return;
        }
        if let Some(idx) = self.lib_drag.take() {
            if Self::in_canvas(x, y) {
                self.lib_drop(idx, x, y);
            }
            self.down = false;
            return;
        }
        if let Overlay::StampEditor(ed, id) = &mut self.overlay {
            let id = *id;
            match ed.mouse_up(x, y) {
                crate::stampedit::Outcome::Open => {}
                crate::stampedit::Outcome::Cancel => self.overlay = Overlay::None,
                crate::stampedit::Outcome::Save(img) => {
                    self.overlay = Overlay::None;
                    if let Some(p) = self.stamp_override(id) {
                        let _ = std::fs::create_dir_all(p.parent().unwrap_or(&p));
                        if crate::save_png(&p, &img).is_err() {
                            let t = self.ini_text("FileStrings", "GeneralFileErr", "An error occurred while saving this file.");
                            self.alert(t);
                        }
                    } else {
                        self.alert("Log in to keep your own stamps.");
                    }
                    self.rebuild_tray();
                }
            }
            return;
        }
        if let Overlay::Print(d) = &mut self.overlay {
            match d.mouse_up(x, y) {
                crate::win32::PrintResult::Print(n) => {
                    self.overlay = Overlay::None;
                    self.actions.push(Action::PrintPoster(n));
                }
                crate::win32::PrintResult::Cancel => self.overlay = Overlay::None,
                crate::win32::PrintResult::None => {}
            }
            return;
        }
        if self.picker.captured() {
            self.picker.release(x, y, &self.assets, &self.audio, self.menu_state.tool_sounds);
            self.down = false;
            return;
        }
        if let Some((i, h, z0, sx, sy)) = self.scale_drag.take() {
            self.finish_scale(i, h, z0, sx, sy, x - CANVAS_X, y - CANVAS_Y);
            self.down = false;
            return;
        }
        if self.menu_press {
            self.menu_press = false;
            if let Some(id) = self.menubar.release(&self.fonts, &self.menu_state, x, y) {
                self.menu_command(id);
            }
            return;
        }
        self.down = false;
        self.drag_obj = None;
        self.drag_text = None;
        if self.brush_held {
            self.brush_held = false;
            let (bx, by) = ((x - CANVAS_X) as f32 + 0.5, (y - CANVAS_Y) as f32 + 0.5);
            self.with_brush(|t, c, ctx| t.end(c, bx, by, ctx));
            self.canvas.dirty = true;
            return;
        }
        self.stroke_up();
    }

    fn canvas_down(&mut self, x: i32, y: i32) {
        let (cx, cy) = (x - CANVAS_X, y - CANVAS_Y);
        let (fx, fy) = (cx as f32 + 0.5, cy as f32 + 0.5);
        // Placing a sticker/animation.
        if let Some((frames, fps, source)) = self.armed.take() {
            if self.objects.len() >= self.max_objects() {
                let t = self.ini_text("GeneralStrings", "TooManyPageObjects", "There are too many objects in the picture.");
                self.alert(t);
            } else {
                let (w, h) = (frames[0].width as i32, frames[0].height as i32);
                let movie = self.armed_movie.take();
                if movie.is_some() {
                    // One movie per picture: a new one replaces the old.
                    self.objects.retain(|o| o.movie.is_none());
                }
                self.objects.push(PageObj::new(frames, fps, cx - w / 2, cy - h / 2, source, movie));
                self.selected = Some(self.objects.len() - 1);
                self.canvas.dirty = true;
            }
            self.down = false;
            return;
        }
        // Edit-frame handles of the selected object.
        if let Some(i) = self.selected.filter(|&i| i < self.objects.len()) {
            if let Some(h) = self.objects[i].handle_at(cx, cy) {
                match h {
                    Handle::ScaleTL | Handle::ScaleBR => self.scale_drag = Some((i, h, self.objects[i].zoom, cx, cy)),
                    Handle::FlipH => self.objects[i].flip_h = !self.objects[i].flip_h,
                    Handle::FlipV => self.objects[i].flip_v = !self.objects[i].flip_v,
                    Handle::Loop => self.objects[i].looping = !self.objects[i].looping,
                    Handle::Step => self.objects[i].step(),
                }
                self.canvas.dirty = true;
                return;
            }
        }
        // The glove also moves text boxes (until they are flattened).
        if self.tool == ToolKind::Mover && matches!(self.item(G::Variant), Some(Item::Grab(GrabTool::Hand))) && self.object_at(x, y).is_none() {
            if let Some(i) = self.text_at(x, y) {
                let t = &self.texts[i];
                self.drag_text = Some((i, cx - t.x, cy - t.y));
                return;
            }
        }
        // Objects float above the picture: clicking one selects it and drags it.
        if let Some(i) = self.object_at(x, y) {
            let o = &self.objects[i];
            self.drag_obj = Some((i, cx - o.x, cy - o.y));
            self.selected = Some(i);
            return;
        }
        self.selected = None;
        if let Stroke::PasteAim = self.stroke {
            self.stroke = Stroke::None;
            if let Some(mut o) = self.clipboard.clone() {
                let (w, h) = o.size();
                o.x = cx - w / 2;
                o.y = cy - h / 2;
                self.objects.push(o);
                self.selected = Some(self.objects.len() - 1);
                self.canvas.dirty = true;
            }
            self.down = false;
            return;
        }
        // Clicks that continue a multi-click shape.
        match std::mem::replace(&mut self.stroke, Stroke::None) {
            Stroke::CurveBend { a, b, pen, base, .. } => {
                self.canvas.img = base;
                let pts = paint::curve_points(a, (fx, fy), b);
                self.stroke_path(&pts, &pen, false);
                self.down = false;
                return;
            }
            Stroke::Polygon { mut pts, filled, pen, base, .. } => {
                let first = pts[0];
                let close = pts.len() >= 3 && ((first.0 - fx).powi(2) + (first.1 - fy).powi(2)).sqrt() < 8.0;
                if close {
                    self.canvas.img = base;
                    if filled {
                        paint::fill_polygon(&mut self.canvas.img, &pts, self.color);
                    }
                    self.stroke_path(&pts, &pen, true);
                } else {
                    pts.push((fx, fy));
                    self.stroke = Stroke::Polygon { pts, filled, pen, base };
                }
                self.down = false;
                return;
            }
            Stroke::CutPolygon { mut pts, .. } => {
                let first = pts[0];
                if pts.len() >= 3 && ((first.0 - fx).powi(2) + (first.1 - fy).powi(2)).sqrt() < 8.0 {
                    self.cut_out(&pts);
                } else {
                    pts.push((fx, fy));
                    self.stroke = Stroke::CutPolygon { pts, cur: (fx, fy) };
                }
                self.down = false;
                return;
            }
            other => self.stroke = other,
        }
        if let Some(id) = self.current_brush_id() {
            self.brush_down(id, fx, fy);
            return;
        }
        match self.tool {
            ToolKind::Pencil => {
                let pen = self.current_pen();
                let shape = match self.item(G::Shape) {
                    Some(Item::Shape(k)) => k,
                    _ => Shape::Freehand,
                };
                let filled = matches!(self.item(G::FillMode), Some(Item::Filled(true)));
                self.canvas.checkpoint();
                match shape {
                    // Freehand runs on the brush engine (current_brush_id).
                    Shape::Freehand => {}
                    Shape::Polygon => {
                        self.stroke = Stroke::Polygon { pts: vec![(fx, fy)], filled, pen, base: self.canvas.img.clone() };
                        self.down = false;
                    }
                    _ => {
                        self.stroke = Stroke::ShapeDrag { shape, start: (fx, fy), cur: (fx, fy), filled, pen, base: self.canvas.img.clone() };
                    }
                }
            }
            // Brushes, Sound Art and spray run on the brush engine (current_brush_id).
            ToolKind::Brush => {}
            ToolKind::Fill => {
                let kind = match self.item(G::Options) {
                    Some(Item::Fill(k)) => k,
                    _ => FillKind::Tint(0),
                };
                let style = tools::fill_style(kind, &self.assets);
                let region = crate::fills::region(&self.canvas.img, cx, cy);
                if !region.is_empty() {
                    self.canvas.checkpoint();
                    paint::paint_region(&mut self.canvas.img, &region, &style, self.color, (fx, fy));
                    self.play_cfg_sound(match kind {
                        FillKind::Tint(_) => "FillSolid",
                        FillKind::Pattern(_) => "FillPattern",
                        FillKind::Gradient(_) => "FillGradient",
                    });
                    if let paint::FillStyle::Gradient(g) = style {
                        // Holding the button and moving moves where the blend sits.
                        self.stroke = Stroke::Blend { region, grad: g };
                    }
                }
                if !matches!(self.stroke, Stroke::Blend { .. }) {
                    self.down = false;
                }
            }
            ToolKind::Mixer => match self.item(G::Options) {
                Some(Item::MixFx(i)) => {
                    // The Big Mixer runs once over the whole picture, centred on the click.
                    self.canvas.checkpoint();
                    let sz = self.tray().map(|t| t.size_idx()).unwrap_or(1) as i32 + 1;
                    crate::mixer::apply(&mut self.canvas.img, i as usize, (cx, cy), sz, self.space, self.color, &mut self.rng);
                    self.play_cfg_sound(crate::mixer::NAMES[i as usize]);
                    self.down = false;
                }
                _ => self.down = false,
            },
            ToolKind::Stamp => {
                let scale = match self.item(G::Size) {
                    Some(Item::StampSize(n)) => [1, 2, 4][(n as usize).clamp(1, 3) - 1],
                    _ => 1,
                };
                if let Some(Item::Stamp(id)) = self.item(G::Options) {
                    if let Some(st) = self.stamp_image_by_id(id) {
                        let img = if scale == 1 { st } else { scale_nearest(&st, scale) };
                        self.canvas.checkpoint();
                        gfx::blit(&mut self.canvas.img, &img, cx - img.width as i32 / 2, cy - img.height as i32 / 2);
                        self.play_cfg_sound("SimpleBitmap");
                    }
                }
                self.down = false;
            }
            ToolKind::Text if self.menu_state.small_kids && matches!(self.item(G::Options), Some(Item::Letter(_))) => {
                let Some(Item::Letter(i)) = self.item(G::Options) else { unreachable!() };
                // The letter is a simple-bitmap stamp of the glyph in the current colour.
                if let Some(g) = self.letter_bitmap(i as usize, self.color) {
                    self.commit_text();
                    self.canvas.checkpoint();
                    gfx::blit(&mut self.canvas.img, &g, cx - g.width as i32 / 2, cy - g.height as i32 / 2);
                    self.play_cfg_sound("SimpleBitmap");
                }
                self.down = false;
            }
            ToolKind::Text => {
                if let Some(i) = self.text_at(x, y) {
                    if self.editing.map(|e| e.0) != Some(i) {
                        self.commit_text();
                    }
                    let i = self.text_at(x, y).unwrap_or(i);
                    let n = self.texts[i].text.chars().count();
                    let t = &self.texts[i];
                    (self.text_font, self.text_size, self.text_bold, self.text_italic, self.text_underline, self.text_align) =
                        (t.font, t.size, t.bold, t.italic, t.underline, t.align);
                    self.editing = Some((i, n));
                } else {
                    self.commit_text();
                    let lh = (self.text_size * 1.2).ceil() as i32;
                    self.texts.push(crate::textbox::TextBox {
                        x: cx - 3,
                        y: cy - lh / 2 - 3,
                        text: String::new(),
                        font: self.text_font,
                        size: self.text_size,
                        bold: self.text_bold,
                        italic: self.text_italic,
                        underline: self.text_underline,
                        align: self.text_align,
                        color: self.color,
                    });
                    self.editing = Some((self.texts.len() - 1, 0));
                }
                self.down = false;
            }
            ToolKind::Mover => {
                if let (Some(Item::Grab(GrabTool::Scissors)), Some(Item::Cut(shape))) = (self.item(G::Variant), self.item(G::Options)) {
                    match shape {
                        CutShape::Polygon => {
                            self.stroke = Stroke::CutPolygon { pts: vec![(fx, fy)], cur: (fx, fy) };
                            self.down = false;
                        }
                        CutShape::Cookie(id) => {
                            if let Some(r) = crate::regions::load(&self.assets, id) {
                                let (x0, y0) = (cx - r.width as i32 / 2, cy - r.height as i32 / 2);
                                self.cut_mask(x0, y0, r.width as i32, r.height as i32, |x, y| r.contains(x, y));
                            }
                            self.down = false;
                        }
                        _ => self.stroke = Stroke::Cut { shape, pts: vec![(fx, fy)], start: (fx, fy), cur: (fx, fy) },
                    }
                }
            }
            ToolKind::Eraser => {
                if matches!(self.item(G::Variant), Some(Item::BigEraser(true))) {
                    if let Some(Item::BigErase(i)) = self.item(G::Options) {
                        self.start_big_erase(i as usize, (fx, fy));
                    }
                    self.down = false;
                }
            }
        }
    }

    /// The current Drawing Tool as a pen (Pencil line width or real-media pad).
    fn current_pen(&self) -> Pen {
        let size = self.tray().map(|t| t.size_idx()).unwrap_or(1) as usize;
        let sec = match self.item(G::Variant) {
            Some(Item::Draw(DrawTool::Chalk)) => "Chalk",
            Some(Item::Draw(DrawTool::Crayon)) => "Crayon",
            Some(Item::Draw(DrawTool::Marker)) => "Marker",
            _ => return Pen::Pencil([2.0, 10.0, 20.0][size.min(2)]),
        };
        let p = tools::media_params(&self.assets, sec);
        let pad = tools::pad_for(&self.pads, &p, 6.0).scaled([0.6, 1.0, 1.6][size.min(2)]);
        Pen::Media(p, pad, self.paper.clone())
    }

    /// Draw a path (shape outline) with a pen.
    fn stroke_path(&mut self, pts: &[(f32, f32)], pen: &Pen, closed: bool) {
        let mut pts = pts.to_vec();
        if closed && !pts.is_empty() {
            pts.push(pts[0]);
        }
        match pen {
            Pen::Pencil(w) => {
                for win in pts.windows(2) {
                    paint::line(&mut self.canvas.img, win[0].0, win[0].1, win[1].0, win[1].1, *w, self.color);
                }
            }
            Pen::Media(p, pad, paper) => {
                let mut st = MediaStroke::new(p.clone(), pad.clone(), self.rng.next_u32() as u64);
                st.paper = paper.clone();
                for &(x, y) in &pts {
                    st.to(&mut self.canvas.img, x, y, self.color);
                }
            }
        }
    }

    /// Scissors: lift the pixels inside a polygon into a movable piece, leaving white.
    fn cut_out(&mut self, pts: &[(f32, f32)]) {
        let mut inside = Vec::new();
        paint::for_polygon_pixels(CANVAS_W as i32, CANVAS_H as i32, pts, |x, y| inside.push((x, y)));
        if inside.len() < 4 {
            return;
        }
        let set: std::collections::HashSet<(i32, i32)> = inside.into_iter().collect();
        let (x0, y0, x1, y1) = paint::region_bbox(&set.iter().copied().collect::<Vec<_>>());
        self.cut_mask(x0, y0, x1 - x0 + 1, y1 - y0 + 1, |x, y| set.contains(&(x + x0, y + y0)));
    }

    fn cut_mask(&mut self, x0: i32, y0: i32, w: i32, h: i32, inside: impl Fn(i32, i32) -> bool) {
        if w <= 0 || h <= 0 {
            return;
        }
        self.canvas.checkpoint();
        let mut piece = Image::new(w as u32, h as u32);
        for y in 0..h {
            for x in 0..w {
                if inside(x, y) {
                    if let Some(p) = self.canvas.img.get(x0 + x, y0 + y) {
                        piece.put(x, y, p);
                        self.canvas.img.put(x0 + x, y0 + y, WHITE);
                    }
                }
            }
        }
        self.objects.push(PageObj::new(vec![piece], 1.0, x0, y0, None, None));
        self.selected = Some(self.objects.len() - 1);
        self.play_wav(tools::TP, 18020, audio::CH_TOOL);
    }



    /// Play a tool's start sound from its `[<Name>Snd]` section in KPD3.cfg
    /// (the ids are tWAVs in Data/Tools/WackyBrushSounds.MHK).
    fn play_cfg_sound(&self, name: &str) {
        let sec = format!("{name}Snd");
        if let Some(id) = self.assets.cfg.get_i32(&sec, "startSoundID") {
            self.play_wav("Data/Tools/WackyBrushSounds.MHK", id as u16, audio::CH_TOOL);
        }
    }

    fn start_big_erase(&mut self, i: usize, at: (f32, f32)) {
        self.commit_text();
        self.canvas.checkpoint();
        self.play_cfg_sound(BIG_ERASERS[i.min(14)]);
        if i == 14 {
            // Vanish: instant.
            self.canvas.img = Image::filled(CANVAS_W, CANVAS_H, WHITE);
            return;
        }
        self.big_erase = Some(BigErase { idx: i, from: self.canvas.img.clone(), t: 0.0, at, ticks: 0, tick_acc: 0.0, extra: 0 });
    }

    fn update_big_erase(&mut self, dt: f32) {
        let Some(be) = self.big_erase.as_mut() else { return };
        be.t += dt;
        if be.idx == 0 {
            // Firecracker: every ~120 ms a white oval grows by 20 px per side around the click;
            // once it covers the page, 7 more ticks, then the page is white.
            be.tick_acc += dt;
            while be.tick_acc >= 120.0 {
                be.tick_acc -= 120.0;
                be.ticks += 1;
                let r = be.ticks as f32 * 20.0;
                let (x0, y0, x1, y1) = (be.at.0 - r, be.at.1 - r, be.at.0 + r, be.at.1 + r);
                let pts = paint::oval_points((x0, y0), (x1, y1));
                paint::fill_polygon(&mut self.canvas.img, &pts, WHITE);
                if x0 <= 0.0 && y0 <= 0.0 && x1 >= CANVAS_W as f32 && y1 >= CANVAS_H as f32 {
                    be.extra += 1;
                }
                if be.extra >= 7 {
                    self.canvas.img = Image::filled(CANVAS_W, CANVAS_H, WHITE);
                    self.big_erase = None;
                    return;
                }
            }
        } else {
            let k = (be.t / 1200.0).min(1.0);
            let white = Image::filled(CANVAS_W, CANVAS_H, WHITE);
            crate::slideshow::transition(BIG_ERASE_FX[be.idx], k, &be.from, &white, &mut self.canvas.img);
            if k >= 1.0 {
                // the effect ends on the blank page
                self.canvas.img = white;
                self.big_erase = None;
            }
        }
    }

    fn max_objects(&self) -> usize {
        self.assets.ini.get_i32("PageObjectLimits", "SWFMax").unwrap_or(15).max(1) as usize + 20
    }




    fn stroke_to(&mut self, _prev: (i32, i32), (x, y): (i32, i32)) {
        let (cx, cy) = (x - CANVAS_X, y - CANVAS_Y);
        let (fx, fy) = (cx as f32 + 0.5, cy as f32 + 0.5);
        let mut st = std::mem::replace(&mut self.stroke, Stroke::None);
        match &mut st {
            Stroke::ShapeDrag { shape, start, cur, filled, pen, base } => {
                *cur = (fx, fy);
                self.canvas.img = base.clone();
                let pts = match shape {
                    Shape::Rect => Some(paint::rect_points(*start, *cur)),
                    Shape::Oval => Some(paint::oval_points(*start, *cur)),
                    _ => None,
                };
                let pen = pen.clone();
                match pts {
                    Some(p) => {
                        if *filled {
                            paint::fill_polygon(&mut self.canvas.img, &p, self.color);
                        }
                        self.stroke_path(&p, &pen, true);
                    }
                    None => self.stroke_path(&[*start, *cur], &pen, false),
                }
            }
            Stroke::Blend { region, grad } => {
                // Holding the button and moving moves the gradient's focus.
                crate::fills::gradient_fill(&mut self.canvas.img, region, grad, (fx, fy));
            }
            Stroke::Cut { shape, pts, cur, .. } => {
                *cur = (fx, fy);
                if *shape == CutShape::Free {
                    pts.push((fx, fy));
                }
            }
            _ => {}
        }
        self.stroke = st;
    }

    /// Mouse released: finish shapes, cuts and blends.
    fn stroke_up(&mut self) {
        match std::mem::replace(&mut self.stroke, Stroke::None) {
            Stroke::ShapeDrag { shape: Shape::Curve, start, cur, pen, base, .. } => {
                // First drag sets the end points; now the line bends with the mouse.
                self.stroke = Stroke::CurveBend { a: start, b: cur, pen, base };
            }
            Stroke::Cut { shape, pts, start, cur } => {
                let poly = match shape {
                    CutShape::Free => pts,
                    CutShape::Circle => paint::oval_points(start, cur),
                    _ => paint::rect_points(start, cur),
                };
                self.cut_out(&poly);
            }
            keep @ (Stroke::Polygon { .. } | Stroke::CutPolygon { .. } | Stroke::CurveBend { .. } | Stroke::PasteAim) => self.stroke = keep,
            other => {
                if !matches!(other, Stroke::None) {
                    self.canvas.dirty = true;
                }
                // keep multi-click strokes alive (re-stored above)
            }
        }
    }

    fn drawer_click(&mut self, x: i32, y: i32) {
        let Some(d) = self.drawer.as_mut() else { return };
        // Arrows at the right end (from Easel scripts 1071-1077).
        if x >= 690 {
            let pages = d.items.len().div_ceil(SLOT_COUNT).max(1);
            if y < 461 {
                d.page = (d.page + pages - 1) % pages;
            } else {
                d.page = (d.page + 1) % pages;
            }
            d.ensure_thumbs(d.page * SLOT_COUNT, SLOT_COUNT);
            return;
        }
        if (SLOT_Y0..SLOT_Y0 + 52).contains(&y) && x >= SLOT_X0 {
            let slot = ((x - SLOT_X0) / SLOT_W) as usize;
            if slot < SLOT_COUNT {
                let idx = d.page * SLOT_COUNT + slot;
                self.drawer_pick(idx);
            }
        }
    }


    /// Run an original menu command (ids from KPD3.exe's menu resource).
    pub fn menu_command(&mut self, id: u16) {
        use crate::menubar::cmd::*;
        if !self.menu_state.enabled(id) {
            return;
        }
        match id {
            NEW => self.actions.push(Action::New),
            OPEN => self.actions.push(Action::Open),
            SAVE_AS => self.actions.push(Action::SaveAs),
            EXPORT => self.actions.push(Action::Export),
            PRINT => self.overlay = Overlay::Print(crate::win32::PrintDialog::new()),
            NEW_USER => self.actions.push(Action::Login),
            EXIT => self.actions.push(Action::Quit),
            UNDO_REDO => self.undo_redo(),
            CUT => self.cut(),
            COPY => self.copy(),
            PASTE => self.paste(),
            ADD_GRAPHIC => self.actions.push(Action::AddGraphic),
            ADD_MOVIE => self.actions.push(Action::AddMovie),
            IMPORT_BACKGROUND => self.actions.push(Action::ImportBackground),
            IMPORT_SOUND => self.actions.push(Action::ImportSound),
            EDIT_ABC_TEXT => self.actions.push(Action::EditAbcText),
            CHECK_SPELLING => self.start_spell(),
            EMAIL_SETUP => self.actions.push(Action::EmailSetup),
            EMAIL_SEND => self.overlay = Overlay::Email(crate::win32::EmailDialog::new()),
            FLATTEN_TEXT => self.actions.push(Action::Flatten(225)),
            FLATTEN_STICKERS => self.actions.push(Action::Flatten(214)),
            SAY_IT_THIS_WAY => self.actions.push(Action::SayItThisWay),
            IDEA_MACHINE => self.actions.push(Action::IdeaMachine),
            TOOL_SOUNDS => self.menu_state.tool_sounds = !self.menu_state.tool_sounds,
            TEXT_TO_SPEECH => self.menu_state.text_to_speech = !self.menu_state.text_to_speech,
            ATTACHED_SOUND => self.menu_state.attached_sound = !self.menu_state.attached_sound,
            MANUAL_ADVANCE => self.menu_state.manual_advance = !self.menu_state.manual_advance,
            PNG_COPY => self.menu_state.png_copy = !self.menu_state.png_copy,
            SMALL_KIDS => {
                self.menu_state.small_kids = !self.menu_state.small_kids;
                self.menubar.close();
                self.menubar.layout(&self.fonts, self.menu_state.small_kids);
            }
            PROJECT_MODE => self.command(270),
            HELP => self.command(239),
            ABOUT => {
                self.about = self
                    .assets
                    .archive("Data/About.MHK")
                    .and_then(|a| a.ids(assets::T_SCR).first().copied())
                    .and_then(|sid| Player::new(&self.assets, "Data/About.MHK", sid));
            }
            USER_GUIDE => self.actions.push(Action::UserGuide),
            _ => {}
        }
    }


    /// The Undo Guy / Edit > Undo/Redo: undo, and clicking again redoes it.
    pub fn undo_redo(&mut self) {
        // While typing, Undo/Redo swaps the text box with its state before this typing run.
        if let Some((i, caret)) = self.editing {
            if let Some((ti, text, c)) = self.text_undo.take().filter(|u| u.0 == i) {
                if let Some(t) = self.texts.get_mut(ti) {
                    let cur = std::mem::replace(&mut t.text, text);
                    self.text_undo = Some((ti, cur, caret));
                    self.editing = Some((ti, c.min(t.text.chars().count())));
                    self.typing_run = false;
                    self.canvas.dirty = true;
                }
                return;
            }
        }
        if self.canvas.last_was_undo {
            self.redo();
        } else {
            self.undo();
        }
    }

    fn cut(&mut self) {
        if let Some(i) = self.selected.take().filter(|&i| i < self.objects.len()) {
            self.clipboard = Some(self.objects.remove(i));
        }
    }

    fn copy(&mut self) {
        if let Some(o) = self.selected.and_then(|i| self.objects.get(i)) {
            self.clipboard = Some(o.clone());
        }
    }

    fn paste(&mut self) {
        if let Some(mut o) = self.clipboard.clone() {
            let (w, h) = o.size();
            o.x = (CANVAS_W as i32 - w) / 2;
            o.y = (CANVAS_H as i32 - h) / 2;
            self.objects.push(o);
            self.selected = Some(self.objects.len() - 1);
            self.canvas.dirty = true;
        }
    }

    /// Toolbox > Flatten Stickers and Animations: merge every object into the picture.
    pub fn flatten_objects(&mut self) {
        if self.objects.is_empty() {
            return;
        }
        self.canvas.checkpoint();
        let imgs: Vec<(Image, i32, i32)> = self.objects.iter().map(|o| (o.current(self.playing, self.movie_time), o.x, o.y)).collect();
        for (im, x, y) in imgs {
            gfx::blit(&mut self.canvas.img, &im, x, y);
        }
        self.objects.clear();
        self.selected = None;
    }

    /// Start the placement-options dialog for an imported picture.
    pub fn import_picture(&mut self, img: Image, target: FitTarget) {
        match target {
            // Import Background asks how to place it (DIALOG 134 options).
            FitTarget::Background => self.overlay = Overlay::Fit(target, img, 0),
            // Add Graphic: a floating bitmap object at the centre, original size.
            FitTarget::Graphic => {
                let obj = knock_transparent(&img);
                let (w, h) = (obj.width as i32, obj.height as i32);
                self.objects.push(PageObj::new(vec![obj], 1.0, (CANVAS_W as i32 - w) / 2, (CANVAS_H as i32 - h) / 2, None, None));
                self.selected = Some(self.objects.len() - 1);
                self.canvas.dirty = true;
            }
        }
    }

    pub fn set_page_sound(&mut self, s: Arc<Sound>) {
        self.page_sound = Some(s.clone());
        self.audio.play(s, audio::CH_MUSIC, false);
        self.canvas.dirty = true;
    }

    fn fit_layout() -> (i32, i32, i32, i32) {
        (170, 150, 460, 270)
    }

    fn fit_radio_rect(i: usize) -> (i32, i32, i32, i32) {
        let (px, py, _, _) = Self::fit_layout();
        (px + 22, py + 52 + i as i32 * 24, 230, 16)
    }

    fn fit_buttons() -> [(i32, i32, i32, i32); 2] {
        let (px, py, pw, ph) = Self::fit_layout();
        [(px + pw - 180, py + ph - 36, 75, 23), (px + pw - 95, py + ph - 36, 75, 23)]
    }

    fn fit_click(&mut self, x: i32, y: i32) {
        let Overlay::Fit(target, img, sel) = std::mem::replace(&mut self.overlay, Overlay::None) else { return };
        for i in 0..4 {
            if crate::win32::inside(Self::fit_radio_rect(i), x, y) {
                self.overlay = Overlay::Fit(target, img, i);
                return;
            }
        }
        let [ok, cancel] = Self::fit_buttons();
        if crate::win32::inside(ok, x, y) {
            let placed = fit_picture(&img, sel);
            match target {
                FitTarget::Background => self.canvas.replace(placed),
                FitTarget::Graphic => {
                    let obj = knock_transparent(&placed);
                    self.objects.push(PageObj::new(vec![obj], 1.0, 0, 0, None, None));
                    self.selected = Some(self.objects.len() - 1);
                    self.canvas.dirty = true;
                }
            }
            return;
        }
        if crate::win32::inside(cancel, x, y) {
            return;
        }
        self.overlay = Overlay::Fit(target, img, sel);
    }

    /// "Import Background" placement options (DIALOG 134: preview + four radios).
    fn draw_fit(&self, frame: &mut Image, sel: usize) {
        if self.fonts.is_empty() {
            return;
        }
        let face = self.menubar_face();
        let c = crate::win32::frame(frame, &self.fonts, face, Self::fit_layout(), "Import Background");
        let Overlay::Fit(_, img, _) = &self.overlay else { return };
        for (i, label) in FIT_CHOICES.iter().enumerate() {
            let r = Self::fit_radio_rect(i);
            crate::win32::radio(frame, &self.fonts, face, r.0, r.1, label, i == sel, true);
        }
        // "Preview:" box with the placement result, canvas-shaped.
        let (bx, by, bw, bh) = (c.0 + c.2 - 170, c.1 + 26, 160, 120);
        self.fonts.draw(frame, face, crate::win32::FACE_SIZE, bx as f32, (by - 6) as f32, 0xff00_0000, "Preview:");
        gfx::fill_rect(frame, bx - 1, by - 1, bw + 2, bh + 2, 0xff80_8080);
        let prev = gfx::resize(&fit_picture(img, sel), bw as u32, bh as u32);
        gfx::blit(frame, &prev, bx, by);
        let [ok, cancel] = Self::fit_buttons();
        crate::win32::button(frame, &self.fonts, face, ok, "OK", false, true);
        crate::win32::button(frame, &self.fonts, face, cancel, "Cancel", false, false);
    }

    /// Menus and Win32-style dialogs use the built-in bitmap UI font on every OS.
    fn menubar_face(&self) -> usize {
        crate::ui::UI_BITMAP
    }

    pub fn load_picture(&mut self, img: Image) {
        self.commit_text();
        let img = if img.width != CANVAS_W || img.height != CANVAS_H {
            gfx::resize(&img, CANVAS_W, CANVAS_H)
        } else {
            img
        };
        self.new_picture();
        self.canvas.img = img;
    }

    // ------------------------------------------------------------------ update & draw

    /// A decoded QuickTime movie: the next click on the picture places it.
    pub fn add_movie(&mut self, m: crate::movie::Movie) {
        let poster = m.frame(0);
        self.armed = Some((vec![poster], 1.0, None));
        self.armed_movie = Some(Rc::new(m));
    }

    fn set_color(&mut self, c: u32) {
        let changed = self.color != c;
        self.color = c;
        // Tinted tray icons (texture fills, realistic brushes) follow the colour.
        if changed && matches!(self.tool, ToolKind::Fill | ToolKind::Brush | ToolKind::Text) {
            self.rebuild_tray();
        }

    }


    pub fn update(&mut self, dt: f32) {
        self.clock += dt;
        self.sync_hands_free();
        if self.brush.is_some() {
            let ms = dt.round().max(0.0) as u32;
            if self.brush_held {
                let (bx, by) = ((self.mouse.0 - CANVAS_X) as f32 + 0.5, (self.mouse.1 - CANVAS_Y) as f32 + 0.5);
                self.with_brush(|t, c, ctx| t.tick(c, bx, by, ms, ctx));
            } else if self.brush.as_ref().is_some_and(|b| b.1.is_animating()) {
                self.with_brush(|t, c, ctx| t.idle(c, ms, ctx));
            }
        }
        if let Some(r) = &self.sound_art {
            r.trim(48000);
        }
        if let Overlay::StampEditor(ed, _) = &mut self.overlay {
            ed.update(dt, &self.assets, &self.audio);
        }
        if let Some(p) = self.lib_panel.as_mut() {
            p.update(dt, &self.assets, &self.audio, self.menu_state.tool_sounds);
        }
        // Recording: the meter (script 18274, 101 frames) shows the share of the 10 s used.
        if self.recorder.as_ref().is_some_and(|r| r.full()) {
            if let Some(r) = self.recorder.take() {
                self.recording = Some(std::sync::Arc::new(r.finish()));
            }
        }
        if let Some(m) = self.rec_meter.as_mut() {
            let secs = self.recorder.as_ref().map(|r| r.seconds()).or_else(|| self.recording.as_ref().map(|s| s.samples.len() as f32 / s.sample_rate.max(1) as f32)).unwrap_or(0.0);
            m.seek_frame(((secs / 10.0) * 100.0).round().clamp(0.0, 100.0) as usize);
        }
        self.menu_state.redo = self.canvas.last_was_undo;
        self.menu_state.has_text = self.texts.iter().any(|t| !t.text.trim().is_empty());
        self.menu_state.undo_what = if self.editing.is_some() { self.undo_words.1 } else { self.undo_words.0 };
        self.hover_time += dt;
        self.picker.update(dt, &self.assets, &self.audio, self.menu_state.tool_sounds);
        if let Some(sh) = self.shutter.as_mut() {
            if sh.update(dt, &self.assets, &self.audio, self.menu_state.tool_sounds) {
                self.panel = None;
                self.rebuild_panel();
            }
            if self.shutter.as_ref().is_some_and(|s| s.done()) {
                self.shutter = None;
            }
        }
        if let Some(p) = self.panel.as_mut() {
            p.update(dt, &self.assets, &self.audio, self.menu_state.tool_sounds);
        }
        let sel = self.panel_selection();
        if let Some(p) = self.panel.as_mut() {
            p.sync(&sel);
        }
        self.update_big_erase(dt);
        let mut sounds: Vec<u16> = Vec::new();
        for b in self.buttons.iter_mut() {
            if let Some(p) = b.player() {
                sounds.extend(p.update(dt));
            }
        }
        // Dynamite: when its click animation is done, clear the page.
        if self.dynamite_clear {
            let done = self
                .buttons
                .iter()
                .find(|b| b.cmd == 266)
                .and_then(|b| b.player_ref())
                .is_none_or(|p| p.finished);
            if done {
                self.dynamite_clear = false;
                self.commit_text();
                self.canvas.checkpoint();
                self.canvas.img = Image::filled(CANVAS_W, CANVAS_H, WHITE);
                self.objects.clear();
                if let Some(b) = self.buttons.iter_mut().find(|b| b.cmd == 266) {
                    b.show(0, false);
                }
            }
        }
        if let Some(p) = self.undo_talk.as_mut() {
            sounds.extend(p.update(dt));
            if p.finished {
                self.undo_talk = None;
            }
        }
        for p in self.effects.iter_mut() {
            sounds.extend(p.update(dt));
        }
        self.effects.retain(|p| !p.finished);
        self.play_script_sounds(EASEL, 1000, &sounds);

        for o in self.objects.iter_mut() {
            o.t += dt / 1000.0;
        }
        if self.playing {
            self.movie_time += dt / 1000.0;
            let longest = self.objects.iter().filter_map(|o| o.movie.as_ref()).map(|m| m.duration()).fold(0.0, f32::max);
            let has_movies = longest > 0.0;
            if has_movies && self.movie_time > longest && self.page_sound.is_none() {
                self.playing = false;
            }
        }
    }

    pub fn draw(&self, frame: &mut Image) {
        crate::anim::set_tint(self.dropper_preview.unwrap_or(self.color));
        match &self.bg {
            Some(bg) => frame.pixels.copy_from_slice(&bg.pixels),
            None => frame.pixels.fill(gfx::rgb(60, 70, 160)),
        }
        // Picture (with the brush engine's live previews), text boxes, objects.
        match self.brush.as_ref().filter(|b| b.1.is_animating() || self.brush_held) {
            Some((_, tool)) => {
                let mut shown = self.canvas.img.clone();
                tool.overlay(&mut shown);
                gfx::blit(frame, &shown, CANVAS_X, CANVAS_Y);
            }
            None => gfx::blit(frame, &self.canvas.img, CANVAS_X, CANVAS_Y),
        }
        let clip = (CANVAS_X, CANVAS_Y, CANVAS_X + CANVAS_W as i32, CANVAS_Y + CANVAS_H as i32);
        if !self.texts.is_empty() {
            let mut layer = Image::new(CANVAS_W, CANVAS_H);
            for t in &self.texts {
                t.draw(&mut layer, &self.fonts, 0, 0);
            }
            gfx::blit(frame, &layer, CANVAS_X, CANVAS_Y);
        }
        for (i, o) in self.objects.iter().enumerate() {
            let img = o.current(self.playing, self.movie_time);
            gfx::blit_clip(frame, &img, CANVAS_X + o.x, CANVAS_Y + o.y, Some(clip));
            if self.selected == Some(i) {
                o.draw_frame(frame, CANVAS_X, CANVAS_Y, self.handles.as_deref(), self.mouse);
            }
        }
        // Polygon / curve guides.
        if let Stroke::CutPolygon { pts, cur } = &self.stroke {
            for w in pts.windows(2) {
                dashed(frame, (w[0].0 + CANVAS_X as f32, w[0].1 + CANVAS_Y as f32), (w[1].0 + CANVAS_X as f32, w[1].1 + CANVAS_Y as f32));
            }
            if let Some(l) = pts.last() {
                dashed(frame, (l.0 + CANVAS_X as f32, l.1 + CANVAS_Y as f32), (cur.0 + CANVAS_X as f32, cur.1 + CANVAS_Y as f32));
            }
        }
        if let Stroke::Cut { shape, pts, start, cur } = &self.stroke {
            let poly = match shape {
                CutShape::Free => pts.clone(),
                CutShape::Circle => paint::oval_points(*start, *cur),
                _ => paint::rect_points(*start, *cur),
            };
            for i in 0..poly.len() {
                let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
                if *shape == CutShape::Free && i + 1 == poly.len() {
                    break;
                }
                dashed(frame, (a.0 + CANVAS_X as f32, a.1 + CANVAS_Y as f32), (b.0 + CANVAS_X as f32, b.1 + CANVAS_Y as f32));
            }
        }
        // Selection marquee.
        // Text box being typed into: dashed edit frame and caret.
        if let Some((i, c)) = self.editing {
            if let Some(t) = self.texts.get(i) {
                let (rx, ry, w, h) = t.rect(&self.fonts);
                let (x0, y0) = ((CANVAS_X + rx) as f32, (CANVAS_Y + ry) as f32);
                let (x1, y1) = (x0 + w as f32, y0 + h as f32);
                for (a, b) in [((x0, y0), (x1, y0)), ((x1, y0), (x1, y1)), ((x1, y1), (x0, y1)), ((x0, y1), (x0, y0))] {
                    dashed(frame, a, b);
                }
                if (self.clock / 500.0) as i64 % 2 == 0 {
                    let (cx, cy, ch) = t.caret(&self.fonts, c);
                    gfx::fill_rect(frame, CANVAS_X + rx + cx, CANVAS_Y + ry + cy, 1, ch, t.color);
                }
            }
        }
        // A fixed-shape cutter follows the pointer (marching-ants outline).
        if self.tool == ToolKind::Mover && Self::in_canvas(self.mouse.0, self.mouse.1) && self.stroke_is_idle() {
            if let (Some(Item::Grab(GrabTool::Scissors)), Some(Item::Cut(CutShape::Cookie(id)))) = (self.item(G::Variant), self.item(G::Options)) {
                if let Some(r) = crate::regions::load(&self.assets, id) {
                    let (x0, y0) = (self.mouse.0 - r.width as i32 / 2, self.mouse.1 - r.height as i32 / 2);
                    let phase = (self.clock / 120.0) as i32;
                    for yy in 0..r.height as i32 {
                        for xx in 0..r.width as i32 {
                            let inside = |x: i32, y: i32| x >= 0 && y >= 0 && x < r.width as i32 && y < r.height as i32 && r.mask[(y * r.width as i32 + x) as usize];
                            if inside(xx, yy) && !(inside(xx - 1, yy) && inside(xx + 1, yy) && inside(xx, yy - 1) && inside(xx, yy + 1)) {
                                let c = if ((xx + yy + phase) / 4) % 2 == 0 { 0xff00_0000 } else { 0xffff_ffff };
                                let (px, py) = (x0 + xx, y0 + yy);
                                if px >= clip.0 && py >= clip.1 && px < clip.2 && py < clip.3 {
                                    frame.put(px, py, c);
                                }
                            }
                        }
                    }
                }
            }
        }
        // The stamp follows the pointer as a ghost (centred on its middle).
        if Self::in_canvas(self.mouse.0, self.mouse.1) && self.drawer.is_none() && !self.menubar.shown && self.armed.is_none() {
            if let Some(g) = self.stamp_ghost() {
                gfx::blit_clip(frame, &g, self.mouse.0 - g.width as i32 / 2, self.mouse.1 - g.height as i32 / 2, Some(clip));
            }
        }
        // Armed sticker preview follows the mouse.
        if let Some((frames, _, _)) = &self.armed {
            if Self::in_canvas(self.mouse.0, self.mouse.1) {
                let f = &frames[0];
                gfx::blit_clip(frame, f, self.mouse.0 - f.width as i32 / 2, self.mouse.1 - f.height as i32 / 2, Some(clip));
            }
        }
        // Stamp preview outline.
        if self.tool == ToolKind::Stamp && Self::in_canvas(self.mouse.0, self.mouse.1) && !self.down {
            if let Some(st) = &self.stamp {
                let s = self.stamp_scale.max(1) as i32;
                let (w, h) = (st.width as i32 * s, st.height as i32 * s);
                marquee(frame, self.mouse.0 - w / 2, self.mouse.1 - h / 2, w, h);
            }
        }

        // Original UI layers.
        for p in &self.decor {
            p.draw(frame);
        }
        if let Some(p) = &self.splat {
            // Current colour splat: Easel tBMP 7000 (7001 in Small Kids Mode), an 8-bit
            // grey mask drawn with Tint multiply: dst = C5 * g >> 8, g == 0 kept.
            let id = if self.menu_state.small_kids { 7001 } else { 7000 };
            let pos = p.placed().first().map(|c| (c.1, c.2)).unwrap_or((17, 540));
            if let Some(mask) = self.splat_mask.get(&id) {
                let (_, r, g, b) = gfx::split(self.dropper_preview.unwrap_or(self.color));
                let c5 = |v: u32| v >> 3;
                let (r5, g5, b5) = (c5(r), c5(g), c5(b));
                let ex = |v: u32| ((v << 3) | (v >> 2)) as u8;
                for yy in 0..mask.height as i32 {
                    for xx in 0..mask.width as i32 {
                        let gv = mask.indices[(yy * mask.width as i32 + xx) as usize] as u32;
                        if gv == 0 {
                            continue;
                        }
                        let (fx, fy) = (pos.0 + xx, pos.1 + yy);
                        if fx < 0 || fy < 0 || fx >= frame.width as i32 || fy >= frame.height as i32 {
                            continue;
                        }
                        frame.pixels[(fy * frame.width as i32 + fx) as usize] = gfx::rgb(ex(r5 * gv >> 8), ex(g5 * gv >> 8), ex(b5 * gv >> 8));
                    }
                }
            } else {
                p.draw(frame);
            }
        }
        match &self.undo_talk {
            Some(p) => p.draw(frame),
            None => {
                if let Some(p) = &self.undo_face {
                    p.draw(frame);
                }
            }
        }
        for b in &self.buttons {
            if !self.button_visible(b) {
                continue;
            }
            if b.cmd == 243 && !self.has_playable() {
                if let Some(p) = &self.play_disabled {
                    p.draw(frame);
                    continue;
                }
            }
            if let Some(p) = b.player_ref() {
                p.draw(frame);
            }
        }
        for p in &self.effects {
            p.draw(frame);
        }

        self.draw_tray(frame);
        self.draw_drawer(frame);

        match &self.overlay {
            Overlay::Fit(_, _, sel) => self.draw_fit(frame, *sel),
            Overlay::Print(d) => d.draw(frame, &self.fonts, self.menubar_face()),
            Overlay::StampEditor(ed, _) => ed.draw(frame, &self.fonts),
            Overlay::Email(d) => d.draw(frame, &self.fonts, self.menubar_face()),
            Overlay::Spell(d, _) => d.draw(frame, &self.fonts, self.menubar_face(), (self.clock / 530.0) as i64 % 2 == 0),
            Overlay::None => {}
        }
        self.picker.draw(frame);
        self.draw_lib_popup(frame);
        if let Some(p) = &self.about {
            p.draw(frame);
        }
        self.menubar.draw(frame, &self.fonts, &self.menu_state);
    }

    fn draw_drawer(&self, frame: &mut Image) {
        let Some(d) = self.drawer.as_ref().filter(|d| d.kind == LibKind::Paper) else { return };
        if let Some(p) = &self.drawer_frame {
            p.draw(frame);
        } else {
            gfx::fill_round_rect(frame, 305, 420, 408, 80, 20, gfx::rgb(110, 230, 100));
        }
        if let Some(p) = &self.slot_frames {
            p.draw(frame);
        }
        for s in 0..SLOT_COUNT {
            let idx = d.page * SLOT_COUNT + s;
            let x = SLOT_X0 + s as i32 * SLOT_W;
            if let Some(item) = d.items.get(idx) {
                if let Some(t) = &item.thumb {
                    gfx::blit(frame, t, x + 2 + (48 - t.width as i32) / 2, SLOT_Y0 + 2 + (48 - t.height as i32) / 2);
                }
                if ui::hit(self.mouse.0, self.mouse.1, x, SLOT_Y0, SLOT_W, 52) {
                    gfx::stroke_rect(frame, x, SLOT_Y0, SLOT_W + 1, 52, 2, gfx::rgb(40, 200, 60));
                }
            }
        }
        let pages = d.items.len().div_ceil(SLOT_COUNT);
        if pages > 1 {
            ui::arrow_button(frame, 690, 434, 18, -1, true, true);
            ui::arrow_button(frame, 690, 470, 18, 1, true, true);
        }
        // Hovered item title.
        let (mx, my) = self.mouse;
        if (SLOT_Y0..SLOT_Y0 + 52).contains(&my) && mx >= SLOT_X0 && mx < SLOT_X0 + SLOT_W * SLOT_COUNT as i32 {
            let idx = d.page * SLOT_COUNT + ((mx - SLOT_X0) / SLOT_W) as usize;
            if let Some(item) = d.items.get(idx) {
                if !item.title.is_empty() {
                    self.bubble(frame, mx, SLOT_Y0 - 30, &item.title);
                }
            }
        }
    }


    fn bubble(&self, frame: &mut Image, x: i32, y: i32, text: &str) {
        if self.fonts.is_empty() {
            return;
        }
        let f = self.fonts.ui_face();
        let w = self.fonts.measure(f, 15.0, text) as i32 + 16;
        let x = (x - w / 2).clamp(2, 798 - w);
        gfx::fill_round_rect(frame, x, y, w, 24, 10, gfx::rgb(255, 255, 210));
        gfx::stroke_rect(frame, x + 3, y, w - 6, 1, 1, gfx::rgb(120, 120, 60));
        self.fonts.draw(frame, f, 15.0, (x + 8) as f32, (y + 17) as f32, gfx::rgb(0, 0, 0), text);
    }



    /// Cursor for the current state. `Cur(n)` = cursor n of `General.MHK` (tCUR 10000+n).
    pub fn cursor_hint(&self) -> CursorHint {
        let (x, y) = self.mouse;
        if self.menubar.hot(y) && !self.menubar.shown {
            return CursorHint::Drop;
        }
        if self.menubar.shown {
            return CursorHint::Cur(28);
        }
        if self.picker.eyedropper {
            return CursorHint::Cur(6);
        }
        if !matches!(self.overlay, Overlay::None) {
            return CursorHint::Cur(28);
        }
        if self.hover_btn.is_some() {
            return CursorHint::Cur(14);
        }
        if !Self::in_canvas(x, y) || self.drawer_rect().is_some_and(|r| ui::hit(x, y, r.0, r.1, r.2, r.3)) {
            return CursorHint::Cur(28);
        }
        if self.armed.is_some() {
            return CursorHint::Cur(11);
        }
        CursorHint::Cur(match self.tool {
            ToolKind::Pencil => match self.item(G::Variant) {
                Some(Item::Draw(DrawTool::Marker)) => 2,
                Some(Item::Draw(DrawTool::Chalk | DrawTool::Crayon)) => 0,
                _ => 1,
            },
            ToolKind::Brush if matches!(self.item(G::Variant), Some(Item::Paint(PaintTool::SoundArt))) => 21,
            ToolKind::Brush if matches!(self.item(G::Variant), Some(Item::Paint(PaintTool::Spray))) => 6,
            ToolKind::Mover if matches!(self.item(G::Variant), Some(Item::Grab(GrabTool::Scissors))) => 13,
            ToolKind::Eraser if matches!(self.item(G::Variant), Some(Item::BigEraser(true))) => 4,
            ToolKind::Brush => 15,
            ToolKind::Fill => 17,
            ToolKind::Mixer => 12,
            ToolKind::Stamp => 24,
            ToolKind::Text => 19,
            ToolKind::Mover => {
                if self.drag_obj.is_some() {
                    8
                } else {
                    9
                }
            }
            ToolKind::Eraser => 3,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorHint {
    /// Cursor index into `Data/General.MHK` (tCUR id 10000 + n).
    Cur(u16),
    /// The white-outlined triangle that reveals the menu bar.
    Drop,
}

fn marquee(frame: &mut Image, x: i32, y: i32, w: i32, h: i32) {
    for i in 0..w.max(0) {
        let c = if (i / 4) % 2 == 0 { 0xff00_0000 } else { WHITE };
        frame.put(x + i, y, c);
        frame.put(x + i, y + h, c);
    }
    for i in 0..h.max(0) {
        let c = if (i / 4) % 2 == 0 { 0xff00_0000 } else { WHITE };
        frame.put(x, y + i, c);
        frame.put(x + w, y + i, c);
    }
}

fn scale_nearest(src: &Image, s: u32) -> Image {
    let mut out = Image::new(src.width * s, src.height * s);
    for y in 0..out.height {
        for x in 0..out.width {
            out.pixels[(y * out.width + x) as usize] = src.pixels[((y / s) * src.width + x / s) as usize];
        }
    }
    out
}

impl Easel {
    /// Self-test: screen position of the panel control that selects item `idx` of group `g`
    /// (pages the options scroller when needed).
    pub fn test_tray_pos(&mut self, g: G, idx: usize) -> Option<(i32, i32)> {
        self.shutter = None;
        self.rebuild_panel();
        if g == G::Options {
            let n = self.panel.as_ref()?.slots().max(1);
            let grp = self.trays.get_mut(&self.tool)?.groups.iter_mut().find(|x| x.kind == G::Options)?;
            if idx >= grp.items.len() {
                return None;
            }
            grp.page = idx / n;
            let r = self.panel.as_ref()?.slot_rect(idx % n);
            return Some((r.0 + r.2 / 2, r.1 + r.3 / 2));
        }
        let (cmd, value) = self.panel_target(g, idx)?;
        let b = self.panel.as_ref()?.buttons.iter().find(|b| b.cmd == cmd && (value == u16::MAX || b.value == value))?;
        Some(((b.rect.left as i32 + b.rect.right as i32) / 2, (b.rect.top as i32 + b.rect.bottom as i32) / 2))
    }

    /// The panel (cmd, value) that selects item `idx` of group `g` (u16::MAX = any value).
    fn panel_target(&self, g: G, idx: usize) -> Option<(u16, u16)> {
        let any = u16::MAX;
        let shape = match self.item(G::Shape) {
            Some(Item::Shape(s)) => Some(s),
            _ => None,
        };
        let filled = matches!(self.item(G::FillMode), Some(Item::Filled(true)));
        let v = self.tray().and_then(|t| t.group(G::Variant)).map(|g| g.sel).unwrap_or(0);
        Some(match (self.tool, g) {
            (_, G::Size) => (18200, idx as u16 + 1),
            (ToolKind::Pencil, G::Variant) => (18400, idx as u16),
            (ToolKind::Pencil, G::Shape) => {
                let s = [Shape::Freehand, Shape::Curve, Shape::Line, Shape::Rect, Shape::Oval, Shape::Polygon].get(idx).copied();
                (pencil_shape_cmd(s, filled), any)
            }
            (ToolKind::Pencil, G::FillMode) => (pencil_shape_cmd(shape, idx == 0), any),
            (ToolKind::Brush, G::Variant) => (18000 + idx as u16, any),
            (ToolKind::Brush, G::Mode) => ([[18003, 18004], [18006, 18005], [18007, 18008]][v.min(2)][idx.min(1)], any),
            (ToolKind::Fill, G::Variant) => (18300 + idx as u16, any),
            (ToolKind::Mixer, G::Variant) => (18600 + idx as u16, any),
            (ToolKind::Mover, G::Variant) => ([22301, 22300][idx.min(1)], any),
            (ToolKind::Eraser, G::Variant) => ([18501, 18500][idx.min(1)], any),
            (ToolKind::Eraser, G::Shape) => ([18201, 18202][idx.min(1)], any),
            _ => return None,
        })
    }

    /// Self-test: press a library-panel command (e.g. 200 = record mode).
    pub fn test_lib_command(&mut self, cmd: u16) {
        self.lib_command(cmd);
    }

    /// Self-test: run an easel button command.
    /// Self-test: start big eraser `i` (0–14, `BIG_ERASERS` order) at canvas point `at`.
    pub fn test_big_erase(&mut self, i: usize, at: (f32, f32)) {
        self.start_big_erase(i, at);
    }

    /// Self-test: whether a big eraser is still running.
    pub fn test_big_erasing(&self) -> bool {
        self.big_erase.is_some()
    }

    pub fn test_command(&mut self, cmd: u16) {
        self.command(cmd);
    }

    /// Self-test: centre of the library panel's visible slot k.
    pub fn test_lib_slot(&self, k: usize) -> Option<(i32, i32)> {
        let r = self.lib_panel.as_ref()?.slot_rect(k);
        Some((r.0 + r.2 / 2, r.1 + r.3 / 2))
    }

    /// Self-test: centre of the current panel's button with this command.
    pub fn panel_button(&mut self, cmd: u16) -> Option<(i32, i32)> {
        self.shutter = None;
        self.rebuild_panel();
        let b = self.panel.as_ref()?.buttons.iter().find(|b| b.cmd == cmd)?;
        Some(((b.rect.left as i32 + b.rect.right as i32) / 2, (b.rect.top as i32 + b.rect.bottom as i32) / 2))
    }

    pub fn tool_button(&self, cmd: u16) -> Option<(i32, i32)> {
        self.buttons.iter().find(|b| b.cmd == cmd).map(|b| ((b.rect.left as i32 + b.rect.right as i32) / 2, (b.rect.top as i32 + b.rect.bottom as i32) / 2))
    }

    /// Used by the headless self-test: blank page with a colored frame.
    pub fn new_picture_for_test(&mut self) {
        self.new_picture();
        gfx::fill_rect(&mut self.canvas.img, 0, 0, CANVAS_W as i32, CANVAS_H as i32, gfx::rgb(250, 220, 90));
        gfx::fill_circle(&mut self.canvas.img, 300.0, 226.0, 120.0, gfx::rgb(40, 120, 230));
    }
}

/// Apply one of the four import placement options to a picture for the 600x452 page.
pub fn fit_picture(img: &Image, mode: usize) -> Image {
    let (cw, ch) = (CANVAS_W, CANVAS_H);
    match mode {
        0 => gfx::resize(img, cw, ch),
        1 => {
            let s = (cw as f32 / img.width as f32).min(ch as f32 / img.height as f32);
            let r = gfx::resize(img, (img.width as f32 * s) as u32, (img.height as f32 * s) as u32);
            let mut out = Image::filled(cw, ch, WHITE);
            gfx::blit(&mut out, &r, (cw as i32 - r.width as i32) / 2, (ch as i32 - r.height as i32) / 2);
            out
        }
        2 => {
            let mut out = Image::filled(cw, ch, WHITE);
            gfx::blit(&mut out, img, (cw as i32 - img.width as i32) / 2, (ch as i32 - img.height as i32) / 2);
            out
        }
        _ => {
            // Repeat many small copies (tiles about a quarter of the page wide).
            let s = (150.0 / img.width as f32).min(113.0 / img.height as f32).min(1.0);
            let t = gfx::resize(img, ((img.width as f32 * s) as u32).max(1), ((img.height as f32 * s) as u32).max(1));
            let mut out = Image::filled(cw, ch, WHITE);
            for y in (0..ch as i32).step_by(t.height as usize) {
                for x in (0..cw as i32).step_by(t.width as usize) {
                    gfx::blit(&mut out, &t, x, y);
                }
            }
            out
        }
    }
}

fn knock_transparent(img: &Image) -> Image {
    img.clone()
}


fn dashed(frame: &mut Image, a: (f32, f32), b: (f32, f32)) {
    let mut k = 0;
    gfx::walk_line(a.0, a.1, b.0, b.1, 1.0, |x, y| {
        frame.put(x as i32, y as i32, if (k / 4) % 2 == 0 { 0xff00_0000 } else { WHITE });
        k += 1;
    });
}



/// Big erasers (tool ids 123..137) in panel order; names = KPD3.cfg sound sections.
pub const BIG_ERASERS: [&str; 15] = [
    "Firecracker", "ClockFace", "IrisHeart", "IrisStar", "Windmill", "PageTear", "Spiral", "Implode", "Engulf", "Fade", "Explode",
    "Kaleidoscope", "Waterfall", "WipeRight", "Vanish",
];
/// The QuickTime-effect erasers, as the matching wipe of the engine's transition set.
const BIG_ERASE_FX: [usize; 15] = [0, 1, 7, 9, 23, 14, 20, 5, 2, 4, 3, 11, 22, 27, 0];


fn pencil_shape_cmd(shape: Option<Shape>, filled: bool) -> u16 {
    match (shape, filled) {
        (Some(Shape::Line), _) => 18450,
        (Some(Shape::Curve), _) => 18451,
        (Some(Shape::Rect), false) => 18452,
        (Some(Shape::Rect), true) => 18453,
        (Some(Shape::Oval), false) => 18454,
        (Some(Shape::Oval), true) => 18455,
        (Some(Shape::Polygon), false) => 18456,
        (Some(Shape::Polygon), true) => 18457,
        _ => 18458,
    }
}

struct BigErase {
    idx: usize,
    from: Image,
    t: f32,
    at: (f32, f32),
    ticks: u32,
    tick_acc: f32,
    extra: u32,
}

/// Default `[SmallKidsAlphabet] AlphabetSet` (with the original's "xzy").
pub const ALPHABET: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxzy1234567890,.;?!'@#$%&*/+-=<>()";

/// TP.MHK voice for alphabet letter `i` (RAlphaPanel, docs/RE_PANELS.md §3.6).
pub fn letter_voice(i: usize) -> u16 {
    match i {
        0..=25 => 22000 + i as u16,
        26..=67 => (i + 21974) as u16,
        68 => 22092,
        69..=71 => (i + 22021) as u16,
        _ => 22042,
    }
}
