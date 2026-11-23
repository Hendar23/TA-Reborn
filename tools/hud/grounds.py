"""Draws the grounds the modern HUD's build menus stand their pictures on (original art for TA
Reborn): where each unit goes, as Forged Alliance's build icons tell it by what lies behind them.
Grass for land, water for the sea, the two split on the diagonal for what goes on both (land at the
top left, the sea at the bottom right, a shore between), sky for aircraft.

Each ground is seen from above, as a battlefield is, and made here from noise (spectral synthesis
and cellular noise) and a few shapes: meadow grass with darker clumps and dry patches lit from the
top left; water with a swell, ripples and glints, lighter where the sky shows in it; the shore's
wet sand and foam; clouds over a deep sky. Then each is graded to sit in the interface's brass and
gunmetal and keep the unit in front of it readable: its colours muted, its detail calmer where the
unit stands, a pool of light behind the unit and the edges darkened (`finish`).

The output is one PNG, the four grounds side by side in the order of `hud::domain::Domain` (land,
both, sea, air), each SIDE pixels square and opaque. The game brings each down to the sizes it
draws them at as it loads them (egui's wgpu renderer has no mipmaps): draw them here at the largest.

    python tools/hud/grounds.py [--out apps/ta_game/hud/grounds.png] [--preview <dir>]

Needs numpy and Pillow. Run `oxipng -o 3` on the PNG after.
"""
import argparse
import os

import numpy as np
from PIL import Image

# The order of hud::domain::Domain.
GROUNDS = ["land", "both", "sea", "air"]
# Pixels a side, as the game reads them (hud::domain::SIDE), and how much larger they are drawn
# before being brought down to it.
SIDE = 192
SUPER = 2
N = SIDE * SUPER
TAU = 2.0 * np.pi

# ---------------------------------------------------------------------------------------------
# Building blocks
# ---------------------------------------------------------------------------------------------


def spectral(beta, lo=1.0, hi=None, seed=0, aniso=(1.0, 1.0)):
    """Noise by spectral synthesis: random phases, power falling as f^-beta between `lo` and `hi`
    cycles across the picture (`aniso` stretches the frequencies along y and x). Mean 0, std 1."""
    rng = np.random.default_rng(seed)
    fy = np.fft.fftfreq(N) * N * aniso[0]
    fx = np.fft.rfftfreq(N) * N * aniso[1]
    f = np.sqrt(fy[:, None] ** 2 + fx[None, :] ** 2)
    amp = np.zeros_like(f)
    nz = f > 0
    amp[nz] = f[nz] ** (-beta / 2.0)
    amp[f < lo] = 0.0
    if hi is not None:
        amp[f > hi] = 0.0
    spec = amp * np.exp(1j * rng.uniform(0, TAU, amp.shape))
    img = np.fft.irfft2(spec, s=(N, N))
    return (img - img.mean()) / (img.std() + 1e-12)


def cells(grid, seed, warp=None, jitter=0.9):
    """Cellular (Worley) noise on a `grid` x `grid` jittered lattice over the picture, its points
    looked up through `warp` (an offset in cells for each pixel, (dy, dx)): the distances to the
    nearest and the second nearest point, in cells."""
    rng = np.random.default_rng(seed)
    off = 0.5 + (rng.random((grid, grid, 2)) - 0.5) * jitter
    ys, xs = np.mgrid[0:N, 0:N].astype(np.float64) * grid / N
    if warp is not None:
        ys = ys + warp[0]
        xs = xs + warp[1]
    cx = np.floor(xs).astype(int)
    cy = np.floor(ys).astype(int)
    f1 = np.full((N, N), 1e9)
    f2 = np.full((N, N), 1e9)
    for oy in (-1, 0, 1):
        for ox in (-1, 0, 1):
            nx = (cx + ox) % grid
            ny = (cy + oy) % grid
            px = cx + ox + off[ny, nx, 0]
            py = cy + oy + off[ny, nx, 1]
            d = np.hypot(xs - px, ys - py)
            f2 = np.where(d < f1, f1, np.minimum(f2, d))
            f1 = np.minimum(f1, d)
    return f1, f2


def smoothstep(e0, e1, x):
    t = np.clip((x - e0) / (e1 - e0), 0.0, 1.0)
    return t * t * (3.0 - 2.0 * t)


def blur(img, sigma):
    """A gaussian blur of `sigma` pixels (by the Fourier transform: the picture wraps, which its
    edges, darkened by the vignette, never show)."""
    fy = np.fft.fftfreq(N)[:, None]
    fx = np.fft.fftfreq(N)[None, :]
    g = np.exp(-2.0 * (np.pi * sigma) ** 2 * (fy ** 2 + fx ** 2))
    if img.ndim == 2:
        return np.real(np.fft.ifft2(np.fft.fft2(img) * g))
    return np.dstack([np.real(np.fft.ifft2(np.fft.fft2(img[..., c]) * g)) for c in range(img.shape[2])])


def coords():
    """Each pixel's place, 0 to 1 across (x) and down (y)."""
    ys, xs = np.mgrid[0:N, 0:N].astype(np.float64)
    return (ys + 0.5) / N, (xs + 0.5) / N


def rgb(*c):
    return np.array(c, dtype=np.float64) / 255.0


def mix(a, b, t):
    t = t[..., None] if np.ndim(t) == 2 else t
    return a + (b - a) * t


def relief(height, strength):
    """How a height field is lit from the top left (1 on the flat, more on the slopes facing the
    light): a gentle shading for the ground's own shapes."""
    gy, gx = np.gradient(height)
    n = np.dstack([-gx * strength, -gy * strength, np.ones_like(height)])
    n /= np.linalg.norm(n, axis=2, keepdims=True)
    light = np.array([-0.5, -0.6, 0.62])
    light /= np.linalg.norm(light)
    return np.clip(n @ light, 0.0, 1.0) / light[2]


# ---------------------------------------------------------------------------------------------
# The grounds
# ---------------------------------------------------------------------------------------------


def grass(seed=11):
    """A meadow from above: greens in broad soft patches, a few darker clumps of bushes and dry
    patches, the grass's own fine grain, lit from the top left."""
    macro = spectral(3.4, 1.0, 9, seed)
    meso = spectral(2.4, 6, 28, seed + 1)
    fine = spectral(1.2, 40, None, seed + 2)
    dry = spectral(3.2, 1.0, 6, seed + 3)
    clumps = blur(spectral(2.8, 4, 20, seed + 4), 1.5 * SUPER)
    dark, mid, light = rgb(44, 58, 32), rgb(68, 86, 46), rgb(100, 116, 66)
    t = np.clip(0.52 + 0.2 * macro + 0.07 * meso, 0.0, 1.0)
    col = mix(mix(dark, mid, np.clip(t * 2.0, 0, 1)), light, np.clip(t * 2.0 - 1.0, 0, 1))
    col = mix(col, rgb(110, 104, 68), 0.32 * smoothstep(1.0, 1.9, dry))
    bush = smoothstep(1.2, 2.1, clumps)
    col = col * (1.0 - 0.34 * bush[..., None]) + rgb(24, 36, 22) * (0.14 * bush[..., None])
    col = col * (1.0 + 0.045 * fine[..., None])
    height = 0.6 * macro + 0.25 * meso + 1.2 * bush + 0.04 * fine
    col = col * (0.86 + 0.17 * relief(height, 2.6))[..., None]
    return col


def water(seed=21):
    """The sea from above: deep teal-blue, a long swell, ripples catching the light, a few
    glints, lighter towards the top where the sky shows in it."""
    y, x = coords()
    deep, shallow = rgb(18, 44, 60), rgb(46, 86, 100)
    col = mix(shallow, deep, smoothstep(0.0, 1.0, y * 0.9 + 0.05))
    swell = spectral(2.8, 2, 14, seed, aniso=(1.0, 2.6))
    col = col * (1.0 + 0.07 * swell[..., None])
    depth = spectral(3.4, 1.5, 6, seed + 1)
    col = mix(col, deep * 0.85, 0.3 * smoothstep(0.4, 1.6, depth))
    # The light the ripples gather: the edges of warped cells, softened, in drifts (where the
    # swell turns to the sky), never a net over the whole.
    warp = (spectral(2.5, 2, 12, seed + 2) * 0.4, spectral(2.5, 2, 12, seed + 3) * 0.4)
    f1, f2 = cells(10, seed + 4, warp)
    ripple = blur(1.0 - smoothstep(0.0, 0.16, f2 - f1), 0.8 * SUPER)
    drift = smoothstep(-0.6, 1.2, spectral(3.0, 1.5, 8, seed + 5))
    lit = 0.2 * ripple * drift * (1.0 - 0.4 * y)
    col = col + (rgb(118, 170, 176) - col) * lit[..., None]
    glints = smoothstep(2.4, 3.2, spectral(0.6, 20, None, seed + 6)) * ripple * drift
    col = col + rgb(200, 220, 214) * (0.2 * glints)[..., None]
    return col


def shore(land, sea, seed=31):
    """Land at the top left and the sea at the bottom right, split on the diagonal by a ragged
    shore: wet sand along it, the water paler in the shallows and foam where it breaks."""
    y, x = coords()
    # How far into the sea from the diagonal (bottom left to top right), in the picture's width,
    # pushed about by noise so the shore wanders.
    d = (x + y - 1.0) / np.sqrt(2.0)
    d = d + 0.045 * spectral(3.2, 1.0, 5, seed) + 0.006 * spectral(2.0, 6, 30, seed + 1)
    shallows = 1.0 - smoothstep(0.0, 0.22, d)
    sea = mix(sea, rgb(64, 110, 108), 0.55 * shallows)
    out = mix(land, sea, smoothstep(-0.004, 0.004, d))
    sand = smoothstep(-0.05, -0.012, d) * (1.0 - smoothstep(-0.004, 0.002, d))
    grain = spectral(1.4, 30, None, seed + 2)
    out = mix(out, rgb(116, 106, 76) * (1.0 + 0.05 * grain[..., None]), 0.75 * sand)
    wet = smoothstep(-0.016, -0.002, d) * (1.0 - smoothstep(-0.002, 0.004, d))
    out = mix(out, rgb(84, 80, 60), 0.5 * wet)
    foam_noise = spectral(1.8, 10, None, seed + 3)
    foam = (smoothstep(0.0, 0.006, d) * (1.0 - smoothstep(0.006, 0.03, d))
            + 0.45 * smoothstep(0.04, 0.05, d) * (1.0 - smoothstep(0.05, 0.065, d)) * smoothstep(0.0, 1.2, foam_noise))
    out = mix(out, rgb(176, 192, 186), 0.55 * np.clip(foam, 0, 1) * (0.75 + 0.25 * foam_noise))
    return out


def sky(seed=41):
    """Clouds over a deep sky from above them: a muted steel blue, darker at the top, hazier
    lower down, with soft drifts of cloud lit from the top left."""
    y, x = coords()
    top, low = rgb(30, 50, 76), rgb(86, 110, 132)
    col = mix(top, low, smoothstep(0.0, 1.0, y))
    n = spectral(3.0, 1.0, 12, seed, aniso=(1.0, 2.2))
    puff = spectral(2.4, 4, 24, seed + 1)
    amount = n + 0.3 * puff + 1.0 * (y - 0.5)
    cloud = smoothstep(0.1, 1.5, amount)
    shade = relief(blur(cloud, 5.0 * SUPER), 26.0)
    white = rgb(164, 174, 184) * (0.86 + 0.16 * shade)[..., None]
    col = mix(col, white, 0.62 * cloud)
    # Thin haze drawn out across the clear sky above them.
    wisp = blur(smoothstep(0.3, 1.8, spectral(2.8, 1.5, 10, seed + 2, aniso=(1.0, 3.5))), 3.0 * SUPER)
    col = mix(col, rgb(120, 138, 158), 0.18 * wisp * (1.0 - cloud))
    return col


# ---------------------------------------------------------------------------------------------
# The grade
# ---------------------------------------------------------------------------------------------


def finish(col):
    """The grade every ground is given: calmer where the unit stands (its detail blended towards
    a softened copy), its colours muted, a pool of light behind the unit, light from the top
    left and the edges darkened, so that the unit in front reads."""
    y, x = coords()
    r = np.hypot(x - 0.5, (y - 0.46) * 1.05)
    calm = 0.45 * (1.0 - smoothstep(0.08, 0.36, r))
    col = mix(col, blur(col, 2.2 * SUPER), calm)
    grey = col @ np.array([0.299, 0.587, 0.114])
    col = mix(col, np.dstack([grey] * 3), 0.22)
    pool = np.exp(-((x - 0.5) ** 2 + (y - 0.44) ** 2) / (2 * 0.27 ** 2))
    key = 1.05 - 0.1 * (x + y) / 2.0
    # The vignette on a rounded square: the corners darkest, the middles of the edges less.
    e = np.maximum(np.abs(x - 0.5), np.abs(y - 0.5)) * 2.0
    q = (np.abs(x - 0.5) ** 4 + np.abs(y - 0.5) ** 4) ** 0.25 * 2.0
    edge = 1.0 - 0.38 * smoothstep(0.55, 1.08, 0.5 * (e + q))
    gain = (0.8 + 0.36 * pool) * key * edge
    return np.clip(col * gain[..., None], 0.0, 1.0)


def ground(name):
    if name == "land":
        return grass()
    if name == "sea":
        return water()
    if name == "both":
        return shore(grass(seed=12), water(seed=22))
    if name == "air":
        return sky()
    raise KeyError(name)


def down(col):
    """Brought down from the size it was drawn at to SIDE."""
    img = Image.fromarray((np.clip(col, 0, 1) * 255).round().astype(np.uint8), "RGB")
    return np.asarray(img.resize((SIDE, SIDE), Image.LANCZOS), dtype=np.uint8)


def sheet():
    return np.concatenate([down(finish(ground(name))) for name in GROUNDS], axis=1)


# ---------------------------------------------------------------------------------------------
# The preview
# ---------------------------------------------------------------------------------------------


def preview(out, path):
    """The grounds as the build menu shows them, on a gunmetal plate: at the deck's least and most
    sizes and twice that (a screen at 200 %), each in a dark rim with a light lip under it."""
    sizes = [46, 66, 132]
    gap, pad = 6, 14
    width = pad * 2 + sum(len(GROUNDS) * (s + gap) for s in sizes) + gap * len(sizes)
    height = pad * 2 + max(sizes)
    plate = np.zeros((height, width, 3), dtype=np.float64)
    grad = np.linspace(0, 1, height)[:, None, None]
    plate[:] = rgb(66, 72, 78) + (rgb(24, 26, 28) - rgb(66, 72, 78)) * grad
    xo = pad
    for s in sizes:
        for i in range(len(GROUNDS)):
            cell = Image.fromarray(out[:, i * SIDE:(i + 1) * SIDE]).resize((s, s), Image.LANCZOS)
            yo = pad + (max(sizes) - s)
            plate[yo:yo + s, xo:xo + s] = np.asarray(cell, dtype=np.float64) / 255.0
            plate[yo:yo + s, xo] = plate[yo:yo + s, xo + s - 1] = 0.02
            plate[yo, xo:xo + s] = plate[yo + s - 1, xo:xo + s] = 0.02
            if yo + s < height:
                plate[yo + s, xo:xo + s] = np.minimum(plate[yo + s, xo:xo + s] + 0.11, 1.0)
            xo += s + gap
        xo += gap
    Image.fromarray((np.clip(plate, 0, 1) * 255).round().astype(np.uint8)).save(path)


def main():
    ap = argparse.ArgumentParser()
    root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    ap.add_argument("--out", default=os.path.join(root, "apps", "ta_game", "hud", "grounds.png"))
    ap.add_argument("--preview", default=None)
    args = ap.parse_args()
    out = sheet()
    os.makedirs(os.path.dirname(args.out), exist_ok=True)
    Image.fromarray(out, "RGB").save(args.out, optimize=True)
    print(f"{args.out}: {out.shape[1]}x{out.shape[0]}, {len(GROUNDS)} grounds ({', '.join(GROUNDS)}) of {SIDE}")
    if args.preview:
        os.makedirs(args.preview, exist_ok=True)
        preview(out, os.path.join(args.preview, "grounds.png"))
        Image.fromarray(out, "RGB").resize((out.shape[1] * 2, out.shape[0] * 2), Image.NEAREST).save(os.path.join(args.preview, "grounds_x2.png"))
        print(f"preview in {args.preview}")


if __name__ == "__main__":
    main()
