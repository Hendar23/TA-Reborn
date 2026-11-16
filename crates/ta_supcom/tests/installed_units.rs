//! The player's own copy of Forged Alliance, its units read whole: every blueprint, and for every
//! unit a player can make, its meshes, textures and animations, and the bones its weapons and
//! build beams name. Ignored (it needs the game installed, and takes a while):
//! `cargo test --release -p ta_supcom --test installed_units -- --ignored --nocapture`
//! (`TA_SUPCOM_DIR=<folder>` a copy of the game elsewhere).

use std::collections::BTreeMap;
use std::path::PathBuf;

use ta_supcom::files::Files;
use ta_supcom::install::{self, Install, Store};
use ta_supcom::unit::{self, Faction, Tech};
use ta_supcom::{sca, scm};

fn the_game() -> Option<Install> {
    if let Some(dir) = std::env::var_os("TA_SUPCOM_DIR") {
        return Some(Install { root: PathBuf::from(dir), store: Store::Folder });
    }
    install::discover().into_iter().next()
}

/// Every unit blueprint reads, and the four factions have what a player may make of them.
#[test]
#[ignore]
fn every_unit_blueprint_reads() {
    let Some(game) = the_game() else {
        eprintln!("no copy of Forged Alliance found: nothing to read");
        return;
    };
    let files = Files::own(&game).unwrap();
    let all = unit::catalogue(&files).unwrap();
    let listed = files.list("units/").iter().filter(|p| p.ends_with("_unit.bp")).count();
    eprintln!("{} of {listed} unit blueprints read", all.len());
    assert_eq!(all.len(), listed, "every blueprint reads");
    let playable: Vec<_> = all.iter().filter(|u| u.playable()).collect();
    let mut by_faction: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    for u in &playable {
        *by_faction.entry(u.faction.name().to_string()).or_default().entry(format!("{:?}", u.tech)).or_default() += 1;
    }
    for (faction, tiers) in &by_faction {
        eprintln!("{faction}: {} units {tiers:?}", tiers.values().sum::<usize>());
    }
    for faction in Faction::PLAYABLE {
        let theirs: Vec<_> = playable.iter().filter(|u| u.faction == faction).collect();
        assert!(theirs.len() > 50, "{}: {} units", faction.name(), theirs.len());
        assert_eq!(theirs.iter().filter(|u| u.tech == Some(Tech::Commander) && u.is("COMMAND")).count(), 1, "{}: one commander", faction.name());
    }
    let nameless: Vec<&str> = playable.iter().filter(|u| u.name.is_empty() && u.description.is_empty()).map(|u| u.id.as_str()).collect();
    eprintln!("playable units with neither a name nor a description: {nameless:?}");
    // What each builder makes, by its lists' categories, and units nobody makes.
    let mut made = std::collections::BTreeSet::new();
    for builder in playable.iter().filter(|u| !u.buildable.is_empty()) {
        for u in &playable {
            if builder.buildable.iter().any(|e| u.matches(e)) {
                made.insert(u.id.clone());
            }
        }
    }
    let orphans: Vec<&str> = playable.iter().filter(|u| !made.contains(&u.id) && !u.is("COMMAND")).map(|u| u.id.as_str()).collect();
    eprintln!("{} playable units made by some builder; made by none: {orphans:?}", made.len());
}

/// Every unit a player can make: its meshes read, its textures are there, the bones its weapons
/// and build beams name are its mesh's, and its walk and deaths read and move its mesh's bones.
#[test]
#[ignore]
fn every_playable_units_meshes_and_animations_read() {
    let Some(game) = the_game() else {
        eprintln!("no copy of Forged Alliance found: nothing to read");
        return;
    };
    let files = Files::own(&game).unwrap();
    let all = unit::catalogue(&files).unwrap();
    let playable: Vec<_> = all.into_iter().filter(|u| u.playable()).collect();
    let (mut meshes, mut animations, mut vertices, mut problems) = (0usize, 0usize, 0usize, Vec::new());
    let mut largest = (0usize, String::new());
    for u in &playable {
        let Some(lod0) = u.display.lods.first() else {
            problems.push(format!("{}: no mesh", u.id));
            continue;
        };
        let mesh = match files.read(&lod0.mesh).unwrap() {
            Some(bytes) => scm::parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", lod0.mesh)),
            None => {
                problems.push(format!("{}: no {}", u.id, lod0.mesh));
                continue;
            }
        };
        meshes += 1;
        vertices += mesh.vertices.len();
        if mesh.vertices.len() > largest.0 {
            largest = (mesh.vertices.len(), u.id.clone());
        }
        for lod in &u.display.lods[1..] {
            if let Some(bytes) = files.read(&lod.mesh).unwrap() {
                scm::parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", lod.mesh));
                meshes += 1;
            }
        }
        for texture in [&lod0.albedo, &lod0.specular, &lod0.normals] {
            if files.read(texture).unwrap().is_none() {
                problems.push(format!("{}: no {texture}", u.id));
            }
        }
        let mut bones: Vec<(String, &str)> = Vec::new();
        for w in &u.weapons {
            for b in [&w.yaw_bone, &w.pitch_bone, &w.muzzle_bone].into_iter().flatten() {
                bones.push((b.clone(), "weapon"));
            }
            for rack in &w.racks {
                if !rack.rack.is_empty() {
                    bones.push((rack.rack.clone(), "rack"));
                }
                bones.extend(rack.muzzles.iter().map(|m| (m.clone(), "muzzle")));
            }
        }
        if let Some((yaw, pitch, effects)) = &u.build_bones {
            bones.extend([yaw, pitch].into_iter().flatten().map(|b| (b.clone(), "build")));
            bones.extend(effects.iter().map(|b| (b.clone(), "build effect")));
        }
        for (bone, what) in bones {
            if mesh.bone(&bone).is_none() {
                problems.push(format!("{}: its {what} bone {bone} is not its mesh's", u.id));
            }
        }
        let anims = u.display.walk.iter().map(|(a, _)| a).chain(&u.display.deaths);
        for path in anims {
            let Some(bytes) = files.read(path).unwrap() else {
                problems.push(format!("{}: no {path}", u.id));
                continue;
            };
            let anim = sca::parse(&bytes).unwrap_or_else(|e| panic!("{path}: {e}"));
            animations += 1;
            let strangers: Vec<&str> = anim.bones.iter().filter(|b| mesh.bone(b).is_none()).map(String::as_str).collect();
            if !strangers.is_empty() {
                problems.push(format!("{}: {path} moves bones its mesh has not: {strangers:?}", u.id));
            }
        }
    }
    eprintln!("{} playable units: {meshes} meshes ({vertices} vertices in their first levels; the largest {} with {}), {animations} animations", playable.len(), largest.1, largest.0);
    eprintln!("{} problems:", problems.len());
    for p in &problems {
        eprintln!("  {p}");
    }
}
