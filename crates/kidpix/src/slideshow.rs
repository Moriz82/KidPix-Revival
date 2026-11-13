//! Slide Show: arrange pictures, pick a transition and sound for each, play them back.
//! Transition names, icons and sounds come from the CD (`GFXTransitions.MHK`,
//! `Soundfx/TransitionSounds.MHK`); the transition effects themselves are new code.

use crate::assets::{self, Assets};
use crate::audio::{self, Audio};
use crate::gfx::{self, Image};
use crate::paint::{CANVAS_H, CANVAS_W, WHITE};
use crate::ui::{self, Fonts};
use std::rc::Rc;

const GFX: &str = "Data/GFXTransitions.MHK";
const SND: &str = "Data/Soundfx/TransitionSounds.MHK";
const THUMB_W: i32 = 120;
const THUMB_H: i32 = 90;
const VISIBLE: usize = 5;
const HOLD_MS: f32 = 2600.0;
const TRANS_MS: f32 = 1300.0;

pub struct Slide {
    pub img: Image,
    thumb: Image,
    pub transition: usize,
    pub sound: Option<usize>,
}

enum Mode {
    Edit,
    Play { idx: usize, t: f32, transitioning: bool },
}

pub struct SlideShow {
    assets: Rc<Assets>,
    audio: Rc<Audio>,
    pub slides: Vec<Slide>,
    sel: usize,
    scroll: usize,
    trans_names: Vec<String>,
    trans_icons: Vec<Image>,
    sound_names: Vec<String>,
    mode: Mode,
    mouse: (i32, i32),
    pub closed: bool,
    pub want_file: bool,
}

pub fn parse_strl(data: &[u8]) -> Vec<String> {
    let Some((&n, rest)) = data.split_first() else {
        return vec![];
    };
    rest.split(|&b| b == 0)
        .take(n as usize)
        .map(mohawk::latin1)
        .collect()
}

fn strl(assets: &Assets, archive: &str) -> Vec<String> {
    let Some(a) = assets.archive(archive) else { return vec![] };
    let tag = mohawk::Tag(*b"STRL");
    // Use the list whose length matches the item count best (first one for transitions,
    // the last one in the sound archive).
    let ids = a.ids(tag);
    let id = if archive == SND { ids.last() } else { ids.first() };
    id.and_then(|&i| a.get(tag, i)).map(parse_strl).unwrap_or_default()
}

impl SlideShow {
    pub fn new(assets: Rc<Assets>, audio: Rc<Audio>) -> SlideShow {
        let mut trans_names = strl(&assets, GFX);
        if trans_names.is_empty() {
            trans_names = (1..=30).map(|i| format!("Transition {i}")).collect();
        }
        let trans_icons = (0..trans_names.len())
            .map(|i| {
                assets
                    .image(GFX, 7500 + i as u16)
                    .map(|im| gfx::resize_nearest(&im, 2))
                    .unwrap_or_else(|| Image::filled(46, 42, gfx::hsv(i as f32 * 12.0, 0.5, 0.9)))
            })
            .collect();
        let sound_names = strl(&assets, SND);
        SlideShow {
            assets,
            audio,
            slides: Vec::new(),
            sel: 0,
            scroll: 0,
            trans_names,
            trans_icons,
            sound_names,
            mode: Mode::Edit,
            mouse: (0, 0),
            closed: false,
            want_file: false,
        }
    }

    pub fn add(&mut self, img: Image) {
        let img = if img.width != CANVAS_W || img.height != CANVAS_H {
            gfx::resize(&img, CANVAS_W, CANVAS_H)
        } else {
            img
        };
        let thumb = gfx::resize(&img, THUMB_W as u32, THUMB_H as u32);
        let transition = self.slides.len() % self.trans_names.len().max(1);
        self.slides.push(Slide {
            img,
            thumb,
            transition,
            sound: None,
        });
        self.sel = self.slides.len() - 1;
        if self.sel >= self.scroll + VISIBLE {
            self.scroll = self.sel + 1 - VISIBLE;
        }
    }

    pub fn playing(&self) -> bool {
        matches!(self.mode, Mode::Play { .. })
    }

    fn play(&mut self) {
        if self.slides.is_empty() {
            return;
        }
        self.mode = Mode::Play {
            idx: 0,
            t: 0.0,
            transitioning: false,
        };
    }

    fn stop(&mut self) {
        self.mode = Mode::Edit;
        self.audio.stop_channel(audio::CH_MUSIC);
    }

    pub fn key_escape(&mut self) {
        if self.playing() {
            self.stop();
        } else {
            self.closed = true;
        }
    }

    pub fn update(&mut self, dt: f32) {
        let n = self.slides.len();
        let mut sound_to_play = None;
        if let Mode::Play { idx, t, transitioning } = &mut self.mode {
            *t += dt;
            if !*transitioning && *t >= HOLD_MS {
                if *idx + 1 >= n {
                    self.mode = Mode::Edit;
                    return;
                }
                *transitioning = true;
                *t = 0.0;
                sound_to_play = self.slides[*idx + 1].sound;
            } else if *transitioning && *t >= TRANS_MS {
                *transitioning = false;
                *idx += 1;
                *t = 0.0;
            }
        }
        if let Some(s) = sound_to_play {
            if let Some(w) = self.assets.sound(SND, 12000 + s as u16) {
                self.audio.play(w, audio::CH_MUSIC, false);
            }
        }
    }

    // ---------------------------------------------------------------- layout

    fn strip_rect(i: usize) -> (i32, i32, i32, i32) {
        (70 + i as i32 * (THUMB_W + 14), 78, THUMB_W, THUMB_H)
    }

    fn buttons() -> [(&'static str, i32, i32, i32); 7] {
        // label, x, y, w
        [
            ("Add this picture", 40, 196, 150),
            ("Add a file...", 198, 196, 112),
            ("Remove", 318, 196, 80),
            ("< Move", 406, 196, 72),
            ("Move >", 486, 196, 72),
            ("Play", 600, 196, 70),
            ("Close", 680, 196, 76),
        ]
    }

    fn trans_rect(i: usize) -> (i32, i32, i32, i32) {
        (44 + (i % 10) as i32 * 72, 262 + (i / 10) as i32 * 62, 64, 56)
    }

    fn sound_rect(i: usize) -> (i32, i32, i32, i32) {
        (44 + (i % 6) as i32 * 120, 466 + (i / 6) as i32 * 20, 116, 18)
    }

    // ---------------------------------------------------------------- input

    pub fn mouse_move(&mut self, x: i32, y: i32) {
        self.mouse = (x, y);
    }

    /// Returns an action the easel must perform (add current picture).
    pub fn mouse_down(&mut self, x: i32, y: i32) -> Option<SlideAction> {
        if self.playing() {
            self.stop();
            return None;
        }
        // Strip arrows.
        if ui::hit(x, y, 36, 110, 26, 26) {
            self.scroll = self.scroll.saturating_sub(1);
            return None;
        }
        if ui::hit(x, y, 740, 110, 26, 26) {
            if self.scroll + VISIBLE < self.slides.len() {
                self.scroll += 1;
            }
            return None;
        }
        for v in 0..VISIBLE {
            let i = self.scroll + v;
            let r = Self::strip_rect(v);
            if i < self.slides.len() && ui::hit(x, y, r.0, r.1, r.2, r.3) {
                self.sel = i;
                return None;
            }
        }
        for (k, (_, bx, by, bw)) in Self::buttons().iter().enumerate() {
            if !ui::hit(x, y, *bx, *by, *bw, 30) {
                continue;
            }
            match k {
                0 => return Some(SlideAction::AddCurrent),
                1 => {
                    self.want_file = true;
                }
                2 => {
                    if self.sel < self.slides.len() {
                        self.slides.remove(self.sel);
                        self.sel = self.sel.min(self.slides.len().saturating_sub(1));
                        self.scroll = self.scroll.min(self.slides.len().saturating_sub(VISIBLE));
                    }
                }
                3 => {
                    if self.sel > 0 && self.sel < self.slides.len() {
                        self.slides.swap(self.sel, self.sel - 1);
                        self.sel -= 1;
                    }
                }
                4 => {
                    if self.sel + 1 < self.slides.len() {
                        self.slides.swap(self.sel, self.sel + 1);
                        self.sel += 1;
                    }
                }
                5 => self.play(),
                _ => self.closed = true,
            }
            return None;
        }
        if self.sel < self.slides.len() {
            for i in 0..self.trans_names.len() {
                let r = Self::trans_rect(i);
                if ui::hit(x, y, r.0, r.1, r.2, r.3) {
                    self.slides[self.sel].transition = i;
                    return None;
                }
            }
            for i in 0..self.sound_names.len() {
                let r = Self::sound_rect(i);
                if ui::hit(x, y, r.0, r.1, r.2, r.3) {
                    let none = self.sound_names[i].to_lowercase().contains("no sound");
                    self.slides[self.sel].sound = if none { None } else { Some(i) };
                    if !none {
                        if let Some(w) = self.assets.sound(SND, 12000 + i as u16) {
                            self.audio.play(w, audio::CH_MUSIC, false);
                        }
                    }
                    return None;
                }
            }
        }
        None
    }

    // ---------------------------------------------------------------- draw

    pub fn draw(&self, frame: &mut Image, fonts: &Fonts) {
        if let Mode::Play { idx, t, transitioning } = &self.mode {
            frame.pixels.fill(0xff00_0000);
            let (sx, sy) = (100, 74);
            if *transitioning && idx + 1 < self.slides.len() {
                let next = &self.slides[idx + 1];
                let mut out = Image::new(CANVAS_W, CANVAS_H);
                transition(next.transition, (*t / TRANS_MS).min(1.0), &self.slides[*idx].img, &next.img, &mut out);
                gfx::blit(frame, &out, sx, sy);
            } else {
                gfx::blit(frame, &self.slides[*idx].img, sx, sy);
            }
            return;
        }
        // Dim the easel and draw the editor panel.
        for p in frame.pixels.iter_mut() {
            *p = gfx::lerp(*p, 0xff10_1040, 150);
        }
        ui::panel(frame, 20, 28, 760, 566);
        let face = fonts.ui_face();
        let ink = gfx::rgb(30, 30, 110);
        if !fonts.is_empty() {
            fonts.draw(frame, face, 24.0, 44.0, 64.0, ink, "Slide Show");
            let info = format!("{} slide{}", self.slides.len(), if self.slides.len() == 1 { "" } else { "s" });
            fonts.draw(frame, face, 15.0, 200.0, 62.0, gfx::rgb(80, 80, 120), &info);
        }
        ui::arrow_button(frame, 36, 110, 26, -1, false, self.scroll > 0);
        ui::arrow_button(frame, 740, 110, 26, 1, false, self.scroll + VISIBLE < self.slides.len());
        for v in 0..VISIBLE {
            let i = self.scroll + v;
            let r = Self::strip_rect(v);
            if let Some(s) = self.slides.get(i) {
                gfx::fill_rect(frame, r.0 - 3, r.1 - 3, r.2 + 6, r.3 + 6, if i == self.sel { gfx::rgb(230, 40, 40) } else { gfx::rgb(150, 150, 200) });
                gfx::blit(frame, &s.thumb, r.0, r.1);
                if !fonts.is_empty() {
                    fonts.draw(frame, face, 13.0, r.0 as f32, (r.1 + r.3 + 14) as f32, ink, &format!("{}", i + 1));
                }
            } else {
                gfx::stroke_rect(frame, r.0, r.1, r.2, r.3, 1, gfx::rgb(180, 180, 210));
            }
        }
        for (label, bx, by, bw) in Self::buttons() {
            let hover = ui::hit(self.mouse.0, self.mouse.1, bx, by, bw, 30);
            gfx::fill_round_rect(frame, bx, by, bw, 30, 12, if hover { gfx::rgb(90, 120, 240) } else { gfx::rgb(60, 80, 200) });
            if !fonts.is_empty() {
                fonts.draw_centered(frame, face, 15.0, (bx + bw / 2) as f32, (by + 21) as f32, (bw - 8) as f32, WHITE, label);
            }
        }
        let cur = self.slides.get(self.sel);
        if !fonts.is_empty() {
            fonts.draw(frame, face, 16.0, 44.0, 254.0, ink, "Transition to this slide:");
            fonts.draw(frame, face, 16.0, 44.0, 460.0, ink, "Sound:");
        }
        for (i, icon) in self.trans_icons.iter().enumerate() {
            let r = Self::trans_rect(i);
            let selected = cur.is_some_and(|s| s.transition == i);
            let hover = ui::hit(self.mouse.0, self.mouse.1, r.0, r.1, r.2, r.3);
            gfx::fill_round_rect(frame, r.0, r.1, r.2, r.3, 8, if selected { gfx::rgb(250, 200, 200) } else if hover { gfx::rgb(210, 240, 210) } else { gfx::rgb(232, 234, 250) });
            gfx::blit(frame, icon, r.0 + (r.2 - icon.width as i32) / 2, r.1 + 2);
            if selected {
                gfx::stroke_rect(frame, r.0, r.1, r.2, r.3, 2, gfx::rgb(230, 40, 40));
            }
            if hover && !fonts.is_empty() {
                // name below the grid
                fonts.draw(frame, face, 15.0, 300.0, 254.0, gfx::rgb(160, 30, 30), &self.trans_names[i]);
            }
        }
        for (i, name) in self.sound_names.iter().enumerate() {
            let r = Self::sound_rect(i);
            let none = name.to_lowercase().contains("no sound");
            let selected = cur.is_some_and(|s| if none { s.sound.is_none() } else { s.sound == Some(i) });
            gfx::fill_round_rect(frame, r.0, r.1, r.2, r.3, 6, if selected { gfx::rgb(250, 200, 200) } else { gfx::rgb(232, 234, 250) });
            if !fonts.is_empty() {
                fonts.draw_centered(frame, face, 13.0, (r.0 + r.2 / 2) as f32, (r.1 + 14) as f32, (r.2 - 6) as f32, ink, name);
            }
        }
    }
}

pub enum SlideAction {
    AddCurrent,
}

/// Composite transition `kind` at progress `t` (0..1) from `a` to `b` into `out`.
pub fn transition(kind: usize, t: f32, a: &Image, b: &Image, out: &mut Image) {
    let w = a.width as i32;
    let h = a.height as i32;
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    let maxr = (cx * cx + cy * cy).sqrt();
    let get = |img: &Image, x: i32, y: i32| img.pixels[(y.clamp(0, h - 1) * w + x.clamp(0, w - 1)) as usize];
    let te = t * t * (3.0 - 2.0 * t); // ease
    for y in 0..h {
        for x in 0..w {
            let fx = x as f32 + 0.5;
            let fy = y as f32 + 0.5;
            let dx = fx - cx;
            let dy = fy - cy;
            let r = (dx * dx + dy * dy).sqrt();
            let ang = (dx.atan2(-dy) + std::f32::consts::TAU) % std::f32::consts::TAU; // 0 at 12 o'clock
            let hash = ((x as u32 / 16).wrapping_mul(73856093) ^ (y as u32 / 16).wrapping_mul(19349663)) % 1000;
            let px: u32 = match kind {
                0 => if dx.abs() < te * cx { get(b, x, y) } else { get(a, x, y) },
                1 => if ang < t * std::f32::consts::TAU { get(b, x, y) } else { get(a, x, y) },
                2 => {
                    // engulf: new picture grows out of the middle
                    let s = te.max(0.001);
                    if dx.abs() < cx * s && dy.abs() < cy * s {
                        get(b, (cx + dx / s) as i32, (cy + dy / s) as i32)
                    } else {
                        get(a, x, y)
                    }
                }
                3 => if (hash as f32) < te * 1000.0 { get(b, x, y) } else { get(a, x, y) },
                4 => gfx::lerp(get(a, x, y), get(b, x, y), (t * 255.0) as u32),
                5 => {
                    // implode: old picture shrinks into the middle
                    let s = (1.0 - te).max(0.001);
                    if dx.abs() < cx * s && dy.abs() < cy * s {
                        get(a, (cx + dx / s) as i32, (cy + dy / s) as i32)
                    } else {
                        get(b, x, y)
                    }
                }
                6 => if r < te * maxr { get(b, x, y) } else { get(a, x, y) },
                7 => {
                    let s = te * maxr * 1.3 / 120.0;
                    let (u, v) = (dx / (s * 100.0).max(0.01), -dy / (s * 100.0).max(0.01) + 0.25);
                    let heart = (u * u + v * v - 1.0).powi(3) - u * u * v * v * v <= 0.0;
                    if heart || te >= 0.999 { get(b, x, y) } else { get(a, x, y) }
                }
                8 => if dx.abs() < te * cx && dy.abs() < te * cy { get(b, x, y) } else { get(a, x, y) },
                9 => {
                    let k = (ang * 5.0 / std::f32::consts::TAU).fract();
                    let star = 0.5 + 0.5 * (2.0 * k - 1.0).abs();
                    if r < te * maxr * 1.9 * star { get(b, x, y) } else { get(a, x, y) }
                }
                10 => {
                    // jaws: zig-zag teeth open from the middle
                    let tooth = ((x % 40) as f32 - 20.0).abs() / 20.0 * 30.0;
                    if dy.abs() < te * (cy + 30.0) - tooth { get(b, x, y) } else { get(a, x, y) }
                }
                11 => {
                    let k = (ang * 8.0 / std::f32::consts::TAU).fract();
                    if k < te { get(b, x, y) } else { get(a, x, y) }
                }
                12 => {
                    let rows = 8;
                    let rh = h / rows;
                    let row = (y / rh).min(rows - 1);
                    let pos = if row % 2 == 0 { x as f32 / w as f32 } else { 1.0 - x as f32 / w as f32 };
                    if (row as f32 + pos) / rows as f32 <= t { get(b, x, y) } else { get(a, x, y) }
                }
                13 => if dy.abs() < te * cy { get(b, x, y) } else { get(a, x, y) },
                14 => {
                    let jag = ((y as f32 / 9.0).sin() * 12.0) + ((y * 7 % 13) as f32);
                    if fx + fy * 0.4 + jag < te * (w as f32 + h as f32 * 0.4 + 40.0) { get(b, x, y) } else { get(a, x, y) }
                }
                15 => {
                    let off = (te * w as f32) as i32;
                    if x >= w - off { get(b, x - (w - off), y) } else { get(a, x + off, y) }
                }
                16 => {
                    let off = (te * h as f32) as i32;
                    if y < off { get(b, x, y + h - off) } else { get(a, x, y) }
                }
                17 => {
                    let off = (te * w as f32) as i32;
                    if x >= w - off { get(b, x - (w - off), y) } else { get(a, x, y) }
                }
                18 => {
                    let off = (te * w as f32) as i32;
                    if x < off { get(b, x + w - off, y) } else { get(a, x, y) }
                }
                19 => {
                    let off = (te * h as f32) as i32;
                    if y >= h - off { get(b, x, y - (h - off)) } else { get(a, x, y) }
                }
                20 => {
                    // spiral of 40px blocks from the outside in
                    let (bx, by) = (x / 40, y / 40);
                    let (nx, ny) = ((w + 39) / 40, (h + 39) / 40);
                    let ring = bx.min(by).min(nx - 1 - bx).min(ny - 1 - by);
                    let rings = (nx.min(ny) + 1) / 2;
                    let order = ring as f32 / rings as f32 + (ang / std::f32::consts::TAU) / rings as f32;
                    if order < t { get(b, x, y) } else { get(a, x, y) }
                }
                21 => {
                    let sx = cx + (t * 6.0).cos() * cx * 0.5 * (1.0 - t);
                    let sy = cy + (t * 6.0).sin() * cy * 0.5 * (1.0 - t);
                    let d = ((fx - sx).powi(2) + (fy - sy).powi(2)).sqrt();
                    if d < te * maxr * 1.2 { get(b, x, y) } else { gfx::lerp(get(a, x, y), 0xff00_0000, (t * 120.0) as u32) }
                }
                22 => {
                    let delay = ((x / 20) * 37 % 23) as f32 / 23.0 * 0.4;
                    let k = ((t - delay) / 0.6).clamp(0.0, 1.0);
                    if fy < k * h as f32 { get(b, x, y) } else { get(a, x, y) }
                }
                23 => {
                    let k = (ang * 4.0 / std::f32::consts::TAU).fract();
                    if k < te { get(b, x, y) } else { get(a, x, y) }
                }
                24 => if fx + fy < te * (w + h) as f32 { get(b, x, y) } else { get(a, x, y) },
                25 => if fy < te * h as f32 { get(b, x, y) } else { get(a, x, y) },
                26 => if fx > (1.0 - te) * w as f32 { get(b, x, y) } else { get(a, x, y) },
                27 => if fx < te * w as f32 { get(b, x, y) } else { get(a, x, y) },
                28 => if fy > (1.0 - te) * h as f32 { get(b, x, y) } else { get(a, x, y) },
                _ => {
                    let s = te.max(0.001);
                    if dx.abs() < cx * s && dy.abs() < cy * s {
                        get(b, (cx + dx / s) as i32, (cy + dy / s) as i32)
                    } else {
                        gfx::lerp(get(a, x, y), get(b, x, y), (t * 255.0) as u32)
                    }
                }
            };
            out.pixels[(y * w + x) as usize] = px;
        }
    }
    let _ = assets::T_BMP;
}
