#!/usr/bin/env python3
"""Draw the Kid Pix Revival app icon (original artwork, nothing from the CD) and write
packaging/icons/: icon.png (1024), icon-256.png, icon-512.png, kidpix-revival.ico,
kidpix-revival.icns.  Needs Pillow:  python3 scripts/make_icons.py"""
import io, math, os, struct
from PIL import Image, ImageDraw, ImageFilter

S = 2048  # supersampled canvas, scaled down to 1024
OUT = os.path.join(os.path.dirname(__file__), "..", "packaging", "icons")


def lerp(a, b, t):
    return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(len(a)))


def rounded_mask(size, radius):
    m = Image.new("L", (size, size), 0)
    ImageDraw.Draw(m).rounded_rectangle((0, 0, size - 1, size - 1), radius, fill=255)
    return m


def draw():
    pad = int(S * 0.06)
    tile = S - 2 * pad
    # background: diagonal violet -> sky gradient in a rounded square
    bg = Image.new("RGBA", (tile, tile))
    top, bot = (110, 52, 230), (36, 170, 245)
    px = bg.load()
    for y in range(tile):
        for x in range(0, tile):
            t = (x + y) / (2 * tile)
            px[x, y] = lerp(top, bot, t) + (255,)
    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    # soft drop shadow under the tile
    sh = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    ImageDraw.Draw(sh).rounded_rectangle((pad, pad + 30, pad + tile, pad + tile + 30), int(tile * 0.22), fill=(0, 0, 0, 110))
    img.alpha_composite(sh.filter(ImageFilter.GaussianBlur(28)))
    img.paste(bg, (pad, pad), rounded_mask(tile, int(tile * 0.22)))

    # tilted white paper card
    card = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    cw, ch = int(tile * 0.70), int(tile * 0.56)
    cx, cy = S // 2, int(S * 0.47)
    paper = Image.new("RGBA", (cw, ch), (0, 0, 0, 0))
    pd = ImageDraw.Draw(paper)
    pd.rounded_rectangle((0, 0, cw - 1, ch - 1), int(cw * 0.06), fill=(255, 253, 246, 255))
    # rainbow squiggle across the paper
    colors = [(235, 50, 60), (250, 145, 30), (250, 215, 40), (60, 190, 90), (40, 120, 235), (140, 70, 210)]
    band = ch * 0.055
    for k, c in enumerate(colors):
        r = band * 0.58
        for i in range(0, 1201):
            t = i / 1200
            x = cw * 0.08 + t * cw * 0.84
            y = ch * 0.30 + k * band + math.sin(t * math.pi * 2.2) * ch * 0.16
            pd.ellipse((x - r, y - r, x + r, y + r), fill=c + (255,))
    # three paint dots
    for (fx, fy, r, c) in [(0.18, 0.80, 0.07, (235, 50, 60)), (0.36, 0.84, 0.055, (250, 215, 40)), (0.52, 0.80, 0.06, (60, 190, 90))]:
        x, y, rr = cw * fx, ch * fy, cw * r
        pd.ellipse((x - rr, y - rr, x + rr, y + rr), fill=c + (255,))
    paper = paper.rotate(8, resample=Image.BICUBIC, expand=True)
    psh = Image.new("RGBA", paper.size, (0, 0, 0, 0))
    psh.putalpha(paper.split()[3].point(lambda a: int(a * 0.45)))
    card.alpha_composite(psh.filter(ImageFilter.GaussianBlur(20)), (cx - paper.width // 2 + 18, cy - paper.height // 2 + 26))
    card.alpha_composite(paper, (cx - paper.width // 2, cy - paper.height // 2))
    img.alpha_composite(card)

    # a chunky crayon lying diagonally at the lower right
    cr = Image.new("RGBA", (int(S * 0.62), int(S * 0.13)), (0, 0, 0, 0))
    d = ImageDraw.Draw(cr)
    w, h = cr.size
    d.rounded_rectangle((int(w * 0.16), 0, w - 1, h - 1), int(h * 0.35), fill=(255, 196, 30, 255))
    d.rectangle((int(w * 0.30), 0, int(w * 0.36), h - 1), fill=(225, 150, 10, 255))
    d.rectangle((int(w * 0.78), 0, int(w * 0.84), h - 1), fill=(225, 150, 10, 255))
    d.polygon([(int(w * 0.17), 0), (int(w * 0.17), h - 1), (0, h // 2)], fill=(240, 70, 140, 255))
    d.polygon([(int(w * 0.06), int(h * 0.36)), (int(w * 0.06), int(h * 0.64)), (0, h // 2)], fill=(170, 30, 90, 255))
    cr = cr.rotate(35, resample=Image.BICUBIC, expand=True)
    crs = Image.new("RGBA", cr.size, (0, 0, 0, 0))
    crs.putalpha(cr.split()[3].point(lambda a: int(a * 0.5)))
    ox, oy = int(S * 0.46), int(S * 0.50)
    img.alpha_composite(crs.filter(ImageFilter.GaussianBlur(16)), (ox + 16, oy + 22))
    img.alpha_composite(cr, (ox, oy))

    # a yellow star at the upper left
    st = ImageDraw.Draw(img)
    sx, sy, R = S * 0.24, S * 0.20, S * 0.085
    pts = []
    for i in range(10):
        a = -math.pi / 2 + i * math.pi / 5
        r = R if i % 2 == 0 else R * 0.45
        pts.append((sx + r * math.cos(a), sy + r * math.sin(a)))
    st.polygon(pts, fill=(255, 230, 60, 255), outline=(230, 160, 20, 255), width=10)
    return img.resize((1024, 1024), Image.LANCZOS)


def png_bytes(im):
    b = io.BytesIO()
    im.save(b, "PNG")
    return b.getvalue()


def write_ico(im, path, sizes=(16, 24, 32, 48, 64, 128, 256)):
    imgs = [png_bytes(im.resize((s, s), Image.LANCZOS)) for s in sizes]
    head = struct.pack("<HHH", 0, 1, len(sizes))
    off = 6 + 16 * len(sizes)
    dirs = b""
    for s, data in zip(sizes, imgs):
        dirs += struct.pack("<BBBBHHII", s % 256, s % 256, 0, 0, 1, 32, len(data), off)
        off += len(data)
    open(path, "wb").write(head + dirs + b"".join(imgs))


def write_icns(im, path):
    entries = [(b"icp4", 16), (b"icp5", 32), (b"icp6", 64), (b"ic07", 128), (b"ic08", 256), (b"ic09", 512), (b"ic10", 1024)]
    body = b""
    for tag, s in entries:
        data = png_bytes(im.resize((s, s), Image.LANCZOS))
        body += tag + struct.pack(">I", len(data) + 8) + data
    open(path, "wb").write(b"icns" + struct.pack(">I", len(body) + 8) + body)


if __name__ == "__main__":
    os.makedirs(OUT, exist_ok=True)
    icon = draw()
    icon.save(os.path.join(OUT, "icon.png"))
    icon.resize((512, 512), Image.LANCZOS).save(os.path.join(OUT, "icon-512.png"))
    icon.resize((256, 256), Image.LANCZOS).save(os.path.join(OUT, "icon-256.png"))
    write_ico(icon, os.path.join(OUT, "kidpix-revival.ico"))
    write_icns(icon, os.path.join(OUT, "kidpix-revival.icns"))
    print("wrote", os.path.abspath(OUT))
