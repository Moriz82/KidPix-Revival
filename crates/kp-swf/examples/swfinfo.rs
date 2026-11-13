//! Feature census over a directory of SWF files (or single files).
//!
//! Usage: swfinfo <dir-or-file>... [-v]

use std::collections::BTreeMap;
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let verbose = args.iter().any(|a| a == "-v");
    let mut files: Vec<PathBuf> = Vec::new();
    for a in args.iter().filter(|a| !a.starts_with('-')) {
        let p = PathBuf::from(a);
        if p.is_dir() {
            let mut v: Vec<PathBuf> = std::fs::read_dir(&p)
                .expect("read dir")
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("swf")))
                .collect();
            v.sort();
            files.extend(v);
        } else {
            files.push(p);
        }
    }
    let mut versions = BTreeMap::new();
    let mut tags: BTreeMap<u16, (usize, usize)> = BTreeMap::new();
    let mut stats = kp_swf::ShapeStats::default();
    let mut files_with = BTreeMap::<&str, usize>::new();
    let mut failed = 0;
    let mut warned = 0;
    let (mut min_stage, mut max_stage) = ((f32::MAX, f32::MAX), (0f32, 0f32));
    let mut rates = BTreeMap::new();
    let mut frames = BTreeMap::new();
    for f in &files {
        let data = std::fs::read(f).expect("read");
        let m = match kp_swf::Movie::parse(&data) {
            Ok(m) => m,
            Err(e) => {
                failed += 1;
                println!("FAIL {}: {e}", f.display());
                continue;
            }
        };
        *versions.entry(m.version()).or_insert(0) += 1;
        *rates.entry(format!("{}", m.frame_rate())).or_insert(0) += 1;
        let fc = m.frame_count();
        *frames.entry(if fc > 10 { 99 } else { fc }).or_insert(0) += 1;
        let (x0, y0, x1, y1) = m.stage();
        let (w, h) = (x1 - x0, y1 - y0);
        min_stage = (min_stage.0.min(w), min_stage.1.min(h));
        max_stage = (max_stage.0.max(w), max_stage.1.max(h));
        if !m.warnings().is_empty() {
            warned += 1;
            for w in m.warnings() {
                println!("WARN {}: {w}", f.display());
            }
        }
        for (&c, &n) in m.tag_counts() {
            let e = tags.entry(c).or_default();
            e.0 += n;
            e.1 += 1;
        }
        let s = m.shape_stats();
        let pairs = [
            ("linear gradients", s.linear_gradients),
            ("radial gradients", s.radial_gradients),
            ("focal gradients", s.focal_gradients),
            ("bitmap fills", s.bitmap_fills),
            ("new style groups", s.new_style_groups),
            ("line styles", s.line_styles),
            ("curved edges", s.curved_edges),
        ];
        for (k, v) in pairs {
            if v > 0 {
                *files_with.entry(k).or_default() += 1;
                if verbose && k != "line styles" && k != "curved edges" {
                    println!("{}: {k} x{v}", f.display());
                }
            }
        }
        stats.solid_fills += s.solid_fills;
        stats.linear_gradients += s.linear_gradients;
        stats.radial_gradients += s.radial_gradients;
        stats.focal_gradients += s.focal_gradients;
        stats.bitmap_fills += s.bitmap_fills;
        stats.line_styles += s.line_styles;
        stats.new_style_groups += s.new_style_groups;
        stats.curved_edges += s.curved_edges;
        stats.straight_edges += s.straight_edges;
    }
    println!(
        "files: {} (parse failures {failed}, with warnings {warned})",
        files.len()
    );
    println!("versions: {versions:?}");
    println!("frame counts (99 = >10): {frames:?}");
    println!("frame rates: {rates:?}");
    println!("stage size range: {min_stage:?} .. {max_stage:?}");
    println!("tags (code: count / files): {tags:?}");
    println!("shape stats total: {stats:?}");
    println!("files using: {files_with:?}");
}
