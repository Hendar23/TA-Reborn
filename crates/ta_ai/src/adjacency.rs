//! Adjacency (Supreme Commander's: what a building gives the buildings of its player it touches,
//! `ta_sim`'s `sim/adjacency.rs`): the computer players put a power plant whose bonuses reach a
//! factory of theirs beside it (what building costs it), and storage whose bonuses raise what a
//! building of theirs makes beside it (the extractors' metal, the plants' power) where the more it
//! makes pays for it within [`PAYBACK`]. TA's own units give nothing: nothing here happens in its
//! games.

use ta_sim::world::{Occupant, World};
use ta_sim::{AdjacencyAffect, Fx, FxVec3, Sim, Unit, UnitId};

use super::info::Role;
use super::{Plan, SkirmishAi, Survey, Task, dist, fx};

/// Seconds the more a building makes beside storage must pay for the storage in: eight minutes
/// (Forged Alliance's storage beside a tech 2 extractor pays in about five, beside a tech 1 one
/// in fifteen).
const PAYBACK: i64 = 480;

/// A footprint: its top-left cell and the cell past its bottom-right (x0, z0, x1, z1).
type Bounds = (i32, i32, i32, i32);

fn bounds_of(sim: &Sim, type_id: usize, pos: FxVec3) -> Bounds {
    let fp = sim.types.get(type_id).footprint;
    let (x, z) = World::footprint_origin(World::pos_to_anchor(pos, fp), fp);
    (x, z, x + fp.0 as i32, z + fp.1 as i32)
}

/// How far two footprints share an edge (`ta_sim`'s rule: along an edge, a corner alone none).
fn contact(a: Bounds, b: Bounds) -> i32 {
    let overlap = |a0: i32, a1: i32, b0: i32, b1: i32| (a1.min(b1) - a0.max(b0)).max(0);
    if a.2 == b.0 || b.2 == a.0 {
        overlap(a.1, a.3, b.1, b.3)
    } else if a.3 == b.1 || b.3 == a.1 {
        overlap(a.0, a.2, b.0, b.2)
    } else {
        0
    }
}

impl SkirmishAi {
    /// A site for a building of `type_id` touching `target` (a finished building of ours): beside
    /// it along the most of its edge, the nearest to the builder of those, as well as beside what
    /// it upgrades into (the ground its upgrades take kept for them); off deposits not yet mined
    /// and vents, out of a factory's way out, reachable, not planned for something else or under
    /// fire, and with the lane round the block it makes with `target` (and what touches that
    /// already) kept free.
    pub(crate) fn site_beside(&self, sim: &Sim, b: &Unit, type_id: usize, target: &Unit, plan: &Plan) -> Option<FxVec3> {
        let t = sim.types.get(type_id);
        let fp = t.footprint;
        let now = bounds_of(sim, target.type_id, target.pos);
        // (What it upgrades into may stand on more of the ground.)
        let mut room = now;
        for to in sim.types.upgrade_chain(target.type_id) {
            let o = bounds_of(sim, to, target.pos);
            room = (room.0.min(o.0), room.1.min(o.1), room.2.max(o.2), room.3.max(o.3));
        }
        // The block: the target and the buildings touching it.
        let mut block: Vec<u32> = vec![target.id.slot];
        let (x0, z0, x1, z1) = room;
        for x in x0..x1 {
            for z in [z0 - 1, z1] {
                block.extend(sim.world.building_at(x, z));
            }
        }
        for z in z0..z1 {
            for x in [x0 - 1, x1] {
                block.extend(sim.world.building_at(x, z));
            }
        }
        let (w, h) = (fp.0 as i32, fp.1 as i32);
        let mut sites: Vec<(i32, i32, i32)> = Vec::new();
        for z in z0 - h + 1..z1 {
            sites.push((x0 - w, z, 0));
            sites.push((x1, z, 0));
        }
        for x in x0 - w + 1..x1 {
            sites.push((x, z0 - h, 0));
            sites.push((x, z1, 0));
        }
        let r = fx(t.radius.to_int() + 16);
        let mut best: Option<(i32, i64, i32, i32, FxVec3)> = None;
        for (x, z, _) in sites {
            let here = (x, z, x + w, z + h);
            // (Beside it now, and clear of the ground its upgrades take.)
            let along = contact(here, now);
            if along <= 0 || contact(here, room) <= 0 {
                continue;
            }
            let anchor = (x + w / 2, z + h / 2);
            let pos = sim.world.anchor_to_pos(anchor, fp);
            let d = dist(pos, b.pos);
            if best.is_some_and(|(ba, bd, ..)| (along, -d) <= (ba, -bd)) {
                continue;
            }
            if !sim.can_place(type_id, pos) || self.covers_free_resource(pos, fp) || self.blocks_exit(pos, fp) || !Self::lane_free_beside(sim, anchor, fp, &block) {
                continue;
            }
            if !plan.free(pos, r) || self.threat.at(pos) != 0 || self.failed.iter().any(|(ft, p, _)| *ft == type_id && p.within_xz(pos, fx(32))) || !self.builder_reaches(sim, b, pos, type_id) {
                continue;
            }
            best = Some((along, d, x, z, pos));
        }
        best.map(|(.., pos)| pos)
    }

    /// Whether a footprint would cover (or come near) a deposit no extractor stands on yet, or a
    /// vent: one mined (the extractor storage goes beside) is no ground to keep free.
    fn covers_free_resource(&self, pos: FxVec3, fp: (u32, u32)) -> bool {
        let r = fx(fp.0.max(fp.1) as i64 * 8 + 40);
        self.deposits.iter().any(|d| d.marked && d.owner.is_none() && d.pos.within_xz(pos, r)) || self.vents.iter().any(|v| v.pos.within_xz(pos, r))
    }

    /// [`SkirmishAi::lane_free`] with the buildings of `block` (one it is put beside, and those
    /// beside that) allowed in the lane: the block keeps the lane round it.
    fn lane_free_beside(sim: &Sim, anchor: (i32, i32), fp: (u32, u32), block: &[u32]) -> bool {
        const LANE: i32 = 3;
        let o = World::footprint_origin(anchor, fp);
        let (w, h) = (fp.0 as i32, fp.1 as i32);
        for z in o.1 - LANE..o.1 + h + LANE {
            for x in o.0 - LANE..o.0 + w + LANE {
                let inside = x >= o.0 && x < o.0 + w && z >= o.1 && z < o.1 + h;
                if inside {
                    continue;
                }
                if let Occupant::Unit(slot) = sim.world.occupant(x, z)
                    && !block.contains(&slot)
                {
                    return false;
                }
            }
        }
        true
    }

    /// A power plant of type `g` beside a factory of ours its bonuses reach (what building costs
    /// it), the factory nearest the builder first: none where none does, or no site beside one is
    /// found near the builder.
    pub(crate) fn plant_beside_factory(&self, sim: &Sim, s: &Survey, b: &Unit, g: usize, plan: &Plan) -> Option<FxVec3> {
        if sim.types.get(g).adjacency.is_empty() {
            return None;
        }
        let build = 1u8 << AdjacencyAffect::BuildEnergy.index() | 1u8 << AdjacencyAffect::BuildMetal.index();
        let mut factories: Vec<&&Unit> = s.factories.iter().filter(|f| sim.adjacency_bonus(g, f).1 & build != 0 && !full(sim, f)).collect();
        factories.sort_by_key(|f| (dist(f.pos, b.pos), f.id.slot));
        factories.into_iter().take(3).find_map(|f| self.site_beside(sim, b, g, f, plan)).filter(|&p| dist(p, b.pos) <= 900)
    }

    /// Storage beside a building of ours whose making it raises (the extractors' metal, the
    /// plants' power: Forged Alliance's storage round them), where what it raises pays for it
    /// within [`PAYBACK`] (metal and a sixtieth of the energy, as the computer players weigh a
    /// price): the best of them, near home and out of danger.
    pub(crate) fn storage_beside_task(&self, sim: &Sim, s: &Survey, b: &Unit, opts: &[usize], plan: &Plan) -> Option<Task> {
        let stores: Vec<usize> = opts.iter().copied().filter(|&o| self.info[o].role == Role::Storage && !sim.types.get(o).adjacency.is_empty()).collect();
        if stores.is_empty() {
            return None;
        }
        let mut best: Option<(i64, u32, usize, UnitId)> = None;
        for u in s.own.iter().filter(|u| u.is_complete() && matches!(self.info[u.type_id].role, Role::Extractor | Role::Generator | Role::MetalMaker)) {
            if full(sim, u) || dist(u.pos, self.base) > 1800 || self.threat.at(u.pos) != 0 {
                continue;
            }
            let made = sim.unit_flows(u);
            for &st in &stores {
                let (adds, _) = sim.adjacency_bonus(st, u);
                // (Metal a second, and energy's sixtieth: what it raises is worth.)
                let gain = made.metal_make * adds[AdjacencyAffect::MetalMake.index()] + made.energy_make * adds[AdjacencyAffect::EnergyMake.index()] / 60;
                if gain <= Fx::ZERO {
                    continue;
                }
                let price = self.info[st].cost;
                if gain * PAYBACK < fx(price) {
                    continue;
                }
                // (The most it raises for its price.)
                let worth = (gain * 1000).to_int() / price.max(1);
                if best.is_none_or(|(w, slot, ..)| (worth, std::cmp::Reverse(u.id.slot)) > (w, std::cmp::Reverse(slot))) {
                    best = Some((worth, u.id.slot, st, u.id));
                }
            }
        }
        let (_, _, st, target) = best?;
        let target = sim.unit(target)?;
        self.site_beside(sim, b, st, target, plan).filter(|&p| dist(p, b.pos) <= 1200).map(|p| Task::Build(st, p))
    }
}

/// Whether a building takes no more neighbours' bonuses (as many as its size class holds give
/// it some already).
fn full(sim: &Sim, u: &Unit) -> bool {
    let most = sim.types.get(u.type_id).adjacency_size as usize;
    most > 0 && u.adjacency.as_ref().is_some_and(|a| a.from.len() >= most)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn footprints_touch_as_the_simulation_has_it() {
        assert_eq!(contact((10, 10, 14, 14), (14, 11, 16, 13)), 2);
        assert_eq!(contact((10, 10, 14, 14), (14, 14, 16, 16)), 0, "a corner alone");
        assert_eq!(contact((10, 10, 14, 14), (15, 11, 17, 13)), 0, "a cell apart");
        assert_eq!(contact((10, 10, 14, 14), (9, 8, 11, 10)), 1);
    }
}
