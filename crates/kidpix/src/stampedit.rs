//! The Rubber Stamp editor (`Data/StampEd.MHK`, `RStampEditor`, docs/RE_PANELS.md §3.5):
//! a 32×32 stamp shown 8× in the edit grid, an actual-size preview, the colour palette and
//! the pencil / paint can / dropper / clear / flip / undo tools; RESTORE ORIGINAL,
//! CANCEL and OK (saves when changed).

use crate::assets::Assets;
use crate::audio::Audio;
use crate::dialogs::Dialog;
use crate::gfx::{self, Image};
use crate::ui::Fonts;

const ARCH: &str = "Data/StampEd.MHK";
/// Edit grid: 32 cells of 8 px inside script 14001 (320,124)–(577,381).
const GRID: (i32, i32) = (321, 125);
const CELL: i32 = 8;
/// Actual-size preview inside script 14002 (611,124)–(646,159).
const PREVIEW: (i32, i32) = (613, 126);
/// Palette hit map `tBMP 14000` (117×165) sits at (191,257) over the palette art.
const PAL_AT: (i32, i32) = (191, 257);
/// Stamp transparency: white (RGB555 0x7FFF).
pub const CLEAR: u32 = 0xffff_ffff;

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Pencil,
    Fill,
    Dropper,
}

pub enum Outcome {
    Open,
    Cancel,
    /// OK with the edited stamp (only when it changed).
    Save(Image),
}

pub struct StampEditor {
    dlg: Dialog,
    pub stamp: Image,
    original: Image,
    undo: Option<Image>,
    mode: Mode,
    pub color: u32,
    palette: Option<std::rc::Rc<Image>>,
    painting: bool,
    last_cell: Option<(i32, i32)>,
}

impl StampEditor {
    pub fn new(assets: &Assets, stamp: &Image, color: u32) -> StampEditor {
        let mut s = Image::filled(32, 32, CLEAR);
        gfx::blit(&mut s, stamp, (32 - stamp.width as i32) / 2, (32 - stamp.height as i32) / 2);
        for p in s.pixels.iter_mut() {
            *p |= 0xff00_0000;
        }
        let dlg = Dialog::new(assets, ARCH, &[14000, 14001, 14002, 14003], &[14000]);
        let mut e = StampEditor {
            dlg,
            original: s.clone(),
            stamp: s,
            undo: None,
            mode: Mode::Pencil,
            color: color | 0xff00_0000,
            palette: assets.image(ARCH, 14000),
            painting: false,
            last_cell: None,
        };
        e.sync_modes();
        e
    }

    fn sync_modes(&mut self) {
        let sel = match self.mode {
            Mode::Pencil => 19000,
            Mode::Fill => 19001,
            Mode::Dropper => 19002,
        };
        for b in self.dlg.buttons.iter_mut() {
            if (19000..=19002).contains(&b.cmd) {
                b.show(if b.cmd == sel { 2 } else { 0 }, false);
            }
        }
    }

    fn cell_at(x: i32, y: i32) -> Option<(i32, i32)> {
        let (cx, cy) = ((x - GRID.0).div_euclid(CELL), (y - GRID.1).div_euclid(CELL));
        (x >= GRID.0 && y >= GRID.1 && cx < 32 && cy < 32).then_some((cx, cy))
    }

    fn palette_color(&self, x: i32, y: i32) -> Option<u32> {
        let p = self.palette.as_ref()?;
        let c = p.get(x - PAL_AT.0, y - PAL_AT.1)?;
        (c >> 24 != 0).then_some(c | 0xff00_0000)
    }

    fn checkpoint(&mut self) {
        self.undo = Some(self.stamp.clone());
    }

    fn paint_cell(&mut self, cx: i32, cy: i32) {
        match self.mode {
            Mode::Pencil => self.stamp.put(cx, cy, self.color),
            Mode::Dropper => {
                if let Some(c) = self.stamp.get(cx, cy) {
                    self.color = c;
                }
            }
            Mode::Fill => {
                let Some(target) = self.stamp.get(cx, cy) else { return };
                if target == self.color {
                    return;
                }
                let mut stack = vec![(cx, cy)];
                while let Some((x, y)) = stack.pop() {
                    if self.stamp.get(x, y) != Some(target) {
                        continue;
                    }
                    self.stamp.put(x, y, self.color);
                    stack.extend([(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)]);
                }
            }
        }
    }

    pub fn mouse_move(&mut self, x: i32, y: i32, assets: &Assets, audio: &Audio) {
        self.dlg.mouse_move(x, y, assets, audio);
        if self.painting && self.mode == Mode::Pencil {
            if let Some((cx, cy)) = Self::cell_at(x, y) {
                // One cell at a time along the drag.
                let (mut px, mut py) = self.last_cell.unwrap_or((cx, cy));
                let n = (cx - px).abs().max((cy - py).abs()).max(1);
                let (sx, sy) = (px, py);
                for k in 1..=n {
                    px = sx + (cx - sx) * k / n;
                    py = sy + (cy - sy) * k / n;
                    self.paint_cell(px, py);
                }
                self.last_cell = Some((cx, cy));
            }
        }
        self.sync_modes();
    }

    pub fn mouse_down(&mut self, x: i32, y: i32, assets: &Assets, audio: &Audio) {
        if let Some((cx, cy)) = Self::cell_at(x, y) {
            if self.mode != Mode::Dropper {
                self.checkpoint();
            }
            self.paint_cell(cx, cy);
            self.painting = true;
            self.last_cell = Some((cx, cy));
            return;
        }
        if let Some(c) = self.palette_color(x, y) {
            self.color = c;
            return;
        }
        self.dlg.mouse_down(x, y, assets, audio);
    }

    pub fn mouse_up(&mut self, x: i32, y: i32) -> Outcome {
        self.painting = false;
        self.last_cell = None;
        let Some(cmd) = self.dlg.mouse_up(x, y) else {
            self.sync_modes();
            return Outcome::Open;
        };
        match cmd {
            19000 => self.mode = Mode::Pencil,
            19001 => self.mode = Mode::Fill,
            19002 => self.mode = Mode::Dropper,
            19003 => {
                self.checkpoint();
                self.stamp = Image::filled(32, 32, CLEAR);
            }
            19004 => {
                self.checkpoint();
                let src = self.stamp.clone();
                for y in 0..32 {
                    for x in 0..32 {
                        self.stamp.put(x, y, src.get(31 - x, y).unwrap_or(CLEAR));
                    }
                }
            }
            19005 => {
                if let Some(u) = self.undo.take() {
                    self.undo = Some(std::mem::replace(&mut self.stamp, u));
                }
            }
            19006 => {
                self.checkpoint();
                self.stamp = self.original.clone();
            }
            19007 => return Outcome::Cancel,
            19008 => {
                return if self.stamp.pixels != self.original.pixels { Outcome::Save(self.stamp.clone()) } else { Outcome::Cancel };
            }
            _ => {}
        }
        self.sync_modes();
        Outcome::Open
    }

    pub fn update(&mut self, dt: f32, assets: &Assets, audio: &Audio) {
        self.dlg.update(dt, assets, audio);
    }

    pub fn draw(&self, frame: &mut Image, fonts: &Fonts) {
        self.dlg.draw(frame, fonts);
        // The grid shows the stamp as plain 8×8 blocks (no grid lines), as in the original.
        for y in 0..32 {
            for x in 0..32 {
                let c = self.stamp.get(x, y).unwrap_or(CLEAR);
                gfx::fill_rect(frame, GRID.0 + x * CELL, GRID.1 + y * CELL, CELL, CELL, c);
            }
        }
        // Current colour in the bar above the palette (script 14031's frame).
        gfx::fill_rect(frame, 223, 256, 53, 6, self.color);
        gfx::fill_rect(frame, PREVIEW.0, PREVIEW.1, 32, 32, CLEAR);
        gfx::blit(frame, &self.stamp, PREVIEW.0, PREVIEW.1);
    }
}

/// Where an edited stamp is kept: `<user>/My Stamps/<library>/<id>.png`.
pub fn override_path(user_dir: &std::path::Path, stamps_dir: &str, library: &str, id: u16) -> std::path::PathBuf {
    user_dir.join(stamps_dir).join(library).join(format!("{id}.png"))
}

pub fn load_png(path: &std::path::Path) -> Option<Image> {
    let dec = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).ok()?));
    let mut r = dec.read_info().ok()?;
    let mut buf = vec![0; r.output_buffer_size()];
    let info = r.next_frame(&mut buf).ok()?;
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return None;
    }
    Some(Image::from_rgba8(info.width, info.height, &buf[..info.buffer_size()]))
}
