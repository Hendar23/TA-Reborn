//! Enhancements (`[ENHANCEMENTS]`, `Order::Enhance`) with made-up units: their cost and time, one
//! a slot and what one needs first, taking one out and calling one off, what they change (health
//! and mending, build power and menu, weapons, sensors, a cloak), and the same game on every
//! computer.

use super::*;

/// A commander-like builder (ACU) with five enhancements: an engineering suite (SUITE) and its
/// better (SUITE3, which needs it) in one slot, a gun (GUN2, in place of its own) in another, and
/// a missile (MISSILE, in a slot of its own) and a cloak (CLOAK) sharing a third.
fn base() -> Sim {
    let acu = "Builder=1; WorkerTime=10; CanMove=1; MaxVelocity=1; BuildDistance=200; FootprintX=2; FootprintZ=2; \
        Weapon1=GUN; RadarDistance=0; SightDistance=400; \
        [ENHANCEMENTS] { \
            [SUITE] { Name=Engineering Suite; Slot=LCH; BuildCostMetal=300; BuildCostEnergy=3000; BuildTime=100; \
                WorkerTime=20; MaxDamage=1000; HealRate=10; BuildOptions=STATION; } \
            [SUITE3] { Name=Better Suite; Slot=LCH; Prerequisite=SUITE; BuildCostMetal=600; BuildTime=300; \
                WorkerTime=50; MaxDamage=2000; BuildOptions=STATION STATION3; } \
            [GUN2] { Slot=RCH; BuildCostMetal=100; BuildTime=100; Weapon1=GUN2; } \
            [MISSILE] { Slot=Back; BuildCostMetal=100; BuildTime=100; Weapon2=MISSILE; } \
            [CLOAK] { Slot=Back; BuildCostMetal=100; BuildTime=100; CloakCost=50; CloakCostMoving=100; \
                Stealth=1; RadarDistance=500; SightDistance=100; } \
        }";
    let data = data_of(&[
        unit("ACU", acu),
        unit("STATION", "FootprintX=2; FootprintZ=2; BuildTime=100; BuildCostMetal=50;"),
        unit("STATION3", "FootprintX=2; FootprintZ=2; BuildTime=100; BuildCostMetal=50;"),
        unit("TANK", "CanMove=1; MaxVelocity=1; FootprintX=1; FootprintZ=1;"),
        weapon_file("GUN", "range=300;"),
        weapon_file("GUN2", "range=600;"),
        weapon_file("MISSILE", "range=800;"),
    ]);
    let mut sim = sim_of(data, 2);
    for p in &mut sim.players {
        p.endless = true;
    }
    sim.set_starting_resources(0, Fx::from_int(100_000), Fx::from_int(1_000_000));
    sim
}

fn acu(sim: &mut Sim) -> UnitId {
    sim.spawn_unit(sim.types.id("ACU").unwrap(), 0, FxVec3::from_ints(400, 60, 400), 16384, true).unwrap()
}

/// The index of the ACU's enhancement `id`.
fn index(sim: &Sim, id: &str) -> u16 {
    sim.types.by_name("ACU").unwrap().def.enhancements.iter().position(|e| e.id == id).unwrap() as u16
}

fn order(sim: &mut Sim, unit: UnitId, id: &str, remove: bool, queue: bool) {
    let index = index(sim, id);
    sim.step(&[Command::Order { player: 0, units: vec![unit], order: Order::Enhance { index, remove }, queue }]);
}

/// Steps until enhancement `id` is installed on `unit` (at most `ticks`), returning the ticks taken.
fn until_installed(sim: &mut Sim, unit: UnitId, id: &str, ticks: u64) -> Option<u64> {
    let index = index(sim, id);
    for k in 1..=ticks {
        sim.step(&[]);
        let events = sim.take_events();
        if events.iter().any(|e| matches!(e, SimEvent::UnitEnhanced { id: u, index: i, on: true } if *u == unit && *i == index)) {
            return Some(k);
        }
    }
    None
}

/// An enhancement takes its build time at the unit's own build power and costs its price, no
/// more; installed, its numbers are the unit's: more health (and its health risen by as much),
/// mending, build power and units on its menu.
#[test]
fn an_enhancement_costs_its_price_over_its_build_time() {
    let mut sim = base();
    let a = acu(&mut sim);
    sim.set_health(a, Fx::from_int(500));
    let (metal, energy) = (sim.players[0].economy.metal, sim.players[0].economy.energy);
    let station = sim.types.id("STATION").unwrap();
    assert!(!sim.build_options(a).contains(&station), "not on its menu before");
    order(&mut sim, a, "SUITE", false, false);
    assert!(sim.unit(a).unwrap().self_work.is_some(), "under way from its first tick");
    // SUITE's 100 of build time at the ACU's 10 a second: ten seconds, 300 ticks (the first above).
    let ticks = until_installed(&mut sim, a, "SUITE", 400).expect("installed");
    assert_eq!(ticks + 1, 300, "its build time at its build power");
    let e = &sim.players[0].economy;
    assert_eq!(metal - e.metal, Fx::from_int(300), "its metal, all of it and no more");
    assert_eq!(energy - e.energy, Fx::from_int(3000), "its energy");
    let t = sim.types.by_name("ACU").unwrap().clone();
    let u = sim.unit(a).unwrap();
    assert_eq!(u.enhancements, [index(&sim, "SUITE")]);
    assert!(u.self_work.is_none() && u.orders.is_empty(), "done with it");
    assert_eq!(u.max_health(&t), Fx::from_int(2000), "its health and the suite's");
    assert!(u.health >= Fx::from_int(1500) && u.health < Fx::from_int(1502), "risen by the suite's 1,000: {}", u.health);
    assert_eq!(u.work_rate(&t), Fx::from_int(30).0 as u64, "its build power and the suite's");
    assert!(sim.build_options(a).contains(&station), "the suite's station on its menu");
    // It mends: ten hit points a second (none of its own).
    let before = sim.unit(a).unwrap().health;
    for _ in 0..300 {
        sim.step(&[]);
    }
    let mended = sim.unit(a).unwrap().health - before;
    assert_eq!(mended, Fx::from_int(100), "a hundred in ten seconds");
    // And it builds the station, at its new pace: 100 of build time at 30 a second.
    sim.step(&[Command::Order { player: 0, units: vec![a], order: Order::Build { type_id: station, pos: FxVec3::from_ints(400, 60, 520) }, queue: false }]);
    let mut built = None;
    for k in 0..400 {
        sim.step(&[]);
        if sim.units().any(|o| o.type_id == station && o.is_complete()) {
            built = Some(k);
            break;
        }
    }
    let k = built.expect("the station built");
    assert!(k < 150, "at the suite's pace, not the ACU's own (ten seconds): {k} ticks");
}

/// Short of metal an install goes as slowly as construction does.
#[test]
fn an_enhancement_stalls_with_the_economy() {
    let mut sim = base();
    let a = acu(&mut sim);
    sim.players[0].economy.metal = Fx::ZERO;
    order(&mut sim, a, "SUITE", false, false);
    for _ in 0..60 {
        sim.step(&[]);
    }
    assert_eq!(sim.unit(a).unwrap().self_work.unwrap().work, 0, "nothing paid, nothing done");
    assert!(sim.unit(a).unwrap().enhancements.is_empty());
}

/// One needing another waits for it: alone, the order fails and nothing is paid; after it, it
/// takes its slot in its place (its numbers in place of the other's, not on top of them). One a
/// slot: the cloak takes the missile's place.
#[test]
fn an_enhancement_needs_its_prerequisite_and_takes_its_slot() {
    let mut sim = base();
    let a = acu(&mut sim);
    let metal = sim.players[0].economy.metal;
    order(&mut sim, a, "SUITE3", false, false);
    let events = sim.take_events();
    assert!(events.iter().any(|e| matches!(e, SimEvent::OrderFailed { unit, .. } if *unit == a)), "refused without the suite");
    assert!(sim.unit(a).unwrap().orders.is_empty() && sim.unit(a).unwrap().self_work.is_none());
    assert_eq!(sim.players[0].economy.metal, metal, "nothing paid");
    assert_eq!(sim.enhancement_fit(sim.unit(a).unwrap(), index(&sim, "SUITE3")), crate::EnhancementFit::Needs(index(&sim, "SUITE")));
    // The suite, then the better one queued after it.
    order(&mut sim, a, "SUITE", false, false);
    order(&mut sim, a, "SUITE3", false, true);
    until_installed(&mut sim, a, "SUITE", 400).expect("the suite");
    // (Free now, in the suite's place: its turn comes the next tick.)
    let fit = sim.enhancement_fit(sim.unit(a).unwrap(), index(&sim, "SUITE3"));
    assert_eq!(fit, crate::EnhancementFit::Free { replaces: Some(index(&sim, "SUITE")) });
    sim.step(&[]);
    assert_eq!(sim.enhancement_fit(sim.unit(a).unwrap(), index(&sim, "SUITE3")), crate::EnhancementFit::Installing);
    // 300 at 30 a second: ten seconds again (its first tick above).
    let ticks = until_installed(&mut sim, a, "SUITE3", 400).expect("the better suite");
    assert_eq!(ticks + 1, 300, "at the suite's build power");
    let t = sim.types.by_name("ACU").unwrap().clone();
    let u = sim.unit(a).unwrap();
    assert_eq!(u.enhancements, [index(&sim, "SUITE3")], "in the suite's place");
    assert_eq!(u.max_health(&t), Fx::from_int(3000), "its own and the better suite's, not the first's too");
    assert_eq!(u.work_rate(&t), Fx::from_int(60).0 as u64);
    let (s1, s3) = (sim.types.id("STATION").unwrap(), sim.types.id("STATION3").unwrap());
    assert!(sim.build_options(a).contains(&s1) && sim.build_options(a).contains(&s3));
    assert_eq!(sim.players[0].economy.metal, metal - Fx::from_int(900), "both paid for");
    // The missile, then the cloak in its slot.
    order(&mut sim, a, "MISSILE", false, false);
    until_installed(&mut sim, a, "MISSILE", 400).expect("the missile");
    let missile = sim.weapon_types.by_name("MISSILE").unwrap().id;
    assert_eq!(sim.unit(a).unwrap().weapon_slots(&t)[1], Some(missile), "its second slot armed");
    match sim.enhancement_fit(sim.unit(a).unwrap(), index(&sim, "CLOAK")) {
        crate::EnhancementFit::Free { replaces } => assert_eq!(replaces, Some(index(&sim, "MISSILE"))),
        other => panic!("{other:?}"),
    }
    order(&mut sim, a, "CLOAK", false, false);
    until_installed(&mut sim, a, "CLOAK", 400).expect("the cloak");
    let u = sim.unit(a).unwrap();
    let mut on = u.enhancements.clone();
    on.sort();
    let mut want = vec![index(&sim, "SUITE3"), index(&sim, "CLOAK")];
    want.sort();
    assert_eq!(on, want, "the missile gone for the cloak");
    assert_eq!(u.weapon_slots(&t)[1], None, "and its weapon with it");
}

/// Taken out, an enhancement is gone at once and for nothing: its weapon swapped back, its health
/// with it (the unit's health no more than its most).
#[test]
fn taking_an_enhancement_out_is_at_once_and_free() {
    let mut sim = base();
    let a = acu(&mut sim);
    let t = sim.types.by_name("ACU").unwrap().clone();
    let (gun, gun2) = (sim.weapon_types.by_name("GUN").unwrap().id, sim.weapon_types.by_name("GUN2").unwrap().id);
    assert_eq!(sim.unit(a).unwrap().weapon_slots(&t)[0], Some(gun));
    order(&mut sim, a, "GUN2", false, false);
    until_installed(&mut sim, a, "GUN2", 400).expect("the gun");
    assert_eq!(sim.unit(a).unwrap().weapon_slots(&t)[0], Some(gun2), "its gun swapped");
    assert_eq!(sim.weapon_types.get(gun2).range, Fx::from_int(600));
    order(&mut sim, a, "SUITE", false, false);
    until_installed(&mut sim, a, "SUITE", 400).expect("the suite");
    assert_eq!(sim.unit(a).unwrap().health, Fx::from_int(2000));
    let metal = sim.players[0].economy.metal;
    order(&mut sim, a, "GUN2", true, false);
    assert_eq!(sim.unit(a).unwrap().weapon_slots(&t)[0], Some(gun), "its own gun back at once");
    order(&mut sim, a, "SUITE", true, false);
    let u = sim.unit(a).unwrap();
    assert!(u.enhancements.is_empty() && u.boost.is_none(), "as it was built");
    assert_eq!((u.health, u.max_health(&t)), (Fx::from_int(1000), Fx::from_int(1000)), "its health down to its most");
    assert!(sim.players[0].economy.metal >= metal, "for nothing");
    let station = sim.types.id("STATION").unwrap();
    assert!(!sim.build_options(a).contains(&station), "the station off its menu again");
    // A removal of one not installed does nothing.
    order(&mut sim, a, "SUITE", true, false);
    assert!(sim.unit(a).unwrap().orders.is_empty());
}

/// Stop calls an install off: the metal paid for it comes back, its energy does not.
#[test]
fn stop_calls_an_install_off_and_gives_its_metal_back() {
    let mut sim = base();
    let a = acu(&mut sim);
    let (metal, energy) = (sim.players[0].economy.metal, sim.players[0].economy.energy);
    order(&mut sim, a, "SUITE", false, false);
    for _ in 0..149 {
        sim.step(&[]);
    }
    assert_eq!(metal - sim.players[0].economy.metal, Fx::from_int(150), "half of it paid");
    sim.step(&[Command::Order { player: 0, units: vec![a], order: Order::Stop, queue: false }]);
    let u = sim.unit(a).unwrap();
    assert!(u.self_work.is_none() && u.enhancements.is_empty(), "called off");
    assert_eq!(sim.players[0].economy.metal, metal, "its metal back");
    assert_eq!(energy - sim.players[0].economy.energy, Fx::from_int(1500), "its energy spent");
    // Another order in its place calls it off too (a move).
    order(&mut sim, a, "SUITE", false, false);
    for _ in 0..30 {
        sim.step(&[]);
    }
    sim.step(&[Command::Order { player: 0, units: vec![a], order: Order::Move(FxVec3::from_ints(600, 60, 600)), queue: false }]);
    assert!(sim.unit(a).unwrap().self_work.is_none(), "a new order calls it off");
}

/// Its sensors and a cloak: farther sight and radar, stealth, and a cloak it may switch on.
#[test]
fn an_enhancement_brings_its_sensors_and_cloak() {
    let mut sim = base();
    let a = acu(&mut sim);
    let t = sim.types.by_name("ACU").unwrap().clone();
    assert!(sim.unit(a).unwrap().cloak_costs(&t).is_none() && !sim.unit(a).unwrap().stealthy(&t));
    // (No cloak to switch on yet.)
    sim.step(&[Command::SetCloak { player: 0, units: vec![a], on: true }]);
    assert!(!sim.unit(a).unwrap().cloak_on);
    order(&mut sim, a, "CLOAK", false, false);
    until_installed(&mut sim, a, "CLOAK", 400).expect("the cloak");
    let u = sim.unit(a).unwrap();
    assert_eq!(u.radar(&t), Fx::from_int(500));
    assert_eq!(u.sight(&t), Fx::from_int(500));
    assert!(u.stealthy(&t));
    let (still, moving, _) = u.cloak_costs(&t).expect("a cloak");
    assert_eq!((still, moving), (Fx::from_int(50), Fx::from_int(100)));
    let energy = sim.players[0].economy.energy;
    sim.step(&[Command::SetCloak { player: 0, units: vec![a], on: true }]);
    for _ in 0..30 {
        sim.step(&[]);
    }
    assert!(sim.unit(a).unwrap().cloaked, "cloaked");
    let spent = energy - sim.players[0].economy.energy;
    assert!(spent >= Fx::from_int(49) && spent <= Fx::from_int(52), "50 energy a second standing: {spent}");
    // Taken out, the cloak goes off.
    order(&mut sim, a, "CLOAK", true, false);
    sim.step(&[]);
    assert!(!sim.unit(a).unwrap().cloak_on && !sim.unit(a).unwrap().cloaked);
}

/// The same orders make the same game, enhancements and all; and the checksum tells a unit with
/// one from a unit without.
#[test]
fn enhancements_are_the_same_on_every_computer() {
    let run = || {
        let mut sim = base();
        let a = acu(&mut sim);
        order(&mut sim, a, "SUITE", false, false);
        order(&mut sim, a, "GUN2", false, true);
        let mut sums = Vec::new();
        for _ in 0..700 {
            sim.step(&[]);
            sums.push(sim.checksum());
        }
        (sim, sums)
    };
    let (sim, a) = run();
    let (_, b) = run();
    assert_eq!(a, b, "the same game twice");
    let copy = sim.clone();
    assert_eq!(copy.checksum(), sim.checksum(), "a copy the same");
    let mut without = sim.clone();
    let id = sim.units().find(|u| !u.enhancements.is_empty()).map(|u| u.id).unwrap();
    without.unit_mut(id).unwrap().enhancements.clear();
    assert_ne!(without.checksum(), sim.checksum(), "its enhancements in the checksum");
}
