//! 3DO model reader.
//!
//! A 3DO is a tree of pieces ("objects"). Each piece has an offset from its parent, its own
//! vertex list and polygons ("primitives"). Coordinates are 16.16 fixed point in TA world
//! units. Polygons reference a texture by name (an entry in `textures/*.gaf`) or, when
//! untextured, a palette colour.
//!
//! Piece header (13 × i32): version(1), vertex count, primitive count, selection primitive
//! (-1 = none), x/y/z offset from parent, name ptr, 0, vertex ptr, primitive ptr,
//! sibling ptr, child ptr. Primitive (8 × i32): colour index, index count, 0, index ptr
//! (i16 array), texture name ptr (0 = none), 3 × unknown.

use crate::bin::{Budget, cstr_at, i16_at, i32_at, table};
use crate::{FormatError, Result};

const MAX_PIECES: usize = 1024;

#[derive(Clone, Debug)]
pub struct Primitive {
    /// Palette index used when the primitive has no texture.
    pub color_index: u32,
    /// Indices into the owning piece's vertex list.
    pub indices: Vec<u16>,
    pub texture: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Piece {
    pub name: String,
    pub parent: Option<usize>,
    /// Offset from the parent piece, 16.16 fixed point.
    pub offset: [i32; 3],
    /// Vertex positions relative to this piece's origin, 16.16 fixed point.
    pub vertices: Vec<[i32; 3]>,
    pub primitives: Vec<Primitive>,
    /// Index of the primitive that is the unit's ground selection plate, if any.
    pub selection_primitive: Option<usize>,
    pub children: Vec<usize>,
}

/// A parsed 3DO model; `pieces[0]` is the root, pieces are in depth-first order.
#[derive(Clone, Debug, Default)]
pub struct Model3do {
    pub pieces: Vec<Piece>,
}

pub const FIXED_ONE: f32 = 65536.0;

impl Model3do {
    pub fn parse(data: &[u8]) -> Result<Model3do> {
        let mut model = Model3do::default();
        parse_siblings(data, 0, None, &mut model, 0, &mut std::collections::HashSet::new(), &mut Budget(128 << 20))?;
        if model.pieces.is_empty() {
            return Err(FormatError::invalid("3DO", "no pieces"));
        }
        Ok(model)
    }

    /// Case-insensitive piece lookup.
    pub fn piece_index(&self, name: &str) -> Option<usize> {
        self.pieces.iter().position(|p| p.name.eq_ignore_ascii_case(name))
    }

    /// The model as a 3DO file, as [`Model3do::parse`] reads one: the first root piece at the
    /// start, each piece's header followed by its name, vertices, primitives (and their indices
    /// and texture names), then its children; siblings linked in order. The models TA Reborn makes
    /// of other games' units (crates/ta_supcom) are written so.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let roots: Vec<usize> = self.pieces.iter().enumerate().filter(|(_, p)| p.parent.is_none()).map(|(i, _)| i).collect();
        if !roots.is_empty() {
            write_siblings(self, &roots, &mut out);
        }
        out
    }

    /// Distinct texture names referenced by the model.
    pub fn texture_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .pieces
            .iter()
            .flat_map(|p| p.primitives.iter().filter_map(|pr| pr.texture.clone()))
            .collect();
        names.sort_by_key(|n| n.to_ascii_lowercase());
        names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        names
    }
}

/// Writes the pieces `list`, siblings in order, each linked to the next; where the first is.
fn write_siblings(model: &Model3do, list: &[usize], out: &mut Vec<u8>) -> usize {
    let mut previous: Option<usize> = None;
    let mut first = out.len();
    for (k, &i) in list.iter().enumerate() {
        let at = write_piece(model, i, out);
        if k == 0 {
            first = at;
        }
        if let Some(prev) = previous {
            put_i32(out, prev + 44, at as i32);
        }
        previous = Some(at);
    }
    first
}

/// Writes piece `i` and, after it, its children; where its header is.
fn write_piece(model: &Model3do, i: usize, out: &mut Vec<u8>) -> usize {
    let p = &model.pieces[i];
    let at = out.len();
    out.resize(at + 52, 0);
    put_i32(out, at, 1);
    put_i32(out, at + 4, p.vertices.len() as i32);
    put_i32(out, at + 8, p.primitives.len() as i32);
    put_i32(out, at + 12, p.selection_primitive.map_or(-1, |s| s as i32));
    for (k, v) in p.offset.iter().enumerate() {
        put_i32(out, at + 16 + k * 4, *v);
    }
    let name_at = out.len();
    out.extend(p.name.chars().map(|c| if (c as u32) < 256 { c as u8 } else { b'?' }));
    out.push(0);
    put_i32(out, at + 28, name_at as i32);
    let vertex_at = out.len();
    for v in &p.vertices {
        for c in v {
            out.extend_from_slice(&c.to_le_bytes());
        }
    }
    put_i32(out, at + 36, vertex_at as i32);
    let primitive_at = out.len();
    out.resize(primitive_at + p.primitives.len() * 32, 0);
    put_i32(out, at + 40, primitive_at as i32);
    for (k, prim) in p.primitives.iter().enumerate() {
        let o = primitive_at + k * 32;
        put_i32(out, o, prim.color_index as i32);
        put_i32(out, o + 4, prim.indices.len() as i32);
        let index_at = out.len();
        for &idx in &prim.indices {
            out.extend_from_slice(&(idx as i16).to_le_bytes());
        }
        put_i32(out, o + 12, index_at as i32);
        if let Some(texture) = prim.texture.as_ref().filter(|t| !t.is_empty()) {
            let texture_at = out.len();
            out.extend(texture.chars().map(|c| if (c as u32) < 256 { c as u8 } else { b'?' }));
            out.push(0);
            put_i32(out, o + 16, texture_at as i32);
        }
    }
    if !p.children.is_empty() {
        let child = write_siblings(model, &p.children, out);
        put_i32(out, at + 48, child as i32);
    }
    at
}

fn put_i32(out: &mut [u8], at: usize, v: i32) {
    out[at..at + 4].copy_from_slice(&v.to_le_bytes());
}

fn parse_siblings(data: &[u8], mut offset: usize, parent: Option<usize>, model: &mut Model3do, depth: usize, seen: &mut std::collections::HashSet<usize>, budget: &mut Budget) -> Result<()> {
    if depth > 64 {
        return Err(FormatError::invalid("3DO", "piece nesting exceeds 64 levels"));
    }
    loop {
        if model.pieces.len() >= MAX_PIECES || !seen.insert(offset) {
            return Err(FormatError::invalid("3DO", "too many pieces (cyclic links?)"));
        }
        let version = i32_at(data, offset)?;
        if version != 1 {
            return Err(FormatError::invalid("3DO", format!("piece at {offset} has version {version}")));
        }
        let vertex_count = i32_at(data, offset + 4)?.max(0) as usize;
        let primitive_count = i32_at(data, offset + 8)?.max(0) as usize;
        let selection = i32_at(data, offset + 12)?;
        let off = [i32_at(data, offset + 16)?, i32_at(data, offset + 20)?, i32_at(data, offset + 24)?];
        let name = cstr_at(data, i32_at(data, offset + 28)? as usize)?;
        let vertex_ptr = i32_at(data, offset + 36)? as usize;
        let primitive_ptr = i32_at(data, offset + 40)? as usize;
        let sibling_ptr = i32_at(data, offset + 44)?;
        let child_ptr = i32_at(data, offset + 48)?;

        table(data, vertex_ptr, vertex_count, 12)?;
        table(data, primitive_ptr, primitive_count, 32)?;
        budget.take(vertex_count * std::mem::size_of::<[i32; 3]>() + primitive_count * std::mem::size_of::<Primitive>(), "3DO")?;

        let mut vertices = Vec::with_capacity(vertex_count);
        for v in 0..vertex_count {
            let o = vertex_ptr + v * 12;
            vertices.push([i32_at(data, o)?, i32_at(data, o + 4)?, i32_at(data, o + 8)?]);
        }

        let mut primitives = Vec::with_capacity(primitive_count);
        for p in 0..primitive_count {
            let o = primitive_ptr + p * 32;
            let color_index = i32_at(data, o)? as u32;
            let index_count = i32_at(data, o + 4)?.max(0) as usize;
            let index_ptr = i32_at(data, o + 12)? as usize;
            let texture_ptr = i32_at(data, o + 16)?;
            table(data, index_ptr, index_count, 2)?;
            budget.take(index_count * 2, "3DO")?;
            let mut indices = Vec::with_capacity(index_count);
            for i in 0..index_count {
                let idx = i16_at(data, index_ptr + i * 2)?;
                if idx < 0 || idx as usize >= vertex_count {
                    return Err(FormatError::invalid("3DO", format!("vertex index {idx} out of range in '{name}'")));
                }
                indices.push(idx as u16);
            }
            let texture = if texture_ptr > 0 {
                Some(cstr_at(data, texture_ptr as usize)?).filter(|t| !t.is_empty())
            } else {
                None
            };
            budget.take(texture.as_ref().map_or(0, String::len), "3DO")?;
            primitives.push(Primitive { color_index, indices, texture });
        }

        let index = model.pieces.len();
        model.pieces.push(Piece {
            name,
            parent,
            offset: off,
            vertices,
            primitives,
            selection_primitive: usize::try_from(selection).ok().filter(|&s| s < primitive_count),
            children: Vec::new(),
        });
        if let Some(p) = parent {
            model.pieces[p].children.push(index);
        }
        if child_ptr > 0 {
            parse_siblings(data, child_ptr as usize, Some(index), model, depth + 1, seen, budget)?;
        }
        if sibling_ptr <= 0 {
            return Ok(());
        }
        offset = sibling_ptr as usize;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn piece(name: &str, parent: Option<usize>, offset: [i32; 3], vertices: Vec<[i32; 3]>, primitives: Vec<Primitive>, children: Vec<usize>) -> Piece {
        Piece { name: name.into(), parent, offset, vertices, primitives, selection_primitive: None, children }
    }

    /// A hull with a turret and an exhaust on it, a barrel on the turret: written and read back,
    /// the same tree, places, polygons and textures.
    #[test]
    fn a_written_model_reads_back_the_same() {
        let tri = |a, b, c, texture: Option<&str>| Primitive { color_index: 7, indices: vec![a, b, c], texture: texture.map(str::to_string) };
        let model = Model3do {
            pieces: vec![
                piece("base", None, [0, 0, 0], vec![[0, 0, 0], [65536, 0, 0], [0, 0, 65536]], vec![tri(0, 1, 2, Some("armsteel"))], vec![1, 3]),
                piece("turret", Some(0), [0, 4 << 16, -(3 << 16)], vec![[1, 2, 3], [4, 5, 6], [7, 8, 9]], vec![tri(2, 1, 0, None)], vec![2]),
                piece("barrel", Some(1), [0, -(1 << 15), 3 << 16], vec![[10, 11, 12]], vec![], vec![]),
                piece("exhaust", Some(0), [2 << 16, 0, -(7 << 15)], vec![], vec![], vec![]),
            ],
        };
        let back = Model3do::parse(&model.to_bytes()).unwrap();
        assert_eq!(back.pieces.len(), 4);
        for (a, b) in model.pieces.iter().zip(&back.pieces) {
            assert_eq!((a.name.as_str(), a.parent, a.offset, &a.vertices, &a.children), (b.name.as_str(), b.parent, b.offset, &b.vertices, &b.children));
            assert_eq!(a.primitives.len(), b.primitives.len());
            for (pa, pb) in a.primitives.iter().zip(&b.primitives) {
                assert_eq!((pa.color_index, &pa.indices, &pa.texture), (pb.color_index, &pb.indices, &pb.texture));
            }
        }
        assert_eq!(back.texture_names(), ["armsteel"]);
    }
}
