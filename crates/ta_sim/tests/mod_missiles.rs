//! A mod's missiles fly to what they are fired at (a report and a check; ignored: it needs a mod):
//! `TA_TEST_MOD=<absolute mod folder> cargo test --release -p ta_sim --test mod_missiles --
//! --ignored --nocapture` (`TA_UNITS=ARMMECH,CORUSUB`: those units alone; `TA_MISSILE_AT=0.5,0.9`:
//! the enemy that share of the weapon's range away, half by default; `TA_MISSILE_TARGET=CORHLT`:
//! the enemy a unit of that type, a building say, for weapons whose bad targets are mobile units).
//!
//! Each unit carrying a missile (a guided or vertically launched weapon drawn as a model, a bomb or
//! a flame, not a stockpiled or anti-missile one) stands on a flat map (a sea for ships and
//! submarines; aircraft fly over it) with an invulnerable enemy that far off, everything in sight,
//! and is sent at it for twenty seconds. The report lists, for each such weapon, the missiles it
//! fired, how far from their launcher they got at the most, how high they went, how long they flew
//! and how far from the enemy they went off (`TA_TRACE=1`: the first three missiles tick by tick).
//! The check: every missile leaves from its unit and none is still near where it left when its
//! time is up (TA: Escalation's vertical missiles, which start faster than their `weaponvelocity`
//! and slow down, hung over their launchers and went off there; the C.O.R.E.'s rockets left from
//! far off the map).

use std::sync::Arc;

use ta_data::{GameData, MapInfo, RenderType};
use ta_formats::tnt::{FEATURE_NONE, TntMap};
use ta_sim::{Cheat, Command, Fx, FxVec3, Locomotion, Order, PlayerSetup, Sim, Target, Trajectory, UnitTypes};

/// A flat map 160 cells a side: land 60 high, or a sea 60 deep over a bed 20 high.
fn flat_map(sea: bool) -> TntMap {
    let cells = 160 * 160;
    let (ground, sea_level) = if sea { (20, 80) } else { (60, 0) };
    TntMap { width: 160, height: 160, sea_level, tiles: Vec::new(), tile_map: Vec::new(), heights: vec![ground; cells], features: vec![FEATURE_NONE; cells], feature_names: Vec::new(), minimap: Default::default(), flags: Vec::new() }
}

/// The game and the mod `TA_TEST_MOD` names, mounted as the game mounts them.
fn game_and_mod() -> Option<ta_vfs::Vfs> {
    let Ok(install) = ta_vfs::install::discover(None) else {
        eprintln!("TA install not found; skipping");
        return None;
    };
    let Ok(dir) = std::env::var("TA_TEST_MOD") else {
        eprintln!("TA_TEST_MOD not set; skipping");
        return None;
    };
    let module = ta_vfs::mods::read_mod(std::path::Path::new(&dir)).expect("TA_TEST_MOD: a mod's folder").expect("TA_TEST_MOD: a mod's folder");
    println!("mod: {} {}", module.manifest.name, module.manifest.version);
    let mounted = ta_vfs::mods::mount(Some(&install.root), None, &[&module], &|m| m.manifest.id.clone());
    mounted.install.unwrap();
    mounted.mods.expect("the mod mounts");
    Some(mounted.vfs)
}

/// A missile in flight: where it was launched from and the farthest it has got from there (px).
struct Missile {
    id: u32,
    from: FxVec3,
    farthest: f64,
}

/// What became of the missiles of one weapon.
#[derive(Default)]
struct Flights {
    fired: u32,
    /// The farthest any got from where it was launched (px).
    farthest: f64,
    /// The highest any climbed over where it was launched (px).
    highest: f64,
    /// Ticks flown and how far from the enemy each went off, or the end of its life (`true`).
    ended: Vec<(u32, f64, bool)>,
    /// Those whose time was up near where they were launched: hung there.
    hung: u32,
    /// Those launched far from their unit.
    launched_afar: u32,
}

#[test]
#[ignore]
fn a_mods_missiles_fly_to_their_targets() {
    let Some(vfs) = game_and_mod() else { return };
    let data = Arc::new(GameData::load(&vfs));
    let types = Arc::new(UnitTypes::compile(&data, &|p| vfs.read_opt(p)));
    let only: Option<Vec<String>> = std::env::var("TA_UNITS").ok().map(|v| v.split(',').map(|s| s.trim().to_ascii_uppercase()).collect());
    let shares: Vec<f64> = std::env::var("TA_MISSILE_AT").ok().map(|v| v.split(',').filter_map(|s| s.trim().parse().ok()).collect()).unwrap_or_else(|| vec![0.5]);
    let weapons = ta_sim::WeaponTypes::compile(&data);
    // (Not the walkers' stomps: points or shells launched upright at rest, going off underfoot.)
    let missile = |w: usize| {
        let w = weapons.get(w);
        matches!(w.def.render_type, RenderType::Model | RenderType::Bomb | RenderType::Flame) && (w.trajectory == Trajectory::Guided || w.vlaunch) && !w.def.interceptor && !w.def.stockpile
    };
    let carriers: Vec<usize> = types
        .types
        .iter()
        .filter(|t| t.weapons.iter().flatten().any(|&w| missile(w)))
        .filter(|t| only.as_ref().is_none_or(|o| o.contains(&t.name)))
        .map(|t| t.id)
        .collect();
    // The enemy: the sturdiest of the mod's tanks (ships for ships), of a tank's size and height
    // (`TA_MISSILE_TARGET=<unit>` another).
    let named = std::env::var("TA_MISSILE_TARGET").ok().and_then(|n| types.id(&n.to_ascii_uppercase()));
    let sturdiest = |kind: Locomotion| {
        named.or_else(|| {
            types
                .types
                .iter()
                .filter(|t| t.locomotion == kind && t.def.can_move && !t.def.is_commander && t.footprint.0.max(t.footprint.1) <= 4 && t.model.height >= Fx::from_int(16))
                .max_by_key(|t| (t.max_health, std::cmp::Reverse(t.id)))
                .map(|t| t.id)
        })
    };
    let (ground, ship) = (sturdiest(Locomotion::Ground), sturdiest(Locomotion::Naval));
    println!("targets: ground {:?}, water {:?}", ground.map(|g| &types.get(g).name), ship.map(|s| &types.get(s).name));
    let mut stuck = Vec::new();
    for &kind in &carriers {
        let t = types.get(kind);
        let sea = t.locomotion == Locomotion::Naval || t.def.min_water_depth > 0.0;
        for &share in &shares {
            for (slot, &w) in t.weapons.iter().enumerate() {
                let Some(w) = w.filter(|&w| missile(w)) else { continue };
                let wt = weapons.get(w);
                let setups: Vec<PlayerSetup> = (0..2).map(|i| PlayerSetup { side: "ARM".into(), color: i, start: FxVec3::from_ints(200 + 1000 * i as i64, 60, 200), metal: Fx::from_int(100_000), energy: Fx::from_int(100_000), ai: false }).collect();
                let mut sim = Sim::new(types.clone(), data.clone(), &flat_map(sea), &MapInfo::default(), 0, &setups, 11);
                for p in &mut sim.players {
                    p.base_storage = (Fx::from_int(100_000_000), Fx::from_int(100_000_000));
                    p.economy.metal = Fx::from_int(50_000_000);
                    p.economy.energy = Fx::from_int(50_000_000);
                }
                let (side, centre) = (160 * 16, FxVec3::from_ints(160 * 8, 60, 160 * 8 + 800));
                let spawned = if t.locomotion == Locomotion::Air { sim.spawn_airborne(kind, 0, centre, 32768, None) } else { sim.spawn_unit(kind, 0, centre, 32768, true) };
                let Some(id) = spawned else {
                    println!("{:<10} could not stand there", t.name);
                    continue;
                };
                // Straight ahead of it (north), that share of the weapon's reach off, clear of it.
                let reach = (wt.range.to_f64() * share).max(t.radius.to_f64() + 96.0).min(side as f64 / 2.0 + 600.0) as i64;
                let Some(enemy) = (if sea { ship } else { ground }).and_then(|e| sim.spawn_unit(e, 1, centre - FxVec3::from_ints(0, 0, reach), 0, true)) else {
                    println!("{:<10} (no enemy could stand {reach} px off)", t.name);
                    continue;
                };
                // (Everything in sight: a long reach's enemy is past its unit's own sight.)
                sim.los_enabled = false;
                sim.cheats_allowed = true;
                sim.step(&[
                    Command::Cheat { player: 1, cheat: Cheat::Invulnerable(true) },
                    Command::Order { player: 0, units: vec![id], order: Order::Attack(Target::Unit(enemy)), queue: false },
                ]);
                let mut flights = Flights::default();
                let mut missiles: Vec<Missile> = Vec::new();
                let trace = std::env::var("TA_TRACE").is_ok();
                // (Far off: off the map, or 1,000 px from its unit, which no muzzle is: the Zenith
                // drops its bombs from a point its script puts on the ground 460 px under it. Hung:
                // its time up within half the enemy's distance of where it was launched, or 200 px.)
                let near = 1000.0;
                let hang = (reach as f64 / 2.0).min(200.0);
                for tick in 0..600 {
                    sim.step(&[]);
                    let unit_at = sim.unit(id).map(|u| u.pos);
                    // (In flight, and those gone this tick: some never live to be seen in flight.)
                    let seen = sim.projectiles.iter().filter(|p| p.weapon == w && p.owner == id).map(|p| (p, false));
                    let gone = sim.spent.iter().filter(|(p, _)| p.weapon == w && p.owner == id).map(|(p, _)| (p, true));
                    for (p, ended) in seen.chain(gone) {
                        let k = match missiles.iter().position(|m| m.id == p.id) {
                            Some(k) => k,
                            None => {
                                // (Launched this tick: it left from where it was at its start.)
                                let from = p.prev_pos;
                                flights.fired += 1;
                                let off_map = from.x < Fx::ZERO || from.z < Fx::ZERO || from.x > Fx::from_int(side) || from.z > Fx::from_int(side);
                                if off_map || unit_at.is_some_and(|at| from.distance(at).to_f64() > near) {
                                    flights.launched_afar += 1;
                                }
                                missiles.push(Missile { id: p.id, from, farthest: 0.0 });
                                missiles.len() - 1
                            }
                        };
                        let m = &mut missiles[k];
                        m.farthest = m.farthest.max(p.pos.distance(m.from).to_f64());
                        flights.farthest = flights.farthest.max(m.farthest);
                        flights.highest = flights.highest.max((p.pos.y - m.from.y).to_f64());
                        let target = sim.unit(enemy).map_or(p.target_pos, |e| e.pos);
                        if trace && k < 3 {
                            println!("  tick {tick} missile {} age {}: at {:?}, speed {:.1}, {:.0} px off{}", p.id, p.age, p.pos.to_f32().map(|c| c as i32), p.vel.length().to_f64() * 30.0, p.pos.distance(target).to_f64(), if ended { ", gone" } else { "" });
                        }
                        if ended {
                            let timed_out = p.age >= p.ttl;
                            flights.ended.push((p.age, p.pos.distance(target).to_f64(), timed_out));
                            if timed_out && m.farthest < hang {
                                flights.hung += 1;
                            }
                        }
                    }
                }
                let lives: Vec<String> = flights.ended.iter().take(4).map(|&(age, off, timed_out)| format!("{age}t/{off:.0}px{}", if timed_out { " (out of time)" } else { "" })).collect();
                let faults = [(flights.hung, "hung"), (flights.launched_afar, "launched far off")].iter().filter(|(n, _)| *n > 0).map(|(n, what)| format!(", {n} {what}")).collect::<String>();
                println!(
                    "{:<10} {:<20} w{} {:<18} {reach:>5} px: fired {:>2}, farthest {:>5.0}, highest {:>5.0}, {}{faults}",
                    t.name,
                    t.def.display_name,
                    slot + 1,
                    wt.name,
                    flights.fired,
                    flights.farthest,
                    flights.highest,
                    lives.join(", ")
                );
                if flights.hung > 0 || flights.launched_afar > 0 {
                    stuck.push(format!("{} {}", t.name, wt.name));
                }
            }
        }
    }
    assert!(stuck.is_empty(), "missiles that hung over their launcher or left from far off: {stuck:?}");
}
