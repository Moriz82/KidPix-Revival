//! Contact sheet of every cel of a tBMH set: celsheet <file.MHK> <set id> <out.png>
use mohawk::*;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let ar = Archive::open(&a[1]).unwrap();
    let id: u16 = a[2].parse().unwrap();
    let cels = decode_bitmap_set(ar.get(Tag(*b"tBMH"), id).unwrap(), None).unwrap();
    for (i, c) in cels.iter().enumerate() { if c.width <= 40 && c.height <= 40 { println!("cel {} {}x{}", i + 1, c.width, c.height); } }
    let cols = 16; let cw = 110; let ch = 90;
    let rows = (cels.len() + cols - 1) / cols;
    let mut out = Image::filled((cols * cw) as u32, (rows * ch) as u32, 0xff808080);
    for (i, c) in cels.iter().enumerate() {
        let (ox, oy) = ((i % cols) * cw, (i / cols) * ch);
        for y in 0..c.height.min(ch as u32 - 2) { for x in 0..c.width.min(cw as u32 - 2) {
            let p = c.pixels[(y * c.width + x) as usize];
            if p >> 24 > 0 { out.pixels[((oy as u32 + y) * out.width + ox as u32 + x) as usize] = p | 0xff000000; }
        }}
    }
    let f = std::fs::File::create(&a[3]).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), out.width, out.height);
    enc.set_color(png::ColorType::Rgba); enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&out.to_rgba8()).unwrap();
}
