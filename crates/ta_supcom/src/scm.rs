//! Supreme Commander's model meshes (`.scm`, version 5): a unit's (or a projectile's) one mesh,
//! every vertex bound to one bone of its skeleton, as TA's pieces hold their own polygons. Laid out
//! as the files are, little-endian (docs/SUPCOM_UNITS.md has the table):
//!
//! - the header: `MODL`, the version (5), then where and how many of: the bones with geometry, the
//!   vertices, the extra vertices (none in the game's own), the triangles' indices, the info text,
//!   and the bones in all;
//! - the bones' names, NUL-ended, after a `NAME` mark;
//! - the bones, 108 bytes each: the inverse of the bone's rest pose in the model (a 4 x 4 matrix,
//!   rows), its place and turn relative to its parent (a quaternion, w first), where its name is,
//!   its parent (-1 for the root) and two words unused;
//! - the vertices, 68 bytes each: place, normal, tangent and binormal (in the model, at rest; the
//!   tangent and binormal run against the texture's u and v, `-dP/du` and `-dP/dv`), two texture
//!   coordinates, and four bone indices of which the first binds it;
//! - the triangles: three u16 indices each;
//! - the info text.
//!
//! Places are in the mesh's own units: times the blueprint's `Display.UniformScale` they are the
//! game's grid squares (ogrids). The axes are right-handed, y up, the unit facing +z, its right at
//! -x; a triangle's corners run anticlockwise seen from outside.

use crate::{Error, Result};

/// The most bones or vertices a mesh may claim (the game's largest has 46,000 vertices).
const MAX_ITEMS: u32 = 1_000_000;
const BONE_SIZE: usize = 108;
const VERTEX_SIZE: usize = 68;

/// A bone: its name, its parent (none for the root), its place and turn relative to the parent at
/// rest (a unit quaternion, `[w, x, y, z]`), and the inverse of its rest pose in the model as the
/// file has it (row-major).
#[derive(Clone, Debug, PartialEq)]
pub struct Bone {
    pub name: String,
    pub parent: Option<usize>,
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub rest_inverse: [f32; 16],
}

/// A vertex at rest, in the model: its place, normal, tangent and binormal, its two texture
/// coordinates, and the bone it moves with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub tangent: [f32; 3],
    pub binormal: [f32; 3],
    pub uv0: [f32; 2],
    pub uv1: [f32; 2],
    pub bone: usize,
}

/// A mesh: its bones in the file's order (a parent always before its children), its vertices, its
/// triangles (indices into the vertices) and its info text.
#[derive(Clone, Debug, PartialEq)]
pub struct Mesh {
    pub bones: Vec<Bone>,
    pub vertices: Vec<Vertex>,
    pub triangles: Vec<[u32; 3]>,
    pub info: String,
}

impl Mesh {
    /// The bone named `name` (any case), as the blueprints name a turret's or a muzzle's.
    pub fn bone(&self, name: &str) -> Option<usize> {
        self.bones.iter().position(|b| b.name.eq_ignore_ascii_case(name))
    }

    /// Each bone's place and turn in the model at rest, its parents' composed with its own (in
    /// plain arithmetic: the same bits on every computer).
    pub fn rest_poses(&self) -> Vec<([f32; 3], [f32; 4])> {
        let mut out: Vec<([f32; 3], [f32; 4])> = Vec::with_capacity(self.bones.len());
        for bone in &self.bones {
            let pose = match bone.parent {
                None => (bone.position, bone.rotation),
                Some(p) => {
                    let (place, turn) = out[p];
                    let moved = rotate(turn, bone.position);
                    ([place[0] + moved[0], place[1] + moved[1], place[2] + moved[2]], multiply(turn, bone.rotation))
                }
            };
            out.push(pose);
        }
        out
    }
}

/// The quaternion product `a b` (`[w, x, y, z]`).
pub fn multiply(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let [aw, ax, ay, az] = a;
    let [bw, bx, by, bz] = b;
    [
        aw * bw - ax * bx - ay * by - az * bz,
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
    ]
}

/// `v` turned by the unit quaternion `q`.
pub fn rotate(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
    let [w, x, y, z] = q;
    // v + 2 w (u x v) + 2 u x (u x v), u = (x, y, z).
    let c = [y * v[2] - z * v[1], z * v[0] - x * v[2], x * v[1] - y * v[0]];
    let cc = [y * c[2] - z * c[1], z * c[0] - x * c[2], x * c[1] - y * c[0]];
    [v[0] + 2.0 * (w * c[0] + cc[0]), v[1] + 2.0 * (w * c[1] + cc[1]), v[2] + 2.0 * (w * c[2] + cc[2])]
}

/// Reads a mesh.
pub fn parse(b: &[u8]) -> Result<Mesh> {
    let r = Reader { b };
    if b.len() < 48 || &b[0..4] != b"MODL" {
        return Err(bad("not a model mesh"));
    }
    let version = r.u32(4)?;
    if version != 5 {
        return Err(bad(&format!("version {version} (5 is read)")));
    }
    let [bone_at, _weighted, vertex_at, _extra_at, vertex_count, index_at, index_count, info_at, info_len, bone_count] =
        [8, 12, 16, 20, 24, 28, 32, 36, 40, 44].map(|o| r.u32(o).unwrap_or(0));
    if bone_count == 0 || bone_count > MAX_ITEMS || vertex_count > MAX_ITEMS || index_count > MAX_ITEMS * 3 || index_count % 3 != 0 {
        return Err(bad(&format!("{bone_count} bones, {vertex_count} vertices, {index_count} indices")));
    }
    // (What it claims to hold must be in the file before room is made for it.)
    let fits = |at: u32, count: u32, size: usize| (at as usize).checked_add((count as usize).saturating_mul(size)).is_some_and(|end| end <= b.len());
    if !fits(bone_at, bone_count, BONE_SIZE) || !fits(vertex_at, vertex_count, VERTEX_SIZE) || !fits(index_at, index_count, 2) {
        return Err(bad(&format!("{bone_count} bones, {vertex_count} vertices and {index_count} indices claimed, past the end of its {} bytes", b.len())));
    }
    let mut bones = Vec::with_capacity(bone_count as usize);
    for i in 0..bone_count as usize {
        let at = bone_at as usize + i * BONE_SIZE;
        // (The whole record there first: then its words read.)
        r.at::<BONE_SIZE>(at)?;
        let rest_inverse = std::array::from_fn(|k| r.f32(at + k * 4).unwrap_or(0.0));
        let position = [r.f32(at + 64)?, r.f32(at + 68)?, r.f32(at + 72)?];
        let rotation = [r.f32(at + 76)?, r.f32(at + 80)?, r.f32(at + 84)?, r.f32(at + 88)?];
        let name = r.cstr(r.u32(at + 92)? as usize)?;
        let parent = match r.i32(at + 96)? {
            -1 => None,
            p if p >= 0 && (p as usize) < i => Some(p as usize),
            p => return Err(bad(&format!("bone {i} ({name}) has parent {p}"))),
        };
        bones.push(Bone { name, parent, position, rotation, rest_inverse });
    }
    let mut vertices = Vec::with_capacity(vertex_count as usize);
    for i in 0..vertex_count as usize {
        let at = vertex_at as usize + i * VERTEX_SIZE;
        let v3 = |o: usize| -> Result<[f32; 3]> { Ok([r.f32(at + o)?, r.f32(at + o + 4)?, r.f32(at + o + 8)?]) };
        let bone = r.byte(at + 64)? as usize;
        if bone >= bones.len() {
            return Err(bad(&format!("vertex {i} bound to bone {bone} of {}", bones.len())));
        }
        vertices.push(Vertex {
            position: v3(0)?,
            normal: v3(12)?,
            tangent: v3(24)?,
            binormal: v3(36)?,
            uv0: [r.f32(at + 48)?, r.f32(at + 52)?],
            uv1: [r.f32(at + 56)?, r.f32(at + 60)?],
            bone,
        });
    }
    let mut triangles = Vec::with_capacity(index_count as usize / 3);
    for t in 0..index_count as usize / 3 {
        let corner = |k: usize| -> Result<u32> {
            let v = r.u16(index_at as usize + (t * 3 + k) * 2)? as u32;
            if v >= vertex_count {
                return Err(bad(&format!("triangle {t} names vertex {v} of {vertex_count}")));
            }
            Ok(v)
        };
        triangles.push([corner(0)?, corner(1)?, corner(2)?]);
    }
    let info = if info_len == 0 {
        String::new()
    } else {
        let end = (info_at as usize).checked_add(info_len as usize).filter(|&e| e <= b.len()).ok_or_else(|| bad("its info runs past the end"))?;
        String::from_utf8_lossy(&b[info_at as usize..end]).trim_end_matches('\0').to_string()
    };
    Ok(Mesh { bones, vertices, triangles, info })
}

fn bad(what: &str) -> Error {
    Error::Format(format!("scm: {what}"))
}

struct Reader<'a> {
    b: &'a [u8],
}

impl Reader<'_> {
    fn at<const N: usize>(&self, at: usize) -> Result<[u8; N]> {
        at.checked_add(N).and_then(|end| self.b.get(at..end)).map(|s| s.try_into().unwrap()).ok_or_else(|| bad(&format!("ends early (at byte {at})")))
    }

    fn byte(&self, at: usize) -> Result<u8> {
        Ok(self.at::<1>(at)?[0])
    }

    fn u16(&self, at: usize) -> Result<u16> {
        Ok(u16::from_le_bytes(self.at(at)?))
    }

    fn u32(&self, at: usize) -> Result<u32> {
        Ok(u32::from_le_bytes(self.at(at)?))
    }

    fn i32(&self, at: usize) -> Result<i32> {
        Ok(i32::from_le_bytes(self.at(at)?))
    }

    fn f32(&self, at: usize) -> Result<f32> {
        Ok(f32::from_le_bytes(self.at(at)?))
    }

    /// The NUL-ended string at `at` (at most 255 bytes).
    fn cstr(&self, at: usize) -> Result<String> {
        let rest = self.b.get(at..).ok_or_else(|| bad("a name past the end"))?;
        let len = rest.iter().take(256).position(|&c| c == 0).ok_or_else(|| bad("a name without its end"))?;
        Ok(String::from_utf8_lossy(&rest[..len]).into_owned())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A mesh file as the game writes one: `bones` (name, parent, place, turn), `vertices`
    /// (place, uv, bone) and `triangles`.
    pub(crate) fn write(bones: &[(&str, i32, [f32; 3], [f32; 4])], vertices: &[([f32; 3], [f32; 2], u8)], triangles: &[[u16; 3]]) -> Vec<u8> {
        let mut out = vec![0u8; 64];
        out.extend_from_slice(b"NAME");
        let mut name_at = Vec::new();
        for (name, ..) in bones {
            name_at.push(out.len() as u32);
            out.extend_from_slice(name.as_bytes());
            out.push(0);
        }
        while !out.len().is_multiple_of(16) {
            out.push(0xC5);
        }
        let bone_at = out.len() as u32;
        for (i, (_, parent, place, turn)) in bones.iter().enumerate() {
            for k in 0..16 {
                out.extend_from_slice(&(if k % 5 == 0 { 1.0f32 } else { 0.0 }).to_le_bytes());
            }
            for v in place.iter().chain(turn.iter()) {
                out.extend_from_slice(&v.to_le_bytes());
            }
            out.extend_from_slice(&name_at[i].to_le_bytes());
            out.extend_from_slice(&parent.to_le_bytes());
            out.extend_from_slice(&[0u8; 8]);
        }
        let vertex_at = out.len() as u32;
        for (place, uv, bone) in vertices {
            for v in place {
                out.extend_from_slice(&v.to_le_bytes());
            }
            // (Normal up, tangent and binormal along -x and -z.)
            for v in [0.0f32, 1.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, -1.0] {
                out.extend_from_slice(&v.to_le_bytes());
            }
            for v in uv.iter().chain(uv.iter()) {
                out.extend_from_slice(&v.to_le_bytes());
            }
            out.extend_from_slice(&[*bone, 0, 0, 0]);
        }
        let index_at = out.len() as u32;
        for t in triangles {
            for v in t {
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
        let info_at = out.len() as u32;
        out.extend_from_slice(b"made in a test\0");
        let header = [5u32, bone_at, bones.len() as u32, vertex_at, 0, vertices.len() as u32, index_at, triangles.len() as u32 * 3, info_at, 15, bones.len() as u32];
        out[0..4].copy_from_slice(b"MODL");
        for (k, v) in header.iter().enumerate() {
            out[4 + k * 4..8 + k * 4].copy_from_slice(&v.to_le_bytes());
        }
        out
    }

    pub(crate) const TURNED: [f32; 4] = [0.0, 0.0, -1.0, 0.0];
    const STILL: [f32; 4] = [1.0, 0.0, 0.0, 0.0];

    /// A tank's skeleton: a hull, its turret, the barrel on that, and an exhaust turned half round.
    pub(crate) fn tank() -> Vec<u8> {
        write(
            &[
                ("Tank", -1, [0.0, 0.0, 0.0], STILL),
                ("Turret", 0, [0.0, 4.0, -3.0], STILL),
                ("Turret_Barrel", 1, [0.0, -0.5, 3.0], STILL),
                ("Exhaust", 0, [2.0, 0.0, -3.5], TURNED),
            ],
            &[([1.0, 0.0, 2.0], [0.0, 0.0], 0), ([-1.0, 0.0, 2.0], [1.0, 0.0], 0), ([0.0, 4.0, -3.0], [0.5, 1.0], 1), ([0.0, 3.5, 5.0], [0.25, 0.5], 2)],
            &[[0, 1, 2], [1, 2, 3]],
        )
    }

    #[test]
    fn a_mesh_reads_its_bones_vertices_and_triangles() {
        let mesh = parse(&tank()).unwrap();
        assert_eq!(mesh.bones.iter().map(|b| b.name.as_str()).collect::<Vec<_>>(), ["Tank", "Turret", "Turret_Barrel", "Exhaust"]);
        assert_eq!(mesh.bones[2].parent, Some(1));
        assert_eq!(mesh.bones[0].parent, None);
        assert_eq!(mesh.vertices.len(), 4);
        assert_eq!(mesh.vertices[3].bone, 2);
        assert_eq!(mesh.vertices[2].uv0, [0.5, 1.0]);
        assert_eq!((mesh.vertices[0].normal, mesh.vertices[0].tangent, mesh.vertices[0].binormal), ([0.0, 1.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 0.0, -1.0]));
        assert_eq!(mesh.triangles, vec![[0, 1, 2], [1, 2, 3]]);
        assert_eq!(mesh.info, "made in a test");
        assert_eq!(mesh.bone("turret_barrel"), Some(2));
    }

    /// A bone's rest pose in the model is its parents' composed with its own: the barrel sits on
    /// the turret, and a child of the turned exhaust would be turned with it.
    #[test]
    fn rest_poses_compose_down_the_skeleton() {
        let mesh = parse(&tank()).unwrap();
        let poses = mesh.rest_poses();
        assert_eq!(poses[2].0, [0.0, 3.5, 0.0]);
        assert_eq!(poses[3].1, TURNED);
        // Half a turn about y takes x to -x and z to -z.
        assert_eq!(rotate(TURNED, [1.0, 2.0, 3.0]), [-1.0, 2.0, -3.0]);
    }

    #[test]
    fn a_broken_mesh_is_refused_not_misread() {
        let good = tank();
        assert!(parse(&good[..40]).is_err(), "cut short");
        let mut other = good.clone();
        other[4] = 6;
        assert!(parse(&other).is_err(), "another version");
        // A vertex bound to a bone the mesh has not got.
        let bad_bone = write(&[("Root", -1, [0.0; 3], STILL)], &[([0.0; 3], [0.0; 2], 3)], &[]);
        assert!(parse(&bad_bone).is_err());
        // A triangle naming a vertex past the last.
        let bad_index = write(&[("Root", -1, [0.0; 3], STILL)], &[([0.0; 3], [0.0; 2], 0)], &[[0, 0, 5]]);
        assert!(parse(&bad_index).is_err());
        // A parent after its child.
        let bad_parent = write(&[("Root", 1, [0.0; 3], STILL), ("Child", -1, [0.0; 3], STILL)], &[], &[]);
        assert!(parse(&bad_parent).is_err());
        // A million vertices claimed in a file of a few hundred bytes: refused before room is
        // made for them.
        let mut claims = good.clone();
        claims[24..28].copy_from_slice(&999_999u32.to_le_bytes());
        let err = parse(&claims).unwrap_err().to_string();
        assert!(err.contains("past the end"), "{err}");
    }
}
