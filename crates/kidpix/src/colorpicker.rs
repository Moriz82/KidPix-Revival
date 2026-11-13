//! The original Color Picker (`Data/BigPicker.MHK`): it slides up when the mouse moves
//! toward the color splotch, offers regular colors, grays and skin tones plus the Eye
//! Dropper, and closes when the mouse leaves it.

use crate::anim::Player;
use crate::assets::Assets;
use crate::audio::{self, Audio};
use crate::gfx::Image;

const ARCH: &str = "Data/BigPicker.MHK";
/// Picker area (script bounds) and the splotch area that opens it.
const BOUNDS: (i32, i32, i32, i32) = (39, 364, 205, 600);
const SPLOTCH: (i32, i32, i32, i32) = (0, 491, 85, 600);
const DROPPER: (i32, i32, i32, i32) = (103, 563, 150, 597);
/// Cel holding the color swatches.
const PALETTE_CEL: u16 = 6;

enum State {
    Closed,
    Opening(Player),
    Open(Player),
}

pub struct ColorPicker {
    state: State,
    dropper: Option<Player>,
    pub eyedropper: bool,
}

fn inside(r: (i32, i32, i32, i32), x: i32, y: i32) -> bool {
    x >= r.0 && y >= r.1 && x < r.2 && y < r.3
}

pub enum Pick {
    Nothing,
    Color(u32),
    EyeDropper,
}

impl ColorPicker {
    pub fn new() -> ColorPicker {
        ColorPicker { state: State::Closed, dropper: None, eyedropper: false }
    }

    pub fn is_open(&self) -> bool {
        !matches!(self.state, State::Closed)
    }

    fn sounds(p: &mut Player, assets: &Assets, audio: &Audio, enabled: bool, dt: f32) {
        let base = p.script.bitmap_set;
        for i in p.update(dt) {
            if enabled {
                if let Some(s) = assets.sound(ARCH, base + i) {
                    audio.play(s, audio::CH_UI, false);
                }
            }
        }
    }

    pub fn mouse_move(&mut self, x: i32, y: i32, assets: &Assets, audio: &Audio, sounds: bool) {
        match self.state {
            State::Closed => {
                if !self.eyedropper && inside(SPLOTCH, x, y) {
                    if let Some(mut p) = Player::new(assets, ARCH, 1500) {
                        Self::sounds(&mut p, assets, audio, sounds, 0.0);
                        self.state = State::Opening(p);
                        self.dropper = Player::new(assets, ARCH, 1503).map(|mut d| {
                            d.finish_now();
                            d
                        });
                    }
                }
            }
            _ => {
                if !inside(BOUNDS, x, y) && !inside(SPLOTCH, x, y) {
                    self.state = State::Closed;
                }
            }
        }
    }

    pub fn update(&mut self, dt: f32, assets: &Assets, audio: &Audio, sounds: bool) {
        if let State::Opening(p) = &mut self.state {
            Self::sounds(p, assets, audio, sounds, dt);
            if p.finished {
                if let Some(mut held) = Player::new(assets, ARCH, 1502) {
                    held.finish_now();
                    self.state = State::Open(held);
                }
            }
        }
        if let Some(d) = self.dropper.as_mut() {
            Self::sounds(d, assets, audio, sounds, dt);
        }
    }

    /// A press while the picker is open.
    pub fn press(&mut self, x: i32, y: i32, assets: &Assets, audio: &Audio, sounds: bool) -> Option<Pick> {
        let player = match &self.state {
            State::Open(p) | State::Opening(p) => p,
            State::Closed => return None,
        };
        if !inside(BOUNDS, x, y) {
            return None;
        }
        if inside(DROPPER, x, y) {
            if let Some(mut d) = Player::new(assets, ARCH, 1505) {
                Self::sounds(&mut d, assets, audio, sounds, 0.0);
                self.dropper = Some(d);
            }
            self.eyedropper = true;
            self.state = State::Closed;
            return Some(Pick::EyeDropper);
        }
        // The color is the swatch pixel under the pointer in the palette artwork.
        for (cel, cx, cy) in player.placed() {
            if cel != PALETTE_CEL {
                continue;
            }
            if let Some(img) = player.sprites.cel(cel) {
                if let Some(p) = img.get(x - cx, y - cy) {
                    if p >> 24 == 255 {
                        return Some(Pick::Color(p | 0xff00_0000));
                    }
                }
            }
        }
        Some(Pick::Nothing)
    }

    pub fn draw(&self, frame: &mut Image) {
        match &self.state {
            State::Opening(p) | State::Open(p) => {
                p.draw(frame);
                if let (State::Open(_), Some(d)) = (&self.state, &self.dropper) {
                    d.draw(frame);
                }
            }
            State::Closed => {}
        }
    }
}
