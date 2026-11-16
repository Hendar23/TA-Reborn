//! A unit's file (`units/<id>.fbi`) and its weapons' (`weapons/supcom_<faction>.tdf`), made from
//! its blueprint at TA's scale (docs/SUPCOM_UNITS.md): 32 pixels an ogrid, 30 ticks a second, its
//! health, costs, build time, speeds, sight and weapons Forged Alliance's own.
//!
//! - **Build time:** Forged Alliance builds a unit in `BuildTime / BuildRate` seconds, as TA does
//!   in `BuildTime / WorkerTime`; both are taken ten times ([`WORK`]), so its builders' work
//!   stands beside TA's (a commander's 100 against TA's 300) and every unit takes as long to
//!   build as in Forged Alliance.
//! - **Weapons:** up to eight slots ([`Weapons`], [`MAX_WEAPONS`]: TA's three and the five more TA
//!   Reborn gives mods): a commander's main gun first and its Overcharge second, made TA's D-gun
//!   (`commandfire`); otherwise the most damaging first, an anti-air one kept where there is one.
//!   Weapons an enhancement brings, and dummies, are left out; the death weapon is what the unit
//!   explodes as. Each looks as TA's weapon of its kind does (a
//!   laser, a shell, a missile, a torpedo, a bomb) and hits as Forged Alliance's.

use std::fmt::Write as _;

use super::UNIT_SCALE;
use crate::unit::{Faction, Motion, Tech, UnitBlueprint, WeaponBlueprint};

/// Forged Alliance's build rates and build times, taken ten times.
pub const WORK: f64 = 10.0;

/// TA's side name for a faction.
pub fn side(faction: &Faction) -> &'static str {
    match faction {
        Faction::Uef => "UEF",
        Faction::Cybran => "CYBRAN",
        Faction::Aeon => "AEON",
        Faction::Seraphim => "SERAPHIM",
        Faction::Other(_) => "SUPCOM",
    }
}

/// The most weapons a unit carries in TA Reborn (`Weapon1` to `Weapon8`: ta_data's `MAX_WEAPONS`).
pub const MAX_WEAPONS: usize = 8;

/// The weapons in its slots, and the one it dies with.
pub struct Weapons<'a> {
    pub slots: Vec<&'a WeaponBlueprint>,
    pub death: Option<&'a WeaponBlueprint>,
    /// The slot of the commander's Overcharge (TA's D-gun), if any.
    pub dgun: Option<usize>,
}

/// What a weapon does, by its category and its projectile's name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Laser,
    Shell,
    Artillery,
    Missile,
    Torpedo,
    Bomb,
    AntiAir,
    Overcharge,
    Nuke,
    AntiMissile,
}

pub fn kind(w: &WeaponBlueprint) -> Kind {
    let proj = w.projectile.as_deref().unwrap_or("").to_ascii_lowercase();
    let cat = w.category.to_ascii_lowercase();
    if w.damage_type.eq_ignore_ascii_case("overcharge") || w.label.eq_ignore_ascii_case("overcharge") {
        return Kind::Overcharge;
    }
    if w.nuke.is_some() {
        return Kind::Nuke;
    }
    if cat == "defense" || proj.contains("antimissile") || proj.contains("antinuke") || proj.contains("intercept") {
        return Kind::AntiMissile;
    }
    if cat == "bomb" || proj.contains("bomb") {
        return Kind::Bomb;
    }
    if cat == "anti navy" || proj.contains("torpedo") || proj.contains("depthcharge") {
        return Kind::Torpedo;
    }
    let air_only = !w.target_layers.is_empty() && w.target_layers.iter().all(|(_, to)| to.split('|').all(|l| l.trim().eq_ignore_ascii_case("air")));
    if cat == "anti air" || air_only {
        return Kind::AntiAir;
    }
    if cat == "missile" || proj.contains("missile") || proj.contains("rocket") {
        return Kind::Missile;
    }
    if cat == "artillery" || w.ballistic_arc.eq_ignore_ascii_case("RULEUBA_HighArc") {
        return Kind::Artillery;
    }
    if w.beam_lifetime > 0.0 || w.continuous_beam || proj.contains("laser") || proj.contains("beam") || proj.contains("disintegrator") || proj.contains("sonic") {
        return Kind::Laser;
    }
    Kind::Shell
}

/// Damage a second, for choosing which weapons take the slots.
fn power(w: &WeaponBlueprint) -> f64 {
    w.damage.max(0.0) * w.salvo_size.max(1) as f64 * w.rate_of_fire.max(0.01) + w.nuke.as_ref().map_or(0.0, |n| n.inner_damage)
}

/// The weapons that take its slots (eight at most), and the death weapon.
pub fn weapons(u: &UnitBlueprint) -> Weapons<'_> {
    let usable = |w: &&WeaponBlueprint| {
        !w.category.eq_ignore_ascii_case("death")
            && w.table.get_ci("EnabledByEnhancement").is_none()
            && !w.label.to_ascii_lowercase().contains("dummy")
            && !w.label.to_ascii_lowercase().contains("antitorpedo")
            && !w.projectile.as_deref().unwrap_or("").to_ascii_lowercase().contains("antitorpedo")
            && (w.damage > 0.0 || w.nuke.is_some() || kind(w) == Kind::AntiMissile)
    };
    let death = u.weapons.iter().find(|w| w.category.eq_ignore_ascii_case("death") || w.label.eq_ignore_ascii_case("DeathWeapon"));
    let mut armed: Vec<&WeaponBlueprint> = u.weapons.iter().filter(usable).collect();
    let mut slots: Vec<&WeaponBlueprint> = Vec::new();
    let mut dgun = None;
    if u.is("COMMAND") || u.is("SUBCOMMANDER") {
        // The main gun, then the Overcharge.
        if let Some(i) = armed.iter().position(|w| kind(w) != Kind::Overcharge) {
            slots.push(armed.remove(i));
        }
        if let Some(i) = armed.iter().position(|w| kind(w) == Kind::Overcharge) {
            dgun = Some(slots.len());
            slots.push(armed.remove(i));
        }
    }
    // The rest by their damage (the first among equals first), an anti-air one kept.
    armed.sort_by(|a, b| power(b).total_cmp(&power(a)));
    let anti_air = armed.iter().position(|w| kind(w) == Kind::AntiAir);
    while slots.len() < MAX_WEAPONS && !armed.is_empty() {
        if slots.len() == MAX_WEAPONS - 1
            && anti_air.is_some()
            && !slots.iter().any(|w| kind(w) == Kind::AntiAir)
            && let Some(i) = armed.iter().position(|w| kind(w) == Kind::AntiAir)
        {
            slots.push(armed.remove(i));
            continue;
        }
        slots.push(armed.remove(0));
    }
    Weapons { slots, death, dgun }
}

/// A weapon's section name: `<id>_<label>`, letters and digits; a label the unit's weapons give
/// more than once numbered after its first (an Aeon carrier's two `AntiAirMissiles01`).
pub fn weapon_name(u: &UnitBlueprint, w: &WeaponBlueprint) -> String {
    let clean = |w: &WeaponBlueprint| -> String { w.label.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_uppercase() };
    let label = clean(w);
    let label = if label.is_empty() { "WEAPON".to_string() } else { label };
    let before = u.weapons.iter().take_while(|other| !std::ptr::eq(*other, w)).filter(|other| clean(other) == clean(w)).count();
    let name = format!("{}_{label}", u.id).to_ascii_uppercase();
    if before == 0 { name } else { format!("{name}_{}", before + 1) }
}

/// TA's explosion for a blast `diameter` pixels across.
fn explosion(diameter: f64) -> &'static str {
    match diameter {
        d if d < 24.0 => "explode5",
        d if d < 64.0 => "explode3",
        d if d < 160.0 => "explode2",
        _ => "explode4",
    }
}

/// A weapon's section: its range, reload, speed, blast and damage Forged Alliance's at TA's
/// scale; how it flies and looks as TA's weapons of its kind.
/// What an air transport carries, by its mesh's attach bones (`…Attachpoint…`: Forged Alliance
/// sets its passengers there, small units on the plain or `_sml`/`_Small` ones, larger on `_Med`
/// and `_Lrg` ones; a side's own named `Left_`, `Right_`):
/// as many units as it has small slots (or larger, where it has none), as large as its largest
/// slots take (TA's `TransportSize`: footprints up to twice it). None without attach bones.
pub fn transport(mesh: &crate::scm::Mesh) -> Option<(u32, u32)> {
    let (mut small, mut medium, mut large) = (0u32, 0u32, 0u32);
    for b in &mesh.bones {
        let name = b.name.to_ascii_lowercase();
        if !name.contains("attachpoint") {
            continue;
        }
        if name.contains("lrg") {
            large += 1;
        } else if name.contains("med") {
            medium += 1;
        } else {
            small += 1;
        }
    }
    let units = if small > 0 { small } else { medium + large };
    (units > 0).then_some((units, if large > 0 { 3 } else if medium > 0 { 2 } else { 1 }))
}

/// A nuke's range, pixels: the whole of any map.
const NUKE_RANGE: i64 = 32000;

/// The speed a strategic missile flies at, at least: TA's nukes' (`NUCLEAR_MISSILE`'s 350 pixels a
/// second). Forged Alliance's leave their silo at a crawl (5 ogrids a second) and speed up on their
/// way, which TA's flight model, a missile at its weapon's velocity, cannot do: at the crawl, one
/// took a minute to cross a 20 km map.
const NUKE_SPEED: i64 = 350;

/// How long a missile flies before it falls, seconds (TA's `flighttime`, its projectile's life):
/// long enough to cover `range` pixels at its `speed` half again (it turns as it tracks), with
/// its climb to speed from `start` at `acceleration` and a second to launch.
pub fn flight_time(range: i64, speed: i64, start: i64, acceleration: i64) -> i64 {
    let speed = speed.max(1) as f64;
    let climb = (speed - start as f64).max(0.0) / acceleration.max(1) as f64;
    (range as f64 / speed * 1.5 + climb + 1.0).ceil().max(3.0) as i64
}

pub fn weapon_section(u: &UnitBlueprint, w: &WeaponBlueprint) -> String {
    let k = kind(w);
    let px = |ogrids: f64| (ogrids * UNIT_SCALE).round() as i64;
    let mut s = String::new();
    let mut put = |key: &str, value: String| {
        let _ = writeln!(s, "\t{key}={value};");
    };
    put("name", if w.display_name.is_empty() { w.label.clone() } else { w.display_name.replace(';', ",") });
    put("range", if k == Kind::Nuke { NUKE_RANGE } else { px(w.max_radius.max(1.0)) }.to_string());
    if w.min_radius > 0.0 {
        put("minbarrelangle", "-15".into());
    }
    put("reloadtime", format!("{:.3}", 1.0 / w.rate_of_fire.max(0.01)));
    let speed = (w.muzzle_velocity.max(5.0) * UNIT_SCALE).round() as i64;
    let speed = if k == Kind::Nuke { speed.max(NUKE_SPEED) } else { speed };
    put("weaponvelocity", speed.to_string());
    let blast = match &w.nuke {
        Some(n) => n.outer_radius * 2.0 * UNIT_SCALE,
        None => (w.damage_radius * 2.0 * UNIT_SCALE).max(8.0),
    };
    put("areaofeffect", (blast.round() as i64).to_string());
    put("edgeeffectiveness", if w.nuke.is_some() { "0.25".into() } else { "0.5".into() });
    if w.salvo_size > 1 {
        put("burst", w.salvo_size.to_string());
        put("burstrate", format!("{:.3}", w.salvo_delay.max(0.05)));
    }
    // Firing tolerance, degrees to TA's angle units.
    put("tolerance", ((w.firing_tolerance.max(1.0) * 65536.0 / 360.0).round() as i64).max(500).to_string());
    if w.turreted {
        put("turret", "1".into());
    }
    if w.energy_required > 0.0 {
        put("energypershot", (w.energy_required.round() as i64).to_string());
    }
    let (render, model, start, hit) = match k {
        Kind::Laser => (0, "", "lasrfir1", "lasrhit1"),
        Kind::Shell => (4, "", "cannon1", "xplomed2"),
        Kind::Artillery => (4, "", "cannhvy1", "xplomed2"),
        Kind::Missile => (1, "missile", "rocklit3", "xplomed2"),
        Kind::Torpedo => (1, "torpedo", "torpedo1", "xplodep1"),
        Kind::Bomb => (6, "bomb", "bombrel", "xplomed2"),
        Kind::AntiAir => (4, "", "cannon1", "xplosml2"),
        Kind::Overcharge => (3, "dgun", "disigun1", "xplomas2"),
        Kind::Nuke => (1, "crblmssl", "misicbm1", "xplomas2"),
        Kind::AntiMissile => (1, "amdrocket", "Rockhvy1", "xplomed4"),
    };
    put("rendertype", render.to_string());
    if !model.is_empty() {
        put("model", model.into());
    }
    match k {
        Kind::Laser => {
            put("lineofsight", "1".into());
            put("beamweapon", "1".into());
            put("duration", format!("{:.2}", w.beam_lifetime.clamp(0.05, 1.0)));
            put("color", "180".into());
            put("color2", "181".into());
        }
        Kind::Shell => {
            let arc = !w.ballistic_arc.eq_ignore_ascii_case("RULEUBA_None");
            put("ballistic", if arc { "1" } else { "0" }.into());
            put("lineofsight", if arc { "0" } else { "1" }.into());
        }
        Kind::Artillery => put("ballistic", "1".into()),
        Kind::Missile | Kind::AntiMissile | Kind::Nuke => {
            let (start, acceleration) = ((speed / 3).max(60), (speed / 2).max(60));
            put("lineofsight", "1".into());
            put("selfprop", "1".into());
            put("smoketrail", "1".into());
            put("startvelocity", start.to_string());
            put("weaponacceleration", acceleration.to_string());
            put("turnrate", "16000".into());
            put("tracks", "1".into());
            put("guidance", "1".into());
            put("flighttime", flight_time(if k == Kind::Nuke { NUKE_RANGE } else { px(w.max_radius.max(1.0)) }, speed, start, acceleration).to_string());
            if matches!(k, Kind::Nuke | Kind::AntiMissile) || w.manual_fire {
                put("vlaunch", "1".into());
            }
            if k == Kind::Nuke {
                put("stockpile", "1".into());
                put("targetable", "1".into());
                put("commandfire", "1".into());
                put("noautorange", "1".into());
            }
            if k == Kind::AntiMissile {
                put("interceptor", "1".into());
                put("coverage", px(w.max_radius.max(1.0)).to_string());
                put("noautorange", "1".into());
            } else if w.manual_fire {
                put("commandfire", "1".into());
            }
        }
        Kind::Torpedo => {
            put("waterweapon", "1".into());
            put("lineofsight", "1".into());
            put("selfprop", "1".into());
            put("tracks", "1".into());
            put("turnrate", "12000".into());
        }
        Kind::Bomb => put("dropped", "1".into()),
        Kind::AntiAir => {
            put("toairweapon", "1".into());
            put("lineofsight", "1".into());
        }
        Kind::Overcharge => {
            put("commandfire", "1".into());
            put("lineofsight", "1".into());
            put("noexplode", "1".into());
        }
    }
    put("soundstart", start.into());
    put("soundhit", hit.into());
    put("explosiongaf", "fx".into());
    put("explosionart", explosion(blast).into());
    put("waterexplosiongaf", "fx".into());
    put("waterexplosionart", "h2oboom1".into());
    let damage = match &w.nuke {
        Some(n) => n.inner_damage,
        None => w.damage,
    };
    format!("[{}]\n\t{{\n{s}\t[DAMAGE]\n\t\t{{\n\t\tdefault={};\n\t\t}}\n\t}}\n", weapon_name(u, w), damage.round().max(1.0) as i64)
}

/// A unit's death: its death weapon's section (a blast where it falls), or none.
pub fn death_section(u: &UnitBlueprint, w: &WeaponBlueprint) -> Option<String> {
    let (damage, radius) = match &w.nuke {
        Some(n) => (n.inner_damage, n.outer_radius),
        None => (w.damage, w.damage_radius),
    };
    if damage <= 0.0 {
        return None;
    }
    let blast = (radius * 2.0 * UNIT_SCALE).max(16.0);
    Some(format!(
        "[{}_DEATH]\n\t{{\n\tname={};\n\trendertype=4;\n\tballistic=1;\n\tturret=1;\n\trange=480;\n\treloadtime=3.45;\n\tweaponvelocity=250;\n\tareaofeffect={};\n\tedgeeffectiveness=0.5;\n\tsoundhit=xplolrg3;\n\texplosiongaf=fx;\n\texplosionart={};\n\t[DAMAGE]\n\t\t{{\n\t\tdefault={};\n\t\t}}\n\t}}\n",
        u.id.to_ascii_uppercase(),
        if w.display_name.is_empty() { "Death".to_string() } else { w.display_name.replace(';', ",") },
        blast.round() as i64,
        explosion(blast),
        damage.round() as i64
    ))
}

/// How it builds: none, standing in place (a factory) or moving (an engineer, a commander).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Builds {
    No,
    Factory,
    Mobile,
}

pub fn builds(u: &UnitBlueprint) -> Builds {
    if u.build_rate <= 0.0 || build_lists(u).is_empty() {
        return Builds::No;
    }
    // (A building that is no factory builds only its own upgrade in Forged Alliance, on itself:
    // not yet in TA Reborn.)
    match u.motion {
        Motion::None if u.is("FACTORY") => Builds::Factory,
        Motion::None => Builds::No,
        _ => Builds::Mobile,
    }
}

/// What a unit's build lists name that it builds from the start: a commander's own tier (its
/// engineering enhancements, which bring the others, are not yet in TA Reborn), not upgrades of
/// itself.
pub fn build_lists(u: &UnitBlueprint) -> Vec<&str> {
    u.buildable
        .iter()
        .map(String::as_str)
        .filter(|e| {
            let upper = e.to_ascii_uppercase();
            !upper.contains("TIER2COMMANDER") && !upper.contains("TIER3COMMANDER") && !upper.ends_with("UPGRADE") && e.split_whitespace().count() > 1
        })
        .collect()
}

/// TA's footprint cells (16 pixels) of the unit's footprint (or its size where it gives none).
pub fn footprint(u: &UnitBlueprint) -> (i64, i64) {
    let [x, z] = u.footprint.unwrap_or([u.size[0].max(0.5), u.size[2].max(0.5)]);
    let cells = |ogrids: f64| ((ogrids * UNIT_SCALE / 16.0).round() as i64).clamp(1, 64);
    // (An extractor covers a metal spot as TA's does, three cells a side: what it gives is the
    // metal under it.)
    if u.is("MASSEXTRACTION") {
        return (cells(x).max(3), cells(z).max(3));
    }
    // (A unit that moves takes a square, as TA's do: it turns where it stands.)
    if u.motion != Motion::None {
        let side = cells(x).max(cells(z));
        return (side, side);
    }
    (cells(x), cells(z))
}

/// A factory's yard: a corridor of cells open while it works, from the middle to its front edge
/// (TA's buildings face +z, the yard map's last rows), the rest solid.
pub fn yard_map(fx: i64, fz: i64) -> String {
    let half = (fx / 4).max(1);
    let (from, to) = (fx / 2 - half, fx / 2 + half);
    let mut s = String::new();
    for z in 0..fz {
        for x in 0..fx {
            s.push(if x >= from && x < to && z >= fz / 2 - 1 { 'c' } else { 'o' });
        }
    }
    s
}

/// The unit's file.
pub fn unit_file(u: &UnitBlueprint, weapons: &Weapons, carries: Option<(u32, u32)>) -> String {
    let id = u.id.to_ascii_uppercase();
    let (fx, fz) = footprint(u);
    let side = side(&u.faction);
    let builds = builds(u);
    let walks = u.display.walk.is_some();
    let mut f = String::new();
    let mut put = |k: &str, v: String| {
        let _ = writeln!(f, "\t{k}={v};");
    };
    put("UnitName", id.clone());
    put("Version", "1".into());
    put("Side", side.into());
    put("Objectname", u.id.to_ascii_lowercase());
    // (The commanders have no name of their own: their description.)
    let named = if !u.name.is_empty() { &u.name } else if !u.description.is_empty() { &u.description } else { &id };
    put("Name", named.replace(';', ","));
    put("Description", u.description.replace(';', ","));
    put("FootprintX", fx.to_string());
    put("FootprintZ", fz.to_string());
    put("MaxDamage", (u.health.round().max(1.0) as i64).to_string());
    put("BuildCostMetal", (u.cost_mass.round() as i64).to_string());
    put("BuildCostEnergy", (u.cost_energy.round() as i64).to_string());
    put("BuildTime", ((u.build_time * WORK).round().max(1.0) as i64).to_string());
    put("SightDistance", ((u.intel.vision.max(u.intel.water_vision) * UNIT_SCALE).round() as i64).to_string());
    if u.intel.radar > 0.0 {
        put("RadarDistance", ((u.intel.radar * UNIT_SCALE).round() as i64).to_string());
    }
    if u.intel.sonar > 0.0 {
        put("SonarDistance", ((u.intel.sonar * UNIT_SCALE).round() as i64).to_string());
    }
    if u.intel.jam > 0.0 {
        put("RadarDistanceJam", ((u.intel.jam * UNIT_SCALE).round() as i64).to_string());
    }
    if u.intel.radar_stealth {
        put("Stealth", "1".into());
    }
    // Economy.
    if u.production_energy > 0.0 {
        put("EnergyMake", format!("{}", u.production_energy));
    }
    if u.upkeep_energy > 0.0 {
        put("EnergyUse", format!("{}", u.upkeep_energy));
    }
    if u.storage_energy > 0.0 {
        put("EnergyStorage", (u.storage_energy.round() as i64).to_string());
    }
    if u.storage_mass > 0.0 {
        put("MetalStorage", (u.storage_mass.round() as i64).to_string());
    }
    if u.is("MASSEXTRACTION") {
        // TA's extractor gives about 2 metal a second on its spots: Forged Alliance's T1 gives 2,
        // its T2 6, its T3 18.
        let rate = match u.tech {
            Some(Tech::T2) => 0.003,
            Some(Tech::T3) => 0.009,
            _ => 0.001,
        };
        put("ExtractsMetal", format!("{rate}"));
        put("MetalStorage", "50".into());
        // (As TA's: working once built, its upkeep paid while it works.)
        put("ActivateWhenBuilt", "1".into());
        put("onoffable", "1".into());
    } else if u.production_mass > 0.0 {
        if u.upkeep_energy > 0.0 {
            // A fabricator: energy made mass while it works.
            put("MakesMetal", format!("{}", u.production_mass));
            put("OnOffable", "1".into());
            put("ActivateWhenBuilt", "1".into());
        } else {
            put("MetalMake", format!("{}", u.production_mass));
        }
    } else if u.upkeep_energy > 0.0 {
        // Kept working at an energy cost (radar, sonar, jammers, stealth, shields): as TA's, on
        // once built and switchable, the upkeep paid while on.
        put("ActivateWhenBuilt", "1".into());
        put("OnOffable", "1".into());
    }
    // Building.
    if builds != Builds::No {
        put("Builder", "1".into());
        put("WorkerTime", ((u.build_rate * WORK).round().max(1.0) as i64).to_string());
        let reach = u.table.path(&["Economy"]).and_then(|e| e.get_ci("MaxBuildDistance")).and_then(|v| v.num()).unwrap_or(5.0);
        put("BuildDistance", ((reach * UNIT_SCALE).round() as i64).max(64).to_string());
        put("CanReclamate", "1".into());
        if builds == Builds::Factory {
            put("YardMap", yard_map(fx, fz));
        }
    }
    let commander = u.is("COMMAND");
    if commander {
        put("Commander", "1".into());
        put("ShowPlayerName", "1".into());
        put("cancapture", "1".into());
    }
    // Weapons.
    for (slot, w) in weapons.slots.iter().enumerate() {
        put(&format!("Weapon{}", slot + 1), weapon_name(u, w));
    }
    if weapons.dgun.is_some() {
        put("canDgun", "1".into());
    }
    if weapons.slots.iter().any(|w| kind(w) == Kind::Nuke) {
        put("onoffable", "0".into());
    }
    match weapons.death.and_then(|w| death_section(u, w).map(|_| w)) {
        Some(_) => {
            put("ExplodeAs", format!("{id}_DEATH"));
            put("SelfDestructAs", format!("{id}_DEATH"));
        }
        None => {
            let big = fx * fz >= 16;
            put("ExplodeAs", if big { "BIG_UNITEX" } else { "SMALL_UNITEX" }.into());
            put("SelfDestructAs", if big { "BIG_UNIT" } else { "SMALL_UNIT" }.into());
        }
    }
    put("Corpse", format!("{}_dead", u.id.to_ascii_lowercase()));
    // Kinds, for targeting, the computer players and the menus.
    let level = match u.tech {
        Some(Tech::T2) => "LEVEL2",
        Some(Tech::T3) | Some(Tech::Experimental) => "LEVEL3",
        _ => "LEVEL1",
    };
    let air = u.motion == Motion::Air;
    let mut cats: Vec<String> = vec![side.into(), level.into(), if air { "VTOL" } else { "NOTAIR" }.into()];
    if u.motion != Motion::Sub {
        cats.push("NOTSUB".into());
    }
    if !weapons.slots.is_empty() {
        cats.push("WEAPON".into());
    }
    if commander {
        cats.push("COMMANDER".into());
    }
    if u.motion == Motion::None {
        cats.push("NOWEAPON".into());
        cats.retain(|c| c != "NOWEAPON" || weapons.slots.is_empty());
    }
    put("Category", cats.join(" "));
    let ted = match (u.motion, builds) {
        _ if commander => "COMMANDER",
        (Motion::None, Builds::Factory) => "PLANT",
        (Motion::None, _) if u.is("MASSEXTRACTION") || u.is("MASSFABRICATION") => "METAL",
        (Motion::None, _) if u.is("ENERGYPRODUCTION") || u.is("ENERGYSTORAGE") => "ENERGY",
        (Motion::None, _) if !weapons.slots.is_empty() => "FORT",
        (Motion::None, _) => "SPECIAL",
        (Motion::Air, _) => "VTOL",
        (Motion::Water, _) => "SHIP",
        (Motion::Sub, _) => "WATER",
        (_, Builds::Mobile) => "CNSTR",
        _ if walks => "KBOT",
        _ => "TANK",
    };
    put("TEDClass", ted.into());
    put("SoundCategory", sound_category(u, ted).into());
    put("ThreeD", "1".into());
    put("ShootMe", "1".into());
    if !weapons.slots.is_empty() {
        put("canattack", "1".into());
        put("firestandorders", "1".into());
        put("StandingFireOrder", "2".into());
    }
    // Movement.
    if u.motion != Motion::None {
        let speed = if air { u.max_airspeed.unwrap_or(u.max_speed) } else { u.max_speed };
        put("MaxVelocity", format!("{:.3}", speed * UNIT_SCALE / 30.0));
        put("Acceleration", format!("{:.4}", (u.acceleration * UNIT_SCALE / 900.0).max(0.01)));
        put("BrakeRate", format!("{:.4}", (u.brake.max(u.acceleration) * UNIT_SCALE / 900.0).max(0.01)));
        put("TurnRate", ((u.turn_rate.max(30.0) * 65536.0 / 360.0 / 30.0).round() as i64).to_string());
        put("BMcode", "1".into());
        for k in ["canmove", "canpatrol", "canguard", "canstop", "mobilestandorders"] {
            put(k, "1".into());
        }
        put("StandingMoveOrder", "1".into());
        put("maneuverleashlength", "640".into());
        put("DefaultMissionType", "Standby".into());
        if walks {
            put("Upright", "1".into());
        }
        if let Some(class) = movement_class(u, fx) {
            put("MovementClass", class.into());
        }
        match u.motion {
            Motion::Hover => put("canhover", "1".into()),
            Motion::Water => {
                put("Floater", "1".into());
                put("MinWaterDepth", "6".into());
            }
            Motion::Sub => {
                put("MinWaterDepth", "20".into());
                put("WaterLine", "20".into());
                put("Upright", "1".into());
            }
            Motion::Amphibious => put("MaxWaterDepth", "255".into()),
            Motion::Air => {
                put("canfly", "1".into());
                put("cruisealt", ((u.elevation.unwrap_or(18.0) * 8.0).clamp(60.0, 250.0).round() as i64).to_string());
                if u.is("TRANSPORTATION")
                    && let Some((units, size)) = carries
                {
                    put("canload", "1".into());
                    put("TransportCapacity", units.to_string());
                    put("TransportMaxUnits", units.to_string());
                    put("TransportSize", size.to_string());
                }
                if weapons.slots.iter().any(|w| w.turreted) || u.is("GROUNDATTACK") {
                    put("HoverAttack", "1".into());
                }
            }
            _ => {}
        }
    } else {
        put("BMcode", "0".into());
        put("MaxSlope", "15".into());
        // On the water (floating), under it (on the sea bed, or below the surface: extractors and
        // power plants on deposits there too), or on land.
        let under = u.layers.seabed || u.layers.sub;
        put("MaxWaterDepth", if u.layers.water || under { "255" } else { "0" }.into());
        if !u.layers.land {
            if u.layers.water {
                put("MinWaterDepth", "5".into());
                put("Floater", "1".into());
            } else if under {
                put("MinWaterDepth", "15".into());
            }
        }
    }
    format!("[UNITINFO]\n\t{{\n{f}\t}}\n")
}

/// TA's sound category for the unit's kind (TA's own sounds, `gamedata/sound.tdf`'s: Forged
/// Alliance's are not read); the Cybran's and the Seraphim's the Core's.
fn sound_category(u: &UnitBlueprint, ted: &str) -> &'static str {
    let core = matches!(u.faction, Faction::Cybran | Faction::Seraphim);
    let pick = |arm: &'static str, cor: &'static str| if core { cor } else { arm };
    match ted {
        "COMMANDER" => pick("ARM_COM", "CORE_COM"),
        "PLANT" if u.is("AIR") => "AIRPLANT",
        "PLANT" if u.is("NAVAL") => "SHIPYARD",
        "PLANT" => "TANKPLANT",
        "VTOL" => pick("ARM_VTOL", "CORE_VTOL"),
        "SHIP" => pick("ARM_SHIP", "CORE_SHIP"),
        "WATER" => pick("ARM_SUB", "CORE_SUB"),
        "KBOT" => pick("ARM_KBOT", "COR_KBOT"),
        "TANK" => pick("ARM_TANK", "CORE_TANK"),
        "CNSTR" => pick("ARM_CVEHICLE", "CORE_CVEHICLE"),
        "METAL" => pick("ARM_MEX", "COR_MEX"),
        "ENERGY" => pick("ARM_SOLAR", "CORE_SOLAR"),
        "FORT" => "LLT",
        "SPECIAL" => pick("ARM_RADAR", "CORE_RADAR"),
        _ => pick("ARM_ESTOR", "CORE_ESTOR"),
    }
}

/// TA's movement class for the unit, by how it moves and its footprint.
pub fn movement_class(u: &UnitBlueprint, cells: i64) -> Option<&'static str> {
    Some(match u.motion {
        Motion::None | Motion::Air => return None,
        Motion::Water => {
            if cells <= 4 {
                "BOATS4"
            } else {
                "BOATS6"
            }
        }
        Motion::Sub => "BOATD3",
        Motion::Hover => {
            if cells <= 3 {
                "TANKHOVER3"
            } else {
                "TANKHOVER4"
            }
        }
        Motion::Amphibious => {
            if cells <= 2 {
                "TANKDS2"
            } else {
                "TANKDH3"
            }
        }
        Motion::Land => match (u.display.walk.is_some(), cells) {
            (true, ..=2) => "KBOTSS2",
            (false, ..=2) => "TANKSH2",
            _ => "TANKSH3",
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unit::{self, Rack};

    fn striker() -> UnitBlueprint {
        unit::parse(
            "UEL0201",
            "UnitBlueprint { Categories = { 'UEF', 'MOBILE', 'LAND', 'TECH1', 'DIRECTFIRE', 'BUILTBYTIER1FACTORY' }, \
             Defense = { Health = 300, MaxHealth = 300 }, \
             Economy = { BuildCostEnergy = 266, BuildCostMass = 56, BuildTime = 280 }, \
             General = { FactionName = 'UEF', UnitName = 'MA12 Striker', TechLevel = 'RULEUTL_Basic' }, \
             Intel = { VisionRadius = 20 }, \
             Physics = { MotionType = 'RULEUMT_Land', MaxSpeed = 3.4, MaxAcceleration = 3.4, MaxBrake = 3.4, TurnRate = 90 }, \
             Footprint = { SizeX = 1, SizeZ = 1 }, \
             Weapon = { { Label = 'MainGun', WeaponCategory = 'Direct Fire', DisplayName = 'Gauss Cannon', Damage = 24, MaxRadius = 18, RateOfFire = 1, \
               MuzzleVelocity = 25, BallisticArc = 'RULEUBA_LowArc', ProjectileId = '/projectiles/TDFGauss01/TDFGauss01_proj.bp', \
               Turreted = true, TurretBoneYaw = 'Turret', TurretBonePitch = 'Turret_Barrel', TurretBoneMuzzle = 'Turret_Muzzle', \
               RackBones = { { RackBone = 'Turret_Barrel', MuzzleBones = { 'Turret_Muzzle' } } }, \
               FireTargetLayerCapsTable = { Land = 'Land|Water|Seabed' } } } }",
        )
        .unwrap()
    }

    /// The Striker's file: FA's health, costs, build time (ten times), speed at TA's scale, its
    /// gun's section; its weapon a shell, ballistic, its range 32 pixels an ogrid.
    #[test]
    fn a_tanks_file_and_its_gun() {
        let u = striker();
        let w = weapons(&u);
        assert_eq!(w.slots.len(), 1);
        let file = unit_file(&u, &w, None);
        for line in ["UnitName=UEL0201;", "Side=UEF;", "MaxDamage=300;", "BuildCostMetal=56;", "BuildCostEnergy=266;", "BuildTime=2800;", "FootprintX=2;", "SightDistance=640;", "Weapon1=UEL0201_MAINGUN;", "MovementClass=TANKSH2;", "TEDClass=TANK;", "TurnRate=546;", "Corpse=uel0201_dead;"] {
            assert!(file.contains(line), "{line} in {file}");
        }
        assert!(file.contains("MaxVelocity=3.627;"), "{file}");
        let gun = weapon_section(&u, w.slots[0]);
        for line in ["[UEL0201_MAINGUN]", "range=576;", "reloadtime=1.000;", "weaponvelocity=800;", "rendertype=4;", "ballistic=1;", "default=24;"] {
            assert!(gun.contains(line), "{line} in {gun}");
        }
        // Read back as TA Reborn reads it.
        let doc = ta_formats::tdf::parse(&gun);
        assert_eq!(doc.sections.len(), 1);
    }

    /// A commander: its main gun first, its Overcharge second as TA's D-gun; an enhancement's
    /// missile and its death weapon not in its slots.
    #[test]
    fn a_commanders_overcharge_is_its_dgun() {
        let gun = |label: &str, category: &str, damage: f64| WeaponBlueprint { label: label.into(), category: category.into(), damage, rate_of_fire: 1.0, max_radius: 20.0, ..Default::default() };
        let mut u = UnitBlueprint { id: "UEL0001".into(), faction: Faction::Uef, categories: vec!["COMMAND".into()], ..Default::default() };
        let mut tac = gun("TacMissile", "Missile", 6000.0);
        tac.table = crate::lua::parse("t = { EnabledByEnhancement = 'TacticalMissile' }").unwrap()[0].1.table().cloned().unwrap();
        let mut oc = gun("OverCharge", "Direct Fire", 12000.0);
        oc.damage_type = "Overcharge".into();
        u.weapons = vec![gun("RightZephyr", "Direct Fire", 100.0), oc, tac, gun("DeathWeapon", "Death", 0.0)];
        let w = weapons(&u);
        assert_eq!(w.slots.iter().map(|w| w.label.as_str()).collect::<Vec<_>>(), ["RightZephyr", "OverCharge"]);
        assert_eq!(w.dgun, Some(1));
        let file = unit_file(&u, &w, None);
        assert!(file.contains("canDgun=1;") && file.contains("Commander=1;") && file.contains("TEDClass=COMMANDER;"), "{file}");
        assert!(weapon_section(&u, w.slots[1]).contains("commandfire=1;"));
        let _ = Rack::default();
    }

    /// An air transport's slots, by its attach bones however they are named: small ones (plain,
    /// `_sml`, `_Small`, on either side) counted, sized by its largest; larger ones counted where
    /// there are no small; none at all, none.
    #[test]
    fn a_transport_carries_what_its_attach_bones_hold() {
        let still = [1.0, 0.0, 0.0, 0.0];
        let mesh = |bones: &[&str]| {
            let mut all = vec![("Hull", -1, [0.0f32; 3], still)];
            all.extend(bones.iter().map(|b| (*b, 0, [0.0f32; 3], still)));
            crate::scm::parse(&crate::scm::tests::write(&all, &[([0.0; 3], [0.0; 2], 0)], &[[0, 0, 0]])).unwrap()
        };
        assert_eq!(transport(&mesh(&["Left_Attachpoint_sml_01", "Right_Attachpoint_sml_01", "Attachpoint_Small_03", "Attachpoint_Med_01", "Attachpoint_Lrg_01"])), Some((3, 3)));
        assert_eq!(transport(&mesh(&["Attachpoint01", "Attachpoint02"])), Some((2, 1)));
        assert_eq!(transport(&mesh(&["Attachpoint_Med_01"])), Some((1, 2)), "a gunship's one slot");
        assert_eq!(transport(&mesh(&["Turret", "Muzzle"])), None);
    }

    /// A missile lives long enough to reach as far as its range, a nuke across the map.
    #[test]
    fn missiles_fly_as_far_as_their_range() {
        // 1,920 pixels at 640 a second from 213: 3 s, half again, 0.7 s to speed, a second.
        assert_eq!(flight_time(1920, 640, 213, 320), 7);
        assert!(flight_time(NUKE_RANGE, 1600, 533, 800) as f64 * 1600.0 >= NUKE_RANGE as f64);
        assert_eq!(flight_time(10, 640, 213, 320), 3, "never less than three seconds");
        let mut missile = WeaponBlueprint { label: "Tac".into(), category: "Missile".into(), damage: 100.0, rate_of_fire: 0.1, max_radius: 120.0, muzzle_velocity: 10.0, ..Default::default() };
        missile.projectile = Some("/projectiles/TIFMissileCruise01/TIFMissileCruise01_proj.bp".into());
        let section = weapon_section(&striker(), &missile);
        let life: f64 = section.lines().find_map(|l| l.trim().strip_prefix("flighttime=")).and_then(|v| v.trim_end_matches(';').parse().ok()).expect(&section);
        assert!(life * 320.0 >= 120.0 * 32.0, "{section}");
        // A strategic missile reaches across the map at TA's nukes' speed at least, its range given
        // once.
        let nuke = WeaponBlueprint {
            label: "NukeMissiles".into(),
            category: "Missile".into(),
            damage: 1.0,
            rate_of_fire: 0.1,
            max_radius: 20000.0,
            muzzle_velocity: 5.0,
            nuke: Some(crate::unit::Nuke { inner_damage: 70000.0, inner_radius: 30.0, outer_damage: 500.0, outer_radius: 40.0 }),
            ..Default::default()
        };
        let section = weapon_section(&striker(), &nuke);
        let value = |key: &str| section.lines().filter_map(|l| l.trim().strip_prefix(key)).map(|v| v.trim_end_matches(';').to_string()).collect::<Vec<_>>();
        assert_eq!(value("range="), [NUKE_RANGE.to_string()], "{section}");
        assert_eq!(value("weaponvelocity="), [NUKE_SPEED.to_string()], "{section}");
        let life: i64 = value("flighttime=")[0].parse().unwrap();
        assert!(life * NUKE_SPEED >= NUKE_RANGE && life < 200, "{life} s: {section}");
    }

    /// The fields TA Reborn reads: builders reclaim (`CanReclamate`), a radar is on once built
    /// and switchable (its upkeep paid while on), an air transport carries what its attach bones
    /// hold, and structures stand on the water, under it or on land as their layers allow.
    #[test]
    fn builders_reclaim_sensors_switch_transports_carry_and_structures_stand_where_they_may() {
        use crate::unit::Layers;
        let none = Weapons { slots: Vec::new(), death: None, dgun: None };
        let engineer = UnitBlueprint { id: "UEL0105".into(), faction: Faction::Uef, motion: Motion::Land, build_rate: 5.0, buildable: vec!["BUILTBYTIER1ENGINEER UEF".into()], categories: vec!["ENGINEER".into()], ..Default::default() };
        assert!(unit_file(&engineer, &none, None).contains("CanReclamate=1;"));
        let radar = UnitBlueprint { id: "UEB3101".into(), faction: Faction::Uef, categories: vec!["STRUCTURE".into(), "RADAR".into()], upkeep_energy: 20.0, layers: Layers { land: true, ..Default::default() }, ..Default::default() };
        let file = unit_file(&radar, &none, None);
        assert!(file.contains("EnergyUse=20;") && file.contains("ActivateWhenBuilt=1;") && file.contains("OnOffable=1;") && file.contains("MaxWaterDepth=0;"), "{file}");
        let transport = UnitBlueprint { id: "UEA0107".into(), faction: Faction::Uef, motion: Motion::Air, categories: vec!["AIR".into(), "TRANSPORTATION".into()], ..Default::default() };
        let file = unit_file(&transport, &none, Some((6, 3)));
        assert!(file.contains("TransportCapacity=6;") && file.contains("TransportMaxUnits=6;") && file.contains("TransportSize=3;") && file.contains("canload=1;"), "{file}");
        assert!(!unit_file(&transport, &none, None).contains("canload"), "no attach bones, no transport");
        // An extractor on land or the sea bed; a torpedo trap on the sea bed alone; a sonar afloat.
        let structure = |id: &str, layers: Layers| UnitBlueprint { id: id.into(), faction: Faction::Uef, categories: vec!["STRUCTURE".into()], layers, ..Default::default() };
        let file = unit_file(&structure("UEB1103", Layers { land: true, seabed: true, ..Default::default() }), &none, None);
        assert!(file.contains("MaxWaterDepth=255;") && !file.contains("MinWaterDepth"), "{file}");
        let file = unit_file(&structure("XRB2308", Layers { sub: true, ..Default::default() }), &none, None);
        assert!(file.contains("MaxWaterDepth=255;") && file.contains("MinWaterDepth=15;") && !file.contains("Floater"), "{file}");
        let file = unit_file(&structure("UEB3102", Layers { water: true, ..Default::default() }), &none, None);
        assert!(file.contains("MinWaterDepth=5;") && file.contains("Floater=1;"), "{file}");
    }

    #[test]
    fn a_factorys_yard_opens_a_corridor_to_its_front() {
        // Six by six: the back two rows solid, the middle two columns open from there to the front.
        assert_eq!(yard_map(6, 6), format!("{}{}", "o".repeat(12), "ooccoo".repeat(4)));
        // Two by two: all of it the yard.
        assert_eq!(yard_map(2, 2), "cccc");
    }

}
