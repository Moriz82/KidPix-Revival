//! Data-driven buttons: an `MBTN` table whose entries name rest / rollover / press
//! scripts (see docs/FORMATS.md). Used by the easel and every other original screen.

use crate::anim::Player;
use crate::assets::{self, Assets};
use mohawk::Rect;

pub struct UiButton {
    pub cmd: u16,
    pub group: u16,
    /// MBTN field 8: the value sent with the command (sizes 1..3, pencil index…).
    pub value: u16,
    pub idle: Option<Player>,
    pub hover: Option<Player>,
    pub click: Option<Player>,
    /// "On" / selected look: MBTN field 2, or field 3 when field 2 is 0 (Appendix B).
    pub on: Option<Player>,
    pub rect: Rect,
    /// Displayed script: 0 = rest (MBTN field 0), 1 = rollover (field 1), 2 = press (field 10).
    pub showing: u8,
}

impl UiButton {
    pub fn player(&mut self) -> Option<&mut Player> {
        match self.showing {
            0 => self.idle.as_mut(),
            1 => self.hover.as_mut().or(self.idle.as_mut()),
            3 => self.on.as_mut().or(self.click.as_mut()).or(self.idle.as_mut()),
            _ => self.click.as_mut().or(self.idle.as_mut()),
        }
    }
    pub fn player_ref(&self) -> Option<&Player> {
        match self.showing {
            0 => self.idle.as_ref(),
            1 => self.hover.as_ref().or(self.idle.as_ref()),
            3 => self.on.as_ref().or(self.click.as_ref()).or(self.idle.as_ref()),
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
                on: mk(if b.raw[2] != 0 { b.raw[2] } else { b.raw[3] }),
                rect,
                showing: 0,
            };
            ub.show(0, false);
            ub
        })
        .collect()
}


