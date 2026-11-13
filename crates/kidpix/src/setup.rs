//! Finding (or importing) the user's own copy of the Kid Pix Deluxe 3 CD.

use crate::assets::is_game_root;
use std::path::{Path, PathBuf};

/// App data folder, e.g. `~/.local/share/KidPixRevival`.
pub fn data_dir() -> PathBuf {
    directories::ProjectDirs::from("", "", "KidPixRevival")
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("KidPixRevival"))
}

fn imported_root() -> PathBuf {
    data_dir().join("game")
}

/// Use a folder in place when it already is a game root; otherwise import it.
fn use_or_import(source: &Path) -> Result<PathBuf, String> {
    if source.is_dir() {
        if let Some(r) = find_root(source) {
            return Ok(r);
        }
    }
    let dest = crate::importer::import_dest();
    let r = crate::importer::import_archive(source, &dest)?;
    rfd::MessageDialog::new()
        .set_title("Kid Pix Revival")
        .set_description(format!(
            "Your Kid Pix Deluxe 3 files were copied to\n{}\n\nThe CD or CD image isn't needed any more.",
            r.display()
        ))
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
    Ok(r)
}

/// Look inside `dir` and its immediate children for a game root.
fn find_root(dir: &Path) -> Option<PathBuf> {
    if is_game_root(dir) {
        return Some(dir.to_path_buf());
    }
    let rd = std::fs::read_dir(dir).ok()?;
    rd.flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .find(|p| is_game_root(p))
}

fn force_import(source: &Path) -> Result<PathBuf, String> {
    let dest = crate::importer::import_dest();
    crate::importer::import_archive(source, &dest)
}

/// Resolve the game root from CLI arguments, environment, previous imports or by asking.
pub fn locate(data: Option<PathBuf>, import_src: Option<PathBuf>) -> Option<PathBuf> {
    if let Some(src) = import_src {
        match force_import(&src) {
            Ok(r) => return Some(r),
            Err(e) => {
                eprintln!("error: {e}");
                return None;
            }
        }
    }
    if let Some(d) = data {
        return match find_root(&d) {
            Some(r) => Some(r),
            None => {
                eprintln!("error: no Kid Pix Deluxe 3 data in {}", d.display());
                None
            }
        };
    }
    if let Ok(d) = std::env::var("KIDPIX_DATA") {
        if let Some(r) = find_root(Path::new(&d)) {
            return Some(r);
        }
    }
    let mut candidates = vec![imported_root()];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("game"));
            candidates.push(dir.to_path_buf());
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("game"));
        candidates.push(cwd);
    }
    for c in candidates {
        if let Some(r) = find_root(&c) {
            return Some(r);
        }
    }
    ask_user()
}

fn ask_user() -> Option<PathBuf> {
    loop {
        let res = rfd::MessageDialog::new()
            .set_title("Kid Pix Revival")
            .set_description(
                "Kid Pix Revival plays the pictures, sounds and tools from your own \
                 Kid Pix Deluxe 3 CD.\n\nChoose your CD image (.iso or .zip) — or press \
                 No to pick a folder instead (a CD, a copy of it, or an installed Kid Pix Deluxe 3).",
            )
            .set_buttons(rfd::MessageButtons::YesNoCancel)
            .show();
        let pick = match res {
            rfd::MessageDialogResult::Yes => rfd::FileDialog::new()
                .set_title("Choose your Kid Pix Deluxe 3 CD image")
                .add_filter("CD image", &["iso", "zip", "ISO", "ZIP"])
                .pick_file(),
            rfd::MessageDialogResult::No => rfd::FileDialog::new()
                .set_title("Choose your Kid Pix Deluxe 3 folder")
                .pick_folder(),
            _ => return None,
        };
        let Some(p) = pick else { continue };
        match use_or_import(&p) {
            Ok(r) => return Some(r),
            Err(e) => {
                rfd::MessageDialog::new()
                    .set_title("Kid Pix Revival")
                    .set_description(e)
                    .set_buttons(rfd::MessageButtons::Ok)
                    .show();
            }
        }
    }
}
