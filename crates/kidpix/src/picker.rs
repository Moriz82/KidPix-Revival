//! The SlideShow organizer — "Picker" / "Project Zone" (`RPicker`, docs/RE_SHELL.md §9.1):
//! a 96-slot slide show shown 12 slots at a time, slide drag & drop, the trash, the saved
//! slide-show strip, the effects view with its three slide dialogs (§9.3, §9.4), the
//! background colour dialog (§9.6), `.kpp` open / save (§9.2) and the idle characters.

use crate::anim::Player;
use crate::assets::{Assets, SpriteSet};
use crate::audio::{self, Audio};
use crate::dialogs::{self, Dialog, Key};
use crate::easel::CursorHint;
use crate::gfx::{self, Image};
use crate::menubar::{cmd, MenuBar, MenuState};
use crate::slideshow::{self, Project, SlideRef, SlideSettings, TRANS_SOUNDS};
use crate::ui::Fonts;
use mohawk::Rect;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

pub const PICKER: &str = "Data/Picker.MHK";
pub const PICKER_HELP: &str = "Data/PickerHelp.MHK";
const TRANS_DLG: &str = "Data/TransSound.MHK";
const TIMER_DLG: &str = "Data/TimerSettings.MHK";
const AUDIO_DLG: &str = "Data/SlideShow.MHK";
const BGCOLOR_DLG: &str = "Data/RPickBgColorDlg.MHK";
const DELETERS: &str = "Data/Deleters.MHK";
const WARN: &str = "Data/Warnings.MHK";

/// MBTN 888 commands.
const GO_EASEL: u16 = 5567;
const TRASH: u16 = 1363;
const NEW: u16 = 1365;
const PLAY: u16 = 1370;
const EFFECTS: u16 = 1371;
const PRINT: u16 = 1372;
const SAVE: u16 = 1373;
const HELP: u16 = 1374;
/// Engine ids of the code-made controls.
const C_UP: u16 = 0xf001;
const C_DOWN: u16 = 0xf002;
const C_LEFT: u16 = 0xf003;
const C_RIGHT: u16 = 0xf004;
const C_LOAD: u16 = 0xf100;
const C_TRANS: u16 = 0xf200;
const C_TIMING: u16 = 0xf300;
const C_SOUND: u16 = 0xf400;
const C_SHOW: u16 = 0xf500;

const SLOTS: usize = 12;
const SLOT_X: [i32; 4] = [80, 195, 310, 425];
const SLOT_Y: [i32; 3] = [142, 242, 342];
const STRIP: usize = 4;
const TEXT: u32 = 0xfffa_faff;

/// Requests to the shell.
#[derive(Debug, Clone, PartialEq)]
pub enum PickerAction {
    GoEasel,
    /// Play the show (start index into the filled slides; None = from the beginning).
    Play(Option<usize>),
    Help,
    Login,
    Quit,
    UserGuide,
}

/// Art of one control state.
enum Art {
    None,
    Script(Player),
    /// A Picker cel drawn at a screen position.
    Cel(u16, i32, i32),
    /// Several cels (a script frame with some cels left out).
    Cels(Vec<(u16, i32, i32)>),
}

impl Art {
    fn player(&mut self) -> Option<&mut Player> {
        match self {
            Art::Script(p) => Some(p),
            _ => None,
        }
    }
}

struct Ctl {
    id: u16,
    rect: Rect,
    /// rest, rollover, press, disabled.
    art: [Art; 4],
    state: u8,
}

impl Ctl {
    fn new(id: u16, rect: Rect, rest: Art, over: Art, press: Art, disabled: Art) -> Ctl {
        let mut c = Ctl { id, rect, art: [rest, over, press, disabled], state: 0 };
        c.show(0, false);
        c
    }

    /// Switch state; returns the sound indices of the state's first frame when `play`.
    fn show(&mut self, s: u8, play: bool) -> Vec<u16> {
        self.state = s;
        let Some(p) = self.art[s as usize].player() else { return vec![] };
        if play {
            p.restart();
            p.update(0.0)
        } else {
            p.finish_now();
            vec![]
        }
    }

    fn update(&mut self, dt: f32) -> Vec<u16> {
        self.art[self.state as usize].player().map(|p| p.update(dt)).unwrap_or_default()
    }
}

struct Drag {
    kind: DragKind,
    start: (i32, i32),
    active: bool,
}

#[derive(Clone)]
enum DragKind {
    Slide(usize),
    Show(PathBuf),
}

enum Follow {
    New,
    Open(PathBuf),
    OpenDialog,
    GoEasel,
    Quit,
    Login,
}

enum Kind {
    Trans { slot: usize, tr: usize, snd: usize, ttop: usize, stop: usize, hover: Option<(bool, usize)> },
    Timer { slot: usize },
    Audio { slot: usize },
    BgColor { sel: usize, chip: Rect },
    DeleteSlide(usize),
    DeleteShow(PathBuf),
    AskSave(Follow),
    /// Small Kids Mode: "Give your SlideShow a name" (EnterName.MHK), then continue.
    NameShow(Option<Follow>),
    Alert,
}

struct Modal {
    dlg: Dialog,
    kind: Kind,
    /// Extra controls drawn by code (dialog scroll arrows).
    ctls: Vec<Ctl>,
    /// Name-field rects (transition / sound) for the transition dialog.
    fields: Vec<Rect>,
}

struct Preview {
    from: Image,
    to: Image,
    kind: usize,
    t: f32,
    mix: Image,
    bg: u32,
}

pub struct Picker {
    assets: Rc<Assets>,
    audio: Rc<Audio>,
    sprites: Option<Rc<SpriteSet>>,
    bg: Option<Rc<Image>>,
    ctls: Vec<Ctl>,
    hover: Option<usize>,
    pressed: Option<usize>,
    statics: Vec<Player>,
    held: HashMap<u16, Player>,
    effects_on: Option<Player>,
    chars_rest: Vec<Player>,
    idle: Option<(usize, Player)>,
    idle_wait: f32,
    lights: Option<Player>,
    trash_anim: Option<Player>,
    strip_anim: Option<(Player, i32)>,
    pub project: Project,
    thumbs: HashMap<PathBuf, Option<Rc<Image>>>,
    pub effects: bool,
    drag: Option<Drag>,
    drop_slot: Option<usize>,
    over_trash: bool,
    shows: Vec<PathBuf>,
    pub strip_pos: usize,
    modal: Option<Modal>,
    preview: Option<Preview>,
    menubar: MenuBar,
    menu_press: bool,
    about: Option<Player>,
    pub actions: Vec<PickerAction>,
    clipboard: Option<SlideRef>,
    rng: u32,
    pub user_pictures: Option<PathBuf>,
    pub projects_dir: Option<PathBuf>,
    trans_names: Vec<String>,
    sound_names: Vec<String>,
    timer_names: Vec<String>,
    untitled: String,
    max: usize,
    mouse: (i32, i32),
    scroll_wait: f32,
    /// Small Kids Mode: shows are named with the graphical EnterName dialog.
    pub small_kids: bool,
}

fn held(assets: &Assets, arch: &str, id: u16) -> Option<Player> {
    Player::new(assets, arch, id).map(|mut p| {
        p.finish_now();
        p
    })
}

fn script(assets: &Assets, id: u16) -> Art {
    Player::new(assets, PICKER, id).map(Art::Script).unwrap_or(Art::None)
}

fn bounds(assets: &Assets, arch: &str, id: u16) -> Rect {
    assets.script(arch, id).map(|s| s.bounds).unwrap_or_default()
}

/// Screen position of the first cel of a script's first frame.
fn first_cel_pos(assets: &Assets, arch: &str, id: u16) -> Option<(u16, i32, i32)> {
    Player::new(assets, arch, id).and_then(|p| p.placed().first().copied())
}

fn slot_pos(i: usize) -> (i32, i32) {
    (SLOT_X[i % 4], SLOT_Y[i / 4])
}

fn hash(mut x: u32) -> u32 {
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    x
}

impl Picker {
    pub fn new(assets: Rc<Assets>, audio: Rc<Audio>, fonts: &Fonts) -> Picker {
        let a = &*assets;
        let sprites = a.sprites(PICKER, 5000);
        let mut ctls = Vec::new();
        // MBTN 888 buttons (the kind-5 slot hotspots are handled by the slot grid).
        for b in crate::buttons::load(a, PICKER, 888) {
            if b.idle.is_none() {
                continue;
            }
            let take = |p: Option<Player>| p.map(Art::Script).unwrap_or(Art::None);
            let rect = b.rect;
            ctls.push(Ctl::new(b.cmd, rect, take(b.idle), take(b.hover), take(b.click), Art::None));
        }
        let ctl = |id: u16, rest: u16, over: u16, press: u16, dis: u16| {
            let r = bounds(a, PICKER, rest);
            let opt = |s: u16| if s == 0 { Art::None } else { script(a, s) };
            Ctl::new(id, r, opt(rest), opt(over), opt(press), opt(dis))
        };
        ctls.push(ctl(C_UP, 5060, 5061, 5062, 5067));
        ctls.push(ctl(C_DOWN, 5063, 5064, 5065, 5066));
        ctls.push(ctl(C_LEFT, 5217, 5218, 5219, 0));
        ctls.push(ctl(C_RIGHT, 5220, 5221, 5222, 0));
        for k in 0..STRIP as u16 {
            ctls.push(ctl(C_SHOW + k, 5009 + 3 * k, 5010 + 3 * k, 5011 + 3 * k, 0));
        }
        for i in 0..SLOTS as u16 {
            // Normal view: the Load button under each slot.
            ctls.push(ctl(C_LOAD + i, 5193 + i, 5181 + i, 5205 + i, 0));
            // Effects view: transition arrow (rest = red arrow + icon well; 5144 also lists
            // the green "selected" outline, which is drawn for the selected slot instead).
            let rest = Player::new(a, PICKER, 5144 + i)
                .map(|p| Art::Cels(p.placed().into_iter().filter(|c| c.0 != 107).collect()))
                .unwrap_or(Art::None);
            ctls.push(Ctl::new(C_TRANS + i, bounds(a, PICKER, 5144 + i), rest, script(a, 5156 + i), script(a, 5168 + i), Art::None));
            // Timing label (rest cel 12 where the rollover script shows cel 13) and the
            // audio-options speaker (rest cel 15, rollover cel 16).
            let lab = first_cel_pos(a, PICKER, 5106 + i).map(|(_, x, y)| Art::Cel(12, x, y)).unwrap_or(Art::None);
            ctls.push(Ctl::new(C_TIMING + i, bounds(a, PICKER, 5106 + i), lab, script(a, 5106 + i), Art::None, Art::None));
            let spk = first_cel_pos(a, PICKER, 5119 + i).map(|(_, x, y)| Art::Cel(15, x, y)).unwrap_or(Art::None);
            ctls.push(Ctl::new(C_SOUND + i, bounds(a, PICKER, 5119 + i), spk, script(a, 5119 + i), Art::None, Art::None));
        }
        let statics = [5225u16, 5227].iter().filter_map(|&i| held(a, PICKER, i)).collect();
        let mut hmap = HashMap::new();
        for id in (5068..=5103).chain(5021..=5024).chain(5132..=5143).chain([5229, 5230]) {
            if let Some(p) = held(a, PICKER, id) {
                hmap.insert(id, p);
            }
        }
        let chars_rest = [5244u16, 5248, 5257].iter().filter_map(|&i| held(a, PICKER, i)).collect();
        let max = a.cfg.get_i32("PortfolioSettings", "MaxProjectSize").map(|v| v.clamp(12, 999) as usize).unwrap_or(96);
        let menu_face = crate::ui::UI_BITMAP;
        let untitled = a.cfg.get("Paths2", "UntitledProjName").unwrap_or("KP1").to_string();
        let seed = if crate::selftest::TEST_MODE.load(std::sync::atomic::Ordering::Relaxed) {
            12345
        } else {
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(7) | 1
        };
        let mut p = Picker {
            sprites,
            bg: a.image(PICKER, 5000),
            ctls,
            hover: None,
            pressed: None,
            statics,
            held: hmap,
            effects_on: held(a, PICKER, 5032),
            chars_rest,
            idle: None,
            idle_wait: 4000.0,
            lights: None,
            trash_anim: None,
            strip_anim: None,
            project: Project::new(max),
            thumbs: HashMap::new(),
            effects: false,
            drag: None,
            drop_slot: None,
            over_trash: false,
            shows: Vec::new(),
            strip_pos: 0,
            modal: None,
            preview: None,
            menubar: MenuBar::new(fonts, menu_face),
            menu_press: false,
            about: None,
            actions: Vec::new(),
            clipboard: None,
            rng: seed,
            user_pictures: None,
            projects_dir: None,
            trans_names: slideshow::strl(a, TRANS_SOUNDS, 7500),
            sound_names: slideshow::strl(a, TRANS_SOUNDS, 8500),
            timer_names: slideshow::strl(a, TIMER_DLG, 7000),
            untitled,
            max,
            mouse: (0, 0),
            scroll_wait: 0.0,
            small_kids: false,
            assets,
            audio,
        };
        p.refresh_controls();
        p
    }

    /// Called whenever the Picker screen is shown.
    pub fn enter(&mut self, pictures: Option<PathBuf>, projects: Option<PathBuf>) {
        if self.projects_dir != projects {
            self.project = Project::new(self.max);
            self.thumbs.clear();
        }
        self.user_pictures = pictures;
        self.projects_dir = projects;
        if let Some(d) = &self.projects_dir {
            let _ = std::fs::create_dir_all(d);
        }
        self.refresh_shows();
        self.menubar.close();
        self.menu_press = false;
        self.drag = None;
        self.lights = Player::new(&self.assets, PICKER, 5223);
        self.idle_wait = 3000.0;
        self.refresh_controls();
    }

    /// Open a slide show (the last one at start-up, a strip icon, File > Open).
    pub fn open(&mut self, path: &Path) -> Result<(), String> {
        let p = Project::load(path, self.max)?;
        self.project = p;
        self.refresh_controls();
        Ok(())
    }

    fn rand(&mut self, n: u32) -> u32 {
        self.rng = hash(self.rng);
        self.rng % n.max(1)
    }

    fn play_sounds(&self, arch: &str, base: u16, idx: Vec<u16>) {
        for k in idx {
            if let Some(s) = self.assets.sound(arch, base + k) {
                self.audio.play(s, audio::CH_UI, false);
            }
        }
    }

    // ------------------------------------------------------------------ model helpers

    fn thumb(&mut self, path: &Path) -> Option<Rc<Image>> {
        self.thumbs.entry(path.to_path_buf()).or_insert_with(|| slideshow::page_thumb(path).map(Rc::new)).clone()
    }

    fn thumb_cached(&self, path: &Path) -> Option<Rc<Image>> {
        self.thumbs.get(path).cloned().flatten()
    }

    fn visible_slot(&self, slot: usize) -> Option<usize> {
        (slot >= self.project.top_left && slot < self.project.top_left + SLOTS).then(|| slot - self.project.top_left)
    }

    fn max_top(&self) -> usize {
        self.max.saturating_sub(SLOTS).div_ceil(4) * 4
    }

    fn scroll(&mut self, rows: i32) -> bool {
        let t = (self.project.top_left as i32 + rows * 4).clamp(0, self.max_top() as i32) as usize;
        let moved = t != self.project.top_left;
        self.project.top_left = t;
        self.refresh_controls();
        moved
    }

    pub fn refresh_shows(&mut self) {
        self.shows = self
            .projects_dir
            .as_ref()
            .and_then(|d| std::fs::read_dir(d).ok())
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("kpp")))
                    .collect()
            })
            .unwrap_or_default();
        self.shows.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
        self.strip_pos = self.strip_pos.min(self.shows.len().saturating_sub(STRIP));
    }

    /// Disabled / hidden states that depend on the model.
    fn refresh_controls(&mut self) {
        let top = self.project.top_left;
        let max_top = self.max_top();
        let n = self.ctls.len();
        for i in 0..n {
            let id = self.ctls[i].id;
            let dis = match id {
                C_UP => top == 0,
                C_DOWN => top >= max_top,
                _ => false,
            };
            if dis && self.ctls[i].state != 3 {
                self.ctls[i].show(3, false);
            } else if !dis && self.ctls[i].state == 3 {
                self.ctls[i].show(0, false);
            }
        }
        // Prefetch the thumbnails of the visible slots.
        for k in 0..SLOTS {
            if let Some(Some(s)) = self.project.slides.get(top + k) {
                let p = s.path.clone();
                self.thumb(&p);
            }
        }
    }

    fn ctl_visible(&self, c: &Ctl) -> bool {
        let filled = |i: u16| self.project.slides.get(self.project.top_left + i as usize).is_some_and(|s| s.is_some());
        match c.id {
            x if (C_LOAD..C_LOAD + 12).contains(&x) => !self.effects,
            x if (C_TRANS..C_TRANS + 12).contains(&x) => self.effects && filled(x - C_TRANS),
            x if (C_TIMING..C_TIMING + 12).contains(&x) => self.effects && filled(x - C_TIMING),
            x if (C_SOUND..C_SOUND + 12).contains(&x) => self.effects && filled(x - C_SOUND),
            x if (C_SHOW..C_SHOW + 4).contains(&x) => self.strip_pos + ((x - C_SHOW) as usize) < self.shows.len() && self.strip_anim.is_none(),
            _ => true,
        }
    }

    fn ctl_at(&self, x: i32, y: i32) -> Option<usize> {
        // Later controls are on top (slot overlays above the grid, buttons above all).
        (0..self.ctls.len()).rev().find(|&i| {
            let c = &self.ctls[i];
            c.state != 3 && self.ctl_visible(c) && c.rect.contains(x, y)
        })
    }

    fn slot_at(&self, x: i32, y: i32) -> Option<usize> {
        (0..SLOTS).find(|&i| {
            let (sx, sy) = slot_pos(i);
            x >= sx && x < sx + 96 && y >= sy && y < sy + 72
        })
    }

    fn trash_rect(&self) -> Rect {
        self.ctls.iter().find(|c| c.id == TRASH).map(|c| c.rect).unwrap_or(Rect { left: 595, top: 490, right: 745, bottom: 600 })
    }

    fn mark_dirty(&mut self) {
        self.project.dirty = true;
    }

    /// Put a picture into a slot (Load buttons, Add > Load Picture...).
    pub fn load_into(&mut self, slot: usize, path: PathBuf) -> bool {
        if slot >= self.max || slideshow::page_thumb(&path).is_none() {
            return false;
        }
        let path = std::path::absolute(&path).unwrap_or(path);
        self.thumbs.remove(&path);
        // A picture loaded over a slide keeps that slide's effects.
        let mut slide = SlideRef::new(path.clone());
        if let Some(old) = &self.project.slides[slot] {
            slide.settings = old.settings;
        }
        self.project.slides[slot] = Some(slide);
        self.project.selected = slot;
        self.thumb(&path);
        self.mark_dirty();
        true
    }

    fn pick_picture(&mut self, slot: usize) {
        let mut d = rfd::FileDialog::new()
            .set_title(self.assets.ini.get("OpenDialogTitlesWin", "LoadPage").unwrap_or("Load Picture"))
            .add_filter("Kid Pix Files", &["kpx", "KPX"])
            .add_filter("Pictures", &["kpx", "png", "bmp", "jpg", "jpeg", "gif", "KPX", "PNG", "BMP", "JPG", "JPEG", "GIF"]);
        if let Some(p) = self.user_pictures.as_ref().filter(|d| d.is_dir()) {
            d = d.set_directory(p);
        }
        if let Some(p) = d.pick_file() {
            if !self.load_into(slot, p) {
                self.alert_text("Couldn't open this Kid Pix picture.");
            }
        }
    }

    // ------------------------------------------------------------------ commands

    fn command(&mut self, id: u16) {
        match id {
            GO_EASEL => self.request(Follow::GoEasel),
            NEW => self.request(Follow::New),
            PLAY => self.play(None),
            EFFECTS => {
                self.effects = !self.effects;
            }
            PRINT => self.print(),
            SAVE => {
                self.save_then(false, None);
            }
            HELP => self.actions.push(PickerAction::Help),
            C_UP => {
                self.scroll(-1);
            }
            C_DOWN => {
                self.scroll(1);
            }
            C_LEFT => self.strip_scroll(-1),
            C_RIGHT => self.strip_scroll(1),
            x if (C_LOAD..C_LOAD + 12).contains(&x) => {
                let slot = self.project.top_left + (x - C_LOAD) as usize;
                self.project.selected = slot;
                self.pick_picture(slot);
            }
            x if (C_TRANS..C_TRANS + 12).contains(&x) => self.open_trans(self.project.top_left + (x - C_TRANS) as usize),
            x if (C_TIMING..C_TIMING + 12).contains(&x) => self.open_timer(self.project.top_left + (x - C_TIMING) as usize),
            x if (C_SOUND..C_SOUND + 12).contains(&x) => self.open_audio(self.project.top_left + (x - C_SOUND) as usize),
            x if (C_SHOW..C_SHOW + 4).contains(&x) => {
                if let Some(p) = self.shows.get(self.strip_pos + (x - C_SHOW) as usize).cloned() {
                    if self.project.path.as_ref() != Some(&p) {
                        self.request(Follow::Open(p));
                    }
                }
            }
            _ => {}
        }
    }

    fn strip_scroll(&mut self, dir: i32) {
        let max = self.shows.len().saturating_sub(STRIP);
        let np = (self.strip_pos as i32 + dir).clamp(0, max as i32) as usize;
        // Moving: icons slide along the markers of 5238/5239; at the end they bump (5242/5243).
        let (id, moved) = if np != self.strip_pos { (if dir > 0 { 5238 } else { 5239 }, true) } else { (if dir > 0 { 5242 } else { 5243 }, false) };
        if let Some(mut p) = Player::new(&self.assets, PICKER, id) {
            let snd = p.update(0.0);
            self.play_sounds(PICKER, 5000, snd);
            self.strip_anim = Some((p, if moved { dir } else { 0 }));
        }
        if !moved {
            return;
        }
        // The position changes when the animation ends (see update).
    }

    /// Ask "save the slide show?" (Warnings 7543) when there are unsaved changes.
    fn request(&mut self, f: Follow) {
        if self.project.dirty && self.project.filled() > 0 {
            let d = Dialog::new(&self.assets, WARN, &[7500, 7543], &[780]);
            self.modal = Some(Modal { dlg: d, kind: Kind::AskSave(f), ctls: vec![], fields: vec![] });
            return;
        }
        self.follow(f);
    }

    fn follow(&mut self, f: Follow) {
        match f {
            Follow::New => {
                self.project = Project::new(self.max);
                self.refresh_controls();
            }
            Follow::Open(p) => {
                if let Err(e) = self.open(&p) {
                    self.alert_text(&format!("Couldn't open this SlideShow.\n{e}"));
                }
            }
            Follow::OpenDialog => {
                let mut d = rfd::FileDialog::new()
                    .set_title(self.assets.ini.get("OpenDialogTitlesWin", "OpenProject").unwrap_or("Open SlideShow"))
                    .add_filter("Kid Pix SlideShow", &["kpp", "KPP"]);
                if let Some(dir) = self.projects_dir.as_ref().filter(|d| d.is_dir()) {
                    d = d.set_directory(dir);
                }
                if let Some(p) = d.pick_file() {
                    self.follow(Follow::Open(p));
                }
            }
            Follow::GoEasel => self.actions.push(PickerAction::GoEasel),
            Follow::Quit => self.actions.push(PickerAction::Quit),
            Follow::Login => self.actions.push(PickerAction::Login),
        }
    }

    /// Save (1373): in place when the show has a file, else ask for a name (Save dialog, or
    /// EnterName in Small Kids Mode); `then` continues after a successful save.
    fn save_then(&mut self, ask: bool, then: Option<Follow>) {
        if self.small_kids && (ask || self.project.path.is_none()) {
            self.open_name_show(then);
            return;
        }
        if self.save(ask) {
            if let Some(f) = then {
                self.follow(f);
            }
        }
    }

    fn open_name_show(&mut self, then: Option<Follow>) {
        const ARCH: &str = "Data/EnterName.MHK";
        let mut d = Dialog::new(&self.assets, ARCH, &[7200, 7210], &[777]);
        let r = bounds(&self.assets, ARCH, 7201);
        let r = if r == Rect::default() { Rect { left: 273, top: 246, right: 529, bottom: 270 } } else { r };
        let name = self.project.name().unwrap_or_else(|| self.untitled.clone());
        let mut f = dialogs::Field::new(r, &name);
        f.placeholder = self.project.path.is_none();
        f.color = gfx::rgb(255, 255, 255);
        f.accept = Some((ARCH.into(), 7200));
        f.reject = Some((ARCH.into(), 7201));
        f.legal = Some(self.assets.ini.get("LoginAlphabet", "LegalAlphabet").unwrap_or("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789 ").to_string());
        d.field = Some(f);
        self.modal = Some(Modal { dlg: d, kind: Kind::NameShow(then), ctls: vec![], fields: vec![] });
    }

    /// Returns false when cancelled or failed.
    fn save(&mut self, ask: bool) -> bool {
        let path = match (&self.project.path, ask) {
            (Some(p), false) => Some(p.clone()),
            _ => {
                let name = self.project.name().unwrap_or_else(|| self.untitled.clone());
                let mut d = rfd::FileDialog::new()
                    .set_title(self.assets.ini.get("OpenDialogTitlesWin", "SaveProject").unwrap_or("Save SlideShow As"))
                    .add_filter("Kid Pix SlideShow", &["kpp"])
                    .set_file_name(format!("{name}.kpp"));
                if let Some(dir) = self.projects_dir.as_ref().filter(|d| d.is_dir()) {
                    d = d.set_directory(dir);
                }
                d.save_file()
            }
        };
        let Some(mut p) = path else { return false };
        if p.extension().is_none() {
            p.set_extension("kpp");
        }
        self.save_to(&p)
    }

    pub fn save_to(&mut self, p: &Path) -> bool {
        match self.project.save(p) {
            Ok(()) => {
                self.refresh_shows();
                // Keep the saved show visible in the strip.
                if let Some(i) = self.shows.iter().position(|s| s == p) {
                    if i < self.strip_pos || i >= self.strip_pos + STRIP {
                        self.strip_pos = i.saturating_sub(STRIP - 1).min(self.shows.len().saturating_sub(STRIP));
                    }
                }
                true
            }
            Err(e) => {
                let msg = self.assets.ini.get("FileStrings", "GeneralFileErr").unwrap_or("An error occurred while saving this file.").to_string();
                self.alert_text(&format!("{msg}\n{e}"));
                false
            }
        }
    }

    fn play(&mut self, start: Option<usize>) {
        if self.project.filled() == 0 {
            return;
        }
        self.actions.push(PickerAction::Play(start));
    }

    /// The filled slides in order (what the player shows).
    pub fn playlist(&self) -> Vec<SlideRef> {
        self.project.slides.iter().flatten().cloned().collect()
    }

    fn print(&mut self) {
        // [I] Dialog 151 (print mode / range) is not rebuilt: every slide is printed as a
        // Full Page sheet (§12).
        let pages: Vec<Image> = self.playlist().iter().filter_map(|s| slideshow::load_page(&s.path)).map(|p| p.composite).collect();
        if pages.is_empty() {
            return;
        }
        if let Err(e) = crate::print::print_pages(&pages) {
            self.alert_text(&format!("Couldn't print: {e}"));
        }
    }

    fn alert_text(&mut self, text: &str) {
        let mut d = Dialog::new(&self.assets, "Data/RAlertDlg.MHK", &[10000], &[777]);
        let r = bounds(&self.assets, "Data/RAlertDlg.MHK", 10005);
        let r = if r == Rect::default() { Rect { left: 284, top: 185, right: 554, bottom: 311 } } else { r };
        d.body = Some((r, text.to_string(), 16.0));
        self.modal = Some(Modal { dlg: d, kind: Kind::Alert, ctls: vec![], fields: vec![] });
    }

    fn alert_art(&mut self, art: u16) {
        let d = Dialog::new(&self.assets, WARN, &[7500, art], &[777]);
        self.modal = Some(Modal { dlg: d, kind: Kind::Alert, ctls: vec![], fields: vec![] });
    }

    // ------------------------------------------------------------------ dialogs

    fn dlg_ctl(&self, arch: &str, id: u16, rest: u16, over: u16, press: u16, dis: u16) -> Ctl {
        let mk = |s: u16| Player::new(&self.assets, arch, s).map(Art::Script).unwrap_or(Art::None);
        Ctl::new(id, bounds(&self.assets, arch, rest), mk(rest), mk(over), mk(press), mk(dis))
    }

    fn open_trans(&mut self, slot: usize) {
        let Some(Some(s)) = self.project.slides.get(slot) else { return };
        let st = s.settings;
        self.project.selected = slot;
        let d = Dialog::new(&self.assets, TRANS_DLG, &[7000, 7001, 7002, 7012, 7003, 7013], &[777]);
        let fields = Player::new(&self.assets, TRANS_DLG, 7001)
            .map(|p| {
                p.placed()
                    .iter()
                    .filter_map(|&(cel, x, y)| {
                        let img = p.sprites.cel(cel)?;
                        Some(Rect { left: x as i16, top: y as i16, right: (x + img.width as i32) as i16, bottom: (y + img.height as i32) as i16 })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let ctls = vec![
            self.dlg_ctl(TRANS_DLG, 1, 7004, 7005, 7006, 7011),
            self.dlg_ctl(TRANS_DLG, 2, 7007, 7008, 7009, 7010),
            self.dlg_ctl(TRANS_DLG, 3, 7014, 7015, 7016, 7021),
            self.dlg_ctl(TRANS_DLG, 4, 7017, 7018, 7019, 7020),
        ];
        let tr = st.transition as usize;
        let snd = st.sound as usize;
        let mut m = Modal {
            dlg: d,
            kind: Kind::Trans { slot, tr, snd, ttop: (tr / 6).min(3), stop: (snd / 4).min(7), hover: None },
            ctls,
            fields,
        };
        Self::refresh_trans(&mut m);
        self.modal = Some(m);
    }

    fn refresh_trans(m: &mut Modal) {
        let Kind::Trans { ttop, stop, .. } = m.kind else { return };
        let dis = [ttop == 0, ttop >= 3, stop == 0, stop >= 7];
        for (k, c) in m.ctls.iter_mut().enumerate() {
            if dis[k] && c.state != 3 {
                c.show(3, false);
            } else if !dis[k] && c.state == 3 {
                c.show(0, false);
            }
        }
    }

    fn open_timer(&mut self, slot: usize) {
        let Some(Some(s)) = self.project.slides.get(slot) else { return };
        let st = s.settings;
        self.project.selected = slot;
        let mut d = Dialog::new(&self.assets, TIMER_DLG, &[17000, 17001], &[889]);
        let r = bounds(&self.assets, TIMER_DLG, 17006);
        let mut f = dialogs::Field::new(if r == Rect::default() { Rect { left: 399, top: 175, right: 434, bottom: 197 } } else { r }, &st.seconds.to_string());
        f.legal = Some("0123456789".into());
        f.max = 3;
        f.color = TEXT;
        f.size = 15.0;
        d.field = Some(f);
        let cmd = [1407, 1408, 1409, 1411][st.advance.min(3) as usize];
        d.set_selected(cmd, true);
        self.modal = Some(Modal { dlg: d, kind: Kind::Timer { slot }, ctls: vec![], fields: vec![] });
    }

    fn open_audio(&mut self, slot: usize) {
        let Some(Some(s)) = self.project.slides.get(slot) else { return };
        let st = s.settings;
        self.project.selected = slot;
        let mut d = Dialog::new(&self.assets, AUDIO_DLG, &[7000], &[777]);
        d.set_selected(1415, st.play_sound != 0);
        d.set_selected(1416, st.play_text != 0);
        self.modal = Some(Modal { dlg: d, kind: Kind::Audio { slot }, ctls: vec![], fields: vec![] });
    }

    fn open_bg_color(&mut self) {
        let mut d = Dialog::new(&self.assets, BGCOLOR_DLG, &[7700], &[777]);
        let sel = self.project.bg_color;
        for (i, b) in d.buttons.iter().enumerate() {
            d.selected[i] = b.cmd == 1000 && b.value as usize == sel;
        }
        // The preview chip: cel 18 of the frame script.
        let chip = Player::new(&self.assets, BGCOLOR_DLG, 7700)
            .and_then(|p| {
                let (cel, x, y) = p.placed().into_iter().find(|c| c.0 == 18)?;
                let img = p.sprites.cel(cel)?;
                Some(Rect { left: x as i16, top: y as i16, right: (x + img.width as i32) as i16, bottom: (y + img.height as i32) as i16 })
            })
            .unwrap_or(Rect { left: 170, top: 191, right: 212, bottom: 214 });
        self.modal = Some(Modal { dlg: d, kind: Kind::BgColor { sel, chip }, ctls: vec![], fields: vec![] });
    }

    fn confirm_delete_slide(&mut self, slot: usize) {
        let d = Dialog::new(&self.assets, DELETERS, &[7800, 7811], &[777]);
        self.modal = Some(Modal { dlg: d, kind: Kind::DeleteSlide(slot), ctls: vec![], fields: vec![] });
    }

    fn confirm_delete_show(&mut self, p: PathBuf) {
        if self.project.path.as_ref() == Some(&p) {
            // "The project can't be deleted because it is in use."
            self.alert_art(7536);
            return;
        }
        let d = Dialog::new(&self.assets, DELETERS, &[7800, 7813], &[777]);
        self.modal = Some(Modal { dlg: d, kind: Kind::DeleteShow(p), ctls: vec![], fields: vec![] });
    }

    fn set_slide(&mut self, slot: usize, f: impl FnOnce(&mut SlideSettings)) {
        if let Some(Some(s)) = self.project.slides.get_mut(slot) {
            let before = s.settings;
            f(&mut s.settings);
            if s.settings != before {
                self.project.dirty = true;
            }
        }
    }

    fn timer_seconds(m: &Modal) -> u8 {
        m.dlg.field.as_ref().and_then(|f| f.text.parse::<u32>().ok()).unwrap_or(0).min(255) as u8
    }

    fn modal_command(&mut self, cmd: u16) {
        let Some(mut m) = self.modal.take() else { return };
        let close = |p: &mut Self| p.audio.stop_channel(audio::CH_FX);
        match &mut m.kind {
            Kind::Trans { slot, tr, snd, .. } => match cmd {
                dialogs::OK => {
                    let (slot, tr, snd) = (*slot, *tr as u16, *snd as u16);
                    self.set_slide(slot, |s| {
                        s.transition = tr;
                        s.sound = snd;
                    });
                    close(self);
                    return;
                }
                dialogs::REPLAY => {
                    let (slot, tr, snd) = (*slot, *tr, *snd as u16);
                    self.modal = Some(m);
                    self.start_preview(slot, tr, snd);
                    return;
                }
                dialogs::CANCEL => {
                    close(self);
                    return;
                }
                _ => {}
            },
            Kind::Timer { slot } => {
                let slot = *slot;
                match cmd {
                    1407 | 1408 | 1409 | 1411 => {
                        m.dlg.select_only(|b| matches!(b.cmd, 1407 | 1408 | 1409 | 1411), m.dlg.last_index.unwrap_or(0));
                    }
                    1410 | 1412 => {
                        let s = Self::timer_seconds(&m);
                        let s = if cmd == 1410 { s.wrapping_add(1) } else { s.wrapping_sub(1) };
                        if let Some(f) = m.dlg.field.as_mut() {
                            f.set(&s.to_string());
                        }
                        m.dlg.set_selected(1407, true);
                        for c in [1408, 1409, 1411] {
                            m.dlg.set_selected(c, false);
                        }
                    }
                    dialogs::OK => {
                        let secs = Self::timer_seconds(&m);
                        let mode = [1407u16, 1408, 1409, 1411]
                            .iter()
                            .position(|&c| m.dlg.index_of(c).is_some_and(|i| m.dlg.selected[i]))
                            .unwrap_or(0) as u8;
                        self.set_slide(slot, |s| {
                            s.seconds = secs;
                            s.advance = mode;
                        });
                        return;
                    }
                    dialogs::CANCEL => return,
                    _ => {}
                }
            }
            Kind::Audio { slot } => {
                let slot = *slot;
                match cmd {
                    1415 | 1416 => {
                        let on = m.dlg.index_of(cmd).is_some_and(|i| m.dlg.selected[i]);
                        m.dlg.set_selected(cmd, !on);
                    }
                    dialogs::OK => {
                        let get = |c: u16| m.dlg.index_of(c).is_some_and(|i| m.dlg.selected[i]) as u16;
                        let (ps, pt) = (get(1415), get(1416));
                        self.set_slide(slot, |s| {
                            s.play_sound = ps;
                            s.play_text = pt;
                        });
                        return;
                    }
                    dialogs::CANCEL => return,
                    _ => {}
                }
            }
            Kind::BgColor { sel, .. } => match cmd {
                1000 => {
                    if let Some(i) = m.dlg.last_index {
                        *sel = m.dlg.buttons[i].value as usize;
                        let s = *sel;
                        for (k, b) in m.dlg.buttons.iter().enumerate() {
                            m.dlg.selected[k] = b.cmd == 1000 && b.value as usize == s;
                        }
                    }
                }
                dialogs::OK => {
                    if self.project.bg_color != *sel {
                        self.project.bg_color = *sel;
                        self.project.dirty = true;
                    }
                    return;
                }
                dialogs::CANCEL => return,
                _ => {}
            },
            Kind::DeleteSlide(slot) => {
                if cmd == dialogs::OK {
                    let slot = *slot;
                    if let Some(s) = self.project.slides.get_mut(slot) {
                        if s.take().is_some() {
                            self.project.dirty = true;
                        }
                    }
                }
                return;
            }
            Kind::DeleteShow(p) => {
                if cmd == dialogs::OK {
                    let _ = std::fs::remove_file(&*p);
                    self.refresh_shows();
                }
                return;
            }
            Kind::AskSave(_) => {
                let Kind::AskSave(f) = std::mem::replace(&mut m.kind, Kind::Alert) else { return };
                match cmd {
                    dialogs::YES => self.save_then(false, Some(f)),
                    dialogs::NO => self.follow(f),
                    _ => {}
                }
                return;
            }
            Kind::NameShow(_) => {
                let Kind::NameShow(then) = std::mem::replace(&mut m.kind, Kind::Alert) else { return };
                if cmd != dialogs::OK {
                    return;
                }
                let name = m.dlg.field.as_ref().map(|f| f.text.trim().to_string()).unwrap_or_default();
                let Some(dir) = self.projects_dir.clone().filter(|_| !name.is_empty()) else { return };
                let path = dir.join(format!("{name}.kpp"));
                if path.exists() && self.project.path.as_ref() != Some(&path) {
                    // "This name has already been used!"
                    self.alert_art(7541);
                    return;
                }
                if self.save_to(&path) {
                    if let Some(f) = then {
                        self.follow(f);
                    }
                }
                return;
            }
            Kind::Alert => return,
        }
        self.modal = Some(m);
    }

    fn start_preview(&mut self, slot: usize, kind: usize, snd: u16) {
        let bg = slideshow::bg_color(self.project.bg_color);
        let to = self.project.slides.get(slot).cloned().flatten().and_then(|s| slideshow::load_page(&s.path)).map(|p| p.composite);
        let Some(to) = to else { return };
        let from = self.project.slides[..slot]
            .iter()
            .rev()
            .flatten()
            .next()
            .and_then(|s| slideshow::load_page(&s.path))
            .map(|p| p.composite)
            .unwrap_or_else(|| Image::filled(to.width, to.height, bg));
        if snd != slideshow::NO_SOUND {
            if let Some(w) = self.assets.sound(TRANS_SOUNDS, 12000 + snd) {
                self.audio.play(w, audio::CH_FX, false);
            }
        }
        let mix = from.clone();
        self.preview = Some(Preview { from, to, kind, t: 0.0, mix, bg });
    }

    /// Mouse press inside the transition dialog's icon grids.
    fn trans_click(&mut self, x: i32, y: i32) -> bool {
        let Some(m) = self.modal.as_mut() else { return false };
        let Kind::Trans { tr, snd, ttop, stop, .. } = &mut m.kind else { return false };
        for k in 0..4 {
            if m.ctls[k].state != 3 && m.ctls[k].rect.contains(x, y) {
                let s = m.ctls[k].show(2, true);
                match k {
                    0 => *ttop = ttop.saturating_sub(1),
                    1 => *ttop = (*ttop + 1).min(3),
                    2 => *stop = stop.saturating_sub(1),
                    _ => *stop = (*stop + 1).min(7),
                }
                Self::refresh_trans(m);
                self.play_sounds(TRANS_DLG, 7000, s);
                return true;
            }
        }
        for j in 0..12u16 {
            if bounds(&self.assets, TRANS_DLG, 7034 + j).contains(x, y) {
                let idx = *ttop * 6 + j as usize;
                if idx < slideshow::TRANSITIONS {
                    *tr = idx;
                    if let Some(mut p) = Player::new(&self.assets, TRANS_DLG, 7050 + j) {
                        let s = p.update(0.0);
                        self.play_sounds(TRANS_DLG, 7000, s);
                    }
                }
                return true;
            }
        }
        for j in 0..4u16 {
            if bounds(&self.assets, TRANS_DLG, 7046 + j).contains(x, y) {
                let idx = *stop * 4 + j as usize;
                if idx < slideshow::TRANS_SOUND_COUNT {
                    *snd = idx;
                    self.audio.stop_channel(audio::CH_FX);
                    if idx as u16 != slideshow::NO_SOUND {
                        if let Some(w) = self.assets.sound(TRANS_SOUNDS, 12000 + idx as u16) {
                            self.audio.play(w, audio::CH_FX, false);
                        }
                    }
                }
                return true;
            }
        }
        false
    }

    // ------------------------------------------------------------------ input

    pub fn busy(&self) -> bool {
        self.modal.is_some() || self.preview.is_some()
    }

    /// PickerHelp.MHK tables for the current view (§7).
    pub fn help_tables(&self) -> Vec<u16> {
        vec![777, if self.effects { 778 } else { 779 }]
    }

    pub fn mouse_move(&mut self, x: i32, y: i32, fonts: &Fonts, ms: &MenuState) {
        self.mouse = (x, y);
        self.idle_wait = self.idle_wait.max(3000.0);
        self.menubar.motion(fonts, ms, x, y);
        if self.menu_press || self.menubar.open.is_some() {
            return;
        }
        if self.preview.is_some() {
            return;
        }
        if let Some(m) = self.modal.as_mut() {
            m.dlg.mouse_move(x, y, &self.assets, &self.audio);
            let mut snds = Vec::new();
            for c in m.ctls.iter_mut() {
                if c.state == 3 || c.state == 2 {
                    continue;
                }
                let inside = c.rect.contains(x, y);
                if inside && c.state == 0 {
                    snds.extend(c.show(1, true));
                } else if !inside && c.state == 1 {
                    c.show(0, false);
                }
            }
            if let Kind::Trans { hover, .. } = &mut m.kind {
                *hover = (0..12u16)
                    .find(|&j| bounds(&self.assets, TRANS_DLG, 7034 + j).contains(x, y))
                    .map(|j| (false, j as usize))
                    .or_else(|| (0..4u16).find(|&j| bounds(&self.assets, TRANS_DLG, 7046 + j).contains(x, y)).map(|j| (true, j as usize)));
            }
            self.play_sounds(TRANS_DLG, 7000, snds);
            return;
        }
        if let Some(d) = self.drag.as_mut() {
            if !d.active && ((x - d.start.0).abs() > 3 || (y - d.start.1).abs() > 3) {
                d.active = true;
            }
            if d.active {
                let slide = matches!(d.kind, DragKind::Slide(_));
                self.drop_slot = if slide { self.slot_at(x, y) } else { None };
                let over = self.trash_rect().contains(x, y);
                if over != self.over_trash {
                    self.over_trash = over;
                    if let Some(i) = self.ctls.iter().position(|c| c.id == TRASH) {
                        let s = self.ctls[i].show(if over { 1 } else { 0 }, over);
                        self.play_sounds(PICKER, 5000, s);
                    }
                }
                return;
            }
        }
        let h = self.ctl_at(x, y);
        if h == self.hover {
            return;
        }
        if let Some(i) = self.hover {
            if self.ctls[i].state == 1 {
                self.ctls[i].show(0, false);
            }
        }
        if let Some(i) = h {
            if self.pressed.is_none() && self.ctls[i].state == 0 {
                let s = self.ctls[i].show(1, true);
                self.play_sounds(PICKER, 5000, s);
            }
        }
        self.hover = h;
    }

    pub fn mouse_down(&mut self, x: i32, y: i32) {
        self.mouse = (x, y);
        self.idle_wait = self.idle_wait.max(3000.0);
        if self.about.take().is_some() {
            return;
        }
        if self.menubar.press(x, y) {
            self.menu_press = true;
            return;
        }
        if self.preview.is_some() {
            self.preview = None;
            return;
        }
        if self.modal.is_some() {
            if self.trans_click(x, y) {
                return;
            }
            let m = self.modal.as_mut().unwrap();
            if let Kind::Timer { .. } = m.kind {
                if m.dlg.field.as_ref().is_some_and(|f| f.rect.contains(x, y)) {
                    m.dlg.select_only(|b| matches!(b.cmd, 1407 | 1408 | 1409 | 1411), m.dlg.index_of(1407).unwrap_or(0));
                }
            }
            m.dlg.mouse_down(x, y, &self.assets, &self.audio);
            return;
        }
        if let Some(i) = self.ctl_at(x, y) {
            let id = self.ctls[i].id;
            // Strip icons can be dragged to the trash; they open on release.
            if (C_SHOW..C_SHOW + 4).contains(&id) {
                if let Some(p) = self.shows.get(self.strip_pos + (id - C_SHOW) as usize).cloned() {
                    self.drag = Some(Drag { kind: DragKind::Show(p), start: (x, y), active: false });
                }
            }
            self.pressed = Some(i);
            let s = self.ctls[i].show(2, true);
            self.play_sounds(PICKER, 5000, s);
            return;
        }
        if let Some(k) = self.slot_at(x, y) {
            let slot = self.project.top_left + k;
            self.project.selected = slot;
            if self.project.slides.get(slot).is_some_and(|s| s.is_some()) {
                self.drag = Some(Drag { kind: DragKind::Slide(slot), start: (x, y), active: false });
            }
        }
    }

    pub fn mouse_up(&mut self, x: i32, y: i32, fonts: &Fonts, ms: &mut MenuState) {
        self.mouse = (x, y);
        if self.menu_press {
            self.menu_press = false;
            if let Some(id) = self.menubar.release(fonts, ms, x, y) {
                self.menu_command(id, ms);
            }
            return;
        }
        if let Some(m) = self.modal.as_mut() {
            for c in m.ctls.iter_mut() {
                if c.state == 2 {
                    c.show(if c.rect.contains(x, y) { 1 } else { 0 }, false);
                }
            }
            let Kind::Trans { .. } = m.kind else {
                if let Some(c) = m.dlg.mouse_up(x, y) {
                    self.modal_command(c);
                }
                return;
            };
            if let Some(c) = m.dlg.mouse_up(x, y) {
                self.modal_command(c);
            } else if let Some(m) = self.modal.as_mut() {
                Self::refresh_trans(m);
            }
            return;
        }
        let drag = self.drag.take();
        let was_over_trash = std::mem::take(&mut self.over_trash);
        let drop = self.drop_slot.take();
        if let Some(d) = drag.filter(|d| d.active) {
            if let Some(i) = self.pressed.take() {
                self.ctls[i].show(0, false);
            }
            if let Some(i) = self.ctls.iter().position(|c| c.id == TRASH) {
                self.ctls[i].show(0, false);
            }
            if was_over_trash {
                // The trash swallows it (5004 for a slide, 5003 eats a slide-show icon).
                let anim = if matches!(d.kind, DragKind::Show(_)) { 5003 } else { 5004 };
                if let Some(mut p) = Player::new(&self.assets, PICKER, anim) {
                    let s = p.update(0.0);
                    self.play_sounds(PICKER, 5000, s);
                    self.trash_anim = Some(p);
                }
                match d.kind {
                    DragKind::Slide(s) => self.confirm_delete_slide(s),
                    DragKind::Show(p) => self.confirm_delete_show(p),
                }
            } else if let (DragKind::Slide(from), Some(k)) = (d.kind, drop) {
                let to = self.project.top_left + k;
                if to != from {
                    self.project.move_slide(from, to);
                    self.project.selected = to;
                    self.mark_dirty();
                }
            }
            return;
        }
        if let Some(i) = self.pressed.take() {
            let inside = self.ctls[i].rect.contains(x, y);
            self.ctls[i].show(if inside { 1 } else { 0 }, false);
            if inside {
                let id = self.ctls[i].id;
                self.command(id);
            }
        }
    }

    pub fn wheel(&mut self, steps: f32) {
        if self.busy() {
            return;
        }
        if steps > 0.0 {
            self.scroll(-1);
        } else if steps < 0.0 {
            self.scroll(1);
        }
    }

    /// Keys; returns true when used.
    pub fn key(&mut self, k: Key) -> bool {
        if self.preview.is_some() {
            if k == Key::Escape {
                self.preview = None;
            }
            return true;
        }
        if let Some(m) = self.modal.as_mut() {
            let c = m.dlg.key(k, &self.assets, &self.audio);
            if let Kind::Timer { .. } = m.kind {
                // The seconds field is clamped to 0–255 and typing selects "Timer".
                if let Some(f) = m.dlg.field.as_mut() {
                    if f.text.parse::<u32>().is_ok_and(|v| v > 255) {
                        f.set("255");
                    }
                }
                if matches!(k, Key::Char(_) | Key::Back | Key::Delete) {
                    m.dlg.select_only(|b| matches!(b.cmd, 1407 | 1408 | 1409 | 1411), m.dlg.index_of(1407).unwrap_or(0));
                }
            }
            if let Some(c) = c {
                self.modal_command(c);
            }
            return true;
        }
        if k == Key::Escape {
            self.menubar.close();
            self.drag = None;
            self.drop_slot = None;
            return true;
        }
        if matches!(k, Key::PageUp) {
            self.scroll(-3);
        }
        if matches!(k, Key::PageDown) {
            self.scroll(3);
        }
        true
    }

    /// A Picker-mode menu command (menu mode 0x81, §4).
    pub fn menu_command(&mut self, id: u16, ms: &mut MenuState) {
        if !ms.enabled(id) || self.busy() {
            return;
        }
        let sel = self.project.selected;
        match id {
            cmd::NEW => self.request(Follow::New),
            cmd::OPEN => self.request(Follow::OpenDialog),
            cmd::SAVE_AS => self.save_then(true, None),
            cmd::PRINT => self.print(),
            cmd::EXPORT => self.alert_text("SlideShow To Go isn't available in this version."),
            cmd::NEW_USER => self.request(Follow::Login),
            cmd::EXIT => self.request(Follow::Quit),
            cmd::CUT | cmd::COPY => {
                if let Some(Some(s)) = self.project.slides.get(sel) {
                    self.clipboard = Some(s.clone());
                    if id == cmd::CUT {
                        self.project.slides[sel] = None;
                        self.mark_dirty();
                    }
                }
            }
            cmd::PASTE => {
                if let Some(s) = self.clipboard.clone() {
                    if sel < self.max {
                        self.project.slides[sel] = Some(s);
                        self.mark_dirty();
                        self.refresh_controls();
                    }
                }
            }
            cmd::LOAD_PICTURE => self.pick_picture(sel),
            cmd::CHANGE_EFFECTS => self.open_trans(sel),
            cmd::PICK_BG_COLOR => self.open_bg_color(),
            cmd::TEXT_TO_SPEECH => {
                ms.text_to_speech = !ms.text_to_speech;
                if !ms.text_to_speech {
                    crate::speech::stop();
                }
            }
            cmd::MANUAL_ADVANCE => {
                ms.manual_advance = !ms.manual_advance;
                self.project.manual_advance = ms.manual_advance;
                self.mark_dirty();
            }
            cmd::CLEAN_UP => {
                self.project.clean_up();
                self.mark_dirty();
                self.refresh_controls();
            }
            cmd::PROJECT_MODE => self.request(Follow::GoEasel),
            cmd::HELP => self.actions.push(PickerAction::Help),
            cmd::ABOUT => {
                self.about = self
                    .assets
                    .archive("Data/About.MHK")
                    .and_then(|a| a.ids(crate::assets::T_SCR).first().copied())
                    .and_then(|sid| Player::new(&self.assets, "Data/About.MHK", sid));
            }
            cmd::USER_GUIDE => self.actions.push(PickerAction::UserGuide),
            _ => {}
        }
    }

    /// Quit from the window's close box: ask to save first.
    pub fn request_quit(&mut self) {
        if self.modal.is_none() {
            self.request(Follow::Quit);
        }
    }

    // ------------------------------------------------------------------ update / draw

    pub fn update(&mut self, dt: f32) {
        let mut snd: Vec<(&'static str, u16, Vec<u16>)> = Vec::new();
        for c in self.ctls.iter_mut() {
            let s = c.update(dt);
            if !s.is_empty() {
                snd.push((PICKER, 5000, s));
            }
        }
        if let Some(m) = self.modal.as_mut() {
            m.dlg.update(dt, &self.assets, &self.audio);
            for c in m.ctls.iter_mut() {
                let s = c.update(dt);
                if !s.is_empty() {
                    snd.push((TRANS_DLG, 7000, s));
                }
            }
        }
        if let Some(p) = self.preview.as_mut() {
            p.t += dt / slideshow::TRANSITION_MS;
            slideshow::transition(p.kind, p.t.min(1.0), &p.from, &p.to, &mut p.mix);
            if p.t > 1.6 {
                self.preview = None;
            }
        }
        if let Some(p) = self.lights.as_mut() {
            let s = p.update(dt);
            if !s.is_empty() {
                snd.push((PICKER, 5000, s));
            }
            if p.finished {
                self.lights = None;
            }
        }
        if let Some(p) = self.trash_anim.as_mut() {
            let s = p.update(dt);
            if !s.is_empty() {
                snd.push((PICKER, 5000, s));
            }
            if p.finished {
                self.trash_anim = None;
            }
        }
        if let Some((p, dir)) = self.strip_anim.as_mut() {
            let s = p.update(dt);
            if !s.is_empty() {
                snd.push((PICKER, 5000, s));
            }
            if p.finished {
                let d = *dir;
                self.strip_anim = None;
                if d != 0 {
                    let max = self.shows.len().saturating_sub(STRIP);
                    self.strip_pos = (self.strip_pos as i32 + d).clamp(0, max as i32) as usize;
                }
            }
        }
        // Dragging a slide over the scroll arrows scrolls the grid.
        if let Some(d) = self.drag.as_ref().filter(|d| d.active && matches!(d.kind, DragKind::Slide(_))) {
            let _ = d;
            self.scroll_wait -= dt;
            let (x, y) = self.mouse;
            let up = bounds(&self.assets, PICKER, 5060).contains(x, y);
            let down = bounds(&self.assets, PICKER, 5063).contains(x, y);
            if (up || down) && self.scroll_wait <= 0.0 {
                self.scroll(if up { -1 } else { 1 });
                self.scroll_wait = 500.0;
            }
        }
        // Idle characters ('TOut' timer, random 3–7 s): one of the three plays a random
        // animation from its range.
        if let Some((_, p)) = self.idle.as_mut() {
            let s = p.update(dt);
            if !s.is_empty() {
                snd.push((PICKER, 5000, s));
            }
            if p.finished {
                self.idle = None;
                self.idle_wait = 3000.0 + self.rand(4000) as f32;
            }
        } else if !self.busy() {
            self.idle_wait -= dt;
            if self.idle_wait <= 0.0 {
                let who = self.rand(3) as usize;
                let (lo, hi) = [(5245u16, 5247u16), (5249, 5256), (5258, 5264)][who];
                let id = lo + self.rand((hi - lo + 1) as u32) as u16;
                match Player::new(&self.assets, PICKER, id) {
                    Some(mut p) => {
                        let s = p.update(0.0);
                        if !s.is_empty() {
                            snd.push((PICKER, 5000, s));
                        }
                        self.idle = Some((who, p));
                    }
                    None => self.idle_wait = 3000.0,
                }
            }
        }
        for (a, base, s) in snd {
            self.play_sounds(a, base, s);
        }
        if let Some(p) = self.about.as_mut() {
            p.update(dt);
        }
    }

    fn draw_ctl(&self, frame: &mut Image, c: &Ctl) {
        let art = &c.art[c.state as usize];
        let art = if matches!(art, Art::None) && c.state != 3 { &c.art[0] } else { art };
        match art {
            Art::None => {}
            Art::Script(p) => p.draw(frame),
            Art::Cel(cel, x, y) => {
                if let Some(img) = self.sprites.as_ref().and_then(|s| s.cel(*cel)) {
                    gfx::blit(frame, img, *x, *y);
                }
            }
            Art::Cels(v) => {
                for &(cel, x, y) in v {
                    if let Some(img) = self.sprites.as_ref().and_then(|s| s.cel(cel)) {
                        gfx::blit(frame, img, x, y);
                    }
                }
            }
        }
    }

    fn draw_held(&self, frame: &mut Image, id: u16) {
        if let Some(p) = self.held.get(&id) {
            p.draw(frame);
        }
    }

    fn draw_cel(&self, frame: &mut Image, cel: u16, x: i32, y: i32) {
        if let Some(img) = self.sprites.as_ref().and_then(|s| s.cel(cel)) {
            gfx::blit(frame, img, x, y);
        }
    }

    fn timing_label(&self, s: &SlideSettings) -> String {
        match s.advance {
            0 => format!("{} {}", s.seconds, self.timer_names.first().map(|s| s.as_str()).unwrap_or("Seconds")),
            m => self.timer_names.get(m as usize).cloned().unwrap_or_default(),
        }
    }

    /// One strip icon (scripts of position 0 moved to marker (mx,my)) with its label.
    fn draw_show_icon(&self, frame: &mut Image, fonts: &Fonts, name: &str, k: usize, state: u8, at: Option<(i32, i32)>, current: bool) {
        let base = 5009 + 3 * k as u16 + state.min(2) as u16;
        let (ox, oy) = match at {
            Some((mx, my)) => {
                let (bx, by) = [(112, 500), (220, 506), (332, 508), (442, 497)][k];
                (mx - bx, my - by)
            }
            None => (0, 0),
        };
        let label;
        if let Some(p) = Player::new(&self.assets, PICKER, base).map(|mut p| {
            p.finish_now();
            p
        }) {
            let placed = p.placed();
            for &(cel, x, y) in &placed {
                if let Some(img) = p.sprites.cel(cel) {
                    gfx::blit(frame, img, x + ox, y + oy);
                }
            }
            label = placed.get(1).and_then(|&(cel, x, y)| p.sprites.cel(cel).map(|i| (x + ox, y + oy, i.width as i32, i.height as i32)));
        } else {
            label = None;
        }
        if current {
            if let Some((_, x, y)) = self.held.get(&(5021 + k as u16)).and_then(|p| p.placed().first().copied()) {
                self.draw_cel(frame, 39, x + ox, y + oy);
            }
        }
        if let (Some((x, y, w, h)), false) = (label, fonts.is_empty()) {
            // Plain green sans on every label, as in the User Guide's Project Zone shot.
            let _ = state;
            fonts.draw_centered(frame, plain_face(fonts), 12.0, (x + w / 2) as f32, (y + h - 4) as f32, (w - 8) as f32, gfx::rgb(6, 255, 2), name);
        }
    }

    pub fn draw(&self, frame: &mut Image, fonts: &Fonts, ms: &MenuState) {
        if let Some(p) = &self.preview {
            frame.pixels.fill(p.bg);
            let (x, y) = crate::slideplay::slide_origin(&self.assets);
            gfx::blit(frame, &p.mix, x, y);
            return;
        }
        match &self.bg {
            Some(bg) => gfx::blit(frame, bg, 0, 0),
            None => frame.pixels.fill(0xff10_1060),
        }
        for p in &self.statics {
            p.draw(frame);
        }
        if let Some(p) = &self.lights {
            p.draw(frame);
        }
        let top = self.project.top_left;
        let dragging_slide = self.drag.as_ref().filter(|d| d.active).and_then(|d| if let DragKind::Slide(s) = d.kind { Some(s) } else { None });
        // Slots: thumbnails or the grey "empty" placeholder.
        for k in 0..SLOTS {
            let (x, y) = slot_pos(k);
            let slot = top + k;
            match self.project.slides.get(slot).cloned().flatten() {
                Some(s) => match self.thumb_cached(&s.path) {
                    Some(t) => {
                        if t.width == 96 && t.height == 72 {
                            gfx::blit(frame, &t, x, y);
                        } else {
                            gfx::blit(frame, &gfx::resize(&t, 96, 72), x, y);
                        }
                    }
                    None => self.draw_cel(frame, 1, x, y),
                },
                // Empty slots: a thin blue outline (as in the User Guide's Project Zone shot).
                None => gfx::stroke_rect(frame, x, y, 96, 72, 2, gfx::rgb(42, 96, 218)),
            }
            // Slot number badge, top right [I] (drawn like the User Guide shows it).
            draw_slot_badge(frame, fonts, x + 96 - 23, y, slot + 1);
        }
        // Selection: the red frame (cel 3, scripts 5092+).
        if let Some(k) = self.visible_slot(self.project.selected) {
            if self.effects {
                self.draw_held(frame, 5080 + k as u16);
            } else {
                self.draw_held(frame, 5092 + k as u16);
            }
        }
        // Per-view slot controls.
        for c in &self.ctls {
            let id = c.id;
            let slotctl = (C_LOAD..C_SHOW).contains(&id);
            if slotctl && self.ctl_visible(c) {
                // The Load button only shows under the chosen slot.
                if (C_LOAD..C_LOAD + 12).contains(&id) && self.visible_slot(self.project.selected) != Some((id - C_LOAD) as usize) {
                    continue;
                }
                self.draw_ctl(frame, c);
            }
        }
        if self.effects {
            for k in 0..SLOTS {
                let Some(s) = self.project.slides.get(top + k).cloned().flatten() else { continue };
                // Transition icon in the arrow's well (cel 132).
                if let Some((_, x, y)) = Player::new(&self.assets, PICKER, 5144 + k as u16).and_then(|p| p.placed().into_iter().find(|c| c.0 == 132)) {
                    if let Some(icon) = self.assets.image(TRANS_SOUNDS, 12000 + s.settings.transition) {
                        gfx::blit(frame, &icon, x, y);
                    }
                }
                if self.project.selected == top + k {
                    self.draw_held(frame, 5132 + k as u16);
                }
                if !fonts.is_empty() {
                    let r = bounds(&self.assets, PICKER, 5106 + k as u16);
                    let text = self.timing_label(&s.settings);
                    fonts.draw_centered(frame, fonts.ui_face(), 12.0, (r.left as i32 + r.width() / 2) as f32, (r.bottom - 5) as f32, (r.width() - 4) as f32, gfx::rgb(20, 20, 90), &text);
                }
            }
        }
        if let Some(k) = self.drop_slot {
            if dragging_slide.is_some() {
                self.draw_held(frame, 5092 + k as u16);
            }
        }
        // Grid scroll arrows.
        for c in &self.ctls {
            if matches!(c.id, C_UP | C_DOWN) {
                self.draw_ctl(frame, c);
            }
        }
        // Saved slide shows strip.
        let names: Vec<String> = self.shows.iter().map(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()).collect();
        let cur = self.project.path.as_ref();
        if let Some((p, dir)) = &self.strip_anim {
            // Icons travel along the cel-1 markers of the scroll script.
            let markers: Vec<(i32, i32)> = p.placed().iter().filter(|c| c.0 == 1).map(|&(_, x, y)| (x, y)).collect();
            let first = if markers.len() > STRIP && *dir < 0 { self.strip_pos as i32 - 1 } else { self.strip_pos as i32 };
            for (m, &(mx, my)) in markers.iter().enumerate() {
                let idx = first + m as i32;
                if idx < 0 || idx as usize >= names.len() {
                    continue;
                }
                let is_cur = cur == self.shows.get(idx as usize);
                self.draw_show_icon(frame, fonts, &names[idx as usize], 0, 0, Some((mx, my)), is_cur);
            }
        } else {
            for k in 0..STRIP {
                let idx = self.strip_pos + k;
                let Some(name) = names.get(idx) else { break };
                let state = self.ctls.iter().find(|c| c.id == C_SHOW + k as u16).map(|c| c.state).unwrap_or(0);
                let dragged = self.drag.as_ref().is_some_and(|d| d.active && matches!(&d.kind, DragKind::Show(p) if Some(p) == self.shows.get(idx)));
                if dragged {
                    continue;
                }
                self.draw_show_icon(frame, fonts, name, k, state, None, cur == self.shows.get(idx));
            }
        }
        // The strip's ends (icons scroll under them), then its arrows.
        self.draw_held(frame, 5229);
        self.draw_held(frame, 5230);
        for c in &self.ctls {
            if matches!(c.id, C_LEFT | C_RIGHT) {
                self.draw_ctl(frame, c);
            }
        }
        // Characters at rest or playing their idle animation.
        for (i, p) in self.chars_rest.iter().enumerate() {
            match &self.idle {
                Some((who, a)) if *who == i => a.draw(frame),
                _ => p.draw(frame),
            }
        }
        // MBTN 888 buttons.
        for c in &self.ctls {
            if c.id >= C_UP {
                continue;
            }
            if c.id == TRASH {
                if let Some(p) = &self.trash_anim {
                    p.draw(frame);
                    continue;
                }
            }
            if c.id == EFFECTS && self.effects && c.state == 0 {
                if let Some(p) = &self.effects_on {
                    p.draw(frame);
                    continue;
                }
            }
            self.draw_ctl(frame, c);
        }
        // Texts: slide-show title and the selected slide's name.
        {
            // Plain green sans (Arial) as in the User Guide's Project Zone screenshot.
            let face = fonts.find("Arial").or_else(|| fonts.find("Liberation Sans")).unwrap_or(crate::ui::UI_BITMAP);
            const TEXT: u32 = 0xff06_ff02;
            let title = self.project.name().unwrap_or_else(|| self.untitled.clone());
            let r = bounds(&self.assets, PICKER, 5225);
            fonts.draw_centered(frame, face, 16.0, (r.left as i32 + r.width() / 2) as f32, (r.bottom - 3) as f32, (r.width() - 8) as f32, TEXT, &title);
            let name = match self.project.slides.get(self.project.selected).cloned().flatten() {
                Some(s) => s.name(),
                None => "Empty Slide".to_string(),
            };
            let r = bounds(&self.assets, PICKER, 5227);
            fonts.draw_centered(frame, face, 15.0, (r.left as i32 + r.width() / 2) as f32, (r.bottom - 5) as f32, (r.width() - 8) as f32, TEXT, &name);
        }
        // The dragged slide / slide-show icon follows the pointer.
        if let Some(d) = self.drag.as_ref().filter(|d| d.active) {
            let (x, y) = self.mouse;
            match &d.kind {
                DragKind::Slide(s) => {
                    if let Some(t) = self.project.slides.get(*s).cloned().flatten().and_then(|s| self.thumb_cached(&s.path)) {
                        let mut g = (*t).clone();
                        for p in g.pixels.iter_mut() {
                            *p = (*p & 0x00ff_ffff) | 0xc000_0000;
                        }
                        gfx::blit(frame, &g, x - 48, y - 36);
                    }
                }
                DragKind::Show(_) => self.draw_cel(frame, 38, x - 43, y - 25),
            }
        }
        if let Some(m) = &self.modal {
            self.draw_modal(frame, fonts, m);
        }
        if let Some(p) = &self.about {
            p.draw(frame);
        }
        self.menubar.draw(frame, fonts, ms);
    }

    fn draw_modal(&self, frame: &mut Image, fonts: &Fonts, m: &Modal) {
        m.dlg.draw(frame, fonts);
        match &m.kind {
            Kind::Trans { tr, snd, ttop, stop, hover, .. } => {
                for c in &m.ctls {
                    self.draw_ctl_any(frame, c);
                }
                // Transition icons in the 2×6 grid.
                let wells: Vec<(i32, i32, i32, i32)> = Player::new(&self.assets, TRANS_DLG, 7002)
                    .map(|p| p.placed().iter().filter_map(|&(c, x, y)| p.sprites.cel(c).map(|i| (x, y, i.width as i32, i.height as i32))).collect())
                    .unwrap_or_default();
                for (j, &(x, y, w, h)) in wells.iter().enumerate() {
                    let idx = ttop * 6 + j;
                    if idx >= slideshow::TRANSITIONS {
                        continue;
                    }
                    if let Some(icon) = self.assets.image(TRANS_SOUNDS, 12000 + idx as u16) {
                        gfx::blit(frame, &icon, x + (w - icon.width as i32) / 2, y + (h - icon.height as i32) / 2);
                    }
                    if idx == *tr {
                        if let Some(p) = held(&self.assets, TRANS_DLG, 7050 + j as u16) {
                            p.draw(frame);
                        }
                    } else if *hover == Some((false, j)) {
                        if let Some(p) = held(&self.assets, TRANS_DLG, 7034 + j as u16) {
                            p.draw(frame);
                        }
                    }
                }
                let wells: Vec<(i32, i32, i32, i32)> = Player::new(&self.assets, TRANS_DLG, 7012)
                    .map(|p| p.placed().iter().filter_map(|&(c, x, y)| p.sprites.cel(c).map(|i| (x, y, i.width as i32, i.height as i32))).collect())
                    .unwrap_or_default();
                for (j, &(x, y, w, h)) in wells.iter().enumerate() {
                    let idx = stop * 4 + j;
                    if idx >= slideshow::TRANS_SOUND_COUNT {
                        continue;
                    }
                    if let Some(icon) = self.assets.image(TRANS_SOUNDS, 12030 + idx as u16) {
                        gfx::blit(frame, &icon, x + (w - icon.width as i32) / 2, y + (h - icon.height as i32) / 2);
                    }
                    if idx == *snd {
                        if let Some(p) = held(&self.assets, TRANS_DLG, 7062 + j as u16) {
                            p.draw(frame);
                        }
                    } else if *hover == Some((true, j)) {
                        if let Some(p) = held(&self.assets, TRANS_DLG, 7046 + j as u16) {
                            p.draw(frame);
                        }
                    }
                }
                if !fonts.is_empty() {
                    // Blue plain sans names (User Guide screenshot of this dialog).
                    let face = plain_face(fonts);
                    let names = [self.trans_names.get(*tr).cloned().unwrap_or_default(), self.sound_names.get(*snd).cloned().unwrap_or_default()];
                    for (r, n) in m.fields.iter().zip(names.iter()) {
                        fonts.draw_centered(frame, face, 15.0, (r.left as i32 + r.width() / 2) as f32, (r.bottom - 6) as f32, (r.width() - 8) as f32, gfx::rgb(32, 32, 255), n);
                    }
                }
            }
            Kind::BgColor { sel, chip } => {
                gfx::fill_rect(frame, chip.left as i32, chip.top as i32, chip.width(), chip.height(), slideshow::bg_color(*sel));
            }
            _ => {}
        }
    }

    fn draw_ctl_any(&self, frame: &mut Image, c: &Ctl) {
        if let Art::Script(p) = &c.art[c.state as usize] {
            p.draw(frame);
        } else if let Art::Script(p) = &c.art[0] {
            p.draw(frame);
        }
    }

    pub fn cursor_hint(&self) -> CursorHint {
        let (x, y) = self.mouse;
        if self.menubar.hot(y) && !self.menubar.shown {
            return CursorHint::Drop;
        }
        if self.menubar.shown || self.busy() {
            return CursorHint::Cur(28);
        }
        if self.drag.as_ref().is_some_and(|d| d.active) || self.hover.is_some() || self.slot_at(x, y).is_some() {
            return CursorHint::Cur(14);
        }
        CursorHint::Cur(28)
    }

    // ------------------------------------------------------------------ test hooks

    /// Headless tests: open one of the dialogs for a slot.
    pub fn test_open(&mut self, which: &str, slot: usize) {
        match which {
            "trans" => self.open_trans(slot),
            "timer" => self.open_timer(slot),
            "audio" => self.open_audio(slot),
            "bgcolor" => self.open_bg_color(),
            "delete" => self.confirm_delete_slide(slot),
            _ => {}
        }
    }

    pub fn test_close(&mut self) {
        self.modal = None;
        self.preview = None;
    }

    /// Centre of visible slot `k` (tests).
    pub fn slot_center(k: usize) -> (i32, i32) {
        let (x, y) = slot_pos(k);
        (x + 48, y + 36)
    }
}

/// The numbered badge on a slot's top-right corner: green bevelled square, navy face,
/// white number in the UI font.
fn draw_slot_badge(frame: &mut Image, fonts: &Fonts, x: i32, y: i32, n: usize) {
    let (w, h) = (22, 22);
    gfx::fill_rect(frame, x, y, w, h, gfx::rgb(0, 104, 9));
    gfx::fill_rect(frame, x, y, w - 1, h - 1, gfx::rgb(192, 255, 192));
    gfx::fill_rect(frame, x + 1, y + 1, w - 2, h - 2, gfx::rgb(0, 255, 64));
    gfx::fill_rect(frame, x + 3, y + 3, w - 6, h - 6, gfx::rgb(0, 0, 206));
    let t = n.to_string();
    let tw = fonts.measure(crate::ui::UI_BITMAP, 0.0, &t);
    fonts.draw(frame, crate::ui::UI_BITMAP, 0.0, x as f32 + (w as f32 - tw) / 2.0, (y + 15) as f32, gfx::rgb(255, 255, 255), &t);
}

/// The plain system sans the original draws its dynamic texts with (Arial), or the
/// metric-compatible Liberation Sans, else the built-in UI font.
fn plain_face(fonts: &Fonts) -> usize {
    fonts.find("Arial").or_else(|| fonts.find("Liberation Sans")).unwrap_or(crate::ui::UI_BITMAP)
}
