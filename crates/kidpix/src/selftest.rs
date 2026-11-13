//! Headless smoke test: drive the easel with scripted input and write screenshots.

use crate::assets::Assets;
use crate::audio::Audio;
use crate::easel::Easel;
use crate::gfx::Image;
use std::path::Path;
use std::rc::Rc;

fn shot(e: &Easel, out: &Path, name: &str) {
    let mut frame = Image::new(800, 600);
    e.draw(&mut frame);
    let p = out.join(format!("{name}.png"));
    crate::save_png(&p, &frame).expect("write png");
    eprintln!("wrote {}", p.display());
}

fn click(e: &mut Easel, x: i32, y: i32) {
    e.mouse_move(x, y);
    e.mouse_down(x, y);
    e.mouse_up(x, y);
    e.update(50.0);
}

fn drag(e: &mut Easel, pts: &[(i32, i32)]) {
    e.mouse_move(pts[0].0, pts[0].1);
    e.mouse_down(pts[0].0, pts[0].1);
    for &(x, y) in &pts[1..] {
        e.mouse_move(x, y);
        e.update(16.0);
    }
    let l = pts[pts.len() - 1];
    e.mouse_up(l.0, l.1);
}

fn wave(x0: i32, y0: i32, len: i32, amp: f32) -> Vec<(i32, i32)> {
    (0..=len / 4).map(|i| (x0 + i * 4, y0 + ((i as f32 / 5.0).sin() * amp) as i32)).collect()
}

pub fn run(assets: Rc<Assets>, out: &Path) {
    std::fs::create_dir_all(out).unwrap();
    let mut e = Easel::new(assets, Rc::new(Audio::silent()));
    for _ in 0..40 {
        e.update(50.0);
    }
    shot(&e, out, "01_easel");
    // Pencil line (current tool), then wacky brushes with different options.
    e.color = crate::gfx::rgb(200, 30, 30);
    drag(&mut e, &wave(150, 100, 200, 20.0));
    click(&mut e, 70, 310); // wacky brush button
    for (i, y) in [(0usize, 150), (1, 200), (2, 250), (3, 300)] {
        click(&mut e, 322 + 56 * i as i32 + 20, 550);
        e.color = crate::gfx::hsv(i as f32 * 90.0, 0.9, 0.9);
        drag(&mut e, &wave(400, y, 250, 15.0));
    }
    shot(&e, out, "02_brushes");
    // Paint can.
    click(&mut e, 30, 365);
    e.color = crate::gfx::rgb(250, 220, 60);
    click(&mut e, 200, 420);
    shot(&e, out, "03_fill");
    // Backgrounds drawer.
    click(&mut e, 45, 70);
    for _ in 0..10 {
        e.update(50.0);
    }
    shot(&e, out, "04_backgrounds_drawer");
    click(&mut e, 330 + 25, 460);
    shot(&e, out, "05_background_picked");
    // Stamps.
    click(&mut e, 30, 418);
    for _ in 0..10 {
        e.update(50.0);
    }
    click(&mut e, 330 + 51 * 2 + 25, 460);
    click(&mut e, 300, 300);
    click(&mut e, 360, 320);
    shot(&e, out, "06_stamps");
    // Palette overlay.
    click(&mut e, 40, 560);
    shot(&e, out, "07_palette");
    click(&mut e, 40, 340);
    // Mixer kaleidoscope.
    click(&mut e, 72, 365);
    click(&mut e, 322 + 56 + 20, 550);
    shot(&e, out, "08_mixer");
    // Undo face.
    click(&mut e, 38, 262);
    for _ in 0..10 {
        e.update(80.0);
    }
    shot(&e, out, "09_undo");
    // Original gradient / pattern fills and alphabet stamps.
    e.new_picture();
    click(&mut e, 30, 365); // paint can
    e.color = crate::gfx::rgb(40, 90, 220);
    drag(&mut e, &[(160, 270), (660, 270)]); // split canvas with a pencil? (paint can ignores drags)
    click(&mut e, 22, 312); // pencil
    click(&mut e, 322 + 56 * 2 + 20, 550); // thick pencil
    drag(&mut e, &[(112, 270), (712, 270)]);
    drag(&mut e, &[(412, 48), (412, 500)]);
    click(&mut e, 30, 365); // paint can
    click(&mut e, 322 + 34 + 16, 521 + 16); // first gradient
    click(&mut e, 200, 120);
    click(&mut e, 322 + 34 * 3 + 16, 521 + 34 + 16); // a later icon (row 2)
    click(&mut e, 600, 120);
    click(&mut e, 712 - 10, 527 + 40); // next page
    click(&mut e, 712 - 10, 527 + 40);
    click(&mut e, 322 + 34 * 5 + 16, 521 + 16);
    click(&mut e, 200, 400);
    shot(&e, out, "11_fills");
    click(&mut e, 72, 418); // letters (ABC)
    for _ in 0..1 {
        click(&mut e, 712 - 10, 527 + 40);
    }
    e.color = crate::gfx::rgb(220, 40, 120);
    for (k, x) in [(0, 470), (1, 540), (2, 610)] {
        click(&mut e, 322 + 34 * (6 + k) + 16, 521 + 16);
        click(&mut e, x, 400);
    }
    shot(&e, out, "12_letters");
    // Slide show: add two pictures, pick transitions, play.
    click(&mut e, 770, 575); // slide show button
    click(&mut e, 100, 210); // add this picture
    e.new_picture_for_test();
    click(&mut e, 100, 210);
    click(&mut e, 44 + 6 * 72 + 30, 262 + 30); // iris circle
    click(&mut e, 44 + 2 * 120 + 50, 466 + 8); // a sound
    shot(&e, out, "13_slideshow_editor");
    click(&mut e, 630, 210); // play
    for _ in 0..((2600 + 650) / 50) {
        e.update(50.0);
    }
    shot(&e, out, "14_slideshow_transition");
    e.key_escape();
    e.key_escape();
    // Stickers and an animation.
    e.new_picture();
    click(&mut e, 45, 120); // stickers oval
    for _ in 0..5 {
        e.update(50.0);
    }
    click(&mut e, 330 + 51 * 0 + 25, 460);
    click(&mut e, 250, 200);
    click(&mut e, 330 + 51 * 3 + 25, 460);
    click(&mut e, 480, 220);
    click(&mut e, 45, 160); // animations oval
    for _ in 0..5 {
        e.update(50.0);
    }
    click(&mut e, 330 + 51 * 1 + 25, 460);
    click(&mut e, 400, 360);
    e.mouse_move(480, 220);
    e.wheel(4.0); // grow the second sticker
    for _ in 0..80 {
        e.update(50.0); // let the 3.5 s hint disappear
    }
    shot(&e, out, "15_stickers");
    // Menus.
    click(&mut e, 10, 8);
    e.mouse_move(40, 50);
    shot(&e, out, "10_menu");
}
