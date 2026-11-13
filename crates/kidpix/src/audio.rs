//! Minimal cpal-based mixer: fire-and-forget voices with linear resampling.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use mohawk::Sound;
use std::sync::{Arc, Mutex};

struct Voice {
    sound: Arc<Sound>,
    /// Position in source frames (fractional).
    pos: f64,
    step: f64,
    looping: bool,
    channel: u8,
    volume: f32,
}

#[derive(Default)]
struct Shared {
    voices: Vec<Voice>,
    next_id: u64,
}

pub struct Audio {
    shared: Arc<Mutex<Shared>>,
    rate: u32,
    _stream: Option<cpal::Stream>,
}

/// Mixer channels: a new sound on an exclusive channel stops the previous one there.
pub const CH_FX: u8 = 0;
pub const CH_UI: u8 = 1;
pub const CH_TOOL: u8 = 2;
pub const CH_MUSIC: u8 = 3;

impl Audio {
    pub fn new() -> Audio {
        let shared = Arc::new(Mutex::new(Shared::default()));
        match Self::open(shared.clone()) {
            Ok((stream, rate)) => Audio {
                shared,
                rate,
                _stream: Some(stream),
            },
            Err(e) => {
                eprintln!("warning: audio disabled: {e}");
                Audio {
                    shared,
                    rate: 44100,
                    _stream: None,
                }
            }
        }
    }

    /// A mixer that discards everything (headless tests).
    pub fn silent() -> Audio {
        Audio {
            shared: Arc::new(Mutex::new(Shared::default())),
            rate: 44100,
            _stream: None,
        }
    }

    fn open(shared: Arc<Mutex<Shared>>) -> Result<(cpal::Stream, u32), String> {
        let host = cpal::default_host();
        let dev = host
            .default_output_device()
            .ok_or("no output device")?;
        let cfg = dev.default_output_config().map_err(|e| e.to_string())?;
        let rate = cfg.sample_rate().0;
        let channels = cfg.channels() as usize;
        let fmt = cfg.sample_format();
        let config: cpal::StreamConfig = cfg.into();
        let err = |e| eprintln!("audio stream error: {e}");
        let stream = match fmt {
            cpal::SampleFormat::F32 => dev.build_output_stream(
                &config,
                move |out: &mut [f32], _| mix(&shared, out, channels, |v| v),
                err,
                None,
            ),
            cpal::SampleFormat::I16 => dev.build_output_stream(
                &config,
                move |out: &mut [i16], _| mix(&shared, out, channels, |v| (v * 32767.0) as i16),
                err,
                None,
            ),
            cpal::SampleFormat::U16 => dev.build_output_stream(
                &config,
                move |out: &mut [u16], _| {
                    mix(&shared, out, channels, |v| ((v * 32767.0) as i32 + 32768) as u16)
                },
                err,
                None,
            ),
            f => return Err(format!("unsupported sample format {f:?}")),
        }
        .map_err(|e| e.to_string())?;
        stream.play().map_err(|e| e.to_string())?;
        Ok((stream, rate))
    }

    /// Play a sound. Exclusive channels (UI/TOOL/MUSIC) replace what's playing there.
    pub fn play(&self, sound: Arc<Sound>, channel: u8, looping: bool) -> u64 {
        let mut s = self.shared.lock().unwrap();
        if channel != CH_FX {
            s.voices.retain(|v| v.channel != channel);
        }
        s.next_id += 1;
        let id = s.next_id;
        let step = sound.sample_rate as f64 / self.rate as f64;
        s.voices.push(Voice {
            sound,
            pos: 0.0,
            step,
            looping,
            channel,
            volume: 0.9,
        });
        if s.voices.len() > 16 {
            s.voices.remove(0);
        }
        id
    }

    pub fn stop_channel(&self, channel: u8) {
        self.shared
            .lock()
            .unwrap()
            .voices
            .retain(|v| v.channel != channel);
    }

    pub fn channel_busy(&self, channel: u8) -> bool {
        self.shared
            .lock()
            .unwrap()
            .voices
            .iter()
            .any(|v| v.channel == channel)
    }
}

fn mix<T: Copy>(shared: &Mutex<Shared>, out: &mut [T], channels: usize, conv: impl Fn(f32) -> T) {
    let mut s = match shared.lock() {
        Ok(s) => s,
        Err(_) => return,
    };
    let frames = out.len() / channels.max(1);
    let mut buf = vec![0f32; frames * 2];
    for v in s.voices.iter_mut() {
        let sc = v.sound.channels.max(1) as usize;
        let total = v.sound.samples.len() / sc;
        if total == 0 {
            v.pos = f64::MAX;
            continue;
        }
        for f in 0..frames {
            if v.pos >= total as f64 {
                if v.looping {
                    v.pos -= total as f64;
                } else {
                    break;
                }
            }
            let i = v.pos as usize;
            let frac = (v.pos - i as f64) as f32;
            let j = (i + 1).min(total - 1);
            for c in 0..2 {
                let sc_i = c.min(sc - 1);
                let a = v.sound.samples[i * sc + sc_i] as f32;
                let b = v.sound.samples[j * sc + sc_i] as f32;
                buf[f * 2 + c] += (a + (b - a) * frac) / 32768.0 * v.volume;
            }
            v.pos += v.step;
        }
    }
    s.voices.retain(|v| {
        let total = v.sound.samples.len() / v.sound.channels.max(1) as usize;
        v.looping && total > 0 || v.pos < total as f64
    });
    for f in 0..frames {
        for c in 0..channels {
            let val = buf[f * 2 + c.min(1)].clamp(-1.0, 1.0);
            out[f * channels + c] = conv(val);
        }
    }
}
