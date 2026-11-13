//! Locate or import the user's own Kid Pix Deluxe 3 CD.
//!
//! Sources: a folder (CD copy or Windows install), the CD's `.iso` image, or a `.zip`
//! holding either. Importing copies the files the engine needs into a destination folder
//! so the CD/ISO is no longer needed afterwards.
//!
//! The CD is a hybrid Windows/Mac disc: many files appear twice in the ISO9660 directory,
//! once as the Mac resource fork (an "associated file", flag bit 0x04) and once as the
//! real data. Associated records are skipped here.

use std::fmt;
use std::fs::{self, File};
use std::io::{self, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Format(String),
    NotKidPix(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "{e}"),
            Error::Format(s) => write!(f, "{s}"),
            Error::NotKidPix(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<zip::result::ZipError> for Error {
    fn from(e: zip::result::ZipError) -> Self {
        Error::Format(format!("zip: {e}"))
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Debug, Default)]
pub struct Progress {
    pub done_bytes: u64,
    pub total_bytes: u64,
    pub current: String,
}

// ------------------------------------------------------------------------- paths

/// Case-insensitive resolution of a `/`-separated path below `root`.
pub fn resolve_ci(root: &Path, rel: &str) -> Option<PathBuf> {
    let mut cur = root.to_path_buf();
    for part in rel.split(['/', '\\']).filter(|p| !p.is_empty()) {
        let direct = cur.join(part);
        if direct.exists() {
            cur = direct;
            continue;
        }
        let lower = part.to_lowercase();
        let found = fs::read_dir(&cur)
            .ok()?
            .flatten()
            .find(|e| e.file_name().to_string_lossy().to_lowercase() == lower)?;
        cur = found.path();
    }
    Some(cur)
}

pub fn is_game_root(dir: &Path) -> bool {
    resolve_ci(dir, "Data/Easel.MHK").is_some_and(|p| p.is_file() && p.metadata().map(|m| m.len() > 100_000).unwrap_or(false))
}

/// `dir` itself or one of its direct subfolders, if it holds the game data.
pub fn find_game_root(dir: &Path) -> Option<PathBuf> {
    if is_game_root(dir) {
        return Some(dir.to_path_buf());
    }
    fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .find(|p| is_game_root(p))
}

/// Which files of the CD the engine keeps (relative, `/`-separated).
pub fn wanted(rel: &str) -> bool {
    let l = rel.to_lowercase();
    let top = l.split('/').next().unwrap_or("");
    match top {
        "data" => !l.starts_with("data/players/"),
        "libraries" | "idea machine" | "qtmovies" => true,
        "install" => l.starts_with("install/fonts/"),
        "kpinstall" => l.ends_with(".cfg") || l.ends_with(".ini") || l.ends_with(".lex") || l.ends_with(".env") || l.ends_with(".pdf"),
        _ => l == "read me.txt",
    }
}

// ------------------------------------------------------------------------- ISO 9660

const SECTOR: u64 = 2048;

#[derive(Clone, Debug)]
pub struct IsoEntry {
    pub path: String,
    pub lba: u32,
    pub size: u64,
    pub is_dir: bool,
}

pub struct Iso<R: Read + Seek> {
    r: R,
    joliet: bool,
    root: (u32, u32),
}

impl<R: Read + Seek> Iso<R> {
    pub fn open(mut r: R) -> Result<Iso<R>> {
        let mut primary = None;
        let mut joliet = None;
        for s in 16..64u64 {
            let mut vd = [0u8; 2048];
            r.seek(SeekFrom::Start(s * SECTOR))?;
            r.read_exact(&mut vd)?;
            if &vd[1..6] != b"CD001" {
                return Err(Error::Format("not an ISO 9660 image".into()));
            }
            let root = (le32(&vd[156 + 2..]), le32(&vd[156 + 10..]));
            match vd[0] {
                1 => primary = Some(root),
                2 if vd[88] == 0x25 && vd[89] == 0x2f && matches!(vd[90], 0x40 | 0x43 | 0x45) => {
                    joliet = Some(root)
                }
                255 => break,
                _ => {}
            }
        }
        let (root, is_joliet) = match (joliet, primary) {
            (Some(j), _) => (j, true),
            (None, Some(p)) => (p, false),
            _ => return Err(Error::Format("ISO has no volume descriptor".into())),
        };
        Ok(Iso {
            r,
            joliet: is_joliet,
            root,
        })
    }

    /// All files and folders (data forks only), depth-first.
    pub fn entries(&mut self) -> Result<Vec<IsoEntry>> {
        let mut out = Vec::new();
        let mut stack = vec![(String::new(), self.root.0, self.root.1)];
        let mut seen = std::collections::HashSet::new();
        while let Some((prefix, lba, size)) = stack.pop() {
            if !seen.insert(lba) || size > 64 << 20 {
                continue;
            }
            let mut buf = vec![0u8; size as usize];
            self.r.seek(SeekFrom::Start(lba as u64 * SECTOR))?;
            self.r.read_exact(&mut buf)?;
            let mut pos = 0usize;
            while pos < buf.len() {
                let len = buf[pos] as usize;
                if len == 0 {
                    // Records never span sectors; skip to the next one.
                    pos = (pos / SECTOR as usize + 1) * SECTOR as usize;
                    continue;
                }
                if pos + len > buf.len() || len < 34 {
                    break;
                }
                let rec = &buf[pos..pos + len];
                pos += len;
                let flags = rec[25];
                let name_len = rec[32] as usize;
                let raw = &rec[33..33 + name_len.min(len - 33)];
                if name_len == 1 && (raw[0] == 0 || raw[0] == 1) {
                    continue; // . and ..
                }
                if flags & 0x04 != 0 {
                    continue; // associated file = Mac resource fork
                }
                let name = if self.joliet { decode_ucs2(raw) } else { decode_primary(raw) };
                let path = if prefix.is_empty() { name } else { format!("{prefix}/{name}") };
                let e_lba = le32(&rec[2..]);
                let e_size = le32(&rec[10..]) as u64;
                if flags & 0x02 != 0 {
                    stack.push((path.clone(), e_lba, e_size as u32));
                    out.push(IsoEntry { path, lba: e_lba, size: 0, is_dir: true });
                } else {
                    out.push(IsoEntry { path, lba: e_lba, size: e_size, is_dir: false });
                }
            }
        }
        Ok(out)
    }

    pub fn copy_to(&mut self, e: &IsoEntry, w: &mut impl Write, progress: &mut dyn FnMut(u64)) -> Result<()> {
        self.r.seek(SeekFrom::Start(e.lba as u64 * SECTOR))?;
        let mut left = e.size;
        let mut buf = vec![0u8; 1 << 20];
        while left > 0 {
            let n = left.min(buf.len() as u64) as usize;
            self.r.read_exact(&mut buf[..n])?;
            w.write_all(&buf[..n])?;
            left -= n as u64;
            progress(n as u64);
        }
        Ok(())
    }
}

fn le32(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

pub fn decode_ucs2(raw: &[u8]) -> String {
    let units: Vec<u16> = raw.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
    let s = String::from_utf16_lossy(&units);
    strip_version(&s)
}

pub fn decode_primary(raw: &[u8]) -> String {
    let s: String = raw.iter().map(|&b| b as char).collect();
    let s = strip_version(&s);
    s.strip_suffix('.').map(str::to_string).unwrap_or(s)
}

fn strip_version(s: &str) -> String {
    match s.rfind(';') {
        Some(i) if s[i + 1..].chars().all(|c| c.is_ascii_digit()) => s[..i].to_string(),
        _ => s.to_string(),
    }
}

/// Make a CD path safe as a relative path on any OS.
fn safe_rel(rel: &str) -> Option<PathBuf> {
    let mut p = PathBuf::new();
    for part in rel.split('/') {
        if part.is_empty() || part == "." || part == ".." {
            return None;
        }
        let clean: String = part
            .chars()
            .map(|c| if matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*' | '\\') || c.is_control() { '_' } else { c })
            .collect();
        let clean = clean.trim_end_matches(['.', ' ']).to_string();
        if clean.is_empty() {
            return None;
        }
        p.push(clean);
    }
    Some(p)
}

/// Import from an ISO image file.
pub fn import_iso(iso_path: &Path, dest: &Path, progress: &mut dyn FnMut(&Progress)) -> Result<PathBuf> {
    let f = BufReader::with_capacity(1 << 20, File::open(iso_path)?);
    let mut iso = Iso::open(f)?;
    let entries = iso.entries()?;
    if !entries.iter().any(|e| e.path.eq_ignore_ascii_case("Data/Easel.MHK")) {
        return Err(Error::NotKidPix(format!(
            "{} is not a Kid Pix Deluxe 3 CD image (no Data/Easel.MHK).",
            iso_path.display()
        )));
    }
    let files: Vec<&IsoEntry> = entries.iter().filter(|e| !e.is_dir && wanted(&e.path)).collect();
    let total: u64 = files.iter().map(|e| e.size).sum();
    let mut done = 0u64;
    fs::create_dir_all(dest)?;
    for e in files {
        let Some(rel) = safe_rel(&e.path) else { continue };
        let out = dest.join(&rel);
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = out.with_extension("part");
        {
            let mut w = io::BufWriter::new(File::create(&tmp)?);
            let name = e.path.clone();
            iso.copy_to(e, &mut w, &mut |n| {
                done += n;
                progress(&Progress { done_bytes: done, total_bytes: total, current: name.clone() });
            })?;
            w.flush()?;
        }
        fs::rename(&tmp, &out)?;
    }
    if !is_game_root(dest) {
        return Err(Error::Format("import finished but the result is incomplete".into()));
    }
    Ok(dest.to_path_buf())
}

/// Import from a ZIP holding either the `.iso` or an extracted CD tree.
pub fn import_zip(zip_path: &Path, dest: &Path, progress: &mut dyn FnMut(&Progress)) -> Result<PathBuf> {
    let mut zip = zip::ZipArchive::new(BufReader::new(File::open(zip_path)?))?;
    // Case 1: an ISO inside.
    let iso_idx = (0..zip.len()).find(|&i| {
        zip.by_index(i)
            .map(|f| f.name().to_lowercase().ends_with(".iso"))
            .unwrap_or(false)
    });
    fs::create_dir_all(dest)?;
    if let Some(i) = iso_idx {
        let tmp = dest.join(".kidpix-import.iso.part");
        let result = (|| {
            let mut f = zip.by_index(i)?;
            let total = f.size();
            let mut w = io::BufWriter::new(File::create(&tmp)?);
            let mut buf = vec![0u8; 1 << 20];
            let mut done = 0u64;
            loop {
                let n = f.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                w.write_all(&buf[..n])?;
                done += n as u64;
                // First half of the progress bar: unpacking the image.
                progress(&Progress { done_bytes: done / 2, total_bytes: total, current: "Unpacking CD image".into() });
            }
            w.flush()?;
            drop(w);
            import_iso(&tmp, dest, &mut |p| {
                progress(&Progress {
                    done_bytes: total / 2 + p.done_bytes * (total / 2) / p.total_bytes.max(1),
                    total_bytes: total,
                    current: p.current.clone(),
                })
            })
        })();
        let _ = fs::remove_file(&tmp);
        return result;
    }
    // Case 2: extracted files inside (possibly under one top folder).
    let names: Vec<String> = (0..zip.len()).filter_map(|i| zip.by_index(i).ok().map(|f| f.name().replace('\\', "/"))).collect();
    let prefix = names
        .iter()
        .find_map(|n| {
            let l = n.to_lowercase();
            l.find("data/easel.mhk").map(|p| n[..p].to_string())
        })
        .ok_or_else(|| Error::NotKidPix(format!("{} doesn't contain a Kid Pix Deluxe 3 CD.", zip_path.display())))?;
    let total: u64 = (0..zip.len()).filter_map(|i| zip.by_index(i).ok().map(|f| f.size())).sum();
    let mut done = 0u64;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i)?;
        let name = f.name().replace('\\', "/");
        let Some(rel) = name.strip_prefix(&prefix) else { continue };
        if f.is_dir() || !wanted(rel) {
            continue;
        }
        let Some(rel_p) = safe_rel(rel) else { continue };
        let out = dest.join(rel_p);
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut w = io::BufWriter::new(File::create(&out)?);
        done += io::copy(&mut f, &mut w)?;
        progress(&Progress { done_bytes: done, total_bytes: total, current: name.clone() });
    }
    if !is_game_root(dest) {
        return Err(Error::Format("import finished but the result is incomplete".into()));
    }
    Ok(dest.to_path_buf())
}

/// Copy the needed files from a folder (CD copy / install).
pub fn import_dir(src: &Path, dest: &Path, progress: &mut dyn FnMut(&Progress)) -> Result<PathBuf> {
    let root = find_game_root(src).ok_or_else(|| {
        Error::NotKidPix(format!("{} doesn't look like a Kid Pix Deluxe 3 CD or install.", src.display()))
    })?;
    let mut files = Vec::new();
    walk(&root, &root, &mut files)?;
    let files: Vec<(String, PathBuf, u64)> = files.into_iter().filter(|(r, _, _)| wanted(r)).collect();
    let total = files.iter().map(|f| f.2).sum();
    let mut done = 0;
    for (rel, path, size) in files {
        let Some(rel_p) = safe_rel(&rel) else { continue };
        let out = dest.join(rel_p);
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&path, &out)?;
        done += size;
        progress(&Progress { done_bytes: done, total_bytes: total, current: rel });
    }
    Ok(dest.to_path_buf())
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf, u64)>) -> Result<()> {
    for e in fs::read_dir(dir)?.flatten() {
        let p = e.path();
        let md = e.metadata()?;
        if md.is_dir() {
            walk(root, &p, out)?;
        } else {
            let rel = p.strip_prefix(root).unwrap_or(&p).to_string_lossy().replace('\\', "/");
            out.push((rel, p, md.len()));
        }
    }
    Ok(())
}

/// Import from any supported source into `dest`. Returns the game root.
pub fn import(source: &Path, dest: &Path, progress: &mut dyn FnMut(&Progress)) -> Result<PathBuf> {
    if source.is_dir() {
        return import_dir(source, dest, progress);
    }
    let mut magic = [0u8; 4];
    File::open(source)?.read_exact(&mut magic).ok();
    if &magic == b"PK\x03\x04" {
        import_zip(source, dest, progress)
    } else {
        import_iso(source, dest, progress)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(decode_primary(b"EASEL.MHK;1"), "EASEL.MHK");
        assert_eq!(decode_primary(b"DATA."), "DATA");
        let j: Vec<u8> = "Thank You.klb;1".encode_utf16().flat_map(|u| u.to_be_bytes()).collect();
        assert_eq!(decode_ucs2(&j), "Thank You.klb");
    }

    #[test]
    fn filter() {
        assert!(wanted("Data/Easel.MHK"));
        assert!(wanted("Libraries/Stamps/City.klb"));
        assert!(wanted("INSTALL/Fonts/MARKET.TTF"));
        assert!(wanted("KpInstall/KPD3.cfg"));
        assert!(!wanted("KpInstall/KPD3.exe"));
        assert!(!wanted("INSTALL/SETUP32.EXE"));
        assert!(!wanted("AOL/setup.exe"));
        assert!(!wanted("Data/Players/SwFlsh32.exe"));
    }

    #[test]
    fn safe_paths() {
        assert!(safe_rel("../x").is_none());
        assert_eq!(safe_rel("Idea Machine/Turtle.kpx").unwrap(), PathBuf::from("Idea Machine").join("Turtle.kpx"));
        assert_eq!(safe_rel("a/b:c").unwrap(), PathBuf::from("a").join("b_c"));
    }

    /// KIDPIX_ISO=<iso> [KIDPIX_REF_DIR=<reference tree>] cargo test -p kp-import -- --ignored
    #[test]
    #[ignore]
    fn real_iso() {
        let Ok(iso) = std::env::var("KIDPIX_ISO") else { return };
        let dest = std::env::temp_dir().join("kp-import-test");
        let _ = fs::remove_dir_all(&dest);
        import(Path::new(&iso), &dest, &mut |_| {}).unwrap();
        let mut files = Vec::new();
        walk(&dest, &dest, &mut files).unwrap();
        for (rel, _, size) in &files {
            let l = rel.to_lowercase();
            if l.ends_with(".mhk") || l.ends_with(".klb") {
                assert_ne!(*size, 4990, "{rel} is a resource fork");
            }
        }
        if let Ok(r) = std::env::var("KIDPIX_REF_DIR") {
            for (rel, p, _) in &files {
                let a = fs::read(p).unwrap();
                let b = fs::read(Path::new(&r).join(rel)).unwrap();
                assert!(a == b, "{rel} differs");
            }
        }
        let _ = fs::remove_dir_all(&dest);
    }
}
