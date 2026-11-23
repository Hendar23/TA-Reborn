//! What a building gives the buildings beside it (`[ADJACENCY]` within its `[UNITINFO]`), and its
//! size class (`AdjacencySize`): Supreme Commander's adjacency, which the Supreme Commander
//! factions' structures carry (power generators cutting what the factories beside them pay to
//! build, storage raising what the extractors beside it make). TA's own units have none.
//!
//! ```text
//! [UNITINFO]
//! {
//!     UnitName=MYPLANT;
//!     AdjacencySize=4;
//!     ...
//!     [ADJACENCY]
//!     {
//!         [CheaperBuilds]
//!         {
//!         Affects=BuildEnergy;
//!         Add=-0.05;
//!         Size=16;
//!         }
//!         [QuickerGuns]
//!         {
//!         Affects=ReloadTime;
//!         Add=-0.1;
//!         Size=4;
//!         Category=ARTILLERY;
//!         }
//!     }
//! }
//! ```
//!
//! Each entry is a bonus: what it changes in a building beside this one (`Affects`, a key of
//! [`AdjacencyAffect`]), the share it adds to it (`Add`: -0.0625 cuts it by a sixteenth, 0.125
//! raises it by an eighth), and which buildings take it: those of the size class `Size` (none: any),
//! whose `Category` holds every token the entry's `Category` names (none: any). A building takes
//! the bonuses of every neighbour of its player touching it, summed, and from at most as many
//! neighbours as its `AdjacencySize` (none: as many as touch it).

use ta_formats::tdf::TdfSection;

/// Most bonuses a unit's file may give (the rest are left out, with a warning).
pub const MAX_ADJACENCY: usize = 64;

/// What a bonus changes in the building that takes it: one of its figures, times one plus the
/// shares its neighbours add, never below none.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AdjacencyAffect {
    /// The energy its building costs it (a construction's, an upgrade's in place, a stockpiled
    /// missile's): its builds take as long, for less.
    BuildEnergy,
    /// The metal its building costs it, as [`AdjacencyAffect::BuildEnergy`].
    BuildMetal,
    /// Its upkeep (`EnergyUse`, while it is on).
    EnergyUse,
    /// The energy its weapons' shots cost (`energypershot`, not a stockpiled missile's).
    EnergyPerShot,
    /// Its weapons' reload (`reloadtime`): below one, it fires faster.
    ReloadTime,
    /// The energy it makes (`EnergyMake`, the wind's and the tide's share, a solar collector's).
    EnergyMake,
    /// The metal it makes (`MetalMake`, what it extracts, what it makes of energy).
    MetalMake,
}

impl AdjacencyAffect {
    pub const ALL: [AdjacencyAffect; 7] = [
        AdjacencyAffect::BuildEnergy,
        AdjacencyAffect::BuildMetal,
        AdjacencyAffect::EnergyUse,
        AdjacencyAffect::EnergyPerShot,
        AdjacencyAffect::ReloadTime,
        AdjacencyAffect::EnergyMake,
        AdjacencyAffect::MetalMake,
    ];

    /// The affect a unit file's `Affects` names (any case).
    pub fn of(name: &str) -> Option<AdjacencyAffect> {
        Self::ALL.into_iter().find(|a| a.key().eq_ignore_ascii_case(name.trim()))
    }

    /// Its name in a unit file.
    pub fn key(self) -> &'static str {
        match self {
            AdjacencyAffect::BuildEnergy => "BuildEnergy",
            AdjacencyAffect::BuildMetal => "BuildMetal",
            AdjacencyAffect::EnergyUse => "EnergyUse",
            AdjacencyAffect::EnergyPerShot => "EnergyPerShot",
            AdjacencyAffect::ReloadTime => "ReloadTime",
            AdjacencyAffect::EnergyMake => "EnergyMake",
            AdjacencyAffect::MetalMake => "MetalMake",
        }
    }

    /// Its place in [`AdjacencyAffect::ALL`].
    pub fn index(self) -> usize {
        self as usize
    }
}

/// One bonus a building gives the buildings beside it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AdjacencyDef {
    /// Its section's name (the Supreme Commander factions': the buff it was made of).
    pub name: String,
    /// What it changes: none where its `Affects` names nothing TA Reborn plays (the mod check says
    /// so), as written in [`AdjacencyDef::affects`].
    pub affect: Option<AdjacencyAffect>,
    pub affects: String,
    /// The share it adds to the figure (`Add`).
    pub add: f64,
    /// The size class of the buildings that take it (`Size`; 0: any).
    pub size: u32,
    /// The `Category` tokens a building that takes it must have, every one (`Category`; empty:
    /// any).
    pub category: String,
}

impl AdjacencyDef {
    /// Reads one bonus's section.
    pub fn from_tdf(s: &TdfSection) -> AdjacencyDef {
        let affects = s.get("Affects").unwrap_or("").trim().to_string();
        AdjacencyDef {
            name: s.name.trim().to_string(),
            affect: AdjacencyAffect::of(&affects),
            affects,
            add: s.get_f64_or("Add", 0.0),
            size: s.get_i64_or("Size", 0).clamp(0, u32::MAX as i64) as u32,
            category: s.get("Category").unwrap_or("").split_whitespace().map(str::to_ascii_uppercase).collect::<Vec<_>>().join(" "),
        }
    }

    /// The bonuses of a unit's `[UNITINFO]` section: its `[ADJACENCY]` section's own, in the
    /// file's order.
    pub fn of_unit(info: &TdfSection) -> Vec<AdjacencyDef> {
        let Some(list) = info.sections.iter().find(|s| s.name.eq_ignore_ascii_case("ADJACENCY")) else { return Vec::new() };
        if list.sections.len() > MAX_ADJACENCY {
            log::warn!("{}: more than {MAX_ADJACENCY} adjacency bonuses; the rest are left out", info.get("UnitName").unwrap_or("?"));
        }
        list.sections.iter().take(MAX_ADJACENCY).map(AdjacencyDef::from_tdf).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ta_formats::tdf;

    #[test]
    fn a_buildings_adjacency_is_read_in_its_order() {
        let doc = tdf::parse(
            "[UNITINFO]{UnitName=MYPLANT;AdjacencySize=4;\n[ADJACENCY]{\n[CheaperBuilds]{Affects=BuildEnergy;Add=-0.05;Size=16;}\n\
             [QuickerGuns]{Affects=reloadtime;Add=-0.1;Size=4;Category= artillery  plant ;}\n[Odd]{Affects=Shields;Add=0.5;}\n}}",
        );
        let info = doc.section("UNITINFO").unwrap();
        let a = AdjacencyDef::of_unit(info);
        assert_eq!(a.len(), 3);
        assert_eq!((a[0].name.as_str(), a[0].affect, a[0].add, a[0].size, a[0].category.as_str()), ("CheaperBuilds", Some(AdjacencyAffect::BuildEnergy), -0.05, 16, ""));
        assert_eq!((a[1].affect, a[1].size, a[1].category.as_str()), (Some(AdjacencyAffect::ReloadTime), 4, "ARTILLERY PLANT"), "any case; tokens in capitals");
        assert_eq!((a[2].affect, a[2].affects.as_str(), a[2].size), (None, "Shields", 0), "what it names that is not played kept as written");
        assert!(AdjacencyDef::of_unit(tdf::parse("[UNITINFO]{UnitName=ARMSOLAR;}").section("UNITINFO").unwrap()).is_empty());
        for a in AdjacencyAffect::ALL {
            assert_eq!(AdjacencyAffect::of(a.key()), Some(a));
            assert_eq!(AdjacencyAffect::ALL[a.index()], a);
        }
    }
}
