//! The tool runtime (RE_BRUSHES §2): event flow, timing, sounds, factory.

use crate::assets::BrushAssets;
use crate::config::{BrushConfig, BrushParams, SoundParams};
use crate::ids::BrushId;
use crate::pad::{Effect, PadEngine, PadKind};
use crate::rng::MsRand;
use crate::sound::{SoundEvent, SoundTracker};
use mohawk::Image;

/// Live input state handed to every call.
#[derive(Clone, Debug)]
pub struct Ctx<'a> {
    pub assets: &'a BrushAssets,
    /// Current palette colour (24-bit).
    pub color: [u8; 3],
    /// Panel size button: 1 = small, 2 = medium, 3 = large (`trunc(panel size)`).
    pub size: u8,
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    /// Space bar. Pad brushes sample it at mouse down (texture / effect inversion),
    /// Wacky Brushes read it live.
    pub space: bool,
    /// Easel texture picker index `n` (0 = none) → Easel.MHK tBMP 17000+n (§3.7).
    pub texture: u16,
    /// Sound Art input (§6): see [`Voice`].
    pub voice: Voice,
    /// Current artist's name (ABC brush, §5.17).
    pub artist_name: &'a str,
    /// Pencil shape: `false` = round (MBTN 18201), `true` = square (18202) (§4) [I].
    pub pencil_square: bool,
}

impl<'a> Ctx<'a> {
    pub fn new(assets: &'a BrushAssets) -> Ctx<'a> {
        Ctx {
            assets,
            color: [0, 0, 0],
            size: 2,
            shift: false,
            ctrl: false,
            alt: false,
            space: false,
            texture: 0,
            voice: Voice::default(),
            artist_name: "",
            pencil_square: false,
        }
    }
}

/// Microphone analysis for the Sound Art brushes (§6.1), as the original computes it:
/// 22,050 Hz mono, a 1024-point FFT over the latest 1024 samples (no window), bin
/// magnitude `trunc(|X_k| / sqrt(1024) / 256)` for bins 0..512 (`@0x464120`); `freq` =
/// the first strongest bin, `amp` = its magnitude (either 0 = silence).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Voice {
    /// The microphone is recording. Without it the original skips the brush tick
    /// entirely (nothing is drawn).
    pub live: bool,
    pub freq: u16,
    pub amp: u16,
    /// Bin magnitudes 0..512 (empty when not live).
    pub spectrum: std::sync::Arc<[u16]>,
}

impl Voice {
    pub const RATE: u32 = 22_050;
    pub const WINDOW: usize = 1024;

    /// Analyse the latest samples recorded at 22,050 Hz (fewer than 1024 are padded with
    /// silence in front, as the original's sliding buffer starts zeroed).
    pub fn analyze(samples: &[i16]) -> Voice {
        let n = Self::WINDOW;
        let mut re = vec![0.0f64; n];
        let tail = &samples[samples.len().saturating_sub(n)..];
        for (i, &v) in tail.iter().enumerate() {
            re[n - tail.len() + i] = v as f64;
        }
        let mut im = vec![0.0f64; n];
        fft(&mut re, &mut im);
        let norm = (n as f64).sqrt();
        let mut spectrum = vec![0u16; n / 2];
        let (mut freq, mut amp) = (0u16, 0u16);
        for k in 0..n / 2 {
            let m = ((re[k] * re[k] + im[k] * im[k]).sqrt() / norm * 0.003_906_25) as i64 as u16;
            spectrum[k] = m;
            if m > amp {
                amp = m;
                freq = k as u16;
            }
        }
        Voice { live: true, freq, amp, spectrum: spectrum.into() }
    }

    /// [`Voice::analyze`] for samples recorded at another rate (linear resampling).
    pub fn analyze_at(samples: &[i16], rate: u32) -> Voice {
        if rate == Self::RATE || rate == 0 {
            return Self::analyze(samples);
        }
        let step = rate as f64 / Self::RATE as f64;
        let need = ((Self::WINDOW as f64) * step).ceil() as usize + 2;
        let src = &samples[samples.len().saturating_sub(need)..];
        let mut out = Vec::with_capacity(Self::WINDOW);
        let mut t = src.len() as f64 - 1.0 - (Self::WINDOW as f64 - 1.0) * step;
        for _ in 0..Self::WINDOW {
            if t < 0.0 {
                out.push(0);
            } else {
                let i = t as usize;
                let fr = t - i as f64;
                let a = src[i] as f64;
                let b = *src.get(i + 1).unwrap_or(&src[i]) as f64;
                out.push((a + (b - a) * fr) as i16);
            }
            t += step;
        }
        Self::analyze(&out)
    }

    /// A pure tone of `hz` with peak `amplitude` (sample units), analysed as if spoken
    /// into the microphone — for tests and previews.
    pub fn tone(hz: f32, amplitude: f32) -> Voice {
        let w = 2.0 * std::f64::consts::PI * hz as f64 / Self::RATE as f64;
        let s: Vec<i16> = (0..Self::WINDOW).map(|i| (amplitude as f64 * (w * i as f64).sin()) as i16).collect();
        Self::analyze(&s)
    }

    /// Magnitude of bin `i` (0 outside the spectrum).
    pub fn bin(&self, i: usize) -> u16 {
        self.spectrum.get(i).copied().unwrap_or(0)
    }
}

/// In-place iterative radix-2 FFT (length a power of two).
fn fft(re: &mut [f64], im: &mut [f64]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * std::f64::consts::PI / len as f64;
        for start in (0..n).step_by(len) {
            for k in 0..len / 2 {
                let (wr, wi) = ((ang * k as f64).cos(), (ang * k as f64).sin());
                let (a, b) = (start + k, start + k + len / 2);
                let (xr, xi) = (re[b] * wr - im[b] * wi, re[b] * wi + im[b] * wr);
                re[b] = re[a] - xr;
                im[b] = im[a] - xi;
                re[a] += xr;
                im[a] += xi;
            }
        }
        len <<= 1;
    }
}

/// Internal per-call context.
pub(crate) struct Cx<'a, 'b> {
    pub ctx: &'a Ctx<'b>,
    pub rng: &'a mut MsRand,
    pub now: u64,
    pub snd: &'a mut SoundTracker,
    pub ev: &'a mut Vec<SoundEvent>,
    pub size_table: [i32; 3],
}

impl Cx<'_, '_> {
    /// Brush size from the class size table for the current panel size (§2.2).
    pub fn size(&self) -> i32 {
        self.size_table[(self.ctx.size.clamp(1, 3) - 1) as usize]
    }
    /// Play the tool's special effect (if not already playing).
    pub fn fx(&mut self) {
        self.snd.fx(self.now, self.ctx.assets, self.ev);
    }
}

/// What every brush implements (the original's `v5/v8/v6` plus background animation).
pub(crate) trait BrushImpl: Send {
    /// `v5`: mouse down.
    fn down(&mut self, cv: &mut Image, p: (i32, i32), cx: &mut Cx);
    /// `v8`: every tick while the button is held (also when the mouse did not move).
    fn held(&mut self, cv: &mut Image, p: (i32, i32), cx: &mut Cx);
    /// `v6`: mouse up.
    fn up(&mut self, _cv: &mut Image, _p: (i32, i32), _cx: &mut Cx) {}
    /// Background animation (drips, particles, sprites), also after mouse up.
    fn animate(&mut self, _cv: &mut Image, _dt: u32, _cx: &mut Cx) {}
    fn animating(&self) -> bool {
        false
    }
    /// Transient drawing on top of the canvas (not part of the picture).
    fn overlay(&self, _img: &mut Image) {}
    /// Tool switch: make everything that is still running permanent.
    fn deactivate(&mut self, _cv: &mut Image, _cx: &mut Cx) {}
    /// The brush was selected (`v3`): hands-free brushes start painting here.
    fn activate(&mut self, _cv: &mut Image, _cx: &mut Cx) {}
}

/// One brush tool instance (the original's tool object). Create it when the user
/// selects a brush and keep it while the brush stays selected: some state deliberately
/// survives strokes (TriSpin's angle, RealBrush's dried pad, Spiral's colours, sounds).
///
/// Driving it (see the crate docs for the full contract):
///
/// ```text
/// mouse down          → begin(canvas, x, y, ctx)
/// every frame, held   → tick(canvas, x, y, dt_ms, ctx)   (also when the mouse is still)
/// mouse up            → end(canvas, x, y, ctx)
/// every frame, not held, while is_animating() → idle(canvas, dt_ms, ctx)
/// display             → canvas, then overlay(&mut copy) if has_overlay()
/// tool switch         → deactivate(canvas, ctx)
/// ```
pub struct BrushTool {
    pub id: BrushId,
    pub params: BrushParams,
    pub sound_params: SoundParams,
    /// The brush's `rand()` state (the original shares one global generator; seed it
    /// from the clock like `srand(timeGetTime())`, or fixed for reproducible output).
    pub rng: MsRand,
    now: u64,
    held: bool,
    last: (i32, i32),
    snd: SoundTracker,
    imp: Box<dyn BrushImpl>,
}

/// Name used by the task description for the stroke driver.
pub type Stroke = BrushTool;

impl BrushTool {
    pub fn new(id: BrushId, config: &BrushConfig, seed: u32) -> BrushTool {
        let params = config.params(id);
        let sound_params = config.sounds(id);
        let mut rng = MsRand::new(seed);
        let imp = make_impl(id, params, &mut rng);
        BrushTool { id, params, sound_params, rng, now: 0, held: false, last: (0, 0), snd: SoundTracker::new(sound_params), imp }
    }

    /// Tool clock in milliseconds (advanced only by `tick`/`idle`).
    pub fn now_ms(&self) -> u64 {
        self.now
    }

    pub fn is_held(&self) -> bool {
        self.held
    }

    fn run<R>(&mut self, ctx: &Ctx, f: impl FnOnce(&mut dyn BrushImpl, &mut Cx) -> R) -> (R, Vec<SoundEvent>) {
        let mut ev = Vec::new();
        let size_table = self.id.size_table();
        let mut cx = Cx { ctx, rng: &mut self.rng, now: self.now, snd: &mut self.snd, ev: &mut ev, size_table };
        let r = f(self.imp.as_mut(), &mut cx);
        (r, ev)
    }

    fn pt(x: f32, y: f32) -> (i32, i32) {
        (x.floor() as i32, y.floor() as i32)
    }

    /// Mouse down at canvas coordinates (x, y).
    pub fn begin(&mut self, canvas: &mut Image, x: f32, y: f32, ctx: &Ctx) -> Vec<SoundEvent> {
        let p = Self::pt(x, y);
        self.held = true;
        self.last = p;
        let now = self.now;
        let (_, ev) = self.run(ctx, |imp, cx| {
            imp.down(canvas, p, cx);
            cx.snd.mouse_down(p, now, cx.ctx.assets, cx.ev);
        });
        ev
    }

    /// One idle tick of the original while the button is held: advance the clock by
    /// `dt_ms`, update sounds (`v20`), then let the brush act (`v8`) with the mouse at
    /// (x, y), then run background animation. Call it every frame (≈ 60 Hz) even when the
    /// mouse is still — spacing-0 brushes, timed brushes and sprays depend on it.
    pub fn tick(&mut self, canvas: &mut Image, x: f32, y: f32, dt_ms: u32, ctx: &Ctx) -> Vec<SoundEvent> {
        self.now += dt_ms as u64;
        if !self.held {
            return self.animate(canvas, dt_ms, ctx);
        }
        let p = Self::pt(x, y);
        self.last = p;
        let now = self.now;
        let (_, ev) = self.run(ctx, |imp, cx| {
            cx.snd.tick(p, now, cx.ctx.assets, cx.ev);
            imp.held(canvas, p, cx);
            imp.animate(canvas, dt_ms, cx);
        });
        ev
    }

    /// Mouse moved (no time passes): `tick(x, y, 0)`.
    pub fn drag(&mut self, canvas: &mut Image, x: f32, y: f32, ctx: &Ctx) -> Vec<SoundEvent> {
        self.tick(canvas, x, y, 0, ctx)
    }

    /// Time passes without a new mouse position: while held, a tick at the last point;
    /// after release, only background animation (drips, particles, spray sprites).
    pub fn idle(&mut self, canvas: &mut Image, dt_ms: u32, ctx: &Ctx) -> Vec<SoundEvent> {
        if self.held {
            let (x, y) = (self.last.0 as f32, self.last.1 as f32);
            self.tick(canvas, x, y, dt_ms, ctx)
        } else {
            self.now += dt_ms as u64;
            self.animate(canvas, dt_ms, ctx)
        }
    }

    fn animate(&mut self, canvas: &mut Image, dt_ms: u32, ctx: &Ctx) -> Vec<SoundEvent> {
        self.run(ctx, |imp, cx| imp.animate(canvas, dt_ms, cx)).1
    }

    /// Mouse up.
    pub fn end(&mut self, canvas: &mut Image, x: f32, y: f32, ctx: &Ctx) -> Vec<SoundEvent> {
        let p = Self::pt(x, y);
        let now = self.now;
        self.held = false;
        self.run(ctx, |imp, cx| {
            imp.up(canvas, p, cx);
            cx.snd.mouse_up(now, cx.ctx.assets, cx.ev);
        })
        .1
    }

    /// Whether `idle` should keep being called after mouse up.
    pub fn is_animating(&self) -> bool {
        self.imp.animating()
    }

    /// Draw transient previews (live particles, running spray sprites) onto a copy of the
    /// canvas used for display. Never call it on the picture itself.
    pub fn overlay(&self, target: &mut Image) {
        self.imp.overlay(target);
    }

    /// The brush has just been selected. Hands-free Sound Art (`Touchless`) starts painting
    /// from the canvas centre right away (RE_BRUSHES §6.3); then keep calling `idle`
    /// while [`BrushTool::is_animating`]. Other brushes ignore it.
    pub fn activate(&mut self, canvas: &mut Image, ctx: &Ctx) -> Vec<SoundEvent> {
        self.run(ctx, |imp, cx| imp.activate(canvas, cx)).1
    }

    /// The tool is being switched away: finish animations permanently, stop sounds.
    pub fn deactivate(&mut self, canvas: &mut Image, ctx: &Ctx) -> Vec<SoundEvent> {
        let now = self.now;
        self.held = false;
        self.run(ctx, |imp, cx| {
            imp.deactivate(canvas, cx);
            cx.snd.stop_all(now, cx.ev);
        })
        .1
    }
}

fn make_impl(id: BrushId, p: BrushParams, rng: &mut MsRand) -> Box<dyn BrushImpl> {
    use crate::wacky as w;
    use BrushId::*;
    let pad = |k: PadKind| -> Box<dyn BrushImpl> { Box::new(PadEngine::new(k, p)) };
    match id {
        RealBrush => pad(PadKind::RealBrush),
        Airbrush => pad(PadKind::Airbrush),
        BristleBrush => pad(PadKind::Bristle),
        ExtrusionBrush => pad(PadKind::Extrusion),
        CustomBrush(_) | Chalk => pad(PadKind::Plain),
        Crayon => pad(PadKind::Crayon),
        Marker => pad(PadKind::Marker),
        Drippy => pad(PadKind::Drippy),
        SmearBrush => pad(PadKind::Smear),
        BlurBrush => pad(PadKind::Blur),
        GreyscaleBrush => pad(PadKind::Greyscale),
        ColorizeBrush => pad(PadKind::Colorize),
        XRayBrush => pad(PadKind::Effect(Effect::XRay)),
        EmbossBrush => pad(PadKind::Effect(Effect::Emboss)),
        EdgeBrush => pad(PadKind::Effect(Effect::Edge)),
        NuclearBrush => pad(PadKind::Effect(Effect::Nuclear)),
        SolarizeBrush => pad(PadKind::Effect(Effect::Solarize)),
        HueShiftBrush => pad(PadKind::Effect(Effect::HueShift)),
        PosterizeBrush => pad(PadKind::Effect(Effect::Posterize)),
        MosaicBrush => pad(PadKind::Mosaic),
        Circle => Box::new(crate::pencil::Pencil::new()),
        CustomVoiceBrush(n) => Box::new(crate::voice::VoiceBrush::new(p, n, false)),
        Touchless(n) => Box::new(crate::voice::VoiceBrush::new(p, n, true)),
        SprayGun(k) => Box::new(crate::spray::SprayGun::new(k)),
        CyclicBitmap(k) => Box::new(crate::spray::Cyclic::new(k)),
        Madona => Box::new(w::Madona::default()),
        Spiral => Box::new(w::Spiral::new(rng)),
        Poly => Box::new(w::Poly::default()),
        TriSpin => Box::new(w::TriSpin::default()),
        WhispyCircle => Box::new(w::Whispy::default()),
        Particle => Box::new(w::Particles::new()),
        Garland => Box::new(w::Garland::default()),
        LeakyPen => Box::new(w::LeakyPen::default()),
        ScratchyPen => Box::new(w::Scratchy::default()),
        LoopingLine => Box::new(w::Looping::new(0.4)),
        LoopingLine2 => Box::new(w::Looping::new(3.0)),
        Bricks => Box::new(w::Tiles::new(true)),
        Log => Box::new(w::Tiles::new(false)),
        Window => Box::new(w::Window::default()),
        Door => Box::new(w::Door::new(false)),
        DoorPlain => Box::new(w::Door::new(true)),
        Roof => Box::new(w::Roof::default()),
        DotToDot => Box::new(w::DotToDot::default()),
        Abc => Box::new(w::Abc::default()),
        Rainbow => Box::new(w::Rainbow::default()),
        ColorTile => Box::new(w::ColorTile::default()),
        EightWayMirror => Box::new(w::Mirror8::default()),
        Splat => Box::new(w::Splat::default()),
        Tree => Box::new(w::Tree::default()),
    }
}
