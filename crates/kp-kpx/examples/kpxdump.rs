//! Print the structure of `.kpx` files and optionally export their rasters.
//!
//! ```text
//! cargo run -p kp-kpx --example kpxdump -- [--png <dir>] [--check] <file.kpx>...
//! ```
//!
//! `--png <dir>` writes `<stem>.background.png`, `<stem>.composite.png` and
//! `<stem>.thumbnail.png`; `--check` verifies that a rewrite is byte-identical.

use kp_kpx::{Image, ObjectKind, Page};
use std::path::{Path, PathBuf};

fn save_png(img: &Image, path: &Path) -> std::io::Result<()> {
    let file = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut enc = png::Encoder::new(file, img.width, img.height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header().map_err(std::io::Error::other)?;
    w.write_image_data(&img.to_rgba8()).map_err(std::io::Error::other)?;
    Ok(())
}

fn dump(path: &Path, page: &Page) {
    println!("{}", path.display());
    let raster = |name: &str, r: Option<&kp_kpx::Raster>| match r {
        Some(r) => println!(
            "  {name:<10} {}x{}  {} bytes",
            r.image.width,
            r.image.height,
            r.cached_bytes().map_or(0, |b| b.len())
        ),
        None => println!("  {name:<10} -"),
    };
    raster("background", Some(&page.background));
    raster("composite", page.composite.as_ref());
    raster("thumbnail", page.thumbnail.as_ref());
    match &page.sound {
        Some(s) => match s.decode() {
            Ok(pcm) => println!(
                "  sound      {} bytes, {} Hz, {} ch, {:.2} s, rect {:?}",
                s.data.len(),
                pcm.sample_rate,
                pcm.channels,
                pcm.duration_secs(),
                s.rect
            ),
            Err(e) => println!("  sound      {} bytes (undecodable: {e})", s.data.len()),
        },
        None => println!("  sound      -"),
    }
    println!("  objects    {}", page.objects.len());
    for o in &page.objects {
        let r = o.rect;
        print!(
            "    {} {:<3} rect t{} l{} b{} r{} extra {:#010x}",
            o.tag(),
            o.id,
            r.top,
            r.left,
            r.bottom,
            r.right,
            o.extra
        );
        match &o.kind {
            ObjectKind::Sticker(swf) => {
                let p = o.sticker_placement();
                println!(
                    "  SWF {} bytes, size {:?}, state {}, phase {}",
                    swf.len(),
                    p.size(),
                    p.state,
                    p.phase
                );
            }
            ObjectKind::Text(t) => println!(
                "  \"{}\" font {:?} {}pt style {:#x} just {} colour {:?}",
                t.text.replace('\r', "\\r").replace('\n', "\\n"),
                t.face_name(),
                t.size,
                t.style,
                t.justification,
                t.color
            ),
            ObjectKind::Bitmap(d) | ObjectKind::Movie(d) => println!("  {} bytes", d.len()),
            ObjectKind::Other { data, .. } => println!("  {} bytes (raw)", data.len()),
        }
    }
    for r in &page.extra_resources {
        println!("  uncatalogued {} {} ({} bytes)", r.tag, r.id, r.data.len());
    }
}

fn main() {
    let mut png_dir: Option<PathBuf> = None;
    let mut check = false;
    let mut files = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--png" => png_dir = args.next().map(PathBuf::from),
            "--check" => check = true,
            "-h" | "--help" => {
                eprintln!("usage: kpxdump [--png <dir>] [--check] <file.kpx>...");
                return;
            }
            _ => files.push(PathBuf::from(a)),
        }
    }
    if files.is_empty() {
        eprintln!("usage: kpxdump [--png <dir>] [--check] <file.kpx>...");
        std::process::exit(2);
    }
    let mut failed = false;
    for f in &files {
        let bytes = match std::fs::read(f) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("{}: {e}", f.display());
                failed = true;
                continue;
            }
        };
        let page = match kp_kpx::read(&bytes) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("{}: {e}", f.display());
                failed = true;
                continue;
            }
        };
        dump(f, &page);
        if check {
            match kp_kpx::write(&page) {
                Ok(out) if out == bytes => println!("  rewrite    byte-identical"),
                Ok(out) => {
                    let at = out.iter().zip(&bytes).position(|(a, b)| a != b);
                    println!("  rewrite    DIFFERS (first at {at:?}, len {} vs {})", out.len(), bytes.len());
                    failed = true;
                }
                Err(e) => {
                    println!("  rewrite    failed: {e}");
                    failed = true;
                }
            }
        }
        if let Some(dir) = &png_dir {
            if let Err(e) = std::fs::create_dir_all(dir) {
                eprintln!("{}: {e}", dir.display());
                std::process::exit(1);
            }
            let stem = f.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            let mut out = vec![("background", &page.background.image)];
            if let Some(c) = &page.composite {
                out.push(("composite", &c.image));
            }
            if let Some(t) = &page.thumbnail {
                out.push(("thumbnail", &t.image));
            }
            for (what, img) in out {
                let p = dir.join(format!("{stem}.{what}.png"));
                if let Err(e) = save_png(img, &p) {
                    eprintln!("{}: {e}", p.display());
                    failed = true;
                }
            }
        }
    }
    if failed {
        std::process::exit(1);
    }
}
