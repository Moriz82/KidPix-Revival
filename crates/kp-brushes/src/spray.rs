//! Spray Can (RE_BRUSHES §7): "shapes" (`RCyclic`, Cyclic.MHK) and "moving paint"
//! (`RSprayGun`, Anims.MHK).

use crate::assets::{SpriteSet, ANIMS_MHK, CYCLIC_MHK, T_SCR};
use crate::pad::{draw_frame, script_anchor, SCRIPT_FRAME_MS};
use crate::raster::blit;
use crate::sound::SoundEvent;
use crate::tool::{BrushImpl, Cx};
use mohawk::{Cel, Image, Op};
use std::sync::Arc;

/// Shapes spray stamp period: every 100 ms while held, also when still [O].
pub const CYCLIC_PERIOD_MS: u64 = 100;
/// Moving-paint spray: a repeating RTimer started at mouse down with 50 ms
/// (`@0x443ce0`); the first shot is sent at the click itself [O].
pub const SPRAY_PERIOD_MS: u64 = 50;

/// Spray "shapes": random cels of a Cyclic.MHK set.
pub struct Cyclic {
    set_id: u16,
    set: Option<Arc<SpriteSet>>,
    count: u16,
    next: u64,
    due_now: bool,
}

impl Cyclic {
    pub fn new(k: u8) -> Cyclic {
        Cyclic { set_id: 24000 + 100 * k as u16, set: None, count: 0, next: 0, due_now: false }
    }

    fn stamp(&self, cv: &mut Image, p: (i32, i32), cx: &mut Cx) {
        match &self.set {
            Some(set) if self.count >= 2 => {
                let n = cx.rng.rnd(1, self.count as i32 - 1) as u16;
                if let Some(cel) = set.cel(n) {
                    let (rx, ry) = set.reg(n);
                    blit(cv, cel, p.0 - rx, p.1 - ry);
                }
            }
            _ => {
                // [I] no art: a small random-coloured diamond
                let c = crate::color::gdi_rgb5(cx.rng.color());
                let r = 4;
                crate::raster::fill_polygon(cv, &[(p.0, p.1 - r), (p.0 + r, p.1), (p.0, p.1 + r), (p.0 - r, p.1)], c);
            }
        }
    }
}

impl BrushImpl for Cyclic {
    fn down(&mut self, _cv: &mut Image, _p: (i32, i32), cx: &mut Cx) {
        if self.set.is_none() {
            self.set = cx.ctx.assets.sprite_set(CYCLIC_MHK, self.set_id);
            self.count = cx
                .ctx
                .assets
                .count(CYCLIC_MHK, self.set_id)
                .map(|c| c.1)
                .or_else(|| self.set.as_ref().map(|s| s.cels.len() as u16 + 1))
                .unwrap_or(0);
        }
        // `@0x442d00` clears the stamp time, so the first tick stamps at once
        self.due_now = true;
    }
    /// `@0x442cc0`: when the stamp time has passed, stamp and wait 100 ms from now.
    fn held(&mut self, cv: &mut Image, p: (i32, i32), cx: &mut Cx) {
        if self.due_now || self.next < cx.now {
            self.due_now = false;
            self.stamp(cv, p, cx);
            self.next = cx.now + CYCLIC_PERIOD_MS;
        }
    }
}

/// One running moving-paint sprite.
struct Sprite {
    set: Arc<SpriteSet>,
    /// (cels, sound indices triggered when this frame is shown)
    frames: Vec<(Vec<Cel>, Vec<u16>)>,
    idx: usize,
    wait: f32,
    origin: (i32, i32),
    anchor: (i32, i32),
}

/// Spray "moving paint": animated sprites from an Anims.MHK set.
pub struct SprayGun {
    set_id: u16,
    set: Option<Arc<SpriteSet>>,
    scripts: Vec<u16>,
    running: Vec<Sprite>,
    next: u64,
}

impl SprayGun {
    pub fn new(k: u8) -> SprayGun {
        SprayGun { set_id: 24000 + 100 * k as u16, set: None, scripts: Vec::new(), running: Vec::new(), next: 0 }
    }

    fn fire(&mut self, cv: &Image, p: (i32, i32), cx: &mut Cx) {
        let (w, h) = (cv.width as i32, cv.height as i32);
        if p.0 < 0 || p.1 < 0 || p.0 >= w || p.1 >= h {
            return;
        }
        let Some(set) = self.set.clone() else {
            return;
        };
        if self.scripts.is_empty() {
            return;
        }
        let first = self.scripts[0] as i32;
        let pick = cx.rng.rnd(first, first + self.scripts.len() as i32 - 1) as u16;
        let id = if self.scripts.contains(&pick) { pick } else { self.scripts[(pick as i32 - first) as usize % self.scripts.len()] };
        let Some(script) = cx.ctx.assets.script(ANIMS_MHK, id) else { return };
        let mut frames = Vec::new();
        let mut cur: Vec<Cel> = Vec::new();
        let mut snd: Vec<u16> = Vec::new();
        for op in &script.ops {
            match op {
                Op::Frame(c) => cur = c.clone(),
                Op::Sound { index, .. } => snd.push(*index),
                Op::Tick => frames.push((cur.clone(), std::mem::take(&mut snd))),
                _ => {}
            }
        }
        if frames.is_empty() {
            return;
        }
        let cels: Vec<Vec<Cel>> = frames.iter().map(|f| f.0.clone()).collect();
        let anchor = script_anchor(&cels);
        // Confine the sprite's (translated) bounds to the canvas [O]/[I].
        let b = script.bounds;
        let dx = p.0 - b.left as i32 - anchor.0;
        let dy = p.1 - b.top as i32 - anchor.1;
        let (mut l, mut t) = (b.left as i32 + dx, b.top as i32 + dy);
        let (bw, bh) = (b.width(), b.height());
        let mut origin = p;
        if l + bw > w {
            origin.0 -= l + bw - w;
            l = w - bw;
        }
        if l < 0 {
            origin.0 -= l;
        }
        if t + bh > h {
            origin.1 -= t + bh - h;
            t = h - bh;
        }
        if t < 0 {
            origin.1 -= t;
        }
        let set_id = self.set_id;
        for &i in &frames[0].1 {
            cx.ev.push(SoundEvent::play(ANIMS_MHK, set_id + i, false));
        }
        self.running.push(Sprite { set, frames, idx: 0, wait: SCRIPT_FRAME_MS, origin, anchor });
    }

    /// Advance a sprite by `dt`; when its script ends the last frame is composited into
    /// the picture and `true` is returned.
    fn advance(s: &mut Sprite, cv: &mut Image, dt: f32, set_id: u16, ev: &mut Vec<SoundEvent>) -> bool {
        s.wait -= dt;
        while s.wait <= 0.0 {
            if s.idx + 1 >= s.frames.len() {
                draw_frame(cv, &s.set, &s.frames[s.idx].0, s.origin, s.anchor, None);
                return true;
            }
            s.idx += 1;
            for &i in &s.frames[s.idx].1 {
                // [I] script sound index i → Anims.MHK tWAV (set id + i)
                ev.push(SoundEvent::play(ANIMS_MHK, set_id + i, false));
            }
            s.wait += SCRIPT_FRAME_MS;
        }
        false
    }
}

impl BrushImpl for SprayGun {
    fn down(&mut self, cv: &mut Image, p: (i32, i32), cx: &mut Cx) {
        if self.set.is_none() {
            self.set = cx.ctx.assets.sprite_set(ANIMS_MHK, self.set_id);
            self.scripts = cx.ctx.assets.ids_in(ANIMS_MHK, T_SCR, self.set_id, self.set_id + 99);
        }
        self.next = cx.now + SPRAY_PERIOD_MS;
        if self.set.is_none() {
            // [I] no Anims.MHK: behave like a plain shape spray
            let c = crate::color::gdi_rgb5(cx.rng.color());
            crate::raster::fill_ellipse(cv, p.0 - 3, p.1 - 3, p.0 + 4, p.1 + 4, c);
            return;
        }
        self.fire(cv, p, cx); // the mouse down sends the first timer message itself [O]
    }
    fn held(&mut self, cv: &mut Image, p: (i32, i32), cx: &mut Cx) {
        if cx.now >= self.next {
            self.next = cx.now + SPRAY_PERIOD_MS;
            if self.set.is_none() {
                let c = crate::color::gdi_rgb5(cx.rng.color());
                let (ox, oy) = (cx.rng.rnd(-12, 12), cx.rng.rnd(-12, 12));
                crate::raster::fill_ellipse(cv, p.0 + ox - 3, p.1 + oy - 3, p.0 + ox + 4, p.1 + oy + 4, c);
            } else {
                self.fire(cv, p, cx);
            }
        }
    }
    fn animate(&mut self, cv: &mut Image, dt: u32, cx: &mut Cx) {
        let set_id = self.set_id;
        let mut i = 0;
        while i < self.running.len() {
            let done = Self::advance(&mut self.running[i], cv, dt as f32, set_id, cx.ev);
            if done {
                self.running.swap_remove(i);
            } else {
                i += 1;
            }
        }
    }
    fn animating(&self) -> bool {
        !self.running.is_empty()
    }
    fn overlay(&self, img: &mut Image) {
        for s in &self.running {
            draw_frame(img, &s.set, &s.frames[s.idx].0, s.origin, s.anchor, None);
        }
    }
    fn deactivate(&mut self, cv: &mut Image, _cx: &mut Cx) {
        for s in self.running.drain(..) {
            if let Some(f) = s.frames.last() {
                draw_frame(cv, &s.set, &f.0, s.origin, s.anchor, None);
            }
        }
    }
}
