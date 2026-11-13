//! Print an 8-bit tBMP's index values as a coarse grid: idxdump <file> <id> [step]
use mohawk::*;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let ar = Archive::open(&a[1]).unwrap();
    let id: u16 = a[2].parse().unwrap();
    let step: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(4);
    let ix = decode_indexed(ar.get(Tag(*b"tBMP"), id).unwrap()).unwrap();
    println!("{}x{}", ix.width, ix.height);
    for y in (0..ix.height as usize).step_by(step) {
        let row: String = (0..ix.width as usize).step_by(step).map(|x| { let v = ix.indices[y * ix.width as usize + x]; if v == 0 { '.' } else if v == 255 { '#' } else { char::from(b'a' + (v % 26)) } }).collect();
        println!("{y:3} {row}");
    }
}
