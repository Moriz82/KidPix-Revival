//! Sound Art brushes (RE_BRUSHES §6): `CustomVoiceBrush1–10` (voice + mouse) and
//! `Touchless1–10` (hands-free), transcribed from KPD3.exe.
//!
//! The engine supplies the microphone analysis through [`crate::Ctx::voice`]
//! ([`crate::Voice::analyze`]: 512 FFT bin magnitudes, peak bin `freq`, peak `amp`).
//! Per tick (`v8`, `@0x450380` / hands-free `@0x44b1f0`) the brush scales them into
//! `A = amp · amplitudeBias / 100` and `F = freq · frequencyBias / 100`, recolours, moves
//! along the mouse (or its own physics) and calls the class's pad routine (`v14`) once per
//! step. Shapes are GDI ovals in the 5-bit voice colour.

use std::sync::Arc;

use crate::color::*;
use crate::config::BrushParams;
use crate::pad::{walk_line, DEFAULT_STEP};
use crate::raster::*;
use crate::tool::{BrushImpl, Cx};
use mohawk::Image;

pub struct VoiceBrush {
    p: BrushParams,
    /// 1..=10
    n: u8,
    hands_free: bool,
    /// Word 0x598: set by mouse down (or hands-free activation), cleared by mouse up.
    armed: bool,
    step: i32,
    /// Pad size (field 0x2a), from the class's `v26`.
    size: i32,
    /// Panel size 1..=3 (field 0x136).
    s: i32,
    /// Pad rect top-left (field 0x50); side = `size`.
    rect: (i32, i32),
    /// Last brush point (field 0x18).
    last: (i32, i32),
    /// Line start of CVB10 (0x5b8) / Touchless10 (0x638).
    from: (i32, i32),
    base: Rgb5,
    color: Rgb5,
    spectrum: Arc<[u16]>,
    a: f64,
    f: f64,
    /// Largest peak bin / magnitude since the stroke began (0x57e / 0x57c).
    max_freq: i16,
    max_amp: i16,
    // ---- hands-free (RHandsFreeBrush) ----
    running: bool,
    pos: (f64, f64),
    vel: (f64, f64),
    grav: (f64, f64),
    center: (i32, i32),
    bounds: (i32, i32, i32, i32),
    /// 0x5a4: time of the last update; the input callback turns it into "ms since".
    t_last: u32,
    /// 0x5a0: moves since the last hue step (only read by the colour getter).
    moves: u32,
}

/// `timeGetTime()` stand-in: the tool clock plus a boot offset, so that a value turned
/// into "elapsed" and back behaves like the original's (always large).
fn clock(cx: &Cx) -> u32 {
    (cx.now as u32).wrapping_add(10_000_000)
}

impl VoiceBrush {
    pub fn new(p: BrushParams, n: u8, hands_free: bool) -> VoiceBrush {
        let step = if p.spacing != 0 { p.spacing as i32 } else { DEFAULT_STEP };
        VoiceBrush {
            p,
            n: n.clamp(1, 10),
            hands_free,
            armed: false,
            step,
            size: 1,
            s: 1,
            rect: (0, 0),
            last: (0, 0),
            from: (0, 0),
            base: BLACK5,
            color: BLACK5,
            spectrum: Arc::from(Vec::new()),
            a: 0.0,
            f: 0.0,
            max_freq: 0,
            max_amp: 0,
            running: false,
            pos: (0.0, 0.0),
            vel: (0.0, 0.0),
            grav: (0.0, 0.0),
            center: (0, 0),
            bounds: (0, 0, 600, 452),
            t_last: 0,
            moves: 0,
        }
    }

    /// `v26`: the pad size. Every class but CVB10 takes `brushID` when it is 1..127 and
    /// otherwise its own default; CVB10 keeps the base class's Brushes.MHK pad
    /// (`1001 + size - 1`).
    fn pad_size(&self, cx: &Cx) -> i32 {
        let id = self.p.brush_id;
        if !self.hands_free && self.n == 10 {
            let pid = if id != 0 { id as i32 + self.s - 1 } else { 1001 + self.s - 1 };
            return cx.ctx.assets.pad(pid.clamp(0, 0xffff) as u16, None).side.max(1);
        }
        if id > 0 && id < 128 {
            return id as i32;
        }
        match (self.hands_free, self.n) {
            (false, 1 | 2) => 5,
            (_, 5) => 2,
            (false, 7) => 3,
            _ => 1,
        }
    }

    /// `v41` (`@0x4501a0`): random offset within `min(s·trunc(A), oscillation)`.
    fn osc(&self, cx: &mut Cx) -> (i32, i32) {
        let m = (self.s.wrapping_mul(trunc(self.a)) as i16).min(self.p.oscillation);
        if m != 0 {
            let r = cx.rng.rand() % m as i32;
            if r != 0 {
                // the angle is degrees passed to cos/sin as radians
                let th = (cx.rng.rand() % 360) as f64;
                return (trunc(th.cos() * r as f64) as i16 as i32, trunc(th.sin() * r as f64) as i16 as i32);
            }
        }
        (0, 0)
    }

    fn mag(&self, i: usize) -> u16 {
        self.spectrum.get(i).copied().unwrap_or(0)
    }

    /// Reset the voice state and take the brush colour (shared mouse-down part, `@0x450670`).
    fn arm(&mut self, p: (i32, i32), cx: &Cx) {
        self.s = cx.ctx.size.clamp(1, 3) as i32;
        self.size = self.pad_size(cx);
        self.rect = (p.0 - self.size / 2, p.1 - self.size / 2);
        self.last = p;
        self.a = 0.0;
        self.f = 0.0;
        self.max_freq = 0;
        self.max_amp = 0;
        self.moves = 0;
        self.t_last = clock(cx);
        let c = cx.ctx.color;
        let c5 = |v: u8| ((255 / 2 + v as i32 * 31) / 255).min(31);
        self.base = [c5(c[0]), c5(c[1]), c5(c[2])];
        self.color = self.base;
        self.armed = true;
    }

    /// Take the microphone analysis. Returns false when it reported silence.
    fn listen(&mut self, cx: &Cx, threshold: f64) -> bool {
        let v = &cx.ctx.voice;
        self.spectrum = v.spectrum.clone();
        if v.freq == 0 || v.amp == 0 {
            return false;
        }
        if v.freq as i32 > self.max_freq as i32 {
            self.max_freq = v.freq as i16;
        }
        if v.amp as i32 > self.max_amp as i32 {
            self.max_amp = v.amp as i16;
        }
        self.a = self.p.amplitude_bias as f64 * 0.01 * v.amp as f64;
        self.f = self.p.frequency_bias as f64 * 0.01 * v.freq as f64;
        self.recolor(threshold);
        true
    }

    /// Above the threshold the hue of the brush colour is shifted by `F` degrees.
    fn recolor(&mut self, threshold: f64) {
        if self.f <= threshold {
            self.color = self.base;
        } else {
            let (h, s, v) = rgb2hsv(self.base[0], self.base[1], self.base[2]);
            let h = (h as f64 + self.f) as i64 as i16;
            let (r, g, b) = hsv2rgb((h % 360) as i32, s, v);
            self.color = [r, g, b];
        }
    }

    /// Walk from the last point to `p` (`v15`): the pad rect is first centred on the
    /// start point (`v16`), then the class routine runs once per step.
    fn walk(&mut self, cv: &mut Image, to: (i32, i32), cx: &mut Cx) {
        let from = self.last;
        self.rect = (from.0 - self.size / 2, from.1 - self.size / 2);
        for (dx, dy) in walk_line(from, to, self.step) {
            self.step_pad(cv, dx, dy, cx);
        }
    }

    /// `v14` of the class, after its move part.
    fn step_pad(&mut self, cv: &mut Image, dx: i32, dy: i32, cx: &mut Cx) {
        if !self.armed {
            return;
        }
        if self.hands_free {
            self.confine(dx, dy);
        }
        if self.n != 10 {
            self.rect.0 += dx;
            self.rect.1 += dy;
        }
        if self.hands_free {
            self.touchless(cv, dx, dy, cx);
        } else {
            self.voice(cv, dx, dy, cx);
        }
    }

    fn r(&self) -> (i32, i32, i32, i32) {
        (self.rect.0, self.rect.1, self.rect.0 + self.size, self.rect.1 + self.size)
    }

    /// `rand() % (u16)trunc(x) + 1`, capped (unsigned) at `cap`.
    fn grow(cx: &mut Cx, x: f64, cap: i32) -> i32 {
        let d = trunc(x) as u16 as i32;
        let r = cx.rng.rand();
        let g = if d == 0 { 1 } else { r % d + 1 };
        if g as u16 as i32 > cap {
            cap
        } else {
            g
        }
    }

    /// `-(trunc(A) & 0xffff) % d` (C remainder) or 0.
    fn neg_mod(a: f64, d: i32) -> i32 {
        if d as i16 == 0 {
            0
        } else {
            -(trunc(a) & 0xffff) % (d as i16 as i32)
        }
    }

    /// CustomVoiceBrush1–10 `v14`.
    fn voice(&mut self, cv: &mut Image, dx: i32, dy: i32, cx: &mut Cx) {
        let (a, f, s) = (self.a, self.f, self.s);
        let (l, t, r, b) = self.r();
        let c = self.color;
        match self.n {
            1 if a != 0.0 => {
                let (ox, oy) = self.osc(cx);
                let g = Self::grow(cx, (f + 100.0) * 0.01, 4 * s);
                paint_oval(cv, l - ox - g, t - oy - g, r - ox + g, b - oy + g, c);
            }
            2 if a != 0.0 => {
                let (ox, oy) = self.osc(cx);
                let g = Self::grow(cx, (a + 100.0) * 0.01, 12 * s);
                frame_oval(cv, l - ox - g, t - oy - g, r - ox + g, b - oy + g, 1, c);
            }
            3 if f != 0.0 => {
                let (ox, oy) = self.osc(cx);
                let g = Self::grow(cx, (f + 10.0) * 0.1, 12 * s);
                frame_oval(cv, l - ox - g, t - oy - g, r - ox + g, b - oy + g, 1, c);
            }
            4 if a != 0.0 && f != 0.0 => {
                let (ox, oy) = self.osc(cx);
                let mut g = 1 - trunc((f + 10.0) * -0.1);
                if g as u16 as i32 > 12 * s {
                    g = 12 * s;
                }
                invert_oval(cv, l - ox - g, t - oy - g, r - ox + g, b - oy + g);
            }
            5 => {
                // one filled oval per bin up to the highest peak bin so far; bins with
                // sound are scattered by the oscillation
                for i in 0..=self.max_freq.max(-1) as i32 {
                    let (mut ox, mut oy) = (0, 0);
                    if self.mag(i as usize) != 0 {
                        let o = self.osc(cx);
                        ox = o.0 * s;
                        oy = o.1 * s;
                    }
                    paint_oval(cv, l + ox, t + oy, r + ox, b + oy, c);
                }
            }
            6 => {
                let (ox, oy) = self.osc(cx);
                let ry = Self::neg_mod(a, oy);
                let rx = Self::neg_mod(a, ox);
                let (ox, oy) = (ox * s, oy * s);
                frame_oval(cv, l + rx + ox, t + ry + oy, r - rx + ox, b - ry + oy, 1, c);
            }
            7 => self.hue_rings_rand(cv, cx, 0),
            8 => {
                let k = trunc(a);
                let (ox, oy) = self.osc(cx);
                let w = Self::grow(cx, (f + 10.0) * 0.1, 2 * s);
                frame_oval(cv, l - k + ox, t - k + oy, r + k + ox, b + k + oy, w, c);
            }
            9 => {
                let d = -6 / s;
                let k = trunc(a / d as f64);
                let (ox, oy) = self.osc(cx);
                let (l, t, r, b) = (l + k + ox, t + k + oy, r - k + ox, b - k + oy);
                invert_oval(cv, l, t, r, b);
                frame_oval(cv, l, t, r, b, 1, c);
            }
            10 => {
                let (ox, oy) = self.osc(cx);
                let q = (l + dx + ox, t + dy + oy);
                line1(cv, self.from, q, c);
                self.from = q;
            }
            _ => {}
        }
    }

    /// CVB7 / Touchless7: per bin up to the highest peak so far, a ring; bins louder than
    /// `min` are scattered, take the base hue shifted by their magnitude (which becomes
    /// the brush colour) and grow by `(mag/100 + 1)·s`.
    fn hue_rings_rand(&mut self, cv: &mut Image, cx: &mut Cx, min: u16) {
        let (l, t, r, b) = self.r();
        let s = self.s;
        let mut pen = self.color;
        for i in 0..=self.max_freq.max(-1) as i32 {
            let m = self.mag(i as usize);
            if m > min {
                let (ox, oy) = self.osc(cx);
                let (h, hs, hv) = rgb2hsv(self.base[0], self.base[1], self.base[2]);
                let h = (h + m as i32) as i16;
                let (cr, cg, cb) = hsv2rgb((h % 360) as i32, hs, hv);
                self.color = [cr, cg, cb];
                pen = self.color;
                let g = ((m as i32 / 100 + 1) * s) as i16 as i32;
                frame_oval(cv, l + ox - g, t + oy - g, r + ox + g, b + oy + g, 1, pen);
            } else {
                frame_oval(cv, l, t, r, b, 1, pen);
            }
        }
    }

    /// Touchless1–10 `v14` (after `v46` moved the point and the doubled rect move).
    fn touchless(&mut self, cv: &mut Image, dx: i32, dy: i32, cx: &mut Cx) {
        let (a, f, s) = (self.a, self.f, self.s);
        let (l, t, r, b) = self.r();
        let c = self.color;
        match self.n {
            1 if a != 0.0 => {
                let (ox, oy) = self.osc(cx);
                let g = Self::grow(cx, (f + 100.0) * 0.01, 4 * s);
                paint_oval(cv, l - ox - g, t - oy - g, r - ox + g, b - oy + g, c);
            }
            2 if a != 0.0 => {
                let (ox, oy) = self.osc(cx);
                let g = Self::grow(cx, (a + 100.0) * 0.01, 4 * s);
                frame_oval(cv, l - ox - g, t - oy - g, r - ox + g, b - oy + g, 1, c);
            }
            3 if f != 0.0 => {
                let k = trunc(a);
                let (ox, oy) = self.osc(cx);
                let w = Self::grow(cx, (f + 10.0) * 0.1, 2 * s);
                frame_oval(cv, l - k + ox, t - k + oy, r + k + ox, b + k + oy, w, c);
            }
            4 if a != 0.0 && f != 0.0 => {
                let (ox, oy) = self.osc(cx);
                let (gx, gy) = (ox * s, oy * s);
                invert_oval(cv, l - ox - gx, t - oy - gy, r - ox + gx, b - oy + gy);
            }
            5 => {
                for i in 0..=self.max_freq.max(-1) as i32 {
                    let (mut ox, mut oy) = (0, 0);
                    if self.mag(i as usize) != 0 {
                        let o = self.osc(cx);
                        ox = o.0 * s;
                        oy = o.1 * s;
                    }
                    paint_oval(cv, l + ox, t + oy, r + ox, b + oy, c);
                }
            }
            6 => {
                let (ox, oy) = self.osc(cx);
                let ry = Self::neg_mod(a, oy);
                let rx = Self::neg_mod(a, ox);
                let (ox, oy) = (ox * s, oy * s);
                frame_oval(cv, l + rx + ox, t + ry + oy, r - rx + ox, b - ry + oy, 1, c);
            }
            7 => self.hue_rings_rand(cv, cx, 10),
            8 => {
                let k = trunc(a);
                let (ox, oy) = self.osc(cx);
                let (ox, oy) = (ox * s, oy * s);
                frame_oval(cv, l - k + ox, t - k + oy, r + k + ox, b + k + oy, 1, c);
            }
            9 => {
                let k = trunc(a * -0.1);
                let (ox, oy) = self.osc(cx);
                let (ox, oy) = (ox * s, oy * s);
                let (l, t, r, b) = (l + k + ox, t + k + oy, r - k + ox, b - k + oy);
                invert_oval(cv, l, t, r, b);
                frame_oval(cv, l, t, r, b, 1, c);
            }
            10 => {
                let (ox, oy) = self.osc(cx);
                let q = (l + dx + ox, t + dy + oy);
                line1(cv, self.from, q, c);
                self.from = q;
            }
            _ => {}
        }
    }

    /// `v46` (`@0x44b670`): move the pad rect and the point by one step, then keep the
    /// rect inside the canvas — wrapped around when `wrap`, else pinned to the edge (the
    /// velocity is not reversed). The position follows the point.
    fn confine(&mut self, dx: i32, dy: i32) {
        self.moves += 1;
        self.rect.0 += dx;
        self.rect.1 += dy;
        let (bl, bt, br, bb) = self.bounds;
        let size = self.size;
        if self.p.wrap != 0 {
            let (l, r) = (self.rect.0, self.rect.0 + size);
            if l < bl {
                let d = br - size - bl;
                self.rect.0 += d;
                self.last.0 += d;
            } else if r > br {
                let d = bl + r - br - l;
                self.rect.0 += d;
                self.last.0 += d;
            } else {
                self.last.0 += dx;
            }
            let t = self.rect.1;
            if t < bt {
                self.rect.1 += bb - bt;
                self.last.1 += bb - bt;
            } else if t > bb {
                self.rect.1 += bt - bb;
                self.last.1 += bt - bb;
            } else {
                self.last.1 += dy;
            }
        } else {
            self.last.0 += dx;
            self.last.1 += dy;
            let pin_x = |x: i32| if x < bl { bl } else if x >= br { br - 1 } else { x };
            let pin_y = |y: i32| if y < bt { bt } else if y >= bb { bb - 1 } else { y };
            let (l, t, r, b) = (self.rect.0, self.rect.1, self.rect.0 + size, self.rect.1 + size);
            if l < bl {
                let d = pin_x(l) - l;
                self.last.0 += d;
                self.rect.0 += d;
            } else if r > br {
                let d = pin_x(r) - r;
                self.last.0 += d;
                self.rect.0 += d;
            }
            if t < bt {
                let d = pin_y(t) - t;
                self.last.1 += d;
                self.rect.1 += d;
            } else if b > bb {
                let d = pin_y(b) - b;
                self.last.1 += d;
                self.rect.1 += d;
            }
        }
        self.pos = (self.last.0 as f64, self.last.1 as f64);
    }

    /// One hands-free update (`@0x44b1f0`), driven by the input callback (`@0x44b170`).
    fn hands_free_tick(&mut self, cv: &mut Image, cx: &mut Cx) {
        if !self.armed || !cx.ctx.voice.live {
            return;
        }
        // the callback turns the stored time into "ms since"
        let now = clock(cx);
        self.t_last = now.wrapping_sub(self.t_last);
        let v = cx.ctx.voice.clone();
        self.spectrum = v.spectrum.clone();
        let sound = v.freq != 0 && v.amp != 0;
        if !(sound || self.t_last > self.p.gravity_time as i32 as u32) {
            self.a = 0.0;
            self.f = 0.0;
            return;
        }
        if v.freq as i32 > self.max_freq as i32 {
            self.max_freq = v.freq as i16;
        }
        if v.amp as i32 > self.max_amp as i32 {
            self.max_amp = v.amp as i16;
        }
        self.a = self.p.amplitude_bias as f64 * 0.01 * v.amp as f64;
        self.f = self.p.frequency_bias as f64 * 0.01 * v.freq as f64;
        self.recolor(1.0);
        self.t_last = now;
        // the voice pushes: |A| along the angle F° (taken as radians, as the original)
        let mag = trunc(self.a) as u16 as f64;
        let ang = ((trunc(self.f) as u16) % 360) as f64;
        let push = (trunc(ang.cos() * mag) as i16 as f64, trunc(ang.sin() * mag) as i16 as f64);
        if self.p.gravity_bias != 0 {
            // pulled back to the centre, ∝ distance², beyond gravityRange
            let d = (self.center.0 as f64 - self.pos.0, self.center.1 as f64 - self.pos.1);
            let d2 = d.0 * d.0 + d.1 * d.1;
            let dist = d2.sqrt();
            let g = if dist <= self.p.gravity_range as f64 {
                (0.0, 0.0)
            } else {
                let k = self.p.gravity_bias as f64 * 0.001 * d2;
                (k * d.0 / dist, k * d.1 / dist)
            };
            let acc = self.p.acceleration_bias as f64 * 0.1;
            self.grav = (acc * g.0, acc * g.1);
        }
        let mut vx = push.0 + self.grav.0 + self.vel.0;
        let mut vy = push.1 + self.grav.1 + self.vel.1;
        vx = vx.clamp(-50.0, 50.0);
        vy = vy.clamp(-50.0, 50.0);
        if self.p.damp_bias != 0 {
            let d = self.p.damp_bias as f64 * 0.01;
            if vx > d {
                vx -= d;
            } else if vx < -d {
                vx += d;
            }
            if vy > d {
                vy -= d;
            } else if vy < -d {
                vy += d;
            }
        }
        self.vel = (vx, vy);
        self.pos.0 += vx;
        self.pos.1 += vy;
        let to = (trunc(self.pos.0) as i16 as i32, trunc(self.pos.1) as i16 as i32);
        self.walk(cv, to, cx);
    }
}

impl BrushImpl for VoiceBrush {
    fn down(&mut self, _cv: &mut Image, p: (i32, i32), cx: &mut Cx) {
        if self.hands_free {
            // Touchless1–9 ignore clicks; Touchless10 restarts its line at the click
            if self.n == 10 && self.running {
                self.arm(p, cx);
                self.from = p;
            }
            return;
        }
        // the base mouse-down's first pad call runs before the brush is armed: no stamp
        self.arm(p, cx);
        self.from = p;
    }

    fn held(&mut self, cv: &mut Image, p: (i32, i32), cx: &mut Cx) {
        if self.hands_free || !self.armed || !cx.ctx.voice.live {
            return;
        }
        if !self.listen(cx, 10.0) {
            self.a = 0.0;
            self.f = 0.0;
        }
        if self.p.spacing != 0 {
            if p != self.last && ((p.0 - self.last.0).abs() >= self.p.spacing as i32 || (p.1 - self.last.1).abs() >= self.p.spacing as i32) {
                self.walk(cv, p, cx);
                self.last = p;
            }
        } else if p != self.last {
            self.walk(cv, p, cx);
            self.last = p;
        } else {
            self.step_pad(cv, 0, 0, cx);
        }
    }

    fn up(&mut self, _cv: &mut Image, _p: (i32, i32), _cx: &mut Cx) {
        if !self.hands_free {
            self.armed = false;
        }
    }

    /// Hands-free: start at the canvas centre with the current colour (`@0x44ae90`).
    fn activate(&mut self, cv: &mut Image, cx: &mut Cx) {
        if !self.hands_free || self.running {
            return;
        }
        self.bounds = (0, 0, cv.width as i32, cv.height as i32);
        self.center = (cv.width as i32 / 2, cv.height as i32 / 2);
        self.arm(self.center, cx);
        self.pos = (self.center.0 as f64, self.center.1 as f64);
        self.vel = (0.0, 0.0);
        self.grav = (0.0, 0.0);
        self.from = self.center;
        self.running = true;
    }

    fn animate(&mut self, cv: &mut Image, _dt: u32, cx: &mut Cx) {
        if self.hands_free && self.running {
            self.hands_free_tick(cv, cx);
        }
    }

    fn animating(&self) -> bool {
        self.hands_free && self.running
    }

    fn deactivate(&mut self, _cv: &mut Image, _cx: &mut Cx) {
        self.running = false;
        self.armed = false;
    }
}
