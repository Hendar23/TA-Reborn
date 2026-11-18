//! The computer players' enhancements: a commander installs its engineering suite once the
//! economy reaches the tech climb's, one at a time and the deeper suite later; one with none (TA's
//! Commanders) is left alone.

use ta_data::EnhancementDef;

use super::*;

/// The fixture's commander with build power, a tech 2 suite (SUITE: the factory on its menu, more
/// build power) and a tech 3 one over it (SUITE3).
fn with_suites() -> Sim {
    fixture_edit(|data| {
        let id = data.unit_id("COMMANDER").unwrap();
        data.units[id].worker_time = 100.0;
        data.units[id].enhancements = vec![
            EnhancementDef { id: "SUITE".into(), name: "Suite".into(), slot: "LCH".into(), build_cost_metal: 300.0, build_cost_energy: 3000.0, build_time: 100.0, worker_time: 100.0, build_options: vec!["FACTORY".into()], ..Default::default() },
            EnhancementDef { id: "SUITE3".into(), name: "Suite 3".into(), slot: "LCH".into(), prerequisite: "SUITE".into(), build_cost_metal: 900.0, build_cost_energy: 9000.0, build_time: 300.0, worker_time: 200.0, build_options: vec!["FACTORY".into(), "EXTRACTOR".into()], ..Default::default() },
        ];
    })
}

fn rich(sim: &mut Sim, minutes: u64) {
    sim.tick = minutes * MINUTE;
    let e = &mut sim.players[0].economy;
    e.metal_capacity = fx(10_000);
    e.energy_capacity = fx(100_000);
    e.metal = fx(5000);
    e.energy = fx(50_000);
}

fn enhancements_ordered(out: &[Command]) -> Vec<(UnitId, u16)> {
    out.iter()
        .filter_map(|c| match c {
            Command::Order { units, order: Order::Enhance { index, remove: false }, .. } => Some((units[0], *index)),
            _ => None,
        })
        .collect()
}

#[test]
fn the_commander_installs_its_engineering_suite_once_the_economy_allows() {
    let mut sim = with_suites();
    let commander = sim.spawn_unit(sim.types.id("COMMANDER").unwrap(), 0, FxVec3::from_ints(200, 60, 200), 0, true).unwrap();
    let mut ai = SkirmishAi::new(0, &sim, Difficulty::Normal);
    // At the start: not yet (the economy is not the tech climb's).
    let mut out = Vec::new();
    ai.run_enhancements(&sim, &ai.survey(&sim), &mut out);
    assert!(enhancements_ordered(&out).is_empty(), "{out:?}");
    // Ten minutes in, metal to spare: the first suite, not the one past it.
    rich(&mut sim, 10);
    let mut out = Vec::new();
    ai.run_enhancements(&sim, &ai.survey(&sim), &mut out);
    assert_eq!(enhancements_ordered(&out), [(commander, 0)]);
    // Installed (its work under way, then done): one at a time, and the deeper suite waits for a
    // factory of tech 2 (or riches).
    sim.step(&out);
    let mut out = Vec::new();
    ai.run_enhancements(&sim, &ai.survey(&sim), &mut out);
    assert!(enhancements_ordered(&out).is_empty(), "one at a time: {out:?}");
    for _ in 0..400 {
        rich(&mut sim, 10);
        sim.step(&[]);
    }
    assert_eq!(sim.unit(commander).unwrap().enhancements, [0], "the suite installed");
    let factory = sim.types.id("FACTORY").unwrap();
    assert!(sim.build_options(commander).contains(&factory), "its menu grown");
    let mut out = Vec::new();
    ai.run_enhancements(&sim, &ai.survey(&sim), &mut out);
    let s = ai.survey(&sim);
    if s.factory_tier < 2 && !(ai.metal_avg > Fx::from_ratio(5, 10) && s.m_inc >= fx(ai.p.tech_income * 2)) {
        assert!(enhancements_ordered(&out).is_empty(), "the deeper suite later: {out:?}");
    }
}

/// A commander without enhancements (TA's) is never told to install one.
#[test]
fn a_commander_without_enhancements_installs_none() {
    let mut sim = fixture();
    sim.spawn_unit(sim.types.id("COMMANDER").unwrap(), 0, FxVec3::from_ints(200, 60, 200), 0, true).unwrap();
    rich(&mut sim, 20);
    let mut ai = SkirmishAi::new(0, &sim, Difficulty::Hard);
    let mut out = Vec::new();
    ai.run_enhancements(&sim, &ai.survey(&sim), &mut out);
    assert!(out.is_empty(), "{out:?}");
}
