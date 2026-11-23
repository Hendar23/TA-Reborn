//! Adjacency (`[ADJACENCY]`, `AdjacencySize`: Supreme Commander's) with made-up buildings: which
//! touch, the bonuses each takes and what they change (what building costs, upkeep, what is made, a
//! shot's energy and a reload), the most neighbours a size class takes, a frame given up giving
//! back what was paid, and the bonuses following buildings finished, lost, upgraded and given away.

use ta_data::AdjacencyAffect;

use super::*;

/// A power generator giving buildings of size 16 a cheaper build and those of size 4 a cheaper
/// upkeep (artillery of that size a quicker reload); storage raising what extractors of size 4
/// make; a workshop of size 16 building a frame; an extractor, a radar and two guns of size 4 (one
/// of them artillery); a station with no size class; a workshop that upgrades; a plant that
/// upgrades into one giving more; a builder that takes buildings apart.
fn base() -> Sim {
    let power = "FootprintX=2; FootprintZ=2; EnergyMake=10; AdjacencySize=4; BuildTime=100; BuildCostMetal=40;\n\
        [ADJACENCY]{\n\
        [Build16]{Affects=BuildEnergy; Add=-0.25; Size=16;}\n\
        [Metal16]{Affects=BuildMetal; Add=-0.5; Size=16;}\n\
        [Upkeep4]{Affects=EnergyUse; Add=-0.5; Size=4;}\n\
        [Shot4]{Affects=EnergyPerShot; Add=-0.5; Size=4;}\n\
        [Fire4]{Affects=ReloadTime; Add=-0.5; Size=4; Category=ARTILLERY;}\n\
        [Odd]{Affects=Shields; Add=-0.5;}\n\
        }";
    let data = data_of(&[
        unit("GEN", power),
        unit("STORE", "FootprintX=2; FootprintZ=2; MetalStorage=100; AdjacencySize=4;\n[ADJACENCY]{\n[More4]{Affects=MetalMake; Add=0.25; Size=4;}\n}"),
        unit("WORKSHOP", "Builder=1; WorkerTime=30; BuildDistance=300; FootprintX=4; FootprintZ=4; AdjacencySize=16; UpgradesTo=WORKSHOP2;"),
        unit("WORKSHOP2", "Builder=1; WorkerTime=30; BuildDistance=300; FootprintX=4; FootprintZ=4; AdjacencySize=16; BuildTime=300; BuildCostMetal=300; BuildCostEnergy=3000;"),
        unit("FRAME", "FootprintX=2; FootprintZ=2; BuildTime=300; BuildCostMetal=100; BuildCostEnergy=1000;"),
        unit("MEX", "FootprintX=2; FootprintZ=2; ExtractsMetal=0.001; EnergyUse=4; ActivateWhenBuilt=1; AdjacencySize=4;"),
        unit("RADAR", "FootprintX=2; FootprintZ=2; EnergyUse=20; OnOffable=1; ActivateWhenBuilt=1; RadarDistance=500; AdjacencySize=4;"),
        unit("GUN", "FootprintX=2; FootprintZ=2; Weapon1=SHELL; Category=ARTILLERY; AdjacencySize=4;"),
        unit("GUN2", "FootprintX=2; FootprintZ=2; Weapon1=SHELL; Category=FORT; AdjacencySize=4;"),
        unit("STATION", "Builder=1; WorkerTime=30; BuildDistance=300; FootprintX=2; FootprintZ=2;"),
        // A plant that upgrades into one giving more, and a builder that takes buildings apart.
        unit("PLANT1", "FootprintX=2; FootprintZ=2; WorkerTime=30; UpgradesTo=PLANT2; AdjacencySize=4;\n[ADJACENCY]{\n[Build16]{Affects=BuildEnergy; Add=-0.1; Size=16;}\n}"),
        unit("PLANT2", "FootprintX=2; FootprintZ=2; BuildTime=60; BuildCostMetal=10; AdjacencySize=4;\n[ADJACENCY]{\n[Build16]{Affects=BuildEnergy; Add=-0.3; Size=16;}\n}"),
        unit("WRECKER", "Builder=1; WorkerTime=600; BuildDistance=300; CanReclamate=1; FootprintX=2; FootprintZ=2;"),
        weapon_file("SHELL", "energypershot=40;"),
    ]);
    let mut sim = sim_of(data, 2);
    sim.set_team(1, 0);
    for p in &mut sim.players {
        p.endless = true;
    }
    sim.set_starting_resources(0, Fx::from_int(100_000), Fx::from_int(1_000_000));
    sim
}

/// A building of `name`, finished (or not), its footprint's top-left cell at `origin`.
fn at(sim: &mut Sim, name: &str, owner: u8, origin: (i32, i32), complete: bool) -> UnitId {
    let t = sim.types.id(name).unwrap();
    let fp = sim.types.get(t).footprint;
    let pos = FxVec3::from_ints(origin.0 as i64 * 16 + fp.0 as i64 * 8, 60, origin.1 as i64 * 16 + fp.1 as i64 * 8);
    let id = sim.spawn_unit(t, owner, pos, 0, complete).unwrap();
    assert_eq!(sim.unit(id).unwrap().occupied_origin, Some(origin), "{name} where it was put");
    id
}

fn change(sim: &Sim, id: UnitId, affect: AdjacencyAffect) -> f64 {
    sim.unit(id).unwrap().adjacency.as_ref().map_or(0.0, |a| a.change[affect.index()].to_f64())
}

fn givers(sim: &Sim, id: UnitId) -> Vec<UnitId> {
    sim.unit(id).unwrap().adjacency.as_ref().map_or_else(Vec::new, |a| a.from.iter().map(|&(g, _)| g).collect())
}

/// Buildings touch where their footprints meet along an edge: not at a corner alone, a cell apart,
/// or across players; a bonus goes to the size it names, to a building with every token of its
/// category, and to one with something for it to change.
#[test]
fn bonuses_go_to_the_buildings_touching_that_take_them() {
    let mut sim = base();
    let shop = at(&mut sim, "WORKSHOP", 0, (20, 20), true);
    let power = at(&mut sim, "GEN", 0, (24, 21), true);
    let corner = at(&mut sim, "GEN", 0, (18, 24), true);
    let apart = at(&mut sim, "GEN", 0, (20, 25), true);
    let theirs = at(&mut sim, "GEN", 1, (18, 20), true);
    let radar = at(&mut sim, "RADAR", 0, (24, 19), true);
    let gun = at(&mut sim, "GUN", 0, (26, 21), true);
    let gun2 = at(&mut sim, "GUN2", 0, (24, 23), false);
    sim.step(&[]);
    assert_eq!(givers(&sim, shop), [power], "the one along its edge: not at its corner, a cell apart or another player's");
    assert_eq!((change(&sim, shop, AdjacencyAffect::BuildEnergy), change(&sim, shop, AdjacencyAffect::BuildMetal)), (-0.25, -0.5), "its size's bonuses");
    assert_eq!(change(&sim, shop, AdjacencyAffect::EnergyUse), 0.0, "another size's not, and nothing to cut");
    // (The radar along the generator's top edge: its upkeep; the gun along its east edge,
    // artillery: its reload and its shots' energy.)
    assert_eq!(givers(&sim, radar), [power]);
    assert_eq!(change(&sim, radar, AdjacencyAffect::EnergyUse), -0.5);
    assert_eq!((change(&sim, gun, AdjacencyAffect::ReloadTime), change(&sim, gun, AdjacencyAffect::EnergyPerShot)), (-0.5, -0.5));
    assert!(sim.unit(gun2).unwrap().adjacency.is_none(), "unfinished, it takes nothing");
    assert!([theirs, corner, apart].iter().all(|&g| sim.unit(g).unwrap().adjacency.is_none()));
    // What the generator gives, each with what it changes.
    let bit = |a: AdjacencyAffect| 1u8 << a.index();
    let given = sim.adjacency_given(sim.unit(power).unwrap());
    assert_eq!(given, [(shop, bit(AdjacencyAffect::BuildEnergy) | bit(AdjacencyAffect::BuildMetal)), (radar, bit(AdjacencyAffect::EnergyUse)), (gun, bit(AdjacencyAffect::EnergyPerShot) | bit(AdjacencyAffect::ReloadTime))]);
    // Finished, the second gun (no artillery) takes the shots' cut alone.
    sim.finish_frame(gun2);
    sim.step(&[]);
    assert_eq!((change(&sim, gun2, AdjacencyAffect::ReloadTime), change(&sim, gun2, AdjacencyAffect::EnergyPerShot)), (0.0, -0.5));
    // A station without a size class takes none of the bonuses for sizes.
    let station = at(&mut sim, "STATION", 0, (40, 40), true);
    at(&mut sim, "GEN", 0, (42, 40), true);
    sim.step(&[]);
    assert!(sim.unit(station).unwrap().adjacency.is_none());
}

/// A builder beside a generator pays less for what it builds, as it goes, its build as long as
/// ever; the figures say so; given up, a frame gives back what was paid for it, not its price.
#[test]
fn a_builder_beside_a_generator_pays_less_and_gets_back_what_it_paid() {
    let mut sim = base();
    let shop = at(&mut sim, "WORKSHOP", 0, (20, 20), true);
    at(&mut sim, "GEN", 0, (24, 21), true);
    let frame = at(&mut sim, "FRAME", 0, (14, 20), false);
    sim.step(&[]);
    let (metal, energy) = (sim.players[0].stats.metal_spent, sim.players[0].stats.energy_spent);
    sim.step(&[Command::Order { player: 0, units: vec![shop], order: Order::Repair(frame), queue: false }]);
    let mut ticks = 0;
    while !sim.unit(frame).unwrap().is_complete() {
        sim.step(&[]);
        ticks += 1;
        if ticks == 30 {
            // (A second in: its draw, a second's share of the build at its price beside the generator.)
            let f = sim.unit_flows(sim.unit(shop).unwrap());
            assert!((f.metal_use.to_f64() - 100.0 * 0.5 * 30.0 / 300.0).abs() < 0.01 && (f.energy_use.to_f64() - 1000.0 * 0.75 * 30.0 / 300.0).abs() < 0.01, "{f:?}");
        }
        assert!(ticks < 400);
    }
    assert!((299..=305).contains(&ticks), "as long as ever: {ticks} ticks");
    assert_eq!(sim.players[0].stats.metal_spent - metal, Fx::from_int(50), "half the metal");
    assert_eq!(sim.players[0].stats.energy_spent - energy, Fx::from_int(750), "three quarters of the energy");
    // Half a frame, given up: what was paid comes back as it un-builds, its price's half no more.
    let frame = at(&mut sim, "FRAME", 0, (14, 24), false);
    sim.step(&[Command::Order { player: 0, units: vec![shop], order: Order::Repair(frame), queue: false }]);
    for _ in 0..150 {
        sim.step(&[]);
    }
    sim.step(&[Command::Order { player: 0, units: vec![shop], order: Order::Stop, queue: false }]);
    let paid = sim.players[0].stats.metal_spent - metal - Fx::from_int(50);
    assert!(paid > Fx::from_int(20) && paid < Fx::from_int(30), "paid {paid:?}");
    let before = sim.players[0].economy.metal;
    for _ in 0..(FRAME_GRACE + FRAME_DECAY + 60) {
        sim.step(&[]);
    }
    assert!(sim.unit(frame).is_none_or(|u| u.dead), "un-built");
    let back = sim.players[0].economy.metal - before;
    assert!((back - paid).abs() <= Fx(4), "back {back:?}, paid {paid:?}");
}

/// Storage beside an extractor raises what it makes; a generator beside it cuts its upkeep; the
/// units' figures add up to the economy's.
#[test]
fn storage_raises_an_extractor_and_the_figures_add_up() {
    let mut sim = base();
    for (x, z) in [(10, 10), (40, 10)] {
        for dz in 0..2 {
            for dx in 0..2 {
                sim.world.set_metal(x + dx, z + dz, 100);
            }
        }
    }
    let lone = at(&mut sim, "MEX", 0, (40, 10), true);
    let mex = at(&mut sim, "MEX", 0, (10, 10), true);
    let stores = [at(&mut sim, "STORE", 0, (12, 10), true), at(&mut sim, "STORE", 0, (8, 10), true)];
    let power = at(&mut sim, "GEN", 0, (10, 12), true);
    for _ in 0..3 {
        sim.step(&[]);
    }
    let (f, l) = (sim.unit_flows(sim.unit(mex).unwrap()), sim.unit_flows(sim.unit(lone).unwrap()));
    assert!(l.metal_make > Fx::ZERO);
    assert!((f.metal_make.to_f64() - l.metal_make.to_f64() * 1.5).abs() < 0.01, "two storages, a quarter more each: {f:?} against {l:?}");
    assert!((f.energy_use.to_f64() - 2.0).abs() < 0.01 && (l.energy_use.to_f64() - 4.0).abs() < 0.01, "half the upkeep beside the generator: {f:?}");
    assert_eq!(givers(&sim, mex), [stores[0], stores[1], power]);
    let (mut metal, mut energy_make, mut energy_use) = (Fx::ZERO, Fx::ZERO, Fx::ZERO);
    for u in sim.units() {
        let f = sim.unit_flows(u);
        (metal, energy_make, energy_use) = (metal + f.metal_make, energy_make + f.energy_make, energy_use + f.energy_use);
    }
    let e = &sim.players[0].economy;
    assert_eq!((metal, energy_make, energy_use), (e.metal_income, e.energy_income, e.energy_expense), "the figures add up to the economy's");
}

/// A building takes bonuses from at most as many neighbours as its size class holds: those along
/// the most of its edge first.
#[test]
fn a_size_class_holds_so_many_neighbours() {
    let mut sim = base();
    // An extractor (size 4) with five storages round it: four sides, one along a cell of its edge.
    let mex = at(&mut sim, "MEX", 0, (10, 10), true);
    let half = at(&mut sim, "STORE", 0, (11, 8), true);
    let whole: Vec<UnitId> = [(12, 10), (8, 10), (10, 12), (9, 8)].into_iter().map(|o| at(&mut sim, "STORE", 0, o, true)).collect();
    sim.step(&[]);
    // (Of the two along its top, each a cell of it, the earlier; the three along whole sides.)
    assert_eq!(givers(&sim, mex).len(), 4);
    assert!((change(&sim, mex, AdjacencyAffect::MetalMake) - 1.0).abs() < 1e-4, "four quarters");
    assert_eq!(givers(&sim, mex), [half, whole[0], whole[1], whole[2]]);
}

/// The bonuses follow the buildings: one finished gives, one lost or given away gives no more, and
/// one upgraded in place takes its neighbours' bonuses in its new form; the same twice over.
#[test]
fn bonuses_follow_buildings_finished_lost_upgraded_and_given() {
    let run = || {
        let mut sim = base();
        let shop = at(&mut sim, "WORKSHOP", 0, (20, 20), true);
        let power = at(&mut sim, "GEN", 0, (24, 21), false);
        let other = at(&mut sim, "GEN", 0, (21, 18), true);
        sim.step(&[]);
        assert_eq!(givers(&sim, shop), [other]);
        sim.finish_frame(power);
        sim.step(&[]);
        assert_eq!(givers(&sim, shop), [power, other], "finished, it gives");
        assert_eq!(change(&sim, shop, AdjacencyAffect::BuildMetal), -1.0);
        assert_eq!(sim.unit(shop).unwrap().adjacency_factor(AdjacencyAffect::BuildMetal), Some(Fx::ZERO), "never below none");
        sim.remove_unit(other);
        sim.step(&[]);
        assert_eq!(givers(&sim, shop), [power], "lost, it gives no more");
        // Upgraded, the new workshop takes the generator's bonus.
        let to = sim.types.id("WORKSHOP2").unwrap();
        sim.step(&[Command::Order { player: 0, units: vec![shop], order: Order::Upgrade(to), queue: false }]);
        let mut new = None;
        for _ in 0..400 {
            sim.step(&[]);
            new = new.or_else(|| sim.take_events().iter().find_map(|e| match e {
                SimEvent::UnitUpgraded { from, to, .. } if *from == shop => Some(*to),
                _ => None,
            }));
            if new.is_some() {
                break;
            }
        }
        let new = new.expect("upgraded");
        sim.step(&[]);
        assert_eq!(givers(&sim, new), [power], "upgraded, it takes its neighbour's bonus");
        assert_eq!(sim.adjacency_given(sim.unit(power).unwrap()).iter().map(|&(g, _)| g).collect::<Vec<_>>(), [new]);
        // Given to an ally, the generator gives nothing to its old player's workshop.
        sim.apply(&Command::GiveUnits { player: 0, units: vec![power], to: 1 });
        sim.step(&[]);
        assert!(sim.unit(new).unwrap().adjacency.is_none(), "another player's gives nothing");
        sim.checksum()
    };
    assert_eq!(run(), run(), "the same game twice");
}

/// Upgrading in place beside a generator pays less as it goes, and called off gives back what
/// was paid.
#[test]
fn an_upgrade_beside_a_generator_pays_less() {
    let mut sim = base();
    let shop = at(&mut sim, "WORKSHOP", 0, (20, 20), true);
    at(&mut sim, "GEN", 0, (24, 21), true);
    sim.step(&[]);
    let (metal, energy) = (sim.players[0].stats.metal_spent, sim.players[0].stats.energy_spent);
    let to = sim.types.id("WORKSHOP2").unwrap();
    sim.step(&[Command::Order { player: 0, units: vec![shop], order: Order::Upgrade(to), queue: false }]);
    for _ in 0..150 {
        sim.step(&[]);
    }
    let (paid, paid_energy) = (sim.players[0].stats.metal_spent - metal, sim.players[0].stats.energy_spent - energy);
    // (About half the work in five seconds: 300 × 30 / 300 a second's tenth.)
    let share = sim.self_work_progress(sim.unit(shop).unwrap()).unwrap().1.to_f64();
    assert!((paid.to_f64() - 300.0 * 0.5 * share).abs() < 1.0 && (paid_energy.to_f64() - 3000.0 * 0.75 * share).abs() < 2.0, "paid {paid:?} {paid_energy:?} at {share}");
    let before = sim.players[0].economy.metal;
    sim.step(&[Command::Order { player: 0, units: vec![shop], order: Order::Stop, queue: false }]);
    sim.step(&[]);
    let back = sim.players[0].economy.metal - before;
    assert!((back - paid).abs() <= Fx(4), "back {back:?}, paid {paid:?}");
}

/// A gun beside a generator: its shots cost less energy; an artillery piece reloads faster.
#[test]
fn guns_beside_a_generator_fire_for_less_and_artillery_sooner() {
    let mut sim = base();
    let power = at(&mut sim, "GEN", 0, (20, 20), true);
    let gun = at(&mut sim, "GUN", 0, (22, 20), true);
    let lone = at(&mut sim, "GUN", 0, (40, 40), true);
    sim.step(&[]);
    let w = sim.weapon_types.get(sim.types.get(sim.unit(gun).unwrap().type_id).weapons[0].unwrap()).clone();
    assert_eq!((sim.shot_energy(gun, &w), sim.shot_energy(lone, &w)), (Fx::from_int(20), Fx::from_int(40)));
    assert_eq!(sim.unit(gun).unwrap().adjacency_factor(AdjacencyAffect::ReloadTime), Some(Fx::from_ratio(1, 2)));
    assert_eq!(givers(&sim, gun), [power]);
    assert!(sim.unit(lone).unwrap().adjacency.is_none());
}

/// A game without a type giving any bonus (TA's own) keeps nothing of adjacency.
#[test]
fn without_bonuses_nothing_of_adjacency_runs() {
    let data = data_of(&[unit("SOLAR", "FootprintX=2; FootprintZ=2; EnergyMake=10; AdjacencySize=4;"), unit("LAB", "FootprintX=4; FootprintZ=4; AdjacencySize=16;")]);
    let mut sim = sim_of(data, 1);
    assert!(!sim.types.adjacency);
    let lab = at(&mut sim, "LAB", 0, (20, 20), true);
    at(&mut sim, "SOLAR", 0, (24, 21), true);
    sim.step(&[]);
    assert!(sim.unit(lab).unwrap().adjacency.is_none() && sim.unit(lab).unwrap().adjacency_key.is_none());
}

/// An upgrade whose next form takes more ground than it stands on (a factory a row longer) is
/// refused while another building stands there (put beside it for its bonuses), and goes ahead
/// once the ground is free.
#[test]
fn an_upgrade_needs_room_for_what_it_becomes() {
    let data = data_of(&[
        unit("GROW1", "FootprintX=2; FootprintZ=2; WorkerTime=30; UpgradesTo=GROW2; BuildTime=100;"),
        unit("GROW2", "FootprintX=2; FootprintZ=3; BuildTime=30; BuildCostMetal=10;"),
        unit("BLOCK", "FootprintX=2; FootprintZ=1; BuildTime=100;"),
    ]);
    let mut sim = sim_of(data, 1);
    sim.set_starting_resources(0, Fx::from_int(10_000), Fx::from_int(10_000));
    let grow = at(&mut sim, "GROW1", 0, (20, 20), true);
    let block = at(&mut sim, "BLOCK", 0, (20, 19), true);
    let to = sim.types.id("GROW2").unwrap();
    assert!(!sim.room_to_become(grow, to));
    sim.take_events();
    sim.step(&[Command::Order { player: 0, units: vec![grow], order: Order::Upgrade(to), queue: false }]);
    sim.step(&[]);
    let failed = sim.take_events().iter().any(|e| matches!(e, SimEvent::OrderFailed { unit, reason } if *unit == grow && reason.contains("no room")));
    assert!(failed && sim.unit(grow).unwrap().orders.is_empty() && sim.unit(grow).unwrap().self_work.is_none(), "refused");
    sim.remove_unit(block);
    sim.step(&[]);
    assert!(sim.room_to_become(grow, to));
    sim.step(&[Command::Order { player: 0, units: vec![grow], order: Order::Upgrade(to), queue: false }]);
    for _ in 0..60 {
        sim.step(&[]);
    }
    assert!(sim.units().any(|u| u.type_id == to && !u.dead), "upgraded once the ground was free");
}

/// A building lost in battle (killed, not taken off the map) gives no more, nor is it among those
/// its neighbour helps; a building beside it builds at its whole price again.
#[test]
fn a_building_killed_gives_and_takes_no_more() {
    let mut sim = base();
    let shop = at(&mut sim, "WORKSHOP", 0, (20, 20), true);
    let power = at(&mut sim, "GEN", 0, (24, 21), true);
    let radar = at(&mut sim, "RADAR", 0, (24, 19), true);
    sim.step(&[]);
    let helped = |sim: &Sim| sim.adjacency_given(sim.unit(power).unwrap()).iter().map(|&(g, _)| g).collect::<Vec<_>>();
    assert_eq!(helped(&sim), [shop, radar]);
    sim.damage_unit(radar, Fx::from_int(100_000), power, FxVec3::ZERO);
    sim.step(&[]);
    assert_eq!(helped(&sim), [shop], "the radar killed: the workshop alone");
    sim.damage_unit(power, Fx::from_int(100_000), shop, FxVec3::ZERO);
    for _ in 0..3 {
        sim.step(&[]);
    }
    assert!(sim.unit(shop).unwrap().adjacency.is_none(), "the generator killed: nothing");
    assert_eq!(sim.unit(shop).unwrap().adjacency_factor(AdjacencyAffect::BuildEnergy), None);
}

/// A building that gives, upgraded in place, gives what its new form gives (the slot it stood in
/// another's now).
#[test]
fn a_giver_upgraded_gives_what_its_new_form_gives() {
    let mut sim = base();
    let shop = at(&mut sim, "WORKSHOP", 0, (20, 20), true);
    let plant = at(&mut sim, "PLANT1", 0, (24, 21), true);
    sim.step(&[]);
    assert!((change(&sim, shop, AdjacencyAffect::BuildEnergy) + 0.1).abs() < 1e-4);
    let to = sim.types.id("PLANT2").unwrap();
    sim.step(&[Command::Order { player: 0, units: vec![plant], order: Order::Upgrade(to), queue: false }]);
    let mut new = None;
    for _ in 0..200 {
        sim.step(&[]);
        new = new.or_else(|| sim.take_events().iter().find_map(|e| match e {
            SimEvent::UnitUpgraded { from, to, .. } if *from == plant => Some(*to),
            _ => None,
        }));
        if new.is_some() {
            break;
        }
    }
    let new = new.expect("upgraded");
    sim.step(&[]);
    assert_eq!(givers(&sim, shop), [new]);
    assert!((change(&sim, shop, AdjacencyAffect::BuildEnergy) + 0.3).abs() < 1e-4, "its new form's bonus");
}

/// A unit built for less beside a generator, taken apart by its side, gives back what was paid for
/// it, not its price; so does one upgraded for less (what the upgrade was paid): building beside a
/// generator and taking apart makes no metal.
#[test]
fn a_unit_built_for_less_gives_back_what_was_paid_when_taken_apart() {
    let mut sim = base();
    // (Room in the store for what comes back.)
    sim.players[0].economy.metal = Fx::from_int(5_000);
    let shop = at(&mut sim, "WORKSHOP", 0, (20, 20), true);
    at(&mut sim, "GEN", 0, (24, 21), true);
    let wrecker = at(&mut sim, "WRECKER", 0, (14, 26), true);
    let frame = at(&mut sim, "FRAME", 0, (14, 20), false);
    sim.step(&[]);
    let spent = sim.players[0].stats.metal_spent;
    sim.step(&[Command::Order { player: 0, units: vec![shop], order: Order::Repair(frame), queue: false }]);
    for _ in 0..400 {
        sim.step(&[]);
        if sim.unit(frame).unwrap().is_complete() {
            break;
        }
    }
    assert!(sim.unit(frame).unwrap().is_complete());
    let paid = sim.players[0].stats.metal_spent - spent;
    assert_eq!(paid, Fx::from_int(50), "half its price");
    let taken_apart = |sim: &mut Sim, target: UnitId| {
        let before = sim.players[0].economy.metal;
        sim.step(&[Command::Order { player: 0, units: vec![wrecker], order: Order::ReclaimUnit(target), queue: false }]);
        for _ in 0..600 {
            sim.step(&[]);
            if sim.unit(target).is_none_or(|u| u.dead) {
                break;
            }
        }
        assert!(sim.unit(target).is_none_or(|u| u.dead), "taken apart");
        sim.players[0].economy.metal - before
    };
    let back = taken_apart(&mut sim, frame);
    assert!((back - paid).abs() <= Fx(4), "back {back:?}, paid {paid:?}: not its price");
    // Upgraded beside the generator, for half the metal: taken apart, that half back.
    let spent = sim.players[0].stats.metal_spent;
    let to = sim.types.id("WORKSHOP2").unwrap();
    sim.step(&[Command::Order { player: 0, units: vec![shop], order: Order::Upgrade(to), queue: false }]);
    let mut new = None;
    for _ in 0..600 {
        sim.step(&[]);
        new = new.or_else(|| sim.take_events().iter().find_map(|e| match e {
            SimEvent::UnitUpgraded { from, to, .. } if *from == shop => Some(*to),
            _ => None,
        }));
        if new.is_some() {
            break;
        }
    }
    let new = new.expect("upgraded");
    let paid = sim.players[0].stats.metal_spent - spent;
    assert!((paid - Fx::from_int(150)).abs() <= Fx(4), "half the upgrade's price: {paid:?}");
    assert_eq!(sim.unit(new).unwrap().metal_saved, Fx::from_int(300) - paid);
    let back = taken_apart(&mut sim, new);
    assert!((back - paid).abs() <= Fx(4), "back {back:?}, paid {paid:?}");
}
