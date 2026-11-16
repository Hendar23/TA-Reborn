//! The player's own copy of Forged Alliance, its units read whole: every blueprint, and for every
//! unit a player can make, its meshes, textures and animations, and the bones its weapons and
//! build beams name; the conventions the conversion rests on; and every unit made a TA model the
//! same, byte for byte, twice. Ignored (it needs the game installed, and takes a while):
//! `cargo test --release -p ta_supcom --test installed_units -- --ignored --nocapture`
//! (`TA_SUPCOM_DIR=<folder>` a copy of the game elsewhere).

use std::collections::BTreeMap;
use std::path::PathBuf;

use ta_supcom::files::Files;
use ta_supcom::install::{self, Install, Store};
use ta_formats::surface::Material;
use ta_supcom::factions::{make, model, pictures, textures};
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

/// What the conversion takes the meshes to be (docs/SUPCOM_UNITS.md), held over every unit a
/// player can make: the first vector of a vertex's frame is its normal (the triangles' corners
/// in order wind anticlockwise about it), the next two run against the texture's u and v; a
/// bone's bind pose (the inverse of the file's matrix) is its rest pose (its place and turn
/// composed down the skeleton); the units face +z, their right at -x.
#[test]
#[ignore]
fn the_meshes_are_as_the_conversion_takes_them() {
    let Some(game) = the_game() else {
        eprintln!("no copy of Forged Alliance found: nothing to read");
        return;
    };
    let files = Files::own(&game).unwrap();
    let all = unit::catalogue(&files).unwrap();
    let (mut wound, mut against, mut triangles, mut apart) = (0usize, 0usize, 0usize, Vec::new());
    let mut meshes = BTreeMap::new();
    for u in all.iter().filter(|u| u.playable()) {
        let Some(lod0) = u.display.lods.first() else { continue };
        let Some(bytes) = files.read(&lod0.mesh).unwrap() else { continue };
        let mesh = scm::parse(&bytes).unwrap();
        for t in &mesh.triangles {
            let p = |i: u32| mesh.vertices[i as usize].position;
            let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
            let (e1, e2) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
            let n = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
            let v = &mesh.vertices[t[0] as usize];
            if n.iter().map(|x| x * x).sum::<f32>() < 1e-12 {
                continue;
            }
            triangles += 1;
            wound += (n[0] * v.normal[0] + n[1] * v.normal[1] + n[2] * v.normal[2] > 0.0) as usize;
            // (The tangent against u: along the corners' u, the place goes the other way.)
            let (du1, du2) = (mesh.vertices[t[1] as usize].uv0[0] - v.uv0[0], mesh.vertices[t[2] as usize].uv0[0] - v.uv0[0]);
            let (dv1, dv2) = (mesh.vertices[t[1] as usize].uv0[1] - v.uv0[1], mesh.vertices[t[2] as usize].uv0[1] - v.uv0[1]);
            let det = du1 * dv2 - du2 * dv1;
            if det.abs() > 1e-9 {
                let dpdu = [0, 1, 2].map(|k| (e1[k] * dv2 - e2[k] * dv1) / det);
                against += (dpdu[0] * v.tangent[0] + dpdu[1] * v.tangent[1] + dpdu[2] * v.tangent[2] < 0.0) as usize;
            }
        }
        let rest = mesh.rest_poses();
        let size = mesh.vertices.iter().fold(1e-6f32, |m, v| m.max(v.position.iter().fold(0f32, |a, c| a.max(c.abs()))));
        for (b, bone) in mesh.bones.iter().enumerate() {
            let m = bone.rest_inverse;
            let t = [m[12], m[13], m[14]];
            let bind = [0, 1, 2].map(|k| -(t[0] * m[4 * k] + t[1] * m[4 * k + 1] + t[2] * m[4 * k + 2]));
            let d = (0..3).map(|k| (bind[k] - rest[b].0[k]).powi(2)).sum::<f32>().sqrt();
            if d > size * 0.02 && mesh.vertices.iter().any(|v| v.bone == b) {
                apart.push(format!("{} {}", u.id, bone.name));
            }
        }
        meshes.insert(u.id.clone(), mesh);
    }
    eprintln!("{wound} of {triangles} triangles wound about their normals, {against} with tangents against u");
    assert!(wound * 100 >= triangles * 99, "the normal comes first");
    assert!(against * 100 >= triangles * 90, "the tangent runs against u");
    assert!(apart.is_empty(), "bones bound away from their rest: {apart:?}");
    // The commanders' right arms at -x, their left at +x; the Striker's gun forward of its hull.
    let place = |id: &str, bone: &str| -> [f32; 3] {
        let mesh = &meshes[id];
        mesh.rest_poses()[mesh.bone(bone).unwrap_or_else(|| panic!("{id} has no {bone}"))].0
    };
    assert!(place("XSL0001", "Right_Arm_B01")[0] < 0.0 && place("XSL0001", "Left_Arm_B01")[0] > 0.0);
    assert!(place("UAL0001", "Right_Turret_Muzzle")[0] < 0.0 && place("UAL0001", "Left_Turret_Muzzle")[0] > 0.0);
    assert!(place("UEL0201", "Turret_Muzzle")[2] > 0.0 && place("UEL0201", "Exhaust_Left")[2] < 0.0);
}

/// Every unit a player can make made a TA model with its surface, its paint, its model for far
/// off and its build picture, as `ta-tools supcom-units` makes them: each surface fits its model
/// as written and read back, the far model has the near one's pieces, every unit has its icon,
/// and making them again gives the same bytes (a mod of units is compared byte for byte before a
/// multiplayer battle).
#[test]
#[ignore]
fn every_unit_is_made_a_model_the_same_twice() {
    let Some(game) = the_game() else {
        eprintln!("no copy of Forged Alliance found: nothing to read");
        return;
    };
    let files = Files::own(&game).unwrap();
    let all = unit::catalogue(&files).unwrap();
    let make = |u: &unit::UnitBlueprint| -> Vec<Vec<u8>> {
        let lod0 = &u.display.lods[0];
        let mesh = scm::parse(&files.read(&lod0.mesh).unwrap().unwrap()).unwrap();
        let paint: Vec<Vec<u8>> = [&lod0.albedo, &lod0.specular, &lod0.normals].iter().map(|p| files.read(p).unwrap().map(|b| textures::texture(&b).unwrap()).unwrap_or_default()).collect();
        let material = Material { name: u.id.to_ascii_lowercase(), albedo: "a".into(), specular: "s".into(), normals: "n".into(), mean: textures::mean(&paint[0]) };
        let near = model::convert(&mesh, u.display.uniform_scale, material.clone());
        let mut out = vec![near.model.to_bytes(), near.surface.to_bytes()];
        let back = ta_formats::model3do::Model3do::parse(&out[0]).unwrap();
        assert!(near.surface.fits(&back), "{}: its surface fits its model as read back", u.id);
        if let Some(bytes) = u.display.lods.get(1).and_then(|l| files.read(&l.mesh).unwrap()) {
            let far = model::convert_onto(&scm::parse(&bytes).unwrap(), &mesh, &near, u.display.uniform_scale, material);
            let far_back = ta_formats::model3do::Model3do::parse(&far.model.to_bytes()).unwrap();
            assert!(far.surface.fits(&far_back), "{}: the far surface fits", u.id);
            assert_eq!(far_back.pieces.iter().map(|p| &p.name).collect::<Vec<_>>(), back.pieces.iter().map(|p| &p.name).collect::<Vec<_>>(), "{}: the far model's pieces", u.id);
            out.extend([far.model.to_bytes(), far.surface.to_bytes()]);
        }
        out.extend(paint);
        let icon = files.read(&pictures::icon_path(u)).unwrap().unwrap_or_else(|| panic!("{}: no icon", u.id));
        let frame = pictures::frame_path(u).and_then(|p| files.read(&p).unwrap());
        assert!(frame.is_some(), "{}: no frame for its kind", u.id);
        out.push(pictures::picture(&icon, frame.as_deref()).unwrap());
        out
    };
    let playable: Vec<_> = all.iter().filter(|u| u.playable() && !u.display.lods.is_empty()).collect();
    let mut bytes = 0usize;
    for u in &playable {
        let first = make(u);
        bytes += first.iter().map(Vec::len).sum::<usize>();
        assert!(first == make(u), "{}: made again, the same bytes", u.id);
    }
    eprintln!("{} units made twice the same, {} MB each time", playable.len(), bytes >> 20);
}



/// The whole mod, as `ta-tools supcom-units` and the launcher make it, made twice from the
/// player's copy: every unit made, every file the same both times (each file's path, size and
/// hash compared in the order they come), each unit with its model, surface, paint, picture,
/// unit file and script, and its factions' side, menus, wrecks and weapons.
#[test]
#[ignore]
fn the_mod_is_made_the_same_twice() {
    use std::hash::{Hash, Hasher};
    let Some(game) = the_game() else {
        eprintln!("no copy of Forged Alliance found: nothing to read");
        return;
    };
    let files = Files::own(&game).unwrap();
    let units: Vec<_> = unit::catalogue(&files).unwrap().into_iter().filter(|u| u.playable()).collect();
    let run = || {
        let mut out: Vec<(String, usize, u64)> = Vec::new();
        let made = make::make(&files, &units, "test", &mut |path, bytes| {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            bytes.hash(&mut h);
            out.push((path.to_string(), bytes.len(), h.finish()));
            Ok(())
        }, &mut |_, _| true)
        .unwrap();
        (made, out)
    };
    let (made, first) = run();
    assert_eq!(made.units, units.len(), "every unit made: {:?}", made.skipped);
    let (_, second) = run();
    assert!(first == second, "made again, the same files");
    // Stopped by its watch before the third unit: two made, and said so.
    let stopped = make::make(&files, &units, "test", &mut |_, _| Ok(()), &mut |i, _| i < 2).unwrap();
    assert!(stopped.stopped && stopped.units == 2, "{stopped:?}");
    for u in &units {
        let id = u.id.to_ascii_lowercase();
        for path in [format!("objects3d/{id}.3do"), format!("objects3d/{id}.surface"), format!("units/{id}.fbi"), format!("scripts/{id}.cob"), format!("unitpics/{id}.pcx")] {
            assert!(first.iter().any(|(p, ..)| *p == path), "{path}");
        }
    }
    for path in ["gamedata/sidedata_supcom.tdf", "download/supcom_uef.tdf", "features/corpses/supcom_cybran.tdf", "weapons/supcom_seraphim.tdf", "mod.toml"] {
        assert!(first.iter().any(|(p, ..)| p == path), "{path}");
    }
    eprintln!("{} files, {} MB, the same twice", first.len(), first.iter().map(|f| f.1).sum::<usize>() >> 20);
}
