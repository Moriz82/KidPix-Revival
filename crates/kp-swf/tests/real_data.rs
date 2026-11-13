//! Tests over the user's own sticker/toon movies.
//!
//! Set `KIDPIX_SWF_DIR` to a directory of `.swf` files extracted from the Kid Pix
//! Deluxe 3 CD (sticker and toon libraries). Without it these tests are skipped.

use std::path::PathBuf;

use kp_swf::Movie;

fn movies() -> Option<Vec<(String, Vec<u8>)>> {
    let Some(dir) = std::env::var_os("KIDPIX_SWF_DIR") else {
        eprintln!("KIDPIX_SWF_DIR not set; skipping real-data test");
        return None;
    };
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("KIDPIX_SWF_DIR is not a readable directory")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("swf")))
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "no .swf files in KIDPIX_SWF_DIR");
    Some(
        paths
            .into_iter()
            .map(|p| {
                let name = p.file_name().unwrap().to_string_lossy().into_owned();
                let data = std::fs::read(&p).expect("read swf");
                (name, data)
            })
            .collect(),
    )
}

#[test]
fn every_movie_renders_frame0() {
    let Some(movies) = movies() else { return };
    let mut failures = Vec::new();
    let mut warned = 0;
    for (name, data) in &movies {
        let m = match Movie::parse(data) {
            Ok(m) => m,
            Err(e) => {
                failures.push(format!("{name}: parse error: {e}"));
                continue;
            }
        };
        if !m.warnings().is_empty() {
            warned += 1;
            eprintln!("{name}: warnings {:?}", m.warnings());
        }
        let (x0, y0, x1, y1) = m.stage();
        let img = m.render_frame(0, 1.0);
        let (ew, eh) = (
            (x1 - x0 - 1e-3).ceil() as u32,
            (y1 - y0 - 1e-3).ceil() as u32,
        );
        if (img.width, img.height) != (ew, eh) || img.pixels.len() != (ew * eh * 4) as usize {
            failures.push(format!(
                "{name}: size {}x{} != {ew}x{eh}",
                img.width, img.height
            ));
        }
        if img.opaque_bounds().is_none() {
            failures.push(format!("{name}: frame 0 is empty"));
        }
    }
    eprintln!(
        "{} movies: {} ok, {} failed, {} with warnings",
        movies.len(),
        movies.len() - failures.len(),
        failures.len(),
        warned
    );
    assert!(failures.is_empty(), "failures:\n{}", failures.join("\n"));
}

#[test]
fn content_bounds_are_tight_and_cover_all_frames() {
    let Some(movies) = movies() else { return };
    const PAD: f32 = 3.0;
    let mut failures = Vec::new();
    let mut static_toons = Vec::new();
    for (name, data) in &movies {
        let m = Movie::parse(data).expect("parse");
        let (cx0, cy0, cx1, cy1) = m.content_bounds();
        assert!(cx1 > cx0 && cy1 > cy0, "{name}: empty content bounds");
        let rect = (cx0 - PAD, cy0 - PAD, cx1 + PAD, cy1 + PAD);
        let mut union: Option<(u32, u32, u32, u32)> = None;
        let mut first = None;
        let mut animates = false;
        for f in 0..m.frame_count() {
            let img = m.render_frame_rect(f, 1.0, rect);
            if let Some((x0, y0, x1, y1)) = img.opaque_bounds() {
                // nothing may be drawn outside the bounds (1px slack for rounding)
                let (w, h) = (img.width, img.height);
                if x0 + 1 < PAD as u32
                    || y0 + 1 < PAD as u32
                    || x1 > w - PAD as u32 + 1
                    || y1 > h - PAD as u32 + 1
                {
                    failures.push(format!(
                        "{name} frame {f}: pixels {:?} outside bounds in {w}x{h}",
                        (x0, y0, x1, y1)
                    ));
                }
                union = Some(match union {
                    None => (x0, y0, x1, y1),
                    Some(u) => (u.0.min(x0), u.1.min(y0), u.2.max(x1), u.3.max(y1)),
                });
            }
            match &first {
                None => first = Some(img),
                Some(a) => animates |= *a != img,
            }
        }
        // tightness: drawn pixels reach within ~1.5px of every side
        let w = (rect.2 - rect.0 - 1e-3).ceil() as u32;
        let h = (rect.3 - rect.1 - 1e-3).ceil() as u32;
        match union {
            None => failures.push(format!("{name}: nothing drawn in any frame")),
            Some((x0, y0, x1, y1)) => {
                let p = PAD as u32;
                if x0 > p + 1 || y0 > p + 1 || x1 + p + 1 < w || y1 + p + 1 < h {
                    failures.push(format!(
                        "{name}: bounds {:?} not tight, pixels {:?} in {w}x{h}",
                        m.content_bounds(),
                        (x0, y0, x1, y1)
                    ));
                }
            }
        }
        if m.frame_count() > 1 && !animates {
            static_toons.push(name.clone());
        }
    }
    eprintln!("multi-frame movies with identical frames: {static_toons:?}");
    assert!(failures.is_empty(), "failures:\n{}", failures.join("\n"));
}

#[test]
fn crop_and_repeat_are_consistent() {
    let Some(movies) = movies() else { return };
    for (name, data) in movies.iter().step_by(25) {
        let m = Movie::parse(data).expect("parse");
        let last = m.frame_count() - 1;
        let a = m.render_frame(last, 0.75);
        assert_eq!(a, m.render_frame(last, 0.75), "{name}: not deterministic");
        assert_eq!(
            a,
            m.render_frame_rect(last, 0.75, m.stage()),
            "{name}: crop != full"
        );
    }
}
