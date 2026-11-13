//! Decoder tests on hand-built data (no CD needed).

use mohawk::*;

fn be16(v: u16) -> [u8; 2] {
    v.to_be_bytes()
}

#[test]
fn sprite_rle16_runs_literals_and_skips() {
    // 6x2 sprite: row0 = skip1, literal 2 px, skip 1, run 2 px; row1 = missing (transparent)
    let mut d = Vec::new();
    for v in [6u16, 2, 12, 0x33] {
        d.extend(be16(v));
    }
    // row header: dwords, xstart=1
    let body: Vec<u8> = [
        be16(2), be16(1), // literal 2 px, skip 1 after
        be16(0x7c00), be16(0x03e0), // red, green
        be16(0x8002), be16(0), // run 2, skip 0
        be16(0x001f), be16(0x001f), // blue pair
    ]
    .concat();
    d.extend(be16((body.len() / 4) as u16));
    d.extend(be16(1));
    d.extend(body);
    let img = decode_bitmap(&d, None).unwrap();
    assert_eq!((img.width, img.height), (6, 2));
    let p = &img.pixels;
    assert_eq!(p[0], 0); // skipped
    assert_eq!(p[1], 0xffff_0000);
    assert_eq!(p[2], 0xff00_ff00);
    assert_eq!(p[3], 0); // skip after literal
    assert_eq!(p[4], 0xff00_00ff);
    assert_eq!(p[5], 0xff00_00ff);
    assert!(p[6..].iter().all(|&v| v == 0)); // omitted row
}

#[test]
fn rle8_and_palette() {
    let mut d = Vec::new();
    for v in [4u16, 1, 4, 0x12] {
        d.extend(be16(v));
    }
    let row = [0x81u8, 7, 0x01, 1, 2]; // run 2 of 7, literal 2: 1,2
    d.extend(be16(row.len() as u16));
    d.extend(row);
    let mut pal = Vec::new();
    pal.extend(be16(1));
    pal.extend(be16(7));
    for i in 1..8u8 {
        pal.extend([i * 10, 0, 0, 0]);
    }
    let pal = Palette::parse(&pal).unwrap();
    let img = decode_bitmap(&d, Some(&pal)).unwrap();
    assert_eq!(img.pixels, vec![0xff46_0000, 0xff46_0000, 0xff0a_0000, 0xff14_0000]);
}

#[test]
fn lz_literals_and_backref() {
    // "abcabcabc": 3 literals then a back-reference of length 6.
    let mut d = Vec::new();
    d.extend(9u32.to_be_bytes());
    d.extend(0u32.to_be_bytes());
    d.extend(be16(0x400));
    d.push(0b0000_0111); // 3 literals then one reference
    d.extend(b"abc");
    // insert pos after 3 bytes = 3; want back distance 3 -> pos = 0 => offLen + 66 ≡ 0 (mod 1024)
    let len_bits = (6 - 3) << 10;
    let off = (1024 - 66) as u16;
    d.extend(be16(len_bits | off));
    assert_eq!(unpack_lz(&d).unwrap(), b"abcabcabc");
}

#[test]
fn script_ops() {
    let mut d = vec![0u8; 26];
    d[2..4].copy_from_slice(&be16(1000));
    d[16..24].copy_from_slice(&[0, 4, 0, 40, 0, 85, 0, 97]);
    d.extend([4, 4, 0, 100]); // tempo
    d.extend([3, 6, 0, 2, 0, 1]); // sound 2
    d.extend([0x10, 1]);
    d.extend(be16(0x8001));
    d.extend(be16(396));
    d.extend(be16(260));
    d.extend([1, 2, 0, 2]);
    let s = parse_script(&d).unwrap();
    assert_eq!(s.bitmap_set, 1000);
    assert_eq!(s.bounds, Rect { left: 4, top: 40, right: 85, bottom: 97 });
    assert_eq!(s.ops[0], Op::Tempo(100));
    assert_eq!(s.ops[1], Op::Sound { index: 2, flag: 1 });
    assert_eq!(s.frames().len(), 1);
    assert_eq!(s.first_frame()[0], Cel { cel: 1, visible: true, x: 396, y: 260 });
}

#[test]
fn pcm_sound() {
    let mut data = Vec::new();
    data.extend(be16(22050));
    data.extend(2u32.to_be_bytes());
    data.extend([16, 1]);
    data.extend(be16(0));
    data.extend(be16(0));
    data.extend([0u8; 8]);
    data.extend(be16(1000));
    data.extend(be16((-1000i16) as u16));
    let mut d = Vec::new();
    d.extend(b"MHWK");
    d.extend(0u32.to_be_bytes());
    d.extend(b"WAVE");
    d.extend(b"Data");
    d.extend((data.len() as u32).to_be_bytes());
    d.extend(data);
    let s = decode_sound(&d).unwrap();
    assert_eq!(s.sample_rate, 22050);
    assert_eq!(s.samples, vec![1000, -1000]);
}

#[test]
fn archive_roundtrip() {
    // Build a minimal archive with one tBMP resource.
    let payload = b"hello".to_vec();
    let dir = 0x1c;
    let mut b = Vec::new();
    b.extend(b"MHWK");
    b.extend(0u32.to_be_bytes());
    b.extend(b"RSRC");
    b.extend(be16(0x100));
    b.extend(be16(0));
    b.extend(0u32.to_be_bytes());
    b.extend((dir as u32).to_be_bytes());
    let file_table_off = 4 + 8 + 2 + 4 + 2; // after type list, res table and name table
    b.extend(be16(file_table_off));
    b.extend(be16(0));
    // dir
    b.extend(be16(0)); // name list (unused)
    b.extend(be16(1)); // one type
    b.extend(b"tBMP");
    b.extend(be16(12)); // res table off
    b.extend(be16(18)); // name table off
    b.extend(be16(1));
    b.extend(be16(7)); // id 7
    b.extend(be16(1)); // file index 1
    b.extend(be16(0)); // no names
    let data_off = (dir + file_table_off as usize + 4 + 10) as u32;
    b.extend(1u32.to_be_bytes());
    b.extend(data_off.to_be_bytes());
    b.extend(be16(payload.len() as u16));
    b.extend([0, 0]);
    b.extend(be16(0));
    b.extend(&payload);
    let a = Archive::from_bytes(b).unwrap();
    assert_eq!(a.get(Tag(*b"tBMP"), 7), Some(&b"hello"[..]));
    assert_eq!(a.ids(Tag(*b"tBMP")), vec![7]);
}
