//! `qtcmp <qtx> <movie.mov> [ref.yuv]`: decode every frame, report errors and, with a
//! yuv410p reference dump, per-plane mismatches.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let t = kp_qt::svq1::Tables::from_qtx_file(&a[1]).expect("tables");
    let data = std::fs::read(&a[2]).unwrap();
    let f = kp_qt::mov::File::parse(&data).unwrap();
    let v = f.tracks.iter().find(|t| t.is_video()).unwrap();
    let r = a.get(3).and_then(|p| std::fs::read(p).ok());
    let mut d = kp_qt::svq1::Decoder::new(&t);
    for (i, s) in v.samples.iter().enumerate() {
        let b = &data[s.offset as usize..(s.offset + s.size) as usize];
        let ty = u32::from_be_bytes([b[0], b[1], b[2], b[3]]) & 0x3ff;
        match d.decode(b) {
            Err(e) => {
                println!("frame {i}: hdr {:08x} ({ty:x}) error {e}", u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
                if i > 3 { break; }
            }
            Ok(p) => {
                if let Some(r) = &r {
                    let (w, h) = (p.width as usize, p.height as usize);
                    let (cw, ch) = (w.div_ceil(4), h.div_ceil(4));
                    let fs = w * h + 2 * cw * ch;
                    if r.len() < (i + 1) * fs { continue; }
                    let fr = &r[i * fs..(i + 1) * fs];
                    let mut o = 0;
                    let mut msg = String::new();
                    for (k, (pw, ph)) in [(w, h), (cw, ch), (cw, ch)].into_iter().enumerate() {
                        let (pl, pitch, _) = &p.planes[k];
                        let mut bad = 0; let mut first = None; let mut sum = 0i64;
                        for y in 0..ph { for x in 0..pw {
                            let (m, e) = (pl[y * pitch + x] as i64, fr[o + y * pw + x] as i64);
                            if m != e { bad += 1; sum += (m - e).abs(); if first.is_none() { first = Some((x, y, m, e)); } }
                        } }
                        msg += &format!(" p{k}: {bad} bad (avg {:.1}) first {:?};", if bad > 0 { sum as f64 / bad as f64 } else { 0.0 }, first);
                        o += pw * ph;
                    }
                    println!("frame {i}:{msg}");
                }
            }
        }
        if i >= 6 && std::env::var_os("ALL").is_none() { break; }
    }
}
