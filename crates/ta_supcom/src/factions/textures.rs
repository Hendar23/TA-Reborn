//! A unit's paint as TA Reborn draws a model's surface (`ta_formats::surface`): its colour,
//! specular and team colour, and normal maps, each made BC3 of [`SIDE`] by [`SIDE`] texels with
//! every mip level below. Forged Alliance's blocks are kept as they are wherever a texture has a
//! level of that size (most do); the others are decoded, resized and encoded again
//! (`ta_formats::dds::to_bc3_levels`).

use ta_formats::dds;

use crate::Result;

/// The side of the paint's textures.
pub const SIDE: u32 = 512;

/// A texture made the paint's: a DDS file of BC3 levels.
pub fn texture(bytes: &[u8]) -> Result<Vec<u8>> {
    let levels = dds::to_bc3_levels(bytes, SIDE)?;
    let mut out = dds::header(SIDE, SIDE, levels.len() as u32, Some(b"DXT5"));
    for level in levels {
        out.extend_from_slice(&level);
    }
    Ok(out)
}

/// A specular map with the player's colour where an albedo's alpha says (both the paint's):
/// how the Seraphim's are painted (their specular maps' alpha empty), as the others' are.
pub fn team_from_alpha(specular: &[u8], albedo: &[u8]) -> Result<Vec<u8>> {
    let (mut spec, colour) = (dds::decode(specular, 0)?, dds::decode(albedo, 0)?);
    if (spec.width, spec.height) != (colour.width, colour.height) {
        return Ok(specular.to_vec());
    }
    for (s, c) in spec.rgba.iter_mut().zip(&colour.rgba) {
        s[3] = c[3];
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
        assert_eq!((header.width, header.height, header.levels, header.format), (SIDE, SIDE, 10, dds::Format::Dxt5));
        assert!(header.level_data(&paint, 9).is_some());
        assert_eq!(paint.len(), 128 + (0..10).map(|l| ((SIDE >> l).max(4) / 4).pow(2) as usize * 16).sum::<usize>());
        // (Its colour, to a 565's rounding.)
        let [r, g, b] = mean(&paint);
        assert!(r.abs_diff(200) <= 4 && g.abs_diff(100) <= 2 && b.abs_diff(50) <= 4, "{r} {g} {b}");
        // Made again, the same bytes.
        assert_eq!(texture(&paint).unwrap(), paint);
    }

    /// The Seraphim's team colour, from the albedo's alpha into the specular map's.
    #[test]
    fn a_specular_map_takes_its_team_colour_from_the_albedo() {
        let (specular, albedo) = (plain([10, 200, 30, 0]), plain([90, 90, 90, 255]));
        let both = team_from_alpha(&specular, &albedo).unwrap();
        let texel = dds::decode(&both, 0).unwrap().rgba[100];
        assert_eq!(texel[3], 255);
        assert!(texel[1].abs_diff(200) <= 4, "{texel:?}");
        assert_eq!(dds::Header::parse(&both).unwrap().levels, 10);
    }
}
