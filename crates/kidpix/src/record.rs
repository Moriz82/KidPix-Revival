//! Microphone recording (cpal input) for the sound recorder.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use mohawk::Sound;
use std::sync::{Arc, Mutex};

pub struct Recorder {
    buf: Arc<Mutex<Vec<i16>>>,
    rate: u32,
    _stream: cpal::Stream,
    max_samples: usize,
}

impl Recorder {
    /// Start recording from the default input device (mono, device rate), at most `max_secs`.
    pub fn start(max_secs: f32) -> Result<Recorder, String> {
        let host = cpal::default_host();
        let dev = host.default_input_device().ok_or("no microphone found")?;
        let cfg = dev.default_input_config().map_err(|e| e.to_string())?;
        let rate = cfg.sample_rate().0;
        let ch = cfg.channels() as usize;
        let fmt = cfg.sample_format();
        let config: cpal::StreamConfig = cfg.into();
        let buf = Arc::new(Mutex::new(Vec::new()));
        let max_samples = (max_secs * rate as f32) as usize;
        let err = |e| eprintln!("recording error: {e}");
        macro_rules! build {
            ($t:ty, $conv:expr) => {{
                let b = buf.clone();
                dev.build_input_stream(
                    &config,
                    move |data: &[$t], _| {
                        let mut v = b.lock().unwrap();
                        for frame in data.chunks(ch.max(1)) {
                            if v.len() >= max_samples {
                                break;
                            }
                            let s: f32 = frame.iter().map(|&x| $conv(x)).sum::<f32>() / frame.len() as f32;
                            v.push((s.clamp(-1.0, 1.0) * 32767.0) as i16);
                        }
                    },
                    err,
                    None,
                )
            }};
        }
        let stream = match fmt {
            cpal::SampleFormat::F32 => build!(f32, |x: f32| x),
            cpal::SampleFormat::I16 => build!(i16, |x: i16| x as f32 / 32768.0),
            cpal::SampleFormat::U16 => build!(u16, |x: u16| (x as f32 - 32768.0) / 32768.0),
            f => return Err(format!("unsupported microphone format {f:?}")),
        }
        .map_err(|e| e.to_string())?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(Recorder { buf, rate, _stream: stream, max_samples })
    }

    pub fn seconds(&self) -> f32 {
        self.buf.lock().unwrap().len() as f32 / self.rate as f32
    }

    pub fn full(&self) -> bool {
        self.buf.lock().unwrap().len() >= self.max_samples
    }

    /// Peak level of the last 50 ms (0..1), for a level meter.
    pub fn level(&self) -> f32 {
        let v = self.buf.lock().unwrap();
        let n = (self.rate / 20) as usize;
        v.iter().rev().take(n).map(|s| (*s as f32).abs() / 32768.0).fold(0.0, f32::max)
    }

    pub fn finish(self) -> Sound {
        let samples = std::mem::take(&mut *self.buf.lock().unwrap());
        Sound { sample_rate: self.rate, channels: 1, samples, loop_count: 0 }
    }
}
