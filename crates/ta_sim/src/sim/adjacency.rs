//! Adjacency (Supreme Commander's; a unit type's `[ADJACENCY]` and `AdjacencySize`): what the
//! finished buildings of a player give those of its own they touch. Two buildings touch where their
//! footprints meet along an edge, a cell of it at least (corners alone do not touch, nor buildings a
//! cell apart; one of another player's gives nothing). Each bonus of a neighbour's that a building
//! takes (its `Size` the building's size class, or none; every token of its `Category` in the
//! building's; and something of the building's for it to change: its build power or missiles for
//! what building costs it, its upkeep, its weapons' shots and reloads, what it makes) adds its share
//! to one of the building's figures, summed over its neighbours as Forged Alliance sums its buffs:
//! the figure times one and the shares, never below none (`unit::Adjacency`). A building takes
//! bonuses from at most as many neighbours as its size class says (`AdjacencySize`: as many of the
//! smallest structures as fit round it in Forged Alliance, whose wider footprints keep that many
//! there), those touching it along the most of its edge first.
//!
//! What it changes is paid where the economy pays: what building costs a builder (construction,
//! an upgrade in place, a stockpiled missile, `Sim::apply_build_power`, `Sim::work_on_self`,
//! `Sim::update_stockpile`), upkeep and what is made (`Sim::economy_end`), a shot's energy
//! (`Sim::shot_energy`) and a reload (`Sim::fire`); the figures the interface shows say the same
//! (`flows.rs`). A building's bonuses are found again whenever a building touching it, or it, is
//! finished, dies, is upgraded or changes hands: at the economy's end, what stands otherwise than
//! when last looked at is found (`Unit::adjacency_key`), it and those touching it looked at again,
//! in slot order. With no type giving any bonus (TA's own games), none of it runs.

use ta_data::AdjacencyAffect;

use super::*;
use crate::unit::{ADJACENCY_AFFECTS, Adjacency};

/// A footprint: its top-left cell and its size in cells.
pub type Rect = ((i32, i32), (u32, u32));

/// How far two footprints share an edge, in cells: none where they do not touch along one (they
/// overlap, meet at a corner alone, or stand apart).
pub fn contact(a: Rect, b: Rect) -> u32 {
    let ((ax, az), (aw, ah)) = a;
    let ((bx, bz), (bw, bh)) = b;
    let (ax1, az1, bx1, bz1) = (ax + aw as i32, az + ah as i32, bx + bw as i32, bz + bh as i32);
    let overlap = |a0: i32, a1: i32, b0: i32, b1: i32| (a1.min(b1) - a0.max(b0)).max(0) as u32;
    if ax1 == bx || bx1 == ax {
        overlap(az, az1, bz, bz1)
    } else if az1 == bz || bz1 == az {
        overlap(ax, ax1, bx, bx1)
    } else {
        0
    }
}

impl Sim {
    /// Whether a building of type `t` (the unit `u`, where one stands) has something an affect
    /// changes: build power or a stockpiled missile for what building costs it, an upkeep, a weapon
    /// whose shots cost energy, a weapon to reload, energy or metal it makes.
    pub(crate) fn takes_affect(&self, t: &UnitType, u: Option<&Unit>, affect: AdjacencyAffect) -> bool {
        let slots = u.map_or(&t.weapons, |u| u.weapon_slots(t));
        match affect {
            AdjacencyAffect::BuildEnergy | AdjacencyAffect::BuildMetal => u.map_or(t.work_rate, |u| u.work_rate(t)) > 0 || u.map_or(t.stockpile, |u| u.stockpiles(t)),
            AdjacencyAffect::EnergyUse => t.energy_use > Fx::ZERO,
            AdjacencyAffect::EnergyPerShot => slots.iter().flatten().any(|&w| {
                let w = self.weapon_types.get(w);
                !w.def.stockpile && w.energy_per_shot > Fx::ZERO
            }),
            AdjacencyAffect::ReloadTime => slots.iter().any(Option::is_some),
            AdjacencyAffect::EnergyMake => t.energy_make > Fx::ZERO || t.wind_generator > Fx::ZERO || t.tidal_generator > Fx::ZERO || t.energy_use < Fx::ZERO,
            AdjacencyAffect::MetalMake => t.metal_make > Fx::ZERO || t.extracts_metal > Fx::ZERO || t.makes_metal > Fx::ZERO,
        }
    }

    /// The finished buildings touching a footprint (`skip`: the slot of the building on it, left
    /// out), each with its slot, its player and how far it shares an edge with it, in slot order.
    pub(crate) fn touching(&self, rect: Rect, skip: u32) -> Vec<(u32, u8, u32)> {
        let ((x0, z0), (w, h)) = rect;
        let (x1, z1) = (x0 + w as i32, z0 + h as i32);
        let mut found: Vec<u32> = Vec::new();
        for x in x0..x1 {
            for z in [z0 - 1, z1] {
                found.extend(self.world.building_at(x, z));
            }
        }
        for z in z0..z1 {
            for x in [x0 - 1, x1] {
                found.extend(self.world.building_at(x, z));
            }
        }
        found.sort_unstable();
        found.dedup();
        found
            .into_iter()
            .filter(|&s| s != skip)
            .filter_map(|s| {
                let (owner, origin, fp) = self.units.get(s as usize)?.as_ref()?.adjacency_key?;
                Some((s, owner, contact(rect, (origin, fp)))).filter(|&(_, _, c)| c > 0)
            })
            .collect()
    }

    /// What the bonuses of a building of type `gt` give one of type `t` (the unit `u`, where one
    /// stands) beside it: the share each figure changes by, and a bit for each figure changed.
    pub(crate) fn bonuses_for(&self, gt: &UnitType, t: &UnitType, u: Option<&Unit>) -> ([Fx; ADJACENCY_AFFECTS], u8) {
        let mut adds = [Fx::ZERO; ADJACENCY_AFFECTS];
        let mut bits = 0u8;
        for b in &gt.adjacency {
            if b.size != 0 && b.size != t.adjacency_size || !b.category.is_some_and(|c| t.category_mask.contains_all(&c)) || !self.takes_affect(t, u, b.affect) {
                continue;
            }
            adds[b.affect.index()] += b.add;
            bits |= 1 << b.affect.index();
        }
        (adds, bits)
    }

    /// What a building of type `t` (the unit `u`, where one stands) of `owner`'s on the footprint
    /// `rect` takes from the finished buildings of its player touching it (`skip`: its own slot):
    /// none where it takes nothing.
    pub(crate) fn adjacency_taken(&self, t: &UnitType, u: Option<&Unit>, owner: u8, rect: Rect, skip: u32) -> Option<Adjacency> {
        let mut givers: Vec<(u32, u32, [Fx; ADJACENCY_AFFECTS], u8)> = Vec::new();
        for (slot, neighbour, contact) in self.touching(rect, skip) {
            let Some(g) = self.units[slot as usize].as_ref().filter(|_| neighbour == owner) else { continue };
            let gt = self.types.get(g.type_id);
            if gt.adjacency.is_empty() {
                continue;
            }
            let (adds, bits) = self.bonuses_for(gt, t, u);
            if bits != 0 {
                givers.push((slot, contact, adds, bits));
            }
        }
        // (No more neighbours than its size class holds: those along the most of its edge first.)
        let most = t.adjacency_size as usize;
        if most > 0 && givers.len() > most {
            givers.sort_by_key(|&(slot, contact, ..)| (std::cmp::Reverse(contact), slot));
            givers.truncate(most);
            givers.sort_by_key(|&(slot, ..)| slot);
        }
        if givers.is_empty() {
            return None;
        }
        let mut a = Adjacency::default();
        for (slot, _, adds, bits) in givers {
            for (c, add) in a.change.iter_mut().zip(adds) {
                *c += add;
            }
            let Some(id) = self.units[slot as usize].as_ref().map(|g| g.id) else { continue };
            a.from.push((id, bits));
        }
        Some(a)
    }

    /// Adjacency brought up to date (at the economy's end, before what the buildings make and pay
    /// is counted): the buildings standing otherwise than when last looked at (finished, dead,
    /// taken off the map, upgraded, changed hands) found, and their bonuses and those of the
    /// buildings touching them, where they stood and where they stand, found again in slot order.
    pub(super) fn update_adjacency(&mut self) {
        if !self.types.adjacency {
            return;
        }
        let types = self.types.clone();
        let mut around: Vec<Rect> = Vec::new();
        let mut dirty: Vec<u32> = Vec::new();
        for (slot, u) in self.units.iter_mut().enumerate() {
            let Some(u) = u else { continue };
            let t = types.get(u.type_id);
            let now = (t.locomotion == Locomotion::Static && u.is_complete() && !u.dead && u.carried_by.is_none()).then_some(u.occupied_origin).flatten().map(|origin| (u.owner, origin, t.footprint));
            if now != u.adjacency_key {
                around.extend(u.adjacency_key.iter().chain(now.iter()).map(|&(_, origin, fp)| (origin, fp)));
                u.adjacency_key = now;
                dirty.push(slot as u32);
            }
        }
        if dirty.is_empty() {
            return;
        }
        for rect in around {
            dirty.extend(self.touching(rect, u32::MAX).into_iter().map(|(slot, ..)| slot));
        }
        dirty.sort_unstable();
        dirty.dedup();
        for slot in dirty {
            let Some(u) = self.units[slot as usize].as_ref() else { continue };
            let taken = u.adjacency_key.and_then(|(owner, origin, fp)| self.adjacency_taken(types.get(u.type_id), Some(u), owner, (origin, fp), slot));
            if let Some(u) = self.units[slot as usize].as_mut() {
                u.adjacency = taken.map(Box::new);
            }
        }
    }

    /// What a building of type `giver` would give `receiver` beside it: the share each of its
    /// figures would change by, and a bit for each figure changed (the computer players weigh where
    /// to build so).
    pub fn adjacency_bonus(&self, giver: usize, receiver: &Unit) -> ([Fx; ADJACENCY_AFFECTS], u8) {
        self.bonuses_for(self.types.get(giver), self.types.get(receiver.type_id), Some(receiver))
    }

    /// The finished buildings of its player a building gives something to (the unit `u`, of type
    /// `t`), each with the figures it changes there (a bit an affect): those whose bonuses list it.
    pub fn adjacency_given(&self, u: &Unit) -> Vec<(UnitId, u8)> {
        let Some((_, origin, fp)) = u.adjacency_key else { return Vec::new() };
        self.touching((origin, fp), u.id.slot)
            .into_iter()
            .filter_map(|(slot, ..)| {
                let n = self.units[slot as usize].as_ref()?;
                let bits = n.adjacency.as_ref()?.from.iter().find(|(g, _)| *g == u.id)?.1;
                Some((n.id, bits))
            })
            .collect()
    }

    /// What a building of type `type_id` of `owner`'s put at `pos` would take from the finished
    /// buildings of its player it touched, and give them (each with the figures it would change,
    /// a bit an affect): what placing it shows.
    pub fn adjacency_preview(&self, type_id: usize, pos: FxVec3, owner: u8) -> (Option<Adjacency>, Vec<(UnitId, u8)>) {
        if !self.types.adjacency {
            return (None, Vec::new());
        }
        let t = self.types.get(type_id);
        let rect = (World::footprint_origin(World::pos_to_anchor(pos, t.footprint), t.footprint), t.footprint);
        let taken = self.adjacency_taken(t, None, owner, rect, u32::MAX);
        let mut given = Vec::new();
        if !t.adjacency.is_empty() {
            for (slot, neighbour, _) in self.touching(rect, u32::MAX) {
                let Some(n) = self.units[slot as usize].as_ref().filter(|_| neighbour == owner) else { continue };
                let (_, bits) = self.bonuses_for(t, self.types.get(n.type_id), Some(n));
                if bits != 0 {
                    given.push((n.id, bits));
                }
            }
        }
        (taken, given)
    }
}

/// A figure scaled by a factor of adjacency's, where it has one.
#[inline]
pub(crate) fn scaled(amount: Fx, factor: Option<Fx>) -> Fx {
    match factor {
        Some(f) => amount * f,
        None => amount,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn footprints_touch_along_an_edge() {
        let a = ((10, 10), (4, 4));
        assert_eq!(contact(a, ((14, 11), (2, 2))), 2, "beside its east edge");
        assert_eq!(contact(a, ((8, 9), (2, 2))), 1, "beside its west edge, a cell of it");
        assert_eq!(contact(a, ((11, 6), (2, 4))), 2, "above it");
        assert_eq!(contact(a, ((14, 14), (2, 2))), 0, "a corner alone");
        assert_eq!(contact(a, ((15, 11), (2, 2))), 0, "a cell apart");
        assert_eq!(contact(a, ((12, 12), (4, 4))), 0, "overlapping");
        assert_eq!(contact(a, ((6, 4), (20, 6))), 4, "along all of its north edge");
    }
}
