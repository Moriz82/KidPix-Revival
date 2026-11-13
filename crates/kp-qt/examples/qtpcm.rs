fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::fs::read(&a[1]).unwrap();
    let f = kp_qt::mov::File::parse(&data).unwrap();
    let t = f.tracks.iter().find(|t| t.is_audio()).unwrap();
    println!("fmt {:?} ch {} bits {} rate {} chunks {} first {:?} frames {:?}", String::from_utf8_lossy(&t.format), t.channels, t.sample_bits, t.sample_rate, t.samples.len(), t.samples.first(), &t.chunk_frames[..3]);
    let pcm = t.decode_audio(&data).unwrap();
    std::fs::write(&a[2], pcm.iter().flat_map(|s| s.to_le_bytes()).collect::<Vec<u8>>()).unwrap();
}
