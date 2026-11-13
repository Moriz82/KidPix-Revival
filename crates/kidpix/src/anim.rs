//! Playback of `tSCR` animation scripts against a sprite set.

use crate::assets::{Assets, SpriteSet};
use crate::gfx::{self, Image};
use mohawk::{Cel, Op, Rect, Script};
use std::rc::Rc;

/// Milliseconds per tick at the default tempo. Scripts specify tempo 100.
pub fn tick_ms(tempo: u16) -> f32 {
    tempo.max(10) as f32
}

/// A script instance being played.
pub struct Player {
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
    for c in cels {
        if !c.visible {
            continue;
        }
        let Some(img) = sprites.cel(c.cel) else {
            continue;
        };
        let (rx, ry) = sprites.reg(c.cel);
        let x = bounds.left as i32 + c.x as i32 - rx;
        let y = bounds.top as i32 + c.y as i32 - ry;
        gfx::blit_clip(dst, img, x, y, clip);
    }
}
