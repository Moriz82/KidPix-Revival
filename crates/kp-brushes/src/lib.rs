//! Clean-room implementation of the Kid Pix Deluxe 3 brushes, following
//! `docs/RE_BRUSHES.md` (section numbers in comments refer to it). Confidence tags:
//! `[O]` observed in the original, `[I]` our reasonable choice where the notes are
//! inferred/undetermined.
//!
//! # Driving a brush
//!
//! ```no_run
//! use kp_brushes::*;
//! let assets = BrushAssets::new("game");
//! let config = BrushConfig::load(std::path::Path::new("game")).unwrap_or_default();
//! let mut canvas = mohawk::Image::filled(600, 452, 0xffff_ffff);
//! let mut tool = BrushTool::new(BrushId::Airbrush, &config, 12345);
//! let mut ctx = Ctx::new(&assets);
//! ctx.color = [200, 30, 30];
//! ctx.size = 2;
//! let mut sounds = tool.begin(&mut canvas, 100.0, 100.0, &ctx);   // mouse down
//! for i in 0..60 {                                                  // every frame while held
//!     sounds.extend(tool.tick(&mut canvas, 100.0 + i as f32 * 3.0, 100.0, 16, &ctx));
//! }
//! sounds.extend(tool.end(&mut canvas, 280.0, 100.0, &ctx));         // mouse up
//! while tool.is_animating() {                                       // drips, particles, sprays
//!     sounds.extend(tool.idle(&mut canvas, 16, &ctx));
//! }
//! ```
//!
//! * **Timing.** The original calls the tool on every idle tick while the button is down,
//!   moved or not (§2.1). Call [`BrushTool::tick`] once per frame with the current mouse
//!   position and the elapsed milliseconds. `spacing = 0` brushes (Airbrush,
//!   CustomBrush10/16, Greyscale, Sound Art) stamp once per tick when the mouse is still,
//!   so the frame rate sets their density (≈ 60 Hz recommended) [I]. Timed brushes and
//!   sprays use the tool clock (sum of `dt`).
//! * **After mouse up** keep calling [`BrushTool::idle`] while
//!   [`BrushTool::is_animating`] (Drippy drips, Particle, moving-paint spray).
//! * **Display.** Show the canvas, then [`BrushTool::overlay`] on a copy (live particles
//!   and spray sprites are not part of the picture until they finish).
//! * **Tool switch.** [`BrushTool::deactivate`] makes running animations permanent.
//! * **Sounds.** Every call returns [`SoundEvent`]s (play / play looping / stop a `tWAV`
//!   in the named archive) following §2.4.
//! * **Canvas.** Any opaque ARGB [`mohawk::Image`] (normally 600×452); brushes work in
//!   RGB555 internally (see [`color`] for the quantisation convention).

pub mod assets;
pub mod color;
pub mod config;
pub mod ids;
pub mod pad;
pub mod pencil;
pub mod raster;
pub mod rng;
pub mod sound;
pub mod spray;
pub mod tool;
pub mod voice;
pub mod wacky;

pub use assets::{BrushAssets, Pad, SoundInfo, SpriteSet, Texture};
pub use config::{BrushConfig, BrushParams, Ini, SoundParams};
pub use ids::{BrushId, PENCIL_MEDIA, REALISTIC, WACKY, WAND};
pub use rng::MsRand;
pub use sound::SoundEvent;
pub use tool::{BrushTool, Ctx, Stroke, Voice};
