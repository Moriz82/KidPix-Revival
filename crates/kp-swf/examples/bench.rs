//! Timing over a directory of SWF files: parse, content bounds and per-frame render.
//!
//! Usage: bench <dir> [scale]

use std::time::Instant;

fn pct(v: &mut [f64], p: f64) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[((v.len() - 1) as f64 * p).round() as usize]
}

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = args.next().expect("usage: bench <dir> [scale]");
    let scale: f32 = args.next().map_or(1.0, |s| s.parse().expect("scale"));
    let mut paths: Vec<_> = std::fs::read_dir(&dir)
        .expect("read dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "swf"))
        .collect();
    paths.sort();
    let (mut parse, mut bounds, mut frames) = (vec![], vec![], vec![]);
    let mut slowest = (0.0f64, String::new());
    for p in &paths {
        let data = std::fs::read(p).expect("read");
        let t = Instant::now();
        let m = kp_swf::Movie::parse(&data).expect("parse");
        parse.push(t.elapsed().as_secs_f64() * 1e3);
        let t = Instant::now();
        let _ = m.content_bounds();
        let ms = t.elapsed().as_secs_f64() * 1e3;
        bounds.push(ms);
        if ms > slowest.0 {
            slowest = (ms, format!("{} ({} frames)", p.display(), m.frame_count()));
        }
        for f in 0..m.frame_count() {
            let t = Instant::now();
            let img = m.render_frame(f, scale);
            frames.push(t.elapsed().as_secs_f64() * 1e3);
            std::hint::black_box(img);
        }
    }
    println!("slowest content_bounds: {:.1} ms {}", slowest.0, slowest.1);
    for (name, v) in [
        ("parse", &mut parse),
        ("content_bounds", &mut bounds),
        ("render_frame", &mut frames),
    ] {
        let n = v.len();
        let sum: f64 = v.iter().sum();
        println!(
            "{name:15} n={n:5} mean {:.2} ms  p50 {:.2}  p90 {:.2}  max {:.2}",
            sum / n as f64,
            pct(v, 0.5),
            pct(v, 0.9),
            pct(v, 1.0)
        );
    }
}
