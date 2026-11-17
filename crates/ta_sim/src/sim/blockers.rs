//! What closes a way: a unit that cannot get where it is going though the ground itself leads
//! there has something standing in its way, a wreck fallen in a pass, rocks, trees, a building.
//! [`Sim::way_blockers`] finds the features among it, for whoever sent the unit to clear them
//! (a computer player's constructors reclaim them; a scenario's waves fire on them).
//!
//! The ground's own way to the goal is searched for first (the terrain alone, as the regions
//! have it: what stands on it left out), then the ground the unit can get to now, nearest the
//! goal along that way first. Where it stops is in front of what closes the way, on the way's
//! side of it: the features standing in the cells beyond it, nearer the goal, are those to
//! clear. (Not the place nearest the goal as the crow flies, as a route that cannot get there
//! ends: in a winding pass that is a dead end of it, far from the wreckage that closes it.)

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use rustc_hash::FxHashMap;

use super::*;

/// Cells the search for the ground's way looks at most, and the search for where the unit can
/// get to (a pass several screens long is well within it; a way longer still is not looked at).
const WAY_CELLS: usize = 20_000;
const REACH_CELLS: usize = 10_000;
/// Cells looked at without coming any nearer the goal before the search for where the unit can
/// get to stops (it has found the front of the ground it can get to).
const REACH_STALE: usize = 1_500;
/// Within this many steps of the goal (the ground's way, a step a cell), the unit gets there.
const THERE_STEPS: u32 = 6;
/// The ground's way is looked at this many steps further than it takes to the unit, for the
/// detours round what stands in it (a corridor along the way, not the ground all round).
const WAY_MARGIN: u32 = 48;

/// A step's cost along the ground's way: straight and diagonal (as the pathfinder's).
const STRAIGHT: u32 = 10;
const DIAGONAL: u32 = 14;

impl Sim {
    /// The features standing in the way of a unit of movement class `spec` at `from` going to
    /// `to`, when the ground itself leads there but what stands on it closes the way: those at
    /// the front of the ground it can get to, beyond it along the ground's way, nearest first
    /// (any kind: callers pick those they can clear). None when it can get there, when the
    /// ground itself does not lead there (another island, a cliff), when the way is longer
    /// than the search looks, or when only buildings close it.
    pub fn way_blockers(&self, spec: &MoveSpec, from: FxVec3, to: FxVec3) -> Vec<usize> {
        let world = &self.world;
        let (w, h) = (world.width as i32, world.height as i32);
        let start = World::pos_to_anchor(from, spec.footprint);
        // The unit's region (one standing in a yard, or on a cell a wreck fell on: the nearest
        // round it).
        let region = (0..=3).find_map(|r| ring(start, r).map(|c| world.component(spec, c)).find(|&c| c != 0)).unwrap_or(0);
        if region == 0 {
            return Vec::new();
        }
        let Some(goal) = world.nearest_in_component(spec, region, World::pos_to_anchor(to, spec.footprint), 8) else { return Vec::new() };
        let Some(regions) = world.components_of(spec) else { return Vec::new() };
        let index = |c: (i32, i32)| c.1 as u32 * w as u32 + c.0 as u32;
        let ground = |c: (i32, i32)| c.0 >= 0 && c.1 >= 0 && c.0 < w && c.1 < h && regions[index(c) as usize] == region;
        // The ground's way: steps from the goal to every cell near the way to the unit (A*
        // from the goal towards the unit, then on a little for the detours).
        let octile = |a: (i32, i32), b: (i32, i32)| {
            let (dx, dz) = (a.0.abs_diff(b.0), a.1.abs_diff(b.1));
            let (lo, hi) = if dx < dz { (dx, dz) } else { (dz, dx) };
            DIAGONAL * lo + STRAIGHT * (hi - lo)
        };
        let mut way: FxHashMap<u32, u32> = FxHashMap::default();
        let mut open: BinaryHeap<Reverse<(u32, u32, u32)>> = BinaryHeap::new();
        let mut best_g: FxHashMap<u32, u32> = FxHashMap::default();
        best_g.insert(index(goal), 0);
        open.push(Reverse((octile(goal, start), 0, index(goal))));
        let mut to_unit: Option<u32> = None;
        let mut limit = u32::MAX;
        while let Some(Reverse((f, g, i))) = open.pop() {
            if way.len() >= WAY_CELLS || f > limit {
                break;
            }
            if way.contains_key(&i) || best_g.get(&i).is_some_and(|&b| b < g) {
                continue;
            }
            way.insert(i, g);
            let c = ((i % w as u32) as i32, (i / w as u32) as i32);
            if c == start && to_unit.is_none() {
                to_unit = Some(g);
                // On a little past the unit (the way's cells round it, for detours).
                limit = g + WAY_MARGIN * STRAIGHT;
            }
            for (dx, dz) in NEIGHBOURS {
                let n = (c.0 + dx, c.1 + dz);
                if !ground(n) {
                    continue;
                }
                // (A diagonal step needs both sides open, as a route's does.)
                if dx != 0 && dz != 0 && !(ground((c.0 + dx, c.1)) && ground((c.0, c.1 + dz))) {
                    continue;
                }
                let ng = g + if dx != 0 && dz != 0 { DIAGONAL } else { STRAIGHT };
                let ni = index(n);
                if way.contains_key(&ni) || best_g.get(&ni).is_some_and(|&b| b <= ng) {
                    continue;
                }
                best_g.insert(ni, ng);
                open.push(Reverse((ng + octile(n, start), ng, ni)));
            }
        }
        let Some(from_unit) = to_unit else { return Vec::new() };
        if from_unit <= THERE_STEPS * STRAIGHT {
            return Vec::new();
        }
        // Where the unit can get to, nearest the goal along the ground's way first: the front
        // of it is the cell nearest the goal it reaches.
        let passable = world.passable_for(spec);
        let steps = |c: (i32, i32)| way.get(&index(c)).copied();
        let mut seen: FxHashMap<u32, ()> = FxHashMap::default();
        let mut front = (from_unit, start);
        let mut queue: BinaryHeap<Reverse<(u32, u32)>> = BinaryHeap::new();
        queue.push(Reverse((from_unit, index(start))));
        seen.insert(index(start), ());
        let (mut looked, mut since_nearer) = (0usize, 0usize);
        while let Some(Reverse((d, i))) = queue.pop() {
            looked += 1;
            if looked > REACH_CELLS || since_nearer > REACH_STALE {
                break;
            }
            let c = ((i % w as u32) as i32, (i / w as u32) as i32);
            if d < front.0 {
                front = (d, c);
                since_nearer = 0;
            } else {
                since_nearer += 1;
            }
            if d <= THERE_STEPS * STRAIGHT {
                return Vec::new();
            }
            for (dx, dz) in NEIGHBOURS {
                let n = (c.0 + dx, c.1 + dz);
                let Some(nd) = steps(n) else { continue };
                let ni = index(n);
                if seen.contains_key(&ni) || !passable(n, None) {
                    continue;
                }
                if dx != 0 && dz != 0 && !(passable((c.0 + dx, c.1), None) && passable((c.0, c.1 + dz), None)) {
                    continue;
                }
                seen.insert(ni, ());
                queue.push(Reverse((nd, ni)));
            }
        }
        // Beyond the front, nearer the goal: the cells the unit cannot stand on, and the
        // features on them (within a footprint and a cell of it).
        let (front_steps, at) = front;
        let reach = spec.footprint.0.max(spec.footprint.1) as i32 / 2 + 2;
        let mut found: Vec<(i64, usize)> = Vec::new();
        for r in 1..=reach {
            for n in ring(at, r) {
                if steps(n).is_none_or(|s| s >= front_steps) || passable(n, None) {
                    continue;
                }
                let (x0, z0) = World::footprint_origin(n, spec.footprint);
                for z in z0..z0 + spec.footprint.1 as i32 {
                    for x in x0..x0 + spec.footprint.0 as i32 {
                        if let Occupant::Feature(f) = world.occupant(x, z)
                            && let Some(feature) = self.features.get(f as usize).filter(|f| f.alive)
                            && !found.iter().any(|&(_, k)| k == f as usize)
                        {
                            let d = (feature.pos - world.anchor_to_pos(at, spec.footprint)).xz().length_sq_raw();
                            found.push((d.min(i64::MAX as i128) as i64, f as usize));
                        }
                    }
                }
            }
        }
        found.sort_unstable();
        found.into_iter().map(|(_, f)| f).collect()
    }
}

/// The eight ways from a cell, in a fixed order.
const NEIGHBOURS: [(i32, i32); 8] = [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (-1, 1), (1, -1), (-1, -1)];

/// The cells `r` from `c` (Chebyshev), in a fixed order (the cell itself for 0).
fn ring(c: (i32, i32), r: i32) -> impl Iterator<Item = (i32, i32)> {
    (-r..=r).flat_map(move |dz| (-r..=r).map(move |dx| (dx, dz))).filter(move |&(dx, dz)| dx.abs() == r || dz.abs() == r).map(move |(dx, dz)| (c.0 + dx, c.1 + dz))
}
