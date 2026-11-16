//! The factions' mod made whole: every file, in a fixed order, handed to a writer (a folder, an
//! archive, a test), from the player's copy of Forged Alliance. What `ta-tools supcom-units` and
//! the launcher make.

use ta_formats::surface::Material;

use super::{faction, model, pictures, script, textures, units, UNIT_SCALE};
use crate::files::Files;
use crate::scm::{self, Mesh};
use crate::unit::{Faction, UnitBlueprint};
use crate::Result;

/// The mod's id and name.
pub const MOD_ID: &str = "supcom-factions";
pub const MOD_NAME: &str = "Supreme Commander factions";

/// What was made.
#[derive(Clone, Debug, Default)]
pub struct Made {
    pub units: usize,
    pub files: usize,
    pub bytes: u64,
    /// Units left out, and why.
    pub skipped: Vec<(String, String)>,
}

/// The mod's manifest: it needs a TA Reborn as new as the one that made it (models with surfaces
/// of their own, side add-ons).
pub fn manifest(version: &str) -> String {
    format!(
        "id = \"{MOD_ID}\"\nname = \"{MOD_NAME}\"\nversion = \"{version}\"\nrequires_reborn = \"{version}\"\ndescription = \"Supreme Commander: Forged Alliance's four factions, made from the player's own copy of the game.\"\ntags = [\"units\", \"factions\", \"supreme commander\"]\n"
    )
}

/// Makes the mod of `units` (the playable ones of the player's copy), writing each file through
/// `write` (a path in the mod, its bytes), in a fixed order: the same bytes on every computer.
pub fn make(files: &Files, units: &[UnitBlueprint], version: &str, write: &mut dyn FnMut(&str, &[u8]) -> Result<()>) -> Result<Made> {
    let mut made = Made::default();
    let mut put = |path: &str, bytes: &[u8], made: &mut Made| -> Result<()> {
        made.files += 1;
        made.bytes += bytes.len() as u64;
        write(path, bytes)
    };
    put("mod.toml", manifest(version).as_bytes(), &mut made)?;
    let mut done: Vec<&UnitBlueprint> = Vec::new();
    for u in units {
        match unit(files, u, &mut |path, bytes| put(path, bytes, &mut made))? {
            Ok(()) => done.push(u),
            Err(why) => made.skipped.push((u.id.clone(), why)),
        }
    }
    made.units = done.len();
    // The factions' files.
    let factions: Vec<(Faction, Vec<&UnitBlueprint>)> = Faction::PLAYABLE.into_iter().map(|f| {
        let theirs = done.iter().copied().filter(|u| u.faction == f).collect();
        (f, theirs)
    }).filter(|(_, u): &(Faction, Vec<&UnitBlueprint>)| !u.is_empty()).collect();
    put("gamedata/sidedata_supcom.tdf", faction::side_add_on(&factions).as_bytes(), &mut made)?;
    for (f, theirs) in &factions {
        let name = units::side(f).to_ascii_lowercase();
        put(&format!("download/supcom_{name}.tdf"), faction::menus(theirs).as_bytes(), &mut made)?;
        put(&format!("features/corpses/supcom_{name}.tdf"), faction::wrecks(theirs).as_bytes(), &mut made)?;
        put(&format!("weapons/supcom_{name}.tdf"), faction::weapons(theirs).as_bytes(), &mut made)?;
    }
    Ok(made)
}

/// One unit's files: its model with its surface and its model for far off, its paint, its build
/// picture, its unit file and its script. `Err(why)` (inside the Ok) when it is left out.
fn unit(files: &Files, u: &UnitBlueprint, put: &mut dyn FnMut(&str, &[u8]) -> Result<()>) -> Result<std::result::Result<(), String>> {
    let Some(lod0) = u.display.lods.first() else { return Ok(Err("no mesh".into())) };
    let Some(bytes) = files.read(&lod0.mesh)? else { return Ok(Err(format!("no {}", lod0.mesh))) };
    let mesh: Mesh = match scm::parse(&bytes) {
        Ok(m) => m,
        Err(e) => return Ok(Err(format!("{}: {e}", lod0.mesh))),
    };
    let name = u.id.to_ascii_lowercase();
    // Its paint: each texture made the surface's, or a plain one where it has none; the
    // Seraphim's team colour moved from their albedo's alpha into the specular map's.
    let mut paint: Vec<(String, Vec<u8>)> = Vec::new();
    for (kind, source, plain) in [("albedo", &lod0.albedo, textures::PLAIN_ALBEDO), ("specteam", &lod0.specular, textures::PLAIN_SPECULAR), ("normals", &lod0.normals, textures::PLAIN_NORMALS)] {
        let bytes = match files.read(source)? {
            Some(b) => textures::texture(&b).unwrap_or_else(|_| textures::plain(plain)),
            None => textures::plain(plain),
        };
        paint.push((format!("textures/supcom/{name}_{kind}.dds"), bytes));
    }
    if lod0.shader.eq_ignore_ascii_case("seraphim") {
        paint[1].1 = textures::team_from_alpha(&paint[1].1, &paint[0].1)?;
    }
    for (path, bytes) in &paint {
        put(path, bytes)?;
    }
    let material = Material { name: name.clone(), albedo: paint[0].0.clone(), specular: paint[1].0.clone(), normals: paint[2].0.clone(), mean: textures::mean(&paint[0].1) };
    let mut near = model::convert(&mesh, u.display.uniform_scale, material.clone());
    if let Some(lod1) = u.display.lods.get(1)
        && let Some(bytes) = files.read(&lod1.mesh)?
        && let Ok(far_mesh) = scm::parse(&bytes)
    {
        let far = model::convert_onto(&far_mesh, &mesh, &near, u.display.uniform_scale, material);
        let far_name = format!("{name}_lod1");
        put(&format!("objects3d/{far_name}.3do"), &far.model.to_bytes())?;
        put(&format!("objects3d/{far_name}.surface"), &far.surface.to_bytes())?;
        near.surface.far = far_name;
    }
    put(&format!("objects3d/{name}.3do"), &near.model.to_bytes())?;
    put(&format!("objects3d/{name}.surface"), &near.surface.to_bytes())?;
    // Its build picture.
    if let Some(icon) = files.read(&pictures::icon_path(u))? {
        let frame = match pictures::frame_path(u) {
            Some(p) => files.read(&p)?,
            None => None,
        };
        if let Ok(picture) = pictures::picture(&icon, frame.as_deref()) {
            put(&format!("unitpics/{name}.pcx"), &picture)?;
        }
    }
    // Its unit file and its script.
    let weapons = units::weapons(u);
    put(&format!("units/{name}.fbi"), units::unit_file(u, &weapons, units::transport(&mesh)).as_bytes())?;
    let builds = units::builds(u);
    let plan = script::Plan {
        unit: u,
        mesh: &mesh,
        pieces: near.model.pieces.iter().map(|p| p.name.clone()).collect(),
        weapons: weapons.slots.clone(),
        factory: builds == units::Builds::Factory,
        builder: builds != units::Builds::No,
        pixels: u.display.uniform_scale * UNIT_SCALE,
    };
    put(&format!("scripts/{name}.cob"), &script::script(&plan).to_bytes())?;
    Ok(Ok(()))
}
