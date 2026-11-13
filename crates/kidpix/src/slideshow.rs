//! Slide Show data (docs/RE_SHELL.md §9): the `.kpp` slide-show file (§9.2), the pages it
//! points at (`.kpx` thumbnail / composite / sound / text, §8), the background colour
//! table (§9.6) and the transition renderer (§9.5). The organizer screen is `picker.rs`,
//! the player `slideplay.rs`.

use crate::gfx::{self, Image};
use mohawk::Sound;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Transition names / icons / sounds (`STRL 7500` / `STRL 8500`, `tBMP 12000+i`, `tWAV 12000+i`).
pub const TRANS_SOUNDS: &str = "Data/Soundfx/TransitionSounds.MHK";
/// Number of transitions (STRL 7500).
pub const TRANSITIONS: usize = 30;
/// Number of transition sounds (STRL 8500); the last one is "No Sound".
pub const TRANS_SOUND_COUNT: usize = 31;
pub const NO_SOUND: u16 = 30;
/// [I] QuickTime's default effect duration is not in the .qfx files; one second.
pub const TRANSITION_MS: f32 = 1000.0;
/// Slide geometry in the player (`[CanvasSettings] slideLeft/slideTop`, 600×452).
pub const SLIDE_W: u32 = 600;
pub const SLIDE_H: u32 = 452;
pub const THUMB_W: u32 = 96;
pub const THUMB_H: u32 = 72;

/// Read a Mohawk `STRL` string list (count byte, then NUL-terminated strings).
pub fn parse_strl(data: &[u8]) -> Vec<String> {
    let Some((&n, rest)) = data.split_first() else {
        return vec![];
    };
    rest.split(|&b| b == 0).take(n as usize).map(mohawk::latin1).collect()
}

/// `STRL id` of an archive.
pub fn strl(assets: &crate::assets::Assets, archive: &str, id: u16) -> Vec<String> {
    assets
        .archive(archive)
        .and_then(|a| a.get(mohawk::Tag(*b"STRL"), id).map(parse_strl))
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------------- .kpp

/// Per-slide settings string `"%u,%u,%u,%u,%u,%u,%u"` (§9.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlideSettings {
    /// Audio Options "Play Sound" (+0xe).
    pub play_sound: u16,
    /// Audio Options "Play Text" (+0x10).
    pub play_text: u16,
    /// Timer seconds 0–255 (+0x16).
    pub seconds: u8,
    /// Advance mode: 0 Seconds, 1 Mouse Click, 2 Key Down, 3 Read Text Done (+0x17).
    pub advance: u8,
    /// Transition index 0–29 (+0x12).
    pub transition: u16,
    /// Transition sound index 0–30, 30 = No Sound (+0x14).
    pub sound: u16,
    /// "Override" flag (+0x18): ≠0 → the attached sound / text are not played [inf].
    pub overrides: u16,
}

impl Default for SlideSettings {
    fn default() -> Self {
        SlideSettings { play_sound: 1, play_text: 1, seconds: 7, advance: 0, transition: 4, sound: 4, overrides: 0 }
    }
}

impl SlideSettings {
    pub fn parse(fields: &[&str]) -> SlideSettings {
        let d = SlideSettings::default();
        let num = |i: usize| fields.get(i).and_then(|s| s.trim().parse::<u32>().ok());
        let mut s = SlideSettings {
            play_sound: num(0).map(|v| v as u16).unwrap_or(d.play_sound),
            play_text: num(1).map(|v| v as u16).unwrap_or(d.play_text),
            seconds: num(2).map(|v| v.min(255) as u8).unwrap_or(d.seconds),
            advance: num(3).map(|v| v.min(255) as u8).unwrap_or(d.advance),
            transition: num(4).map(|v| v as u16).unwrap_or(d.transition),
            sound: num(5).map(|v| v as u16).unwrap_or(d.sound),
            overrides: num(6).map(|v| v as u16).unwrap_or(d.overrides),
        };
        if s.advance >= 5 {
            s.advance = 0;
        }
        if s.transition as usize >= TRANSITIONS {
            s.transition = d.transition;
        }
        if s.sound as usize >= TRANS_SOUND_COUNT {
            s.sound = NO_SOUND;
        }
        s
    }

    pub fn format(&self) -> String {
        format!(
            "{},{},{},{},{},{},{}",
            self.play_sound, self.play_text, self.seconds, self.advance, self.transition, self.sound, self.overrides
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SlideRef {
    /// Absolute path of the picture (`.kpx`, or any picture the engine can read).
    pub path: PathBuf,
    pub settings: SlideSettings,
}

impl SlideRef {
    pub fn new(path: PathBuf) -> SlideRef {
        SlideRef { path, settings: SlideSettings::default() }
    }

    /// Name shown in the Picker's slide-name field (the file name without extension).
    pub fn name(&self) -> String {
        self.path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
    }
}

/// A slide show: `[PortfolioSettings] MaxProjectSize` slots, some of them empty.
#[derive(Clone, Debug)]
pub struct Project {
    pub slides: Vec<Option<SlideRef>>,
    /// Index into the background colour table (§9.6, default 14 Black).
    pub bg_color: usize,
    pub bg_sound: i32,
    pub looping: bool,
    pub manual_advance: bool,
    /// Picker scroll position (first visible slot) and selected slot, 0-based [I].
    pub top_left: usize,
    pub selected: usize,
    /// The file it was loaded from / saved to.
    pub path: Option<PathBuf>,
    pub dirty: bool,
}

pub const DEFAULT_BG: usize = 14;

impl Project {
    pub fn new(max: usize) -> Project {
        Project {
            slides: vec![None; max.max(1)],
            bg_color: DEFAULT_BG,
            bg_sound: 0,
            looping: false,
            manual_advance: false,
            top_left: 0,
            selected: 0,
            path: None,
            dirty: false,
        }
    }

    pub fn filled(&self) -> usize {
        self.slides.iter().filter(|s| s.is_some()).count()
    }

    pub fn name(&self) -> Option<String> {
        self.path.as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().into_owned())
    }

    /// Parse `.kpp` text; `dir` = the folder of the `.kpp` (page paths are relative to it).
    pub fn parse(text: &str, dir: &Path, max: usize) -> Project {
        let ini = crate::assets::Ini::parse(text);
        let mut p = Project::new(max);
        if let Some(pages) = ini.sections.get("pages") {
            for (k, v) in pages {
                let Some(n) = k.strip_prefix("page").and_then(|n| n.parse::<usize>().ok()) else { continue };
                if n == 0 || n > p.slides.len() {
                    continue;
                }
                let mut parts = v.split(',');
                let path = parts.next().unwrap_or("").trim();
                if path.is_empty() {
                    continue;
                }
                let fields: Vec<&str> = parts.collect();
                p.slides[n - 1] = Some(SlideRef { path: resolve_path(path, dir), settings: SlideSettings::parse(&fields) });
            }
        }
        let num = |sec: &str, key: &str| ini.get_i32(sec, key);
        p.bg_color = num("SlideShow", "BackgroundColor").filter(|v| (0..=20).contains(v)).map(|v| v as usize).unwrap_or(DEFAULT_BG);
        p.bg_sound = num("SlideShow", "BackgroundSound").unwrap_or(0);
        p.looping = num("SlideShow", "Looping").is_some_and(|v| v != 0);
        p.manual_advance = num("SlideShow", "ManualAdvance").is_some_and(|v| v != 0);
        let last = p.slides.len().saturating_sub(1);
        p.top_left = num("PickerDisplay", "TopLeftPage").map(|v| (v.max(0) as usize).min(last)).unwrap_or(0);
        p.top_left -= p.top_left % 4;
        p.selected = num("PickerDisplay", "SelectedPage").map(|v| (v.max(0) as usize).min(last)).unwrap_or(0);
        p
    }

    pub fn load(path: &Path, max: usize) -> Result<Project, String> {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        let text: String = bytes.iter().map(|&c| c as char).collect();
        let full = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
        let dir = full.parent().unwrap_or(Path::new("."));
        let mut p = Project::parse(&text, dir, max);
        p.path = Some(path.to_path_buf());
        Ok(p)
    }

    /// `.kpp` text for a file saved in `dir` (INI, CRLF line ends like the Windows original).
    pub fn to_kpp(&self, dir: &Path) -> String {
        let mut out = String::new();
        let mut line = |s: String| {
            out.push_str(&s);
            out.push_str("\r\n");
        };
        line("[Pages]".into());
        let high = self.slides.iter().rposition(|s| s.is_some()).map(|i| i + 1).unwrap_or(0);
        line(format!("Page_highnumber={high}"));
        for (i, s) in self.slides.iter().enumerate() {
            if let Some(s) = s {
                let rel = relative_path(&s.path, dir).replace(',', "_");
                line(format!("Page{}={},{}", i + 1, rel, s.settings.format()));
            }
        }
        line(String::new());
        line("[SlideShow]".into());
        line(format!("BackgroundColor={}", self.bg_color));
        line(format!("BackgroundSound={}", self.bg_sound));
        line(format!("Looping={}", self.looping as u8));
        line(format!("ManualAdvance={}", self.manual_advance as u8));
        line(String::new());
        line("[PickerDisplay]".into());
        line(format!("TopLeftPage={}", self.top_left));
        line(format!("SelectedPage={}", self.selected));
        out
    }

    pub fn save(&mut self, path: &Path) -> Result<(), String> {
        let dir = path.parent().unwrap_or(Path::new("."));
        std::fs::write(path, self.to_kpp(dir)).map_err(|e| e.to_string())?;
        self.path = Some(path.to_path_buf());
        self.dirty = false;
        Ok(())
    }

    /// Compact the show: filled slots move up in order (Controls > Clean Up) [inf].
    pub fn clean_up(&mut self) {
        let max = self.slides.len();
        let mut v: Vec<Option<SlideRef>> = self.slides.drain(..).filter(|s| s.is_some()).collect();
        v.resize(max, None);
        self.slides = v;
    }

    /// Move the slide in `from` to `to`, shifting the slides in between (drag & drop).
    pub fn move_slide(&mut self, from: usize, to: usize) {
        if from == to || from >= self.slides.len() || to >= self.slides.len() {
            return;
        }
        if self.slides[to].is_none() {
            self.slides.swap(from, to);
            return;
        }
        let s = self.slides.remove(from);
        self.slides.insert(to, s);
    }
}

/// `target` relative to `dir` with Windows separators (`..\Cat.kpx`); the absolute path
/// when they share no folder (another drive).
pub fn relative_path(target: &Path, dir: &Path) -> String {
    use std::path::Component;
    let target = std::path::absolute(target).unwrap_or_else(|_| target.to_path_buf());
    let dir = std::path::absolute(dir).unwrap_or_else(|_| dir.to_path_buf());
    let t: Vec<Component> = target.components().collect();
    let d: Vec<Component> = dir.components().collect();
    let common = t.iter().zip(d.iter()).take_while(|(a, b)| a == b).count();
    // Relative only when the two share a folder (not just the root / drive).
    let shared_dirs = t[..common].iter().filter(|c| matches!(c, Component::Normal(_))).count();
    if shared_dirs == 0 {
        return target.to_string_lossy().into_owned();
    }
    let mut parts: Vec<String> = Vec::new();
    for _ in common..d.len() {
        parts.push("..".into());
    }
    for c in &t[common..] {
        parts.push(c.as_os_str().to_string_lossy().into_owned());
    }
    parts.join("\\")
}

/// A page path from a `.kpp` (relative to `dir`, either separator) as a real path.
pub fn resolve_path(s: &str, dir: &Path) -> PathBuf {
    let norm = s.replace('\\', "/");
    let p = Path::new(&norm);
    let is_abs = p.is_absolute() || norm.as_bytes().get(1) == Some(&b':');
    let full = if is_abs { PathBuf::from(&norm) } else { dir.join(p) };
    // Tidy `..` so the path displays nicely.
    let mut out = PathBuf::new();
    for c in full.components() {
        match c {
            std::path::Component::ParentDir if out.file_name().is_some() => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    if out.exists() {
        return out;
    }
    // Case-insensitive lookup (files made on Windows).
    crate::assets::resolve_ci(Path::new("/"), &out.to_string_lossy()).filter(|_| out.is_absolute()).unwrap_or(out)
}

// ---------------------------------------------------------------------------------- pages

/// What the slide show needs from a page file.
pub struct Page {
    pub composite: Image,
    pub sound: Option<Arc<Sound>>,
    pub texts: Vec<String>,
}

fn is_kpx(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("kpx"))
}

fn flatten_white(mut img: Image) -> Image {
    for p in img.pixels.iter_mut() {
        *p = gfx::blend(0xffff_ffff, *p);
    }
    img
}

fn other_picture(path: &Path) -> Option<Image> {
    let img = crate::formats::load_image(path).ok()?;
    Some(flatten_white(img))
}

/// The slot thumbnail: the page's `tTHM 1` (96×72); other pictures are scaled.
pub fn page_thumb(path: &Path) -> Option<Image> {
    if is_kpx(path) {
        let a = mohawk::Archive::open(path).ok()?;
        if let Some(img) = a.get(crate::assets::T_THM, 1).and_then(|d| mohawk::decode_bitmap(d, None).ok()) {
            return Some(img);
        }
        let id = if a.has(crate::assets::T_BMP, 2) { 2 } else { *a.ids(crate::assets::T_BMP).first()? };
        let img = mohawk::decode_bitmap(a.get(crate::assets::T_BMP, id)?, None).ok()?;
        return Some(gfx::resize(&img, THUMB_W, THUMB_H));
    }
    other_picture(path).map(|i| gfx::resize(&i, THUMB_W, THUMB_H))
}

/// Text of a `tTXT` text-box object (0x38-byte header, then Latin-1 text, §8.6).
pub fn parse_ttxt(data: &[u8]) -> Option<String> {
    let len = u32::from_be_bytes(data.get(0..4)?.try_into().ok()?) as usize;
    let body = data.get(0x38..0x38 + len.min(data.len().saturating_sub(0x38)))?;
    let s = mohawk::latin1(body).replace('\r', "\n");
    (!s.trim().is_empty()).then_some(s)
}

/// The slide image (`tBMP 2` composite, else `tBMP 1`), the page sound and its text boxes.
pub fn load_page(path: &Path) -> Option<Page> {
    if is_kpx(path) {
        let a = mohawk::Archive::open(path).ok()?;
        let bmp = crate::assets::T_BMP;
        let id = if a.has(bmp, 2) { 2 } else { *a.ids(bmp).first()? };
        let mut img = mohawk::decode_bitmap(a.get(bmp, id)?, None).ok()?;
        if img.width != SLIDE_W || img.height != SLIDE_H {
            img = gfx::resize(&img, SLIDE_W, SLIDE_H);
        }
        let sound = a
            .ids(crate::assets::T_WAV)
            .first()
            .and_then(|&i| a.get(crate::assets::T_WAV, i))
            .and_then(|d| mohawk::decode_sound(d).ok())
            .map(Arc::new);
        let tag = mohawk::Tag(*b"tTXT");
        let texts = a.ids(tag).iter().filter_map(|&i| a.get(tag, i).and_then(parse_ttxt)).collect();
        return Some(Page { composite: flatten_white(img), sound, texts });
    }
    let img = other_picture(path)?;
    let img = if img.width != SLIDE_W || img.height != SLIDE_H { gfx::resize(&img, SLIDE_W, SLIDE_H) } else { img };
    Some(Page { composite: img, sound: None, texts: Vec::new() })
}

// ---------------------------------------------------------------------------------- colours

/// Background colour table (§9.6): name and the swatch colour of RPickBgColorDlg.
/// [I] Playback uses the swatch RGB (what the child picked); the code's percent table only
/// differs slightly, except its Yellow entry which would come out white.
pub const BG_COLORS: [(&str, [u8; 3]); 21] = [
    ("White", [238, 238, 238]),
    ("Pink", [255, 156, 156]),
    ("Orange", [255, 205, 156]),
    ("Yellow", [255, 255, 156]),
    ("Green", [156, 255, 156]),
    ("Blue", [156, 255, 255]),
    ("Purple", [205, 205, 255]),
    ("Grey", [115, 115, 115]),
    ("Red", [255, 0, 0]),
    ("Orange2", [255, 98, 49]),
    ("Yellow2", [255, 255, 0]),
    ("Green2", [0, 172, 0]),
    ("Blue2", [0, 98, 255]),
    ("Blue3", [98, 0, 255]),
    ("Black", [0, 0, 0]),
    ("Red", [172, 0, 0]),
    ("Brown", [131, 49, 0]),
    ("Brown2", [123, 131, 0]),
    ("Green3", [0, 82, 0]),
    ("Blue4", [0, 0, 238]),
    ("Blue5", [49, 0, 205]),
];

pub fn bg_color(idx: usize) -> u32 {
    let [r, g, b] = BG_COLORS.get(idx).unwrap_or(&BG_COLORS[DEFAULT_BG]).1;
    gfx::rgb(r, g, b)
}

// ---------------------------------------------------------------------------------- transitions

/// Arrival time of the new picture per pixel (0..1) for the wipe-style effects, cached
/// per (effect, size) because some shapes (heart, star) are costly to evaluate.
type Field = std::rc::Rc<Vec<f32>>;

thread_local! {
    static FIELDS: RefCell<HashMap<(usize, u32, u32), Option<Field>>> = RefCell::new(HashMap::new());
}

fn field(kind: usize, w: u32, h: u32) -> Option<Field> {
    FIELDS.with(|f| {
        f.borrow_mut()
            .entry((kind, w, h))
            .or_insert_with(|| build_field(kind, w, h).map(std::rc::Rc::new))
            .clone()
    })
}

/// Softness (in time units) of the soft-edged QuickTime wipes ("soft" in the .qfx).
fn softness(kind: usize) -> f32 {
    match kind {
        6 | 7 | 8 | 9 | 11 | 12 | 20 => 0.06,
        _ => 0.0,
    }
}

/// Unit 5-point star "radius" at angle `a` (0 = up, clockwise): the distance from the
/// centre to the star outline along that ray, outer radius 1.
fn star_radius(a: f32) -> f32 {
    use std::f32::consts::TAU;
    let inner = 0.382;
    let seg = TAU / 10.0;
    let a = a.rem_euclid(TAU);
    let k = (a / seg).floor();
    let a0 = k * seg;
    let (r0, r1) = if k as i32 % 2 == 0 { (1.0, inner) } else { (inner, 1.0) };
    // Intersect the ray with the segment between the two vertices.
    let p0 = (r0 * libm::sinf(a0), r0 * libm::cosf(a0));
    let p1 = (r1 * libm::sinf(a0 + seg), r1 * libm::cosf(a0 + seg));
    let d = (libm::sinf(a), libm::cosf(a));
    let e = (p1.0 - p0.0, p1.1 - p0.1);
    let den = d.0 * e.1 - d.1 * e.0;
    if den.abs() < 1e-6 {
        return r0;
    }
    (p0.0 * e.1 - p0.1 * e.0) / den
}

/// Smallest heart scale containing (u, v) (unit heart: (x²+y²−1)³ − x²y³ ≤ 0, y up).
fn heart_scale(u: f32, v: f32) -> f32 {
    let inside = |s: f32| {
        let (x, y) = (u / s, v / s + 0.15);
        let q = x * x + y * y - 1.0;
        q * q * q - x * x * y * y * y <= 0.0
    };
    let (mut lo, mut hi) = (0.0f32, 4.0f32);
    for _ in 0..22 {
        let m = (lo + hi) / 2.0;
        if inside(m.max(1e-4)) {
            hi = m;
        } else {
            lo = m;
        }
    }
    hi
}

fn build_field(kind: usize, w: u32, h: u32) -> Option<Vec<f32>> {
    use std::f32::consts::{PI, TAU};
    let (wf, hf) = (w as f32, h as f32);
    let (cx, cy) = (wf / 2.0, hf / 2.0);
    let mut v = vec![0f32; (w * h) as usize];
    // Matrix wipes (smp4) work on a grid of blocks.
    let (gc, gr) = (8usize, 6usize);
    let spiral: Vec<usize> = if kind == 20 {
        // Clockwise spiral from the top-left block inward.
        let mut order = vec![usize::MAX; gc * gr];
        let (mut l, mut t, mut r, mut b) = (0i32, 0i32, gc as i32 - 1, gr as i32 - 1);
        let mut n = 0;
        while l <= r && t <= b {
            for x in l..=r {
                order[(t as usize) * gc + x as usize] = n;
                n += 1;
            }
            for y in t + 1..=b {
                order[(y as usize) * gc + r as usize] = n;
                n += 1;
            }
            if t < b {
                for x in (l..r).rev() {
                    order[(b as usize) * gc + x as usize] = n;
                    n += 1;
                }
            }
            if l < r {
                for y in (t + 1..b).rev() {
                    order[(y as usize) * gc + l as usize] = n;
                    n += 1;
                }
            }
            l += 1;
            t += 1;
            r -= 1;
            b -= 1;
        }
        order
    } else {
        Vec::new()
    };
    for y in 0..h {
        for x in 0..w {
            let fx = x as f32 + 0.5;
            let fy = y as f32 + 0.5;
            let dx = fx - cx;
            let dy = fy - cy;
            let r = (dx * dx + dy * dy).sqrt();
            // 0 at 12 o'clock, clockwise.
            // libm (pure Rust) keeps the wipe shapes bit-identical on every OS
            let ang = libm::atan2f(dx, -dy).rem_euclid(TAU);
            let d = match kind {
                // Barn Door: SMPTE 21, vertical edges opening from the centre.
                0 => dx.abs() / cx,
                // Clock Face: SMPTE 201, clockwise sweep from 12 o'clock.
                1 => ang / TAU,
                // Iris Circle: 119.
                6 => r,
                // Iris Heart: 130.
                7 => heart_scale(dx / cx, -dy / cx),
                // Iris Rectangle: 101 (keeps the picture's aspect).
                8 => (dx.abs() / cx).max(dy.abs() / cy),
                // Iris Star: 128 (five points).
                9 => r / star_radius(ang).max(0.05),
                // Jaws: 74 (barnZigZagWipe horizontal), two zig-zag edges opening up and
                // down from the horizontal centre line.
                10 => {
                    let period = wf / 8.0;
                    let tri = ((fx / period).fract() - 0.5).abs() * 2.0;
                    dy.abs() + tri * hf * 0.066
                }
                // Kaleidoscope: 102 diamond iris repeated 3×3.
                11 => {
                    let (tw, th) = (wf / 3.0, hf / 3.0);
                    let ux = (fx % tw) - tw / 2.0;
                    let uy = (fy % th) - th / 2.0;
                    ux.abs() / (tw / 2.0) + uy.abs() / (th / 2.0)
                }
                // Lawnmower: 301 horizontal snake, row by row.
                12 => {
                    let rh = hf / gr as f32;
                    let row = ((fy / rh) as usize).min(gr - 1);
                    let pos = if row % 2 == 0 { fx / wf } else { 1.0 - fx / wf };
                    (row as f32 + pos) / gr as f32
                }
                // Open Sesame: 47 (miscDiagonalWipe doubleBarnDoor), four wedges split
                // along both diagonals and retract towards the four edges.
                13 => (fx / wf - fy / hf).abs().min((fx / wf + fy / hf - 1.0).abs()),
                // Spotlight: 231 (fanWipe top), a beam from the top centre that widens
                // until it covers the picture.
                21 => libm::atan2f(dx.abs(), fy) / (PI / 2.0),
                // Spiral: 310, blocks in a clockwise spiral.
                20 => {
                    let bx = ((fx / wf * gc as f32) as usize).min(gc - 1);
                    let by = ((fy / hf * gr as f32) as usize).min(gr - 1);
                    spiral[by * gc + bx] as f32 / (gc * gr) as f32
                }
                // Waterfall: 350, columns pouring down one after the other.
                22 => {
                    let cols = 16usize;
                    let col = ((fx / wf * cols as f32) as usize).min(cols - 1);
                    col as f32 / cols as f32 * 0.6 + fy / hf * 0.4
                }
                // Windmill: 207, four blades.
                23 => (ang % (TAU / 4.0)) / (TAU / 4.0),
                // Wipe Diagonal: 41, top-left to bottom-right.
                24 => fx / wf + fy / hf,
                // Wipe Down: 2.
                25 => fy / hf,
                // Wiper: 223, a blade pivoting at the bottom centre sweeping left to right.
                26 => {
                    let a = libm::atan2f(hf - fy, fx - cx); // 0 = right, PI = left
                    1.0 - a.clamp(0.0, PI) / PI
                }
                // Wipe Right: 1.
                27 => fx / wf,
                _ => return None,
            };
            v[(y * w + x) as usize] = d;
        }
    }
    let max = v.iter().cloned().filter(|d| d.is_finite()).fold(0.0f32, f32::max).max(1e-6);
    for d in v.iter_mut() {
        if !d.is_finite() {
            *d = max;
        }
    }
    // Arrival times strictly above 0 so t = 0 shows only the old picture.
    let n = v.len() as f32;
    for (i, d) in v.iter_mut().enumerate() {
        *d = (*d / max).clamp(0.0, 1.0) * 0.999 + 0.001 * (i as f32 / n);
    }
    Some(v)
}

#[inline]
fn sample(img: &Image, x: f32, y: f32) -> u32 {
    let xi = (x.floor() as i32).clamp(0, img.width as i32 - 1);
    let yi = (y.floor() as i32).clamp(0, img.height as i32 - 1);
    img.pixels[(yi as u32 * img.width + xi as u32) as usize]
}

/// Draw `b` scaled into the rect (l,t,r,b) over `a` (outside the rect `a` stays).
fn grow_rect(a: &Image, b: &Image, out: &mut Image, rect: (f32, f32, f32, f32), inner_is_b: bool) {
    let (w, h) = (out.width as i32, out.height as i32);
    let (l, t, r, bt) = rect;
    let (rw, rh) = ((r - l).max(1e-3), (bt - t).max(1e-3));
    for y in 0..h {
        for x in 0..w {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let inside = fx >= l && fx < r && fy >= t && fy < bt;
            let (scaled, other) = if inner_is_b { (b, a) } else { (a, b) };
            let px = if inside {
                sample(scaled, (fx - l) / rw * w as f32, (fy - t) / rh * h as f32)
            } else {
                other.pixels[(y * w + x) as usize]
            };
            out.pixels[(y * w + x) as usize] = px;
        }
    }
}

/// QuickTime explode ('xplo'): the new picture grows out of point `o` (normalised, the
/// picture spans −1..1 in both axes); implode ('mplo') shrinks the old one into it.
fn explode(a: &Image, b: &Image, out: &mut Image, t: f32, o: (f32, f32), implode: bool) {
    let (w, h) = (out.width as f32, out.height as f32);
    let px = |nx: f32| (nx + 1.0) / 2.0 * w;
    let py = |ny: f32| (ny + 1.0) / 2.0 * h;
    let (ox, oy) = (px(o.0), py(o.1));
    let s = if implode { 1.0 - t } else { t };
    let rect = (ox + (0.0 - ox) * s, oy + (0.0 - oy) * s, ox + (w - ox) * s, oy + (h - oy) * s);
    grow_rect(a, b, out, rect, !implode);
}

/// Composite transition `kind` (STRL 7500 order, §9.5) at progress `t` (0..1) from `a` to
/// `b` into `out` (all the same size).
pub fn transition(kind: usize, t: f32, a: &Image, b: &Image, out: &mut Image) {
    let t = t.clamp(0.0, 1.0);
    let (w, h) = (a.width, a.height);
    if out.width != w || out.height != h {
        *out = Image::new(w, h);
    }
    let b_owned;
    let b = if b.width != w || b.height != h {
        b_owned = gfx::resize(b, w, h);
        &b_owned
    } else {
        b
    };
    if t >= 1.0 {
        out.pixels.copy_from_slice(&b.pixels);
        return;
    }
    let (wi, hi) = (w as i32, h as i32);
    let get = |img: &Image, x: i32, y: i32| img.pixels[(y.clamp(0, hi - 1) * wi + x.clamp(0, wi - 1)) as usize];
    if let Some(f) = field(kind, w, h) {
        let s = softness(kind);
        for (i, px) in out.pixels.iter_mut().enumerate() {
            let d = f[i];
            let k = if s > 0.0 { ((t * (1.0 + s) - d) / s).clamp(0.0, 1.0) } else if t >= d { 1.0 } else { 0.0 };
            *px = if k <= 0.0 {
                a.pixels[i]
            } else if k >= 1.0 {
                b.pixels[i]
            } else {
                gfx::lerp(a.pixels[i], b.pixels[i], (k * 255.0) as u32)
            };
        }
        return;
    }
    // QuickTime tweens the effect percentage ('pcnt' 0→1) linearly
    let te = t;
    match kind {
        // Engulf: explode whose centre moves from below the picture to above it
        // (xcnt 0, ycnt tween 2 → −2).
        2 => explode(a, b, out, te, (0.0, 2.0 - 4.0 * te), false),
        // Explode: the new picture grows out of the centre.
        3 => explode(a, b, out, te, (0.0, 0.0), false),
        // Implode: the old picture shrinks into the centre.
        5 => explode(a, b, out, te, (0.0, 0.0), true),
        // Page Tear: explode whose centre moves from beyond the top-left corner to the
        // right edge (xcnt tween −2 → 2, ycnt tween −2 → 0).
        14 => explode(a, b, out, te, (-2.0 + 4.0 * te, -2.0 + 2.0 * te), false),
        // Push: the new picture pushes the old one out to the left.
        15 => {
            let off = (te * w as f32) as i32;
            for y in 0..hi {
                for x in 0..wi {
                    let p = if x >= wi - off { get(b, x - (wi - off), y) } else { get(a, x + off, y) };
                    out.pixels[(y * wi + x) as usize] = p;
                }
            }
        }
        // Slides (QuickTime 'slid'): the new picture slides in over the old one.
        16 | 17 | 18 | 19 | 28 => {
            let offx = (te * w as f32) as i32;
            let offy = (te * h as f32) as i32;
            for y in 0..hi {
                for x in 0..wi {
                    let p = match kind {
                        16 if y < offy => get(b, x, y + hi - offy),
                        17 if x >= wi - offx => get(b, x - (wi - offx), y),
                        18 if x < offx => get(b, x + wi - offx, y),
                        // Slide Up; Wipe Up uses the same effect in the original data.
                        19 | 28 if y >= hi - offy => get(b, x, y - (hi - offy)),
                        _ => get(a, x, y),
                    };
                    out.pixels[(y * wi + x) as usize] = p;
                }
            }
        }
        // Zoom: the old picture zooms towards the viewer (up to 5×) and fades into the new.
        29 => {
            let z = 1.0 + 4.0 * te;
            let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
            for y in 0..hi {
                for x in 0..wi {
                    let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
                    let pa = sample(a, cx + (fx - cx) / z, cy + (fy - cy) / z);
                    out.pixels[(y * wi + x) as usize] = gfx::lerp(pa, get(b, x, y), (te * 255.0) as u32);
                }
            }
        }
        // Fade (QuickTime dissolve) and anything unknown.
        _ => {
            let k = (t * 255.0) as u32;
            for (i, px) in out.pixels.iter_mut().enumerate() {
                *px = gfx::lerp(a.pixels[i], b.pixels[i], k);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip() {
        let s = SlideSettings::parse(&["0", "1", "12", "2", "17", "30", "0"]);
        assert_eq!(s.format(), "0,1,12,2,17,30,0");
        assert_eq!(SlideSettings::parse(&[]), SlideSettings::default());
        assert_eq!(SlideSettings::parse(&["1", "1", "7", "9"]).advance, 0);
    }

    #[test]
    fn kpp_round_trip() {
        let dir = Path::new("/home/u/Users/Sam/My Pictures/My Slide Shows");
        let mut p = Project::new(96);
        p.slides[0] = Some(SlideRef::new(PathBuf::from("/home/u/Users/Sam/My Pictures/Cat.kpx")));
        p.slides[4] = Some(SlideRef { path: PathBuf::from("/home/u/Users/Sam/My Pictures/My Slide Shows/Dog.kpx"), settings: SlideSettings { seconds: 3, ..Default::default() } });
        p.bg_color = 3;
        let text = p.to_kpp(dir);
        assert!(text.contains("Page1=..\\Cat.kpx,1,1,7,0,4,4,0\r\n"), "{text}");
        assert!(text.contains("Page5=Dog.kpx,1,1,3,0,4,4,0"));
        assert!(text.contains("Page_highnumber=5"));
        let q = Project::parse(&text, dir, 96);
        assert_eq!(q.slides[0].as_ref().unwrap().path, PathBuf::from("/home/u/Users/Sam/My Pictures/Cat.kpx"));
        assert_eq!(q.slides[4].as_ref().unwrap().settings.seconds, 3);
        assert!(q.slides[1].is_none());
        assert_eq!(q.bg_color, 3);
    }

    #[test]
    fn transitions_cover_both_ends() {
        let a = Image::filled(60, 40, 0xffff0000);
        let b = Image::filled(60, 40, 0xff0000ff);
        let mut o = Image::new(60, 40);
        for k in 0..TRANSITIONS {
            transition(k, 0.0, &a, &b, &mut o);
            assert!(o.pixels.iter().all(|&p| p == 0xffff0000), "kind {k} at 0");
            transition(k, 1.0, &a, &b, &mut o);
            let blue = o.pixels.iter().filter(|&&p| p == 0xff0000ff).count();
            assert!(blue * 100 >= o.pixels.len() * 99, "kind {k} at 1: {blue}");
        }
    }
}

#[cfg(test)]
mod transition_tests {
    use super::*;

    /// Every transition starts on the old picture and ends exactly on the new one.
    #[test]
    fn transitions_start_and_finish() {
        let a = Image::filled(600, 452, 0xff10_2030);
        let b = Image::filled(600, 452, 0xffff_ffff);
        let mut out = Image::new(600, 452);
        for k in 0..TRANSITIONS {
            transition(k, 0.0, &a, &b, &mut out);
            assert!(out.pixels.iter().all(|&p| p == a.pixels[0]), "transition {k} at 0 %");
            transition(k, 1.0, &a, &b, &mut out);
            let left = out.pixels.iter().filter(|&&p| p != b.pixels[0]).count();
            assert_eq!(left, 0, "transition {k} at 100 % leaves {left} old pixels");
        }
    }
}
