use kp_brushes::*;
use mohawk::Image;
use std::path::PathBuf;

fn game_root() -> Option<PathBuf> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../game");
    p.join("KpInstall").exists().then_some(p)
}

#[test]
fn real_cfg_parses() {
    let Some(root) = game_root() else {
        eprintln!("game dir missing, skipping");
        return;
    };
    let cfg = BrushConfig::load(&root).expect("KPD3.cfg");
    let p = cfg.params(BrushId::Airbrush);
    assert_eq!((p.brush_id, p.spacing, p.density_bias, p.spatter, p.oscillation, p.shadow_bias), (1001, 0, 4, 1, 2, 7));
    let b = cfg.params(BrushId::BristleBrush);
    assert_eq!((b.brush_id, b.dry_out_rate, b.smear_bias, b.shadow_bias), (1022, 20, 7, 10));
    let c1 = cfg.params(BrushId::CustomBrush(1));
    assert_eq!((c1.brush_id, c1.spacing, c1.density_bias, c1.oscillation), (1052, 1, 1, 3));
    assert_eq!((c1.shadow_offset, c1.shadow_bias, c1.glint_offset, c1.glint_bias), (7, 12, 3, 1));
    // lower-case `densitybias` is read (case-insensitive INI reader)
    assert_eq!(cfg.params(BrushId::CustomBrush(7)).density_bias, 0);
    assert_eq!(cfg.params(BrushId::CustomBrush(11)).density_bias, 2);
    let c12 = cfg.params(BrushId::CustomBrush(12));
    assert_eq!((c12.dry_out_rate, c12.dry_out_threshold), (10, 15));
    // BlurBrush reads [RealBrush]
    assert_eq!(cfg.params(BrushId::BlurBrush).brush_id, 1004);
    // Drippy's commented-out keys are ignored
    let d = cfg.params(BrushId::Drippy);
    assert_eq!((d.brush_id, d.spacing, d.glint_offset), (1013, 1, 0));
    // voice/hands-free extras; `acceleratonBias` typo is never found
    let t = cfg.params(BrushId::Touchless(4));
    assert_eq!((t.oscillation, t.amplitude_bias, t.frequency_bias, t.gravity_time, t.acceleration_bias), (50, 10, 1024, 10, 1));
    assert_eq!(cfg.params(BrushId::CustomVoiceBrush(2)).frequency_bias, 1256);

    let m = cfg.sounds(BrushId::Madona);
    assert!(m.continuous && !m.lingers);
    assert_eq!((m.start, m.loop_, m.stop), (0, 7374, 7375));
    let rb = cfg.sounds(BrushId::RealBrush);
    assert_eq!((rb.start, rb.north, rb.east, rb.south, rb.west), (7289, 7295, 7292, 7296, 7293));
    assert_eq!(cfg.sounds(BrushId::Drippy).fx, 7380);
    assert_eq!(cfg.sounds(BrushId::Bricks).fx, 7353);
    // Spiral shares its name with transition 129, so it finds [SpiralSnd] (start 7224)
    assert_eq!(cfg.sounds(BrushId::Spiral).start, 7224);
    assert!(cfg.sounds(BrushId::CustomVoiceBrush(3)).is_silent());
    // [GreyScaleBrushSnd] matches GreyscaleBrush case-insensitively
    assert_eq!(cfg.sounds(BrushId::GreyscaleBrush).loop_, 7256);
    let air = cfg.sounds(BrushId::Airbrush);
    assert_eq!((air.loop_, air.stop, air.south, air.west, air.continuous), (7279, 7325, 7322, 7322, true));
    // "off" ids are 0
    assert_eq!(cfg.sounds(BrushId::SmearBrush).loop_, 0);
}

fn ctx_for(assets: &BrushAssets) -> Ctx<'_> {
    let mut c = Ctx::new(assets);
    c.color = [30, 120, 220];
    c.size = 2;
    c.artist_name = "Ada";
    c.voice = Voice::tone(400.0, 6000.0);
    c
}

/// White canvas; wand brushes that are no-ops on white (blur, greyscale, …) get stripes.
fn canvas_for(id: BrushId) -> Image {
    let mut img = Image::filled(600, 452, 0xffff_ffff);
    if WAND.contains(&id) {
        for y in 0..452 {
            for x in 0..600 {
                if (x / 6 + y / 9) % 3 == 0 {
                    img.put(x, y, 0xffe0_4020);
                } else if (x / 6 + y / 9) % 3 == 1 {
                    img.put(x, y, 0xff20_8040);
                }
            }
        }
    }
    img
}

fn run_stroke(id: BrushId, assets: &BrushAssets, cfg: &BrushConfig) -> (Image, Image, usize) {
    let ctx = ctx_for(assets);
    let mut cv = canvas_for(id);
    let before = cv.clone();
    let mut tool = BrushTool::new(id, cfg, 1);
    let mut n = tool.activate(&mut cv, &ctx).len();
    n += tool.begin(&mut cv, 150.0, 200.0, &ctx).len();
    for i in 1..=90 {
        let t = i as f32 / 90.0;
        let (x, y) = (150.0 + 300.0 * t, 200.0 + 60.0 * (t * 6.0).sin());
        n += tool.tick(&mut cv, x, y, 16, &ctx).len();
        if i % 15 == 0 {
            // hold still for a moment
            for _ in 0..4 {
                n += tool.idle(&mut cv, 16, &ctx).len();
            }
        }
    }
    n += tool.end(&mut cv, 450.0, 200.0, &ctx).len();
    let mut guard = 0;
    while tool.is_animating() && guard < 2000 {
        n += tool.idle(&mut cv, 16, &ctx).len();
        guard += 1;
    }
    let mut ov = cv.clone();
    tool.overlay(&mut ov);
    n += tool.deactivate(&mut cv, &ctx).len();
    (before, cv, n)
}

fn smoke(assets: &BrushAssets, cfg: &BrushConfig) {
    let mut unchanged = Vec::new();
    for id in BrushId::all() {
        let (before, after, _) = run_stroke(id, assets, cfg);
        assert!(after.pixels.iter().all(|p| p >> 24 == 0xff), "{id:?} broke opacity");
        let changed = before.pixels.iter().zip(&after.pixels).filter(|(a, b)| a != b).count();
        // Touchless4 only inverts ovals grown by the oscillation, which is 0 without
        // the CD's KPD3.cfg (as in the original)
        if changed == 0 && !(id == BrushId::Touchless(4) && cfg.params(id).oscillation == 0) {
            unchanged.push(id);
        }
    }
    assert!(unchanged.is_empty(), "brushes that did not change the canvas: {unchanged:?}");
}

#[test]
fn every_brush_draws_without_game() {
    smoke(&BrushAssets::empty(), &BrushConfig::default());
}

#[test]
fn every_brush_draws_with_game() {
    let Some(root) = game_root() else {
        eprintln!("game dir missing, skipping");
        return;
    };
    let cfg = BrushConfig::load(&root).unwrap();
    smoke(&BrushAssets::new(&root), &cfg);
}

#[test]
fn deterministic_with_same_seed() {
    let assets = BrushAssets::empty();
    let cfg = BrushConfig::default();
    for id in [BrushId::Airbrush, BrushId::Garland, BrushId::Splat, BrushId::Particle] {
        let a = run_stroke(id, &assets, &cfg).1;
        let b = run_stroke(id, &assets, &cfg).1;
        assert!(a == b, "{id:?} not deterministic");
    }
}

#[test]
fn real_pads_load() {
    let Some(root) = game_root() else {
        return;
    };
    let assets = BrushAssets::new(&root);
    let p = assets.pad(1001, None);
    assert_eq!((p.w, p.h, p.side), (10, 10, 10));
    assert_eq!(*p.a.iter().max().unwrap(), 82);
    let p = assets.pad(1004, None);
    assert_eq!((p.w, p.h, p.side), (14, 13, 14));
    let h = assets.pad(1031, Some(33));
    assert_eq!(h.side, 33);
    // 16-bit pad (black disc on white) → coverage high in the middle, 0 at the corner
    let p = assets.pad(1008, None);
    assert!(p.at(p.w / 2, p.h / 2) > 200 && p.at(0, 0) < 20);
    assert!(assets.texture(1).is_some());
    let s = assets.sound_info(7256).unwrap();
    assert!(s.looping);
}

#[test]
fn pad_engine_exact_single_stamp() {
    // One Chalk-like stamp with densityBias 1 and no RNG-dependent features:
    // t = a/254, d = trunc(c*t + d*(1-t)) on white.
    let assets = BrushAssets::empty();
    let mut cfg_text = String::from("[CustomBrush2]\nbrushID = 1001\nspacing = 1\n");
    cfg_text.push_str("[CustomBrush2Snd]\n");
    let cfg = BrushConfig::parse(&cfg_text);
    let mut ctx = Ctx::new(&assets);
    ctx.color = [0, 0, 0];
    ctx.size = 1;
    let mut cv = Image::filled(600, 452, 0xffff_ffff);
    let mut tool = BrushTool::new(BrushId::CustomBrush(2), &cfg, 1);
    tool.begin(&mut cv, 100.0, 100.0, &ctx);
    let pad = assets.pad(1001, None);
    let side = pad.side;
    let (l, t) = (100 - side / 2, 100 - side / 2);
    for py in 0..side {
        for px in 0..side {
            let a = pad.at(px, py) as f64;
            let want = if a == 0.0 { 31 } else { (31.0 * (1.0 - a / 254.0)) as i32 };
            let got = color::argb_to_rgb5(cv.get(l + px, t + py).unwrap());
            assert_eq!(got, [want, want, want], "pixel {px},{py} a={a}");
        }
    }
}
