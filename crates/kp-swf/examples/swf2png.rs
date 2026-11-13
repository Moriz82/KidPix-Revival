//! Render every frame of an SWF movie to PNG files.
//!
//! Usage: swf2png <input.swf> <out_dir> [scale] [--crop]
//!
//! `--crop` renders only the movie's content bounds instead of the whole stage.

use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::time::Instant;

fn write_png(path: &Path, img: &kp_swf::Rgba) -> Result<(), Box<dyn std::error::Error>> {
    let w = BufWriter::new(File::create(path)?);
    let mut enc = png::Encoder::new(w, img.width.max(1), img.height.max(1));
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut wr = enc.write_header()?;
    if img.pixels.is_empty() {
        wr.write_image_data(&[0, 0, 0, 0])?;
    } else {
        wr.write_image_data(&img.pixels)?;
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let crop = args.iter().any(|a| a == "--crop");
    let pos: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    if pos.len() < 2 {
        eprintln!("usage: swf2png <input.swf> <out_dir> [scale] [--crop]");
        std::process::exit(2);
    }
    let input = PathBuf::from(pos[0]);
    let out_dir = PathBuf::from(pos[1]);
    let scale: f32 = pos.get(2).map(|s| s.parse()).transpose()?.unwrap_or(1.0);
    std::fs::create_dir_all(&out_dir)?;

    let data = std::fs::read(&input)?;
    let t = Instant::now();
    let movie = kp_swf::Movie::parse(&data)?;
    let parse_ms = t.elapsed().as_secs_f64() * 1e3;
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "movie".into());
    let rect = if crop {
        movie.content_bounds()
    } else {
        movie.stage()
    };
    eprintln!(
        "{}: SWF v{}, stage {:?}, {} frames @ {} fps, content {:?}, parse {:.2} ms",
        input.display(),
        movie.version(),
        movie.stage(),
        movie.frame_count(),
        movie.frame_rate(),
        movie.content_bounds(),
        parse_ms
    );
    for w in movie.warnings() {
        eprintln!("  warning: {w}");
    }
    let mut total = 0.0;
    for f in 0..movie.frame_count() {
        let t = Instant::now();
        let img = movie.render_frame_rect(f, scale, rect);
        total += t.elapsed().as_secs_f64() * 1e3;
        let name = if movie.frame_count() == 1 {
            format!("{stem}.png")
        } else {
            format!("{stem}_{f:03}.png")
        };
        write_png(&out_dir.join(name), &img)?;
    }
    eprintln!(
        "  rendered {} frame(s), {:.2} ms/frame",
        movie.frame_count(),
        total / movie.frame_count() as f64
    );
    Ok(())
}
