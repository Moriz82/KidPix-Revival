//! The original graphical dialogs (`RDlgBase` family, docs/RE_SHELL.md §5.2): a frame
//! script, message art, MBTN button tables with rest / rollover / press / disabled art
//! (`rest + 3`), and optional text field, wrapped body text or name list.

use crate::anim::Player;
use crate::assets::Assets;
use crate::audio::{self, Audio};
use crate::buttons::{self, UiButton};
use crate::gfx::{self, Image};
use crate::ui::Fonts;
use mohawk::Rect;

pub const CANCEL: u16 = 1395;
pub const OK: u16 = 1396;
pub const YES: u16 = 1702;
pub const NO: u16 = 1703;
pub const REPLAY: u16 = 1433;

/// Keys a dialog cares about.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Key {
    Char(char),
    Back,
    Delete,
    Left,
    Right,
    Enter,
    Escape,
    PageUp,
    PageDown,
}

pub struct Field {
    pub rect: Rect,
    pub text: String,
    pub caret: usize,
    pub max: usize,
    /// Allowed characters (None = any printable).
    pub legal: Option<String>,
    pub color: u32,
    pub size: f32,
    /// Sound for an accepted / rejected key: (archive, tWAV id).
    pub accept: Option<(String, u16)>,
    pub reject: Option<(String, u16)>,
    /// Placeholder text ("Your Name"): the first typed key replaces it.
    pub placeholder: bool,
    blink: f32,
}

impl Field {
    pub fn new(rect: Rect, text: &str) -> Field {
        Field {
            rect,
            text: text.to_string(),
            caret: text.chars().count(),
            max: 0xf5,
            legal: None,
            color: gfx::rgb(0, 0, 0),
            size: 14.0,
            accept: None,
            reject: None,
            placeholder: false,
            blink: 0.0,
        }
    }

    fn byte_at(&self, c: usize) -> usize {
        self.text.char_indices().nth(c).map(|(i, _)| i).unwrap_or(self.text.len())
    }

    /// Returns Some(accepted) for keys the field handles.
    fn key(&mut self, k: Key) -> Option<bool> {
        if self.placeholder && matches!(k, Key::Char(_) | Key::Back | Key::Delete) {
            self.placeholder = false;
            self.text.clear();
            self.caret = 0;
            if !matches!(k, Key::Char(_)) {
                return Some(true);
            }
        }
        let n = self.text.chars().count();
        match k {
            Key::Char(c) => {
                let ok = !c.is_control()
                    && n < self.max
                    && self.legal.as_ref().is_none_or(|l| l.contains(c));
                if ok {
                    let at = self.byte_at(self.caret);
                    self.text.insert(at, c);
                    self.caret += 1;
                }
                Some(ok)
            }
            Key::Back => {
                if self.caret > 0 {
                    self.caret -= 1;
                    let at = self.byte_at(self.caret);
                    self.text.remove(at);
                    Some(true)
                } else {
                    Some(false)
                }
            }
            Key::Delete => {
                if self.caret < n {
                    let at = self.byte_at(self.caret);
                    self.text.remove(at);
                    Some(true)
                } else {
                    Some(false)
                }
            }
            Key::Left => {
                self.caret = self.caret.saturating_sub(1);
                Some(true)
            }
            Key::Right => {
                self.caret = (self.caret + 1).min(n);
                Some(true)
            }
            _ => None,
        }
    }

    pub fn set(&mut self, s: &str) {
        self.placeholder = false;
        self.text = s.to_string();
        self.caret = s.chars().count();
    }
}

/// The login dialog's name list (8 rows, scroll arrows and thumb).
pub struct List {
    pub items: Vec<String>,
    pub top: usize,
    pub selected: Vec<usize>,
    pub multi: bool,
    rows: Vec<Rect>,
    normal: Vec<Player>,
    highlight: Vec<Player>,
    up: [Option<Player>; 4],
    down: [Option<Player>; 4],
    track: Option<Player>,
    thumb: Option<Player>,
    up_state: u8,
    down_state: u8,
    pub size: f32,
    last_click: Option<(usize, f32)>,
    clock: f32,
}

impl List {
    pub fn login(assets: &Assets, arch: &str) -> List {
        let p = |id| held(assets, arch, id);
        let highlight: Vec<Player> = (0..8).filter_map(|i| p(7644 + i)).collect();
        let normal: Vec<Player> = (0..8).filter_map(|i| p(7652 + i)).collect();
        let rows = (0..8)
            .map(|i| p(7652 + i).map(|q| q.bounds()).unwrap_or(Rect { left: 233, top: 206 + 25 * i as i16, right: 442, bottom: 230 + 25 * i as i16 }))
            .collect();
        List {
            items: Vec::new(),
            top: 0,
            selected: Vec::new(),
            multi: false,
            rows,
            normal,
            highlight,
            up: [p(7606), p(7607), p(7608), p(7613)],
            down: [p(7609), p(7610), p(7611), p(7612)],
            track: p(7605),
            thumb: p(7614),
            up_state: 0,
            down_state: 0,
            size: 14.0,
            last_click: None,
            clock: 0.0,
        }
    }

    fn page(&self) -> usize {
        self.rows.len().max(1)
    }

    fn max_top(&self) -> usize {
        self.items.len().saturating_sub(self.page())
    }

    pub fn scroll(&mut self, d: i32) {
        self.top = (self.top as i32 + d).clamp(0, self.max_top() as i32) as usize;
    }

    fn arrow_rects(&self) -> (Option<Rect>, Option<Rect>) {
        (self.up[0].as_ref().map(|p| p.bounds()), self.down[0].as_ref().map(|p| p.bounds()))
    }
}

pub enum ListEvent {
    Selected(usize),
    DoubleClick(usize),
}

fn held(assets: &Assets, arch: &str, id: u16) -> Option<Player> {
    Player::new(assets, arch, id).map(|mut p| {
        p.finish_now();
        p
    })
}

pub struct Dialog {
    pub archive: String,
    layers: Vec<Player>,
    pub buttons: Vec<UiButton>,
    disabled_art: Vec<Option<Player>>,
    pub disabled: Vec<bool>,
    pub hidden: Vec<bool>,
    hover: Option<usize>,
    pressed: Option<usize>,
    pub field: Option<Field>,
    /// A second, unfocused text field (focus swaps it with `field`).
    pub field2: Option<Field>,
    /// `field` / `field2` are swapped relative to their creation order.
    pub fields_swapped: bool,
    /// Text drawn by code, wrapped in a rect: (rect, text, size).
    pub body: Option<(Rect, String, f32)>,
    pub body_color: u32,
    pub list: Option<List>,
    pub sounds: bool,
    /// Buttons drawn in their "on" state (the press art held): radio buttons, check boxes,
    /// swatches.
    pub selected: Vec<bool>,
    selected_art: Vec<Option<Player>>,
    /// Index of the button the last `mouse_up` clicked (several hotspots share a command).
    pub last_index: Option<usize>,
}

impl Dialog {
    /// `layers` are drawn in order (frame first, then message art); `mbtns` are loaded in order.
    pub fn new(assets: &Assets, archive: &str, layers: &[u16], mbtns: &[u16]) -> Dialog {
        let layers = layers.iter().filter_map(|&id| held(assets, archive, id)).collect();
        let mut buttons = Vec::new();
        for &m in mbtns {
            buttons.extend(buttons::load(assets, archive, m));
        }
        let disabled_art = buttons
            .iter()
            .map(|b| b.idle.as_ref().and_then(|p| held(assets, archive, p.id + 3)))
            .collect();
        // Hotspot records (MBTN kind 5) have no rest art: their rect is the press script's.
        for b in buttons.iter_mut() {
            if b.rect == Rect::default() {
                if let Some(p) = b.click.as_ref() {
                    b.rect = p.bounds();
                }
            }
        }
        // "On" art: MBTN field 2 (kind 1) or field 3 (kind 2) — the checked box / lit
        // button; hotspots without one show their press art.
        let on_ids: Vec<u16> = mbtns
            .iter()
            .flat_map(|&m| {
                assets
                    .archive(archive)
                    .and_then(|a| a.get(crate::assets::MBTN, m).map(mohawk::parse_buttons))
                    .and_then(|r| r.ok())
                    .unwrap_or_default()
                    .into_iter()
                    .map(|b| if b.raw[2] != 0 { b.raw[2] } else { b.raw[3] })
            })
            .collect();
        let selected_art = buttons
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let id = on_ids.get(i).copied().filter(|&v| v != 0).or_else(|| b.click.as_ref().map(|p| p.id))?;
                held(assets, archive, id)
            })
            .collect();
        let n = buttons.len();
        Dialog {
            archive: archive.to_string(),
            layers,
            buttons,
            disabled_art,
            disabled: vec![false; n],
            hidden: vec![false; n],
            hover: None,
            pressed: None,
            field: None,
            field2: None,
            fields_swapped: false,
            body: None,
            body_color: gfx::rgb(255, 255, 255),
            list: None,
            sounds: true,
            selected: vec![false; n],
            selected_art,
            last_index: None,
        }
    }

    /// Show the button(s) with `cmd` in their "on" state.
    pub fn set_selected(&mut self, cmd: u16, on: bool) {
        for (i, b) in self.buttons.iter().enumerate() {
            if b.cmd == cmd {
                self.selected[i] = on;
            }
        }
    }

    /// Select exactly one button of a radio group (by button index).
    pub fn select_only(&mut self, group: impl Fn(&UiButton) -> bool, index: usize) {
        for (i, b) in self.buttons.iter().enumerate() {
            if group(b) {
                self.selected[i] = i == index;
            }
        }
    }

    pub fn index_of(&self, cmd: u16) -> Option<usize> {
        self.buttons.iter().position(|b| b.cmd == cmd)
    }

    pub fn set_hidden(&mut self, cmd: u16, hidden: bool) {
        for (i, b) in self.buttons.iter().enumerate() {
            if b.cmd == cmd {
                self.hidden[i] = hidden;
            }
        }
    }

    pub fn set_group_hidden(&mut self, group: u16, hidden: bool) {
        for (i, b) in self.buttons.iter().enumerate() {
            if b.group == group {
                self.hidden[i] = hidden;
            }
        }
    }

    pub fn set_disabled(&mut self, cmd: u16, disabled: bool) {
        for (i, b) in self.buttons.iter().enumerate() {
            if b.cmd == cmd {
                self.disabled[i] = disabled;
            }
        }
    }

    fn live(&self, i: usize) -> bool {
        !self.hidden[i] && !self.disabled[i]
    }

    fn button_at(&self, x: i32, y: i32) -> Option<usize> {
        (0..self.buttons.len()).find(|&i| !self.hidden[i] && self.buttons[i].rect.contains(x, y))
    }

    fn play(&self, assets: &Assets, audio: &Audio, arch: &str, id: u16) {
        if self.sounds {
            if let Some(s) = assets.sound(arch, id) {
                audio.play(s, audio::CH_UI, false);
            }
        }
    }

    fn play_script_sounds(b: &mut UiButton, assets: &Assets, audio: &Audio, arch: &str, enabled: bool) {
        let base = b.sound_base();
        if let Some(p) = b.player() {
            for k in p.update(0.0) {
                if enabled {
                    if let Some(s) = assets.sound(arch, base + k) {
                        audio.play(s, audio::CH_UI, false);
                    }
                }
            }
        }
    }

    pub fn mouse_move(&mut self, x: i32, y: i32, assets: &Assets, audio: &Audio) {
        let h = self.button_at(x, y).filter(|&i| self.live(i));
        if let Some(l) = self.list.as_mut() {
            let (u, d) = l.arrow_rects();
            let can_up = l.top > 0;
            let can_down = l.top < l.max_top();
            let nu = if !can_up { 3 } else if u.is_some_and(|r| r.contains(x, y)) { 1 } else { 0 };
            let nd = if !can_down { 3 } else if d.is_some_and(|r| r.contains(x, y)) { 1 } else { 0 };
            if nu == 1 && l.up_state != 1 {
                if let Some(p) = l.up[1].as_mut() {
                    let base = p.script.bitmap_set;
                    p.restart();
                    for k in p.update(0.0) {
                        if self.sounds {
                            if let Some(s) = assets.sound(&self.archive, base + k) {
                                audio.play(s, audio::CH_UI, false);
                            }
                        }
                    }
                }
            }
            if nd == 1 && l.down_state != 1 {
                if let Some(p) = l.down[1].as_mut() {
                    let base = p.script.bitmap_set;
                    p.restart();
                    for k in p.update(0.0) {
                        if self.sounds {
                            if let Some(s) = assets.sound(&self.archive, base + k) {
                                audio.play(s, audio::CH_UI, false);
                            }
                        }
                    }
                }
            }
            l.up_state = nu;
            l.down_state = nd;
        }
        if self.pressed.is_some() {
            return;
        }
        if h == self.hover {
            return;
        }
        if let Some(i) = self.hover {
            self.buttons[i].show(0, false);
        }
        if let Some(i) = h {
            self.buttons[i].show(1, true);
            let arch = self.archive.clone();
            Self::play_script_sounds(&mut self.buttons[i], assets, audio, &arch, self.sounds);
        }
        self.hover = h;
    }

    /// Mouse press. Returns a list event when a name row was clicked.
    pub fn mouse_down(&mut self, x: i32, y: i32, assets: &Assets, audio: &Audio) -> Option<ListEvent> {
        if let Some(i) = self.button_at(x, y).filter(|&i| self.live(i)) {
            self.pressed = Some(i);
            self.buttons[i].show(2, true);
            let arch = self.archive.clone();
            Self::play_script_sounds(&mut self.buttons[i], assets, audio, &arch, self.sounds);
            return None;
        }
        if self.field2.as_ref().is_some_and(|f| f.rect.contains(x, y)) {
            std::mem::swap(&mut self.field, &mut self.field2);
            self.fields_swapped = !self.fields_swapped;
        }
        if let Some(f) = self.field.as_mut() {
            if f.rect.contains(x, y) {
                f.caret = f.text.chars().count();
            }
        }
        let l = self.list.as_mut()?;
        let (u, d) = l.arrow_rects();
        if u.is_some_and(|r| r.contains(x, y)) {
            l.scroll(-1);
            return None;
        }
        if d.is_some_and(|r| r.contains(x, y)) {
            l.scroll(1);
            return None;
        }
        if let Some(t) = l.track.as_ref().map(|p| p.bounds()).filter(|r| r.contains(x, y)) {
            let page = l.page() as i32;
            let thumb_y = thumb_top(l, t);
            l.scroll(if y < thumb_y { -page } else { page });
            return None;
        }
        let row = l.rows.iter().position(|r| r.contains(x, y))?;
        let idx = l.top + row;
        if idx >= l.items.len() {
            return None;
        }
        let dbl = l.last_click.is_some_and(|(i, t)| i == idx && l.clock - t < 450.0);
        l.last_click = Some((idx, l.clock));
        if l.multi {
            if let Some(p) = l.selected.iter().position(|&s| s == idx) {
                l.selected.remove(p);
            } else {
                l.selected.push(idx);
            }
        } else {
            l.selected = vec![idx];
        }
        Some(if dbl { ListEvent::DoubleClick(idx) } else { ListEvent::Selected(idx) })
    }

    /// Mouse release: returns the command of a button clicked through.
    pub fn mouse_up(&mut self, x: i32, y: i32) -> Option<u16> {
        let i = self.pressed.take()?;
        let inside = self.buttons[i].rect.contains(x, y);
        self.buttons[i].show(if inside { 1 } else { 0 }, false);
        self.hover = inside.then_some(i);
        let hit = inside && self.live(i);
        self.last_index = hit.then_some(i);
        hit.then(|| self.buttons[i].cmd)
    }

    /// A key: returns a command for Enter / Escape when the dialog maps them.
    pub fn key(&mut self, k: Key, assets: &Assets, audio: &Audio) -> Option<u16> {
        if let Some(l) = self.list.as_mut() {
            match k {
                Key::PageUp => {
                    let p = l.page() as i32;
                    l.scroll(-p);
                    return None;
                }
                Key::PageDown => {
                    let p = l.page() as i32;
                    l.scroll(p);
                    return None;
                }
                _ => {}
            }
        }
        if let Some(f) = self.field.as_mut() {
            if let Some(ok) = f.key(k) {
                f.blink = 0.0;
                let snd = if ok { f.accept.clone() } else { f.reject.clone() };
                if let Some((a, id)) = snd {
                    self.play(assets, audio, &a, id);
                }
                return None;
            }
        }
        match k {
            Key::Enter => [OK, YES, 1500, 1604].into_iter().find(|&c| self.index_of(c).is_some_and(|i| self.live(i))),
            Key::Escape => [CANCEL, NO].into_iter().find(|&c| self.index_of(c).is_some_and(|i| self.live(i))),
            _ => None,
        }
    }

    pub fn update(&mut self, dt: f32, assets: &Assets, audio: &Audio) {
        let arch = self.archive.clone();
        for b in self.buttons.iter_mut() {
            let base = b.sound_base();
            if let Some(p) = b.player() {
                for k in p.update(dt) {
                    if self.sounds {
                        if let Some(s) = assets.sound(&arch, base + k) {
                            audio.play(s, audio::CH_UI, false);
                        }
                    }
                }
            }
        }
        if let Some(f) = self.field.as_mut() {
            f.blink += dt;
        }
        if let Some(l) = self.list.as_mut() {
            l.clock += dt;
        }
    }

    pub fn draw(&self, frame: &mut Image, fonts: &Fonts) {
        for p in &self.layers {
            p.draw(frame);
        }
        for (i, b) in self.buttons.iter().enumerate() {
            if self.hidden[i] {
                continue;
            }
            if self.disabled[i] {
                if let Some(p) = &self.disabled_art[i] {
                    p.draw(frame);
                    continue;
                }
            }
            if self.selected[i] && b.showing != 2 {
                if let Some(p) = &self.selected_art[i] {
                    p.draw(frame);
                    continue;
                }
            }
            if let Some(p) = b.player_ref() {
                p.draw(frame);
            }
        }
        let face = fonts.ui_face();
        if let Some(l) = &self.list {
            if let Some(t) = &l.track {
                t.draw(frame);
            }
            let st = |s: u8| if s == 3 { 3 } else { s as usize };
            if let Some(p) = l.up[st(l.up_state)].as_ref().or(l.up[0].as_ref()) {
                p.draw(frame);
            }
            if let Some(p) = l.down[st(l.down_state)].as_ref().or(l.down[0].as_ref()) {
                p.draw(frame);
            }
            if let (Some(th), Some(t)) = (&l.thumb, l.track.as_ref().map(|p| p.bounds())) {
                let y = thumb_top(l, t);
                let r = th.bounds();
                let mut c = Image::new(frame.width, frame.height);
                let _ = &mut c;
                // Draw the thumb shifted to its position.
                for (cel, x, yy) in th.placed() {
                    if let Some(img) = th.sprites.cel(cel) {
                        gfx::blit(frame, img, x, yy + (y - r.top as i32));
                    }
                }
            }
            if !fonts.is_empty() {
                for (k, r) in l.rows.iter().enumerate() {
                    let idx = l.top + k;
                    let Some(name) = l.items.get(idx) else { break };
                    let art = if l.selected.contains(&idx) { l.highlight.get(k) } else { l.normal.get(k) };
                    if let Some(p) = art {
                        p.draw(frame);
                    }
                    let base = r.bottom as f32 - (r.bottom - r.top) as f32 * 0.28;
                    fonts.draw(frame, face, l.size, r.left as f32 + 6.0, base, gfx::rgb(0, 0, 0), name);
                }
            }
        }
        if fonts.is_empty() {
            return;
        }
        if let Some(f) = &self.field2 {
            let r = f.rect;
            let base = r.bottom as f32 - (r.bottom - r.top) as f32 * 0.28;
            fonts.draw(frame, face, f.size, r.left as f32 + 5.0, base, f.color, &f.text);
        }
        if let Some(f) = &self.field {
            let r = f.rect;
            let base = r.bottom as f32 - (r.bottom - r.top) as f32 * 0.28;
            let x0 = r.left as f32 + 5.0;
            let w = fonts.draw(frame, face, f.size, x0, base, f.color, &f.text);
            let _ = w;
            if (f.blink / 530.0) as i32 % 2 == 0 {
                let pre: String = f.text.chars().take(f.caret).collect();
                let cx = x0 + fonts.measure(face, f.size, &pre);
                gfx::fill_rect(frame, cx as i32, r.top as i32 + 5, 1, (r.bottom - r.top) as i32 - 10, f.color);
            }
        }
        if let Some((r, text, size)) = &self.body {
            draw_wrapped(frame, fonts, face, *size, *r, text, self.body_color);
        }
    }
}

fn thumb_top(l: &List, track: Rect) -> i32 {
    let th = l.thumb.as_ref().map(|p| p.bounds().bottom - p.bounds().top).unwrap_or(17) as i32;
    let span = (track.bottom - track.top) as i32 - th;
    let m = l.max_top();
    track.top as i32 + if m == 0 { 0 } else { span * l.top as i32 / m as i32 }
}

/// Word-wrap `text` into `r`, centred lines, shrinking the size until it fits.
pub fn draw_wrapped(frame: &mut Image, fonts: &Fonts, face: usize, size: f32, r: Rect, text: &str, color: u32) {
    let w = (r.right - r.left) as f32;
    let h = (r.bottom - r.top) as f32;
    let mut s = size;
    loop {
        let lines = wrap(fonts, face, s, text, w);
        let lh = s * 1.3;
        if lines.len() as f32 * lh <= h || s <= 9.0 {
            let total = lines.len() as f32 * lh;
            let mut y = r.top as f32 + ((h - total) / 2.0).max(0.0) + s;
            for line in lines {
                let lw = fonts.measure(face, s, &line);
                fonts.draw(frame, face, s, r.left as f32 + (w - lw) / 2.0, y, color, &line);
                y += lh;
            }
            return;
        }
        s -= 1.0;
    }
}

fn wrap(fonts: &Fonts, face: usize, size: f32, text: &str, w: f32) -> Vec<String> {
    let mut out = Vec::new();
    for para in text.split('\n') {
        let mut line = String::new();
        for word in para.split_whitespace() {
            let cand = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
            if fonts.measure(face, size, &cand) > w && !line.is_empty() {
                out.push(std::mem::take(&mut line));
                line = word.to_string();
            } else {
                line = cand;
            }
        }
        out.push(line);
    }
    out
}

impl Dialog {
    /// The two text fields in creation order.
    pub fn fields_in_order(&self) -> (Option<&Field>, Option<&Field>) {
        if self.fields_swapped {
            (self.field2.as_ref(), self.field.as_ref())
        } else {
            (self.field.as_ref(), self.field2.as_ref())
        }
    }
}
