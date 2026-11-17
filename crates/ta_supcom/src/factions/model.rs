//! A unit's mesh as a TA model: its skeleton its piece tree, each bone a piece at the bone's place
//! at rest, holding the triangles of the vertices bound to it.
//!
//! TA's pieces turn about their own axes from no turn at rest; Supreme Commander's bones may rest
//! turned. A piece is therefore laid along the model's axes at its bone's place (its offset its
//! bone's place less its parent's), its vertices where they are at rest less that place: the
//! model at rest is the mesh's exactly, and a piece a script turns turns about the model's axes
//! there (a turret about the up axis, a barrel about the side one), as TA's do.
//!
//! The mesh's axes are the engine's (right-handed, y up, the unit facing +z, its right at -x), and
//! a 3DO holds z the other way round (docs/FORMATS.md): z is negated, and with the mirror each
//! triangle's corners are taken the other way round (TA's faces run clockwise seen from outside
//! once z is flipped back, the mesh's anticlockwise).
//!
//! A surface holds its texture coordinates from 0 to 1 (TA Reborn keeps them in 16 bits, and a
//! texture's edge clamps): a triangle the mesh paints from a repeat of its texture (u or v past 1,
//! or under 0) is moved by whole textures to lie within it, the same texels as the repeat gave it
//! ([`fit_uvs`]), its corners copied where other triangles share them. One that straddles a
//! texture's edge cannot be moved so, and is left as it is.

use std::collections::BTreeMap;

use ta_formats::model3do::{Model3do, Piece, Primitive};
use ta_formats::surface::{self, Material, Surface};

use super::UNIT_SCALE;
use crate::scm::Mesh;

/// The palette colour the polygons are given (a light grey), where their surface is not drawn:
/// the model's paint is its material's.
const FILL: u32 = 248;

/// A mesh made into a model: the model, its surface (texture coordinates and normals, every
/// polygon painted with `material`), and for each piece the bone it is (the pieces in the
/// order a 3DO lists them: each piece's children after it, depth first).
#[derive(Clone, Debug)]
pub struct Converted {
    pub model: Model3do,
    pub surface: Surface,
    pub bones: Vec<usize>,
    /// Triangles whose corners were bound to more than one bone: given to their first corner's.
    pub split_triangles: usize,
    /// Triangles painted from a repeat of the texture moved into it, and those left straddling a
    /// texture's edge ([`fit_uvs`]).
    pub uvs_moved: usize,
    pub uvs_straddling: usize,
}

/// The most vertices a piece may hold: a 3DO's polygons name them in 16 bits.
const MAX_PIECE_VERTICES: usize = u16::MAX as usize + 1;

/// What of a model counts as standing on the ground ([`ground_extent`]): from this far under its
/// foot (ogrids; deeper is a foundation, meant to stay in the ground)...
pub const GROUND_UNDER: f64 = 0.25;
/// ...to this far over it (what the ground beside a building on a slope would run into).
pub const GROUND_OVER: f64 = 0.5;

/// How wide (ogrids, across and along, about its middle) what of `mesh` stands on the ground is,
/// at `uniform_scale`: what lies from [`GROUND_UNDER`] under its foot (the mesh's y 0, the ground
/// Forged Alliance stands it on) to [`GROUND_OVER`] over it. None when nothing does.
pub fn ground_extent(mesh: &Mesh, uniform_scale: f64) -> Option<[f64; 2]> {
    let mut reach: Option<[f64; 2]> = None;
    for v in &mesh.vertices {
        let y = v.position[1] as f64 * uniform_scale;
        if (-GROUND_UNDER..=GROUND_OVER).contains(&y) {
            let r = reach.get_or_insert([0.0; 2]);
            r[0] = r[0].max((v.position[0] as f64 * uniform_scale).abs());
            r[1] = r[1].max((v.position[2] as f64 * uniform_scale).abs());
        }
    }
    reach.map(|[x, z]| [2.0 * x, 2.0 * z])
}

/// Each triangle whose texture coordinates all lie within one repeat of the texture moved by whole
/// textures into 0..1 (its corners copied, once each, where another triangle shares them unmoved):
/// how many were moved, and how many straddle a texture's edge (left as they are). `Err` when a
/// piece would hold more vertices than a 3DO can name.
pub fn fit_uvs(pieces: &mut [Piece], faces: &mut [surface::Piece]) -> std::result::Result<(usize, usize), String> {
    let (mut moved, mut straddling) = (0, 0);
    for (piece, face) in pieces.iter_mut().zip(faces.iter_mut()) {
        let mut copies: BTreeMap<(u16, i64, i64), u16> = BTreeMap::new();
        for k in 0..piece.primitives.len() {
            let corners = piece.primitives[k].indices.clone();
            let uv = |i: u16| face.vertices[i as usize].uv;
            let (mut lo, mut hi) = ([f32::INFINITY; 2], [f32::NEG_INFINITY; 2]);
            for &i in &corners {
                for a in 0..2 {
                    lo[a] = lo[a].min(uv(i)[a]);
                    hi[a] = hi[a].max(uv(i)[a]);
                }
            }
            if lo[0] >= 0.0 && lo[1] >= 0.0 && hi[0] <= 1.0 && hi[1] <= 1.0 {
                continue;
            }
            let shift = [lo[0].floor(), lo[1].floor()];
            if !(hi[0] - shift[0] <= 1.0 && hi[1] - shift[1] <= 1.0 && shift.iter().all(|s| s.is_finite())) {
                straddling += 1;
                continue;
            }
            moved += 1;
            let key = (shift[0] as i64, shift[1] as i64);
            let mut indices = Vec::with_capacity(corners.len());
            for &i in &corners {
                let copy = match copies.get(&(i, key.0, key.1)) {
                    Some(&c) => c,
                    None => {
                        if piece.vertices.len() >= MAX_PIECE_VERTICES {
                            return Err(format!("piece {} holds more than {MAX_PIECE_VERTICES} vertices", piece.name));
                        }
                        let mut v = face.vertices[i as usize];
                        v.uv = [v.uv[0] - shift[0], v.uv[1] - shift[1]];
                        piece.vertices.push(piece.vertices[i as usize]);
                        face.vertices.push(v);
                        let c = (piece.vertices.len() - 1) as u16;
                        copies.insert((i, key.0, key.1), c);
                        c
                    }
                };
                indices.push(copy);
            }
            piece.primitives[k].indices = indices;
        }
    }
    Ok((moved, straddling))
}

/// `mesh` made into a model, its units times `uniform_scale` ogrids (the blueprint's
/// `Display.UniformScale`), painted with `material`. `Err` when a piece would hold more vertices
/// than a 3DO can name.
pub fn convert(mesh: &Mesh, uniform_scale: f64, material: Material) -> std::result::Result<Converted, String> {
    let scale = uniform_scale * UNIT_SCALE * 65536.0;
    let fixed = |v: f32| -> i32 { (v as f64 * scale).round() as i32 };
    // A place in the mesh as the 3DO holds it: z the other way round.
    let place = |p: [f32; 3]| -> [i32; 3] { [fixed(p[0]), fixed(p[1]), fixed(-p[2])] };
    let unit = |n: f32| -> i16 { (n.clamp(-1.0, 1.0) * 32767.0).round() as i16 };
    let rest = mesh.rest_poses();
    // The bones depth first, each one's children in the mesh's order.
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); mesh.bones.len()];
    for (b, bone) in mesh.bones.iter().enumerate() {
        if let Some(p) = bone.parent {
            children[p].push(b);
        }
    }
    let mut bones: Vec<usize> = Vec::with_capacity(mesh.bones.len());
    let mut stack: Vec<usize> = (0..mesh.bones.len()).filter(|&b| mesh.bones[b].parent.is_none()).rev().collect();
    while let Some(b) = stack.pop() {
        bones.push(b);
        stack.extend(children[b].iter().rev());
    }
    let mut piece_of = vec![0usize; mesh.bones.len()];
    for (k, &b) in bones.iter().enumerate() {
        piece_of[b] = k;
    }
    let mut pieces: Vec<Piece> = bones
        .iter()
        .map(|&b| {
            let bone = &mesh.bones[b];
            let at = rest[b].0;
            let from = bone.parent.map_or([0.0; 3], |p| rest[p].0);
            let offset = place([at[0] - from[0], at[1] - from[1], at[2] - from[2]]);
            let children = children[b].iter().map(|&c| piece_of[c]).collect();
            Piece { name: bone.name.clone(), parent: bone.parent.map(|p| piece_of[p]), offset, vertices: Vec::new(), primitives: Vec::new(), selection_primitive: None, children }
        })
        .collect();
    let mut faces: Vec<surface::Piece> = vec![surface::Piece::default(); pieces.len()];
    // Each piece's vertices: the mesh's bound to its bone, in the mesh's order, then those split
    // triangles bring in (each once a piece).
    let add = |piece: usize, v: u32, pieces: &mut Vec<Piece>, faces: &mut Vec<surface::Piece>| -> std::result::Result<u16, String> {
        if pieces[piece].vertices.len() >= MAX_PIECE_VERTICES {
            return Err(format!("piece {} holds more than {MAX_PIECE_VERTICES} vertices", pieces[piece].name));
        }
        let at = rest[bones[piece]].0;
        let vertex = &mesh.vertices[v as usize];
        let p = vertex.position;
        pieces[piece].vertices.push(place([p[0] - at[0], p[1] - at[1], p[2] - at[2]]));
        let n = vertex.normal;
        faces[piece].vertices.push(surface::Vertex { uv: vertex.uv0, normal: [unit(n[0]), unit(n[1]), unit(-n[2])] });
        Ok((pieces[piece].vertices.len() - 1) as u16)
    };
    let mut own: Vec<u16> = Vec::with_capacity(mesh.vertices.len());
    for (v, vertex) in mesh.vertices.iter().enumerate() {
        own.push(add(piece_of[vertex.bone], v as u32, &mut pieces, &mut faces)?);
    }
    let mut copies: BTreeMap<(usize, u32), u16> = BTreeMap::new();
    let mut split_triangles = 0;
    for tri in &mesh.triangles {
        let bone = mesh.vertices[tri[0] as usize].bone;
        let piece = piece_of[bone];
        let mut corners = [0u16; 3];
        let mut split = false;
        // (The other way round: see the module's notes.)
        for (k, v) in [tri[0], tri[2], tri[1]].into_iter().enumerate() {
            corners[k] = if mesh.vertices[v as usize].bone == bone {
                own[v as usize]
            } else {
                split = true;
                match copies.get(&(piece, v)) {
                    Some(&i) => i,
                    None => {
                        let i = add(piece, v, &mut pieces, &mut faces)?;
                        copies.insert((piece, v), i);
                        i
                    }
                }
            };
        }
        split_triangles += split as usize;
        pieces[piece].primitives.push(Primitive { color_index: FILL, indices: corners.to_vec(), texture: Some(material.name.clone()) });
    }
    let (uvs_moved, uvs_straddling) = fit_uvs(&mut pieces, &mut faces)?;
    Ok(Converted { model: Model3do { pieces }, surface: Surface { materials: vec![material], pieces: faces, far: String::new() }, bones, split_triangles, uvs_moved, uvs_straddling })
}

/// A lighter mesh of the same unit (Forged Alliance's second level of detail, whose skeleton
/// keeps fewer bones, its root renamed `lod1_<id>`) laid onto `first`'s pieces: each of its bones
/// the piece of the same name, or of the nearest of its parents with one (the root the root's),
/// its vertices where they are at rest less that piece's place. Same pieces, same order: drawn
/// with the first's piece matrices, far off. `first_mesh` is the mesh `first` was made from.
/// `Err` when a piece would hold more vertices than a 3DO can name.
pub fn convert_onto(mesh: &Mesh, first_mesh: &Mesh, first: &Converted, uniform_scale: f64, material: Material) -> std::result::Result<Converted, String> {
    let scale = uniform_scale * UNIT_SCALE * 65536.0;
    let fixed = |v: f32| -> i32 { (v as f64 * scale).round() as i32 };
    let place = |p: [f32; 3]| -> [i32; 3] { [fixed(p[0]), fixed(p[1]), fixed(-p[2])] };
    let unit = |n: f32| -> i16 { (n.clamp(-1.0, 1.0) * 32767.0).round() as i16 };
    let first_rest = first_mesh.rest_poses();
    // Each of the first's pieces' places at rest, and each of this mesh's bones' piece.
    let at: Vec<[f32; 3]> = first.bones.iter().map(|&b| first_rest[b].0).collect();
    let named = |name: &str| first.model.pieces.iter().position(|p| p.name.eq_ignore_ascii_case(name));
    let mut piece_of: Vec<usize> = Vec::with_capacity(mesh.bones.len());
    for bone in &mesh.bones {
        let found = match bone.parent {
            None => Some(0),
            Some(_) => named(&bone.name),
        };
        let p = found.unwrap_or_else(|| bone.parent.map_or(0, |p| piece_of[p]));
        piece_of.push(p);
    }
    let mut pieces: Vec<Piece> = first.model.pieces.iter().map(|p| Piece { vertices: Vec::new(), primitives: Vec::new(), ..p.clone() }).collect();
    let mut faces: Vec<surface::Piece> = vec![surface::Piece::default(); pieces.len()];
    let mut index: BTreeMap<(usize, u32), u16> = BTreeMap::new();
    for tri in &mesh.triangles {
        let piece = piece_of[mesh.vertices[tri[0] as usize].bone];
        let mut corners = [0u16; 3];
        for (k, v) in [tri[0], tri[2], tri[1]].into_iter().enumerate() {
            corners[k] = match index.get(&(piece, v)) {
                Some(&i) => i,
                None => {
                    if pieces[piece].vertices.len() >= MAX_PIECE_VERTICES {
                        return Err(format!("piece {} holds more than {MAX_PIECE_VERTICES} vertices", pieces[piece].name));
                    }
                    let vertex = &mesh.vertices[v as usize];
                    let (p, o) = (vertex.position, at[piece]);
                    pieces[piece].vertices.push(place([p[0] - o[0], p[1] - o[1], p[2] - o[2]]));
                    let n = vertex.normal;
                    faces[piece].vertices.push(surface::Vertex { uv: vertex.uv0, normal: [unit(n[0]), unit(n[1]), unit(-n[2])] });
                    let i = (pieces[piece].vertices.len() - 1) as u16;
                    index.insert((piece, v), i);
                    i
                }
            };
        }
        pieces[piece].primitives.push(Primitive { color_index: FILL, indices: corners.to_vec(), texture: Some(material.name.clone()) });
    }
    let (uvs_moved, uvs_straddling) = fit_uvs(&mut pieces, &mut faces)?;
    Ok(Converted { model: Model3do { pieces }, surface: Surface { materials: vec![material], pieces: faces, far: String::new() }, bones: first.bones.clone(), split_triangles: 0, uvs_moved, uvs_straddling })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scm;

    /// 16.16 TA pixels of `n` of the test tank's mesh units at a scale of 1/16: 32 pixels an
    /// ogrid, so 2 pixels a unit.
    fn px(n: f64) -> i32 {
        (n * 2.0 * 65536.0) as i32
    }

    fn paint() -> Material {
        Material { name: "tank".into(), albedo: "a.dds".into(), specular: "s.dds".into(), normals: "n.dds".into(), mean: [1, 2, 3] }
    }

    /// What of a mesh stands on the ground: what lies from a quarter of an ogrid under its foot to
    /// half an ogrid over it, as wide each way as it reaches from its middle; its foundations and
    /// what stands higher left out; none when nothing is there.
    #[test]
    fn a_meshs_ground_extent_is_what_stands_on_the_ground() {
        let at = |x: f32, y: f32, z: f32| scm::Vertex { position: [x, y, z], normal: [0.0, 1.0, 0.0], tangent: [1.0, 0.0, 0.0], binormal: [0.0, 0.0, 1.0], uv0: [0.0; 2], uv1: [0.0; 2], bone: 0 };
        // (Ten mesh units an ogrid.)
        let vertices = vec![at(-4.0, 0.0, 3.0), at(6.0, 4.0, -2.0), at(1.0, -2.0, -5.0), at(20.0, 30.0, 0.0), at(0.0, -9.0, 25.0)];
        let mesh = Mesh { bones: Vec::new(), vertices, triangles: Vec::new(), info: String::new() };
        let [x, z] = ground_extent(&mesh, 0.1).unwrap();
        assert!((x - 1.2).abs() < 1e-6 && (z - 1.0).abs() < 1e-6, "{x} {z}");
        let high = Mesh { vertices: vec![at(0.0, 10.0, 0.0)], ..mesh.clone() };
        assert_eq!(ground_extent(&high, 0.1), None);
    }

    /// The test tank (crates/ta_supcom/src/scm.rs): its bones its pieces, at their places, z
    /// the other way round.
    #[test]
    fn a_tanks_bones_become_its_pieces_at_their_places() {
        let mesh = scm::parse(&scm::tests::tank()).unwrap();
        let out = convert(&mesh, 1.0 / 16.0, paint()).unwrap();
        let m = &out.model;
        assert_eq!(m.pieces.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Tank", "Turret", "Turret_Barrel", "Exhaust"]);
        assert_eq!(m.pieces[0].children, [1, 3]);
        assert_eq!(out.bones, [0, 1, 2, 3]);
        assert_eq!(m.pieces[1].offset, [0, px(4.0), px(3.0)], "the turret, from the hull");
        assert_eq!(m.pieces[2].offset, [0, px(-0.5), px(-3.0)], "the barrel, from the turret");
        // A vertex where it is at rest, less its piece's place: the barrel's at (0, 3.5, 5) less
        // the barrel's (0, 3.5, 0).
        assert_eq!(m.pieces[2].vertices, [[0, 0, px(-5.0)]]);
        assert_eq!(m.pieces[1].vertices, [[0, 0, 0]]);
        // The exhaust rests turned half round, but its piece lies along the model's axes, so
        // nothing of it is turned.
        assert_eq!(m.pieces[3].offset, [px(2.0), 0, px(3.5)]);
    }

    /// A triangle across two bones goes to its first corner's, the other corners' vertices
    /// copied there at their places (once each): the model at rest is still the mesh. Its
    /// corners are taken the other way round.
    #[test]
    fn a_triangle_across_bones_goes_whole_to_its_first_corners() {
        let mesh = scm::parse(&scm::tests::tank()).unwrap();
        let out = convert(&mesh, 1.0 / 16.0, paint()).unwrap();
        // The test tank's triangles are 0, 1 (the hull's), 2 (the turret's) and 1, 2, 3 (the
        // barrel's).
        assert_eq!(out.split_triangles, 2);
        let hull = &out.model.pieces[0];
        assert_eq!(hull.vertices.len(), 4);
        assert_eq!(hull.primitives.iter().map(|p| p.indices.clone()).collect::<Vec<_>>(), [vec![0, 2, 1], vec![1, 3, 2]]);
        // The turret's vertex (0, 4, -3) copied into the hull where it is, with its texture
        // coordinates.
        assert_eq!(hull.vertices[2], [0, px(4.0), px(3.0)]);
        assert_eq!(out.surface.pieces[0].vertices[2].uv, [0.5, 1.0]);
    }

    /// The surface: a vertex for each of the model's, its texture coordinates and its normal
    /// (z the other way round, as the places), every polygon painted with the material.
    #[test]
    fn the_surface_fits_the_model() {
        let mesh = scm::parse(&scm::tests::tank()).unwrap();
        let out = convert(&mesh, 1.0 / 16.0, paint()).unwrap();
        assert!(out.surface.fits(&out.model));
        assert_eq!(out.surface.materials, [paint()]);
        let hull = &out.surface.pieces[0];
        assert_eq!(hull.vertices[1].uv, [1.0, 0.0]);
        assert_eq!(hull.vertices[0].normal, [0, 32767, 0]);
        assert!(out.model.pieces.iter().flat_map(|p| &p.primitives).all(|p| p.texture.as_deref() == Some("tank")));
    }

    /// Bones listed out of the depth-first order a 3DO keeps become its pieces in that order.
    #[test]
    fn pieces_follow_the_3dos_order() {
        let still = [1.0, 0.0, 0.0, 0.0];
        let file = scm::tests::write(
            &[("Root", -1, [0.0; 3], still), ("A", 0, [1.0, 0.0, 0.0], still), ("B", 0, [2.0, 0.0, 0.0], still), ("A1", 1, [0.0, 1.0, 0.0], still)],
            &[([0.0; 3], [0.0; 2], 0), ([1.0, 1.0, 0.0], [0.0; 2], 3), ([2.0, 0.0, 0.0], [0.0; 2], 2)],
            &[[0, 0, 0], [1, 1, 1], [2, 2, 2]],
        );
        let mesh = scm::parse(&file).unwrap();
        let out = convert(&mesh, 1.0 / 16.0, paint()).unwrap();
        assert_eq!(out.model.pieces.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Root", "A", "A1", "B"]);
        assert_eq!(out.bones, [0, 1, 3, 2]);
        assert_eq!(out.model.pieces[2].parent, Some(1));
        assert_eq!(out.model.pieces[0].children, [1, 3]);
        // A1's vertex at its place, B's at its.
        assert_eq!(out.model.pieces[2].vertices, [[0, 0, 0]]);
        assert_eq!(out.model.pieces[3].vertices, [[0, 0, 0]]);
        // Written and read back, the same order.
        let back = Model3do::parse(&out.model.to_bytes()).unwrap();
        assert_eq!(back.pieces.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Root", "A", "A1", "B"]);
        assert!(out.surface.fits(&back));
    }

    /// A lighter mesh whose skeleton kept only the hull (its root renamed) and the turret is laid
    /// onto the first's pieces: the turret's triangle on the turret, the rest on the hull, at
    /// their places.
    #[test]
    fn a_second_level_takes_the_firsts_pieces() {
        let first_mesh = scm::parse(&scm::tests::tank()).unwrap();
        let first = convert(&first_mesh, 1.0 / 16.0, paint()).unwrap();
        let still = [1.0, 0.0, 0.0, 0.0];
        let file = scm::tests::write(
            &[("lod1_Tank", -1, [0.0; 3], still), ("Turret", 0, [0.0, 4.0, -3.0], still), ("Rivet", 1, [0.0, 1.0, 0.0], still)],
            &[([1.0, 0.0, 2.0], [0.0; 2], 0), ([0.0, 4.0, -3.0], [0.0; 2], 1), ([0.0, 5.0, -3.0], [0.0; 2], 2)],
            &[[0, 0, 0], [1, 2, 1]],
        );
        let far = convert_onto(&scm::parse(&file).unwrap(), &first_mesh, &first, 1.0 / 16.0, paint()).unwrap();
        assert_eq!(far.model.pieces.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Tank", "Turret", "Turret_Barrel", "Exhaust"]);
        assert_eq!(far.model.pieces[1].offset, first.model.pieces[1].offset);
        assert_eq!(far.model.pieces[0].vertices, [[px(1.0), 0, px(-2.0)]]);
        // (The rivet, a bone the first has not, on its parent the turret: a unit above it.)
        assert_eq!(far.model.pieces[1].vertices, [[0, 0, 0], [0, px(1.0), 0]]);
        assert!(far.model.pieces[2].vertices.is_empty());
        assert!(far.surface.fits(&far.model));
    }

    /// A triangle painted from a repeat of its texture is moved into it by whole textures (its
    /// corners copied, the other triangle sharing them left alone); one straddling an edge stays.
    #[test]
    fn texture_coordinates_are_moved_into_the_texture() {
        let still = [1.0, 0.0, 0.0, 0.0];
        let file = scm::tests::write(
            &[("Root", -1, [0.0; 3], still)],
            &[([0.0; 3], [1.25, 0.5], 0), ([1.0, 0.0, 0.0], [1.75, 0.5], 0), ([0.0, 1.0, 0.0], [1.5, 0.9], 0), ([0.0, 0.0, 1.0], [0.5, 0.5], 0), ([1.0, 1.0, 0.0], [-0.25, 0.5], 0)],
            &[[0, 1, 2], [0, 3, 2], [3, 4, 2]],
        );
        let out = convert(&scm::parse(&file).unwrap(), 1.0 / 16.0, paint()).unwrap();
        assert_eq!((out.uvs_moved, out.uvs_straddling), (1, 2));
        let (piece, face) = (&out.model.pieces[0], &out.surface.pieces[0]);
        let uvs = |k: usize| piece.primitives[k].indices.iter().map(|&i| face.vertices[i as usize].uv).collect::<Vec<_>>();
        assert!(uvs(0).iter().all(|uv| (0.0..=1.0).contains(&uv[0])), "{:?}", uvs(0));
        assert_eq!(uvs(1), [[1.25, 0.5], [1.5, 0.9], [0.5, 0.5]], "straddling, left as it was");
        assert_eq!(piece.vertices.len(), 8, "the moved triangle's corners copied");
        assert!(out.surface.fits(&out.model));
    }

    /// The model written as a 3DO reads back with the same tree.
    #[test]
    fn the_model_writes_as_a_3do() {
        let mesh = scm::parse(&scm::tests::tank()).unwrap();
        let out = convert(&mesh, 0.05, paint()).unwrap();
        let back = Model3do::parse(&out.model.to_bytes()).unwrap();
        assert_eq!(back.pieces.len(), 4);
        assert_eq!(back.piece_index("turret_barrel"), Some(2));
        assert_eq!(back.pieces[2].parent, Some(1));
    }
}
