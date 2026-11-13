//! QuickTime movies (Sorenson Video 1 + IMA4 on the CD), decoded natively by `kp-qt`
//! with the Sorenson tables read from the user's own
//! `INSTALL/QT4/QuickTime/QuickTimeAuthoring.qtx` (imported with the CD). Movies in other
//! formats (or a missing `.qtx`) fall back to an `ffmpeg` executable when one is
//! available. Frames are kept as RGB555 to stay small.

use crate::gfx::Image;
use mohawk::Sound;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;

pub struct Movie {
    pub width: u32,
    pub height: u32,
    pub fps: f32,
    frames: Vec<Vec<u16>>,
    pub audio: Option<Sound>,
}

impl Movie {
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    pub fn frame(&self, i: usize) -> Image {
        let f = &self.frames[i.min(self.frames.len().saturating_sub(1))];
        Image {
            width: self.width,
            height: self.height,
            pixels: f.iter().map(|&p| mohawk::rgb555_to_argb(p)).collect(),
        }
    }

    pub fn duration(&self) -> f32 {
        self.frames.len() as f32 / self.fps.max(1.0)
    }
}

static GAME_ROOT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Where the imported CD lives (for the Sorenson tables in its `.qtx`).
pub fn set_game_root(root: &Path) {
    let _ = GAME_ROOT.set(root.to_path_buf());
}

/// The Sorenson Video tables, read once from the game's `QuickTimeAuthoring.qtx`.
fn sorenson_tables() -> Option<&'static kp_qt::svq1::Tables> {
    static T: std::sync::OnceLock<Option<kp_qt::svq1::Tables>> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let root = GAME_ROOT.get()?;
        let p = crate::assets::resolve_ci(root, "INSTALL/QT4/QuickTime/QuickTimeAuthoring.qtx")?;
        match kp_qt::svq1::Tables::from_qtx_file(&p) {
            Ok(t) => Some(t),
            Err(e) => {
                eprintln!("{}: {e}", p.display());
                None
            }
        }
    })
    .as_ref()
}

/// Native decode (Sorenson Video 1 + IMA4 / PCM).
fn load_native(path: &Path) -> Result<Movie, String> {
    let t = sorenson_tables().ok_or("QuickTimeAuthoring.qtx (Sorenson Video) is missing from the imported CD data")?;
    let m = kp_qt::Movie::decode(path, t).map_err(|e| e.to_string())?;
    if m.frames.is_empty() {
        return Err("the movie has no pictures".into());
    }
    let to555 = |p: u32| (((p >> 19) & 31) << 10 | ((p >> 11) & 31) << 5 | ((p >> 3) & 31)) as u16;
    let frames = m.frames.iter().map(|f| f.iter().map(|&p| to555(p)).collect()).collect();
    let audio = (!m.audio.is_empty()).then(|| Sound { sample_rate: m.audio_rate, channels: m.audio_channels, samples: m.audio, loop_count: 0 });
    Ok(Movie { width: m.width, height: m.height, fps: m.fps, frames, audio })
}

pub fn ffmpeg() -> Option<PathBuf> {
    let name = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };
    // Next to the program first (portable installs), then PATH.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            let p = d.join(name);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths).map(|d| d.join(name)).find(|p| p.is_file())
    })
}

fn probe_size(ff: &Path, path: &Path) -> Option<(u32, u32, f32)> {
    // ffmpeg -i prints stream info on stderr; parse "WxH" and "fps".
    let out = Command::new(ff).arg("-hide_banner").arg("-i").arg(path).stderr(Stdio::piped()).stdout(Stdio::null()).output().ok()?;
    let text = String::from_utf8_lossy(&out.stderr);
    let line = text.lines().find(|l| l.contains("Video:"))?;
    let mut size = None;
    for tok in line.split([' ', ',']) {
        if let Some((w, h)) = tok.split_once('x') {
            if let (Ok(w), Ok(h)) = (w.parse::<u32>(), h.trim_end_matches(|c: char| !c.is_ascii_digit()).parse::<u32>()) {
                if w > 0 && h > 0 && w < 4096 {
                    size = Some((w, h));
                    break;
                }
            }
        }
    }
    let fps = line
        .split(',')
        .find_map(|p| p.trim().strip_suffix(" fps").and_then(|v| v.trim().parse::<f32>().ok()))
        .unwrap_or(15.0);
    size.map(|(w, h)| (w, h, fps))
}

/// Decode a movie fully (blocking). Use [`load_async`] from the UI.
pub fn load(path: &Path) -> Result<Movie, String> {
    let native = load_native(path);
    if native.is_ok() || ffmpeg().is_none() {
        return native;
    }
    load_ffmpeg(path)
}

fn load_ffmpeg(path: &Path) -> Result<Movie, String> {
    let ff = ffmpeg().ok_or("This movie format needs ffmpeg (https://ffmpeg.org) installed or next to Kid Pix Revival.")?;
    let (w, h, fps) = probe_size(&ff, path).ok_or("couldn't read the movie")?;
    let mut child = Command::new(&ff)
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(["-f", "rawvideo", "-pix_fmt", "rgb555be", "-"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut raw = Vec::new();
    child.stdout.take().unwrap().read_to_end(&mut raw).map_err(|e| e.to_string())?;
    let _ = child.wait();
    let fsize = (w * h * 2) as usize;
    let frames: Vec<Vec<u16>> = raw
        .chunks_exact(fsize)
        .map(|c| c.chunks_exact(2).map(|p| u16::from_be_bytes([p[0], p[1]])).collect())
        .collect();
    if frames.is_empty() {
        return Err("the movie has no pictures".into());
    }
    let audio = Command::new(&ff)
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(["-vn", "-f", "s16le", "-ac", "1", "-ar", "22050", "-"])
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success() && !o.stdout.is_empty())
        .map(|o| Sound {
            sample_rate: 22050,
            channels: 1,
            samples: o.stdout.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect(),
            loop_count: 0,
        });
    Ok(Movie { width: w, height: h, fps, frames, audio })
}

/// Decode on a worker thread; poll the receiver from the UI loop.
pub fn load_async(path: PathBuf) -> mpsc::Receiver<Result<Movie, String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(load(&path));
    });
    rx
}

#[cfg(test)]
mod tests {
    /// The native decoder on a CD movie: `game/QTmovies/*.mov` (or KIDPIX_MOVIE=<.mov> with
    /// the CD data two folders up). Skipped without the data.
    #[test]
    fn decode_real_movie() {
        let p = match std::env::var("KIDPIX_MOVIE") {
            Ok(p) => std::path::PathBuf::from(p),
            Err(_) => std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../game/QTmovies/Cows.mov"),
        };
        let p = std::fs::canonicalize(&p).unwrap_or(p);
        let Some(root) = p.parent().and_then(|d| d.parent()) else { return };
        if !p.is_file() || crate::assets::resolve_ci(root, "INSTALL/QT4/QuickTime/QuickTimeAuthoring.qtx").is_none() {
            eprintln!("no CD movie / QuickTimeAuthoring.qtx, skipping");
            return;
        }
        super::set_game_root(root);
        let m = super::load_native(&p).expect("native decode");
        assert!(m.frame_count() > 10);
        assert_eq!((m.width, m.height), (320, 240));
        let f = m.frame(m.frame_count() / 2);
        assert!(f.pixels.iter().any(|&p| p & 0xffffff != 0));
        assert!(m.audio.as_ref().is_some_and(|a| a.channels == 2 && a.sample_rate == 44100));
    }
}
