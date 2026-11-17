//! What closes a way (`Sim::way_blockers`), on made-up ground: a pass shut by wreckage; and a
//! scenario's wave that finds its way shut by it.

use super::*;
use crate::mission::{MissionRules, Wave};

/// A 64 × 64-cell map, open to the west (x below 20) and the east (x from 44), void between but
/// for two ways east to west: a pass along the top (z 4 to 9) and, lower down, a dead end from
/// the east (z 40 to 45, x from 24) that comes nearer the west, as the crow flies, than the
/// pass does: a winding pass's dead end, as The Pass has. A kbot (2 × 2 cells, its gun arcing
/// over the ground: a made-up unit has no model to fire from up off it), a building, and
/// wreckage (what a kbot leaves); two players.
fn pass() -> Sim {
    let data = data_of(&[
        unit("KBOT", "CanMove=1; MaxVelocity=2; Acceleration=1; BrakeRate=1; TurnRate=32768; FootprintX=2; FootprintZ=2; Weapon1=GUN; BuildCostMetal=100; Corpse=WRECK;"),
        unit("DEPOT", "FootprintX=2; FootprintZ=2;"),
        ("weapons/gun.tdf".into(), "[GUN] { range=200; reloadtime=0.5; weaponvelocity=400; ballistic=1; [DAMAGE] { default=40; } }".into()),
        ("features/all/wreck.tdf".into(), "[WRECK] { footprintx=2; footprintz=2; blocking=1; reclaimable=1; metal=50; damage=200; featuredead=HEAP; } [HEAP] { footprintx=2; footprintz=2; blocking=0; reclaimable=1; metal=25; }".into()),
    ]);
    let mut map = flat_map();
    for z in 0..64 {
        for x in 20..44 {
            let pass = (4..10).contains(&z);
            let dead_end = (40..46).contains(&z) && x >= 24;
            if !pass && !dead_end {
                map.features[z * 64 + x] = FEATURE_VOID;
            }
        }
    }
    sim_on_map(data, 2, &map)
}

/// Cell (x, z)'s middle, in pixels.
fn cell(x: i64, z: i64) -> FxVec3 {
    FxVec3::from_ints(x * 16 + 8, 0, z * 16 + 8)
}

#[test]
fn the_wreckage_shutting_a_pass_is_found_not_the_dead_end_nearer_the_goal() {
    let mut sim = pass();
    let spec = sim.move_spec(sim.types.id("KBOT").unwrap());
    let (from, to) = (cell(54, 43), cell(8, 43));
    assert!(sim.way_blockers(&spec, from, to).is_empty(), "the pass is open");
    // Three wrecks across the pass, the middle of it.
    let wrecks: Vec<usize> = [5, 7, 9].iter().map(|&z| sim.place_feature("WRECK", FxVec3::from_ints(31 * 16, 0, z * 16), 0).unwrap()).collect();
    let found = sim.way_blockers(&spec, from, to);
    assert!(!found.is_empty(), "the wreckage shuts the pass");
    assert!(found.iter().all(|f| wrecks.contains(f)), "only the wreckage in the pass: {found:?}");
    // (From the dead end too, where a way that cannot get there stops.)
    let found = sim.way_blockers(&spec, cell(25, 42), to);
    assert!(!found.is_empty() && found.iter().all(|f| wrecks.contains(f)), "from the dead end: {found:?}");
    // One wreck gone (a heap, which blocks nothing), a gap a kbot gets through: open again.
    let middle = wrecks[1];
    let (origin, fp) = (sim.features[middle].origin, (2, 2));
    sim.features[middle].alive = false;
    sim.world.release(Occupant::Feature(middle as u32), origin, fp);
    assert!(sim.way_blockers(&spec, from, to).is_empty(), "a gap opened: {:?}", sim.way_blockers(&spec, from, to));
}

#[test]
fn nothing_is_blamed_where_the_ground_itself_does_not_lead() {
    let mut sim = pass();
    let spec = sim.move_spec(sim.types.id("KBOT").unwrap());
    // Wreckage at the dead end's end does not shut anything: the ground goes no further.
    sim.place_feature("WRECK", cell(25, 42), 0);
    assert!(sim.way_blockers(&spec, cell(54, 43), cell(8, 43)).is_empty());
    // Into the void: no way on the ground at all.
    assert!(sim.way_blockers(&spec, cell(54, 43), cell(30, 30)).is_empty());
}

/// A scenario's wave sent through the pass, shut by wreckage: its units, their way stopping
/// short of the player's depot beyond it, fire on the wreckage (the waves bring no constructors
/// to reclaim it) till a wreck falls to a heap, which blocks nothing, and go on through.
#[test]
fn a_waves_units_break_up_the_wreckage_shutting_their_way() {
    let mut sim = pass();
    let (kbot, depot) = (sim.types.id("KBOT").unwrap(), sim.types.id("DEPOT").unwrap());
    sim.spawn_unit(depot, 0, cell(8, 43), 0, true).unwrap();
    let wrecks: Vec<usize> = [5, 7, 9].iter().map(|&z| sim.place_feature("WRECK", FxVec3::from_ints(31 * 16, 0, z * 16), 0).unwrap()).collect();
    sim.mission = Some(MissionRules { human: 0, waves: vec![Wave { tick: 1, owner: 1, units: vec![(kbot, 4)], from: cell(54, 20), target: None, hunt: true, friendly: false }], ..Default::default() });
    let mut through = false;
    for _ in 0..30 * 120 {
        sim.step(&[]);
        if sim.units().any(|u| u.owner == 1 && u.pos.x < Fx::from_int(20 * 16)) {
            through = true;
            break;
        }
    }
    let broken = wrecks.iter().filter(|&&w| !sim.features[w].alive).count();
    assert!(broken > 0, "a wreck was broken up");
    assert!(through, "the wave got through the pass");
}
