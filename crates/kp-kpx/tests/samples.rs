//! Tests against the user's own CD data (skipped when `game/` is absent) and
//! a synthetic page round-trip that always runs.

use kp_kpx::{
    read, write, Image, Object, ObjectKind, Page, PageSound, Rect, StickerPlacement, TextBox, PAGE_HEIGHT,
    PAGE_WIDTH, THUMB_HEIGHT, THUMB_WIDTH,
};
use std::path::{Path, PathBuf};

/// The game data directory: `$KP_GAME_DIR`, else `<repo>/game`.
fn game_dir() -> Option<PathBuf> {
    let dir = std::env::var_os("KP_GAME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../game"));
    dir.is_dir().then_some(dir)
}

fn find_kpx(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = rd.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            find_kpx(&p, out);
        } else if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("kpx")) {
            out.push(p);
        }
    }
}

fn samples() -> Option<Vec<PathBuf>> {
    let Some(dir) = game_dir() else {
        eprintln!("note: no game directory found; skipping .kpx sample tests");
        return None;
    };
    let mut v = Vec::new();
    find_kpx(&dir, &mut v);
    if v.is_empty() {
        eprintln!("note: no .kpx files under {}; skipping", dir.display());
        return None;
    }
    Some(v)
}

#[test]
fn every_sample_parses() {
    let Some(files) = samples() else { return };
    for f in &files {
        let page = kp_kpx::read_file(f).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
        assert_eq!(page.background.image.width, PAGE_WIDTH, "{}", f.display());
        assert_eq!(page.background.image.height, PAGE_HEIGHT, "{}", f.display());
        let thumb = page.thumbnail.as_ref().expect("thumbnail");
        assert_eq!((thumb.image.width, thumb.image.height), (THUMB_WIDTH, THUMB_HEIGHT));
        assert!(page.composite.is_some());
        if let Some(s) = &page.sound {
            let pcm = s.decode().unwrap_or_else(|e| panic!("{}: sound: {e}", f.display()));
            assert_eq!(pcm.sample_rate, 22050);
            assert!(!pcm.samples.is_empty());
        }
    }
    eprintln!("parsed {} .kpx samples", files.len());
}

#[test]
fn every_sample_rewrites_byte_identically() {
    let Some(files) = samples() else { return };
    let mut identical = 0;
    for f in &files {
        let orig = std::fs::read(f).unwrap();
        let page = read(&orig).unwrap();
        let out = write(&page).unwrap();
        if out != orig {
            let at = out.iter().zip(&orig).position(|(a, b)| a != b).unwrap_or(out.len().min(orig.len()));
            panic!(
                "{}: rewrite differs at {at:#x} (len {} vs {})",
                f.display(),
                out.len(),
                orig.len()
            );
        }
        identical += 1;
    }
    eprintln!("{identical}/{} samples rewritten byte-identically", files.len());
}

#[test]
fn modified_sample_still_reads_back() {
    let Some(files) = samples() else { return };
    let mut page = kp_kpx::read_file(&files[0]).unwrap();
    // Paint a red square and add objects: rasters are re-encoded, catalogue rebuilt.
    for y in 100..150 {
        for x in 100..150 {
            page.background.image.put(x, y, 0xFFFF_0000);
        }
    }
    assert!(page.background.is_modified());
    let id = page.add_object(
        ObjectKind::Text(TextBox::new(Rect::new(10, 10, 60, 200), "Hello", "Arial", 18)),
        Rect::new(10, 10, 60, 200),
        0,
    );
    assert_eq!(id, 1);
    let bytes = write(&page).unwrap();
    let back = read(&bytes).unwrap();
    assert_eq!(back.objects, page.objects);
    assert_eq!(back.sound, page.sound);
    let p = back.background.image.get(125, 125).unwrap();
    assert!((p >> 16 & 0xFF) > 200 && (p >> 8 & 0xFF) < 60, "red survived JPEG: {p:#x}");
    // Unmodified rasters keep their original bytes.
    assert_eq!(
        back.thumbnail.as_ref().unwrap().cached_bytes(),
        page.thumbnail.as_ref().unwrap().cached_bytes()
    );
    assert_eq!(write(&back).unwrap(), bytes);
}

fn gradient() -> Image {
    let mut img = Image::new(PAGE_WIDTH, PAGE_HEIGHT);
    for y in 0..PAGE_HEIGHT {
        for x in 0..PAGE_WIDTH {
            let r = x * 255 / PAGE_WIDTH;
            let g = y * 255 / PAGE_HEIGHT;
            img.pixels[(y * PAGE_WIDTH + x) as usize] = 0xFF00_0000 | r << 16 | g << 8 | 0x80;
        }
    }
    img
}

fn max_channel_diff(a: &Image, b: &Image) -> u32 {
    assert_eq!((a.width, a.height), (b.width, b.height));
    a.pixels
        .iter()
        .zip(&b.pixels)
        .flat_map(|(p, q)| (0..4).map(move |k| ((p >> (8 * k)) & 0xFF).abs_diff((q >> (8 * k)) & 0xFF)))
        .max()
        .unwrap_or(0)
}

#[test]
fn fresh_page_round_trips() {
    let mut page = Page::new(gradient());

    let swf = b"FWS\x06\x15\x00\x00\x00\x78\x00\x05\x5F\x00\x00\x0F\xA0\x00\x00\x0C\x01\x00".to_vec();
    let placement = StickerPlacement {
        width: 120,
        height: 90,
        state: 1,
        phase: 3,
    };
    let sid = page.next_id(kp_kpx::T_SWF);
    page.objects.push(Object::sticker(sid, Rect::from_xywh(40, 50, 120, 90), swf.clone(), placement));
    let sid2 = page.add_object(ObjectKind::Sticker(swf), Rect::from_xywh(300, 200, 64, 64), 0);
    assert_eq!((sid, sid2), (1, 2));

    let mut tb = TextBox::new(Rect::new(300, 20, 340, 400), "Hi from Kid Pix – “quotes” é", "Comic Sans MS", 28);
    tb.set_rgb(200, 30, 40);
    tb.justification = 1;
    tb.style = 1;
    page.objects.push(Object::text(1, tb));

    let pcm: Vec<i16> = (0..22050).map(|i| ((i as f32 * 0.02).sin() * 8000.0) as i16).collect();
    page.sound = Some(PageSound::encode(22050, &pcm));

    let mut composite = page.background.image.clone();
    for y in 50..140 {
        for x in 40..160 {
            composite.put(x, y, 0xFF20_40FF);
        }
    }
    page.set_composite(composite.clone());

    let bytes = write(&page).unwrap();
    // Structure is readable by the generic Mohawk reader too.
    let arc = mohawk::Archive::from_bytes(bytes.clone()).unwrap();
    assert_eq!(arc.ids(kp_kpx::T_SWF), vec![1, 2]);
    assert_eq!(arc.ids(kp_kpx::T_TXT), vec![1]);
    assert!(arc.get(kp_kpx::T_WAV, 1).is_some());

    let back = read(&bytes).unwrap();
    assert_eq!(back.objects, page.objects);
    assert_eq!(back.sound, page.sound);
    assert_eq!(back.texts().next().unwrap().1.text, "Hi from Kid Pix – “quotes” é");
    assert_eq!(back.stickers().next().unwrap().sticker_placement(), placement);
    assert!(max_channel_diff(&back.background.image, &page.background.image) <= 12);
    assert!(max_channel_diff(&back.composite.as_ref().unwrap().image, &composite) <= 40);
    let th = &back.thumbnail.as_ref().unwrap().image;
    assert_eq!((th.width, th.height), (THUMB_WIDTH, THUMB_HEIGHT));
    let decoded = back.sound.as_ref().unwrap().decode().unwrap();
    assert_eq!(decoded.samples.len(), pcm.len());

    // A second generation reuses the JPEG bytes and must be byte-identical.
    assert_eq!(write(&back).unwrap(), bytes);
}

#[test]
fn blank_page_has_expected_shape() {
    let page = Page::new_blank();
    let bytes = write(&page).unwrap();
    assert_eq!(&bytes[..4], b"MHWK");
    assert_eq!(&bytes[0x1C..0x24], &[0, 0, 0xFF, 0xFD, 0, 0, 0, 0]);
    let back = read(&bytes).unwrap();
    assert!(back.objects.is_empty() && back.sound.is_none());
    assert_eq!(back.background.image.pixels[0] | 0x0101_0101, 0xFFFF_FFFF);
    assert_eq!(write(&back).unwrap(), bytes);
}
