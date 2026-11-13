//! The real-media pad engine `RRealMedia` and its subclasses (RE_BRUSHES §3, §4 media).
//!
//! Covers RealBrush, Airbrush, BristleBrush, ExtrusionBrush, CustomBrush1–16, Chalk,
//! Crayon, Marker, Drippy and every Mixer Wand brush (Smear, Blur, Greyscale, Colorize,
//! XRay, Emboss, Edge, Nuclear, Solarize, HueShift, Posterize, Mosaic).

use crate::assets::{Pad, SpriteSet, Texture, ICONS_MHK};
use crate::color::*;
use crate::config::BrushParams;
use crate::tool::{BrushImpl, Cx};
use mohawk::{Cel, Image};
use std::sync::Arc;

/// Opacity scale constant `K` (§3.6).
pub const K: f64 = 254.0;
/// Default stamp step when cfg `spacing == 0` (§3.3).
pub const DEFAULT_STEP: i32 = 3;

/// Mixer-wand effects applied on mouse up (§3.9).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    XRay,
    Emboss,
    Edge,
    Nuclear,
    Solarize,
    HueShift,
    Posterize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PadKind {
    /// Pure engine (CustomBrush1–16, Chalk).
    Plain,
    Airbrush,
    Bristle,
    Marker,
    Crayon,
    RealBrush,
    Extrusion,
    Drippy,
    Smear,
    Blur,
    Greyscale,
    Colorize,
    Effect(Effect),
    Mosaic,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Normal,
    Shadow,
    Glint,
}

/// Bresenham walk from `a` to `b` (`@0x4448c0`): returns the accumulated `(dx, dy)` of
/// every stamp, a stamp whenever the major-axis accumulation reaches `step`, plus a final
/// one for any remainder, so the last stamp lands exactly on `b`.
pub fn walk_line(a: (i32, i32), b: (i32, i32), step: i32) -> Vec<(i32, i32)> {
    let step = step.max(1);
    let mut out = Vec::new();
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let sx = if dx >= 0 { 1 } else { -1 };
    let sy = if dy >= 0 { 1 } else { -1 };
    let (ax, ay) = (2 * dx.abs(), 2 * dy.abs());
    let (mut accx, mut accy) = (0, 0);
    if ax <= ay {
        let mut err = ax - ay / 2;
        let mut y = a.1;
        while y != b.1 {
            if err >= 0 {
                err -= ay;
                accx += sx;
            }
            accy += sy;
            y += sy;
            err += ax;
            if sy * accy >= step {
                out.push((accx, accy));
                accx = 0;
                accy = 0;
            }
        }
    } else {
        let mut err = ay - ax / 2;
        let mut x = a.0;
        while x != b.0 {
            if err >= 0 {
                err -= ax;
                accy += sy;
            }
            accx += sx;
            x += sx;
            err += ay;
            if sx * accx >= step {
                out.push((accx, accy));
                accx = 0;
                accy = 0;
            }
        }
    }
    if accx != 0 || accy != 0 {
        out.push((accx, accy));
    }
    out
}

/// One running drip animation (Drippy, §3.8).
struct Drip {
    set: Arc<SpriteSet>,
    frames: Vec<Vec<Cel>>,
    idx: usize,
    wait: f32,
    origin: (i32, i32),
    anchor: (i32, i32),
    color: Rgb5,
}

/// Frame period of `tSCR` playback: tempo 100 → 100 ms per frame [I] (same reading as
/// the engine's `anim.rs`).
pub const SCRIPT_FRAME_MS: f32 = 100.0;

/// Draw one script frame with the first cel's registration point at `origin` (§7, §3.8
/// placement [I]). `tint` replaces every cel colour (coverage = cel alpha).
pub(crate) fn draw_frame(img: &mut Image, set: &SpriteSet, cels: &[Cel], origin: (i32, i32), anchor: (i32, i32), tint: Option<Rgb5>) {
    for c in cels.iter().filter(|c| c.visible) {
        let Some(cel) = set.cel(c.cel) else { continue };
        let (rx, ry) = set.reg(c.cel);
        let x = origin.0 + c.x as i32 - anchor.0 - rx;
        let y = origin.1 + c.y as i32 - anchor.1 - ry;
        match tint {
            None => crate::raster::blit(img, cel, x, y),
            Some(col) => {
                let cov: Vec<u8> = cel.pixels.iter().map(|p| (p >> 24) as u8).collect();
                crate::raster::coverage(img, &cov, cel.width as usize, cel.height as usize, x, y, col);
            }
        }
    }
}

pub(crate) fn script_frames(s: &mohawk::Script) -> Vec<Vec<Cel>> {
    s.frames().into_iter().map(|f| f.to_vec()).collect()
}

/// Registration anchor of a script: its first frame's first visible cel position.
pub(crate) fn script_anchor(frames: &[Vec<Cel>]) -> (i32, i32) {
    frames
        .iter()
        .flat_map(|f| f.iter())
        .find(|c| c.visible)
        .map(|c| (c.x as i32, c.y as i32))
        .unwrap_or((0, 0))
}

pub struct PadEngine {
    pub kind: PadKind,
    pub p: BrushParams,
    pad: Option<Pad>,
    pad_size: u8,
    hilite: Option<Arc<Pad>>,
    step: i32,
    /// Brush rect top-left; side = `size`.
    rect: (i32, i32),
    size: i32,
    last: (i32, i32),
    space: bool,
    s: i32,
    c5: Rgb5,
    c8: [u8; 3],
    smear: Vec<Rgb5>,
    paint: Vec<Rgb5>,
    counter: u32,
    tex: Option<Arc<Texture>>,
    backup: Option<Image>,
    marks: Vec<bool>,
    drips: Vec<Drip>,
    next_drip: u64,
}

impl PadEngine {
    pub fn new(kind: PadKind, p: BrushParams) -> PadEngine {
        let step = if p.spacing != 0 { p.spacing as i32 } else { DEFAULT_STEP };
        PadEngine {
            kind,
            p,
            pad: None,
            pad_size: 0,
            hilite: None,
            step,
            rect: (0, 0),
            size: 1,
            last: (0, 0),
            space: false,
            s: 1,
            c5: BLACK5,
            c8: [0; 3],
            smear: Vec::new(),
            paint: Vec::new(),
            counter: 0,
            tex: None,
            backup: None,
            marks: Vec::new(),
            drips: Vec::new(),
            next_drip: 0,
        }
    }

    /// The pad currently loaded (after the first mouse down).
    pub fn pad(&self) -> Option<&Pad> {
        self.pad.as_ref()
    }

    // ---- §3.5 randomness ---------------------------------------------------------------

    fn spatter_keep(&self, cx: &mut Cx) -> bool {
        match self.kind {
            PadKind::Colorize => true,
            PadKind::Bristle => cx.rng.rnd(0, self.p.spatter as i32 * self.s * 2) == 0,
            _ => cx.rng.rnd(0, self.p.spatter as i32) == 0,
        }
    }

    fn oscillation(&self, cx: &mut Cx) -> (i32, i32) {
        let m = self.p.oscillation as i32 * self.s;
        if m != 0 {
            let r = cx.rng.rand() % m;
            if r != 0 {
                let th = (cx.rng.rand() % 360) as f64; // degrees value used as radians [O]
                return (trunc(th.cos() * r as f64), trunc(th.sin() * r as f64));
            }
        }
        (0, 0)
    }

    fn density(&self, cx: &mut Cx) -> f64 {
        let db = self.p.density_bias as i32;
        match self.kind {
            PadKind::Airbrush => cx.rng.rnd(1, db + 1) as f64 * 255.0,
            PadKind::Bristle => cx.rng.rnd(1, db + 1) as f64 * 254.0,
            _ => {
                if db == 0 {
                    K
                } else {
                    db as f64 * K
                }
            }
        }
    }

    // ---- §3.6 blend --------------------------------------------------------------------

    fn coverage_t(&self, a: i32, tx: Option<u8>, mode: Mode, cx: &mut Cx) -> f64 {
        let bias = |b: i16| if b == 0 { f64::INFINITY } else { b as f64 * K }; // [I] bias 0 → no effect
        match mode {
            Mode::Shadow => a as f64 / bias(self.p.shadow_bias),
            Mode::Glint => a as f64 / bias(self.p.glint_bias),
            Mode::Normal => {
                let mut num = a;
                let mut extra = 0.0;
                if let Some(t) = tx {
                    let tv = if self.space { 255 - t } else { t };
                    num += tv as i32 / self.p.texture_bias as i32;
                    // Spec said 255 + D; the code adds 254.0 (`v24`) — see corrections.
                    extra = 254.0;
                }
                let d = self.density(cx);
                if d <= 0.0 {
                    0.0
                } else {
                    num as f64 / (extra + d)
                }
            }
        }
    }

    fn texel(&self, x: i32, y: i32, x0: i32) -> Option<u8> {
        let t = self.tex.as_ref()?;
        if t.w <= 0 || t.h <= 0 || t.data.is_empty() {
            return None;
        }
        let row = y % t.h;
        let col = x0 % t.w + (x - x0); // rows wrap, columns are not re-wrapped [O]
        let idx = (row as i64 * t.w as i64 + col as i64).rem_euclid(t.data.len() as i64) as usize;
        Some(t.data[idx])
    }

    /// Per-pixel blend (`v42`) for the base engine and the per-pixel overrides.
    #[allow(clippy::too_many_arguments)]
    fn pixel(&mut self, cv: &mut Image, x: i32, y: i32, a: u8, idx: usize, x0: i32, col: Rgb5, mode: Mode, cx: &mut Cx) {
        let dw = argb_to_555(cv.pixels[(y as u32 * cv.width + x as u32) as usize]);
        let d = unpack555(dw);
        let out: u16 = match self.kind {
            PadKind::Greyscale => {
                let (h, _, v) = rgb2hsv(d[0], d[1], d[2]);
                let (r, g, b) = hsv2rgb(h, 0, v);
                pack555([r, g, b])
            }
            PadKind::Colorize => {
                let (hb, sb, _) = rgb2hsv(self.c8[0] as i32, self.c8[1] as i32, self.c8[2] as i32);
                let (_, _, vd) = rgb2hsv(d[0], d[1], d[2]);
                let (r, g, b) = hsv2rgb(hb, sb, vd);
                // [I] The code multiplies by the raw pad byte (`c'*a + d*(1-a)`), which
                // overflows for a > 1; we use a/255 (see corrections, unverified).
                let t = a as f64 / 255.0;
                let m = |c: i32, d: i32| trunc(c as f64 * t + d as f64 * (1.0 - t));
                pack555([m(r, d[0]), m(g, d[1]), m(b, d[2])])
            }
            PadKind::Effect(_) => {
                if self.marks[idx] {
                    return;
                }
                self.marks[idx] = true;
                pack555([31 - d[0], 31 - d[1], 31 - d[2]])
            }
            PadKind::Mosaic => {
                let c = 1 << self.s;
                let sx = (x.div_euclid(c) * c + c / 2).min(cv.width as i32 - 1);
                let sy = y.div_euclid(c) * c;
                match cv.get(sx, sy) {
                    Some(p) => argb_to_555(p),
                    None => return,
                }
            }
            PadKind::Marker => {
                let tx = if mode == Mode::Normal { self.texel(x, y, x0) } else { None };
                let t = self.coverage_t(a as i32, tx, mode, cx);
                let m = |c: i32, d: i32| round(c as f64 * t + d as f64 * (1.0 - t)).clamp(0, 31).min(d);
                pack555([m(col[0], d[0]), m(col[1], d[1]), m(col[2], d[2])])
            }
            _ => {
                let tx = if mode == Mode::Normal { self.texel(x, y, x0) } else { None };
                let t = self.coverage_t(a as i32, tx, mode, cx);
                if mode == Mode::Normal && !self.smear.is_empty() {
                    let s = self.smear[idx];
                    let n = |s: i32, d: i32| byte_clamp31((s as f64 * t + d as f64 * (1.0 - t) + 0.5) as i64);
                    let new = [n(s[0], d[0]), n(s[1], d[1]), n(s[2], d[2])];
                    if new != WHITE5 {
                        let k = self.p.smear_bias as f64 / 100.0;
                        let u = |s: i32, n: i32| clamp31(round(s as f64 * (1.0 - k) + n as f64 * k));
                        self.smear[idx] = [u(s[0], new[0]), u(s[1], new[1]), u(s[2], new[2])];
                    }
                    pack555(new)
                } else {
                    let n = |c: i32, d: i32| (c as f64 * t + d as f64 * (1.0 - t)) as i64 as i32;
                    pack555_unmasked([n(col[0], d[0]), n(col[1], d[1]), n(col[2], d[2])])
                }
            }
        };
        put555(cv, x, y, out);
    }

    /// One pad pass at top-left `r` (§3.4 `pass`).
    fn pass(&mut self, cv: &mut Image, r: (i32, i32), col: Rgb5, mode: Mode, cx: &mut Cx) {
        let Some(pad) = self.pad.take() else { return };
        let (w, h) = (cv.width as i32, cv.height as i32);
        for py in 0..pad.side {
            for px in 0..pad.side {
                let a = pad.a[(py * pad.side + px) as usize];
                if a == 0 {
                    continue;
                }
                let (x, y) = (r.0 + px, r.1 + py);
                if x < 0 || y < 0 || x >= w || y >= h {
                    continue;
                }
                if !self.spatter_keep(cx) {
                    continue;
                }
                let idx = if matches!(self.kind, PadKind::Effect(_)) {
                    (y * w + x) as usize
                } else {
                    (py * pad.side + px) as usize
                };
                self.pixel(cv, x, y, a, idx, r.0, col, mode, cx);
            }
        }
        self.pad = Some(pad);
    }

    // ---- §3.4 stamp and class overrides ------------------------------------------------

    fn stamp(&mut self, cv: &mut Image, dx: i32, dy: i32, cx: &mut Cx) {
        match self.kind {
            PadKind::RealBrush => return self.stamp_realbrush(cv, dx, dy),
            PadKind::Crayon => return self.stamp_crayon(cv, dx, dy, cx),
            PadKind::Extrusion => return self.stamp_extrusion(cv, dx, dy),
            PadKind::Smear => return self.stamp_smear(cv, dx, dy),
            PadKind::Blur => return self.stamp_blur(cv, dx, dy),
            _ => {}
        }
        self.rect.0 += dx;
        self.rect.1 += dy;
        let (ox, oy) = self.oscillation(cx);
        let r = (self.rect.0 + ox, self.rect.1 + oy);
        if self.p.shadow_offset != 0 {
            let o = self.p.shadow_offset as i32;
            self.pass(cv, (r.0 + o, r.1 + o), BLACK5, Mode::Shadow, cx);
        }
        if self.p.glint_offset != 0 {
            let o = self.p.glint_offset as i32;
            self.pass(cv, (r.0 - o, r.1 - o), WHITE5, Mode::Glint, cx);
        }
        let c = self.c5;
        self.pass(cv, r, c, Mode::Normal, cx);
    }

    fn for_pad(&self, cv: &Image, r: (i32, i32), mut f: impl FnMut(i32, i32, i32, i32, u8)) {
        let Some(pad) = &self.pad else { return };
        let (w, h) = (cv.width as i32, cv.height as i32);
        for py in 0..pad.side {
            for px in 0..pad.side {
                let a = pad.a[(py * pad.side + px) as usize];
                let (x, y) = (r.0 + px, r.1 + py);
                if a != 0 && x >= 0 && y >= 0 && x < w && y < h {
                    f(px, py, x, y, a);
                }
            }
        }
    }

    /// RealBrush "loaded brush" (`@0x44bdb0`).
    fn stamp_realbrush(&mut self, cv: &mut Image, dx: i32, dy: i32) {
        self.rect.0 += dx;
        self.rect.1 += dy;
        let Some(mut pad) = self.pad.take() else { return };
        let (w, h) = (cv.width as i32, cv.height as i32);
        let mut pickup = true;
        for py in 0..pad.side {
            for px in 0..pad.side {
                let pi = (py * pad.side + px) as usize;
                let a = pad.a[pi];
                let (x, y) = (self.rect.0 + px, self.rect.1 + py);
                if a == 0 || x < 0 || y < 0 || x >= w || y >= h {
                    continue;
                }
                let d = get5(cv, x, y).unwrap();
                if d == WHITE5 {
                    pickup = false;
                }
                let t = a as f64 / 254.0;
                let b = self.paint[pi];
                let n = |b: i32, d: i32| clamp31(round(b as f64 * t + d as f64 * (1.0 - t)));
                let new = [n(b[0], d[0]), n(b[1], d[1]), n(b[2], d[2])];
                if pickup {
                    let m = |n: i32, b: i32| clamp31(round(n as f64 * 0.1 + b as f64 * 0.9));
                    self.paint[pi] = [m(new[0], b[0]), m(new[1], b[1]), m(new[2], b[2])];
                }
                put5(cv, x, y, new);
                if self.counter % 100 == 0 && a > 64 {
                    pad.a[pi] = a - 1; // the pad dries and stays dry [O]
                }
                self.counter = self.counter.wrapping_add(1);
            }
        }
        self.pad = Some(pad);
    }

    /// Crayon (`@0x448090`).
    fn stamp_crayon(&mut self, cv: &mut Image, dx: i32, dy: i32, cx: &mut Cx) {
        self.rect.0 += dx;
        self.rect.1 += dy;
        let mut hits = Vec::new();
        let r = self.rect;
        self.for_pad(cv, r, |_, _, x, y, _| hits.push((x, y)));
        let c = self.c5;
        for (x, y) in hits {
            if cx.rng.rnd(0, 3) != 0 {
                continue;
            }
            let a = match self.texel(x, y, r.0) {
                Some(t) => t as i32 + 96,
                None => 64,
            };
            let t = a as f64 / 254.0;
            let d = get5(cv, x, y).unwrap();
            let n = |c: i32, d: i32| (c as f64 * t + d as f64 * (1.0 - t)) as i64 as i32;
            put555(cv, x, y, pack555_unmasked([n(c[0], d[0]), n(c[1], d[1]), n(c[2], d[2])]));
        }
    }

    /// ExtrusionBrush "3-D tube" (`@0x44a110`).
    fn stamp_extrusion(&mut self, cv: &mut Image, dx: i32, dy: i32) {
        self.rect.0 += dx;
        self.rect.1 += dy;
        let r = self.rect;
        let mut px_list = Vec::new();
        // 1. shadow at (+3,+3)
        self.for_pad(cv, (r.0 + 3, r.1 + 3), |_, _, x, y, a| px_list.push((x, y, a)));
        for (x, y, a) in px_list.drain(..) {
            let d = get5(cv, x, y).unwrap();
            let k = 1.0 - a as f64 / 5080.0;
            put5(cv, x, y, [trunc(d[0] as f64 * k), trunc(d[1] as f64 * k), trunc(d[2] as f64 * k)]);
        }
        // 2. highlight (Brushes.MHK 1030+s in a 33×33 buffer) in the upper-left quadrant
        if let Some(hp) = self.hilite.clone() {
            let off = if self.s >= 3 { 2 } else { 1 };
            let (w, h) = (cv.width as i32, cv.height as i32);
            let lim_x = r.0 + self.size / 2 + 2;
            let lim_y = r.1 + self.size / 2 + 2;
            for py in 0..hp.side {
                for px in 0..hp.side {
                    let a2 = hp.at(px, py);
                    let (x, y) = (r.0 + off + px, r.1 + off + py);
                    if a2 == 0 || x < 0 || y < 0 || x >= w || y >= h || y >= lim_y || x >= lim_x {
                        continue;
                    }
                    let d = get5(cv, x, y).unwrap();
                    let k = a2 as f64 / 508.0;
                    let m = |d: i32| clamp31(round(d as f64 * (1.0 - k) + 31.0 * k));
                    put5(cv, x, y, [m(d[0]), m(d[1]), m(d[2])]);
                }
            }
        }
        // 3. main pass
        self.for_pad(cv, r, |_, _, x, y, a| px_list.push((x, y, a)));
        let c = self.c5;
        for (x, y, a) in px_list {
            let d = get5(cv, x, y).unwrap();
            let t = a as f64 / 254.0;
            let m = |c: i32, d: i32| clamp31(round(c as f64 * t + d as f64 * (1.0 - t)));
            put5(cv, x, y, [m(c[0], d[0]), m(c[1], d[1]), m(c[2], d[2])]);
        }
    }

    /// SmearBrush (`@0x44e390`): drag the pixels under the old rect along.
    fn stamp_smear(&mut self, cv: &mut Image, dx: i32, dy: i32) {
        let (w, h) = (cv.width as i32, cv.height as i32);
        let n = self.size;
        let sx = self.rect.0.clamp(0, (w - n).max(0));
        let sy = self.rect.1.clamp(0, (h - n).max(0));
        let mut buf = vec![BLACK5; (n * n) as usize];
        for py in 0..n {
            for px in 0..n {
                buf[(py * n + px) as usize] = get5(cv, sx + px, sy + py).unwrap_or(WHITE5);
            }
        }
        self.rect.0 += dx;
        self.rect.1 += dy;
        let mut hits = Vec::new();
        self.for_pad(cv, self.rect, |px, py, x, y, a| hits.push((px, py, x, y, a)));
        for (px, py, x, y, a) in hits {
            let b = buf[(py * n + px) as usize];
            let d = get5(cv, x, y).unwrap();
            let t = a as f64 / 127.0;
            let m = |b: i32, d: i32| byte_clamp31((b as f64 * t + d as f64 * (1.0 - t) + 0.5) as i64);
            put5(cv, x, y, [m(b[0], d[0]), m(b[1], d[1]), m(b[2], d[2])]);
        }
    }

    /// BlurBrush (`@0x447310`): 3×3 box mean of a snapshot.
    fn stamp_blur(&mut self, cv: &mut Image, dx: i32, dy: i32) {
        self.rect.0 += dx;
        self.rect.1 += dy;
        let (w, h) = (cv.width as i32, cv.height as i32);
        let n = self.size;
        let (l, t) = (self.rect.0 - 1, self.rect.1 - 1);
        let sw = n + 2;
        let mut snap = vec![BLACK5; (sw * sw) as usize];
        for yy in 0..sw {
            for xx in 0..sw {
                let x = (l + xx).clamp(0, w - 1); // [I] edge pixels repeat
                let y = (t + yy).clamp(0, h - 1);
                snap[(yy * sw + xx) as usize] = get5(cv, x, y).unwrap();
            }
        }
        let mut hits = Vec::new();
        self.for_pad(cv, self.rect, |px, py, x, y, a| hits.push((px, py, x, y, a)));
        for (px, py, x, y, a) in hits {
            let mut sum = [0i32; 3];
            for oy in 0..3 {
                for ox in 0..3 {
                    let s = snap[((py + oy) * sw + px + ox) as usize];
                    for k in 0..3 {
                        sum[k] += s[k];
                    }
                }
            }
            let avg = [sum[0] / 9, sum[1] / 9, trunc(sum[2] as f64 * (1.0 / 9.0))];
            let d = get5(cv, x, y).unwrap();
            let tt = a as f64 / 255.0;
            let m = |s: i32, d: i32| clamp31(round(s as f64 * tt + d as f64 * (1.0 - tt)));
            put5(cv, x, y, [m(avg[0], d[0]), m(avg[1], d[1]), m(avg[2], d[2])]);
        }
    }

    // ---- effects (§3.9) ----------------------------------------------------------------

    fn effect(&self, e: Effect, s: &Image, x: i32, y: i32) -> Rgb5 {
        let c = get5(s, x, y).unwrap();
        let (h, sat, v) = rgb2hsv(c[0], c[1], c[2]);
        let fold = |v: i32| (if v > 15 { 31 - v } else { v } * 2).clamp(0, 31);
        let emboss_v = || {
            // [I] neighbour clamped into the picture (original has a bottom-rows quirk)
            let nx = (x + 2).min(s.width as i32 - 1);
            let ny = (y + 2).min(s.height as i32 - 1);
            let n = get5(s, nx, ny).unwrap();
            let (_, _, vn) = rgb2hsv(n[0], n[1], n[2]);
            (v - vn + 15).clamp(0, 31)
        };
        let (r, g, b) = match e {
            Effect::XRay => (31 - c[0], 31 - c[1], 31 - c[2]),
            Effect::Nuclear => hsv2rgb((h + 180) % 360, sat, v),
            Effect::HueShift => hsv2rgb((h + if self.space { 330 } else { 30 }) % 360, sat, v),
            Effect::Solarize => hsv2rgb(h, sat, fold(v)),
            Effect::Posterize => {
                let v2 = if self.space { ((v + 2) / 2) * 2 } else { ((v + 8) / 8) * 8 };
                hsv2rgb(h, sat, v2.min(31))
            }
            Effect::Emboss => hsv2rgb(h, 0, emboss_v()),
            Effect::Edge => hsv2rgb(h, 0, fold(emboss_v())),
        };
        [clamp31(r), clamp31(g), clamp31(b)]
    }

    fn finish_effect(&mut self, cv: &mut Image, e: Effect) {
        let Some(s) = self.backup.take() else { return };
        let (w, h) = (cv.width as i32, cv.height as i32);
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) as usize;
                let marked = self.marks[i];
                if self.space {
                    if marked {
                        cv.pixels[i] = s.pixels[i];
                    } else {
                        put5(cv, x, y, self.effect(e, &s, x, y));
                    }
                } else if marked {
                    put5(cv, x, y, self.effect(e, &s, x, y));
                }
            }
        }
        self.marks.clear();
    }

    // ---- drips -------------------------------------------------------------------------

    fn spawn_drip(&mut self, cv: &mut Image, p: (i32, i32), cx: &mut Cx) {
        let (base, n) = if self.s < 3 { (3200, 8) } else { (3208, 16) };
        let id = base + cx.rng.rnd(0, n - 1) as u16;
        cx.fx();
        let assets = cx.ctx.assets;
        let (Some(script), Some(set)) = (assets.script(ICONS_MHK, id), assets.sprite_set(ICONS_MHK, 3200)) else {
            // [I] no drip art: a short vertical run of paint
            let len = cx.rng.rnd(6, 6 + 6 * self.s);
            crate::raster::line_w(cv, (p.0, p.1), (p.0, p.1 + len), 2, self.c5);
            return;
        };
        let frames = script_frames(&script);
        let anchor = script_anchor(&frames);
        let mut d = Drip { set, frames, idx: 0, wait: 0.0, origin: p, anchor, color: self.c5 };
        Self::advance_drip(&mut d, cv, 0.0);
        self.drips.push(d);
    }

    fn advance_drip(d: &mut Drip, cv: &mut Image, dt: f32) {
        d.wait -= dt;
        while d.wait <= 0.0 && d.idx < d.frames.len() {
            draw_frame(cv, &d.set, &d.frames[d.idx], d.origin, d.anchor, Some(d.color));
            d.idx += 1;
            d.wait += SCRIPT_FRAME_MS;
        }
    }

    fn load(&mut self, cx: &mut Cx) {
        let s = cx.ctx.size.clamp(1, 3);
        self.s = s as i32;
        if self.p.dry_out_rate != 0 || s != self.pad_size || self.pad.is_none() {
            self.pad_size = s;
            let base = if self.p.brush_id != 0 { self.p.brush_id as i32 } else { 1001 };
            let id = (base + s as i32 - 1).clamp(0, 65535) as u16;
            self.pad = Some((*cx.ctx.assets.pad(id, None)).clone());
            if self.kind == PadKind::Extrusion {
                self.hilite = Some(cx.ctx.assets.pad(1030 + s as u16, Some(33)));
            }
        }
        self.size = self.pad.as_ref().map(|p| p.side).unwrap_or(1);
    }
}

impl BrushImpl for PadEngine {
    fn down(&mut self, cv: &mut Image, p: (i32, i32), cx: &mut Cx) {
        self.space = cx.ctx.space;
        self.load(cx);
        self.rect = (p.0 - self.size / 2, p.1 - self.size / 2);
        self.last = p;
        self.c8 = cx.ctx.color;
        self.c5 = palette5(cx.ctx.color);
        let n = (self.size * self.size) as usize;
        self.smear = if self.p.smear_bias != 0 { vec![self.c5; n] } else { Vec::new() };
        self.tex = if cx.ctx.texture != 0 && self.p.texture_bias > 0 { cx.ctx.assets.texture(cx.ctx.texture) } else { None };
        match self.kind {
            PadKind::RealBrush => {
                self.paint = vec![self.c5; n];
                self.counter = 0;
            }
            PadKind::Effect(_) => {
                self.backup = Some(cv.clone());
                self.marks = vec![false; (cv.width * cv.height) as usize];
            }
            PadKind::Drippy => self.next_drip = cx.now, // [I] first drip on the first tick
            _ => {}
        }
        self.stamp(cv, 0, 0, cx);
    }

    fn held(&mut self, cv: &mut Image, p: (i32, i32), cx: &mut Cx) {
        if self.p.spacing == 0 {
            if p == self.last {
                self.stamp(cv, 0, 0, cx);
            } else {
                for (dx, dy) in walk_line(self.last, p, self.step) {
                    self.stamp(cv, dx, dy, cx);
                }
                self.last = p;
            }
        } else if p != self.last && ((p.0 - self.last.0).abs() >= self.step || (p.1 - self.last.1).abs() >= self.step) {
            for (dx, dy) in walk_line(self.last, p, self.step) {
                self.stamp(cv, dx, dy, cx);
            }
            self.last = p;
        }
        if self.kind == PadKind::Drippy && cx.now >= self.next_drip {
            self.next_drip = cx.now + cx.rng.rnd(100, 150) as u64;
            self.spawn_drip(cv, p, cx);
        }
    }

    fn up(&mut self, cv: &mut Image, _p: (i32, i32), _cx: &mut Cx) {
        if let PadKind::Effect(e) = self.kind {
            self.finish_effect(cv, e);
        }
    }

    fn animate(&mut self, cv: &mut Image, dt: u32, _cx: &mut Cx) {
        for d in &mut self.drips {
            Self::advance_drip(d, cv, dt as f32);
        }
        self.drips.retain(|d| d.idx < d.frames.len());
    }

    fn animating(&self) -> bool {
        !self.drips.is_empty()
    }

    fn deactivate(&mut self, cv: &mut Image, _cx: &mut Cx) {
        for d in &mut self.drips {
            while d.idx < d.frames.len() {
                Self::advance_drip(d, cv, SCRIPT_FRAME_MS);
            }
        }
        self.drips.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walk_ends_on_target() {
        for &(b, step) in &[((10, 3), 3), ((-7, 12), 1), ((5, 5), 4), ((0, -9), 2), ((1, 0), 3)] {
            let v = walk_line((0, 0), b, step);
            let s = v.iter().fold((0, 0), |a, d| (a.0 + d.0, a.1 + d.1));
            assert_eq!(s, b);
        }
        assert_eq!(walk_line((0, 0), (9, 0), 3), vec![(3, 0), (3, 0), (3, 0)]);
        assert_eq!(walk_line((0, 0), (10, 0), 3), vec![(3, 0), (3, 0), (3, 0), (1, 0)]);
        assert!(walk_line((4, 4), (4, 4), 3).is_empty());
    }
}
