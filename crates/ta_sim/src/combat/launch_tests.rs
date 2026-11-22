//! Missiles as their weapons' speeds say. Made-up units and weapons on flat ground.

use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

use crate::Command;

/// Weapons for these tests: a vertical launcher whose missiles leave faster than their top
/// speed (a negative `weaponvelocity`) and slow down as they fly (`weaponacceleration` negative);
/// the same, gathering speed as TA's launchers do; and one launched "upright" at a negative speed.
const WEAPONS: &str = "
    [SLOWING] { range=600; reloadtime=2; rendertype=1; vlaunch=1; tracks=1; selfprop=1; twophase=1;
        startvelocity=600; weaponvelocity=-5; weaponacceleration=-60; turnrate=60000;
        weapontimer=0.5; flighttime=3; areaofeffect=40; [DAMAGE] { default=1; } }
    [GATHERING] { range=600; reloadtime=2; rendertype=1; vlaunch=1; tracks=1; selfprop=1; twophase=1;
        startvelocity=200; weaponvelocity=500; weaponacceleration=150; turnrate=60000;
        weapontimer=0.5; flighttime=3; areaofeffect=40; [DAMAGE] { default=1; } }
    [DROP] { range=300; reloadtime=2; rendertype=1; vlaunch=1; guidance=1; selfprop=1; twophase=1;
        startvelocity=-300; weaponvelocity=-300; weaponacceleration=-2; turnrate=40000;
        weapontimer=900; flighttime=900; areaofeffect=40; [DAMAGE] { default=1; } }
";

/// A game of made-up units (`UnitName` and their FBI keys) on flat ground 60 high, 1024 px a side.
fn game_of(units: &[(&str, &str)]) -> Sim {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!("ta-launch-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    std::fs::create_dir_all(dir.join("units")).unwrap();
    std::fs::create_dir_all(dir.join("weapons")).unwrap();
    for &(name, keys) in units {
        std::fs::write(dir.join(format!("units/{name}.fbi")), format!("[UNITINFO] {{ UnitName={name}; MaxDamage=1000; SightDistance=900; {keys} }}")).unwrap();
    }
    std::fs::write(dir.join("weapons/launch.tdf"), WEAPONS).unwrap();
    let mut vfs = ta_vfs::Vfs::new();
    vfs.mount_mod_dir(&dir, "test").unwrap();
    let data = Arc::new(ta_data::GameData::load(&vfs));
    let types = Arc::new(crate::UnitTypes::compile(&data, &|_| None));
    std::fs::remove_dir_all(dir).unwrap();
    let side = 64;
    let map = ta_formats::tnt::TntMap { width: side, height: side, sea_level: 0, tiles: Vec::new(), tile_map: Vec::new(), heights: vec![60; (side * side) as usize], features: vec![ta_formats::tnt::FEATURE_NONE; (side * side) as usize], feature_names: Vec::new(), minimap: Default::default(), flags: Vec::new() };
    let players: Vec<_> = (0..2).map(|i| crate::PlayerSetup { side: "ARM".into(), color: i, start: FxVec3::ZERO, metal: Fx::from_int(1000), energy: Fx::from_int(1000), ai: false }).collect();
    let mut sim = Sim::new(types, data, &map, &ta_data::MapInfo::default(), 0, &players, 7);
    sim.vision.set_all(0, crate::vision::LOS);
    sim
}

fn spawn(sim: &mut Sim, name: &str, owner: u8, x: i64, z: i64) -> UnitId {
    sim.spawn_unit(sim.types.id(name).unwrap(), owner, FxVec3::from_ints(x, 0, z), 0, true).unwrap()
}

/// The unit's first missile launched within `ticks`, sent at `target` by an attack order: where it
/// was each tick it flew, and its speed (px a tick); and whether it went off within 40 px of the
/// target (its blast's reach and the target's size) before its flight time was up.
fn first_flight(sim: &mut Sim, launcher: UnitId, target: UnitId, ticks: u32) -> (Vec<(FxVec3, Fx)>, bool) {
    let order = [Command::Order { player: 0, units: vec![launcher], order: Order::Attack(Target::Unit(target)), queue: false }];
    let mut missile = None;
    let mut flight = Vec::new();
    for tick in 0..ticks {
        sim.step(if tick == 0 { &order } else { &[] });
        let id = missile.or_else(|| sim.projectiles.iter().find(|p| p.owner == launcher).map(|p| p.id));
        let Some(id) = id else { continue };
        missile = Some(id);
        if let Some(p) = sim.projectiles.iter().find(|p| p.id == id) {
            flight.push((p.pos, p.vel.length()));
        } else if let Some((p, _)) = sim.spent.iter().find(|(p, _)| p.id == id) {
            let at = sim.unit(target).unwrap().pos;
            return (flight, p.age < p.ttl && p.pos.distance(at) < Fx::from_int(40) + sim.unit_type(sim.unit(target).unwrap()).radius);
        }
    }
    (flight, false)
}

/// A vertically launched missile that leaves its tube faster than its `weaponvelocity` (a negative
/// one: TA: Escalation's vertical missiles are made so) and has a negative `weaponacceleration`
/// slows down as it flies, from the speed it left at, and gets to its target: it climbs out of its
/// tube at its start speed, turns over and comes down on the target before its flight time is up.
/// (Held to its top speed from its first tick, half a pixel a tick, it hung over the launcher until
/// its flight time was up, and went off there.) One that gathers speed up to its top speed, as TA's
/// own launchers' rockets do, flies as it always did.
#[test]
fn a_missile_launched_faster_than_its_top_speed_slows_down_on_its_way_to_its_target() {
    let mut sim = game_of(&[
        ("BATTERY", "Weapon1=SLOWING; Category=GROUND;"),
        ("RACK", "Weapon1=GATHERING; Category=GROUND;"),
        ("TARGET", "Category=GROUND;"),
    ]);
    sim.cheats_allowed = true;
    sim.step(&[Command::Cheat { player: 1, cheat: crate::Cheat::Invulnerable(true) }]);
    let battery = spawn(&mut sim, "BATTERY", 0, 300, 700);
    let target = spawn(&mut sim, "TARGET", 1, 300, 350);
    let w = sim.weapon_types.get(sim.unit_type(sim.unit(battery).unwrap()).weapons[0].unwrap()).clone();
    let (start, slowing) = (w.start_velocity, w.acceleration);
    assert!(start > w.velocity && slowing < Fx::ZERO, "it leaves faster than its top speed, and slows");
    let (flight, hit) = first_flight(&mut sim, battery, target, 150);
    assert!(flight.len() > 20, "it flies: {} ticks", flight.len());
    let launched = flight[0].0;
    // Its climb (half a second, 15 ticks): straight up at its start speed, less what it has lost.
    let (top, speed) = flight[13];
    assert!(top.y - launched.y > Fx::from_int(100), "it climbs out of its tube: {:?} from {:?}", top, launched);
    assert!(speed < start && speed > start - slowing.abs() * 16 && speed > Fx::from_int(15), "slowing as it climbs: {speed:?} from {start:?}");
    // Then on, slower and slower, and down onto the target.
    let speeds: Vec<Fx> = flight.iter().map(|&(_, s)| s).collect();
    assert!(speeds.windows(2).all(|s| s[1] <= s[0]), "never gathering speed: {speeds:?}");
    assert!(hit, "it goes off at the target, before its flight time is up: last at {:?}", flight.last());
    assert!(flight.iter().any(|&(p, _)| p.distance(launched) > Fx::from_int(250)), "far from its launcher");

    // TA's way, gathering speed up to its top speed: as it always flew.
    let rack = spawn(&mut sim, "RACK", 0, 600, 700);
    let target = spawn(&mut sim, "TARGET", 1, 600, 350);
    let w = sim.weapon_types.get(sim.unit_type(sim.unit(rack).unwrap()).weapons[0].unwrap()).clone();
    let (flight, hit) = first_flight(&mut sim, rack, target, 150);
    let speeds: Vec<Fx> = flight.iter().map(|&(_, s)| s).collect();
    for (i, &s) in speeds.iter().take(14).enumerate() {
        assert_eq!(s, w.start_velocity + w.acceleration * (i as i64 + 1), "climbing, its start speed and a tick's acceleration a tick");
    }
    // (Once it turns, its speed is read off its velocity, to a hair.)
    let hair = Fx::from_ratio(1, 1000);
    assert!(speeds.windows(2).all(|s| s[1] + hair >= s[0] && s[1] <= w.velocity + hair), "gathering speed up to its top speed: {speeds:?}");
    assert!(hit, "at the target");
}

/// What a missile's acceleration does to its speed: TA's gather speed up to their top speed and
/// hold it; one launched faster than its top speed is not pulled down to it (with no acceleration
/// it holds its start speed); one slowing down slows to its top speed and no further; one launched
/// at a negative speed (straight down, from an upright launch) holds it.
#[test]
fn a_missiles_speed_goes_by_its_acceleration_towards_its_top_speed() {
    let weapon = |keys: &str| {
        let mut data = ta_data::GameData::load(&ta_vfs::Vfs::new());
        data.weapons.push(ta_data::WeaponDef::from_tdf(&ta_formats::tdf::parse(&format!("[W] {{ rendertype=1; guidance=1; tracks=1; selfprop=1; turnrate=30000; {keys} }}")).sections[0]));
        crate::WeaponTypes::compile(&data).types.remove(0)
    };
    let per_tick = |v: i64| Fx::from_int(v) / 30;
    // Gathering speed (TA's): to the top and held there, even from just past it.
    let w = weapon("startvelocity=300; weaponvelocity=600; weaponacceleration=300;");
    assert_eq!(w.speed_after(per_tick(300)), per_tick(300) + w.acceleration);
    assert_eq!(w.speed_after(per_tick(599)), per_tick(600));
    assert_eq!(w.speed_after(per_tick(600) + Fx::EPSILON), per_tick(600), "a hair over its top speed: held to it");
    // Launched faster than its top speed: never pulled down to it.
    let w = weapon("startvelocity=900; weaponvelocity=600;");
    assert_eq!(w.speed_after(per_tick(900)), per_tick(900), "no acceleration: its start speed held");
    // Slowing down from a start over its top speed: to its top speed (here half a pixel a tick, the
    // least a weapon flies at) and no further.
    let w = weapon("startvelocity=900; weaponvelocity=-30; weaponacceleration=-90;");
    assert_eq!(w.speed_after(per_tick(900)), per_tick(900) + w.acceleration);
    assert_eq!(w.speed_after(Fx::from_ratio(1, 2)), Fx::from_ratio(1, 2), "down to its top speed and no further");
    assert!(w.speed_after(per_tick(900)) < per_tick(900) && w.speed_after(per_tick(900)) > per_tick(890));
    // Launched at a negative speed: held there (straight down from an upright launch).
    let w = weapon("vlaunch=1; startvelocity=-300; weaponvelocity=-300; weaponacceleration=-2;");
    assert_eq!(w.speed_after(per_tick(-300)), per_tick(-300));
}

/// Launched "upright" at a negative speed, a weapon's missile goes straight down from its launcher
/// (TA: Escalation's bombers drop their pulse bombs so) and goes off on the ground under it. (Its
/// negative start read as none, it rose half a pixel a tick, for its flight time of minutes.)
#[test]
fn a_missile_launched_upright_at_a_negative_speed_drops_straight_down() {
    let mut sim = game_of(&[("DROPPER", "Weapon1=DROP; Category=GROUND;")]);
    let dropper = spawn(&mut sim, "DROPPER", 0, 400, 400);
    // (Held 200 px over the ground, as an aircraft flies.)
    sim.unit_mut(dropper).unwrap().pos.y += Fx::from_int(200);
    let w = sim.weapon_types.get(sim.unit_type(sim.unit(dropper).unwrap()).weapons[0].unwrap()).clone();
    let below = FxVec3::from_ints(400, 60, 400);
    sim.fire(dropper, 0, &w, Target::Ground(below), below, 0);
    let launched = sim.projectiles[0].pos;
    let mut fell = Vec::new();
    for _ in 0..60 {
        sim.step(&[]);
        match sim.projectiles.iter().find(|p| p.owner == dropper) {
            Some(p) => fell.push(p.pos),
            None => break,
        }
    }
    assert!(fell.iter().all(|p| p.x == launched.x && p.z == launched.z), "straight down");
    assert!(fell.windows(2).all(|p| p[1].y < p[0].y), "falling: {fell:?}");
    let (p, _) = sim.spent.iter().find(|(p, _)| p.owner == dropper).expect("gone off within two seconds");
    assert!(p.age < 30 && (p.pos.y - below.y).abs() <= Fx::from_int(1), "on the ground under it: {:?}", p.pos);
}
