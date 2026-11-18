//! Clearing the way (`clear.rs`), on made-up ground (no install needed): a pass between us and
//! the enemy shut by wreckage.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use ta_data::{GameData, MapInfo};
use ta_formats::tnt::{FEATURE_NONE, FEATURE_VOID, TntMap};
use ta_sim::{PlayerSetup, UnitTypes};

use super::*;

/// A 64 × 64-cell map, open to the west (x below 24) and the east (x from 40), void between but
/// for a pass six cells wide (z 28 to 33), shut by three wrecks across its middle (x 30 and 31).
/// Ours: the Commander (which reclaims if `commander_reclaims`), four tanks and, with `builder`,
/// a constructor, all west; the enemy's Commander east. The wrecks and the tanks come back.
fn shut_pass(builder: bool, commander_reclaims: bool) -> (Sim, Vec<usize>, Vec<UnitId>) {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!("ta-ai-clear-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    std::fs::create_dir_all(dir.join("units")).unwrap();
    std::fs::create_dir_all(dir.join("weapons")).unwrap();
    std::fs::create_dir_all(dir.join("features/all")).unwrap();
    let reclaims = if commander_reclaims { "CanReclamate=1;" } else { "" };
    for (name, keys) in [
        ("COMMANDER", format!("Commander=1; Builder=1; WorkerTime=300; BuildDistance=120; CanMove=1; MaxVelocity=2; Acceleration=1; BrakeRate=1; TurnRate=32768; FootprintX=2; FootprintZ=2; {reclaims}")),
        ("BUILDER", "Builder=1; WorkerTime=200; BuildDistance=120; CanMove=1; MaxVelocity=2; Acceleration=1; BrakeRate=1; TurnRate=32768; FootprintX=2; FootprintZ=2; CanReclamate=1;".into()),
        ("TANK", "CanMove=1; MaxVelocity=2; Acceleration=1; BrakeRate=1; TurnRate=32768; FootprintX=2; FootprintZ=2; Weapon1=GUN;".into()),
    ] {
        std::fs::write(dir.join(format!("units/{name}.fbi")), format!("[UNITINFO] {{ UnitName={name}; Objectname={name}; MaxDamage=3000; SightDistance=400; BuildCostMetal=100; {keys} }}")).unwrap();
    }
    // (The tanks' guns arc over the ground: a made-up unit has no model to fire from up off it.)
    std::fs::write(dir.join("weapons/test.tdf"),"[GUN] { range=200; reloadtime=0.5; weaponvelocity=400; ballistic=1; [DAMAGE] { default=40; } }").unwrap();
    std::fs::write(dir.join("features/all/wreck.tdf"), "[WRECK] { footprintx=2; footprintz=2; blocking=1; reclaimable=1; metal=50; damage=200; featuredead=HEAP; } [HEAP] { footprintx=2; footprintz=2; blocking=0; reclaimable=1; metal=25; }").unwrap();
    let mut vfs = ta_vfs::Vfs::new();
    vfs.mount_mod_dir(&dir, "test").unwrap();
    let data = Arc::new(GameData::load(&vfs));
    std::fs::remove_dir_all(dir).unwrap();
    let types = Arc::new(UnitTypes::compile(&data, &|_| None));
    let cells = 64 * 64;
    let mut features = vec![FEATURE_NONE; cells];
    for z in 0..64 {
        for x in 24..40 {
            if !(28..34).contains(&z) {
                features[z * 64 + x] = FEATURE_VOID;
            }
        }
    }
    let map = TntMap { width: 64, height: 64, sea_level: 0, heights: vec![60; cells], features, tiles: vec![], tile_map: vec![], feature_names: vec![], minimap: Default::default(), flags: Vec::new() };
    let cell = |x: i64, z: i64| FxVec3::from_ints(x * 16 + 8, 60, z * 16 + 8);
    let players: Vec<_> = (0..2u8).map(|c| PlayerSetup { side: "ARM".into(), color: c, start: cell(if c == 0 { 6 } else { 56 }, 30), metal: fx(1000), energy: fx(1000), ai: c == 0 }).collect();
    let mut sim = Sim::new(types, data, &map, &MapInfo::default(), 0, &players, 7);
    sim.los_enabled = false;
    let id = |sim: &Sim, name: &str| sim.types.id(name).unwrap();
    sim.spawn_unit(id(&sim, "COMMANDER"), 0, cell(6, 30), 0, true).unwrap();
    sim.spawn_unit(id(&sim, "COMMANDER"), 1, cell(56, 30), 0, true).unwrap();
    if builder {
        sim.spawn_unit(id(&sim, "BUILDER"), 0, cell(10, 30), 0, true).unwrap();
    }
    let tanks: Vec<UnitId> = (0..4).map(|k| sim.spawn_unit(id(&sim, "TANK"), 0, cell(16, 26 + 3 * k), 0, true).unwrap()).collect();
    let wrecks: Vec<usize> = [29, 31, 33].iter().map(|&z| sim.place_feature("WRECK", FxVec3::from_ints(31 * 16, 0, z * 16), 0).unwrap()).collect();
    (sim, wrecks, tanks)
}

/// The tanks sent at the enemy (as an attack is), then the computer player left to it for up to
/// a minute: whether the pass opened, the orders it gave (to whom), and the computer player.
fn play(sim: &mut Sim, tanks: &[UnitId]) -> (bool, Vec<(Vec<UnitId>, Order)>, SkirmishAi) {
    let goal = FxVec3::from_ints(56 * 16, 60, 30 * 16);
    sim.step(&[Command::Order { player: 0, units: tanks.to_vec(), order: Order::Fight(goal), queue: false }]);
    let mut ai = SkirmishAi::new(0, sim, Difficulty::Normal);
    let spec = sim.move_spec(sim.types.id("TANK").unwrap());
    let mut given = Vec::new();
    for _ in 0..60 * SECOND {
        let out = ai.think(sim);
        for c in &out {
            if let Command::Order { units, order, .. } = c {
                given.push((units.clone(), order.clone()));
            }
        }
        sim.step(&out);
        if sim.way_blockers(&spec, sim.unit(tanks[0]).unwrap().pos, goal).is_empty() {
            return (true, given, ai);
        }
    }
    (false, given, ai)
}

#[test]
fn a_constructor_reclaims_the_wreckage_shutting_the_way_to_the_enemy() {
    let (mut sim, wrecks, tanks) = shut_pass(true, true);
    let (open, given, _) = play(&mut sim, &tanks);
    assert!(open, "the pass opened: {given:?}");
    assert!(given.iter().any(|(_, o)| matches!(o, Order::Reclaim(f) if wrecks.contains(f))), "a constructor was sent to reclaim the wreckage: {given:?}");
    assert!(!given.iter().any(|(_, o)| matches!(o, Order::Attack(Target::Ground(_)))), "the army waited for it: {given:?}");
    assert!(wrecks.iter().any(|&w| !sim.features[w].alive) && !sim.features.iter().any(|f| f.alive && sim.data.features[f.def].name.eq_ignore_ascii_case("HEAP")), "reclaimed, not broken up");
}

#[test]
fn with_no_constructor_to_send_the_army_breaks_the_wreckage_up() {
    let (mut sim, wrecks, tanks) = shut_pass(false, false);
    let (open, given, mut ai) = play(&mut sim, &tanks);
    assert!(open, "the pass opened: {given:?}");
    let gunners: Vec<UnitId> = given.iter().filter(|(_, o)| matches!(o, Order::Attack(Target::Ground(_)))).flat_map(|(u, _)| u.clone()).collect();
    assert!(!gunners.is_empty() && gunners.iter().all(|g| tanks.contains(g)), "the tanks fired on the wreckage: {given:?}");
    assert!(wrecks.iter().any(|&w| !sim.features[w].alive), "a wreck was broken up");
    // Then on to where they were going.
    let mut onward = false;
    for _ in 0..5 * SECOND {
        let out = ai.think(&sim);
        onward |= out.iter().any(|c| matches!(c, Command::Order { order: Order::Fight(_), units, .. } if units.iter().any(|u| gunners.contains(u))));
        sim.step(&out);
    }
    assert!(onward, "the gunners went on");
}
