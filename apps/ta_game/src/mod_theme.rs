//! A mod's interface theme in a battle (its manifest's `[theme]`, `ta_vfs::mods::Theme`): the
//! battle's palette (`ta_ui::theme::set_palette`, which both HUDs and egui's own widgets draw
//! with), the modern HUD's panel and bar pictures (`hud_skin`) and its titles' font
//! (`ta_ui::theme::set_battle_font`). The theme is the last mod's in the load order that has one;
//! the side it is drawn for is that of the player whose interface is shown (a spectator's, as
//! they follow another, theirs). Off the battlefield, or with the player's MODS' INTERFACE THEMES
//! off, TA Reborn's own.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use ta_ui::theme::{Palette, parse_colour};
use ta_vfs::mods::{ModInfo, ModKind, Theme, media_path};

/// The most a theme's font may weigh.
const FONT_MOST: u64 = 16 << 20;

/// A mod's interface theme, with the folder it names its pictures and font from.
#[derive(Clone, Debug)]
pub struct ModTheme {
    pub id: String,
    pub dir: PathBuf,
    pub theme: Theme,
}

/// The theme of the mods in play: the last in the load order that has one (a folder's).
pub fn of(mods: &[ModInfo]) -> Option<ModTheme> {
    mods.iter().rev().find(|m| m.kind == ModKind::Directory && !m.manifest.theme.is_empty())
        .map(|m| ModTheme { id: m.manifest.id.clone(), dir: m.path.clone(), theme: m.manifest.theme.clone() })
}

impl ModTheme {
    /// The battle's palette for `side`: TA Reborn's with the theme's colours (a colour written
    /// otherwise than `#rrggbb` is left as TA Reborn's).
    pub fn palette(&self, side: &str) -> Palette {
        Palette::REBORN.themed(&|key| self.theme.colour(side, key).and_then(parse_colour))
    }

    /// The picture tiled behind the plates (`panel`) or the top bar (`bar`) for `side`, where the
    /// theme names one that is there.
    pub fn texture(&self, side: &str, which: &str) -> Option<PathBuf> {
        self.theme.texture(side, which).and_then(|named| media_path(&self.dir, named))
    }

    /// The theme's font, where it names one that is there and reads.
    pub fn font(&self) -> Option<Arc<Vec<u8>>> {
        let path = media_path(&self.dir, &self.theme.font)?;
        let bytes = read_bounded(&path, FONT_MOST)?;
        if ta_ui::theme::font_reads(&bytes) { Some(Arc::new(bytes)) } else {
            log::warn!("mod {}: the theme's font {} does not read: TA Reborn's own is used", self.id, path.display());
            None
        }
    }
}

fn read_bounded(path: &Path, most: u64) -> Option<Vec<u8>> {
    use std::io::Read;
    let file = std::fs::File::open(path).ok()?;
    if file.metadata().ok()?.len() > most { return None; }
    let mut bytes = Vec::new();
    file.take(most + 1).read_to_end(&mut bytes).ok()?;
    (bytes.len() as u64 <= most).then_some(bytes)
}

/// What a battle's interface is drawn in now, so that it changes only when the theme or the side
/// does: the theme's mod and the side, and the font read for it.
#[derive(Default)]
pub struct Applied {
    key: Option<(String, String)>,
    font: Option<Arc<Vec<u8>>>,
    /// The pictures behind the plates and the top bar, while they are the theme's.
    pub textures: (Option<PathBuf>, Option<PathBuf>),
}

impl Applied {
    /// Draws the interface in `theme` for `side` (None for either: TA Reborn's own). The
    /// palette, the font and the pictures change only when the theme or the side does.
    pub fn apply(&mut self, ctx: &egui::Context, theme: Option<&ModTheme>, side: Option<&str>) {
        let key = theme.zip(side).map(|(theme, side)| (theme.id.clone(), side.to_ascii_lowercase()));
        if key == self.key { return; }
        match theme.zip(side) {
            Some((theme, side)) => {
                ta_ui::theme::set_palette(ctx, theme.palette(side));
                // (Its font read once, whatever side.)
                if self.key.as_ref().is_none_or(|(id, _)| *id != theme.id) { self.font = theme.font(); }
                ta_ui::theme::set_battle_font(ctx, self.font.clone());
                self.textures = (theme.texture(side, "panel"), theme.texture(side, "bar"));
                log::info!("interface theme of mod {} for {side}", theme.id);
            }
            None => {
                ta_ui::theme::set_palette(ctx, Palette::REBORN);
                ta_ui::theme::set_battle_font(ctx, None);
                self.font = None;
                self.textures = (None, None);
            }
        }
        self.key = key;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ta_vfs::mods::ModManifest;

    fn module(id: &str, theme: &str) -> ModInfo {
        let manifest: ModManifest = toml::from_str(&format!("id = \"{id}\"\nname = \"{id}\"\n{theme}")).unwrap();
        ModInfo { manifest, path: PathBuf::from(id), kind: ModKind::Directory }
    }

    /// The theme is the last mod's in the load order that has one; a mod without one, after it,
    /// leaves it.
    #[test]
    fn the_theme_is_the_last_themed_mods_in_the_load_order() {
        let mods = [module("a", "[theme]\naccent = \"#ff0000\""), module("b", "[theme]\naccent = \"#00ff00\""), module("c", "")];
        assert_eq!(of(&mods).map(|t| t.id), Some("b".to_string()));
        assert!(of(&mods[2..]).is_none());
    }

    /// A theme's palette is TA Reborn's with its colours, a side's own over the theme's, and a
    /// colour that is no colour left as TA Reborn's.
    #[test]
    fn a_themes_palette_takes_its_colours_and_a_sides_own() {
        let theme = of(&[module("t", "[theme]\naccent = \"#5ab4ff\"\ntext = \"oops\"\nmetal = \"#88aacc\"\n[theme.sides.core]\naccent = \"#ce4834\"")]).unwrap();
        let arm = theme.palette("ARM");
        assert_eq!(arm.accent, egui::Color32::from_rgb(0x5a, 0xb4, 0xff));
        assert_ne!(arm.accent_bright, Palette::REBORN.accent_bright, "its shades made from it");
        assert_eq!(arm.text, Palette::REBORN.text, "no colour: TA Reborn's");
        assert_eq!(arm.metal_bar, egui::Color32::from_rgb(0x88, 0xaa, 0xcc));
        assert_eq!(arm.panel, Palette::REBORN.panel, "left out: TA Reborn's");
        assert_eq!(theme.palette("Core").accent, egui::Color32::from_rgb(0xce, 0x48, 0x34));
    }

    /// TA Reborn's own colours given as a theme make (nearly) TA Reborn's palette: the shades a
    /// theme's colours are made into are those of TA Reborn's own.
    #[test]
    fn reborns_colours_as_a_theme_make_reborns_palette() {
        let hex = |c: egui::Color32| format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b());
        let p = Palette::REBORN;
        let theme = of(&[module("t", &format!("[theme]\naccent = \"{}\"\ntext = \"{}\"\ndim = \"{}\"\npanel = \"{}\"\ngood = \"{}\"\nbad = \"{}\"", hex(p.accent), hex(p.text), hex(p.steel), hex(p.panel), hex(p.good), hex(p.bad)))]).unwrap();
        let made = theme.palette("arm");
        let near = |a: egui::Color32, b: egui::Color32, name: &str| {
            let d = (a.r() as i32 - b.r() as i32).abs().max((a.g() as i32 - b.g() as i32).abs()).max((a.b() as i32 - b.b() as i32).abs());
            assert!(d <= 12, "{name}: {a:?} against {b:?}");
        };
        for (name, a, b) in [("accent_bright", made.accent_bright, p.accent_bright), ("accent_dim", made.accent_dim, p.accent_dim), ("accent_deep", made.accent_deep, p.accent_deep),
            ("on_accent", made.on_accent, p.on_accent), ("selection", made.selection, p.selection), ("text_bright", made.text_bright, p.text_bright), ("steel_dim", made.steel_dim, p.steel_dim),
            ("bg", made.bg, p.bg), ("bg_deep", made.bg_deep, p.bg_deep), ("panel_hi", made.panel_hi, p.panel_hi), ("faint_bg", made.faint_bg, p.faint_bg), ("widget", made.widget, p.widget),
            ("widget_hover", made.widget_hover, p.widget_hover), ("line", made.line, p.line), ("line_hi", made.line_hi, p.line_hi), ("good_deep", made.good_deep, p.good_deep), ("bad_deep", made.bad_deep, p.bad_deep)] {
            near(a, b, name);
        }
    }
}
