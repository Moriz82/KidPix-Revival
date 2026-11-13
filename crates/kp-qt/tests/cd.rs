//! Against the CD data (`game/QTmovies`, `QuickTimeAuthoring.qtx`); skipped without it.
//! With `KP_QT_REF=<dir>` it also compares the decoded planes with reference YUV 4:1:0
//! dumps `<dir>/<movie>.yuv` (e.g. made with `ffmpeg -pix_fmt yuv410p -f rawvideo`).

use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../game")
}

fn qtx() -> Option<PathBuf> {
    let p = root().join("INSTALL/QT4/QuickTime/QuickTimeAuthoring.qtx");
    if p.is_file() {
        return Some(p);
    }
    std::env::var_os("KP_QTX").map(PathBuf::from).filter(|p| p.is_file())
}

#[test]
fn decodes_cd_movies() {
    let Some(q) = qtx() else {
        eprintln!("no QuickTimeAuthoring.qtx, skipping");
        return;
    };
    let tables = kp_qt::svq1::Tables::from_qtx_file(q).expect("tables");
    let dir = root().join("QTmovies");
    let Ok(rd) = std::fs::read_dir(&dir) else { return };
    let refdir = std::env::var_os("KP_QT_REF").map(PathBuf::from);
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("mov") {
            continue;
        }
        let data = std::fs::read(&p).unwrap();
        let file = kp_qt::mov::File::parse(&data).unwrap();
        let v = file.tracks.iter().find(|t| t.is_video()).unwrap();
        let mut dec = kp_qt::svq1::Decoder::new(&tables);
        let reference = refdir.as_ref().and_then(|d| std::fs::read(d.join(p.with_extension("yuv").file_name().unwrap())).ok());
        let mut mismatched = 0;
        for (i, s) in v.samples.iter().enumerate() {
            let pic = dec.decode(&data[s.offset as usize..(s.offset + s.size) as usize]).unwrap_or_else(|e| panic!("{}: frame {i}: {e}", p.display()));
            if let Some(r) = &reference {
                let (w, h) = (pic.width as usize, pic.height as usize);
                let (cw, ch) = (w.div_ceil(4), h.div_ceil(4));
                let fsize = w * h + 2 * cw * ch;
                if r.len() < (i + 1) * fsize {
                    continue;
                }
                let f = &r[i * fsize..(i + 1) * fsize];
                let mut o = 0;
                for (k, (pw, ph)) in [(w, h), (cw, ch), (cw, ch)].into_iter().enumerate() {
                    let (plane, pitch, _) = &pic.planes[k];
                    for y in 0..ph {
                        if plane[y * pitch..y * pitch + pw] != f[o + y * pw..o + (y + 1) * pw] {
                            mismatched += 1;
                        }
                    }
                    o += pw * ph;
                }
            }
        }
        eprintln!("{}: {} frames, {} mismatched rows", p.display(), v.samples.len(), mismatched);
        if reference.is_some() {
            assert_eq!(mismatched, 0, "{}", p.display());
        }
        if let Some(a) = file.tracks.iter().find(|t| t.is_audio()) {
            let pcm = a.decode_audio(&data).unwrap();
            assert!(pcm.len() as u64 >= a.total_time * a.channels as u64 * 9 / 10, "audio length");
            // reference: raw s16le interleaved at the track's own rate (ffmpeg -f s16le)
            if let Some(r) = refdir.as_ref().and_then(|d| std::fs::read(d.join(p.with_extension("pcm").file_name().unwrap())).ok()) {
                let rs: Vec<i16> = r.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
                let n = rs.len().min(pcm.len());
                let bad = (0..n).filter(|&i| rs[i] != pcm[i]).count();
                eprintln!("  audio {} Hz × {}: {} samples, {} differ from the reference", a.sample_rate, a.channels, n, bad);
                assert_eq!(bad, 0, "{} audio", p.display());
            }
        }
    }
}
