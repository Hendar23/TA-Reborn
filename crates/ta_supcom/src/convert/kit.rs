//! Which of TA's worlds a map becomes ([`Kit`]: its planet, and the features of TA's that stand
//! in for the game's trees, bushes, rocks, deposits and vents), and the features of the map's
//! own: TA's trees, bushes and rocks under names of their own (`SC1_Green_T1`), as passable as
//! the game's forests were and worth what its props were, written beside the map
//! (`maps/<map>.features`). Their names carry [`super::FEATURES_VERSION`], so a map always
//! names the features it was made with.

use std::collections::BTreeMap;

use ta_formats::tdf::{TdfDocument, TdfSection};

use crate::scmap::ScMap;

/// TA's own features (from the original game alone, mods left out): every section by its name
/// in lower case, with the world (its `features/<world>/` folder) it is of.
#[derive(Clone, Default)]
pub struct Templates {
    pub by_name: BTreeMap<String, (String, TdfSection)>,
}

impl Templates {
    pub fn get(&self, name: &str) -> Option<&(String, TdfSection)> {
        self.by_name.get(&name.to_ascii_lowercase())
    }

    pub fn has(&self, name: &str) -> bool {
        self.by_name.contains_key(&name.to_ascii_lowercase())
    }

    /// A feature's footprint (cells across, its larger side).
    pub fn footprint(&self, name: &str) -> u32 {
        self.get(name).map_or(1, |(_, s)| s.get_i64_or("footprintx", 1).max(s.get_i64_or("footprintz", 1)).clamp(1, 16) as u32)
    }

    /// TA's features as the original game has them: `vfs` should hold the game alone (no mods,
    /// nor TA Reborn's own), so that every player's maps are made from the same.
    pub fn load(vfs: &ta_vfs::Vfs) -> Templates {
        let docs = vfs.list("features", Some("tdf"), true).into_iter().filter_map(|p| {
            let world = p.split('/').nth(1).filter(|_| p.matches('/').count() >= 2).unwrap_or("all").to_string();
            vfs.read_opt(&p).map(|b| (world, ta_formats::tdf::parse_bytes(&b)))
        });
        Templates::from_documents(docs)
    }

    /// The templates of `files` (TDF documents, in the order read, each with its world).
    pub fn from_documents(docs: impl IntoIterator<Item = (String, TdfDocument)>) -> Templates {
        let mut by_name = BTreeMap::new();
        for (world, doc) in docs {
            for s in doc.sections {
                by_name.insert(s.name.to_ascii_lowercase(), (world.clone(), s));
            }
        }
        Templates { by_name }
    }
}

/// A world of TA's a map becomes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Kit {
    /// In the map's own features' names (`SC1_Green_T1`).
    pub name: &'static str,
    /// The OTA's `planet`: what the renderer, the sky and the weather go by.
    pub planet: &'static str,
    /// Living trees (the first ones there are are used), bushes, rocks, deposits, vents.
    pub trees: &'static [&'static str],
    pub bushes: &'static [&'static str],
    pub rocks: &'static [&'static str],
    /// The deposit on land and under water (each about 2 metal a second to an extractor, as a
    /// mass point gives in the game).
    pub metal: &'static str,
    pub water_metal: &'static str,
    pub vents: &'static [&'static str],
}

const GREEN_TREES: &[&str] = &["Tree1", "Tree2", "Tree3", "Tree4", "Tree5", "Tree6", "Tree7"];
const GREEN_BUSHES: &[&str] = &["Shrub1", "Shrub2", "Shrub3"];
const GREEN_ROCKS: &[&str] = &["Rock", "Rock1a", "Rock2", "Rock2a", "Rock3", "Rock3a", "Rock4", "Rock4a", "Rock05"];

pub const GREEN: Kit = Kit { name: "Green", planet: "Green planet", trees: GREEN_TREES, bushes: GREEN_BUSHES, rocks: GREEN_ROCKS, metal: "RockMetal3", water_metal: "GreenAquaOre3", vents: &["GreenVent01", "GreenVent02", "GreenVent03"] };
pub const ARCHI: Kit = Kit { name: "Archi", planet: "Archipelago", trees: &["ArchiTree01", "ArchiTree02", "ArchiTree03", "ArchiTree04", "ArchiTree05", "ArchiTree06"], bushes: &["ArchiFoliage01", "ArchiFoliage02", "ArchiFoliage03", "ArchiFoliage04"], rocks: GREEN_ROCKS, metal: "ArchMetal3", water_metal: "ArchMetal3", vents: &["ArchiVent01", "ArchiVent02"] };
pub const LUSH: Kit = Kit { name: "Lush", planet: "Lush", trees: &["LushTree01", "Lush02", "Lush03", "Lush04", "Lush05", "Lush06"], bushes: &["lushfoliage01", "lushfoliage02", "lushfoliage03", "lushfoliage04", "lushfoliage05"], rocks: &["LushRock01", "LushRock02", "LushRock03", "LushRock04", "LushRock05", "LushRock06", "LushRock07"], metal: "LushMetal3", water_metal: "AquaOre3", vents: &["LushVent01", "LushVent02", "LushVent03"] };
pub const DESERT: Kit = Kit { name: "Desert", planet: "Desert", trees: &[], bushes: &[], rocks: &["DryRock01", "DryRock02", "DryRock03", "DryRock04", "DryRock05", "DryRock06", "DryRock07", "DryRock08", "DryRock09"], metal: "DryMetal03", water_metal: "DryMetal03", vents: &["DryVent01", "DryVent02", "DryVent03", "DryVent04"] };
pub const ICE: Kit = Kit { name: "Ice", planet: "Ice", trees: GREEN_TREES, bushes: &[], rocks: &["IceChunk01", "IceChunk02", "IceChunk07", "IceChunk08", "IceChunk09", "IceChunk10", "IceChunk13", "IceChunk14"], metal: "IceMetal03", water_metal: "IceAquaOre3", vents: &["IceVent01", "IceVent02", "IceVent03", "IceVent04"] };
pub const LAVA: Kit = Kit { name: "Lava", planet: "Lava", trees: &["Tree1Dead", "Tree2Dead", "Tree3Dead", "Tree4Dead", "Tree5Dead", "Tree6Dead"], bushes: &[], rocks: &["LavaRockC012", "LavaRockC015", "LavaRockC008", "LavaRockC005", "LavaRockB017", "LavaRockB012", "LavaRockB011", "LavaRockA016"], metal: "LavaMetal3", water_metal: "LavaMetal3", vents: &["Lavavent001", "Lavavent002", "Lavavent003", "Lavavent004"] };
pub const MARS: Kit = Kit { name: "Mars", planet: "Red Planet", trees: &["MarsPlant02", "MarsPlant03", "MarsPlant04", "MarsPlant05", "MarsPlant06"], bushes: &["MarsPlant01", "MarsPlant07"], rocks: &["MarsRock01", "MarsRock02", "MarsRock03", "MarsRock04", "MarsRock05", "MarsRock06", "MarsRock07", "MarsRock08"], metal: "MarsMetal03", water_metal: "MarsMetal03", vents: &["MarsVent01", "MarsVent02", "MarsVent03", "MarsVent04"] };
pub const CRYSTAL: Kit = Kit { name: "Crystal", planet: "Crystal", trees: &[], bushes: &[], rocks: &["SlateRock01", "SlateRock02", "SlateRock03", "SlateRock04", "SlateRock05", "SlateRock06", "SlateRock07", "SlateRock08"], metal: "CrystalMetal3", water_metal: "CrystalAquaOre3", vents: &["CrystalVent01", "CrystalVent02", "CrystalVent03"] };
pub const SLATE: Kit = Kit { name: "Slate", planet: "Slate", trees: &["Tree1Dead", "Tree2Dead", "Tree3Dead"], bushes: &[], rocks: &["SlateRock01", "SlateRock02", "SlateRock03", "SlateRock04", "SlateRock05", "SlateRock06", "SlateRock07", "SlateRock08"], metal: "SlateMetal4", water_metal: "SlateMetal4", vents: &["SlateVent01", "SlateVent02", "SlateVent03"] };

/// The kit of a Supreme Commander environment (`/env/<world>/...`).
fn kit_of_env(env: &str) -> Option<Kit> {
    Some(match env {
        "evergreen" | "evergreen2" => GREEN,
        "tropical" | "paradise" => ARCHI,
        "swamp" => LUSH,
        "desert" => DESERT,
        "tundra" => ICE,
        "lava" => LAVA,
        "redrocks" | "red barrens" | "redbarrens" => MARS,
        "crystalline" => CRYSTAL,
        "geothermal" => SLATE,
        _ => return None,
    })
}

/// The environment a path is of (`/env/Evergreen/Layers/x.dds` is `evergreen`).
fn env_of(path: &str) -> Option<String> {
    let p = path.replace('\\', "/").to_ascii_lowercase();
    let rest = p.trim_start_matches('/').strip_prefix("env/")?;
    Some(rest.split('/').next()?.to_string())
}

/// The world a map is of: the environment its ground's textures and its props come from most
/// (a stratum counting as much as a hundred props), as a kit whose features TA has (else
/// TA's green world's, which every copy of TA has).
pub fn choose(map: &ScMap, templates: &Templates) -> Kit {
    let mut votes: BTreeMap<&'static str, (u64, Kit)> = BTreeMap::new();
    let mut vote = |path: &str, weight: u64| {
        if let Some(kit) = env_of(path).and_then(|e| kit_of_env(&e)) {
            votes.entry(kit.name).or_insert((0, kit)).0 += weight;
        }
    };
    for layer in &map.albedo {
        vote(&layer.path, 100);
    }
    for prop in &map.props {
        vote(&prop.blueprint, 1);
    }
    // The most votes, ties to the name first in order (the same on every computer).
    let best = votes.values().fold(None::<(u64, Kit)>, |best, &(n, k)| match best {
        Some((m, _)) if m >= n => best,
        _ => Some((n, k)),
    });
    let kit = best.map_or(GREEN, |(_, k)| k);
    if templates.has(kit.metal) && kit.vents.iter().any(|v| templates.has(v)) { kit } else { GREEN }
}

/// What a prop of the game becomes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Class {
    Tree,
    /// A group of trees in one prop: so many trees.
    Trees(u32),
    Bush,
    /// A rock: its size (world units, its larger side), the mass it is worth, and whether the
    /// game's units had to go round it (most rocks; a few small ones they drove over).
    Rock { size: f32, mass: f32, blocks: bool },
    /// Nothing TA has (markers, emitters, geysers, the cities' clutter).
    None,
}

/// The shaders the game draws its factions' units with (team-coloured): props drawn with them
/// are the cities' clutter, not the land's.
const UNIT_SHADERS: &[&str] = &["Unit", "Insect", "Aeon", "AeonCZAR", "Seraphim", "Metal"];

/// Whether a prop is a city's or a faction's, not the land's: the factions' props (`/env/uef/`,
/// `/env/aeon/`, `/env/cybran/`, `/env/seraphim/`: buses, cars, docks, fences, street lights,
/// adjacency nodes, crystals), wrecks', or one drawn as units are ([`UNIT_SHADERS`]: the
/// Seraphim's street light in `/env/common/`). The fences and street lights are `Tree`s to the
/// game (to be knocked down), the rest 300 mass each: neither is what they become in TA.
fn city_prop(path: &str, bp: Option<&crate::lua::Table>) -> bool {
    use crate::lua::Value;
    if matches!(env_of(path).as_deref(), Some("uef" | "aeon" | "cybran" | "seraphim" | "wreckage")) {
        return true;
    }
    let Some(lods) = bp.and_then(|t| t.path(&["Display", "Mesh"])).and_then(|m| m.get_ci("LODs")).and_then(Value::table) else { return false };
    lods.array.iter().filter_map(Value::table).filter_map(|l| l.get_ci("ShaderName").and_then(Value::str)).any(|s| UNIT_SHADERS.iter().any(|u| s.eq_ignore_ascii_case(u)))
}

/// What a prop blueprint is, from its path and what the blueprint says (`bp`: its reclaim and
/// size, when it could be read).
pub fn classify(path: &str, bp: Option<&crate::lua::Table>) -> Class {
    use crate::lua::Value;
    let p = path.replace('\\', "/").to_ascii_lowercase();
    if city_prop(&p, bp) {
        return Class::None;
    }
    let num = |keys: &[&str]| -> f32 {
        let mut t = match bp { Some(t) => t, None => return 0.0 };
        for k in &keys[..keys.len() - 1] {
            match t.get_ci(k).and_then(Value::table) {
                Some(n) => t = n,
                None => return 0.0,
            }
        }
        t.get_ci(keys[keys.len() - 1]).and_then(Value::num).unwrap_or(0.0) as f32
    };
    if ["/emitters/", "/geysers/", "/mudpots/", "/markers/", "/structures/", "/devtest/", "bridge"].iter().any(|s| p.contains(s)) {
        return Class::None;
    }
    let energy = num(&["Economy", "ReclaimEnergyMax"]);
    let mass = num(&["Economy", "ReclaimMassMax"]);
    let script = bp.and_then(|t| t.get_ci("ScriptClass")).and_then(Value::str).unwrap_or("").to_ascii_lowercase();
    let blocks = bp.and_then(|t| t.get_ci("Physics")).and_then(Value::table).and_then(|t| t.get_ci("BlockPath")).and_then(Value::bool);
    // The file's own name, which the worlds' props are named by (`boulder05`, `icerocksm04`,
    // `pine06_groupa`): the folders are not the same in every world.
    let file = p.rsplit('/').next().unwrap_or("");
    let named = |words: &[&str]| words.iter().any(|w| file.contains(w));
    if script == "treegroup" || p.contains("/trees/groups/") || (p.contains("/trees/") && file.contains("group")) {
        // (A group is worth about as much as the trees in it, each 5 to 15.)
        return Class::Trees(((energy / 10.0).round() as u32).clamp(2, 6));
    }
    // (A street light's name holds "tree" too.)
    if script == "tree" || p.contains("/trees/") || (named(&["tree", "pine", "palm", "oak", "birch", "cypress", "ficus"]) && !file.contains("street")) {
        return Class::Tree;
    }
    let rock_named = named(&["rock", "boulder", "stone", "iceberg", "crystal", "pod", "lavaberg", "cliff"]);
    if p.contains("/rocks/") || p.contains("/boulders/") || p.contains("/crystals/") || rock_named || (blocks == Some(true) && mass >= 2.0) || (p.contains("/clutter/") && mass >= 5.0) {
        if mass <= 0.0 && !rock_named && !p.contains("/rocks/") {
            return Class::None;
        }
        let size = num(&["SizeX"]).max(num(&["SizeZ"])).max(0.5);
        return Class::Rock { size, mass: mass.max(1.0), blocks: blocks != Some(false) };
    }
    if p.contains("/bush") || p.contains("/logs/") || p.contains("/ferns") || named(&["bush", "fern", "shrub", "log", "cactus", "manzanita", "weed", "grass"]) || (energy > 0.0 && mass <= 0.0 && blocks != Some(true)) {
        return Class::Bush;
    }
    Class::None
}

/// The map's own features: what each of its names (`SC1_Green_T1`) is.
#[derive(Default)]
pub struct Own {
    defs: BTreeMap<String, TdfSection>,
}

/// What a rock is worth, in tiers (TA's rocks are 25 to 100; the game's 2.5 to 300).
const ROCK_TIERS: [(f32, &str, u32); 4] = [(10.0, "S", 5), (40.0, "M", 20), (120.0, "L", 60), (f32::MAX, "X", 150)];

/// Whether a feature of TA's blocks nothing and is worth nothing (rubble, a scar, a smudge): a
/// feature of the map's own may leave it as it is.
fn harmless(d: &TdfSection) -> bool {
    !d.get_bool_or("blocking", true) && d.get_f64_or("metal", 0.0) <= 0.0 && d.get_f64_or("energy", 0.0) <= 0.0 && !d.get_bool_or("geothermal", false)
}

/// What a feature of the map's own is worth and what it blocks: what its remains may not exceed.
struct Caps {
    metal: f64,
    energy: f64,
    blocking: bool,
    footprint: (i64, i64),
}

impl Caps {
    fn of(s: &TdfSection) -> Caps {
        Caps {
            metal: s.get_f64_or("metal", 0.0).max(0.0),
            energy: s.get_f64_or("energy", 0.0).max(0.0),
            blocking: s.get_bool_or("blocking", true),
            footprint: (s.get_i64_or("footprintx", 1).max(1), s.get_i64_or("footprintz", 1).max(1)),
        }
    }

    /// Holds remains of TA's (`d`) to them: worth no more, blocking no more (nor more cells).
    fn hold(&self, d: &mut TdfSection) {
        let number = |v: f64| if v.fract() == 0.0 { format!("{}", v as i64) } else { format!("{v}") };
        let metal = d.get_f64_or("metal", 0.0).min(self.metal).max(0.0);
        let energy = d.get_f64_or("energy", 0.0).min(self.energy).max(0.0);
        let blocking = self.blocking && d.get_bool_or("blocking", true);
        d.set("metal", number(metal));
        d.set("energy", number(energy));
        d.set("blocking", if blocking { "1" } else { "0" });
        if blocking {
            let (fx, fz) = (d.get_i64_or("footprintx", 1).max(1), d.get_i64_or("footprintz", 1).max(1));
            d.set("footprintx", fx.min(self.footprint.0).to_string());
            d.set("footprintz", fz.min(self.footprint.1).to_string());
        }
    }
}

impl Own {
    /// The name of the map's own feature standing for a prop of `class` (`variant` choosing
    /// among the kit's trees, bushes or rocks), defined from TA's template; `None` when the kit
    /// has nothing for it. `fp` is the cells a rock's footprint should cover.
    pub fn name(&mut self, kit: &Kit, templates: &Templates, class: Class, variant: usize, fp: u32) -> Option<String> {
        let available = |list: &[&'static str]| -> Vec<&'static str> { list.iter().copied().filter(|n| templates.has(n)).collect() };
        let v = super::FEATURES_VERSION;
        match class {
            Class::Tree | Class::Trees(_) => {
                let trees = available(kit.trees);
                let t = *trees.get(variant % trees.len().max(1))?;
                let name = format!("SC{v}_{}_T{}", kit.name, trees.iter().position(|&n| n == t).unwrap_or(0) + 1);
                self.define(&name, templates, t, &[("blocking", "0"), ("flamable", "0"), ("reclaimable", "1"), ("energy", "10"), ("metal", "1")]);
                Some(name)
            }
            Class::Bush => {
                let bushes = available(kit.bushes);
                let b = *bushes.get(variant % bushes.len().max(1))?;
                let name = format!("SC{v}_{}_B{}", kit.name, bushes.iter().position(|&n| n == b).unwrap_or(0) + 1);
                self.define(&name, templates, b, &[("blocking", "0"), ("flamable", "0"), ("reclaimable", "1"), ("energy", "5"), ("metal", "0")]);
                Some(name)
            }
            Class::Rock { mass, blocks, .. } => {
                let rocks = available(kit.rocks);
                if rocks.is_empty() {
                    return None;
                }
                // The rocks of the footprint nearest `fp`, then the variant among them.
                let best = rocks.iter().map(|r| templates.footprint(r).abs_diff(fp)).min()?;
                let fitting: Vec<&str> = rocks.iter().copied().filter(|r| templates.footprint(r).abs_diff(fp) == best).collect();
                let r = fitting[variant % fitting.len()];
                let &(_, tier, metal) = ROCK_TIERS.iter().find(|(top, _, _)| mass <= *top)?;
                let name = format!("SC{v}_{}_R{}{tier}{}", kit.name, rocks.iter().position(|&n| n == r).unwrap_or(0) + 1, if blocks { "" } else { "P" });
                self.define(&name, templates, r, &[("blocking", if blocks { "1" } else { "0" }), ("reclaimable", "1"), ("metal", &metal.to_string()), ("energy", "0")]);
                Some(name)
            }
            Class::None => None,
        }
    }

    fn define(&mut self, name: &str, templates: &Templates, template: &str, set: &[(&str, &str)]) {
        if self.defs.contains_key(name) {
            return;
        }
        let Some((world, t)) = templates.get(template) else { return };
        let mut s = TdfSection::new(name);
        for (k, v) in &t.entries {
            s.set(k, v.clone());
        }
        for (k, v) in set {
            s.set(k, *v);
        }
        s.set("rebornworld", world.clone());
        self.remains(s, templates);
    }

    /// Defines feature `s` (of the map's own), and what it leaves: when destroyed (its
    /// `featuredead`), TA's remains of the template it was made from, but never worth more than
    /// it is nor blocking more than it did (TA's rocks crack into rocks as big and as rich as they
    /// were, a 75-metal rock for a 5-metal pebble, and its Martian rocks into larger ones). Each
    /// step of TA's that would is taken over under a name of the map's own (`<name>_D1`, `_D2`...),
    /// its metal, energy and blocking no more than this feature's, nor its footprint where it
    /// blocks; the first that blocks nothing and is worth nothing (rubble, a scar, a smudge) is
    /// TA's own, by its name; past what cannot be told, nothing. What reclaiming it leaves, and
    /// what it burns into, are TA's where they too block nothing and are worth nothing, else
    /// nothing.
    fn remains(&mut self, mut s: TdfSection, templates: &Templates) {
        // (TA's rocks crack twice before they crumble; none goes further.)
        const STEPS: usize = 4;
        let caps = Caps::of(&s);
        let name = s.name.clone();
        let mut next = s.get("featuredead").unwrap_or("").trim().to_string();
        let mut chain: Vec<TdfSection> = Vec::new();
        let mut last = String::new();
        for step in 1..=STEPS {
            let Some((world, d)) = (!next.is_empty()).then(|| templates.get(&next)).flatten() else { break };
            if harmless(d) {
                last = d.name.clone();
                break;
            }
            let mut copy = TdfSection::new(format!("{name}_D{step}"));
            for (k, v) in &d.entries {
                copy.set(k, v.clone());
            }
            caps.hold(&mut copy);
            copy.set("rebornworld", world.clone());
            next = d.get("featuredead").unwrap_or("").trim().to_string();
            chain.push(copy);
        }
        // Each leaves the next; the last, TA's harmless remains or nothing.
        let names: Vec<String> = chain.iter().map(|c| c.name.clone()).chain(std::iter::once(last)).collect();
        for (section, dead) in std::iter::once(&mut s).chain(chain.iter_mut()).zip(&names) {
            section.set("featuredead", dead.clone());
            for key in ["featurereclamate", "featureburnt"] {
                let left = section.get(key).unwrap_or("").trim().to_string();
                let kept = templates.get(&left).filter(|(_, d)| harmless(d)).map_or(String::new(), |_| left);
                section.set(key, kept);
            }
        }
        for section in std::iter::once(s).chain(chain) {
            self.defs.insert(section.name.clone(), section);
        }
    }

    /// The footprint (cells across, its larger side) of the map's own feature `name` (1 if there is
    /// none such).
    pub fn footprint(&self, name: &str) -> i32 {
        self.defs.get(name).map_or(1, |s| s.get_i64_or("footprintx", 1).max(s.get_i64_or("footprintz", 1)).clamp(1, 16) as i32)
    }

    /// The `maps/<map>.features` file: the definitions, in name order.
    pub fn document(&self) -> TdfDocument {
        TdfDocument { sections: self.defs.values().cloned().collect() }
    }

    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// TA's templates the tests use: a few of green's and Mars', with what they leave as TA's
    /// leave it (a rock cracks into rocks as big and as rich, then crumbles into rubble; a tree
    /// leaves a smudge, and burns into a dead tree that blocks).
    pub(crate) fn templates() -> Templates {
        let green = "[Tree1]{world=greenworld;description=Tree;category=trees;footprintx=1;footprintz=1;height=40;filename=trees;seqname=leaf1;energy=250;blocking=1;flamable=1;featuredead=smudge01;featurereclamate=smudge01;featureburnt=Tree1Dead;}\
            [Tree2]{world=greenworld;description=Tree;category=trees;footprintx=1;footprintz=1;height=40;filename=trees;seqname=leaf2;energy=250;blocking=1;}\
            [Tree1Dead]{description=Dead Tree;category=trees;footprintx=1;footprintz=1;energy=50;blocking=1;featuredead=smudge03;}\
            [Smudge01]{description=Smudge;category=smudges;footprintx=1;footprintz=1;blocking=0;featuredead=Smudge01;}\
            [Shrub1]{description=Shrub;category=shrubs;footprintx=1;footprintz=1;energy=20;}\
            [Rock2]{description=Rock;category=rocks;footprintx=3;footprintz=3;metal=100;blocking=1;}\
            [Rock4]{description=Rock;category=rocks;footprintx=2;footprintz=2;metal=75;blocking=1;featuredead=rock4a;featurereclamate=rock04recscar;}\
            [Rock4a]{description=Rock;category=rocks;footprintx=2;footprintz=2;metal=75;blocking=1;filename=rockshurt;featuredead=rock4b;featurereclamate=rock04recscar;}\
            [Rock4b]{description=Rock;category=rocks;footprintx=2;footprintz=2;metal=75;blocking=1;filename=rockshurt2;featuredead=rockgone04;}\
            [Rockgone04]{description=Rubble;category=rocks;footprintx=3;footprintz=3;blocking=0;}\
            [Rock04RecScar]{description=Scar;category=rocks;footprintx=2;footprintz=2;blocking=0;}\
            [RockMetal3]{description=Metal;category=rocks;footprintx=3;footprintz=3;metal=223;indestructible=1;}\
            [GreenVent01]{description=Thermal Vent;category=steamvents;footprintx=1;footprintz=1;geothermal=1;}\
            [DragonsTeeth]{description=Dragon's Teeth;category=dragonteeth;footprintx=2;footprintz=2;metal=8;blocking=1;}";
        let mars = "[MarsRock01]{description=Rock;category=rocks;footprintx=3;footprintz=2;metal=50;blocking=1;featuredead=marsrock03hit;}\
            [MarsRock03Hit]{description=Rock;category=rocks;footprintx=3;footprintz=3;metal=25;blocking=1;featuredead=marsrock03gone;}\
            [MarsRock03Gone]{description=Rubble;category=rocks;footprintx=3;footprintz=3;blocking=0;}";
        Templates::from_documents([("green".to_string(), ta_formats::tdf::parse(green)), ("mars".to_string(), ta_formats::tdf::parse(mars))])
    }

    /// A destroyed rock of the map's own leaves TA's cracked rocks under names of its own, worth
    /// no more than it was and blocking no more (cells and all), then TA's rubble; a passable
    /// rock's remains are passable; a tree leaves TA's smudge, and burns into nothing that blocks.
    #[test]
    fn what_features_leave_is_never_worth_more_nor_blocks_more() {
        let t = templates();
        let mut own = Own::default();
        let pebble = own.name(&GREEN, &t, Class::Rock { size: 0.5, mass: 5.0, blocks: true }, 0, 2).unwrap();
        // (Martian rocks crack into larger ones.)
        let mars = Kit { rocks: &["MarsRock01"], ..MARS };
        let rock = own.name(&mars, &t, Class::Rock { size: 3.0, mass: 30.0, blocks: true }, 0, 3).unwrap();
        let small = own.name(&mars, &t, Class::Rock { size: 0.5, mass: 1.0, blocks: false }, 0, 1).unwrap();
        let tree = own.name(&GREEN, &t, Class::Tree, 0, 1).unwrap();
        let doc = own.document();
        let get = |n: &str, k: &str| doc.section(n).and_then(|s| s.get(k)).map(str::to_string);
        assert_eq!(get(&pebble, "metal").as_deref(), Some("5"));
        let d1 = get(&pebble, "featuredead").unwrap();
        assert_eq!(d1, format!("{pebble}_D1"));
        assert_eq!((get(&d1, "metal").as_deref(), get(&d1, "blocking").as_deref(), get(&d1, "filename").as_deref()), (Some("5"), Some("1"), Some("rockshurt")));
        let d2 = get(&d1, "featuredead").unwrap();
        assert_eq!((get(&d2, "metal").as_deref(), get(&d2, "filename").as_deref()), (Some("5"), Some("rockshurt2")));
        assert_eq!(get(&d2, "featuredead").as_deref(), Some("Rockgone04"), "TA's rubble, which blocks nothing and is worth nothing");
        assert_eq!(get(&pebble, "featurereclamate").as_deref(), Some("rock04recscar"));
        // Martian rocks crack into larger ones: held to the rock's own cells.
        let hit = get(&rock, "featuredead").unwrap();
        assert_eq!((get(&hit, "footprintx").as_deref(), get(&hit, "footprintz").as_deref(), get(&hit, "metal").as_deref()), (Some("3"), Some("2"), Some("20")));
        assert_eq!(get(&hit, "featuredead").as_deref(), Some("MarsRock03Gone"));
        // A rock the game's units drove over leaves nothing that blocks.
        assert_eq!(get(&get(&small, "featuredead").unwrap(), "blocking").as_deref(), Some("0"));
        // Trees: TA's smudge when destroyed; they burn into nothing (TA's dead tree blocks).
        assert_eq!((get(&tree, "featuredead").as_deref(), get(&tree, "featureburnt").as_deref()), (Some("Smudge01"), Some("")));
        // Every feature of the map's own, and every remains, by these rules.
        for s in &doc.sections {
            let next = s.get("featuredead").unwrap_or("");
            let Some(n) = doc.section(next) else {
                assert!(next.is_empty() || t.get(next).is_some_and(|(_, d)| harmless(d)), "{}: {next}", s.name);
                continue;
            };
            assert!(n.get_f64_or("metal", 0.0) <= s.get_f64_or("metal", 0.0) && n.get_f64_or("energy", 0.0) <= s.get_f64_or("energy", 0.0), "{} -> {}", s.name, n.name);
            assert!(!n.get_bool_or("blocking", true) || (s.get_bool_or("blocking", true) && n.get_i64_or("footprintx", 1) <= s.get_i64_or("footprintx", 1) && n.get_i64_or("footprintz", 1) <= s.get_i64_or("footprintz", 1)), "{} -> {}", s.name, n.name);
        }
    }

    /// The cities' props are left out (the fences and street lights the game calls trees, the
    /// buses, cars, docks and adjacency nodes worth 300 mass), whatever they are worth and
    /// however they are scripted; the land's own props are what they were.
    #[test]
    fn the_cities_props_are_left_out() {
        let bp = |text: &str| crate::lua::top(&crate::lua::parse(text).unwrap(), "PropBlueprint").and_then(crate::lua::Value::table).cloned();
        let fence = bp("PropBlueprint { Display = { Mesh = { LODs = { { ShaderName = 'Unit' } } } }, Economy = { ReclaimEnergyMax = 5, ReclaimMassMax = 0.5 }, Physics = { BlockPath = false }, ScriptClass = 'Tree', SizeX = 1.5 }");
        let bus = bp("PropBlueprint { Display = { Mesh = { LODs = { { ShaderName = 'Unit' } } } }, Economy = { ReclaimMassMax = 300 }, Physics = { BlockPath = true }, SizeX = .5, SizeZ = 1 }");
        assert_eq!(classify("/env/UEF/Props/UEF_Fence_prop.bp", fence.as_ref()), Class::None);
        assert_eq!(classify("/env/uef/props/uef_streetlight_01_prop.bp", fence.as_ref()), Class::None);
        assert_eq!(classify("/env/uef/props/uef_bus_prop.bp", bus.as_ref()), Class::None);
        assert_eq!(classify("/env/cybran/props/cybranadjacencynodethree_prop.bp", bus.as_ref()), Class::None);
        assert_eq!(classify("/env/aeon/props/aeon_fence_prop.bp", None), Class::None);
        assert_eq!(classify("/env/seraphim/props/seraphim_crystal_prop.bp", None), Class::None);
        // Drawn as units are, wherever it is kept.
        assert_eq!(classify("/env/common/props/seraphim_streetlight_prop.bp", bus.as_ref()), Class::None);
        // (A street light's name holds "tree" too.)
        assert_eq!(classify("/env/evergreen/props/streetlight_prop.bp", None), Class::None);
        // The land's props, as before.
        let rock = bp("PropBlueprint { Display = { Mesh = { LODs = { { ShaderName = 'NormalMappedTerrain' } } } }, Economy = { ReclaimMassMax = 10 }, Physics = { BlockPath = true }, SizeX = 1 }");
        assert_eq!(classify("/env/evergreen/props/rocks/rock01_prop.bp", rock.as_ref()), Class::Rock { size: 1.0, mass: 10.0, blocks: true });
        assert_eq!(classify("/env/evergreen/props/trees/pine06_prop.bp", None), Class::Tree);
    }

    #[test]
    fn a_maps_world_is_where_its_ground_and_props_come_from() {
        let t = templates();
        let mut m = ScMap { albedo: vec![crate::scmap::Layer { path: "/env/Evergreen/layers/eg_gravel01_albedo.dds".into(), scale: 4.0 }], ..Default::default() };
        assert_eq!(choose(&m, &t), GREEN);
        // A desert's ground, but the desert's features are not in this copy of TA: green.
        m.albedo = vec![crate::scmap::Layer { path: "/env/desert/layers/des_sand_albedo.dds".into(), scale: 4.0 }];
        assert_eq!(choose(&m, &t), GREEN);
        assert_eq!(env_of("/ENV/Red Barrens/Layers/x.dds").as_deref(), Some("red barrens"));
        assert_eq!(env_of("/textures/x.dds"), None);
    }

    #[test]
    fn props_become_passable_trees_and_rocks_worth_what_the_game_made_them() {
        let t = templates();
        assert_eq!(classify("/env/evergreen/props/trees/pine06_s2_prop.bp", None), Class::Tree);
        let bp = crate::lua::parse("PropBlueprint { Economy = { ReclaimEnergyMax = 40, ReclaimMassMax = 4 }, SizeX = 2.5, SizeZ = 1 }").unwrap();
        let bp = crate::lua::top(&bp, "PropBlueprint").and_then(crate::lua::Value::table).cloned();
        assert_eq!(classify("/env/Evergreen/Props/Trees/Groups/Pine06_GroupA_prop.bp", bp.as_ref()), Class::Trees(4));
        assert_eq!(classify("/env/evergreen/props/rocks/rock01_prop.bp", bp.as_ref()), Class::Rock { size: 2.5, mass: 4.0, blocks: true });
        // Red rocks' small rocks, which the game's units drove over.
        let small = crate::lua::parse("PropBlueprint { Economy = { ReclaimMassMax = 1 }, Physics = { BlockPath = false }, SizeX = 1 }").unwrap();
        let small = crate::lua::top(&small, "PropBlueprint").and_then(crate::lua::Value::table).cloned();
        assert_eq!(classify("/env/redrocks/props/rock_sm02_prop.bp", small.as_ref()), Class::Rock { size: 1.0, mass: 1.0, blocks: false });
        assert_eq!(classify("/env/common/props/markers/m_mass_prop.bp", None), Class::None);
        let mut own = Own::default();
        assert_eq!(own.name(&GREEN, &t, Class::Tree, 1, 1).as_deref(), Some("SC1_Green_T2"));
        assert_eq!(own.name(&GREEN, &t, Class::Rock { size: 2.0, mass: 30.0, blocks: true }, 0, 2).as_deref(), Some("SC1_Green_R2M"));
        assert_eq!(own.name(&GREEN, &t, Class::Rock { size: 2.0, mass: 30.0, blocks: false }, 0, 2).as_deref(), Some("SC1_Green_R2MP"));
        assert_eq!(own.document().section("SC1_Green_R2MP").and_then(|s| s.get("blocking")), Some("0"));
        let doc = own.document();
        let tree = doc.section("SC1_Green_T2").unwrap();
        assert_eq!((tree.get("blocking"), tree.get("energy"), tree.get("seqname"), tree.get("rebornworld")), (Some("0"), Some("10"), Some("leaf2"), Some("green")));
        let rock = doc.section("SC1_Green_R2M").unwrap();
        assert_eq!((rock.get("footprintx"), rock.get("metal")), (Some("2"), Some("20")));
        // Desert's trees: none (its sands have no trees of TA's).
        assert_eq!(own.name(&DESERT, &t, Class::Tree, 0, 1), None);
    }
}
