//! Builders whose default mission is a repair patrol (TA's `DefaultMissionType=RepairPatrol`:
//! TA: Escalation's repair tower upgrades, which its air repair pads make, "Automatically repairs
//! all nearby units") mend the damaged units of their side and its allies within their reach by
//! themselves whenever they have nothing else to do, standing where they are: a module its pad has
//! taken on as well as one standing loose. (Read but never acted on, the mission left them idle:
//! a builder that cannot move, patrol or guard waits for orders, and a module on its pad took
//! none.)
//!
//! Builders whose default mission is a patrol where they stand (`DefaultMissionType=AssistPatrol`:
//! Supreme Commander's engineering stations, the Kennel and the Hive) work round them by
//! themselves whenever they have nothing else to do, as a builder on patrol does on its way: they
//! help build what their player is building within their reach (a frame, a factory's unit, a
//! building upgrading in place), else mend the damaged units of their side and its allies there,
//! else reclaim the wreckage there (wrecks and their heaps: not trees and rocks, so the ground round
//! a base stays as its player left it). The nearest first, and nothing found out of reach again.
//! What they find is their only order; the player's next order, queued or not, takes its place
//! (`Unit::auto_work`).

use super::*;

/// How often (ticks; each builder on a tick of its own, by its slot) an idle one looks for work.
const LOOK_EVERY: u64 = 15;
/// How far past its reach a unit's middle may stand and its footprint still come within it (px):
/// half the diagonal of a mod's building 17 cells a side.
const FOOTPRINT_REACH: i64 = 192;

impl Sim {
    /// Whether builders of this type repair what is round them by themselves.
    pub(crate) fn repairs_round(t: &UnitType) -> bool {
        t.is_builder && t.def.default_mission.eq_ignore_ascii_case("RepairPatrol")
    }

    /// Whether `u`, carried by another unit, works where it rides: a module that repairs round
    /// itself on the building that took it on (one that carries none, unlike a transport).
    pub(crate) fn module_at_work(&self, u: &Unit, t: &UnitType) -> bool {
        Self::repairs_round(t) && u.carried_by.and_then(|c| self.unit(c)).is_some_and(|c| !c.dead && self.types.get(c.type_id).transport_capacity == 0)
    }

    /// An idle builder that repairs round itself: the nearest damaged unit of its side or an
    /// ally's (finished, not carried, not being taken apart) whose footprint is within its reach,
    /// its order to mend it put first.
    pub(super) fn repair_round(&mut self, id: UnitId, t: &UnitType) {
        if !(self.tick + id.slot as u64).is_multiple_of(LOOK_EVERY) {
            return;
        }
        let Some(u) = self.unit(id).filter(|u| u.is_complete() && !u.dead && u.orders.is_empty()) else { return };
        let (pos, owner) = (u.pos, u.owner);
        let reach = t.build_distance + t.radius;
        let mut near = std::mem::take(&mut self.scratch);
        self.grid.query(pos, reach + Fx::from_int(FOOTPRINT_REACH), &mut near);
        let mut best: Option<(Fx, u32, UnitId)> = None;
        for &slot in &near {
            let Some(o) = self.units.get(slot as usize).and_then(|o| o.as_ref()) else { continue };
            if o.id == id || o.dead || !o.is_complete() || o.carried_by.is_some() || !self.allied(owner, o.owner) || self.being_taken_apart(o) {
                continue;
            }
            let ot = self.types.get(o.type_id);
            if o.health >= ot.max_health {
                continue;
            }
            let d = Self::distance_to_footprint(pos, o.pos, ot.footprint);
            if d <= reach && best.is_none_or(|(bd, bs, _)| (d, o.id.slot) < (bd, bs)) {
                best = Some((d, o.id.slot, o.id));
            }
        }
        self.scratch = near;
        if let Some((_, _, target)) = best
            && let Some(u) = self.unit_mut(id)
        {
            u.orders.push_front(Order::Repair(target));
            u.auto_work = true;
        }
    }

    /// Whether builders of this type work round them by themselves (`AssistPatrol`).
    pub(crate) fn works_round(t: &UnitType) -> bool {
        t.is_builder && t.def.default_mission.eq_ignore_ascii_case("AssistPatrol")
    }

    /// Whether builders of this type find their own work whenever they have none (`RepairPatrol`,
    /// `AssistPatrol`): one waiting for it is no builder idle for want of an order.
    pub fn works_by_itself(t: &UnitType) -> bool {
        Self::repairs_round(t) || Self::works_round(t)
    }

    /// An idle builder that works round itself (`AssistPatrol`): what it finds within its reach,
    /// put first among its orders: the nearest of its player's units unfinished or upgrading in
    /// place (not one its side is taking apart), else the nearest damaged unit of its side or an
    /// ally's (finished, not carried), else the nearest wreck worth reclaiming; none it found out
    /// of reach lately.
    pub(super) fn work_round(&mut self, id: UnitId, t: &UnitType) {
        if !(self.tick + id.slot as u64).is_multiple_of(LOOK_EVERY) {
            return;
        }
        let Some(u) = self.unit(id).filter(|u| u.is_complete() && !u.dead && u.orders.is_empty()) else { return };
        let (pos, owner, skip) = (u.pos, u.owner, u.unreachable.clone());
        let reach = t.build_distance + t.radius;
        let mut near = std::mem::take(&mut self.scratch);
        self.grid.query(pos, reach + Fx::from_int(FOOTPRINT_REACH), &mut near);
        // (What it is: 0 to help build, 1 to mend; then how near, then its slot.)
        let mut best: Option<(u8, Fx, u32, UnitId)> = None;
        for &slot in &near {
            let Some(o) = self.units.get(slot as usize).and_then(|o| o.as_ref()) else { continue };
            if o.id == id || o.dead || o.carried_by.is_some() || !self.allied(owner, o.owner) || self.being_taken_apart(o) || skip.contains(&Self::work_key(true, o.id.slot)) {
                continue;
            }
            let ot = self.types.get(o.type_id);
            let building = !o.is_complete() || o.self_work.is_some();
            let rank = if building && o.owner == owner {
                0
            } else if o.is_complete() && o.health < o.max_health(ot) {
                1
            } else {
                continue;
            };
            let d = Self::distance_to_footprint(pos, o.pos, ot.footprint);
            if d <= reach && best.is_none_or(|(br, bd, bs, _)| (rank, d, o.id.slot) < (br, bd, bs)) {
                best = Some((rank, d, o.id.slot, o.id));
            }
        }
        self.scratch = near;
        let order = match best {
            Some((_, _, _, target)) => Some(Order::Repair(target)),
            None if t.def.can_reclaim => {
                // Wreckage within its reach, the nearest first.
                let mut found = Vec::new();
                self.feature_grid.near(pos, reach + Fx::from_int(FOOTPRINT_REACH), &mut found);
                let mut best: Option<(Fx, usize)> = None;
                for i in found {
                    let (i, f) = (i as usize, &self.features[i as usize]);
                    if !f.alive || !self.is_wreckage(f.def) || !self.data.features[f.def].reclaimable || skip.contains(&Self::work_key(false, i as u32)) || self.reclaim_value(f.def).0 <= Fx::ZERO {
                        continue;
                    }
                    let def = &self.data.features[f.def];
                    let d = Self::distance_to_footprint(pos, f.pos, (def.footprint_x, def.footprint_z));
                    if d <= reach && best.is_none_or(|(bd, bi)| (d, i) < (bd, bi)) {
                        best = Some((d, i));
                    }
                }
                best.map(|(_, i)| Order::Reclaim(i))
            }
            None => None,
        };
        if let Some(order) = order
            && let Some(u) = self.unit_mut(id)
        {
            u.orders.push_front(order);
            u.auto_work = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unit::PlayerSetup;
    use std::sync::Arc;

    fn unit(name: &str, keys: &str) -> (String, String) {
        (format!("units/{}.fbi", name.to_ascii_lowercase()), format!("[UNITINFO]\n{{\nUnitName={name};\nObjectname={name};\nMaxDamage=1000;\n{keys}\n}}\n"))
    }

    /// TA: Escalation's repair tower upgrade as its file has it (a module that takes no ground,
    /// its pad's), another that waits for orders, an air repair pad, a transport and a tank.
    fn sim() -> Sim {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let tower = "BuildTime=100; FootprintX=0; FootprintZ=0; WorkerTime=360; Builder=1; Builddistance=448; canmove=0; canpatrol=0; canguard=0; MaxVelocity=1; Acceleration=0;";
        let files = [
            unit("TOWER", &format!("{tower} DefaultMissionType=RepairPatrol;")),
            unit("IDLER", &format!("{tower} DefaultMissionType=Standby;")),
            unit("PAD", "BuildTime=3000; FootprintX=3; FootprintZ=3; IsAirBase=1;"),
            unit("CARRIER", "BuildTime=100; FootprintX=2; FootprintZ=2; CanMove=1; MaxVelocity=1; TransportCapacity=4; TransportSize=4;"),
            // (Mended at the tower's pace, a whole one in about eight seconds.)
            unit("TANK", "BuildTime=3000; FootprintX=2; FootprintZ=2; CanMove=1; MaxVelocity=1;"),
            // (A fast one, mended slowly: it leaves the tower's reach long before it is whole.)
            unit("RUNNER", "BuildTime=30000; FootprintX=2; FootprintZ=2; CanMove=1; MaxVelocity=4; Acceleration=1; BrakeRate=1;"),
            // An engineering station as the Supreme Commander factions' are made (`AssistPatrol`),
            // a building it may help put up, and wrecks and a rock to reclaim.
            unit("STATION", "BuildTime=100; FootprintX=2; FootprintZ=2; WorkerTime=300; Builder=1; BuildDistance=300; CanReclamate=1; canguard=1; DefaultMissionType=AssistPatrol;"),
            unit("HUT", "BuildTime=300; BuildCostMetal=100; BuildCostEnergy=500; FootprintX=2; FootprintZ=2; Corpse=HUT_DEAD;"),
            // (A building that upgrades in place, slowly by itself.)
            unit("SMITHY", "BuildTime=100; FootprintX=2; FootprintZ=2; WorkerTime=30; UpgradesTo=SMITHY2;"),
            unit("SMITHY2", "BuildTime=3000; BuildCostMetal=100; BuildCostEnergy=1000; FootprintX=2; FootprintZ=2;"),
            ("features/all/hut_dead.tdf".into(), "[HUT_DEAD] { footprintx=2; footprintz=2; blocking=1; reclaimable=1; metal=40; damage=500; }".into()),
            ("features/all/rock.tdf".into(), "[ROCK] { footprintx=1; footprintz=1; blocking=1; reclaimable=1; metal=20; }".into()),
        ];
        let dir = std::env::temp_dir().join(format!("ta-sim-repair-round-{}-{}", std::process::id(), NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
        for (path, text) in &files {
            let p = dir.join(path);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        }
        let mut vfs = ta_vfs::Vfs::new();
        vfs.mount_mod_dir(&dir, "test").unwrap();
        let data = Arc::new(ta_data::GameData::load(&vfs));
        std::fs::remove_dir_all(&dir).ok();
        let types = Arc::new(crate::defs::UnitTypes::compile(&data, &|_| None));
        let map = ta_formats::tnt::TntMap {
            width: 96,
            height: 96,
            sea_level: 0,
            tiles: Vec::new(),
            tile_map: Vec::new(),
            heights: vec![60; 96 * 96],
            features: vec![ta_formats::tnt::FEATURE_NONE; 96 * 96],
            feature_names: Vec::new(),
            minimap: Default::default(),
            flags: Vec::new(),
        };
        let player = |side: &str, color| PlayerSetup { side: side.into(), color, start: FxVec3::ZERO, metal: Fx::from_int(1000), energy: Fx::from_int(1000), ai: false };
        Sim::new(types, data, &map, &ta_data::MapInfo::default(), 0, &[player("ARM", 0), player("CORE", 1)], 7)
    }

    fn spawn(sim: &mut Sim, name: &str, owner: u8, x: i64, z: i64) -> UnitId {
        let type_id = sim.types.id(name).unwrap();
        sim.spawn_unit(type_id, owner, FxVec3::from_ints(x, 60, z), 0, true).unwrap()
    }

    fn health(sim: &Sim, id: UnitId) -> Fx {
        sim.unit(id).unwrap().health
    }

    /// An engineering station with nothing to do works round it by itself, within its reach and
    /// no further: first it helps build its player's frame (not the enemy's), then mends a damaged
    /// unit of its side (not the enemy's), then reclaims the wreck (not the rock); then it is idle.
    #[test]
    fn an_idle_station_builds_mends_and_reclaims_round_it() {
        let mut sim = sim();
        sim.set_starting_resources(0, Fx::from_int(10_000), Fx::from_int(100_000));
        let station = spawn(&mut sim, "STATION", 0, 600, 600);
        let hut = sim.types.id("HUT").unwrap();
        let frame = sim.spawn_unit(hut, 0, FxVec3::from_ints(800, 60, 600), 0, false).unwrap();
        let far_frame = sim.spawn_unit(hut, 0, FxVec3::from_ints(1300, 60, 600), 0, false).unwrap();
        let enemy_frame = sim.spawn_unit(hut, 1, FxVec3::from_ints(600, 60, 400), 0, false).unwrap();
        let tank = spawn(&mut sim, "TANK", 0, 600, 750);
        let enemy = spawn(&mut sim, "TANK", 1, 450, 600);
        for id in [tank, enemy] {
            sim.set_health(id, Fx::from_int(300));
        }
        let wreck = sim.place_feature("HUT_DEAD", FxVec3::from_ints(700, 60, 800), 0).unwrap();
        let rock = sim.place_feature("ROCK", FxVec3::from_ints(560, 60, 700), 0).unwrap();
        for _ in 0..20 {
            sim.step(&[]);
        }
        assert_eq!(sim.unit(station).unwrap().orders.front(), Some(&Order::Repair(frame)), "its frame first: {:?}", sim.unit(station).unwrap().orders);
        let mut order = Vec::new();
        for _ in 0..1500 {
            sim.step(&[]);
            let front = sim.unit(station).unwrap().orders.front().cloned();
            if let Some(o) = front && order.last() != Some(&o) {
                order.push(o);
            }
        }
        assert_eq!(order, [Order::Repair(frame), Order::Repair(tank), Order::Reclaim(wreck)], "building, then mending, then reclaiming");
        assert!(sim.unit(frame).unwrap().is_complete(), "it helped finish its frame");
        let most = sim.types.get(sim.unit(tank).unwrap().type_id).max_health;
        assert_eq!(health(&sim, tank), most, "and mended its tank");
        assert!(!sim.features[wreck].alive, "and reclaimed the wreck");
        assert!(sim.features[rock].alive, "not the rock");
        assert!(!sim.unit(far_frame).unwrap().is_complete() && !sim.unit(enemy_frame).unwrap().is_complete(), "nothing out of reach, nothing of the enemy's");
        assert_eq!(health(&sim, enemy), Fx::from_int(300));
        assert!(sim.unit(station).unwrap().orders.is_empty(), "nothing left to do: idle");
    }

    /// The player's order comes first: given one, queued or not, the station drops what it found
    /// itself and does that; idle again, it goes back to work round it.
    #[test]
    fn a_stations_orders_come_first() {
        let mut sim = sim();
        sim.set_starting_resources(0, Fx::from_int(10_000), Fx::from_int(100_000));
        let station = spawn(&mut sim, "STATION", 0, 600, 600);
        let tank = spawn(&mut sim, "TANK", 0, 600, 750);
        let other = spawn(&mut sim, "TANK", 0, 750, 600);
        for id in [tank, other] {
            sim.set_health(id, Fx::from_int(300));
        }
        for _ in 0..20 {
            sim.step(&[]);
        }
        let found = sim.unit(station).unwrap().orders.front().cloned();
        assert!(matches!(found, Some(Order::Repair(_))), "{found:?}");
        let Some(Order::Repair(first)) = found else { unreachable!() };
        let told = if first == tank { other } else { tank };
        // (Queued, it does not wait behind what the station chose itself.)
        sim.step(&[Command::Order { player: 0, units: vec![station], order: Order::Repair(told), queue: true }]);
        assert_eq!(sim.unit(station).unwrap().orders.iter().cloned().collect::<Vec<_>>(), [Order::Repair(told)]);
        for _ in 0..300 {
            sim.step(&[]);
        }
        let most = sim.types.get(sim.unit(tank).unwrap().type_id).max_health;
        assert_eq!(health(&sim, told), most, "what it was told first");
        for _ in 0..300 {
            sim.step(&[]);
        }
        assert_eq!(health(&sim, first), most, "then, idle, what it finds itself");
    }

    /// An idle station helps a building of its player's upgrading in place within its reach, done
    /// long before one upgrading alone out of it; told to stop, it drops that, and idle again it
    /// takes it up once more when it next looks round.
    #[test]
    fn a_station_helps_an_upgrade_and_takes_it_up_again_after_a_stop() {
        let mut sim = sim();
        sim.set_starting_resources(0, Fx::from_int(10_000), Fx::from_int(100_000));
        let station = spawn(&mut sim, "STATION", 0, 600, 600);
        let near = spawn(&mut sim, "SMITHY", 0, 760, 600);
        let far = spawn(&mut sim, "SMITHY", 0, 1400, 1400);
        let to = sim.types.id("SMITHY2").unwrap();
        sim.step(&[Command::Order { player: 0, units: vec![near, far], order: Order::Upgrade(to), queue: false }]);
        for _ in 0..20 {
            sim.step(&[]);
        }
        assert_eq!(sim.unit(station).unwrap().orders.front(), Some(&Order::Repair(near)), "the upgrade within its reach");
        sim.step(&[Command::Order { player: 0, units: vec![station], order: Order::Stop, queue: false }]);
        assert!(sim.unit(station).unwrap().orders.is_empty() && sim.unit(station).unwrap().build.is_none(), "stopped");
        for _ in 0..16 {
            sim.step(&[]);
        }
        assert_eq!(sim.unit(station).unwrap().orders.front(), Some(&Order::Repair(near)), "idle again, back at it");
        let mut upgraded = Vec::new();
        for _ in 0..600 {
            sim.step(&[]);
            for e in sim.take_events() {
                if let SimEvent::UnitUpgraded { from, .. } = e {
                    upgraded.push(from);
                }
            }
        }
        assert_eq!(upgraded, [near], "helped, it is done; the other, alone, is not");
        assert!(sim.unit(far).unwrap().self_work.is_some());
        // (What works round it by itself is never idle for want of an order.)
        let type_of = |name: &str| sim.types.get(sim.types.id(name).unwrap());
        assert!(Sim::works_by_itself(type_of("STATION")) && Sim::works_by_itself(type_of("TOWER")) && !Sim::works_by_itself(type_of("IDLER")));
    }

    /// Standing loose (TA: Escalation's Arm pad never takes its tower on), it mends its side's
    /// damaged units within its reach by itself, the nearest first, and none out of it, nor the
    /// enemy's; a builder that waits for orders mends nothing.
    #[test]
    fn a_repair_tower_mends_what_is_round_it_by_itself() {
        let mut sim = sim();
        let tower = spawn(&mut sim, "TOWER", 0, 600, 600);
        let idler = spawn(&mut sim, "IDLER", 0, 1300, 1300);
        let near = spawn(&mut sim, "TANK", 0, 800, 600);
        let nearer = spawn(&mut sim, "TANK", 0, 600, 700);
        let far = spawn(&mut sim, "TANK", 0, 600, 1200);
        let enemy = spawn(&mut sim, "TANK", 1, 450, 600);
        let by_idler = spawn(&mut sim, "TANK", 0, 1400, 1300);
        for id in [near, nearer, far, enemy, by_idler] {
            sim.set_health(id, Fx::from_int(300));
        }
        // The nearest first.
        for _ in 0..20 {
            sim.step(&[]);
        }
        assert_eq!(sim.unit(tower).unwrap().orders.front(), Some(&Order::Repair(nearer)), "{:?}", sim.unit(tower).unwrap().orders);
        for _ in 0..900 {
            sim.step(&[]);
        }
        let most = sim.types.get(sim.unit(near).unwrap().type_id).max_health;
        assert_eq!(health(&sim, nearer), most);
        assert_eq!(health(&sim, near), most);
        assert_eq!(health(&sim, far), Fx::from_int(300), "out of its reach");
        assert_eq!(health(&sim, enemy), Fx::from_int(300), "the enemy's");
        assert_eq!(health(&sim, by_idler), Fx::from_int(300), "a builder without the mission waits for orders");
        assert!(sim.unit(idler).unwrap().orders.is_empty());
        assert!(sim.unit(tower).unwrap().orders.is_empty(), "nothing left to mend: idle again");
    }

    /// A unit walking out of its reach while it is mended is let go without a word: the tower's
    /// own work, no order of the player's refused.
    #[test]
    fn a_repair_tower_lets_a_unit_go_out_of_its_reach_quietly() {
        let mut sim = sim();
        let tower = spawn(&mut sim, "TOWER", 0, 600, 600);
        let tank = spawn(&mut sim, "RUNNER", 0, 800, 600);
        sim.set_health(tank, Fx::from_int(300));
        for _ in 0..30 {
            sim.step(&[]);
        }
        assert_eq!(sim.unit(tower).unwrap().orders.front(), Some(&Order::Repair(tank)));
        sim.take_events();
        sim.step(&[Command::Order { player: 0, units: vec![tank], order: Order::Move(FxVec3::from_ints(600, 60, 1400)), queue: false }]);
        let mut failed = false;
        for _ in 0..600 {
            sim.step(&[]);
            failed |= sim.take_events().iter().any(|e| matches!(e, SimEvent::OrderFailed { unit, .. } if *unit == tower));
        }
        assert!(sim.unit(tank).unwrap().pos.z > Fx::from_int(1200), "the tank went");
        assert!(sim.unit(tower).unwrap().orders.is_empty(), "the tower let it go");
        assert!(!failed, "and said nothing of it");
        assert!(health(&sim, tank) < sim.types.get(sim.unit(tank).unwrap().type_id).max_health, "out of reach, it was mended no more");
    }

    /// On the pad that took it on (TA: Escalation's Core pad attaches its tower), it works where it
    /// rides; in a transport's hold, it does not.
    #[test]
    fn a_repair_tower_on_its_pad_works_there() {
        let mut sim = sim();
        let pad = spawn(&mut sim, "PAD", 0, 600, 600);
        let tower = spawn(&mut sim, "TOWER", 0, 600, 600);
        sim.attach(pad, tower, None);
        let carrier = spawn(&mut sim, "CARRIER", 0, 1300, 1300);
        let rider = spawn(&mut sim, "TOWER", 0, 1300, 1300);
        sim.attach(carrier, rider, None);
        let tank = spawn(&mut sim, "TANK", 0, 800, 600);
        let by_carrier = spawn(&mut sim, "TANK", 0, 1400, 1300);
        for id in [tank, by_carrier, pad] {
            sim.set_health(id, Fx::from_int(300));
        }
        for _ in 0..900 {
            sim.step(&[]);
        }
        assert_eq!(sim.unit(tower).unwrap().carried_by, Some(pad));
        let most = sim.types.get(sim.unit(tank).unwrap().type_id).max_health;
        assert_eq!(health(&sim, tank), most);
        assert_eq!(health(&sim, pad), sim.types.get(sim.unit(pad).unwrap().type_id).max_health, "its pad too");
        assert_eq!(health(&sim, by_carrier), Fx::from_int(300), "a transport's passenger does no work");
    }
}
