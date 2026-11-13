//! The slide-show player (`RSlideShowModule` + sequencer `RTransition`, docs/RE_SHELL.md
//! §9.7): full 800×600 screen in the show's background colour, each slide's composite at
//! (100,74), the slide's transition with its transition sound, then a wait per advance mode.

use crate::assets::Assets;
use crate::audio::{self, Audio};
use crate::dialogs::Key;
use crate::gfx::{self, Image};
use crate::slideshow::{self, SlideRef, SlideSettings, NO_SOUND, SLIDE_H, SLIDE_W, TRANSITION_MS, TRANS_SOUNDS};
use std::rc::Rc;

/// Where the slide goes on the screen (`[CanvasSettings] slideLeft / slideTop`).
pub fn slide_origin(assets: &Assets) -> (i32, i32) {
    let l = assets.ini.get_i32("CanvasSettings", "slideLeft").unwrap_or(100);
    let t = assets.ini.get_i32("CanvasSettings", "slideTop").unwrap_or(74);
    (l, t)
}

/// Global prefs the player reads (§1.3).
#[derive(Clone, Copy, Debug)]
pub struct PlayPrefs {
    pub text_to_speech: bool,
    pub attached_sound: bool,
    pub manual_advance: bool,
    /// Sequencer loop flag (PlayOnce == 0); the Picker's PLAY button always plays once.
    pub looping: bool,
}

enum State {
    /// Load the slide at `pos` (skips files that can't be read).
    Fetch,
    Transition { t: f32 },
    Wait { elapsed: f32, speaking: bool },
    Done,
}

pub struct SlidePlayer {
    assets: Rc<Assets>,
    audio: Rc<Audio>,
    slides: Vec<SlideRef>,
    pos: usize,
    from_start: bool,
    prefs: PlayPrefs,
    bg: u32,
    origin: (i32, i32),
    shown: Image,
    next: Option<(Image, SlideSettings)>,
    next_sound: Option<std::sync::Arc<mohawk::Sound>>,
    next_text: String,
    mix: Image,
    state: State,
    /// The show ended or was stopped (Esc / Q): back to the Picker.
    pub done: bool,
}

impl SlidePlayer {
    /// `start` = first slide (index into the filled slides) when started from a slide.
    pub fn new(assets: Rc<Assets>, audio: Rc<Audio>, slides: Vec<SlideRef>, bg_color: usize, start: Option<usize>, prefs: PlayPrefs) -> SlidePlayer {
        let bg = slideshow::bg_color(bg_color);
        let origin = slide_origin(&assets);
        let pos = start.unwrap_or(0).min(slides.len().saturating_sub(1));
        SlidePlayer {
            origin,
            slides,
            pos,
            from_start: start.is_none_or(|s| s == 0),
            prefs,
            bg,
            shown: Image::filled(SLIDE_W, SLIDE_H, bg),
            next: None,
            next_sound: None,
            next_text: String::new(),
            mix: Image::new(SLIDE_W, SLIDE_H),
            state: State::Fetch,
            done: false,
            assets,
            audio,
        }
    }

    fn stop(&mut self) {
        self.audio.stop_channel(audio::CH_FX);
        self.audio.stop_channel(audio::CH_MUSIC);
        crate::speech::stop();
        self.state = State::Done;
        self.done = true;
    }

    fn settings(&self) -> SlideSettings {
        self.next.as_ref().map(|n| n.1).unwrap_or_default()
    }

    fn fetch(&mut self) {
        let mut tries = 0;
        while tries < self.slides.len() {
            let s = self.slides[self.pos].clone();
            if let Some(page) = slideshow::load_page(&s.path) {
                self.next = Some((page.composite, s.settings));
                self.next_sound = page.sound;
                self.next_text = page.texts.join("\n");
                break;
            }
            tries += 1;
            if !self.step_pos() {
                self.stop();
                return;
            }
        }
        if self.next.is_none() {
            self.stop();
            return;
        }
        let st = self.settings();
        // The previous page's sound and speech stop with the new slide [I].
        self.audio.stop_channel(audio::CH_MUSIC);
        crate::speech::stop();
        if st.sound != NO_SOUND {
            if let Some(w) = self.assets.sound(TRANS_SOUNDS, 12000 + st.sound) {
                self.audio.play(w, audio::CH_FX, false);
            }
        }
        self.state = State::Transition { t: 0.0 };
    }

    /// Advance the position; false at the end of a show that doesn't loop.
    fn step_pos(&mut self) -> bool {
        self.pos += 1;
        if self.pos >= self.slides.len() {
            if self.prefs.looping && self.from_start {
                self.pos = 0;
            } else {
                return false;
            }
        }
        true
    }

    fn start_page(&mut self) {
        let st = self.settings();
        if let Some((img, _)) = self.next.as_ref() {
            self.shown = img.clone();
        }
        let mut speaking = false;
        if st.overrides == 0 {
            if st.play_sound != 0 && self.prefs.attached_sound {
                if let Some(s) = self.next_sound.clone() {
                    self.audio.play(s, audio::CH_MUSIC, false);
                }
            }
            if st.play_text != 0 && self.prefs.text_to_speech && !self.next_text.trim().is_empty() {
                speaking = crate::speech::speak(&self.next_text, 0);
            }
        }
        self.state = State::Wait { elapsed: 0.0, speaking };
    }

    fn advance(&mut self) {
        if !self.step_pos() {
            self.stop();
            return;
        }
        self.state = State::Fetch;
    }

    pub fn update(&mut self, dt: f32) {
        match &mut self.state {
            State::Fetch => self.fetch(),
            State::Transition { t } => {
                *t += dt / TRANSITION_MS;
                let t = *t;
                if t >= 1.0 {
                    self.start_page();
                } else if let Some((img, st)) = &self.next {
                    slideshow::transition(st.transition as usize, t, &self.shown, img, &mut self.mix);
                }
            }
            State::Wait { elapsed, speaking } => {
                *elapsed += dt;
                let (el, spk) = (*elapsed, *speaking);
                let st = self.settings();
                if self.manual() {
                    return;
                }
                match st.advance {
                    0 if el >= st.seconds as f32 * 1000.0 => self.advance(),
                    3 if spk && el > 300.0 && !crate::speech::speaking() => self.advance(),
                    _ => {}
                }
            }
            State::Done => {}
        }
    }

    fn manual(&self) -> bool {
        self.prefs.manual_advance
    }

    fn waiting(&self) -> Option<bool> {
        match self.state {
            State::Wait { speaking, .. } => Some(speaking),
            _ => None,
        }
    }

    pub fn mouse_down(&mut self) {
        let Some(speaking) = self.waiting() else { return };
        let mode = self.settings().advance;
        // Read Text Done without text to read acts like Mouse Click.
        if self.manual() || mode == 1 || (mode == 3 && !speaking) {
            self.advance();
        }
    }

    pub fn key(&mut self, k: Key) {
        if matches!(k, Key::Escape | Key::Char('q') | Key::Char('Q')) {
            self.stop();
            return;
        }
        if self.waiting().is_some() && (self.manual() || self.settings().advance == 2) {
            self.advance();
        }
    }

    pub fn draw(&self, frame: &mut Image) {
        frame.pixels.fill(self.bg);
        let (x, y) = self.origin;
        match &self.state {
            State::Transition { t } if *t > 0.0 => gfx::blit(frame, &self.mix, x, y),
            _ => gfx::blit(frame, &self.shown, x, y),
        }
    }

    /// Headless tests: jump into the middle of the current transition.
    pub fn seek_transition(&mut self, k: f32) {
        if matches!(self.state, State::Fetch) {
            self.fetch();
        }
        if let State::Transition { t } = &mut self.state {
            *t = 0.0;
            self.update(k * TRANSITION_MS);
        }
    }
}
