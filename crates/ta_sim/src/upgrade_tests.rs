//! Upgrading in place (`UpgradesTo`, `Order::Upgrade`) with made-up units: its cost and time, its
//! stall, calling it off, the unit put in the old one's place, a factory's work carried over, the
//! builders helping, and the same game on every computer.

use super::*;

/// A generator that upgrades twice (GEN1 into GEN2 into GEN3), a lab that upgrades (LAB1 into LAB2,
/// both making TANK), a builder, and a tower that upgrades into nothing.
fn base() -> Sim {
    let data = data_of(&[
        unit("GEN1", "WorkerTime=30; UpgradesTo=gen2; EnergyMake=0; MaxDamage=1000; FootprintX=2; FootprintZ=2; BuildTime=100; BuildCostMetal=50;"),
        unit("GEN2", "WorkerTime=60; UpgradesTo=GEN3; EnergyMake=0; MaxDamage=2000; FootprintX=2; FootprintZ=2; BuildTime=300; BuildCostMetal=300; BuildCostEnergy=3000;"),
        unit("GEN3", "MaxDamage=4000; FootprintX=2; FootprintZ=2; BuildTime=600; BuildCostMetal=900; BuildCostEnergy=9000;"),
        unit("POWER1", "WorkerTime=30; UpgradesTo=GEN2; EnergyMake=10; FootprintX=2; FootprintZ=2; BuildTime=100;"),
        unit("LAB1", "Builder=1; WorkerTime=30; UpgradesTo=LAB2; TEDClass=PLANT; FootprintX=4; FootprintZ=4; BuildTime=100; YardMap=oooooooooooooooo;"),
        unit("LAB2", "Builder=1; WorkerTime=60; TEDClass=PLANT; FootprintX=4; FootprintZ=4; BuildTime=900; BuildCostMetal=600; BuildCostEnergy=6000; YardMap=oooooooooooooooo;"),
        unit("TANK", "CanMove=1; MaxVelocity=1; Acceleration=0.1; BuildTime=300; BuildCostMetal=60; FootprintX=1; FootprintZ=1;"),
        unit("BUILDER", "Builder=1; WorkerTime=30; CanMove=1; MaxVelocity=1; BuildDistance=200; FootprintX=1; FootprintZ=1;"),
        unit("TOWER", "WorkerTime=30; FootprintX=1; FootprintZ=1;"),
    ]);
    let data = with_menu(with_menu(data, "LAB1", &["TANK"]), "LAB2", &["TANK"]);
    let mut sim = sim_of(data, 2);
    for p in &mut sim.players {
        p.endless = true;
    }
    sim.set_starting_resources(0, Fx::from_int(100_000), Fx::from_int(1_000_000));
    sim
}

fn put(sim: &mut Sim, name: &str, x: i64, z: i64) -> UnitId {
    sim.spawn_unit(sim.types.id(name).unwrap(), 0, FxVec3::from_ints(x, 60, z), 16384, true).unwrap()
}

fn upgrade(sim: &mut Sim, id: UnitId, to: &str, queue: bool) {
    let to = sim.types.id(to).unwrap();
    sim.step(&[Command::Order { player: 0, units: vec![id], order: Order::Upgrade(to), queue }]);
}

/// The unit of `name` standing nearest `id`'s place (the one an upgrade put there).
fn upgraded(sim: &Sim, events: &[SimEvent], from: UnitId) -> Option<UnitId> {
    events.iter().find_map(|e| match e {
        SimEvent::UnitUpgraded { from: f, to, .. } if *f == from => Some(*to),
        _ => None,
    }).filter(|&to| sim.unit(to).is_some())
}

/// Steps until `from` is upgraded (at most `ticks`), returning the new unit and the ticks taken.
fn until_upgraded(sim: &mut Sim, from: UnitId, ticks: u64) -> Option<(UnitId, u64)> {
    for k in 1..=ticks {
        sim.step(&[]);
        let events = sim.take_events();
        if let Some(to) = upgraded(sim, &events, from) {
            return Some((to, k));
        }
    }
    None
}

/// An upgrade takes the new type's build time at the unit's own build power and costs its price,
/// no more; done, a unit of that type stands where the old one stood, facing as it faced, with its
/// share of its health, and the old one is gone without a loss.
#[test]
fn an_upgrade_costs_its_types_price_over_its_build_time() {
    let mut sim = base();
    let gen1 = put(&mut sim, "GEN1", 300, 300);
    sim.set_health(gen1, Fx::from_int(500));
    let (pos, heading) = (sim.unit(gen1).unwrap().pos, sim.unit(gen1).unwrap().heading);
    let (metal, energy) = (sim.players[0].economy.metal, sim.players[0].economy.energy);
    upgrade(&mut sim, gen1, "GEN2", false);
    assert!(sim.unit(gen1).unwrap().self_work.is_some(), "under way from its first tick");
    // GEN2's 300 of build time at GEN1's 30 a second: ten seconds, 300 ticks (the first above).
    let (gen2, ticks) = until_upgraded(&mut sim, gen1, 400).expect("upgraded");
    assert_eq!(ticks + 1, 300, "its build time at its build power");
    let e = &sim.players[0].economy;
    assert_eq!(metal - e.metal, Fx::from_int(300), "its metal, all of it and no more");
    assert_eq!(energy - e.energy, Fx::from_int(3000), "its energy");
    let u = sim.unit(gen2).unwrap();
    assert_eq!(sim.types.get(u.type_id).name, "GEN2");
    assert_eq!((u.pos, u.heading, u.owner), (pos, heading, 0), "in its place, facing as it faced");
    assert_eq!(u.health, Fx::from_int(1000), "half its health, as the old one had");
    assert!(u.is_complete() && u.finished && u.self_work.is_none());
    for _ in 0..2 {
        sim.step(&[]);
    }
    assert!(sim.unit(gen1).is_none(), "the old one gone");
    assert_eq!((sim.players[0].stats.lost, sim.players[0].stats.built), (0, 1), "no loss; an upgrade built");
    assert_eq!(sim.players[0].unit_count, 1);
    assert!(sim.world.occupant(sim.unit(gen2).unwrap().occupied_origin.unwrap().0, sim.unit(gen2).unwrap().occupied_origin.unwrap().1) == crate::world::Occupant::Unit(gen2.slot), "its ground the new one's");
}

/// Short of metal the upgrade goes as slowly as construction does: none, nothing done; a trickle,
/// a share of the pace.
#[test]
fn an_upgrade_stalls_with_the_economy() {
    let mut sim = base();
    let gen1 = put(&mut sim, "GEN1", 300, 300);
    sim.players[0].economy.metal = Fx::ZERO;
    upgrade(&mut sim, gen1, "GEN2", false);
    for _ in 0..60 {
        sim.step(&[]);
    }
    assert_eq!(sim.unit(gen1).unwrap().self_work.unwrap().work, 0, "nothing paid, nothing done");
    // Half the metal it asks for a tick: about half the pace.
    let per_tick = Fx::from_int(300) / 300;
    let mut done = Vec::new();
    for _ in 0..60 {
        sim.players[0].economy.metal += per_tick / 2;
        sim.step(&[]);
        done.push(sim.unit(gen1).unwrap().self_work.unwrap().work);
    }
    let full = 60 * sim.types.by_name("GEN1").unwrap().work_rate;
    let got = *done.last().unwrap();
    assert!(got > full / 3 && got < full * 2 / 3, "about half the pace: {got} of {full}");
    assert!(sim.unit(gen1).is_some_and(|u| !u.dead), "still the old one");
}

/// Stop calls an upgrade off: the metal paid for it comes back, its energy does not, and the unit
/// is as it was.
#[test]
fn stop_calls_an_upgrade_off_and_gives_its_metal_back() {
    let mut sim = base();
    let gen1 = put(&mut sim, "GEN1", 300, 300);
    let (metal, energy) = (sim.players[0].economy.metal, sim.players[0].economy.energy);
    upgrade(&mut sim, gen1, "GEN2", false);
    for _ in 0..149 {
        sim.step(&[]);
    }
    // Half of it: 150 metal and 1,500 energy paid.
    assert_eq!(metal - sim.players[0].economy.metal, Fx::from_int(150));
    sim.step(&[Command::Order { player: 0, units: vec![gen1], order: Order::Stop, queue: false }]);
    let u = sim.unit(gen1).unwrap();
    assert!(u.self_work.is_none() && sim.types.get(u.type_id).name == "GEN1", "called off, as it was");
    assert_eq!(sim.players[0].economy.metal, metal, "its metal back");
    assert_eq!(energy - sim.players[0].economy.energy, Fx::from_int(1500), "its energy spent");
    // Taken back off its orders (a Shift-click), it is called off as well.
    let before = sim.players[0].economy.metal;
    upgrade(&mut sim, gen1, "GEN2", false);
    for _ in 0..30 {
        sim.step(&[]);
    }
    assert!(sim.players[0].economy.metal < before);
    let to = sim.types.id("GEN2").unwrap();
    sim.step(&[Command::DropOrder { player: 0, unit: gen1, index: 0, order: Order::Upgrade(to) }]);
    assert!(sim.unit(gen1).unwrap().self_work.is_none());
    assert_eq!(sim.players[0].economy.metal, before, "taken back, its metal back");
}

/// Upgrades queued one after another go up the ladder: GEN1 to GEN2, then GEN2 to GEN3, the
/// second order carried over to the unit the first made. A unit whose type upgrades into nothing,
/// or into something else, takes no such order.
#[test]
fn upgrades_queued_go_up_the_ladder() {
    let mut sim = base();
    let gen1 = put(&mut sim, "GEN1", 300, 300);
    upgrade(&mut sim, gen1, "GEN2", false);
    upgrade(&mut sim, gen1, "GEN3", true);
    let (gen2, _) = until_upgraded(&mut sim, gen1, 400).unwrap();
    assert_eq!(sim.unit(gen2).unwrap().orders.front(), Some(&Order::Upgrade(sim.types.id("GEN3").unwrap())), "the next step carried over");
    // GEN3's 600 at GEN2's 60: ten seconds.
    let (gen3, ticks) = until_upgraded(&mut sim, gen2, 400).unwrap();
    assert!((299..=301).contains(&ticks), "{ticks}");
    assert_eq!(sim.types.get(sim.unit(gen3).unwrap().type_id).name, "GEN3");
    // Not an upgrade of its type's.
    let tower = put(&mut sim, "TOWER", 600, 600);
    upgrade(&mut sim, tower, "GEN2", false);
    assert!(sim.unit(tower).unwrap().orders.is_empty() && sim.unit(tower).unwrap().self_work.is_none());
    let gen1b = put(&mut sim, "GEN1", 600, 300);
    upgrade(&mut sim, gen1b, "LAB2", false);
    assert!(sim.unit(gen1b).unwrap().orders.is_empty(), "not its upgrade");
    // Nor one this battle does not allow.
    let mut allowed = vec![true; sim.types.types.len()];
    allowed[sim.types.id("GEN2").unwrap()] = false;
    sim.allowed = Some(allowed);
    upgrade(&mut sim, gen1b, "GEN2", false);
    assert!(sim.unit(gen1b).unwrap().orders.is_empty(), "not allowed");
}

/// A generator makes power while it upgrades (Forged Alliance's extractors and generators work on).
#[test]
fn a_generator_works_on_while_it_upgrades() {
    let mut sim = base();
    let power = put(&mut sim, "POWER1", 300, 300);
    upgrade(&mut sim, power, "GEN2", false);
    for _ in 0..60 {
        sim.step(&[]);
    }
    assert!(sim.unit(power).unwrap().self_work.is_some());
    assert!((sim.players[0].economy.energy_income - Fx::from_int(10)).abs() < Fx::from_ratio(1, 100), "its 10 energy a second on");
}

/// A factory makes nothing while it upgrades: the unit on its pad waits, its queue too; done, the
/// new factory finishes that unit and goes on with the queue, its route kept.
#[test]
fn a_factory_makes_nothing_while_it_upgrades_and_carries_its_work_over() {
    let mut sim = base();
    let lab = put(&mut sim, "LAB1", 600, 600);
    // (No script here to ready it.)
    sim.unit_mut(lab).unwrap().in_build_stance = true;
    let tank = sim.types.id("TANK").unwrap();
    sim.step(&[Command::Produce { player: 0, factory: lab, type_id: tank, count: 3 }]);
    sim.step(&[Command::SetRally { player: 0, factory: lab, pos: FxVec3::from_ints(900, 0, 900) }]);
    for _ in 0..60 {
        sim.step(&[]);
    }
    let frame = sim.unit(lab).unwrap().build.map(|b| b.target).expect("a tank on the pad");
    upgrade(&mut sim, lab, "LAB2", false);
    let held = sim.unit(frame).unwrap().build_work;
    for _ in 0..100 {
        sim.step(&[]);
    }
    assert_eq!(sim.unit(frame).unwrap().build_work, held, "the unit on the pad waits");
    assert_eq!(sim.unit(lab).unwrap().factory_queue.len(), 3, "its queue too");
    // LAB2's 900 at LAB1's 30: thirty seconds.
    let (lab2, _) = until_upgraded(&mut sim, lab, 1000).expect("upgraded");
    let f = sim.unit(lab2).unwrap();
    assert_eq!(f.factory_queue.len(), 3);
    assert_eq!(f.build.map(|b| b.target), Some(frame), "the unit on its pad is the new one's");
    assert_eq!(f.rally, vec![Order::Move(FxVec3::from_ints(900, 0, 900))]);
    assert_eq!(sim.unit(frame).unwrap().factory, Some(lab2));
    for _ in 0..200 {
        sim.step(&[]);
    }
    assert!(sim.unit(frame).unwrap().is_complete(), "finished by the new one");
}

/// Builders guarding a factory that upgrades put their power into the upgrade, not into the unit on
/// its pad, which waits for the factory as it does.
#[test]
fn guards_help_a_factorys_upgrade_not_the_unit_on_its_pad() {
    let mut sim = base();
    let lab = put(&mut sim, "LAB1", 600, 600);
    sim.unit_mut(lab).unwrap().in_build_stance = true;
    let tank = sim.types.id("TANK").unwrap();
    sim.step(&[Command::Produce { player: 0, factory: lab, type_id: tank, count: 3 }]);
    for _ in 0..60 {
        sim.step(&[]);
    }
    let frame = sim.unit(lab).unwrap().build.map(|b| b.target).expect("a tank on the pad");
    let builder = put(&mut sim, "BUILDER", 600, 700);
    sim.step(&[Command::Order { player: 0, units: vec![builder], order: Order::Guard(lab), queue: false }]);
    upgrade(&mut sim, lab, "LAB2", false);
    let held = sim.unit(frame).unwrap().build_work;
    let start = sim.unit(lab).unwrap().self_work.unwrap().work;
    let ticks = 120;
    for _ in 0..ticks {
        sim.step(&[]);
    }
    assert_eq!(sim.unit(frame).unwrap().build_work, held, "the unit on the pad waits, guards and all");
    let done = sim.unit(lab).unwrap().self_work.expect("still upgrading").work - start;
    let alone = ticks * sim.types.by_name("LAB1").unwrap().work_rate;
    assert!(done > alone * 3 / 2, "the guard's power in the upgrade: {done} against {alone} alone");
}

/// Builders helping (guarding it, or told to repair it) lend their build power to the upgrade,
/// their side paying for their share.
#[test]
fn builders_guarding_an_upgrade_lend_their_power() {
    let mut alone = base();
    let gen1 = put(&mut alone, "GEN1", 300, 300);
    let mut helped = alone.clone();
    upgrade(&mut alone, gen1, "GEN2", false);
    let (_, by_itself) = until_upgraded(&mut alone, gen1, 400).unwrap();
    let builder = put(&mut helped, "BUILDER", 380, 300);
    helped.step(&[Command::Order { player: 0, units: vec![builder], order: Order::Guard(gen1), queue: false }]);
    upgrade(&mut helped, gen1, "GEN2", false);
    let (gen2, with_help) = until_upgraded(&mut helped, gen1, 400).unwrap();
    assert!(with_help * 10 < by_itself * 7, "{with_help} ticks helped, {by_itself} alone");
    assert_eq!(helped.unit(builder).unwrap().orders.back(), Some(&Order::Guard(gen2)), "guarding the new one");
}

/// The same orders give the same game, tick for tick, and a copy taken during an upgrade plays on
/// as the original.
#[test]
fn an_upgrade_plays_the_same_on_every_computer() {
    let run = || {
        let mut sim = base();
        let gen1 = put(&mut sim, "GEN1", 300, 300);
        let lab = put(&mut sim, "LAB1", 600, 600);
        let tank = sim.types.id("TANK").unwrap();
        sim.step(&[Command::Produce { player: 0, factory: lab, type_id: tank, count: 2 }]);
        upgrade(&mut sim, gen1, "GEN2", false);
        upgrade(&mut sim, lab, "LAB2", false);
        let mut sums = Vec::new();
        for _ in 0..200 {
            sim.step(&[]);
            sums.push(sim.checksum());
        }
        (sim, sums)
    };
    let (mut a, first) = run();
    let (_, second) = run();
    assert_eq!(first, second);
    let mut copy = a.clone();
    for _ in 0..400 {
        a.step(&[]);
        copy.step(&[]);
        assert_eq!(a.checksum(), copy.checksum());
    }
    // The checksum sees the work under way.
    let mut sim = base();
    let gen1 = put(&mut sim, "GEN1", 300, 300);
    let before = sim.checksum();
    sim.unit_mut(gen1).unwrap().self_work = Some(crate::unit::SelfWork { job: crate::unit::SelfJob::Upgrade(1), work: 5 });
    assert_ne!(sim.checksum(), before);
}
