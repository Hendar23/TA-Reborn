//! The ground painted as Supreme Commander paints it, seen from above: the lower stratum, the
//! eight strata over it by their masks (`clamp(2m - 1, 0, 1)` each, later over earlier), the
//! upper stratum by its own alpha, the albedo decals by theirs, lit as the game's terrain shader
//! lights them (the sun over the heightmap's slopes, the ambience, the multiplier and the
//! shadows' fill: [`Painter::light`]), and under the water tinted by the water's ramp of colours
//! by depth; then matched to the game's own picture of the map ([`Painter::calibrate`]).
//!
//! Colours are sRGB (0 to 1) as the game's textures hold them; nothing here uses a platform's
//! powers, roots or trigonometry ([`super::trig`]), so the picture is the same everywhere.

use std::collections::BTreeMap;
use std::sync::Arc;

use super::land::{Frame, height};
use crate::dds::{self, Image};
use crate::files::Files;
use crate::scmap::ScMap;
use crate::{Error, Result};

/// A stratum: its texture (the mip level that suits the map's scale) and the world units one
/// repeat of it covers.
struct Stratum {
    image: Image,
    scale: f64,
}

/// An albedo decal, ready to paint: its corner, the world-to-texture steps along its two sides.
struct Decal {
    image: Arc<Image>,
    corner: [f64; 2],
    /// (cos θ, sin θ) / scale.x and (-sin θ, cos θ) / scale.z.
    along: [f64; 2],
    across: [f64; 2],
}

/// The world is cut into squares of this many units for finding the decals over a place.
const GRID: f64 = 8.0;
/// Texels a pixel the strata are decoded at: at TA's scale a texture's repeat covers a few dozen
/// pixels, and one texel a pixel averaged its grain away.
const SHARPEN: f64 = 2.0;
/// How far the calibration to the game's own picture of the map may brighten or darken it, and
/// how far it may change one channel against the others. (The game's pictures of its maps are
/// brighter than its terrain shader lights them, most by a quarter to two fifths, one by four
/// fifths: `how_each_map_matches_its_preview` in the tests of the installed game.)
const GAIN: (f64, f64) = (0.75, 2.0);
const TINT: f64 = 0.12;
/// The fewest places of land the calibration compares (a twentieth of its first grid's).
const MATCH_PLACES: usize = 115;

/// Where the game's own picture of a map (its preview) shows a place: the map fitted into the
/// picture whole, its longer side across it, in the middle (a map twice as wide as it is deep
/// fills the middle half of a square picture, black above and below).
#[derive(Clone, Copy, Debug, PartialEq)]
struct PreviewFit {
    /// The picture's pixels a world unit, and where the map's corner lies in it.
    scale: f64,
    x0: f64,
    y0: f64,
}

impl PreviewFit {
    /// A map `w × h` world units in a picture `pw × ph` pixels.
    fn of(w: f64, h: f64, pw: f64, ph: f64) -> PreviewFit {
        let scale = (pw / w).min(ph / h);
        PreviewFit { scale, x0: (pw - w * scale) / 2.0, y0: (ph - h * scale) / 2.0 }
    }

    /// The picture's position (pixels) of world position (u, v).
    fn at(&self, u: f64, v: f64) -> (f64, f64) {
        (self.x0 + u * self.scale, self.y0 + v * self.scale)
    }
}

pub struct Painter<'a> {
    map: &'a ScMap,
    frame: Frame,
    lower: Option<Stratum>,
    /// The masked strata that have a texture: (their mask channel 0..8, the stratum).
    strata: Vec<(usize, Stratum)>,
    upper: Option<Stratum>,
    masks: [Option<Image>; 2],
    decals: Vec<Decal>,
    /// Decals over each square of the world ([`GRID`]), in the map's order.
    grid: Vec<Vec<u32>>,
    grid_w: usize,
    grid_h: usize,
    /// The map's light, as the game's terrain shader takes it ([`Painter::light`]): towards the
    /// sun, the sun's colour, the ambience, the multiplier and the shadows' fill.
    sun: [f64; 3],
    sun_colour: [f64; 3],
    ambience: [f64; 3],
    multiplier: f64,
    shadow_fill: [f64; 3],
    /// The water's surface and abyss elevations, and its ramp of colours by depth.
    water: Option<(f64, f64, Option<Image>)>,
    /// What every colour is multiplied by: the map as the game shows it ([`Painter::calibrate`]).
    gain: [f64; 3],
    /// How the colours were matched to the game's picture of the map, if they were.
    matched: Option<Matched>,
    /// For each TA cell of cliff a way for tanks was opened through, the cells to the ground it
    /// is painted as ([`Painter::open_ways`]); empty where there are none.
    ways: Vec<Option<(i8, i8)>>,
}

/// How a map's colours were matched to the game's own picture of it ([`Painter::calibrate`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Matched {
    /// The places of land compared.
    pub places: usize,
    /// The mean colour there (sRGB, 0 to 1) of the painting, lit, and of the picture.
    pub ours: [f64; 3],
    pub theirs: [f64; 3],
    /// The gain that would have matched them, and the one applied (within [`GAIN`] and [`TINT`]).
    pub wanted: [f64; 3],
    pub gain: [f64; 3],
}

/// Decodes texture `path` at the mip level whose larger side is about `side` texels (none when
/// the game has no such file, or it is no texture).
fn texture(files: &Files, path: &str, side: f64) -> Result<Option<Image>> {
    if path.trim().is_empty() {
        return Ok(None);
    }
    let Some(bytes) = files.read(path)? else {
        log::warn!("{path}: not in the game's files");
        return Ok(None);
    };
    let header = match dds::Header::parse(&bytes) {
        Ok(h) => h,
        Err(e) => {
            log::warn!("{path}: {e}");
            return Ok(None);
        }
    };
    let level = header.level_for(side.max(1.0).ceil() as u32);
    match dds::decode(&bytes, level) {
        Ok(image) => Ok(Some(image)),
        Err(e) => {
            log::warn!("{path}: {e}");
            Ok(None)
        }
    }
}

/// `image`'s colour at texel position (x, y) (texel centres at .5), between texels; repeated
/// (`wrap`) or clamped at the edges.
#[inline]
fn sample(image: &Image, x: f64, y: f64, wrap: bool) -> [f64; 4] {
    let (x, y) = (x - 0.5, y - 0.5);
    let (fx, fy) = (x.floor(), y.floor());
    let (tx, ty) = (x - fx, y - fy);
    let (ix, iy) = (fx as i64, fy as i64);
    let get = |dx: i64, dy: i64| if wrap { image.wrapped(ix + dx, iy + dy) } else { image.clamped(ix + dx, iy + dy) };
    let (a, b, c, d) = (get(0, 0), get(1, 0), get(0, 1), get(1, 1));
    std::array::from_fn(|k| {
        let top = a[k] as f64 + (b[k] as f64 - a[k] as f64) * tx;
        let bottom = c[k] as f64 + (d[k] as f64 - c[k] as f64) * tx;
        (top + (bottom - top) * ty) / 255.0
    })
}

impl Stratum {
    /// Its colour at world position (u, v).
    #[inline]
    fn at(&self, u: f64, v: f64) -> [f64; 4] {
        let (w, h) = (self.image.width as f64, self.image.height as f64);
        sample(&self.image, u / self.scale * w, v / self.scale * h, true)
    }
}

#[inline]
fn lerp3(a: [f64; 3], b: [f64; 3], t: f64) -> [f64; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn normalise(v: [f64; 3]) -> [f64; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if l > 1e-9 { [v[0] / l, v[1] / l, v[2] / l] } else { [0.0, 1.0, 0.0] }
}

impl<'a> Painter<'a> {
    /// Gathers what `map` is painted with, from the game's files, for painting at `frame`'s scale.
    pub fn new(map: &'a ScMap, files: &Files, frame: Frame) -> Result<Painter<'a>> {
        let stratum = |layer: &crate::scmap::Layer| -> Result<Option<Stratum>> {
            let scale = if layer.scale.is_finite() && layer.scale > 0.0 { layer.scale as f64 } else { 4.0 };
            // (One repeat covers `scale` world units, `scale * s` pixels: the level that size.)
            Ok(texture(files, &layer.path, scale * frame.s * SHARPEN)?.map(|image| Stratum { image, scale }))
        };
        let albedo = &map.albedo;
        let lower = match albedo.first() { Some(l) => stratum(l)?, None => None };
        let count = if map.four_strata() { 4 } else { 8 };
        let mut strata = Vec::new();
        for (k, layer) in albedo.iter().enumerate().skip(1).take(count) {
            if let Some(s) = stratum(layer)? {
                strata.push((k - 1, s));
            }
        }
        let upper = match albedo.get(9) { Some(l) => stratum(l)?, None => None };
        let mask = |bytes: &[u8]| dds::decode(bytes, 0).ok().filter(|m| m.width > 0 && m.height > 0);
        let masks = [mask(&map.masks[0]), if map.four_strata() { None } else { mask(&map.masks[1]) }];
        // The albedo decals, each texture decoded once at the size it is painted at.
        let mut images: BTreeMap<(String, u32), Arc<Image>> = BTreeMap::new();
        let mut decals = Vec::new();
        for d in &map.decals {
            if d.kind != 1 || d.textures.is_empty() || !(d.scale[0] > 0.0 && d.scale[2] > 0.0) {
                continue;
            }
            let path = d.textures[0].to_ascii_lowercase();
            let side = (d.scale[0].max(d.scale[2]) as f64 * frame.s).ceil().max(1.0) as u32;
            // (Sides rounded up to a power of two: one decode serves decals of about one size.)
            let side = side.next_power_of_two();
            let image = match images.get(&(path.clone(), side)) {
                Some(i) => i.clone(),
                None => {
                    let Some(i) = texture(files, &path, side as f64)? else { continue };
                    let i = Arc::new(i);
                    images.insert((path, side), i.clone());
                    i
                }
            };
            let (s, c) = super::trig::sin_cos(d.rotation[1] as f64);
            let (sx, sz) = (d.scale[0] as f64, d.scale[2] as f64);
            decals.push(Decal { image, corner: [d.position[0] as f64, d.position[2] as f64], along: [c / sx, s / sx], across: [-s / sz, c / sz] });
        }
        let (grid_w, grid_h) = ((map.width as f64 / GRID).ceil() as usize + 1, (map.height as f64 / GRID).ceil() as usize + 1);
        let mut grid = vec![Vec::new(); grid_w * grid_h];
        for (i, d) in decals.iter().enumerate() {
            // Its four corners in the world, and the squares their box covers.
            let (sx, sz) = (1.0 / (d.along[0] * d.along[0] + d.along[1] * d.along[1]).sqrt(), 1.0 / (d.across[0] * d.across[0] + d.across[1] * d.across[1]).sqrt());
            let ax = [d.along[0] * sx * sx, d.along[1] * sx * sx];
            let az = [d.across[0] * sz * sz, d.across[1] * sz * sz];
            let corners = [d.corner, [d.corner[0] + ax[0], d.corner[1] + ax[1]], [d.corner[0] + az[0], d.corner[1] + az[1]], [d.corner[0] + ax[0] + az[0], d.corner[1] + ax[1] + az[1]]];
            let (x0, x1) = corners.iter().fold((f64::MAX, f64::MIN), |(a, b), c| (a.min(c[0]), b.max(c[0])));
            let (z0, z1) = corners.iter().fold((f64::MAX, f64::MIN), |(a, b), c| (a.min(c[1]), b.max(c[1])));
            let cell = |v: f64, n: usize| ((v / GRID).floor().max(0.0) as usize).min(n - 1);
            for gz in cell(z0, grid_h)..=cell(z1, grid_h) {
                for gx in cell(x0, grid_w)..=cell(x1, grid_w) {
                    grid[gz * grid_w + gx].push(i as u32);
                }
            }
        }
        let l = &map.lighting;
        let sun = normalise(l.sun_direction.map(|v| v as f64));
        let sun = if sun[1] > 0.05 { sun } else { normalise([0.4, 0.8, 0.45]) };
        // (A map's light as it is written, zeros and all: most of the game's maps have no
        // ambience, their shadows lit by the fill alone. Only what no map could mean is replaced.)
        let sane = |c: [f32; 3], d: [f64; 3]| { let c = c.map(|v| v as f64); if c.iter().all(|v| v.is_finite() && *v >= 0.0 && *v < 16.0) { c } else { d } };
        let sun_colour = sane(l.sun_colour, [1.0, 1.0, 1.0]);
        let ambience = sane(l.ambience, [0.0, 0.0, 0.0]);
        let shadow_fill = sane(l.shadow_fill, [0.3, 0.3, 0.3]);
        let multiplier = if l.multiplier.is_finite() && l.multiplier > 0.0 && l.multiplier < 16.0 { l.multiplier as f64 } else { 1.0 };
        let water = match &map.water {
            Some(w) => {
                let ramp = texture(files, &w.ramp, 256.0)?;
                Some((w.surface as f64, (w.abyss as f64).min(w.surface as f64 - 1.0), ramp))
            }
            None => None,
        };
        if lower.is_none() && strata.is_empty() {
            return Err(Error::Missing(format!("the ground's textures ({})", albedo.first().map_or("", |l| l.path.as_str()))));
        }
        let mut painter = Painter { map, frame, lower, strata, upper, masks, decals, grid, grid_w, grid_h, sun, sun_colour, ambience, multiplier, shadow_fill, water, gain: [1.0; 3], matched: None, ways: Vec::new() };
        painter.calibrate();
        Ok(painter)
    }

    /// The albedo decals, and how many lie over a square of the world on average and at most
    /// (the painting's cost: every pixel tries those over its square).
    pub fn decal_load(&self) -> (usize, f64, usize) {
        let total: usize = self.grid.iter().map(Vec::len).sum();
        (self.decals.len(), total as f64 / self.grid.len().max(1) as f64, self.grid.iter().map(Vec::len).max().unwrap_or(0))
    }

    /// The ground's colour (sRGB, 0 to 1) at world position (u, v), unlit, as its textures and
    /// decals paint it.
    pub fn albedo(&self, u: f64, v: f64) -> [f64; 3] {
        let mut c = match &self.lower {
            Some(s) => { let t = s.at(u, v); [t[0], t[1], t[2]] }
            None => [0.4, 0.4, 0.4],
        };
        if !self.strata.is_empty() {
            let (w, h) = (self.map.width as f64, self.map.height as f64);
            let mut weights = [0.0f64; 8];
            for (m, mask) in self.masks.iter().enumerate() {
                let Some(mask) = mask else { continue };
                let t = sample(mask, u / w * mask.width as f64, v / h * mask.height as f64, false);
                for k in 0..4 {
                    weights[m * 4 + k] = t[k];
                }
            }
            for (k, s) in &self.strata {
                let weight = (2.0 * weights[*k] - 1.0).clamp(0.0, 1.0);
                if weight > 0.0 {
                    let t = s.at(u, v);
                    c = lerp3(c, [t[0], t[1], t[2]], weight);
                }
            }
        }
        if let Some(s) = &self.upper {
            let t = s.at(u, v);
            c = lerp3(c, [t[0], t[1], t[2]], t[3]);
        }
        let (gx, gz) = ((u / GRID).floor(), (v / GRID).floor());
        if gx >= 0.0 && gz >= 0.0 && (gx as usize) < self.grid_w && (gz as usize) < self.grid_h {
            for &i in &self.grid[gz as usize * self.grid_w + gx as usize] {
                let d = &self.decals[i as usize];
                let (dx, dz) = (u - d.corner[0], v - d.corner[1]);
                let a = dx * d.along[0] + dz * d.along[1];
                let b = dx * d.across[0] + dz * d.across[1];
                if (0.0..1.0).contains(&a) && (0.0..1.0).contains(&b) {
                    let t = sample(&d.image, a * d.image.width as f64, b * d.image.height as f64, false);
                    c = lerp3(c, [t[0], t[1], t[2]], t[3]);
                }
            }
        }
        c
    }

    /// The light ground takes whose normal is `ndl` (its dot product with the way to the sun), as
    /// the game's terrain shader lights it: the sun's colour by how squarely the ground faces it,
    /// and the ambience; then the multiplier, and the shadows' fill for what light falls short of
    /// one (`multiplier × l + fill × (1 − l)`). Ground facing away from the sun takes the fill: on
    /// the many maps without ambience, its only light.
    fn light(&self, ndl: f64) -> [f64; 3] {
        let ndl = ndl.max(0.0);
        std::array::from_fn(|k| {
            let l = self.sun_colour[k] * ndl + self.ambience[k];
            self.multiplier * l + self.shadow_fill[k] * (1.0 - l)
        })
    }

    /// Paints the TA cells of cliff a way for tanks was opened through (`ways`; `cliff`, the cells
    /// of cliff left) as the ground nearest each (a cell's whole texture taken from the nearest
    /// cell neither cliff nor way, within 12), lit as level ground: the cliff's rock and its
    /// shading no longer stand where tanks now drive.
    pub fn open_ways(&mut self, ways: &[bool], cliff: &[bool]) {
        const REACH: i32 = 12;
        let (w, h) = (self.frame.w, self.frame.h);
        if !ways.iter().any(|&b| b) || ways.len() != w * h || cliff.len() != w * h {
            return;
        }
        let mut spiral: Vec<(i32, i32)> = (-REACH..=REACH).flat_map(|dz| (-REACH..=REACH).map(move |dx| (dx, dz))).filter(|&(dx, dz)| dx * dx + dz * dz <= REACH * REACH).collect();
        spiral.sort_by_key(|&(dx, dz)| (dx * dx + dz * dz, dz, dx));
        self.ways = (0..w * h)
            .map(|i| {
                if !ways[i] {
                    return None;
                }
                let (x, z) = ((i % w) as i32, (i / w) as i32);
                let ground = spiral.iter().copied().find(|&(dx, dz)| {
                    let (nx, nz) = (x + dx, z + dz);
                    nx >= 0 && nz >= 0 && (nx as usize) < w && (nz as usize) < h && !ways[nz as usize * w + nx as usize] && !cliff[nz as usize * w + nx as usize]
                });
                Some(ground.map_or((0, 0), |(dx, dz)| (dx as i8, dz as i8)))
            })
            .collect();
    }

    /// The cells to the ground a place of a way is painted as, if (u, v) is on one.
    fn way_at(&self, u: f64, v: f64) -> Option<(i8, i8)> {
        if self.ways.is_empty() {
            return None;
        }
        let (x, z) = self.frame.pixels(u, v);
        let (cx, cz) = ((x / 16.0).floor(), (z / 16.0).floor());
        if cx < 0.0 || cz < 0.0 || cx >= self.frame.w as f64 || cz >= self.frame.h as f64 {
            return None;
        }
        self.ways[cz as usize * self.frame.w + cx as usize]
    }

    /// The ground's colour (sRGB, 0 to 1) at world position (u, v): painted, lit, and under the
    /// water tinted by its depth.
    pub fn colour(&self, u: f64, v: f64) -> [f32; 3] {
        let m = self.map;
        let (albedo, ndl) = match self.way_at(u, v) {
            // A way for tanks through a cliff: the ground nearest it, level.
            Some((dx, dz)) => {
                let cell = self.frame.cell();
                (self.albedo(u + dx as f64 * cell, v + dz as f64 * cell), self.sun[1])
            }
            None => {
                // The slope from the heights a cell apart (a world unit at the game's own scale).
                let d = self.frame.cell().max(1.0);
                let (hx0, hx1) = (height(m, u - d, v), height(m, u + d, v));
                let (hz0, hz1) = (height(m, u, v - d), height(m, u, v + d));
                let n = normalise([-(hx1 - hx0) / (2.0 * d), 1.0, -(hz1 - hz0) / (2.0 * d)]);
                (self.albedo(u, v), n[0] * self.sun[0] + n[1] * self.sun[1] + n[2] * self.sun[2])
            }
        };
        let light = self.light(ndl);
        let mut c: [f64; 3] = std::array::from_fn(|k| albedo[k] * light[k]);
        if let Some((surface, abyss, ramp)) = &self.water {
            let h = height(m, u, v);
            if h < *surface {
                let t = ((surface - h) / (surface - abyss)).clamp(0.0, 1.0);
                let tint = match ramp {
                    Some(r) => sample(r, t * (r.width as f64 - 1.0) + 0.5, r.height as f64 * 0.5, false),
                    None => {
                        let w = m.water.as_ref().map_or([0.0, 0.1, 0.2], |w| w.colour);
                        [w[0] as f64, w[1] as f64, w[2] as f64, 0.15 + 0.7 * t]
                    }
                };
                c = lerp3(c, [tint[0], tint[1], tint[2]], tint[3]);
            }
        }
        std::array::from_fn(|k| (c[k] * self.gain[k]).clamp(0.0, 1.0) as f32)
    }

    /// Matches the painting's colours on land to the game's own picture of the map (its preview,
    /// which the game drew with the map's light): the mean of each channel over the land, sampled
    /// on a grid, scaled to the picture's, within [`GAIN`] and [`TINT`]. A map whose picture is
    /// missing or blank keeps its colours.
    fn calibrate(&mut self) {
        let preview = match dds::decode(&self.map.preview, 0) {
            Ok(p) if p.width >= 16 && p.height >= 16 => p,
            Ok(p) => {
                log::debug!("not matched to its preview: the picture is {}x{}", p.width, p.height);
                return;
            }
            Err(e) => {
                log::debug!("not matched to its preview: {e}");
                return;
            }
        };
        let m = self.map;
        let (w, h) = (m.width as f64, m.height as f64);
        let fit = PreviewFit::of(w, h, preview.width as f64, preview.height as f64);
        let surface = m.water.as_ref().map_or(f64::MIN, |w| w.surface as f64 + 1.0);
        // The land on a grid of places, 48 a side, or four times as fine where that finds too
        // little (a map mostly sea: its islands, its plateaus).
        let mut found = ([0.0f64; 3], [0.0f64; 3], 0usize);
        for n in [48usize, 192] {
            let (mut ours, mut theirs, mut count) = ([0.0f64; 3], [0.0f64; 3], 0usize);
            for j in 0..n {
                for i in 0..n {
                    let (u, v) = ((i as f64 + 0.5) / n as f64 * w, (j as f64 + 0.5) / n as f64 * h);
                    if height(m, u, v) < surface {
                        continue;
                    }
                    // The picture's pixel there, and ours over the ground it covers.
                    let (px, py) = fit.at(u, v);
                    let p = sample(&preview, px, py, false);
                    let span = 1.0 / fit.scale;
                    let mut c = [0.0f64; 3];
                    for (du, dv) in [(-0.25, -0.25), (0.25, -0.25), (-0.25, 0.25), (0.25, 0.25)] {
                        let q = self.colour(u + du * span, v + dv * span);
                        for k in 0..3 {
                            c[k] += q[k] as f64 / 4.0;
                        }
                    }
                    for k in 0..3 {
                        ours[k] += c[k];
                        theirs[k] += p[k];
                    }
                    count += 1;
                }
            }
            found = (ours, theirs, count);
            if count >= MATCH_PLACES {
                break;
            }
        }
        let (ours, theirs, n) = found;
        // (Too little land to tell, or a blank picture.)
        if n < MATCH_PLACES || theirs.iter().all(|&t| t < 0.02 * n as f64) {
            log::debug!("not matched to its preview: land at {n} places, the picture's mean there {:?}", theirs.map(|v| (v / n.max(1) as f64 * 255.0).round()));
            return;
        }
        let wanted: [f64; 3] = std::array::from_fn(|k| if ours[k] > 1e-6 { theirs[k] / ours[k] } else { 1.0 });
        let mean = (wanted[0] + wanted[1] + wanted[2]) / 3.0;
        let level = mean.clamp(GAIN.0, GAIN.1);
        self.gain = wanted.map(|g| level * (g / mean).clamp(1.0 - TINT, 1.0 + TINT));
        let matched = Matched { places: n, ours: ours.map(|v| v / n as f64), theirs: theirs.map(|v| v / n as f64), wanted, gain: self.gain };
        log::debug!("matched to its preview: {matched:?}");
        self.matched = Some(matched);
    }

    /// How the colours were matched to the game's picture of the map ([`Painter::calibrate`]), if
    /// they were.
    pub fn matched(&self) -> Option<Matched> {
        self.matched
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scmap::{Layer, Lighting};

    /// A texture of one colour (uncompressed), `side` texels.
    fn plain(side: u32, rgba: [u8; 4]) -> Vec<u8> {
        let mut b = crate::dds::header(side, side, 1, None);
        for _ in 0..side * side {
            b.extend_from_slice(&[rgba[2], rgba[1], rgba[0], rgba[3]]);
        }
        b
    }

    #[test]
    fn strata_paint_by_their_masks_and_decals_over_them() {
        let dir = crate::tests::Temp::new("paint");
        let archive = dir.0.join("env.scd");
        let (red, green, blue) = (plain(4, [200, 0, 0, 255]), plain(4, [0, 200, 0, 255]), plain(4, [0, 0, 200, 255]));
        std::fs::write(&archive, crate::scd::tests::zip(&[("env/x/red.dds", &red, false), ("env/x/green.dds", &green, false), ("env/x/blue.dds", &blue, false)])).unwrap();
        let files = Files::of(Vec::new(), vec![crate::scd::Scd::open(&archive).unwrap()]);
        // A 32 x 32 map: red under everything, green where mask 0's red channel is full (its
        // west half), a blue decal 4 units square at (20, 20).
        let (w, h) = (32u32, 32u32);
        let mut mask = crate::dds::header(16, 16, 1, None);
        for _y in 0..16 {
            for x in 0..16 {
                let r = if x < 8 { 255 } else { 0 };
                mask.extend_from_slice(&[0, 0, r, 0]);
            }
        }
        let mut albedo = vec![Layer { path: "/env/x/red.dds".into(), scale: 4.0 }, Layer { path: "/env/x/green.dds".into(), scale: 4.0 }];
        albedo.resize(10, Layer::default());
        let map = ScMap {
            width: w,
            height: h,
            height_scale: 1.0 / 128.0,
            heights: vec![1280; ((w + 1) * (h + 1)) as usize],
            shader: "TTerrainXP".into(),
            albedo,
            masks: [mask, Vec::new()],
            decals: vec![crate::scmap::Decal { kind: 1, textures: vec!["/env/x/blue.dds".into()], scale: [4.0, 4.0, 4.0], position: [20.0, 10.0, 20.0], rotation: [0.0; 3] }],
            // (Light of one: the sun overhead, white, nothing else.)
            lighting: Lighting { multiplier: 1.0, sun_direction: [0.0, 1.0, 0.0], ambience: [0.0; 3], sun_colour: [1.0; 3], ..Default::default() },
            ..Default::default()
        };
        let frame = Frame::of([0.0, 0.0, 32.0, 32.0]);
        let p = Painter::new(&map, &files, frame).unwrap();
        let near = |a: [f32; 3], b: [f32; 3]| (0..3).all(|k| (a[k] - b[k]).abs() < 0.01);
        let g = 200.0 / 255.0;
        assert!(near(p.colour(4.0, 4.0), [0.0, g, 0.0]), "{:?}", p.colour(4.0, 4.0));
        assert!(near(p.colour(28.0, 4.0), [g, 0.0, 0.0]), "{:?}", p.colour(28.0, 4.0));
        assert!(near(p.colour(22.0, 22.0), [0.0, 0.0, g]), "{:?}", p.colour(22.0, 22.0));
        assert!(near(p.colour(25.0, 25.0), [g, 0.0, 0.0]), "past the decal: {:?}", p.colour(25.0, 25.0));
    }

    /// A map of grey ground (`albedo`, 0 to 255), `w × h` world units, its heights `f(x, z)` (world
    /// units), lit by `lighting`, its preview `preview` (when not empty).
    fn grey_map(w: u32, h: u32, f: impl Fn(u32, u32) -> f32, lighting: Lighting, preview: Vec<u8>) -> (crate::tests::Temp, Files, ScMap) {
        let dir = crate::tests::Temp::new("paint-light");
        let archive = dir.0.join("env.scd");
        let grey = plain(4, [128, 128, 128, 255]);
        std::fs::write(&archive, crate::scd::tests::zip(&[("env/x/grey.dds", &grey, false)])).unwrap();
        let files = Files::of(Vec::new(), vec![crate::scd::Scd::open(&archive).unwrap()]);
        let mut albedo = vec![Layer { path: "/env/x/grey.dds".into(), scale: 4.0 }];
        albedo.resize(10, Layer::default());
        let map = ScMap {
            width: w,
            height: h,
            height_scale: 1.0 / 128.0,
            heights: (0..=h).flat_map(|z| (0..=w).map(move |x| (x, z))).map(|(x, z)| (f(x, z) * 128.0) as u16).collect(),
            shader: "TTerrainXP".into(),
            albedo,
            lighting,
            preview,
            ..Default::default()
        };
        (dir, files, map)
    }

    /// Ground lit as the game's terrain shader lights it: the sun's colour by how squarely it
    /// faces it, the ambience, the multiplier, and the shadows' fill where light falls short. A
    /// slope facing away from the sun, on a map without ambience, takes the fill: never black.
    /// A sun of one colour and a fill of another tint the ground as the game does.
    #[test]
    fn ground_is_lit_as_the_game_lights_it_and_slopes_away_from_the_sun_take_the_fill() {
        // The sun to the east, 45° up; a ramp rising eastwards (facing away) between x 16 and 24.
        let s = std::f32::consts::FRAC_1_SQRT_2;
        let lighting = Lighting { multiplier: 1.2, sun_direction: [s, s, 0.0], ambience: [0.0; 3], sun_colour: [0.72, 0.71, 0.27], shadow_fill: [0.70, 0.70, 1.04], ..Default::default() };
        let ramp = |x: u32, _z: u32| if x <= 16 { 10.0 } else if x >= 24 { 50.0 } else { 10.0 + (x - 16) as f32 * 5.0 };
        let (_dir, files, map) = grey_map(32, 32, ramp, lighting, Vec::new());
        let p = Painter::new(&map, &files, Frame::of([0.0, 0.0, 32.0, 32.0])).unwrap();
        let albedo = 128.0 / 255.0;
        // Flat ground: l = sun colour × cos 45°; 1.2 l + fill (1 - l).
        let flat = p.colour(6.0, 6.0);
        for k in 0..3 {
            let l = [0.72, 0.71, 0.27][k] * std::f64::consts::FRAC_1_SQRT_2;
            let want = (albedo * (1.2 * l + [0.70, 0.70, 1.04][k] * (1.0 - l))) as f32;
            assert!((flat[k] - want).abs() < 0.01, "channel {k}: {} against {want}", flat[k]);
        }
        // Lavender (blue the most), as the game shows Seraphim Glaciers: not cream.
        assert!(flat[2] > flat[0] && flat[2] > flat[1], "{flat:?}");
        // The slope facing away from the sun: the fill alone.
        let away = p.colour(20.0, 16.0);
        for k in 0..3 {
            let want = (albedo * [0.70, 0.70, 1.04][k]).min(1.0) as f32;
            assert!((away[k] - want).abs() < 0.01, "channel {k}: {} against {want}", away[k]);
        }
    }

    /// A way for tanks opened through a cliff is painted as the ground beside it, level: not as
    /// the cliff's face, dark where it faced away from the sun.
    #[test]
    fn a_way_through_a_cliff_is_painted_as_level_ground() {
        let s = std::f32::consts::FRAC_1_SQRT_2;
        let lighting = Lighting { multiplier: 1.2, sun_direction: [-s, s, 0.0], ambience: [0.0; 3], sun_colour: [1.0; 3], shadow_fill: [0.3; 3], ..Default::default() };
        // A cliff 20 units high at x 16, its face to the east (away from a western sun).
        let (_dir, files, map) = grey_map(32, 32, |x, _| if x <= 16 { 30.0 } else { 10.0 }, lighting, Vec::new());
        let mut p = Painter::new(&map, &files, Frame::of([0.0, 0.0, 32.0, 32.0])).unwrap();
        let level = p.colour(6.0, 10.5);
        let face = p.colour(16.5, 10.5);
        assert!(face[0] < level[0] * 0.6, "the face is in shadow: {face:?} against {level:?}");
        let mut ways = vec![false; 32 * 32];
        let mut cliff = vec![false; 32 * 32];
        for z in 0..32 {
            cliff[z * 32 + 16] = true;
        }
        ways[10 * 32 + 16] = true;
        cliff[10 * 32 + 16] = false;
        p.open_ways(&ways, &cliff);
        let way = p.colour(16.5, 10.5);
        assert!((0..3).all(|k| (way[k] - level[k]).abs() < 0.01), "the way: {way:?} against level ground {level:?}");
        assert_eq!(p.colour(16.5, 20.5), face, "the cliff elsewhere as it was");
    }

    /// A map wider than it is deep shows in the middle of its square preview, black above and
    /// below: matched to the picture where the map is, not to the black (a 2:1 map's snow came
    /// out khaki when the whole picture was stretched over it).
    #[test]
    fn a_wide_maps_colours_match_its_letterboxed_preview() {
        assert_eq!(PreviewFit::of(512.0, 256.0, 256.0, 256.0), PreviewFit { scale: 0.5, x0: 0.0, y0: 64.0 });
        assert_eq!(PreviewFit::of(256.0, 256.0, 256.0, 256.0).at(128.0, 64.0), (128.0, 64.0));
        assert_eq!(PreviewFit::of(256.0, 512.0, 256.0, 256.0).at(0.0, 512.0), (64.0, 256.0));
        // A 64 x 32 map whose picture is grey 153 in its middle half, black elsewhere.
        let mut preview = crate::dds::header(32, 32, 1, None);
        for y in 0..32 {
            for _x in 0..32 {
                let v = if (8..24).contains(&y) { 153 } else { 0 };
                preview.extend_from_slice(&[v, v, v, 255]);
            }
        }
        let lighting = Lighting { multiplier: 1.0, sun_direction: [0.0, 1.0, 0.0], ambience: [0.0; 3], sun_colour: [1.0; 3], ..Default::default() };
        let (_dir, files, map) = grey_map(64, 32, |_, _| 10.0, lighting, preview);
        let p = Painter::new(&map, &files, Frame::of([0.0, 0.0, 64.0, 32.0])).unwrap();
        let c = p.colour(20.0, 10.0);
        // (The picture's pixels at the band's edges blend into the black a little.)
        assert!(c.iter().all(|&v| (v - 0.6).abs() < 0.02), "matched to the picture's grey: {c:?}");
    }
}
