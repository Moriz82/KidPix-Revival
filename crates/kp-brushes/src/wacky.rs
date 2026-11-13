//! The Wacky Brushes (RE_BRUSHES §5), ids 20–44 except Drippy (a pad brush, `pad.rs`).

use crate::assets::ICONS_MHK;
use crate::color::*;
use crate::raster::*;
use crate::rng::MsRand;
use crate::tool::{BrushImpl, Cx};
use mohawk::Image;
use std::f64::consts::PI;
use std::sync::Arc;

/// RTimedTool timer: `next = now + interval` at mouse down; a tick fires when
/// `now > next`, then `next += interval` (one fire per tick at most) [O].
#[derive(Default, Clone, Copy)]
struct Timer {
    interval: u64,
    next: u64,
}

impl Timer {
    fn start(&mut self, now: u64, interval: u64) {
        self.interval = interval;
        self.next = now + interval;
    }
    fn due(&mut self, now: u64) -> bool {
        if now > self.next {
            self.next += self.interval;
            true
        } else {
            false
        }
    }
}

fn cur(cx: &Cx) -> Rgb5 {
    gdi_rgb5(cx.ctx.color)
}

fn rand_gdi(rng: &mut MsRand) -> Rgb5 {
    gdi_rgb5(rng.color())
}

fn restore(cv: &mut Image, snap: &Option<Image>) {
    if let Some(s) = snap {
        if s.pixels.len() == cv.pixels.len() {
            cv.pixels.copy_from_slice(&s.pixels);
        }
    }
}

/// `Pt2Rect`: rect spanned by two points, `[min, max)`.
fn pt2rect(a: (i32, i32), b: (i32, i32)) -> (i32, i32, i32, i32) {
    (a.0.min(b.0), a.1.min(b.1), a.0.max(b.0), a.1.max(b.1))
}

// ---------------------------------------------------------------------------------- 5.1

/// Madona: 3-D pyramid (drag tool, `@0x43fe20`).
#[derive(Default)]
pub struct Madona {
    snap: Option<Image>,
    start: (i32, i32),
    s: i32,
    faces: [[u8; 3]; 4],
    last: (i32, i32),
}

impl Madona {
    fn draw(&self, cv: &mut Image, apex: (i32, i32)) {
        let (x0, y0) = self.start;
        let l = x0 - self.s;
        let t = y0 - self.s;
        let (r, b) = (l + 2 * self.s, t + 2 * self.s);
        let edges = [((l, t), (r, t)), ((r, t), (r, b)), ((r, b), (l, b)), ((l, b), (l, t))];
        let (dx, dy) = (apex.0 - x0, apex.1 - y0);
        let d = if dx.abs() < dy.abs() {
            if dy < 0 {
                0
            } else {
                2
            }
        } else if dx >= 0 {
            1
        } else {
            3
        };
        for k in [0usize, 1, 3, 2] {
            let f = k ^ d;
            let (e0, e1) = edges[f];
            fill_polygon(cv, &[e0, e1, apex], gdi_rgb5(self.faces[f]));
        }
    }
}

impl BrushImpl for Madona {
    fn down(&mut self, cv: &mut Image, p: (i32, i32), cx: &mut Cx) {
        self.snap = Some(cv.clone());
        self.start = p;
        self.s = cx.size();
        for f in &mut self.faces {
            *f = cx.rng.color();
        }
        self.last = p;
        self.draw(cv, p); // [I] a click alone shows the pyramid from above
    }
    fn held(&mut self, cv: &mut Image, p: (i32, i32), _cx: &mut Cx) {
        if p != self.last {
            restore(cv, &self.snap);
            self.draw(cv, p);
            self.last = p;
        }
    }
    fn up(&mut self, _cv: &mut Image, _p: (i32, i32), _cx: &mut Cx) {
        self.snap = None;
    }
}

// ---------------------------------------------------------------------------------- 5.3

/// Spiral (timed 50 ms, `@0x4ae6f0`).
pub struct Spiral {
    colors: [[u8; 3]; 16],
    centre: (i32, i32),
    alpha: f32,
    r: f32,
    ci: usize,
    prev: (i32, i32),
    timer: Timer,
    cos: [f32; 32],
    sin: [f32; 32],
}

const PI16: f32 = 2.0 / 32.0 * std::f32::consts::PI; // flt_58DC40

impl Spiral {
    pub fn new(rng: &mut MsRand) -> Spiral {
        let mut colors = [[0u8; 3]; 16];
        for c in &mut colors {
            *c = rng.color();
        }
        let mut cos = [0f32; 32];
        let mut sin = [0f32; 32];
        for k in 0..32 {
            cos[k] = (k as f32 * PI16).cos();
            sin[k] = (k as f32 * PI16).sin();
        }
        Spiral { colors, centre: (0, 0), alpha: 0.0, r: 1.5, ci: 1, prev: (0, 0), timer: Timer::default(), cos, sin }
    }

    fn segments(&mut self, cv: &mut Image, cx: &Cx) {
        let k = cx.ctx;
        for _ in 0..20 {
            if !k.space {
                self.ci = (self.ci + 1) % 16;
            }
            let w = if k.ctrl || k.shift { 3 } else { 1 };
            let i = ((self.alpha / PI16) as i32 - if self.alpha != 0.0 { 1 } else { 0 }).rem_euclid(32) as usize;
            let np = (
                self.centre.0 + (self.r * self.cos[i]) as i32,
                self.centre.1 + (self.r * self.sin[i]) as i32,
            );
            line_w(cv, self.prev, np, w, gdi_rgb5(self.colors[self.ci]));
            self.prev = np;
            self.alpha += PI16;
            if self.alpha >= 2.0 * std::f32::consts::PI {
                self.alpha = 0.0;
            }
            self.r += if k.ctrl {
                0.2
            } else if k.shift {
                0.085
            } else {
                0.075
            };
        }
    }
}

impl BrushImpl for Spiral {
    fn down(&mut self, _cv: &mut Image, p: (i32, i32), cx: &mut Cx) {
        self.centre = p;
        self.alpha = 0.0;
        self.r = 1.5;
        self.ci = 1;
        self.prev = p;
        self.timer.start(cx.now, 50);
    }
    fn held(&mut self, cv: &mut Image, _p: (i32, i32), cx: &mut Cx) {
        // Movement is ignored: the spiral keeps growing around the click point [O].
        if self.timer.due(cx.now) {
            self.segments(cv, cx);
        }
    }
}

// ---------------------------------------------------------------------------------- 5.4

/// Poly: filled polygon, a vertex is fixed after 1000 ms (drag tool, `@0x442010`).
#[derive(Default)]
pub struct Poly {
    snap: Option<Image>,
    pts: Vec<(i32, i32)>,
    last_fix: u64,
    last: (i32, i32),
}

impl BrushImpl for Poly {
    fn down(&mut self, cv: &mut Image, p: (i32, i32), cx: &mut Cx) {
        self.snap = Some(cv.clone());
        self.pts = vec![p];
        self.last_fix = cx.now;
        self.last = p;
    }
    fn held(&mut self, cv: &mut Image, p: (i32, i32), cx: &mut Cx) {
        if p == self.last {
            return;
        }
        if cx.now.saturating_sub(self.last_fix) > 1000 {
            self.pts.push(p);
            self.last_fix = cx.now;
        }
        restore(cv, &self.snap);
        let mut v = self.pts.clone();
        if v.last() != Some(&p) {
            v.push(p);
        }
        fill_polygon(cv, &v, cur(cx));
        self.last = p;
    }
    fn up(&mut self, _cv: &mut Image, _p: (i32, i32), _cx: &mut Cx) {
        self.snap = None;
    }
}

// ---------------------------------------------------------------------------------- 5.5

/// TriSpin: rotating Gouraud triangle; the angle persists across strokes (`@0x446b50`).
#[derive(Default)]
pub struct TriSpin {
    angle_deg: f64,
    last: (i32, i32),
}

impl BrushImpl for TriSpin {
    fn down(&mut self, _cv: &mut Image, p: (i32, i32), _cx: &mut Cx) {
        self.last = p;
    }
    fn held(&mut self, cv: &mut Image, p: (i32, i32), cx: &mut Cx) {
        if p == self.last {
            return;
        }
        self.last = p;
        let s = cx.size() as f64;
        let a = self.angle_deg.to_radians();
        let (ca, sa) = (a.cos(), a.sin());
        let tr = |x: f64, y: f64| {
            let (rx, ry) = (x * ca - y * sa, x * sa + y * ca);
            // GL y points up [I]
            ((p.0 as f64 + 0.5 + rx * s) as f32, (p.1 as f64 + 0.5 - ry * s) as f32)
        };
        let v = [tr(1.0, 1.0), tr(-1.0, -1.0), tr(1.0, -1.0)];
        let col = [[1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [1.0, 1.0, 0.0]];
        gl_triangle(cv, v, col, if cx.ctx.shift { 0.085 } else { 0.3 });
        self.angle_deg = (self.angle_deg + 6.0) % 360.0;
    }
}

// ---------------------------------------------------------------------------------- 5.6

/// WhispyCircle: two translucent rings per move (`@0x446d40`).
#[derive(Default)]
pub struct Whispy {
    last: (i32, i32),
}

impl BrushImpl for Whispy {
    fn down(&mut self, _cv: &mut Image, p: (i32, i32), _cx: &mut Cx) {
        self.last = p;
    }
    fn held(&mut self, cv: &mut Image, p: (i32, i32), cx: &mut Cx) {
        if p == self.last {
            return;
        }
        self.last = p;
        let rr = cx.ctx.size.clamp(1, 3) as f32 * 10.0 - 5.0;
        let c = cx.ctx.color;
        let f = |v: u8| v as f32 / 255.0;
        let pale = [f(255 - c[0] / 2), f(255 - c[1] / 2), f(255 - c[2] / 2)];
        let alpha = if cx.ctx.shift { 25.0 / 255.0 } else { 60.0 / 255.0 };
        let (x, y) = (p.0 as f32 + 0.5, p.1 as f32 + 0.5);
        gl_ring(cv, x, y, rr - 3.0, rr - 2.0, pale, alpha);
        gl_ring(cv, x, y, rr - 1.0, rr, [f(c[0]), f(c[1]), f(c[2])], alpha);
    }
}

// ---------------------------------------------------------------------------------- 5.7

#[derive(Clone)]
struct Particle {
    pos: [f32; 3],
    dir: [f32; 3],
    life: i32,
    col: [f32; 3],
    dcol: [f32; 3],
}

/// Particle update period [I]: the original updates on an input-manager idle callback
/// (registered with mask 8), not a timer.
pub const PARTICLE_UPDATE_MS: u32 = 30;
const POOL: usize = 1000;

/// Particle (`RParticle`, `@0x440980`): an "Emitter"-style particle system. Particles
/// leave the emitter upwards in z (pitch π/2 ± size°, any yaw), fall back under the
/// force (0, 0, −0.1) and, when they reach z ≤ 0 or run out of life, are painted as one
/// pixel in their current colour (`@0x441330`, `SetPixel`). The live particles are drawn
/// by OpenGL at z > 0, which the projection clips, so nothing shows until they land.
pub struct Particles {
    live: Vec<Particle>,
    emitter: [f32; 3],
    pitch_var: f32,
    start_col: [f32; 3],
    end_col: [f32; 3],
    acc: u32,
    last: (i32, i32),
}

impl Particles {
    pub fn new() -> Particles {
        Particles { live: Vec::new(), emitter: [400.0, 300.0, 150.0], pitch_var: 0.0, start_col: [0.0; 3], end_col: [0.8; 3], acc: 0, last: (0, 0) }
    }

    /// `rnd()` = `(rand() − 0x3fff) · 0.00006103888` (`@0x440930`).
    fn rnd(rng: &mut MsRand) -> f32 {
        (rng.rand() - 0x3fff) as f32 * 0.000_061_038_88
    }

    /// `@0x441260`: emit `10 + trunc(rnd · 15)` particles (`@0x440f40` each).
    fn emit(&mut self, rng: &mut MsRand) {
        let n = 10 + (Self::rnd(rng) as f64 * 15.0) as i64;
        for _ in 0..n.max(0) {
            if self.live.len() >= POOL {
                break;
            }
            let yaw = Self::rnd(rng) * 6.283_185_5;
            let pitch = Self::rnd(rng) * self.pitch_var + 1.570_796_4;
            let (cp, sp) = ((pitch as f64).cos(), (pitch as f64).sin());
            let (cy, sy) = ((yaw as f64).cos(), (yaw as f64).sin());
            let speed = Self::rnd(rng) * 1.0 + 3.0;
            let dir = [(-(sy * cp)) as f32 * speed, (cy * cp) as f32 * speed, sp as f32 * speed];
            let mut start = [0.0f32; 3];
            for (k, c) in start.iter_mut().enumerate() {
                // start colour variance is 0, but its rnd() is still drawn
                let _ = Self::rnd(rng);
                *c = self.start_col[k];
            }
            let mut end = [0.0f32; 3];
            for (k, c) in end.iter_mut().enumerate() {
                *c = Self::rnd(rng) * 0.05 + self.end_col[k];
            }
            let life = 60 + (Self::rnd(rng) as f64 * 15.0) as i64 as i32;
            let mut dcol = [0.0f32; 3];
            for k in 0..3 {
                dcol[k] = ((end[k] - start[k]) as f64 / life as f64) as f32;
            }
            self.live.push(Particle { pos: self.emitter, dir, life, col: start, dcol });
        }
    }

    fn dot(img: &mut Image, p: &Particle) {
        let c8 = |v: f32| (v.clamp(0.0, 1.0) as f64 * 255.0) as i64 as u8;
        let c = [gdi5(c8(p.col[0])) as i32, gdi5(c8(p.col[1])) as i32, gdi5(c8(p.col[2])) as i32];
        let (x, y) = (p.pos[0] as i64 as i32, p.pos[1] as i64 as i32);
        if x >= 0 && y >= 0 && x < img.width as i32 && y < img.height as i32 {
            put5(img, x, y, c);
        }
    }

    /// `@0x441140` for every particle: a dead one (life ≤ 0 or z ≤ 0) is painted and
    /// freed; a live one moves, accelerates, ages and changes colour.
    fn update(&mut self, cv: &mut Image) {
        let mut i = 0;
        while i < self.live.len() {
            let p = &mut self.live[i];
            if p.life <= 0 || p.pos[2] <= 0.0 {
                let p = self.live.swap_remove(i);
                Self::dot(cv, &p);
                continue;
            }
            for k in 0..3 {
                p.pos[k] += p.dir[k];
                p.col[k] += p.dcol[k];
            }
            p.dir[2] += -0.1;
            p.life -= 1;
            i += 1;
        }
    }
}

impl Default for Particles {
    fn default() -> Self {
        Particles::new()
    }
}

impl BrushImpl for Particles {
    /// `@0x440af0`: pitch spread = size°, start colour = 1 − current, end = current; the
    /// emitter moves to the point and emits.
    fn down(&mut self, _cv: &mut Image, p: (i32, i32), cx: &mut Cx) {
        self.pitch_var = cx.size() as f32 * std::f32::consts::PI * 0.005_555_555_7;
        let c = cx.ctx.color;
        for k in 0..3 {
            let v = c[k] as f32 * 0.003_921_569;
            self.end_col[k] = v;
            self.start_col[k] = 1.0 - v;
        }
        self.emitter[0] = p.0 as i16 as f32;
        self.emitter[1] = p.1 as i16 as f32;
        self.last = p;
        self.emit(cx.rng);
    }
    /// `@0x440c30`: every mouse move re-positions the emitter and emits.
    fn held(&mut self, _cv: &mut Image, p: (i32, i32), cx: &mut Cx) {
        if p != self.last {
            self.emitter[0] = p.0 as i16 as f32;
            self.emitter[1] = p.1 as i16 as f32;
            self.emit(cx.rng);
            self.last = p;
        }
    }
    fn animate(&mut self, cv: &mut Image, dt: u32, _cx: &mut Cx) {
        self.acc += dt;
        while self.acc >= PARTICLE_UPDATE_MS {
            self.acc -= PARTICLE_UPDATE_MS;
            self.update(cv);
        }
    }
    fn animating(&self) -> bool {
        !self.live.is_empty()
    }
    /// `@0x441510`: on deactivation every live particle's life is set to 0, so the next
    /// update paints them all.
    fn deactivate(&mut self, cv: &mut Image, _cx: &mut Cx) {
        for p in self.live.iter_mut() {
            p.life = 0;
        }
        self.update(cv);
    }
}

// ---------------------------------------------------------------------------------- 5.8

/// Garland starburst (timed 20 ms, pen 2, `@0x43e380`).
#[derive(Default)]
pub struct Garland {
    timer: Timer,
    last: (i32, i32),
}

impl Garland {
    fn burst(&self, cv: &mut Image, p: (i32, i32), cx: &mut Cx) {
        let s = cx.size() as f64;
        for _ in 0..11 {
            let c = if cx.ctx.shift { cur(cx) } else { rand_gdi(cx.rng) };
            let th = cx.rng.rnd(0, 360) as f64; // used as radians [O]
            let e = (p.0 + trunc(th.cos() * s), p.1 + trunc(th.sin() * s));
            line_w(cv, p, e, 2, c);
        }
    }
}

impl BrushImpl for Garland {
    fn down(&mut self, _cv: &mut Image, p: (i32, i32), cx: &mut Cx) {
        self.timer.start(cx.now, 20);
        self.last = p;
    }
    fn held(&mut self, cv: &mut Image, p: (i32, i32), cx: &mut Cx) {
        if p != self.last {
            self.last = p;
            self.burst(cv, p, cx); // base v11 → v14 on movement [O]
        }
        if self.timer.due(cx.now) {
            self.burst(cv, p, cx);
        }
    }
}

// ---------------------------------------------------------------------------------- 5.9

/// LeakyPen: growing blob (timed 40 ms, `@0x43f890`).
#[derive(Default)]
pub struct LeakyPen {
    timer: Timer,
    r: i64,
    last: (i32, i32),
}

impl LeakyPen {
    fn blob(&mut self, cv: &mut Image, p: (i32, i32), cx: &Cx) {
        self.r += 0x8000;
        let hw = ((self.r + 0x8000) >> 16) as i32;
        let hh = (((self.r as f64 * 1.25) as i64 + 0x8000) >> 16) as i32;
        fill_ellipse(cv, p.0 - hw, p.1 - hh, p.0 + hw, p.1 + hh, cur(cx));
    }
}

impl BrushImpl for LeakyPen {
    fn down(&mut self, _cv: &mut Image, p: (i32, i32), cx: &mut Cx) {
        self.timer.start(cx.now, 40);
        self.r = (cx.size() as i64) << 16;
        self.last = p;
    }
    fn held(&mut self, cv: &mut Image, p: (i32, i32), cx: &mut Cx) {
        if p != self.last {
            self.last = p;
            self.r = (cx.size() as i64) << 16;
            self.blob(cv, p, cx); // [I] v11 resets the radius, then draws
        }
        if self.timer.due(cx.now) {
            self.blob(cv, p, cx);
        }
    }
}

// --------------------------------------------------------------------------------- 5.10

/// ScratchyPen: jittered 1-px line (`@0x442830`).
#[derive(Default)]
pub struct Scratchy {
    prev: (i32, i32),
    last: (i32, i32),
}

impl BrushImpl for Scratchy {
    fn down(&mut self, _cv: &mut Image, p: (i32, i32), _cx: &mut Cx) {
        self.prev = p;
        self.last = p;
    }
    fn held(&mut self, cv: &mut Image, p: (i32, i32), cx: &mut Cx) {
        if p == self.last {
            return;
        }
        self.last = p;
        let s = cx.size();
        let np = (p.0 + cx.rng.rnd(-s, s), p.1 + cx.rng.rnd(-s, s));
        let c = if cx.ctx.shift { rand_gdi(cx.rng) } else { cur(cx) };
        line1(cv, self.prev, np, c);
        self.prev = np;
    }
}

// --------------------------------------------------------------------------------- 5.11

/// LoopingLine (α += 0.4) / LoopingLine2 (α += 3.0), pen 2 (`@0x43fc30`).
pub struct Looping {
    inc: f64,
    alpha: f64,
    first: bool,
    prev: (i32, i32),
    last: (i32, i32),
}

impl Looping {
    pub fn new(inc: f64) -> Looping {
        Looping { inc, alpha: 0.0, first: true, prev: (0, 0), last: (0, 0) }
    }
}

impl BrushImpl for Looping {
    fn down(&mut self, _cv: &mut Image, p: (i32, i32), _cx: &mut Cx) {
        self.alpha = 0.0;
        self.first = true;
        self.last = p;
        self.prev = p;
    }
    fn held(&mut self, cv: &mut Image, p: (i32, i32), cx: &mut Cx) {
        if p == self.last {
            return;
        }
        self.last = p;
        let s = cx.size() as f64;
        let np = (p.0 + trunc(self.alpha.cos() * s), p.1 + trunc(self.alpha.sin() * s));
        self.alpha += self.inc;
        if self.alpha >= 2.0 * PI {
            self.alpha -= 2.0 * PI;
        }
        if self.first {
            self.first = false;
        } else {
            let c = if cx.ctx.shift { rand_gdi(cx.rng) } else { cur(cx) };
            line_w(cv, self.prev, np, 2, c);
        }
        self.prev = np;
    }
}

// --------------------------------------------------------------------------------- 5.12

/// Bricks (Icons 3613–3618, running bond) and Log (3606–3612) grid tiles.
pub struct Tiles {
    bricks: bool,
    cell: Option<(i32, i32)>,
    tiles: Vec<Arc<Image>>,
    last: (i32, i32),
}

impl Tiles {
    pub fn new(bricks: bool) -> Tiles {
        Tiles { bricks, cell: None, tiles: Vec::new(), last: (0, 0) }
    }

    fn load(&mut self, cx: &Cx) {
        if !self.tiles.is_empty() {
            return;
        }
        let ids: Vec<u16> = if self.bricks { (3613..=3618).collect() } else { (3606..=3612).collect() };
        let bricks = self.bricks;
        for (k, id) in ids.into_iter().enumerate() {
            self.tiles.push(cx.ctx.assets.bitmap_or(ICONS_MHK, id, || synthetic_tile(bricks, k)));
        }
    }

    fn place(&mut self, cv: &mut Image, p: (i32, i32), cx: &mut Cx) {
        let (tw, th) = (self.tiles[0].width as i32, self.tiles[0].height as i32);
        let row = p.1.div_euclid(th.max(1));
        let shift = if self.bricks && row % 2 == 0 && !cx.ctx.shift { -tw / 2 } else { 0 };
        let col = (p.0 - shift).div_euclid(tw.max(1));
        if self.cell == Some((col, row)) {
            return;
        }
        self.cell = Some((col, row));
        let k = cx.rng.rnd(0, self.tiles.len() as i32 - 1) as usize;
        blit(cv, &self.tiles[k], col * tw + shift, row * th);
        cx.fx();
    }
}

fn synthetic_tile(bricks: bool, k: usize) -> Image {
    let base = if bricks { 0xffb0_4030u32 } else { 0xff8a_5a2a };
    let mut img = Image::filled(32, 16, base.wrapping_add((k as u32 % 4) * 0x0004_0404));
    for x in 0..32 {
        img.put(x, 15, 0xffd8_d0c0);
    }
    for y in 0..16 {
        img.put(31, y, 0xffd8_d0c0);
    }
    img
}

impl BrushImpl for Tiles {
    fn down(&mut self, cv: &mut Image, p: (i32, i32), cx: &mut Cx) {
        self.load(cx);
        self.cell = None;
        self.last = p;
        self.place(cv, p, cx);
    }
    fn held(&mut self, cv: &mut Image, p: (i32, i32), cx: &mut Cx) {
        if p != self.last {
            self.last = p;
            self.place(cv, p, cx);
        }
    }
}

// --------------------------------------------------------------------------------- 5.13

/// Window: Icons 3605 stretched into the drag rect.
#[derive(Default)]
pub struct Window {
    snap: Option<Image>,
    start: (i32, i32),
    last: (i32, i32),
    art: Option<Arc<Image>>,
}

impl BrushImpl for Window {
    fn down(&mut self, cv: &mut Image, p: (i32, i32), cx: &mut Cx) {
        self.snap = Some(cv.clone());
        self.start = p;
        self.last = p;
        self.art = Some(cx.ctx.assets.bitmap_or(ICONS_MHK, 3605, synthetic_window));
    }
    fn held(&mut self, cv: &mut Image, p: (i32, i32), _cx: &mut Cx) {
        if p == self.last {
            return;
        }
        self.last = p;
        restore(cv, &self.snap);
        let (l, t, r, b) = pt2rect(self.start, p);
        if let Some(a) = &self.art {
            stretch_blit(cv, a, l, t, r, b);
        }
    }
    fn up(&mut self, _cv: &mut Image, _p: (i32, i32), _cx: &mut Cx) {
        self.snap = None;
    }
}

fn synthetic_window() -> Image {
    let n = 64;
    let mut img = Image::new(n, n);
    for y in 0..n as i32 {
        for x in 0..n as i32 {
            let frame = x < 4 || y < 4 || x >= n as i32 - 4 || y >= n as i32 - 4 || (x - 32).abs() < 2 || (y - 32).abs() < 2;
            img.put(x, y, if frame { 0xff60_4020 } else { 0x80a0_d0ff });
        }
    }
    img
}

// --------------------------------------------------------------------------------- 5.14

/// Door (gradient, panes 3600, knob 3601) and DoorPlain (panels 3602–3604) [O]/[I].
pub struct Door {
    plain: bool,
    snap: Option<Image>,
    start: (i32, i32),
    last: (i32, i32),
    pieces: Vec<Arc<Image>>,
}

impl Door {
    pub fn new(plain: bool) -> Door {
        Door { plain, snap: None, start: (0, 0), last: (0, 0), pieces: Vec::new() }
    }

    fn rect(&self, p: (i32, i32)) -> (i32, i32, i32, i32) {
        let (mw, mh) = (3 * self.pieces[0].width as i32, 3 * self.pieces[0].height as i32);
        let (sx, sy) = self.start;
        let (l, r) = if p.0 >= sx { (sx, p.0.max(sx + mw)) } else { (p.0.min(sx - mw), sx) };
        let (t, b) = if p.1 >= sy { (sy, p.1.max(sy + mh)) } else { (p.1.min(sy - mh), sy) };
        (l, t, r, b)
    }

    fn frame(cv: &mut Image, l: i32, t: i32, r: i32, b: i32) {
        let k = BLACK5;
        line1(cv, (l, t), (r - 1, t), k);
        line1(cv, (r - 1, t), (r - 1, b - 1), k);
        line1(cv, (r - 1, b - 1), (l, b - 1), k);
        line1(cv, (l, b - 1), (l, t), k);
    }

    fn draw(&self, cv: &mut Image, p: (i32, i32)) {
        let (l, t, r, b) = self.rect(p);
        let (w, h) = (r - l, b - t);
        if self.plain {
            self.draw_plain(cv, l, t, r, b);
            return;
        }
        // smooth vertical gradient (255,255,207) → white
        for y in t..b {
            let f = if h > 1 { (y - t) as f32 / (h - 1) as f32 } else { 0.0 };
            let bl = 207.0 + (255.0 - 207.0) * f;
            let c = [31, 31, (bl / 255.0 * 31.0 + 0.5) as i32];
            fill_rect(cv, l, y, r, y + 1, c);
        }
        Self::frame(cv, l, t, r, b);
        let pane = &self.pieces[0];
        let (pw, ph) = (pane.width as i32, pane.height as i32);
        let cols = ((w - 18) / (pw + 2)).max(0);
        let rows = ((h - 40) / (ph + 2)).max(0);
        let total = cols * (pw + 2) - 2;
        let x0 = l + (w - total) / 2;
        for j in 0..rows {
            for i in 0..cols {
                blit(cv, pane, x0 + i * (pw + 2), t + 10 + j * (ph + 2));
            }
        }
        let knob = &self.pieces[1];
        blit(cv, knob, r - knob.width as i32 - 2, t + (h - knob.height as i32) / 2);
    }

    /// DoorPlain (`@0x43cfc0` background, `@0x43d000` panels): the rect filled with
    /// RGB(204, 102, 51) and framed by a black 1-px `Rectangle`; then rows of wood panels
    /// from top + 10, the panel art cycling 3602 → 3603 → 3604 per row, 8 px apart, as
    /// many columns as `(w − 12) / (pw + 8)`, centred; a row is drawn only while it ends
    /// above `bottom − 20`.
    fn draw_plain(&self, cv: &mut Image, l: i32, t: i32, r: i32, b: i32) {
        fill_rect(cv, l, t, r, b, gdi_rgb5([204, 102, 51]));
        Self::frame(cv, l, t, r, b);
        let panels = &self.pieces[..3];
        let pw = panels[0].width as i32;
        let cols = (r - l - 12) / (pw + 8);
        let x0 = (r - (cols * (pw + 8)) as i16 as i32 - l + 8) / 2;
        let mut y = t + 10;
        let mut i = 0;
        loop {
            let piece = &panels[i];
            let ph = piece.height as i32;
            if y + ph >= b - 20 {
                break;
            }
            for k in 0..cols.max(0) {
                blit(cv, piece, l + k * (pw + 8) + x0, y);
            }
            y += ph + 8;
            i = (i + 1) % panels.len();
        }
    }
}

impl BrushImpl for Door {
    fn down(&mut self, cv: &mut Image, p: (i32, i32), cx: &mut Cx) {
        if self.pieces.is_empty() {
            let ids: &[u16] = if self.plain { &[3602, 3603, 3604] } else { &[3600, 3601] };
            for &id in ids {
                self.pieces.push(cx.ctx.assets.bitmap_or(ICONS_MHK, id, || {
                    let (w, h) = match id {
                        3600 => (15, 19),
                        3601 => (7, 20),
                        3602 => (20, 20),
                        3603 => (20, 31),
                        _ => (20, 39),
                    };
                    let mut img = Image::filled(w, h, if id == 3601 { 0xffc0_a000 } else { 0xff90_c0f0 });
                    for x in 0..w as i32 {
                        img.put(x, 0, 0xff00_0000);
                        img.put(x, h as i32 - 1, 0xff00_0000);
                    }
                    img
                }));
            }
        }
        self.snap = Some(cv.clone());
        self.start = p;
        self.last = p;
    }
    fn held(&mut self, cv: &mut Image, p: (i32, i32), _cx: &mut Cx) {
        if p == self.last {
            return;
        }
        self.last = p;
        restore(cv, &self.snap);
        self.draw(cv, p);
    }
    fn up(&mut self, _cv: &mut Image, _p: (i32, i32), _cx: &mut Cx) {
        self.snap = None;
    }
}

// --------------------------------------------------------------------------------- 5.15

/// Roof: shingle-textured trapezoid (Icons 3300) with a black outline.
#[derive(Default)]
pub struct Roof {
    snap: Option<Image>,
    start: (i32, i32),
    last: (i32, i32),
    tex: Option<Arc<Image>>,
}

impl Roof {
    fn draw(&self, cv: &mut Image, p: (i32, i32)) {
        let (l, t, r, b) = pt2rect(self.start, p);
        let (w, h) = (r - l, b - t);
        if w < 2 || h < 2 {
            return;
        }
        let Some(tex) = &self.tex else { return };
        let inset = w as f32 / 4.0;
        let (tw, th) = (tex.width as i32, tex.height as i32);
        for y in t.max(0)..b.min(cv.height as i32) {
            // top edge (y = t) inset by w/4, bottom edge full width
            let f = 1.0 - (y as f32 + 0.5 - t as f32) / h as f32;
            let xl = (l as f32 + inset * f).round() as i32;
            let xr = (r as f32 - inset * f).round() as i32;
            for x in xl.max(0)..xr.min(cv.width as i32) {
                // texture coordinate = canvas offset / tile size, repeat-wrapped [I]
                let tp = tex.pixels[(y.rem_euclid(th) * tw + x.rem_euclid(tw)) as usize];
                put5(cv, x, y, argb_to_rgb5(tp | 0xff00_0000));
            }
        }
        let tl = (l + (w / 4), t);
        let tr = (r - 1 - (w / 4), t);
        let (bl, br) = ((l, b - 1), (r - 1, b - 1));
        for (a, c) in [(tl, tr), (tr, br), (br, bl), (bl, tl)] {
            line1(cv, a, c, BLACK5);
        }
        put5(cv, tl.0, tl.1, BLACK5);
    }
}

impl BrushImpl for Roof {
    fn down(&mut self, cv: &mut Image, p: (i32, i32), cx: &mut Cx) {
        self.snap = Some(cv.clone());
        self.start = p;
        self.last = p;
        self.tex = Some(cx.ctx.assets.bitmap_or(ICONS_MHK, 3300, || {
            let mut img = Image::filled(64, 32, 0xff80_3020);
            for x in 0..64 {
                img.put(x, 15, 0xff40_1008);
                img.put(x, 31, 0xff40_1008);
            }
            img
        }));
    }
    fn held(&mut self, cv: &mut Image, p: (i32, i32), _cx: &mut Cx) {
        if p == self.last {
            return;
        }
        self.last = p;
        restore(cv, &self.snap);
        self.draw(cv, p);
    }
    fn up(&mut self, _cv: &mut Image, _p: (i32, i32), _cx: &mut Cx) {
        self.snap = None;
    }
}

// --------------------------------------------------------------------------------- 5.16

/// DotToDot: numbered dots joined by a polyline (timed 100 ms, `@0x43d2a0`).
#[derive(Default)]
pub struct DotToDot {
    snap: Option<Image>,
    pts: Vec<(i32, i32)>,
    flag: bool,
    timer: Timer,
    last: (i32, i32),
}

impl DotToDot {
    fn redraw(&self, cv: &mut Image, cx: &Cx) {
        restore(cv, &self.snap);
        let c = cur(cx);
        for w in self.pts.windows(2) {
            line1(cv, w[0], w[1], c);
        }
        for (i, &(x, y)) in self.pts.iter().enumerate() {
            fill_ellipse(cv, x - 2, y - 2, x + 3, y + 3, c);
            // number centred in the box (x-2, y-2)-(x+22, y+22) [I]; built-in pixel font [I]
            let s = format!("{}", i + 1);
            let wtxt = s.len() as i32 * 12;
            text(cv, None, &s, 12.0, x + 10 - wtxt / 2, y + 10 - 7, c);
        }
    }
}

impl BrushImpl for DotToDot {
    fn down(&mut self, cv: &mut Image, p: (i32, i32), cx: &mut Cx) {
        self.snap = Some(cv.clone());
        self.pts.clear();
        self.flag = true; // [I] the click point gets dot 1
        self.timer.start(cx.now, 100);
        self.last = p;
    }
    fn held(&mut self, cv: &mut Image, p: (i32, i32), cx: &mut Cx) {
        if p != self.last {
            self.flag = true;
            self.last = p;
        }
        if self.timer.due(cx.now) && self.flag {
            self.flag = false;
            self.pts.push(p);
            self.redraw(cv, cx);
            cx.fx();
        }
    }
    fn up(&mut self, _cv: &mut Image, _p: (i32, i32), _cx: &mut Cx) {
        self.snap = None;
    }
}

// --------------------------------------------------------------------------------- 5.17

/// ABC: the artist's name letter by letter in Jester (`@0x43a780`).
#[derive(Default)]
pub struct Abc {
    idx: usize,
    last: (i32, i32),
    cw: i32,
    lh: i32,
}

impl Abc {
    fn letter(&mut self, cv: &mut Image, p: (i32, i32), cx: &mut Cx) {
        let name: Vec<char> = if cx.ctx.artist_name.trim().is_empty() {
            "Kid Pix".chars().collect() // [I] no artist name
        } else {
            cx.ctx.artist_name.chars().collect()
        };
        let ch = name[self.idx % name.len()];
        self.idx += 1;
        let px = cx.size() as f32;
        let font = cx.ctx.assets.jester();
        let g = glyph(font, ch, px);
        // [I] letter centred on the mouse point
        let (x, y) = (p.0 - g.advance / 2, p.1 - px as i32 / 2);
        coverage(cv, &g.cov, g.w, g.h, x + g.ox, y + g.oy, cur(cx));
        self.cw = g.advance.max(1);
        self.lh = px as i32;
        self.last = p;
        cx.fx();
    }
}

impl BrushImpl for Abc {
    fn down(&mut self, cv: &mut Image, p: (i32, i32), cx: &mut Cx) {
        self.idx = 0;
        self.letter(cv, p, cx);
    }
    fn held(&mut self, cv: &mut Image, p: (i32, i32), cx: &mut Cx) {
        if (p.0 - self.last.0).abs() > self.cw || (p.1 - self.last.1).abs() > self.lh {
            self.letter(cv, p, cx);
        }
    }
}

// --------------------------------------------------------------------------------- 5.18

/// Rainbow arch with the 8-texel 1-D texture (`@0x441650`).
#[derive(Default)]
pub struct Rainbow {
    snap: Option<Image>,
    start: (i32, i32),
    last: (i32, i32),
}

const RAINBOW_TEX: [[f32; 4]; 8] = [
    [0.0, 0.0, 0.0, 0.0],
    [127.0, 0.0, 127.0, 80.0],
    [191.0, 0.0, 191.0, 80.0],
    [0.0, 0.0, 255.0, 80.0],
    [0.0, 255.0, 0.0, 80.0],
    [255.0, 255.0, 0.0, 80.0],
    [255.0, 127.0, 0.0, 80.0],
    [255.0, 0.0, 0.0, 80.0],
];

fn rainbow_sample(s: f32) -> [f32; 4] {
    let c = s * 8.0 - 0.5;
    let i0 = c.floor();
    let f = c - i0;
    let a = RAINBOW_TEX[(i0 as i32).rem_euclid(8) as usize];
    let b = RAINBOW_TEX[(i0 as i32 + 1).rem_euclid(8) as usize];
    let mut o = [0.0; 4];
    for k in 0..4 {
        o[k] = (a[k] + (b[k] - a[k]) * f) / 255.0;
    }
    o
}

impl Rainbow {
    fn draw(&self, cv: &mut Image, p: (i32, i32), cx: &Cx) {
        let (l, t, r, b) = pt2rect(self.start, p);
        let (w, h) = ((r - l) as f32, (b - t) as f32);
        if w < 2.0 || h < 2.0 {
            return;
        }
        let rin = if cx.ctx.space { 0.85 } else { 0.6 };
        let blend = !cx.ctx.shift;
        for y in t.max(0)..b.min(cv.height as i32) {
            for x in l.max(0)..r.min(cv.width as i32) {
                // arch standing on the rect's bottom edge [I]
                let u = (x as f32 + 0.5 - l as f32) / w;
                let v = (b as f32 - (y as f32 + 0.5)) / h;
                let (xx, yy) = (2.0 * u - 1.0, v);
                let rho = (xx * xx + yy * yy).sqrt();
                if yy < 0.0 || rho > 1.0 || rho < rin {
                    continue;
                }
                let s = 0.1875 + (1.0 - rho) / (1.0 - rin) * 0.75;
                let c = rainbow_sample(s);
                if blend {
                    blend_px(cv, x, y, [c[0], c[1], c[2]], c[3]);
                } else {
                    put5(cv, x, y, [(c[0] * 31.0 + 0.5) as i32, (c[1] * 31.0 + 0.5) as i32, (c[2] * 31.0 + 0.5) as i32]);
                }
            }
        }
    }
}

impl BrushImpl for Rainbow {
    fn down(&mut self, cv: &mut Image, p: (i32, i32), _cx: &mut Cx) {
        self.snap = Some(cv.clone());
        self.start = p;
        self.last = p;
    }
    fn held(&mut self, cv: &mut Image, p: (i32, i32), cx: &mut Cx) {
        if p == self.last {
            return;
        }
        self.last = p;
        restore(cv, &self.snap);
        self.draw(cv, p, cx);
    }
    fn up(&mut self, _cv: &mut Image, _p: (i32, i32), _cx: &mut Cx) {
        self.snap = None;
    }
}

// --------------------------------------------------------------------------------- 5.19

/// ColorTile: translucent rectangle (`@0x43b720`).
#[derive(Default)]
pub struct ColorTile {
    snap: Option<Image>,
    start: (i32, i32),
    last: (i32, i32),
}

impl BrushImpl for ColorTile {
    fn down(&mut self, cv: &mut Image, p: (i32, i32), _cx: &mut Cx) {
        self.snap = Some(cv.clone());
        self.start = p;
        self.last = p;
    }
    fn held(&mut self, cv: &mut Image, p: (i32, i32), cx: &mut Cx) {
        if p == self.last {
            return;
        }
        self.last = p;
        restore(cv, &self.snap);
        let (l, t, r, b) = pt2rect(self.start, p);
        let c = cx.ctx.color;
        let src = [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0];
        let a = 180.0 / 255.0;
        let shift = cx.ctx.shift;
        for y in t.max(0)..b.min(cv.height as i32) {
            for x in l.max(0)..r.min(cv.width as i32) {
                blend_px_with(cv, x, y, |d, k| {
                    let s = src[k as usize];
                    if shift {
                        s + d * (1.0 - s) // (ONE, ONE_MINUS_SRC_COLOR)
                    } else {
                        s + d * (1.0 - a) // (ONE, ONE_MINUS_SRC_ALPHA)
                    }
                });
            }
        }
    }
    fn up(&mut self, _cv: &mut Image, _p: (i32, i32), _cx: &mut Cx) {
        self.snap = None;
    }
}

// --------------------------------------------------------------------------------- 5.20

/// 8WayMirror: eight mirrored pen heads (`@0x43a620`).
#[derive(Default)]
pub struct Mirror8 {
    origin: (i32, i32),
    heads: [(i32, i32); 8],
    last: (i32, i32),
}

impl BrushImpl for Mirror8 {
    fn down(&mut self, _cv: &mut Image, p: (i32, i32), _cx: &mut Cx) {
        let (x, y) = p;
        self.origin = p;
        self.heads = [(x - 5, y - 10), (x + 5, y - 10), (x + 10, y - 5), (x + 10, y + 5), (x + 5, y + 10), (x - 5, y + 10), (x - 10, y + 5), (x - 10, y - 5)];
        self.last = p;
    }
    fn held(&mut self, cv: &mut Image, p: (i32, i32), cx: &mut Cx) {
        if p == self.last {
            return;
        }
        let (dx, dy) = (p.0 - self.last.0, p.1 - self.last.1);
        self.last = p;
        let mv = [(dx, dy), (-dx, dy), (-dy, dx), (-dy, -dx), (-dx, -dy), (dx, -dy), (dy, -dx), (dy, dx)];
        let w = if cx.ctx.space { 2 } else { 1 };
        let c = cur(cx);
        for (h, m) in self.heads.iter_mut().zip(mv) {
            let n = (h.0 + m.0, h.1 + m.1);
            let from = if cx.ctx.alt { self.origin } else { *h };
            line_w(cv, from, n, w, c);
            *h = n;
        }
    }
}

// --------------------------------------------------------------------------------- 5.21

/// Splat: random blobs (timed 50 ms, `@0x443aa0`).
#[derive(Default)]
pub struct Splat {
    timer: Timer,
    last: (i32, i32),
}

impl Splat {
    fn splat(&self, cv: &mut Image, p: (i32, i32), cx: &mut Cx) {
        let s = cx.size();
        for _ in 0..6 {
            let c = if cx.ctx.shift { cur(cx) } else { rand_gdi(cx.rng) };
            if cx.ctx.space {
                let mut v = [(0, 0); 3];
                for q in &mut v {
                    *q = (p.0 + cx.rng.rnd(-s, s), p.1 + cx.rng.rnd(-s, s));
                }
                fill_polygon(cv, &v, c);
            } else {
                let r = cx.rng.rnd(3 * s, 8 * s);
                let ox = cx.rng.rnd(-2 * r, 2 * r);
                let oy = cx.rng.rnd(-2 * r, 2 * r);
                fill_ellipse(cv, p.0 - r + ox, p.1 - r + oy, p.0 + r + ox, p.1 + r + oy, c);
            }
        }
    }
}

impl BrushImpl for Splat {
    fn down(&mut self, _cv: &mut Image, p: (i32, i32), cx: &mut Cx) {
        self.timer.start(cx.now, 50);
        self.last = p;
    }
    fn held(&mut self, cv: &mut Image, p: (i32, i32), cx: &mut Cx) {
        if p != self.last {
            self.last = p;
            self.splat(cv, p, cx); // base v11 → v14 on movement [O]
        }
        if self.timer.due(cx.now) {
            self.splat(cv, p, cx);
        }
    }
}

// --------------------------------------------------------------------------------- 5.22

/// Tree: recursive branches (`@0x4463d0`).
#[derive(Default)]
pub struct Tree {
    last: (i32, i32),
    last_draw: u64,
}

impl Tree {
    fn draw(&self, cv: &mut Image, p: (i32, i32), cx: &mut Cx) {
        let (x, y) = p;
        let k = cx.ctx;
        let l = if k.space {
            cx.rng.rnd(25, 31) // [I] Space wins over Shift
        } else if k.shift {
            cx.rng.rnd(21, 25)
        } else {
            cx.rng.rnd(18, 22)
        };
        let mut col = cur(cx);
        let pen = ((l as f64 * 1.5) as i32 / 7).max(1);
        line_w(cv, (x - 1, y), (x - 1, y - l - 2), pen, col);
        let spread = cx.rng.rnd(5, 30);
        Self::branch(cv, (x, y - l), l, 270, spread, &mut col, cx);
    }

    fn branch(cv: &mut Image, mut at: (i32, i32), mut len: i32, mut angle: i32, spread: i32, col: &mut Rgb5, cx: &mut Cx) {
        loop {
            if cx.ctx.shift {
                *col = rand_gdi(cx.rng);
            }
            let a1 = angle - spread;
            let a2 = angle + spread;
            let end = |a: i32, rng: &mut MsRand| {
                let r = (a as f64).to_radians();
                let ex = at.0 + trunc(r.cos() * (len + rng.rnd(-5, 5)) as f64);
                let ey = at.1 + trunc(r.sin() * (len + rng.rnd(-5, 5)) as f64);
                (ex, ey)
            };
            let e1 = end(a1, cx.rng);
            let e2 = end(a2, cx.rng);
            let pen = (len / 7).max(1);
            line_w(cv, at, e1, pen, *col);
            line_w(cv, at, e2, pen, *col);
            len = (len as f64 * 0.9) as i32;
            if len <= 10 {
                return;
            }
            let a1b = a1 - cx.rng.rnd(-20, 20);
            Self::branch(cv, e1, len, a1b, spread, col, cx);
            at = e2;
            angle = a2;
        }
    }
}

impl BrushImpl for Tree {
    fn down(&mut self, cv: &mut Image, p: (i32, i32), cx: &mut Cx) {
        self.draw(cv, p, cx);
        self.last = p;
        self.last_draw = cx.now;
        cx.fx();
    }
    fn held(&mut self, cv: &mut Image, p: (i32, i32), cx: &mut Cx) {
        // Alt: a tree on every move, at most one per 50 ms (the original busy-waits) [O]/[I]
        if cx.ctx.alt && p != self.last && cx.now.saturating_sub(self.last_draw) >= 50 {
            self.draw(cv, p, cx);
            self.last_draw = cx.now;
        }
        self.last = p;
    }
}
