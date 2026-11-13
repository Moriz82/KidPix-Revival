fn main() {
    let a: Vec<String> = std::env::args().collect();
    let t = kp_qt::svq1::Tables::from_qtx_file(&a[1]).unwrap();
    let m = kp_qt::Movie::decode(&a[2], &t).unwrap();
    let f = &m.frames[a[4].parse::<usize>().unwrap()];
    std::fs::write(&a[3], f.iter().flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8]).collect::<Vec<u8>>()).unwrap();
    println!("{}x{} {} fps {} frames audio {} Hz x{} {} samples", m.width, m.height, m.fps, m.frames.len(), m.audio_rate, m.audio_channels, m.audio.len());
}
