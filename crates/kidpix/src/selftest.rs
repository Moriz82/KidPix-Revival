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

/// Drag library item `k` (visible slot) from the bottom panel onto the page.
fn lib_drag(e: &mut Easel, k: usize, to: (i32, i32)) {
    let Some((x, y)) = e.test_lib_slot(k) else {
        eprintln!("library slot {k} not found");
        return;
    };
    drag(e, &[(x, y), ((x + to.0) / 2, (y + to.1) / 2), to]);
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

/// Set while the headless self-test runs: random seeds become fixed so screenshots repeat.
pub static TEST_MODE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn run(assets: Rc<Assets>, out: &Path) {
    TEST_MODE.store(true, std::sync::atomic::Ordering::Relaxed);
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
    pick(&mut e, G::Size, 2);
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
    lib_drag(&mut e, 0, (400, 250));
    shot(&e, out, "07_backgrounds_drawer");

    // Rubber stamps: next folder, size 2, a stamp.
    tool(&mut e, 236);
    if let Some((x, y)) = e.panel_button(18704) {
        click(&mut e, x, y); // next folder
    }
    pick(&mut e, G::Size, 1);
    pick(&mut e, G::Options, 4);
    click(&mut e, 300, 300);
    click(&mut e, 360, 320);
    e.mouse_move(500, 250);
    shot(&e, out, "08_stamps");
    if let Some((x, y)) = e.panel_button(18702) {
        click(&mut e, x, y);
        e.mouse_move(180, 400);
        shot(&e, out, "08b_stamp_library_popup");
        click(&mut e, 180, 400);
    }
    if let Some((x, y)) = e.panel_button(18701) {
        click(&mut e, x, y);
        click(&mut e, 250, 300); // palette: a colour
        drag(&mut e, &[(330, 135), (420, 200), (500, 260)]);
        shot(&e, out, "08c_stamp_editor");
        click(&mut e, 500, 404); // CANCEL
    }

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

    // Original colour picker: press the wheel, drag onto a cell, release.
    e.mouse_move(40, 560);
    e.mouse_down(40, 560);
    settle(&mut e, 600.0);
    e.mouse_move(120, 450);
    e.update(50.0);
    shot(&e, out, "10_color_picker");
    e.mouse_up(120, 450);
    settle(&mut e, 700.0);
    e.mouse_move(400, 300);

    // Erasers: little round eraser, then the big eraser.
    tool(&mut e, 228);
    pick(&mut e, G::Size, 2);
    drag(&mut e, &wave(130, 100, 400, 20.0));
    shot(&e, out, "11_little_eraser");
    pick(&mut e, G::Variant, 1);
    pick(&mut e, G::Options, 3);
    click(&mut e, 400, 300);
    settle(&mut e, 400.0);
    shot(&e, out, "12_big_eraser");
    settle(&mut e, 1000.0);

    // Print dialog (DIALOG 150): choose Poster 3 x 3, then cancel.
    let (x, y) = e.tool_button(242).unwrap();
    click(&mut e, x, y);
    click(&mut e, 290, 266);
    click(&mut e, 314, 308);
    shot(&e, out, "12b_print_dialog");
    click(&mut e, 450, 351);
    let _ = std::mem::take(&mut e.actions);

    // Import Background options (DIALOG 134).
    let small = gfx::resize(&e.flattened(), 200, 120);
    e.import_picture(small, crate::easel::FitTarget::Background);
    click(&mut e, 200, 254); // "Repeat Many Times"
    shot(&e, out, "12c_import_background");
    click(&mut e, 552, 397); // Cancel

    // Undo Guy: undo, click again redoes.
    click(&mut e, 38, 262);
    settle(&mut e, 400.0);
    shot(&e, out, "13_undo");

    // Text boxes: type, restyle from the panel, move one with the glove.
    e.new_picture();
    tool(&mut e, 229);
    e.color = gfx::rgb(220, 40, 120);
    click(&mut e, 200, 150);
    for c in "Kid Pix is fun!".chars() {
        e.key_char(c);
    }
    for cmd in [202, 202, 202, 207] {
        if let Some((x, y)) = e.panel_button(cmd) {
            click(&mut e, x, y);
        }
    }
    click(&mut e, 250, 330);
    for cmd in [201, 201, 208, 209, 205] {
        if let Some((x, y)) = e.panel_button(cmd) {
            click(&mut e, x, y);
        }
    }
    for c in "Hello\nthere".chars() {
        e.key_char(c);
    }
    e.update(10.0);
    for t in &e.texts {
        eprintln!("text box {:?} {:?} font {} size {}", (t.x, t.y), t.text, t.font, t.size);
    }
    shot(&e, out, "14_letters");
    // Small Kids Mode: alphabet stamps with voices.
    e.menu_command(crate::menubar::cmd::SMALL_KIDS);
    e.new_picture();
    tool(&mut e, 229);
    for (k, x) in [(0usize, 250), (8, 330), (3, 410)] {
        pick(&mut e, G::Options, k);
        click(&mut e, x, 250);
    }
    e.mouse_move(480, 250);
    shot(&e, out, "14b_small_kids_letters");
    e.menu_command(crate::menubar::cmd::SMALL_KIDS);
    // Paper texture + chalk.
    e.new_picture();
    let (px, py) = e.tool_button(238).unwrap();
    click(&mut e, px, py);
    click(&mut e, 330 + 51 * 3 + 25, 460);
    tool(&mut e, 232);
    pick(&mut e, G::Variant, 1);
    pick(&mut e, G::Size, 2);
    e.color = gfx::rgb(30, 60, 200);
    for y in [120, 160, 200, 240] {
        drag(&mut e, &wave(150, y, 420, 10.0));
    }
    shot(&e, out, "15_paper_chalk");

    // Stickers and an animation (selected one shows its edit frame).
    e.new_picture();
    click(&mut e, 45, 120);
    settle(&mut e, 250.0);
    lib_drag(&mut e, 0, (250, 200));
    lib_drag(&mut e, 3, (480, 220));
    e.mouse_move(480, 220);
    e.wheel(4.0);
    click(&mut e, 45, 160);
    settle(&mut e, 250.0);
    lib_drag(&mut e, 1, (400, 360));
    settle(&mut e, 4000.0);
    shot(&e, out, "16_stickers");

    // A QuickTime movie from the CD (native Sorenson Video decoder).
    if let Some(mp) = assets_movie(&e) {
        match crate::movie::load(&mp) {
            Ok(m) => {
                eprintln!("movie: {}x{} {} frames, {} fps, sound {:?}", m.width, m.height, m.frame_count(), m.fps, m.audio.as_ref().map(|a| (a.sample_rate, a.channels)));
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

    // Later shots start from a fresh page.
    e.new_picture();

    // Original hidden menu bar: press at the very top, drag down into the Toolbox menu.
    e.mouse_move(150, 1);
    e.mouse_down(150, 1);
    e.mouse_move(150, 10);
    e.mouse_move(150, 60);
    shot(&e, out, "18_menu");
    e.mouse_up(500, 300);
    e.update(50.0);

    run_shell(e.assets_rc(), out);
    run_dialogs(e.assets_rc(), out);
    run_kpx(e.assets_rc(), out);
    run_spell(e.assets_rc(), out);
    run_panels(e.assets_rc(), out);
}

fn sshot(s: &crate::shell::Shell, out: &Path, name: &str) {
    let mut frame = Image::new(800, 600);
    s.draw(&mut frame);
    let p = out.join(format!("{name}.png"));
    crate::save_png(&p, &frame).expect("write png");
    eprintln!("wrote {}", p.display());
}

fn sclick(s: &mut crate::shell::Shell, x: i32, y: i32) {
    s.mouse_move(x, y);
    s.mouse_down(x, y);
    s.mouse_up(x, y);
    s.update(50.0);
    s.pump();
}

/// The shell: splash, login, the save prompt and help mode.
fn run_shell(assets: Rc<Assets>, out: &Path) {
    use crate::dialogs::Key;
    let users = out.join("users_root");
    let _ = std::fs::remove_dir_all(&users);
    let _ = std::fs::create_dir_all(users.join("Users").join("Sam"));
    let _ = std::fs::create_dir_all(users.join("Users").join("Alex"));
    let mut s = crate::shell::Shell::with_users(users, assets, Rc::new(Audio::silent()), true);
    for _ in 0..60 {
        s.update(100.0);
    }
    sshot(&s, out, "21_splash");
    s.key(Key::Escape);
    s.update(50.0);
    for c in "Jamie".chars() {
        s.key(Key::Char(c));
    }
    s.mouse_move(530, 216);
    s.update(50.0);
    sshot(&s, out, "22_login");
    s.key(Key::Enter);
    s.update(50.0);
    // Draw something, then ask for a new page: the save prompt appears.
    let (x, y) = s.easel.tool_button(232).unwrap();
    sclick(&mut s, x, y);
    s.mouse_move(200, 200);
    s.mouse_down(200, 200);
    for k in 0..20 {
        s.mouse_move(200 + k * 10, 200 + (k % 5) * 6);
        s.update(16.0);
    }
    s.mouse_up(400, 220);
    s.easel.actions.push(crate::easel::Action::New);
    s.pump();
    s.update(50.0);
    sshot(&s, out, "23_ask_save");
    sclick(&mut s, 400, 322); // No
    let _ = s.take_actions();
    // Help mode: ?, then the pencil tool's help.
    let (x, y) = s.easel.tool_button(239).unwrap();
    sclick(&mut s, x, y);
    s.update(500.0);
    sshot(&s, out, "24_help_mode");
    sclick(&mut s, 26, 305);
    s.update(50.0);
    sshot(&s, out, "25_help_dialog");
    run_picker(&mut s, out);
}

/// Toolbox dialogs: Edit ABC Text, Say It This Way, Flatten.
fn run_dialogs(assets: Rc<Assets>, out: &Path) {
    use crate::easel::Action;
    let users = out.join("users_dialogs");
    let _ = std::fs::remove_dir_all(&users);
    let mut s = crate::shell::Shell::with_users(users, assets, Rc::new(Audio::silent()), false);
    s.login_as("Tester");
    // The sound library with the artist's own "My Sounds" folder, then record mode.
    if let Some(dir) = s.user_subdir("DirSounds", "My Sounds") {
        let _ = std::fs::create_dir_all(&dir);
        let tone = mohawk::Sound { sample_rate: 22050, channels: 1, samples: (0..22050).map(|i| ((i as f32 * 0.06).sin() * 8000.0) as i16).collect(), loop_count: 0 };
        let _ = crate::formats::save_wav(&dir.join("My Song.wav"), &tone);
    }
    s.easel.test_command(225);
    s.update(50.0);
    sshot(&s, out, "29_sound_library");
    s.easel.test_lib_command(200);
    s.update(50.0);
    sshot(&s, out, "29b_sound_record_mode");
    s.easel.test_lib_command(200);
    // Email > Set Up (Parental Controls): tick "Enable Kid Pix Email", OK; then Email > Send.
    s.easel.actions.push(Action::EmailSetup);
    s.pump();
    sclick(&mut s, 397, 435);
    s.update(50.0);
    sshot(&s, out, "33_parental_controls");
    sclick(&mut s, 451, 468);
    assert!(s.easel.menu_state.email_enabled, "email enabled");
    s.easel.menu_command(crate::menubar::cmd::EMAIL_SEND);
    s.update(50.0);
    sshot(&s, out, "34_email_send");
    sclick(&mut s, 470, 371); // DON'T SEND
    let _ = s.take_actions();
    for (a, name) in [(Action::EditAbcText, "26_abc_text"), (Action::SayItThisWay, "27_say_it_this_way"), (Action::Flatten(225), "28_flatten")] {
        s.easel.actions.push(a);
        s.pump();
        s.update(50.0);
        sshot(&s, out, name);
        s.key(crate::dialogs::Key::Escape);
        s.update(50.0);
    }
}

fn settle_shell(s: &mut crate::shell::Shell, ms: f32) {
    let mut t = 0.0;
    while t < ms {
        s.update(50.0);
        s.pump();
        t += 50.0;
    }
}

/// The SlideShow organizer (Picker), its dialogs and the player (docs/RE_SHELL.md §9).
fn run_picker(s: &mut crate::shell::Shell, out: &Path) {
    use crate::dialogs::Key;
    use crate::picker::Picker;
    // Close the help dialog (OK), then go to the SlideShow like easel button 270 + main.
    sclick(s, 460, 414);
    let _ = s.take_actions();
    s.enter_picker();
    settle_shell(s, 400.0);
    sshot(s, out, "40_picker_empty");
    let ideas = s.assets.path("Idea Machine");
    {
        let p = s.picker.as_mut().expect("picker");
        if let Some(dir) = &ideas {
            for (k, name) in ["Castle", "Frog", "Space Pilot"].iter().enumerate() {
                if !p.load_into(k, dir.join(format!("{name}.kpx"))) {
                    eprintln!("could not load {name}.kpx");
                }
            }
        }
        p.project.selected = 1;
        // A few different effects so the effects view shows them.
        if let Some(Some(sl)) = p.project.slides.get_mut(1) {
            sl.settings.transition = 7;
            sl.settings.advance = 1;
        }
        if let Some(Some(sl)) = p.project.slides.get_mut(2) {
            sl.settings.transition = 20;
            sl.settings.seconds = 12;
        }
    }
    s.mouse_move(Picker::slot_center(5).0, Picker::slot_center(5).1);
    settle_shell(s, 100.0);
    sshot(s, out, "41_picker_slides");
    // Drag slide 3 to slot 5 (mid-drag shot shows the red drop target), then back.
    {
        let (a, b) = (Picker::slot_center(2), Picker::slot_center(5));
        s.mouse_move(a.0, a.1);
        s.mouse_down(a.0, a.1);
        for k in 1..=6 {
            s.mouse_move(a.0 + (b.0 - a.0) * k / 6, a.1 + (b.1 - a.1) * k / 6);
            s.update(16.0);
        }
        sshot(s, out, "42_picker_drag");
        s.mouse_up(b.0, b.1);
        s.update(50.0);
        let p = s.picker.as_mut().unwrap();
        assert!(p.project.slides[5].is_some() && p.project.slides[2].is_none(), "drag moved the slide");
        p.project.move_slide(5, 2);
    }
    // Effects view (camera button).
    sclick(s, 608, 452);
    s.mouse_move(300, 470);
    settle_shell(s, 100.0);
    sshot(s, out, "43_picker_effects");
    for (which, name) in [("trans", "44_dlg_transition"), ("timer", "45_dlg_timer"), ("audio", "46_dlg_audio"), ("bgcolor", "47_dlg_bgcolor")] {
        s.picker.as_mut().unwrap().test_open(which, 1);
        s.mouse_move(400, 300);
        settle_shell(s, 100.0);
        sshot(s, out, name);
        s.picker.as_mut().unwrap().test_close();
    }
    // Save the show: the strip lists it.
    let dir = s.user_subdir("DirProjects", "My Pictures/My Slide Shows").expect("user");
    let _ = std::fs::create_dir_all(&dir);
    {
        let p = s.picker.as_mut().unwrap();
        assert!(p.save_to(&dir.join("Space Trip.kpp")));
        let text = std::fs::read_to_string(dir.join("Space Trip.kpp")).unwrap_or_default();
        eprintln!("Space Trip.kpp:\n{text}");
        assert!(p.open(&dir.join("Space Trip.kpp")).is_ok());
        assert_eq!(p.project.filled(), 3);
    }
    sclick(s, 608, 452); // back to the normal view
    settle_shell(s, 100.0);
    sshot(s, out, "48_picker_saved");
    // More saved shows: the strip scrolls along the markers of script 5238.
    for n in ["Birthday", "Dinosaurs", "Ocean", "Robots", "Zoo"] {
        let _ = std::fs::copy(dir.join("Space Trip.kpp"), dir.join(format!("{n}.kpp")));
    }
    s.picker.as_mut().unwrap().refresh_shows();
    sclick(s, 557, 546);
    s.update(60.0);
    s.update(60.0);
    sshot(s, out, "51_picker_strip_scroll");
    settle_shell(s, 2600.0);
    s.mouse_move(400, 300);
    settle_shell(s, 6000.0);
    sshot(s, out, "52_picker_strip_idle");
    // Play: the player shows slide 2's Iris Heart transition half-way.
    sclick(s, 625, 388);
    settle_shell(s, 50.0);
    if let Some(p) = s.player.as_mut() {
        // First slide fully shown, then into the second slide's transition.
        p.seek_transition(1.0);
        p.mouse_down();
        p.key(Key::Enter);
        p.update(10.0);
    }
    // The first slide waits 7 s on its timer: skip it.
    settle_shell(s, 7200.0);
    if let Some(p) = s.player.as_mut() {
        p.seek_transition(0.5);
    }
    sshot(s, out, "49_player_transition");
    s.key(Key::Escape);
    settle_shell(s, 100.0);
    assert!(s.screen == crate::shell::Screen::Picker, "Esc returns to the Picker");
    // Help mode in the Picker (PickerHelp 777 + 779).
    sclick(s, 763, 515);
    s.mouse_move(Picker::slot_center(1).0, Picker::slot_center(1).1);
    settle_shell(s, 500.0);
    sshot(s, out, "50_picker_help");
    s.key(Key::Escape);
    // Back to the Paint Zone.
    sclick(s, 771, 571);
    settle_shell(s, 100.0);
    assert!(s.screen == crate::shell::Screen::Easel, "5567 goes back to the easel");
}

/// Native .kpx: open an Idea Machine page; save a page with a sticker and a text box and
/// load it back.
fn run_kpx(assets: Rc<Assets>, out: &Path) {
    let mut e = Easel::new(assets.clone(), Rc::new(Audio::silent()));
    let im = assets.root.join("Idea Machine");
    if let Some(p) = std::fs::read_dir(&im).ok().and_then(|rd| rd.flatten().map(|d| d.path()).filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("kpx"))).min()) {
        match crate::kpxio::read(&p) {
            Ok(page) => {
                e.load_kpx(page);
                shot(&e, out, "30_idea_machine_page");
            }
            Err(err) => eprintln!("kpx read failed: {err}"),
        }
    }
    e.new_picture_for_test();
    click(&mut e, 45, 120);
    settle(&mut e, 250.0);
    lib_drag(&mut e, 2, (300, 200));
    tool(&mut e, 229);
    click(&mut e, 150, 380);
    for c in "Saved as .kpx".chars() {
        e.key_char(c);
    }
    let page = e.to_kpx();
    let path = out.join("roundtrip.kpx");
    if let Err(err) = crate::kpxio::write(&page, &path) {
        eprintln!("kpx write failed: {err}");
        return;
    }
    let back = match crate::kpxio::read(&path) {
        Ok(p) => p,
        Err(err) => {
            eprintln!("kpx reread failed: {err}");
            return;
        }
    };
    eprintln!("roundtrip: {} objects, {} texts", back.objects.len(), back.texts().count());
    // Saving alongside the Kid Pix format: a PNG copy next to the .kpx, and plain images.
    e.menu_state.png_copy = true;
    let kpx = out.join("saved_with_copy.kpx");
    crate::save_picture(&mut e, &kpx).expect("save kpx");
    assert!(kpx.is_file() && kpx.with_extension("png").is_file(), "kpx + png copy");
    for ext in ["png", "jpg", "bmp"] {
        let img_path = out.join(format!("saved_image.{ext}"));
        crate::save_picture(&mut e, &img_path).expect("save image");
        let img = crate::formats::load_image(&img_path).expect("read image back");
        assert_eq!((img.width, img.height), (600, 452));
    }
    eprintln!("saved .kpx with PNG copy, and .png/.jpg/.bmp images");
    let mut f = Easel::new(assets, Rc::new(Audio::silent()));
    f.load_kpx(back);
    shot(&f, out, "31_kpx_roundtrip");
}

/// Toolbox > Check Spelling: a misspelt word is found, suggested and changed.
fn run_spell(assets: Rc<Assets>, out: &Path) {
    let mut e = Easel::new(assets, Rc::new(Audio::silent()));
    tool(&mut e, 229);
    click(&mut e, 200, 200);
    for c in "My dogg likes the pictre".chars() {
        e.key_char(c);
    }
    e.update(16.0);
    e.menu_command(crate::menubar::cmd::CHECK_SPELLING);
    e.update(16.0);
    shot(&e, out, "32_spell_checker");
    for _ in 0..2 {
        e.key_char('\r'); // Change to the first suggestion
    }
    let fixed = e.texts.first().map(|t| t.text.clone()).unwrap_or_default();
    eprintln!("spell checked: {fixed:?}");
    assert!(!fixed.contains("pictre"), "Check Spelling changed the word");
    if crate::spell::Checker::new(&e.assets_rc().root, vec![]).uses_lexicon() {
        assert_eq!(fixed, "My dog likes the picture", "CD lexicon corrections");
    }
}

/// Every tool panel in each of its modes, cropped to the bottom strip (for side-by-side
/// checks against the User Guide's panel screenshots).
fn run_panels(assets: Rc<Assets>, out: &Path) {
    let dir = out.join("panels");
    let _ = std::fs::create_dir_all(&dir);
    let mut e = Easel::new(assets, Rc::new(Audio::silent()));
    e.color = gfx::rgb(0, 230, 0);
    let cases: &[(&str, u16, &[(G, usize)])] = &[
        ("pencil", 232, &[(G::Variant, 0)]),
        ("brush_real", 233, &[(G::Variant, 0), (G::Mode, 0)]),
        ("brush_wacky", 233, &[(G::Variant, 0), (G::Mode, 1)]),
        ("soundart_mouse", 233, &[(G::Variant, 1), (G::Mode, 0)]),
        ("soundart_handsfree", 233, &[(G::Variant, 1), (G::Mode, 1)]),
        ("spray_shapes", 233, &[(G::Variant, 2), (G::Mode, 0)]),
        ("spray_moving", 233, &[(G::Variant, 2), (G::Mode, 1)]),
        ("fill_solid", 234, &[(G::Variant, 0)]),
        ("fill_gradient", 234, &[(G::Variant, 1)]),
        ("fill_pattern", 234, &[(G::Variant, 2)]),
        ("mixer_big", 237, &[(G::Variant, 0)]),
        ("mixer_wand", 237, &[(G::Variant, 1)]),
        ("stamp", 236, &[]),
        ("text", 229, &[]),
        ("mover_grab", 227, &[(G::Variant, 0)]),
        ("mover_cut", 227, &[(G::Variant, 1)]),
        ("eraser_small", 228, &[(G::Variant, 0)]),
        ("eraser_big", 228, &[(G::Variant, 1)]),
    ];
    for (name, cmd, picks) in cases {
        tool(&mut e, *cmd);
        settle(&mut e, 600.0);
        for (g, i) in picks.iter() {
            pick(&mut e, *g, *i);
        }
        e.mouse_move(400, 300);
        settle(&mut e, 2000.0);
        let mut frame = Image::new(800, 600);
        e.draw(&mut frame);
        let crop = gfx::crop(&frame, 90, 500, 652, 100);
        crate::save_png(&dir.join(format!("{name}.png")), &crop).expect("png");
    }
    let _ = std::mem::take(&mut e.actions);
    let _ = std::mem::take(&mut e.alerts);
    eprintln!("wrote {} panel crops", cases.len());
}
