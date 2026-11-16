//! A mod's units with more weapons than TA's three fire them (a report and a check; ignored: it
//! needs a mod): `TA_TEST_MOD=<absolute mod folder> cargo test --release -p ta_sim --test
//! mod_weapons -- --ignored --nocapture` (`TA_UNITS=UEL0401,URL0402`: those units alone). The
//! Supreme Commander factions (`ta-tools supcom-units <folder>`) have 23 such units.
//!
//! Each unit with a weapon past its third slot stands on a flat map (a sea for ships and
//! submarines) among invulnerable enemies within its weapons' reach, on the ground or the water and
//! in the air, for twenty seconds. The report lists, by slot, the weapons that fired (a reload
//! starting is a shot) and those that did not. The check: some weapon past the third fires, through
//! the numbered script functions (`AimWeapon4`, `QueryWeapon4`, `FireWeapon4`...), as no weapon
//! past the third did before TA Reborn carried eight.

use std::sync::Arc;

use ta_data::unit::MAX_WEAPONS;
use ta_data::{GameData, MapInfo};
use ta_formats::tnt::{FEATURE_NONE, TntMap};
use ta_sim::{Cheat, Command, Fx, FxVec3, Locomotion, PlayerSetup, Sim, UnitTypes};

/// A flat map 96 cells a side: land 60 high, or a sea 60 deep over a bed 20 high.
fn flat_map(sea: bool) -> TntMap {
    let cells = 96 * 96;
    let (ground, sea_level) = if sea { (20, 80) } else { (60, 0) };
    TntMap { width: 96, height: 96, sea_level, tiles: Vec::new(), tile_map: Vec::new(), heights: vec![ground; cells], features: vec![FEATURE_NONE; cells], feature_names: Vec::new(), minimap: Default::default(), flags: Vec::new() }
}

/// The game and the mod `TA_TEST_MOD` names, mounted as the game mounts them.
fn game_and_mod() -> Option<ta_vfs::Vfs> {
    let Ok(install) = ta_vfs::install::discover(None) else {
        eprintln!("TA install not found; skipping");
        return None;
    };
    let Ok(dir) = std::env::var("TA_TEST_MOD") else {
        eprintln!("TA_TEST_MOD not set; skipping");
        return None;
    };
    let module = ta_vfs::mods::read_mod(std::path::Path::new(&dir)).expect("TA_TEST_MOD: a mod's folder").expect("TA_TEST_MOD: a mod's folder");
    println!("mod: {} {}", module.manifest.name, module.manifest.version);
    let mounted = ta_vfs::mods::mount(Some(&install.root), None, &[&module], &|m| m.manifest.id.clone());
    mounted.install.unwrap();
    mounted.mods.expect("the mod mounts");
    Some(mounted.vfs)
}

#[test]
#[ignore]
fn a_mods_units_fire_their_weapons_past_the_third() {
    let Some(vfs) = game_and_mod() else { return };
    let data = Arc::new(GameData::load(&vfs));
    let types = Arc::new(UnitTypes::compile(&data, &|p| vfs.read_opt(p)));
    let only: Option<Vec<String>> = std::env::var("TA_UNITS").ok().map(|v| v.split(',').map(|s| s.trim().to_ascii_uppercase()).collect());
    let many: Vec<usize> = types
        .types
        .iter()
        .filter(|t| t.weapons[3..].iter().any(Option::is_some))
        .filter(|t| only.as_ref().is_none_or(|o| o.contains(&t.name)))
        .map(|t| t.id)
        .collect();
    assert!(!many.is_empty(), "no unit of the mod carries more than three weapons");
    // Targets: sturdy enemies of the mod's own on the ground, on the water and in the air, of a
    // tank's size or a cruiser's (an experimental's footprint would not fit beside the unit).
    let sturdiest = |kind: Locomotion| {
        types.types.iter().filter(|t| t.locomotion == kind && t.def.can_move && !t.def.is_commander && t.footprint.0.max(t.footprint.1) <= 4).max_by_key(|t| (t.max_health, std::cmp::Reverse(t.id))).map(|t| t.id)
    };
    let (ground, ship, plane) = (sturdiest(Locomotion::Ground), sturdiest(Locomotion::Naval), sturdiest(Locomotion::Air));
    println!("targets: ground {:?}, water {:?}, air {:?}", ground.map(|g| &types.get(g).name), ship.map(|s| &types.get(s).name), plane.map(|p| &types.get(p).name));
    let mut past_third = 0;
    for &kind in &many {
        let t = types.get(kind);
        let sea = t.locomotion == Locomotion::Naval;
        let setups: Vec<PlayerSetup> = (0..2).map(|i| PlayerSetup { side: "ARM".into(), color: i, start: FxVec3::from_ints(200 + 1000 * i as i64, 60, 200), metal: Fx::from_int(100_000), energy: Fx::from_int(100_000), ai: false }).collect();
        let mut sim = Sim::new(types.clone(), data.clone(), &flat_map(sea), &MapInfo::default(), 0, &setups, 11);
        for p in &mut sim.players {
            p.base_storage = (Fx::from_int(10_000_000), Fx::from_int(10_000_000));
            p.economy.metal = Fx::from_int(5_000_000);
            p.economy.energy = Fx::from_int(5_000_000);
        }
        let centre = FxVec3::from_ints(768, 60, 768);
        let Some(id) = sim.spawn_unit(kind, 0, centre, 0, true) else {
            println!("{:<10} could not stand there", t.name);
            continue;
        };
        // Enemies round it, each weapon's half reach out (clear of the unit), in turn about it;
        // aircraft above.
        let surface = if sea { ship } else { ground };
        let clear = t.radius.to_f64() as i64 + 64;
        let reaches: Vec<i64> = t.weapons.iter().flatten().map(|&w| (sim.weapon_types.get(w).range.to_f64() as i64 / 2).clamp(clear, 900.max(clear))).collect();
        for (k, reach) in reaches.iter().enumerate() {
            let angle = k as f64 * std::f64::consts::TAU / reaches.len() as f64;
            let at = |r: i64| FxVec3::from_ints(768 + (r as f64 * angle.sin()) as i64, 60, 768 + (r as f64 * angle.cos()) as i64);
            if let Some(s) = surface
                && sim.spawn_unit(s, 1, at(*reach), 0, true).is_none()
            {
                println!("{:<10} (an enemy could not stand {reach} px off)", t.name);
            }
            if let Some(p) = plane {
                sim.spawn_airborne(p, 1, at(*reach), 0, None);
            }
        }
        sim.step(&[Command::Cheat { player: 1, cheat: Cheat::Invulnerable(true) }]);
        let mut shots = [0u32; MAX_WEAPONS];
        let mut last = [0u32; MAX_WEAPONS];
        for _ in 0..600 {
            sim.step(&[]);
            let Some(u) = sim.unit(id) else { break };
            for slot in 0..MAX_WEAPONS {
                let r = u.weapons[slot].reload;
                if r > last[slot] + 1 {
                    shots[slot] += 1;
                }
                last[slot] = r;
            }
        }
        let report: Vec<String> = (0..MAX_WEAPONS)
            .filter_map(|slot| {
                let w = sim.weapon_types.get(t.weapons[slot]?);
                let functions = t.callins.weapon_functions[slot];
                Some(format!("{}:{}{}", slot + 1, w.def.name, if shots[slot] > 0 { format!(" x{}", shots[slot]) } else { format!(" (none; {})", functions[0]) }))
            })
            .collect();
        println!("{:<10} {:<24} {}", t.name, t.def.display_name, report.join(", "));
        if shots[3..].iter().any(|&s| s > 0) {
            past_third += 1;
        }
    }
    println!("{} of {} units fired a weapon past their third", past_third, many.len());
    assert!(past_third > 0, "no weapon past the third fired");
}
