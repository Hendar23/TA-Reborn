"""Draws the example mod's pictures (docs/examples/example-mod/media): its banner, icon, gameplay
preview and screenshots, original vector art lettered in the bundled Sofia Sans, and its interface
theme's panel and bar pictures (media/hud).

    python tools/examples/example_mod_art.py [--out docs/examples/example-mod/media]

Needs resvg and ImageMagick (`magick`) on the PATH; `oxipng -o 3` the PNGs after.
"""
import argparse
import math
import os
import shutil
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
FONTS = [os.path.join(ROOT, "crates", "ta_ui", "fonts", name) for name in ("SofiaSansSemiCondensed.ttf", "SofiaSansExtraCondensed.ttf")]

AMBER = "#d8b456"
AMBER_BRIGHT = "#f0d27a"
STEEL = "#9e998a"
TEXT = "#e6e1d2"
DEEP = "#0d0e10"


def render(svg, png, width, height):
    """Renders an SVG to a PNG of `width` × `height` with resvg and the bundled fonts."""
    with tempfile.NamedTemporaryFile("w", suffix=".svg", delete=False, encoding="utf-8") as f:
        f.write(svg)
        source = f.name
    try:
        args = ["resvg", "-w", str(width), "-h", str(height)]
        for font in FONTS:
            args += ["--use-font-file", font]
        subprocess.run(args + [source, png], check=True)
    finally:
        os.remove(source)


def ridge(seed, y, height, width, step):
    """A row of hills along `y`, as an SVG path's points."""
    points = []
    for x in range(0, width + step, step):
        h = height * (0.5 + 0.3 * math.sin(x * 0.011 + seed) + 0.2 * math.sin(x * 0.037 + seed * 2.1))
        points.append(f"{x},{y - h:.1f}")
    return f"M0,{y} L" + " L".join(points) + f" L{width},{y} Z"


def floor_grid(width, top, bottom, color, opacity):
    """Lines of a grid on the ground, running to the horizon at `top`."""
    lines = []
    cx = width / 2
    for i in range(-14, 15):
        x = cx + i * 120
        lines.append(f'<line x1="{cx + i * 8:.1f}" y1="{top}" x2="{x:.1f}" y2="{bottom}" stroke="{color}" stroke-opacity="{opacity}" stroke-width="1.5"/>')
    for k in range(1, 10):
        t = (k / 9) ** 2.2
        y = top + (bottom - top) * t
        lines.append(f'<line x1="0" y1="{y:.1f}" x2="{width}" y2="{y:.1f}" stroke="{color}" stroke-opacity="{opacity * (0.4 + 0.6 * t):.3f}" stroke-width="1.5"/>')
    return "\n".join(lines)


def banner():
    w, h = 1280, 640
    return f'''<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">
  <defs>
    <linearGradient id="sky" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#232833"/><stop offset="0.62" stop-color="#3a3428"/><stop offset="1" stop-color="{DEEP}"/>
    </linearGradient>
    <radialGradient id="sun" cx="0.5" cy="0.62" r="0.42">
      <stop offset="0" stop-color="{AMBER_BRIGHT}" stop-opacity="0.85"/><stop offset="0.25" stop-color="{AMBER}" stop-opacity="0.35"/><stop offset="1" stop-color="{AMBER}" stop-opacity="0"/>
    </radialGradient>
    <linearGradient id="ground" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#17181b"/><stop offset="1" stop-color="{DEEP}"/>
    </linearGradient>
  </defs>
  <rect width="{w}" height="{h}" fill="url(#sky)"/>
  <rect width="{w}" height="{h}" fill="url(#sun)"/>
  <path d="{ridge(0.7, 400, 120, w, 16)}" fill="#2a2a2c"/>
  <path d="{ridge(2.3, 400, 70, w, 12)}" fill="#1f1f22"/>
  <rect y="398" width="{w}" height="{h - 398}" fill="url(#ground)"/>
  {floor_grid(w, 398, h, AMBER, 0.35)}
  <rect x="0" y="396" width="{w}" height="3" fill="{AMBER_BRIGHT}" opacity="0.8"/>
  <text x="640" y="250" text-anchor="middle" font-family="Sofia Sans Semi Condensed" font-weight="800" font-size="132" letter-spacing="10" fill="{TEXT}">EXAMPLE MOD</text>
  <text x="640" y="306" text-anchor="middle" font-family="Sofia Sans Semi Condensed" font-weight="600" font-size="34" letter-spacing="9" fill="{AMBER}">A TA REBORN MOD TEMPLATE</text>
</svg>'''


def hexagon(cx, cy, r, rotation=30):
    return " ".join(f"{cx + r * math.cos(math.radians(60 * k + rotation)):.1f},{cy + r * math.sin(math.radians(60 * k + rotation)):.1f}" for k in range(6))


def icon():
    s = 256
    return f'''<svg xmlns="http://www.w3.org/2000/svg" width="{s}" height="{s}" viewBox="0 0 {s} {s}">
  <defs>
    <linearGradient id="plate" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#3b3d42"/><stop offset="1" stop-color="#16171a"/></linearGradient>
  </defs>
  <rect x="8" y="8" width="240" height="240" rx="28" fill="url(#plate)" stroke="#5a5c62" stroke-width="4"/>
  <polygon points="{hexagon(128, 128, 92)}" fill="none" stroke="{AMBER}" stroke-width="10"/>
  <polygon points="{hexagon(128, 128, 70)}" fill="{AMBER}" fill-opacity="0.12"/>
  <text x="128" y="166" text-anchor="middle" font-family="Sofia Sans Semi Condensed" font-weight="800" font-size="112" fill="{TEXT}">E</text>
</svg>'''


def radar(frame, frames):
    """A frame of the preview: a radar screen, its beam going round, contacts lit as it passes."""
    w, h = 480, 270
    cx, cy, r = 240, 135, 112
    angle = 360 * frame / frames
    contacts = [(40, 0.55), (150, 0.8), (235, 0.4), (310, 0.7)]
    blips = []
    for bearing, distance in contacts:
        since = (angle - bearing) % 360
        glow = max(0.0, 1.0 - since / 300)
        x = cx + r * distance * math.cos(math.radians(bearing - 90))
        y = cy + r * distance * math.sin(math.radians(bearing - 90))
        blips.append(f'<circle cx="{x:.1f}" cy="{y:.1f}" r="{3 + 4 * glow:.1f}" fill="{AMBER_BRIGHT}" fill-opacity="{0.15 + 0.85 * glow:.2f}"/>')
    wedge = []
    for k in range(18):
        a0 = math.radians(angle - 90 - k * 2.5)
        a1 = math.radians(angle - 90 - (k + 1) * 2.5)
        wedge.append(f'<path d="M{cx},{cy} L{cx + r * math.cos(a0):.1f},{cy + r * math.sin(a0):.1f} A{r},{r} 0 0 0 {cx + r * math.cos(a1):.1f},{cy + r * math.sin(a1):.1f} Z" fill="{AMBER}" fill-opacity="{0.32 * (1 - k / 18):.3f}"/>')
    beam = math.radians(angle - 90)
    rings = "".join(f'<circle cx="{cx}" cy="{cy}" r="{r * f:.1f}" fill="none" stroke="{AMBER}" stroke-opacity="0.3" stroke-width="1.5"/>' for f in (0.33, 0.66, 1.0))
    return f'''<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">
  <rect width="{w}" height="{h}" fill="#111214"/>
  {floor_grid(w, 0, h, STEEL, 0.08)}
  <circle cx="{cx}" cy="{cy}" r="{r + 6}" fill="#0b0c0d" stroke="#4a4b4f" stroke-width="3"/>
  {rings}
  <line x1="{cx - r}" y1="{cy}" x2="{cx + r}" y2="{cy}" stroke="{AMBER}" stroke-opacity="0.2"/>
  <line x1="{cx}" y1="{cy - r}" x2="{cx}" y2="{cy + r}" stroke="{AMBER}" stroke-opacity="0.2"/>
  {"".join(wedge)}
  <line x1="{cx}" y1="{cy}" x2="{cx + r * math.cos(beam):.1f}" y2="{cy + r * math.sin(beam):.1f}" stroke="{AMBER_BRIGHT}" stroke-width="2.5"/>
  {"".join(blips)}
  <text x="16" y="30" font-family="Sofia Sans Semi Condensed" font-weight="700" font-size="20" letter-spacing="3" fill="{AMBER}">EXAMPLE MOD · RADAR</text>
</svg>'''


def map_screenshot():
    w, h = 1280, 720
    cells = []
    for row in range(-1, 10):
        for col in range(-1, 16):
            cx = col * 90 + (45 if row % 2 else 0)
            cy = row * 78
            shade = (math.sin(col * 0.7 + row * 1.3) + 1) / 2
            fill = f"rgb({int(40 + 30 * shade)},{int(52 + 34 * shade)},{int(40 + 18 * shade)})"
            cells.append(f'<polygon points="{hexagon(cx, cy, 52)}" fill="{fill}" stroke="#0f1010" stroke-width="3"/>')
    route = "M120,620 C320,520 360,300 600,320 S940,160 1160,120"
    return f'''<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">
  <rect width="{w}" height="{h}" fill="#1b1d1a"/>
  {"".join(cells)}
  <path d="{route}" fill="none" stroke="{AMBER_BRIGHT}" stroke-width="6" stroke-dasharray="18 12"/>
  <circle cx="120" cy="620" r="18" fill="#5c92d6" stroke="{TEXT}" stroke-width="4"/>
  <circle cx="1160" cy="120" r="18" fill="#ce4834" stroke="{TEXT}" stroke-width="4"/>
  <rect x="0" y="0" width="{w}" height="56" fill="#0d0e10" fill-opacity="0.85"/>
  <text x="28" y="38" font-family="Sofia Sans Semi Condensed" font-weight="700" font-size="28" letter-spacing="4" fill="{TEXT}">EXAMPLE MOD · THE CAMPAIGN MAP</text>
</svg>'''


def roster_screenshot():
    w, h = 1280, 720
    cards = []
    for i in range(4):
        x = 70 + i * 290
        cards.append(f'''<rect x="{x}" y="150" width="250" height="440" rx="10" fill="#22252b" stroke="#4a4d55" stroke-width="3"/>
  <polygon points="{hexagon(x + 125, 290, 70)}" fill="{AMBER}" fill-opacity="{0.15 + 0.12 * i:.2f}" stroke="{AMBER}" stroke-width="5"/>
  <rect x="{x + 30}" y="420" width="190" height="14" rx="7" fill="#3a3d44"/><rect x="{x + 30}" y="420" width="{60 + 40 * i}" height="14" rx="7" fill="{AMBER}"/>
  <rect x="{x + 30}" y="460" width="190" height="14" rx="7" fill="#3a3d44"/><rect x="{x + 30}" y="460" width="{170 - 30 * i}" height="14" rx="7" fill="#78d65c"/>
  <text x="{x + 125}" y="540" text-anchor="middle" font-family="Sofia Sans Semi Condensed" font-weight="700" font-size="30" letter-spacing="3" fill="{TEXT}">MARK {"I" * (i + 1) if i < 3 else "IV"}</text>''')
    return f'''<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">
  <rect width="{w}" height="{h}" fill="#141518"/>
  {floor_grid(w, 0, h, STEEL, 0.06)}
  {"".join(cards)}
  <text x="70" y="100" font-family="Sofia Sans Semi Condensed" font-weight="800" font-size="44" letter-spacing="6" fill="{TEXT}">EXAMPLE MOD · NEW UNITS</text>
</svg>'''


def hud_textures(out):
    """The example theme's pictures: brushed blue steel, seamless, behind the HUD's plates (192 x
    192) and its top bar (256 x 40, a sheen down it)."""
    hud = os.path.join(out, "hud")
    os.makedirs(hud, exist_ok=True)
    subprocess.run(["magick", "-size", "192x192", "xc:gray", "+noise", "Random", "-colorspace", "gray", "-virtual-pixel", "tile",
                    "-motion-blur", "0x24+0", "-blur", "0x0.7", "-auto-level", "-level", "25%,75%", "+level-colors", "#121922,#253242",
                    "-colors", "48", os.path.join(hud, "panel.png")], check=True)
    subprocess.run(["magick", "-size", "256x40", "xc:gray", "+noise", "Random", "-colorspace", "gray", "-virtual-pixel", "tile",
                    "-motion-blur", "0x30+0", "-auto-level", "-level", "30%,70%", "+level-colors", "#0f151d,#1f2a37",
                    "(", "-size", "256x40", "gradient:#ffffff22-#00000055", ")", "-compose", "softlight", "-composite",
                    "-colors", "48", os.path.join(hud, "bar.png")], check=True)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", default=os.path.join(ROOT, "docs", "examples", "example-mod", "media"))
    args = parser.parse_args()
    for tool in ("resvg", "magick"):
        if shutil.which(tool) is None:
            sys.exit(f"{tool} is not on the PATH")
    os.makedirs(args.out, exist_ok=True)
    render(banner(), os.path.join(args.out, "banner.png"), 1280, 640)
    render(icon(), os.path.join(args.out, "icon.png"), 256, 256)
    render(map_screenshot(), os.path.join(args.out, "campaign-map.png"), 1280, 720)
    render(roster_screenshot(), os.path.join(args.out, "new-units.png"), 1280, 720)
    hud_textures(args.out)
    frames = 36
    with tempfile.TemporaryDirectory() as work:
        for n in range(frames):
            render(radar(n, frames), os.path.join(work, f"f{n:03d}.png"), 480, 270)
        subprocess.run(["magick", "-delay", "6", "-loop", "0", os.path.join(work, "f*.png"), "+dither", "-colors", "32", "-layers", "Optimize",
                        os.path.join(args.out, "gameplay.gif")], check=True)
    for folder, _, names in os.walk(args.out):
        for name in sorted(names):
            path = os.path.join(folder, name)
            print(f"{os.path.relpath(path, args.out)}: {os.path.getsize(path):,} bytes")


if __name__ == "__main__":
    main()
