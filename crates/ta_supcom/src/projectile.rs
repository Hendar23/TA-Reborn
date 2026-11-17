//! Supreme Commander's projectiles as their blueprints describe them
//! (`projectiles/<name>/<name>_proj.bp`, read with [`crate::lua`]), the half of a weapon its unit's
//! blueprint names by `ProjectileId`: what kind of shot it is (its categories: a `TACTICAL` or
//! `STRATEGIC` `MISSILE` is what missile defences take), what a counted one costs to build (its
//! `Economy`: a nuke, an anti-nuke, a tactical missile), and how it flies (`Physics`: speeds in
//! ogrids a second, its turn in degrees a second, whether it homes). What its script splits it
//! into, which no blueprint says, is [`crate::behaviour::fragments`]'s.

use crate::lua::{self, Table, Value};
use crate::{Error, Result};

/// A projectile's blueprint.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProjectileBlueprint {
    /// Its file, as a unit's weapon names it (lower case, no leading `/`).
    pub path: String,
    pub categories: Vec<String>,
    /// What one costs to build, and its build time (`Economy`): only counted projectiles, built
    /// by their launcher at its `Economy.BuildRate`, have them.
    pub cost_mass: f64,
    pub cost_energy: f64,
    pub build_time: f64,
    /// `Physics`: its speed leaving the muzzle, its top speed and its acceleration (ogrids a
    /// second, a second squared), whether it homes on its target and how fast it turns (degrees
    /// a second), and how long it lives (seconds; 0: as long as it flies).
    pub initial_speed: f64,
    pub max_speed: f64,
    pub acceleration: f64,
    pub tracks: bool,
    pub turn_rate: f64,
    pub lifetime: f64,
    pub table: Table,
}

impl ProjectileBlueprint {
    /// Whether it has the category `name` (any case).
    pub fn is(&self, name: &str) -> bool {
        self.categories.iter().any(|c| c.eq_ignore_ascii_case(name))
    }

    /// A strategic missile (`STRATEGIC MISSILE`: the nukes), what anti-nukes take.
    pub fn strategic_missile(&self) -> bool {
        self.is("STRATEGIC") && self.is("MISSILE")
    }

    /// A tactical missile (`TACTICAL MISSILE`: the launchers', the missile trucks', the
    /// cruisers'), what tactical missile defences take.
    pub fn tactical_missile(&self) -> bool {
        self.is("TACTICAL") && self.is("MISSILE")
    }

    /// Whether building one costs anything (a counted projectile's price).
    pub fn priced(&self) -> bool {
        self.build_time > 0.0 && (self.cost_mass > 0.0 || self.cost_energy > 0.0)
    }
}

/// The path a weapon's `ProjectileId` names, as files are read (`/projectiles/a/a_proj.bp`:
/// lower case, no leading `/`).
pub fn normalise(id: &str) -> String {
    id.trim().trim_start_matches('/').replace('\\', "/").to_ascii_lowercase()
}

/// Reads a projectile blueprint: `path` is its file's (as [`normalise`] gives it), `text` the
/// file.
pub fn parse(path: &str, text: &str) -> Result<ProjectileBlueprint> {
    let statements = lua::parse(text)?;
    let table = statements
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("ProjectileBlueprint"))
        .and_then(|(_, value)| value.table())
        .ok_or_else(|| Error::Format(format!("{path}: no ProjectileBlueprint")))?
        .clone();
    let num = |section: &str, key: &str| table.path(&[section]).and_then(|t| t.get_ci(key)).and_then(Value::num).unwrap_or(0.0);
    let categories = table.get_ci("Categories").and_then(Value::table).map(|t| t.array.iter().filter_map(Value::str).map(str::to_string).collect()).unwrap_or_default();
    let physics = table.path(&["Physics"]);
    let tracks = physics.and_then(|p| p.get_ci("TrackTarget")).and_then(Value::bool).unwrap_or(false);
    // (Some write `LifeTime`; `get_ci` finds either.)
    Ok(ProjectileBlueprint {
        path: normalise(path),
        categories,
        cost_mass: num("Economy", "BuildCostMass"),
        cost_energy: num("Economy", "BuildCostEnergy"),
        build_time: num("Economy", "BuildTime"),
        initial_speed: num("Physics", "InitialSpeed"),
        max_speed: num("Physics", "MaxSpeed"),
        acceleration: num("Physics", "Acceleration"),
        tracks,
        turn_rate: num("Physics", "TurnRate"),
        lifetime: num("Physics", "Lifetime"),
        table,
    })
}

/// The blueprint of the projectile `id` names (a weapon's `ProjectileId`) in the game's files,
/// if it is there and reads.
pub fn read(files: &crate::files::Files, id: &str) -> Result<Option<ProjectileBlueprint>> {
    let path = normalise(id);
    let Some(bytes) = files.read(&path)? else { return Ok(None) };
    match parse(&path, &String::from_utf8_lossy(&bytes)) {
        Ok(p) => Ok(Some(p)),
        Err(e) => {
            log::warn!("{path}: {e}");
            Ok(None)
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A counted missile in the game's own words: a strategic one, built at 12,000 mass and
    /// 1,350,000 energy over 324,000 of its launcher's build power.
    pub(crate) const NUKE: &str = r#"
ProjectileBlueprint {
    Categories = { 'UEF', 'PROJECTILE', 'INDIRECTFIRE', 'STRATEGIC', 'MISSILE', 'NOSPLASHDAMAGE' },
    Defense = { Health = 25, MaxHealth = 25 },
    Economy = { BuildCostEnergy = 1350000, BuildCostMass = 12000, BuildTime = 324000 },
    Physics = { Acceleration = 3, InitialSpeed = 0, Lifetime = 600, MaxSpeed = 30, TrackTarget = true, TurnRate = 0 },
}
"#;

    #[test]
    fn a_missiles_blueprint_reads() {
        let p = parse("/Projectiles/TIFMissileNuke01/TIFMissileNuke01_proj.bp", NUKE).unwrap();
        assert_eq!(p.path, "projectiles/tifmissilenuke01/tifmissilenuke01_proj.bp");
        assert!(p.strategic_missile() && !p.tactical_missile() && p.priced());
        assert_eq!((p.cost_mass, p.cost_energy, p.build_time), (12000.0, 1350000.0, 324000.0));
        assert_eq!((p.initial_speed, p.max_speed, p.acceleration, p.lifetime), (0.0, 30.0, 3.0, 600.0));
        assert!(p.tracks);
        // A shell with no price and no categories of a missile.
        let shell = parse("shell", "ProjectileBlueprint { Categories = { 'PROJECTILE', 'DIRECTFIRE' }, Physics = { InitialSpeed = 25 } }").unwrap();
        assert!(!shell.strategic_missile() && !shell.tactical_missile() && !shell.priced() && !shell.tracks);
        assert!(parse("nothing", "UnitBlueprint { }").is_err());
    }
}
