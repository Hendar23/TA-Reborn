//! TrueType and OpenType font files as TA Reborn's interface reads them: whether one reads (egui
//! panics on font data it cannot parse, so a font is only ever handed to it once it does) and a
//! face's vertical metrics. Shared by the interface (`ta_ui::theme`) and the mod check
//! (`presentation`), so that a theme's font the check passes is one the game draws.

/// A face's vertical metrics, in ems.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VMetrics {
    pub ascent: f32,
    /// (Below the baseline: positive.)
    pub descent: f32,
    pub gap: f32,
    /// The height of its capitals.
    pub cap: f32,
}

impl VMetrics {
    /// The height of a line of it.
    pub fn row(&self) -> f32 {
        self.ascent + self.descent + self.gap
    }

    /// How far down to move its glyphs (in ems) so its capitals sit in the middle of their
    /// line: egui puts the baseline an ascent below the line's top and the whole line gap
    /// under the text, which leaves capitals high (Bahnschrift's by 0.16 em), while
    /// everything centres text by its line (buttons, fields, rows beside lamps and arrows).
    pub fn cap_centring(&self) -> f32 {
        self.row() / 2.0 - (self.ascent - self.cap / 2.0)
    }
}

/// Whether a font file reads as egui reads it: a first face with a table directory that `skrifa`
/// opens, with the vertical metrics the interface sets it by (and so units per em).
pub fn reads(bytes: &[u8]) -> bool {
    has_face(bytes, 0) && skrifa::FontRef::from_index(bytes, 0).is_ok() && vmetrics(bytes, 0).is_some()
}

/// A big-endian `u16` at `at`, where the file has one there.
fn be16(bytes: &[u8], at: usize) -> Option<u16> {
    bytes.get(at..at.checked_add(2)?).map(|b| u16::from_be_bytes([b[0], b[1]]))
}

/// A big-endian `u32` at `at`, where the file has one there.
fn be32(bytes: &[u8], at: usize) -> Option<u32> {
    bytes.get(at..at.checked_add(4)?).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

/// Where face `index`'s table directory starts: a collection's (`ttcf`) offset for it, else 0
/// for the one face of a plain font file.
fn face_offset(bytes: &[u8], index: u32) -> Option<usize> {
    if bytes.starts_with(b"ttcf") {
        let count = be32(bytes, 8)?;
        if index >= count { return None; }
        be32(bytes, 12usize.checked_add(4usize.checked_mul(index as usize)?)?).map(|o| o as usize)
    } else {
        (index == 0).then_some(0)
    }
}

/// Whether the font file has a face `index` with a table directory (egui panics on font
/// data it cannot parse).
pub fn has_face(bytes: &[u8], index: u32) -> bool {
    let Some(offset) = face_offset(bytes, index) else { return false };
    // TrueType, CFF ("OTTO") or Apple's "true", and the directory's records in the file.
    matches!(be32(bytes, offset), Some(0x0001_0000 | 0x4F54_544F | 0x7472_7565))
        && be16(bytes, offset.saturating_add(4)).is_some_and(|tables| {
            offset.checked_add(12 + 16 * tables as usize).is_some_and(|end| bytes.len() >= end)
        })
}

/// A face's vertical metrics as egui reads them: OS/2's typographic ones when the font asks
/// for them, else hhea's (`skrifa`'s rule); its capitals' height from OS/2 (else an
/// estimate). None for a face without them (or of no units per em).
pub fn vmetrics(bytes: &[u8], index: u32) -> Option<VMetrics> {
    let be16 = |at: usize| be16(bytes, at);
    let be32 = |at: usize| be32(bytes, at);
    let offset = face_offset(bytes, index)?;
    let tables = be16(offset.checked_add(4)?)? as usize;
    let table = |tag: &[u8; 4]| {
        (0..tables).find_map(|i| {
            let record = offset.checked_add(12 + 16 * i)?;
            if bytes.get(record..record.checked_add(4)?)? == tag { be32(record + 8).map(|o| o as usize) } else { None }
        })
    };
    let units = be16(table(b"head")?.checked_add(18)?)? as f32;
    let hhea = table(b"hhea")?;
    let hhea = (be16(hhea.checked_add(4)?)? as i16, be16(hhea.checked_add(6)?)? as i16, be16(hhea.checked_add(8)?)? as i16);
    let os2 = table(b"OS/2");
    let typo = os2.and_then(|os2| Some((be16(os2.checked_add(62)?)?, (be16(os2.checked_add(68)?)? as i16, be16(os2.checked_add(70)?)? as i16, be16(os2.checked_add(72)?)? as i16))));
    let (ascent, descent, gap) = match typo {
        Some((selection, typo)) if selection & 0x80 != 0 => typo,
        _ if hhea.0 != 0 || hhea.1 != 0 => hhea,
        Some((_, typo)) => typo,
        None => return None,
    };
    // (sCapHeight is in OS/2 from its version 2.)
    let cap = os2.filter(|&os2| be16(os2).is_some_and(|version| version >= 2)).and_then(|os2| be16(os2.checked_add(88)?)).map(|c| c as i16).filter(|&c| c > 0);
    let em = |v: i16| v as f32 / units;
    (units > 0.0).then(|| VMetrics { ascent: em(ascent), descent: em(descent).abs(), gap: em(gap).max(0.0), cap: cap.map_or(em(ascent) * 0.7, em) })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A font file of one face whose table directory names `tables` (tag, offset), each table's
    /// bytes laid at its offset.
    fn font(tables: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
        let mut bytes = vec![0, 1, 0, 0];
        bytes.extend((tables.len() as u16).to_be_bytes());
        bytes.extend([0u8; 6]);
        let mut at = 12 + 16 * tables.len();
        let mut data: Vec<u8> = Vec::new();
        for (tag, table) in tables {
            bytes.extend(*tag);
            bytes.extend([0u8; 4]);
            bytes.extend((at as u32).to_be_bytes());
            bytes.extend((table.len() as u32).to_be_bytes());
            at += table.len();
            data.extend(table);
        }
        bytes.extend(data);
        bytes
    }

    fn head(units: u16) -> Vec<u8> {
        let mut head = vec![0u8; 54];
        head[18..20].copy_from_slice(&units.to_be_bytes());
        head
    }

    fn hhea(ascent: i16, descent: i16, gap: i16) -> Vec<u8> {
        let mut hhea = vec![0u8; 36];
        hhea[4..6].copy_from_slice(&ascent.to_be_bytes());
        hhea[6..8].copy_from_slice(&descent.to_be_bytes());
        hhea[8..10].copy_from_slice(&gap.to_be_bytes());
        hhea
    }

    #[test]
    fn a_faces_metrics_are_read_from_its_tables() {
        let bytes = font(&[(b"head", head(1000)), (b"hhea", hhea(800, -200, 100))]);
        assert!(has_face(&bytes, 0));
        let metrics = vmetrics(&bytes, 0).unwrap();
        assert_eq!((metrics.ascent, metrics.descent, metrics.gap), (0.8, 0.2, 0.1));
        assert!((metrics.row() - 1.1).abs() < 1e-6);
        // No units per em: no metrics, and so it does not read.
        let bytes = font(&[(b"head", head(0)), (b"hhea", hhea(800, -200, 100))]);
        assert!(vmetrics(&bytes, 0).is_none());
        assert!(!reads(&bytes));
    }

    /// Cut-off, garbage and lying files are refused, never a panic (a theme's font is any file a
    /// mod names): every prefix of a font and of a collection, and offsets past the file's end.
    #[test]
    fn broken_font_files_do_not_read_and_never_panic() {
        let whole = font(&[(b"head", head(1000)), (b"hhea", hhea(800, -200, 100))]);
        let mut collection = b"ttcf".to_vec();
        collection.extend([0, 1, 0, 0]);
        collection.extend(1u32.to_be_bytes());
        collection.extend(16u32.to_be_bytes());
        collection.extend(&whole);
        assert!(reads(&whole));
        // (Its directory: the header and two records.)
        let directory = 12 + 16 * 2;
        for (file, start) in [(&whole, 0), (&collection, 16)] {
            for end in 0..file.len() {
                let cut = &file[..end];
                let _ = (has_face(cut, 0), vmetrics(cut, 0), has_face(cut, 3), vmetrics(cut, 3), reads(cut));
                if end < start + directory { assert!(!has_face(cut, 0) && !reads(cut), "a font cut at {end} of {} bytes", file.len()); }
            }
        }
        // A collection's header alone, or one whose face lies past its end.
        assert!(!has_face(b"ttcf\0\x01\0\0\0\0\0\x01", 0));
        let mut far = b"ttcf\0\x01\0\0\0\0\0\x01".to_vec();
        far.extend(u32::MAX.to_be_bytes());
        assert!(!has_face(&far, 0) && vmetrics(&far, 0).is_none());
        // A directory of more tables than the file holds, and plain garbage.
        let mut lying = whole.clone();
        lying[4..6].copy_from_slice(&u16::MAX.to_be_bytes());
        assert!(!has_face(&lying, 0));
        assert!(!reads(&[0xffu8; 64]) && !reads(b"") && !reads(b"OTTO"));
    }
}
