//! The computer players and adjacency, with made-up buildings: a power plant whose bonuses reach a
//! factory goes up beside it (not in the way of what the factory makes, nor on what its upgrade
//! takes), and storage goes beside an extractor whose metal it raises where that pays.

use ta_data::{AdjacencyAffect, AdjacencyDef};

use super::*;

/// The fixture with a plant giving factories of size 16 a cheaper build, storage raising what
/// extractors of size 4 make, a factory of size 16 (four cells a side) and an extractor of size 4.
fn sim() -> Sim {
    let mut sim = fixture_edit(|data| {
        let bonus = |affect: AdjacencyAffect, add: f64, size: u32| AdjacencyDef { name: format!("{affect:?}{size}"), affect: Some(affect), affects: affect.key().into(), add, size, category: String::new() };
        let factory = data.unit_id("FACTORY").unwrap();
        let f = &mut data.units[factory];
        (f.footprint_x, f.footprint_z, f.adjacency_size) = (4, 4, 16);
        let extractor = data.unit_id("EXTRACTOR").unwrap();
        data.units[extractor].adjacency_size = 4;
        let mut plant = data.units[extractor].clone();
        (plant.name, plant.object, plant.extracts_metal, plant.energy_make) = ("PLANT".into(), "PLANT".into(), 0.0, 20.0);
        plant.adjacency = vec![bonus(AdjacencyAffect::BuildEnergy, -0.25, 16)];
        let mut store = data.units[extractor].clone();
        (store.name, store.object, store.extracts_metal, store.metal_storage) = ("STORE".into(), "STORE".into(), 0.0, 500.0);
        store.adjacency = vec![bonus(AdjacencyAffect::MetalMake, 0.125, 4)];
        data.units.push(plant);
        data.units.push(store);
        let builder = data.unit_id("BUILDER").unwrap();
        data.units[builder].build_options.extend(["PLANT".to_string(), "STORE".to_string(), "EXTRACTOR".to_string()]);
    });
    sim.set_starting_resources(0, fx(10_000), fx(100_000));
    sim
}

/// A plant goes up touching the factory, along the whole of a side.
#[test]
fn a_plant_goes_up_beside_a_factory_it_helps() {
    let mut sim = sim();
    let factory = sim.spawn_unit(sim.types.id("FACTORY").unwrap(), 0, FxVec3::from_ints(400, 60, 400), 0, true).unwrap();
    let builder = sim.spawn_unit(sim.types.id("BUILDER").unwrap(), 0, FxVec3::from_ints(300, 60, 300), 0, true).unwrap();
    sim.step(&[]);
    let ai = SkirmishAi::new(0, &sim, Difficulty::Hard);
    let s = ai.survey(&sim);
    let plant = sim.types.id("PLANT").unwrap();
    let site = ai.plant_beside_factory(&sim, &s, sim.unit(builder).unwrap(), plant, &Plan::default()).expect("a site beside it");
    let (taken, given) = sim.adjacency_preview(plant, site, 0);
    assert!(taken.is_none());
    assert_eq!(given.iter().map(|&(id, _)| id).collect::<Vec<_>>(), [factory], "touching the factory, its bonus reaching it");
    // Not a plant that gives nothing.
    let extractor = sim.types.id("EXTRACTOR").unwrap();
    assert!(ai.plant_beside_factory(&sim, &s, sim.unit(builder).unwrap(), extractor, &Plan::default()).is_none());
}

/// Storage goes up touching a rich extractor (its metal pays for it soon), none beside a poor one.
#[test]
fn storage_goes_beside_an_extractor_where_it_pays() {
    let mut sim = sim();
    let extractor = sim.types.id("EXTRACTOR").unwrap();
    let rich = sim.spawn_unit(extractor, 0, FxVec3::from_ints(300, 60, 400), 0, true).unwrap();
    let (x, z) = sim.unit(rich).unwrap().occupied_origin.unwrap();
    for dz in 0..2 {
        for dx in 0..2 {
            sim.world.set_metal(x + dx, z + dz, 255);
        }
    }
    let builder = sim.spawn_unit(sim.types.id("BUILDER").unwrap(), 0, FxVec3::from_ints(300, 60, 300), 0, true).unwrap();
    for _ in 0..3 {
        sim.step(&[]);
    }
    let ai = SkirmishAi::new(0, &sim, Difficulty::Hard);
    let s = ai.survey(&sim);
    // (The types added to the data after it was read are no menu's: offered by hand.)
    let store = sim.types.id("STORE").unwrap();
    let opts = vec![store, extractor];
    let task = ai.storage_beside_task(&sim, &s, sim.unit(builder).unwrap(), &opts, &Plan::default());
    let Some(Task::Build(t, site)) = task else { panic!("storage beside it: {task:?}") };
    assert_eq!(t, store);
    assert_eq!(sim.adjacency_preview(store, site, 0).1.iter().map(|&(id, _)| id).collect::<Vec<_>>(), [rich]);
    // An extractor on bare ground makes nothing: no storage beside it.
    let mut sim2 = self::sim();
    sim2.spawn_unit(extractor, 0, FxVec3::from_ints(300, 60, 400), 0, true).unwrap();
    let builder = sim2.spawn_unit(sim2.types.id("BUILDER").unwrap(), 0, FxVec3::from_ints(300, 60, 300), 0, true).unwrap();
    sim2.step(&[]);
    let ai = SkirmishAi::new(0, &sim2, Difficulty::Hard);
    let s = ai.survey(&sim2);
    assert!(ai.storage_beside_task(&sim2, &s, sim2.unit(builder).unwrap(), &opts, &Plan::default()).is_none());
}
