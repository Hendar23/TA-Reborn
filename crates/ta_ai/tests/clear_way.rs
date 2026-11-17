//! Wreckage shutting a pass, on real game data (skipped without a TA install): The Pass, where
//! the wrecks of a scenario's first wave of kbots shut the way for every wave after them. A
//! computer player whose army is sent through it clears the way: its constructors (the
//! Commander among them, near and safe) reclaim the wreckage, its army breaks it up when none
//! can come; and the army goes on through.
//!
//! `TA_TEST_LOG=1` prints the computer player's debug log.

mod common;

use ta_ai::{Difficulty, SkirmishAi};
use ta_sim::{Command, Fx, FxVec3, Order, PlayerSetup, Sim, Target, UnitId};

/// Where Last Stand's first wave left its wrecks across The Pass (from the east), in pixels.
const WRECKS: [(i64, i64); 6] = [(1381, 371), (1407, 422), (1408, 399), (1454, 385), (1410, 351), (1445, 441)];

/// The Pass with ARM in the west (start 1, nobody plays it) and CORE in the east (start 2),
/// each with its Commander; the wrecks across the pass; and eight of CORE's kbots by its start,
/// with a construction kbot when `constructor`. Our side's `SkirmishAi` (CORE's) and the kbots.
fn the_pass(constructor: bool, commander: bool) -> Option<(Sim, SkirmishAi, Vec<UnitId>, Vec<usize>, FxVec3)> {
    let d = common::data()?;
    let m = common::map("The Pass")?;
    let starts = &m.info.schemas[0].start_positions;
    let spot = |k: usize| FxVec3::new(Fx::from_f32(starts[k].1), Fx::ZERO, Fx::from_f32(starts[k].2));
    let players: Vec<PlayerSetup> = [("ARM", 0), ("CORE", 1)].iter().map(|&(side, k)| PlayerSetup { side: side.into(), color: k as u8, start: spot(k), metal: Fx::from_int(1000), energy: Fx::from_int(1000), ai: k == 1 }).collect();
    let mut sim = Sim::new(d.types.clone(), d.data.clone(), &m.map, &m.info, 0, &players, 7);
    let id = |name: &str| d.types.id(name).unwrap();
    sim.spawn_unit(id("ARMCOM"), 0, spot(0), 0, true).unwrap();
    if commander {
        sim.spawn_unit(id("CORCOM"), 1, spot(1), 0, true).unwrap();
    }
    let wrecks: Vec<usize> = WRECKS.iter().map(|&(x, z)| sim.place_feature("corak_dead", FxVec3::from_ints(x, 0, z), 0).unwrap()).collect();
    let near = |dx: i64, dz: i64| spot(1) + FxVec3::from_ints(dx, 0, dz);
    let kbots: Vec<UnitId> = (0..8).filter_map(|k| sim.find_build_site(id("CORAK"), near(-200 + 40 * (k % 4), -120 + 60 * (k / 4)), 20).and_then(|p| sim.spawn_unit(id("CORAK"), 1, p, 0, true))).collect();
    if constructor && let Some(p) = sim.find_build_site(id("CORCK"), near(-100, 160), 20) {
        sim.spawn_unit(id("CORCK"), 1, p, 0, true);
    }
    let ai = SkirmishAi::new(1, &sim, Difficulty::Normal);
    Some((sim, ai, kbots, wrecks, spot(0)))
}

/// The kbots sent at the enemy's start (as an attack is), then the computer player left to it
/// for `minutes`: the orders it gave, and whether a kbot got through the pass (west of it).
fn play(sim: &mut Sim, ai: &mut SkirmishAi, kbots: &[UnitId], goal: FxVec3, minutes: u64) -> (Vec<(Vec<UnitId>, Order)>, bool) {
    sim.step(&[Command::Order { player: 1, units: kbots.to_vec(), order: Order::Fight(goal), queue: false }]);
    let mut given = Vec::new();
    for _ in 0..30 * 60 * minutes {
        let out = ai.think(sim);
        for c in &out {
            if let Command::Order { units, order, .. } = c {
                given.push((units.clone(), order.clone()));
            }
        }
        sim.step(&out);
        sim.take_events();
        // (The pass's western mouth is near x 800.)
        if sim.units().any(|u| kbots.contains(&u.id) && u.pos.x < Fx::from_int(800)) {
            return (given, true);
        }
    }
    (given, false)
}

#[test]
fn a_constructor_reclaims_wreckage_out_of_the_pass_and_the_army_goes_through() {
    common::data();
    let Some((mut sim, mut ai, kbots, wrecks, goal)) = the_pass(true, true) else { return };
    let (given, through) = play(&mut sim, &mut ai, &kbots, goal, 4);
    let reclaims = given.iter().filter(|(_, o)| matches!(o, Order::Reclaim(f) if wrecks.contains(f))).count();
    let shots = given.iter().filter(|(_, o)| matches!(o, Order::Attack(Target::Ground(_)))).count();
    let gone = wrecks.iter().filter(|&&w| !sim.features[w].alive).count();
    println!("The Pass: {reclaims} reclaim orders, {shots} fire orders on the wreckage, {gone} of {} wrecks gone; through: {through}", wrecks.len());
    assert!(reclaims > 0, "a constructor was sent to reclaim the wreckage");
    assert!(gone > 0, "wreckage was cleared");
    assert!(through, "the army got through the pass");
}

#[test]
fn with_no_constructor_the_army_breaks_wreckage_up_and_goes_through() {
    common::data();
    let Some((mut sim, mut ai, kbots, wrecks, goal)) = the_pass(false, false) else { return };
    let (given, through) = play(&mut sim, &mut ai, &kbots, goal, 4);
    let shots = given.iter().filter(|(_, o)| matches!(o, Order::Attack(Target::Ground(_)))).count();
    let gone = wrecks.iter().filter(|&&w| !sim.features[w].alive).count();
    println!("The Pass, no constructor: {shots} fire orders on the wreckage, {gone} of {} wrecks gone; through: {through}", wrecks.len());
    assert!(shots > 0, "the army fired on the wreckage");
    assert!(gone > 0, "wreckage was broken up");
    assert!(through, "the army got through the pass");
}
