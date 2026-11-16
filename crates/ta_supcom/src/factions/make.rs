//! The factions' mod made whole: every file, in a fixed order, handed to a writer (a folder, an
//! archive, a test), from the player's copy of Forged Alliance. What `ta-tools supcom-units` and
//! the launcher make.

use ta_formats::surface::Material;

use super::{anims, faction, model, pictures, script, textures, units, UNIT_SCALE};
use crate::files::Files;
use crate::scm::{self, Mesh};
use crate::sca;
use crate::unit::{Faction, UnitBlueprint};
use crate::Result;

/// The mod's id and name.
pub const MOD_ID: &str = "supcom-factions";
pub const MOD_NAME: &str = "Supreme Commander factions";
/// The mod's words and tags as its manifest keeps them, in English.
pub const DESCRIPTION: &str = "Supreme Commander: Forged Alliance's four factions, their units and buildings, made from your own copy of the game.";
pub const TAGS: [&str; 3] = ["units", "factions", "supreme commander"];
/// The converter's version, the mod's own (its manifest's `version`): a mod made by an older one
/// is offered to be made again. Raised whenever what is made changes.
pub const VERSION: u32 = 3;

/// What was made.
#[derive(Clone, Debug, Default)]
pub struct Made {
    pub units: usize,
    pub files: usize,
    pub bytes: u64,
    /// Units left out, and why.
    pub skipped: Vec<(String, String)>,
    /// Stopped before every unit was made (the watch said so): the files written are no whole mod.
    pub stopped: bool,
}

/// The mod's manifest: it needs a TA Reborn as new as the one that made it, `reborn` (models with
/// surfaces of their own, side add-ons).
pub fn manifest(reborn: &str) -> String {
    let tags: Vec<String> = TAGS.iter().map(|t| format!("\"{t}\"")).collect();
    format!(
        "id = \"{MOD_ID}\"\nname = \"{MOD_NAME}\"\nversion = \"{VERSION}\"\nrequires_reborn = \"{reborn}\"\ndescription = \"{DESCRIPTION}\"\ntags = [{}]\n",
        tags.join(", ")
    )
}

/// Makes the mod of `units` (the playable ones of the player's copy), for TA Reborn `reborn` and
/// newer, writing each file through `write` (a path in the mod, its bytes), in a fixed order: the
/// same bytes on every computer. `watch` is told before each unit how many are made and the
/// unit's id: false from it stops the making there ([`Made::stopped`]).
pub fn make(files: &Files, units: &[UnitBlueprint], reborn: &str, write: &mut dyn FnMut(&str, &[u8]) -> Result<()>, watch: &mut dyn FnMut(usize, &str) -> bool) -> Result<Made> {
    let mut made = Made::default();
    let mut put = |path: &str, bytes: &[u8], made: &mut Made| -> Result<()> {
        made.files += 1;
        made.bytes += bytes.len() as u64;
        write(path, bytes)
    };
    put("mod.toml", manifest(reborn).as_bytes(), &mut made)?;
    let mut done: Vec<&UnitBlueprint> = Vec::new();
    for (i, u) in units.iter().enumerate() {
        if !watch(i, &u.id) {
            made.stopped = true;
            made.units = done.len();
            return Ok(made);
        }
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
    // Its walk and its deaths (the far model has the same pieces: they play on it as well).
    let animation = |path: &str| -> Result<Option<sca::Animation>> { Ok(files.read(path)?.and_then(|b| sca::parse(&b).ok())) };
    let walk = match &u.display.walk {
        Some((path, _)) => animation(path)?,
        None => None,
    };
    let mut deaths = Vec::new();
    for path in &u.display.deaths {
        deaths.extend(animation(path)?);
    }
    let clips = anims::anims(&mesh, &near.bones, u.display.uniform_scale * UNIT_SCALE, walk.as_ref(), &deaths);
    if !clips.clips.is_empty() {
        put(&format!("objects3d/{name}.anims"), &clips.to_bytes())?;
    }
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
