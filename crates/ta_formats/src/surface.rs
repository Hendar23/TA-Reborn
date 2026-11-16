//! A model's surface (`objects3d/<name>.surface`, beside its 3DO): what TA Reborn paints a 3DO
//! with beyond TA's palette colours and GAF textures. Every vertex of every piece has texture
//! coordinates and a smooth normal, and the primitives name materials in place of TA textures:
//! each a colour map (albedo), a specular and team colour map, and a normal map, DDS textures in
//! the game's files. A primitive naming no material, or one the surface lacks, is drawn as TA
//! draws it. The models TA Reborn makes of Supreme Commander's units have surfaces
//! (crates/ta_supcom); docs/MODDING.md tells mod makers how to give their models one.
//!
//! The textures' channels: the albedo's colour (sRGB; its alpha unused); the specular map's red
//! how much it mirrors its surroundings, green how glossy it is, blue where it glows by itself,
//! alpha where it takes the player's colour; the normal map's tangent-space normal, x in its
//! alpha and y in its green (z made up), u to the right and v down the picture.
//!
//! A surface may name the model drawn in its model's place far off (`far`: another 3DO with a
//! surface, of the same pieces in the same order, lighter): Supreme Commander's second level of
//! detail.
//!
//! Laid out little-endian:
//! - `TASURF`, two zero bytes, the version (1), the number of materials and of pieces (u32s);
//! - the far model's name (a u16 length and that many bytes of UTF-8; none: empty);
//! - each material: its name (as the primitives give it, any case) and its three textures' paths,
//!   each a u16 length and that many bytes of UTF-8; then its colour on average (sRGB, three
//!   bytes: what it is drawn in from afar, or before its textures are read) and a byte unused;
//! - each piece in the 3DO's order (as [`Model3do::parse`](crate::model3do::Model3do::parse)
//!   lists them): its number of vertices (the piece's own), then each vertex's texture
//!   coordinates (two f32: u from the picture's left, v from its top, 1 its far side) and normal
//!   (three i16 of 32,767ths, in the 3DO's own axes, as its vertices are, and one unused).

use crate::{FormatError, Result};

const MAGIC: &[u8; 8] = b"TASURF\0\0";
const VERSION: u32 = 1;
/// The most materials, pieces or vertices a surface may claim.
const MAX_ITEMS: u32 = 1 << 20;

/// A material: the name the primitives give it, its textures, and its colour on average.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Material {
    pub name: String,
    pub albedo: String,
    pub specular: String,
    pub normals: String,
    pub mean: [u8; 3],
}

/// A vertex's texture coordinates and normal (32,767ths).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vertex {
    pub uv: [f32; 2],
    pub normal: [i16; 3],
}

/// A piece's vertices, in the order of the 3DO piece's.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Piece {
    pub vertices: Vec<Vertex>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Surface {
    pub materials: Vec<Material>,
    pub pieces: Vec<Piece>,
    /// The model drawn in this one's place far off (its name in `objects3d/`), or empty.
    pub far: String,
}

impl Surface {
    /// The material named `name` (any case).
    pub fn material(&self, name: &str) -> Option<usize> {
        self.materials.iter().position(|m| m.name.eq_ignore_ascii_case(name))
    }

    /// Whether it fits `model`: a piece for each of its pieces, a vertex for each of theirs.
    pub fn fits(&self, model: &crate::model3do::Model3do) -> bool {
        self.pieces.len() == model.pieces.len() && self.pieces.iter().zip(&model.pieces).all(|(s, p)| s.vertices.len() == p.vertices.len())
    }

    pub fn parse(b: &[u8]) -> Result<Surface> {
        let mut r = Reader { b, at: 0 };
        if r.take(8)? != MAGIC {
            return Err(FormatError::invalid("surface", "not a surface"));
        }
        let version = r.u32()?;
        if version != VERSION {
            return Err(FormatError::invalid("surface", format!("version {version} ({VERSION} is read)")));
        }
        let (material_count, piece_count) = (r.u32()?, r.u32()?);
        if material_count > MAX_ITEMS || piece_count > MAX_ITEMS {
            return Err(FormatError::invalid("surface", format!("{material_count} materials, {piece_count} pieces")));
        }
        let far = r.string()?;
        let mut materials = Vec::with_capacity(material_count.min(1024) as usize);
        for _ in 0..material_count {
            let (name, albedo, specular, normals) = (r.string()?, r.string()?, r.string()?, r.string()?);
            let mean = r.take(4)?;
            materials.push(Material { name, albedo, specular, normals, mean: [mean[0], mean[1], mean[2]] });
        }
        let mut pieces = Vec::with_capacity(piece_count.min(1024) as usize);
        for _ in 0..piece_count {
            let count = r.u32()?;
            if count > MAX_ITEMS || (count as usize).saturating_mul(16) > b.len().saturating_sub(r.at) {
                return Err(FormatError::invalid("surface", format!("a piece of {count} vertices")));
            }
            let vertices = (0..count)
                .map(|_| {
                    let v = r.take(16)?;
                    let f = |k: usize| f32::from_le_bytes([v[k], v[k + 1], v[k + 2], v[k + 3]]);
                    let i = |k: usize| i16::from_le_bytes([v[k], v[k + 1]]);
                    Ok(Vertex { uv: [f(0), f(4)], normal: [i(8), i(10), i(12)] })
                })
                .collect::<Result<Vec<Vertex>>>()?;
            pieces.push(Piece { vertices });
        }
        Ok(Surface { materials, pieces, far })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        for v in [VERSION, self.materials.len() as u32, self.pieces.len() as u32] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        let put = |out: &mut Vec<u8>, s: &str| {
            let bytes = s.as_bytes();
            let len = bytes.len().min(u16::MAX as usize);
            out.extend_from_slice(&(len as u16).to_le_bytes());
            out.extend_from_slice(&bytes[..len]);
        };
        put(&mut out, &self.far);
        for m in &self.materials {
            for s in [&m.name, &m.albedo, &m.specular, &m.normals] {
                put(&mut out, s);
            }
            out.extend_from_slice(&[m.mean[0], m.mean[1], m.mean[2], 0]);
        }
        for p in &self.pieces {
            out.extend_from_slice(&(p.vertices.len() as u32).to_le_bytes());
            for v in &p.vertices {
                out.extend_from_slice(&v.uv[0].to_le_bytes());
                out.extend_from_slice(&v.uv[1].to_le_bytes());
                for n in v.normal {
                    out.extend_from_slice(&n.to_le_bytes());
                }
                out.extend_from_slice(&[0, 0]);
            }
        }
        out
    }
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.at.checked_add(n).filter(|&e| e <= self.b.len()).ok_or(FormatError::Eof { offset: self.at, needed: n })?;
        let s = &self.b[self.at..end];
        self.at = end;
        Ok(s)
    }

    fn u32(&mut self) -> Result<u32> {
        let s = self.take(4)?;
        Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }

    fn string(&mut self) -> Result<String> {
        let len = self.take(2)?;
        let len = u16::from_le_bytes([len[0], len[1]]) as usize;
        String::from_utf8(self.take(len)?.to_vec()).map_err(|_| FormatError::invalid("surface", "a name not in UTF-8"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Surface {
        Surface {
            materials: vec![Material { name: "uel0201".into(), albedo: "textures/supcom/uel0201_albedo.dds".into(), specular: "textures/supcom/uel0201_specteam.dds".into(), normals: "textures/supcom/uel0201_normals.dds".into(), mean: [90, 100, 110] }],
            pieces: vec![
                Piece { vertices: vec![Vertex { uv: [0.0, 0.5], normal: [0, 32767, 0] }, Vertex { uv: [1.0, 0.25], normal: [-32767, 0, 0] }] },
                Piece::default(),
            ],
            far: "uel0201_lod1".into(),
        }
    }

    #[test]
    fn a_surface_reads_back_as_written() {
        let s = sample();
        let back = Surface::parse(&s.to_bytes()).unwrap();
        assert_eq!(back, s);
        assert_eq!(back.material("UEL0201"), Some(0));
        assert_eq!(back.material("other"), None);
    }

    #[test]
    fn a_broken_surface_is_refused() {
        let good = sample().to_bytes();
        assert!(Surface::parse(&good[..good.len() - 3]).is_err(), "cut short");
        assert!(Surface::parse(b"TASURF\0\0\x02\0\0\0").is_err(), "another version");
        assert!(Surface::parse(b"MODL").is_err());
        let mut huge = MAGIC.to_vec();
        for v in [VERSION, 0, 1] {
            huge.extend_from_slice(&v.to_le_bytes());
        }
        huge.extend_from_slice(&[0, 0]);
        huge.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(Surface::parse(&huge).is_err(), "a piece claiming more vertices than there are");
    }

    #[test]
    fn a_surface_fits_its_model_only() {
        use crate::model3do::{Model3do, Piece as ModelPiece};
        let piece = |n: usize, parent| ModelPiece { name: "p".into(), parent, offset: [0; 3], vertices: vec![[0; 3]; n], primitives: Vec::new(), selection_primitive: None, children: Vec::new() };
        let model = Model3do { pieces: vec![piece(2, None), piece(0, Some(0))] };
        assert!(sample().fits(&model));
        let other = Model3do { pieces: vec![piece(3, None), piece(0, Some(0))] };
        assert!(!sample().fits(&other));
    }
}
