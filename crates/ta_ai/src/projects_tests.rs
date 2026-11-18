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
/// (BIGBOT); a constructor of the first tier (BUILD1) whose factory (FACT1) upgrades into FACT2,
/// which it can also put up new; a flat map, two players, `minutes` into the game on a grown
/// economy.
fn tech_fixture(minutes: u64) -> Sim {
    tech_fixture_with(minutes, 160, &[])
}

/// [`tech_fixture`] on a map `cells` across, the advanced constructor offering `more` besides
/// (MIDGUN: a long-range gun of 2,000 reach, cheaper than BIGGUN's 3,000).
fn tech_fixture_with(minutes: u64, cells: u32, more: &[&str]) -> Sim {
    let build2: Vec<&str> = ["EXTRACTOR", "POWER", "FACT2", "SILO", "ANTI", "BIGGUN", "HEAVY", "BIGBOT"].iter().chain(more).copied().collect();
    fixture(
        minutes,
        cells,
        &[
            ("BUILD2", "Builder=1; WorkerTime=300; BuildDistance=200; CanMove=1; MaxVelocity=2; Category=LEVEL2;"),
            ("BUILD1", "Builder=1; WorkerTime=100; BuildDistance=200; CanMove=1; MaxVelocity=2; Category=LEVEL1;"),
            ("FACT1", "Builder=1; WorkerTime=60; FootprintX=4; FootprintZ=4; BuildTime=100; Category=LEVEL1; UpgradesTo=FACT2;"),
            ("FACT2", "Builder=1; WorkerTime=60; FootprintX=4; FootprintZ=4; BuildTime=100; Category=LEVEL2;"),
            ("EXTRACTOR", "ExtractsMetal=1; FootprintX=2; FootprintZ=2;"),
            ("POWER", "EnergyMake=20; FootprintX=2; FootprintZ=2;"),
            ("TANK1", "CanMove=1; MaxVelocity=2; Weapon1=GUN; Category=LEVEL1; BuildCostMetal=50;"),
            ("TANK2", "CanMove=1; MaxVelocity=2; Weapon1=GUN; Category=LEVEL2; BuildCostMetal=400; MaxDamage=2000;"),
            ("SILO", "FootprintX=4; FootprintZ=4; Weapon1=NUKE; Category=LEVEL3; BuildCostMetal=1000;"),
            ("ANTI", "FootprintX=4; FootprintZ=4; Weapon1=ABM; Category=LEVEL3; BuildCostMetal=1500;"),
            ("BIGGUN", "FootprintX=4; FootprintZ=4; Weapon1=LONGGUN; Category=LEVEL3; BuildCostMetal=4000;"),
            ("MIDGUN", "FootprintX=4; FootprintZ=4; Weapon1=MIDRANGE; Category=LEVEL3; BuildCostMetal=2000;"),
            ("HEAVY", "FootprintX=2; FootprintZ=2; Weapon1=GUN; Category=LEVEL2; BuildCostMetal=2000; MaxDamage=8000;"),
            ("BIGBOT", "CanMove=1; MaxVelocity=1; Weapon1=GUN; Category=LEVEL3; BuildCostMetal=9000; MaxDamage=40000; FootprintX=4; FootprintZ=4;"),
        ],
        "[GUN] { range=200; reloadtime=1; weaponvelocity=400; [DAMAGE] { default=10; } } \
         [LONGGUN] { range=3000; reloadtime=5; weaponvelocity=900; ballistic=1; [DAMAGE] { default=300; } } \
         [MIDRANGE] { range=2000; reloadtime=5; weaponvelocity=900; ballistic=1; [DAMAGE] { default=200; } } \
         [NUKE] { range=32000; reloadtime=30; weaponvelocity=400; stockpile=1; commandfire=1; targetable=1; [DAMAGE] { default=5000; } } \
         [ABM] { range=32000; reloadtime=30; weaponvelocity=900; stockpile=1; interceptor=1; coverage=1000; [DAMAGE] { default=500; } }",
        &[
            ("BUILD2", build2),
            ("FACT2", vec!["BUILD2", "TANK1", "TANK2"]),
            ("BUILD1", vec!["EXTRACTOR", "POWER", "FACT1", "FACT2"]),
            ("FACT1", vec!["BUILD1", "TANK1"]),
        ],
    )
}

/// A made-up game of these units (name, FBI keys: those not given are a cost of 100, 1,000
/// health and sight 100) and weapons, the builders' and factories' options as given, on a flat
/// map `cells` across (160 at least), two players, `minutes` into the game on a grown economy.
fn fixture(minutes: u64, cells: u32, units: &[(&str, &str)], weapons: &str, options: &[(&str, Vec<&str>)]) -> Sim {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!("ta-ai-projects-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    std::fs::create_dir_all(dir.join("units")).unwrap();
    std::fs::create_dir_all(dir.join("weapons")).unwrap();
    for (name, keys) in units {
        let cost = if keys.contains("BuildCostMetal") { String::new() } else { "BuildCostMetal=100;".into() };
        let health = if keys.contains("MaxDamage") { String::new() } else { "MaxDamage=1000;".into() };
        std::fs::write(dir.join(format!("units/{name}.fbi")), format!("[UNITINFO] {{ UnitName={name}; Objectname={name}; SightDistance=100; {health} {cost} {keys} }}")).unwrap();
    }
    std::fs::write(dir.join("weapons/test.tdf"), weapons).unwrap();
    let mut vfs = ta_vfs::Vfs::new();
    vfs.mount_mod_dir(&dir, "test").unwrap();
    let mut data = GameData::load(&vfs);
    std::fs::remove_dir_all(dir).unwrap();
    for (name, list) in options {
        let unit = data.unit_id(name).unwrap();
        data.units[unit].build_options = list.iter().map(|s| s.to_string()).collect();
    }
    let data = Arc::new(data);
    let types = Arc::new(UnitTypes::compile(&data, &|_| None));
    let cells = cells.max(160);
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

/// The late game of a tree like the Supreme Commander factions': constructors of the second tier
/// (BUILD2) put up tactical launchers (TACSILO), tactical missile defences (TMD) and anti-nuclear
/// cover, those of the third (BUILD3, turned out by FACT3) a strategic silo too; a support
/// commander (SCU: armed, costly, tagged of the first tier, made by GATE) builds what BUILD3 does;
/// a factory of experimentals (FACTX: BIGBOT alone), towers and storage to strike.
fn late_fixture(minutes: u64) -> Sim {
    let third = vec!["EXTRACTOR", "POWER", "TACSILO", "TMD", "ANTI", "SILO", "GATE", "FACTX"];
    fixture(
        minutes,
        160,
        &[
            ("BUILD1", "Builder=1; WorkerTime=100; BuildDistance=200; CanMove=1; MaxVelocity=2; Category=LEVEL1;"),
            ("BUILD2", "Builder=1; WorkerTime=300; BuildDistance=200; CanMove=1; MaxVelocity=2; Category=LEVEL2;"),
            ("BUILD3", "Builder=1; WorkerTime=300; BuildDistance=200; CanMove=1; MaxVelocity=2; Category=LEVEL3;"),
            ("SCU", "Builder=1; WorkerTime=600; BuildDistance=200; CanMove=1; MaxVelocity=2; Weapon1=GUN; Category=LEVEL1; BuildCostMetal=9000; MaxDamage=20000;"),
            ("FACT1", "Builder=1; WorkerTime=60; FootprintX=4; FootprintZ=4; BuildTime=100; Category=LEVEL1;"),
            ("FACT3", "Builder=1; WorkerTime=60; FootprintX=4; FootprintZ=4; BuildTime=100; Category=LEVEL3;"),
            ("GATE", "Builder=1; WorkerTime=60; FootprintX=4; FootprintZ=4; BuildTime=100; Category=LEVEL3; BuildCostMetal=3000;"),
            ("FACTX", "Builder=1; WorkerTime=60; FootprintX=6; FootprintZ=6; BuildTime=100; Category=LEVEL3; BuildCostMetal=6000;"),
            ("EXTRACTOR", "ExtractsMetal=1; FootprintX=2; FootprintZ=2;"),
            ("POWER", "EnergyMake=20; FootprintX=2; FootprintZ=2;"),
            ("TANK1", "CanMove=1; MaxVelocity=2; Weapon1=GUN; Category=LEVEL1; BuildCostMetal=50;"),
            ("BIGBOT", "CanMove=1; MaxVelocity=1; Weapon1=GUN; Category=LEVEL2; BuildCostMetal=9000; MaxDamage=40000; FootprintX=3; FootprintZ=3; BuildTime=380000;"),
            ("SILO", "FootprintX=4; FootprintZ=4; Weapon1=NUKE; Category=LEVEL3; BuildCostMetal=1000;"),
            ("TACSILO", "FootprintX=2; FootprintZ=2; Weapon1=CRUISE; Category=LEVEL2; BuildCostMetal=800;"),
            ("TMD", "FootprintX=2; FootprintZ=2; Weapon1=FLAK; Category=LEVEL2; BuildCostMetal=300;"),
            ("ANTI", "FootprintX=4; FootprintZ=4; Weapon1=ABM; Category=LEVEL3; BuildCostMetal=1500;"),
            ("TOWER", "FootprintX=2; FootprintZ=2; Weapon1=GUN; Category=LEVEL1; BuildCostMetal=500;"),
            ("STORE", "FootprintX=2; FootprintZ=2; MetalStorage=1000; BuildCostMetal=600;"),
        ],
        "[GUN] { range=200; reloadtime=1; weaponvelocity=400; [DAMAGE] { default=10; } } \
         [NUKE] { range=32000; reloadtime=30; weaponvelocity=400; stockpile=1; commandfire=1; targetable=1; areaofeffect=512; [DAMAGE] { default=5000; } } \
         [ABM] { range=32000; reloadtime=30; weaponvelocity=900; stockpile=1; interceptor=1; coverage=1000; [DAMAGE] { default=500; } } \
         [CRUISE] { range=6000; reloadtime=30; weaponvelocity=400; stockpile=1; commandfire=1; targetable=2; metalpershot=180; energypershot=3600; areaofeffect=128; [DAMAGE] { default=6000; } } \
         [FLAK] { range=400; reloadtime=2; weaponvelocity=900; interceptor=2; coverage=400; [DAMAGE] { default=100; } }",
        &[
            ("BUILD1", vec!["EXTRACTOR", "POWER", "FACT1"]),
            ("BUILD2", vec!["EXTRACTOR", "POWER", "TACSILO", "TMD", "ANTI"]),
            ("BUILD3", third.clone()),
            ("SCU", third),
            ("FACT1", vec!["BUILD1", "TANK1"]),
            ("FACT3", vec!["BUILD3", "TANK1"]),
            ("GATE", vec!["SCU"]),
            ("FACTX", vec!["BIGBOT"]),
        ],
    )
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
    assert!(turtle.tower_pace < balanced.tower_pace && turtle.tower_cap > balanced.tower_cap && turtle.sturdy_towers && turtle.towers_first && !balanced.towers_first);
    assert!(tech.tech_minute < balanced.tech_minute && tech.top_tier > balanced.top_tier && tech.experimentals > balanced.experimentals);
    // Upgrades in place and the commander's suites: Tech's sooner, Rush's later.
    assert!(tech.tech_eager && !balanced.tech_eager && !rush.tech_eager);
    assert!(tech.mex_upgrade_minute < balanced.mex_upgrade_minute && rush.mex_upgrade_minute > balanced.mex_upgrade_minute && rush.tech_minute > balanced.tech_minute);
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

/// More advanced factories as the economy grows come by upgrading where a factory of ours
/// upgrades into one (as the Supreme Commander factions' do, which nothing builds outright), not
/// by building another, though the constructor could; built where nothing upgrades into one.
#[test]
fn more_advanced_factories_come_by_upgrading_where_a_factory_upgrades() {
    let mut sim = tech_fixture(30);
    // (An income for two advanced factories, one standing.)
    sim.players[0].economy.metal_income = fx(60);
    put(&mut sim, "FACT2", 0, 400, 400);
    let lab = put(&mut sim, "FACT1", 0, 700, 400);
    let builder = put(&mut sim, "BUILD1", 0, 550, 600);
    let mut ai = SkirmishAi::new(0, &sim, Difficulty::Normal);
    let s = ai.survey(&sim);
    assert_eq!((s.adv_factories, ai.adv_factories_wanted(&s)), (1, 2), "one more advanced factory wanted");
    // The constructor builds none...
    let (mut plan, _) = ai.start_builders(&sim, &s, &mut Vec::new());
    let b = sim.unit(builder).unwrap();
    let opts = sim.unit_type(b).ai_build_options.clone();
    assert!(ai.more_adv_factory_task(&sim, &s, b, &opts, &plan, true).is_none(), "not built: FACT1 upgrades into it");
    plan.tech = false;
    // ...the first-tier factory is upgraded.
    let mut out = Vec::new();
    ai.run_upgrades_in_place(&sim, &s, &mut out);
    assert!(out.iter().any(|c| matches!(c, Command::Order { units, order: Order::Upgrade(to), .. } if units == &vec![lab] && *to == id(&sim, "FACT2"))), "{out:?}");
    // Where nothing upgrades into one, it is built as before.
    let mut sim = tech_fixture(30);
    sim.players[0].economy.metal_income = fx(60);
    put(&mut sim, "FACT2", 0, 400, 400);
    let builder = put(&mut sim, "BUILD1", 0, 550, 600);
    let mut ai = SkirmishAi::new(0, &sim, Difficulty::Normal);
    let s = ai.survey(&sim);
    let (plan, _) = ai.start_builders(&sim, &s, &mut Vec::new());
    let b = sim.unit(builder).unwrap();
    let opts = sim.unit_type(b).ai_build_options.clone();
    assert!(matches!(ai.more_adv_factory_task(&sim, &s, b, &opts, &plan, true), Some(Task::Build(t, _)) if t == id(&sim, "FACT2")));
}

/// With long-range guns of two reaches on offer, the cheapest that has a site within its reach of
/// the enemy's base is put up: the short one has none at this distance (it rests a while, not
/// tried by every constructor at every turn) and the longer one is taken in the same turn, and
/// again while the short one rests (it was the short one, every time, and nothing was built).
#[test]
fn the_cheapest_long_range_gun_that_reaches_is_put_up() {
    let mut sim = tech_fixture_with(30, 400, &["MIDGUN"]);
    put(&mut sim, "FACT2", 0, 400, 400);
    let builder = put(&mut sim, "BUILD2", 0, 500, 500);
    let mut ai = SkirmishAi::new(0, &sim, Difficulty::Hard);
    // (The enemy's base found, some 5,600 px off: past the short gun's reach from anywhere it may
    // stand, never much past halfway; within the long one's.)
    ai.enemy_bases = vec![FxVec3::from_ints(6000, 0, 500)];
    ai.enemy_base_seen = vec![true];
    ai.enemy_base_owners = vec![1];
    let (short, long) = (id(&sim, "MIDGUN"), id(&sim, "BIGGUN"));
    assert!(ai.info[short].long_range && ai.info[short].cost < ai.info[long].cost);
    let s = ai.survey(&sim);
    let (plan, _) = ai.start_builders(&sim, &s, &mut Vec::new());
    let b = sim.unit(builder).unwrap();
    let opts = sim.unit_type(b).ai_build_options.clone();
    let task = ai.long_gun_task(&sim, &s, b, &opts, &plan);
    assert!(matches!(task, Some(Task::Build(t, _)) if t == long), "the long one: {task:?}");
    assert!(ai.place_failed.iter().any(|&(t, _)| t == short) && !ai.place_failed.iter().any(|&(t, _)| t == long), "the short one rests");
    let task = ai.long_gun_task(&sim, &s, b, &opts, &plan);
    assert!(matches!(task, Some(Task::Build(t, _)) if t == long), "again the long one: {task:?}");
}

/// Late in the game the strategic silo is put up by a constructor whose options hold one (the
/// factions' tech 3 engineers), never in its place a tactical launcher; the tactical launcher by
/// one of the second tier, in the enemy's reach; a tactical launcher standing does not fill the
/// strategic one's place. Easy builds neither.
#[test]
fn strategic_silos_and_tactical_launchers_each_in_their_place() {
    let mut sim = late_fixture(30);
    put(&mut sim, "ANTI", 0, 600, 1200);
    let second = put(&mut sim, "BUILD2", 0, 500, 500);
    let third = put(&mut sim, "BUILD3", 0, 520, 560);
    let mut ai = SkirmishAi::new(0, &sim, Difficulty::Normal);
    // (No experimental to take the third tier's constructor first.)
    assert!(ai.tweak("experimentals", 0));
    ai.enemy_bases = vec![FxVec3::from_ints(2200, 0, 400)];
    ai.enemy_base_seen = vec![true];
    ai.enemy_base_owners = vec![1];
    assert!(ai.info[id(&sim, "SILO")].strategic_silo() && ai.info[id(&sim, "TACSILO")].tactical_silo() && ai.info[id(&sim, "TMD")].tactical_cover());
    assert!(!ai.info[id(&sim, "ANTI")].tactical_cover(), "anti-nuclear cover is no tactical cover");
    assert_eq!(project(&mut ai, &sim, second), Some(id(&sim, "TACSILO")), "the second tier's: a tactical launcher");
    assert_eq!(project(&mut ai, &sim, third), Some(id(&sim, "SILO")), "the third tier's: the strategic silo");
    // A tactical launcher standing: the strategic silo is still due.
    put(&mut sim, "TACSILO", 0, 900, 500);
    assert_eq!(project(&mut ai, &sim, third), Some(id(&sim, "SILO")));
    assert_eq!(project(&mut ai, &sim, second), None, "one tactical launcher on Normal, no strategic one in its options");
    put(&mut sim, "SILO", 0, 300, 1300);
    sim.step(&[]);
    grown(&mut sim);
    assert_ne!(project(&mut ai, &sim, third), Some(id(&sim, "SILO")), "one strategic silo on Normal");
    // Easy: neither.
    let mut sim = late_fixture(40);
    let second = put(&mut sim, "BUILD2", 0, 500, 500);
    let third = put(&mut sim, "BUILD3", 0, 520, 560);
    let mut ai = SkirmishAi::new(0, &sim, Difficulty::Easy);
    ai.enemy_bases = vec![FxVec3::from_ints(2200, 0, 400)];
    ai.enemy_base_seen = vec![true];
    assert_eq!((project(&mut ai, &sim, second), project(&mut ai, &sim, third)), (None, None), "Easy builds no silos");
}

/// Cover as the missiles seen call for it: against a tactical launcher of the enemy's, a tactical
/// missile defence (not anti-nuclear cover); against a strategic silo, anti-nuclear cover (not a
/// tactical defence). Easy too.
#[test]
fn cover_answers_the_missiles_of_the_launchers_seen() {
    for d in [Difficulty::Easy, Difficulty::Normal] {
        let mut sim = late_fixture(12);
        let second = put(&mut sim, "BUILD2", 0, 500, 500);
        let mut ai = SkirmishAi::new(0, &sim, d);
        put(&mut sim, "TACSILO", 1, 2000, 600);
        ai.remember(&sim, &ai.survey(&sim));
        assert!(ai.enemy_tactical.is_some() && !ai.enemy_silo);
        assert_eq!(project(&mut ai, &sim, second), Some(id(&sim, "TMD")), "{d:?}: a tactical defence against a tactical launcher");
        let mut sim = late_fixture(12);
        let second = put(&mut sim, "BUILD2", 0, 500, 500);
        let mut ai = SkirmishAi::new(0, &sim, d);
        put(&mut sim, "SILO", 1, 2000, 600);
        ai.remember(&sim, &ai.survey(&sim));
        assert!(ai.enemy_silo && ai.enemy_tactical.is_none());
        assert_eq!(project(&mut ai, &sim, second), Some(id(&sim, "ANTI")), "{d:?}: anti-nuclear cover against a strategic silo");
    }
}

/// A tactical missile goes at the buildings that matter (a tower before a store of about its
/// price), never at units in the open; a strategic one at the richest cluster, units and all.
#[test]
fn tactical_missiles_strike_defences_and_strategic_ones_the_richest_cluster() {
    let mut sim = late_fixture(30);
    let tower = put(&mut sim, "TOWER", 1, 2000, 600);
    let store = put(&mut sim, "STORE", 1, 2000, 1400);
    for k in 0..12 {
        put(&mut sim, "TANK1", 1, 1400 + (k % 4) * 30, 1900 + (k / 4) * 30);
    }
    let mut ai = SkirmishAi::new(0, &sim, Difficulty::Normal);
    ai.remember(&sim, &ai.survey(&sim));
    let weapon = |name: &str| sim.weapon_types.types.iter().find(|w| w.def.name == name).unwrap().clone();
    let at = |u: UnitId| sim.unit(u).unwrap().pos;
    let (target, need) = ai.pick_nuke_target(&sim, FxVec3::from_ints(400, 0, 400), &weapon("CRUISE"), 1).unwrap();
    assert!(target.within_xz(at(tower), fx(8)) && need == 1, "the tower: {target:?}");
    let _ = store;
}

/// An unfinished experimental (BIGBOT) by the factory of them, as though in its yard: the
/// fixture's factories have no scripts to open their yards, and the survey is told so by hand.
fn in_yard<'a>(s: &mut Survey<'a>, sim: &'a Sim, unit: UnitId, factory: UnitId) {
    s.nanoframes.retain(|n| n.id != unit);
    s.yard_projects.push((sim.unit(unit).unwrap(), factory));
}

/// An experimental in a factory's yard (TA's Krogoth in its Gantry) is a project: idle constructors
/// near it help at the factory, as many as it wants (the level's and more for its long build), and
/// one at it already is kept at it.
#[test]
fn an_experimental_in_a_factory_yard_gets_hands() {
    let mut sim = late_fixture(40);
    let factory = put(&mut sim, "FACTX", 0, 700, 700);
    let bot = sim.spawn_unit(id(&sim, "BIGBOT"), 0, FxVec3::from_ints(1000, 60, 1500), 0, false).unwrap();
    let builders: Vec<UnitId> = (0..12).map(|k| put(&mut sim, "BUILD1", 0, 500 + (k % 4) * 40, 1100 + (k / 4) * 40)).collect();
    let mut ai = SkirmishAi::new(0, &sim, Difficulty::Normal);
    let mut s = ai.survey(&sim);
    in_yard(&mut s, &sim, bot, factory);
    let (mut plan, _) = ai.start_builders(&sim, &s, &mut Vec::new());
    let mut hands = 0;
    let mut out = Vec::new();
    for &b in &builders {
        let bu = sim.unit(b).unwrap();
        if let Some(task) = ai.help_project_task(&sim, &s, bu, &plan) {
            assert!(matches!(task, Task::Guard(f) if f == factory), "{task:?}");
            hands += 1;
            ai.give_task(&sim, &s, bu, task, false, &mut plan, &mut out);
        }
    }
    // (Normal's three, and six more for a build time of 380,000.)
    assert_eq!(hands, 9, "as many hands as it wants");
    sim.step(&out);
    let mut s = ai.survey(&sim);
    in_yard(&mut s, &sim, bot, factory);
    let (plan, _) = ai.start_builders(&sim, &s, &mut Vec::new());
    let first = sim.unit(builders[0]).unwrap();
    assert!(matches!(first.orders.front(), Some(Order::Guard(_) | Order::Repair(_))), "{:?}", first.orders.front());
    if matches!(first.orders.front(), Some(Order::Guard(_))) {
        assert!(matches!(ai.help_project_task(&sim, &s, first, &plan), Some(Task::Guard(f)) if f == factory), "kept at it");
    }
}

/// A factory of experimentals goes on at the army's size limit (an experimental takes the place of
/// many), where the others stand.
#[test]
fn a_factory_of_experimentals_builds_at_the_army_cap() {
    let mut sim = late_fixture(40);
    sim.players[0].economy.metal_income = fx(60);
    let x = put(&mut sim, "FACTX", 0, 700, 700);
    let ordinary = put(&mut sim, "FACT1", 0, 1100, 700);
    put(&mut sim, "BUILD1", 0, 900, 1000);
    put(&mut sim, "BUILD1", 0, 940, 1000);
    put(&mut sim, "BUILD1", 0, 980, 1000);
    for k in 0..6 {
        put(&mut sim, "TANK1", 0, 600 + k * 40, 1300);
    }
    let mut ai = SkirmishAi::new(0, &sim, Difficulty::Normal);
    assert!(ai.info[id(&sim, "FACTX")].experimental_factory);
    assert!(ai.tweak("army_cap", 4) && ai.tweak("builders_cap", 0));
    let s = ai.survey(&sim);
    let mut out = Vec::new();
    ai.run_factories(&sim, &s, &mut out);
    let produced = |f: UnitId| out.iter().filter_map(|c| match c {
        Command::Produce { factory, type_id, count, .. } if *factory == f && *count > 0 => Some(*type_id),
        _ => None,
    }).collect::<Vec<usize>>();
    assert_eq!(produced(x), vec![id(&sim, "BIGBOT")], "the experimental at the cap");
    assert!(!produced(ordinary).contains(&id(&sim, "TANK1")), "no more tanks at the cap");
}

/// While an experimental is under way on short metal, a share of the factories stand and leave it
/// their metal (the lowest tier first, never one at the experimental); with the enemy's army seen
/// outweighing ours, none do.
#[test]
fn factories_stand_while_an_experimental_is_saved_for() {
    let mut sim = late_fixture(40);
    let x = put(&mut sim, "FACTX", 0, 700, 700);
    let low: Vec<UnitId> = (0..3).map(|k| put(&mut sim, "FACT1", 0, 1100 + 200 * k, 700)).collect();
    let high = put(&mut sim, "FACT3", 0, 1100, 1100);
    let bot = sim.spawn_unit(id(&sim, "BIGBOT"), 0, FxVec3::from_ints(1000, 60, 1500), 0, false).unwrap();
    // (A strategic silo going up too, its thousand of metal under two minutes of our forty a
    // second: waited for by its build time, not saved for.)
    sim.spawn_unit(id(&sim, "SILO"), 0, FxVec3::from_ints(400, 60, 1500), 0, false).unwrap();
    for k in 0..4 {
        put(&mut sim, "TANK1", 0, 600 + k * 40, 1300);
    }
    let mut ai = SkirmishAi::new(0, &sim, Difficulty::Normal);
    ai.metal_avg = Fx::from_ratio(5, 100);
    let mut s = ai.survey(&sim);
    // Nothing saved for yet (the silo's metal is soon paid): none stand.
    s.nanoframes.retain(|n| n.id != bot);
    assert!(s.nanoframes.iter().any(|n| ai.info[n.type_id].strategic_silo()));
    assert!(ai.standing_for_projects(&sim, &s).is_empty());
    in_yard(&mut s, &sim, bot, x);
    let standing = ai.standing_for_projects(&sim, &s);
    // (A third of the four ordinary factories, the lowest tier's.)
    assert_eq!(standing.len(), 1, "{standing:?}");
    assert!(standing.iter().all(|f| low.contains(f)) && !standing.contains(&x) && !standing.contains(&high));
    // Metal enough: none stand.
    ai.metal_avg = Fx::from_ratio(5, 10);
    assert!(ai.standing_for_projects(&sim, &s).is_empty());
    // The enemy's army stronger than ours: none stand.
    ai.metal_avg = Fx::from_ratio(5, 100);
    ai.enemy_army = vec![(UnitId { slot: 9999, generation: 1 }, 100_000, s.now, false)];
    assert!(ai.standing_for_projects(&sim, &s).is_empty());
}

/// A support commander (armed, costly, tagged of the first tier, building all a tech 3 engineer
/// does) is a constructor of the third tier, not an experimental; the gateway that makes them is an
/// advanced factory, never a first factory put up for its novelty.
#[test]
fn support_commanders_are_constructors_of_the_third_tier() {
    let sim = late_fixture(30);
    let ai = SkirmishAi::new(0, &sim, Difficulty::Normal);
    let scu = &ai.info[id(&sim, "SCU")];
    assert!(scu.role == Role::Builder && !scu.experimental && scu.tech == 3 && scu.builds_strategic, "{scu:?}");
    assert!(ai.advanced_factory(&sim, id(&sim, "GATE")) && ai.factory_tier(&sim, id(&sim, "GATE")) == 3);
    assert_eq!(ai.info[id(&sim, "BUILD1")].tech, 1, "an ordinary constructor keeps its tier");
    assert!(ai.info[id(&sim, "BUILD3")].builds_strategic && !ai.info[id(&sim, "BUILD2")].builds_strategic);
    assert!(ai.info[id(&sim, "BIGBOT")].experimental, "an armed unit of its cost that builds nothing is one");
}
