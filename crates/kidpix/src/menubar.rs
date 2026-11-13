//! The original menu bar (KPD3.exe RT_MENU 128). It is hidden until the pointer reaches
//! the top edge of the screen; pressing there shows the bar, and the menus are used by
//! dragging (press, drag to a command, release) or by clicking.

use crate::gfx::{self, Image};
use crate::ui::Fonts;

/// Command ids from the original menu resource.
pub mod cmd {
    pub const NEW: u16 = 108;
    pub const OPEN: u16 = 101;
    pub const SAVE_AS: u16 = 102;
    pub const EXPORT: u16 = 105;
    pub const PRINT: u16 = 106;
    pub const NEW_USER: u16 = 109;
    pub const EXIT: u16 = 107;
    pub const UNDO_REDO: u16 = 201;
    pub const CUT: u16 = 202;
    pub const COPY: u16 = 203;
    pub const PASTE: u16 = 204;
    pub const ADD_GRAPHIC: u16 = 111;
    pub const ADD_MOVIE: u16 = 113;
    pub const LOAD_PICTURE: u16 = 116;
    pub const IMPORT_BACKGROUND: u16 = 115;
    pub const IMPORT_SOUND: u16 = 112;
    pub const EDIT_ABC_TEXT: u16 = 224;
    pub const CHECK_SPELLING: u16 = 210;
    pub const FLATTEN_TEXT: u16 = 225;
    pub const FLATTEN_STICKERS: u16 = 214;
    pub const SAY_IT_THIS_WAY: u16 = 238;
    pub const CHANGE_EFFECTS: u16 = 215;
    pub const PICK_BG_COLOR: u16 = 216;
    pub const IDEA_MACHINE: u16 = 239;
    pub const TOOL_SOUNDS: u16 = 218;
    pub const TEXT_TO_SPEECH: u16 = 211;
    pub const ATTACHED_SOUND: u16 = 220;
    pub const MANUAL_ADVANCE: u16 = 232;
    pub const CLEAN_UP: u16 = 222;
    pub const PROJECT_MODE: u16 = 234;
    pub const EMAIL_SETUP: u16 = 212;
    pub const EMAIL_SEND: u16 = 110;
    pub const SMALL_KIDS: u16 = 219;
    pub const HELP: u16 = 213;
    pub const ABOUT: u16 = 100;
    pub const USER_GUIDE: u16 = 237;
}

#[derive(Clone, Copy)]
pub enum Item {
    Cmd(u16, &'static str),
    Sep,
}

pub struct Menu {
    pub title: &'static str,
    pub items: &'static [Item],
}

use Item::{Cmd, Sep};

pub static MENUS: &[Menu] = &[
    Menu {
        title: "File",
        items: &[
            Cmd(cmd::NEW, "New..."),
            Cmd(cmd::OPEN, "Open..."),
            Cmd(cmd::SAVE_AS, "Save As...\tCtrl+S"),
            Sep,
            Cmd(cmd::EXPORT, "Export..."),
            Sep,
            Cmd(cmd::PRINT, "Print..."),
            Cmd(cmd::NEW_USER, "Log In as New User..."),
            Sep,
            Cmd(cmd::EXIT, "Exit\tCtrl+Q"),
        ],
    },
    Menu {
        title: "Edit",
        items: &[
            Cmd(cmd::UNDO_REDO, "Undo/Redo\tCtrl+Z"),
            Sep,
            Cmd(cmd::CUT, "Cut\tCtrl+X"),
            Cmd(cmd::COPY, "Copy\tCtrl+C"),
            Cmd(cmd::PASTE, "Paste\tCtrl+V"),
        ],
    },
    Menu {
        title: "Add",
        items: &[
            Cmd(cmd::ADD_GRAPHIC, "Add Graphic..."),
            Cmd(cmd::ADD_MOVIE, "Add Movie..."),
            Cmd(cmd::LOAD_PICTURE, "Load Picture..."),
            Sep,
            Cmd(cmd::IMPORT_BACKGROUND, "Import Background..."),
            Cmd(cmd::IMPORT_SOUND, "Import Sound..."),
        ],
    },
    Menu {
        title: "Toolbox",
        items: &[
            Cmd(cmd::EDIT_ABC_TEXT, "Edit ABC Text..."),
            Cmd(cmd::CHECK_SPELLING, "Check Spelling..."),
            Cmd(cmd::FLATTEN_TEXT, "Flatten Text..."),
            Cmd(cmd::FLATTEN_STICKERS, "Flatten Stickers and Animations..."),
            Cmd(cmd::SAY_IT_THIS_WAY, "Say It This Way..."),
            Sep,
            Cmd(cmd::CHANGE_EFFECTS, "Change Effects..."),
            Cmd(cmd::PICK_BG_COLOR, "Pick Background Color..."),
            Sep,
            Cmd(cmd::IDEA_MACHINE, "Idea Machine..."),
        ],
    },
    Menu {
        title: "Controls",
        items: &[
            Cmd(cmd::TOOL_SOUNDS, "Turn Tool Sounds On/Off"),
            Cmd(cmd::TEXT_TO_SPEECH, "Turn Text to Speech On/Off"),
            Cmd(cmd::ATTACHED_SOUND, "Turn Attached Sound On/Off"),
            Sep,
            Cmd(cmd::MANUAL_ADVANCE, "Turn Manual Advance On/Off"),
            Cmd(cmd::CLEAN_UP, "Clean Up"),
            Sep,
            Cmd(cmd::PROJECT_MODE, "Go to Project/Paint Mode"),
        ],
    },
    Menu {
        title: "Email",
        items: &[Cmd(cmd::EMAIL_SETUP, "Set Up..."), Cmd(cmd::EMAIL_SEND, "Send...")],
    },
    Menu {
        title: "Small Kids Mode",
        items: &[Cmd(cmd::SMALL_KIDS, "Turn Small Kids Mode On/Off")],
    },
    Menu {
        title: "Help",
        items: &[
            Cmd(cmd::HELP, "Help"),
            Cmd(cmd::ABOUT, "About Kid Pix Deluxe 3"),
            Cmd(cmd::USER_GUIDE, "View User Guide"),
        ],
    },
];

/// Original accelerator table (Ctrl+letter → command).
pub fn accelerator(c: char) -> Option<u16> {
    Some(match c.to_ascii_uppercase() {
        'C' => cmd::COPY,
        'Q' => cmd::EXIT,
        'S' => cmd::SAVE_AS,
        'V' => cmd::PASTE,
        'X' => cmd::CUT,
        'Z' => cmd::UNDO_REDO,
        _ => return None,
    })
}

/// Live state that changes item labels / availability.
#[derive(Clone, Copy, Default)]
pub struct MenuState {
    pub tool_sounds: bool,
    pub text_to_speech: bool,
    pub attached_sound: bool,
    pub manual_advance: bool,
    pub small_kids: bool,
    pub slideshow_mode: bool,
}

impl MenuState {
    pub fn label(&self, id: u16, base: &'static str) -> String {
        let onoff = |on: bool, what: &str| format!("Turn {what} {}", if on { "Off" } else { "On" });
        match id {
            cmd::TOOL_SOUNDS => onoff(self.tool_sounds, "Tool Sounds"),
            cmd::TEXT_TO_SPEECH => onoff(self.text_to_speech, "Text to Speech"),
            cmd::ATTACHED_SOUND => onoff(self.attached_sound, "Attached Sound"),
            cmd::MANUAL_ADVANCE => onoff(self.manual_advance, "Manual Advance"),
            cmd::SMALL_KIDS => onoff(self.small_kids, "Small Kids Mode"),
            cmd::PROJECT_MODE => {
                if self.slideshow_mode {
                    "Go to Paint Zone".into()
                } else {
                    "Go to SlideShow".into()
                }
            }
            _ => base.to_string(),
        }
    }

    pub fn enabled(&self, id: u16) -> bool {
        match id {
            // The Kid Pix email web service is long gone.
            cmd::EMAIL_SETUP | cmd::EMAIL_SEND => false,
            cmd::LOAD_PICTURE | cmd::MANUAL_ADVANCE | cmd::CLEAN_UP | cmd::CHANGE_EFFECTS | cmd::PICK_BG_COLOR => {
                self.slideshow_mode
            }
            cmd::PRINT => !self.small_kids,
            _ => true,
        }
    }
}

pub struct MenuBar {
    pub shown: bool,
    pub open: Option<usize>,
    hover_item: Option<usize>,
    dragging: bool,
    title_x: Vec<(i32, i32)>,
    face: usize,
}

pub const BAR_H: i32 = 19;
const SMALL_KIDS_MENU: usize = 6;
const ITEM_H: i32 = 18;
const SEP_H: i32 = 8;
const FONT: f32 = 13.0;
const GREY: u32 = 0xffc0_c0c0;
const NAVY: u32 = 0xff00_0080;

impl MenuBar {
    pub fn new(fonts: &Fonts, face: usize) -> MenuBar {
        let mut m = MenuBar { shown: false, open: None, hover_item: None, dragging: false, title_x: Vec::new(), face };
        m.layout(fonts, false);
        m
    }

    /// In Small Kids Mode only the Small Kids Mode menu is shown.
    pub fn layout(&mut self, fonts: &Fonts, small_kids: bool) {
        let mut x = 6;
        self.title_x.clear();
        for (i, m) in MENUS.iter().enumerate() {
            let visible = !small_kids || i == SMALL_KIDS_MENU;
            let w = if visible { fonts.measure(self.face, FONT, m.title) as i32 + 14 } else { 0 };
            self.title_x.push((x, w));
            x += w;
        }
    }

    /// Pointer is in the hot strip at the top of the screen.
    pub fn hot(&self, y: i32) -> bool {
        y < 3 || (self.shown && y < BAR_H)
    }

    fn title_at(&self, x: i32, y: i32) -> Option<usize> {
        if y >= BAR_H {
            return None;
        }
        self.title_x.iter().position(|&(tx, w)| w > 0 && x >= tx && x < tx + w)
    }

    fn menu_rect(&self, fonts: &Fonts, m: usize, st: &MenuState) -> (i32, i32, i32, i32) {
        let (x, _) = self.title_x[m];
        let mut w = 120;
        let mut h = 4;
        for it in MENUS[m].items {
            match it {
                Cmd(id, l) => {
                    let label = st.label(*id, l);
                    let (a, b) = label.split_once('\t').unwrap_or((&label, ""));
                    w = w.max(fonts.measure(self.face, FONT, a) as i32 + fonts.measure(self.face, FONT, b) as i32 + 60);
                    h += ITEM_H;
                }
                Sep => h += SEP_H,
            }
        }
        (x, BAR_H, w, h)
    }

    fn item_at(&self, fonts: &Fonts, m: usize, st: &MenuState, x: i32, y: i32) -> Option<usize> {
        let (mx, my, mw, _) = self.menu_rect(fonts, m, st);
        if x < mx || x >= mx + mw {
            return None;
        }
        let mut yy = my + 2;
        for (i, it) in MENUS[m].items.iter().enumerate() {
            let h = if matches!(it, Sep) { SEP_H } else { ITEM_H };
            if y >= yy && y < yy + h {
                return matches!(it, Cmd(..)).then_some(i);
            }
            yy += h;
        }
        None
    }

    /// Returns true if the bar consumed the press.
    pub fn press(&mut self, x: i32, y: i32) -> bool {
        if !self.shown {
            if y < 3 || y < BAR_H && x < 2 {
                self.shown = true;
                self.dragging = true;
                self.open = self.title_at(x.max(8), 2);
                return true;
            }
            return false;
        }
        if let Some(t) = self.title_at(x, y) {
            self.open = if self.open == Some(t) { None } else { Some(t) };
            self.dragging = true;
            return true;
        }
        if self.open.is_some() {
            // Click on an item is handled on release; elsewhere closes.
            self.dragging = true;
            return true;
        }
        self.shown = false;
        false
    }

    pub fn motion(&mut self, fonts: &Fonts, st: &MenuState, x: i32, y: i32) {
        if !self.shown {
            return;
        }
        if let Some(t) = self.title_at(x, y) {
            if self.open.is_some() || self.dragging {
                self.open = Some(t);
            }
        }
        self.hover_item = self.open.and_then(|m| self.item_at(fonts, m, st, x, y));
    }

    /// Release: returns a chosen command.
    pub fn release(&mut self, fonts: &Fonts, st: &MenuState, x: i32, y: i32) -> Option<u16> {
        let was_drag = self.dragging;
        self.dragging = false;
        let m = self.open?;
        if let Some(i) = self.item_at(fonts, m, st, x, y) {
            if let Cmd(id, _) = MENUS[m].items[i] {
                self.close();
                return st.enabled(id).then_some(id);
            }
        }
        if self.title_at(x, y).is_some() {
            return None; // stays open for click-style use
        }
        if was_drag {
            self.close();
        }
        None
    }

    pub fn close(&mut self) {
        self.shown = false;
        self.open = None;
        self.hover_item = None;
        self.dragging = false;
    }

    pub fn draw(&self, frame: &mut Image, fonts: &Fonts, st: &MenuState) {
        if !self.shown || fonts.is_empty() {
            return;
        }
        gfx::fill_rect(frame, 0, 0, frame.width as i32, BAR_H, GREY);
        gfx::fill_rect(frame, 0, BAR_H - 1, frame.width as i32, 1, 0xff80_8080);
        for (i, m) in MENUS.iter().enumerate() {
            let (x, w) = self.title_x[i];
            if w == 0 {
                continue;
            }
            let open = self.open == Some(i);
            if open {
                gfx::fill_rect(frame, x, 1, w, BAR_H - 2, NAVY);
            }
            let c = if open { 0xffff_ffff } else { 0xff00_0000 };
            fonts.draw(frame, self.face, FONT, (x + 7) as f32, 14.0, c, m.title);
        }
        let Some(m) = self.open else { return };
        let (x, y, w, h) = self.menu_rect(fonts, m, st);
        // Classic 3-D menu frame.
        gfx::fill_rect(frame, x, y, w, h, GREY);
        gfx::fill_rect(frame, x, y, w, 1, 0xffdf_dfdf);
        gfx::fill_rect(frame, x, y, 1, h, 0xffdf_dfdf);
        gfx::fill_rect(frame, x + 1, y + 1, w - 2, 1, 0xffff_ffff);
        gfx::fill_rect(frame, x + 1, y + 1, 1, h - 2, 0xffff_ffff);
        gfx::fill_rect(frame, x, y + h - 1, w, 1, 0xff00_0000);
        gfx::fill_rect(frame, x + w - 1, y, 1, h, 0xff00_0000);
        gfx::fill_rect(frame, x + 1, y + h - 2, w - 2, 1, 0xff80_8080);
        gfx::fill_rect(frame, x + w - 2, y + 1, 1, h - 2, 0xff80_8080);
        let mut yy = y + 2;
        for (i, it) in MENUS[m].items.iter().enumerate() {
            match it {
                Sep => {
                    gfx::fill_rect(frame, x + 3, yy + 3, w - 6, 1, 0xff80_8080);
                    gfx::fill_rect(frame, x + 3, yy + 4, w - 6, 1, 0xffff_ffff);
                    yy += SEP_H;
                }
                Cmd(id, l) => {
                    let enabled = st.enabled(*id);
                    let hot = self.hover_item == Some(i);
                    if hot {
                        gfx::fill_rect(frame, x + 3, yy, w - 6, ITEM_H, NAVY);
                    }
                    let label = st.label(*id, l);
                    let (a, b) = label.split_once('\t').unwrap_or((&label, ""));
                    let col = if !enabled {
                        0xff80_8080
                    } else if hot {
                        0xffff_ffff
                    } else {
                        0xff00_0000
                    };
                    if !enabled && !hot {
                        fonts.draw(frame, self.face, FONT, (x + 21) as f32, (yy + 14) as f32, 0xffff_ffff, a);
                    }
                    fonts.draw(frame, self.face, FONT, (x + 20) as f32, (yy + 13) as f32, col, a);
                    if !b.is_empty() {
                        let bw = fonts.measure(self.face, FONT, b);
                        fonts.draw(frame, self.face, FONT, (x + w) as f32 - bw - 14.0, (yy + 13) as f32, col, b);
                    }
                    yy += ITEM_H;
                }
            }
        }
    }
}

/// The white-outlined down triangle shown when the pointer reaches the top edge.
pub fn drop_cursor() -> Image {
    let mut img = Image::new(15, 9);
    for y in 0..8 {
        for x in y..15 - y {
            let edge = y == 0 || x == y || x == 14 - y;
            img.put(x, y, if edge { 0xffff_ffff } else { 0xff00_0000 });
        }
    }
    img.put(7, 8, 0xffff_ffff);
    img
}
