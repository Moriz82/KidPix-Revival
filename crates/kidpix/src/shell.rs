//! The application shell around the Paint Zone (docs/RE_SHELL.md §1–§7): splash screen,
//! the graphical login dialog with the users folder, "save first?" prompts, alerts and
//! help mode. Owns the easel and filters its actions.

use crate::anim::Player;
use crate::assets::Assets;
use crate::audio::{self, Audio};
use crate::dialogs::{self, Dialog, Key, ListEvent};
use crate::easel::{Action, CursorHint, Easel};
use crate::gfx::Image;
use crate::picker::{Picker, PickerAction, PICKER_HELP};
use crate::slideplay::{PlayPrefs, SlidePlayer};
use mohawk::Rect;
use std::path::PathBuf;
use std::rc::Rc;

const SPLASH: &str = "Data/Splash.MHK";
const LOGIN: &str = "Data/RDlgLogin.MHK";
const WARN: &str = "Data/Warnings.MHK";
const DELETERS: &str = "Data/Deleters.MHK";
const ALERT: &str = "Data/RAlertDlg.MHK";
const HELPDLG: &str = "Data/RHelpDlg.MHK";
const EASEL_HELP: &str = "Data/EaselHelp.MHK";
const MISC: &str = "Data/Misc.MHK";
const PLACEHOLDER: &str = "Your Name";

pub struct User {
    pub name: String,
    pub dir: PathBuf,
}

struct SplashScreen {
    bg: Option<Player>,
    anim: Option<Player>,
}

enum Purpose {
    Login { from_menu: bool, admin: bool },
    AskSave(Action),
    DeleteUsers(Vec<String>),
    Help,
    Alert,
    /// Toolbox > Edit ABC Text (RABCTextDlg).
    AbcText,
    /// Toolbox > Say It This Way (RPronounceDlg).
    Pronounce,
    /// Toolbox > Flatten Text (225) / Flatten Stickers and Animations (214).
    Flatten(u16),
    /// Name a recorded sound (EnterSnd.MHK).
    NameSound,
    /// Email > Set Up: Parental Controls (RParentalDlg), with the checkbox state.
    Parental(bool),
    /// Small Kids Mode: name the page (EnterName.MHK), then save and maybe continue.
    NamePage(Option<Action>),
}

struct Modal {
    dlg: Dialog,
    purpose: Purpose,
}

struct HelpMode {
    intro: Option<Player>,
    spots: Vec<(u16, Rect)>,
    /// EaselHelp or PickerHelp (§7).
    archive: &'static str,
}

/// The current module (§1.1): Paint Zone, SlideShow organizer, slide-show player.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Easel,
    Picker,
    Player,
}

pub struct Shell {
    pub assets: Rc<Assets>,
    pub audio: Rc<Audio>,
    pub easel: Easel,
    splash: Option<SplashScreen>,
    modal: Option<Modal>,
    /// A dialog waiting under the current one (login under the delete confirm).
    stacked: Option<Modal>,
    help: Option<HelpMode>,
    help_voice: Option<u16>,
    pub user: Option<User>,
    /// Actions cleared by the shell (prompts answered); `main` carries them out.
    pub actions: Vec<Action>,
    pub quit: bool,
    mouse: (i32, i32),
    /// Folder holding `Users/` (the game folder; tests use a scratch dir).
    pub users_root: PathBuf,
    pub screen: Screen,
    /// The SlideShow organizer (created the first time it is shown).
    pub picker: Option<Picker>,
    pub player: Option<SlidePlayer>,
    /// Prefs `ProjectString` (last slide show) and `ProjectPosition` (strip scroll).
    last_show: Option<PathBuf>,
    project_pos: usize,
}

impl Shell {
    pub fn new(assets: Rc<Assets>, audio: Rc<Audio>, splash: bool) -> Shell {
        Self::with_users(assets.root.clone(), assets, audio, splash)
    }

    pub fn with_users(users_root: PathBuf, assets: Rc<Assets>, audio: Rc<Audio>, splash: bool) -> Shell {
        let easel = Easel::new(assets.clone(), audio.clone());
        let mut s = Shell {
            splash: None,
            modal: None,
            stacked: None,
            help: None,
            help_voice: None,
            user: None,
            actions: Vec::new(),
            quit: false,
            mouse: (0, 0),
            users_root,
            screen: Screen::Easel,
            picker: None,
            player: None,
            last_show: None,
            project_pos: 0,
            easel,
            assets,
            audio,
        };
        let show = splash && s.assets.ini.get("GeneralSettings", "ShowSplashScreen").is_none_or(|v| v.eq_ignore_ascii_case("true"));
        if show {
            let bg = Player::new(&s.assets, SPLASH, 1000).map(|mut p| {
                p.finish_now();
                p
            });
            let anim = Player::new(&s.assets, SPLASH, 1001);
            if bg.is_some() || anim.is_some() {
                s.splash = Some(SplashScreen { bg, anim });
            } else {
                s.open_login(false);
            }
        } else {
            s.open_login(false);
        }
        s
    }

    /// Headless tests: log in as `name` without the dialog.
    pub fn login_as(&mut self, name: &str) {
        self.splash = None;
        self.modal = None;
        self.set_user(name);
    }

    pub fn users_dir(&self) -> PathBuf {
        let d = self.assets.cfg.get("DirectoryOrganization", "DirUsers").unwrap_or("Users");
        self.users_root.join(d)
    }

    pub fn user_subdir(&self, key: &str, default: &str) -> Option<PathBuf> {
        let u = self.user.as_ref()?;
        let d = self.assets.cfg.get("DirectoryOrganization", key).unwrap_or(default);
        Some(u.dir.join(d))
    }

    fn list_users(&self) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(self.users_dir())
            .map(|rd| rd.flatten().filter(|e| e.path().is_dir()).map(|e| e.file_name().to_string_lossy().into_owned()).collect())
            .unwrap_or_default();
        v.sort_by_key(|s| s.to_lowercase());
        v
    }

    fn create_user(&self, name: &str) -> PathBuf {
        let dir = self.users_dir().join(name);
        let cfg = &self.assets.cfg;
        let mut subs = vec![];
        for (k, d) in [
            ("DirPictures", "My Pictures"),
            ("DirProjects", "My Pictures/My Slide Shows"),
            ("DirBackground", "My Backgrounds"),
            ("DirMovies", "My Movies"),
            ("DirSounds", "My Sounds"),
            ("DirStamps", "My Stamps"),
            ("DirBmpStickers", "My Bitmapstickers"),
            ("DirStickers", "My Stickers"),
            ("DirToons", "My Toons"),
        ] {
            subs.push(cfg.get("DirectoryOrganization", k).unwrap_or(d).to_string());
        }
        for s in subs {
            let _ = std::fs::create_dir_all(dir.join(s));
        }
        dir
    }

    fn set_user(&mut self, name: &str) {
        // Easel menu mode while the prefs are applied (Small Kids Mode is an Easel item);
        // the Picker is re-entered at the end when the login came from there.
        self.easel.menu_state.slideshow_mode = false;
        let dir = self.create_user(name);
        let prefs = std::fs::read_to_string(dir.join("user.ini")).map(|t| crate::assets::Ini::parse(&t)).unwrap_or_default();
        let flag = |k: &str, d: bool| prefs.get("GlobalUser", k).and_then(|v| v.trim().parse::<i32>().ok()).map(|v| v != 0).unwrap_or(d);
        let ms = &mut self.easel.menu_state;
        ms.text_to_speech = flag("TTS", true);
        ms.tool_sounds = flag("ToolSnds", true);
        ms.attached_sound = flag("AttachedSnds", true);
        ms.manual_advance = flag("ManualAdvance", false);
        ms.png_copy = flag("PngCopy", false);
        ms.email_enabled = flag("EmailEnabled", false);
        let small = flag("SmallKids", false);
        if small != self.easel.menu_state.small_kids {
            self.easel.menu_command(crate::menubar::cmd::SMALL_KIDS);
        }
        if let Some(s) = prefs.get("GlobalUser", "ABCString") {
            self.easel.abc_string = s.to_string();
        }
        if let Some(a) = prefs.get("SmallKidsAlphabet", "AlphabetSet").filter(|a| !a.is_empty()) {
            self.easel.alphabet = a.chars().collect();
        }
        if let Some(f) = prefs.get("SmallKidsAlphabet", "FontName") {
            self.easel.alphabet_font = f.to_string();
        }
        if let Some(z) = prefs.get_i32("SmallKidsAlphabet", "FontSize") {
            self.easel.alphabet_size = z as f32;
        }
        self.easel.user_dir = Some(dir.clone());
        self.easel.artist_name = name.to_string();
        self.easel.user_pictures = Some(dir.join(self.assets.cfg.get("DirectoryOrganization", "DirPictures").unwrap_or("My Pictures")));
        self.easel.words = std::fs::read_to_string(dir.join("Words.dic"))
            .map(|t| t.lines().filter_map(|l| l.split_once('=')).map(|(a, b)| (a.trim().to_string(), b.trim().to_string())).collect())
            .unwrap_or_default();
        self.project_pos = prefs.get_i32("GlobalUser", "ProjectPosition").map(|v| v.max(0) as usize).unwrap_or(0);
        self.last_show = prefs.get("GlobalUser", "ProjectString").filter(|s| !s.is_empty()).map(PathBuf::from);
        self.user = Some(User { name: name.to_string(), dir });
        if self.screen != Screen::Easel {
            // Logged in as someone else from the Picker: show their slide shows.
            self.picker = None;
            self.player = None;
            self.enter_picker();
        }
    }

    /// Persist the per-user prefs (`user.ini [GlobalUser]`).
    pub fn save_prefs(&self) {
        let Some(u) = &self.user else { return };
        let ms = &self.easel.menu_state;
        let b = |v: bool| if v { 1 } else { 0 };
        let text = format!(
            "[GlobalUser]\nTTS={}\nSmallKids={}\nToolSnds={}\nAttachedSnds={}\nPlayOnce=1\nManualAdvance={}\nABCString=\"{}\"\nPngCopy={}\nEmailEnabled={}\n",
            b(ms.text_to_speech),
            b(ms.small_kids),
            b(ms.tool_sounds),
            b(ms.attached_sound),
            b(ms.manual_advance),
            self.easel.abc_string,
            b(ms.png_copy),
            b(ms.email_enabled)
        );
        let (pos, show) = match &self.picker {
            Some(p) => (p.strip_pos, p.project.path.clone()),
            None => (self.project_pos, self.last_show.clone()),
        };
        let show = show.map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
        let text = format!("{text}ProjectPosition={pos}\nProjectString=\"{show}\"\n");
        let _ = std::fs::write(u.dir.join("user.ini"), text);
    }

    // ------------------------------------------------------------------ dialogs

    fn open_login(&mut self, from_menu: bool) {
        let mut d = Dialog::new(&self.assets, LOGIN, &[7600, 7601, 7604], &[777, 778]);
        d.set_group_hidden(1600, true);
        d.set_hidden(if from_menu { 1502 } else { 1395 }, true);
        if from_menu {
            // EXIT shares its spot with CANCEL in the login table only.
            for (i, b) in d.buttons.iter().enumerate() {
                if b.cmd == 1502 && b.group == 1600 {
                    d.hidden[i] = true;
                }
            }
        }
        let rect = Player::new(&self.assets, LOGIN, 7603).map(|p| p.bounds()).unwrap_or(Rect { left: 228, top: 136, right: 443, bottom: 166 });
        let mut f = dialogs::Field::new(rect, PLACEHOLDER);
        // [I] The code's colour bytes read as pure blue, which is unreadable on the
        // navy field art; white is used instead.
        f.color = crate::gfx::rgb(255, 255, 255);
        f.placeholder = true;
        f.legal = Some(
            self.assets
                .ini
                .get("LoginAlphabet", "LegalAlphabet")
                .unwrap_or("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789 ")
                .to_string(),
        );
        f.accept = Some((LOGIN.into(), 7600));
        f.reject = Some((MISC.into(), 31503));
        if let Some(u) = &self.user {
            f.set(&u.name);
        }
        d.field = Some(f);
        let mut l = dialogs::List::login(&self.assets, LOGIN);
        l.items = self.list_users();
        d.list = Some(l);
        self.modal = Some(Modal { dlg: d, purpose: Purpose::Login { from_menu, admin: false } });
        self.refresh_login();
    }

    fn refresh_login(&mut self) {
        let Some(m) = self.modal.as_mut() else { return };
        let Purpose::Login { admin, .. } = m.purpose else { return };
        let name = m.dlg.field.as_ref().map(|f| f.text.trim().to_string()).unwrap_or_default();
        let valid = !name.is_empty() && name != PLACEHOLDER;
        m.dlg.set_disabled(if admin { 1604 } else { 1500 }, !valid);
        if admin {
            let none = m.dlg.list.as_ref().is_none_or(|l| l.selected.is_empty());
            m.dlg.set_disabled(1606, none);
        }
    }

    fn ask_save(&mut self, follow: Action) {
        if !self.easel.canvas.dirty {
            self.actions.push(follow);
            return;
        }
        let art = match follow {
            Action::Quit => 7544,
            Action::New => 7546,
            Action::IdeaMachine => 7548,
            Action::Open => 7547,
            Action::Login => 7545,
            Action::GoSlideShow => 7542,
            _ => 7544,
        };
        let d = Dialog::new(&self.assets, WARN, &[7500, art], &[780]);
        self.modal = Some(Modal { dlg: d, purpose: Purpose::AskSave(follow) });
    }

    /// The generic alert box with a message drawn by code.
    pub fn alert(&mut self, text: &str) {
        let mut d = Dialog::new(&self.assets, ALERT, &[10000], &[777]);
        let r = Player::new(&self.assets, ALERT, 10005).map(|p| p.bounds()).unwrap_or(Rect { left: 284, top: 185, right: 554, bottom: 311 });
        d.body = Some((r, text.to_string(), 16.0));
        self.modal = Some(Modal { dlg: d, purpose: Purpose::Alert });
    }

    fn open_name_page(&mut self, then: Option<Action>) {
        const ARCH: &str = "Data/EnterName.MHK";
        let mut d = Dialog::new(&self.assets, ARCH, &[7200, 7211], &[777]);
        let r = Player::new(&self.assets, ARCH, 7201).map(|p| p.bounds()).unwrap_or(Rect { left: 273, top: 246, right: 529, bottom: 270 });
        let mut f = dialogs::Field::new(r, "Untitled");
        f.placeholder = true;
        f.color = crate::gfx::rgb(255, 255, 255);
        f.accept = Some((ARCH.into(), 7200));
        f.reject = Some((ARCH.into(), 7201));
        d.field = Some(f);
        self.modal = Some(Modal { dlg: d, purpose: Purpose::NamePage(then) });
    }

    fn open_abc_text(&mut self) {
        const ARCH: &str = "Data/RABCTextDlg.MHK";
        let mut d = Dialog::new(&self.assets, ARCH, &[8200], &[777]);
        d.set_hidden(1641, true);
        let r = Player::new(&self.assets, ARCH, 8213).map(|p| p.bounds()).unwrap_or(Rect { left: 242, top: 230, right: 555, bottom: 293 });
        let mut f = dialogs::Field::new(r, &self.easel.abc_string);
        f.color = crate::gfx::rgb(255, 255, 255);
        f.size = 18.0;
        d.field = Some(f);
        self.modal = Some(Modal { dlg: d, purpose: Purpose::AbcText });
    }

    fn open_pronounce(&mut self) {
        const ARCH: &str = "Data/RPronounceDlg.MHK";
        let mut d = Dialog::new(&self.assets, ARCH, &[6500], &[777]);
        let rect = |id, dflt| Player::new(&self.assets, ARCH, id).map(|p| p.bounds()).unwrap_or(dflt);
        let mut a = dialogs::Field::new(rect(6517, Rect { left: 250, top: 225, right: 459, bottom: 249 }), "");
        let mut b = dialogs::Field::new(rect(6518, Rect { left: 250, top: 288, right: 459, bottom: 312 }), "");
        a.color = crate::gfx::rgb(255, 255, 255);
        b.color = crate::gfx::rgb(255, 255, 255);
        d.field = Some(a);
        d.field2 = Some(b);
        self.modal = Some(Modal { dlg: d, purpose: Purpose::Pronounce });
    }

    /// Per-user pronunciation exceptions (`Words.dic`, one `word=say-as` per line).
    fn words_path(&self) -> Option<PathBuf> {
        self.user.as_ref().map(|u| u.dir.join("Words.dic"))
    }

    fn start_help(&mut self) {
        let mut spots = Vec::new();
        let (archive, tables) = match (&self.picker, self.screen) {
            // PickerHelp: 777 screen + 779 normal view / 778 effects view.
            (Some(p), Screen::Picker) => (PICKER_HELP, p.help_tables()),
            _ => (EASEL_HELP, self.easel.help_tables().into_iter().chain([777]).collect()),
        };
        for t in tables {
            spots.extend(hotspots(&self.assets, archive, t));
        }
        // [I] Script 10000 is the first hotspot's rect marker (EaselHelp: tool oval 1,
        // PickerHelp: the title, a yellow box); the rect scripts are hit areas, not
        // pictures, so help mode shows only the help cursors.
        self.help = Some(HelpMode { intro: None, spots, archive });
    }

    fn open_help(&mut self, cmd: u16) {
        self.help = None;
        let Some(key) = crate::helpkeys::HELP_KEYS.iter().find(|(c, _)| *c == cmd).map(|(_, k)| *k) else { return };
        let text = self.assets.ini.get("HelpStrings", key).unwrap_or("").to_string();
        let mut d = Dialog::new(&self.assets, HELPDLG, &[9000], &[777]);
        let r = Player::new(&self.assets, HELPDLG, 9001).map(|p| p.bounds()).unwrap_or(Rect { left: 230, top: 133, right: 570, bottom: 384 });
        d.body = Some((r, text, 18.0));
        self.modal = Some(Modal { dlg: d, purpose: Purpose::Help });
        self.help_voice = self.assets.cfg.get_i32("HelpSnds", key).map(|v| v as u16);
        self.play_help_voice();
    }

    fn play_help_voice(&self) {
        self.audio.stop_channel(audio::CH_FX);
        if let Some(id) = self.help_voice {
            if let Some(s) = self.assets.sound(HELPDLG, id) {
                self.audio.play(s, audio::CH_FX, false);
            }
        }
    }

    fn dialog_command(&mut self, cmd: u16) {
        let Some(m) = self.modal.as_mut() else { return };
        match &mut m.purpose {
            Purpose::Login { from_menu, admin } => {
                let (from_menu, is_admin) = (*from_menu, *admin);
                let name = m.dlg.field.as_ref().map(|f| f.text.trim().to_string()).unwrap_or_default();
                match cmd {
                    1500 | 1604 => {
                        if name.is_empty() || name == PLACEHOLDER {
                            return;
                        }
                        if !is_admin && name.eq_ignore_ascii_case("kidpixadmin") {
                            *admin = true;
                            m.dlg.set_group_hidden(1500, true);
                            m.dlg.set_group_hidden(1600, false);
                            if let Some(l) = m.dlg.list.as_mut() {
                                l.multi = true;
                                l.selected.clear();
                            }
                            if let Some(f) = m.dlg.field.as_mut() {
                                f.set("");
                            }
                            self.refresh_login();
                            return;
                        }
                        if is_admin {
                            self.create_user(&name);
                            let users = self.list_users();
                            if let Some(l) = self.modal.as_mut().and_then(|m| m.dlg.list.as_mut()) {
                                l.items = users;
                            }
                            self.refresh_login();
                            return;
                        }
                        self.modal = None;
                        self.set_user(&name);
                        if from_menu {
                            self.easel.new_picture();
                        }
                    }
                    1603 => {
                        *admin = false;
                        m.dlg.set_group_hidden(1600, true);
                        m.dlg.set_group_hidden(1500, false);
                        m.dlg.set_hidden(if from_menu { 1502 } else { 1395 }, true);
                        if let Some(l) = m.dlg.list.as_mut() {
                            l.multi = false;
                            l.selected.clear();
                        }
                        self.refresh_login();
                    }
                    1606 => {
                        let names: Vec<String> = m
                            .dlg
                            .list
                            .as_ref()
                            .map(|l| l.selected.iter().filter_map(|&i| l.items.get(i).cloned()).collect())
                            .unwrap_or_default();
                        if names.is_empty() {
                            return;
                        }
                        let art = if names.len() == 1 { 7809 } else { 7810 };
                        let d = Dialog::new(&self.assets, DELETERS, &[7800, art], &[777]);
                        // Stack: keep the login dialog underneath.
                        let login = self.modal.take();
                        self.stacked = login;
                        self.modal = Some(Modal { dlg: d, purpose: Purpose::DeleteUsers(names) });
                    }
                    1502 => {
                        self.modal = None;
                        self.quit = true;
                    }
                    dialogs::CANCEL => self.modal = None,
                    _ => {}
                }
            }
            Purpose::AskSave(follow) => {
                let follow = follow.clone();
                match cmd {
                    dialogs::YES => {
                        self.modal = None;
                        self.actions.push(Action::Save);
                        self.actions.push(follow);
                    }
                    dialogs::NO => {
                        self.modal = None;
                        self.actions.push(follow);
                    }
                    _ => self.modal = None,
                }
            }
            Purpose::DeleteUsers(names) => {
                let names = std::mem::take(names);
                if cmd == dialogs::OK {
                    let dir = self.users_dir();
                    for n in names.iter() {
                        let _ = std::fs::remove_dir_all(dir.join(n));
                    }
                }
                self.modal = self.stacked.take();
                let users = self.list_users();
                if let Some(l) = self.modal.as_mut().and_then(|m| m.dlg.list.as_mut()) {
                    l.items = users;
                    l.selected.clear();
                    l.top = l.top.min(l.items.len().saturating_sub(1));
                }
                self.refresh_login();
            }
            Purpose::Help => match cmd {
                dialogs::REPLAY => self.play_help_voice(),
                _ => {
                    self.audio.stop_channel(audio::CH_FX);
                    self.modal = None;
                }
            },
            Purpose::Alert => self.modal = None,
            Purpose::AbcText => {
                if cmd == dialogs::OK {
                    if let Some(f) = &m.dlg.field {
                        self.easel.abc_string = f.text.clone();
                    }
                    self.save_prefs();
                }
                self.modal = None;
            }
            Purpose::Pronounce => {
                let (a, b) = m.dlg.fields_in_order();
                let (orig, say) = (a.map(|f| f.text.trim().to_string()).unwrap_or_default(), b.map(|f| f.text.trim().to_string()).unwrap_or_default());
                match cmd {
                    1801 => {
                        if !orig.is_empty() && !say.is_empty() {
                            self.easel.words.retain(|(w, _)| !w.eq_ignore_ascii_case(&orig));
                            self.easel.words.push((orig, say));
                            if let Some(p) = self.words_path() {
                                let text: String = self.easel.words.iter().map(|(w, s)| format!("{w}={s}\n")).collect();
                                let _ = std::fs::write(p, text);
                            }
                        }
                    }
                    1802 => {
                        crate::speech::speak(&orig, 0);
                    }
                    1803 => {
                        crate::speech::speak(&say, 0);
                    }
                    _ => self.modal = None,
                }
            }
            Purpose::Parental(on) => match cmd {
                1760 => {
                    *on = !*on;
                    let v = *on;
                    m.dlg.set_selected(1760, v);
                }
                dialogs::OK => {
                    let v = *on;
                    self.easel.menu_state.email_enabled = v;
                    self.modal = None;
                    self.save_prefs();
                }
                _ => self.modal = None,
            },
            Purpose::NamePage(then) => {
                let then = then.take();
                if cmd == dialogs::OK {
                    let name = m.dlg.field.as_ref().map(|f| f.text.trim().to_string()).filter(|n| !n.is_empty()).unwrap_or_else(|| "Untitled".into());
                    let safe: String = name.chars().map(|c| if c.is_alphanumeric() || c == ' ' { c } else { '_' }).collect();
                    if let Some(dir) = self.easel.user_pictures.clone() {
                        let _ = std::fs::create_dir_all(&dir);
                        self.actions.push(Action::SaveTo(dir.join(format!("{safe}.kpx"))));
                        if let Some(a) = then {
                            self.actions.push(a);
                        }
                    }
                }
                self.modal = None;
            }
            Purpose::NameSound => {
                if cmd == dialogs::OK {
                    let name = m.dlg.field.as_ref().map(|f| f.text.trim().to_string()).filter(|n| !n.is_empty()).unwrap_or_else(|| "Untitled".into());
                    let dir = self.user_subdir("DirSounds", "My Sounds");
                    if let (Some(dir), Some(snd)) = (dir, self.easel.recording.clone()) {
                        let _ = std::fs::create_dir_all(&dir);
                        let safe: String = name.chars().map(|c| if c.is_alphanumeric() || c == ' ' { c } else { '_' }).collect();
                        if let Err(e) = crate::formats::save_wav(&dir.join(format!("{safe}.wav")), &snd) {
                            eprintln!("save sound: {e}");
                            let t = self.easel.ini_text("FileStrings", "GeneralFileErr", "An error occurred while saving this file.");
                            self.alert(&t);
                        }
                    }
                }
                self.modal = None;
            }
            Purpose::Flatten(what) => {
                let what = *what;
                match cmd {
                    1600 | 1601 => {
                        let all = cmd == 1601;
                        if what == 225 {
                            self.easel.flatten_texts_mode(all);
                        } else {
                            self.easel.flatten_objects_mode(all);
                        }
                        self.modal = None;
                    }
                    _ => self.modal = None,
                }
            }
        }
    }

    // ------------------------------------------------------------------ easel actions

    /// Move the easel's actions through the shell: prompts first where the original asks.
    pub fn pump(&mut self) {
        if self.modal.is_none() && !self.easel.alerts.is_empty() {
            let t = self.easel.alerts.remove(0);
            self.alert(&t);
        }
        let acts: Vec<Action> = std::mem::take(&mut self.easel.actions);
        for a in acts {
            // Small Kids Mode names pages with the graphical EnterName dialog.
            if self.easel.menu_state.small_kids {
                match a {
                    Action::Save | Action::SaveAs => {
                        self.open_name_page(None);
                        continue;
                    }
                    Action::New if self.easel.canvas.dirty => {
                        self.open_name_page(Some(Action::New));
                        continue;
                    }
                    _ => {}
                }
            }
            match a {
                Action::Quit | Action::New | Action::Open | Action::IdeaMachine | Action::GoSlideShow => self.ask_save(a),
                Action::Login => {
                    if self.easel.canvas.dirty {
                        self.ask_save(Action::Login);
                    } else {
                        self.open_login(true);
                    }
                }
                Action::Help => self.start_help(),
                Action::EditAbcText => self.open_abc_text(),
                Action::EmailSetup => {
                    let mut d = Dialog::new(&self.assets, "Data/RParentalDlg.MHK", &[9000], &[777]);
                    let on = self.easel.menu_state.email_enabled;
                    d.set_selected(1760, on);
                    self.modal = Some(Modal { dlg: d, purpose: Purpose::Parental(on) });
                }
                Action::SaveRecording => {
                    const ARCH: &str = "Data/EnterSnd.MHK";
                    let mut d = Dialog::new(&self.assets, ARCH, &[7200, 7210], &[777]);
                    let r = Player::new(&self.assets, ARCH, 7201).map(|p| p.bounds()).unwrap_or(Rect { left: 273, top: 246, right: 529, bottom: 270 });
                    let mut f = dialogs::Field::new(r, "Untitled");
                    f.placeholder = true;
                    f.accept = Some((ARCH.into(), 7200));
                    d.field = Some(f);
                    self.modal = Some(Modal { dlg: d, purpose: Purpose::NameSound });
                }
                Action::SayItThisWay => self.open_pronounce(),
                Action::Flatten(what) => {
                    let d = Dialog::new(&self.assets, "Data/RFlattenDlg.MHK", &[8000], &[777]);
                    self.modal = Some(Modal { dlg: d, purpose: Purpose::Flatten(what) });
                }
                other => self.actions.push(other),
            }
        }
        let acts = self.picker.as_mut().map(|p| std::mem::take(&mut p.actions)).unwrap_or_default();
        for a in acts {
            match a {
                PickerAction::GoEasel => self.leave_picker(),
                PickerAction::Play(start) => self.start_player(start),
                PickerAction::Help => self.start_help(),
                PickerAction::Login => self.open_login(true),
                PickerAction::Quit => self.quit = true,
                PickerAction::UserGuide => self.actions.push(Action::UserGuide),
            }
        }
    }

    // ------------------------------------------------------------------ slide show

    /// Switch to the SlideShow organizer (easel button 270 / Controls menu, after the
    /// save prompt). Menu mode 0x81: Picker-only items enabled.
    pub fn enter_picker(&mut self) {
        let pictures = self.easel.user_pictures.clone();
        let projects = self.user_subdir("DirProjects", "My Pictures/My Slide Shows");
        let first = self.picker.is_none();
        let picker = self.picker.get_or_insert_with(|| Picker::new(self.assets.clone(), self.audio.clone(), &self.easel.fonts));
        picker.enter(pictures, projects);
        picker.small_kids = self.easel.menu_state.small_kids;
        if first {
            picker.strip_pos = self.project_pos;
            picker.refresh_shows();
            if let Some(p) = self.last_show.as_ref().filter(|p| p.is_file()) {
                let _ = picker.open(p);
            }
        }
        self.easel.menu_state.slideshow_mode = true;
        self.screen = Screen::Picker;
    }

    /// Back to the Paint Zone (Picker 5567 / "Go to Paint Zone").
    pub fn leave_picker(&mut self) {
        self.player = None;
        self.easel.menu_state.slideshow_mode = false;
        self.screen = Screen::Easel;
    }

    fn start_player(&mut self, start: Option<usize>) {
        let Some(p) = self.picker.as_ref() else { return };
        let slides = p.playlist();
        if slides.is_empty() {
            return;
        }
        let ms = &self.easel.menu_state;
        let prefs = PlayPrefs {
            text_to_speech: ms.text_to_speech,
            attached_sound: ms.attached_sound,
            manual_advance: ms.manual_advance || p.project.manual_advance,
            // PLAY sets PlayOnce = 1 (§9.7).
            looping: false,
        };
        self.player = Some(SlidePlayer::new(self.assets.clone(), self.audio.clone(), slides, p.project.bg_color, start, prefs));
        self.screen = Screen::Player;
    }

    /// A menu command (accelerators from `main`) for the current screen.
    pub fn menu_command(&mut self, id: u16) {
        match self.screen {
            Screen::Easel => self.easel.menu_command(id),
            Screen::Picker => {
                if let Some(p) = self.picker.as_mut() {
                    p.menu_command(id, &mut self.easel.menu_state);
                }
            }
            Screen::Player => {}
        }
    }

    pub fn wheel(&mut self, steps: f32) {
        if self.busy() || self.help.is_some() {
            return;
        }
        match self.screen {
            Screen::Easel => self.easel.wheel(steps),
            Screen::Picker => {
                if let Some(p) = self.picker.as_mut() {
                    p.wheel(steps);
                }
            }
            Screen::Player => {}
        }
    }

    /// The window's close box.
    pub fn request_quit(&mut self) {
        match (self.screen, self.picker.as_mut()) {
            (Screen::Picker, Some(p)) => p.request_quit(),
            (Screen::Player, _) => self.quit = true,
            _ => self.easel.actions.push(Action::Quit),
        }
    }

    /// A key without a dialog meaning (function keys…): the player advances on any key.
    pub fn other_key(&mut self) {
        if let (Screen::Player, Some(p)) = (self.screen, self.player.as_mut()) {
            p.key(Key::Enter);
        }
    }

    /// Called by `main` for a guarded action after the prompt: login opens here.
    pub fn take_actions(&mut self) -> Vec<Action> {
        let mut out = Vec::new();
        for a in std::mem::take(&mut self.actions) {
            if a == Action::Login {
                self.open_login(true);
            } else {
                out.push(a);
            }
        }
        out
    }

    pub fn busy(&self) -> bool {
        self.splash.is_some() || self.modal.is_some()
    }

    // ------------------------------------------------------------------ input

    pub fn mouse_move(&mut self, x: i32, y: i32) {
        self.mouse = (x, y);
        if self.splash.is_some() {
            return;
        }
        if let Some(m) = self.modal.as_mut() {
            m.dlg.mouse_move(x, y, &self.assets, &self.audio);
            return;
        }
        if self.help.is_some() {
            return;
        }
        match self.screen {
            Screen::Easel => self.easel.mouse_move(x, y),
            Screen::Picker => {
                if let Some(p) = self.picker.as_mut() {
                    p.mouse_move(x, y, &self.easel.fonts, &self.easel.menu_state);
                }
            }
            Screen::Player => {}
        }
    }

    pub fn mouse_down(&mut self, x: i32, y: i32) {
        if self.splash.is_some() {
            self.end_splash();
            return;
        }
        if let Some(m) = self.modal.as_mut() {
            if let Some(ev) = m.dlg.mouse_down(x, y, &self.assets, &self.audio) {
                if let Purpose::Login { admin, .. } = m.purpose {
                    let (idx, dbl) = match ev {
                        ListEvent::Selected(i) => (i, false),
                        ListEvent::DoubleClick(i) => (i, true),
                    };
                    let name = m.dlg.list.as_ref().and_then(|l| l.items.get(idx).cloned()).unwrap_or_default();
                    if !admin {
                        if let Some(f) = m.dlg.field.as_mut() {
                            f.set(&name);
                        }
                    }
                    self.refresh_login();
                    if dbl && !admin {
                        self.dialog_command(1500);
                    }
                }
            }
            return;
        }
        if let Some(h) = &self.help {
            let hit = h.spots.iter().find(|(_, r)| r.contains(x, y)).map(|(c, _)| *c);
            if let Some(c) = hit {
                self.open_help(c);
            }
            return;
        }
        match self.screen {
            Screen::Easel => self.easel.mouse_down(x, y),
            Screen::Picker => {
                if let Some(p) = self.picker.as_mut() {
                    p.mouse_down(x, y);
                }
            }
            Screen::Player => {
                if let Some(p) = self.player.as_mut() {
                    p.mouse_down();
                }
            }
        }
    }

    pub fn mouse_up(&mut self, x: i32, y: i32) {
        if let Some(m) = self.modal.as_mut() {
            if let Some(cmd) = m.dlg.mouse_up(x, y) {
                self.dialog_command(cmd);
            }
            return;
        }
        if self.splash.is_some() || self.help.is_some() {
            return;
        }
        match self.screen {
            Screen::Easel => self.easel.mouse_up(x, y),
            Screen::Picker => {
                if let Some(p) = self.picker.as_mut() {
                    p.mouse_up(x, y, &self.easel.fonts, &mut self.easel.menu_state);
                }
            }
            Screen::Player => {}
        }
    }

    /// A key press. Returns true when the shell consumed it.
    pub fn key(&mut self, k: Key) -> bool {
        if self.splash.is_some() {
            self.end_splash();
            return true;
        }
        if let Some(m) = self.modal.as_mut() {
            if let Some(cmd) = m.dlg.key(k, &self.assets, &self.audio) {
                self.dialog_command(cmd);
            }
            self.refresh_login();
            return true;
        }
        if self.help.is_some() {
            if k == Key::Escape {
                self.help = None;
            }
            return true;
        }
        match self.screen {
            Screen::Easel => false,
            Screen::Picker => self.picker.as_mut().is_some_and(|p| p.key(k)),
            Screen::Player => {
                if let Some(p) = self.player.as_mut() {
                    p.key(k);
                }
                true
            }
        }
    }

    fn end_splash(&mut self) {
        if self.splash.take().is_some() {
            self.audio.stop_channel(audio::CH_MUSIC);
            self.open_login(false);
        }
    }

    pub fn update(&mut self, dt: f32) {
        if let Some(s) = self.splash.as_mut() {
            let mut done = s.anim.is_none();
            if let Some(a) = s.anim.as_mut() {
                let base = a.script.bitmap_set;
                for k in a.update(dt) {
                    if let Some(snd) = self.assets.sound(SPLASH, base + k) {
                        self.audio.play(snd, audio::CH_MUSIC, false);
                    }
                }
                done = a.finished;
            }
            if done {
                self.end_splash();
            }
            return;
        }
        if let Some(m) = self.modal.as_mut() {
            m.dlg.update(dt, &self.assets, &self.audio);
        }
        if let Some(h) = self.help.as_mut() {
            if let Some(p) = h.intro.as_mut() {
                let base = p.script.bitmap_set;
                for k in p.update(dt) {
                    if let Some(s) = self.assets.sound(h.archive, base + k) {
                        self.audio.play(s, audio::CH_UI, false);
                    }
                }
            }
        }
        if self.modal.is_none() {
            match self.screen {
                Screen::Easel => self.easel.update(dt),
                Screen::Picker => {
                    if let Some(p) = self.picker.as_mut() {
                        p.update(dt);
                    }
                }
                Screen::Player => {
                    if let Some(p) = self.player.as_mut() {
                        p.update(dt);
                    }
                    if self.player.as_ref().is_none_or(|p| p.done) {
                        self.player = None;
                        self.screen = Screen::Picker;
                    }
                }
            }
        }
    }

    pub fn draw(&self, frame: &mut Image) {
        crate::anim::set_tint(self.easel.color);
        if let Some(s) = &self.splash {
            frame.pixels.fill(0xff00_0000);
            if let Some(p) = &s.bg {
                p.draw(frame);
            }
            if let Some(p) = &s.anim {
                p.draw(frame);
            }
            return;
        }
        if self.user.is_none() && self.modal.as_ref().is_some_and(|m| matches!(m.purpose, Purpose::Login { from_menu: false, .. })) {
            frame.pixels.fill(0xff00_0000);
        } else {
            match (self.screen, &self.picker, &self.player) {
                (Screen::Picker, Some(p), _) => p.draw(frame, &self.easel.fonts, &self.easel.menu_state),
                (Screen::Player, _, Some(p)) => p.draw(frame),
                _ => self.easel.draw(frame),
            }
        }
        if let Some(h) = &self.help {
            if let Some(p) = &h.intro {
                p.draw(frame);
            }

        }
        if let Some(m) = &self.stacked {
            m.dlg.draw(frame, &self.easel.fonts);
        }
        if let Some(m) = &self.modal {
            m.dlg.draw(frame, &self.easel.fonts);
        }
    }

    pub fn cursor_hint(&self) -> CursorHint {
        if self.splash.is_some() || self.modal.is_some() {
            return CursorHint::Cur(28);
        }
        if let Some(h) = &self.help {
            let (x, y) = self.mouse;
            return CursorHint::Cur(if h.spots.iter().any(|(_, r)| r.contains(x, y)) { 23 } else { 22 });
        }
        match (self.screen, &self.picker) {
            (Screen::Picker, Some(p)) => p.cursor_hint(),
            (Screen::Player, _) => CursorHint::Cur(28),
            _ => self.easel.cursor_hint(),
        }
    }
}

/// Hotspot records of an MBTN table: (command, rect). Kind-5 records take their rect
/// from the script in field 10; buttons use their rest script's bounds.
pub fn hotspots(assets: &Assets, arch: &str, id: u16) -> Vec<(u16, Rect)> {
    let Some(list) = assets.archive(arch).and_then(|a| a.get(crate::assets::MBTN, id).map(mohawk::parse_buttons)).and_then(|r| r.ok()) else {
        return Vec::new();
    };
    list.into_iter()
        .filter_map(|b| {
            let sid = if b.raw[7] == 5 { b.raw[10] } else { b.idle };
            let r = assets.script(arch, sid)?.bounds;
            Some((b.command, r))
        })
        .collect()
}
