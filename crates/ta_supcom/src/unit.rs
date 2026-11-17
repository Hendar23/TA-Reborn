//! Supreme Commander's units as their blueprints describe them (`units/<id>/<id>_unit.bp`, read
//! with [`crate::lua`]): who makes them, what they cost, how they move, see and fight, and the
//! meshes, textures and animations they are drawn with. The values are the game's own: health,
//! mass and energy as written, distances and sizes in grid squares (ogrids), speeds in ogrids a
//! second, turns in degrees (a second), times in seconds. The whole table is kept too
//! ([`UnitBlueprint::table`]) for what is not read here.

use crate::lua::{self, Table, Value};
use crate::{Error, Result};

/// The four factions of Forged Alliance; a blueprint of another (the campaign's, civilians')
/// keeps its name.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Faction {
    Uef,
    Cybran,
    Aeon,
    Seraphim,
    Other(String),
}

impl Default for Faction {
    fn default() -> Faction {
        Faction::Other(String::new())
    }
}

impl Faction {
    pub const PLAYABLE: [Faction; 4] = [Faction::Uef, Faction::Cybran, Faction::Aeon, Faction::Seraphim];

    fn of(name: &str) -> Faction {
        match name.to_ascii_lowercase().as_str() {
            "uef" => Faction::Uef,
            "cybran" => Faction::Cybran,
            "aeon" => Faction::Aeon,
            "seraphim" => Faction::Seraphim,
            _ => Faction::Other(name.to_string()),
        }
    }

    /// The faction's name as the game writes it.
    pub fn name(&self) -> &str {
        match self {
            Faction::Uef => "UEF",
            Faction::Cybran => "Cybran",
            Faction::Aeon => "Aeon",
            Faction::Seraphim => "Seraphim",
            Faction::Other(name) => name,
        }
    }
}

/// How a unit moves (`Physics.MotionType`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Motion {
    /// Structures.
    #[default]
    None,
    Land,
    /// Walks on land and the sea bed (`RULEUMT_Amphibious`, and the frigate-legged
    /// `RULEUMT_AmphibiousFloating`).
    Amphibious,
    Hover,
    /// Ships on the surface.
    Water,
    /// Submarines (`RULEUMT_SurfacingSub`).
    Sub,
    Air,
}

impl Motion {
    fn of(name: &str) -> Motion {
        match name.to_ascii_lowercase().as_str() {
            "ruleumt_land" => Motion::Land,
            "ruleumt_amphibious" | "ruleumt_amphibiousfloating" => Motion::Amphibious,
            "ruleumt_hover" => Motion::Hover,
            "ruleumt_water" => Motion::Water,
            "ruleumt_surfacingsub" => Motion::Sub,
            "ruleumt_air" => Motion::Air,
            _ => Motion::None,
        }
    }
}

/// A unit's tier: tech 1 to 3, experimental, or a commander (ACU or support commander).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tech {
    T1,
    T2,
    T3,
    Experimental,
    Commander,
}

/// Where a unit may be built and stand (`Physics.BuildOnLayerCaps`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Layers {
    pub land: bool,
    pub water: bool,
    pub seabed: bool,
    pub sub: bool,
    pub air: bool,
}

/// What a unit sees and hides (`Intel`, ogrids).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Intel {
    pub vision: f64,
    pub water_vision: f64,
    pub radar: f64,
    pub sonar: f64,
    pub omni: f64,
    pub radar_stealth: bool,
    pub sonar_stealth: bool,
    pub cloak: bool,
    /// The radius of a stealth field it gives the units round it, and of its radar jamming.
    pub stealth_field: f64,
    pub jam: f64,
}

/// A rack of barrels (`RackBones`): the bone that recoils and the muzzles shots leave from.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rack {
    pub rack: String,
    pub muzzles: Vec<String>,
}

/// A weapon (an entry of `Weapon`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WeaponBlueprint {
    pub label: String,
    /// `WeaponCategory`: 'Direct Fire', 'Indirect Fire', 'Anti Air', 'Missile', 'Bomb',
    /// 'Anti Navy', 'Defense', 'Death', 'Kamikaze', 'Direct Fire Naval', 'Direct Fire Experimental'...
    pub category: String,
    pub display_name: String,
    pub damage: f64,
    pub damage_radius: f64,
    pub damage_type: String,
    pub max_radius: f64,
    pub min_radius: f64,
    /// Shots a second (of a whole salvo).
    pub rate_of_fire: f64,
    pub muzzle_velocity: f64,
    /// `BallisticArc`: 'RULEUBA_None', 'RULEUBA_LowArc', 'RULEUBA_HighArc'.
    pub ballistic_arc: String,
    pub salvo_size: u32,
    pub salvo_delay: f64,
    pub projectile: Option<String>,
    /// For each layer the unit may be on, what it may shoot at (`FireTargetLayerCapsTable`, the
    /// layers as written: 'Land|Water|Seabed').
    pub target_layers: Vec<(String, String)>,
    pub turreted: bool,
    pub yaw_bone: Option<String>,
    pub pitch_bone: Option<String>,
    pub muzzle_bone: Option<String>,
    pub racks: Vec<Rack>,
    pub rack_recoil: f64,
    /// The turret's arc (`TurretYaw` either side of `TurretYawRange`, degrees) and speed
    /// (degrees a second), and the same up and down.
    pub yaw: (f64, f64),
    pub yaw_speed: f64,
    pub pitch: (f64, f64),
    pub pitch_speed: f64,
    pub firing_tolerance: f64,
    /// A beam's life in seconds (0: not a beam; a continuous beam keeps firing).
    pub beam_lifetime: f64,
    pub continuous_beam: bool,
    /// Energy each salvo needs before it fires.
    pub energy_required: f64,
    pub energy_drain: f64,
    /// A strategic missile's: its rings of damage (`NukeInnerRingDamage`...); none otherwise.
    pub nuke: Option<Nuke>,
    /// Needs to be ordered to fire (`ManualFire`: the commanders' overcharge, a launcher's
    /// strategic missile).
    pub manual_fire: bool,
    /// Its racks all fire at each shot (`RackFireTogether`), rather than one a shot in turn.
    pub racks_together: bool,
    /// Seconds it waits once every rack has fired (`RackSalvoReloadTime`), charges before a
    /// salvo (`RackSalvoChargeTime`) and before each muzzle fires (`MuzzleChargeDelay`).
    pub rack_reload: f64,
    pub rack_charge: f64,
    pub muzzle_charge: f64,
    /// Damage over time: `Damage` dealt `DoTPulses` times over `DoTTime` seconds (no time, once).
    pub dot_time: f64,
    pub dot_pulses: u32,
    /// A beam hits every `BeamCollisionDelay` seconds and a tick (0.1 s).
    pub beam_collision_delay: f64,
    /// Fires projectiles its launcher builds and stores (`CountedProjectile`: missiles), at most
    /// `MaxProjectileStorage` of them.
    pub counted: bool,
    pub max_storage: u32,
    /// What it may aim at alone (`TargetRestrictOnlyAllow`: 'TACTICAL MISSILE', 'AIR'...), as
    /// written.
    pub only_allow: String,
    /// Its projectile's blueprint (`ProjectileId`'s), where the game has it.
    pub shot: Option<crate::projectile::ProjectileBlueprint>,
    /// What its projectile splits into as it lands, all its splits followed (the Salvation's
    /// shell into six, each of those into six more: 36 pieces, each with the shell's damage).
    pub split: Option<Split>,
    pub table: Table,
}

/// A projectile's pieces once it has split, all the way down: how many reach the ground, and the
/// share of the projectile's damage each deals.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Split {
    pub pieces: u32,
    pub share: f64,
}

impl WeaponBlueprint {
    /// How many times its projectile's damage lands for each one fired: its pieces, each with its
    /// share (1 for a projectile that does not split).
    pub fn hits_per_shot(&self) -> f64 {
        self.split.map_or(1.0, |s| s.pieces as f64 * s.share)
    }

    /// How many times its `Damage` is dealt for each hit: its damage over time's pulses (1 without).
    pub fn pulses(&self) -> f64 {
        if self.dot_time > 0.0 { self.dot_pulses.max(1) as f64 } else { 1.0 }
    }
}

/// A strategic missile's two rings of damage.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Nuke {
    pub inner_damage: f64,
    pub inner_radius: f64,
    pub outer_damage: f64,
    pub outer_radius: f64,
}

/// A level of detail of the mesh: the mesh and its textures (game paths, lower case), and the
/// distance it is drawn to.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Lod {
    pub mesh: String,
    pub albedo: String,
    pub specular: String,
    pub normals: String,
    pub cutoff: f64,
    /// How the game shades it (`ShaderName`: `Unit`, `Insect`, `Aeon`, `Seraphim`...), which
    /// says what its textures' channels hold (docs/SUPCOM_UNITS.md); empty where none is named.
    pub shader: String,
}

/// How a unit is drawn (`Display`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Display {
    /// Mesh units to ogrids.
    pub uniform_scale: f64,
    pub lods: Vec<Lod>,
    /// The walk (and how many times faster than its frames it plays at full speed).
    pub walk: Option<(String, f64)>,
    pub deaths: Vec<String>,
    pub build_attach_bone: Option<String>,
}

/// What its wreck is worth (`Wreckage`): shares of its cost and its health.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Wreckage {
    pub mass: f64,
    pub energy: f64,
    pub health: f64,
}

/// A unit's blueprint.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UnitBlueprint {
    /// Its folder's name (`UEL0201`), as the archive writes it.
    pub id: String,
    pub faction: Faction,
    pub name: String,
    pub description: String,
    pub categories: Vec<String>,
    pub tech: Option<Tech>,
    pub health: f64,
    pub regen: f64,
    pub cost_mass: f64,
    pub cost_energy: f64,
    pub build_time: f64,
    pub build_rate: f64,
    /// What it builds (`Economy.BuildableCategory`): each entry the categories a unit must all
    /// have ('BUILTBYTIER1ENGINEER UEF').
    pub buildable: Vec<String>,
    pub production_mass: f64,
    pub production_energy: f64,
    pub upkeep_energy: f64,
    pub storage_mass: f64,
    pub storage_energy: f64,
    pub motion: Motion,
    pub max_speed: f64,
    pub max_speed_reverse: f64,
    pub acceleration: f64,
    pub brake: f64,
    pub turn_rate: f64,
    pub layers: Layers,
    pub size: [f64; 3],
    pub footprint: Option<[f64; 2]>,
    pub skirt: Option<[f64; 2]>,
    pub max_slope: Option<f64>,
    pub elevation: Option<f64>,
    pub max_airspeed: Option<f64>,
    pub intel: Intel,
    pub weapons: Vec<WeaponBlueprint>,
    pub display: Display,
    /// The bones an engineer aims its build beam with, and those the beams leave from.
    pub build_bones: Option<(Option<String>, Option<String>, Vec<String>)>,
    pub wreckage: Option<Wreckage>,
    pub upgrades_to: Option<String>,
    pub upgrades_from: Option<String>,
    pub transport_class: Option<u32>,
    /// What its script switches off as it is made (`units/<id>/<id>_script.lua`).
    pub birth: crate::behaviour::Birth,
    pub table: Table,
}

impl UnitBlueprint {
    /// Whether it has the category `name` (any case).
    pub fn is(&self, name: &str) -> bool {
        self.categories.iter().any(|c| c.eq_ignore_ascii_case(name))
    }

    /// Whether a player may make it in Forged Alliance's skirmish: of one of the four factions,
    /// in the game (not the campaign's own), and built by something (its `BUILTBY…` categories,
    /// an air transport's `TRANSPORTBUILTBY…`), or a commander.
    pub fn playable(&self) -> bool {
        Faction::PLAYABLE.contains(&self.faction)
            && (self.is("PRODUCTSC1") || self.is("PRODUCTFA"))
            && (self.is("COMMAND")
                || self.categories.iter().any(|c| {
                    let c = c.to_ascii_uppercase();
                    c.starts_with("BUILTBY") || c.starts_with("TRANSPORTBUILTBY")
                }))
    }

    /// Whether a builder whose `buildable` holds `expression` ('BUILTBYTIER1ENGINEER UEF') may
    /// make this unit: it has every category the expression names.
    pub fn matches(&self, expression: &str) -> bool {
        let mut any = false;
        for word in expression.split_whitespace() {
            any = true;
            if !self.is(word) {
                return false;
            }
        }
        any
    }
}

/// Reads a unit blueprint: `id` is its folder's name, `text` the file.
pub fn parse(id: &str, text: &str) -> Result<UnitBlueprint> {
    let statements = lua::parse(text)?;
    let table = statements
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("UnitBlueprint"))
        .and_then(|(_, value)| value.table())
        .ok_or_else(|| Error::Format(format!("{id}: no UnitBlueprint")))?
        .clone();
    let num = |path: &[&str], key: &str| -> Option<f64> { table.path(path).and_then(|t| t.get_ci(key)).and_then(Value::num) };
    let text_at = |path: &[&str], key: &str| -> Option<String> { table.path(path).and_then(|t| t.get_ci(key)).and_then(Value::str).map(str::to_string) };
    let general = |key: &str| text_at(&["General"], key);
    let categories: Vec<String> = table.get_ci("Categories").and_then(Value::table).map(|t| t.array.iter().filter_map(Value::str).map(str::to_string).collect()).unwrap_or_default();
    let has = |name: &str| categories.iter().any(|c| c.eq_ignore_ascii_case(name));
    let tech = if has("COMMAND") || has("SUBCOMMANDER") {
        Some(Tech::Commander)
    } else if has("EXPERIMENTAL") {
        Some(Tech::Experimental)
    } else if has("TECH3") {
        Some(Tech::T3)
    } else if has("TECH2") {
        Some(Tech::T2)
    } else if has("TECH1") {
        Some(Tech::T1)
    } else {
        None
    };
    let layers = table.path(&["Physics", "BuildOnLayerCaps"]).map_or_else(Layers::default, |t| {
        let on = |k: &str| t.get_ci(k).and_then(Value::bool).unwrap_or(false);
        Layers { land: on("LAYER_Land"), water: on("LAYER_Water"), seabed: on("LAYER_Seabed"), sub: on("LAYER_Sub"), air: on("LAYER_Air") }
    });
    let size = [num(&[], "SizeX").unwrap_or(1.0), num(&[], "SizeY").unwrap_or(1.0), num(&[], "SizeZ").unwrap_or(1.0)];
    let footprint = num(&["Footprint"], "SizeX").zip(num(&["Footprint"], "SizeZ")).map(|(x, z)| [x, z]);
    let skirt = num(&["Physics"], "SkirtSizeX").zip(num(&["Physics"], "SkirtSizeZ")).map(|(x, z)| [x, z]);
    let intel = table.get_ci("Intel").and_then(Value::table).map_or_else(Intel::default, |t| {
        let n = |k: &str| t.get_ci(k).and_then(Value::num).unwrap_or(0.0);
        let b = |k: &str| t.get_ci(k).and_then(Value::bool).unwrap_or(false);
        Intel {
            vision: n("VisionRadius"),
            water_vision: n("WaterVisionRadius"),
            radar: n("RadarRadius"),
            sonar: n("SonarRadius"),
            omni: n("OmniRadius"),
            radar_stealth: b("RadarStealth"),
            sonar_stealth: b("SonarStealth"),
            cloak: b("Cloak"),
            stealth_field: n("RadarStealthFieldRadius").max(n("SonarStealthFieldRadius")),
            jam: n("JamRadius"),
        }
    });
    let weapons = table.get_ci("Weapon").and_then(Value::table).map(|t| t.array.iter().filter_map(Value::table).map(weapon).collect()).unwrap_or_default();
    let buildable = table.path(&["Economy"]).and_then(|t| t.get_ci("BuildableCategory")).and_then(Value::table).map(|t| t.array.iter().filter_map(Value::str).map(str::to_string).collect()).unwrap_or_default();
    let display = display(id, table.get_ci("Display").and_then(Value::table));
    let build_bones = table.path(&["General", "BuildBones"]).map(|t| {
        let s = |k: &str| t.get_ci(k).and_then(Value::str).map(str::to_string);
        let effects = t.get_ci("BuildEffectBones").and_then(Value::table).map(|t| t.array.iter().filter_map(Value::str).map(str::to_string).collect()).unwrap_or_default();
        (s("YawBone"), s("PitchBone"), effects)
    });
    let wreckage = table.get_ci("Wreckage").and_then(Value::table).map(|t| {
        let n = |k: &str| t.get_ci(k).and_then(Value::num).unwrap_or(0.0);
        Wreckage { mass: n("MassMult"), energy: n("EnergyMult"), health: n("HealthMult") }
    });
    let blueprint_id = |s: Option<String>| s.filter(|s| !s.trim().is_empty()).map(|s| s.trim().to_string());
    Ok(UnitBlueprint {
        id: id.to_string(),
        faction: Faction::of(&general("FactionName").unwrap_or_default()),
        name: strip_loc(&general("UnitName").unwrap_or_default()),
        description: strip_loc(&text_at(&[], "Description").unwrap_or_default()),
        tech,
        health: num(&["Defense"], "MaxHealth").or_else(|| num(&["Defense"], "Health")).unwrap_or(1.0),
        regen: num(&["Defense"], "RegenRate").unwrap_or(0.0),
        cost_mass: num(&["Economy"], "BuildCostMass").unwrap_or(0.0),
        cost_energy: num(&["Economy"], "BuildCostEnergy").unwrap_or(0.0),
        build_time: num(&["Economy"], "BuildTime").unwrap_or(0.0),
        build_rate: num(&["Economy"], "BuildRate").unwrap_or(0.0),
        buildable,
        production_mass: num(&["Economy"], "ProductionPerSecondMass").unwrap_or(0.0),
        production_energy: num(&["Economy"], "ProductionPerSecondEnergy").unwrap_or(0.0),
        upkeep_energy: num(&["Economy"], "MaintenanceConsumptionPerSecondEnergy").unwrap_or(0.0),
        storage_mass: num(&["Economy"], "StorageMass").unwrap_or(0.0),
        storage_energy: num(&["Economy"], "StorageEnergy").unwrap_or(0.0),
        motion: Motion::of(&text_at(&["Physics"], "MotionType").unwrap_or_default()),
        max_speed: num(&["Physics"], "MaxSpeed").unwrap_or(0.0),
        max_speed_reverse: num(&["Physics"], "MaxSpeedReverse").unwrap_or(0.0),
        acceleration: num(&["Physics"], "MaxAcceleration").unwrap_or(0.0),
        brake: num(&["Physics"], "MaxBrake").unwrap_or(0.0),
        turn_rate: num(&["Physics"], "TurnRate").unwrap_or(0.0),
        layers,
        size,
        footprint,
        skirt,
        max_slope: num(&["Footprint"], "MaxSlope"),
        elevation: num(&["Physics"], "Elevation"),
        max_airspeed: num(&["Air"], "MaxAirspeed"),
        intel,
        weapons,
        display,
        build_bones,
        wreckage,
        upgrades_to: blueprint_id(general("UpgradesTo")),
        upgrades_from: blueprint_id(general("UpgradesFrom")),
        transport_class: num(&["Transport"], "TransportClass").map(|n| n as u32),
        birth: crate::behaviour::Birth::default(),
        categories,
        table,
    })
}

fn weapon(t: &Table) -> WeaponBlueprint {
    let n = |k: &str| t.get_ci(k).and_then(Value::num).unwrap_or(0.0);
    let s = |k: &str| t.get_ci(k).and_then(Value::str).map(str::to_string).filter(|s| !s.is_empty());
    let b = |k: &str| t.get_ci(k).and_then(Value::bool).unwrap_or(false);
    let racks = t
        .get_ci("RackBones")
        .and_then(Value::table)
        .map(|r| {
            r.array
                .iter()
                .filter_map(Value::table)
                .map(|rack| Rack {
                    rack: rack.get_ci("RackBone").and_then(Value::str).unwrap_or_default().to_string(),
                    muzzles: rack.get_ci("MuzzleBones").and_then(Value::table).map(|m| m.array.iter().filter_map(Value::str).map(str::to_string).collect()).unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default();
    let target_layers = t.get_ci("FireTargetLayerCapsTable").and_then(Value::table).map(|l| l.fields.iter().filter_map(|(k, v)| Some((k.clone(), v.str()?.to_string()))).collect()).unwrap_or_default();
    let nuke = t.get_ci("NukeInnerRingDamage").and_then(Value::num).map(|inner_damage| Nuke {
        inner_damage,
        inner_radius: n("NukeInnerRingRadius"),
        outer_damage: n("NukeOuterRingDamage"),
        outer_radius: n("NukeOuterRingRadius"),
    });
    WeaponBlueprint {
        label: s("Label").unwrap_or_default(),
        category: s("WeaponCategory").unwrap_or_default(),
        display_name: s("DisplayName").unwrap_or_default(),
        damage: n("Damage"),
        damage_radius: n("DamageRadius"),
        damage_type: s("DamageType").unwrap_or_default(),
        max_radius: n("MaxRadius"),
        min_radius: n("MinRadius"),
        rate_of_fire: n("RateOfFire"),
        muzzle_velocity: n("MuzzleVelocity"),
        ballistic_arc: s("BallisticArc").unwrap_or_default(),
        salvo_size: n("MuzzleSalvoSize").max(1.0) as u32,
        salvo_delay: n("MuzzleSalvoDelay"),
        projectile: s("ProjectileId").map(|p| p.to_ascii_lowercase()),
        target_layers,
        turreted: b("Turreted"),
        yaw_bone: s("TurretBoneYaw"),
        pitch_bone: s("TurretBonePitch"),
        muzzle_bone: s("TurretBoneMuzzle"),
        racks,
        rack_recoil: n("RackRecoilDistance"),
        yaw: (n("TurretYaw"), n("TurretYawRange")),
        yaw_speed: n("TurretYawSpeed"),
        pitch: (n("TurretPitch"), n("TurretPitchRange")),
        pitch_speed: n("TurretPitchSpeed"),
        firing_tolerance: n("FiringTolerance"),
        beam_lifetime: n("BeamLifetime"),
        continuous_beam: b("ContinuousBeam"),
        energy_required: n("EnergyRequired"),
        energy_drain: n("EnergyDrainPerSecond"),
        nuke,
        // (`ManualFire = 1` in some, `true` in others.)
        manual_fire: b("ManualFire") || n("ManualFire") > 0.0,
        racks_together: b("RackFireTogether"),
        rack_reload: n("RackSalvoReloadTime"),
        rack_charge: n("RackSalvoChargeTime"),
        muzzle_charge: n("MuzzleChargeDelay"),
        dot_time: n("DoTTime"),
        dot_pulses: n("DoTPulses").max(0.0) as u32,
        beam_collision_delay: n("BeamCollisionDelay"),
        counted: b("CountedProjectile"),
        max_storage: n("MaxProjectileStorage").max(0.0) as u32,
        only_allow: s("TargetRestrictOnlyAllow").unwrap_or_default(),
        shot: None,
        split: None,
        table: t.clone(),
    }
}

fn display(id: &str, t: Option<&Table>) -> Display {
    let own = format!("units/{0}/{0}", id.to_ascii_lowercase());
    let Some(t) = t else { return Display { uniform_scale: 1.0, lods: default_lods(&own), ..Display::default() } };
    // A unit drawn with another's mesh names its blueprint (`/Units/URL0101/URL0101_mesh.bp`, a
    // prop's for the walls); `<none>`: nothing drawn (the concrete pads).
    let stem = match t.get_ci("MeshBlueprint").and_then(Value::str).map(|m| m.trim().trim_start_matches('/').to_ascii_lowercase()) {
        Some(m) if m.starts_with('<') => return Display { uniform_scale: t.get_ci("UniformScale").and_then(Value::num).unwrap_or(1.0), ..Display::default() },
        Some(m) => match m.rsplit_once('/') {
            Some((folder, _)) => format!("{folder}/{}", folder.rsplit('/').next().unwrap_or(folder)),
            None => own,
        },
        None => own,
    };
    let lods = t
        .path(&["Mesh", "LODs"])
        .map(|l| {
            l.array
                .iter()
                .filter_map(Value::table)
                .enumerate()
                .map(|(k, lod)| {
                    let s = |key: &str| lod.get_ci(key).and_then(Value::str).filter(|s| !s.is_empty()).map(|s| s.trim_start_matches('/').to_ascii_lowercase());
                    let mut own = default_lod(&stem, k);
                    own.mesh = s("MeshName").unwrap_or(own.mesh);
                    own.albedo = s("AlbedoName").unwrap_or(own.albedo);
                    own.specular = s("SpecularName").unwrap_or(own.specular);
                    own.normals = s("NormalsName").unwrap_or(own.normals);
                    own.cutoff = lod.get_ci("LODCutoff").and_then(Value::num).unwrap_or(0.0);
                    own.shader = lod.get_ci("ShaderName").and_then(Value::str).unwrap_or("").to_string();
                    own
                })
                .collect::<Vec<Lod>>()
        })
        .filter(|l| !l.is_empty())
        .unwrap_or_else(|| default_lods(&stem));
    let anim = |key: &str| t.get_ci(key).and_then(Value::str).map(|s| s.trim_start_matches('/').to_ascii_lowercase());
    let walk = anim("AnimationWalk").map(|a| (a, t.get_ci("AnimationWalkRate").and_then(Value::num).unwrap_or(1.0)));
    let deaths = t
        .get_ci("AnimationDeath")
        .and_then(Value::table)
        .map(|d| d.array.iter().filter_map(Value::table).filter_map(|e| e.get_ci("Animation").and_then(Value::str)).map(|a| a.trim_start_matches('/').to_ascii_lowercase()).collect())
        .unwrap_or_default();
    Display {
        uniform_scale: t.get_ci("UniformScale").and_then(Value::num).unwrap_or(1.0),
        lods,
        walk,
        deaths,
        build_attach_bone: t.get_ci("BuildAttachBone").and_then(Value::str).map(str::to_string),
    }
}

/// The game's own names for a mesh's two levels and their textures, `stem` its folder and name
/// (`units/uel0201/uel0201`), when its blueprint names none.
fn default_lods(stem: &str) -> Vec<Lod> {
    vec![default_lod(stem, 0), default_lod(stem, 1)]
}

/// The game's own names for level `k` of a mesh and its textures (the first's textures unnumbered).
fn default_lod(stem: &str, k: usize) -> Lod {
    let textures = if k == 0 { stem.to_string() } else { format!("{stem}_lod{k}") };
    Lod {
        mesh: format!("{stem}_lod{k}.scm"),
        albedo: format!("{textures}_albedo.dds"),
        specular: format!("{textures}_specteam.dds"),
        normals: format!("{textures}_normalsts.dds"),
        cutoff: 0.0,
        shader: String::new(),
    }
}

/// Every unit blueprint in the game's files (`units/<id>/<id>_unit.bp`), in order of their ids,
/// each with what its script switches off as it is made and its weapons' projectiles
/// ([`attach`]); one that does not read is left out, and said in the log.
pub fn catalogue(files: &crate::files::Files) -> Result<Vec<UnitBlueprint>> {
    let mut out = Vec::new();
    for path in files.list("units/") {
        let Some((folder, file)) = path.strip_prefix("units/").and_then(|rest| rest.split_once('/')) else { continue };
        if file.contains('/') || file != format!("{folder}_unit.bp") {
            continue;
        }
        let Some(bytes) = files.read(&path)? else { continue };
        let id = folder.to_ascii_uppercase();
        match parse(&id, &String::from_utf8_lossy(&bytes)) {
            Ok(bp) => out.push(bp),
            Err(e) => log::warn!("{path}: {e}"),
        }
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    attach(files, &mut out)?;
    Ok(out)
}

/// How many times a projectile's splits are followed down (the deepest, the Salvation's, two).
const MAX_SPLITS: usize = 4;

/// Each of `units` given what the game's files say beside its blueprint: what its script
/// (`units/<id>/<id>_script.lua`) switches off as it is made ([`crate::behaviour::birth`]), and
/// each weapon's projectile's blueprint and what that splits into ([`crate::behaviour::fragments`],
/// from the projectile's script beside its blueprint). A file that is missing or does not read
/// leaves that part as it is.
pub fn attach(files: &crate::files::Files, units: &mut [UnitBlueprint]) -> Result<()> {
    use std::collections::BTreeMap;
    let mut shots: BTreeMap<String, Option<crate::projectile::ProjectileBlueprint>> = BTreeMap::new();
    let mut splits: BTreeMap<String, Option<Split>> = BTreeMap::new();
    for u in units.iter_mut() {
        let id = u.id.to_ascii_lowercase();
        if let Some(bytes) = files.read(&format!("units/{id}/{id}_script.lua"))? {
            u.birth = crate::behaviour::birth(&String::from_utf8_lossy(&bytes));
        }
        for w in &mut u.weapons {
            let Some(path) = w.projectile.as_deref().map(crate::projectile::normalise) else { continue };
            if !shots.contains_key(&path) {
                let read = crate::projectile::read(files, &path)?;
                shots.insert(path.clone(), read);
            }
            w.shot = shots[&path].clone();
            w.split = split(files, &path, &mut splits, 0)?;
        }
    }
    Ok(())
}

/// What the projectile at `path` splits into, all the way down (its script beside its blueprint:
/// `x_proj.bp`'s `x_script.lua`).
fn split(files: &crate::files::Files, path: &str, known: &mut std::collections::BTreeMap<String, Option<Split>>, depth: usize) -> Result<Option<Split>> {
    if let Some(s) = known.get(path) {
        return Ok(*s);
    }
    let Some(stem) = path.strip_suffix("_proj.bp") else { return Ok(None) };
    let found = match files.read(&format!("{stem}_script.lua"))? {
        Some(bytes) => crate::behaviour::fragments(&String::from_utf8_lossy(&bytes)),
        None => None,
    };
    let out = match found {
        Some(f) => {
            let child = crate::projectile::normalise(&f.child);
            let below = if depth + 1 < MAX_SPLITS && child != path { split(files, &child, known, depth + 1)? } else { None };
            let (pieces, share) = below.map_or((1, 1.0), |b| (b.pieces, b.share));
            Some(Split { pieces: f.count.saturating_mul(pieces), share: f.share * share })
        }
        None => None,
    };
    known.insert(path.to_string(), out);
    Ok(out)
}

/// A text the game localises ('<LOC uel0201_name>MA12 Striker') as its English, which follows the
/// key.
pub fn strip_loc(text: &str) -> String {
    crate::map::strip_loc(text).trim().to_string()
}

/// The localisation key of such a text (`uel0201_name`), if it has one.
pub fn loc_key(text: &str) -> Option<&str> {
    text.trim().strip_prefix("<LOC ").and_then(|rest| rest.split_once('>')).map(|(key, _)| key.trim())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A blueprint in the game's own words: a medium tank of the UEF with one turreted cannon.
    pub(crate) const TANK: &str = r#"
UnitBlueprint {
    Categories = { 'PRODUCTSC1', 'SELECTABLE', 'BUILTBYTIER1FACTORY', 'UEF', 'MOBILE', 'LAND', 'TECH1', 'DIRECTFIRE' },
    Defense = { ArmorType = 'Normal', Health = 300, MaxHealth = 300, RegenRate = 0 },
    Description = '<LOC uel0201_desc>Medium Tank',
    Display = {
        Mesh = { IconFadeInZoom = 130, LODs = { { LODCutoff = 100, ShaderName = 'Unit' }, { LODCutoff = 215, ShaderName = 'Unit' } } },
        UniformScale = 0.07,
    },
    Economy = { BuildCostEnergy = 266, BuildCostMass = 56, BuildTime = 280 },
    General = { FactionName = 'UEF', TechLevel = 'RULEUTL_Basic', UnitName = '<LOC uel0201_name>MA12 Striker' },
    Intel = { VisionRadius = 20 },
    Physics = {
        BuildOnLayerCaps = { LAYER_Air = false, LAYER_Land = true, LAYER_Orbit = false, LAYER_Seabed = false, LAYER_Sub = false, LAYER_Water = false },
        MaxAcceleration = 3.4, MaxBrake = 3.4, MaxSpeed = 3.4, MotionType = 'RULEUMT_Land', TurnRate = 90,
    },
    SizeX = 0.7, SizeY = 0.5, SizeZ = 0.9,
    Weapon = {
        {
            BallisticArc = 'RULEUBA_LowArc',
            Damage = 14, DamageRadius = 0, DamageType = 'Normal', DisplayName = 'Gauss Cannon',
            FireTargetLayerCapsTable = { Land = 'Land|Water|Seabed', Water = 'Land|Water|Seabed' },
            Label = 'MainGun', MaxRadius = 18, MuzzleVelocity = 30,
            ProjectileId = '/projectiles/TDFGauss01/TDFGauss01_proj.bp',
            RackBones = { { MuzzleBones = { 'Turret_Muzzle' }, RackBone = 'Turret_Barrel' } },
            RackRecoilDistance = -1, RateOfFire = 1,
            TurretBoneMuzzle = 'Turret_Muzzle', TurretBonePitch = 'Turret_Barrel', TurretBoneYaw = 'Turret',
            TurretPitch = 10, TurretPitchRange = 20, TurretPitchSpeed = 30,
            TurretYaw = 0, TurretYawRange = 180, TurretYawSpeed = 100,
            Turreted = true,
            WeaponCategory = 'Direct Fire',
        },
    },
    Wreckage = { EnergyMult = 0, HealthMult = 0.9, MassMult = 0.9 },
}
"#;

    #[test]
    fn a_tanks_blueprint_reads() {
        let bp = parse("UEL0201", TANK).unwrap();
        assert_eq!(bp.faction, Faction::Uef);
        assert_eq!(bp.name, "MA12 Striker");
        assert_eq!(bp.description, "Medium Tank");
        assert_eq!(bp.tech, Some(Tech::T1));
        assert_eq!((bp.health, bp.cost_mass, bp.cost_energy, bp.build_time), (300.0, 56.0, 266.0, 280.0));
        assert_eq!(bp.motion, Motion::Land);
        assert_eq!((bp.max_speed, bp.turn_rate), (3.4, 90.0));
        assert!(bp.layers.land && !bp.layers.water);
        assert_eq!(bp.size, [0.7, 0.5, 0.9]);
        assert_eq!(bp.intel.vision, 20.0);
        assert!(bp.playable());
        // An air transport is built by the factories' transport category; a unit built by nothing,
        // or the campaign's, is no player's.
        let transport = UnitBlueprint { faction: Faction::Uef, categories: vec!["PRODUCTSC1".into(), "TRANSPORTBUILTBYTIER1FACTORY".into()], ..Default::default() };
        assert!(transport.playable());
        assert!(!UnitBlueprint { categories: vec!["PRODUCTSC1".into()], ..transport.clone() }.playable());
        assert!(!UnitBlueprint { categories: vec!["TRANSPORTBUILTBYTIER1FACTORY".into()], ..transport }.playable());
        let gun = &bp.weapons[0];
        assert_eq!((gun.label.as_str(), gun.category.as_str()), ("MainGun", "Direct Fire"));
        assert_eq!((gun.damage, gun.max_radius, gun.rate_of_fire, gun.muzzle_velocity), (14.0, 18.0, 1.0, 30.0));
        assert_eq!((gun.yaw_bone.as_deref(), gun.pitch_bone.as_deref(), gun.muzzle_bone.as_deref()), (Some("Turret"), Some("Turret_Barrel"), Some("Turret_Muzzle")));
        assert_eq!(gun.racks, vec![Rack { rack: "Turret_Barrel".into(), muzzles: vec!["Turret_Muzzle".into()] }]);
        assert_eq!(gun.projectile.as_deref(), Some("/projectiles/tdfgauss01/tdfgauss01_proj.bp"));
        assert_eq!(gun.target_layers[0], ("Land".to_string(), "Land|Water|Seabed".to_string()));
        assert_eq!(bp.display.uniform_scale, 0.07);
        assert_eq!(bp.display.lods.len(), 2);
        assert_eq!(bp.display.lods[0].mesh, "units/uel0201/uel0201_lod0.scm");
        assert_eq!(bp.display.lods[1].albedo, "units/uel0201/uel0201_lod1_albedo.dds");
        assert_eq!(bp.wreckage, Some(Wreckage { mass: 0.9, energy: 0.0, health: 0.9 }));
    }

    #[test]
    fn a_builder_makes_what_has_every_category_its_list_names() {
        let tank = parse("UEL0201", TANK).unwrap();
        assert!(tank.matches("BUILTBYTIER1FACTORY UEF MOBILE LAND"));
        assert!(!tank.matches("BUILTBYTIER1FACTORY CYBRAN"));
        assert!(!tank.matches(""), "an empty list makes nothing");
    }

    #[test]
    fn localised_names_read_as_their_english() {
        assert_eq!(strip_loc("<LOC uel0201_name>MA12 Striker"), "MA12 Striker");
        assert_eq!(loc_key("<LOC uel0201_name>MA12 Striker"), Some("uel0201_name"));
        assert_eq!(loc_key("Plain"), None);
    }
}
