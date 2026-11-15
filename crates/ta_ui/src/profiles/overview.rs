//! A service record at a glance: the tallies that matter, where the career's XP came from, the
//! modes played, the best battles, the favourite units and faction, and the latest battles.

use egui::{Color32, Rect, RichText, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use ta_config::achievements;
use ta_config::profiles::{Career, MatchMode, MatchRecord};

use super::career::{CareerData, duration, mode_label, utc_date};
use crate::i18n::{grouped, n, t, tf};
use crate::theme::*;
use crate::widgets;

/// The career's XP by source, of the XP the head counts ([`Career::total_xp`]): the battles' XP
/// ([`Career::xp`], each battle at its own rate) shared out by what earned it, in proportion to the
/// career's totals as every battle scores them (each battle's own split differs a little: shares,
/// not amounts, are shown), and the achievements' rewards.
pub fn xp_shares(career: &Career) -> Vec<(&'static str, Color32, u64)> {
    let built = career.units_built;
    let sources = [
        (n("Kills"), RED, career.kills.saturating_mul(20)),
        (n("Construction"), AMBER, built.saturating_mul(10).saturating_add(career.buildings_built.min(built).saturating_mul(15))),
        (n("Captures"), SKY, career.units_captured.saturating_mul(30)),
        (n("Economy"), GREEN, career.metal_gathered / 100 + career.energy_gathered / 1000),
        (n("Time played"), STEEL, career.played_seconds / 30 * 5),
        (n("Result"), CREAM, career.matches_finished.saturating_mul(100).saturating_add(career.wins.saturating_mul(300))),
    ];
    let battles = shared_out(career.xp, &sources.map(|(_, _, weight)| weight));
    let mut parts: Vec<_> = sources.iter().zip(battles).map(|(&(label, color, _), xp)| (label, color, xp)).collect();
    parts.push((n("Achievements"), AMBER_BRIGHT, career.achievement_xp()));
    parts.into_iter().filter(|(_, _, xp)| *xp > 0).collect()
}

/// `total` shared out in proportion to `weights`, in whole numbers that add up to it (the largest
/// remainders rounded up); nothing when the weights are nothing.
fn shared_out<const N: usize>(total: u64, weights: &[u64; N]) -> [u64; N] {
    let sum: u128 = weights.iter().map(|w| u128::from(*w)).sum();
    if sum == 0 { return [0; N]; }
    let exact = weights.map(|w| u128::from(total) * u128::from(w));
    let mut out = exact.map(|x| (x / sum) as u64);
    let short = total.saturating_sub(out.iter().sum());
    let mut order: Vec<usize> = (0..N).collect();
    order.sort_by(|a, b| (exact[*b] % sum).cmp(&(exact[*a] % sum)).then(a.cmp(b)));
    for i in order.into_iter().take(short as usize) { out[i] += 1; }
    out
}

pub fn overview(ui: &mut Ui, data: &CareerData) {
    let c = &data.career;
    let earned = achievements::ALL.iter().filter(|a| a.is_unlocked(c)).count();
    let rate = (c.wins * 100).checked_div(c.matches_finished).map_or_else(String::new, |percent| tf("{percent}% won", &[("percent", &percent)]));
    tiles(ui, &[
        (n("BATTLES FINISHED"), grouped(c.matches_finished), String::new()),
        (n("VICTORIES"), grouped(c.wins), rate),
        (n("TIME PLAYED"), duration(c.played_seconds), String::new()),
        (n("ACHIEVEMENTS"), format!("{earned} / {}", achievements::ALL.len()), String::new()),
        (n("CAREER XP"), grouped(c.total_xp()), String::new()),
    ]);
    xp_by_source(ui, c);
    let wide = ui.available_width() >= 620.0;
    let gap = 12.0;
    let half = if wide { (ui.available_width() - gap) / 2.0 } else { ui.available_width() };
    let pair = |ui: &mut Ui, left: &dyn Fn(&mut Ui), right: &dyn Fn(&mut Ui)| {
        if wide {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                ui.vertical(|ui| { ui.set_width(half); left(ui); });
                ui.vertical(|ui| { ui.set_width(half); right(ui); });
            });
        } else {
            left(ui);
            right(ui);
        }
    };
    pair(ui, &|ui: &mut Ui| by_mode(ui, c), &|ui: &mut Ui| best_battles(ui, c));
    pair(ui, &|ui: &mut Ui| favourite_units(ui, c), &|ui: &mut Ui| factions(ui, c));
    recent(ui, &data.history);
    ui.add(egui::Label::new(RichText::new(t("Favourites reflect time played and units constructed. Older matches keep their earned XP; detailed statistics begin when recorded by this version."))
        .color(STEEL).size(12.0)).wrap());
}

/// Big figures in a row of tiles, each with its caption and a note under it.
fn tiles(ui: &mut Ui, values: &[(&str, String, String)]) {
    let width = ui.available_width();
    let cols = if width >= 640.0 { values.len() } else if width >= 300.0 { 3.min(values.len()) } else { 2 };
    let gap = 8.0;
    let tile_w = (width - gap * (cols - 1) as f32) / cols as f32;
    for row in values.chunks(cols) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for (label, value, note) in row {
                let (rect, response) = ui.allocate_exact_size(vec2(tile_w, 66.0), Sense::hover());
                response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, format!("{}: {value}", t(label))));
                let painter = ui.painter();
                painter.rect_filled(rect, 5.0, Color32::from_black_alpha(80));
                painter.rect_stroke(rect, 5.0, Stroke::new(1.0, LINE), StrokeKind::Inside);
                let big = painter.layout_no_wrap(value.clone(), strong(22.0), TEXT_BRIGHT);
                painter.galley(pos2(rect.min.x + 10.0, rect.min.y + 8.0), big, TEXT_BRIGHT);
                let mut job = egui::text::LayoutJob::simple(t(label).to_string(), strong(10.5), STEEL, tile_w - 20.0);
                job.wrap.max_rows = 1;
                job.wrap.overflow_character = Some('…');
                let caption = painter.layout_job(job);
                painter.galley(pos2(rect.min.x + 10.0, rect.max.y - 12.0 - caption.size().y), caption, STEEL);
                if !note.is_empty() {
                    let note = painter.layout_no_wrap(note.clone(), body(11.0), AMBER);
                    painter.galley(pos2(rect.max.x - 10.0 - note.size().x, rect.min.y + 14.0), note, AMBER);
                }
            }
        });
    }
}

fn xp_by_source(ui: &mut Ui, career: &Career) {
    let shares = xp_shares(career);
    let total: u64 = shares.iter().map(|(_, _, xp)| *xp).sum();
    if total == 0 { return; }
    widgets::card(ui, |ui| {
        ui.label(widgets::caption("XP BY SOURCE"));
        let (bar, response) = ui.allocate_exact_size(vec2(ui.available_width(), 14.0), Sense::hover());
        let painter = ui.painter();
        painter.add(crate::metal::well(bar, BG_DEEP));
        let inner = bar.shrink(2.0);
        let mut x = inner.min.x;
        for (_, color, xp) in &shares {
            let w = inner.width() * *xp as f32 / total as f32;
            painter.rect_filled(Rect::from_min_max(pos2(x, inner.min.y), pos2(x + w, inner.max.y)), 0.0, color.gamma_multiply(0.85));
            x += w;
        }
        let text: Vec<String> = shares.iter().map(|(label, _, xp)| match (xp * 100 + total / 2) / total {
            0 => format!("{} <1%", t(label)),
            percent => format!("{} {percent}%", t(label)),
        }).collect();
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, text.join(", ")));
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 14.0;
            for ((_, color, _), line) in shares.iter().zip(&text) {
                let galley = widgets::galley(ui, RichText::new(line).font(body(12.0)).color(TEXT));
                let (rect, _) = ui.allocate_exact_size(vec2(14.0 + galley.size().x, galley.size().y), Sense::hover());
                ui.painter().rect_filled(Rect::from_center_size(pos2(rect.min.x + 4.5, rect.center().y), vec2(9.0, 9.0)), 2.0, *color);
                ui.painter().galley(pos2(rect.min.x + 14.0, rect.min.y), galley, TEXT);
            }
        });
    });
}

fn by_mode(ui: &mut Ui, career: &Career) {
    widgets::card(ui, |ui| {
        ui.label(widgets::caption("BY MODE"));
        let modes = [MatchMode::Campaign, MatchMode::Skirmish, MatchMode::Multiplayer, MatchMode::Scenario, MatchMode::Conquest, MatchMode::Tutorial];
        let played: Vec<_> = modes.iter().filter_map(|m| career.modes.get(m).filter(|s| s.played_seconds > 0 || s.matches_finished > 0).map(|s| (*m, s))).collect();
        if played.is_empty() {
            ui.add(egui::Label::new(RichText::new(t("Play a battle to fill in your record by mode.")).color(STEEL)).wrap());
            return;
        }
        egui::Grid::new("career_modes").num_columns(4).spacing(vec2(14.0, 6.0)).striped(false).show(ui, |ui| {
            for head in [n("Mode"), n("Won"), n("Finished"), n("Time")] {
                ui.label(RichText::new(t(head)).font(strong(10.5)).color(STEEL));
            }
            ui.end_row();
            for (mode, s) in played {
                ui.label(RichText::new(t(mode_label(mode))).font(strong(13.0)).color(TEXT_BRIGHT));
                ui.label(RichText::new(grouped(s.wins)).color(if s.wins > 0 { GREEN } else { TEXT }));
                ui.label(grouped(s.matches_finished));
                ui.label(RichText::new(duration(s.played_seconds)).color(STEEL));
                ui.end_row();
            }
        });
    });
}

fn best_battles(ui: &mut Ui, career: &Career) {
    widgets::card(ui, |ui| {
        ui.label(widgets::caption("BEST BATTLES"));
        let feat = |key: &str| career.feats.get(key).copied().unwrap_or(0);
        let rows = [
            (n("Enemies destroyed"), feat("battle_kills"), ""),
            (n("Units and buildings built"), feat("battle_built"), ""),
            (n("Destroyed by your Commander"), feat("battle_commander_kills"), ""),
            (n("Metal income"), feat("battle_metal_income"), "/s"),
            (n("Energy income"), feat("battle_energy_income"), "/s"),
            (n("Largest army (metal)"), feat("battle_army"), ""),
        ];
        let shown: Vec<_> = rows.iter().filter(|(_, v, _)| *v > 0).collect();
        if shown.is_empty() {
            ui.add(egui::Label::new(RichText::new(t("Your best battles appear here once you have fought a battle from its start.")).color(STEEL)).wrap());
            return;
        }
        for (label, value, unit) in shown {
            ui.horizontal(|ui| {
                ui.label(RichText::new(t(label)).color(TEXT));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(format!("{}{unit}", grouped(*value))).font(strong(14.0)).color(TEXT_BRIGHT));
                });
            });
        }
    });
}

fn favourite_units(ui: &mut Ui, career: &Career) {
    widgets::card(ui, |ui| {
        ui.label(widgets::caption("FAVOURITE UNITS"));
        let mut units: Vec<(&String, &u64)> = career.unit_usage.iter().filter(|(_, n)| **n > 0).collect();
        units.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        if units.is_empty() {
            ui.add(egui::Label::new(RichText::new(t("Build units to discover your favourite unit.")).color(STEEL)).wrap());
            return;
        }
        let most = *units[0].1 as f32;
        for (id, count) in units.into_iter().take(6) {
            let name = career.unit_names.get(id).map_or(id.as_str(), String::as_str);
            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::hover());
            let painter = ui.painter();
            let fill = Rect::from_min_size(rect.min, vec2(rect.width() * (*count as f32 / most).max(0.02), rect.height()));
            painter.rect_filled(fill, 2.0, AMBER_DEEP);
            let text = painter.layout_no_wrap(name.to_string(), body(12.5), TEXT_BRIGHT);
            painter.galley(pos2(rect.min.x + 6.0, rect.center().y - text.size().y / 2.0), text, TEXT_BRIGHT);
            let number = painter.layout_no_wrap(grouped(*count), strong(12.5), AMBER);
            painter.galley(pos2(rect.max.x - 6.0 - number.size().x, rect.center().y - number.size().y / 2.0), number, AMBER);
        }
    });
}

fn factions(ui: &mut Ui, career: &Career) {
    widgets::card(ui, |ui| {
        ui.label(widgets::caption("FACTIONS"));
        let total: u64 = career.faction_usage.values().sum();
        if total == 0 {
            ui.add(egui::Label::new(RichText::new(t("Play a match to discover your favourite faction.")).color(STEEL)).wrap());
            return;
        }
        let mut sides: Vec<_> = career.faction_usage.iter().collect();
        sides.sort_by(|a, b| b.1.cmp(a.1));
        for (side, seconds) in sides.into_iter().take(4) {
            let share = *seconds as f32 / total as f32;
            let color = if side == "ARM" { SKY } else if side == "CORE" { RED } else { AMBER };
            ui.horizontal(|ui| {
                ui.label(RichText::new(side).font(strong(13.0)).color(color));
                ui.label(RichText::new(tf("{time} played", &[("time", &duration(*seconds))])).color(STEEL));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(format!("{:.0}%", share * 100.0)).font(strong(13.0)).color(TEXT_BRIGHT));
                });
            });
            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 6.0), Sense::hover());
            ui.painter().add(crate::metal::well(rect, BG_DEEP));
            ui.painter().rect_filled(Rect::from_min_size(rect.min, vec2(rect.width() * share, rect.height())), 1.0, color.gamma_multiply(0.8));
        }
    });
}

fn recent(ui: &mut Ui, history: &[MatchRecord]) {
    if history.is_empty() { return; }
    widgets::card(ui, |ui| {
        ui.label(widgets::caption("LATEST BATTLES"));
        for record in history.iter().take(5) {
            let c = &record.counters;
            let (label, color) = if c.won { (n("VICTORY"), GREEN) } else if c.finished { (n("DEFEAT"), RED) } else { (n("UNFINISHED"), STEEL) };
            ui.horizontal_wrapped(|ui| {
                widgets::tag(ui, label, color);
                let place = if record.context.map.is_empty() { t(mode_label(record.context.mode)).to_string() }
                    else { format!("{} · {}", t(mode_label(record.context.mode)), record.context.map) };
                ui.label(RichText::new(place).font(strong(13.0)).color(TEXT_BRIGHT));
                ui.label(RichText::new(duration(c.played_seconds)).color(STEEL));
                if record.updated_at != 0 { ui.label(RichText::new(utc_date(record.updated_at)).color(STEEL).size(12.0)); }
                ui.label(RichText::new(tf("+{xp} XP", &[("xp", &grouped(record.earned_xp))])).color(AMBER));
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xp_shares_leave_out_what_earned_nothing() {
        // (Battles at the current rate: three quarters of what each source scored at first.)
        let career = Career { xp: 300, kills: 10, played_seconds: 600, matches_finished: 1, ..Default::default() };
        let shares = xp_shares(&career);
        assert_eq!(shares.iter().map(|(l, _, xp)| (*l, *xp)).collect::<Vec<_>>(), [("Kills", 150), ("Time played", 75), ("Result", 75)]);
    }

    /// The shares are of the XP the head counts: the battles' XP as the career has it (whatever
    /// the rates its battles were scored at), and the achievements' rewards whole.
    #[test]
    fn xp_shares_add_up_to_the_career_xp_the_head_shows() {
        let mut career = Career { xp: 11_882, kills: 300, units_built: 420, buildings_built: 90, units_captured: 2,
            metal_gathered: 60_000, energy_gathered: 700_000, played_seconds: 9_000, matches_finished: 9, wins: 5, ..Default::default() };
        for id in ["career_kills_1", "career_kills_100", "career_built_1", "career_buildings_1", "career_captures_1", "rank_2", "rank_10"] {
            assert!(achievements::find(id).is_some(), "{id}");
            career.achievements.insert(id.into(), 1);
        }
        let shares = xp_shares(&career);
        let total: u64 = shares.iter().map(|(_, _, xp)| *xp).sum();
        assert_eq!(total, career.total_xp());
        let battles: u64 = shares.iter().filter(|(l, _, _)| *l != "Achievements").map(|(_, _, xp)| *xp).sum();
        assert_eq!(battles, career.xp, "the battles' XP as recorded, not recounted at the first rate");
        let earned = shares.iter().find(|(l, _, _)| *l == "Achievements").unwrap().2;
        assert_eq!(earned, career.achievement_xp());
        assert_eq!(shared_out(10, &[1, 1, 1]), [4, 3, 3]);
        assert_eq!(shared_out(7, &[0, 0]), [0, 0]);
        assert_eq!(shared_out(u64::MAX / 2, &[u64::MAX, 1]).iter().map(|x| u128::from(*x)).sum::<u128>(), u128::from(u64::MAX / 2));
    }
}
