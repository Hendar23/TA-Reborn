//! A unit's animations (`objects3d/<id>.anims`, ta_formats' `anims`): its walk and its deaths,
//! made from Forged Alliance's (`.sca`) for its model (docs/SUPCOM_UNITS.md).
//!
//! An animation turns and moves the mesh's bones, each relative to its parent; the model's pieces
//! are those bones laid along the model's axes at their places at rest ([`super::model`]). So a
//! frame's bones are composed down the skeleton (a bone the animation leaves alone keeps its place
//! and turn at rest under its parent), and each piece is drawn where its bone has gone, turned by
//! its bone's turn less its turn at rest: the piece then holds its vertices where the mesh's
//! skinning puts them. Each piece's place and turn under its parent piece, less where it stands in
//! the model, is the key, in a TA script's units and axes (moves along x negated, turns about z;
//! applied about y, then x, then z, as the engine composes a script's).
//!
//! A walk's root keeps to where it stands (the unit's own movement carries it): its going across
//! the ground over the cycle is taken out, its rise and fall kept. A death keeps all of its root's
//! movement (a falling, toppling body).
//!
//! In plain arithmetic, with our own arctangent ([`crate::convert::trig`]): the same bytes on every
//! computer.

use ta_formats::anims::{Anims, Clip, Key};

use crate::convert::trig::atan2;
use crate::sca::Animation;
use crate::scm::Mesh;

/// A place and a turn (a unit quaternion, `[w, x, y, z]`).
type Pose = ([f64; 3], [f64; 4]);

/// The least move (65,536ths of a pixel) or turn (angle units) a piece must make in some frame
/// for the clip to move it.
const STILL: i32 = 64;

/// `walk` (a cycle) and `deaths` made into clips for the model made of `mesh` (`pieces`: each
/// piece's bone, in the model's order: `model::Converted::bones`), its mesh units `pixels` pixels.
/// Clips that move no piece are left out.
pub fn anims(mesh: &Mesh, pieces: &[usize], pixels: f64, walk: Option<&Animation>, deaths: &[Animation]) -> Anims {
    let mut clips = Vec::new();
    if let Some(w) = walk
        && let Some(c) = clip("walk", mesh, pieces, pixels, w, true)
    {
        clips.push(c);
    }
    for (k, d) in deaths.iter().enumerate() {
        let name = if k == 0 { "death".to_string() } else { format!("death{}", k + 1) };
        if let Some(c) = clip(&name, mesh, pieces, pixels, d, false) {
            clips.push(c);
        }
    }
    Anims { clips }
}

/// One animation as a clip (`walk`: its root kept where it stands), or none if it moves nothing.
fn clip(name: &str, mesh: &Mesh, pieces: &[usize], pixels: f64, anim: &Animation, walk: bool) -> Option<Clip> {
    if anim.frames.is_empty() || pieces.is_empty() {
        return None;
    }
    let rest = rest(mesh);
    // Each mesh bone's in the animation, by name.
    let bone_in: Vec<Option<usize>> = mesh.bones.iter().map(|b| anim.bone(&b.name)).collect();
    let local = |b: usize, frame: usize| -> Pose {
        match bone_in[b] {
            Some(a) => {
                let (p, q) = anim.frames[frame].keys[a];
                (p.map(f64::from), normalized(q.map(f64::from)))
            }
            None => (mesh.bones[b].position.map(f64::from), normalized(mesh.bones[b].rotation.map(f64::from))),
        }
    };
    // Where the walk's root starts, across the ground (taken out of every frame).
    let roots: Vec<usize> = (0..mesh.bones.len()).filter(|&b| mesh.bones[b].parent.is_none()).collect();
    let start: Vec<[f64; 3]> = roots.iter().map(|&b| local(b, 0).0).collect();
    let mut frames: Vec<Vec<Key>> = Vec::with_capacity(anim.frames.len());
    for f in 0..anim.frames.len() {
        // The bones composed down the skeleton (parents come first in a mesh).
        let mut world: Vec<Pose> = Vec::with_capacity(mesh.bones.len());
        for b in 0..mesh.bones.len() {
            let (mut place, turn) = local(b, f);
            let pose = match mesh.bones[b].parent {
                None => {
                    if walk && let Some(r) = roots.iter().position(|&r| r == b) {
                        place[0] = start[r][0];
                        place[2] = start[r][2];
                    }
                    (place, turn)
                }
                Some(p) => {
                    let (pp, pq) = world[p];
                    let moved = rotate(pq, place);
                    ([pp[0] + moved[0], pp[1] + moved[1], pp[2] + moved[2]], multiply(pq, turn))
                }
            };
            world.push(pose);
        }
        // Each piece drawn where its bone is, turned by its bone's turn less its turn at rest.
        let drawn: Vec<Pose> = (0..mesh.bones.len()).map(|b| (world[b].0, multiply(world[b].1, conjugate(rest[b].1)))).collect();
        let keys = pieces
            .iter()
            .map(|&b| {
                let (place, turn, at_rest) = match mesh.bones[b].parent {
                    None => (drawn[b].0, drawn[b].1, rest[b].0),
                    Some(p) => {
                        let back = conjugate(drawn[p].1);
                        let d = [drawn[b].0[0] - drawn[p].0[0], drawn[b].0[1] - drawn[p].0[1], drawn[b].0[2] - drawn[p].0[2]];
                        let r = [rest[b].0[0] - rest[p].0[0], rest[b].0[1] - rest[p].0[1], rest[b].0[2] - rest[p].0[2]];
                        (rotate(back, d), multiply(back, drawn[b].1), r)
                    }
                };
                key(place, turn, at_rest, pixels)
            })
            .collect();
        frames.push(keys);
    }
    // The pieces it moves.
    let moved: Vec<usize> = (0..pieces.len())
        .filter(|&k| frames.iter().any(|keys| keys[k].moved.iter().chain(&keys[k].turned).any(|v| v.abs() >= STILL)))
        .collect();
    if moved.is_empty() {
        return None;
    }
    let first = anim.frames[0].time;
    Some(Clip {
        name: name.to_string(),
        seconds: anim.duration.max(anim.frames.last().map_or(0.0, |f| f.time - first)),
        pieces: moved.iter().map(|&k| k as u32).collect(),
        times: anim.frames.iter().map(|f| (f.time - first).max(0.0)).collect(),
        keys: frames.iter().flat_map(|keys| moved.iter().map(|&k| keys[k])).collect(),
    })
}

/// Each bone's place and turn at rest, its parents' composed with its own.
fn rest(mesh: &Mesh) -> Vec<Pose> {
    let mut out: Vec<Pose> = Vec::with_capacity(mesh.bones.len());
    for bone in &mesh.bones {
        let (place, turn) = (bone.position.map(f64::from), normalized(bone.rotation.map(f64::from)));
        out.push(match bone.parent {
            None => (place, turn),
            Some(p) => {
                let (pp, pq) = out[p];
                let moved = rotate(pq, place);
                ([pp[0] + moved[0], pp[1] + moved[1], pp[2] + moved[2]], multiply(pq, turn))
            }
        });
    }
    out
}

/// A piece at `place` under its parent turned by `turn`, standing at `at_rest` there in the model,
/// as a script's move and turn (mesh units `pixels` pixels).
fn key(place: [f64; 3], turn: [f64; 4], at_rest: [f64; 3], pixels: f64) -> Key {
    let fixed = |v: f64| (v * pixels * 65536.0).round().clamp(i32::MIN as f64, i32::MAX as f64) as i32;
    // (A script's x moves are negated: the engine's x is the mesh's.)
    let moved = [-fixed(place[0] - at_rest[0]), fixed(place[1] - at_rest[1]), fixed(place[2] - at_rest[2])];
    let (y, x, z) = euler_yxz(turn);
    let units = |radians: f64| -> i32 { ((radians * 32768.0 / std::f64::consts::PI).round() as i64).rem_euclid(65536) as i32 };
    let wrapped = |a: i32| if a >= 32768 { a - 65536 } else { a };
    // (And its z turns.)
    Key { moved, turned: [wrapped(units(x)), wrapped(units(y)), wrapped(units(-z))] }
}

/// Under this, the cosine of the turn about x counts as none: the piece points straight up or
/// down, where the turns about y and z are one.
const STRAIGHT_UP: f64 = 1e-6;

/// The turns about y, x and z (radians) that composed so (`R = Ry Rx Rz`) make `q`.
fn euler_yxz(q: [f64; 4]) -> (f64, f64, f64) {
    let [w, x, y, z] = q;
    let m00 = 1.0 - 2.0 * (y * y + z * z);
    let m01 = 2.0 * (x * y - w * z);
    let m02 = 2.0 * (x * z + w * y);
    let m22 = 1.0 - 2.0 * (x * x + y * y);
    let m10 = 2.0 * (x * y + w * z);
    let m11 = 1.0 - 2.0 * (x * x + z * z);
    let m12 = (2.0 * (y * z - w * x)).clamp(-1.0, 1.0);
    // asin(-m12), from the arctangent: the same bits everywhere.
    let cos_pitch = (1.0 - m12 * m12).max(0.0).sqrt();
    let pitch = atan2(-m12, cos_pitch);
    if cos_pitch < STRAIGHT_UP {
        // Pointing straight up or down (sin x = -m12, ±1), the turns about y and z are one: with
        // none about z, the row Ry Rx makes of x's axis holds the turn about y (its first two
        // entries cos y and sin y, the second's sign with sin x's).
        return (atan2(-m12 * m01, m00), pitch, 0.0);
    }
    (atan2(m02, m22), pitch, atan2(m10, m11))
}

fn normalized(q: [f64; 4]) -> [f64; 4] {
    let n = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    if n > 0.0 { q.map(|v| v / n) } else { [1.0, 0.0, 0.0, 0.0] }
}

fn conjugate(q: [f64; 4]) -> [f64; 4] {
    [q[0], -q[1], -q[2], -q[3]]
}

/// The quaternion product `a b`.
fn multiply(a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
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
fn rotate(q: [f64; 4], v: [f64; 3]) -> [f64; 3] {
    let [w, x, y, z] = q;
    let c = [y * v[2] - z * v[1], z * v[0] - x * v[2], x * v[1] - y * v[0]];
    let cc = [y * c[2] - z * c[1], z * c[0] - x * c[2], x * c[1] - y * c[0]];
    [v[0] + 2.0 * (w * c[0] + cc[0]), v[1] + 2.0 * (w * c[1] + cc[1]), v[2] + 2.0 * (w * c[2] + cc[2])]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sca::Frame;
    use crate::scm::Bone;

    /// A hip (the root, at rest 2 up) and a leg under it resting turned a quarter about x (as
    /// Supreme Commander's bones may rest), 1 ahead of it.
    fn mesh() -> Mesh {
        let s = std::f32::consts::FRAC_1_SQRT_2;
        let bone = |name: &str, parent: Option<usize>, position: [f32; 3], rotation: [f32; 4]| Bone { name: name.into(), parent, position, rotation, rest_inverse: [0.0; 16] };
        Mesh { bones: vec![bone("Hip", None, [0.0, 2.0, 0.0], [1.0, 0.0, 0.0, 0.0]), bone("Leg", Some(0), [0.0, 0.0, 1.0], [s, s, 0.0, 0.0])], vertices: Vec::new(), triangles: Vec::new(), info: String::new() }
    }

    fn animation(keys: Vec<Vec<([f32; 3], [f32; 4])>>) -> Animation {
        let frames = keys.into_iter().enumerate().map(|(k, keys)| Frame { time: k as f32 * 0.5, flags: 0, keys }).collect();
        Animation { duration: 1.0, bones: vec!["hip".into(), "LEG".into()], parents: vec![None, Some(0)], root: ([0.0; 3], [1.0, 0.0, 0.0, 0.0]), frames }
    }

    #[test]
    fn a_bone_at_rest_is_a_piece_at_rest_and_its_turns_are_a_scripts() {
        let m = mesh();
        let s = std::f32::consts::FRAC_1_SQRT_2;
        let rest = ([0.0, 2.0, 0.0], [1.0, 0.0, 0.0, 0.0]);
        let leg_rest = ([0.0, 0.0, 1.0], [s, s, 0.0, 0.0]);
        // The leg turned a further quarter about its own x (the model's x: it rests turned about
        // x), and the hip walked 3 ahead and up 0.5.
        let leg_on = ([0.0, 0.0, 1.0], [0.0, 1.0, 0.0, 0.0]);
        let a = animation(vec![vec![rest, leg_rest], vec![([0.0, 2.5, 3.0], [1.0, 0.0, 0.0, 0.0]), leg_on]]);
        let clips = anims(&m, &[0, 1], 32.0, Some(&a), &[]);
        let walk = clips.clip("walk").unwrap();
        assert_eq!(walk.pieces, [0, 1], "both move");
        assert_eq!(walk.frame(0), [Key::default(); 2], "the first frame is the rest");
        let [hip, leg] = [walk.frame(1)[0], walk.frame(1)[1]];
        assert_eq!(hip.moved, [0, (0.5 * 32.0 * 65536.0) as i32, 0], "the walk's root rises, its way ahead taken out");
        assert_eq!(leg.moved, [0, 0, 0]);
        assert_eq!(leg.turned, [16384, 0, 0], "a quarter about x");
        // A death keeps its root's going.
        let clips = anims(&m, &[0, 1], 32.0, None, &[a.clone(), a]);
        assert_eq!(clips.clips.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["death", "death2"]);
        assert_eq!(clips.clip("death").unwrap().frame(1)[0].moved[2], (3.0 * 32.0 * 65536.0) as i32);
    }

    #[test]
    fn a_turn_about_z_or_y_reads_back_as_a_scripts() {
        // A script's z turn is negated (a positive one turns the piece's x towards -y).
        let half = std::f64::consts::FRAC_1_SQRT_2;
        let k = key([0.0; 3], [half, 0.0, 0.0, half], [0.0; 3], 1.0);
        assert_eq!(k.turned, [0, 0, -16384]);
        let k = key([0.0; 3], [half, 0.0, half, 0.0], [0.0; 3], 1.0);
        assert_eq!(k.turned, [0, 16384, 0]);
        // A move along x is negated as well.
        assert_eq!(key([1.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0], [0.0; 3], 2.0).moved, [-131072, 0, 0]);
    }

    /// A piece turned to point straight up or down keeps the rest of its turn: about x a quarter,
    /// and about y as much as it was (at that pitch the turns about y and z are one, all given to
    /// y). The rotation that sends x to -z and y to x.
    #[test]
    fn a_turn_straight_up_keeps_its_heading() {
        let k = key([0.0; 3], [0.5, 0.5, 0.5, -0.5], [0.0; 3], 1.0);
        assert_eq!(k.turned, [16384, 16384, 0]);
        // And down: a quarter the other way about x, and a quarter about y.
        let k = key([0.0; 3], [0.5, -0.5, 0.5, 0.5], [0.0; 3], 1.0);
        assert_eq!(k.turned, [-16384, 16384, 0]);
        // Composed back as the engine composes a script's turns (Ry Rx Rz), x goes where the
        // quaternion sends it.
        let (a, b, c) = euler_yxz([0.5, 0.5, 0.5, -0.5]);
        let x = [a.cos() * c.cos() + a.sin() * b.sin() * c.sin(), b.cos() * c.sin(), -a.sin() * c.cos() + a.cos() * b.sin() * c.sin()];
        assert!(x[0].abs() < 1e-9 && x[1].abs() < 1e-9 && (x[2] + 1.0).abs() < 1e-9, "{a} {b} {c}: {x:?}");
    }

    #[test]
    fn an_animation_that_moves_nothing_is_no_clip() {
        let m = mesh();
        let s = std::f32::consts::FRAC_1_SQRT_2;
        let still = animation(vec![vec![([0.0, 2.0, 0.0], [1.0, 0.0, 0.0, 0.0]), ([0.0, 0.0, 1.0], [s, s, 0.0, 0.0])]; 3]);
        assert!(anims(&m, &[0, 1], 32.0, Some(&still), &[still.clone()]).clips.is_empty());
    }
}
