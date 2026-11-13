//! The easel screen: original UI art and button scripts read from the user's CD, plus
//! the engine's own painting tools.

use crate::anim::Player;
use crate::assets::{self, Assets, SpriteSet};
use crate::audio::{self, Audio};
use crate::gfx::{self, Image};
use crate::library::{LibDrawer, LibKind};
use crate::slideshow::{SlideAction, SlideShow};
use crate::paint::{self, Canvas, MediaStroke, Rng, CANVAS_H, CANVAS_W, CANVAS_X, CANVAS_Y, WHITE};
use crate::tools::{self, EraserKind, FillKind, Opt, PadBank, Stroke, ToolKind, Wacky};
use crate::ui::{self, Fonts};
use mohawk::{Rect, Sound};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

pub const EASEL: &str = "Data/Easel.MHK";

/// Requests from the easel to the application shell.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Save,
    SaveAs,
    Open,
    New,
    Quit,
    SlideAddFile,
}

struct UiButton {
    cmd: u16,
    idle: Option<Player>,
    hover: Option<Player>,
    click: Option<Player>,
    rect: Rect,
    /// Displayed script: 0 = rest (MBTN field 0), 1 = rollover (field 1), 2 = press (field 10).
    showing: u8,
}

impl UiButton {
    fn player(&mut self) -> Option<&mut Player> {
        match self.showing {
            0 => self.idle.as_mut(),
            1 => self.hover.as_mut().or(self.idle.as_mut()),
            _ => self.click.as_mut().or(self.idle.as_mut()),
        }
    }
    fn player_ref(&self) -> Option<&Player> {
        match self.showing {
            0 => self.idle.as_ref(),
            1 => self.hover.as_ref().or(self.idle.as_ref()),
            _ => self.click.as_ref().or(self.idle.as_ref()),
        }
    }
    fn show(&mut self, which: u8, play: bool) {
        self.showing = which;
        if let Some(p) = self.player() {
            if play {
                p.restart();
            } else {
                p.finish_now();
            }
        }
    }
}

pub struct PageObj {
    pub frames: Vec<Image>,
    pub fps: f32,
    pub x: i32,
    pub y: i32,
    pub t: f32,
    /// Flash source (stickers/animations) so resizing re-renders sharply.
    pub source: Option<Rc<[u8]>>,
    pub zoom: f32,
}

impl PageObj {
    fn frame(&self, playing: bool) -> &Image {
        if playing && self.frames.len() > 1 {
            let i = (self.t * self.fps.max(1.0)) as usize % self.frames.len();
            &self.frames[i]
        } else {
            &self.frames[0]
        }
    }
    fn size(&self) -> (i32, i32) {
        (self.frames[0].width as i32, self.frames[0].height as i32)
    }
}

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
    Menu(usize),
}

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
    opt_sel: HashMap<ToolKind, usize>,
    options: Vec<Opt>,
    option_icons: Vec<Image>,
    option_icons_small: Vec<Image>,
    opt_page: usize,
    pub color: u32,
    stroke: Stroke,
    mouse: (i32, i32),
    down: bool,
    overlay: Overlay,
    drawer: Option<LibDrawer>,
    pub objects: Vec<PageObj>,
    drag_obj: Option<(usize, i32, i32)>,
    armed: Option<(Vec<Image>, f32, Option<Rc<[u8]>>)>,
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
const MENU_TITLES: [&str; 4] = ["File", "Edit", "Add", "Help"];
const MENU_X: [i32; 4] = [8, 52, 96, 136];
const SLOT_X0: i32 = 330;
const SLOT_Y0: i32 = 435;
const SLOT_W: i32 = 51;
const SLOT_COUNT: usize = 7;
const FACE_RECT: (i32, i32, i32, i32) = (0, 238, 77, 53);
const COLOR_RECT: (i32, i32, i32, i32) = (0, 491, 85, 109);

fn menu_items(i: usize) -> &'static [&'static str] {
    match i {
        0 => &["New picture", "Open...", "Save", "Save as...", "Quit"],
        1 => &["Undo", "Redo", "Clear picture"],
        2 => &["Background", "Sticker", "Animation", "Sound", "Stamp"],
        _ => &["About Kid Pix Revival"],
    }
}

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
        242 => "Print (saves a picture file)",
        243 => "Play",
        244 => "Stop",
        270 => "Slide Show",
        281 => "Save",
        266 => "Dynamite: erase everything",
        267 => "New picture",
        _ => "",
    }
}

impl Easel {
    pub fn new(assets: Rc<Assets>, audio: Rc<Audio>) -> Easel {
        let fonts = Fonts::load(&assets.font_files());
        let bg = assets.image(EASEL, 1000);
        let mut buttons = Vec::new();
        if let Some(list) = assets
            .archive(EASEL)
            .and_then(|a| a.get(assets::MBTN, 75).map(mohawk::parse_buttons))
            .and_then(|r| r.ok())
        {
            for b in list {
                let mk = |s: u16| if s == 0 { None } else { Player::new(&assets, EASEL, s) };
                let idle = mk(b.idle);
                let rect = idle.as_ref().map(|p| p.bounds()).unwrap_or_default();
                let mut ub = UiButton {
                    cmd: b.command,
                    idle,
                    hover: mk(b.hover),
                    click: mk(b.click),
                    rect,
                    showing: 0,
                };
                ub.show(0, false);
                buttons.push(ub);
            }
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
            opt_sel: HashMap::new(),
            options: Vec::new(),
            option_icons: Vec::new(),
            option_icons_small: Vec::new(),
            opt_page: 0,
            color: gfx::rgb(0, 0, 0),
            stroke: Stroke::None,
            mouse: (0, 0),
            down: false,
            overlay: Overlay::None,
            drawer: None,
            objects: Vec::new(),
            drag_obj: None,
            armed: None,
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
            assets,
            audio,
        };
        e.text_font = e.fonts.ui_face();
        e.select_tool(ToolKind::Pencil, false);
        e
    }

    pub fn message(&mut self, s: impl Into<String>) {
        self.message = Some((s.into(), 3500.0));
    }

    fn play_wav(&self, archive: &str, id: u16, ch: u8) {
        if let Some(s) = self.assets.sound(archive, id) {
            self.audio.play(s, ch, false);
        }
    }

    fn play_script_sounds(&self, archive: &str, base: u16, idx: &[u16]) {
        for &i in idx {
            self.play_wav(archive, base + i, audio::CH_UI);
        }
    }

    // ------------------------------------------------------------------ tools & options

    fn select_tool(&mut self, tool: ToolKind, from_click: bool) {
        self.commit_text();
        self.stroke = Stroke::None;
        self.tool = tool;
        self.options = tools::options(tool, &self.assets, self.fonts.faces.len());
        self.option_icons = self
            .options
            .iter()
            .map(|o| tools::option_icon(o, &self.assets, &self.pads, &self.fonts, OPT_ICON))
            .collect();
        self.option_icons_small = self.option_icons.iter().map(|i| gfx::resize(i, 32, 32)).collect();
        let sel = *self.opt_sel.get(&tool).unwrap_or(&0);
        self.opt_page = sel / self.opt_layout().per_page();
        if tool == ToolKind::Stamp && from_click && self.stamp.is_none() {
            self.open_drawer(LibKind::Stamps);
        } else if tool != ToolKind::Stamp && self.drawer.as_ref().is_some_and(|d| d.kind == LibKind::Stamps) {
            self.drawer = None;
        }
        self.apply_option_state();
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

    fn current_opt(&self) -> Option<&Opt> {
        self.options.get(*self.opt_sel.get(&self.tool).unwrap_or(&0))
    }

    fn apply_option_state(&mut self) {
        match self.current_opt().cloned() {
            Some(Opt::StampSize(n)) => self.stamp_scale = n,
            Some(Opt::TextSize(s)) => self.text_size = s,
            Some(Opt::TextFont(i)) => self.text_font = i,
            _ => {}
        }
    }

    fn choose_option(&mut self, i: usize) {
        if i >= self.options.len() {
            return;
        }
        self.commit_text();
        let opt = self.options[i].clone();
        match opt {
            Opt::Mix(m, _) => {
                self.canvas.checkpoint();
                paint::mix_canvas(&mut self.canvas.img, m, &mut self.rng);
            }
            Opt::Eraser(k, _) if !matches!(k, EraserKind::Square(_)) => {
                self.start_transition(k);
                return;
            }
            Opt::StampLibrary => {
                self.open_drawer(LibKind::Stamps);
                return;
            }
            _ => {}
        }
        self.opt_sel.insert(self.tool, i);
        self.apply_option_state();
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
            242 => self.actions.push(Action::SaveAs),
            267 => self.actions.push(Action::New),
            243 => {
                self.playing = true;
                if let Some(s) = &self.page_sound {
                    self.audio.play(s.clone(), audio::CH_MUSIC, false);
                }
            }
            244 => {
                self.playing = false;
                self.audio.stop_channel(audio::CH_MUSIC);
            }
            266 => {
                // Dynamite: the click script plays the explosion; clear when it ends.
                self.dynamite_clear = true;
            }
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
            gfx::blit(&mut img, o.frame(false), o.x, o.y);
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
        let o = &mut self.objects[i];
        let new_zoom = (o.zoom * 1.12f32.powf(steps)).clamp(0.2, 5.0);
        let (ow, oh) = o.size();
        let (cx, cy) = (o.x + ow / 2, o.y + oh / 2);
        let frames = match &o.source {
            Some(src) => crate::stickers::render(src, o.frames.len() > 1, new_zoom).map(|r| r.0),
            None => {
                let f = new_zoom / o.zoom;
                Some(o.frames.iter().map(|im| gfx::resize(im, ((im.width as f32 * f) as u32).max(4), ((im.height as f32 * f) as u32).max(4))).collect())
            }
        };
        if let Some(frames) = frames {
            o.frames = frames;
            o.zoom = new_zoom;
            let (nw, nh) = o.size();
            o.x = cx - nw / 2;
            o.y = cy - nh / 2;
            self.canvas.dirty = true;
        }
    }

    /// Delete key: remove the sticker under the mouse.
    pub fn delete_under_mouse(&mut self) {
        if self.text.is_some() {
            return;
        }
        if let Some(i) = self.object_at(self.mouse.0, self.mouse.1) {
            self.objects.remove(i);
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
            ui::hit(cx, cy, o.x, o.y, w, h) && o.frame(false).get(cx - o.x, cy - o.y).is_some_and(|p| p >> 24 > 16)
        })
    }

    pub fn mouse_move(&mut self, x: i32, y: i32) {
        let prev = self.mouse;
        self.mouse = (x, y);
        if let Some(ss) = self.slideshow.as_mut() {
            ss.mouse_move(x, y);
            return;
        }
        if let Overlay::Menu(_) = self.overlay {
            if y < 18 {
                if let Some(i) = MENU_X.iter().rposition(|&mx| x >= mx - 4) {
                    self.overlay = Overlay::Menu(i);
                }
            }
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

    fn button_sounds(&mut self, i: usize) -> Vec<u16> {
        // Sound ops fire at the start of in/out scripts; collect them by a zero-length update.
        self.buttons[i].player().map(|p| p.update(0.0)).unwrap_or_default()
    }

    fn is_selected(&self, i: usize) -> bool {
        ToolKind::from_command(self.buttons[i].cmd) == Some(self.tool)
    }

    pub fn mouse_down(&mut self, x: i32, y: i32) {
        self.mouse = (x, y);
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
        // Menus.
        if let Overlay::Menu(m) = self.overlay {
            self.overlay = Overlay::None;
            if let Some(item) = self.menu_item_at(m, x, y) {
                self.menu_command(m, item);
            }
            self.down = false;
            return;
        }
        if y < 18 {
            if let Some(i) = MENU_X.iter().rposition(|&mx| x >= mx - 4) {
                if x < 190 {
                    self.overlay = Overlay::Menu(i);
                    self.down = false;
                    return;
                }
            }
        }
        if let Overlay::Palette = self.overlay {
            self.overlay = Overlay::None;
            if let Some(c) = self.palette_at(x, y) {
                self.color = c;
                if self.text.is_some() {
                    self.render_text();
                }
            }
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
        // Option panel.
        if ui::hit(x, y, OPT_X - 4, OPT_Y - 6, 400, 64) {
            self.options_click(x, y);
            self.down = false;
            return;
        }
        // Color picker.
        if ui::hit(x, y, COLOR_RECT.0, COLOR_RECT.1, COLOR_RECT.2, COLOR_RECT.3) {
            self.overlay = Overlay::Palette;
            self.down = false;
            return;
        }
        // Undo face.
        if ui::hit(x, y, FACE_RECT.0, FACE_RECT.1, FACE_RECT.2, FACE_RECT.3) {
            self.undo();
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

    pub fn mouse_up(&mut self, _x: i32, _y: i32) {
        self.down = false;
        self.drag_obj = None;
        match std::mem::replace(&mut self.stroke, Stroke::None) {
            Stroke::MoverSelect { start, cur, copy } => {
                let (x0, y0) = (start.0.min(cur.0), start.1.min(cur.1));
                let (x1, y1) = (start.0.max(cur.0), start.1.max(cur.1));
                if x1 - x0 > 2 && y1 - y0 > 2 {
                    let piece = gfx::crop(&self.canvas.img, x0, y0, x1 - x0, y1 - y0);
                    self.canvas.checkpoint();
                    if !copy {
                        gfx::fill_rect(&mut self.canvas.img, x0, y0, x1 - x0, y1 - y0, WHITE);
                    }
                    // The lifted piece becomes a movable object until the next action.
                    self.objects.push(PageObj {
                        frames: vec![piece],
                        fps: 1.0,
                        x: x0,
                        y: y0,
                        t: 0.0,
                        source: None,
                        zoom: 1.0,
                    });
                    self.message("Drag the piece where you want it.");
                }
            }
            other => {
                if !matches!(other, Stroke::None) {
                    self.canvas.dirty = true;
                }
            }
        }
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
                self.objects.push(PageObj { frames, fps, x: cx - w / 2, y: cy - h / 2, t: 0.0, source, zoom: 1.0 });
                self.canvas.dirty = true;
            }
            self.down = false;
            return;
        }
        // Dragging objects: Moving Van (any option) or eraser click removes.
        if let Some(i) = self.object_at(x, y) {
            match self.tool {
                ToolKind::Mover => {
                    let o = &self.objects[i];
                    self.drag_obj = Some((i, cx - o.x, cy - o.y));
                    return;
                }
                ToolKind::Eraser => {
                    self.objects.remove(i);
                    self.down = false;
                    return;
                }
                _ => {}
            }
        }
        let opt = self.current_opt().cloned();
        match self.tool {
            ToolKind::Pencil => match opt {
                Some(Opt::Media(sec, _)) => {
                    self.canvas.checkpoint();
                    let params = tools::media_params(&self.assets, sec);
                    let pad = tools::pad_for(&self.pads, &params, 6.0);
                    let mut st = MediaStroke::new(params, pad, self.rng.next_u32() as u64);
                    st.to(&mut self.canvas.img, fx, fy, self.color);
                    self.stroke = Stroke::Media(st);
                    let snd = self.felt_sound();
                    self.play_wav("Data/Tools/FeltPenSnds.MHK", snd, audio::CH_TOOL);
                }
                _ => {
                    let width = match opt {
                        Some(Opt::PencilSize(w)) => w,
                        _ => 3.0,
                    };
                    self.canvas.checkpoint();
                    paint::line(&mut self.canvas.img, fx, fy, fx, fy, width, self.color);
                    self.stroke = Stroke::Pencil { last: (fx, fy), width };
                }
            },
            ToolKind::Brush => {
                let kind = match opt {
                    Some(Opt::Wacky(k, _)) => k,
                    _ => Wacky::Rainbow,
                };
                self.canvas.checkpoint();
                let media = match kind {
                    Wacky::Custom(n) => {
                        let p = tools::media_params(&self.assets, &format!("CustomBrush{n}"));
                        let pad = tools::pad_for(&self.pads, &p, 8.0);
                        Some(MediaStroke::new(p, pad, self.rng.next_u32() as u64))
                    }
                    Wacky::Drippy => {
                        let p = tools::media_params(&self.assets, "Drippy");
                        let pad = tools::pad_for(&self.pads, &p, 8.0);
                        Some(MediaStroke::new(p, pad, self.rng.next_u32() as u64))
                    }
                    _ => None,
                };
                if let Some(key) = tools::trail_source(kind) {
                    if !self.trails.contains_key(&key) {
                        if let Some(s) = self.assets.sprites(key.0, key.1) {
                            self.trails.insert(key, s);
                        }
                    }
                }
                let mut st = Stroke::Wacky {
                    kind,
                    last: (fx, fy),
                    dist: 1000.0,
                    rng: Rng::new(self.rng.next_u32() as u64),
                    hue: 0.0,
                    cel: 0,
                    media,
                };
                self.wacky_apply(&mut st, (fx, fy));
                self.stroke = st;
                self.play_brush_sound();
            }
            ToolKind::Fill => {
                let kind = match opt {
                    Some(Opt::Fill(k, _)) => k,
                    _ => FillKind::Solid,
                };
                let style = tools::fill_style(kind, &self.assets);
                let before = self.canvas.img.clone();
                if paint::fill_with(&mut self.canvas.img, cx, cy, &style, self.color) {
                    let after = std::mem::replace(&mut self.canvas.img, before);
                    self.canvas.replace(after);
                }
                self.down = false;
            }
            ToolKind::Mixer => {
                // Clicking the picture re-applies the selected mix.
                if let Some(Opt::Mix(m, _)) = opt {
                    self.canvas.checkpoint();
                    paint::mix_canvas(&mut self.canvas.img, m, &mut self.rng);
                }
                self.down = false;
            }
            ToolKind::Stamp => {
                match &self.stamp {
                    Some(st) => {
                        let s = self.stamp_scale.max(1);
                        let img = if s == 1 { st.clone() } else { scale_nearest(st, s) };
                        self.canvas.checkpoint();
                        gfx::blit(&mut self.canvas.img, &img, cx - img.width as i32 / 2, cy - img.height as i32 / 2);
                        self.play_wav(EASEL, 1013, audio::CH_TOOL);
                    }
                    None => self.open_drawer(LibKind::Stamps),
                }
                self.down = false;
            }
            ToolKind::Text if matches!(opt, Some(Opt::Letter(_))) => {
                let Some(Opt::Letter(id)) = opt else { unreachable!() };
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
                self.text = Some(TextEntry {
                    x: cx,
                    y: cy + (self.text_size * 0.4) as i32,
                    base: self.canvas.img.clone(),
                    text: String::new(),
                });
                self.down = false;
            }
            ToolKind::Mover => {
                let copy = matches!(opt, Some(Opt::MoverCopy));
                if !matches!(opt, Some(Opt::MoverObjects)) {
                    self.stroke = Stroke::MoverSelect { start: (cx, cy), cur: (cx, cy), copy };
                }
            }
            ToolKind::Eraser => {
                let size = match opt {
                    Some(Opt::Eraser(EraserKind::Square(n), _)) => n,
                    _ => 16,
                };
                self.canvas.checkpoint();
                gfx::fill_rect(&mut self.canvas.img, cx - size / 2, cy - size / 2, size, size, WHITE);
                self.stroke = Stroke::Eraser { last: (fx, fy), size };
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
        let sel = *self.opt_sel.get(&ToolKind::Brush).unwrap_or(&0);
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
            Stroke::None => {}
            Stroke::Pencil { last, width } => {
                paint::line(&mut self.canvas.img, last.0, last.1, fx, fy, *width, self.color);
                *last = (fx, fy);
            }
            Stroke::Media(m) => m.to(&mut self.canvas.img, fx, fy, self.color),
            Stroke::Wacky { .. } => {
                self.wacky_apply(&mut st, (fx, fy));
                self.play_brush_sound();
            }
            Stroke::Eraser { last, size } => {
                let s = *size;
                let img = &mut self.canvas.img;
                gfx::walk_line(last.0, last.1, fx, fy, 1.0, |px, py| {
                    gfx::fill_rect(img, px as i32 - s / 2, py as i32 - s / 2, s, s, WHITE)
                });
                *last = (fx, fy);
            }
            Stroke::MoverSelect { cur, .. } => {
                *cur = (cx.clamp(0, CANVAS_W as i32), cy.clamp(0, CANVAS_H as i32));
            }
        }
        self.stroke = st;
    }

    fn opt_layout(&self) -> OptLayout {
        if self.options.len() <= OPT_PER_PAGE {
            OptLayout { cols: OPT_PER_PAGE, rows: 1, cell: OPT_SLOT, icon: OPT_ICON as i32, y0: OPT_Y }
        } else {
            OptLayout { cols: 11, rows: 2, cell: 34, icon: 32, y0: OPT_Y - 6 }
        }
    }

    /// Option index under (x, y), if any.
    fn option_at(&self, x: i32, y: i32) -> Option<usize> {
        let l = self.opt_layout();
        if x < OPT_X || y < l.y0 {
            return None;
        }
        let (c, r) = (((x - OPT_X) / l.cell) as usize, ((y - l.y0) / l.cell) as usize);
        if c >= l.cols || r >= l.rows {
            return None;
        }
        let i = self.opt_page * l.per_page() + r * l.cols + c;
        (i < self.options.len()).then_some(i)
    }

    fn options_click(&mut self, x: i32, y: i32) {
        let l = self.opt_layout();
        let pages = self.options.len().div_ceil(l.per_page()).max(1);
        let arrow_x = OPT_X + l.cols as i32 * l.cell + 4;
        if pages > 1 && x >= arrow_x {
            if y < OPT_Y + 24 {
                self.opt_page = (self.opt_page + pages - 1) % pages;
            } else {
                self.opt_page = (self.opt_page + 1) % pages;
            }
            return;
        }
        if let Some(i) = self.option_at(x, y) {
            self.choose_option(i);
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

    fn menu_item_at(&self, m: usize, x: i32, y: i32) -> Option<usize> {
        let items = menu_items(m);
        let mx = MENU_X[m] - 4;
        if x < mx || x > mx + 170 || y < 18 {
            return None;
        }
        let i = ((y - 20) / 20) as usize;
        (i < items.len()).then_some(i)
    }

    fn menu_command(&mut self, m: usize, item: usize) {
        match (m, item) {
            (0, 0) => self.actions.push(Action::New),
            (0, 1) => self.actions.push(Action::Open),
            (0, 2) => self.actions.push(Action::Save),
            (0, 3) => self.actions.push(Action::SaveAs),
            (0, 4) => self.actions.push(Action::Quit),
            (1, 0) => self.undo(),
            (1, 1) => self.redo(),
            (1, 2) => self.start_transition(EraserKind::ClearFade),
            (2, 0) => self.open_drawer(LibKind::Backgrounds),
            (2, 1) => self.open_drawer(LibKind::Stickers),
            (2, 2) => self.open_drawer(LibKind::Toons),
            (2, 3) => self.open_drawer(LibKind::Sounds),
            (2, 4) => self.select_tool(ToolKind::Stamp, true),
            _ => self.message(
                "Kid Pix Revival: a new cross-platform engine that runs the art and sounds \
                 from your own Kid Pix Deluxe 3 disc.",
            ),
        }
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
        for o in self.objects.iter_mut() {
            o.t += dt / 1000.0;
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
        for o in &self.objects {
            gfx::blit_clip(frame, o.frame(true), CANVAS_X + o.x, CANVAS_Y + o.y, Some(clip));
        }
        // Selection marquee.
        if let Stroke::MoverSelect { start, cur, .. } = &self.stroke {
            let (x0, y0) = (start.0.min(cur.0), start.1.min(cur.1));
            let (w, h) = ((start.0 - cur.0).abs(), (start.1 - cur.1).abs());
            marquee(frame, CANVAS_X + x0, CANVAS_Y + y0, w, h);
        }
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
            let (_, r, g, b) = gfx::split(self.color);
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

        self.draw_options(frame);
        self.draw_drawer(frame);
        self.draw_menu_bar(frame);

        match &self.overlay {
            Overlay::Palette => self.draw_palette(frame),
            Overlay::Menu(m) => self.draw_menu(frame, *m),
            Overlay::None => {}
        }
        self.draw_tooltip(frame);
        if let Some(ss) = &self.slideshow {
            ss.draw(frame, &self.fonts);
        }
    }

    fn draw_options(&self, frame: &mut Image) {
        let l = self.opt_layout();
        let sel = *self.opt_sel.get(&self.tool).unwrap_or(&0);
        let start = self.opt_page * l.per_page();
        let icons = if l.rows > 1 { &self.option_icons_small } else { &self.option_icons };
        for (slot, i) in (start..(start + l.per_page()).min(self.options.len())).enumerate() {
            let x = OPT_X + (slot % l.cols) as i32 * l.cell;
            let y = l.y0 + (slot / l.cols) as i32 * l.cell;
            let one_shot = matches!(
                self.options[i],
                Opt::Mix(..) | Opt::StampLibrary | Opt::Eraser(EraserKind::ClearFade | EraserKind::ClearWipe | EraserKind::ClearDissolve | EraserKind::ClearIris, _)
            );
            let selected = i == sel && !one_shot;
            let hover = self.option_at(self.mouse.0, self.mouse.1) == Some(i);
            let border = if selected {
                gfx::rgb(230, 40, 40)
            } else if hover {
                gfx::rgb(40, 180, 60)
            } else {
                gfx::rgb(120, 130, 210)
            };
            gfx::fill_round_rect(frame, x - 2, y - 2, l.icon + 4, l.icon + 4, if l.rows > 1 { 5 } else { 8 }, border);
            if let Some(icon) = icons.get(i) {
                gfx::blit(frame, icon, x, y);
            }
        }
        let pages = self.options.len().div_ceil(l.per_page());
        if pages > 1 {
            let ax = OPT_X + l.cols as i32 * l.cell + 4;
            ui::arrow_button(frame, ax, OPT_Y - 2, 22, -1, true, true);
            ui::arrow_button(frame, ax, OPT_Y + 28, 22, 1, true, true);
        }
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

    fn draw_menu_bar(&self, frame: &mut Image) {
        if self.fonts.is_empty() {
            return;
        }
        let f = self.fonts.ui_face();
        for (i, t) in MENU_TITLES.iter().enumerate() {
            let open = matches!(self.overlay, Overlay::Menu(m) if m == i);
            if open {
                gfx::fill_rect(frame, MENU_X[i] - 4, 0, self.fonts.measure(f, 14.0, t) as i32 + 8, 18, gfx::rgb(70, 90, 220));
            }
            self.fonts.draw(frame, f, 14.0, MENU_X[i] as f32, 14.0, gfx::rgb(255, 255, 255), t);
        }
    }

    fn draw_menu(&self, frame: &mut Image, m: usize) {
        let items = menu_items(m);
        let x = MENU_X[m] - 4;
        let h = items.len() as i32 * 20 + 6;
        gfx::fill_rect(frame, x + 3, 21, 170, h, gfx::argb(80, 0, 0, 0));
        gfx::fill_rect(frame, x, 18, 170, h, gfx::rgb(250, 250, 255));
        gfx::stroke_rect(frame, x, 18, 170, h, 1, gfx::rgb(40, 50, 170));
        if self.fonts.is_empty() {
            return;
        }
        let f = self.fonts.ui_face();
        for (i, t) in items.iter().enumerate() {
            let y = 20 + i as i32 * 20;
            if ui::hit(self.mouse.0, self.mouse.1, x, y, 170, 20) {
                gfx::fill_rect(frame, x + 1, y, 168, 20, gfx::rgb(200, 210, 255));
            }
            self.fonts.draw(frame, f, 14.0, (x + 8) as f32, (y + 15) as f32, gfx::rgb(0, 0, 0), t);
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
        if self.hover_time < 700.0 {
            return;
        }
        let (mx, my) = self.mouse;
        let label = if let Some(i) = self.hover_btn {
            command_name(self.buttons[i].cmd).to_string()
        } else if let Some(i) = self.option_at(mx, my) {
            self.options.get(i).map(|o| o.label()).unwrap_or_default()
        } else if ui::hit(mx, my, COLOR_RECT.0, COLOR_RECT.1, COLOR_RECT.2, COLOR_RECT.3) {
            "Colors".into()
        } else if ui::hit(mx, my, FACE_RECT.0, FACE_RECT.1, FACE_RECT.2, FACE_RECT.3) {
            "Undo".into()
        } else {
            String::new()
        };
        if !label.is_empty() {
            self.bubble(frame, mx, (my - 34).max(20), &label);
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
        let opt = self.current_opt();
        CursorHint::Cur(match self.tool {
            ToolKind::Pencil => match opt {
                Some(Opt::Media("Marker", _)) => 2,
                Some(Opt::Media("Airbrush", _)) => 6,
                Some(Opt::Media(..)) => 0,
                _ => 1,
            },
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

struct OptLayout {
    cols: usize,
    rows: usize,
    cell: i32,
    icon: i32,
    y0: i32,
}

impl OptLayout {
    fn per_page(&self) -> usize {
        self.cols * self.rows
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorHint {
    /// Cursor index into `Data/General.MHK` (tCUR id 10000 + n).
    Cur(u16),
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
    /// Used by the headless self-test: blank page with a colored frame.
    pub fn new_picture_for_test(&mut self) {
        let ss = self.slideshow.take();
        self.new_picture();
        gfx::fill_rect(&mut self.canvas.img, 0, 0, CANVAS_W as i32, CANVAS_H as i32, gfx::rgb(250, 220, 90));
        gfx::fill_circle(&mut self.canvas.img, 300.0, 226.0, 120.0, gfx::rgb(40, 120, 230));
        self.slideshow = ss;
    }
}
