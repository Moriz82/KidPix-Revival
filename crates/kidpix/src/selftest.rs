//! Headless smoke test: drive the easel with scripted input and write screenshots.

use crate::assets::Assets;
use crate::audio::Audio;
use crate::easel::Easel;
use crate::gfx::{self, Image};
use crate::trays::G;
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

fn tool(e: &mut Easel, cmd: u16) {
    let (x, y) = e.tool_button(cmd).expect("tool button");
    click(e, x, y);
}

fn pick(e: &mut Easel, g: G, i: usize) {
    if let Some((x, y)) = e.test_tray_pos(g, i) {
        click(e, x, y);
    } else {
        eprintln!("tray item {g:?}/{i} not found");
    }
}

fn settle(e: &mut Easel, ms: f32) {
    let mut t = 0.0;
    while t < ms {
        e.update(50.0);
        t += 50.0;
    }
}

fn assets_movie(e: &Easel) -> Option<std::path::PathBuf> {
    let dir = e.assets_root().join("QTmovies");
    let mut v: Vec<_> = std::fs::read_dir(dir).ok()?.flatten().map(|d| d.path()).collect();
    v.sort();
    v.into_iter().find(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("mov")))
}

pub fn run(assets: Rc<Assets>, out: &Path) {
    std::fs::create_dir_all(out).unwrap();
    let mut e = Easel::new(assets, Rc::new(Audio::silent()));
    settle(&mut e, 2000.0);
    shot(&e, out, "01_easel");

    // Drawing Tools: pencil freehand, filled rectangle, empty oval, polygon, curve, crayon.
    tool(&mut e, 232);
    e.color = gfx::rgb(200, 30, 30);
    drag(&mut e, &wave(150, 80, 200, 16.0));
    pick(&mut e, G::Shape, 3); // rectangle
    pick(&mut e, G::FillMode, 0); // filled
    e.color = gfx::rgb(250, 200, 40);
    drag(&mut e, &[(150, 130), (200, 160), (260, 200)]);
    pick(&mut e, G::Shape, 4); // oval
    pick(&mut e, G::FillMode, 1); // empty
    e.color = gfx::rgb(30, 120, 220);
    drag(&mut e, &[(300, 120), (360, 160), (420, 210)]);
    pick(&mut e, G::Shape, 5); // polygon
    e.color = gfx::rgb(40, 160, 60);
    for p in [(460, 200), (520, 110), (580, 200), (520, 170)] {
        click(&mut e, p.0, p.1);
    }
    click(&mut e, 460, 200);
    pick(&mut e, G::Shape, 1); // curve: drag, bend, click
    e.color = gfx::rgb(150, 40, 200);
    drag(&mut e, &[(160, 260), (300, 260), (440, 260)]);
    e.mouse_move(300, 330);
    click(&mut e, 300, 330);
    pick(&mut e, G::Shape, 0);
    pick(&mut e, G::Variant, 2); // crayon
    pick(&mut e, G::Size, 3);
    e.color = gfx::rgb(240, 90, 20);
    drag(&mut e, &wave(460, 280, 200, 20.0));
    shot(&e, out, "02_drawing_tools");

    // Painting: wacky brushes, a realistic brush, 3D spray, moving paint.
    e.new_picture();
    tool(&mut e, 233);
    pick(&mut e, G::Mode, 1);
    for (i, y) in [(0usize, 90), (1, 140), (2, 190)] {
        pick(&mut e, G::Options, i);
        e.color = gfx::hsv(i as f32 * 90.0, 0.9, 0.9);
        drag(&mut e, &wave(150, y, 250, 12.0));
    }
    pick(&mut e, G::Mode, 0); // realistic
    pick(&mut e, G::Options, 3);
    e.color = gfx::rgb(20, 80, 200);
    drag(&mut e, &wave(150, 250, 250, 12.0));
    pick(&mut e, G::Variant, 2); // spray can
    pick(&mut e, G::Mode, 0); // 3D
    pick(&mut e, G::Options, 0);
    e.mouse_move(500, 120);
    e.mouse_down(500, 120);
    for k in 0..30 {
        e.mouse_move(480 + k * 3, 120 + (k % 7) * 4);
        e.update(40.0);
    }
    e.mouse_up(570, 140);
    pick(&mut e, G::Mode, 1); // moving paint
    pick(&mut e, G::Options, 1);
    e.mouse_move(500, 300);
    e.mouse_down(500, 300);
    for k in 0..25 {
        e.mouse_move(470 + k * 4, 300);
        e.update(40.0);
    }
    e.mouse_up(570, 300);
    e.update(200.0);
    shot(&e, out, "03_painting");
    settle(&mut e, 3000.0); // moving paint flattens

    // Fill buckets: solid, blend (dragging moves it), pattern.
    e.new_picture();
    tool(&mut e, 232);
    pick(&mut e, G::Variant, 0);
    pick(&mut e, G::Size, 2);
    pick(&mut e, G::Shape, 2); // straight line
    drag(&mut e, &[(112, 270), (712, 270)]);
    drag(&mut e, &[(412, 48), (412, 500)]);
    tool(&mut e, 234);
    pick(&mut e, G::Variant, 0);
    e.color = gfx::rgb(250, 220, 60);
    pick(&mut e, G::Options, 0);
    click(&mut e, 200, 120);
    pick(&mut e, G::Variant, 1); // blend
    pick(&mut e, G::Options, 5);
    drag(&mut e, &[(600, 120), (640, 150), (660, 170)]);
    pick(&mut e, G::Variant, 2); // pattern
    pick(&mut e, G::Options, 3);
    click(&mut e, 200, 400);
    pick(&mut e, G::Variant, 0);
    pick(&mut e, G::Options, 6); // single-color pattern
    e.color = gfx::rgb(40, 120, 220);
    click(&mut e, 600, 400);
    shot(&e, out, "04_fill_buckets");

    // Mixers: mini blur over a corner, then mega kaleidoscope.
    tool(&mut e, 237);
    pick(&mut e, G::Variant, 1);
    pick(&mut e, G::Options, 1);
    drag(&mut e, &wave(130, 300, 200, 30.0));
    shot(&e, out, "05_mini_mixer");
    pick(&mut e, G::Variant, 0);
    pick(&mut e, G::Options, 1);
    click(&mut e, 400, 300);
    shot(&e, out, "06_mega_mixer");

    // Backgrounds library.
    e.new_picture();
    click(&mut e, 45, 70);
    settle(&mut e, 400.0);
    shot(&e, out, "07_backgrounds_drawer");
    click(&mut e, 330 + 25, 460);
    click(&mut e, 700, 300);

    // Rubber stamps: next folder, size 2, a stamp.
    tool(&mut e, 236);
    click(&mut e, 270, 552); // next folder
    pick(&mut e, G::Size, 1);
    pick(&mut e, G::Options, 4);
    click(&mut e, 300, 300);
    click(&mut e, 360, 320);
    shot(&e, out, "08_stamps");

    // Scissors: square cut and a cookie cutter, then drag a piece.
    tool(&mut e, 227);
    pick(&mut e, G::Variant, 1);
    pick(&mut e, G::Options, 3);
    drag(&mut e, &[(200, 100), (260, 160), (320, 220)]);
    pick(&mut e, G::Options, 6);
    click(&mut e, 550, 300);
    pick(&mut e, G::Variant, 0);
    drag(&mut e, &[(230, 130), (300, 300), (360, 380)]);
    shot(&e, out, "09_scissors");

    // Original color picker: hover the splotch, it slides up; click a swatch.
    e.mouse_move(40, 560);
    settle(&mut e, 1000.0);
    shot(&e, out, "10_color_picker");
    e.mouse_move(120, 450);
    click(&mut e, 120, 450);
    e.mouse_move(400, 300);

    // Erasers: little round eraser, then the big eraser.
    tool(&mut e, 228);
    pick(&mut e, G::Size, 3);
    drag(&mut e, &wave(130, 100, 400, 20.0));
    shot(&e, out, "11_little_eraser");
    pick(&mut e, G::Variant, 1);
    pick(&mut e, G::Options, 3);
    click(&mut e, 400, 300);
    settle(&mut e, 400.0);
    shot(&e, out, "12_big_eraser");
    settle(&mut e, 1000.0);

    // Undo Guy: undo, click again redoes.
    click(&mut e, 38, 262);
    settle(&mut e, 400.0);
    shot(&e, out, "13_undo");

    // Letters.
    e.new_picture();
    tool(&mut e, 229);
    e.color = gfx::rgb(220, 40, 120);
    for (k, x) in [(0, 300), (1, 380), (2, 460)] {
        pick(&mut e, G::Options, k);
        click(&mut e, x, 300);
    }
    shot(&e, out, "14_letters");

    // Paper texture + chalk.
    e.new_picture();
    let (px, py) = e.tool_button(238).unwrap();
    click(&mut e, px, py);
    click(&mut e, 330 + 51 * 3 + 25, 460);
    tool(&mut e, 232);
    pick(&mut e, G::Variant, 1);
    pick(&mut e, G::Size, 3);
    e.color = gfx::rgb(30, 60, 200);
    for y in [120, 160, 200, 240] {
        drag(&mut e, &wave(150, y, 420, 10.0));
    }
    shot(&e, out, "15_paper_chalk");

    // Stickers and an animation (selected one shows its edit frame).
    e.new_picture();
    click(&mut e, 45, 120);
    settle(&mut e, 250.0);
    click(&mut e, 330 + 25, 460);
    click(&mut e, 250, 200);
    click(&mut e, 330 + 51 * 3 + 25, 460);
    click(&mut e, 480, 220);
    e.mouse_move(480, 220);
    e.wheel(4.0);
    click(&mut e, 45, 160);
    settle(&mut e, 250.0);
    click(&mut e, 330 + 51 + 25, 460);
    click(&mut e, 400, 360);
    settle(&mut e, 4000.0);
    shot(&e, out, "16_stickers");

    // A QuickTime movie from the CD (needs ffmpeg).
    if let Some(mp) = assets_movie(&e) {
        match crate::movie::load(&mp) {
            Ok(m) => {
                e.new_picture();
                e.add_movie(m);
                click(&mut e, 412, 274);
                let (x, y) = e.tool_button(243).unwrap();
                click(&mut e, x, y);
                settle(&mut e, 3000.0);
                shot(&e, out, "17_movie");
                let (x, y) = e.tool_button(244).unwrap();
                click(&mut e, x, y);
            }
            Err(err) => eprintln!("movie skipped: {err}"),
        }
    }

    // Original hidden menu bar: press at the very top, drag down into the Toolbox menu.
    e.mouse_move(150, 1);
    e.mouse_down(150, 1);
    e.mouse_move(150, 10);
    e.mouse_move(150, 60);
    shot(&e, out, "18_menu");
    e.mouse_up(500, 300);
    e.update(50.0);

    // Slide show.
    let (x, y) = e.tool_button(270).unwrap();
    click(&mut e, x, y);
    click(&mut e, 100, 210);
    e.new_picture_for_test();
    click(&mut e, 100, 210);
    click(&mut e, 44 + 6 * 72 + 30, 262 + 30);
    shot(&e, out, "19_slideshow");
    click(&mut e, 630, 210);
    settle(&mut e, 3250.0);
    shot(&e, out, "20_slideshow_transition");
    e.key_escape();
    e.key_escape();
}
