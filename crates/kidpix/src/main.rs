#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! Kid Pix Revival — a cross-platform engine for the data of your own Kid Pix Deluxe 3 CD.

mod anim;
mod assets;
mod audio;
mod buttons;
mod colorpicker;
mod easel;
mod gfx;
mod importer;
mod fills;
mod formats;
mod library;
mod menubar;
mod mixer;
mod movie;
mod objects;
mod paint;
mod panels;
mod print;
mod record;
mod regions;
mod selftest;
mod setup;
mod slideshow;
mod speech;
mod stickers;
mod tools;
mod trays;
mod ui;

use assets::Assets;
use audio::Audio;
use easel::{Action, CursorHint, Easel};
use gfx::Image;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{CursorIcon, Window, WindowId};

const W: u32 = 800;
const H: u32 = 600;

struct Splash {
    image: Rc<Image>,
    left_ms: f32,
}

struct Running {
    window: Rc<Window>,
    surface: softbuffer::Surface<Rc<Window>, Rc<Window>>,
    _context: softbuffer::Context<Rc<Window>>,
    easel: Easel,
    frame: Image,
    last: Instant,
    view: (f32, f32, f32),
    modifiers: ModifiersState,
    save_path: Option<PathBuf>,
    splash: Option<Splash>,
    cursor: Option<(CursorHint, u32)>,
    mouse: (i32, i32),
    cursors: std::collections::HashMap<(u16, u32), winit::window::CustomCursor>,
    movie_rx: Option<std::sync::mpsc::Receiver<Result<movie::Movie, String>>>,
}

struct App {
    assets: Rc<Assets>,
    audio: Rc<Audio>,
    run: Option<Running>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.run.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("Kid Pix Revival")
            .with_inner_size(LogicalSize::new(W as f64 * 1.25, H as f64 * 1.25))
            .with_min_inner_size(LogicalSize::new(W as f64 / 2.0, H as f64 / 2.0));
        let window = Rc::new(el.create_window(attrs).expect("create window"));
        let context = softbuffer::Context::new(window.clone()).expect("softbuffer context");
        let surface = softbuffer::Surface::new(&context, window.clone()).expect("softbuffer surface");
        let easel = Easel::new(self.assets.clone(), self.audio.clone());
        let splash = self.assets.image("Data/Splash.MHK", 1000).map(|image| {
            if let Some(a) = self.assets.archive("Data/Splash.MHK") {
                if let Some(&id) = a.ids(assets::T_WAV).first() {
                    if let Some(s) = self.assets.sound("Data/Splash.MHK", id) {
                        self.audio.play(s, audio::CH_MUSIC, false);
                    }
                }
            }
            Splash { image, left_ms: 3000.0 }
        });
        self.run = Some(Running {
            window,
            surface,
            _context: context,
            easel,
            frame: Image::new(W, H),
            last: Instant::now(),
            view: (0.0, 0.0, 1.0),
            modifiers: ModifiersState::empty(),
            save_path: None,
            splash,
            cursor: None,
            mouse: (0, 0),
            cursors: Default::default(),
            movie_rx: None,
        });
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(r) = self.run.as_mut() else { return };
        match event {
            WindowEvent::CloseRequested => {
                if confirm_discard(&r.easel) {
                    el.exit();
                }
            }
            WindowEvent::Resized(_) => r.window.request_redraw(),
            WindowEvent::ModifiersChanged(m) => r.modifiers = m.state(),
            WindowEvent::CursorMoved { position, .. } => {
                let (x, y) = to_logical(r.view, position);
                r.mouse = (x, y);
                r.easel.mouse_move(x, y);
            }
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                if r.splash.is_some() {
                    if state == ElementState::Pressed {
                        r.splash = None;
                        self.audio.stop_channel(audio::CH_MUSIC);
                    }
                    return;
                }
                let (x, y) = r.mouse;
                match state {
                    ElementState::Pressed => r.easel.mouse_down(x, y),
                    ElementState::Released => r.easel.mouse_up(x, y),
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let steps = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, y) => y,
                    winit::event::MouseScrollDelta::PixelDelta(p) => (p.y / 40.0) as f32,
                };
                r.easel.wheel(steps);
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                let ctrl = r.modifiers.control_key() || r.modifiers.super_key();
                match &event.logical_key {
                    Key::Character(s) if ctrl => {
                        // The original accelerator table (Ctrl+C/Q/S/V/X/Z).
                        if let Some(id) = s.chars().next().and_then(menubar::accelerator) {
                            r.easel.menu_command(id);
                        }
                    }
                    Key::Named(NamedKey::Escape) => r.easel.key_escape(),
                    Key::Named(NamedKey::Delete) => r.easel.delete_under_mouse(),
                    Key::Named(NamedKey::Backspace) => r.easel.key_char('\u{8}'),
                    Key::Named(NamedKey::Enter) => r.easel.key_char('\n'),
                    Key::Named(NamedKey::Space) => r.easel.key_char(' '),
                    _ => {
                        if let Some(t) = &event.text {
                            for c in t.chars() {
                                r.easel.key_char(c);
                            }
                        }
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = (now - r.last).as_secs_f32() * 1000.0;
                r.last = now;
                let dt = dt.min(100.0);
                if let Some(s) = r.splash.as_mut() {
                    s.left_ms -= dt;
                    if s.left_ms <= 0.0 {
                        r.splash = None;
                    }
                }
                if let Some(rx) = &r.movie_rx {
                    if let Ok(res) = rx.try_recv() {
                        r.movie_rx = None;
                        match res {
                            Ok(m) => r.easel.add_movie(m),
                            Err(e) => r.easel.message(e),
                        }
                    }
                }
                r.easel.update(dt);
                match &r.splash {
                    Some(s) => {
                        r.frame.pixels.fill(0xff00_0000);
                        let (x, y) = ((W as i32 - s.image.width as i32) / 2, (H as i32 - s.image.height as i32) / 2);
                        gfx::blit(&mut r.frame, &s.image, x, y);
                    }
                    None => r.easel.draw(&mut r.frame),
                }
                r.present();
                r.update_cursor(el, &self.assets);
            }
            _ => {}
        }
        self.handle_actions(el);
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        if let Some(r) = &self.run {
            r.window.request_redraw();
        }
        el.set_control_flow(ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(16)));
    }
}

impl App {
    fn handle_actions(&mut self, el: &ActiveEventLoop) {
        let Some(r) = self.run.as_mut() else { return };
        let actions: Vec<Action> = std::mem::take(&mut r.easel.actions);
        for a in actions {
            match a {
                Action::Save | Action::SaveAs => {
                    let path = match (&a, &r.save_path) {
                        (Action::Save, Some(p)) => Some(p.clone()),
                        _ => rfd::FileDialog::new()
                            .set_title("Save your picture")
                            .add_filter("PNG picture", &["png"])
                            .set_file_name("My Picture.png")
                            .save_file(),
                    };
                    if let Some(mut p) = path {
                        if p.extension().is_none() {
                            p.set_extension("png");
                        }
                        let img = r.easel.flattened();
                        match save_png(&p, &img) {
                            Ok(()) => {
                                r.easel.canvas.dirty = false;
                                r.easel.message(format!("Saved {}", p.file_name().unwrap_or_default().to_string_lossy()));
                                r.save_path = Some(p);
                            }
                            Err(e) => r.easel.message(format!("Couldn't save: {e}")),
                        }
                    }
                }
                Action::Open => {
                    if !confirm_discard(&r.easel) {
                        continue;
                    }
                    let mut dlg = rfd::FileDialog::new()
                        .set_title("Open a picture")
                        .add_filter("Pictures", &["png", "kpx", "PNG", "KPX"]);
                    if let Some(p) = self.assets.path("Idea Machine") {
                        dlg = dlg.set_directory(p);
                    }
                    if let Some(p) = dlg.pick_file() {
                        match load_picture(&p) {
                            Some(img) => {
                                r.easel.load_picture(img);
                                r.save_path = p
                                    .extension()
                                    .is_some_and(|e| e.eq_ignore_ascii_case("png"))
                                    .then(|| p.clone());
                            }
                            None => r.easel.message("Couldn't open that picture."),
                        }
                    }
                }
                Action::New => {
                    if confirm_discard(&r.easel) {
                        r.easel.new_picture();
                        r.save_path = None;
                    }
                }
                Action::Export => {
                    if let Some(p) = rfd::FileDialog::new()
                        .set_title("Export your picture")
                        .add_filter("JPEG picture", &["jpg"])
                        .add_filter("BMP picture", &["bmp"])
                        .add_filter("PNG picture", &["png"])
                        .set_file_name("My Picture.jpg")
                        .save_file()
                    {
                        let img = r.easel.flattened();
                        match formats::export(&img, &p) {
                            Ok(()) => r.easel.message("Exported your picture."),
                            Err(e) => r.easel.message(format!("Couldn't export: {e}")),
                        }
                    }
                }
                Action::AddGraphic | Action::ImportBackground => {
                    if let Some(p) = rfd::FileDialog::new()
                        .set_title(if a == Action::AddGraphic { "Add Graphic" } else { "Import Background" })
                        .add_filter("Pictures", &["bmp", "gif", "jpg", "jpeg", "png", "BMP", "GIF", "JPG", "JPEG", "PNG"])
                        .pick_file()
                    {
                        match formats::load_image(&p) {
                            Ok(img) => r.easel.import_picture(
                                img,
                                if a == Action::AddGraphic { easel::FitTarget::Graphic } else { easel::FitTarget::Background },
                            ),
                            Err(e) => r.easel.message(e),
                        }
                    }
                }
                Action::ImportSound => {
                    if let Some(p) = rfd::FileDialog::new().set_title("Import Sound").add_filter("WAV sound", &["wav", "WAV"]).pick_file() {
                        match formats::load_wav(&p) {
                            Ok(s) => r.easel.set_page_sound(std::sync::Arc::new(s)),
                            Err(e) => r.easel.message(e),
                        }
                    }
                }
                Action::IdeaMachine => {
                    if !confirm_discard(&r.easel) {
                        continue;
                    }
                    let mut dlg = rfd::FileDialog::new().set_title("Idea Machine").add_filter("Kid Pix picture", &["kpx", "KPX"]);
                    if let Some(p) = self.assets.path("Idea Machine") {
                        dlg = dlg.set_directory(p);
                    }
                    if let Some(p) = dlg.pick_file() {
                        match load_picture(&p) {
                            Some(img) => r.easel.load_picture(img),
                            None => r.easel.message("Couldn't open that picture."),
                        }
                    }
                }
                Action::UserGuide => match self.assets.path("KpInstall/KdpxManual.pdf") {
                    Some(p) => {
                        let opener = if cfg!(target_os = "macos") { "open" } else if cfg!(windows) { "explorer" } else { "xdg-open" };
                        let _ = std::process::Command::new(opener).arg(p).spawn();
                    }
                    None => r.easel.message("The User Guide wasn't found in your Kid Pix files."),
                },
                Action::Login | Action::EditAbcText | Action::CheckSpelling | Action::SayItThisWay | Action::StampEditor => {
                    r.easel.dialog_action(a.clone());
                }
                Action::AddMovie => {
                    let mut dlg = rfd::FileDialog::new()
                        .set_title("Add a movie")
                        .add_filter("QuickTime movie", &["mov", "MOV"]);
                    if let Some(p) = self.assets.path("QTmovies") {
                        dlg = dlg.set_directory(p);
                    }
                    if let Some(p) = dlg.pick_file() {
                        r.easel.message("Getting the movie ready...");
                        r.movie_rx = Some(movie::load_async(p));
                    }
                }
                Action::Print => {
                    let img = r.easel.flattened();
                    match print::print(&img) {
                        Ok(_) => r.easel.message("Sent your picture to the printer."),
                        Err(e) => r.easel.message(format!("Couldn't print: {e}")),
                    }
                }
                Action::SlideAddFile => {
                    let mut dlg = rfd::FileDialog::new()
                        .set_title("Add a picture to the slide show")
                        .add_filter("Pictures", &["png", "kpx", "PNG", "KPX"]);
                    if let Some(p) = self.assets.path("Idea Machine") {
                        dlg = dlg.set_directory(p);
                    }
                    if let Some(files) = dlg.pick_files() {
                        for p in files {
                            if let Some(img) = load_picture(&p) {
                                r.easel.slideshow_add(img);
                            }
                        }
                    }
                }
                Action::Quit => {
                    if confirm_discard(&r.easel) {
                        el.exit();
                    }
                }
            }
        }
    }
}

impl Running {
    fn present(&mut self) {
        let size = self.window.inner_size();
        let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else {
            return;
        };
        if self.surface.resize(w, h).is_err() {
            return;
        }
        let Ok(mut buf) = self.surface.buffer_mut() else { return };
        self.view = gfx::present(&self.frame, &mut buf, w.get(), h.get(), 0x0010_1030);
        let _ = buf.present();
    }

    fn update_cursor(&mut self, el: &ActiveEventLoop, assets: &Assets) {
        let hint = self.easel.cursor_hint();
        // Original cursors are 16x16; scale them with the window (at least 2x like the
        // 800x600 original on today's screens).
        let scale = (self.view.2 * self.window.scale_factor() as f32).round().clamp(1.0, 4.0) as u32;
        let scale = scale.max(2);
        if self.cursor == Some((hint, scale)) {
            return;
        }
        self.cursor = Some((hint, scale));
        let (n, drop) = match hint {
            CursorHint::Cur(n) => (n, false),
            CursorHint::Drop => (u16::MAX, true),
        };
        let key = (n, scale);
        if drop && !self.cursors.contains_key(&key) {
            let img = gfx::resize_nearest(&menubar::drop_cursor(), scale);
            if let Ok(src) = winit::window::CustomCursor::from_rgba(img.to_rgba8(), img.width as u16, img.height as u16, 7 * scale as u16, 0) {
                self.cursors.insert(key, el.create_custom_cursor(src));
            }
        }
        if let std::collections::hash_map::Entry::Vacant(e) = self.cursors.entry(key) {
            if let Some(c) = assets.cursor(10000 + n) {
                let img = gfx::resize_nearest(&c.image, scale);
                let src = winit::window::CustomCursor::from_rgba(
                    img.to_rgba8(),
                    img.width as u16,
                    img.height as u16,
                    c.hot_x * scale as u16,
                    c.hot_y * scale as u16,
                );
                if let Ok(src) = src {
                    e.insert(el.create_custom_cursor(src));
                }
            }
        }
        match self.cursors.get(&key) {
            Some(c) => self.window.set_cursor(c.clone()),
            None => self.window.set_cursor(CursorIcon::Default),
        }
    }
}

fn to_logical(view: (f32, f32, f32), p: PhysicalPosition<f64>) -> (i32, i32) {
    let (ox, oy, s) = view;
    (
        ((p.x as f32 - ox) / s.max(0.01)).floor() as i32,
        ((p.y as f32 - oy) / s.max(0.01)).floor() as i32,
    )
}

fn confirm_discard(easel: &Easel) -> bool {
    if !easel.canvas.dirty {
        return true;
    }
    matches!(
        rfd::MessageDialog::new()
            .set_title("Kid Pix Revival")
            .set_description("Your picture has changes that aren't saved. Throw them away?")
            .set_buttons(rfd::MessageButtons::YesNo)
            .show(),
        rfd::MessageDialogResult::Yes
    )
}

pub fn save_png(path: &Path, img: &Image) -> Result<(), String> {
    let f = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), img.width, img.height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header().map_err(|e| e.to_string())?;
    w.write_image_data(&img.to_rgba8()).map_err(|e| e.to_string())
}

fn load_picture(path: &Path) -> Option<Image> {
    let ext = path.extension()?.to_string_lossy().to_lowercase();
    if ext == "kpx" {
        // Kid Pix picture: Mohawk archive whose first bitmap is the page.
        let a = mohawk::Archive::open(path).ok()?;
        let id = *a.ids(assets::T_BMP).first()?;
        return mohawk::decode_bitmap(a.get(assets::T_BMP, id)?, None).ok();
    }
    let dec = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).ok()?));
    let mut reader = dec.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    let data = &buf[..info.buffer_size()];
    let (w, h) = (info.width, info.height);
    let rgba: Vec<u8> = match (info.color_type, info.bit_depth) {
        (png::ColorType::Rgba, png::BitDepth::Eight) => data.to_vec(),
        (png::ColorType::Rgb, png::BitDepth::Eight) => data.as_chunks::<3>().0.iter().flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
        (png::ColorType::Grayscale, png::BitDepth::Eight) => data.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        (png::ColorType::GrayscaleAlpha, png::BitDepth::Eight) => data.as_chunks::<2>().0.iter().flat_map(|c| [c[0], c[0], c[0], c[1]]).collect(),
        _ => return None,
    };
    let mut img = Image::from_rgba8(w, h, &rgba);
    // Flatten onto white.
    for p in img.pixels.iter_mut() {
        *p = gfx::blend(0xffff_ffff, *p);
    }
    Some(img)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!(
            "Kid Pix Revival\n\nUsage: kidpix-revival [--data <dir>] [--import <zip|iso|dir>]\n\n\
             --data <dir>      use an extracted CD copy or Kid Pix Deluxe 3 install folder\n\
             --import <path>   copy the needed files from your CD/ISO/ZIP into the app's data folder\n\n\
             Environment: KIDPIX_DATA=<dir> works like --data."
        );
        return;
    }
    let mut data_arg = None;
    let mut render_test = None;
    let mut import_arg = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--data" => {
                data_arg = args.get(i + 1).map(PathBuf::from);
                i += 1;
            }
            "--render-test" => {
                render_test = args.get(i + 1).map(PathBuf::from);
                i += 1;
            }
            "--import" => {
                import_arg = args.get(i + 1).map(PathBuf::from);
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }
    let root = match setup::locate(data_arg, import_arg) {
        Some(r) => r,
        None => return,
    };
    eprintln!("Using Kid Pix data from {}", root.display());
    let assets = Rc::new(Assets::new(root));
    if let Some(out) = render_test {
        selftest::run(assets, &out);
        return;
    }
    let audio = Rc::new(Audio::new());
    let el = EventLoop::new().expect("event loop");
    let mut app = App {
        assets,
        audio,
        run: None,
    };
    el.run_app(&mut app).expect("run");
}
