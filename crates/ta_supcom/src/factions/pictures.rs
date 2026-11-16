//! A unit's build picture (`unitpics/<id>.pcx`, the build menus' buttons): its icon (Forged
//! Alliance's `textures/ui/common/icons/units/<id>_icon.dds`, 64 by 64 with its alpha) laid over
//! the frame of its kind (`<kind>_up.dds` beside it: `land`, `amph`, `air`, `sea`, by its
//! blueprint's `General.Icon`), as a 24-bit PCX of TA's 64 by 64.

use ta_formats::{dds, pcx};

use crate::Result;
use crate::unit::UnitBlueprint;

/// TA's build pictures' size.
pub const SIDE: u32 = 64;

/// Where a unit's icon is in the game's files.
pub fn icon_path(unit: &UnitBlueprint) -> String {
    format!("textures/ui/common/icons/units/{}_icon.dds", unit.id.to_ascii_lowercase())
}

/// Where the frame of a unit's kind is in the game's files (none where its blueprint names no
/// kind).
pub fn frame_path(unit: &UnitBlueprint) -> Option<String> {
    let kind = unit.table.path(&["General"]).and_then(|g| g.get_ci("Icon")).and_then(|v| v.str())?.trim().to_ascii_lowercase();
    (!kind.is_empty() && kind.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')).then(|| format!("textures/ui/common/icons/units/{kind}_up.dds"))
}

/// The picture: `icon` (a DDS) over `frame` (a DDS; without one, a dark grey), at 64 by 64.
pub fn picture(icon: &[u8], frame: Option<&[u8]>) -> Result<Vec<u8>> {
    let fit = |image: dds::Image| if (image.width, image.height) == (SIDE, SIDE) { image } else { dds::resize(&image, SIDE, SIDE) };
    let icon = fit(dds::decode(icon, 0)?);
    let frame = frame.and_then(|f| dds::decode(f, 0).ok()).map(fit);
    let mut rgb = Vec::with_capacity((SIDE * SIDE * 3) as usize);
    for (i, top) in icon.rgba.iter().enumerate() {
        let under = frame.as_ref().map_or([40, 42, 44, 255], |f| f.rgba[i]);
        let a = top[3] as u32;
        for c in 0..3 {
            rgb.push(((top[c] as u32 * a + under[c] as u32 * (255 - a) + 127) / 255) as u8);
        }
    }
    Ok(pcx::write_rgb(SIDE, SIDE, &rgb))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A plain DDS of one colour, `side` across.
    fn plain(side: u32, rgba: [u8; 4]) -> Vec<u8> {
        let mut file = dds::header(side, side, 1, None);
        for _ in 0..side * side {
            file.extend_from_slice(&[rgba[2], rgba[1], rgba[0], rgba[3]]);
        }
        file
    }

    #[test]
    fn an_icon_is_laid_over_its_frame_at_tas_size() {
        // Half see-through red over blue, the icon smaller than TA's: grown to 64.
        let made = picture(&plain(32, [255, 0, 0, 128]), Some(&plain(64, [0, 0, 255, 255]))).unwrap();
        let back = pcx::PcxImage::parse(&made).unwrap();
        assert_eq!((back.width, back.height), (SIDE, SIDE));
        let rgba = back.to_rgba();
        assert_eq!(&rgba[..4], &[128, 0, 127, 255]);
        // Without a frame, over dark grey.
        let alone = pcx::PcxImage::parse(&picture(&plain(64, [0, 255, 0, 0]), None).unwrap()).unwrap().to_rgba();
        assert_eq!(&alone[..3], &[40, 42, 44]);
    }
}
