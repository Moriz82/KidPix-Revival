//! The original colour picker (`Data/BigPicker.MHK`, docs/RE_PANELS.md §4).
//!
//! * Pressing the colour wheel (Easel script 1118, (0,491)–(85,600)) plays the opening
//!   script 1500; the open panel is script 1502.
//! * While the button stays down, dragging over a cell (mask `tBMP 1500` = 0xFF, aligned to
//!   the screen origin) picks the art pixel under the pointer; each change plays
//!   `Misc.MHK` tWAV 31504.
//! * Releasing closes the panel with script 1501 — unless released on the eye dropper
//!   (1503 rest / 1504 hover / 1505 pressed), which arms pick-from-canvas mode.

use crate::anim::Player;
use crate::assets::{self, Assets};
use crate::audio::{self, Audio};
use crate::gfx::Image;

const ARCH: &str = "Data/BigPicker.MHK";
const WHEEL: (i32, i32, i32, i32) = (0, 491, 85, 600);
const DROPPER: (i32, i32, i32, i32) = (103, 563, 150, 597);

enum State {
    Closed,
    Opening(Player),
    Open(Player),
    Closing(Player),
}

pub struct ColorPicker {
    state: State,
    dropper: Option<Player>,
    dropper_hover: bool,
    /// Pick-from-canvas mode (the eye dropper).
    pub eyedropper: bool,
    /// The button is held since the wheel press.
    held: bool,
    mask: Option<mohawk::Indexed>,
    /// The open panel rendered once, to sample cell colours from.
    art: Option<Image>,
    last: Option<u32>,
}

fn inside(r: (i32, i32, i32, i32), x: i32, y: i32) -> bool {
    x >= r.0 && y >= r.1 && x < r.2 && y < r.3
}

pub enum Pick {
    Nothing,
    EyeDropper,
}

impl ColorPicker {
    pub fn new() -> ColorPicker {
        ColorPicker { state: State::Closed, dropper: None, dropper_hover: false, eyedropper: false, held: false, mask: None, art: None, last: None }
    }


    /// The pointer is captured by the picker (button held since the wheel press).
    pub fn captured(&self) -> bool {
        self.held
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

    fn dropper_script(&mut self, assets: &Assets, audio: &Audio, sounds: bool, id: u16, play: bool) {
        if let Some(mut d) = Player::new(assets, ARCH, id) {
            if play {
                Self::sounds(&mut d, assets, audio, sounds, 0.0);
            } else {
                d.finish_now();
            }
            self.dropper = Some(d);
        }
    }

    /// Mouse press: true when it opened the picker.
    pub fn press(&mut self, x: i32, y: i32, assets: &Assets, audio: &Audio, sounds: bool) -> bool {
        if !matches!(self.state, State::Closed | State::Closing(_)) || !inside(WHEEL, x, y) {
            return false;
        }
        let Some(mut p) = Player::new(assets, ARCH, 1500) else { return false };
        Self::sounds(&mut p, assets, audio, sounds, 0.0);
        self.state = State::Opening(p);
        self.held = true;
        self.eyedropper = false;
        self.last = None;
        self.dropper_hover = false;
        self.dropper_script(assets, audio, sounds, 1503, false);
        if self.mask.is_none() {
            self.mask = assets.archive(ARCH).and_then(|a| a.get(assets::T_BMP, 1500).and_then(|d| mohawk::decode_indexed(d).ok()));
        }
        if self.art.is_none() {
            if let Some(mut open) = Player::new(assets, ARCH, 1502) {
                open.finish_now();
                let mut img = Image::new(800, 600);
                open.draw(&mut img);
                self.art = Some(img);
            }
        }
        true
    }

    /// Pointer motion; while held over a cell returns the newly picked colour.
    pub fn drag(&mut self, x: i32, y: i32, assets: &Assets, audio: &Audio, sounds: bool) -> Option<u32> {
        if !self.held {
            return None;
        }
        let over = inside(DROPPER, x, y);
        if over != self.dropper_hover && matches!(self.state, State::Open(_)) {
            self.dropper_hover = over;
            self.dropper_script(assets, audio, sounds, if over { 1504 } else { 1503 }, over);
        }
        let m = self.mask.as_ref()?;
        if x < 0 || y < 0 || x >= m.width as i32 || y >= m.height as i32 {
            return None;
        }
        if m.indices[(y * m.width as i32 + x) as usize] != 0xff {
            return None;
        }
        let c = self.art.as_ref()?.get(x, y)? | 0xff00_0000;
        if self.last == Some(c) {
            return None;
        }
        self.last = Some(c);
        if sounds {
            if let Some(s) = assets.sound("Data/Misc.MHK", 31504) {
                audio.play(s, audio::CH_UI, false);
            }
        }
        Some(c)
    }

    /// Button released: closes the picker, or arms the eye dropper when released on it.
    pub fn release(&mut self, x: i32, y: i32, assets: &Assets, audio: &Audio, sounds: bool) -> Pick {
        if !self.held {
            return Pick::Nothing;
        }
        self.held = false;
        let dropper = inside(DROPPER, x, y);
        if dropper {
            self.dropper_script(assets, audio, sounds, 1505, true);
            self.eyedropper = true;
        }
        if let Some(mut p) = Player::new(assets, ARCH, 1501) {
            Self::sounds(&mut p, assets, audio, sounds, 0.0);
            self.state = State::Closing(p);
        } else {
            self.state = State::Closed;
        }
        if dropper {
            Pick::EyeDropper
        } else {
            Pick::Nothing
        }
    }

    pub fn update(&mut self, dt: f32, assets: &Assets, audio: &Audio, sounds: bool) {
        match &mut self.state {
            State::Opening(p) => {
                Self::sounds(p, assets, audio, sounds, dt);
                if p.finished {
                    if let Some(mut held) = Player::new(assets, ARCH, 1502) {
                        held.finish_now();
                        self.state = State::Open(held);
                    }
                }
            }
            State::Closing(p) => {
                Self::sounds(p, assets, audio, sounds, dt);
                if p.finished {
                    self.state = State::Closed;
                }
            }
            _ => {}
        }
        if let Some(d) = self.dropper.as_mut() {
            Self::sounds(d, assets, audio, sounds, dt);
        }
    }

    pub fn draw(&self, frame: &mut Image) {
        match &self.state {
            State::Opening(p) | State::Closing(p) => p.draw(frame),
            State::Open(p) => {
                p.draw(frame);
                if let Some(d) = &self.dropper {
                    d.draw(frame);
                }
            }
            State::Closed => {}
        }
    }
}
