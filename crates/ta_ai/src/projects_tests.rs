//! The tech tree's use (`projects.rs`, the top tier's share, personas), on made-up data (no
//! install needed): an advanced constructor whose options hold the big projects, a factory that
//! turns out units of two tiers.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use ta_data::{GameData, MapInfo};
use ta_formats::tnt::{FEATURE_NONE, TntMap};
use ta_sim::{PlayerSetup, UnitTypes};

use super::*;

/// The units: an advanced constructor (BUILD2) that puts up extractors, power, a mixed factory
/// (FACT2: TANK1 of the first tier, cheap and good for its price, TANK2 of the second), a nuclear
/// silo, an anti-nuclear defence, a long-range gun, a heavy tower and a field-built experimental
/// (BIGBOT); a flat map, two players, `minutes` into the game on a grown economy.
fn tech_fixture(minutes: u64) -> Sim {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!("ta-ai-projects-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    std::fs::create_dir_all(dir.join("units")).unwrap();
    std::fs::create_dir_all(dir.join("weapons")).unwrap();
    for (name, keys) in [
        ("BUILD2", "Builder=1; WorkerTime=300; BuildDistance=200; CanMove=1; MaxVelocity=2; Category=LEVEL2;"),
        ("FACT2", "Builder=1; WorkerTime=60; FootprintX=4; FootprintZ=4; BuildTime=100; Category=LEVEL2;"),
        ("EXTRACTOR", "ExtractsMetal=1; FootprintX=2; FootprintZ=2;"),
        ("POWER", "EnergyMake=20; FootprintX=2; FootprintZ=2;"),
        ("TANK1", "CanMove=1; MaxVelocity=2; Weapon1=GUN; Category=LEVEL1; BuildCostMetal=50;"),
        ("TANK2", "CanMove=1; MaxVelocity=2; Weapon1=GUN; Category=LEVEL2; BuildCostMetal=400; MaxDamage=2000;"),
        ("SILO", "FootprintX=4; FootprintZ=4; Weapon1=NUKE; Category=LEVEL3; BuildCostMetal=1000;"),
        ("ANTI", "FootprintX=4; FootprintZ=4; Weapon1=ABM; Category=LEVEL3; BuildCostMetal=1500;"),
        ("BIGGUN", "FootprintX=4; FootprintZ=4; Weapon1=LONGGUN; Category=LEVEL3; BuildCostMetal=4000;"),
        ("HEAVY", "FootprintX=2; FootprintZ=2; Weapon1=GUN; Category=LEVEL2; BuildCostMetal=2000; MaxDamage=8000;"),
        ("BIGBOT", "CanMove=1; MaxVelocity=1; Weapon1=GUN; Category=LEVEL3; BuildCostMetal=9000; MaxDamage=40000; FootprintX=4; FootprintZ=4;"),
    ] {
        let cost = if keys.contains("BuildCostMetal") { String::new() } else { "BuildCostMetal=100;".into() };
        std::fs::write(dir.join(format!("units/{name}.fbi")), format!("[UNITINFO] {{ UnitName={name}; Objectname={name}; MaxDamage=1000; SightDistance=100; {cost} {keys} }}")).unwrap();
    }
    std::fs::write(
        dir.join("weapons/test.tdf"),
        "[GUN] { range=200; reloadtime=1; weaponvelocity=400; [DAMAGE] { default=10; } } \
         [LONGGUN] { range=3000; reloadtime=5; weaponvelocity=900; ballistic=1; [DAMAGE] { default=300; } } \
         [NUKE] { range=32000; reloadtime=30; weaponvelocity=400; stockpile=1; commandfire=1; targetable=1; [DAMAGE] { default=5000; } } \
         [ABM] { range=32000; reloadtime=30; weaponvelocity=900; stockpile=1; interceptor=1; coverage=1000; [DAMAGE] { default=500; } }",
    )
    .unwrap();
    let mut vfs = ta_vfs::Vfs::new();
    vfs.mount_mod_dir(&dir, "test").unwrap();
    let mut data = GameData::load(&vfs);
    std::fs::remove_dir_all(dir).unwrap();
    let builder = data.unit_id("BUILD2").unwrap();
    data.units[builder].build_options = ["EXTRACTOR", "POWER", "FACT2", "SILO", "ANTI", "BIGGUN", "HEAVY", "BIGBOT"].map(String::from).to_vec();
    let factory = data.unit_id("FACT2").unwrap();
    data.units[factory].build_options = ["BUILD2", "TANK1", "TANK2"].map(String::from).to_vec();
    let data = Arc::new(data);
    let types = Arc::new(UnitTypes::compile(&data, &|_| None));
    let cells = 160u32;
    let n = (cells * cells) as usize;
    let map = TntMap { width: cells, height: cells, sea_level: 0, heights: vec![60; n], features: vec![FEATURE_NONE; n], tiles: vec![], tile_map: vec![], feature_names: vec![], minimap: Default::default(), flags: Vec::new() };
    let players: Vec<_> = (0..2).map(|color| PlayerSetup { side: "ARM".into(), color, start: FxVec3::ZERO, metal: fx(1000), energy: fx(1000), ai: false }).collect();
    let mut sim = Sim::new(types, data, &map, &MapInfo::default(), 0, &players, 7);
    sim.los_enabled = false;
    sim.start_spots = vec![FxVec3::from_ints(400, 0, 400), FxVec3::from_ints(2200, 0, 400)];
    sim.players[0].start = sim.start_spots[0];
    sim.players[1].start = sim.start_spots[1];
    sim.tick = minutes * MINUTE;
    grown(&mut sim);
    sim
}

/// Our economy grown: forty metal and two thousand energy a second, the stores half full.
fn grown(sim: &mut Sim) {
    let e = &mut sim.players[0].economy;
    e.metal_capacity = fx(10_000);
    e.energy_capacity = fx(20_000);
    e.metal = fx(5000);
    e.energy = fx(10_000);
    e.metal_income = fx(40);
    e.energy_income = fx(2000);
}

fn id(sim: &Sim, name: &str) -> usize {
    sim.types.id(name).unwrap()
}

fn put(sim: &mut Sim, name: &str, owner: u8, x: i64, z: i64) -> UnitId {
    sim.spawn_unit(id(sim, name), owner, FxVec3::from_ints(x, 60, z), 0, true).unwrap()
}

/// The project an advanced constructor of ours would take up now, if any.
fn project(ai: &mut SkirmishAi, sim: &Sim, builder: UnitId) -> Option<usize> {
    let s = ai.survey(sim);
    let (plan, _) = ai.start_builders(sim, &s, &mut Vec::new());
    let b = sim.unit(builder).unwrap();
    let opts = sim.unit_type(b).ai_build_options.clone();
    match ai.project_task(sim, &s, b, &opts, &plan) {
        Some(Task::Build(t, _)) => Some(t),
        _ => None,
    }
}

/// Late in the game, on a grown economy, an advanced constructor takes up the big projects in
/// turn: anti-nuclear cover first, then a nuclear silo, a heavy tower, a long-range gun once the
/// enemy's base is found within its reach; an experimental beside them. Easy builds none of it
/// (bar anti-nuclear cover against a silo it has seen).
#[test]
fn the_big_projects_come_in_turn_on_a_grown_economy() {
    let mut sim = tech_fixture(30);
    put(&mut sim, "FACT2", 0, 400, 400);
    let builder = put(&mut sim, "BUILD2", 0, 500, 500);
    let mut ai = SkirmishAi::new(0, &sim, Difficulty::Hard);
    // (The enemy's base found, in the long-range gun's reach from our half of the ground.)
    ai.enemy_bases = vec![FxVec3::from_ints(2200, 0, 400)];
    ai.enemy_base_seen = vec![true];
    ai.enemy_base_owners = vec![1];
    let mut taken = Vec::new();
    for _ in 0..8 {
        let Some(t) = project(&mut ai, &sim, builder) else { break };
        taken.push(sim.types.get(t).name.clone());
        // Built (the next comes once it is up, one at a time).
        let at = sim.find_build_site(t, FxVec3::from_ints(600 + 300 * taken.len() as i64, 0, 900), 60).unwrap();
        sim.spawn_unit(t, 0, at, 0, true).unwrap();
        sim.step(&[]);
        grown(&mut sim);
    }
    assert_eq!(taken.first().map(String::as_str), Some("BIGBOT"), "an experimental, beside the others: {taken:?}");
    for name in ["ANTI", "SILO", "HEAVY", "BIGGUN"] {
        assert!(taken.iter().any(|t| t == name), "{name} among the projects: {taken:?}");
    }
    assert!(taken.iter().position(|t| t == "ANTI") < taken.iter().position(|t| t == "SILO"), "anti-nuclear cover before the silo: {taken:?}");
    // Easy: none.
    let mut sim = tech_fixture(40);
    put(&mut sim, "FACT2", 0, 400, 400);
    let builder = put(&mut sim, "BUILD2", 0, 500, 500);
    let mut ai = SkirmishAi::new(0, &sim, Difficulty::Easy);
    assert_eq!(project(&mut ai, &sim, builder), None, "Easy takes up no big projects");
    // ...but anti-nuclear cover once it has seen a silo of the enemy's.
    put(&mut sim, "SILO", 1, 2000, 2000);
    ai.remember(&sim, &ai.survey(&sim));
    assert_eq!(project(&mut ai, &sim, builder), Some(id(&sim, "ANTI")));
}

/// Our long-range guns shell the enemy's buildings we know of in their reach (the worth most
/// first), none beside our own units, and nothing out of reach.
#[test]
fn long_range_guns_shell_what_we_know_of_the_enemys_base() {
    let mut sim = tech_fixture(30);
    let gun = put(&mut sim, "BIGGUN", 0, 400, 400);
    let near = put(&mut sim, "FACT2", 1, 2200, 400);
    let far = put(&mut sim, "FACT2", 1, 400, 2400 + 1000);
    let ours = put(&mut sim, "TANK1", 0, 1600, 1600);
    let beside = put(&mut sim, "FACT2", 1, 1640, 1640);
    let mut ai = SkirmishAi::new(0, &sim, Difficulty::Normal);
    sim.tick = 30 * MINUTE + 10 * SECOND - sim.tick % (10 * SECOND);
    let s = ai.survey(&sim);
    ai.remember(&sim, &s);
    let mut out = Vec::new();
    ai.run_long_guns(&sim, &ai.survey(&sim), &mut out);
    let at = |u: UnitId| sim.unit(u).unwrap().pos;
    assert!(matches!(out.as_slice(), [Command::Order { units, order: Order::Attack(Target::Ground(p)), .. }] if units == &vec![gun] && *p == at(near)), "shells the factory in reach: {out:?}");
    assert!(!out.iter().any(|c| matches!(c, Command::Order { order: Order::Attack(Target::Ground(p)), .. } if *p == at(far) || *p == at(beside))), "not out of reach, not beside our own");
    let _ = ours;
}

/// On a grown economy a factory of two tiers turns out mostly its top tier's units (the level's
/// share), where on a small one it goes by their worth for the price alone (the cheap first tier).
#[test]
fn production_leans_to_the_top_tier_on_a_grown_economy() {
    let mut sim = tech_fixture(20);
    let factory = id(&sim, "FACT2");
    let opts = sim.types.get(factory).ai_build_options.clone();
    let (tank1, tank2) = (id(&sim, "TANK1"), id(&sim, "TANK2"));
    let count = |sim: &Sim, d: Difficulty| {
        let mut ai = SkirmishAi::new(0, sim, d);
        let s = ai.survey(sim);
        (0..100).filter(|_| ai.pick_combat(sim, &s, factory, &opts) == Some(tank2)).count()
    };
    let rich = count(&sim, Difficulty::Hard);
    sim.players[0].economy.metal_income = fx(5);
    let poor = count(&sim, Difficulty::Hard);
    let _ = tank1;
    assert!(rich >= 65, "the top tier on a grown economy: {rich} of 100");
    assert!(poor <= 40, "worth for the price on a small one: {poor} of 100");
    sim.players[0].economy.metal_income = fx(40);
    let easy = count(&sim, Difficulty::Easy);
    assert!(easy < rich, "Easy leans less: {easy} of 100 against {rich}");
}

/// Experimentals wait for the level's minute and number: a factory does not turn one out before.
#[test]
fn experimentals_wait_for_their_minute() {
    let sim = tech_fixture(10);
    let builder_opts = vec![id(&sim, "BIGBOT")];
    let mut ai = SkirmishAi::new(0, &sim, Difficulty::Hard);
    assert!(ai.info[id(&sim, "BIGBOT")].experimental);
    let s = ai.survey(&sim);
    assert_eq!(ai.pick_combat(&sim, &s, id(&sim, "FACT2"), &builder_opts), None, "not ten minutes in");
    let sim = tech_fixture(30);
    let mut ai = SkirmishAi::new(0, &sim, Difficulty::Hard);
    let s = ai.survey(&sim);
    assert_eq!(ai.pick_combat(&sim, &s, id(&sim, "FACT2"), &builder_opts), Some(id(&sim, "BIGBOT")));
}

/// The personas shape the tuning, whatever the level: a rusher attacks sooner with less, a turtle
/// fortifies and attacks later, a teching one climbs sooner and further; and Easy stays without
/// missiles, long-range guns and experimentals whichever it plays.
#[test]
fn personas_shape_the_tuning_and_easy_stays_gentle() {
    let tuned = |d: Difficulty, p: Persona| Params::for_difficulty(d).with_persona(p, d);
    let balanced = tuned(Difficulty::Normal, Persona::Balanced);
    let rush = tuned(Difficulty::Normal, Persona::Rush);
    let turtle = tuned(Difficulty::Normal, Persona::Turtle);
    let tech = tuned(Difficulty::Normal, Persona::Tech);
    assert!(rush.wave_base < balanced.wave_base && rush.wave_cooldown < balanced.wave_cooldown && rush.tech_minute > balanced.tech_minute);
    assert!(turtle.wave_base > balanced.wave_base && turtle.extra_towers > balanced.extra_towers && turtle.heavy_towers > balanced.heavy_towers && turtle.long_guns > balanced.long_guns);
    assert!(tech.tech_minute < balanced.tech_minute && tech.top_tier > balanced.top_tier && tech.experimentals > balanced.experimentals);
    assert!(tuned(Difficulty::Normal, Persona::Air).air_first && tuned(Difficulty::Normal, Persona::Naval).naval_first);
    for p in Persona::ALL {
        let easy = tuned(Difficulty::Easy, p);
        assert!(easy.silos == 0 && easy.nuke_minute == u64::MAX && easy.long_guns == 0 && easy.experimentals == 0, "{p:?} on Easy");
    }
    // Random: one of the others, the same for a game's seed and player, not always the same one.
    let drawn: Vec<Persona> = (0..40).map(|seed| Persona::Random.resolve(seed, 1)).collect();
    assert!(drawn.iter().all(|&p| p != Persona::Random));
    assert_eq!(Persona::Random.resolve(7, 1), Persona::Random.resolve(7, 1));
    assert!(drawn.iter().any(|&p| p != drawn[0]), "{drawn:?}");
    for p in Persona::ALL {
        assert_eq!(Persona::from_key(p.key()), Some(p));
        assert_eq!(Persona::from_code(p.code()), Some(p));
    }
}
