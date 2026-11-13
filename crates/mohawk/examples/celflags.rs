//! Count tSCR cels with / without the 0x8000 flag per archive.
use mohawk::*;
fn main() {
    let root = std::env::args().nth(1).unwrap();
    let mut files = vec![];
    for e in walkdir(std::path::Path::new(&root)) { files.push(e); }
    for f in files {
        let Ok(a) = Archive::open(&f) else { continue };
        let (mut on, mut off) = (0, 0);
        let mut ex = vec![];
        for id in a.ids(Tag(*b"tSCR")) {
            if let Ok(s) = parse_script(a.get(Tag(*b"tSCR"), id).unwrap()) {
                for op in &s.ops {
                    if let Op::Frame(c) = op {
                        for c in c {
                            if c.visible { on += 1 } else { off += 1; if ex.len() < 6 { ex.push(id) } }
                        }
                    }
                }
            }
        }
        if off > 0 { println!("{}: on {on} off {off} e.g. {ex:?}", f.display()); }
    }
}
fn walkdir(d: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut v = vec![];
    if let Ok(rd) = std::fs::read_dir(d) { for e in rd.flatten() { let p = e.path(); if p.is_dir() { v.extend(walkdir(&p)) } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("mhk")) { v.push(p) } } }
    v
}
