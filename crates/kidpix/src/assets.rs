//! Runtime access to the user's own Kid Pix Deluxe 3 data (never bundled).

use mohawk::{Archive, Image, Palette, Sound, Tag};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

pub const T_BMP: Tag = Tag(*b"tBMP");
pub const T_BMH: Tag = Tag(*b"tBMH");
pub const T_SCR: Tag = Tag(*b"tSCR");
pub const T_WAV: Tag = Tag(*b"tWAV");
pub const T_CUR: Tag = Tag(*b"tCUR");
pub const T_THM: Tag = Tag(*b"tTHM");
pub const T_ATX: Tag = Tag(*b"tATX");
pub const T_SWF: Tag = Tag(*b"tSWF");
pub const REGS: Tag = Tag(*b"REGS");
pub const MBTN: Tag = Tag(*b"MBTN");

/// Case-insensitive resolution of a `/`-separated relative path below `root`.
pub fn resolve_ci(root: &Path, rel: &str) -> Option<PathBuf> {
    let mut cur = root.to_path_buf();
    for part in rel.split(['/', '\\']).filter(|p| !p.is_empty()) {
        let direct = cur.join(part);
        if direct.exists() {
            cur = direct;
            continue;
        }
        let lower = part.to_lowercase();
        let found = std::fs::read_dir(&cur)
            .ok()?
            .flatten()
            .find(|e| e.file_name().to_string_lossy().to_lowercase() == lower)?;
        cur = found.path();
    }
    Some(cur)
}

/// True if `dir` looks like a Kid Pix Deluxe 3 CD copy / install.
pub fn is_game_root(dir: &Path) -> bool {
    resolve_ci(dir, "Data/Easel.MHK").is_some_and(|p| p.is_file())
}

/// A set of sprite cels (from a `tBMH`) plus their registration points.
pub struct SpriteSet {
    pub cels: Vec<Image>,
    /// Cels that are 8-bit Tint masks (drawn in the current colour).
    pub masks: Vec<bool>,
    pub reg_x: Vec<i16>,
    pub reg_y: Vec<i16>,
}

impl SpriteSet {
    /// 1-based cel lookup.
    pub fn cel(&self, n: u16) -> Option<&Image> {
        self.cels.get((n as usize).checked_sub(1)?)
    }
    /// Registration point of cel n, if the REGS tables have one (the extra tBMP cel of a
    /// tCNT set has none: it is placed at the script's top-left).
    pub fn reg_of(&self, n: u16) -> Option<(i32, i32)> {
        let i = n as usize;
        Some((*self.reg_x.get(i)? as i32, *self.reg_y.get(i)? as i32))
    }
}

/// Simple INI parser for KPD3.cfg / KPD3.ini (sections, `key = value`, `;` and `//` comments).
#[derive(Default, Debug)]
pub struct Ini {
    pub sections: HashMap<String, Vec<(String, String)>>,
}

impl Ini {
    pub fn parse(text: &str) -> Ini {
        let mut ini = Ini::default();
        let mut cur = String::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with(';') || line.starts_with("//") {
                continue;
            }
            if line.starts_with('[') {
                if let Some(end) = line.find(']') {
                    cur = line[1..end].to_lowercase();
                    ini.sections.entry(cur.clone()).or_default();
                }
                continue;
            }
            if let Some(eq) = line.find('=') {
                let key = line[..eq].trim().to_lowercase();
                let mut val = line[eq + 1..].trim();
                if let Some(i) = val.find("//") {
                    val = val[..i].trim();
                }
                let val = val.trim_end_matches(';').trim().trim_matches('"').to_string();
                ini.sections.entry(cur.clone()).or_default().push((key, val));
            }
        }
        ini
    }

    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        let key = key.to_lowercase();
        self.sections
            .get(&section.to_lowercase())?
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v.as_str())
    }

    pub fn get_i32(&self, section: &str, key: &str) -> Option<i32> {
        self.get(section, key)?.split_whitespace().next()?.parse().ok()
    }

    pub fn has_section(&self, section: &str) -> bool {
        self.sections.contains_key(&section.to_lowercase())
    }
}

pub struct Assets {
    pub root: PathBuf,
    pub cfg: Ini,
    pub ini: Ini,
    archives: RefCell<HashMap<String, Option<Rc<Archive>>>>,
    sprites: RefCell<HashMap<(String, u16), Rc<SpriteSet>>>,
    sounds: RefCell<HashMap<(String, u16), Option<Arc<Sound>>>>,
    images: RefCell<HashMap<(String, u16), Option<Rc<Image>>>>,
}

impl Assets {
    pub fn new(root: PathBuf) -> Assets {
        let read_ini = |rel: &str| {
            resolve_ci(&root, rel)
                .and_then(|p| std::fs::read(p).ok())
                .map(|b| Ini::parse(&b.iter().map(|&c| c as char).collect::<String>()))
                .unwrap_or_default()
        };
        let cfg = read_ini("KpInstall/KPD3.cfg");
        let ini = read_ini("KpInstall/KPD3.ini");
        Assets {
            root,
            cfg,
            ini,
            archives: Default::default(),
            sprites: Default::default(),
            sounds: Default::default(),
            images: Default::default(),
        }
    }

    pub fn path(&self, rel: &str) -> Option<PathBuf> {
        resolve_ci(&self.root, rel)
    }

    /// Open (cached) an archive by path relative to the CD root, e.g. `Data/Easel.MHK`.
    pub fn archive(&self, rel: &str) -> Option<Rc<Archive>> {
        let key = rel.to_lowercase();
        if let Some(a) = self.archives.borrow().get(&key) {
            return a.clone();
        }
        let a = self.path(rel).and_then(|p| match Archive::open(&p) {
            Ok(a) => Some(Rc::new(a)),
            Err(e) => {
                eprintln!("warning: cannot open {}: {e}", p.display());
                None
            }
        });
        self.archives.borrow_mut().insert(key, a.clone());
        a
    }

    /// Sprite set `tBMH id` with `REGS id`/`id+1`.
    pub fn sprites(&self, rel: &str, id: u16) -> Option<Rc<SpriteSet>> {
        let key = (rel.to_lowercase(), id);
        if let Some(s) = self.sprites.borrow().get(&key) {
            return Some(s.clone());
        }
        let a = self.archive(rel)?;
        let (mut cels, mut masks) = match mohawk::decode_bitmap_set_masks(a.get(T_BMH, id)?, None) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("warning: {rel} tBMH {id}: {e}");
                return None;
            }
        };
        // tCNT {base, count}: count = set cels + 1; the last cel is the standalone tBMP `base`
        // (e.g. the splash still, the easel backdrop).
        if let Some(cnt) = a.get(mohawk::Tag(*b"tCNT"), id).filter(|c| c.len() >= 4) {
            let base = u16::from_be_bytes([cnt[0], cnt[1]]);
            let count = u16::from_be_bytes([cnt[2], cnt[3]]) as usize;
            if count > cels.len() {
                if let Some(img) = a.get(T_BMP, base).and_then(|d| mohawk::decode_bitmap(d, None).ok()) {
                    cels.push(img);
                    masks.push(false);
                }
            }
        }
        let reg_x = a.get(REGS, id).map(mohawk::parse_regs).unwrap_or_default();
        let reg_y = a.get(REGS, id + 1).map(mohawk::parse_regs).unwrap_or_default();
        let s = Rc::new(SpriteSet { cels, masks, reg_x, reg_y });
        self.sprites.borrow_mut().insert(key, s.clone());
        Some(s)
    }

    pub fn image(&self, rel: &str, id: u16) -> Option<Rc<Image>> {
        self.image_pal(rel, id, None)
    }

    pub fn image_pal(&self, rel: &str, id: u16, pal: Option<&Palette>) -> Option<Rc<Image>> {
        let key = (rel.to_lowercase(), id);
        if pal.is_none() {
            if let Some(i) = self.images.borrow().get(&key) {
                return i.clone();
            }
        }
        let a = self.archive(rel)?;
        let img = a
            .get(T_BMP, id)
            .and_then(|d| match mohawk::decode_bitmap(d, pal) {
                Ok(i) => Some(Rc::new(i)),
                Err(e) => {
                    eprintln!("warning: {rel} tBMP {id}: {e}");
                    None
                }
            });
        if pal.is_none() {
            self.images.borrow_mut().insert(key, img.clone());
        }
        img
    }

    pub fn sound(&self, rel: &str, id: u16) -> Option<Arc<Sound>> {
        let key = (rel.to_lowercase(), id);
        if let Some(s) = self.sounds.borrow().get(&key) {
            return s.clone();
        }
        let s = self
            .archive(rel)
            .and_then(|a| a.get(T_WAV, id).map(mohawk::decode_sound))
            .and_then(|r| r.ok())
            .map(Arc::new);
        self.sounds.borrow_mut().insert(key, s.clone());
        s
    }

    pub fn script(&self, rel: &str, id: u16) -> Option<mohawk::Script> {
        let a = self.archive(rel)?;
        mohawk::parse_script(a.get(T_SCR, id)?).ok()
    }

    pub fn cursor(&self, id: u16) -> Option<mohawk::Cursor> {
        let a = self.archive("Data/General.MHK")?;
        mohawk::parse_cursor(a.get(T_CUR, id)?).ok()
    }

    /// List `*.klb` files of a library folder (e.g. `Libraries/Stamps`), sorted by name.
    pub fn libraries(&self, rel_dir: &str) -> Vec<PathBuf> {
        let Some(dir) = self.path(rel_dir) else {
            return vec![];
        };
        let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.path())
                    .filter(|p| {
                        p.extension()
                            .is_some_and(|e| e.eq_ignore_ascii_case("klb"))
                    })
                    .collect()
            })
            .unwrap_or_default();
        v.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
        v
    }

    /// TrueType fonts shipped with the original installer (INSTALL/Fonts).
    pub fn font_files(&self) -> Vec<PathBuf> {
        let Some(dir) = self.path("INSTALL/Fonts") else {
            return vec![];
        };
        let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("ttf")))
                    .collect()
            })
            .unwrap_or_default();
        v.sort();
        v.extend(system_text_fonts());
        v
    }
}

/// A few installed system families the original's font list (EnumFontFamilies) would also
/// show, Arial being its default: the common Windows / macOS fonts or their metric-compatible
/// Linux equivalents. Only these files are loaded, so start-up stays quick.
fn system_text_fonts() -> Vec<PathBuf> {
    let names = [
        "arial.ttf", "Arial.ttf", "LiberationSans-Regular.ttf",
        "times.ttf", "Times New Roman.ttf", "LiberationSerif-Regular.ttf",
        "cour.ttf", "Courier New.ttf", "LiberationMono-Regular.ttf",
        "comic.ttf", "Comic Sans MS.ttf",
        "verdana.ttf", "Verdana.ttf",
    ];
    let mut dirs: Vec<PathBuf> = Vec::new();
    if cfg!(windows) {
        dirs.push(PathBuf::from(std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into())).join("Fonts"));
    } else if cfg!(target_os = "macos") {
        dirs.extend(["/System/Library/Fonts/Supplemental", "/Library/Fonts", "/System/Library/Fonts"].map(PathBuf::from));
    } else {
        for d in ["/usr/share/fonts", "/usr/local/share/fonts"] {
            let base = PathBuf::from(d);
            dirs.push(base.clone());
            if let Ok(rd) = std::fs::read_dir(&base) {
                for e in rd.flatten().filter(|e| e.path().is_dir()) {
                    dirs.push(e.path());
                    if let Ok(rd2) = std::fs::read_dir(e.path()) {
                        dirs.extend(rd2.flatten().map(|x| x.path()).filter(|p| p.is_dir()));
                    }
                }
            }
        }
    }
    let mut out = Vec::new();
    for n in names {
        if let Some(p) = dirs.iter().map(|d| d.join(n)).find(|p| p.is_file()) {
            out.push(p);
        }
    }
    out
}
