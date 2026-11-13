//! QuickTime movies of the Kid Pix Deluxe 3 CD (`QTmovies/*.mov`): Sorenson Video 1
//! (`SVQ1`, 320×240 YUV 4:1:0) with IMA 4:1 ADPCM (`ima4`) sound.
//!
//! * [`mov`] reads the track tables of a `.mov` file.
//! * [`svq1`] decodes Sorenson Video 1. Its codebooks and variable-length codes are not
//!   part of this crate: [`svq1::Tables::from_qtx`] reads them from the Sorenson
//!   compressor inside the CD's own `INSTALL/QT4/QuickTime/QuickTimeAuthoring.qtx`.
//! * [`ima4`] decodes QuickTime IMA ADPCM.
//! * [`Movie`] ties them together: decoded RGB frames plus the sound track.

pub mod ima4;
pub mod mov;
pub mod svq1;

use std::path::Path;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Format(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "{e}"),
            Error::Format(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Error {
        Error::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

pub(crate) fn bad<T>(s: impl Into<String>) -> Result<T> {
    Err(Error::Format(s.into()))
}

/// A decoded movie.
pub struct Movie {
    pub width: u32,
    pub height: u32,
    /// Frames per second (media time scale / mean sample duration).
    pub fps: f32,
    /// Frames as 0xRRGGBB.
    pub frames: Vec<Vec<u32>>,
    /// Interleaved 16-bit samples.
    pub audio: Vec<i16>,
    pub audio_rate: u32,
    pub audio_channels: u16,
}

impl Movie {
    /// Decode the whole movie (video with `tables`).
    pub fn decode(path: impl AsRef<Path>, tables: &svq1::Tables) -> Result<Movie> {
        let data = std::fs::read(path)?;
        Self::decode_bytes(&data, tables)
    }

    pub fn decode_bytes(data: &[u8], tables: &svq1::Tables) -> Result<Movie> {
        let file = mov::File::parse(data)?;
        let video = file.tracks.iter().find(|t| t.is_video()).ok_or(Error::Format("no video track".into()))?;
        if &video.format != b"SVQ1" {
            return bad(format!("unsupported video codec {:?}", String::from_utf8_lossy(&video.format)));
        }
        let mut dec = svq1::Decoder::new(tables);
        let mut frames = Vec::with_capacity(video.samples.len());
        let mut last: Option<Vec<u32>> = None;
        for s in &video.samples {
            let bytes = data.get(s.offset as usize..(s.offset + s.size) as usize).ok_or(Error::Format("sample outside the file".into()))?;
            match dec.decode(bytes) {
                Ok(pic) => {
                    let rgb = pic.to_rgb(video.width.max(1), video.height.max(1));
                    last = Some(rgb.clone());
                    frames.push(rgb);
                }
                // a damaged frame repeats the previous picture
                Err(_) if last.is_some() => frames.push(last.clone().unwrap()),
                Err(e) => return Err(e),
            }
        }
        let fps = video.fps();
        let (mut audio, mut audio_rate, mut audio_channels) = (Vec::new(), 0, 0);
        if let Some(a) = file.tracks.iter().find(|t| t.is_audio()) {
            audio_rate = a.sample_rate;
            audio_channels = a.channels;
            audio = a.decode_audio(data)?;
        }
        Ok(Movie { width: video.width, height: video.height, fps, frames, audio, audio_rate, audio_channels })
    }
}
