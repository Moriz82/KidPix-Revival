//! Decode every resource of every Mohawk archive under a directory and report failures.
//! usage: census <game-root> [dump-dir]
use mohawk::*;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
            if matches!(ext.to_ascii_lowercase().as_str(), "mhk" | "klb" | "kpx") {
                out.push(p);
            }
        }
    }
}

fn save_png(path: &Path, img: &Image) {
    let f = std::fs::File::create(path).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), img.width, img.height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&img.to_rgba8()).unwrap();
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let root = PathBuf::from(&args[1]);
    let dump = args.get(2).map(PathBuf::from);
    let mut files = Vec::new();
    walk(&root, &mut files);
    files.sort();
    let mut ok: BTreeMap<String, usize> = BTreeMap::new();
    let mut fail: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let t0 = std::time::Instant::now();
    for f in &files {
        let a = match Archive::open(f) {
            Ok(a) => a,
            Err(e) => { fail.entry("archive".into()).or_default().push(format!("{}: {e}", f.display())); continue; }
        };
        let tags: Vec<Tag> = a.tags().collect();
        for tag in tags {
            for id in a.ids(tag) {
                let d = a.get(tag, id).unwrap();
                let key = tag.to_string();
                let res: std::result::Result<Option<Image>, Error> = match &tag.0 {
                    b"tBMP" | b"tTHM" => decode_bitmap(d, None).map(Some),
                    b"tBMH" => decode_bitmap_set(d, None).map(|v| v.into_iter().next()),
                    b"tWAV" => decode_sound(d).map(|_| None),
                    b"tSCR" => parse_script(d).map(|_| None),
                    b"MBTN" => parse_buttons(d).map(|_| None),
                    b"tCUR" => parse_cursor(d).map(|c| Some(c.image)),
                    b"tPAL" => Palette::parse(d).map(|_| None),
                    _ => Ok(None),
                };
                match res {
                    Ok(img) => {
                        *ok.entry(key).or_default() += 1;
                        if let (Some(dir), Some(img)) = (&dump, img) {
                            let name = format!("{}_{}_{}.png", f.file_stem().unwrap().to_string_lossy(), tag, id);
                            save_png(&dir.join(name), &img);
                        }
                    }
                    Err(e) => fail.entry(key).or_default().push(format!("{} {id}: {e}", f.file_name().unwrap().to_string_lossy())),
                }
            }
        }
    }
    println!("{} archives in {:?}", files.len(), t0.elapsed());
    for (k, n) in &ok { println!("ok   {k}: {n}"); }
    for (k, v) in &fail {
        println!("FAIL {k}: {}", v.len());
        for s in v.iter().take(8) { println!("     {s}"); }
    }
}
