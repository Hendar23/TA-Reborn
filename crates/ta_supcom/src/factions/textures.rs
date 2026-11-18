//! A unit's paint as TA Reborn draws a model's surface (`ta_formats::surface`): its colour,
//! specular and team colour, and normal maps, each made BC3 with every mip level, as large as
//! Forged Alliance has it from [`LEAST`] to [`MOST`] texels a side (`ta_formats::dds::paint_side`).
//! Forged Alliance's blocks are kept as they are wherever a texture has a level of that size
//! (all of its 512s and 1024s; its 2048s from their 1024 level); the smaller ones are enlarged,
//! twice each way at a time keeping their edges sharp (`dds::enlarge`), and encoded again
//! (`ta_formats::dds::to_bc3_levels`).

use ta_formats::dds;

use crate::Result;

/// The least and the largest side of the paint's textures: a texture of 1024 (most of the
/// buildings', the commanders', the larger ships' and experimentals') keeps its texels, where
/// halved to 512 a factory up close was drawn from a quarter of what Forged Alliance paints it
/// with.
pub const LEAST: u32 = dds::PAINT_LEAST;
pub const MOST: u32 = 1024;

/// A texture made the paint's: a DDS file of BC3 levels, of its own side ([`LEAST`] to [`MOST`]).
pub fn texture(bytes: &[u8]) -> Result<Vec<u8>> {
    let side = dds::paint_side(&dds::Header::parse(bytes)?, MOST);
    let levels = dds::to_bc3_levels(bytes, side)?;
    let mut out = dds::header(side, side, levels.len() as u32, Some(b"DXT5"));
    for level in levels {
        out.extend_from_slice(&level);
    }
    Ok(out)
}

/// How much brighter Forged Alliance draws a colour map than its texels hold it, in 256ths, by the
/// shader its unit is drawn with (its first level of detail's `ShaderName`). Forged Alliance's
/// light multiplies the paint in the texels' own (gamma) space: by its map's lighting multiplier
/// and the sun (about 1.3 and 1.3 on its maps, a lit face's paint some 1.6 times as bright as its
/// texel), the UEF's `Unit` adding twice the sky its specular map's red mirrors, the Cybran's
/// `Insect` taking the sun twice (some 2.3 times), the Aeon's `Aeon` six tenths of it (about as
/// bright as the texel, its mirrored sky on top) and the Seraphim's `Seraphim` the shadows' light
/// alone (their look the sky they mirror). TA Reborn lights a colour map as the colour of what is
/// painted (its sun and sky together bringing a lit face to about 1.3 times its colour, in light's
/// own measure): these bring a lit face of each as bright as Forged Alliance draws it.
pub fn paint_gain(shader: &str) -> u32 {
    match shader.to_ascii_lowercase().as_str() {
        "unit" | "metal" => 371,
        "insect" => 512,
        _ => 256,
    }
}

/// A paint texture (a DDS of BC3 levels, as [`texture`] makes them) brightened by `gain` 256ths:
/// each block's two colour ends multiplied, short of white, which brightens every texel the block
/// gives as much (its other two colours are blends of the ends); the alpha as it is. Forged
/// Alliance's blocks stay as they were made, and the ends keep their order.
pub fn brightened(texture: &[u8], gain: u32) -> Vec<u8> {
    let mut out = texture.to_vec();
    let Ok(header) = dds::Header::parse(texture) else { return out };
    if gain == 256 || header.format != dds::Format::Dxt5 {
        return out;
    }
    for level in 0..header.levels {
        let (at, len) = header.level_range(level);
        let Some(blocks) = out.get_mut(at..at + len) else { break };
        for block in blocks.as_chunks_mut::<16>().0 {
            brighten_block(&mut block[8..], gain);
        }
    }
    out
}

/// A colour block (BC3's colour half: two 565 ends, then sixteen 2-bit indices) brightened.
fn brighten_block(colour: &mut [u8], gain: u32) {
    let scale = |c: u16| -> u16 {
        let up = |v: u16, most: u32| ((v as u32 * gain + 128) >> 8).min(most) as u16;
        (up(c >> 11, 31) << 11) | (up((c >> 5) & 63, 63) << 5) | up(c & 31, 31)
    };
    let (c0, c1) = (u16::from_le_bytes([colour[0], colour[1]]), u16::from_le_bytes([colour[2], colour[3]]));
    let (mut n0, mut n1) = (scale(c0), scale(c1));
    let mut bits = u32::from_le_bytes([colour[4], colour[5], colour[6], colour[7]]);
    // (Ends whose order brightening turned round are swapped back, each index to the same colour:
    // the first end's and the second's, and the two blends.)
    if n0 != n1 && (c0 > c1) != (n0 > n1) {
        std::mem::swap(&mut n0, &mut n1);
        bits ^= 0x5555_5555;
    }
    colour[..2].copy_from_slice(&n0.to_le_bytes());
    colour[2..4].copy_from_slice(&n1.to_le_bytes());
    colour[4..].copy_from_slice(&bits.to_le_bytes());
}

/// How much the Seraphim's player's colour glows, in 255ths of the specular map's blue (their
/// lights): Forged Alliance draws them (`UnitFalloff`) with a ramp of colour added over their paint
/// as light where their colour map's alpha says, which the converter makes the player's colour
/// ([`team_from_alpha`]); laid on as paint alone, their markings were flat yellow or green paint
/// on the chrome.
pub const SERAPHIM_GLOW: u32 = 64;

/// A paint texture of BC3 levels (as [`texture`] makes them) as BC1 (DXT1), the same colours in
/// half the bytes: each block's colour half, its alpha left out (a colour map's alpha is not
/// drawn: the Seraphim's, their player's colour, is moved into their specular map's first,
/// [`team_from_alpha`]). BC1 reads a block whose first end is not the greater with three colours
/// and a see-through black, where BC3 reads four: such a block's ends are swapped and each index
/// turned to the same colour, or, both ends the same colour, every texel given the first.
pub fn opaque(texture: &[u8]) -> Vec<u8> {
    let Ok(header) = dds::Header::parse(texture) else { return texture.to_vec() };
    if header.format != dds::Format::Dxt5 {
        return texture.to_vec();
    }
    let mut out = dds::header(header.width, header.height, header.levels, Some(b"DXT1"));
    for level in 0..header.levels {
        let Some(data) = header.level_data(texture, level) else { break };
        for block in data.as_chunks::<16>().0 {
            let mut colour: [u8; 8] = block[8..].try_into().expect("a colour half");
            let (c0, c1) = (u16::from_le_bytes([colour[0], colour[1]]), u16::from_le_bytes([colour[2], colour[3]]));
            if c0 < c1 {
                let bits = u32::from_le_bytes([colour[4], colour[5], colour[6], colour[7]]) ^ 0x5555_5555;
                colour[..2].copy_from_slice(&c1.to_le_bytes());
                colour[2..4].copy_from_slice(&c0.to_le_bytes());
                colour[4..].copy_from_slice(&bits.to_le_bytes());
            } else if c0 == c1 {
                colour[4..].fill(0);
            }
            out.extend_from_slice(&colour);
        }
    }
    out
}

/// A specular map with the player's colour where an albedo's alpha says (both the paint's):
/// how the Seraphim's are painted (their specular maps' alpha empty), as the others' are, glowing
/// as far as `glow` 255ths of it says (their lights, the blue, at least that much of the alpha).
/// (The albedo taken at the specular map's size where the two differ.)
pub fn team_from_alpha(specular: &[u8], albedo: &[u8], glow: u32) -> Result<Vec<u8>> {
    let mut spec = dds::decode(specular, 0)?;
    let colour = {
        let header = dds::Header::parse(albedo)?;
        let colour = dds::decode(albedo, header.level_for(spec.width.max(spec.height)))?;
        if (colour.width, colour.height) == (spec.width, spec.height) { colour } else { dds::resize(&colour, spec.width, spec.height) }
    };
    for (s, c) in spec.rgba.iter_mut().zip(&colour.rgba) {
        s[3] = c[3];
        s[2] = s[2].max(((c[3] as u32 * glow + 127) / 255) as u8);
    }
    let (w, h) = (spec.width, spec.height);
    let levels = dds::bc3_levels(spec);
    let mut out = dds::header(w, h, levels.len() as u32, Some(b"DXT5"));
    for level in levels {
        out.extend_from_slice(&level);
    }
    Ok(out)
}

/// A texture of one colour (RGBA): what stands in for one a unit lacks (a grey colour, no gloss
/// or team colour, a flat normal).
pub fn plain(rgba: [u8; 4]) -> Vec<u8> {
    let mut file = dds::header(4, 4, 1, None);
    for _ in 0..16 {
        file.extend_from_slice(&[rgba[2], rgba[1], rgba[0], rgba[3]]);
    }
    texture(&file).expect("a plain texture")
}

/// What stands in for a missing colour map, specular map and normal map.
pub const PLAIN_ALBEDO: [u8; 4] = [128, 128, 128, 255];
pub const PLAIN_SPECULAR: [u8; 4] = [0, 64, 0, 0];
/// (x 0.5 in the alpha, y 0.5 in the green: straight out.)
pub const PLAIN_NORMALS: [u8; 4] = [128, 128, 255, 128];

/// A paint texture's colour on average (its last level's one texel).
pub fn mean(texture: &[u8]) -> [u8; 3] {
    let Ok(header) = dds::Header::parse(texture) else { return [128; 3] };
    let last = header.levels - 1;
    match header.level_data(texture, last) {
        Some(data) if header.format == dds::Format::Dxt5 => {
            let (w, h) = header.level_size(last);
            let texel = dds::decode_bc3(data, w, h)[0];
            [texel[0], texel[1], texel[2]]
        }
        _ => [128; 3],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_texture_becomes_bc3_of_the_side_with_every_level() {
        let paint = plain([200, 100, 50, 255]);
        let header = dds::Header::parse(&paint).unwrap();
        assert_eq!((header.width, header.height, header.levels, header.format), (LEAST, LEAST, 10, dds::Format::Dxt5));
        assert!(header.level_data(&paint, 9).is_some());
        assert_eq!(paint.len(), 128 + (0..10).map(|l| ((LEAST >> l).max(4) / 4).pow(2) as usize * 16).sum::<usize>());
        // (Its colour, to a 565's rounding.)
        let [r, g, b] = mean(&paint);
        assert!(r.abs_diff(200) <= 4 && g.abs_diff(100) <= 2 && b.abs_diff(50) <= 4, "{r} {g} {b}");
        // Made again, the same bytes.
        assert_eq!(texture(&paint).unwrap(), paint);
    }

    /// A DXT file of `side` with every level, its blocks numbered by their level.
    fn whole(side: u32, fourcc: &[u8; 4]) -> Vec<u8> {
        let levels = side.ilog2() + 1;
        let mut file = dds::header(side, side, levels, Some(fourcc));
        let block = if fourcc == b"DXT1" { 8 } else { 16 };
        for l in 0..levels {
            let blocks = ((side >> l).max(4) / 4).pow(2) as usize;
            file.extend(std::iter::repeat_n(l as u8 + 1, blocks * block));
        }
        file
    }

    /// Forged Alliance's 1024s keep their blocks, its 2048s are taken from their 1024 level, its
    /// 512s stay 512 and smaller ones are enlarged to 512.
    #[test]
    fn a_texture_keeps_its_own_side_from_512_to_1024() {
        let side = |bytes: &[u8]| {
            let h = dds::Header::parse(bytes).unwrap();
            assert_eq!(h.levels_held(bytes), h.levels, "every level");
            (h.width, h.height)
        };
        let big = texture(&whole(1024, b"DXT5")).unwrap();
        assert_eq!(side(&big), (1024, 1024));
        assert_eq!(&big[128..144], &[1; 16], "its own blocks");
        let huge = texture(&whole(2048, b"DXT5")).unwrap();
        assert_eq!(side(&huge), (1024, 1024));
        assert_eq!(&huge[128..144], &[2; 16], "from its second level");
        assert_eq!(side(&texture(&whole(512, b"DXT1")).unwrap()), (512, 512));
        assert_eq!(side(&texture(&whole(256, b"DXT5")).unwrap()), (512, 512));
        assert_eq!(side(&texture(&whole(64, b"DXT5")).unwrap()), (512, 512));
    }

    /// A colour map brightened as Forged Alliance lights it: every texel half as bright again,
    /// white staying white, its alpha and its blocks' indices as they were (to the end colours'
    /// 565 steps); the gain by the shader.
    #[test]
    fn a_colour_map_is_brightened_by_its_ends() {
        let mut file = dds::header(8, 8, 1, None);
        for i in 0..64u32 {
            let v = (i * 2) as u8;
            let texel = if i == 63 { [250, 250, 250, 7] } else { [v, v / 2, 255 - v, 200] };
            file.extend_from_slice(&[texel[2], texel[1], texel[0], texel[3]]);
        }
        let paint = texture(&file).unwrap();
        let bright = brightened(&paint, 384);
        assert_eq!(bright.len(), paint.len());
        assert_eq!(brightened(&paint, 256), paint, "no gain, no change");
        let (before, after) = (dds::decode(&paint, 0).unwrap(), dds::decode(&bright, 0).unwrap());
        for (b, a) in before.rgba.iter().zip(&after.rgba) {
            assert_eq!(a[3], b[3], "alpha untouched");
            for c in 0..3 {
                let want = (b[c] as u32 * 3 / 2).min(255);
                assert!(a[c] as u32 + 9 >= want && a[c] as u32 <= want + 9, "{b:?} -> {a:?}");
            }
        }
        // Every level, down to the last.
        let header = dds::Header::parse(&bright).unwrap();
        let last = header.levels - 1;
        assert_ne!(header.level_data(&bright, last), header.level_data(&paint, last));
        assert_eq!((paint_gain("Unit"), paint_gain("INSECT"), paint_gain("Aeon"), paint_gain("Seraphim")), (371, 512, 256, 256));
    }

    /// Ends whose order brightening turns round (both reds run past white, the second's green
    /// does not) are swapped back with their indices: each texel the same colour.
    #[test]
    fn a_brightened_block_keeps_its_ends_order() {
        // (First end red 20, no green; second red 19, green 10: the first the greater. Doubled,
        // both reds are full, so the second, its green over the first's, would be the greater.)
        let c0: u16 = 20 << 11;
        let c1: u16 = (19 << 11) | (10 << 5);
        let mut block = [0u8; 8];
        block[..2].copy_from_slice(&c0.to_le_bytes());
        block[2..4].copy_from_slice(&c1.to_le_bytes());
        block[4..].copy_from_slice(&0b11_10_01_00u32.to_le_bytes());
        let mut bright = block;
        brighten_block(&mut bright, 512);
        let (n0, n1) = (u16::from_le_bytes([bright[0], bright[1]]), u16::from_le_bytes([bright[2], bright[3]]));
        assert!(n0 > n1, "{n0:#x} {n1:#x}");
        let (before, after) = (dds::decode_colour_block(&block, false), dds::decode_colour_block(&bright, false));
        // (The first texel takes the first end, no green: now full red, still no green; the
        // second the second end, its green doubled; the blends between, in order.)
        assert_eq!(after[0][..3], [255, 0, 0]);
        assert_eq!(after[1][..3], [255, 81, 0]);
        assert!(before[2][1] < before[3][1] && after[2][1] < after[3][1] && after[3][1] < after[1][1]);
    }

    /// The Seraphim's team colour, from the albedo's alpha into the specular map's, glowing as far
    /// as asked, whatever the albedo's size.
    #[test]
    fn a_specular_map_takes_its_team_colour_from_the_albedo() {
        let (specular, albedo) = (plain([10, 200, 30, 0]), plain([90, 90, 90, 255]));
        let both = team_from_alpha(&specular, &albedo, 0).unwrap();
        let texel = dds::decode(&both, 0).unwrap().rgba[100];
        assert_eq!(texel[3], 255);
        assert!(texel[1].abs_diff(200) <= 4 && texel[2].abs_diff(30) <= 4, "{texel:?}");
        assert_eq!(dds::Header::parse(&both).unwrap().levels, 10);
        // (Glowing: its blue at least the glow's share of its alpha; none where there is none.)
        let glowing = dds::decode(&team_from_alpha(&specular, &albedo, SERAPHIM_GLOW).unwrap(), 0).unwrap().rgba[100];
        assert!(glowing[2].abs_diff(SERAPHIM_GLOW as u8) <= 4 && glowing[1].abs_diff(200) <= 4, "{glowing:?}");
        let unmarked = dds::decode(&team_from_alpha(&specular, &plain([90, 90, 90, 0]), SERAPHIM_GLOW).unwrap(), 0).unwrap().rgba[100];
        assert!(unmarked[2].abs_diff(30) <= 4 && unmarked[3] == 0, "{unmarked:?}");
        // A colour map of 1024 (its alpha a 0 and a 255 half) over a specular map of 512.
        let mut big = dds::header(1024, 1024, 1, None);
        for _ in 0..1024 {
            for x in 0..1024u32 {
                big.extend_from_slice(&[90, 90, 90, if x < 512 { 0 } else { 255 }]);
            }
        }
        let both = team_from_alpha(&specular, &texture(&big).unwrap(), 0).unwrap();
        let spec = dds::decode(&both, 0).unwrap();
        assert_eq!((spec.width, spec.height), (512, 512));
        assert_eq!((spec.rgba[100][3], spec.rgba[400][3]), (0, 255), "the left half none, the right all");
    }

    /// A colour map made BC1: half the bytes, every level, every texel the colour it was (blocks
    /// whose ends BC1 would read with a see-through black turned round or made one colour), its
    /// alpha opaque.
    #[test]
    fn a_colour_map_is_made_bc1_with_its_colours() {
        let mut file = dds::header(16, 16, 1, None);
        for i in 0..256u32 {
            let (x, y) = (i % 16, i / 16);
            // (Gradients, a flat corner, and stripes with their ends both ways round.)
            let texel = if x < 4 && y < 4 { [70, 70, 70, 0] } else { [(x * 15) as u8, (y * 15) as u8, ((x + y) * 7) as u8, (i % 7 * 30) as u8] };
            file.extend_from_slice(&[texel[2], texel[1], texel[0], texel[3]]);
        }
        let paint = texture(&file).unwrap();
        // (Some blocks' ends the other way round, as an encoder may leave them: BC3 reads them
        // with four colours all the same.)
        let mut turned = paint.clone();
        let h = dds::Header::parse(&paint).unwrap();
        let (at, len) = h.level_range(0);
        for block in turned[at..at + len].as_chunks_mut::<16>().0.iter_mut().step_by(3) {
            let colour = &mut block[8..];
            let (c0, c1) = ([colour[0], colour[1]], [colour[2], colour[3]]);
            colour[..2].copy_from_slice(&c1);
            colour[2..4].copy_from_slice(&c0);
            let bits = u32::from_le_bytes([colour[4], colour[5], colour[6], colour[7]]) ^ 0x5555_5555;
            colour[4..].copy_from_slice(&bits.to_le_bytes());
        }
        for source in [&paint, &turned] {
            let bc1 = opaque(source);
            let header = dds::Header::parse(&bc1).unwrap();
            assert_eq!((header.format, header.levels, header.levels_held(&bc1)), (dds::Format::Dxt1, h.levels, h.levels));
            assert_eq!(bc1.len() - 128, (source.len() - 128) / 2);
            for level in 0..h.levels {
                let (was, now) = (dds::decode(source, level).unwrap(), dds::decode(&bc1, level).unwrap());
                for (a, b) in was.rgba.iter().zip(&now.rgba) {
                    assert_eq!((a[0], a[1], a[2], b[3]), (b[0], b[1], b[2], 255), "level {level}");
                }
            }
        }
        // (Not a BC3 texture: as it is.)
        assert_eq!(opaque(&file), file);
    }
}
