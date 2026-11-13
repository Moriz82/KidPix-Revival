//! First-run import of the user's CD (folder, ISO or ZIP) via the kp-import crate.

use std::path::{Path, PathBuf};

/// Where imported data goes: a `game` folder next to the program when that folder is
/// writable (portable install), otherwise the per-user app data folder.
pub fn import_dest() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let in_cargo_target = dir.components().any(|c| c.as_os_str() == "target");
            if !in_cargo_target && writable(dir) {
                return dir.join("game");
            }
        }
    }
    crate::setup::data_dir().join("game")
}

fn writable(dir: &Path) -> bool {
    let probe = dir.join(".kidpix-write-test");
    let ok = std::fs::write(&probe, b"x").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

pub fn import_archive(source: &Path, dest: &Path) -> Result<PathBuf, String> {
    eprintln!("Importing {} into {} ...", source.display(), dest.display());
    let mut last = u64::MAX;
    let r = kp_import::import(source, dest, &mut |p| {
        let pct = p.done_bytes * 100 / p.total_bytes.max(1);
        if pct != last {
            last = pct;
            eprint!("\r{pct:3}%");
        }
    });
    eprintln!();
    r.map_err(|e| e.to_string())
}
