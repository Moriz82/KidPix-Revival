//! The drawing surface, undo history and painting algorithms.

use crate::gfx::{self, argb, blend, lerp, split, Image};

pub const CANVAS_X: i32 = 112;
pub const CANVAS_Y: i32 = 48;
pub const CANVAS_W: u32 = 600;
pub const CANVAS_H: u32 = 452;

pub const WHITE: u32 = 0xffff_ffff;
pub const BLACK: u32 = 0xff00_0000;

pub struct Canvas {
    pub img: Image,
    undo: Vec<Image>,
    redo: Vec<Image>,
    pub dirty: bool,
    /// The last history step was an undo (so the Undo Guy redoes next).
    pub last_was_undo: bool,
}

impl Canvas {
    pub fn new() -> Canvas {
        Canvas {
            img: Image::filled(CANVAS_W, CANVAS_H, WHITE),
            undo: Vec::new(),
            redo: Vec::new(),
            dirty: false,
            last_was_undo: false,
        }
    }

    /// Record the current state before a modifying action.
    pub fn checkpoint(&mut self) {
        self.undo.push(self.img.clone());
        if self.undo.len() > 40 {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.dirty = true;
        self.last_was_undo = false;
    }

    pub fn undo(&mut self) -> bool {
        if let Some(prev) = self.undo.pop() {
            let cur = std::mem::replace(&mut self.img, prev);
            self.redo.push(cur);
            self.last_was_undo = true;
            true
        } else {
            false
        }
    }

    pub fn redo(&mut self) -> bool {
        if let Some(next) = self.redo.pop() {
            let cur = std::mem::replace(&mut self.img, next);
            self.undo.push(cur);
            self.last_was_undo = false;
            true
        } else {
            false
        }
    }

    pub fn replace(&mut self, img: Image) {
        self.checkpoint();
        self.img = img;
    }

    pub fn reset(&mut self) {
        self.img = Image::filled(CANVAS_W, CANVAS_H, WHITE);
        self.undo.clear();
        self.redo.clear();
        self.dirty = false;
    }
}

// ---------------------------------------------------------------------------------------
// Tiny deterministic RNG (xorshift) so effects don't need a crate.

#[derive(Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed.max(1))
    }
    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        (x >> 32) as u32
    }
    /// Uniform in [0, 1).
    pub fn f(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.f()
    }
    pub fn below(&mut self, n: u32) -> u32 {
        if n == 0 {
            0
        } else {
            self.next_u32() % n
        }
    }
}

// ---------------------------------------------------------------------------------------
// Brush pads and real-media painting.

/// Alpha mask 0..=255 used as a brush footprint.
#[derive(Clone)]
pub struct Pad {
    pub w: i32,
    pub h: i32,
    pub mask: Vec<u8>,
    /// Optional colored version (16-bit pads keep their own colors).
    pub color: Option<Image>,
}

impl Pad {
    pub fn round(diameter: i32, soft: bool) -> Pad {
        let d = diameter.max(1);
        let r = d as f32 / 2.0;
        let mut mask = vec![0u8; (d * d) as usize];
        for y in 0..d {
            for x in 0..d {
                let dx = x as f32 + 0.5 - r;
                let dy = y as f32 + 0.5 - r;
                let dist = (dx * dx + dy * dy).sqrt();
                let a = if soft {
                    (1.0 - dist / r).clamp(0.0, 1.0)
                } else {
                    (r - dist + 0.5).clamp(0.0, 1.0)
                };
                mask[(y * d + x) as usize] = (a * 255.0) as u8;
            }
        }
        Pad {
            w: d,
            h: d,
            mask,
            color: None,
        }
    }

    /// Build from a decoded brush bitmap: 8-bit pads are density masks (greyscale decode),
    /// 16-bit pads keep their colors and use luminance-independent full alpha.
    pub fn from_image(img: &Image, is_color: bool) -> Pad {
        let mask = img
            .pixels
            .iter()
            .map(|&p| {
                if is_color {
                    (p >> 24) as u8
                } else {
                    (p & 0xff) as u8 // greyscale palette: index == value
                }
            })
            .collect();
        Pad {
            w: img.width as i32,
            h: img.height as i32,
            mask,
            color: if is_color { Some(img.clone()) } else { None },
        }
    }

    pub fn scaled(&self, factor: f32) -> Pad {
        let w = ((self.w as f32 * factor).round() as i32).max(1);
        let h = ((self.h as f32 * factor).round() as i32).max(1);
        let mut mask = vec![0u8; (w * h) as usize];
        for y in 0..h {
            for x in 0..w {
                let sx = ((x as f32 + 0.5) / factor) as i32;
                let sy = ((y as f32 + 0.5) / factor) as i32;
                mask[(y * w + x) as usize] =
                    self.mask[(sy.min(self.h - 1) * self.w + sx.min(self.w - 1)) as usize];
            }
        }
        Pad {
            w,
            h,
            mask,
            color: self.color.as_ref().map(|c| gfx::resize(c, w as u32, h as u32)),
        }
    }
}

/// Parameters of a "real media" brush (names follow the sections in KPD3.cfg).
#[derive(Clone, Debug, Default)]
pub struct MediaParams {
    pub brush_id: u16,
    pub spacing: f32,
    pub spatter: i32,
    pub density_bias: i32,
    pub dry_out_rate: i32,
    pub dry_out_threshold: i32,
    pub oscillation: i32,
    pub shadow_offset: i32,
    pub shadow_bias: i32,
    pub glint_offset: i32,
    pub glint_bias: i32,
}

impl MediaParams {
    pub fn from_cfg(ini: &crate::assets::Ini, section: &str) -> Option<MediaParams> {
        if !ini.has_section(section) {
            return None;
        }
        let g = |k: &str| ini.get_i32(section, k).unwrap_or(0);
        Some(MediaParams {
            brush_id: g("brushID") as u16,
            spacing: g("spacing") as f32,
            spatter: g("spatter"),
            density_bias: g("densityBias"),
            dry_out_rate: g("dryOutRate"),
            dry_out_threshold: g("dryOutThreshold"),
            oscillation: g("oscillation"),
            shadow_offset: g("shadowOffset"),
            shadow_bias: g("shadowBias"),
            glint_offset: g("glintOffset"),
            glint_bias: g("glintBias"),
        })
    }
}

/// Stamp one pad of `color` centred at (cx, cy) with opacity `alpha` (0..1).
pub fn dab(dst: &mut Image, pad: &Pad, cx: f32, cy: f32, color: u32, alpha: f32, rng: Option<(&mut Rng, i32)>) {
    dab_on(dst, pad, cx, cy, color, alpha, rng, None)
}

/// Like [`dab`], with the page's paper texture showing through (Crayon, Chalk, Marker).
#[allow(clippy::too_many_arguments)]
pub fn dab_on(dst: &mut Image, pad: &Pad, cx: f32, cy: f32, color: u32, alpha: f32, rng: Option<(&mut Rng, i32)>, paper: Option<&Image>) {
    let x0 = (cx - pad.w as f32 / 2.0).round() as i32;
    let y0 = (cy - pad.h as f32 / 2.0).round() as i32;
    let (_, cr, cg, cb) = split(color);
    let mut rng = rng;
    for y in 0..pad.h {
        for x in 0..pad.w {
            let m = pad.mask[(y * pad.w + x) as usize];
            if m == 0 {
                continue;
            }
            if let Some((r, spatter)) = rng.as_mut() {
                if *spatter > 0 && r.below((*spatter as u32 + 1) * 2) < *spatter as u32 {
                    continue;
                }
            }
            let grain = match paper {
                Some(p) => {
                    let l = p.get(x0 + x, y0 + y).map(|v| v & 0xff).unwrap_or(255) as f32;
                    0.2 + 0.8 * l / 255.0
                }
                None => 1.0,
            };
            let a = (m as f32 * alpha * grain).clamp(0.0, 255.0) as u8;
            if a == 0 {
                continue;
            }
            let src = match &pad.color {
                Some(img) => {
                    let p = img.pixels[(y * pad.w + x) as usize];
                    (p & 0x00ff_ffff) | (a as u32) << 24
                }
                None => argb(a, cr as u8, cg as u8, cb as u8),
            };
            if let Some(d) = dst.get(x0 + x, y0 + y) {
                dst.put(x0 + x, y0 + y, blend(d, src));
            }
        }
    }
}

/// State for one stroke of a real-media brush.
pub struct MediaStroke {
    pub params: MediaParams,
    pub pad: Pad,
    pub last: Option<(f32, f32)>,
    pub travelled: f32,
    pub rng: Rng,
    /// Paper texture of the page (shows through Crayon/Chalk/Marker).
    pub paper: Option<std::rc::Rc<Image>>,
}

impl MediaStroke {
    pub fn new(params: MediaParams, pad: Pad, seed: u64) -> MediaStroke {
        MediaStroke {
            params,
            pad,
            last: None,
            travelled: 0.0,
            rng: Rng::new(seed),
            paper: None,
        }
    }

    fn opacity(&self) -> f32 {
        let base = 1.0 / (1.0 + self.params.density_bias.max(0) as f32 * 0.6);
        let dry = if self.params.dry_out_rate > 0 {
            let lost = self.travelled * self.params.dry_out_rate as f32 / 600.0;
            let floor = if self.params.dry_out_threshold > 0 {
                1.0 - self.params.dry_out_threshold as f32 / 10.0
            } else {
                0.05
            };
            (1.0 - lost).max(floor.clamp(0.0, 1.0))
        } else {
            1.0
        };
        base * dry
    }

    fn one(&mut self, dst: &mut Image, x: f32, y: f32, color: u32) {
        let osc = self.params.oscillation as f32;
        let (jx, jy) = if osc > 0.0 {
            (self.rng.range(-osc, osc), self.rng.range(-osc, osc))
        } else {
            (0.0, 0.0)
        };
        let (x, y) = (x + jx, y + jy);
        let a = self.opacity();
        if self.params.shadow_offset != 0 {
            let so = self.params.shadow_offset as f32;
            let sa = a / (1.0 + self.params.shadow_bias.max(0) as f32 * 0.4);
            let shade = lerp(color, BLACK, 140);
            dab(dst, &self.pad, x + so, y + so, shade, sa, None);
        }
        let spatter = self.params.spatter;
        let paper = self.paper.clone();
        dab_on(dst, &self.pad, x, y, color, a, Some((&mut self.rng, spatter)), paper.as_deref());
        if self.params.glint_offset != 0 {
            let go = self.params.glint_offset as f32;
            let ga = a / (1.0 + self.params.glint_bias.max(0) as f32 * 0.4);
            let glint = lerp(color, WHITE, 170);
            let small = self.pad.scaled(0.5);
            dab(dst, &small, x - go, y - go, glint, ga, None);
        }
    }

    /// Extend the stroke to (x, y).
    pub fn to(&mut self, dst: &mut Image, x: f32, y: f32, color: u32) {
        let Some((lx, ly)) = self.last else {
            self.one(dst, x, y, color);
            self.last = Some((x, y));
            return;
        };
        let spacing = if self.params.spacing <= 0.0 {
            (self.pad.w.min(self.pad.h) as f32 / 4.0).max(1.0)
        } else {
            self.params.spacing.max(1.0)
        };
        let mut lastp = (lx, ly);
        let mut placed = false;
        let mut pts = Vec::new();
        gfx::walk_line(lx, ly, x, y, spacing, |px, py| pts.push((px, py)));
        for (px, py) in pts {
            self.one(dst, px, py, color);
            self.travelled += spacing;
            lastp = (px, py);
            placed = true;
        }
        if placed {
            self.last = Some(lastp);
        }
    }

    /// For spacing==0 brushes: keep spraying while the button is held still.
    pub fn idle(&mut self, dst: &mut Image, color: u32) {
        if self.params.spacing <= 0.0 {
            if let Some((x, y)) = self.last {
                self.one(dst, x, y, color);
            }
        }
    }
}

/// Hard round line (pencil): stamps a disc along the segment.
pub fn line(dst: &mut Image, x0: f32, y0: f32, x1: f32, y1: f32, width: f32, color: u32) {
    let r = width / 2.0;
    if width <= 1.5 {
        // Bresenham-like 1px line, no AA (classic look).
        let n = ((x1 - x0).abs().max((y1 - y0).abs())).ceil() as i32;
        for i in 0..=n {
            let t = if n == 0 { 0.0 } else { i as f32 / n as f32 };
            dst.put(
                (x0 + (x1 - x0) * t).floor() as i32,
                (y0 + (y1 - y0) * t).floor() as i32,
                color,
            );
        }
        return;
    }
    gfx::fill_circle(dst, x0, y0, r, color);
    gfx::walk_line(x0, y0, x1, y1, (r / 2.0).max(0.5), |x, y| {
        gfx::fill_circle(dst, x, y, r, color)
    });
}

// ---------------------------------------------------------------------------------------
// Flood fill.

/// Pixels connected to (x, y) whose color is within `tolerance` of the seed color.
pub fn flood_region(img: &Image, x: i32, y: i32, tolerance: u32) -> Vec<(i32, i32)> {
    let Some(seed) = img.get(x, y) else {
        return Vec::new();
    };
    let w = img.width as i32;
    let h = img.height as i32;
    let close = |c: u32| {
        let (_, r1, g1, b1) = split(c);
        let (_, r2, g2, b2) = split(seed);
        r1.abs_diff(r2) + g1.abs_diff(g2) + b1.abs_diff(b2) <= tolerance
    };
    let mut seen = vec![false; (w * h) as usize];
    let mut stack = vec![(x, y)];
    let mut region = Vec::new();
    while let Some((sx, sy)) = stack.pop() {
        if seen[(sy * w + sx) as usize] {
            continue;
        }
        let mut lx = sx;
        while lx > 0 && !seen[(sy * w + lx - 1) as usize] && close(img.pixels[(sy * w + lx - 1) as usize]) {
            lx -= 1;
        }
        let mut rx = sx;
        while rx < w - 1 && !seen[(sy * w + rx + 1) as usize] && close(img.pixels[(sy * w + rx + 1) as usize]) {
            rx += 1;
        }
        for xx in lx..=rx {
            let i = (sy * w + xx) as usize;
            if seen[i] {
                continue;
            }
            seen[i] = true;
            region.push((xx, sy));
            for ny in [sy - 1, sy + 1] {
                if ny >= 0 && ny < h {
                    let j = (ny * w + xx) as usize;
                    if !seen[j] && close(img.pixels[j]) {
                        stack.push((xx, ny));
                    }
                }
            }
        }
    }
    region
}

// ---------------------------------------------------------------------------------------
// Pixel effects (used by effect brushes in a radius and by mixer on the whole canvas).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    Invert,
    Greyscale,
    Sepia,
    Posterize,
    Solarize,
    HueShift,
    Blur,
    Mosaic,
    Emboss,
    Edge,
}

pub fn luma(c: u32) -> u32 {
    let (_, r, g, b) = split(c);
    (r * 77 + g * 150 + b * 29) >> 8
}

fn hue_rotate(c: u32, deg: f32) -> u32 {
    let (_, r, g, b) = split(c);
    let (r, g, b) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let v = max;
    let d = max - min;
    let s = if max > 0.0 { d / max } else { 0.0 };
    let mut h = if d == 0.0 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / d) % 6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    h += deg;
    gfx::hsv(h, s, v)
}

/// Apply a per-pixel effect to all pixels of `img` where `mask(x, y)` is true.
pub fn apply_effect(img: &mut Image, effect: Effect, rect: (i32, i32, i32, i32), mask: impl Fn(i32, i32) -> bool) {
    let (x0, y0, x1, y1) = (
        rect.0.max(0),
        rect.1.max(0),
        rect.2.min(img.width as i32),
        rect.3.min(img.height as i32),
    );
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let src = img.clone();
    let get = |x: i32, y: i32| {
        src.get(x.clamp(0, src.width as i32 - 1), y.clamp(0, src.height as i32 - 1))
            .unwrap_or(WHITE)
    };
    for y in y0..y1 {
        for x in x0..x1 {
            if !mask(x, y) {
                continue;
            }
            let c = get(x, y);
            let (_, r, g, b) = split(c);
            let out = match effect {
                Effect::Invert => 0xff00_0000 | (!c & 0x00ff_ffff),
                Effect::Greyscale => {
                    let l = luma(c);
                    gfx::rgb(l as u8, l as u8, l as u8)
                }
                Effect::Sepia => {
                    let l = luma(c) as f32;
                    gfx::rgb(
                        (l * 1.07 + 20.0).min(255.0) as u8,
                        (l * 0.95 + 5.0).min(255.0) as u8,
                        (l * 0.72).min(255.0) as u8,
                    )
                }
                Effect::Posterize => {
                    let q = |v: u32| ((v / 64) * 85).min(255) as u8;
                    gfx::rgb(q(r), q(g), q(b))
                }
                Effect::Solarize => {
                    let s = |v: u32| if v > 127 { 255 - v } else { v } as u8;
                    gfx::rgb(s(r) * 2, s(g) * 2, s(b) * 2)
                }
                Effect::HueShift => hue_rotate(c, 40.0),
                Effect::Blur => {
                    let mut acc = [0u32; 3];
                    for dy in -2..=2 {
                        for dx in -2..=2 {
                            let (_, r, g, b) = split(get(x + dx, y + dy));
                            acc[0] += r;
                            acc[1] += g;
                            acc[2] += b;
                        }
                    }
                    gfx::rgb((acc[0] / 25) as u8, (acc[1] / 25) as u8, (acc[2] / 25) as u8)
                }
                Effect::Mosaic => {
                    let s = 10;
                    get(x - x.rem_euclid(s) + s / 2, y - y.rem_euclid(s) + s / 2)
                }
                Effect::Emboss => {
                    let a = luma(get(x - 1, y - 1)) as i32;
                    let b2 = luma(get(x + 1, y + 1)) as i32;
                    let v = (128 + (b2 - a) * 2).clamp(0, 255) as u8;
                    gfx::rgb(v, v, v)
                }
                Effect::Edge => {
                    let gx = luma(get(x + 1, y)) as i32 - luma(get(x - 1, y)) as i32;
                    let gy = luma(get(x, y + 1)) as i32 - luma(get(x, y - 1)) as i32;
                    let m = ((gx * gx + gy * gy) as f32).sqrt().min(255.0) as u8;
                    gfx::rgb(255 - m, 255 - m, 255 - m)
                }
            };
            img.put(x, y, out);
        }
    }
}

/// Apply an effect within a circle (effect brushes).
pub fn effect_dab(img: &mut Image, effect: Effect, cx: f32, cy: f32, radius: f32) {
    let r2 = radius * radius;
    let rect = (
        (cx - radius) as i32,
        (cy - radius) as i32,
        (cx + radius) as i32 + 1,
        (cy + radius) as i32 + 1,
    );
    apply_effect(img, effect, rect, |x, y| {
        let dx = x as f32 + 0.5 - cx;
        let dy = y as f32 + 0.5 - cy;
        dx * dx + dy * dy <= r2
    });
}

/// Whole-picture transforms used by the Electric Mixer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mix {
    Effect(Effect),
    FlipH,
    FlipV,
    Kaleidoscope,
    Ripple,
    Shatter,
    Swirl,
}

pub fn mix_canvas(img: &mut Image, mix: Mix, rng: &mut Rng) {
    let w = img.width as i32;
    let h = img.height as i32;
    let src = img.clone();
    let get = |x: i32, y: i32| src.pixels[(y.clamp(0, h - 1) * w + x.clamp(0, w - 1)) as usize];
    match mix {
        Mix::Effect(e) => apply_effect(img, e, (0, 0, w, h), |_, _| true),
        Mix::FlipH => {
            for y in 0..h {
                for x in 0..w {
                    img.put(x, y, get(w - 1 - x, y));
                }
            }
        }
        Mix::FlipV => {
            for y in 0..h {
                for x in 0..w {
                    img.put(x, y, get(x, h - 1 - y));
                }
            }
        }
        Mix::Kaleidoscope => {
            for y in 0..h {
                for x in 0..w {
                    let sx = if x < w / 2 { x } else { w - 1 - x };
                    let sy = if y < h / 2 { y } else { h - 1 - y };
                    img.put(x, y, get(sx, sy));
                }
            }
        }
        Mix::Ripple => {
            for y in 0..h {
                for x in 0..w {
                    let ox = (y as f32 / 12.0).sin() * 8.0;
                    let oy = (x as f32 / 15.0).sin() * 6.0;
                    img.put(x, y, get(x + ox as i32, y + oy as i32));
                }
            }
        }
        Mix::Shatter => {
            let tile = 30;
            for ty in (0..h).step_by(tile) {
                for tx in (0..w).step_by(tile) {
                    let ox = rng.range(-6.0, 6.0) as i32;
                    let oy = rng.range(-6.0, 6.0) as i32;
                    for y in ty..(ty + tile as i32).min(h) {
                        for x in tx..(tx + tile as i32).min(w) {
                            img.put(x, y, get(x + ox, y + oy));
                        }
                    }
                }
            }
        }
        Mix::Swirl => {
            let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
            let maxr = cx.min(cy);
            for y in 0..h {
                for x in 0..w {
                    let dx = x as f32 - cx;
                    let dy = y as f32 - cy;
                    let r = (dx * dx + dy * dy).sqrt();
                    let a = (1.0 - (r / maxr).min(1.0)) * 2.2;
                    let (s, c) = a.sin_cos();
                    let sx = cx + dx * c - dy * s;
                    let sy = cy + dx * s + dy * c;
                    img.put(x, y, get(sx as i32, sy as i32));
                }
            }
        }
    }
}

/// Fill styles for the paint can.
#[derive(Clone)]
pub enum FillStyle {
    Solid,
    /// Picture stretched over the filled area (the CD's gradient fills).
    Stretch(Image),
    /// Picture repeated (the CD's pattern fills).
    Tile(Image),
    /// Greyscale texture shaded with the current color (TP 20500+i).
    Tint(mohawk::Indexed),
    /// A `grad` gradient fitted to the region.
    Gradient(crate::fills::Grad),
}

fn sample_bilinear(img: &Image, fx: f32, fy: f32) -> u32 {
    let w = img.width as i32;
    let h = img.height as i32;
    let x0 = (fx.floor() as i32).clamp(0, w - 1);
    let y0 = (fy.floor() as i32).clamp(0, h - 1);
    let x1 = (x0 + 1).min(w - 1);
    let y1 = (y0 + 1).min(h - 1);
    let tx = ((fx - x0 as f32).clamp(0.0, 1.0) * 255.0) as u32;
    let ty = ((fy - y0 as f32).clamp(0.0, 1.0) * 255.0) as u32;
    let p = |x: i32, y: i32| img.pixels[(y * w + x) as usize] | 0xff00_0000;
    lerp(lerp(p(x0, y0), p(x1, y0), tx), lerp(p(x0, y1), p(x1, y1), tx), ty)
}

pub fn fill_with(img: &mut Image, x: i32, y: i32, style: &FillStyle, color: u32) -> bool {
    let region = crate::fills::region(img, x, y);
    paint_region(img, &region, style, color, (x as f32, y as f32));
    !region.is_empty()
}

// ---------------------------------------------------------------------------------------
// Shapes (Drawing Tools) and cut-outs (Scissors).

/// Even-odd scanline fill of a polygon.
pub fn fill_polygon(img: &mut Image, pts: &[(f32, f32)], color: u32) {
    for_polygon_pixels(img.width as i32, img.height as i32, pts, |x, y| img.put(x, y, color));
}

/// Call `f` for every pixel inside the polygon (even-odd rule), clipped to w x h.
pub fn for_polygon_pixels(w: i32, h: i32, pts: &[(f32, f32)], mut f: impl FnMut(i32, i32)) {
    if pts.len() < 3 {
        return;
    }
    let y0 = pts.iter().map(|p| p.1).fold(f32::MAX, f32::min).floor().max(0.0) as i32;
    let y1 = pts.iter().map(|p| p.1).fold(f32::MIN, f32::max).ceil().min(h as f32 - 1.0) as i32;
    let mut xs = Vec::new();
    for y in y0..=y1 {
        let sy = y as f32 + 0.5;
        xs.clear();
        for i in 0..pts.len() {
            let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
            if (a.1 <= sy && b.1 > sy) || (b.1 <= sy && a.1 > sy) {
                xs.push(a.0 + (sy - a.1) / (b.1 - a.1) * (b.0 - a.0));
            }
        }
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        for pair in xs.chunks_exact(2) {
            let xa = pair[0].round().max(0.0) as i32;
            let xb = (pair[1].round() as i32).min(w);
            for x in xa..xb {
                f(x, y);
            }
        }
    }
}

pub fn rect_points(a: (f32, f32), b: (f32, f32)) -> Vec<(f32, f32)> {
    vec![(a.0, a.1), (b.0, a.1), (b.0, b.1), (a.0, b.1)]
}

pub fn oval_points(a: (f32, f32), b: (f32, f32)) -> Vec<(f32, f32)> {
    let (cx, cy) = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
    let (rx, ry) = (((b.0 - a.0) / 2.0).abs(), ((b.1 - a.1) / 2.0).abs());
    let n = ((rx + ry) * 0.8).clamp(16.0, 160.0) as usize;
    (0..n)
        .map(|i| {
            let t = i as f32 / n as f32 * std::f32::consts::TAU;
            (cx + t.cos() * rx, cy + t.sin() * ry)
        })
        .collect()
}

/// Quadratic Bézier from a to b bent towards control point c.
pub fn curve_points(a: (f32, f32), c: (f32, f32), b: (f32, f32)) -> Vec<(f32, f32)> {
    let n = 48;
    (0..=n)
        .map(|i| {
            let t = i as f32 / n as f32;
            let u = 1.0 - t;
            (u * u * a.0 + 2.0 * u * t * c.0 + t * t * b.0, u * u * a.1 + 2.0 * u * t * c.1 + t * t * b.1)
        })
        .collect()
}

pub fn region_bbox(region: &[(i32, i32)]) -> (i32, i32, i32, i32) {
    let mut b = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    for &(x, y) in region {
        b = (b.0.min(x), b.1.min(y), b.2.max(x), b.3.max(y));
    }
    b
}

/// Paint a fill region with a style; `focus` is the radial gradient's focus (click point).
pub fn paint_region(img: &mut Image, region: &[(i32, i32)], style: &FillStyle, color: u32, focus: (f32, f32)) {
    let w = img.width as i32;
    match style {
        FillStyle::Solid => {
            for &(x, y) in region {
                img.pixels[(y * w + x) as usize] = color;
            }
        }
        FillStyle::Tint(t) => crate::fills::tint(img, region, t, color),
        FillStyle::Tile(pat) => crate::fills::pattern(img, region, pat),
        FillStyle::Gradient(g) => crate::fills::gradient_fill(img, region, g, focus),
        FillStyle::Stretch(src) => {
            let (bx0, by0, bx1, by1) = region_bbox(region);
            let (bw, bh) = ((bx1 - bx0 + 1) as f32, (by1 - by0 + 1) as f32);
            for &(px, py) in region {
                let fx = (px - bx0) as f32 / bw * src.width as f32 - 0.5;
                let fy = (py - by0) as f32 / bh * src.height as f32 - 0.5;
                img.pixels[(py * w + px) as usize] = sample_bilinear(src, fx, fy);
            }
        }
    }
}
