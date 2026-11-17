//! Builders whose default mission is a repair patrol (TA's `DefaultMissionType=RepairPatrol`:
//! TA: Escalation's repair tower upgrades, which its air repair pads make, "Automatically repairs
//! all nearby units") mend the damaged units of their side and its allies within their reach by
//! themselves whenever they have nothing else to do, standing where they are: a module its pad has
//! taken on as well as one standing loose. (Read but never acted on, the mission left them idle:
//! a builder that cannot move, patrol or guard waits for orders, and a module on its pad took
//! none.)

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
