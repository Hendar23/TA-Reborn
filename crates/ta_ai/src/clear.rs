//! Clearing the way: wreckage fallen in a pass (or another feature) that closes the way our
//! units are sent. A unit whose route stops short of where it was sent though the ground itself
//! leads there, or that has stood stalled on its way, has its way searched
//! (`Sim::way_blockers`: by the ground's own way, so the wreckage across a winding pass is found,
//! not the dead end of it nearest the goal). What closes it is reclaimed by a constructor (it pays
//! its metal), the Commander among them when it is near and the place is safe; and the army fires
//! on it when no constructor is on it soon (broken up, a wreck leaves a heap, which blocks
//! nothing). What our units can see of their own ways is all it goes by.

use super::*;

/// How often (ticks) our units' ways are looked at (one search at a look).
const LOOK_TICKS: u64 = 60;
/// A unit that has moved less than this (px) in this long, with somewhere to go at least this far
/// off and nothing to fire at, has stalled on its way.
const STALL_PX: i64 = 32;
const STALL_TICKS: u64 = 8 * SECOND;
const STALL_FAR: i64 = 300;
/// A way searched is not searched again for this long, nor another's from near it (px).
const SEARCHED_TICKS: u64 = 20 * SECOND;
const SEARCHED_NEAR: i64 = 500;
/// Wreckage nobody has been held up by for this long is forgotten.
const FORGET_TICKS: u64 = 90 * SECOND;
/// Constructors go this far (px) to clear the way; the Commander this far, the place safe.
const CLEAR_REACH: i64 = 3500;
const COMMANDER_REACH: i64 = 1400;
/// The army fires on wreckage in its way once it has waited this long for a constructor that is
/// not yet there (at once when none is coming): this many of it, those within this far.
const ARMY_WAIT: u64 = 20 * SECOND;
const GUNNERS: usize = 6;
const ARMY_REACH: i64 = 900;

/// A feature closing our units' way, and the work on it.
#[derive(Clone, Debug)]
pub(crate) struct Blocker {
    pub feature: usize,
    pub pos: FxVec3,
    /// Where those it held up were going (its gunners go on there once it is gone).
    pub goal: FxVec3,
    /// When it was found in the way (those found by one search, the nearest first, are one
    /// jam: the first of them still standing is worked on), and last found holding anyone up.
    pub found: u64,
    pub seen: u64,
    /// The constructor sent to reclaim it, and the units firing on it.
    pub builder: Option<UnitId>,
    pub gunners: Vec<UnitId>,
}

impl SkirmishAi {
    /// Our units' ways, looked at: wreckage gone (reclaimed, broken up) or long in nobody's way
    /// forgotten (its gunners going on where they were going); and every two seconds the way of
    /// a unit stopping short or stalled searched, what closes it kept.
    pub(crate) fn find_blockers(&mut self, sim: &Sim, s: &Survey, out: &mut Vec<Command>) {
        let (now, me) = (s.now, self.player);
        let mut done: Vec<Blocker> = Vec::new();
        self.blockers.retain(|b| {
            let keep = sim.features.get(b.feature).is_some_and(|f| f.alive) && now < b.seen + FORGET_TICKS;
            if !keep {
                done.push(b.clone());
            }
            keep
        });
        for b in done {
            let back: Vec<UnitId> = b.gunners.iter().copied().filter(|&g| sim.unit(g).is_some_and(|u| !u.dead && u.owner == me && Self::firing_on(u, b.pos))).collect();
            if !back.is_empty() {
                log::debug!("{} min: P{me} has the way open at ({}, {}): {} units go on", s.minutes, b.pos.x.to_int(), b.pos.z.to_int(), back.len());
                out.push(Command::Order { player: me, units: back, order: Order::Fight(b.goal), queue: false });
            }
        }
        if s.now % LOOK_TICKS >= THINK_TICKS {
            return;
        }
        self.way_searched.retain(|&(_, until)| now < until);
        // Where each surface unit of the army stood at the last looks (to notice it stalled).
        let mut marks: Vec<(UnitId, FxVec3, u64)> = Vec::with_capacity(s.army.len());
        let mut candidate: Option<(&Unit, FxVec3)> = None;
        for u in &s.army {
            if !matches!(self.info[u.type_id].kind, Locomotion::Ground | Locomotion::Hover) || self.info[u.type_id].spec.is_none() {
                continue;
            }
            // (Where it stood when it last moved, and since when.)
            let last = self.marks.binary_search_by_key(&u.id, |m| m.0).ok().map(|k| self.marks[k]);
            let (mark, since) = match last {
                Some((_, p, t)) if p.within_xz(u.pos, fx(STALL_PX)) => (p, t),
                _ => (u.pos, now),
            };
            marks.push((u.id, mark, since));
            if candidate.is_some() {
                continue;
            }
            // On a way that stops short of where it was sent, though the ground leads there...
            let short = match (u.path_goal, u.move_request) {
                (Some(g), Some(m)) if !u.path_partial && !g.within_xz(m, fx(96)) => Some(m),
                _ => None,
            };
            // ...or stalled on its way somewhere well off, not fighting.
            let stalled = match u.orders.front() {
                Some(&(Order::Fight(p) | Order::Move(p))) if now >= since + STALL_TICKS && !u.pos.within_xz(p, fx(STALL_FAR)) && u.weapons.iter().all(|w| w.target.is_none()) => Some(p),
                _ => None,
            };
            let Some(goal) = short.or(stalled) else { continue };
            if self.way_searched.iter().any(|&(p, _)| p.within_xz(u.pos, fx(SEARCHED_NEAR))) || !self.unit_reaches(sim, u, goal) {
                continue;
            }
            // (Held up by wreckage known already: it is still in the way.)
            if let Some(b) = self.blockers.iter_mut().find(|b| b.pos.within_xz(u.pos, fx(ARMY_REACH * 2)) && b.goal.within_xz(goal, fx(ARMY_REACH))) {
                b.seen = now;
                continue;
            }
            candidate = Some((u, goal));
        }
        marks.sort_unstable_by_key(|m| m.0);
        self.marks = marks;
        let Some((u, goal)) = candidate else { return };
        self.way_searched.push((u.pos, now + SEARCHED_TICKS));
        let Some(spec) = self.info[u.type_id].spec else { return };
        let found: Vec<usize> = sim
            .way_blockers(&spec, u.pos, goal)
            .into_iter()
            .filter(|&f| {
                let d = &sim.data.features[sim.features[f].def];
                d.reclaimable || !d.indestructible
            })
            .take(2)
            .collect();
        for f in found {
            if self.blockers.iter().any(|b| b.feature == f) {
                continue;
            }
            let pos = sim.features[f].pos;
            log::debug!("{} min: P{me} finds {} in its way at ({}, {}), going to ({}, {})", s.minutes, sim.data.features[sim.features[f].def].name, pos.x.to_int(), pos.z.to_int(), goal.x.to_int(), goal.z.to_int());
            self.blockers.push(Blocker { feature: f, pos, goal, found: now, seen: now, builder: None, gunners: Vec::new() });
        }
    }

    /// Whether a unit is firing on the ground at `at` (by our order: a wreck there).
    fn firing_on(u: &Unit, at: FxVec3) -> bool {
        matches!(u.orders.front(), Some(&Order::Attack(Target::Ground(p))) if p == at)
    }

    /// The wreck worked on in each jam: the first of those found together still standing.
    fn jam_fronts(&self) -> Vec<usize> {
        let mut jams: Vec<u64> = Vec::new();
        let mut fronts = Vec::new();
        for (k, b) in self.blockers.iter().enumerate() {
            if !jams.contains(&b.found) {
                jams.push(b.found);
                fronts.push(k);
            }
        }
        fronts
    }

    /// A constructor for each wreck in our way that none is reclaiming: free hands first (idle, or
    /// helping at a factory), then one whose job has not begun, the nearest that can get there;
    /// the Commander when it is near, in good health and the place is safe. Not into a fight (no
    /// armed enemy seen near the wreck).
    pub(crate) fn send_clearers(&mut self, sim: &Sim, s: &Survey, out: &mut Vec<Command>) {
        let me = self.player;
        let ordered = ordered_units(out);
        let mut taken: Vec<UnitId> = Vec::new();
        for k in self.jam_fronts() {
            let (f, at) = (self.blockers[k].feature, self.blockers[k].pos);
            if let Some(b) = self.blockers[k].builder
                && sim.unit(b).is_some_and(|u| !u.dead && u.owner == me && u.orders.iter().any(|o| *o == Order::Reclaim(f)))
            {
                taken.push(b);
                continue;
            }
            self.blockers[k].builder = None;
            if !sim.data.features[sim.features[f].def].reclaimable || self.unreachable_feature(f) || self.seen_enemies_near(s, at, 600) > 0 {
                continue;
            }
            let pick = s
                .builders
                .iter()
                .filter(|b| !ordered.contains(&b.id) && !taken.contains(&b.id) && sim.unit_type(b).def.can_reclaim && !self.upgrades.iter().any(|u| u.builder == b.id))
                .filter(|b| {
                    let d = dist(b.pos, at);
                    if self.info[b.type_id].commander {
                        d <= COMMANDER_REACH && s.minutes >= 2 && s.now >= self.retreat_until && b.health * 10 >= sim.unit_type(b).max_health * 6
                    } else {
                        d <= CLEAR_REACH
                    }
                })
                // (A constructor at work on a building is not taken from it.)
                .filter(|b| b.build.is_none() || matches!(b.orders.front(), Some(Order::Guard(_))))
                .filter(|b| !self.boxed_in(b.id, b.pos) && self.unit_reaches(sim, b, at))
                .min_by_key(|b| {
                    let busy = !matches!(b.orders.front(), None | Some(Order::Guard(_)));
                    (busy, dist(b.pos, at), b.id.slot)
                });
            let Some(b) = pick else { continue };
            log::debug!("{} min: P{me} sends its {} to clear {} out of the way", s.minutes, sim.unit_type(b).name, sim.data.features[sim.features[f].def].name);
            taken.push(b.id);
            self.blockers[k].builder = Some(b.id);
            out.push(Command::Order { player: me, units: vec![b.id], order: Order::Reclaim(f), queue: false });
            self.issued_reclaims.push((b.id, f, s.now));
        }
    }

    /// The army fires on wreckage in its way that no constructor will clear soon: the nearest
    /// few of its armed surface units (each then goes on where it was going once the wreck is
    /// gone: see `find_blockers`).
    pub(crate) fn fire_on_blockers(&mut self, sim: &Sim, s: &Survey, out: &mut Vec<Command>) {
        let me = self.player;
        let ordered = ordered_units(out);
        // (A unit fires on one wreck at a time.)
        let mut taken: Vec<UnitId> = Vec::new();
        for k in self.jam_fronts() {
            let b = &self.blockers[k];
            let at = b.pos;
            let mut gunners: Vec<UnitId> = b.gunners.iter().copied().filter(|&g| !taken.contains(&g) && sim.unit(g).is_some_and(|u| !u.dead && u.owner == me && Self::firing_on(u, at))).collect();
            let coming = b.builder.and_then(|id| sim.unit(id)).filter(|u| !u.dead);
            let wait = coming.is_some_and(|u| u.pos.within_xz(at, fx(600))) || coming.is_some() && s.now < b.found + ARMY_WAIT;
            if !wait && gunners.len() < GUNNERS && !sim.data.features[sim.features[b.feature].def].indestructible {
                let mut near: Vec<(i64, UnitId)> = s
                    .army
                    .iter()
                    .filter(|u| matches!(self.info[u.type_id].kind, Locomotion::Ground | Locomotion::Hover) && self.info[u.type_id].ground_dps > 0)
                    .filter(|u| !ordered.contains(&u.id) && !taken.contains(&u.id) && !gunners.contains(&u.id) && u.pos.within_xz(at, fx(ARMY_REACH)) && !self.repairing.iter().any(|r| r.0 == u.id))
                    .filter(|u| u.weapons.iter().all(|w| w.target.is_none()) && self.unit_reaches(sim, u, at))
                    .map(|u| (dist(u.pos, at), u.id))
                    .collect();
                near.sort_unstable_by_key(|&(d, id)| (d, id.slot));
                let more: Vec<UnitId> = near.into_iter().take(GUNNERS - gunners.len()).map(|(_, id)| id).collect();
                if !more.is_empty() {
                    log::debug!("{} min: P{me} fires on the wreck in its way at ({}, {}) with {} units", s.minutes, at.x.to_int(), at.z.to_int(), more.len());
                    out.push(Command::Order { player: me, units: more.clone(), order: Order::Attack(Target::Ground(at)), queue: false });
                    gunners.extend(more);
                }
            }
            taken.extend(gunners.iter().copied());
            self.blockers[k].gunners = gunners;
        }
    }
}
