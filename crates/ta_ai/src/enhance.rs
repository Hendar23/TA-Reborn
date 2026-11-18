//! Enhancements (`Order::Enhance`: Supreme Commander's commanders'): the computer players'
//! commanders take the engineering suites as their side climbs the tiers (their menus grow with
//! what the suites bring), a weapon upgrade once the economy allows, and more health when rich.
//! One at a time, out of danger, into a slot that is empty or holds the one it builds on (never
//! taking out another). TA's own commanders have none: nothing here happens in its games.

use ta_sim::{Command, EnhancementFit as Fit, Fx, Order, Sim, Unit};

use super::{SkirmishAi, Survey, fx, ordered_units};

/// What an enhancement is to a computer player.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Want {
    /// It adds to the menu (an engineering suite).
    Engineering,
    /// It swaps a weapon for one that reaches farther, hits harder or fires faster, or arms an
    /// empty slot with one of its own (not a missile to build).
    Gun,
    /// It adds health or mending, and nothing else wanted.
    Armour,
}

impl SkirmishAi {
    /// What `index` of the commander's type is to us, if anything.
    fn want_of(&self, sim: &Sim, c: &Unit, index: u16) -> Option<Want> {
        let t = sim.unit_type(c);
        let e = t.enhancements.get(index as usize)?;
        if !e.build_options.is_empty() {
            return Some(Want::Engineering);
        }
        let better = (0..e.weapons.len()).any(|slot| {
            let Some(new) = e.weapons[slot].map(|w| sim.weapon_types.get(w)) else { return false };
            if new.command_fire {
                return false;
            }
            match t.weapons[slot].map(|w| sim.weapon_types.get(w)) {
                Some(old) => new.range > old.range || new.default_damage > old.default_damage || new.reload_ticks < old.reload_ticks,
                None => !new.def.stockpile,
            }
        });
        if better {
            return Some(Want::Gun);
        }
        (e.max_health > Fx::ZERO || e.heal_rate > Fx::ZERO).then_some(Want::Armour)
    }

    /// This think's enhancement for our commander, if one is due: standing at home, nothing else
    /// under way, the economy as the tiers' climb asks for the suites, a little later for a gun,
    /// rich for armour.
    pub(crate) fn run_enhancements(&mut self, sim: &Sim, s: &Survey, out: &mut Vec<Command>) {
        let Some(c) = s.commander else { return };
        let t = sim.unit_type(c);
        if t.enhancements.is_empty() || c.self_work.is_some() || matches!(c.orders.front(), Some(Order::Enhance { .. })) {
            return;
        }
        let me = self.player;
        if ordered_units(out).contains(&c.id) || s.now < self.retreat_until || self.enemies_near(s, c.pos, 600) > 0 || !c.pos.within_xz(self.base, fx(1200)) {
            return;
        }
        let tech_economy = s.minutes >= self.p.tech_minute && (s.m_inc >= fx(self.p.tech_income) || s.metal >= fx(1200));
        let rich = self.metal_avg > Fx::from_ratio(5, 10) && s.m_inc >= fx(self.p.tech_income * 2);
        let mut best: Option<(u8, u16)> = None;
        for index in 0..t.enhancements.len() as u16 {
            let e = &t.enhancements[index as usize];
            // Into an empty slot, or over the one it builds on.
            let Fit::Free { replaces } = sim.enhancement_fit(c, index) else { continue };
            if replaces.is_some() && replaces != e.prerequisite {
                continue;
            }
            let Some(want) = self.want_of(sim, c, index) else { continue };
            // The suite past the first once our factories are of its tier (or we are rich).
            let deeper = e.prerequisite.is_some();
            let due = match want {
                Want::Engineering => tech_economy && (!deeper || s.factory_tier >= 2 || rich),
                Want::Gun => tech_economy && s.minutes >= self.p.tech_minute + 2,
                Want::Armour => rich && s.minutes >= self.p.tech_minute + 6,
            };
            // Something to pay with: a part of its price banked, or an income for it (its energy
            // within a minute's).
            let affordable = (s.metal * 2 >= e.cost_metal || s.m_inc >= fx(self.p.tech_income * 2)) && (s.energy * 2 >= e.cost_energy || s.e_inc * 60 >= e.cost_energy);
            if !due || !affordable {
                continue;
            }
            let rank = match want {
                Want::Engineering => 0,
                Want::Gun => 1,
                Want::Armour => 2,
            };
            if best.is_none_or(|(r, i)| (rank, index) < (r, i)) {
                best = Some((rank, index));
            }
        }
        if let Some((_, index)) = best {
            log::debug!("{} min: P{me} has its commander install {}", s.minutes, t.def.enhancements.get(index as usize).map_or("?", |e| e.name.as_str()));
            out.push(Command::Order { player: me, units: vec![c.id], order: Order::Enhance { index, remove: false }, queue: false });
        }
    }
}
