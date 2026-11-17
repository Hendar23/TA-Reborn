//! Enhancements in words (`[ENHANCEMENTS]`: Supreme Commander's commanders'): what each does, a
//! line each, for its tip in the build menu's ENHANCE tab, the information page and the classic
//! interface's page; and what the player may do with it now.

use ta_sim::{EnhancementFit as Fit, Fx, Unit, UnitType};
use ta_ui::i18n::{t, tf};

use crate::game::Game;

/// What a click on an enhancement asks: install it (queued after the unit's orders), or take it
/// out (an installed one).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EnhanceClick {
    Install { index: u16, queue: bool },
    Remove { index: u16 },
}

/// An enhancement's name (its id where it has none).
pub(crate) fn name(ty: &UnitType, index: u16) -> String {
    ty.def.enhancements.get(index as usize).map_or_else(String::new, |e| e.name.clone())
}

/// A whole number, as the figures are shown.
fn whole(v: Fx) -> String {
    format!("{:.0}", v.to_f32())
}

/// What enhancement `index` of `ty` does, a line each: its effects as the engine plays them.
pub(crate) fn effects(game: &Game, ty: &UnitType, index: u16) -> Vec<String> {
    let Some(e) = ty.enhancements.get(index as usize) else { return Vec::new() };
    let sim = &game.sim;
    let mut out = Vec::new();
    if !e.build_options.is_empty() {
        let names: Vec<String> = e.build_options.iter().take(4).map(|&o| crate::chat::unit_label(sim, o)).collect();
        let more = e.build_options.len().saturating_sub(4);
        out.push(if more > 0 {
            tf("Builds {n} more kinds: {names} and {more} others", &[("n", &e.build_options.len()), ("names", &names.join(", ")), ("more", &more)])
        } else {
            tf("Builds {n} more kinds: {names}", &[("n", &e.build_options.len()), ("names", &names.join(", "))])
        });
    }
    if e.work_rate > 0 {
        out.push(tf("Build power +{n}", &[("n", &whole(Fx(e.work_rate as i64)))]));
    }
    if e.max_health > Fx::ZERO {
        out.push(tf("Health +{n}", &[("n", &whole(e.max_health))]));
    }
    if e.heal_rate > Fx::ZERO {
        out.push(tf("Mends {n} hit points a second", &[("n", &whole(e.heal_rate))]));
    }
    for slot in 0..e.weapons.len() {
        let Some(w) = e.weapons[slot] else { continue };
        let w = sim.weapon_types.get(w);
        let label = crate::perspective::weapon_name(w);
        let reach = whole(w.range);
        match ty.weapons[slot] {
            Some(old) if old != w.id => {
                let old = sim.weapon_types.get(old);
                if w.range != old.range {
                    out.push(tf("{weapon}: range {from} to {to}", &[("weapon", &label), ("from", &whole(old.range)), ("to", &reach)]));
                } else {
                    out.push(tf("{weapon}: stronger", &[("weapon", &label)]));
                }
            }
            Some(_) => {}
            None => out.push(tf("Weapon: {weapon}, range {range}", &[("weapon", &label), ("range", &reach)])),
        }
    }
    if e.sight > Fx::ZERO {
        out.push(tf("Sight +{n}", &[("n", &whole(e.sight))]));
    }
    if e.radar > Fx::ZERO {
        out.push(tf("Radar +{n}", &[("n", &whole(e.radar))]));
    }
    if e.sonar > Fx::ZERO {
        out.push(tf("Sonar +{n}", &[("n", &whole(e.sonar))]));
    }
    if let Some((cost, _, _)) = e.cloak {
        out.push(tf("Cloaks for {rate} energy a second", &[("rate", &whole(cost))]));
    }
    if e.stealth {
        out.push(t("Stealthy: not on radar").to_string());
    }
    out
}

/// The slot an enhancement takes, by name.
pub(crate) fn slot_name(ty: &UnitType, index: u16) -> String {
    ty.enhancements.get(index as usize).and_then(|e| ty.enhancement_slots.get(e.slot as usize)).cloned().unwrap_or_default()
}

/// How long `u` takes to install it on its own build power.
pub(crate) fn seconds(u: &Unit, ty: &UnitType, index: u16) -> f32 {
    let Some(e) = ty.enhancements.get(index as usize) else { return 0.0 };
    let rate = u.work_rate(ty).max(1);
    (e.build_work as f64 / rate as f64 / 30.0) as f32
}

/// An enhancement's tip: its name, slot, price and time, what it needs, what it does, and what a
/// click does now.
pub(crate) fn tip(game: &Game, u: &Unit, ty: &UnitType, index: u16) -> String {
    let Some(e) = ty.enhancements.get(index as usize) else { return String::new() };
    let mut lines = vec![name(ty, index)];
    if let Some(d) = ty.def.enhancements.get(index as usize).map(|d| d.description.trim()).filter(|d| !d.is_empty()) {
        lines.push(d.to_string());
    }
    lines.push(tf(
        "Slot {slot}  ·  Metal {metal}  ·  Energy {energy}  ·  {duration} on its own build power",
        &[("slot", &slot_name(ty, index)), ("metal", &whole(e.cost_metal)), ("energy", &whole(e.cost_energy)), ("duration", &super::duration(seconds(u, ty, index)))],
    ));
    if let Some(p) = e.prerequisite {
        lines.push(tf("Needs {name} first", &[("name", &name(ty, p))]));
    }
    lines.extend(effects(game, ty, index));
    lines.push(match game.sim.enhancement_fit(u, index) {
        Fit::Installed => t("Installed  ·  Right-click: take it out").to_string(),
        Fit::Installing => t("Being installed  ·  Stop calls it off, its metal back").to_string(),
        Fit::Needs(p) => tf("Click: {name} first, then this one  ·  Shift: queued", &[("name", &name(ty, p))]),
        Fit::Free { replaces: Some(r) } if Some(r) != e.prerequisite => tf("Click: install it in place of {name}  ·  Shift: queued", &[("name", &name(ty, r))]),
        Fit::Free { .. } => t("Click: install it (the unit stands meanwhile)  ·  Shift: queued").to_string(),
        Fit::No => String::new(),
    });
    lines.retain(|l| !l.is_empty());
    lines.join("\n")
}

/// The enhancements of `u` (of type `ty`) as a list, a row each: its name, slot and price,
/// marked where installed, its progress while being installed, dimmed where it may not be
/// installed now (the classic interface's page of them). A click installs one (Shift: queued;
/// one whose prerequisite is missing, after it), a right-click takes an installed one out.
pub(crate) fn list(ui: &mut egui::Ui, game: &Game, u: &Unit, ty: &UnitType) -> Option<EnhanceClick> {
    let mut click = None;
    let progress = game.sim.self_work_progress(u);
    for (k, e) in ty.enhancements.iter().enumerate() {
        let index = k as u16;
        let fit = game.sim.enhancement_fit(u, index);
        let line = tf(
            "{name}  ·  {slot}  ·  metal {metal}, energy {energy}",
            &[("name", &name(ty, index)), ("slot", &slot_name(ty, index)), ("metal", &whole(e.cost_metal)), ("energy", &whole(e.cost_energy))],
        );
        let line = match fit {
            Fit::Installed => tf("{line}  ·  installed", &[("line", &line)]),
            Fit::Installing => {
                let done = progress.filter(|(j, _)| *j == ta_sim::SelfJob::Enhance(index)).map_or(0.0, |(_, d)| d.to_f32().clamp(0.0, 1.0));
                tf("{line}  ·  {pct}%", &[("line", &line), ("pct", &format!("{:.0}", (done * 100.0).floor()))])
            }
            _ => line,
        };
        let enabled = matches!(fit, Fit::Free { .. } | Fit::Needs(_) | Fit::Installed);
        let on = matches!(fit, Fit::Installed | Fit::Installing);
        let resp = ui.add_enabled(enabled, egui::Button::selectable(on, line)).on_hover_text(tip(game, u, ty, index));
        if resp.clicked() && matches!(fit, Fit::Free { .. } | Fit::Needs(_)) {
            click = Some(EnhanceClick::Install { index, queue: ui.input(|i| i.modifiers.shift) });
        } else if resp.secondary_clicked() && fit == Fit::Installed {
            click = Some(EnhanceClick::Remove { index });
        }
    }
    click
}
