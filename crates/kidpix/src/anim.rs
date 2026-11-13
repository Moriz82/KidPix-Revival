//! Playback of `tSCR` animation scripts against a sprite set.

use crate::assets::{Assets, SpriteSet};
use crate::gfx::{self, Image};
use mohawk::{Cel, Op, Rect, Script};
use std::rc::Rc;

thread_local! {
    static TINT: std::cell::Cell<u32> = const { std::cell::Cell::new(0xff00_0000) };
}

/// The current colour used for 8-bit Tint-mask cels (the panels' pencils, buckets…).
pub fn set_tint(color: u32) {
    TINT.with(|t| t.set(color));
}

/// Draw a Tint mask: `dst = RGB555(C5 * g >> 8)` where the mask value g > 0.
fn draw_tint_mask(dst: &mut Image, mask: &Image, x: i32, y: i32, clip: Option<(i32, i32, i32, i32)>) {
    let c = TINT.with(|t| t.get());
    let (c5r, c5g, c5b) = ((c >> 16 & 0xff) >> 3, (c >> 8 & 0xff) >> 3, (c & 0xff) >> 3);
    let ex = |v: u32| (v << 3) | (v >> 2);
    let (cx0, cy0, cx1, cy1) = clip.unwrap_or((0, 0, dst.width as i32, dst.height as i32));
    for my in 0..mask.height as i32 {
        let py = y + my;
        if py < cy0.max(0) || py >= cy1.min(dst.height as i32) {
            continue;
        }
        for mx in 0..mask.width as i32 {
            let px = x + mx;
            if px < cx0.max(0) || px >= cx1.min(dst.width as i32) {
                continue;
            }
            let g = mask.pixels[(my * mask.width as i32 + mx) as usize] >> 24;
            if g == 0 {
                continue;
            }
            dst.pixels[(py * dst.width as i32 + px) as usize] = 0xff00_0000 | ex(c5r * g >> 8) << 16 | ex(c5g * g >> 8) << 8 | ex(c5b * g >> 8);
        }
    }
}

/// Milliseconds per tick at the default tempo. Scripts specify tempo 100.
pub fn tick_ms(tempo: u16) -> f32 {
    tempo.max(10) as f32
}

/// A script instance being played.
pub struct Player {
    /// tSCR resource id.
    pub id: u16,
    pub script: Rc<Script>,
    pub sprites: Rc<SpriteSet>,
    pc: usize,
    cels: Vec<Cel>,
    wait_ms: f32,
    tempo: u16,
    pub finished: bool,
    pub looping: bool,
}

impl Player {
    pub fn new(assets: &Assets, archive: &str, id: u16) -> Option<Player> {
        let script = Rc::new(assets.script(archive, id)?);
        let sprites = assets.sprites(archive, script.bitmap_set)?;
        let mut p = Player {
            id,
            cels: script.first_frame().to_vec(),
            script,
            sprites,
            pc: 0,
            wait_ms: 0.0,
            tempo: 100,
            finished: false,
            looping: false,
        };
        p.pc = 0;
        Some(p)
    }

    pub fn bounds(&self) -> Rect {
        self.script.bounds
    }

    pub fn restart(&mut self) {
        self.pc = 0;
        self.wait_ms = 0.0;
        self.finished = false;
        self.cels = self.script.first_frame().to_vec();
    }

    /// Jump to the final frame without playing (used for "held" states).
    pub fn finish_now(&mut self) {
        for op in &self.script.ops {
            if let Op::Frame(c) = op {
                self.cels = c.clone();
            }
        }
        self.pc = self.script.ops.len();
        self.finished = true;
    }

    /// Advance by `dt` ms. Returns sound indices triggered.
    pub fn update(&mut self, dt: f32) -> Vec<u16> {
        let mut sounds = Vec::new();
        if self.finished {
            return sounds;
        }
        self.wait_ms -= dt;
        let mut guard = 0;
        while self.wait_ms <= 0.0 && !self.finished {
            guard += 1;
            if guard > 10_000 {
                break;
            }
            if self.pc >= self.script.ops.len() {
                if self.looping {
                    self.pc = 0;
                    continue;
                }
                self.finished = true;
                break;
            }
            match &self.script.ops[self.pc] {
                Op::Tempo(t) => self.tempo = *t,
                Op::Sound { index, .. } => sounds.push(*index),
                Op::Frame(c) => self.cels = c.clone(),
                Op::Tick => self.wait_ms += tick_ms(self.tempo),
                Op::Unknown { .. } => {}
            }
            self.pc += 1;
        }
        sounds
    }

    /// Show frame `n` (held, no sounds).
    pub fn seek_frame(&mut self, n: usize) {
        let frames = self.script.frames();
        if let Some(f) = frames.get(n.min(frames.len().saturating_sub(1))) {
            self.cels = f.to_vec();
        }
        self.finished = true;
    }

    /// Cels of the current frame with their screen positions: (cel, x, y).
    pub fn placed(&self) -> Vec<(u16, i32, i32)> {
        self.cels
            .iter()
            .map(|c| {
                let (rx, ry) = self.sprites.reg_of(c.cel).unwrap_or((c.x as i32, c.y as i32));
                (c.cel, self.script.bounds.left as i32 + c.x as i32 - rx, self.script.bounds.top as i32 + c.y as i32 - ry)
            })
            .collect()
    }

    pub fn draw(&self, dst: &mut Image) {
        draw_cels(dst, &self.sprites, self.script.bounds, &self.cels, None);
    }
}

pub fn draw_cels(
    dst: &mut Image,
    sprites: &SpriteSet,
    bounds: Rect,
    cels: &[Cel],
    clip: Option<(i32, i32, i32, i32)>,
) {
    // Every cel is drawn: the 0x8000 bit is not a visibility flag (full-screen stills such
    // as the splash background have it clear).
    for c in cels {
        let Some(img) = sprites.cel(c.cel) else {
            continue;
        };
        let (rx, ry) = sprites.reg_of(c.cel).unwrap_or((c.x as i32, c.y as i32));
        let x = bounds.left as i32 + c.x as i32 - rx;
        let y = bounds.top as i32 + c.y as i32 - ry;
        if sprites.masks.get((c.cel as usize).wrapping_sub(1)).copied().unwrap_or(false) {
            draw_tint_mask(dst, img, x, y, clip);
        } else {
            gfx::blit_clip(dst, img, x, y, clip);
        }
    }
}
