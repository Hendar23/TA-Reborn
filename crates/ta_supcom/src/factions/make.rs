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
pub const VERSION: u32 = 5;

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

/// What a unit's files are made from, read whole before any is written: a unit whose files do
/// not read is left out with nothing of it written.
struct Read {
    mesh: Mesh,
    paint: [Option<Vec<u8>>; 3],
    far: Option<Mesh>,
    walk: Option<sca::Animation>,
    deaths: Vec<sca::Animation>,
    icon: Option<Vec<u8>>,
    frame: Option<Vec<u8>>,
}

/// Reads what the unit's files are made from: `Err(why)` when one does not read (a file the
/// archive cannot give, a mesh that is no mesh).
fn read(files: &Files, u: &UnitBlueprint) -> std::result::Result<Read, String> {
    let get = |path: &str| files.read(path).map_err(|e| format!("{path}: {e}"));
    let Some(lod0) = u.display.lods.first() else { return Err("no mesh".into()) };
    let Some(bytes) = get(&lod0.mesh)? else { return Err(format!("no {}", lod0.mesh)) };
    let mesh = scm::parse(&bytes).map_err(|e| format!("{}: {e}", lod0.mesh))?;
    let paint = [get(&lod0.albedo)?, get(&lod0.specular)?, get(&lod0.normals)?];
    // (A far mesh that is no mesh is only left out: the near one draws far off too.)
    let far = match u.display.lods.get(1) {
        Some(lod1) => get(&lod1.mesh)?.and_then(|b| scm::parse(&b).ok()),
        None => None,
    };
    let animation = |path: &str| -> std::result::Result<Option<sca::Animation>, String> { Ok(get(path)?.and_then(|b| sca::parse(&b).ok())) };
    let walk = match &u.display.walk {
        Some((path, _)) => animation(path)?,
        None => None,
    };
    let mut deaths = Vec::new();
    for path in &u.display.deaths {
        deaths.extend(animation(path)?);
    }
    let icon = get(&pictures::icon_path(u))?;
    let frame = match pictures::frame_path(u) {
        Some(p) => get(&p)?,
        None => None,
    };
    Ok(Read { mesh, paint, far, walk, deaths, icon, frame })
}

/// One unit's files: its model with its surface and its model for far off, its paint, its build
/// picture, its unit file and its script. `Err(why)` (inside the Ok) when it is left out: what it
/// is made of does not read, or its model cannot be written (nothing of it written then).
fn unit(files: &Files, u: &UnitBlueprint, put: &mut dyn FnMut(&str, &[u8]) -> Result<()>) -> Result<std::result::Result<(), String>> {
    let read = match read(files, u) {
        Ok(r) => r,
        Err(why) => return Ok(Err(why)),
    };
    let lod0 = &u.display.lods[0];
    let mesh = &read.mesh;
    let name = u.id.to_ascii_lowercase();
    // Its paint: each texture made the surface's, or a plain one where it has none; the
    // Seraphim's team colour moved from their albedo's alpha into the specular map's.
    let mut paint: Vec<(String, Vec<u8>)> = Vec::new();
    for ((kind, plain), source) in [("albedo", textures::PLAIN_ALBEDO), ("specteam", textures::PLAIN_SPECULAR), ("normals", textures::PLAIN_NORMALS)].into_iter().zip(&read.paint) {
        let bytes = match source {
            Some(b) => textures::texture(b).unwrap_or_else(|_| textures::plain(plain)),
            None => textures::plain(plain),
        };
        paint.push((format!("textures/supcom/{name}_{kind}.dds"), bytes));
    }
    if lod0.shader.eq_ignore_ascii_case("seraphim") {
        match textures::team_from_alpha(&paint[1].1, &paint[0].1) {
            Ok(both) => paint[1].1 = both,
            Err(e) => return Ok(Err(format!("{}: {e}", lod0.specular))),
        }
    }
    let material = Material { name: name.clone(), albedo: paint[0].0.clone(), specular: paint[1].0.clone(), normals: paint[2].0.clone(), mean: textures::mean(&paint[0].1) };
    let mut near = match model::convert(mesh, u.display.uniform_scale, material.clone()) {
        Ok(m) => m,
        Err(why) => return Ok(Err(format!("{}: {why}", lod0.mesh))),
    };
    // (A far model that cannot be written is left out: the near one draws far off too.)
    let far = read.far.as_ref().and_then(|far_mesh| model::convert_onto(far_mesh, mesh, &near, u.display.uniform_scale, material).ok());
    for (path, bytes) in &paint {
        put(path, bytes)?;
    }
    if let Some(far) = far {
        let far_name = format!("{name}_lod1");
        put(&format!("objects3d/{far_name}.3do"), &far.model.to_bytes())?;
        put(&format!("objects3d/{far_name}.surface"), &far.surface.to_bytes())?;
        near.surface.far = far_name;
    }
    put(&format!("objects3d/{name}.3do"), &near.model.to_bytes())?;
    put(&format!("objects3d/{name}.surface"), &near.surface.to_bytes())?;
    // Its walk and its deaths (the far model has the same pieces: they play on it as well).
    let clips = anims::anims(mesh, &near.bones, u.display.uniform_scale * UNIT_SCALE, read.walk.as_ref(), &read.deaths);
    if !clips.clips.is_empty() {
        put(&format!("objects3d/{name}.anims"), &clips.to_bytes())?;
    }
    // Its build picture.
    if let Some(icon) = &read.icon
        && let Ok(picture) = pictures::picture(icon, read.frame.as_deref())
    {
        put(&format!("unitpics/{name}.pcx"), &picture)?;
    }
    // Its unit file and its script.
    let weapons = units::weapons(u);
    put(&format!("units/{name}.fbi"), units::unit_file(u, &weapons, units::transport(mesh)).as_bytes())?;
    let builds = units::builds(u);
    let plan = script::Plan {
        unit: u,
        mesh,
        pieces: near.model.pieces.iter().map(|p| p.name.clone()).collect(),
        weapons: weapons.slots.clone(),
        factory: builds == units::Builds::Factory,
        // (An air staging platform's repair work sprays as a builder's does.)
        builder: builds != units::Builds::No || u.is("AIRSTAGINGPLATFORM"),
        pixels: u.display.uniform_scale * UNIT_SCALE,
    };
    put(&format!("scripts/{name}.cob"), &script::script(&plan).to_bytes())?;
    Ok(Ok(()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    /// A blueprint of the test faction's: `cats` its categories, the rest its sections.
    fn blueprint(cats: &str, rest: &str) -> String {
        format!("UnitBlueprint {{ Categories = {{ 'PRODUCTSC1', 'UEF', {cats} }}, General = {{ FactionName = 'UEF' }}, Display = {{ UniformScale = 0.05 }}, {rest} }}")
    }

    /// A gun on the test tank's turret, firing `projectile`.
    fn gun(label: &str, extra: &str, projectile: &str) -> String {
        format!(
            "{{ Label = '{label}', WeaponCategory = 'Direct Fire', Damage = 100, MaxRadius = 22, RateOfFire = 1.5, MuzzleVelocity = 30, MuzzleSalvoDelay = 0, MuzzleSalvoSize = 1, \
             RackBones = {{ {{ RackBone = 'Turret_Barrel', MuzzleBones = {{ 'Turret_Barrel', 'Exhaust' }} }} }}, Turreted = true, TurretBoneYaw = 'Turret', TurretBonePitch = 'Turret_Barrel', \
             TurretYawSpeed = 90, TurretPitchSpeed = 45, ProjectileId = '{projectile}', FireTargetLayerCapsTable = {{ Land = 'Land|Water' }}, {extra} }}"
        )
    }

    /// A small faction in an archive of the game's own: a commander (its missile switched off as
    /// it is made), a factory and the one its upgrade makes, an engineer, a tank whose shells split,
    /// a silo storing strategic missiles and an anti-nuke, each drawn with the test tank's mesh,
    /// with the projectiles they fire and the scripts saying what they switch off and split into.
    fn archive() -> Vec<u8> {
        let mesh = crate::scm::tests::tank();
        let units: Vec<(&str, String)> = vec![
            ("UEL0001", blueprint("'COMMAND', 'MOBILE', 'LAND'", &format!(
                "Defense = {{ MaxHealth = 12000, RegenRate = 10 }}, Economy = {{ BuildCostMass = 18000, BuildCostEnergy = 5000000, BuildTime = 6000000, BuildRate = 10, BuildableCategory = {{ 'BUILTBYCOMMANDER UEF' }} }}, \
                 General = {{ FactionName = 'UEF', BuildBones = {{ YawBone = 'Turret', BuildEffectBones = {{ 'Turret_Barrel' }} }} }}, Intel = {{ VisionRadius = 26, SonarRadius = 60 }}, \
                 Physics = {{ MotionType = 'RULEUMT_Amphibious', MaxSpeed = 1.7, TurnRate = 90 }}, \
                 Weapon = {{ {}, {{ Label = 'Missile', WeaponCategory = 'Missile', Damage = 3000, MaxRadius = 60, RateOfFire = 1, CountedProjectile = true, ProjectileId = '/projectiles/Tactical/Tactical_proj.bp' }} }}",
                gun("RightZephyr", "", "/projectiles/Shell/Shell_proj.bp")
            ))),
            ("UEB0101", blueprint("'STRUCTURE', 'FACTORY', 'LAND', 'TECH1', 'BUILTBYCOMMANDER', 'BUILTBYTIER1ENGINEER'",
                "Economy = { BuildCostMass = 240, BuildCostEnergy = 2100, BuildTime = 300, BuildRate = 20, BuildableCategory = { 'BUILTBYTIER1FACTORY UEF MOBILE', 'BUILTBYTIER1FACTORY UEF STRUCTURE LAND' } }, \
                 General = { FactionName = 'UEF', UpgradesTo = 'ueb0201' }, Footprint = { SizeX = 4, SizeZ = 4 }, Display = { UniformScale = 0.05, BuildAttachBone = 'Turret' }")),
            ("UEB0201", blueprint("'STRUCTURE', 'FACTORY', 'LAND', 'TECH2', 'BUILTBYTIER1FACTORY'",
                "Economy = { BuildCostMass = 1040, BuildCostEnergy = 12000, BuildTime = 2400, BuildRate = 40, BuildableCategory = { 'BUILTBYTIER2FACTORY UEF MOBILE' } }, \
                 General = { FactionName = 'UEF', UpgradesFrom = 'ueb0101' }, Footprint = { SizeX = 4, SizeZ = 4 }")),
            ("UEL0105", blueprint("'MOBILE', 'LAND', 'TECH1', 'ENGINEER', 'BUILTBYTIER1FACTORY', 'BUILTBYTIER2FACTORY'",
                "Economy = { BuildCostMass = 52, BuildCostEnergy = 260, BuildTime = 260, BuildRate = 5, BuildableCategory = { 'BUILTBYTIER1ENGINEER UEF' } }, \
                 Physics = { MotionType = 'RULEUMT_Land', MaxSpeed = 1.9, TurnRate = 120 }")),
            ("UEL0201", blueprint("'MOBILE', 'LAND', 'TECH1', 'DIRECTFIRE', 'BUILTBYTIER1FACTORY', 'BUILTBYTIER2FACTORY'", &format!(
                "Defense = {{ MaxHealth = 300 }}, Economy = {{ BuildCostMass = 56, BuildCostEnergy = 266, BuildTime = 280 }}, Physics = {{ MotionType = 'RULEUMT_Land', MaxSpeed = 3.4, TurnRate = 90 }}, \
                 Weapon = {{ {} }}",
                gun("MainGun", "DoTPulses = 3, DoTTime = 1", "/projectiles/Frag/Frag_proj.bp")
            ))),
            ("UEB2305", blueprint("'STRUCTURE', 'TECH1', 'BUILTBYTIER1ENGINEER'",
                "Economy = { BuildCostMass = 15000, BuildCostEnergy = 210000, BuildTime = 9000, BuildRate = 1080 }, \
                 Weapon = { { Label = 'NukeMissiles', WeaponCategory = 'Missile', Damage = 0, MaxRadius = 20000, RateOfFire = 1, CountedProjectile = true, ManualFire = 1, \
                   NukeInnerRingDamage = 70000, NukeInnerRingRadius = 30, NukeOuterRingDamage = 500, NukeOuterRingRadius = 40, ProjectileId = '/projectiles/Nuke/Nuke_proj.bp' } }")),
            ("UEB4302", blueprint("'STRUCTURE', 'TECH1', 'BUILTBYTIER1ENGINEER'",
                "Economy = { BuildCostMass = 1500, BuildCostEnergy = 100000, BuildTime = 9000, BuildRate = 1080 }, \
                 Weapon = { { Label = 'AntiNuke', WeaponCategory = 'Defense', Damage = 30, MaxRadius = 90, RateOfFire = 0.6, CountedProjectile = true, \
                   TargetRestrictOnlyAllow = 'STRATEGIC MISSILE', ProjectileId = '/projectiles/AntiNuke/AntiNuke_proj.bp' } }")),
        ];
        let projectile = |cats: &str, rest: &str| format!("ProjectileBlueprint {{ Categories = {{ 'PROJECTILE', {cats} }}, {rest} }}");
        let mut files: Vec<(String, Vec<u8>)> = Vec::new();
        for (id, bp) in &units {
            files.push((format!("units/{id}/{id}_unit.bp"), bp.clone().into_bytes()));
            files.push((format!("units/{id}/{id}_lod0.scm"), mesh.clone()));
        }
        files.push(("units/UEL0001/UEL0001_script.lua".into(), b"Unit = Class(Base) { Weapons = { Missile = Class(Launcher) { OnCreate = function(self) self:SetWeaponEnabled(false) end, }, }, \
            OnStopBeingBuilt = function(self, builder, layer) self:DisableUnitIntel('Sonar') end, }".to_vec()));
        files.push(("projectiles/Shell/Shell_proj.bp".into(), projectile("'DIRECTFIRE'", "Physics = { InitialSpeed = 30 }").into_bytes()));
        files.push(("projectiles/Frag/Frag_proj.bp".into(), projectile("'INDIRECTFIRE'", "Physics = { InitialSpeed = 30 }").into_bytes()));
        files.push(("projectiles/Frag/Frag_script.lua".into(), b"Frag = Class(Base) { OnImpact = function(self, t, e) local ChildProjectileBP = '/projectiles/Shell/Shell_proj.bp' \
            self:CreateChildProjectile(ChildProjectileBP) local numProjectiles = 4 for i = 0, (numProjectiles -1) do self:CreateChildProjectile(ChildProjectileBP) end end, }".to_vec()));
        files.push(("projectiles/Tactical/Tactical_proj.bp".into(), projectile("'TACTICAL', 'MISSILE'", "Economy = { BuildCostMass = 180, BuildCostEnergy = 3600, BuildTime = 300 }, Physics = { MaxSpeed = 12, TrackTarget = true }").into_bytes()));
        files.push(("projectiles/Nuke/Nuke_proj.bp".into(), projectile("'STRATEGIC', 'MISSILE'", "Economy = { BuildCostMass = 12000, BuildCostEnergy = 1350000, BuildTime = 324000 }, Physics = { MaxSpeed = 30, Acceleration = 3 }").into_bytes()));
        files.push(("projectiles/AntiNuke/AntiNuke_proj.bp".into(), projectile("'ANTIMISSILE'", "Economy = { BuildCostMass = 3600, BuildCostEnergy = 360000, BuildTime = 259200 }, Physics = { MaxSpeed = 90, Acceleration = 30, TrackTarget = true, TurnRate = 180 }").into_bytes()));
        let entries: Vec<(&str, &[u8], bool)> = files.iter().map(|(p, b)| (p.as_str(), b.as_slice(), true)).collect();
        crate::scd::tests::zip(&entries)
    }

    /// The test faction made into the mod: every unit made, and the mod's every file (its path
    /// and bytes) the same on every computer: the same bytes twice here, and the hash the
    /// converter gives on every platform (raised whenever what it makes changes: the bytes must
    /// never depend on a platform's powers, roots or trigonometry, or on an order not fixed).
    #[test]
    fn the_mod_is_the_same_bytes_on_every_computer() {
        let dir = crate::tests::Temp::new("make");
        std::fs::write(dir.0.join("units.scd"), archive()).unwrap();
        let files = Files::of(Vec::new(), vec![crate::scd::Scd::open(&dir.0.join("units.scd")).unwrap()]);
        let units: Vec<UnitBlueprint> = crate::unit::catalogue(&files).unwrap().into_iter().filter(|u| u.playable()).collect();
        assert_eq!(units.len(), 7);
        let run = || {
            let mut hash = Sha256::new();
            let mut paths: Vec<String> = Vec::new();
            let made = make(&files, &units, "test", &mut |path, bytes| {
                hash.update(path.as_bytes());
                hash.update((bytes.len() as u64).to_le_bytes());
                hash.update(bytes);
                paths.push(path.to_string());
                Ok(())
            }, &mut |_, _| true).unwrap();
            let hex: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
            (made, paths, hex)
        };
        let (made, paths, first) = run();
        assert_eq!((made.units, made.skipped.len()), (7, 0), "{:?}", made.skipped);
        assert!(paths.iter().any(|p| p == "units/ueb0201.fbi") && paths.iter().any(|p| p == "weapons/supcom_uef.tdf"), "{paths:?}");
        assert_eq!(run().2, first, "made twice, the same bytes");
        assert_eq!(first, EXPECTED, "the converter's bytes changed: if it was meant to, put the new hash here");
    }

    /// The hash of the test faction's mod (see the test above).
    const EXPECTED: &str = "7154a5d8d73cfd11301699739563c052d272bc7ad3a50b45f6b511b40d3fef75";

    /// A unit whose mesh does not read is left out, with its reason; the rest are made.
    #[test]
    fn a_unit_that_does_not_read_is_left_out_with_its_reason() {
        let dir = crate::tests::Temp::new("make-broken");
        let good = blueprint("'MOBILE', 'LAND', 'TECH1', 'BUILTBYTIER1FACTORY'", "Physics = { MotionType = 'RULEUMT_Land', MaxSpeed = 3, TurnRate = 90 }");
        let mesh = crate::scm::tests::tank();
        let zip = crate::scd::tests::zip(&[
            ("units/UEL0201/UEL0201_unit.bp", good.as_bytes(), false),
            ("units/UEL0201/UEL0201_lod0.scm", &mesh, false),
            ("units/UEL0202/UEL0202_unit.bp", good.as_bytes(), false),
            ("units/UEL0202/UEL0202_lod0.scm", b"MODL not a mesh at all, cut short", false),
        ]);
        std::fs::write(dir.0.join("units.scd"), zip).unwrap();
        let files = Files::of(Vec::new(), vec![crate::scd::Scd::open(&dir.0.join("units.scd")).unwrap()]);
        let units: Vec<UnitBlueprint> = crate::unit::catalogue(&files).unwrap();
        let mut written: Vec<String> = Vec::new();
        let made = make(&files, &units, "test", &mut |path, _| {
            written.push(path.to_string());
            Ok(())
        }, &mut |_, _| true).unwrap();
        assert_eq!(made.units, 1);
        assert_eq!(made.skipped.len(), 1);
        assert!(made.skipped[0].0 == "UEL0202" && made.skipped[0].1.contains("uel0202_lod0.scm"), "{:?}", made.skipped);
        assert!(!written.iter().any(|p| p.contains("uel0202")), "nothing of it written: {written:?}");
    }
}
