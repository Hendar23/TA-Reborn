//! Pads at a building's foot (runways, landing pads, aprons) lifted clear of the ground. TA
//! drew buildings over a flat picture of the map, so a pad modelled a little below the
//! building's base still showed: the Aircraft Plant's runway lies 0.18 below it. On the 3D
//! ground, where a building stands on the highest ground under its footprint, such a pad is
//! buried, and one at the base itself flickers with the ground.
//!
//! And the ground plates under wrecks their files forgot to flag as selection plates, left out
//! as those are (`flag_ground_plates`).

use glam::Vec3;
use ta_formats::model3do::{FIXED_ONE, Model3do};

/// Pads end at least this far above the base (and the ground levelled under the building, at
/// its base: drawn heights are rounded down, `terrain::halves`)... (0.06 was within what a
/// depth buffer tells apart far out, and an air plant's runway flickered with the ground through
/// it as the camera moved, zoomed out.)
const ABOVE: f32 = 0.15;
/// ...and their layers (a marking on a runway) are kept in order within this much more.
const SPAN: f32 = 0.1;
/// Deeper than this under the base is a foundation, meant to go into the ground.
const DEEPEST: f32 = 0.75;
/// A face this level (the difference of its corners' heights) is a pad.
const LEVEL: f32 = 0.3;

/// Lifts the level faces lying between `DEEPEST` below a model's base and `ABOVE` it to just
/// above the base, keeping their order and staying under the next level face above them.
/// Returns how many corners moved. For buildings and wrecks, which stand on the ground.
pub fn lift_ground_pads(model: &mut Model3do) -> usize {
    // Each piece's origin at rest, in model units.
    let mut rest_y = Vec::with_capacity(model.pieces.len());
    for p in &model.pieces {
        let own = p.offset[1] as f32 / FIXED_ONE;
        rest_y.push(p.parent.map_or(0.0, |parent| rest_y[parent]) + own);
    }
    let height = |pi: usize, v: [i32; 3]| rest_y[pi] + v[1] as f32 / FIXED_ONE;
    let (mut corners, mut lowest, mut ceiling) = (Vec::new(), f32::MAX, f32::MAX);
    for (pi, p) in model.pieces.iter().enumerate() {
        for prim in p.primitives.iter().filter(|prim| prim.indices.len() >= 3) {
            let ys: Vec<f32> = prim.indices.iter().filter_map(|&i| p.vertices.get(i as usize)).map(|&v| height(pi, v)).collect();
            let (lo, hi) = ys.iter().fold((f32::MAX, f32::MIN), |(lo, hi), &y| (lo.min(y), hi.max(y)));
            if ys.len() < 3 || hi - lo > LEVEL {
                continue;
            }
            if lo >= -DEEPEST && hi < ABOVE {
                lowest = lowest.min(lo);
                corners.extend(prim.indices.iter().map(|&i| (pi, i as usize)));
            } else if lo >= ABOVE {
                ceiling = ceiling.min(lo);
            }
        }
    }
    if corners.is_empty() {
        return 0;
    }
    corners.sort_unstable();
    corners.dedup();
    let top = (ABOVE + SPAN).min((ABOVE + ceiling) * 0.5);
    let scale = (top - ABOVE) / (ABOVE - lowest).max(1e-3);
    for &(pi, i) in &corners {
        let Some(v) = model.pieces[pi].vertices.get_mut(i) else { continue };
        let y = rest_y[pi] + v[1] as f32 / FIXED_ONE;
        let lifted = ABOVE + (y - lowest) * scale;
        v[1] = ((lifted - rest_y[pi]) * FIXED_ONE).round() as i32;
    }
    corners.len()
}

/// A ground plate is as wide this share of its model, each way (TA's wrecks' are 66% to 100%).
const PLATE_SPAN: f32 = 0.5;

/// Leaves out the ground plates a wreck's model drew, and returns how many: in any piece, a flat,
/// untextured face of four corners facing straight down, lying on the ground the model stands on
/// (or under it) and spanning at least half the model each way, other than the piece's selection
/// plate. TA's wrecks (`*_dead`) were made from their units' models with the plate TA's editor
/// laid under them, black and the size of their footprint, among the rest of their root's faces:
/// 21 of TA's are not flagged as selection plates (the Sea Serpents', the EMP launcher's, the
/// Fido's, the Thud's...), and two more wrecks have a copy of their flagged one under it. TA's
/// camera never saw them (they face down); here they were given their backs where a 3D camera
/// sees them from behind (see `halves::give_backs`: not a face lying at the model's foot, but
/// debris lies lower in most of these), and from above every one was a large black square at its
/// wreck's base, which darkened the ground round it as what stands there does (its occlusion
/// footprint). The largest is flagged as its piece's selection plate where it has none, any other
/// emptied (left out as a stray line is): neither is drawn, nor shadows the ground. (For wrecks,
/// which stand on the ground; `ModelLibrary::add` leaves out the plates units' files forget to
/// flag, alone in pieces of their own.)
pub fn flag_ground_plates(model: &mut Model3do) -> usize {
    let mut rest = Vec::with_capacity(model.pieces.len());
    for p in &model.pieces {
        let own = Vec3::new(p.offset[0] as f32, p.offset[1] as f32, -(p.offset[2] as f32)) / FIXED_ONE;
        rest.push(p.parent.map_or(own, |parent: usize| rest[parent] + own));
    }
    // (As `ModelLibrary::add` reads the vertices: z turned round.)
    let corner = |pi: usize, v: [i32; 3]| rest[pi] + Vec3::new(v[0] as f32, v[1] as f32, -(v[2] as f32)) / FIXED_ONE;
    let (lo, hi) = model
        .pieces
        .iter()
        .enumerate()
        .flat_map(|(pi, p)| p.vertices.iter().map(move |&v| (pi, v)))
        .fold((Vec3::MAX, Vec3::MIN), |(lo, hi), (pi, v)| (lo.min(corner(pi, v)), hi.max(corner(pi, v))));
    if lo.x > hi.x {
        return 0;
    }
    let mut left_out = 0;
    for pi in 0..model.pieces.len() {
        let p = &model.pieces[pi];
        // Each plate and how much ground it covers, the largest first.
        let mut plates: Vec<(usize, f32)> = p
            .primitives
            .iter()
            .enumerate()
            .filter(|&(k, prim)| Some(k) != p.selection_primitive && prim.texture.is_none() && prim.indices.len() == 4)
            .filter_map(|(k, prim)| {
                let c: Vec<Vec3> = prim.indices.iter().map(|&i| p.vertices.get(i as usize).map(|&v| corner(pi, v))).collect::<Option<_>>()?;
                let level = c.iter().all(|q| (q.y - c[0].y).abs() < 0.01);
                let on_ground = c[0].y <= super::GROUND_PLATE_HEIGHT;
                let (a, b) = c.iter().fold((Vec3::MAX, Vec3::MIN), |(a, b), &q| (a.min(q), b.max(q)));
                let wide = b.x - a.x >= PLATE_SPAN * (hi.x - lo.x) && b.z - a.z >= PLATE_SPAN * (hi.z - lo.z);
                (level && on_ground && wide && super::face_normal(&c).y < -0.99).then_some((k, (b.x - a.x) * (b.z - a.z)))
            })
            .collect();
        plates.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        let piece = &mut model.pieces[pi];
        for (n, &(k, _)) in plates.iter().enumerate() {
            if n == 0 && piece.selection_primitive.is_none() {
                piece.selection_primitive = Some(k);
            } else {
                piece.primitives[k].indices.clear();
            }
            left_out += 1;
        }
    }
    left_out
}

#[cfg(test)]
mod tests {
    use ta_formats::model3do::{Piece, Primitive};

    use super::*;

    fn quad(y: f32, x0: f32, x1: f32) -> (Vec<[i32; 3]>, Vec<u16>) {
        let f = |v: f32| (v * FIXED_ONE) as i32;
        (vec![[f(x0), f(y), f(-5.0)], [f(x1), f(y), f(-5.0)], [f(x1), f(y), f(5.0)], [f(x0), f(y), f(5.0)]], vec![0, 1, 2, 3])
    }

    fn piece(name: &str, parent: Option<usize>, offset_y: f32, faces: &[(f32, f32, f32)]) -> Piece {
        let mut vertices = Vec::new();
        let mut primitives = Vec::new();
        for &(y, x0, x1) in faces {
            let (v, idx) = quad(y, x0, x1);
            let first = vertices.len() as u16;
            vertices.extend(v);
            primitives.push(Primitive { color_index: 0, indices: idx.iter().map(|i| first + i).collect(), texture: None });
        }
        Piece { name: name.into(), parent, offset: [0, (offset_y * FIXED_ONE) as i32, 0], vertices, primitives, selection_primitive: None, children: Vec::new() }
    }

    #[test]
    fn a_runway_under_the_base_comes_up_in_order_under_the_pad_over_it() {
        // The Aircraft Plant's shape: a runway 0.18 under the base, a marking on it at the base,
        // a landing pad (its own piece) 0.24 up, a wall rising from the base, and a foundation
        // going 4 into the ground.
        let mut model = Model3do {
            pieces: vec![
                piece("base", None, 0.0, &[(-0.18, -60.0, 60.0), (0.0, -10.0, 10.0), (-4.0, -2.0, 2.0)]),
                piece("pad", Some(0), 0.24, &[(0.0, -20.0, 20.0)]),
            ],
        };
        let wall_top = (20.0 * FIXED_ONE) as i32;
        model.pieces[0].vertices.push([0, wall_top, 0]);
        assert_eq!(lift_ground_pads(&mut model), 8, "the runway's and the marking's corners");
        let y = |pi: usize, vi: usize| model.pieces[pi].vertices[vi][1] as f32 / FIXED_ONE;
        let (runway, marking, foundation) = (y(0, 0), y(0, 4), y(0, 8));
        let pad = 0.24 + y(1, 0);
        assert!(runway >= ABOVE - 1e-4, "the runway is above the base: {runway}");
        assert!(marking > runway + 0.01, "the marking stays on the runway: {marking} over {runway}");
        assert!(pad > marking + 0.01, "the pad stays over both: {pad} over {marking}");
        assert_eq!(foundation, -4.0, "a foundation stays in the ground");
        assert_eq!(model.pieces[0].vertices[12][1], wall_top, "nothing above moves");
    }

    /// A square plate `half` a side from the middle at height `y`, untextured, facing down (or
    /// up), as `ModelLibrary::add` reads it (z turned round), added to `piece`: its index.
    fn plate(piece: &mut Piece, y: f32, half: f32, down: bool, texture: Option<&str>) -> usize {
        let f = |v: f32| (v * FIXED_ONE) as i32;
        let first = piece.vertices.len() as u16;
        piece.vertices.extend([[f(-half), f(y), f(-half)], [f(half), f(y), f(-half)], [f(half), f(y), f(half)], [f(-half), f(y), f(half)]]);
        let mut indices: Vec<u16> = (first..first + 4).collect();
        let corners: Vec<Vec3> = indices.iter().map(|&i| piece.vertices[i as usize]).map(|v| Vec3::new(v[0] as f32, v[1] as f32, -(v[2] as f32)) / FIXED_ONE).collect();
        if (super::super::face_normal(&corners).y < 0.0) != down {
            indices.reverse();
        }
        piece.primitives.push(Primitive { color_index: 0, indices, texture: texture.map(Into::into) });
        piece.primitives.len() - 1
    }

    /// The ground plate a wreck's file left unflagged (as TA's Sea Serpents', EMP launcher's and
    /// Fido's do): black, the size of the footprint, facing down, among its root's faces, debris
    /// lying lower. It is flagged as the piece's selection plate, and a copy of a flagged one is
    /// emptied: neither is drawn. A plate facing up (ground painted to be seen), a textured one, a
    /// small one or one standing over the ground stays as it is.
    #[test]
    fn a_wrecks_unflagged_ground_plate_is_left_out() {
        // Debris (painted) over the ground and sunk into it, then the plate.
        let wreck = |plate_at: f32, half: f32, down: bool, texture: Option<&str>| {
            let mut root = piece("base", None, 0.0, &[(3.0, -4.0, 4.0), (-1.5, -6.0, 6.0)]);
            for p in &mut root.primitives {
                p.texture = Some("wreck003c".into());
            }
            let k = plate(&mut root, plate_at, half, down, texture);
            (Model3do { pieces: vec![root] }, k)
        };
        let (mut model, k) = wreck(0.0, 16.0, true, None);
        assert_eq!(flag_ground_plates(&mut model), 1);
        assert_eq!(model.pieces[0].selection_primitive, Some(k), "the plate is its selection plate");
        assert_eq!(flag_ground_plates(&mut model), 0, "flagged once");
        // A copy of the plate under one flagged already: emptied, the other faces as they were.
        let (mut model, k) = wreck(0.0, 16.0, true, None);
        model.pieces[0].selection_primitive = Some(k);
        let copy = plate(&mut model.pieces[0], 0.0, 16.0, true, None);
        assert_eq!(flag_ground_plates(&mut model), 1);
        assert_eq!(model.pieces[0].selection_primitive, Some(k));
        assert!(model.pieces[0].primitives[copy].indices.is_empty());
        assert!(model.pieces[0].primitives[..copy].iter().all(|p| p.indices.len() == 4));
        // What is no such plate.
        for (what, at, half, down, texture) in [
            ("facing up", 0.0, 16.0, false, None),
            ("textured", 0.0, 16.0, true, Some("wreck005b")),
            ("over the ground", 3.0, 16.0, true, None),
            ("small", 0.0, 2.0, true, None),
        ] {
            let (mut model, _) = wreck(at, half, down, texture);
            let before = model.to_bytes();
            assert_eq!(flag_ground_plates(&mut model), 0, "{what}");
            assert_eq!(model.to_bytes(), before, "{what}");
        }
    }

    #[test]
    fn a_model_without_pads_is_left_alone() {
        let mut model = Model3do { pieces: vec![piece("base", None, 0.0, &[(3.0, -5.0, 5.0)])] };
        assert_eq!(lift_ground_pads(&mut model), 0);
    }
}
