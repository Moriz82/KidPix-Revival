#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! Kid Pix Revival — a cross-platform engine for the data of your own Kid Pix Deluxe 3 CD.

mod anim;
mod assets;
mod audio;
mod buttons;
mod colorpicker;
mod dialogs;
mod easel;
mod gfx;
mod helpkeys;
mod importer;
mod fills;
mod formats;
mod kpxio;
mod library;
mod menubar;
mod mixer;
mod movie;
mod objects;
mod paint;
mod panels;
mod picker;
mod print;
mod record;
mod regions;
mod selftest;
mod setup;
mod shell;
mod slideplay;
mod slideshow;
mod speech;
mod spell;
mod stampedit;
mod stickers;
mod textbox;
mod tools;
mod win32;
mod trays;
mod ui;
mod uifont;

use assets::Assets;
use audio::Audio;
use easel::{Action, CursorHint, Easel};
use shell::Shell;
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

struct Running {
    window: Rc<Window>,
    surface: softbuffer::Surface<Rc<Window>, Rc<Window>>,
    _context: softbuffer::Context<Rc<Window>>,
    shell: Shell,
    frame: Image,
    last: Instant,
    view: (f32, f32, f32),
    modifiers: ModifiersState,
    save_path: Option<PathBuf>,
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
            .with_title("Kid Pix Deluxe 3") // string table 128
            .with_inner_size(LogicalSize::new(W as f64 * 1.25, H as f64 * 1.25))
            .with_min_inner_size(LogicalSize::new(W as f64 / 2.0, H as f64 / 2.0));
        let window = Rc::new(el.create_window(attrs).expect("create window"));
        let context = softbuffer::Context::new(window.clone()).expect("softbuffer context");
        let surface = softbuffer::Surface::new(&context, window.clone()).expect("softbuffer surface");
        let shell = Shell::new(self.assets.clone(), self.audio.clone(), true);
        self.run = Some(Running {
            window,
            surface,
            _context: context,
            shell,
            frame: Image::new(W, H),
            last: Instant::now(),
            view: (0.0, 0.0, 1.0),
            modifiers: ModifiersState::empty(),
            save_path: None,
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
                if r.shell.busy() {
                    r.shell.save_prefs();
                    el.exit();
                } else {
                    r.shell.request_quit();
                }
            }
            WindowEvent::Resized(_) => r.window.request_redraw(),
            WindowEvent::ModifiersChanged(m) => r.modifiers = m.state(),
            WindowEvent::CursorMoved { position, .. } => {
                let (x, y) = to_logical(r.view, position);
                r.mouse = (x, y);
                r.shell.mouse_move(x, y);
            }
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                let (x, y) = r.mouse;
                match state {
                    ElementState::Pressed => r.shell.mouse_down(x, y),
                    ElementState::Released => r.shell.mouse_up(x, y),
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let steps = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, y) => y,
                    winit::event::MouseScrollDelta::PixelDelta(p) => (p.y / 40.0) as f32,
                };
                if !r.shell.busy() {
                    r.shell.wheel(steps);
                }
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                let ctrl = r.modifiers.control_key() || r.modifiers.super_key();
                let dk = match &event.logical_key {
                    Key::Named(NamedKey::Backspace) => Some(dialogs::Key::Back),
                    Key::Named(NamedKey::Delete) => Some(dialogs::Key::Delete),
                    Key::Named(NamedKey::ArrowLeft) => Some(dialogs::Key::Left),
                    Key::Named(NamedKey::ArrowRight) => Some(dialogs::Key::Right),
                    Key::Named(NamedKey::Enter) => Some(dialogs::Key::Enter),
                    Key::Named(NamedKey::Escape) => Some(dialogs::Key::Escape),
                    Key::Named(NamedKey::PageUp) => Some(dialogs::Key::PageUp),
                    Key::Named(NamedKey::PageDown) => Some(dialogs::Key::PageDown),
                    Key::Named(NamedKey::Space) => Some(dialogs::Key::Char(' ')),
                    _ => event.text.as_ref().and_then(|t| t.chars().next()).filter(|c| !c.is_control()).map(dialogs::Key::Char),
                };
                let other_screen = r.shell.screen != shell::Screen::Easel;
                if ctrl && other_screen && !r.shell.busy() {
                    // Picker accelerators (Ctrl+S save, Ctrl+Q exit, Ctrl+X/C/V slides).
                    if let Key::Character(s) = &event.logical_key {
                        if let Some(id) = s.chars().next().and_then(menubar::accelerator) {
                            r.shell.menu_command(id);
                        }
                    }
                    self.handle_actions(el);
                    return;
                }
                if let Some(k) = dk {
                    if r.shell.key(k) {
                        self.handle_actions(el);
                        return;
                    }
                }
                if other_screen {
                    r.shell.other_key();
                    self.handle_actions(el);
                    return;
                }
                let easel = &mut r.shell.easel;
                match &event.logical_key {
                    Key::Character(s) if ctrl => {
                        // The original accelerator table (Ctrl+C/Q/S/V/X/Z).
                        if let Some(id) = s.chars().next().and_then(menubar::accelerator) {
                            easel.menu_command(id);
                        }
                    }
                    Key::Named(NamedKey::Escape) => easel.key_escape(),
                    Key::Named(NamedKey::Delete) => easel.delete_under_mouse(),
                    Key::Named(NamedKey::ArrowLeft) => easel.key_char('\u{1c}'),
                    Key::Named(NamedKey::ArrowRight) => easel.key_char('\u{1d}'),
                    Key::Named(NamedKey::Backspace) => easel.key_char('\u{8}'),
                    Key::Named(NamedKey::Enter) => easel.key_char('\n'),
                    Key::Named(NamedKey::Space) => easel.key_char(' '),
                    _ => {
                        if let Some(t) = &event.text {
                            for c in t.chars() {
                                easel.key_char(c);
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
                if let Some(rx) = &r.movie_rx {
                    if let Ok(res) = rx.try_recv() {
                        r.movie_rx = None;
                        match res {
                            Ok(m) => r.shell.easel.add_movie(m),
                            Err(e) => r.shell.easel.alert(e),
                        }
                    }
                }
                r.shell.update(dt);
                r.shell.draw(&mut r.frame);
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
        if r.shell.quit {
            r.shell.save_prefs();
            el.exit();
            return;
        }
        r.shell.pump();
        let actions: Vec<Action> = r.shell.take_actions();
        let mut abort = false;
        for a in actions {
            if abort {
                break;
            }
            let pictures = r.shell.easel.user_pictures.clone();
            let r_easel = &mut r.shell.easel;
            match a {
                Action::Save | Action::SaveAs | Action::SaveTo(_) => {
                    let path = match (&a, &r.save_path) {
                        (Action::SaveTo(p), _) => Some(p.clone()),
                        (Action::Save, Some(p)) => Some(p.clone()),
                        _ => {
                            // Kid Pix pictures, or (a Kid Pix Revival addition) a plain image.
                            let mut d = rfd::FileDialog::new()
                                .set_title("Save Picture As")
                                .add_filter("Kid Pix Picture (*.kpx)", &["kpx"])
                                .add_filter("PNG Image (*.png)", &["png"])
                                .add_filter("JPEG Image (*.jpg)", &["jpg", "jpeg"])
                                .add_filter("Bitmap Image (*.bmp)", &["bmp"])
                                .set_file_name("Untitled.kpx");
                            if let Some(dir) = pictures.as_ref().filter(|d| d.is_dir()) {
                                d = d.set_directory(dir);
                            }
                            d.save_file()
                        }
                    };
                    if path.is_none() {
                        abort = true;
                    }
                    if let Some(mut p) = path {
                        if p.extension().is_none() {
                            p.set_extension("kpx");
                        }
                        let res = save_picture(r_easel, &p);
                        match res {
                            Ok(()) => {
                                r_easel.canvas.dirty = false;
                                r.save_path = Some(p);
                            }
                            Err(e) => {
                                abort = true;
                                r_easel.alert(format!("Couldn't save the picture: {e}"));
                            }
                        }
                    }
                }
                Action::Open => {
                    if !true {
                        continue;
                    }
                    let mut dlg = rfd::FileDialog::new()
                        .set_title("Open a picture")
                        .add_filter("Pictures", &["png", "kpx", "PNG", "KPX"]);
                    if let Some(p) = pictures.clone().filter(|d| d.is_dir()).or_else(|| self.assets.path("Idea Machine")) {
                        dlg = dlg.set_directory(p);
                    }
                    if let Some(p) = dlg.pick_file() {
                        if open_page(r_easel, &p) {
                            r.save_path = Some(p.clone());
                        }
                    }
                }
                Action::New => {
                    if true {
                        r_easel.new_picture();
                        r.save_path = None;
                    }
                }
                Action::Export => {
                    let name = r.save_path.as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Untitled".into());
                    let mut dlg = rfd::FileDialog::new()
                        .set_title("Export Picture")
                        .add_filter("PNG Image (*.png)", &["png"])
                        .add_filter("JPEG Image (*.jpg)", &["jpg", "jpeg"])
                        .add_filter("Bitmap Image (*.bmp)", &["bmp"])
                        .set_file_name(format!("{name}.png"));
                    if let Some(dir) = pictures.as_ref().filter(|d| d.is_dir()) {
                        dlg = dlg.set_directory(dir);
                    }
                    if let Some(mut p) = dlg.save_file() {
                        if p.extension().is_none() {
                            p.set_extension("png");
                        }
                        let img = r_easel.flattened();
                        match formats::export(&img, &p) {
                            Ok(()) => {}
                            Err(e) => r_easel.alert(format!("Couldn't export the picture: {e}")),
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
                            Ok(img) => r_easel.import_picture(
                                img,
                                if a == Action::AddGraphic { easel::FitTarget::Graphic } else { easel::FitTarget::Background },
                            ),
                            Err(e) => r_easel.alert(e),
                        }
                    }
                }
                Action::ImportSound => {
                    if let Some(p) = rfd::FileDialog::new().set_title("Import Sound").add_filter("WAV sound", &["wav", "WAV"]).pick_file() {
                        match formats::load_wav(&p) {
                            Ok(s) => r_easel.set_page_sound(std::sync::Arc::new(s)),
                            Err(e) => r_easel.alert(e),
                        }
                    }
                }
                Action::IdeaMachine => {
                    if !true {
                        continue;
                    }
                    let mut dlg = rfd::FileDialog::new().set_title("Idea Machine").add_filter("Kid Pix picture", &["kpx", "KPX"]);
                    if let Some(p) = self.assets.path("Idea Machine") {
                        dlg = dlg.set_directory(p);
                    }
                    if let Some(p) = dlg.pick_file() {
                        // An Idea Machine page opens as a new, untitled picture.
                        if open_page(r_easel, &p) {
                            r.save_path = None;
                        }
                    }
                }
                Action::UserGuide => match self.assets.path("KpInstall/KdpxManual.pdf") {
                    Some(p) => {
                        let opener = if cfg!(target_os = "macos") { "open" } else if cfg!(windows) { "explorer" } else { "xdg-open" };
                        let _ = std::process::Command::new(opener).arg(p).spawn();
                    }
                    None => r_easel.alert("The User Guide wasn't found in your Kid Pix files."),
                },
                // Handled by the shell's graphical dialogs (Check Spelling is unavailable).
                Action::Login | Action::EditAbcText | Action::CheckSpelling | Action::SayItThisWay => {}
                Action::AddMovie => {
                    let mut dlg = rfd::FileDialog::new()
                        .set_title("Add a movie")
                        .add_filter("QuickTime movie", &["mov", "MOV"]);
                    if let Some(p) = self.assets.path("QTmovies") {
                        dlg = dlg.set_directory(p);
                    }
                    if let Some(p) = dlg.pick_file() {
                        r.movie_rx = Some(movie::load_async(p));
                    }
                }
                Action::PrintPoster(n) => {
                    let img = r_easel.flattened();
                    match print::print_mode(&img, n) {
                        Ok(_) => {}
                        Err(e) => r_easel.alert(format!("Couldn't print: {e}")),
                    }
                }
                Action::Quit => {
                    r.shell.save_prefs();
                    el.exit();
                }
                Action::GoSlideShow => r.shell.enter_picker(),
                Action::EmailSend { movie } => {
                    // The Kid Pix Email web service is gone: save the picture (JPEG) or the
                    // whole page (.kpx), then open the mail program and the file's folder.
                    let dir = pictures.clone().unwrap_or_else(std::env::temp_dir).join("Email");
                    let _ = std::fs::create_dir_all(&dir);
                    let name = r.save_path.as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "My Picture".into());
                    let file = dir.join(format!("{name}.{}", if movie { "kpx" } else { "jpg" }));
                    let saved = if movie { save_picture(r_easel, &file) } else { formats::export(&r_easel.flattened(), &file) };
                    match saved {
                        Ok(()) => {
                            let opener = if cfg!(target_os = "macos") { "open" } else if cfg!(windows) { "explorer" } else { "xdg-open" };
                            let _ = std::process::Command::new(opener).arg("mailto:?subject=A%20Kid%20Pix%20picture%20for%20you&body=I%20made%20this%20picture%20with%20Kid%20Pix!").spawn();
                            let _ = std::process::Command::new(opener).arg(&dir).spawn();
                        }
                        Err(e) => r_easel.alert(format!("Couldn't save the picture to send: {e}")),
                    }
                }
                Action::Help | Action::Flatten(_) | Action::SaveRecording | Action::EmailSetup => {}
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
        let hint = self.shell.cursor_hint();
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

/// Save the picture by extension: `.kpx` (the Kid Pix format, plus a PNG copy next to it
/// when Controls > PNG Copy is on), or a plain `.png` / `.jpg` / `.bmp` image of the whole
/// picture with its stickers and text (a Kid Pix Revival addition).
pub fn save_picture(easel: &mut Easel, p: &Path) -> Result<(), String> {
    let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    if matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "bmp") {
        let img = easel.flattened();
        return formats::export(&img, p);
    }
    let page = easel.to_kpx();
    kpxio::write(&page, p)?;
    easel.kpx_base = kpxio::read(p).ok();
    if easel.menu_state.png_copy {
        let img = easel.flattened();
        save_png(&p.with_extension("png"), &img).map_err(|e| format!("Couldn't save the PNG copy: {e}"))?;
    }
    Ok(())
}

/// Open a `.kpx` page (stickers, text, sound) or a plain picture into the easel.
fn open_page(easel: &mut Easel, p: &Path) -> bool {
    if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("kpx")) {
        match kpxio::read(p) {
            Ok(page) => {
                easel.load_kpx(page);
                return true;
            }
            Err(e) => eprintln!("kpx: {e}"),
        }
    }
    match load_picture(p) {
        Some(img) => {
            easel.load_picture(img);
            true
        }
        None => {
            let t = easel.ini_text("FileStrings", "CantOpenPictureFile", "Can't open the picture file, %s.").replace("%s", &p.file_name().unwrap_or_default().to_string_lossy());
            easel.alert(t);
            false
        }
    }
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
    movie::set_game_root(&root);
    let assets = Rc::new(Assets::new(root));
    // Debug: render scripts to a PNG: --dump-script <archive> <out.png> <id>...
    if let Some(i) = args.iter().position(|a| a == "--dump-script") {
        let arch = &args[i + 1];
        let out = PathBuf::from(&args[i + 2]);
        let mut frame = Image::filled(W, H, 0xff80_8080);
        for a in &args[i + 3..] {
            if let Some(b) = a.strip_prefix('b').and_then(|b| b.parse::<u16>().ok()) {
                if let Some(img) = assets.image(arch, b) {
                    gfx::blit(&mut frame, &img, 0, 0);
                }
            }
        }
        for id in args[i + 3..].iter().filter_map(|a| a.parse::<u16>().ok()) {
            match anim::Player::new(&assets, arch, id) {
                Some(mut p) => {
                    p.finish_now();
                    for (cel, _, _) in p.placed() {
                        if let Some(img) = p.sprites.cel(cel) {
                            let mask = img.pixels.iter().all(|px| px & 0x00ff_ffff == 0);
                            eprintln!("  cel {cel}: {}x{} {}", img.width, img.height, if mask { "8-bit mask" } else { "colour" });
                        }
                    }
                    eprintln!("{id}: bounds {:?} cels {:?} set: {} cels, regs {}/{}, last cel size {:?} regs_tail {:?} {:?}", p.bounds(), p.placed(), p.sprites.cels.len(), p.sprites.reg_x.len(), p.sprites.reg_y.len(), p.sprites.cels.last().map(|c| (c.width, c.height)), &p.sprites.reg_x[p.sprites.reg_x.len().saturating_sub(4)..], &p.sprites.reg_y[p.sprites.reg_y.len().saturating_sub(4)..]);
                    p.draw(&mut frame);
                }
                None => eprintln!("{id}: missing"),
            }
        }
        save_png(&out, &frame).expect("png");
        return;
    }
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
