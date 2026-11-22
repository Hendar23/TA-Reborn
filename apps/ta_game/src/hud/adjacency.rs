//! Adjacency on the battlefield (Supreme Commander's: what a building gives the buildings of its
//! player it touches, `ta_sim`'s `sim/adjacency.rs`), drawn here alone: the simulation keeps who
//! gives what to whom (`Unit::adjacency`). Between two buildings, a link across the seam where
//! they touch: a thin line along the seam and, at its middle, a chevron for each kind of bonus
//! pointing at the building it helps, in the colour of what it saves or raises (energy's, metal's,
//! a weapon's reload in a warm red). Faint where nothing calls for it; bright, a spark crossing the
//! seam, for the building under the pointer and those selected, and for one being placed, whose
//! links to come are drawn as dashes. Too far out to read, none.

use egui::{Color32, Painter, Pos2, Stroke, Vec2 as EVec2};
use glam::Vec3;
use ta_sim::{AdjacencyAffect, Fx, UnitId};
use ta_ui::i18n::{n, t, tf};

use crate::game::Game;
use crate::theme::pal;

/// A footprint on the ground, world px: x0, z0, x1, z1.
type Area = (f32, f32, f32, f32);

/// Screen points to a world px under which no link is drawn (the camera far out), and over which
/// they are drawn whole.
const NEAR_ENOUGH: (f32, f32) = (0.22, 0.45);

/// The links of how many buildings are drawn at most (the nearest the middle of the view first
/// would want a sort: the first by slot do, which a base never comes near).
const MOST: usize = 600;

fn area(origin: (i32, i32), fp: (u32, u32)) -> Area {
    let (x, z) = (origin.0 as f32 * 16.0, origin.1 as f32 * 16.0);
    (x, z, x + fp.0 as f32 * 16.0, z + fp.1 as f32 * 16.0)
}

/// Where two footprints meet: the seam's two ends and the way from the giver to the one it helps
/// (none where they do not touch along an edge).
fn seam(giver: Area, receiver: Area) -> Option<(Vec3, Vec3, Vec3)> {
    let overlap = |a0: f32, a1: f32, b0: f32, b1: f32| (a0.max(b0), a1.min(b1));
    let (gx0, gz0, gx1, gz1) = giver;
    let (rx0, rz0, rx1, rz1) = receiver;
    let (along, x, dir) = if gx1 == rx0 {
        (overlap(gz0, gz1, rz0, rz1), Some(gx1), Vec3::X)
    } else if rx1 == gx0 {
        (overlap(gz0, gz1, rz0, rz1), Some(gx0), -Vec3::X)
    } else if gz1 == rz0 {
        (overlap(gx0, gx1, rx0, rx1), None, Vec3::Z)
    } else if rz1 == gz0 {
        (overlap(gx0, gx1, rx0, rx1), None, -Vec3::Z)
    } else {
        return None;
    };
    if along.1 <= along.0 {
        return None;
    }
    // (In a little from the corners: the seam's own length, not past the buildings.)
    let inset = ((along.1 - along.0) * 0.15).min(6.0);
    let (a, b) = (along.0 + inset, along.1 - inset);
    Some(match (x, dir.z == 0.0) {
        (Some(x), true) => (Vec3::new(x, 0.0, a), Vec3::new(x, 0.0, b), dir),
        _ => {
            let z = if dir.z > 0.0 { gz1 } else { gz0 };
            (Vec3::new(a, 0.0, z), Vec3::new(b, 0.0, z), dir)
        }
    })
}

/// The colour of a kind of bonus: energy's for what it saves or makes of energy, metal's for
/// metal, a warm red for a quicker reload.
fn colour(affect: AdjacencyAffect) -> Color32 {
    match affect {
        AdjacencyAffect::BuildEnergy | AdjacencyAffect::EnergyUse | AdjacencyAffect::EnergyPerShot | AdjacencyAffect::EnergyMake => pal().energy_bar,
        AdjacencyAffect::BuildMetal | AdjacencyAffect::MetalMake => pal().metal_bar,
        AdjacencyAffect::ReloadTime => Color32::from_rgb(235, 115, 80),
    }
}

/// The colours of the kinds of bonus in `bits` (a bit an affect), each once.
fn colours(bits: u8) -> Vec<Color32> {
    let mut out: Vec<Color32> = Vec::new();
    for a in AdjacencyAffect::ALL {
        let c = colour(a);
        if bits & 1 << a.index() != 0 && !out.contains(&c) {
            out.push(c);
        }
    }
    out
}

/// The links between the buildings in view of ours (or our allies', or everyone's for a
/// spectator) that help each other, and those to come of one being placed (`placing`: its type
/// and where it would stand).
pub(super) fn draw(painter: &Painter, game: &Game, project: &dyn Fn(Vec3) -> Option<Pos2>, ground_at: &dyn Fn(f32, f32) -> f32, zoom: f32, clock: f32, placing: Option<(usize, Vec3)>) {
    let far = ((zoom - NEAR_ENOUGH.0) / (NEAR_ENOUGH.1 - NEAR_ENOUGH.0)).clamp(0.0, 1.0);
    if far <= 0.0 {
        return;
    }
    let sim = &game.sim;
    // (The buildings the player is looking at: under the pointer, and the selected.)
    let mut bright: Vec<UnitId> = game.selection.iter().take(200).copied().collect();
    bright.extend(game.hovered);
    let link = |giver: Area, receiver: Area, bits: u8, lit: bool, coming: bool| {
        let Some((a, b, dir)) = seam(giver, receiver) else { return };
        let on_ground = |p: Vec3| Vec3::new(p.x, ground_at(p.x, p.z) + 2.0, p.z);
        let (Some(sa), Some(sb)) = (project(on_ground(a)), project(on_ground(b))) else { return };
        let strength = far * if lit || coming { 1.0 } else { 0.42 };
        let kinds = colours(bits);
        let seam_colour = kinds.first().copied().unwrap_or(pal().accent);
        // The seam: dark under, the colour over (to read on pale ground and dark).
        if coming {
            painter.extend(egui::Shape::dashed_line(&[sa, sb], Stroke::new(1.6, seam_colour.gamma_multiply(strength)), 5.0, 4.0));
        } else {
            painter.line_segment([sa, sb], Stroke::new(3.2, Color32::from_black_alpha((110.0 * strength) as u8)));
            painter.line_segment([sa, sb], Stroke::new(1.4, seam_colour.gamma_multiply(strength * 0.9)));
        }
        // A chevron a kind, side by side along the seam, pointing at the building helped.
        let mid = (a + b) * 0.5;
        let Some(m) = project(on_ground(mid)) else { return };
        let Some(ahead) = project(on_ground(mid + dir * 8.0)) else { return };
        let forward = (ahead - m).normalized();
        if !forward.x.is_finite() {
            return;
        }
        let side = EVec2::new(-forward.y, forward.x);
        let size = (4.0 + 3.0 * zoom).clamp(4.0, 8.0);
        let spread = size * 1.6;
        for (k, c) in kinds.iter().enumerate() {
            let offset = (k as f32 - (kinds.len() as f32 - 1.0) / 2.0) * spread;
            let tip = m + side * offset + forward * size * 0.6;
            let wing = forward * size;
            let points = [tip - wing + side * size * 0.75, tip, tip - wing - side * size * 0.75];
            painter.add(egui::Shape::line(points.to_vec(), Stroke::new(3.4, Color32::from_black_alpha((120.0 * strength) as u8))));
            painter.add(egui::Shape::line(points.to_vec(), Stroke::new(1.8, c.gamma_multiply(strength))));
        }
        // Lit: a spark crossing the seam from the giver into the building it helps.
        if lit && !coming {
            let phase = (clock * 0.8).fract();
            let from = mid - dir * 14.0;
            let p = from + dir * (28.0 * phase);
            if let Some(s) = project(on_ground(p)) {
                let fade = (1.0 - (phase * 2.0 - 1.0).abs()) * strength;
                painter.circle_filled(s, 2.6, seam_colour.gamma_multiply(fade));
            }
        }
    };
    let mut drawn = 0;
    for u in sim.units() {
        let Some(taken) = u.adjacency.as_ref() else { continue };
        if !game.may_inspect(u.owner) || game.seen(u) != ta_sim::Seen::Visible {
            continue;
        }
        let Some((_, origin, fp)) = u.adjacency_key else { continue };
        let receiver = area(origin, fp);
        for &(g, bits) in &taken.from {
            let Some((_, go, gfp)) = sim.unit(g).and_then(|g| g.adjacency_key) else { continue };
            link(area(go, gfp), receiver, bits, bright.contains(&u.id) || bright.contains(&g), false);
        }
        drawn += 1;
        if drawn >= MOST {
            break;
        }
    }
    // A building being placed: the links it would make, given and taken.
    if let Some((type_id, pos)) = placing {
        let t = sim.types.get(type_id);
        let at = ta_sim::FxVec3::new(Fx::from_f32(pos.x), Fx::from_f32(pos.y), Fx::from_f32(pos.z));
        let anchor = ta_sim::world::World::pos_to_anchor(at, t.footprint);
        let ghost = area(ta_sim::world::World::footprint_origin(anchor, t.footprint), t.footprint);
        let (taken, given) = sim.adjacency_preview(type_id, at, game.hud_player());
        for (g, bits) in taken.map(|a| a.from).unwrap_or_default() {
            let Some((_, go, gfp)) = sim.unit(g).and_then(|g| g.adjacency_key) else { continue };
            link(area(go, gfp), ghost, bits, true, true);
        }
        for (r, bits) in given {
            let Some((_, ro, rfp)) = sim.unit(r).and_then(|r| r.adjacency_key) else { continue };
            link(ghost, area(ro, rfp), bits, true, true);
        }
    }
}

/// What a kind of bonus changes, in the words of the cards.
fn label(affect: AdjacencyAffect) -> &'static str {
    match affect {
        AdjacencyAffect::BuildEnergy => n("build energy"),
        AdjacencyAffect::BuildMetal => n("build metal"),
        AdjacencyAffect::EnergyUse => n("upkeep"),
        AdjacencyAffect::EnergyPerShot => n("shot energy"),
        AdjacencyAffect::ReloadTime => n("reload"),
        AdjacencyAffect::EnergyMake => n("energy made"),
        AdjacencyAffect::MetalMake => n("metal made"),
    }
}

/// A share as a signed percentage, a decimal where it has one ("-6.3%", "+25%").
pub(crate) fn percent(share: f64) -> String {
    // To two places, half away from none, without their zeros: 6.25, 12.5, 25, 14.06.
    let text = format!("{:+.2}", (share * 10_000.0).round() / 100.0);
    format!("{}%", text.trim_end_matches('0').trim_end_matches('.'))
}

/// What the shares of `change` (by affect) come to, a phrase each: "build energy -6.3%".
pub(crate) fn bonus_words(change: &[Fx; ta_sim::unit::ADJACENCY_AFFECTS]) -> Vec<String> {
    AdjacencyAffect::ALL
        .into_iter()
        .filter(|a| !change[a.index()].is_zero())
        .map(|a| tf("{what} {pct}", &[("what", &t(label(a))), ("pct", &percent(change[a.index()].to_f64()))]))
        .collect()
}

/// The hover card's lines on a building's adjacency: what the buildings beside it give it, and
/// how many it helps.
pub(crate) fn card_lines(game: &Game, u: &ta_sim::Unit) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(a) = u.adjacency.as_ref() {
        let words = bonus_words(&a.change).join("  ·  ");
        lines.push(match a.from.len() {
            1 => tf("Beside 1 building: {bonuses}", &[("bonuses", &words)]),
            k => tf("Beside {n} buildings: {bonuses}", &[("n", &k), ("bonuses", &words)]),
        });
    }
    let helped = game.sim.adjacency_given(u).len();
    if helped > 0 {
        lines.push(if helped == 1 { t("Helps 1 building beside it").to_string() } else { tf("Helps {n} buildings beside it", &[("n", &helped)]) });
    }
    lines
}

/// The information page's lines on a type's adjacency: its size class, and what it gives the
/// buildings beside it, a line a kind of bonus (the shares by their size, and what else they
/// must be).
pub(crate) fn type_lines(ty: &ta_sim::UnitType) -> Vec<String> {
    let mut lines = Vec::new();
    if ty.adjacency_size > 0 {
        lines.push(tf("Size class {n}: takes the bonuses of up to {n} buildings beside it", &[("n", &ty.adjacency_size)]));
    }
    for a in AdjacencyAffect::ALL {
        // The file's own figures, as exact as it gives them (the simulation's are as near as
        // its fixed point holds).
        let mut shares: Vec<(u32, f64, String)> = ty.def.adjacency.iter().filter(|d| d.affect == Some(a) && d.add != 0.0).map(|d| (d.size, d.add, d.category.clone())).collect();
        if shares.is_empty() {
            continue;
        }
        shares.sort_by(|x, y| x.0.cmp(&y.0));
        let list: Vec<String> = shares
            .iter()
            .map(|(size, add, category)| {
                let pct = percent(*add);
                match (size, category.is_empty()) {
                    (0, true) => pct,
                    (0, false) => tf("{pct} ({category})", &[("pct", &pct), ("category", &category.to_ascii_lowercase())]),
                    (s, true) => tf("{pct} to size {size}", &[("pct", &pct), ("size", s)]),
                    (s, false) => tf("{pct} to size {size} ({category})", &[("pct", &pct), ("size", s), ("category", &category.to_ascii_lowercase())]),
                }
            })
            .collect();
        lines.push(tf("Beside it, {what}: {shares}", &[("what", &t(label(a))), ("shares", &list.join(", "))]));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_seam_runs_where_two_footprints_meet() {
        let factory = area((20, 20), (4, 4));
        let plant = area((24, 21), (2, 2));
        let (a, b, dir) = seam(plant, factory).expect("they touch");
        assert_eq!(dir, -Vec3::X, "from the plant to the factory, west");
        assert_eq!((a.x, b.x), (384.0, 384.0));
        assert!(a.z > 336.0 && b.z < 368.0 && a.z < b.z, "{a} {b}");
        assert!(seam(area((24, 24), (2, 2)), factory).is_none(), "a corner alone");
        assert!(seam(area((25, 21), (2, 2)), factory).is_none(), "a cell apart");
        let (_, _, dir) = seam(factory, area((21, 18), (2, 2))).unwrap();
        assert_eq!(dir, -Vec3::Z);
    }

    #[test]
    fn shares_read_as_percentages() {
        let read = |shares: &[f64]| shares.iter().map(|s| percent(*s)).collect::<Vec<_>>();
        assert_eq!(read(&[-0.0625, 0.125, -0.25, -0.015625, -0.140625, 0.5, -0.1875, 0.03125, -0.00625]), ["-6.25%", "+12.5%", "-25%", "-1.56%", "-14.06%", "+50%", "-18.75%", "+3.13%", "-0.63%"]);
        assert_eq!(percent(ta_sim::Fx::from_ratio(-1, 48).to_f64()), "-2.08%", "a share as near as Fx holds it");
        assert_eq!(percent(ta_sim::Fx::from_f64(-0.0125).to_f64()), "-1.25%");
    }
}
