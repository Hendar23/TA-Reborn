//! Projects: the big buildings advanced constructors put up one at a time, the others helping to
//! finish them: anti-nuclear cover once missiles may be coming, nuclear silos, heavy defences on
//! the approach, long-range guns in reach of the enemy's base, experimentals (or a factory of
//! them); each once the economy can feed it, as the difficulty and the persona have it (Easy
//! builds none but an anti-nuclear defence against a silo it has seen). And the long-range guns
//! fired at what we know of the enemy's base: a building's guns fire of themselves only at what
//! their side sees.

use super::*;

/// A defence costing this much (metal and energy / 60) or more is a heavy one (TA's Annihilator,
/// Doomsday Machine, Guardian, Punisher).
pub(crate) const HEAVY_COST: i64 = 1500;
/// How long (minutes) of our metal income a project may cost: an experimental, a long-range gun,
/// anything else.
const EXPERIMENTAL_INCOME_MINUTES: i64 = 7;
const LONG_GUN_INCOME_MINUTES: i64 = 5;
/// How often (ticks) the long-range guns are looked at, and how long one keeps its target
/// before it turns to the next (what it shells may be gone unseen).
const GUNS_LOOK: u64 = 10 * SECOND;
const GUN_TARGET_TICKS: u64 = 60 * SECOND;

/// A kind of project taking turns with the others (anti-nuclear cover comes first; an
/// experimental goes up beside them).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Project {
    Silo,
    HeavyTower,
    LongGun,
}

impl SkirmishAi {
    /// Whether a unit type is a project (see the module's notes).
    pub(crate) fn is_project(&self, t: usize) -> bool {
        let i = &self.info[t];
        matches!(i.role, Role::Silo | Role::AntiNuke) || i.long_range || self.is_experimental_project(t) || i.role == Role::Defense && i.cost >= HEAVY_COST
    }

    /// Whether a unit type is an experimental, or a factory of them: a project of its own, put
    /// up beside the others (by the constructors that build them, often few).
    pub(crate) fn is_experimental_project(&self, t: usize) -> bool {
        self.info[t].experimental || self.info[t].experimental_factory
    }

    /// An advanced constructor's project, if one is due (none while another is under way, but an
    /// anti-nuclear defence against an enemy silo seen; an experimental goes up beside another).
    pub(crate) fn project_task(&mut self, sim: &Sim, s: &Survey, b: &Unit, opts: &[usize], plan: &Plan) -> Option<Task> {
        let (m, e) = (s.m_inc.to_int(), s.e_inc.to_int());
        let frames = |experimental: bool| s.nanoframes.iter().any(|n| self.is_project(n.type_id) && self.is_experimental_project(n.type_id) == experimental);
        let under_way = plan.project || frames(false);
        // (Not on an economy short of power or metal: what it has goes to that first. An
        // anti-nuclear defence against a silo seen excepted.)
        let ledger = self.ledger(s);
        let short = ledger.e_low || ledger.metal_starved;
        // Experimentals, one at a time beside the other projects: one the constructor builds
        // itself (Forged Alliance's), or a factory of them (TA's Gantry).
        if !plan.experimental && !frames(true) && !short && s.minutes >= self.p.experimental_minute && m >= 30 && e >= 1000 && s.experimentals < self.p.experimentals
            && let Some(t) = self.experimental_task(sim, s, b, opts, plan)
        {
            return Some(t);
        }
        // Anti-nuclear cover: against a silo seen, as many as the level keeps; else one once
        // missiles may be coming (late in the game, on an economy that could build them too).
        let antis = s.count[Role::AntiNuke as usize] + plan.pending[Role::AntiNuke as usize];
        let wanted = if self.enemy_silo {
            self.p.anti_nukes.max(1)
        } else if s.minutes >= self.p.anti_minute && e >= 600 && m >= 15 {
            self.p.anti_nukes.min(1)
        } else {
            0
        };
        let anti_frame = s.nanoframes.iter().any(|n| self.info[n.type_id].role == Role::AntiNuke) || plan.pending[Role::AntiNuke as usize] > 0;
        if antis < wanted && (!under_way && !short || self.enemy_silo && !anti_frame) {
            let anti = opts.iter().copied().filter(|&o| self.info[o].role == Role::AntiNuke && self.info[o].land).min_by_key(|&o| (self.info[o].cost, o));
            if let Some(a) = anti && let Some(site) = self.place(sim, b, a, Zone::Ring, 60, plan) {
                return Some(Task::Build(a, site));
            }
        }
        if under_way || short {
            return None;
        }
        // The projects due, each kind in its turn: the kind with the fewest of its own built
        // first (one of each before a second of any), in the persona's order of liking.
        let silos = if s.minutes >= self.p.nuke_minute.saturating_add(16) { self.p.silos } else { self.p.silos.min(1) };
        let silos_built = s.count[Role::Silo as usize] + plan.pending[Role::Silo as usize];
        let mut due: Vec<(usize, usize, Project)> = Vec::new();
        if s.minutes >= self.p.nuke_minute && e >= 900 && m >= 18 && silos_built < silos {
            due.push((silos_built, 0, Project::Silo));
        }
        if s.minutes >= self.p.heavy_minute && m >= 20 && s.heavy_towers < self.p.heavy_towers.min(1 + s.minutes as usize / 12) {
            due.push((s.heavy_towers, 0, Project::HeavyTower));
        }
        if s.minutes >= self.p.long_gun_minute && m >= 25 && e >= 700 && s.long_guns < self.p.long_guns {
            due.push((s.long_guns, 0, Project::LongGun));
        }
        let liking: [Project; 3] = match self.persona {
            Persona::Turtle => [Project::HeavyTower, Project::LongGun, Project::Silo],
            Persona::Tech => [Project::Silo, Project::LongGun, Project::HeavyTower],
            Persona::Rush => [Project::HeavyTower, Project::Silo, Project::LongGun],
            _ => [Project::Silo, Project::HeavyTower, Project::LongGun],
        };
        for d in &mut due {
            d.1 = liking.iter().position(|&k| k == d.2).unwrap_or(0);
        }
        due.sort_by_key(|&(built, order, _)| (built, order));
        for (_, _, kind) in due {
            let task = match kind {
                Project::Silo => {
                    let strategic = |o: usize| self.info[o].role == Role::Silo && self.info[o].silo_classes & ta_sim::STRATEGIC_CLASS != 0;
                    let silo = opts.iter().copied().filter(|&o| strategic(o) && self.info[o].land).min_by_key(|&o| (self.info[o].cost, o)).or_else(|| self.pick_cheapest(opts, Role::Silo));
                    silo.and_then(|n| self.place(sim, b, n, Zone::Back, 300, plan).map(|site| Task::Build(n, site)))
                }
                Project::HeavyTower => {
                    let tower = opts
                        .iter()
                        .copied()
                        .filter(|&o| {
                            // (One whose guns draw power only on an income that keeps them firing.)
                            let i = &self.info[o];
                            i.role == Role::Defense && i.land && !i.long_range && i.cost >= HEAVY_COST && i.cost <= m * 60 * LONG_GUN_INCOME_MINUTES && i.fire_energy * 4 <= e
                        })
                        .max_by_key(|&o| (self.info[o].value, Reverse(o)));
                    tower.and_then(|t| self.place(sim, b, t, Zone::Front, 300, plan).map(|site| Task::Build(t, site)))
                }
                Project::LongGun => self.long_gun_task(sim, s, b, opts, plan),
            };
            if task.is_some() {
                return task;
            }
        }
        None
    }

    /// An experimental: one the constructor builds itself where it can (Forged Alliance's), else
    /// a factory of them (TA's Gantry) when we have none: the best it can, within so many minutes
    /// of our metal income.
    fn experimental_task(&mut self, sim: &Sim, s: &Survey, b: &Unit, opts: &[usize], plan: &Plan) -> Option<Task> {
        let budget = s.m_inc.to_int() * 60 * EXPERIMENTAL_INCOME_MINUTES;
        let unit = opts
            .iter()
            .copied()
            .filter(|&o| self.info[o].experimental && self.info[o].kind != Locomotion::Naval && self.info[o].kind != Locomotion::Air && self.info[o].cost <= budget)
            .max_by_key(|&o| (self.info[o].value.max(self.info[o].efficiency), Reverse(o)));
        if let Some(x) = unit && let Some(site) = self.place(sim, b, x, Zone::Front, 220, plan) {
            log::debug!("{} min: P{} starts an experimental, {}", s.minutes, self.player, sim.types.get(x).name);
            return Some(Task::Build(x, site));
        }
        let have_factory = s.factory_types.iter().any(|&f| self.info[f].experimental_factory);
        let factory = opts.iter().copied().filter(|&o| self.info[o].experimental_factory && self.info[o].land && self.info[o].cost <= budget).min_by_key(|&o| (self.info[o].cost, o));
        if !have_factory && let Some(f) = factory && let Some(site) = self.place(sim, b, f, Zone::Ring, 260, plan) {
            log::debug!("{} min: P{} puts up a factory of experimentals, {}", s.minutes, self.player, sim.types.get(f).name);
            return Some(Task::Build(f, site));
        }
        None
    }

    /// A long-range gun on a site of ours from which the nearest enemy base we have found lies
    /// within its reach (towards the enemy, in our half of the ground between), the cheapest
    /// that reaches; none with no enemy base found, or none in reach of any site we can put
    /// one on.
    pub(crate) fn long_gun_task(&mut self, sim: &Sim, s: &Survey, b: &Unit, opts: &[usize], plan: &Plan) -> Option<Task> {
        let budget = s.m_inc.to_int() * 60 * LONG_GUN_INCOME_MINUTES;
        let seen = self.enemy_bases.iter().zip(&self.enemy_base_seen).filter(|&(_, &seen)| seen).map(|(&p, _)| p).min_by_key(|p| (dist(*p, self.base), p.x.0, p.z.0))?;
        let d = dist(seen, self.base);
        let toward = (seen - self.base).xz().normalize();
        // The cheapest that has a site from which it reaches (one too short for the distance
        // has none: a pricier one with the reach is taken), none whose sites were not to be had
        // a moment ago.
        let mut guns: Vec<usize> = opts.iter().copied().filter(|&o| self.info[o].long_range && self.info[o].land && self.info[o].cost <= budget).collect();
        guns.sort_by_key(|&o| (self.info[o].cost, o));
        for gun in guns {
            if self.place_failed.iter().any(|&(t, until)| t == gun && sim.tick < until) {
                continue;
            }
            let reach = self.info[gun].range * 92 / 100;
            // (As far back as it can stand and still reach, never much past halfway.)
            let out = (d - reach).max(200).min(d * 55 / 100);
            let near = Self::clamp_to_map(sim, self.base + toward.scale(fx(out)));
            let r = fx(sim.types.get(gun).radius.to_int() + 16);
            let ok = |p: FxVec3| p.within_xz(seen, fx(reach)) && plan.free(p, r) && self.threat.at(p) == 0 && self.builder_reaches(sim, b, p, gun);
            match self.find_site(sim, gun, near, 24, &ok) {
                Some(site) => {
                    log::debug!("{} min: P{} puts up a long-range gun, {}, {} px from the enemy's base", s.minutes, self.player, sim.types.get(gun).name, dist(site, seen));
                    return Some(Task::Build(gun, site));
                }
                // (Looked for again in a while, not by every constructor at every turn.)
                None => self.place_failed.push((gun, sim.tick + 30 * SECOND)),
            }
        }
        None
    }

    /// Our long-range guns shell what we know of the enemy's buildings in their reach, worth the
    /// most first (its economy and factories, its strategic buildings), away from our own and our
    /// allies' units; each turns to the next after a minute (what it shells may be gone out of
    /// our sight), and back to fire at will with nothing known in reach.
    pub(crate) fn run_long_guns(&mut self, sim: &Sim, s: &Survey, out: &mut Vec<Command>) {
        if s.peace || s.now % GUNS_LOOK >= THINK_TICKS {
            return;
        }
        let me = self.player;
        let friends: Vec<FxVec3> = s.own.iter().chain(s.allies.iter()).map(|u| u.pos).collect();
        self.gun_targets.retain(|&(g, _, _)| sim.unit(g).is_some_and(|u| !u.dead && u.owner == me));
        let guns: Vec<&Unit> = s.own.iter().copied().filter(|u| self.info[u.type_id].long_range && Self::can_command(sim, u)).collect();
        for gun in guns {
            let reach = self.info[gun.type_id].range;
            let shelling = match gun.orders.front() {
                Some(&Order::Attack(Target::Ground(p))) => Some(p),
                _ => None,
            };
            let kept = self.gun_targets.iter().position(|&(g, _, _)| g == gun.id);
            // On a building we still know of, for less than its time: on with it.
            if let (Some(p), Some(k)) = (shelling, kept) {
                let (_, at, since) = self.gun_targets[k];
                if at == p && s.now < since + GUN_TARGET_TICKS && self.enemy_buildings.iter().any(|b| b.1.within_xz(p, fx(48))) {
                    continue;
                }
            }
            let last = kept.map(|k| self.gun_targets[k].1);
            let best = self
                .enemy_buildings
                .iter()
                .filter(|(_, p, _)| p.within_xz(gun.pos, fx(reach - 32)) && !friends.iter().any(|f| f.within_xz(*p, fx(220))))
                // (Not the one it had just now, while another is known.)
                .map(|&(id, p, t)| (self.target_value(t) * 1000 / (1000 + dist(p, gun.pos) / 4) - if last.is_some_and(|l| l.within_xz(p, fx(48))) { 1_000_000 } else { 0 }, id.slot, p))
                .max_by_key(|&(score, slot, _)| (score, Reverse(slot)));
            if let Some(k) = kept {
                self.gun_targets.remove(k);
            }
            match best {
                Some((_, _, p)) => {
                    log::debug!("{} min: P{me} shells ({}, {}) with its {}", s.minutes, p.x.to_int(), p.z.to_int(), sim.unit_type(gun).name);
                    self.gun_targets.push((gun.id, p, s.now));
                    out.push(Command::Order { player: me, units: vec![gun.id], order: Order::Attack(Target::Ground(p)), queue: false });
                }
                None if shelling.is_some() => out.push(Command::Order { player: me, units: vec![gun.id], order: Order::Stop, queue: false }),
                None => {}
            }
        }
    }
}

impl SkirmishAi {
    /// A hand for a project under way: a constructor near enough helps finish it while fewer than
    /// the level's are at it (alone, one takes many minutes over a silo or an anti-nuclear
    /// defence, the Commander only near home).
    pub(crate) fn help_project_task(&self, sim: &Sim, s: &Survey, b: &Unit, plan: &Plan) -> Option<Task> {
        let reach = if self.info[b.type_id].commander { 900 } else { 2500 };
        let frame = s
            .nanoframes
            .iter()
            .filter(|n| self.is_project(n.type_id) && !self.unreachable_units.iter().any(|&(u, _)| u == n.id))
            .min_by_key(|n| (dist(n.pos, b.pos), n.id.slot))?;
        if dist(frame.pos, b.pos) > reach {
            return None;
        }
        let at_it = |h: &Unit| h.build.is_some_and(|j| j.target == frame.id) || matches!(h.orders.front(), Some(&Order::Repair(t)) if t == frame.id);
        let hands = s.builders.iter().filter(|h| h.id != b.id && at_it(h)).count() + plan.hands.iter().filter(|&&f| f == frame.id).count();
        let wanted = match self.difficulty {
            Difficulty::Easy => 1,
            Difficulty::Normal => 3,
            Difficulty::Hard => 4,
        } + (self.persona == Persona::Tech) as usize;
        if hands >= wanted || at_it(b) || !self.unit_reaches(sim, b, frame.pos) {
            return None;
        }
        Some(Task::Repair(frame.id))
    }
}
