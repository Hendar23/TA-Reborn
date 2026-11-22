//! Forged Alliance's adjacency: what a structure gives the structures beside it, read as data from
//! the player's own copy (`lua/sim/AdjacencyBuffs.lua`), never kept here. A blueprint names a table
//! of buffs (`Adjacency = 'T1PowerGeneratorAdjacencyBuffs'`); each buff says which structures take
//! it (`EntityCategory = 'STRUCTURE SIZE4'`: their size class, and any other category they must
//! have, `ARTILLERY`) and what it changes in them (`Affects = { EnergyActive = { Add = -0.0625,
//! Mult = 1.0 } }`): a share added to one of the receiver's figures. Forged Alliance sums the shares
//! every neighbour gives a figure (`lua/sim/Buff.lua`'s `BuffCalculate`: the figure times one plus
//! the adds; every adjacency buff's `Mult` is 1).
//!
//! What each change is, in TA Reborn's terms ([`Affect`], the unit file's `[ADJACENCY]`, see
//! docs/MODDING.md): `EnergyActive` and `MassActive` what building costs it a second (construction,
//! an upgrade in place, a silo's missiles: Forged Alliance's builders and silos), `EnergyMaintenance`
//! its upkeep, `EnergyWeapon` what its weapons' shots cost in energy, `RateOfFire` its weapons'
//! reload (`ChangeRateOfFire(rate / (1 + add))`: the reload times one plus the add),
//! `EnergyProduction` and `MassProduction` what it makes.

use std::collections::{BTreeMap, BTreeSet};

use crate::lua::{self, Value};
use crate::Result;

/// Where the game keeps its adjacency buffs.
pub const PATH: &str = "lua/sim/adjacencybuffs.lua";

/// What a buff changes in the structure that takes it, by Forged Alliance's name for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Affect {
    EnergyActive,
    MassActive,
    EnergyMaintenance,
    EnergyWeapon,
    RateOfFire,
    EnergyProduction,
    MassProduction,
}

impl Affect {
    fn of(name: &str) -> Option<Affect> {
        Some(match name {
            "EnergyActive" => Affect::EnergyActive,
            "MassActive" => Affect::MassActive,
            "EnergyMaintenance" => Affect::EnergyMaintenance,
            "EnergyWeapon" => Affect::EnergyWeapon,
            "RateOfFire" => Affect::RateOfFire,
            "EnergyProduction" => Affect::EnergyProduction,
            "MassProduction" => Affect::MassProduction,
            _ => return None,
        })
    }

    /// The name TA Reborn's unit files give it (`Affects=` in an `[ADJACENCY]` entry: what of the
    /// receiver it scales, by TA's own keys where it has one).
    pub fn key(self) -> &'static str {
        match self {
            Affect::EnergyActive => "BuildEnergy",
            Affect::MassActive => "BuildMetal",
            Affect::EnergyMaintenance => "EnergyUse",
            Affect::EnergyWeapon => "EnergyPerShot",
            Affect::RateOfFire => "ReloadTime",
            Affect::EnergyProduction => "EnergyMake",
            Affect::MassProduction => "MetalMake",
        }
    }
}

/// One buff: its name, the structures that take it, and what it changes in them.
#[derive(Clone, Debug, PartialEq)]
pub struct Buff {
    pub name: String,
    /// The size class it is for (`SIZE4` is 4), where its category names one.
    pub size: Option<u32>,
    /// The other categories its receiver must have, every one (not `STRUCTURE`: adjacency is
    /// between structures alone).
    pub categories: Vec<String>,
    pub affect: Affect,
    /// The share it adds to the receiver's figure (a cut below 0).
    pub add: f64,
}

/// The game's adjacency tables, by name (lower case), each its buffs in order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tables {
    pub tables: BTreeMap<String, Vec<Buff>>,
}

impl Tables {
    /// The buffs of the table `name` (none for a table the game lacks).
    pub fn of(&self, name: &str) -> &[Buff] {
        self.tables.get(&name.to_ascii_lowercase()).map_or(&[], Vec::as_slice)
    }

    /// Every category the buffs name besides the size classes (`ARTILLERY`): a structure needs
    /// those of them it has in its unit file's `Category` for those buffs to reach it.
    pub fn categories(&self) -> BTreeSet<String> {
        self.tables.values().flatten().flat_map(|b| b.categories.iter().cloned()).collect()
    }
}

/// The size class a category is (`SIZE4` is 4), if it is one.
pub fn size_of(category: &str) -> Option<u32> {
    let rest = category.get(..4).filter(|p| p.eq_ignore_ascii_case("SIZE")).map(|_| &category[4..])?;
    (!rest.is_empty() && rest.bytes().all(|c| c.is_ascii_digit())).then(|| rest.parse().ok()).flatten()
}

/// Reads the game's adjacency buffs: none where its copy has no such file, or one that does not
/// read (said in the log; the units are made without).
pub fn read(files: &crate::files::Files) -> Result<Tables> {
    match files.read(PATH)? {
        Some(bytes) => Ok(parse(&String::from_utf8_lossy(&bytes)).unwrap_or_else(|e| {
            log::warn!("{PATH}: {e}; the structures give each other nothing");
            Tables::default()
        })),
        None => Ok(Tables::default()),
    }
}

/// Reads the adjacency buffs' file: its tables (`T1PowerGeneratorAdjacencyBuffs = { 'a', 'b' }`)
/// and its buffs (`BuffBlueprint { Name = 'a', ... }`). A buff of a category TA Reborn cannot read
/// (anything but categories every one of which the receiver must have) or changing what it does
/// not play is left out, and said in the log; a table naming a buff the file lacks leaves it out.
pub fn parse(text: &str) -> Result<Tables> {
    let statements = lua::parse(text)?;
    let mut buffs: BTreeMap<String, Vec<Buff>> = BTreeMap::new();
    for (name, value) in &statements {
        if name != "BuffBlueprint" {
            continue;
        }
        let Some(t) = value.table() else { continue };
        let Some(buff_name) = t.get_ci("Name").and_then(Value::str) else { continue };
        let category = t.get_ci("EntityCategory").and_then(Value::str).unwrap_or("");
        if category.contains(['+', '-', '*', '(', ')', ',']) {
            log::warn!("adjacency buff {buff_name}: its category '{category}' is no list of categories; left out");
            continue;
        }
        let mut size = None;
        let mut categories = Vec::new();
        for word in category.split_whitespace() {
            match size_of(word) {
                Some(s) => size = Some(s),
                None if word.eq_ignore_ascii_case("STRUCTURE") => {}
                None => categories.push(word.to_ascii_uppercase()),
            }
        }
        let Some(affects) = t.get_ci("Affects").and_then(Value::table) else { continue };
        let mut list = Vec::new();
        for (affect, change) in &affects.fields {
            let Some(a) = Affect::of(affect) else {
                log::warn!("adjacency buff {buff_name}: {affect} is not played; left out");
                continue;
            };
            let Some(change) = change.table() else { continue };
            let add = change.get_ci("Add").and_then(Value::num).unwrap_or(0.0);
            if change.get_ci("Mult").and_then(Value::num).is_some_and(|m| m != 1.0) {
                log::warn!("adjacency buff {buff_name}: its {affect} multiplies, which is not played; its add alone is");
            }
            if add != 0.0 {
                list.push(Buff { name: buff_name.to_string(), size, categories: categories.clone(), affect: a, add });
            }
        }
        // (A buff of several changes: a name each, after the first.)
        for (k, b) in list.iter_mut().enumerate().skip(1) {
            b.name = format!("{}{}", b.name, k + 1);
        }
        buffs.entry(buff_name.to_string()).or_default().extend(list);
    }
    let mut tables = BTreeMap::new();
    for (name, value) in &statements {
        let Some(t) = value.table().filter(|t| t.fields.is_empty() && !t.array.is_empty()) else { continue };
        if name == "BuffBlueprint" || !t.array.iter().all(|v| matches!(v, Value::Str(_))) {
            continue;
        }
        let list: Vec<Buff> = t.array.iter().filter_map(Value::str).filter_map(|b| buffs.get(b)).flatten().cloned().collect();
        tables.insert(name.to_ascii_lowercase(), list);
    }
    Ok(Tables { tables })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A made-up file in the game's own form (none of its figures or names): a generator's table and
    /// a store's, their buffs for two sizes, a reload for artillery of the smallest, a buff of two
    /// changes, and what TA Reborn does not read or play (a category that is an expression, a figure
    /// it has no such change for, a table naming a buff the file lacks).
    pub(crate) const BUFFS: &str = r#"
# A made-up buff file, in the form of Forged Alliance's.
local Funcs = import('/lua/sim/somefunctions.lua')

GeneratorBuffs = {
    'GenBuildSize4',
    'GenBuildSize16',
    'GenUpkeepSize4',
    'GenReloadSize4',
    'NoSuchBuff',
}

StoreBuffs = {
    'StoreMakeSize4',
}

ExpressionBuffs = { 'Odd', 'Both' }

BuffBlueprint {
    Name = 'GenBuildSize4',
    DisplayName = 'GenBuild',
    BuffType = 'SOMEBUFF',
    Stacks = 'ALWAYS',
    Duration = -1,
    EntityCategory = 'STRUCTURE SIZE4',
    BuffCheckFunction = Funcs.SomeCheck,
    OnBuffAffect = Funcs.SomeAffect,
    Affects = {
        EnergyActive = {
            Add = -0.05,
            Mult = 1.0,
        },
    },
}

BuffBlueprint {
    Name = 'GenBuildSize16',
    EntityCategory = 'STRUCTURE SIZE16',
    BuffCheckFunction = Funcs.SomeCheck,
    Affects = { EnergyActive = { Add = -0.02, Mult = 1.0 } },
}

BuffBlueprint {
    Name = 'GenUpkeepSize4',
    EntityCategory = 'STRUCTURE SIZE4',
    Affects = { EnergyMaintenance = { Add = -0.04, Mult = 1.0 } },
}

BuffBlueprint {
    Name = 'GenReloadSize4',
    EntityCategory = 'STRUCTURE SIZE4 ARTILLERY',
    Affects = { RateOfFire = { Add = -0.03, Mult = 1.0 } },
}

BuffBlueprint {
    Name = 'StoreMakeSize4',
    EntityCategory = 'STRUCTURE SIZE4',
    Affects = { MassProduction = { Add = 0.2, Mult = 1.0 } },
}

BuffBlueprint {
    Name = 'Odd',
    EntityCategory = 'STRUCTURE - WALL',
    Affects = { MassActive = { Add = -0.1, Mult = 1.0 } },
}

BuffBlueprint {
    Name = 'Both',
    EntityCategory = 'STRUCTURE SIZE8',
    Affects = { MassActive = { Add = -0.06, Mult = 1.0 }, Health = { Add = 0.5, Mult = 1.0 }, MassProduction = { Add = 0.07, Mult = 1.0 } },
}

BuffBlueprint {
    Name = 'SomeHealth',
    BuffType = 'SOMEHEALTH',
    Affects = { MaxHealth = { Add = 0, Mult = 1.1 } },
}
"#;

    #[test]
    fn the_games_adjacency_buffs_read_as_data() {
        let t = parse(BUFFS).unwrap();
        let power = t.of("GeneratorBuffs");
        let summary: Vec<(&str, Option<u32>, &str, f64)> = power.iter().map(|b| (b.name.as_str(), b.size, b.affect.key(), b.add)).collect();
        assert_eq!(
            summary,
            [("GenBuildSize4", Some(4), "BuildEnergy", -0.05), ("GenBuildSize16", Some(16), "BuildEnergy", -0.02), ("GenUpkeepSize4", Some(4), "EnergyUse", -0.04), ("GenReloadSize4", Some(4), "ReloadTime", -0.03),],
            "in the table's order, the buff the file lacks left out"
        );
        assert_eq!(power[3].categories, ["ARTILLERY"], "artillery alone, STRUCTURE left out");
        assert_eq!(t.of("storebuffs")[0].affect, Affect::MassProduction, "a table found whatever its case");
        assert!(t.of("NoSuchTable").is_empty());
        // (A category that is an expression is left out; of a buff of several changes, those
        // played, a name each after the first.)
        let both: Vec<(&str, Affect, f64)> = t.of("ExpressionBuffs").iter().map(|b| (b.name.as_str(), b.affect, b.add)).collect();
        assert_eq!(both, [("Both", Affect::MassActive, -0.06), ("Both2", Affect::MassProduction, 0.07)]);
        assert_eq!(t.categories().into_iter().collect::<Vec<_>>(), ["ARTILLERY"]);
        assert_eq!((size_of("SIZE12"), size_of("size4"), size_of("SIZE"), size_of("SIZEX"), size_of("TECH1")), (Some(12), Some(4), None, None, None));
    }
}
