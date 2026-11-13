//! The original tool option panels from `Data/Tools/TP.MHK` (docs/RE_PANELS.md):
//! a background script per panel, MBTN button tables (tool variants, sizes, shapes,
//! modes…), and an N-slot item scroller with its own slot/selection/hover scripts,
//! paging arrows and page number.

use crate::anim::Player;
use crate::assets::Assets;
use crate::audio::{self, Audio};
use crate::buttons::{self, UiButton};
use crate::gfx::{self, Image};

pub const TP: &str = "Data/Tools/TP.MHK";
pub const CMD_UP: u16 = 18350;
pub const CMD_DOWN: u16 = 18351;
const SLOT: i32 = 52;
const PITCH: i32 = 51;
const SLOT_Y: i32 = 525;
const ROW_RIGHT: i32 = 695;

/// Which art makes up a panel.
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub background: u16,
    pub mbtns: Vec<u16>,
    /// (slot-row script, visible slots)
    pub scroller: Option<(u16, usize)>,
}

pub struct Panel {
    pub layout: Layout,
    bg: Option<Player>,
    pub buttons: Vec<UiButton>,
    /// Buttons not shown (e.g. the audio panel's record-mode group).
    pub hidden: Vec<bool>,
    row: Option<Player>,
    sel_frames: Vec<Player>,
    hover_frames: Vec<Player>,
    arrows_off: Option<Player>,
    hover: Option<usize>,
    pub hover_slot: Option<usize>,
    digits: Option<std::rc::Rc<crate::assets::SpriteSet>>,
}

fn held(assets: &Assets, arch: &str, id: u16) -> Option<Player> {
    Player::new(assets, arch, id).map(|mut p| {
        p.finish_now();
        p
    })
}

fn synthetic(assets: &Assets, cmd: u16, rest: u16, hover: u16, press: u16) -> Option<UiButton> {
    let mk = |s| Player::new(assets, TP, s);
    let idle = mk(rest)?;
    let rect = idle.bounds();
    let mut b = UiButton { cmd, group: cmd, value: 0, idle: Some(idle), hover: mk(hover), click: mk(press), on: None, rect, showing: 0 };
    b.show(0, false);
    Some(b)
}

impl Panel {
    pub fn new(assets: &Assets, layout: Layout) -> Panel {
        let mut buttons = Vec::new();
        for &m in &layout.mbtns {
            buttons.extend(buttons::load(assets, TP, m));
        }
        let (mut sel_frames, mut hover_frames) = (Vec::new(), Vec::new());
        let mut row = None;
        let mut arrows_off = None;
        if let Some((row_id, n)) = layout.scroller {
            row = held(assets, TP, row_id);
            // The User Guide's screenshots show the chosen item in the red frame (18008+,
            // cel 741) and the item under the mouse in the green one (18001+), the reverse
            // of docs/RE_PANELS.md §1.4.
            for k in 0..n {
                if let Some(p) = held(assets, TP, (18015 - n + k) as u16) {
                    sel_frames.push(p);
                }
                if let Some(p) = held(assets, TP, (18008 - n + k) as u16) {
                    hover_frames.push(p);
                }
            }
            buttons.extend(synthetic(assets, CMD_UP, 18028, 18029, 18030));
            buttons.extend(synthetic(assets, CMD_DOWN, 18031, 18032, 18033));
        } else {
            arrows_off = Player::new(assets, TP, 18034).map(|mut p| {
                p.looping = true;
                p
            });
        }
        let n = buttons.len();
        Panel {
            bg: held(assets, TP, layout.background),
            hidden: vec![false; n],
            buttons,
            row,
            sel_frames,
            hover_frames,
            arrows_off,
            hover: None,
            hover_slot: None,
            digits: assets.sprites(crate::easel::EASEL, 1000),
            layout,
        }
    }

    pub fn slots(&self) -> usize {
        self.layout.scroller.map(|s| s.1).unwrap_or(0)
    }

    /// Screen rect of scroller slot k.
    pub fn slot_rect(&self, k: usize) -> (i32, i32, i32, i32) {
        let n = self.slots() as i32;
        (ROW_RIGHT - (n - k as i32) * PITCH - 1, SLOT_Y, SLOT, SLOT)
    }

    pub fn slot_at(&self, x: i32, y: i32) -> Option<usize> {
        (0..self.slots()).find(|&k| {
            let r = self.slot_rect(k);
            x >= r.0 && y >= r.1 && x < r.0 + r.2 && y < r.1 + r.3
        })
    }

    pub fn button_at(&self, x: i32, y: i32) -> Option<usize> {
        (0..self.buttons.len()).find(|&i| !self.hidden.get(i).copied().unwrap_or(false) && self.buttons[i].rect.contains(x, y))
    }

    pub fn set_group_hidden(&mut self, group: u16, hidden: bool) {
        for (i, b) in self.buttons.iter().enumerate() {
            if b.group == group && b.cmd != CMD_UP && b.cmd != CMD_DOWN {
                self.hidden[i] = hidden;
            }
        }
    }

    fn play_sounds(b: &mut UiButton, assets: &Assets, audio: &Audio, enabled: bool) {
        let base = b.sound_base();
        let idx = b.player().map(|p| p.update(0.0)).unwrap_or_default();
        if enabled {
            for k in idx {
                if let Some(s) = assets.sound(TP, base + k) {
                    audio.play(s, audio::CH_UI, false);
                }
            }
        }
    }

    /// Rollover handling; `selected` tells which buttons are shown as chosen.
    pub fn mouse_move(&mut self, x: i32, y: i32, assets: &Assets, audio: &Audio, sounds: bool, selected: &[bool]) {
        self.hover_slot = self.slot_at(x, y);
        let h = self.button_at(x, y);
        if h == self.hover {
            return;
        }
        let sel = |i: usize| selected.get(i).copied().unwrap_or(false);
        if let Some(i) = self.hover {
            if !sel(i) {
                self.buttons[i].show(0, false);
            }
        }
        if let Some(i) = h {
            if !sel(i) {
                self.buttons[i].show(1, true);
                Self::play_sounds(&mut self.buttons[i], assets, audio, sounds);
            }
        }
        self.hover = h;
    }

    /// Press on a button: plays its press script. Returns (cmd, value).
    pub fn press(&mut self, i: usize, assets: &Assets, audio: &Audio, sounds: bool) -> (u16, u16) {
        let b = &mut self.buttons[i];
        b.show(2, true);
        Self::play_sounds(b, assets, audio, sounds);
        (b.cmd, b.value)
    }

    /// Show selected buttons held at the end of their press script, others at rest.
    pub fn sync(&mut self, selected: &[bool]) {
        for (i, b) in self.buttons.iter_mut().enumerate() {
            // Chosen buttons hold their "on" art (MBTN field 2/3, else the press script's end).
            let sel = selected.get(i).copied().unwrap_or(false);
            let on = if b.on.is_some() { 3 } else { 2 };
            let pressing = b.showing == 2 && !b.player_ref().is_some_and(|p| p.finished);
            if sel && b.showing != on && !pressing {
                b.show(on, false);
            } else if !sel && (b.showing == 2 || b.showing == 3) && Some(i) != self.hover && !pressing {
                b.show(0, false);
            }
        }
    }

    pub fn update(&mut self, dt: f32, assets: &Assets, audio: &Audio, sounds: bool) {
        for b in self.buttons.iter_mut() {
            let base = b.sound_base();
            if let Some(p) = b.player() {
                for k in p.update(dt) {
                    if sounds {
                        if let Some(s) = assets.sound(TP, base + k) {
                            audio.play(s, audio::CH_UI, false);
                        }
                    }
                }
            }
        }
        if let Some(p) = self.arrows_off.as_mut() {
            p.update(dt);
        }
    }

    /// Draw the panel. `icons` = icons of the visible items, `selected_slot`, `page`, `pages`.
    pub fn draw(&self, frame: &mut Image, icons: &[Option<&Image>], selected_slot: Option<usize>, page: usize, pages: usize) {
        if let Some(p) = &self.bg {
            p.draw(frame);
        }
        for (i, b) in self.buttons.iter().enumerate() {
            if (b.cmd == CMD_UP || b.cmd == CMD_DOWN) && pages <= 1 || self.hidden.get(i).copied().unwrap_or(false) {
                continue;
            }
            if let Some(p) = b.player_ref() {
                p.draw(frame);
            }
        }
        if let Some(p) = &self.row {
            p.draw(frame);
        }
        for (k, icon) in icons.iter().enumerate().take(self.slots()) {
            if let Some(img) = icon {
                let r = self.slot_rect(k);
                gfx::blit(frame, img, r.0 + (SLOT / 2 - img.width as i32 / 2), r.1 + (SLOT / 2 - img.height as i32 / 2));
            }
        }
        if let Some(k) = selected_slot {
            if let Some(p) = self.sel_frames.get(k) {
                p.draw(frame);
            }
        }
        if let Some(k) = self.hover_slot.filter(|&k| k < icons.len() && icons[k].is_some()) {
            if let Some(p) = self.hover_frames.get(k) {
                p.draw(frame);
            }
        }
        if let Some(p) = &self.arrows_off {
            p.draw(frame);
        }
        // Page number (Easel script 1078 position, digit cels 75 + d).
        if pages > 1 {
            if let Some(d) = &self.digits {
                let n = (page + 1).min(99);
                for (i, digit) in [n / 10, n % 10].iter().enumerate() {
                    if let Some(c) = d.cel(75 + *digit as u16) {
                        gfx::blit(frame, c, 720 + i as i32 * 8, 547);
                    }
                }
            }
        }
    }
}

/// Shutter animation between panels: script 18036 (close) then 18035 (open).
pub struct Shutter {
    close: Option<Player>,
    open: Option<Player>,
    pub swapped: bool,
}

impl Shutter {
    pub fn new(assets: &Assets) -> Shutter {
        Shutter { close: Player::new(assets, TP, 18036), open: Player::new(assets, TP, 18035), swapped: false }
    }

    /// Advance; returns true when it's time to swap in the new panel (once).
    pub fn update(&mut self, dt: f32, assets: &Assets, audio: &Audio, sounds: bool) -> bool {
        let play = |p: &mut Player| {
            let base = p.script.bitmap_set;
            for k in p.update(dt) {
                if sounds {
                    if let Some(s) = assets.sound(TP, base + k) {
                        audio.play(s, audio::CH_UI, false);
                    }
                }
            }
        };
        if !self.swapped {
            match self.close.as_mut() {
                Some(c) if !c.finished => {
                    play(c);
                    false
                }
                _ => {
                    self.swapped = true;
                    true
                }
            }
        } else {
            if let Some(o) = self.open.as_mut() {
                play(o);
            }
            false
        }
    }

    pub fn done(&self) -> bool {
        self.swapped && self.open.as_ref().is_none_or(|o| o.finished)
    }

    pub fn draw(&self, frame: &mut Image) {
        let p = if self.swapped { self.open.as_ref() } else { self.close.as_ref() };
        if let Some(p) = p {
            p.draw(frame);
        }
    }
}
