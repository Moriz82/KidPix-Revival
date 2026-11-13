//! Text-to-speech through the operating system's own voice (the original used the
//! Windows/Mac system speech engines too).

use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

static CURRENT: Mutex<Option<Child>> = Mutex::new(None);
static VOICE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// The voices KPD3's TTSServer offered (L&H TruVoice, docs/RE_SHELL.md §11), each mapped to
/// a matching voice of this system's speech engine.
pub const VOICES: [&str; 3] = ["Adult Male #1", "Adult Female #1", "Adult Female #2"];

pub fn voice() -> usize {
    VOICE.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn set_voice(i: usize) {
    VOICE.store(i % VOICES.len(), std::sync::atomic::Ordering::Relaxed);
}

fn command(text: &str, rate: i32) -> Option<Command> {
    let text: String = text.chars().filter(|c| !c.is_control() || *c == ' ').collect();
    if cfg!(target_os = "windows") {
        let esc = text.replace('\'', "''");
        let mut c = Command::new("powershell");
        c.args([
            "-NoProfile",
            "-Command",
            &format!(
                "Add-Type -AssemblyName System.Speech; $s = New-Object System.Speech.Synthesis.SpeechSynthesizer; try {{ $s.SelectVoiceByHints('{}', 'Adult', {}) }} catch {{}}; $s.Rate = {}; $s.Speak('{esc}')",
                if voice() == 0 { "Male" } else { "Female" },
                if voice() == 2 { 1 } else { 0 },
                rate.clamp(-10, 10)
            ),
        ]);
        Some(c)
    } else if cfg!(target_os = "macos") {
        let mut c = Command::new("say");
        c.args(["-v", ["Fred", "Samantha", "Victoria"][voice()], "-r", &(175 + rate * 15).to_string(), &text]);
        Some(c)
    } else {
        for (prog, rate_flag) in [("espeak-ng", "-s"), ("espeak", "-s"), ("spd-say", "-r")] {
            if which(prog) {
                let mut c = Command::new(prog);
                let r = if prog == "spd-say" { (rate * 10).to_string() } else { (165 + rate * 12).to_string() };
                c.args([rate_flag, &r]);
                if prog == "spd-say" {
                    c.arg("-w");
                    c.args(["-t", ["male1", "female1", "female2"][voice()]]);
                } else {
                    c.args(["-v", ["en+m1", "en+f1", "en+f2"][voice()]]);
                }
                c.arg(&text);
                return Some(c);
            }
        }
        None
    }
}

fn which(prog: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(prog).is_file()))
        .unwrap_or(false)
}

/// Is a speech engine available on this system?
pub fn available() -> bool {
    command("x", 0).is_some()
}

/// Speak `text` (stops anything still speaking). `rate`: -10 (slow) ..= 10 (fast).
pub fn speak(text: &str, rate: i32) -> bool {
    stop();
    let Some(mut c) = command(text, rate) else { return false };
    match c.stdout(Stdio::null()).stderr(Stdio::null()).spawn() {
        Ok(child) => {
            *CURRENT.lock().unwrap() = Some(child);
            true
        }
        Err(_) => false,
    }
}

pub fn speaking() -> bool {
    let mut g = CURRENT.lock().unwrap();
    match g.as_mut() {
        Some(c) => matches!(c.try_wait(), Ok(None)),
        None => false,
    }
}

pub fn stop() {
    if let Some(mut c) = CURRENT.lock().unwrap().take() {
        let _ = c.kill();
        let _ = c.wait();
    }
}
