//! DirectDraw Surface textures (`.dds`), as Supreme Commander stores its pictures and TA Reborn
//! the paint of models with surfaces of their own (`surface.rs`): the block-compressed DXT1,
//! DXT3 and DXT5 (BC1, BC2 and BC3), and uncompressed colour of 32, 24, 16 and 8 bits described
//! by masks. One mip level is decoded at a time, or its blocks read as they are (to give a
//! graphics card that reads them); [`header`] and the block encoders write them.
//!
//! The encoders use plain integer and floating-point arithmetic in a fixed order (no powers,
//! roots or trigonometry): what they write is the same, byte for byte, on every computer.

use crate::{FormatError, Result};

/// The largest side read (Supreme Commander's are 4096 at most).
const MAX_SIDE: u32 = 16384;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Dxt1,
    Dxt3,
    Dxt5,
    /// Uncompressed, `bits` per texel, each channel where its mask says (alpha mask 0: opaque).
    Masked { bits: u32, masks: [u32; 4] },
}

/// A texture's header: its size, format and how many mip levels follow the first.
#[derive(Clone, Copy, Debug)]
pub struct Header {
    pub width: u32,
    pub height: u32,
    pub levels: u32,
    pub format: Format,
    /// Where the first level's texels start.
    data: usize,
}

/// A decoded level: RGBA, row by row from the top.
#[derive(Clone, Debug, Default)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<[u8; 4]>,
}

impl Image {
    /// The texel at (x, y), wrapped round the edges (textures repeat).
    #[inline]
    pub fn wrapped(&self, x: i64, y: i64) -> [u8; 4] {
        let (w, h) = (self.width as i64, self.height as i64);
        self.rgba[(y.rem_euclid(h) * w + x.rem_euclid(w)) as usize]
    }

    /// The texel at (x, y), clamped to the edges.
    #[inline]
    pub fn clamped(&self, x: i64, y: i64) -> [u8; 4] {
        let (w, h) = (self.width as i64, self.height as i64);
        self.rgba[(y.clamp(0, h - 1) * w + x.clamp(0, w - 1)) as usize]
    }
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

impl Header {
    pub fn parse(bytes: &[u8]) -> Result<Header> {
        let bad = |what: &str| FormatError::invalid("DDS", what);
        if bytes.len() < 128 || &bytes[..4] != b"DDS " || u32_at(bytes, 4) != 124 {
            return Err(bad("not a DDS texture"));
        }
        let height = u32_at(bytes, 12);
        let width = u32_at(bytes, 16);
        let levels = u32_at(bytes, 28).max(1);
        if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
            return Err(bad(&format!("bad size {width}x{height}")));
        }
        // The pixel format at 76: size, flags, FourCC, bits, then the R, G, B, A masks.
        let pf_flags = u32_at(bytes, 80);
        let fourcc = &bytes[84..88];
        let format = if pf_flags & 0x4 != 0 {
            match fourcc {
                b"DXT1" => Format::Dxt1,
                b"DXT3" => Format::Dxt3,
                b"DXT5" => Format::Dxt5,
                other => return Err(bad(&format!("unsupported compression {:?}", String::from_utf8_lossy(other)))),
            }
        } else {
            let bits = u32_at(bytes, 88);
            let alpha = if pf_flags & 0x1 != 0 { u32_at(bytes, 104) } else { 0 };
            let masks = [u32_at(bytes, 92), u32_at(bytes, 96), u32_at(bytes, 100), alpha];
            if !matches!(bits, 8 | 16 | 24 | 32) {
                return Err(bad(&format!("{bits}-bit texels")));
            }
            // (Luminance only: one mask, read into red, green and blue alike.)
            let masks = if pf_flags & 0x20000 != 0 { [masks[0], masks[0], masks[0], masks[3]] } else { masks };
            Format::Masked { bits, masks }
        };
        Ok(Header { width, height, levels, format, data: 128 })
    }

    /// The size of mip level `level` (each half the last, at least 1).
    pub fn level_size(&self, level: u32) -> (u32, u32) {
        ((self.width >> level).max(1), (self.height >> level).max(1))
    }

    fn level_bytes(&self, level: u32) -> usize {
        let (w, h) = self.level_size(level);
        match self.format {
            Format::Dxt1 => w.div_ceil(4) as usize * h.div_ceil(4) as usize * 8,
            Format::Dxt3 | Format::Dxt5 => w.div_ceil(4) as usize * h.div_ceil(4) as usize * 16,
            Format::Masked { bits, .. } => w as usize * h as usize * (bits / 8) as usize,
        }
    }

    /// Where mip level `level`'s texels start in the file, and how many bytes they take.
    pub fn level_range(&self, level: u32) -> (usize, usize) {
        let at = self.data + (0..level).map(|l| self.level_bytes(l)).sum::<usize>();
        (at, self.level_bytes(level))
    }

    /// Mip level `level`'s texels as the file holds them (its blocks, for a compressed one), or
    /// none if the file ends before them.
    pub fn level_data<'a>(&self, bytes: &'a [u8], level: u32) -> Option<&'a [u8]> {
        if level >= self.levels {
            return None;
        }
        let (at, len) = self.level_range(level);
        bytes.get(at..at.checked_add(len)?)
    }

    /// The level whose larger side is the smallest still at least `side` (or the first).
    pub fn level_for(&self, side: u32) -> u32 {
        (0..self.levels).rev().find(|&l| { let (w, h) = self.level_size(l); w.max(h) >= side }).unwrap_or(0)
    }
}

/// Decodes mip level `level` of a texture (the last there is, when it has fewer).
pub fn decode(bytes: &[u8], level: u32) -> Result<Image> {
    let header = Header::parse(bytes)?;
    let level = level.min(header.levels - 1);
    let mut at = header.data;
    for l in 0..level {
        at += header.level_bytes(l);
    }
    let need = header.level_bytes(level);
    let Some(data) = bytes.get(at..at + need) else {
        // (A texture missing its smaller levels: its first, if that is there.)
        if level > 0 {
            return decode(bytes, 0);
        }
        return Err(FormatError::invalid("DDS", "texels missing"));
    };
    let (w, h) = header.level_size(level);
    let mut rgba = vec![[0u8; 4]; w as usize * h as usize];
    match header.format {
        Format::Dxt1 | Format::Dxt3 | Format::Dxt5 => {
            let block_bytes = if header.format == Format::Dxt1 { 8 } else { 16 };
            let (bw, bh) = (w.div_ceil(4) as usize, h.div_ceil(4) as usize);
            for by in 0..bh {
                for bx in 0..bw {
                    let block = &data[(by * bw + bx) * block_bytes..][..block_bytes];
                    let texels = match header.format {
                        Format::Dxt1 => dxt1(block, true),
                        Format::Dxt3 => {
                            let mut t = dxt1(&block[8..], false);
                            for (i, px) in t.iter_mut().enumerate() {
                                let nibble = (block[i / 2] >> ((i % 2) * 4)) & 0xF;
                                px[3] = nibble * 17;
                            }
                            t
                        }
                        _ => {
                            let mut t = dxt1(&block[8..], false);
                            let alphas = dxt5_alpha(&block[..8]);
                            for (px, a) in t.iter_mut().zip(alphas) {
                                px[3] = a;
                            }
                            t
                        }
                    };
                    for (i, texel) in texels.iter().enumerate() {
                        let (x, y) = (bx * 4 + i % 4, by * 4 + i / 4);
                        if x < w as usize && y < h as usize {
                            rgba[y * w as usize + x] = *texel;
                        }
                    }
                }
            }
        }
        Format::Masked { bits, masks } => {
            let step = (bits / 8) as usize;
            for (i, px) in rgba.iter_mut().enumerate() {
                let mut v = 0u32;
                for b in 0..step {
                    v |= (data[i * step + b] as u32) << (8 * b);
                }
                let channel = |mask: u32| -> u8 {
                    if mask == 0 {
                        return 0;
                    }
                    let shift = mask.trailing_zeros();
                    let max = mask >> shift;
                    (((v & mask) >> shift) * 255 / max) as u8
                };
                *px = [channel(masks[0]), channel(masks[1]), channel(masks[2]), if masks[3] == 0 { 255 } else { channel(masks[3]) }];
            }
        }
    }
    Ok(Image { width: w, height: h, rgba })
}

/// A 565 colour's 8-bit channels.
fn rgb565(c: u16) -> [u16; 3] {
    let (r, g, b) = ((c >> 11) & 31, (c >> 5) & 63, c & 31);
    [(r << 3) | (r >> 2), (g << 2) | (g >> 4), (b << 3) | (b >> 2)]
}

/// A colour block's 16 texels (DXT1's, or the colour half of DXT3's and DXT5's, which always
/// use four colours: `punch` lets DXT1 use its transparent black).
fn dxt1(block: &[u8], punch: bool) -> [[u8; 4]; 16] {
    let (c0, c1) = (u16::from_le_bytes([block[0], block[1]]), u16::from_le_bytes([block[2], block[3]]));
    let (a, b) = (rgb565(c0), rgb565(c1));
    let mut palette = [[0u8; 4]; 4];
    palette[0] = [a[0] as u8, a[1] as u8, a[2] as u8, 255];
    palette[1] = [b[0] as u8, b[1] as u8, b[2] as u8, 255];
    if c0 > c1 || !punch {
        for i in 0..3 {
            palette[2][i] = ((2 * a[i] + b[i] + 1) / 3) as u8;
            palette[3][i] = ((a[i] + 2 * b[i] + 1) / 3) as u8;
        }
        palette[2][3] = 255;
        palette[3][3] = 255;
    } else {
        for i in 0..3 {
            palette[2][i] = ((a[i] + b[i]) / 2) as u8;
        }
        palette[2][3] = 255;
        palette[3] = [0, 0, 0, 0];
    }
    let bits = u32::from_le_bytes([block[4], block[5], block[6], block[7]]);
    std::array::from_fn(|i| palette[((bits >> (2 * i)) & 3) as usize])
}

/// A DXT5 alpha block's 16 values.
fn dxt5_alpha(block: &[u8]) -> [u8; 16] {
    let table = alpha_table(block[0], block[1]);
    let mut bits = 0u64;
    for (i, &byte) in block[2..8].iter().enumerate() {
        bits |= (byte as u64) << (8 * i);
    }
    std::array::from_fn(|i| table[((bits >> (3 * i)) & 7) as usize])
}

/// A DDS file's header: `width` by `height`, `levels` mip levels, compressed as `fourcc` says
/// (`DXT1`, `DXT3`, `DXT5`) or, with none, uncompressed 32-bit BGRA. The texels follow it, level
/// by level from the largest.
pub fn header(width: u32, height: u32, levels: u32, fourcc: Option<&[u8; 4]>) -> Vec<u8> {
    let mut h = vec![0u8; 128];
    h[..4].copy_from_slice(b"DDS ");
    h[4..8].copy_from_slice(&124u32.to_le_bytes());
    // (Caps, height, width and pixel format given; the mip levels' count; for a compressed one
    // its first level's size, for an uncompressed one its rows' pitch.)
    let mut flags: u32 = 0x1 | 0x2 | 0x4 | 0x1000;
    if levels > 1 {
        flags |= 0x20000;
    }
    let (size_flag, size) = match fourcc {
        Some(b"DXT1") => (0x80000, width.div_ceil(4) * height.div_ceil(4) * 8),
        Some(_) => (0x80000, width.div_ceil(4) * height.div_ceil(4) * 16),
        None => (0x8, width * 4),
    };
    h[8..12].copy_from_slice(&(flags | size_flag).to_le_bytes());
    h[12..16].copy_from_slice(&height.to_le_bytes());
    h[16..20].copy_from_slice(&width.to_le_bytes());
    h[20..24].copy_from_slice(&size.to_le_bytes());
    h[28..32].copy_from_slice(&levels.to_le_bytes());
    h[76..80].copy_from_slice(&32u32.to_le_bytes());
    match fourcc {
        Some(cc) => {
            h[80..84].copy_from_slice(&4u32.to_le_bytes());
            h[84..88].copy_from_slice(cc);
        }
        None => {
            h[80..84].copy_from_slice(&0x41u32.to_le_bytes());
            h[88..92].copy_from_slice(&32u32.to_le_bytes());
            h[92..96].copy_from_slice(&0x00FF_0000u32.to_le_bytes());
            h[96..100].copy_from_slice(&0x0000_FF00u32.to_le_bytes());
            h[100..104].copy_from_slice(&0x0000_00FFu32.to_le_bytes());
            h[104..108].copy_from_slice(&0xFF00_0000u32.to_le_bytes());
        }
    }
    // (A texture, with mip levels: a complex one.)
    let caps = if levels > 1 { 0x1000 | 0x400000 | 0x8 } else { 0x1000 };
    h[108..112].copy_from_slice(&(caps as u32).to_le_bytes());
    h
}

/// A colour block's 16 texels (BC1's, `punch` letting it use its transparent black; or the
/// colour half of BC2's and BC3's, which always take four colours), row by row.
pub fn decode_colour_block(block: &[u8], punch: bool) -> [[u8; 4]; 16] {
    dxt1(block, punch)
}

/// A BC3 alpha block's 16 values, row by row.
pub fn decode_alpha_block(block: &[u8]) -> [u8; 16] {
    dxt5_alpha(block)
}

/// 16 texels (row by row) as a BC3 block: their alpha, then their colour.
pub fn encode_bc3_block(texels: &[[u8; 4]; 16]) -> [u8; 16] {
    let mut out = [0u8; 16];
    out[..8].copy_from_slice(&encode_alpha_block(&texels.map(|t| t[3])));
    out[8..].copy_from_slice(&encode_colour_block(texels));
    out
}

/// 16 alpha values (row by row) as a BC3 alpha block: their least and greatest as its ends,
/// eight values between, each the nearest.
pub fn encode_alpha_block(alphas: &[u8; 16]) -> [u8; 8] {
    let (lo, hi) = (*alphas.iter().min().unwrap(), *alphas.iter().max().unwrap());
    let mut out = [hi, lo, 0, 0, 0, 0, 0, 0];
    if lo == hi {
        // (Every value the first end.)
        return out;
    }
    let table = alpha_table(hi, lo);
    let mut bits = 0u64;
    for (i, &a) in alphas.iter().enumerate() {
        let best = (0..8).min_by_key(|&k| (table[k] as i32 - a as i32).abs()).unwrap();
        bits |= (best as u64) << (3 * i);
    }
    out[2..].copy_from_slice(&bits.to_le_bytes()[..6]);
    out
}

/// 16 texels' colours (row by row) as a four-colour block (BC1's, or the colour half of BC2's
/// and BC3's; the first end the greater, so BC1 reads it with four colours too): its ends along
/// the colours' main axis, refined once by least squares when that fits them better, each texel
/// the nearest of the four colours.
pub fn encode_colour_block(texels: &[[u8; 4]; 16]) -> [u8; 8] {
    let rgb: [[i64; 3]; 16] = texels.map(|t| [t[0] as i64, t[1] as i64, t[2] as i64]);
    // The colours' spread about their mean, in whole numbers (sixteen times each less the sum).
    let mut sum = [0i64; 3];
    for c in &rgb {
        for k in 0..3 {
            sum[k] += c[k];
        }
    }
    let mut cov = [[0i64; 3]; 3];
    for c in &rgb {
        let d = [16 * c[0] - sum[0], 16 * c[1] - sum[1], 16 * c[2] - sum[2]];
        for i in 0..3 {
            for j in 0..3 {
                cov[i][j] += d[i] * d[j];
            }
        }
    }
    // Its main axis: a few steps of power iteration from the widest channel.
    let widest = (0..3).max_by_key(|&i| (cov[i][i], std::cmp::Reverse(i))).unwrap();
    let mut axis = [0f64; 3];
    axis[widest] = 1.0;
    let cov = cov.map(|row| row.map(|x| x as f64));
    for _ in 0..8 {
        let next = [0, 1, 2].map(|i| cov[i][0] * axis[0] + cov[i][1] * axis[1] + cov[i][2] * axis[2]);
        let largest = next.iter().fold(0f64, |m, &x| m.max(x.abs()));
        if largest == 0.0 {
            break;
        }
        axis = next.map(|x| x / largest);
    }
    let along = |c: &[i64; 3]| c[0] as f64 * axis[0] + c[1] as f64 * axis[1] + c[2] as f64 * axis[2];
    let (mut lo, mut hi) = (0, 0);
    for (i, c) in rgb.iter().enumerate() {
        if along(c) < along(&rgb[lo]) {
            lo = i;
        }
        if along(c) > along(&rgb[hi]) {
            hi = i;
        }
    }
    let first = colour_block(&rgb, to_565(rgb[hi]), to_565(rgb[lo]));
    // Least squares: the ends that best give each texel's colour from the weights its index
    // gives them (3, 0, 2 and 1 thirds of the first end).
    let bits = u32::from_le_bytes([first.0[4], first.0[5], first.0[6], first.0[7]]);
    let weight = |i: usize| -> f64 { [3.0, 0.0, 2.0, 1.0][((bits >> (2 * i)) & 3) as usize] / 3.0 };
    let (mut aa, mut ab, mut bb) = (0f64, 0f64, 0f64);
    let (mut ax, mut bx) = ([0f64; 3], [0f64; 3]);
    for (i, c) in rgb.iter().enumerate() {
        let (w, v) = (weight(i), 1.0 - weight(i));
        aa += w * w;
        ab += w * v;
        bb += v * v;
        for k in 0..3 {
            ax[k] += w * c[k] as f64;
            bx[k] += v * c[k] as f64;
        }
    }
    let det = aa * bb - ab * ab;
    if det.abs() < 1e-9 {
        return first.0;
    }
    let solve = |k: usize| -> (i64, i64) {
        let a = (ax[k] * bb - bx[k] * ab) / det;
        let b = (bx[k] * aa - ax[k] * ab) / det;
        (a.round().clamp(0.0, 255.0) as i64, b.round().clamp(0.0, 255.0) as i64)
    };
    let ends = [solve(0), solve(1), solve(2)];
    let refined = colour_block(&rgb, to_565([ends[0].0, ends[1].0, ends[2].0]), to_565([ends[0].1, ends[1].1, ends[2].1]));
    if refined.1 < first.1 { refined.0 } else { first.0 }
}

/// A texture as BC3 mip levels of `side` by `side` texels (a power of two) down to 1 by 1, each
/// level's blocks, the largest first: as the paint of a model with a surface is drawn (TA
/// Reborn's arrays of such layers). DXT5's blocks as they are; DXT1's and DXT3's made BC3 block
/// by block, their colours as they are where BC3 reads them the same; anything else (another
/// size, levels missing, uncompressed) decoded, resized and encoded.
pub fn to_bc3_levels(bytes: &[u8], side: u32) -> Result<Vec<Vec<u8>>> {
    if !side.is_power_of_two() {
        return Err(FormatError::invalid("DDS", format!("a side of {side} (not a power of two)")));
    }
    let h = Header::parse(bytes)?;
    let count = side.ilog2() + 1;
    if let Some(first) = (0..h.levels).find(|&l| h.level_size(l) == (side, side))
        && first + count <= h.levels
        && (first..first + count).all(|l| h.level_data(bytes, l).is_some())
        && matches!(h.format, Format::Dxt1 | Format::Dxt3 | Format::Dxt5)
    {
        return Ok((first..first + count).map(|l| blocks_to_bc3(h.format, h.level_data(bytes, l).unwrap())).collect());
    }
    Ok(bc3_levels(resize(&decode(bytes, h.level_for(side))?, side, side)))
}

/// An image as BC3 mip levels, its own size down to 1 by 1 (each level's blocks, the largest
/// first; each level the last halved).
pub fn bc3_levels(mut image: Image) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        out.push(encode_bc3(&image));
        if image.width == 1 && image.height == 1 {
            return out;
        }
        image = halve(&image);
    }
}

/// A level's blocks made BC3's: DXT5's as they are; DXT1's colours as they are where they take
/// four (an opaque alpha block before them), encoded again where they take three and a
/// transparent black (which BC3 cannot); DXT3's colours as they are, their alpha encoded.
fn blocks_to_bc3(format: Format, data: &[u8]) -> Vec<u8> {
    match format {
        Format::Dxt1 => {
            let mut out = Vec::with_capacity(data.len() * 2);
            for block in data.as_chunks::<8>().0 {
                if u16::from_le_bytes([block[0], block[1]]) > u16::from_le_bytes([block[2], block[3]]) {
                    out.extend_from_slice(&[255, 255, 0, 0, 0, 0, 0, 0]);
                    out.extend_from_slice(block);
                } else {
                    out.extend_from_slice(&encode_bc3_block(&dxt1(block, true)));
                }
            }
            out
        }
        Format::Dxt3 => {
            let mut out = Vec::with_capacity(data.len());
            for block in data.as_chunks::<16>().0 {
                let alphas: [u8; 16] = std::array::from_fn(|i| ((block[i / 2] >> ((i % 2) * 4)) & 0xF) * 17);
                out.extend_from_slice(&encode_alpha_block(&alphas));
                out.extend_from_slice(&block[8..]);
            }
            out
        }
        _ => data.to_vec(),
    }
}

/// An image as BC3 blocks, row by row (the texels past its edge, in a level smaller than a
/// block, the edge's).
pub fn encode_bc3(image: &Image) -> Vec<u8> {
    let (bw, bh) = (image.width.div_ceil(4) as i64, image.height.div_ceil(4) as i64);
    let mut out = Vec::with_capacity((bw * bh * 16) as usize);
    for by in 0..bh {
        for bx in 0..bw {
            let texels: [[u8; 4]; 16] = std::array::from_fn(|i| image.clamped(bx * 4 + (i % 4) as i64, by * 4 + (i / 4) as i64));
            out.extend_from_slice(&encode_bc3_block(&texels));
        }
    }
    out
}

/// A BC3 level of `width` by `height` texels decoded: RGBA, row by row (for a graphics card that
/// cannot read the blocks itself).
pub fn decode_bc3(data: &[u8], width: u32, height: u32) -> Vec<[u8; 4]> {
    let (w, h) = (width as usize, height as usize);
    let bw = w.div_ceil(4);
    let mut rgba = vec![[0u8; 4]; w * h];
    for (k, block) in data.as_chunks::<16>().0.iter().enumerate() {
        let (bx, by) = (k % bw, k / bw);
        let colours = dxt1(&block[8..], false);
        let alphas = dxt5_alpha(&block[..8]);
        for i in 0..16 {
            let (x, y) = (bx * 4 + i % 4, by * 4 + i / 4);
            if x < w && y < h {
                rgba[y * w + x] = [colours[i][0], colours[i][1], colours[i][2], alphas[i]];
            }
        }
    }
    rgba
}

/// `image` halved each way (each texel the mean of the four it covers), down to 1.
pub fn halve(image: &Image) -> Image {
    let (w, h) = ((image.width / 2).max(1), (image.height / 2).max(1));
    let mut rgba = Vec::with_capacity(w as usize * h as usize);
    for y in 0..h as i64 {
        for x in 0..w as i64 {
            let quad = [image.clamped(2 * x, 2 * y), image.clamped(2 * x + 1, 2 * y), image.clamped(2 * x, 2 * y + 1), image.clamped(2 * x + 1, 2 * y + 1)];
            rgba.push(std::array::from_fn(|c| ((quad.iter().map(|t| t[c] as u32).sum::<u32>() + 2) / 4) as u8));
        }
    }
    Image { width: w, height: h, rgba }
}

/// `image` at `width` by `height`: halved while it is twice as large or more, then sampled
/// between texels (bilinear, in 256ths).
pub fn resize(image: &Image, width: u32, height: u32) -> Image {
    let mut image = image.clone();
    while image.width >= 2 * width && image.height >= 2 * height && (image.width, image.height) != (width, height) {
        image = halve(&image);
    }
    if (image.width, image.height) == (width, height) {
        return image;
    }
    let (sw, sh) = (image.width as i64, image.height as i64);
    // Where a target texel's middle falls in the source, in 256ths of a texel from its first
    // texel's middle.
    let at = |t: i64, target: i64, source: i64| -> (i64, i64) {
        let p = ((2 * t + 1) * source * 256) / (2 * target) - 128;
        (p.div_euclid(256), p.rem_euclid(256))
    };
    let mut rgba = Vec::with_capacity(width as usize * height as usize);
    for y in 0..height as i64 {
        let (y0, fy) = at(y, height as i64, sh);
        for x in 0..width as i64 {
            let (x0, fx) = at(x, width as i64, sw);
            let (a, b, c, d) = (image.clamped(x0, y0), image.clamped(x0 + 1, y0), image.clamped(x0, y0 + 1), image.clamped(x0 + 1, y0 + 1));
            rgba.push(std::array::from_fn(|k| {
                let top = a[k] as i64 * (256 - fx) + b[k] as i64 * fx;
                let bottom = c[k] as i64 * (256 - fx) + d[k] as i64 * fx;
                ((top * (256 - fy) + bottom * fy + 32768) >> 16) as u8
            }));
        }
    }
    Image { width, height, rgba }
}

/// A colour's nearest 565.
fn to_565(c: [i64; 3]) -> u16 {
    let q = |v: i64, max: i64| -> u16 { ((v * max + 127) / 255) as u16 };
    (q(c[0], 31) << 11) | (q(c[1], 63) << 5) | q(c[2], 31)
}

/// The four-colour block of ends `a` and `b` (in whichever order makes the first the greater)
/// with each texel's nearest colour, and how far off it comes (the sum of squares).
fn colour_block(rgb: &[[i64; 3]; 16], a: u16, b: u16) -> ([u8; 8], i64) {
    let (c0, c1) = if a >= b { (a, b) } else { (b, a) };
    let mut out = [0u8; 8];
    out[..2].copy_from_slice(&c0.to_le_bytes());
    out[2..4].copy_from_slice(&c1.to_le_bytes());
    let (p0, p1) = (rgb565(c0).map(i64::from), rgb565(c1).map(i64::from));
    let palette = [p0, p1, [0, 1, 2].map(|k| (2 * p0[k] + p1[k] + 1) / 3), [0, 1, 2].map(|k| (p0[k] + 2 * p1[k] + 1) / 3)];
    let (mut bits, mut error) = (0u32, 0i64);
    for (i, c) in rgb.iter().enumerate() {
        let off = |p: &[i64; 3]| (0..3).map(|k| (p[k] - c[k]) * (p[k] - c[k])).sum::<i64>();
        // (Equal ends: one colour, every texel the first.)
        let best = if c0 == c1 { 0 } else { (0..4).min_by_key(|&k| off(&palette[k])).unwrap() };
        error += off(&palette[best]);
        bits |= (best as u32) << (2 * i);
    }
    out[4..].copy_from_slice(&bits.to_le_bytes());
    (out, error)
}

/// A BC3 alpha block's eight values between its ends `a0` and `a1` (as the decoder takes them).
fn alpha_table(a0: u8, a1: u8) -> [u8; 8] {
    let (a0, a1) = (a0 as u16, a1 as u16);
    let mut table = [0u8; 8];
    table[0] = a0 as u8;
    table[1] = a1 as u8;
    if a0 > a1 {
        for i in 1..7u16 {
            table[i as usize + 1] = (((7 - i) * a0 + i * a1 + 3) / 7) as u8;
        }
    } else {
        for i in 1..5u16 {
            table[i as usize + 1] = (((5 - i) * a0 + i * a1 + 2) / 5) as u8;
        }
        table[6] = 0;
        table[7] = 255;
    }
    table
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn uncompressed_texels_and_their_mip_levels_decode() {
        // 4x2 BGRA, then its 2x1 and 1x1 levels.
        let mut bytes = header(4, 2, 3, None);
        for i in 0..8u8 {
            bytes.extend_from_slice(&[i, 10 + i, 20 + i, 200]);
        }
        bytes.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        bytes.extend_from_slice(&[9, 9, 9, 9]);
        let image = decode(&bytes, 0).unwrap();
        assert_eq!((image.width, image.height), (4, 2));
        assert_eq!(image.rgba[5], [25, 15, 5, 200]);
        assert_eq!(image.wrapped(-1, 2), image.rgba[3]);
        let small = decode(&bytes, 1).unwrap();
        assert_eq!((small.width, small.height, small.rgba[1]), (2, 1, [7, 6, 5, 8]));
        let header = Header::parse(&bytes).unwrap();
        assert_eq!((header.level_for(4), header.level_for(2), header.level_for(1)), (0, 1, 2));
    }

    #[test]
    fn dxt_blocks_decode_to_their_colours() {
        // One DXT1 block: red and blue, texel codes 0, 1, 2, 3 along the first row.
        let mut bytes = header(4, 4, 1, Some(b"DXT1"));
        bytes.extend_from_slice(&0xF800u16.to_le_bytes());
        bytes.extend_from_slice(&0x001Fu16.to_le_bytes());
        bytes.extend_from_slice(&[0b1110_0100, 0, 0, 0]);
        let image = decode(&bytes, 0).unwrap();
        assert_eq!(image.rgba[0], [255, 0, 0, 255]);
        assert_eq!(image.rgba[1], [0, 0, 255, 255]);
        assert_eq!(image.rgba[2], [170, 0, 85, 255]);
        assert_eq!(image.rgba[4], [255, 0, 0, 255]);
        // DXT5: alpha 255 and 0, interpolated along the first row.
        let mut bytes = header(4, 4, 1, Some(b"DXT5"));
        bytes.extend_from_slice(&[255, 0, 0b10_001_000, 0b0000_0110, 0, 0, 0, 0]);
        bytes.extend_from_slice(&0x07E0u16.to_le_bytes());
        bytes.extend_from_slice(&0x07E0u16.to_le_bytes());
        bytes.extend_from_slice(&[0; 4]);
        let image = decode(&bytes, 0).unwrap();
        assert_eq!(image.rgba[0], [0, 255, 0, 255]);
        assert_eq!(image.rgba[1][3], 0);
        assert_eq!(image.rgba[2][3], 219);
        assert!(decode(b"DDS nonsense", 0).is_err());
    }

    /// Blocks encoded and decoded come back close: two colours exactly, a gradient within a
    /// few steps, alpha's ends exactly; and a written file's levels read back where they are.
    #[test]
    fn encoded_blocks_decode_close_to_what_was_encoded() {
        let two: [[u8; 4]; 16] = std::array::from_fn(|i| if i % 3 == 0 { [255, 0, 0, 0] } else { [0, 0, 255, 255] });
        let block = encode_bc3_block(&two);
        let colours = decode_colour_block(&block[8..], false);
        let alphas = decode_alpha_block(&block[..8]);
        for i in 0..16 {
            assert_eq!([colours[i][0], colours[i][1], colours[i][2], alphas[i]], two[i], "texel {i}");
        }
        // A ramp of sixteen colours in four: each within half a step of the four's (a sixth of
        // the ramp, 40 of red's 240 and 20 of green's 120) and a 565's rounding.
        let ramp: [[u8; 4]; 16] = std::array::from_fn(|i| [(i * 16) as u8, (i * 8) as u8, 40, 255]);
        let back = decode_colour_block(&encode_colour_block(&ramp), false);
        for i in 0..16 {
            for (k, most) in [44, 24, 5].into_iter().enumerate() {
                assert!((back[i][k] as i32 - ramp[i][k] as i32).abs() <= most, "texel {i} channel {k}: {:?} for {:?}", back[i], ramp[i]);
            }
        }
        let flat = [[90, 90, 90, 255]; 16];
        // (One colour: its nearest 565, every texel.)
        assert_eq!(decode_colour_block(&encode_colour_block(&flat), false)[5], [90, 89, 90, 255]);
        let mut file = header(8, 4, 2, Some(b"DXT5"));
        file.extend_from_slice(&[1; 32]);
        file.extend_from_slice(&[2; 16]);
        let h = Header::parse(&file).unwrap();
        assert_eq!((h.width, h.height, h.levels, h.format), (8, 4, 2, Format::Dxt5));
        assert_eq!(h.level_data(&file, 0), Some(&[1u8; 32][..]));
        assert_eq!(h.level_data(&file, 1), Some(&[2u8; 16][..]));
        assert_eq!(h.level_data(&file, 2), None);
    }

    /// Any texture made BC3 levels of a side: DXT5's of that size copied, DXT1's four-colour
    /// blocks kept under an opaque alpha, three-colour ones encoded again; a smaller texture
    /// grown and its levels made.
    #[test]
    fn textures_become_bc3_levels_of_one_side() {
        // A DXT5 of 8 x 8 with all four levels: copied as it is.
        let mut file = header(8, 8, 4, Some(b"DXT5"));
        for (len, byte) in [(64, 1u8), (16, 2), (16, 3), (16, 4)] {
            file.extend(std::iter::repeat_n(byte, len));
        }
        let levels = to_bc3_levels(&file, 8).unwrap();
        assert_eq!(levels.iter().map(|l| (l.len(), l[0])).collect::<Vec<_>>(), [(64, 1), (16, 2), (16, 3), (16, 4)]);
        // From its 4 x 4 level down.
        assert_eq!(to_bc3_levels(&file, 4).unwrap().len(), 3);
        // A DXT1 of one level, 4 x 4: two four-colour blocks... one block, kept.
        let mut file = header(4, 4, 1, Some(b"DXT1"));
        file.extend_from_slice(&0xF800u16.to_le_bytes());
        file.extend_from_slice(&0x001Fu16.to_le_bytes());
        file.extend_from_slice(&[0b1110_0100, 0, 0, 0]);
        let levels = to_bc3_levels(&file, 1);
        assert!(levels.is_ok());
        let four = to_bc3_levels(&file, 4).unwrap();
        // (A level of 4 x 4 but none below it: decoded and made again, its first level the same
        // colours.)
        let back = decode_bc3(&four[0], 4, 4);
        assert_eq!(back[0], [255, 0, 0, 255]);
        assert_eq!(back[1], [0, 0, 255, 255]);
        assert_eq!(four.len(), 3);
        // A three-colour DXT1 block (black, red and the transparent black) in a full chain (4 x 4,
        // 2 x 2, 1 x 1): made a BC3 block, the transparent black its alpha's.
        let mut file = header(4, 4, 3, Some(b"DXT1"));
        for _ in 0..3 {
            file.extend_from_slice(&0x0000u16.to_le_bytes());
            file.extend_from_slice(&0xF800u16.to_le_bytes());
            file.extend_from_slice(&[0b1111_0100, 0, 0, 0]);
        }
        let levels = to_bc3_levels(&file, 4).unwrap();
        let back = decode_bc3(&levels[0], 4, 4);
        assert_eq!(back[0], [0, 0, 0, 255]);
        assert_eq!(back[1], [255, 0, 0, 255]);
        assert_eq!(back[2], [0, 0, 0, 0], "the transparent black");
        // A 2 x 2 uncompressed texture grown to 8 x 8, four levels.
        let mut file = header(2, 2, 1, None);
        for _ in 0..4 {
            file.extend_from_slice(&[10, 200, 30, 255]);
        }
        let levels = to_bc3_levels(&file, 8).unwrap();
        assert_eq!(levels.iter().map(Vec::len).collect::<Vec<_>>(), [64, 16, 16, 16]);
        assert_eq!(decode_bc3(&levels[3], 1, 1)[0][..3], decode_bc3(&levels[0], 8, 8)[27][..3]);
        assert!(to_bc3_levels(&file, 6).is_err(), "not a power of two");
    }

    #[test]
    fn images_halve_and_resize_evenly() {
        let image = Image { width: 2, height: 2, rgba: vec![[0, 0, 0, 255], [100, 0, 0, 255], [0, 100, 0, 255], [100, 100, 0, 255]] };
        assert_eq!(halve(&image).rgba, [[50, 50, 0, 255]]);
        let grown = resize(&image, 4, 4);
        assert_eq!((grown.width, grown.height), (4, 4));
        // (The corners the corners' colours; between them, between.)
        assert_eq!(grown.rgba[0], [0, 0, 0, 255]);
        assert_eq!(grown.rgba[15], [100, 100, 0, 255]);
        assert_eq!(grown.rgba[1], [25, 0, 0, 255]);
        assert_eq!(resize(&grown, 2, 2).rgba.len(), 4);
    }
}
