//! The easel screen: original UI art and button scripts read from the user's CD, plus
//! the engine's own painting tools.

use crate::anim::Player;
use crate::buttons::UiButton;
use crate::assets::{self, Assets, SpriteSet};
use crate::audio::{self, Audio};
use crate::gfx::{self, Image};
use crate::library::{LibDrawer, LibKind};
use crate::slideshow::{SlideAction, SlideShow};
use crate::paint::{self, Canvas, MediaStroke, Rng, CANVAS_H, CANVAS_W, CANVAS_X, CANVAS_Y, WHITE};
use crate::tools::{self, EraserKind, FillKind, PadBank, Pen, Stroke, ToolKind, Wacky};
use crate::trays::{self, Bucket, CutShape, DrawTool, GrabAction, GrabTool, Item, PaintTool, Shape, Tray, G};
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
    SlideAddFile,
    Print,
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
    StampEditor,
}

pub use crate::objects::PageObj;
use crate::objects::Handle;

struct TextEntry {
    x: i32,
    y: i32,
    base: Image,
    text: String,
}

struct Transition {
    kind: EraserKind,
    from: Image,
    t: f32,
    dur: f32,
    noise: Vec<u8>,
}

enum Overlay {
    None,
    Palette,
    /// Picture-placement options for Add Graphic / Import Background.
    Fit(FitTarget, Image, usize),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FitTarget {
    Graphic,
    Background,
}

pub const FIT_CHOICES: [&str; 4] = [
    "Shrink/Stretch to Fit Canvas",
    "Fill Canvas Without Shrinking/Scaling",
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
    sprays: Vec<SprayAnim>,
    sound_art: Option<crate::record::Recorder>,
    wander: (f32, f32, f32, f32),
    pub color: u32,
    stroke: Stroke,
    mouse: (i32, i32),
    down: bool,
    overlay: Overlay,
    drawer: Option<LibDrawer>,
    pub objects: Vec<PageObj>,
    drag_obj: Option<(usize, i32, i32)>,
    armed: Option<(Vec<Image>, f32, Option<Rc<[u8]>>)>,
    armed_movie: Option<Rc<crate::movie::Movie>>,
    scale_drag: Option<(usize, Handle, f32, i32, i32)>,
    handles: Option<Rc<SpriteSet>>,
    movie_time: f32,
    playing: bool,
    page_sound: Option<Arc<Sound>>,
    text: Option<TextEntry>,
    text_font: usize,
    text_size: f32,
    stamp: Option<Image>,
    stamp_scale: u32,
    pads: PadBank,
    trails: HashMap<(&'static str, u16), Rc<SpriteSet>>,
    transition: Option<Transition>,
    rng: Rng,
    hover_time: f32,
    pub actions: Vec<Action>,
    message: Option<(String, f32)>,
    dynamite_clear: bool,
    slideshow: Option<SlideShow>,
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
const OPT_X: i32 = 322;
const OPT_Y: i32 = 527;
const OPT_SLOT: i32 = 56;
const OPT_PER_PAGE: usize = 6;
const OPT_ICON: u32 = 48;
const LIB_X: i32 = 100;
const LIB_Y: i32 = 512;
const LIB_W: i32 = 195;
const PAL_X: i32 = 6;
const PAL_Y: i32 = 318;
const PAL_COLS: usize = 12;
const PAL_CELL: i32 = 22;
const SLOT_X0: i32 = 330;
const SLOT_Y0: i32 = 435;
const SLOT_W: i32 = 51;
const SLOT_COUNT: usize = 7;
const FACE_RECT: (i32, i32, i32, i32) = (0, 238, 77, 53);
const COLOR_RECT: (i32, i32, i32, i32) = (0, 491, 85, 109);

fn command_name(cmd: u16) -> &'static str {
    match cmd {
        224 => "Backgrounds",
        222 => "Stickers",
        223 => "Animations",
        225 => "Sounds",
        232 => "Pencil",
        233 => "Wacky Brush",
        234 => "Paint Can",
        237 => "Electric Mixer",
        236 => "Stamper",
        229 => "Letters",
        227 => "Moving Van",
        228 => "Eraser",
        239 => "Help",
        242 => "Print",
        243 => "Play",
        244 => "Stop",
        270 => "Slide Show",
        281 => "Save",
        266 => "Dynamite: erase everything",
        267 => "Exit",
        _ => "",
    }
}

impl Easel {
    pub fn new(assets: Rc<Assets>, audio: Rc<Audio>) -> Easel {
        let mut fonts = Fonts::load(&assets.font_files());
        let menu_face = fonts.add_system_font().unwrap_or_else(|| fonts.ui_face());
        let menubar = crate::menubar::MenuBar::new(&fonts, menu_face);
        let bg = assets.image(EASEL, 1000);
        let mut buttons = crate::buttons::load(&assets, EASEL, 75);
        {
            let mk = |s: u16| Player::new(&assets, EASEL, s);
            let idle = mk(1046);
            let rect = idle.as_ref().map(|p| p.bounds()).unwrap_or_default();
            let mut b = UiButton { cmd: NEW_PICTURE_CMD, group: 0, value: 0, idle, hover: mk(1047), click: mk(1048), rect, showing: 0 };
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
            sprays: Vec::new(),
            sound_art: None,
            wander: (300.0, 226.0, 1.5, 0.7),
            color: gfx::rgb(0, 0, 0),
            stroke: Stroke::None,
            mouse: (0, 0),
            down: false,
            overlay: Overlay::None,
            drawer: None,
            objects: Vec::new(),
            drag_obj: None,
            armed: None,
            armed_movie: None,
            scale_drag: None,
            handles: assets.sprites("Data/Buttons.MHK", 7000),
            movie_time: 0.0,
            playing: false,
            page_sound: None,
            text: None,
            text_font: 0,
            text_size: 32.0,
            stamp: None,
            stamp_scale: 1,
            pads,
            trails: HashMap::new(),
            transition: None,
            rng: Rng::new(0x5eed_1234),
            hover_time: 0.0,
            actions: Vec::new(),
            message: None,
            dynamite_clear: false,
            slideshow: None,
            menubar,
            picker: crate::colorpicker::ColorPicker::new(),
            paper: None,
            dropper_preview: None,
            menu_press: false,
            menu_state: crate::menubar::MenuState { tool_sounds: true, text_to_speech: true, attached_sound: true, ..Default::default() },
            selected: None,
            clipboard: None,
            about: None,
            assets,
            audio,
        };
        e.text_font = e.fonts.ui_face();
        e.select_tool(ToolKind::Pencil, false);
        e
    }

    pub fn assets_root(&self) -> std::path::PathBuf {
        self.assets.root.clone()
    }

    pub fn message(&mut self, s: impl Into<String>) {
        self.message = Some((s.into(), 3500.0));
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
        self.drawer = None;
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
                let sizes = if v < 2 { vec![18200] } else { vec![] };
                (18045, [vec![18000, mode], sizes].concat(), Some((18022, 5)))
            }
            ToolKind::Fill => (18046, vec![18300], Some((18018, 7))),
            ToolKind::Mixer => (18047, vec![18600, 18200], Some((18021, 6))),
            ToolKind::Stamp => (18048, vec![18700, 18202], Some((18022, 5))),
            ToolKind::Text if small_kids => (18039, vec![], Some((18000, 7))),
            ToolKind::Text => (18049, vec![9383], None),
            ToolKind::Mover => (18050, vec![926], if v == 1 { Some((18020, 6)) } else { None }),
            ToolKind::Eraser => {
                if v == 1 {
                    (18051, vec![18500], Some((18024, 5)))
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
                (_, 18200) => sel(G::Size) + 1 == b.value as usize,
                (ToolKind::Brush, 18000..=18002) => sel(G::Variant) == (b.cmd - 18000) as usize,
                (ToolKind::Brush, 18003 | 18006 | 18007) => sel(G::Mode) == 0,
                (ToolKind::Brush, 18004 | 18005 | 18008) => sel(G::Mode) == 1,
                (ToolKind::Fill, 18300..=18302) => sel(G::Variant) == (b.cmd - 18300) as usize,
                (ToolKind::Mixer, 18600 | 18601) => sel(G::Variant) == (b.cmd - 18600) as usize,
                (ToolKind::Mover, 22301) => sel(G::Variant) == 0,
                (ToolKind::Mover, 22300) => sel(G::Variant) == 1,
                (ToolKind::Eraser, 18501) => sel(G::Variant) == 0,
                (ToolKind::Eraser, 18500) => sel(G::Variant) == 1,
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
            (ToolKind::Stamp, 18703 | 18704 | 18702) => {
                let n = self.assets.libraries("Libraries/Stamps").len().max(1);
                self.stamp_folder = match cmd {
                    18703 => self.stamp_folder.saturating_sub(1),
                    18704 => (self.stamp_folder + 1).min(n - 1),
                    _ => (self.stamp_folder + 1) % n,
                };
                if let Some(g) = self.trays.get_mut(&ToolKind::Stamp).and_then(|t| t.groups.iter_mut().find(|g| g.kind == G::Options)) {
                    g.sel = 0;
                    g.page = 0;
                }
                self.rebuild_tray();
            }
            (ToolKind::Stamp, 18701) => self.actions.push(Action::StampEditor),
            (ToolKind::Stamp, 18202) => self.set_sel(G::Size, value.saturating_sub(1) as usize),
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
        const SIZES: [f32; 14] = [7.0, 9.0, 10.0, 12.0, 14.0, 16.0, 18.0, 20.0, 22.0, 24.0, 28.0, 32.0, 48.0, 72.0];
        let nf = self.fonts.faces.iter().filter(|(n, _)| n != "System").count().max(1);
        match cmd {
            200 => self.text_font = (self.text_font + nf - 1) % nf,
            201 => self.text_font = (self.text_font + 1) % nf,
            202 | 203 => {
                let i = SIZES.iter().position(|&z| z >= self.text_size).unwrap_or(5);
                let j = if cmd == 202 { (i + 1).min(SIZES.len() - 1) } else { i.saturating_sub(1) };
                self.text_size = SIZES[j];
            }
            232 => {
                if let Some(t) = &self.text {
                    crate::speech::speak(&t.text, 0);
                }
            }
            _ => {}
        }
        if self.text.is_some() {
            self.render_text();
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
        self.tray_icons = t
            .groups
            .iter()
            .map(|g| {
                g.items
                    .iter()
                    .map(|it| match it {
                        Item::Stamp(id) => self.stamp_icon(*id),
                        _ => trays::icon(it, &self.assets, &self.pads, &self.fonts, OPT_ICON),
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
        crate::library::decode_stamp(self.stamp_archive.as_ref()?.get(assets::T_BMP, id)?)
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
            Item::StampEditor => self.actions.push(Action::StampEditor),
            Item::Letter(id) => self.play_wav(tools::TP, id, audio::CH_TOOL),
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
                Err(e) => self.message(format!("Sound Art needs a microphone: {e}")),
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
        if self.drawer.is_some() {
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

    /// Text fields of the panels: stamp library name, text font/size/voice.
    fn draw_panel_fields(&self, frame: &mut Image) {
        if self.fonts.is_empty() {
            return;
        }
        let vogel = self.fonts.faces.iter().position(|(n, _)| n.to_uppercase().starts_with("VOGEN") || n.eq_ignore_ascii_case("vogel")).unwrap_or_else(|| self.fonts.ui_face());
        let ink = gfx::rgb(0, 0, 0);
        match self.tool {
            ToolKind::Stamp => {
                self.fonts.draw_centered(frame, vogel, 16.0, 181.0, 564.0, 130.0, ink, &self.stamp_folder_name());
            }
            ToolKind::Text if !self.menu_state.small_kids => {
                let fname = self.fonts.faces.get(self.text_font).map(|f| f.0.clone()).unwrap_or_default();
                self.fonts.draw_centered(frame, vogel, 13.0, 164.0, 544.0, 106.0, ink, &fname);
                self.fonts.draw_centered(frame, vogel, 13.0, 254.0, 544.0, 26.0, ink, &format!("{}", self.text_size as i32));
                let voice = if crate::speech::available() { "Computer" } else { "-" };
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
        if self.drawer.is_none() {
            self.message(format!("No {} were found in your Kid Pix files.", kind.name()));
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
                self.canvas.dirty = true;
            }
            LibKind::Stickers | LibKind::Toons => {
                let (frames, fps, src) = d.movie_frames(id, kind == LibKind::Toons);
                if !frames.is_empty() {
                    self.armed = Some((frames, fps, src));
                    self.message("Click on your picture to put it there.");
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
            242 => self.actions.push(Action::Print),
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
                // Dynamite: the click script plays the explosion; clear when it ends.
                self.dynamite_clear = true;
            }
            238 => self.open_drawer(LibKind::Paper),
            239 => self.message(
                "Pick a tool on the left, choose a color at the bottom left, then draw! \
                 Click the face to undo. Hover over any button to see its name.",
            ),
            270 => {
                self.commit_text();
                if self.slideshow.is_none() {
                    self.slideshow = Some(SlideShow::new(self.assets.clone(), self.audio.clone()));
                }
            }
            _ => {}
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
        self.canvas.reset();
        self.objects.clear();
        self.page_sound = None;
        self.playing = false;
        self.audio.stop_channel(audio::CH_MUSIC);
    }

    /// Picture with stickers flattened (for saving).
    pub fn flattened(&mut self) -> Image {
        self.commit_text();
        let mut img = self.canvas.img.clone();
        for o in &self.objects {
            gfx::blit(&mut img, &o.current(self.playing, self.movie_time), o.x, o.y);
        }
        img
    }

    fn start_transition(&mut self, kind: EraserKind) {
        self.commit_text();
        self.canvas.checkpoint();
        let n = (CANVAS_W * CANVAS_H) as usize;
        let mut noise = vec![0u8; n];
        for v in noise.iter_mut() {
            *v = self.rng.below(256) as u8;
        }
        self.transition = Some(Transition {
            kind,
            from: self.canvas.img.clone(),
            t: 0.0,
            dur: 900.0,
            noise,
        });
        self.objects.clear();
        self.play_wav("Data/Deleters.MHK", self.deleter_sound_id(), audio::CH_TOOL);
    }

    fn deleter_sound_id(&self) -> u16 {
        self.assets
            .archive("Data/Deleters.MHK")
            .and_then(|a| a.ids(assets::T_WAV).first().copied())
            .unwrap_or(0)
    }

    // ------------------------------------------------------------------ text tool

    fn commit_text(&mut self) {
        self.text = None;
    }

    fn render_text(&mut self) {
        let Some(t) = &self.text else { return };
        let mut img = t.base.clone();
        let mut y = t.y as f32;
        for line in t.text.split('\n') {
            self.fonts.draw(&mut img, self.text_font, self.text_size, t.x as f32, y, self.color, line);
            y += self.text_size * 1.15;
        }
        self.canvas.img = img;
    }

    pub fn key_char(&mut self, c: char) {
        if self.tool != ToolKind::Text {
            return;
        }
        let Some(t) = self.text.as_mut() else { return };
        match c {
            '\u{8}' => {
                t.text.pop();
            }
            '\r' | '\n' => t.text.push('\n'),
            c if !c.is_control() => t.text.push(c),
            _ => return,
        }
        self.render_text();
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
        if self.text.is_some() {
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
        if let Some(ss) = self.slideshow.as_mut() {
            ss.key_escape();
            if ss.closed {
                self.slideshow = None;
            }
            return;
        }
        if !matches!(self.overlay, Overlay::None) {
            self.overlay = Overlay::None;
        } else if self.armed.is_some() {
            self.armed = None;
        } else if self.drawer.is_some() {
            self.drawer = None;
        } else {
            self.commit_text();
        }
    }

    // ------------------------------------------------------------------ input

    fn in_canvas(x: i32, y: i32) -> bool {
        ui::hit(x, y, CANVAS_X, CANVAS_Y, CANVAS_W as i32, CANVAS_H as i32)
    }

    fn drawer_rect(&self) -> Option<(i32, i32, i32, i32)> {
        self.drawer.as_ref()?;
        Some((305, 398, 408, 102))
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

    fn button_at(&self, x: i32, y: i32) -> Option<usize> {
        self.buttons
            .iter()
            .position(|b| self.button_visible(b) && b.rect.contains(x, y))
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
        self.menubar.motion(&self.fonts, &self.menu_state, x, y);
        if self.menu_press || self.menubar.open.is_some() {
            return;
        }
        self.picker.mouse_move(x, y, &self.assets, &self.audio, self.menu_state.tool_sounds);
        let sel = self.panel_selection();
        if let Some(p) = self.panel.as_mut() {
            p.mouse_move(x, y, &self.assets, &self.audio, self.menu_state.tool_sounds, &sel);
        }
        if self.picker.eyedropper {
            self.dropper_preview = Self::in_canvas(x, y).then(|| self.canvas.img.get(x - CANVAS_X, y - CANVAS_Y)).flatten();
        }
        if let Some(ss) = self.slideshow.as_mut() {
            ss.mouse_move(x, y);
            return;
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
                self.stroke = Stroke::CurveBend { a, b, ctrl: (fx, fy), pen, base };
            }
            Stroke::Polygon { pts, filled, pen, base, .. } => {
                self.canvas.img = base.clone();
                let mut p = pts.clone();
                p.push((fx, fy));
                self.stroke_path(&p, &pen, false);
                self.stroke = Stroke::Polygon { pts, cur: (fx, fy), filled, pen, base };
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
        if let Some(ss) = self.slideshow.as_mut() {
            if let Some(SlideAction::AddCurrent) = ss.mouse_down(x, y) {
                let img = self.flattened();
                if let Some(ss) = self.slideshow.as_mut() {
                    ss.add(img);
                }
            }
            if self.slideshow.as_ref().is_some_and(|s| s.want_file) {
                self.slideshow.as_mut().unwrap().want_file = false;
                self.actions.push(Action::SlideAddFile);
            }
            if self.slideshow.as_ref().is_some_and(|s| s.closed) {
                self.slideshow = None;
            }
            return;
        }
        self.down = true;
        if let Overlay::Fit(..) = self.overlay {
            self.fit_click(x, y);
            self.down = false;
            return;
        }
        if let Some(pick) = self.picker.press(x, y, &self.assets, &self.audio, self.menu_state.tool_sounds) {
            if let crate::colorpicker::Pick::Color(c) = pick {
                self.set_color(c);
            }
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
        if self.drawer.is_some() && ui::hit(x, y, LIB_X, LIB_Y, LIB_W, 80) {
            self.library_switch_click(x, y);
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
        self.stroke_up();
    }

    fn canvas_down(&mut self, x: i32, y: i32) {
        let (cx, cy) = (x - CANVAS_X, y - CANVAS_Y);
        let (fx, fy) = (cx as f32 + 0.5, cy as f32 + 0.5);
        // Placing a sticker/animation.
        if let Some((frames, fps, source)) = self.armed.take() {
            if self.objects.len() >= self.max_objects() {
                self.message("That's a lot of stickers! Remove one first.");
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
                    self.stroke = Stroke::Polygon { pts, cur: (fx, fy), filled, pen, base };
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
        let size = self.tray().map(|t| t.size_idx()).unwrap_or(1) as f32;
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
                    Shape::Freehand => match pen {
                        Pen::Pencil(w) => {
                            paint::line(&mut self.canvas.img, fx, fy, fx, fy, w, self.color);
                            self.stroke = Stroke::Pencil { last: (fx, fy), width: w };
                        }
                        Pen::Media(params, pad, paper) => {
                            let mut st = MediaStroke::new(params, pad, self.rng.next_u32() as u64);
                            st.paper = paper;
                            st.to(&mut self.canvas.img, fx, fy, self.color);
                            self.stroke = Stroke::Media(st);
                            let snd = self.felt_sound();
                            self.play_wav("Data/Tools/FeltPenSnds.MHK", snd, audio::CH_TOOL);
                        }
                    },
                    Shape::Polygon => {
                        self.stroke = Stroke::Polygon { pts: vec![(fx, fy)], cur: (fx, fy), filled, pen, base: self.canvas.img.clone() };
                        self.down = false;
                    }
                    _ => {
                        self.stroke = Stroke::ShapeDrag { shape, start: (fx, fy), cur: (fx, fy), filled, pen, base: self.canvas.img.clone() };
                    }
                }
            }
            ToolKind::Brush => match self.item(G::Variant) {
                Some(Item::Paint(PaintTool::Spray)) => {
                    let (arch, id, moving) = match self.item(G::Options) {
                        Some(Item::SprayMoving(id)) => (tools::ANIMS, id, true),
                        Some(Item::Spray3D(id)) => (tools::CYCLIC, id, false),
                        _ => return,
                    };
                    if let Some(set) = self.assets.sprites(arch, id) {
                        self.canvas.checkpoint();
                        self.stroke = Stroke::Spray { set, moving, rng: Rng::new(self.rng.next_u32() as u64), pos: (fx, fy), acc: 1.0, cel: 0, radius: 8.0 + size * 10.0 };
                        self.play_brush_sound();
                    }
                }
                Some(Item::Paint(PaintTool::SoundArt)) => {
                    if !matches!(self.item(G::Mode), Some(Item::HandsFree(true))) {
                        self.canvas.checkpoint();
                        self.stroke = Stroke::SoundArt { last: (fx, fy), hue: 0.0 };
                    }
                }
                _ => {
                    self.canvas.checkpoint();
                    let scale = [0.6, 1.0, 1.6][size.clamp(0.0, 2.0) as usize];
                    match self.item(G::Options) {
                        Some(Item::Real(sec)) => {
                            let p = tools::media_params(&self.assets, &sec);
                            let pad = tools::pad_for(&self.pads, &p, 8.0).scaled(scale);
                            let mut st = MediaStroke::new(p, pad, self.rng.next_u32() as u64);
                            st.paper = self.paper.clone();
                            st.to(&mut self.canvas.img, fx, fy, self.color);
                            self.stroke = Stroke::Media(st);
                        }
                        other => {
                            let kind = match other {
                                Some(Item::Brush(k)) => k,
                                _ => Wacky::Rainbow,
                            };
                            let media = match kind {
                                Wacky::Drippy => {
                                    let p = tools::media_params(&self.assets, "Drippy");
                                    let pad = tools::pad_for(&self.pads, &p, 8.0).scaled(scale);
                                    Some(MediaStroke::new(p, pad, self.rng.next_u32() as u64))
                                }
                                _ => None,
                            };
                            let mut st = Stroke::Wacky { kind, last: (fx, fy), dist: 1000.0, rng: Rng::new(self.rng.next_u32() as u64), hue: 0.0, cel: 0, media };
                            self.wacky_apply(&mut st, (fx, fy));
                            self.stroke = st;
                        }
                    }
                    self.play_brush_sound();
                }
            },
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
                Some(Item::Wand(i)) => {
                    self.canvas.checkpoint();
                    let radius = [5.0, 10.0, 20.0][size.clamp(0.0, 2.0) as usize];
                    let e = wand_effect(i);
                    paint::effect_dab(&mut self.canvas.img, e, fx, fy, radius);
                    self.stroke = Stroke::MiniMix { effect: e, radius, last: (fx, fy) };
                    self.play_cfg_sound(WAND_NAMES[i as usize % WAND_NAMES.len()]);
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
            ToolKind::Text if matches!(self.item(G::Options), Some(Item::Letter(_))) => {
                let Some(Item::Letter(id)) = self.item(G::Options) else { unreachable!() };
                if let Some(g) = tools::letter_glyph(&self.assets, id, self.color, 2.0) {
                    self.commit_text();
                    self.canvas.checkpoint();
                    gfx::blit(&mut self.canvas.img, &g, cx - g.width as i32 / 2, cy - g.height as i32 / 2);
                    self.play_wav(tools::TP, id, audio::CH_TOOL);
                }
                self.down = false;
            }
            ToolKind::Text => {
                self.commit_text();
                self.canvas.checkpoint();
                self.text = Some(TextEntry { x: cx, y: cy + (self.text_size * 0.4) as i32, base: self.canvas.img.clone(), text: String::new() });
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
                } else {
                    let px = [2, 10, 20][size.clamp(0.0, 2.0) as usize];
                    let round = !matches!(self.item(G::Shape), Some(Item::EraserRound(false)));
                    self.canvas.checkpoint();
                    erase_dab(&mut self.canvas.img, fx, fy, px, round);
                    self.stroke = Stroke::Eraser { last: (fx, fy), size: px, round };
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

    fn update_spray(&mut self, dt: f32) {
        if let Stroke::Spray { set, moving, rng, pos, acc, cel, radius } = &mut self.stroke {
            *acc += dt / 1000.0 * 14.0;
            while *acc >= 1.0 && !set.cels.is_empty() {
                *acc -= 1.0;
                let a = rng.range(0.0, std::f32::consts::TAU);
                let d = rng.range(0.0, *radius);
                let (x, y) = (pos.0 + a.cos() * d, pos.1 + a.sin() * d);
                let i = *cel % set.cels.len();
                *cel += 1 + rng.below(3) as usize;
                if *moving {
                    // Moving paint animates for a little while, then flattens.
                    self.sprays.push(SprayAnim { set: set.clone(), x, y, t: 0.0, life: 1600.0 + rng.range(0.0, 900.0), phase: i });
                } else {
                    let c = &set.cels[i];
                    gfx::blit(&mut self.canvas.img, c, x as i32 - c.width as i32 / 2, y as i32 - c.height as i32 / 2);
                }
            }
        }
        let mut done = Vec::new();
        for (k, s) in self.sprays.iter_mut().enumerate() {
            s.t += dt;
            if s.t >= s.life {
                done.push(k);
            }
        }
        for k in done.into_iter().rev() {
            let s = self.sprays.remove(k);
            let c = s.current();
            gfx::blit(&mut self.canvas.img, c, s.x as i32 - c.width as i32 / 2, s.y as i32 - c.height as i32 / 2);
        }
    }

    /// Hands-free Sound Art: the microphone paints on its own.
    fn update_hands_free(&mut self, dt: f32) {
        if self.tool != ToolKind::Brush || !matches!(self.item(G::Mode), Some(Item::HandsFree(true))) {
            return;
        }
        let Some(level) = self.sound_art.as_ref().map(|r| r.level()) else { return };
        if level < 0.04 {
            return;
        }
        let (mut x, mut y, mut vx, mut vy) = self.wander;
        let k = dt / 16.0;
        vx += self.rng.range(-0.4, 0.4) * k;
        vy += self.rng.range(-0.4, 0.4) * k;
        let sp = (vx * vx + vy * vy).sqrt().max(0.01);
        let max = 1.0 + level * 6.0;
        if sp > max {
            vx *= max / sp;
            vy *= max / sp;
        }
        x += vx * k;
        y += vy * k;
        if !(0.0..CANVAS_W as f32).contains(&x) {
            vx = -vx;
            x = x.clamp(0.0, CANVAS_W as f32 - 1.0);
        }
        if !(0.0..CANVAS_H as f32).contains(&y) {
            vy = -vy;
            y = y.clamp(0.0, CANVAS_H as f32 - 1.0);
        }
        self.wander = (x, y, vx, vy);
        let hue = (self.hover_time / 20.0) % 360.0;
        gfx::fill_circle(&mut self.canvas.img, x, y, 2.0 + level * 26.0, gfx::hsv(hue, 0.9, 1.0));
        self.canvas.dirty = true;
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
                self.big_erase = None;
            }
        }
    }

    fn max_objects(&self) -> usize {
        self.assets.ini.get_i32("PageObjectLimits", "SWFMax").unwrap_or(15).max(1) as usize + 20
    }

    fn felt_sound(&mut self) -> u16 {
        let ids = self
            .assets
            .archive("Data/Tools/FeltPenSnds.MHK")
            .map(|a| a.ids(assets::T_WAV))
            .unwrap_or_default();
        if ids.is_empty() {
            0
        } else {
            ids[self.rng.below(ids.len() as u32) as usize]
        }
    }

    fn play_brush_sound(&mut self) {
        // Until the per-brush sound table is mapped, use the brush's position in the list
        // to pick a stable sound from WackyBrushSounds.MHK.
        let ids = self
            .assets
            .archive("Data/Tools/WackyBrushSounds.MHK")
            .map(|a| a.ids(assets::T_WAV))
            .unwrap_or_default();
        if ids.is_empty() {
            return;
        }
        let sel = self.tray().and_then(|t| t.group(G::Options)).map(|g| g.sel).unwrap_or(0);
        let id = ids[(sel * 7) % ids.len()];
        if !self.audio.channel_busy(audio::CH_TOOL) {
            self.play_wav("Data/Tools/WackyBrushSounds.MHK", id, audio::CH_TOOL);
        }
    }

    fn wacky_apply(&mut self, st: &mut Stroke, p: (f32, f32)) {
        if let Stroke::Wacky { kind, last, dist, rng, hue, cel, media } = st {
            let trail = tools::trail_source(*kind).and_then(|k| self.trails.get(&k).cloned());
            tools::wacky_step(
                &mut self.canvas.img,
                *kind,
                *last,
                p,
                dist,
                rng,
                hue,
                cel,
                self.color,
                trail.as_deref(),
                media,
            );
            *last = p;
        }
    }

    fn stroke_to(&mut self, _prev: (i32, i32), (x, y): (i32, i32)) {
        let (cx, cy) = (x - CANVAS_X, y - CANVAS_Y);
        let (fx, fy) = (cx as f32 + 0.5, cy as f32 + 0.5);
        let mut st = std::mem::replace(&mut self.stroke, Stroke::None);
        match &mut st {
            Stroke::Pencil { last, width } => {
                paint::line(&mut self.canvas.img, last.0, last.1, fx, fy, *width, self.color);
                *last = (fx, fy);
            }
            Stroke::Media(m) => m.to(&mut self.canvas.img, fx, fy, self.color),
            Stroke::Wacky { .. } => {
                self.wacky_apply(&mut st, (fx, fy));
                self.play_brush_sound();
            }
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
            Stroke::Eraser { last, size, round } => {
                let (s, r) = (*size, *round);
                let img = &mut self.canvas.img;
                gfx::walk_line(last.0, last.1, fx, fy, 1.0, |px, py| erase_dab(img, px, py, s, r));
                *last = (fx, fy);
            }
            Stroke::MiniMix { effect, radius, last } => {
                let (e, r) = (*effect, *radius);
                let img = &mut self.canvas.img;
                gfx::walk_line(last.0, last.1, fx, fy, (r / 3.0).max(2.0), |px, py| paint::effect_dab(img, e, px, py, r));
                *last = (fx, fy);
            }
            Stroke::Blend { region, grad } => {
                // Holding the button and moving moves the gradient's focus.
                crate::fills::gradient_fill(&mut self.canvas.img, region, grad, (fx, fy));
            }
            Stroke::Spray { pos, .. } => *pos = (fx, fy),
            Stroke::SoundArt { last, hue } => {
                let level = self.sound_art.as_ref().map(|r| r.level()).unwrap_or(0.3);
                let r = 2.0 + level * 30.0;
                *hue = (*hue + 3.0 + level * 20.0) % 360.0;
                let c = gfx::hsv(*hue, 0.9, 1.0);
                let img = &mut self.canvas.img;
                gfx::walk_line(last.0, last.1, fx, fy, 3.0, |px, py| gfx::fill_circle(img, px, py, r, c));
                *last = (fx, fy);
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
                self.stroke = Stroke::CurveBend { a: start, b: cur, ctrl: cur, pen, base };
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

    fn library_switch_click(&mut self, x: i32, y: i32) {
        // Left panel: [<] library name [>], close button top-right.
        if ui::hit(x, y, LIB_X + LIB_W - 26, LIB_Y + 2, 22, 22) {
            self.drawer = None;
            return;
        }
        let Some(d) = self.drawer.as_mut() else { return };
        if y > LIB_Y + 30 {
            if x < LIB_X + LIB_W / 2 {
                d.switch(-1);
            } else {
                d.switch(1);
            }
        }
    }

    fn palette_at(&self, x: i32, y: i32) -> Option<u32> {
        let cols = ui::palette_colors();
        let rows = cols.len().div_ceil(PAL_COLS);
        let (px, py) = (PAL_X + 10, PAL_Y + 10);
        if x < px || y < py {
            return None;
        }
        let c = ((x - px) / PAL_CELL) as usize;
        let r = ((y - py) / PAL_CELL) as usize;
        if c >= PAL_COLS || r >= rows {
            return None;
        }
        cols.get(r * PAL_COLS + c).copied()
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
            PRINT => self.actions.push(Action::Print),
            NEW_USER => self.actions.push(Action::Login),
            EXIT => self.actions.push(Action::Quit),
            UNDO_REDO => self.undo_redo(),
            CUT => self.cut(),
            COPY => self.copy(),
            PASTE => self.paste(),
            ADD_GRAPHIC => self.actions.push(Action::AddGraphic),
            ADD_MOVIE => self.actions.push(Action::AddMovie),
            LOAD_PICTURE => self.actions.push(Action::SlideAddFile),
            IMPORT_BACKGROUND => self.actions.push(Action::ImportBackground),
            IMPORT_SOUND => self.actions.push(Action::ImportSound),
            EDIT_ABC_TEXT => self.actions.push(Action::EditAbcText),
            CHECK_SPELLING => self.actions.push(Action::CheckSpelling),
            FLATTEN_TEXT => self.commit_text(),
            FLATTEN_STICKERS => self.flatten_objects(),
            SAY_IT_THIS_WAY => self.actions.push(Action::SayItThisWay),
            IDEA_MACHINE => self.actions.push(Action::IdeaMachine),
            TOOL_SOUNDS => self.menu_state.tool_sounds = !self.menu_state.tool_sounds,
            TEXT_TO_SPEECH => self.menu_state.text_to_speech = !self.menu_state.text_to_speech,
            ATTACHED_SOUND => self.menu_state.attached_sound = !self.menu_state.attached_sound,
            MANUAL_ADVANCE => self.menu_state.manual_advance = !self.menu_state.manual_advance,
            SMALL_KIDS => {
                self.menu_state.small_kids = !self.menu_state.small_kids;
                self.menubar.close();
                self.menubar.layout(&self.fonts, self.menu_state.small_kids);
                self.slideshow = None;
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

    /// Dialog-based commands handled inside the easel (filled in by the dialog modules).
    pub fn dialog_action(&mut self, a: Action) {
        let what = match a {
            Action::Login => "logging in as a new user",
            Action::EditAbcText => "ABC Text",
            Action::CheckSpelling => "spelling check",
            Action::StampEditor => "Rubber Stamp Editor",
            _ => "pronunciation",
        };
        self.message(format!("The {what} window is still being rebuilt."));
    }

    /// The Undo Guy / Edit > Undo/Redo: undo, and clicking again redoes it.
    pub fn undo_redo(&mut self) {
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
        self.overlay = Overlay::Fit(target, img, 0);
    }

    pub fn set_page_sound(&mut self, s: Arc<Sound>) {
        self.page_sound = Some(s.clone());
        self.audio.play(s, audio::CH_MUSIC, false);
        self.canvas.dirty = true;
    }

    fn fit_layout() -> (i32, i32, i32, i32) {
        (220, 170, 380, 230)
    }

    fn fit_click(&mut self, x: i32, y: i32) {
        let (px, py, pw, ph) = Self::fit_layout();
        let Overlay::Fit(target, img, sel) = std::mem::replace(&mut self.overlay, Overlay::None) else { return };
        for i in 0..4 {
            if ui::hit(x, y, px + 20, py + 50 + i * 30, pw - 40, 26) {
                self.overlay = Overlay::Fit(target, img, i as usize);
                return;
            }
        }
        if ui::hit(x, y, px + pw - 190, py + ph - 44, 80, 30) {
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
        if ui::hit(x, y, px + pw - 100, py + ph - 44, 80, 30) {
            return; // cancel
        }
        self.overlay = Overlay::Fit(target, img, sel);
    }

    fn draw_fit(&self, frame: &mut Image, sel: usize) {
        let (px, py, pw, ph) = Self::fit_layout();
        ui::panel(frame, px, py, pw, ph);
        if self.fonts.is_empty() {
            return;
        }
        let f = self.menubar_face();
        let ink = gfx::rgb(0, 0, 0);
        self.fonts.draw(frame, f, 15.0, (px + 20) as f32, (py + 34) as f32, ink, "How should the picture be placed?");
        for (i, label) in FIT_CHOICES.iter().enumerate() {
            let y = py + 50 + i as i32 * 30;
            gfx::fill_circle(frame, (px + 30) as f32, (y + 13) as f32, 7.0, ink);
            gfx::fill_circle(frame, (px + 30) as f32, (y + 13) as f32, 5.5, WHITE);
            if i == sel {
                gfx::fill_circle(frame, (px + 30) as f32, (y + 13) as f32, 3.0, ink);
            }
            self.fonts.draw(frame, f, 14.0, (px + 46) as f32, (y + 18) as f32, ink, label);
        }
        for (label, bx) in [("OK", px + pw - 190), ("Cancel", px + pw - 100)] {
            gfx::fill_round_rect(frame, bx, py + ph - 44, 80, 30, 6, gfx::rgb(200, 200, 210));
            gfx::stroke_rect(frame, bx, py + ph - 44, 80, 30, 1, ink);
            self.fonts.draw_centered(frame, f, 14.0, (bx + 40) as f32, (py + ph - 24) as f32, 76.0, ink, label);
        }
    }

    fn menubar_face(&self) -> usize {
        self.fonts.faces.iter().position(|(n, _)| n == "System").unwrap_or_else(|| self.fonts.ui_face())
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
        self.message("Click on your picture to put the movie there. Press Play to watch it.");
    }

    fn set_color(&mut self, c: u32) {
        self.color = c;
        if self.text.is_some() {
            self.render_text();
        }
    }

    pub fn slideshow_add(&mut self, img: Image) {
        if let Some(ss) = self.slideshow.as_mut() {
            ss.add(img);
        }
    }

    pub fn update(&mut self, dt: f32) {
        self.hover_time += dt;
        if let Some(ss) = self.slideshow.as_mut() {
            ss.update(dt);
        }
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
        if let Some((_, t)) = self.message.as_mut() {
            *t -= dt;
            if *t <= 0.0 {
                self.message = None;
            }
        }
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

        if let Stroke::Media(m) = &mut self.stroke {
            m.idle(&mut self.canvas.img, self.color);
        }
        self.update_spray(dt);
        self.update_hands_free(dt);
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
        if let Some(tr) = self.transition.as_mut() {
            tr.t += dt;
            let k = (tr.t / tr.dur).min(1.0);
            let w = CANVAS_W as i32;
            let h = CANVAS_H as i32;
            let img = &mut self.canvas.img;
            for y in 0..h {
                for x in 0..w {
                    let i = (y * w + x) as usize;
                    let gone = match tr.kind {
                        EraserKind::ClearWipe => (y as f32) < k * h as f32,
                        EraserKind::ClearDissolve => (tr.noise[i] as f32) < k * 256.0,
                        EraserKind::ClearIris => {
                            let dx = (x - w / 2) as f32;
                            let dy = (y - h / 2) as f32;
                            let maxr = ((w * w + h * h) as f32).sqrt() / 2.0;
                            (dx * dx + dy * dy).sqrt() > (1.0 - k) * maxr
                        }
                        _ => false,
                    };
                    img.pixels[i] = if matches!(tr.kind, EraserKind::ClearFade) {
                        gfx::lerp(tr.from.pixels[i], WHITE, (k * 255.0) as u32)
                    } else if gone {
                        WHITE
                    } else {
                        tr.from.pixels[i]
                    };
                }
            }
            if k >= 1.0 {
                self.transition = None;
            }
        }
    }

    pub fn draw(&self, frame: &mut Image) {
        match &self.bg {
            Some(bg) => frame.pixels.copy_from_slice(&bg.pixels),
            None => frame.pixels.fill(gfx::rgb(60, 70, 160)),
        }
        // Picture + objects.
        gfx::blit(frame, &self.canvas.img, CANVAS_X, CANVAS_Y);
        let clip = (CANVAS_X, CANVAS_Y, CANVAS_X + CANVAS_W as i32, CANVAS_Y + CANVAS_H as i32);
        for (i, o) in self.objects.iter().enumerate() {
            let img = o.current(self.playing, self.movie_time);
            gfx::blit_clip(frame, &img, CANVAS_X + o.x, CANVAS_Y + o.y, Some(clip));
            if self.selected == Some(i) {
                o.draw_frame(frame, CANVAS_X, CANVAS_Y, self.handles.as_deref(), self.mouse);
            }
        }
        for sp in &self.sprays {
            let c = sp.current();
            gfx::blit_clip(frame, c, CANVAS_X + sp.x as i32 - c.width as i32 / 2, CANVAS_Y + sp.y as i32 - c.height as i32 / 2, Some(clip));
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
        // Text caret.
        if let Some(t) = &self.text {
            let lines = t.text.split('\n').count().max(1) as f32;
            let last = t.text.split('\n').next_back().unwrap_or("");
            let cx = CANVAS_X + t.x + self.fonts.measure(self.text_font, self.text_size, last) as i32 + 1;
            let base = CANVAS_Y as f32 + t.y as f32 + (lines - 1.0) * self.text_size * 1.15;
            gfx::fill_rect(frame, cx, (base - self.text_size * 0.8) as i32, 2, self.text_size as i32, self.color);
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
            // Current color indicator: tint the splat cel with the selected color.
            let mut tmp = Image::new(frame.width, frame.height);
            p.draw(&mut tmp);
            let (_, r, g, b) = gfx::split(self.dropper_preview.unwrap_or(self.color));
            for (d, s) in frame.pixels.iter_mut().zip(tmp.pixels.iter()) {
                let a = s >> 24;
                if a == 0 {
                    continue;
                }
                let l = paint::luma(*s);
                let shade = 120 + l / 2;
                let c = gfx::rgb(
                    (r * shade / 255).min(255) as u8,
                    (g * shade / 255).min(255) as u8,
                    (b * shade / 255).min(255) as u8,
                );
                let c = if l > 200 { gfx::lerp(c, WHITE, 120) } else { c };
                *d = gfx::blend(*d, (c & 0x00ff_ffff) | a << 24);
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
            Overlay::Palette => {}
            Overlay::Fit(_, _, sel) => self.draw_fit(frame, *sel),
            Overlay::None => {}
        }
        self.picker.draw(frame);
        self.draw_tooltip(frame);
        if let Some(ss) = &self.slideshow {
            ss.draw(frame, &self.fonts);
        }
        if let Some(p) = &self.about {
            p.draw(frame);
        }
        self.menubar.draw(frame, &self.fonts, &self.menu_state);
    }

    fn draw_drawer(&self, frame: &mut Image) {
        let Some(d) = &self.drawer else { return };
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
        // Library name panel (bottom-left white panel of the original art).
        if !self.fonts.is_empty() {
            let f = self.fonts.ui_face();
            let name = d.library_name();
            self.fonts.draw_centered(frame, f, 18.0, (LIB_X + LIB_W / 2) as f32, (LIB_Y + 24) as f32, (LIB_W - 34) as f32, gfx::rgb(30, 30, 120), d.kind.name());
            self.fonts.draw_centered(frame, f, 16.0, (LIB_X + LIB_W / 2) as f32, (LIB_Y + 56) as f32, (LIB_W - 60) as f32, gfx::rgb(0, 0, 0), &name);
        }
        if d.libs.len() > 1 {
            ui::arrow_button(frame, LIB_X + 4, LIB_Y + 40, 22, -1, false, true);
            ui::arrow_button(frame, LIB_X + LIB_W - 28, LIB_Y + 40, 22, 1, false, true);
        }
        ui::close_button(frame, LIB_X + LIB_W - 26, LIB_Y + 2, 20);
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

    fn draw_palette(&self, frame: &mut Image) {
        let cols = ui::palette_colors();
        let rows = cols.len().div_ceil(PAL_COLS) as i32;
        let w = PAL_COLS as i32 * PAL_CELL + 20;
        let h = rows * PAL_CELL + 20;
        ui::panel(frame, PAL_X, PAL_Y, w, h);
        for (i, &c) in cols.iter().enumerate() {
            let x = PAL_X + 10 + (i % PAL_COLS) as i32 * PAL_CELL;
            let y = PAL_Y + 10 + (i / PAL_COLS) as i32 * PAL_CELL;
            gfx::fill_round_rect(frame, x + 1, y + 1, PAL_CELL - 2, PAL_CELL - 2, 5, c);
            if c == self.color {
                gfx::stroke_rect(frame, x, y, PAL_CELL, PAL_CELL, 2, gfx::rgb(0, 0, 0));
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

    fn draw_tooltip(&self, frame: &mut Image) {
        if let Some((m, _)) = &self.message {
            self.wrap_bubble(frame, m);
            return;
        }
    }

    fn wrap_bubble(&self, frame: &mut Image, text: &str) {
        if self.fonts.is_empty() {
            return;
        }
        let f = self.fonts.ui_face();
        let mut lines: Vec<String> = Vec::new();
        let mut cur = String::new();
        for word in text.split_whitespace() {
            let trial = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
            if self.fonts.measure(f, 16.0, &trial) > 380.0 && !cur.is_empty() {
                lines.push(std::mem::take(&mut cur));
                cur = word.to_string();
            } else {
                cur = trial;
            }
        }
        if !cur.is_empty() {
            lines.push(cur);
        }
        let h = lines.len() as i32 * 20 + 16;
        let (x, y) = (210, 54);
        ui::panel(frame, x, y, 404, h);
        for (i, l) in lines.iter().enumerate() {
            self.fonts.draw(frame, f, 16.0, (x + 14) as f32, (y + 24 + i as i32 * 20) as f32, gfx::rgb(20, 20, 60), l);
        }
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
        if self.slideshow.is_some() {
            return CursorHint::Cur(28);
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
    /// Self-test: screen position of tray item `idx` of group `g` (pages the options list).
    pub fn test_tray_pos(&mut self, g: G, idx: usize) -> Option<(i32, i32)> {
        let gi = self.tray()?.groups.iter().position(|x| x.kind == g)?;
        if g == G::Options {
            let per = self.options_per_page();
            if let Some(grp) = self.trays.get_mut(&self.tool).and_then(|t| t.groups.get_mut(gi)) {
                grp.page = idx / per;
            }
        }
        self.tray_layout().into_iter().find(|e| e.0 == gi && e.1 == idx).map(|e| (e.2 + e.4 / 2, e.3 + e.4 / 2))
    }

    pub fn tool_button(&self, cmd: u16) -> Option<(i32, i32)> {
        self.buttons.iter().find(|b| b.cmd == cmd).map(|b| ((b.rect.left as i32 + b.rect.right as i32) / 2, (b.rect.top as i32 + b.rect.bottom as i32) / 2))
    }

    /// Used by the headless self-test: blank page with a colored frame.
    pub fn new_picture_for_test(&mut self) {
        let ss = self.slideshow.take();
        self.new_picture();
        gfx::fill_rect(&mut self.canvas.img, 0, 0, CANVAS_W as i32, CANVAS_H as i32, gfx::rgb(250, 220, 90));
        gfx::fill_circle(&mut self.canvas.img, 300.0, 226.0, 120.0, gfx::rgb(40, 120, 230));
        self.slideshow = ss;
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

fn erase_dab(img: &mut Image, x: f32, y: f32, size: i32, round: bool) {
    if round {
        gfx::fill_circle(img, x, y, size as f32 / 2.0, WHITE);
    } else {
        gfx::fill_rect(img, x as i32 - size / 2, y as i32 - size / 2, size, size, WHITE);
    }
}

fn dashed(frame: &mut Image, a: (f32, f32), b: (f32, f32)) {
    let mut k = 0;
    gfx::walk_line(a.0, a.1, b.0, b.1, 1.0, |x, y| {
        frame.put(x as i32, y as i32, if (k / 4) % 2 == 0 { 0xff00_0000 } else { WHITE });
        k += 1;
    });
}

/// A blob of moving paint (Spray Can, Moving Paint mode) before it flattens.
pub struct SprayAnim {
    set: Rc<SpriteSet>,
    x: f32,
    y: f32,
    t: f32,
    life: f32,
    phase: usize,
}

impl SprayAnim {
    fn current(&self) -> &Image {
        let n = self.set.cels.len().max(1);
        &self.set.cels[(self.phase + (self.t / 90.0) as usize) % n]
    }
}

/// Big erasers (tool ids 123..137) in panel order; names = KPD3.cfg sound sections.
const BIG_ERASERS: [&str; 15] = [
    "Firecracker", "ClockFace", "IrisHeart", "IrisStar", "Windmill", "PageTear", "Spiral", "Implode", "Engulf", "Fade", "Explode",
    "Kaleidoscope", "Waterfall", "WipeRight", "Vanish",
];
/// The QuickTime-effect erasers, as the matching wipe of the engine's transition set.
const BIG_ERASE_FX: [usize; 15] = [0, 1, 7, 9, 23, 14, 20, 5, 2, 4, 3, 11, 22, 27, 0];

/// Mixer Wand brushes (tool ids 87..98).
const WAND_NAMES: [&str; 12] = [
    "SmearBrush", "BlurBrush", "GreyscaleBrush", "ColorizeBrush", "XRayBrush", "EmbossBrush", "EdgeBrush", "NuclearBrush", "SolarizeBrush",
    "HueShiftBrush", "PosterizeBrush", "MosaicBrush",
];

fn wand_effect(i: u8) -> paint::Effect {
    use paint::Effect::*;
    [Blur, Blur, Greyscale, HueShift, Invert, Emboss, Edge, HueShift, Solarize, HueShift, Posterize, Mosaic][i as usize % 12]
}

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
