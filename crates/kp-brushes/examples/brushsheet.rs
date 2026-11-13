//! Render every brush as one stroke onto a contact sheet.
//! usage: brushsheet <game-root> <out.png>
use kp_brushes::*;
use mohawk::Image;

const CW: u32 = 220;
const CH: u32 = 150;
const COLS: u32 = 8;
/// Where the cell's stroke happens on the full-size canvas.
const OX: i32 = 190;
const OY: i32 = 150;

fn save_png(path: &str, img: &Image) {
    let f = std::fs::File::create(path).expect("create png");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), img.width, img.height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&img.to_rgba8()).unwrap();
}

fn is_drag(id: BrushId) -> bool {
    use BrushId::*;
    matches!(id, Poly | Window | Door | DoorPlain | Roof | Rainbow | ColorTile)
}

/// Canvas the cell is drawn on: white, or a colourful test picture for the wand.
fn base_canvas(id: BrushId) -> Image {
    let mut img = Image::filled(600, 452, 0xffff_ffff);
    if WAND.contains(&id) {
        for y in 0..452i32 {
            for x in 0..600i32 {
                let (cx, cy) = (x - OX, y - OY);
                let r = (cx * 255 / CW as i32).clamp(0, 255) as u32;
                let b = (cy * 255 / CH as i32).clamp(0, 255) as u32;
                let g = if (cx / 12 + cy / 12) % 2 == 0 { 200 } else { 60 };
                img.put(x, y, 0xff00_0000 | r << 16 | g << 8 | b);
            }
        }
    }
    img
}

/// Stroke path in cell coordinates for frame i of n.
fn path(id: BrushId, i: usize, n: usize) -> (f32, f32) {
    use BrushId::*;
    let t = i as f32 / n as f32;
    match id {
        Poly => {
            // a slow loop so vertices get fixed every second
            let a = t * 5.5;
            (110.0 + 70.0 * a.cos(), 75.0 + 50.0 * a.sin())
        }
        _ if is_drag(id) => (30.0 + 160.0 * t, 30.0 + 105.0 * t),
        Madona => (110.0 + 45.0 * t, 85.0 - 40.0 * t),
        Tree => (110.0 + 0.0 * t, 135.0),
        Spiral => (110.0 + 3.0 * (t * 20.0).sin(), 85.0),
        // move, then hold still so the blob grows
        LeakyPen => (40.0 + 140.0 * (t * 2.0).min(1.0), 85.0 + 25.0 * ((t * 2.0).min(1.0) * 6.0).sin()),
        Bricks | Log => (20.0 + 180.0 * t, 85.0 + 40.0 * (t * 9.0).sin()),
        Particle => (60.0 + 100.0 * t, 145.0),
        _ => (25.0 + 170.0 * t, 85.0 + 35.0 * (t * 7.0).sin()),
    }
}

fn render(id: BrushId, assets: &BrushAssets, cfg: &BrushConfig) -> Image {
    let mut ctx = Ctx::new(assets);
    ctx.color = match id {
        BrushId::Marker | BrushId::Crayon | BrushId::Chalk => [200, 40, 40],
        _ => [40, 90, 220],
    };
    ctx.size = 2;
    ctx.artist_name = "Kid Pix";
    let mut cv = base_canvas(id);
    let mut tool = BrushTool::new(id, cfg, 0x1234_5678);
    let n = match id {
        BrushId::DotToDot => 120,
        BrushId::Poly => 240,
        _ => 90,
    };
    let voice = |i: usize| {
        let f = i as f32;
        Voice::tone(150.0 + 900.0 * (f * 0.11).sin().abs(), 1500.0 + 6000.0 * (f * 0.23).sin().abs())
    };
    let p0 = path(id, 0, n);
    ctx.voice = voice(0);
    tool.activate(&mut cv, &ctx);
    tool.begin(&mut cv, OX as f32 + p0.0, OY as f32 + p0.1, &ctx);
    for i in 1..=n {
        let p = path(id, i, n);
        ctx.voice = voice(i);
        tool.tick(&mut cv, OX as f32 + p.0, OY as f32 + p.1, 16, &ctx);
    }
    let p = path(id, n, n);
    tool.end(&mut cv, OX as f32 + p.0, OY as f32 + p.1, &ctx);
    let mut guard = 0;
    let limit = if matches!(id, BrushId::Touchless(_)) { 0 } else { 3000 };
    while tool.is_animating() && guard < limit {
        tool.idle(&mut cv, 16, &ctx);
        guard += 1;
    }
    tool.deactivate(&mut cv, &ctx);
    let mut cell = Image::new(CW, CH);
    for y in 0..CH {
        for x in 0..CW {
            cell.pixels[(y * CW + x) as usize] = cv.get(OX + x as i32, OY + y as i32).unwrap();
        }
    }
    cell
}

fn label(id: BrushId) -> String {
    match id {
        BrushId::SprayGun(k) => format!("SprayGun {}", 24000 + 100 * k as u32),
        BrushId::CyclicBitmap(k) => format!("Cyclic {}", 24000 + 100 * k as u32),
        _ => format!("{} ({})", id.name(), id.tool_id()),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: brushsheet <game-root> <out.png>");
        std::process::exit(2);
    }
    let root = std::path::Path::new(&args[1]);
    let assets = BrushAssets::new(root);
    let cfg = BrushConfig::load(root).unwrap_or_else(|e| {
        eprintln!("warning: no KPD3.cfg ({e}), using defaults");
        BrushConfig::default()
    });
    let ids = BrushId::all();
    let rows = (ids.len() as u32).div_ceil(COLS);
    let lh = 14;
    let mut sheet = Image::filled(COLS * CW, rows * (CH + lh), 0xff40_4040);
    for (i, &id) in ids.iter().enumerate() {
        let t0 = std::time::Instant::now();
        let cell = render(id, &assets, &cfg);
        let (cx, cy) = ((i as u32 % COLS) * CW, (i as u32 / COLS) * (CH + lh));
        for y in 0..CH {
            for x in 0..CW {
                let p = cell.pixels[(y * CW + x) as usize];
                let edge = x == CW - 1 || y == CH - 1;
                sheet.put((cx + x) as i32, (cy + lh + y) as i32, if edge { 0xff80_8080 } else { p });
            }
        }
        raster::text(&mut sheet, None, &label(id).to_uppercase(), 8.0, cx as i32 + 3, cy as i32 + 3, [31, 31, 31]);
        eprintln!("{:<28} {:>6.1} ms", label(id), t0.elapsed().as_secs_f64() * 1000.0);
    }
    save_png(&args[2], &sheet);
    println!("wrote {} ({} brushes)", args[2], ids.len());
}
