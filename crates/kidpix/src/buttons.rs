//! Data-driven buttons: an `MBTN` table whose entries name rest / rollover / press
//! scripts (see docs/FORMATS.md). Used by the easel and every other original screen.

use crate::anim::Player;
use crate::assets::{self, Assets};
use crate::audio::{self, Audio};
use crate::gfx::Image;
use mohawk::Rect;

pub struct UiButton {
    pub cmd: u16,
    pub group: u16,
    /// MBTN field 8: the value sent with the command (sizes 1..3, pencil index…).
    pub value: u16,
    pub idle: Option<Player>,
    pub hover: Option<Player>,
    pub click: Option<Player>,
    pub rect: Rect,
    /// Displayed script: 0 = rest (MBTN field 0), 1 = rollover (field 1), 2 = press (field 10).
    pub showing: u8,
}

impl UiButton {
    pub fn player(&mut self) -> Option<&mut Player> {
        match self.showing {
            0 => self.idle.as_mut(),
            1 => self.hover.as_mut().or(self.idle.as_mut()),
            _ => self.click.as_mut().or(self.idle.as_mut()),
        }
    }
    pub fn player_ref(&self) -> Option<&Player> {
        match self.showing {
            0 => self.idle.as_ref(),
            1 => self.hover.as_ref().or(self.idle.as_ref()),
            _ => self.click.as_ref().or(self.idle.as_ref()),
        }
    }
    pub fn show(&mut self, which: u8, play: bool) {
        self.showing = which;
        if let Some(p) = self.player() {
            if play {
                p.restart();
            } else {
                p.finish_now();
            }
        }
    }
    /// tWAV base for this button's script sound ops (= the script's bitmap-set id).
    pub fn sound_base(&self) -> u16 {
        self.idle
            .as_ref()
            .or(self.hover.as_ref())
            .map(|p| p.script.bitmap_set)
            .unwrap_or(1000)
    }
}

/// Load the buttons of `MBTN id` in `archive`.
pub fn load(assets: &Assets, archive: &str, id: u16) -> Vec<UiButton> {
    let Some(list) = assets
        .archive(archive)
        .and_then(|a| a.get(assets::MBTN, id).map(mohawk::parse_buttons))
        .and_then(|r| r.ok())
    else {
        return Vec::new();
    };
    list.into_iter()
        .map(|b| {
            let mk = |s: u16| if s == 0 { None } else { Player::new(assets, archive, s) };
            let idle = mk(b.idle);
            let rect = idle.as_ref().map(|p| p.bounds()).unwrap_or_default();
            let mut ub = UiButton {
                cmd: b.command,
                group: b.group,
                value: b.raw[8],
                idle,
                hover: mk(b.hover),
                click: mk(b.click),
                rect,
                showing: 0,
            };
            ub.show(0, false);
            ub
        })
        .collect()
}

/// A self-contained set of buttons with standard rollover/press behaviour.
pub struct ButtonSet {
    pub archive: String,
    pub buttons: Vec<UiButton>,
    hover: Option<usize>,
}

impl ButtonSet {
    pub fn new(assets: &Assets, archive: &str, ids: &[u16]) -> ButtonSet {
        let mut buttons = Vec::new();
        for &id in ids {
            buttons.extend(load(assets, archive, id));
        }
        ButtonSet {
            archive: archive.to_string(),
            buttons,
            hover: None,
        }
    }

    pub fn at(&self, x: i32, y: i32) -> Option<usize> {
        self.buttons.iter().position(|b| b.rect.contains(x, y))
    }

    fn sounds(&mut self, i: usize, assets: &Assets, audio: &Audio) {
        let base = self.buttons[i].sound_base();
        let idx = self.buttons[i].player().map(|p| p.update(0.0)).unwrap_or_default();
        for k in idx {
            if let Some(s) = assets.sound(&self.archive, base + k) {
                audio.play(s, audio::CH_UI, false);
            }
        }
    }

    pub fn mouse_move(&mut self, x: i32, y: i32, assets: &Assets, audio: &Audio) {
        let h = self.at(x, y);
        if h == self.hover {
            return;
        }
        if let Some(i) = self.hover {
            if self.buttons[i].showing != 2 {
                self.buttons[i].show(0, false);
            }
        }
        if let Some(i) = h {
            if self.buttons[i].showing != 2 {
                self.buttons[i].show(1, true);
                self.sounds(i, assets, audio);
            }
        }
        self.hover = h;
    }

    /// Press: plays the press script and returns the command id.
    pub fn press(&mut self, x: i32, y: i32, assets: &Assets, audio: &Audio) -> Option<u16> {
        let i = self.at(x, y)?;
        self.buttons[i].show(2, true);
        self.sounds(i, assets, audio);
        Some(self.buttons[i].cmd)
    }

    /// Mark the button with `cmd` as the selected one of its radio group.
    pub fn select(&mut self, cmd: u16) {
        let Some(group) = self.buttons.iter().find(|b| b.cmd == cmd).map(|b| b.group) else {
            return;
        };
        for b in self.buttons.iter_mut() {
            if b.group == group {
                if b.cmd == cmd {
                    b.show(2, false);
                } else if b.showing == 2 {
                    b.show(0, false);
                }
            }
        }
    }

    pub fn update(&mut self, dt: f32, assets: &Assets, audio: &Audio) {
        for b in self.buttons.iter_mut() {
            let base = b.sound_base();
            if let Some(p) = b.player() {
                for k in p.update(dt) {
                    if let Some(s) = assets.sound(&self.archive, base + k) {
                        audio.play(s, audio::CH_UI, false);
                    }
                }
            }
        }
    }

    pub fn draw(&self, frame: &mut Image) {
        for b in &self.buttons {
            if let Some(p) = b.player_ref() {
                p.draw(frame);
            }
        }
    }
}
