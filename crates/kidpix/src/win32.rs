//! Native-looking Win32 dialog pieces (the original's DIALOG templates use MS Sans
//! Serif 8 in the Windows 9x/2000 grey 3-D style): frame with title bar, group boxes,
//! radio buttons and push buttons.

use crate::gfx::{self, Image};
use crate::ui::Fonts;

pub const FACE_SIZE: f32 = 11.0;
const GREY: u32 = 0xffc0_c0c0;
const DARK: u32 = 0xff80_8080;
const BLACK: u32 = 0xff00_0000;
const WHITE: u32 = 0xffff_ffff;
const TITLE: u32 = 0xff00_0080;

pub type R = (i32, i32, i32, i32);

pub fn inside(r: R, x: i32, y: i32) -> bool {
    x >= r.0 && y >= r.1 && x < r.0 + r.2 && y < r.1 + r.3
}

fn bevel(f: &mut Image, r: R, raised: bool) {
    let (x, y, w, h) = r;
    let (tl, br) = if raised { (WHITE, BLACK) } else { (DARK, WHITE) };
    gfx::fill_rect(f, x, y, w, 1, tl);
    gfx::fill_rect(f, x, y, 1, h, tl);
    gfx::fill_rect(f, x, y + h - 1, w, 1, br);
    gfx::fill_rect(f, x + w - 1, y, 1, h, br);
    if raised {
        gfx::fill_rect(f, x + 1, y + h - 2, w - 2, 1, DARK);
        gfx::fill_rect(f, x + w - 2, y + 1, 1, h - 2, DARK);
    }
}

/// Dialog window with caption; returns the client rect.
pub fn frame(f: &mut Image, fonts: &Fonts, face: usize, r: R, title: &str) -> R {
    let (x, y, w, h) = r;
    gfx::fill_rect(f, x, y, w, h, GREY);
    bevel(f, r, true);
    gfx::fill_rect(f, x + 3, y + 3, w - 6, 18, TITLE);
    fonts.draw(f, face, FACE_SIZE + 1.0, (x + 7) as f32, (y + 16) as f32, WHITE, title);
    fonts.draw(f, face, FACE_SIZE + 1.0, (x + 8) as f32, (y + 16) as f32, WHITE, title);
    (x + 4, y + 24, w - 8, h - 28)
}

pub fn group(f: &mut Image, fonts: &Fonts, face: usize, r: R, label: &str) {
    let (x, y, w, h) = r;
    for (dx, c) in [(0, DARK), (1, WHITE)] {
        gfx::fill_rect(f, x + dx, y + 6 + dx, w - 1, 1, c);
        gfx::fill_rect(f, x + dx, y + 6 + dx, 1, h - 7, c);
        gfx::fill_rect(f, x + dx, y + h - 1 + dx - 1, w - 1, 1, c);
        gfx::fill_rect(f, x + w - 2 + dx, y + 6 + dx, 1, h - 7, c);
    }
    let lw = fonts.measure(face, FACE_SIZE, label) as i32 + 4;
    gfx::fill_rect(f, x + 7, y, lw, 13, GREY);
    fonts.draw(f, face, FACE_SIZE, (x + 9) as f32, (y + 10) as f32, BLACK, label);
}

/// Radio button with its label; returns the clickable rect.
pub fn radio(f: &mut Image, fonts: &Fonts, face: usize, x: i32, y: i32, label: &str, checked: bool, enabled: bool) -> R {
    let cx = x + 6;
    let cy = y + 6;
    for yy in -6..=6 {
        for xx in -6..=6 {
            let d = ((xx * xx + yy * yy) as f32).sqrt();
            let c = if d <= 4.5 {
                WHITE
            } else if d <= 5.5 {
                if xx + yy < 0 { BLACK } else { GREY }
            } else if d <= 6.5 {
                if xx + yy < 0 { DARK } else { WHITE }
            } else {
                continue;
            };
            f.put(cx + xx, cy + yy, c);
        }
    }
    if checked {
        for yy in -2..=2 {
            for xx in -2..=2 {
                if xx * xx + yy * yy <= 5 {
                    f.put(cx + xx, cy + yy, if enabled { BLACK } else { DARK });
                }
            }
        }
    }
    let col = if enabled { BLACK } else { DARK };
    if !enabled {
        fonts.draw(f, face, FACE_SIZE, (x + 18) as f32, (y + 11) as f32, WHITE, label);
    }
    fonts.draw(f, face, FACE_SIZE, (x + 17) as f32, (y + 10) as f32, col, label);
    (x, y - 1, 18 + fonts.measure(face, FACE_SIZE, label) as i32 + 2, 14)
}

pub fn button(f: &mut Image, fonts: &Fonts, face: usize, r: R, label: &str, pressed: bool, default: bool) {
    let (x, y, w, h) = r;
    gfx::fill_rect(f, x, y, w, h, GREY);
    if default {
        gfx::fill_rect(f, x, y, w, 1, BLACK);
        gfx::fill_rect(f, x, y + h - 1, w, 1, BLACK);
        gfx::fill_rect(f, x, y, 1, h, BLACK);
        gfx::fill_rect(f, x + w - 1, y, 1, h, BLACK);
        bevel(f, (x + 1, y + 1, w - 2, h - 2), !pressed);
    } else {
        bevel(f, r, !pressed);
    }
    let lw = fonts.measure(face, FACE_SIZE, label);
    let off = if pressed { 1 } else { 0 };
    fonts.draw(f, face, FACE_SIZE, x as f32 + (w as f32 - lw) / 2.0 + off as f32, (y + h / 2 + 4 + off) as f32, BLACK, label);
}

/// The Easel's "Print This Picture" dialog (DIALOG 150).
pub struct PrintDialog {
    /// 0 Full Page, 1 Poster.
    pub mode: u8,
    /// Poster size: 2 or 3 pages across.
    pub poster: u8,
    pressed: Option<u16>,
}

pub enum PrintResult {
    None,
    Cancel,
    /// Pages across (1 = full page).
    Print(u8),
}

const DLG: R = (250, 180, 300, 200);

impl PrintDialog {
    pub fn new() -> PrintDialog {
        PrintDialog { mode: 0, poster: 2, pressed: None }
    }

    fn layout(&self) -> [(u16, R); 6] {
        let (x, y, _, _) = DLG;
        [
            (1000, (x + 30, y + 56, 120, 14)),
            (1004, (x + 30, y + 80, 120, 14)),
            (1006, (x + 54, y + 102, 120, 14)),
            (1007, (x + 54, y + 122, 120, 14)),
            (1013, (x + 60, y + 160, 75, 23)),
            (1014, (x + 165, y + 160, 75, 23)),
        ]
    }

    pub fn mouse_down(&mut self, x: i32, y: i32) {
        self.pressed = self.layout().iter().find(|(_, r)| inside(*r, x, y)).map(|(id, _)| *id);
    }

    pub fn mouse_up(&mut self, x: i32, y: i32) -> PrintResult {
        let Some(id) = self.pressed.take() else { return PrintResult::None };
        let Some((_, r)) = self.layout().into_iter().find(|(i, _)| *i == id) else { return PrintResult::None };
        if !inside(r, x, y) {
            return PrintResult::None;
        }
        match id {
            1000 => self.mode = 0,
            1004 => self.mode = 1,
            1006 if self.mode == 1 => self.poster = 2,
            1007 if self.mode == 1 => self.poster = 3,
            1013 => return PrintResult::Print(if self.mode == 0 { 1 } else { self.poster }),
            1014 => return PrintResult::Cancel,
            _ => {}
        }
        PrintResult::None
    }


    pub fn draw(&self, f: &mut Image, fonts: &Fonts, face: usize) {
        let c = frame(f, fonts, face, DLG, "Print This Picture");
        group(f, fonts, face, (c.0 + 10, c.1 + 8, c.2 - 20, 118), "Print Mode");
        let l = self.layout();
        radio(f, fonts, face, l[0].1 .0, l[0].1 .1, "Full Page", self.mode == 0, true);
        radio(f, fonts, face, l[1].1 .0, l[1].1 .1, "Poster", self.mode == 1, true);
        radio(f, fonts, face, l[2].1 .0, l[2].1 .1, "2 x 2 Pages", self.poster == 2, self.mode == 1);
        radio(f, fonts, face, l[3].1 .0, l[3].1 .1, "3 x 3 Pages", self.poster == 3, self.mode == 1);
        button(f, fonts, face, l[4].1, "OK", self.pressed == Some(1013), true);
        button(f, fonts, face, l[5].1, "Cancel", self.pressed == Some(1014), false);
    }
}

/// Toolbox > Check Spelling (DIALOG 129 "Spell Checker", 218×258 dialog units).
pub struct SpellDialog {
    pub word: String,
    pub change: String,
    pub suggestions: Vec<String>,
    pub sel: Option<usize>,
    pressed: Option<u16>,
}

pub enum SpellResult {
    None,
    Change(String),
    Skip,
    Learn,
    Done,
}

/// Dialog units → pixels for MS Sans Serif 8 (base units 6×13).
fn du(x: i32, y: i32) -> (i32, i32) {
    (x * 3 / 2, y * 13 / 8)
}

const SPELL_ORIGIN: (i32, i32) = (232, 70);

impl SpellDialog {
    pub fn new(word: &str, suggestions: Vec<String>) -> SpellDialog {
        let change = suggestions.first().cloned().unwrap_or_else(|| word.to_string());
        SpellDialog { word: word.to_string(), sel: if suggestions.is_empty() { None } else { Some(0) }, change, suggestions, pressed: None }
    }

    fn client() -> (i32, i32) {
        (SPELL_ORIGIN.0 + 4, SPELL_ORIGIN.1 + 24)
    }

    fn rect(x: i32, y: i32, w: i32, h: i32) -> R {
        let (cx, cy) = Self::client();
        let (px, py) = du(x, y);
        let (pw, ph) = du(w, h);
        (cx + px, cy + py, pw, ph)
    }

    fn buttons() -> [(u16, &'static str, R); 4] {
        [
            (4006, "Change", Self::rect(7, 79, 50, 14)),
            (3999, "Skip", Self::rect(85, 79, 50, 14)),
            (4007, "Learn", Self::rect(161, 79, 50, 14)),
            (3998, "Done", Self::rect(161, 237, 50, 14)),
        ]
    }

    fn list_rect() -> R {
        Self::rect(7, 121, 204, 100)
    }

    pub fn mouse_down(&mut self, x: i32, y: i32) {
        self.pressed = Self::buttons().iter().find(|b| inside(b.2, x, y)).map(|b| b.0);
        let l = Self::list_rect();
        if inside(l, x, y) {
            let i = ((y - l.1 - 2) / 13) as usize;
            if i < self.suggestions.len() {
                self.sel = Some(i);
                self.change = self.suggestions[i].clone();
            }
        }
    }

    pub fn mouse_up(&mut self, x: i32, y: i32) -> SpellResult {
        let Some(id) = self.pressed.take() else { return SpellResult::None };
        if !Self::buttons().iter().any(|b| b.0 == id && inside(b.2, x, y)) {
            return SpellResult::None;
        }
        match id {
            4006 => SpellResult::Change(self.change.clone()),
            3999 => SpellResult::Skip,
            4007 => SpellResult::Learn,
            _ => SpellResult::Done,
        }
    }

    /// Typing into the "Change to:" field; Enter = Change, Escape = Done.
    pub fn key(&mut self, c: char) -> SpellResult {
        match c {
            '\u{8}' => {
                self.change.pop();
            }
            '\r' | '\n' => return SpellResult::Change(self.change.clone()),
            '\u{1b}' => return SpellResult::Done,
            c if !c.is_control() => self.change.push(c),
            _ => {}
        }
        SpellResult::None
    }

    pub fn draw(&self, f: &mut Image, fonts: &Fonts, face: usize, blink: bool) {
        let (w, h) = du(218, 258);
        frame(f, fonts, face, (SPELL_ORIGIN.0, SPELL_ORIGIN.1, w + 8, h + 28), "Spell Checker");
        let text = |f: &mut Image, x: i32, y: i32, s: &str| {
            let r = Self::rect(x, y, 0, 0);
            fonts.draw(f, face, FACE_SIZE, r.0 as f32, (r.1 + 9) as f32, BLACK, s);
        };
        text(f, 7, 7, "Unknown word:");
        let r = Self::rect(70, 7, 0, 0);
        fonts.draw(f, face, FACE_SIZE, r.0 as f32, (r.1 + 9) as f32, BLACK, &self.word);
        fonts.draw(f, face, FACE_SIZE, r.0 as f32 + 1.0, (r.1 + 9) as f32, BLACK, &self.word);
        text(f, 7, 37, "Change to:");
        // Sunken edit field.
        let e = Self::rect(7, 48, 204, 12);
        gfx::fill_rect(f, e.0, e.1, e.2, e.3, WHITE);
        bevel(f, e, false);
        let tw = fonts.draw(f, face, FACE_SIZE, (e.0 + 3) as f32, (e.1 + e.3 / 2 + 4) as f32, BLACK, &self.change);
        if blink {
            gfx::fill_rect(f, e.0 + 3 + tw as i32 + 1, e.1 + 3, 1, e.3 - 6, BLACK);
        }
        for (id, label, r) in Self::buttons() {
            button(f, fonts, face, r, label, self.pressed == Some(id), id == 4006);
        }
        text(f, 7, 110, "Suggestions:");
        let l = Self::list_rect();
        gfx::fill_rect(f, l.0, l.1, l.2, l.3, WHITE);
        bevel(f, l, false);
        for (i, s) in self.suggestions.iter().enumerate() {
            let y = l.1 + 2 + i as i32 * 13;
            if y + 13 > l.1 + l.3 {
                break;
            }
            let sel = self.sel == Some(i);
            if sel {
                gfx::fill_rect(f, l.0 + 2, y, l.2 - 4, 13, TITLE);
            }
            fonts.draw(f, face, FACE_SIZE, (l.0 + 4) as f32, (y + 10) as f32, if sel { WHITE } else { BLACK }, s);
        }
    }
}

/// Email > Send: "Send an Online Greeting To Your Friends" (DIALOG 160).
pub struct EmailDialog {
    pub movie: bool,
    pressed: Option<u16>,
}

pub enum EmailResult {
    None,
    Send { movie: bool },
    Cancel,
}

const EMAIL_DLG: R = (200, 170, 400, 230);

impl EmailDialog {
    pub fn new() -> EmailDialog {
        EmailDialog { movie: false, pressed: None }
    }

    fn layout() -> [(u16, R); 4] {
        let (x, y, _, _) = EMAIL_DLG;
        [
            (1001, (x + 24, y + 60, 160, 14)),
            (1000, (x + 24, y + 110, 160, 14)),
            (1, (x + 80, y + 190, 100, 23)),
            (2, (x + 220, y + 190, 100, 23)),
        ]
    }

    pub fn mouse_down(&mut self, x: i32, y: i32) {
        self.pressed = Self::layout().iter().find(|(_, r)| inside(*r, x, y)).map(|(id, _)| *id);
    }

    pub fn mouse_up(&mut self, x: i32, y: i32) -> EmailResult {
        let Some(id) = self.pressed.take() else { return EmailResult::None };
        let Some((_, r)) = Self::layout().into_iter().find(|(i, _)| *i == id) else { return EmailResult::None };
        if !inside(r, x, y) {
            return EmailResult::None;
        }
        match id {
            1001 => self.movie = false,
            1000 => self.movie = true,
            1 => return EmailResult::Send { movie: self.movie },
            _ => return EmailResult::Cancel,
        }
        EmailResult::None
    }

    pub fn draw(&self, f: &mut Image, fonts: &Fonts, face: usize) {
        let c = frame(f, fonts, face, EMAIL_DLG, "Send an Online Greeting To Your Friends");
        let l = Self::layout();
        radio(f, fonts, face, l[0].1 .0, l[0].1 .1, "Send a Picture", !self.movie, true);
        fonts.draw(f, face, FACE_SIZE, (c.0 + 40) as f32, (l[0].1 .1 + 28) as f32, BLACK, "A still picture (JPEG) of your page.");
        radio(f, fonts, face, l[1].1 .0, l[1].1 .1, "Send a Movie", self.movie, true);
        fonts.draw(f, face, FACE_SIZE, (c.0 + 40) as f32, (l[1].1 .1 + 28) as f32, BLACK, "Your page with its sound and animation (.kpx).");
        fonts.draw(f, face, FACE_SIZE, (c.0 + 16) as f32, (c.1 + 150) as f32, BLACK, "Your email program opens; attach the saved file to the message.");
        button(f, fonts, face, l[2].1, "SEND", self.pressed == Some(1), true);
        button(f, fonts, face, l[3].1, "DON'T SEND", self.pressed == Some(2), false);
    }
}
