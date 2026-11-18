//! Where a unit goes, as the modern HUD's build menus show it: on land, on land or water, at sea or
//! in the air. As Forged Alliance's build icons tell it by what lies behind them, each build
//! option's portrait stands on a picture of its ground: grass, grass and water split on the
//! diagonal, water, sky (original art, `tools/hud/grounds.py`, in place of the gunmetal other
//! portraits stand on). A builder's options are listed land first, then those that go on both, then
//! the sea's, then aircraft, each group in the menu's own order (`menu_order`).
//!
//! Pictures that are not portraits keep their own ground: TA's own pictures (`unitpics`), and the
//! Supreme Commander factions' build pictures, their game's icons on their game's frames
//! (`hud::own_picture`). The classic interface keeps TA's pages and pictures as they are.

use std::sync::Arc;

use egui::{Color32, Context, Id, Painter, Pos2, Rect, TextureHandle, TextureOptions, pos2, vec2};
use ta_data::MoveClass;
use ta_sim::world::MoveSpec;
use ta_sim::{Locomotion, UnitType, UnitTypes};

/// Where a unit goes: what its build picture stands on. (The order is the build menus': land first.)
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Domain {
    /// Units that move on land and wade no deeper than the shallows; buildings that stand on land,
    /// or in a little water at a shore: grass.
    Land,
    /// Hovercraft and amphibians; buildings that stand on land or out in the water: grass and
    /// water, split on the diagonal.
    Both,
    /// Ships and submarines; buildings that must stand in water (shipyards, tidal generators,
    /// floating towers, sonar, the underwater extractors, storages and fusion plants): water.
    Sea,
    /// Aircraft (not their plants, which stand on land): sky.
    Air,
}

/// How deep in the water a unit on the ground must be able to go, or a building be able to stand
/// (px under the surface), to go on water as well as on land. TA's movement classes for units that
/// wade go 12 to 23 deep (the mods' up to 27), and its land buildings that may stand at a shore 1
/// deep (the mods' up to 18); the classes of its amphibians (the Commanders, the amphibious tanks
/// and K-bots, the crawling bombs) go 100 deep, the mods' 255, and so do the mods' buildings that
/// stand anywhere.
pub(crate) const DEEP: i32 = 50;

impl Domain {
    /// Every domain, in the order of the pictures in `grounds.png` (and of the build menus).
    pub(crate) const ALL: [Domain; 4] = [Domain::Land, Domain::Both, Domain::Sea, Domain::Air];

    /// Where a unit of type `ty` goes, as the simulation moves and places it (`classes`: the
    /// game's movement classes): an aircraft in the air, a hovercraft on land and water, a ship or
    /// a submarine at sea (its `Locomotion`); another unit by how deep its movement class lets it
    /// go (`MoveSpec`, from its own `MaxWaterDepth` when it names no class); a building by the
    /// water its placement asks for and allows (`World::placement_error`): any `MinWaterDepth`, the
    /// sea; a `MaxWaterDepth` past [`DEEP`], both; else land.
    pub(crate) fn of(ty: &UnitType, classes: &[MoveClass]) -> Domain {
        match ty.locomotion {
            Locomotion::Air => Domain::Air,
            Locomotion::Hover => Domain::Both,
            Locomotion::Naval => Domain::Sea,
            Locomotion::Ground if MoveSpec::for_type(ty, classes).max_depth >= DEEP => Domain::Both,
            Locomotion::Ground => Domain::Land,
            Locomotion::Static if ty.def.min_water_depth > 0.0 => Domain::Sea,
            Locomotion::Static if ty.def.max_water_depth >= f64::from(DEEP) => Domain::Both,
            Locomotion::Static => Domain::Land,
        }
    }
}

/// Puts a builder's `options` in the order its modern build menu lists them: land first, then what
/// goes on both, then the sea's, then aircraft, each group in the order the menu had (TA's: its
/// pages' and its download entries', `GameData::build_menus`). Nothing else orders a menu: neither
/// the player nor a mod has a way to.
pub(crate) fn menu_order(types: &UnitTypes, classes: &[MoveClass], options: &mut [usize]) {
    // (A stable sort: each group keeps the menu's order.)
    options.sort_by_key(|&o| Domain::of(types.get(o), classes));
}

/// The grounds: `tools/hud/grounds.py` draws them, side by side in the order of [`Domain::ALL`],
/// each [`SIDE`] pixels square.
const SHEET: &[u8] = include_bytes!("../../hud/grounds.png");
const SIDE: u32 = 192;
/// The sizes each ground is kept at (pixels), as it is brought down from [`SIDE`] when first drawn:
/// egui's wgpu renderer has no mipmaps, so a ground drawn small is drawn from one near its size,
/// not filtered down from the whole one (which would shimmer).
const LEVELS: [u32; 6] = [32, 48, 64, 96, 128, 192];
/// Pixels of each ground's edge repeated round it in a size's sheet, so that filtering at its edge
/// never takes in the next ground's.
const PAD: u32 = 2;

/// The grounds at each of [`LEVELS`]: a sheet a size, the four side by side with [`PAD`] round
/// each (none: the picture could not be read).
fn sheets() -> Option<Vec<egui::ColorImage>> {
    use image::imageops::{self, FilterType};
    let sheet = image::load_from_memory(SHEET).ok()?.to_rgba8();
    if (sheet.width(), sheet.height()) != (SIDE * Domain::ALL.len() as u32, SIDE) {
        return None;
    }
    let grounds: Vec<image::RgbaImage> = (0..Domain::ALL.len() as u32).map(|i| imageops::crop_imm(&sheet, i * SIDE, 0, SIDE, SIDE).to_image()).collect();
    Some(
        LEVELS
            .iter()
            .map(|&side| {
                let cell = side + 2 * PAD;
                let mut out = image::RgbaImage::new(cell * grounds.len() as u32, cell);
                for (i, ground) in grounds.iter().enumerate() {
                    let small = if side == SIDE { ground.clone() } else { imageops::resize(ground, side, side, FilterType::CatmullRom) };
                    for y in 0..cell {
                        for x in 0..cell {
                            let from = |v: u32| v.saturating_sub(PAD).min(side - 1);
                            out.put_pixel(i as u32 * cell + x, y, *small.get_pixel(from(x), from(y)));
                        }
                    }
                }
                egui::ColorImage::from_rgba_unmultiplied([out.width() as usize, out.height() as usize], out.as_raw())
            })
            .collect(),
    )
}

/// The grounds' textures, one a size (registered when first drawn, and kept).
fn textures(ctx: &Context) -> Option<Arc<Vec<TextureHandle>>> {
    let id = Id::new("hud_grounds");
    if let Some(textures) = ctx.data(|d| d.get_temp::<Option<Arc<Vec<TextureHandle>>>>(id)) {
        return textures;
    }
    let textures = sheets().map(|sheets| Arc::new(sheets.into_iter().zip(LEVELS).map(|(image, side)| ctx.load_texture(format!("hud-grounds-{side}"), image, TextureOptions::LINEAR)).collect::<Vec<_>>()));
    ctx.data_mut(|d| d.insert_temp(id, textures.clone()));
    textures
}

/// Which of [`LEVELS`] a ground `px` pixels across is drawn from: the smallest at least about as
/// large (a little magnified at most), the largest past it.
fn level(px: f32) -> usize {
    LEVELS.iter().position(|&side| side as f32 >= px * 0.9).unwrap_or(LEVELS.len() - 1)
}

/// Paints the ground of `domain` over `rect`, tinted (the middle of it, where `rect` is not
/// square). False, and nothing painted, when the grounds could not be read.
pub(crate) fn paint(painter: &Painter, rect: Rect, domain: Domain, tint: Color32) -> bool {
    let Some(textures) = textures(painter.ctx()) else { return false };
    let px = rect.width().max(rect.height()) * painter.ctx().pixels_per_point();
    let k = level(px);
    let Some(texture) = textures.get(k) else { return false };
    let [w, h] = texture.size().map(|v| v as f32);
    let (side, cell, pad) = (LEVELS[k] as f32, (LEVELS[k] + 2 * PAD) as f32, PAD as f32);
    let i = Domain::ALL.iter().position(|&d| d == domain).unwrap_or(0) as f32;
    let (rw, rh) = (rect.width().max(1.0), rect.height().max(1.0));
    // The square's middle at the rect's shape.
    let (sx, sy) = if rw < rh { (rw / rh, 1.0) } else { (1.0, rh / rw) };
    let min = Pos2::new(i * cell + pad + side * (1.0 - sx) / 2.0, pad + side * (1.0 - sy) / 2.0);
    let uv = Rect::from_min_size(pos2(min.x / w, min.y / h), vec2(side * sx / w, side * sy / h));
    painter.image(texture.id(), rect, uv, tint);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use ta_data::GameData;

    /// Unit types from FBI lines (`[UNITINFO]` bodies), with TA's wading and deep movement classes.
    fn types(units: &[(&str, &str)]) -> (UnitTypes, Vec<MoveClass>) {
        let mut data = GameData::load(&ta_vfs::Vfs::new());
        let class = |name: &str, max: f64| MoveClass { name: name.into(), footprint_x: 2, footprint_z: 2, max_water_depth: max, min_water_depth: -10000.0, max_slope: 32.0, max_water_slope: 255.0 };
        data.move_classes = vec![class("TANKSH2", 12.0), class("TANKBH3", 23.0), class("TANKSH6", 27.0), class("TANKDH3", 100.0), class("CRAB3", 255.0), class("TANKHOVER3", 10000.0)];
        for (name, body) in units {
            let text = format!("[UNITINFO] {{ UnitName={name}; {body} }}");
            data.units.push(ta_data::UnitDef::from_tdf(&ta_formats::tdf::parse(&text).sections[0]).unwrap());
        }
        let classes = data.move_classes.clone();
        (UnitTypes::compile(&data, &|_| None), classes)
    }

    const MOVES: &str = "CanMove=1; MaxVelocity=2; Acceleration=0.1;";

    /// Each kind of unit and building goes where the simulation lets it: aircraft (seaplanes too)
    /// in the air, hovercraft on both, ships and submarines at sea; a unit on the ground by how
    /// deep its movement class (or, with none, its own depth) lets it go; a building by the water
    /// it needs and allows.
    #[test]
    fn every_kind_of_unit_goes_where_the_simulation_lets_it() {
        let units = [
            ("TANK", format!("{MOVES} MovementClass=TANKSH2; MaxWaterDepth=112;")),
            ("HEAVY", format!("{MOVES} MovementClass=TANKBH3;")),
            ("WADER", format!("{MOVES} MovementClass=TANKSH6;")),
            ("AMPHIBIOUS", format!("{MOVES} MovementClass=TANKDH3; MaxWaterDepth=255;")),
            ("CRAB", format!("{MOVES} MovementClass=CRAB3;")),
            ("CLASSLESS", format!("{MOVES} MaxWaterDepth=255;")),
            ("CLASSLESS_WADER", format!("{MOVES} MaxWaterDepth=20;")),
            ("WALKS_ON_A_HOVER_CLASS", format!("{MOVES} MovementClass=TANKHOVER3;")),
            ("HOVERCRAFT", format!("{MOVES} CanHover=1; MovementClass=TANKHOVER3;")),
            ("WATER_HOVERCRAFT", format!("{MOVES} CanHover=1; MinWaterDepth=12;")),
            ("SHIP", format!("{MOVES} Floater=1; MinWaterDepth=15;")),
            ("SUB", format!("{MOVES} MinWaterDepth=20; WaterLine=20;")),
            ("PLANE", format!("{MOVES} CanFly=1;")),
            ("SEAPLANE", format!("{MOVES} CanFly=1; Amphibious=1; MaxWaterDepth=255;")),
            ("SOLAR", "MaxWaterDepth=0;".into()),
            ("PAD", "MaxWaterDepth=1;".into()),
            ("SHORE_PLANT", "MaxWaterDepth=18;".into()),
            ("ANYWHERE", "MaxWaterDepth=255;".into()),
            ("SHIPYARD", "MinWaterDepth=30; WaterLine=1;".into()),
            ("UNDERWATER_FUSION", "MinWaterDepth=34; MaxWaterDepth=255;".into()),
            ("FLOATING_TOWER", "MinWaterDepth=2; WaterLine=3;".into()),
            ("FLOATING_ANYWHERE", "Floater=1; MaxWaterDepth=255;".into()),
        ];
        let list: Vec<(&str, &str)> = units.iter().map(|(n, b)| (*n, b.as_str())).collect();
        let (types, classes) = types(&list);
        let of = |name: &str| Domain::of(types.by_name(name).unwrap(), &classes);
        for name in ["TANK", "HEAVY", "WADER", "CLASSLESS_WADER", "SOLAR", "PAD", "SHORE_PLANT"] {
            assert_eq!(of(name), Domain::Land, "{name}");
        }
        for name in ["AMPHIBIOUS", "CRAB", "CLASSLESS", "WALKS_ON_A_HOVER_CLASS", "HOVERCRAFT", "WATER_HOVERCRAFT", "ANYWHERE", "FLOATING_ANYWHERE"] {
            assert_eq!(of(name), Domain::Both, "{name}");
        }
        for name in ["SHIP", "SUB", "SHIPYARD", "UNDERWATER_FUSION", "FLOATING_TOWER"] {
            assert_eq!(of(name), Domain::Sea, "{name}");
        }
        for name in ["PLANE", "SEAPLANE"] {
            assert_eq!(of(name), Domain::Air, "{name}");
        }
    }

    /// A menu lists land first, then both, then the sea's, then aircraft, each group in the
    /// menu's own order.
    #[test]
    fn a_menu_lists_land_then_both_then_sea_then_aircraft() {
        let list = [
            ("SHIP", format!("{MOVES} Floater=1; MinWaterDepth=15;")),
            ("TANK", format!("{MOVES} MovementClass=TANKSH2;")),
            ("PLANE", format!("{MOVES} CanFly=1;")),
            ("HOVERCRAFT", format!("{MOVES} CanHover=1;")),
            ("KBOT", format!("{MOVES} MovementClass=TANKSH2;")),
            ("SUB", format!("{MOVES} MinWaterDepth=20; WaterLine=20;")),
            ("AMPHIBIOUS", format!("{MOVES} MovementClass=TANKDH3;")),
            ("GUNSHIP", format!("{MOVES} CanFly=1; HoverAttack=1;")),
            ("SCOUT", format!("{MOVES} MovementClass=TANKSH2;")),
        ];
        let units: Vec<(&str, &str)> = list.iter().map(|(n, b)| (*n, b.as_str())).collect();
        let (types, classes) = types(&units);
        let mut menu: Vec<usize> = list.iter().map(|(n, _)| types.id(n).unwrap()).collect();
        menu_order(&types, &classes, &mut menu);
        let names: Vec<&str> = menu.iter().map(|&o| types.get(o).name.as_str()).collect();
        assert_eq!(names, ["TANK", "KBOT", "SCOUT", "HOVERCRAFT", "AMPHIBIOUS", "SHIP", "SUB", "PLANE", "GUNSHIP"]);
        // A menu all of one kind stays as it was.
        let mut planes = vec![types.id("GUNSHIP").unwrap(), types.id("PLANE").unwrap()];
        menu_order(&types, &classes, &mut planes);
        assert_eq!(planes, [types.id("GUNSHIP").unwrap(), types.id("PLANE").unwrap()]);
    }

    /// The grounds' picture holds the four, in the order of `Domain`, each its own colour (grass
    /// green, water blue, the sky the bluest and lightest, the shore both), and every size is made
    /// from it, padded.
    #[test]
    fn the_grounds_are_the_four_in_order_at_every_size() {
        let sheets = sheets().expect("grounds.png reads, four grounds of SIDE");
        assert_eq!(sheets.len(), LEVELS.len());
        for (sheet, side) in sheets.iter().zip(LEVELS) {
            assert_eq!(sheet.size, [((side + 2 * PAD) * 4) as usize, (side + 2 * PAD) as usize]);
            assert!(sheet.pixels.iter().all(|p| p.a() == 255), "opaque at {side}");
        }
        let full = &sheets[LEVELS.len() - 1];
        let cell = (SIDE + 2 * PAD) as usize;
        let mean = |i: usize, (x0, x1): (f32, f32), (y0, y1): (f32, f32)| -> [f32; 3] {
            let mut sum = [0.0f32; 3];
            let mut n = 0.0;
            for y in ((y0 * SIDE as f32) as usize)..((y1 * SIDE as f32) as usize) {
                for x in ((x0 * SIDE as f32) as usize)..((x1 * SIDE as f32) as usize) {
                    let p = full.pixels[(y + PAD as usize) * full.size[0] + i * cell + x + PAD as usize];
                    sum = [sum[0] + p.r() as f32, sum[1] + p.g() as f32, sum[2] + p.b() as f32];
                    n += 1.0;
                }
            }
            sum.map(|v| v / n)
        };
        let whole = (0.0, 1.0);
        let green = |c: [f32; 3]| c[1] > c[0] && c[1] > c[2];
        let blue = |c: [f32; 3]| c[2] > c[0] && c[2] >= c[1] * 0.95;
        assert!(green(mean(0, whole, whole)), "land: {:?}", mean(0, whole, whole));
        assert!(green(mean(1, (0.0, 0.3), (0.0, 0.3))) && blue(mean(1, (0.7, 1.0), (0.7, 1.0))), "both: grass at the top left, water at the bottom right");
        assert!(blue(mean(2, whole, whole)), "sea: {:?}", mean(2, whole, whole));
        let (sea, sky) = (mean(2, whole, whole), mean(3, whole, whole));
        assert!(blue(sky) && sky.iter().sum::<f32>() > sea.iter().sum::<f32>(), "air: {sky:?}");
        // The middle, where the unit stands, is lit; the corners are darker.
        for i in 0..4 {
            let (middle, corner) = (mean(i, (0.4, 0.6), (0.35, 0.55)), mean(i, (0.0, 0.08), (0.92, 1.0)));
            assert!(middle.iter().sum::<f32>() > corner.iter().sum::<f32>() * 1.2, "{:?}: {middle:?} against {corner:?}", Domain::ALL[i]);
        }
    }

    /// A ground is drawn from the smallest size at least about as large as it is shown.
    #[test]
    fn a_ground_is_drawn_from_the_size_nearest_above() {
        assert_eq!(LEVELS[level(20.0)], 32);
        assert_eq!(LEVELS[level(46.0)], 48);
        assert_eq!(LEVELS[level(66.0)], 64, "a little magnified");
        assert_eq!(LEVELS[level(99.0)], 96);
        assert_eq!(LEVELS[level(132.0)], 128);
        assert_eq!(LEVELS[level(198.0)], 192);
        assert_eq!(LEVELS[level(400.0)], 192, "the largest past it");
    }

    fn install() -> Option<ta_vfs::Vfs> {
        let install = ta_vfs::install::discover(None).ok()?;
        let mut vfs = ta_vfs::Vfs::new();
        vfs.mount_game_dir(&install.root, "base").ok()?;
        Some(vfs)
    }

    /// TA's own units go where a player knows them to go: the buildings on land (the plants of
    /// hovercraft and aircraft too, the air repair pads and the Core's fusion plant that may stand
    /// at a shore) or at sea (the yards, the seaplane platforms, tidal generators, sonar, the
    /// floating towers and the underwater extractors, storages and fusion plants), none on both;
    /// the tanks and K-bots that wade on land; the hovercraft, the amphibious tanks and K-bots, the
    /// Commanders and their decoys and the crawling bombs on both; the ships and submarines at sea;
    /// every aircraft, the seaplanes too, in the air. (`--nocapture` lists every unit's.)
    #[test]
    fn tas_units_go_where_a_player_knows_them_to_go() {
        let Some(vfs) = install() else {
            eprintln!("TA install not found; skipping");
            return;
        };
        let data = GameData::load(&vfs);
        let types = UnitTypes::compile(&data, &|p| vfs.read_opt(p));
        let of = |name: &str| Domain::of(types.by_name(name).unwrap_or_else(|| panic!("{name}")), &data.move_classes);
        let expect = [
            (Domain::Land, "ARMSOLAR ARMWIN ARMMEX ARMMAKR ARMESTOR ARMGEO ARMFUS CORFUS ARMLAB ARMVP ARMAP ARMAAP ARMHP CORHP CORGANT ARMASP CORASP ARMLLT CORPUN ARMDRAG ARMFORT ARMRAD ARMMINE1 CORMINE6 ARMSILO ARMPW ARMSTUMP ARMBULL CORSUMO CORKROG ARMSPID ARMASER CORSPEC ARMFLEA CORMLV ARMCK CORCV"),
            (Domain::Both, "ARMCH CORCH ARMSH CORSNAP ARMTHOVR CORTHOVR ARMAMPH CORAMPH ARMCROC CORSEAL ARMCOM CORCOM ARMDECOM CORDECOM ARMVADER CORROACH"),
            (Domain::Sea, "ARMSY CORSY ARMASY CORASY ARMPLAT CORPLAT ARMTIDE CORTIDE ARMUWMEX CORUWMEX ARMUWFUS CORUWFUS ARMUWES CORUWMS ARMFMKR ARMFHLT CORFHLT ARMFRT ARMFDRAG ARMTL CORATL ARMSONAR CORASON ARMCS CORCS ARMACSUB ARMSUB CORSSUB ARMBATS CORCARRY ARMPT"),
            (Domain::Air, "ARMFIG CORVENG ARMCA ARMACA ARMCSA CORCSA ARMSEAP CORSEAP ARMSFIG ARMSEHAK ARMATLAS CORVALK ARMPEEP ARMBRAWL ARMPNIX CORTITAN"),
        ];
        for (domain, names) in expect {
            for name in names.split_whitespace() {
                assert_eq!(of(name), domain, "{name}");
            }
        }
        // Every unit of the two sides: buildings never in the air, aircraft always.
        for t in types.types.iter().filter(|t| matches!(t.def.side.as_str(), "ARM" | "CORE")) {
            let d = Domain::of(t, &data.move_classes);
            assert_eq!(d == Domain::Air, t.locomotion == Locomotion::Air, "{}", t.name);
            println!("{:9} {:?}", t.name, d);
        }
    }

    /// TA's own menus, ordered: the construction hovercraft's plants with the hovercraft platform
    /// before the shipyard, the construction ship's defences with the light laser tower first, the
    /// advanced K-bot lab's amphibians (the crawling bomb, the Pelican and the decoy Commander)
    /// after its K-bots, each group as TA's pages have it.
    #[test]
    fn tas_menus_list_land_first() {
        let Some(vfs) = install() else {
            eprintln!("TA install not found; skipping");
            return;
        };
        let data = GameData::load(&vfs);
        let types = UnitTypes::compile(&data, &|p| vfs.read_opt(p));
        let menu = |builder: &str, purpose: Option<super::super::Purpose>| -> Vec<String> {
            let b = types.by_name(builder).unwrap();
            let mut options: Vec<usize> = b.build_options.iter().copied().filter(|&o| purpose.is_none_or(|p| super::super::Purpose::of(&types, types.get(o)) == p)).collect();
            menu_order(&types, &data.move_classes, &mut options);
            options.iter().map(|&o| types.get(o).name.clone()).collect()
        };
        use super::super::Purpose;
        assert_eq!(menu("ARMCH", Some(Purpose::Factories)), ["ARMLAB", "ARMVP", "ARMAP", "ARMHP", "ARMSY"]);
        assert_eq!(menu("ARMCS", Some(Purpose::Defence)), ["ARMLLT", "ARMTL", "ARMFRT", "ARMFHLT", "ARMFDRAG"]);
        assert_eq!(menu("ARMCOM", Some(Purpose::Economy)), ["ARMSOLAR", "ARMWIN", "ARMESTOR", "ARMMSTOR", "ARMMEX", "ARMMAKR", "ARMTIDE", "ARMUWES", "ARMUWMS", "ARMUWMEX", "ARMFMKR"], "already land first");
        assert_eq!(
            menu("ARMALAB", None),
            ["ARMACK", "ARMZEUS", "ARMFIDO", "ARMASER", "ARMFAST", "ARMMAV", "ARMMARK", "ARMSNIPE", "ARMSPY", "ARMFARK", "ARMVADER", "ARMAMPH", "ARMDECOM"]
        );
        assert_eq!(menu("CORAVP", None)[..2], ["CORACV".to_string(), "CORREAP".to_string()], "the Crock after the land vehicles");
        assert_eq!(menu("CORAVP", None).last().map(String::as_str), Some("CORSEAL"));
        assert_eq!(menu("ARMPLAT", None), ["ARMCSA", "ARMSEHAK", "ARMFIG", "ARMSFIG", "ARMLANCE", "ARMSEAP"], "aircraft alone: as it was");
    }
}
