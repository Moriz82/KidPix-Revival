//! Content libraries (`Libraries/*/*.klb`): backgrounds, stickers, animations, sounds, stamps.

use crate::assets::{self, Assets};
use crate::gfx::{self, Image};
use mohawk::{Archive, Sound};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LibKind {
    Backgrounds,
    Stickers,
    Toons,
    Sounds,
    Stamps,
    /// Paper textures (`Data/Tools/TextureMaps.MHK`); item id 0 = plain paper.
    Paper,
}

impl LibKind {
    pub fn name(self) -> &'static str {
        match self {
            LibKind::Backgrounds => "Backgrounds",
            LibKind::Stickers => "Stickers",
            LibKind::Toons => "Animations",
            LibKind::Sounds => "Sounds",
            LibKind::Stamps => "Stamps",
            LibKind::Paper => "Paper Texture",
        }
    }
    fn dir(self) -> &'static str {
        match self {
            LibKind::Backgrounds => "Libraries/Backgrounds",
            LibKind::Stickers => "Libraries/Stickers",
            LibKind::Toons => "Libraries/Toons",
            LibKind::Sounds => "Libraries/Sounds",
            LibKind::Stamps => "Libraries/Stamps",
            LibKind::Paper => "Data/Tools",
        }
    }
}

#[derive(Clone)]
pub struct LibItem {
    pub id: u16,
    pub title: String,
    pub thumb: Option<Image>,
}

pub struct LibDrawer {
    pub kind: LibKind,
    pub libs: Vec<PathBuf>,
    pub lib: usize,
    archive: Option<Archive>,
    /// Files of a user folder library ("My Sounds"), indexed by item id.
    files: Vec<PathBuf>,
    pub items: Vec<LibItem>,
    pub page: usize,
}

impl LibDrawer {
    pub fn open(assets: &Assets, kind: LibKind) -> Option<LibDrawer> {
        if kind == LibKind::Paper {
            let a = assets.archive(PAPER)?;
            let mut items = vec![LibItem { id: 0, title: "Plain paper".into(), thumb: Some(plain_thumb()) }];
            for id in a.ids(assets::T_BMP) {
                let thumb = a.get(assets::T_BMP, id).and_then(|d| mohawk::decode_bitmap(d, None).ok()).map(|t| gfx::resize(&t, 48, 36));
                items.push(LibItem { id, title: String::new(), thumb });
            }
            return Some(LibDrawer { kind, libs: vec![PathBuf::from(PAPER)], lib: 0, archive: Some((*a).clone()), files: Vec::new(), items, page: 0 });
        }
        let libs = assets.libraries(kind.dir());
        if libs.is_empty() {
            return None;
        }
        let mut d = LibDrawer {
            kind,
            libs,
            lib: 0,
            archive: None,
            files: Vec::new(),
            items: Vec::new(),
            page: 0,
        };
        d.load();
        Some(d)
    }

    pub fn library_name(&self) -> String {
        if self.kind == LibKind::Paper {
            return String::new();
        }
        self.libs
            .get(self.lib)
            .and_then(|p| p.file_stem())
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default()
    }

    pub fn switch(&mut self, delta: i32) {
        if self.kind == LibKind::Paper {
            return;
        }
        let n = self.libs.len() as i32;
        self.lib = ((self.lib as i32 + delta).rem_euclid(n.max(1))) as usize;
        self.page = 0;
        self.load();
    }

    /// Put a user folder (e.g. the artist's "My Sounds") first in the folder list.
    pub fn add_user_folder(&mut self, dir: PathBuf) {
        if dir.is_dir() && !self.libs.contains(&dir) {
            self.libs.insert(0, dir);
            self.lib = 0;
            self.page = 0;
            self.load();
        }
    }

    fn load(&mut self) {
        self.items.clear();
        self.files.clear();
        if let Some(dir) = self.libs.get(self.lib).filter(|p| p.is_dir()).cloned() {
            self.archive = None;
            let want = |p: &PathBuf| p.extension().is_some_and(|e| self.kind == LibKind::Sounds && e.eq_ignore_ascii_case("wav"));
            let mut files: Vec<PathBuf> = std::fs::read_dir(&dir).map(|rd| rd.flatten().map(|e| e.path()).filter(|p| want(p)).collect()).unwrap_or_default();
            files.sort();
            for (i, f) in files.iter().enumerate() {
                let title = f.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                self.items.push(LibItem { id: i as u16, thumb: Some(name_thumb(&title)), title });
            }
            self.files = files;
            return;
        }
        self.archive = self.libs.get(self.lib).and_then(|p| match Archive::open(p) {
            Ok(a) => Some(a),
            Err(e) => {
                eprintln!("warning: {}: {e}", p.display());
                None
            }
        });
        let Some(a) = &self.archive else { return };
        if self.kind == LibKind::Stamps {
            for id in a.ids(assets::T_BMP) {
                self.items.push(LibItem {
                    id,
                    title: String::new(),
                    thumb: None,
                });
            }
        } else {
            for id in a.ids(assets::T_THM) {
                let title = a.get(assets::T_ATX, id).map(mohawk::latin1).unwrap_or_default();
                self.items.push(LibItem {
                    id,
                    title,
                    thumb: None,
                });
            }
        }
        self.ensure_thumbs(0, self.items.len());
    }

    pub fn ensure_thumbs(&mut self, start: usize, count: usize) {
        let kind = self.kind;
        if kind == LibKind::Paper {
            return;
        }
        let Some(a) = &self.archive else { return };
        for item in self.items.iter_mut().skip(start).take(count) {
            if item.thumb.is_some() {
                continue;
            }
            item.thumb = if kind == LibKind::Stamps {
                a.get(assets::T_BMP, item.id)
                    .and_then(decode_stamp)
                    .map(|i| {
                        let s = (48.0 / i.width.max(i.height) as f32).min(1.5);
                        gfx::resize(&i, (i.width as f32 * s) as u32, (i.height as f32 * s) as u32)
                    })
            } else {
                a.get(assets::T_THM, item.id)
                    .and_then(|d| mohawk::decode_bitmap(d, None).ok())
            };
        }
    }

    pub fn full_bitmap(&self, id: u16) -> Option<Image> {
        let a = self.archive.as_ref()?;
        mohawk::decode_bitmap(a.get(assets::T_BMP, id)?, None).ok()
    }

    pub fn sound(&self, id: u16) -> Option<Arc<Sound>> {
        if let Some(f) = self.files.get(id as usize) {
            return crate::formats::load_wav(f).ok().map(Arc::new);
        }
        let a = self.archive.as_ref()?;
        mohawk::decode_sound(a.get(assets::T_WAV, id)?).ok().map(Arc::new)
    }

    pub fn stamp_image(&self, id: u16) -> Option<Image> {
        decode_stamp(self.archive.as_ref()?.get(assets::T_BMP, id)?)
    }

    /// Frames for a sticker / animation. Uses the Flash renderer when available,
    /// otherwise the library thumbnail.
    pub fn movie_frames(&self, id: u16, animated: bool) -> (Vec<Image>, f32, Option<std::rc::Rc<[u8]>>) {
        let Some(a) = self.archive.as_ref() else {
            return (vec![], 12.0, None);
        };
        if let Some(swf) = a.get(assets::T_SWF, id) {
            if let Some((f, fps)) = crate::stickers::render(swf, animated, 1.0) {
                return (f, fps, Some(swf.into()));
            }
        }
        let thumb = a
            .get(assets::T_THM, id)
            .and_then(|d| mohawk::decode_bitmap(d, None).ok())
            .map(|t| knock_out_white(&gfx::resize(&t, t.width * 2, t.height * 2)));
        (thumb.into_iter().collect(), 12.0, None)
    }
}

pub const PAPER: &str = "Data/Tools/TextureMaps.MHK";

fn plain_thumb() -> Image {
    let mut t = Image::filled(48, 36, 0xffff_ffff);
    gfx::stroke_rect(&mut t, 0, 0, 48, 36, 1, 0xff80_8080);
    t
}

/// Stamps are 16-bit bitmaps on a white field; white becomes transparent.
pub fn decode_stamp(data: &[u8]) -> Option<Image> {
    let img = mohawk::decode_bitmap(data, None).ok()?;
    Some(knock_out_white(&img))
}

fn knock_out_white(img: &Image) -> Image {
    let mut out = img.clone();
    for p in out.pixels.iter_mut() {
        if *p & 0x00ff_ffff >= 0x00f8_f8f8 && (*p >> 16 & 255) >= 0xf8 && (*p >> 8 & 255) >= 0xf8 && (*p & 255) >= 0xf8 {
            *p = 0;
        }
    }
    out
}

/// Thumbnail for a user item without art: its name on a white tile, in the UI font.
fn name_thumb(name: &str) -> Image {
    let mut img = Image::filled(48, 48, 0xffff_ffff);
    gfx::fill_rect(&mut img, 0, 0, 48, 1, 0xff80_8080);
    gfx::fill_rect(&mut img, 0, 47, 48, 1, 0xff80_8080);
    gfx::fill_rect(&mut img, 0, 0, 1, 48, 0xff80_8080);
    gfx::fill_rect(&mut img, 47, 0, 1, 48, 0xff80_8080);
    let fonts = crate::ui::Fonts { faces: Vec::new() };
    let words: Vec<&str> = name.split_whitespace().collect();
    let mut lines: Vec<String> = Vec::new();
    for w in words {
        let cand = match lines.last() {
            Some(l) if fonts.measure(crate::ui::UI_BITMAP, 0.0, &format!("{l} {w}")) <= 44.0 => format!("{l} {w}"),
            _ => {
                lines.push(String::new());
                w.to_string()
            }
        };
        *lines.last_mut().unwrap() = cand;
    }
    for (i, l) in lines.iter().take(3).enumerate() {
        let w = fonts.measure(crate::ui::UI_BITMAP, 0.0, l);
        fonts.draw(&mut img, crate::ui::UI_BITMAP, 0.0, ((48.0 - w) / 2.0).max(2.0), 16.0 + i as f32 * 12.0, 0xff00_0000, l);
    }
    img
}
