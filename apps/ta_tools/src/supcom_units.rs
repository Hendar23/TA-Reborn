//! `ta-tools supcom-units`: Supreme Commander: Forged Alliance's units made into TA units, into a
//! folder to mount as a mod (docs/SUPCOM_UNITS.md). For now each unit's model and a first unit
//! file, to look at in the game (`--demo gallery`).

use std::fmt::Write as _;
use std::path::Path;

use anyhow::{Context, Result};
use ta_formats::surface::Material;
use ta_supcom::factions::{UNIT_SCALE, model, textures};
use ta_supcom::files::Files;
use ta_supcom::install::{self, Install, Store};
use ta_supcom::scm;
use ta_supcom::unit::{self, Motion, UnitBlueprint};

pub fn run(out: &Path, only: Option<&str>, game: Option<&Path>) -> Result<()> {
    let install = match game {
        Some(g) => Install { root: install::validate_picked(g).context("no copy of Forged Alliance there")?, store: Store::Folder },
        None => install::discover().into_iter().next().context("no copy of Forged Alliance found (give its folder with --game)")?,
    };
    eprintln!("Forged Alliance: {} ({:?})", install.root.display(), install.store);
    let files = Files::own(&install)?;
    let wanted: Option<Vec<String>> = only.map(|o| o.split(',').map(|s| s.trim().to_ascii_uppercase()).filter(|s| !s.is_empty()).collect());
    let units: Vec<UnitBlueprint> = unit::catalogue(&files)?.into_iter().filter(|u| u.playable() && wanted.as_ref().is_none_or(|w| w.contains(&u.id.to_ascii_uppercase()))).collect();
    for dir in ["objects3d", "units", "textures/supcom"] {
        std::fs::create_dir_all(out.join(dir))?;
    }
    let manifest = out.join("mod.toml");
    if !manifest.exists() {
        std::fs::write(&manifest, "id = \"supcom-factions\"\nname = \"Supreme Commander factions\"\nversion = \"0.1\"\ndescription = \"Forged Alliance's units, made from the player's own copy.\"\n")?;
    }
    let (mut made, mut split) = (0, 0);
    for u in &units {
        let Some(lod0) = u.display.lods.first() else {
            eprintln!("{}: no mesh", u.id);
            continue;
        };
        let Some(bytes) = files.read(&lod0.mesh)? else {
            eprintln!("{}: no {}", u.id, lod0.mesh);
            continue;
        };
        let mesh = scm::parse(&bytes).with_context(|| lod0.mesh.clone())?;
        let name = u.id.to_ascii_lowercase();
        // Its paint: each texture made the surface's, or a plain one where it has none.
        let mut paths = Vec::new();
        for (kind, source, plain) in [("albedo", &lod0.albedo, textures::PLAIN_ALBEDO), ("specteam", &lod0.specular, textures::PLAIN_SPECULAR), ("normals", &lod0.normals, textures::PLAIN_NORMALS)] {
            let paint = match files.read(source)? {
                Some(bytes) => textures::texture(&bytes).with_context(|| source.clone())?,
                None => {
                    eprintln!("{}: no {source}", u.id);
                    textures::plain(plain)
                }
            };
            let path = format!("textures/supcom/{name}_{kind}.dds");
            std::fs::write(out.join(&path), &paint)?;
            paths.push((path, paint));
        }
        // (The Seraphim's team colour is in their albedo's alpha: in the specular map's, as the
        // others' is.)
        if lod0.shader.eq_ignore_ascii_case("seraphim") {
            let spec = textures::team_from_alpha(&paths[1].1, &paths[0].1)?;
            std::fs::write(out.join(&paths[1].0), &spec)?;
            paths[1].1 = spec;
        }
        let material = Material { name: name.clone(), albedo: paths[0].0.clone(), specular: paths[1].0.clone(), normals: paths[2].0.clone(), mean: textures::mean(&paths[0].1) };
        let mut converted = model::convert(&mesh, u.display.uniform_scale, material.clone());
        split += converted.split_triangles;
        // Its second level of detail, drawn far off, painted as the first.
        if let Some(lod1) = u.display.lods.get(1)
            && let Some(bytes) = files.read(&lod1.mesh)?
        {
            let far_mesh = scm::parse(&bytes).with_context(|| lod1.mesh.clone())?;
            let far = model::convert_onto(&far_mesh, &mesh, &converted, u.display.uniform_scale, material);
            let far_name = format!("{name}_lod1");
            std::fs::write(out.join(format!("objects3d/{far_name}.3do")), far.model.to_bytes())?;
            std::fs::write(out.join(format!("objects3d/{far_name}.surface")), far.surface.to_bytes())?;
            converted.surface.far = far_name;
        }
        std::fs::write(out.join(format!("objects3d/{name}.3do")), converted.model.to_bytes())?;
        std::fs::write(out.join(format!("objects3d/{name}.surface")), converted.surface.to_bytes())?;
        std::fs::write(out.join(format!("units/{name}.fbi")), unit_file(u))?;
        made += 1;
    }
    eprintln!("{made} of {} units made into {} ({split} triangles across bones)", units.len(), out.display());
    Ok(())
}

/// A first unit file: enough for the game to show the unit and move it about.
fn unit_file(u: &UnitBlueprint) -> String {
    let id = u.id.to_ascii_uppercase();
    // TA's footprint squares are 16 pixels; Forged Alliance's ogrids are UNIT_SCALE.
    let cells = |ogrids: f64| ((ogrids * UNIT_SCALE / 16.0).round() as i64).max(1);
    let [fx, fz] = u.footprint.unwrap_or([u.size[0].max(1.0), u.size[2].max(1.0)]);
    let (fx, fz) = (cells(fx), cells(fz));
    let mut f = String::new();
    let mut put = |k: &str, v: String| {
        let _ = writeln!(f, "\t{k}={v};");
    };
    put("UnitName", id.clone());
    put("Version", "1".into());
    put("Side", "ARM".into());
    put("Objectname", id.clone());
    put("Name", if u.name.is_empty() { id.clone() } else { u.name.clone() });
    put("Description", u.description.clone());
    put("FootprintX", fx.to_string());
    put("FootprintZ", fz.to_string());
    put("MaxDamage", format!("{}", u.health.round() as i64));
    put("BuildCostMetal", format!("{}", u.cost_mass.round() as i64));
    put("BuildCostEnergy", format!("{}", u.cost_energy.round() as i64));
    put("BuildTime", format!("{}", u.build_time.round() as i64));
    put("SightDistance", format!("{}", (u.intel.vision * UNIT_SCALE).round() as i64));
    put("ThreeD", "1".into());
    let (class, ted) = match u.motion {
        Motion::None => (None, "PLANT"),
        Motion::Air => (None, "VTOL"),
        Motion::Water => (Some(if fx <= 4 { "BOATS4" } else { "BOATS6" }), "SHIP"),
        Motion::Sub => (Some("BOATD3"), "SHIP"),
        Motion::Hover => (Some(if fx <= 3 { "TANKHOVER3" } else { "TANKHOVER4" }), "TANK"),
        Motion::Amphibious => (Some(if fx <= 2 { "TANKDS2" } else { "TANKDH3" }), "TANK"),
        Motion::Land => (Some(if fx <= 2 { "TANKSH2" } else { "TANKSH3" }), "TANK"),
    };
    put("TEDClass", ted.into());
    put("Category", format!("ARM {} NOTSUB", if u.motion == Motion::Air { "VTOL" } else { "NOTAIR" }));
    if u.motion != Motion::None {
        // Ogrids a second to pixels a tick (30 a second).
        put("MaxVelocity", format!("{:.2}", u.max_speed * UNIT_SCALE / 30.0));
        put("Acceleration", format!("{:.3}", (u.acceleration * UNIT_SCALE / 900.0).max(0.01)));
        put("BrakeRate", format!("{:.3}", (u.brake * UNIT_SCALE / 900.0).max(0.01)));
        // Degrees a second to TA's angle units (65,536 a turn) a second.
        put("TurnRate", format!("{}", (u.turn_rate * 65536.0 / 360.0).round() as i64));
        put("canmove", "1".into());
        put("canpatrol", "1".into());
        put("canguard", "1".into());
        put("canstop", "1".into());
        put("BMcode", "1".into());
    } else {
        put("BMcode", "0".into());
        put("MaxSlope", "10".into());
    }
    if let Some(class) = class {
        put("MovementClass", class.into());
    }
    if u.motion == Motion::Air {
        put("canfly", "1".into());
        put("cruisealt", format!("{}", (u.elevation.unwrap_or(18.0) * UNIT_SCALE / 4.0).round() as i64));
    }
    format!("[UNITINFO]\n\t{{\n{f}\t}}\n")
}
