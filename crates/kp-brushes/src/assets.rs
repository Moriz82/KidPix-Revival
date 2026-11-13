//! Lazily loaded brush art from the user's CD data.
//!
//! Everything degrades gracefully: without a game directory (or with a missing resource)
//! synthetic stand-ins are produced so every brush still draws something (tests, tools).

use mohawk::{Archive, Image, Script, Tag};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

pub const T_BMP: Tag = Tag(*b"tBMP");
pub const T_BMH: Tag = Tag(*b"tBMH");
pub const T_SCR: Tag = Tag(*b"tSCR");
pub const T_WAV: Tag = Tag(*b"tWAV");
pub const T_CNT: Tag = Tag(*b"tCNT");
pub const REGS: Tag = Tag(*b"REGS");

pub const BRUSHES_MHK: &str = "Data/Tools/Brushes.MHK";
pub const ICONS_MHK: &str = crate::ids::ICONS_MHK;
pub const CYCLIC_MHK: &str = crate::ids::CYCLIC_MHK;
pub const ANIMS_MHK: &str = crate::ids::ANIMS_MHK;
pub const EASEL_MHK: &str = "Data/Easel.MHK";
pub const WACKY_SOUNDS_MHK: &str = "Data/Tools/WackyBrushSounds.MHK";
pub const TEXTURE_MAPS_MHK: &str = "Data/Tools/TextureMaps.MHK";

/// An 8-bit brush pad in its square buffer (§3.2): `a[py*side + px]`, 0 = untouched.
#[derive(Clone, Debug)]
pub struct Pad {
    /// Side of the square buffer = `max(w, h)` = brush size field `0x2a`.
    pub side: i32,
    pub w: i32,
    pub h: i32,
    pub a: Vec<u8>,
}

impl Pad {
    pub fn at(&self, px: i32, py: i32) -> u8 {
        if px < 0 || py < 0 || px >= self.side || py >= self.side {
            0
        } else {
            self.a[(py * self.side + px) as usize]
        }
    }

    /// Bitmap drawn at (0,0) into a `side×side` zeroed buffer.
    pub fn from_coverage(w: i32, h: i32, cov: &[u8], side: Option<i32>) -> Pad {
        let side = side.unwrap_or(w.max(h)).max(1);
        let mut a = vec![0u8; (side * side) as usize];
        for y in 0..h.min(side) {
            for x in 0..w.min(side) {
                a[(y * side + x) as usize] = cov[(y * w + x) as usize];
            }
        }
        Pad { side, w, h, a }
    }

    /// Synthetic soft round pad (fallback when Brushes.MHK is missing).
    pub fn soft_disc(d: i32, peak: u8) -> Pad {
        let d = d.max(1);
        let r = d as f32 / 2.0;
        let mut cov = vec![0u8; (d * d) as usize];
        for y in 0..d {
            for x in 0..d {
                let dx = x as f32 + 0.5 - r;
                let dy = y as f32 + 0.5 - r;
                let t = 1.0 - (dx * dx + dy * dy).sqrt() / r;
                if t > 0.0 {
                    cov[(y * d + x) as usize] = ((t.min(0.5) * 2.0) * peak as f32).max(1.0) as u8;
                }
            }
        }
        Pad::from_coverage(d, d, &cov, None)
    }
}

/// 8-bit texture map (Easel.MHK tBMP 17000+n, §3.7).
#[derive(Clone, Debug)]
pub struct Texture {
    pub w: i32,
    pub h: i32,
    pub data: Vec<u8>,
}

/// A `tBMH` sprite set plus its registration points (`REGS id` = x, `REGS id+1` = y).
#[derive(Clone, Debug)]
pub struct SpriteSet {
    pub id: u16,
    /// Cel `n` (1-based in scripts) is `cels[n-1]`.
    pub cels: Vec<Image>,
    /// Indexed by cel number (index 0 unused).
    pub reg_x: Vec<i16>,
    pub reg_y: Vec<i16>,
}

impl SpriteSet {
    pub fn cel(&self, n: u16) -> Option<&Image> {
        self.cels.get((n as usize).checked_sub(1)?)
    }
    pub fn reg(&self, n: u16) -> (i32, i32) {
        (
            self.reg_x.get(n as usize).copied().unwrap_or(0) as i32,
            self.reg_y.get(n as usize).copied().unwrap_or(0) as i32,
        )
    }
}

/// Header facts about a `tWAV` the sound state machine needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SoundInfo {
    pub duration_ms: u32,
    /// `loopCount = 65535` in the `Data` header.
    pub looping: bool,
}

type Cache<T> = Mutex<HashMap<(&'static str, u16), Option<Arc<T>>>>;

/// Brush art loaded lazily from `<game root>`. Cheap to share (`&BrushAssets`); caches
/// are behind mutexes so it is `Send + Sync`.
pub struct BrushAssets {
    root: Option<PathBuf>,
    archives: Mutex<HashMap<&'static str, Option<Arc<Archive>>>>,
    pads: Mutex<HashMap<(u16, i32), Arc<Pad>>>,
    bitmaps: Cache<Image>,
    sets: Cache<SpriteSet>,
    scripts: Cache<Script>,
    textures: Mutex<HashMap<u16, Option<Arc<Texture>>>>,
    sounds: Mutex<HashMap<u16, Option<SoundInfo>>>,
    font: OnceLock<Option<fontdue::Font>>,
}

impl std::fmt::Debug for BrushAssets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BrushAssets({:?})", self.root)
    }
}

/// Resolve `root/a/b/...` matching each component case-insensitively.
pub fn find_ci(root: &Path, parts: &[&str]) -> Option<PathBuf> {
    let mut cur = root.to_path_buf();
    for part in parts {
        let direct = cur.join(part);
        if direct.exists() {
            cur = direct;
            continue;
        }
        let rd = std::fs::read_dir(&cur).ok()?;
        let mut found = None;
        for e in rd.flatten() {
            if e.file_name().to_string_lossy().eq_ignore_ascii_case(part) {
                found = Some(e.path());
                break;
            }
        }
        cur = found?;
    }
    Some(cur)
}

impl BrushAssets {
    /// Assets from a game root (the folder holding `Data/` and `KpInstall/`).
    pub fn new(game_root: impl AsRef<Path>) -> BrushAssets {
        let mut a = BrushAssets::empty();
        a.root = Some(game_root.as_ref().to_path_buf());
        a
    }

    /// No game data: every lookup falls back to synthetic art.
    pub fn empty() -> BrushAssets {
        BrushAssets {
            root: None,
            archives: Mutex::new(HashMap::new()),
            pads: Mutex::new(HashMap::new()),
            bitmaps: Mutex::new(HashMap::new()),
            sets: Mutex::new(HashMap::new()),
            scripts: Mutex::new(HashMap::new()),
            textures: Mutex::new(HashMap::new()),
            sounds: Mutex::new(HashMap::new()),
            font: OnceLock::new(),
        }
    }

    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    /// Open (once) an archive by its path relative to the game root.
    pub fn archive(&self, rel: &'static str) -> Option<Arc<Archive>> {
        let mut m = self.archives.lock().unwrap();
        if let Some(a) = m.get(rel) {
            return a.clone();
        }
        let a = self.root.as_ref().and_then(|r| {
            let parts: Vec<&str> = rel.split('/').collect();
            let p = find_ci(r, &parts)?;
            Archive::open(p).ok().map(Arc::new)
        });
        m.insert(rel, a.clone());
        a
    }

    /// Brush pad `id` (Brushes.MHK tBMP) in a square buffer; `side` overrides the
    /// buffer size (Extrusion's 33×33 highlight buffer). Missing pads become soft discs.
    pub fn pad(&self, id: u16, side: Option<i32>) -> Arc<Pad> {
        let key = (id, side.unwrap_or(0));
        if let Some(p) = self.pads.lock().unwrap().get(&key) {
            return p.clone();
        }
        let pad = self
            .archive(BRUSHES_MHK)
            .and_then(|a| decode_pad(a.get(T_BMP, id)?, side))
            .unwrap_or_else(|| {
                // Brushes.MHK holds triplets small/medium/large starting at 1001.
                let k = (id.saturating_sub(1001) % 3) as i32;
                let d = [10, 15, 23][k as usize];
                let mut p = Pad::soft_disc(d, 200);
                if let Some(s) = side {
                    p = Pad::from_coverage(p.w, p.h, &p.a, Some(s));
                }
                p
            });
        let pad = Arc::new(pad);
        self.pads.lock().unwrap().insert(key, pad.clone());
        pad
    }

    /// A `tBMP` decoded to ARGB (8-bit ones as greyscale).
    pub fn bitmap(&self, archive: &'static str, id: u16) -> Option<Arc<Image>> {
        let key = (archive, id);
        if let Some(b) = self.bitmaps.lock().unwrap().get(&key) {
            return b.clone();
        }
        let img = self
            .archive(archive)
            .and_then(|a| mohawk::decode_bitmap(a.get(T_BMP, id)?, None).ok())
            .map(Arc::new);
        self.bitmaps.lock().unwrap().insert(key, img.clone());
        img
    }

    /// A bitmap or a synthetic stand-in (`fallback` builds it).
    pub fn bitmap_or(&self, archive: &'static str, id: u16, fallback: impl FnOnce() -> Image) -> Arc<Image> {
        if let Some(b) = self.bitmap(archive, id) {
            return b;
        }
        let img = Arc::new(fallback());
        self.bitmaps.lock().unwrap().insert((archive, id), Some(img.clone()));
        img
    }

    pub fn sprite_set(&self, archive: &'static str, id: u16) -> Option<Arc<SpriteSet>> {
        let key = (archive, id);
        if let Some(s) = self.sets.lock().unwrap().get(&key) {
            return s.clone();
        }
        let set = self.archive(archive).and_then(|a| {
            let cels = mohawk::decode_bitmap_set(a.get(T_BMH, id)?, None).ok()?;
            let reg_x = a.get(REGS, id).map(mohawk::parse_regs).unwrap_or_default();
            let reg_y = a.get(REGS, id.wrapping_add(1)).map(mohawk::parse_regs).unwrap_or_default();
            Some(Arc::new(SpriteSet { id, cels, reg_x, reg_y }))
        });
        self.sets.lock().unwrap().insert(key, set.clone());
        set
    }

    pub fn script(&self, archive: &'static str, id: u16) -> Option<Arc<Script>> {
        let key = (archive, id);
        if let Some(s) = self.scripts.lock().unwrap().get(&key) {
            return s.clone();
        }
        let s = self
            .archive(archive)
            .and_then(|a| mohawk::parse_script(a.get(T_SCR, id)?).ok())
            .map(Arc::new);
        self.scripts.lock().unwrap().insert(key, s.clone());
        s
    }

    /// Ids of a resource type within `[lo, hi]`.
    pub fn ids_in(&self, archive: &'static str, tag: Tag, lo: u16, hi: u16) -> Vec<u16> {
        self.archive(archive)
            .map(|a| a.ids(tag).into_iter().filter(|&i| i >= lo && i <= hi).collect())
            .unwrap_or_default()
    }

    /// `tCNT id` → (base, count).
    pub fn count(&self, archive: &'static str, id: u16) -> Option<(u16, u16)> {
        mohawk::parse_count(self.archive(archive)?.get(T_CNT, id)?).ok()
    }

    /// Easel texture `n` (Easel.MHK tBMP 17000+n), 8-bit raw indices [I: copied raw].
    pub fn texture(&self, n: u16) -> Option<Arc<Texture>> {
        if n == 0 {
            return None;
        }
        if let Some(t) = self.textures.lock().unwrap().get(&n) {
            return t.clone();
        }
        let t = self.archive(EASEL_MHK).and_then(|a| {
            let ind = mohawk::decode_indexed(a.get(T_BMP, 17000u16.checked_add(n)?)?).ok()?;
            Some(Arc::new(Texture { w: ind.width as i32, h: ind.height as i32, data: ind.indices }))
        });
        self.textures.lock().unwrap().insert(n, t.clone());
        t
    }

    /// Duration and loop flag of a WackyBrushSounds.MHK `tWAV`.
    pub fn sound_info(&self, id: u16) -> Option<SoundInfo> {
        if id == 0 {
            return None;
        }
        if let Some(s) = self.sounds.lock().unwrap().get(&id) {
            return *s;
        }
        let info = self.archive(WACKY_SOUNDS_MHK).and_then(|a| {
            let s = mohawk::decode_sound(a.get(T_WAV, id)?).ok()?;
            Some(SoundInfo { duration_ms: (s.duration_secs() * 1000.0) as u32, looping: s.loop_count == 0xffff })
        });
        self.sounds.lock().unwrap().insert(id, info);
        info
    }

    /// The "Jester" face (`INSTALL/Fonts/JESTER.TTF`) used by the ABC brush.
    pub fn jester(&self) -> Option<&fontdue::Font> {
        self.font
            .get_or_init(|| {
                let r = self.root.as_ref()?;
                let p = find_ci(r, &["INSTALL", "Fonts", "JESTER.TTF"])?;
                let bytes = std::fs::read(p).ok()?;
                fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default()).ok()
            })
            .as_ref()
    }
}

/// Decode a Brushes.MHK pad (§3.2): 8-bit pads raw, 16-bit sprites by luminance
/// (`a ≈ 255 - luminance`, GDI mapping to a palette whose index 0 is white) [I].
pub fn decode_pad(data: &[u8], side: Option<i32>) -> Option<Pad> {
    let h = mohawk::BitmapHeader::parse(data).ok()?;
    if h.bpp_code() == 2 {
        let ind = mohawk::decode_indexed(data).ok()?;
        Some(Pad::from_coverage(ind.width as i32, ind.height as i32, &ind.indices, side))
    } else {
        let img = mohawk::decode_bitmap(data, None).ok()?;
        let cov: Vec<u8> = img
            .pixels
            .iter()
            .map(|&p| {
                if p >> 24 == 0 {
                    return 0;
                }
                let r = (p >> 16) & 255;
                let g = (p >> 8) & 255;
                let b = p & 255;
                let lum = (r * 30 + g * 59 + b * 11) / 100; // [I] luminance weights
                (255 - lum) as u8
            })
            .collect();
        Some(Pad::from_coverage(img.width as i32, img.height as i32, &cov, side))
    }
}
