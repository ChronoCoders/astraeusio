#!/usr/bin/env python3
"""
Generate per-blog-post OG images. Run once after adding a new post:

    python frontend/scripts/gen-blog-og.py

Reads blog post slugs and titles from posts.js (parsed via a tiny regex)
and writes 1200x630 PNGs to frontend/public/blog/og-<slug>.png.
"""
import re
import sys
from pathlib import Path
from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parent.parent
POSTS = ROOT / "src" / "blog" / "posts.js"
OUT_DIR = ROOT / "public" / "blog"
W, H = 1200, 630
BG = (9, 9, 11)            # zinc-950
FG = (244, 244, 245)       # zinc-100
DIM = (161, 161, 170)      # zinc-400
ACCENT = (251, 146, 60)    # orange-400

def find_font(candidates, size):
    for name in candidates:
        try:
            return ImageFont.truetype(name, size=size)
        except OSError:
            continue
    return ImageFont.load_default()

TITLE_FONT = find_font([
    "C:/Windows/Fonts/seguibl.ttf",   # Segoe UI Black on Windows
    "C:/Windows/Fonts/segoeuib.ttf",  # Segoe UI Bold
    "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
], 64)
MARK_FONT = find_font([
    "C:/Windows/Fonts/consolab.ttf",
    "C:/Windows/Fonts/consola.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
], 22)
TAG_FONT = find_font([
    "C:/Windows/Fonts/segoeui.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
], 28)

POST_RE = re.compile(
    r"slug:\s*'(?P<slug>[^']+)'.*?title:\s*'(?P<title>(?:[^'\\]|\\.)*)'",
    re.DOTALL,
)

def parse_posts():
    src = POSTS.read_text(encoding="utf-8")
    out = []
    for m in POST_RE.finditer(src):
        slug = m.group("slug")
        title = bytes(m.group("title"), "utf-8").decode("unicode_escape")
        out.append((slug, title))
    return out

def wrap(draw, text, font, max_w):
    words = text.split()
    lines, cur = [], ""
    for w in words:
        test = (cur + " " + w).strip()
        if draw.textlength(test, font=font) <= max_w:
            cur = test
        else:
            if cur:
                lines.append(cur)
            cur = w
    if cur:
        lines.append(cur)
    return lines

def draw_logo(d, x, y, size=44):
    cx, cy = x + size // 2, y + size // 2
    # Orbit ring (zinc-700)
    d.ellipse(
        [cx - size // 2, cy - size // 2, cx + size // 2, cy + size // 2],
        outline=(82, 82, 91), width=2,
    )
    # Sun (amber-400)
    r = size // 5
    d.ellipse([cx - r, cy - r, cx + r, cy + r], fill=(251, 191, 36))

def render(slug, title):
    img = Image.new("RGB", (W, H), BG)
    d = ImageDraw.Draw(img)
    # Top bar: logo + wordmark
    draw_logo(d, 80, 70)
    d.text((140, 80), "ASTRAEUSIO", font=MARK_FONT, fill=FG)
    # Accent line under wordmark
    d.rectangle([80, 145, 280, 147], fill=ACCENT)
    # Title (centered vertically in middle band)
    lines = wrap(d, title, TITLE_FONT, max_w=1040)
    line_h = 78
    total = line_h * len(lines)
    y = 280 - total // 2
    for line in lines:
        d.text((80, y), line, font=TITLE_FONT, fill=FG)
        y += line_h
    # Bottom: "From the Astraeusio Blog"
    d.text((80, H - 80), "From the Astraeusio Blog", font=TAG_FONT, fill=DIM)
    return img

def main():
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    posts = parse_posts()
    if not posts:
        print("No posts parsed.", file=sys.stderr)
        sys.exit(1)
    for slug, title in posts:
        out = OUT_DIR / f"og-{slug}.png"
        render(slug, title).save(out, "PNG", optimize=True)
        print(f"wrote {out.relative_to(ROOT)} ({title!r})")
    print(f"\nDone — {len(posts)} OG images generated.")

if __name__ == "__main__":
    main()
