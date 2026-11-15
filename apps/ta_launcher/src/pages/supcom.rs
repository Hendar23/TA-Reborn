//! The dialog that imports Supreme Commander: Forged Alliance's maps (`crate::supcom`): where the
//! game is, its maps (and the player's own) with the game's pictures of them to choose from, and
//! the import under way, map by map. The list draws only the rows in sight (each map's picture
//! uploaded once), and the buttons stay in sight under it however small the window.

use egui::{Color32, CornerRadius, RichText, Sense, Stroke, StrokeKind, Ui, vec2};
use ta_ui::i18n::{n, t, tf};
use ta_ui::panel::{Exit, Panel};
use ta_ui::tooltip::TooltipExt as _;

use super::path_label;
use crate::app::{Action, LauncherApp};
use crate::supcom::{self, State};
use crate::theme::*;
use crate::widgets::{self, ActionButton};

/// A map's picture in the list (points).
const THUMB: f32 = 56.0;
/// A map's card, and so a row of the list, this tall.
const ROW: f32 = THUMB + 8.0;
/// The narrowest a map's card is; as many as fit stand in a row.
const CARD: f32 = 250.0;
const GAP: f32 = 8.0;
/// The footer's height until it has been laid out (its row of buttons).
const FOOTER: f32 = 44.0;
/// The tallest the footer's list of maps that could not be made grows (it scrolls beyond).
const FAILED_HEIGHT: f32 = 96.0;
/// The tallest the list of what could not be read grows (it scrolls beyond).
const SKIPPED_HEIGHT: f32 = 120.0;

/// What the dialog's footer shows, taken before it is drawn (apart from the body, which the
/// dialog draws first).
struct Footer {
    importing: bool,
    progress: supcom::Progress,
    outcome: Option<supcom::Outcome>,
    /// There are maps to choose from: the game found, its maps listed.
    listed: bool,
    chosen: usize,
    blocked: Option<&'static str>,
}

/// What the footer was asked to do.
#[derive(Default)]
struct Asked {
    close: bool,
    actions: Vec<Action>,
}

impl LauncherApp {
    /// Why the chosen maps cannot be imported now, if they cannot.
    pub(crate) fn supcom_blocked(&self) -> Option<&'static str> {
        if self.detection.install().is_none() {
            Some(t("Find your Total Annihilation install on the Play page first."))
        } else if self.game_running() {
            Some(t("Close the game first: maps cannot be imported while it runs."))
        } else if self.installer.busy() || self.maps.importing() {
            Some(t("Wait for the mod or the maps being installed, then import these."))
        } else {
            None
        }
    }

    pub(crate) fn supcom_dialog(&mut self, ctx: &egui::Context) {
        if !self.supcom.open {
            return;
        }
        let width = (ctx.content_rect().width() - 80.0).clamp(1.0, 880.0);
        let height = (ctx.content_rect().height() - 24.0).max(1.0);
        let title = t("SUPREME COMMANDER MAPS").to_string();
        let importing = self.supcom.importing();
        let found = self.supcom.found.clone();
        let footer = Footer {
            importing,
            progress: self.supcom.progress.lock().unwrap().clone(),
            outcome: self.supcom.outcome.clone(),
            listed: found.as_ref().is_some_and(|found| found.install.is_some() && !found.maps.is_empty()),
            chosen: found.as_ref().map_or(0, |found| found.maps.iter().filter(|e| self.supcom.is_chosen(&e.folder)).count()),
            blocked: self.supcom_blocked(),
        };
        // (The footer as tall as it came out last frame: its buttons stay in sight under a body
        // that scrolls.)
        let footer_id = egui::Id::new("launcher_supcom_footer");
        let footer_height = ctx.data(|d| d.get_temp::<f32>(footer_id)).unwrap_or(FOOTER);
        let mut actions = Vec::new();
        let mut asked = Asked::default();
        let panel = Panel::new(&title).exit(Exit::Close).width(width).max_height(height);
        let response = ta_ui::panel::modal_with_footer(ctx, "launcher_supcom", panel, true, footer_height, |ui| self.supcom_body(ui, importing, &mut actions), |ui| {
            let laid = ui.scope(|ui| supcom_footer(ui, &footer, &mut asked)).response.rect.height();
            if (laid - footer_height).abs() > 0.5 {
                ui.ctx().data_mut(|d| d.insert_temp(footer_id, laid));
                ui.ctx().request_discard("the Supreme Commander dialog's footer changed its height");
            }
        });
        if response.exit || asked.close {
            self.supcom.open = false;
        }
        for a in actions.into_iter().chain(asked.actions) {
            self.push(a);
        }
    }

    /// The dialog's body: what it does, where the game is, and its maps to choose from.
    fn supcom_body(&mut self, ui: &mut Ui, importing: bool, actions: &mut Vec<Action>) {
        ui.spacing_mut().item_spacing.y = 6.0;
        ui.add(egui::Label::new(RichText::new(t("Supreme Commander: Forged Alliance's maps, made into TA Reborn maps from your own copy of the game to play in skirmish: their land and ground, forests and rocks, mass points and starting places come with them.")).color(TEXT)).wrap());
        ui.add_space(space::S);
        self.supcom_source(ui, importing, actions);
        let Some(found) = self.supcom.found.clone() else { return };
        if found.install.is_none() {
            return;
        }
        ui.add_space(space::S);
        if found.maps.is_empty() {
            ui.add(egui::Label::new(RichText::new(t("No skirmish maps were found in this copy of the game.")).color(AMBER)).wrap());
            return;
        }
        // How many are chosen, and what they take.
        let chosen: Vec<&supcom::Entry> = found.maps.iter().filter(|e| self.supcom.is_chosen(&e.folder)).collect();
        let gb = supcom::size_estimate(&chosen) as f64 / 1e9;
        let count = chosen.len();
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            if ui.add_enabled(!importing, egui::Button::new(RichText::new(t("SELECT ALL")).font(strong(11.5)))).clicked() {
                self.supcom.choose_all(true);
            }
            if ui.add_enabled(!importing, egui::Button::new(RichText::new(t("SELECT NONE")).font(strong(11.5)))).clicked() {
                self.supcom.choose_all(false);
            }
            let summary = if gb >= 0.1 {
                tf("{n} of {total} maps chosen, about {size} GB on the disk", &[("n", &count), ("total", &found.maps.len()), ("size", &one_decimal(gb))])
            } else {
                tf("{n} of {total} maps chosen", &[("n", &count), ("total", &found.maps.len())])
            };
            ui.label(RichText::new(summary).color(STEEL));
        });
        ui.add_space(space::XS);
        // The list fills the room the body has left (two rows at least: a short window's body
        // scrolls, the buttons under it in sight).
        let room = ui.available_height().max(2.0 * ROW + GAP);
        self.supcom_list(ui, &found, importing, room);
    }

    /// Where the game is (or that it was not found), and the player's own maps' folders.
    fn supcom_source(&mut self, ui: &mut Ui, importing: bool, actions: &mut Vec<Action>) {
        if self.supcom.scanning() && self.supcom.found.is_none() {
            widgets::busy_line(ui, t("Looking for Supreme Commander: Forged Alliance…"));
            return;
        }
        let found = self.supcom.found.clone().unwrap_or_default();
        ui.horizontal_wrapped(|ui| {
            ui.label(widgets::caption("FORGED ALLIANCE"));
            if found.copies.len() > 1 {
                // (More than one copy found: each a switch, the one in use lit. Their maps make
                // the same TA maps.)
                for copy in &found.copies {
                    let on = found.install.as_ref().is_some_and(|game| ta_supcom::install::same_folder(&game.root, &copy.root));
                    let chip = ui.add_enabled_ui(!importing, |ui| widgets::chip(ui, store_label(copy.store), on)).inner;
                    if chip.hover_tip(copy.root.display().to_string()).clicked() && !on {
                        actions.push(Action::SupcomUse(copy.root.clone()));
                    }
                }
            } else if let Some(store) = found.install.as_ref().and_then(|game| game.store.name()) {
                widgets::tag(ui, store, GREEN);
            }
            let label = if found.install.is_some() { n("CHANGE FOLDER…") } else { n("CHOOSE FOLDER…") };
            if ui.add_enabled(!importing, ActionButton::secondary(label).height(26.0)).hover_tip(t("Choose the folder Supreme Commander: Forged Alliance is installed in.")).clicked() {
                actions.push(Action::SupcomBrowse);
            }
        });
        match &found.install {
            Some(game) => {
                path_label(ui, &game.root, TEXT);
            }
            None => {
                ui.add(egui::Label::new(RichText::new(t("Supreme Commander: Forged Alliance was not found. TA Reborn looks for GOG's and Steam's copies of it: if yours is elsewhere, choose its folder.")).color(AMBER)).wrap());
            }
        }
        if let Some(error) = &found.error {
            ui.add(egui::Label::new(RichText::new(error).color(RED)).wrap());
        }
        if found.install.is_some() {
            ui.add_space(space::XS);
            ui.label(widgets::caption("YOUR OWN MAPS"));
            if found.own_folders.is_empty() {
                ui.add(egui::Label::new(RichText::new(t("None found (downloaded maps and Forged Alliance Forever's are listed when they are in your Documents folder).")).color(STEEL)).wrap());
            }
            for folder in &found.own_folders {
                path_label(ui, folder, TEXT);
            }
            // (What could not be read is said, each with why: it is not listed.)
            if !found.skipped.is_empty() {
                ui.collapsing(RichText::new(tf("{n} maps could not be read", &[("n", &found.skipped.len())])).color(AMBER), |ui| {
                    egui::ScrollArea::vertical().id_salt("supcom_skipped").max_height(SKIPPED_HEIGHT).auto_shrink([false, true]).show(ui, |ui| {
                        for line in &found.skipped {
                            ui.add(egui::Label::new(RichText::new(line).color(AMBER)).wrap());
                        }
                    });
                });
            }
        }
    }

    /// The maps to choose from, cards in rows, `height` tall at most: only the rows in sight (and
    /// one beyond, for the keys to reach) are drawn.
    fn supcom_list(&mut self, ui: &mut Ui, found: &supcom::Found, importing: bool, height: f32) {
        // (As many cards in a row as fit beside the scroll lane.)
        let width = (ui.available_width() - ta_ui::scroll::LANE).max(1.0);
        let columns = (((width + GAP) / (CARD + GAP)).floor() as usize).max(1);
        let rows = found.maps.len().div_ceil(columns);
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing = vec2(GAP, GAP);
            let area = egui::ScrollArea::vertical().id_salt("supcom_maps").max_height(height).auto_shrink([false, true]);
            ta_ui::scroll::show_rows(ui, area, ta_ui::scroll::Behind::Plate, ROW, rows, |ui, range| {
                let card = ((ui.available_width() - GAP * (columns - 1) as f32) / columns as f32).max(1.0);
                for row in range {
                    ui.horizontal(|ui| {
                        for entry in found.maps.iter().skip(row * columns).take(columns) {
                            self.supcom_row(ui, entry, importing, card);
                        }
                    });
                }
            });
        });
    }

    /// A map of the list: its box, the game's picture of it, its name, size and players, and
    /// whether it was imported before. A click anywhere on it ticks its box; only the box takes
    /// the keys, named with all the card says.
    fn supcom_row(&mut self, ui: &mut Ui, entry: &supcom::Entry, importing: bool, width: f32) {
        let mut chosen = self.supcom.is_chosen(&entry.folder);
        let (rect, response) = ui.allocate_exact_size(vec2(width, ROW), Sense::CLICK);
        let hovered = response.hovered() && !importing;
        let painter = ui.painter_at(rect);
        if hovered || chosen {
            painter.rect(rect, CornerRadius::same(3), if chosen { Color32::from_rgba_unmultiplied(196, 160, 82, 18) } else { Color32::from_white_alpha(6) }, Stroke::new(1.0, if chosen { AMBER.gamma_multiply(0.5) } else { STEEL.gamma_multiply(0.3) }), StrokeKind::Inside);
        }
        let name = &entry.scenario.name;
        let mut detail = tf("{size} · {n} players", &[("size", &supcom::km(&entry.scenario)), ("n", &entry.scenario.armies.len())]).to_string();
        if entry.players_own {
            detail.push_str(" · ");
            detail.push_str(t("your own map"));
        }
        let mark = entry.state.label().map(|label| (t(label), if entry.state == State::Imported { GREEN } else { AMBER }));
        let spoken = match mark {
            Some((mark, _)) => format!("{name}, {detail}, {mark}"),
            None => format!("{name}, {detail}"),
        };
        // The box.
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(egui::Rect::from_min_size(rect.min + vec2(8.0, (rect.height() - 20.0) / 2.0), vec2(20.0, 20.0))));
        let toggled = child.add_enabled_ui(!importing, |ui| widgets::check_box_named(ui, &mut chosen, &spoken)).inner.changed();
        // The picture.
        let thumb = egui::Rect::from_min_size(rect.min + vec2(38.0, 4.0), vec2(THUMB, THUMB));
        match self.supcom.texture(ui.ctx(), &entry.folder) {
            Some(texture) => painter.image(texture.id(), thumb, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE),
            None => painter.rect_filled(thumb, CornerRadius::same(2), Color32::from_black_alpha(120)),
        };
        painter.rect_stroke(thumb, CornerRadius::same(2), Stroke::new(1.0, STEEL.gamma_multiply(0.5)), StrokeKind::Inside);
        // Its mark at the top right; its name in the room the mark leaves beside it, its size,
        // players and where it came from under them.
        let text_x = thumb.right() + 12.0;
        let room = (rect.right() - text_x - 8.0).max(1.0);
        let mark = mark.map(|(label, color)| {
            let galley = egui::WidgetText::from(RichText::new(label).font(strong(9.5)).color(color)).into_galley(ui, Some(egui::TextWrapMode::Truncate), room, egui::TextStyle::Body);
            (galley, color)
        });
        let title_room = mark.as_ref().map_or(room, |(galley, _)| (room - galley.size().x - 8.0).max(1.0));
        let title = egui::WidgetText::from(RichText::new(name.as_str()).font(strong(14.0)).color(TEXT_BRIGHT)).into_galley(ui, Some(egui::TextWrapMode::Truncate), title_room, egui::TextStyle::Body);
        painter.galley(egui::pos2(text_x, rect.top() + 10.0), title, TEXT_BRIGHT);
        let line = egui::WidgetText::from(RichText::new(detail).font(body(12.5)).color(STEEL)).into_galley(ui, Some(egui::TextWrapMode::Truncate), room, egui::TextStyle::Body);
        painter.galley(egui::pos2(text_x, rect.top() + 34.0), line, STEEL);
        if let Some((galley, color)) = mark {
            let at = egui::pos2(rect.right() - galley.size().x - 8.0, rect.top() + 6.0);
            painter.galley(at, galley, color);
        }
        let response = if entry.scenario.description.trim().is_empty() { response } else { response.on_hover_text(entry.scenario.description.clone()) };
        if (response.clicked() && !importing) || toggled {
            if response.clicked() {
                chosen = !chosen;
            }
            self.supcom.choose(&entry.folder, chosen);
        }
    }
}

/// The import under way (map by map), or the button to start one; what the last came to.
fn supcom_footer(ui: &mut Ui, footer: &Footer, asked: &mut Asked) {
    ui.spacing_mut().item_spacing.y = 6.0;
    if footer.importing {
        let p = &footer.progress;
        let fraction = if p.total == 0 { None } else { Some(((p.done as f32 + p.share.clamp(0.0, 1.0)) / p.total as f32).clamp(0.0, 1.0)) };
        let label = tf("Making {map} ({i} of {n})…", &[("map", &p.current), ("i", &(p.done + 1).min(p.total)), ("n", &p.total)]);
        let width = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(vec2(width, 10.0), Sense::hover());
        ta_ui::progress::paint(ui, rect, fraction, true);
        ui.label(RichText::new(label).color(TEXT));
        if ui.add(ActionButton::secondary("STOP AFTER THIS MAP")).hover_tip(t("The maps made so far are kept.")).clicked() {
            asked.actions.push(Action::SupcomStop);
        }
        return;
    }
    if let Some(outcome) = &footer.outcome {
        ui.add(egui::Label::new(RichText::new(outcome.summary()).color(outcome.went().colour())).wrap());
        if !outcome.failed.is_empty() {
            ui.collapsing(tf("{n} maps could not be made", &[("n", &outcome.failed.len())]), |ui| {
                egui::ScrollArea::vertical().id_salt("supcom_failed").max_height(FAILED_HEIGHT).auto_shrink([false, true]).show(ui, |ui| {
                    for (name, why) in &outcome.failed {
                        ui.label(RichText::new(name).font(strong(12.5)).color(TEXT));
                        ui.add(egui::Label::new(RichText::new(why).color(AMBER)).wrap());
                    }
                });
            });
        }
    }
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        if footer.listed {
            let label = tf("IMPORT {n} MAPS", &[("n", &footer.chosen)]);
            let can = footer.chosen > 0 && footer.blocked.is_none();
            if ui.add_enabled(can, ActionButton::primary(&label)).hover_tip(t("Makes the chosen maps into TA Reborn maps, in a map pack of their own. Most take seconds, the largest a few minutes.")).disabled_tip(footer.blocked.unwrap_or_else(|| t("Choose the maps to import."))).clicked() {
                asked.actions.push(Action::SupcomImport);
            }
        }
        if ui.add(ActionButton::secondary("CLOSE")).clicked() {
            asked.close = true;
        }
    });
}

/// `value` with one decimal, its point as the language in use writes it: a comma where its
/// thousands are grouped otherwise than with commas (`ta_ui::i18n::grouped`'s languages).
fn one_decimal(value: f64) -> String {
    one_decimal_with(value, ta_ui::i18n::grouped(1000).contains(','))
}

/// `value` with one decimal, after a point (`point`) or a comma.
fn one_decimal_with(value: f64, point: bool) -> String {
    let text = format!("{value:.1}");
    if point { text } else { text.replace('.', ",") }
}

/// What a copy of the game is called on its switch: its store's name, or what it is.
fn store_label(store: ta_supcom::install::Store) -> &'static str {
    store.name().unwrap_or(match store {
        ta_supcom::install::Store::Retail => n("Boxed copy"),
        _ => n("Folder"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use egui::{Pos2, Rect};
    use std::path::PathBuf;

    fn launcher(name: &str) -> (PathBuf, egui::Context, LauncherApp) {
        let root = std::env::temp_dir().join(format!("ta-supcom-dialog-{name}-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir_all(&root).unwrap();
        let ctx = egui::Context::default();
        let args = crate::Args::parse_from(["ta-launcher", "--offline", "--pretend-missing", "--config", root.join("config.toml").to_str().unwrap()]);
        let mut app = LauncherApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()), args);
        app.skins.shutdown();
        ta_ui::motion::set_reduced(&ctx, true);
        (root, ctx, app)
    }

    /// `count` of the game's maps, every third imported, the first named at length; a picture
    /// each.
    fn found(count: usize) -> (supcom::Found, supcom::Previews) {
        let maps: Vec<supcom::Entry> = (0..count).map(|i| supcom::Entry {
            folder: PathBuf::from(format!("C:/Game/maps/SCMP_{i:03}")),
            players_own: false,
            scenario: ta_supcom::map::Scenario {
                folder: format!("SCMP_{i:03}"),
                name: if i == 0 { "A Very Long Name For A Map Of The Game".into() } else { format!("Map {i:03}") },
                size: (512, 512),
                armies: vec!["ARMY_1".into(), "ARMY_2".into()],
                ..Default::default()
            },
            state: if i % 3 == 0 { State::Imported } else { State::New },
        }).collect();
        let previews = maps.iter().map(|e| (e.folder.clone(), egui::ColorImage::filled([8, 8], Color32::RED))).collect();
        let install = ta_supcom::install::Install { root: PathBuf::from("C:/Game"), store: ta_supcom::install::Store::Gog };
        (supcom::Found { install: Some(install), maps, ..Default::default() }, previews)
    }

    /// A frame of the dialog, `size` points large: the words drawn, where, and what they are
    /// clipped to.
    fn frame(ctx: &egui::Context, app: &mut LauncherApp, size: egui::Vec2, events: Vec<egui::Event>) -> (egui::FullOutput, Vec<(String, Rect, Rect)>) {
        app.supcom.poll(ctx);
        let input = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)), events, ..Default::default() };
        let mut output = ctx.run_ui(input, |ui| app.supcom_dialog(ui.ctx()));
        output.textures_delta.clear();
        let mut texts = Vec::new();
        fn walk(shape: &egui::Shape, clip: Rect, into: &mut Vec<(String, Rect, Rect)>) {
            match shape {
                egui::Shape::Text(text) => into.push((text.galley.text().to_string(), text.galley.rect.translate(text.pos.to_vec2()), clip)),
                egui::Shape::Vec(shapes) => for shape in shapes { walk(shape, clip, into); },
                _ => {}
            }
        }
        for shape in &output.shapes { walk(&shape.shape, shape.clip_rect, &mut texts); }
        (output, texts)
    }

    /// In a small window (940 × 600 at 1.3 times, as reported) the buttons stay in sight under
    /// the list, which scrolls.
    #[test]
    fn the_dialogs_buttons_stay_in_sight_in_a_small_window() {
        let (root, ctx, mut app) = launcher("small");
        app.supcom.open = true;
        let (found, previews) = found(54);
        app.supcom.pretend_found(found, previews);
        let size = egui::vec2(723.0, 461.0);
        let screen = Rect::from_min_size(Pos2::ZERO, size);
        for _ in 0..4 { frame(&ctx, &mut app, size, Vec::new()); }
        let (_, texts) = frame(&ctx, &mut app, size, Vec::new());
        for words in ["IMPORT 36 MAPS", "CLOSE"] {
            let (_, rect, clip) = texts.iter().find(|(text, _, _)| text == words).unwrap_or_else(|| panic!("{words}: {:?}", texts.iter().map(|t| &t.0).collect::<Vec<_>>()));
            assert!(screen.contains_rect(*rect) && clip.contains_rect(*rect), "{words} in sight: {rect:?}, clipped to {clip:?}");
        }
        drop(app);
        let _ = std::fs::remove_dir_all(root);
    }

    /// With 400 maps the list draws the rows in sight, not all of them, each map's picture made a
    /// texture once (its pixels let go of then), and none kept once the dialog is closed.
    #[test]
    fn the_list_draws_the_rows_in_sight_and_lets_its_pictures_go_when_closed() {
        let (root, ctx, mut app) = launcher("rows");
        app.supcom.open = true;
        let (found, previews) = found(400);
        app.supcom.pretend_found(found, previews);
        let size = egui::vec2(1280.0, 800.0);
        for _ in 0..3 { frame(&ctx, &mut app, size, Vec::new()); }
        let (_, texts) = frame(&ctx, &mut app, size, Vec::new());
        let drawn = texts.iter().filter(|(text, _, _)| text.starts_with("Map ")).count();
        assert!(drawn > 0 && drawn < 60, "{drawn} of 400 maps drawn");
        let (pixels, textures) = app.supcom.pictures_held();
        assert!(textures > 0 && textures < 60 && pixels + textures == 400, "{pixels} pictures as pixels, {textures} as textures");
        // Closed with its cross: nothing held.
        let (_, texts) = frame(&ctx, &mut app, size, Vec::new());
        let (_, close, _) = texts.iter().find(|(text, _, _)| text == "CLOSE").unwrap().clone();
        let click = |at: Pos2| vec![egui::Event::PointerMoved(at), egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() }];
        frame(&ctx, &mut app, size, click(close.center()));
        frame(&ctx, &mut app, size, vec![egui::Event::PointerButton { pos: close.center(), button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() }]);
        assert!(!app.supcom.open, "CLOSE closes it");
        app.supcom.poll(&ctx);
        assert_eq!(app.supcom.pictures_held(), (0, 0));
        drop(app);
        let _ = std::fs::remove_dir_all(root);
    }

    /// The keys stop on each map once, at its box (not at its card as well), whose name says all
    /// the card shows: its name, size, players and whether it was imported. A long name gives the
    /// mark room.
    #[test]
    fn each_map_is_one_named_stop_for_the_keys_and_its_mark_has_room() {
        let (root, ctx, mut app) = launcher("keys");
        app.supcom.open = true;
        let (found, previews) = found(6);
        app.supcom.pretend_found(found, previews);
        let size = egui::vec2(820.0, 900.0);
        for _ in 0..3 { frame(&ctx, &mut app, size, Vec::new()); }
        let (_, texts) = frame(&ctx, &mut app, size, Vec::new());
        let (_, title, _) = texts.iter().find(|(text, _, _)| text == "A Very Long Name For A Map Of The Game").expect("the long name").clone();
        let (_, mark, _) = texts.iter().filter(|(text, _, _)| text == "IMPORTED").min_by(|a, b| (a.1.top() - title.top()).abs().total_cmp(&(b.1.top() - title.top()).abs())).expect("its mark").clone();
        assert!(title.right() <= mark.left(), "the name ({title:?}) stops short of its mark ({mark:?})");
        let tab = egui::Event::Key { key: egui::Key::Tab, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE };
        let mut boxes = Vec::new();
        for _ in 0..30 {
            let (output, _) = frame(&ctx, &mut app, size, vec![tab.clone()]);
            if let Some(focused) = ctx.memory(|m| m.focused()).and_then(|id| ctx.read_response(id)) {
                assert!((focused.rect.height() - ROW).abs() > 0.5, "a map's card took the keys: {:?}", focused.rect);
            }
            for event in &output.platform_output.events {
                if let egui::output::OutputEvent::FocusGained(info) = event
                    && info.typ == egui::WidgetType::Checkbox
                    && let Some(label) = &info.label
                    && !boxes.contains(label)
                {
                    boxes.push(label.clone());
                }
            }
        }
        assert_eq!(boxes.len(), 6, "{boxes:?}");
        assert!(boxes.iter().any(|label| label.starts_with("A Very Long Name For A Map Of The Game, 10 km · 2 players, IMPORTED")), "{boxes:?}");
        assert!(boxes.iter().any(|label| label == "Map 001, 10 km · 2 players"), "{boxes:?}");
        drop(app);
        let _ = std::fs::remove_dir_all(root);
    }

    /// "About 1.2 GB" in English, "etwa 1,2 GB" in German (whose thousands are grouped with
    /// points).
    #[test]
    fn sizes_are_written_with_the_languages_decimal_point() {
        assert_eq!(one_decimal(1.234), "1.2", "English");
        assert_eq!(one_decimal_with(9.68, false), "9,7");
    }
}
