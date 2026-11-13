//! List the UI resources of a Mohawk archive: MBTN tables, tSCR bounds, tBMP/tWAV ids.
//! usage: uidump <file.MHK>
use mohawk::*;

fn main() {
    let path = std::env::args().nth(1).expect("usage: uidump <file.MHK>");
    let a = Archive::open(&path).expect("open");
    if let Some(id) = std::env::args().nth(2).and_then(|s| s.parse::<u16>().ok()) {
        let raw = a.get(Tag(*b"tSCR"), id).expect("no such tSCR");
        println!("raw {} bytes: {:02x?}", raw.len(), &raw[..raw.len().min(96)]);
        println!("{:?}", parse_script(raw).map(|s| s.ops));
        return;
    }
    for t in a.tags().collect::<Vec<_>>() {
        let ids = a.ids(t);
        println!("{}: {} ids {:?} range {:?}..{:?}", String::from_utf8_lossy(&t.0), ids.len(), if ids.len() > 40 { &ids[..40] } else { &ids[..] }, ids.first(), ids.last());
    }
    for id in a.ids(Tag(*b"MBTN")) {
        println!("MBTN {id}");
        for b in parse_buttons(a.get(Tag(*b"MBTN"), id).unwrap()).unwrap_or_default() {
            println!("  rest {} over {} press {} cmd {} val {} kind {} grp {} raw {:?}", b.idle, b.hover, b.click, b.command, b.raw[8], b.raw[7], b.group, b.raw);
        }
    }
    for id in a.ids(Tag(*b"tSCR")) {
        if let Ok(s) = parse_script(a.get(Tag(*b"tSCR"), id).unwrap()) {
            let b = s.bounds;
            let frames = s.frames().len();
            let snd: Vec<_> = s.ops.iter().filter_map(|o| if let Op::Sound { index, .. } = o { Some(*index) } else { None }).collect();
            println!("tSCR {id}: set {} rect {},{}-{},{} frames {frames} snd {snd:?}", s.bitmap_set, b.left, b.top, b.right, b.bottom);
        }
    }
}
