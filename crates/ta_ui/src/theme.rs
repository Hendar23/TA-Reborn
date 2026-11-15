//! Visual identity of the launcher and the game, after Total Annihilation's own screens:
//! grimy brown-black metal, steel pipes and gunmetal plates, olive-black wells, light
//! stencil-like caps, brass for the main action and focus, TA's green lamps and red for
//! status. The materials are painted by [`crate::metal`]; this holds the palette, the fonts
//! and the egui style.
//!
//! Text uses Bahnschrift, the DIN-style variable font that ships with Windows 10 and later.
//! It is read from the system font folder at runtime (never embedded or redistributed); its
//! weight and width axes provide regular, semi-bold and condensed-bold faces from one file.
//! Where it is missing (Linux, the Steam Deck, macOS) the interface is set in Sofia Sans, a
//! technical sans of the same kind with Latin, Greek and Cyrillic, bundled under the SIL Open
//! Font License (`fonts/`): Semi Condensed for text, Extra Condensed for titles, its lines as tall
//! as Bahnschrift's and its text a little narrower ([`ui_face`]). Chinese, Japanese
//! and Korean, which both lack, come from the system's fonts (see [`cjk_faces`]). Every
//! face's capitals are set in the middle of their line (see [`VMetrics::cap_centring`]), so
//! text centred in a box or beside a lamp, a bullet or an arrow sits level with it.
//!
//! A battle's HUD alone may be drawn in a mod's interface theme: what is drawn inside [`hud`]
//! takes the battle's palette ([`set_battle_palette`], through [`pal`]) and, in a context given
//! one, its font ([`set_battle_font`], in the HUD's own families); everything else, and anything
//! drawn in [`reborn`] inside it (a dialog's panel), is TA Reborn's own whatever the theme.

use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use egui::epaint::text::VariationCoords;
use ta_data::font::{VMetrics, has_face, vmetrics};
use egui::style::{HandleShape, Selection, WidgetVisuals, Widgets};
use egui::{
    Color32, CornerRadius, CursorIcon, FontData, FontDefinitions, FontFamily, FontId, FontTweak, Margin,
    Shadow, Stroke, Style, TextStyle, Theme, Visuals, vec2,
};

// ---- Palette ------------------------------------------------------------------------------

/// Deepest background: header, navigation rail and status bar.
pub const BG_DEEP: Color32 = Color32::from_rgb(14, 13, 11);
/// Page background behind the cards.
pub const BG: Color32 = Color32::from_rgb(20, 19, 16);
/// Card surface.
pub const PANEL: Color32 = Color32::from_rgb(31, 30, 26);
/// Raised surface: hovered rows, the selected navigation item.
pub const PANEL_HI: Color32 = Color32::from_rgb(44, 42, 36);
/// Recessed wells (TA's olive-black): text fields, lists, slider rails.
pub const INSET: Color32 = Color32::from_rgb(13, 14, 9);
/// Hairlines and card borders.
pub const LINE: Color32 = Color32::from_rgb(60, 57, 50);
/// Stronger borders: buttons and inputs.
pub const LINE_HI: Color32 = Color32::from_rgb(90, 87, 78);
/// Secondary text (TA's beige-grey labels).
pub const STEEL: Color32 = Color32::from_rgb(158, 153, 138);
/// Tertiary text and inactive glyphs.
pub const STEEL_DIM: Color32 = Color32::from_rgb(124, 119, 106);
/// Body text.
pub const TEXT: Color32 = Color32::from_rgb(216, 211, 196);
/// Headings and emphasised values.
pub const TEXT_BRIGHT: Color32 = Color32::from_rgb(243, 239, 227);
/// Primary accent, brass: the main action, focus and selection. (Named for the amber it
/// was; the names stay so every screen follows the palette.)
pub const AMBER: Color32 = Color32::from_rgb(216, 180, 86);
/// Hovered brass.
pub const AMBER_BRIGHT: Color32 = Color32::from_rgb(242, 210, 124);
/// Pressed brass and brass hairlines.
pub const AMBER_DIM: Color32 = Color32::from_rgb(142, 113, 48);
/// Brass-tinted dark fill for selected and active states.
pub const AMBER_DEEP: Color32 = Color32::from_rgb(56, 46, 21);
/// Text drawn on brass.
pub const ON_AMBER: Color32 = Color32::from_rgb(28, 21, 8);
/// Success, "present", TA's indicator lamps.
pub const GREEN: Color32 = Color32::from_rgb(120, 214, 92);
/// Dark green fill behind success badges.
pub const GREEN_DEEP: Color32 = Color32::from_rgb(26, 44, 20);
/// Errors, "missing".
pub const RED: Color32 = Color32::from_rgb(224, 94, 74);
/// Dark red fill behind error badges.
pub const RED_DEEP: Color32 = Color32::from_rgb(58, 23, 18);
/// Information: replays, radar, allies.
pub const SKY: Color32 = Color32::from_rgb(104, 184, 232);
/// TA's button lettering: pale cream (its in-game buttons' stencil caps).
pub const CREAM: Color32 = Color32::from_rgb(226, 222, 182);

// ---- The battle's palette -----------------------------------------------------------------

/// The colours the game draws its HUD with: the palette above ([`Palette::REBORN`]), unless a
/// mod's interface theme gives a battle colours of its own ([`set_battle_palette`]; the game's
/// `mod_theme`). They are drawn only inside [`hud`]: everything else, the launcher, the menus,
/// the battle's dialogs and pages and the cockpit, is drawn in TA Reborn's own whatever the theme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub bg_deep: Color32,
    pub bg: Color32,
    pub panel: Color32,
    pub panel_hi: Color32,
    pub inset: Color32,
    pub line: Color32,
    pub line_hi: Color32,
    /// Secondary text and labels ([`STEEL`]), and its dimmer kind.
    pub steel: Color32,
    pub steel_dim: Color32,
    pub text: Color32,
    pub text_bright: Color32,
    /// The accent ([`AMBER`]'s brass): titles, selection, the main action, focus.
    pub accent: Color32,
    pub accent_bright: Color32,
    pub accent_dim: Color32,
    pub accent_deep: Color32,
    pub on_accent: Color32,
    /// Health, success, lamps lit ([`GREEN`]); damage, errors ([`RED`]); radar and allies ([`SKY`]).
    pub good: Color32,
    pub good_deep: Color32,
    pub bad: Color32,
    pub bad_deep: Color32,
    pub info: Color32,
    /// Buttons' stencil lettering ([`CREAM`]).
    pub button_text: Color32,
    /// The resource bar's metal and energy.
    pub metal_bar: Color32,
    pub energy_bar: Color32,
    /// egui's own shades: a faint background, a widget's fill and its fill hovered, a selection's.
    pub faint_bg: Color32,
    pub widget: Color32,
    pub widget_hover: Color32,
    pub selection: Color32,
    /// The classic HUD's own lettering (its art is TA's or a mod's): text, its buttons', dimmed,
    /// and a button's held down.
    pub classic_text: Color32,
    pub classic_button: Color32,
    pub classic_dim: Color32,
    pub classic_pressed: Color32,
}

impl Palette {
    /// TA Reborn's own: the constants above.
    pub const REBORN: Palette = Palette {
        bg_deep: BG_DEEP, bg: BG, panel: PANEL, panel_hi: PANEL_HI, inset: INSET, line: LINE, line_hi: LINE_HI,
        steel: STEEL, steel_dim: STEEL_DIM, text: TEXT, text_bright: TEXT_BRIGHT,
        accent: AMBER, accent_bright: AMBER_BRIGHT, accent_dim: AMBER_DIM, accent_deep: AMBER_DEEP, on_accent: ON_AMBER,
        good: GREEN, good_deep: GREEN_DEEP, bad: RED, bad_deep: RED_DEEP, info: SKY, button_text: CREAM,
        metal_bar: Color32::from_rgb(170, 184, 196), energy_bar: Color32::from_rgb(242, 212, 60),
        faint_bg: Color32::from_rgb(26, 25, 21), widget: Color32::from_rgb(40, 39, 34), widget_hover: Color32::from_rgb(54, 52, 45), selection: Color32::from_rgb(96, 78, 32),
        classic_text: Color32::from_rgb(236, 236, 236), classic_button: Color32::from_rgb(223, 223, 183), classic_dim: Color32::from_rgb(170, 170, 160), classic_pressed: Color32::from_rgb(255, 255, 215),
    };

    /// This palette with a theme's colours (`colour` gives each by its key, `None` where the
    /// theme leaves it): the shades a theme does not give are made from those it does, as
    /// [`Palette::REBORN`]'s are from its own (`accent` lights `accent_bright` and darkens to
    /// `accent_dim`, `accent_deep`; `panel` darkens to `bg`, `bg_deep`...).
    pub fn themed(self, colour: &dyn Fn(&str) -> Option<Color32>) -> Palette {
        let mut p = self;
        let toward = |c: Color32, to: Color32, t: f32| c.lerp_to_gamma(to, t);
        let (black, white) = (Color32::BLACK, Color32::WHITE);
        if let Some(accent) = colour("accent") {
            p.accent = accent;
            p.accent_bright = brighter(accent);
            p.accent_dim = toward(accent, black, 0.36);
            p.accent_deep = toward(accent, black, 0.74);
            p.on_accent = toward(accent, black, 0.88);
            p.selection = toward(accent, black, 0.56);
        }
        if let Some(text) = colour("text") {
            p.text = text;
            p.text_bright = toward(text, white, 0.66);
        }
        if let Some(dim) = colour("dim") {
            p.steel = dim;
            p.steel_dim = toward(dim, black, 0.22);
        }
        if let Some(panel) = colour("panel") {
            p.panel = panel;
            p.bg = toward(panel, black, 0.35);
            p.bg_deep = toward(panel, black, 0.55);
            p.panel_hi = toward(panel, white, 0.06);
            p.faint_bg = toward(p.bg, panel, 0.55);
            p.widget = toward(panel, white, 0.04);
            p.widget_hover = toward(panel, white, 0.1);
            p.inset = toward(panel, black, 0.58);
            p.line = toward(panel, white, 0.13);
            p.line_hi = toward(p.line, white, 0.17);
        }
        if let Some(inset) = colour("inset") { p.inset = inset; }
        if let Some(line) = colour("line") {
            p.line = line;
            p.line_hi = toward(line, white, 0.17);
        }
        if let Some(good) = colour("good") {
            p.good = good;
            p.good_deep = toward(good, black, 0.8);
        }
        if let Some(bad) = colour("bad") {
            p.bad = bad;
            p.bad_deep = toward(bad, black, 0.76);
        }
        let set = |field: &mut Color32, key: &str| if let Some(c) = colour(key) { *field = c; };
        set(&mut p.accent_bright, "accent_bright");
        set(&mut p.text_bright, "text_bright");
        set(&mut p.info, "info");
        set(&mut p.button_text, "button_text");
        set(&mut p.metal_bar, "metal");
        set(&mut p.energy_bar, "energy");
        set(&mut p.classic_text, "classic.text");
        set(&mut p.classic_button, "classic.button_text");
        set(&mut p.classic_dim, "classic.dim");
        set(&mut p.classic_pressed, "classic.pressed");
        p
    }
}

/// A colour lit as [`AMBER_BRIGHT`] is [`AMBER`]: brighter (value up an eighth) and a little
/// paler (saturation down a fifth), its hue kept.
fn brighter(c: Color32) -> Color32 {
    let [r, g, b, a] = c.to_srgba_unmultiplied().map(|v| v as f32 / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let (v, chroma) = (max, max - min);
    let s = if max > 0.0 { chroma / max } else { 0.0 };
    let h = if chroma == 0.0 { 0.0 } else if max == r { ((g - b) / chroma).rem_euclid(6.0) } else if max == g { (b - r) / chroma + 2.0 } else { (r - g) / chroma + 4.0 };
    let (v, s) = ((v * 1.12).min(1.0), s * 0.82);
    let c2 = v * s;
    let x = c2 * (1.0 - ((h % 2.0) - 1.0).abs());
    let (r1, g1, b1) = match h as u32 { 0 => (c2, x, 0.0), 1 => (x, c2, 0.0), 2 => (0.0, c2, x), 3 => (0.0, x, c2), 4 => (x, 0.0, c2), _ => (c2, 0.0, x) };
    let m = v - c2;
    let byte = |v: f32| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgba_unmultiplied(byte(r1), byte(g1), byte(b1), (a * 255.0).round() as u8)
}

// ---- The battle's look --------------------------------------------------------------------

/// The look of what is being drawn: TA Reborn's own, or the battle's HUD's ([`hud`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Look {
    Reborn,
    Hud,
}

thread_local! {
    /// The battle's palette, the HUD's ([`set_battle_palette`]).
    static BATTLE_PALETTE: Cell<Palette> = const { Cell::new(Palette::REBORN) };
    /// The look of what is drawn now ([`hud`], [`reborn`]), and whether its lettering is a
    /// themed battle's font (the HUD's, in a context that has one).
    static LOOK: Cell<Look> = const { Cell::new(Look::Reborn) };
    static HUD_FONT: Cell<bool> = const { Cell::new(false) };
    /// Styles made for a look from others ([`styled`]), the last used first.
    static STYLED: RefCell<Vec<Styled>> = const { RefCell::new(Vec::new()) };
}

/// The palette of what is drawn now: the battle's ([`set_battle_palette`]) inside [`hud`], TA
/// Reborn's own ([`Palette::REBORN`]) everywhere else.
pub fn pal() -> Palette {
    match LOOK.get() {
        Look::Hud => BATTLE_PALETTE.get(),
        Look::Reborn => Palette::REBORN,
    }
}

/// The battle's palette, which [`hud`] draws in (TA Reborn's own unless a theme gave another).
pub fn battle_palette() -> Palette {
    BATTLE_PALETTE.get()
}

/// Sets the battle's palette: the colours [`hud`] draws the HUD in, egui's own widgets there with
/// them. Nothing drawn outside it changes.
pub fn set_battle_palette(palette: Palette) {
    BATTLE_PALETTE.set(palette);
}

/// Whether what is drawn now is the battle's HUD ([`hud`]).
pub fn in_hud() -> bool {
    LOOK.get() == Look::Hud
}

/// Draws `add` as the battle's HUD: in the battle's palette ([`pal`]), its titles, numbers and
/// buttons ([`strong`] and [`display`] text) in a theme's font where `ui`'s context has one
/// ([`set_battle_font`]), and egui's own widgets, tooltips and drop-downs in them too. Everything
/// drawn outside it is in TA Reborn's own look, whatever the theme; [`reborn`] gives that back
/// inside it (the dialogs on TA Reborn's plates).
pub fn hud<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    in_look(ui, Look::Hud, add)
}

/// Draws `add` in TA Reborn's own look wherever it is drawn, inside [`hud`] too: the dialogs and
/// pages on TA Reborn's plates ([`crate::panel::Panel`] draws its contents so).
pub fn reborn<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    in_look(ui, Look::Reborn, add)
}

/// [`reborn`] for what makes areas of its own on `ctx` (a dialog's window).
pub fn reborn_on<R>(ctx: &egui::Context, add: impl FnOnce() -> R) -> R {
    // (Outside the HUD, everything is in TA Reborn's own look already, as its makers styled it.)
    if LOOK.get() == Look::Reborn {
        return add();
    }
    let _look = InLook::enter(ctx, Look::Reborn);
    add()
}

fn in_look<R>(ui: &mut egui::Ui, look: Look, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    // (Outside the HUD, everything is in TA Reborn's own look already, as its makers styled it: a
    // launcher's or a menu's panel keeps its style as it is.)
    if look == Look::Reborn && LOOK.get() == Look::Reborn {
        return add(ui);
    }
    let _look = InLook::enter(ui.ctx(), look);
    // (The ui's own style too: what it lays out takes its colours and lettering from it, as the
    // uis made in it (a panel at the top of the screen) take theirs.)
    let own = ui.style().clone();
    let lent = styled(ui.ctx(), &own, look);
    if let Some(style) = &lent {
        ui.set_style(style.clone());
    }
    let inner = add(ui);
    if lent.is_some() {
        ui.set_style(own);
    }
    inner
}

/// A stretch of drawing in one look: the look before it, and the context's own style while
/// another is lent it (what egui makes of itself meanwhile, areas, tooltips and drop-downs, takes
/// the context's style), both put back when it ends.
struct InLook {
    ctx: egui::Context,
    was: (Look, bool),
    own: Option<Arc<Style>>,
}

impl InLook {
    fn enter(ctx: &egui::Context, look: Look) -> InLook {
        let was = (LOOK.replace(look), HUD_FONT.replace(look == Look::Hud && has_battle_font(ctx)));
        let own = ctx.style_of(Theme::Dark);
        let own = styled(ctx, &own, look).map(|lent| {
            ctx.set_style_of(Theme::Dark, lent);
            own
        });
        InLook { ctx: ctx.clone(), was, own }
    }
}

impl Drop for InLook {
    fn drop(&mut self) {
        LOOK.set(self.was.0);
        HUD_FONT.set(self.was.1);
        if let Some(own) = self.own.take() {
            self.ctx.set_style_of(Theme::Dark, own);
        }
    }
}

/// Runs `make` in TA Reborn's own look (the styles a context is installed with are).
fn as_reborn<R>(make: impl FnOnce() -> R) -> R {
    struct Back(Look, bool);
    impl Drop for Back {
        fn drop(&mut self) {
            LOOK.set(self.0);
            HUD_FONT.set(self.1);
        }
    }
    let _back = Back(LOOK.replace(Look::Reborn), HUD_FONT.replace(false));
    make()
}

/// A style made for a look from another ([`styled`]): `to` none where `from` is in it already.
struct Styled {
    from: Arc<Style>,
    look: Look,
    palette: Palette,
    hud_font: bool,
    to: Option<Arc<Style>>,
}

/// `style` in `look`, for `ctx` (whose battle font the HUD's lettering takes where it has one):
/// egui's colours in the look's palette, its buttons' and headings' lettering in the look's
/// families. None when it is so already. Made once, and kept while it is used: the next frames
/// lend the same.
fn styled(ctx: &egui::Context, style: &Arc<Style>, look: Look) -> Option<Arc<Style>> {
    let palette = match look {
        Look::Hud => BATTLE_PALETTE.get(),
        Look::Reborn => Palette::REBORN,
    };
    let hud_font = look == Look::Hud && has_battle_font(ctx);
    STYLED.with_borrow_mut(|made| {
        if let Some(at) = made.iter().position(|s| Arc::ptr_eq(&s.from, style) && s.look == look && s.palette == palette && s.hud_font == hud_font) {
            let hit = made.remove(at);
            let to = hit.to.clone();
            made.insert(0, hit);
            return to;
        }
        let to = restyled(style, palette, hud_font).map(Arc::new);
        // (The style it was made from kept with it: no other can take its place in memory.)
        made.insert(0, Styled { from: style.clone(), look, palette, hud_font, to: to.clone() });
        made.truncate(16);
        to
    })
}

/// `style` with egui's colours in `palette`, its lettering in TA Reborn's families or (`hud_font`)
/// the HUD's; None when it is so already.
fn restyled(style: &Style, palette: Palette, hud_font: bool) -> Option<Style> {
    let visuals = visuals_of(palette);
    let family = |family: &FontFamily| -> Option<FontFamily> {
        let FontFamily::Name(name) = family else { return None };
        let to = match (hud_font, &**name) {
            (true, STRONG_FAMILY) => HUD_STRONG_FAMILY,
            (true, DISPLAY_FAMILY) => HUD_DISPLAY_FAMILY,
            (false, HUD_STRONG_FAMILY) => STRONG_FAMILY,
            (false, HUD_DISPLAY_FAMILY) => DISPLAY_FAMILY,
            _ => return None,
        };
        Some(FontFamily::Name(to.into()))
    };
    if style.visuals == visuals && style.text_styles.values().all(|font| family(&font.family).is_none()) {
        return None;
    }
    let mut style = style.clone();
    style.visuals = visuals;
    for font in style.text_styles.values_mut() {
        if let Some(to) = family(&font.family) {
            font.family = to;
        }
    }
    Some(style)
}

/// A colour TA Reborn's own look draws beside its palette's `reborn`, moved as a theme moved that
/// colour (to `themed`): the colour itself where the theme leaves it, else as much lighter, darker
/// or tinted, channel by channel. For the interface's fixed shades that go with a palette colour
/// (the brass of a lit keycap, of the main action's button).
pub fn retinted(colour: Color32, reborn: Color32, themed: Color32) -> Color32 {
    if reborn == themed {
        return colour;
    }
    let [r, g, b, a] = colour.to_srgba_unmultiplied();
    let (from, to) = (reborn.to_srgba_unmultiplied(), themed.to_srgba_unmultiplied());
    let channel = |c: u8, f: u8, t: u8| {
        let (c, f, t) = (u32::from(c) + 1, u32::from(f) + 1, u32::from(t) + 1);
        ((c * t + f / 2) / f).saturating_sub(1).min(255) as u8
    };
    Color32::from_rgba_unmultiplied(channel(r, from[0], to[0]), channel(g, from[1], to[1]), channel(b, from[2], to[2]), a)
}

/// A colour's relative luminance, as the Web's accessibility guidelines measure it (0 to 1).
fn luminance(c: Color32) -> f32 {
    let channel = |v: u8| {
        let v = f32::from(v) / 255.0;
        if v <= 0.039_28 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * channel(c.r()) + 0.7152 * channel(c.g()) + 0.0722 * channel(c.b())
}

/// The contrast of two (opaque) colours, as the Web's accessibility guidelines measure it (1 to
/// 21): 4.5 reads well as text, 3 as large or bold text.
pub fn contrast(a: Color32, b: Color32) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// An opaque colour readable on `backdrop`: itself where its [`contrast`] with it is `least` or
/// more (TA Reborn's own colours on its own plates are), else drawn towards white or black, away
/// from the backdrop, until it is. For a theme's colours on the HUD's fixed dark backdrops, and
/// fixed colours on a theme's plates.
pub fn legible(colour: Color32, backdrop: Color32, least: f32) -> Color32 {
    if contrast(colour, backdrop) >= least {
        return colour;
    }
    // (Away from it: towards white on what is darker than the grey as far from both.)
    let away = if luminance(backdrop) < 0.179 { Color32::WHITE } else { Color32::BLACK };
    if contrast(away, backdrop) < least {
        return away;
    }
    let (mut short, mut far) = (0.0f32, 1.0f32);
    for _ in 0..12 {
        let t = (short + far) / 2.0;
        if contrast(colour.lerp_to_gamma(away, t), backdrop) >= least { far = t } else { short = t }
    }
    colour.lerp_to_gamma(away, far)
}

/// A colour as a theme writes it: `#rgb`, `#rrggbb` or `#rrggbbaa` (the `#` may be left out).
pub fn parse_colour(text: &str) -> Option<Color32> {
    let hex = text.trim().trim_start_matches('#');
    if !hex.chars().all(|c| c.is_ascii_hexdigit()) { return None; }
    let byte = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).ok();
    match hex.len() {
        3 => {
            let digit = |at: usize| u8::from_str_radix(&hex[at..at + 1], 16).ok().map(|d| d * 17);
            Some(Color32::from_rgb(digit(0)?, digit(1)?, digit(2)?))
        }
        6 => Some(Color32::from_rgb(byte(0)?, byte(2)?, byte(4)?)),
        8 => Some(Color32::from_rgba_unmultiplied(byte(0)?, byte(2)?, byte(4)?, byte(6)?)),
        _ => None,
    }
}

/// The spacing scale, in points: between the parts of one control (`XS`), related items
/// (`S`), groups (`M`), a card's contents and its edge (`L`), cards (`XL`) and a page's
/// regions (`XXL`).
pub mod space {
    pub const XS: f32 = 4.0;
    pub const S: f32 = 8.0;
    pub const M: f32 = 12.0;
    pub const L: f32 = 16.0;
    pub const XL: f32 = 24.0;
    pub const XXL: f32 = 32.0;
}

// ---- Fonts --------------------------------------------------------------------------------

/// Condensed bold family for titles and the Play button.
pub const DISPLAY_FAMILY: &str = "ta-display";
/// Semi-bold family for buttons, labels and navigation.
pub const STRONG_FAMILY: &str = "ta-strong";
/// The HUD's own [`STRONG_FAMILY`] and [`DISPLAY_FAMILY`]: their faces behind a themed battle's
/// font, in a context that has one ([`set_battle_font`]); [`strong`] and [`display`] give them
/// there inside [`hud`]. TA Reborn's own families never hold a theme's font.
pub const HUD_STRONG_FAMILY: &str = "ta-hud-strong";
pub const HUD_DISPLAY_FAMILY: &str = "ta-hud-display";

/// Whether the lettering of what is drawn now is the HUD's in a themed battle's font.
fn hud_lettering() -> bool {
    LOOK.get() == Look::Hud && HUD_FONT.get()
}

/// Condensed bold display text (inside [`hud`], in a themed battle's font).
pub fn display(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(if hud_lettering() { HUD_DISPLAY_FAMILY } else { DISPLAY_FAMILY }.into()))
}

/// Semi-bold text (inside [`hud`], in a themed battle's font).
pub fn strong(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(if hud_lettering() { HUD_STRONG_FAMILY } else { STRONG_FAMILY }.into()))
}

/// Regular body text.
pub fn body(size: f32) -> FontId {
    FontId::proportional(size)
}

/// Monospaced text (paths, file names).
pub fn mono(size: f32) -> FontId {
    FontId::monospace(size)
}

/// The weights of the interface's text: [`body`], [`strong`] and [`display`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Weight {
    Regular,
    Strong,
    Display,
}

/// A face of the interface's typeface: a font file, and the variation coordinates that make the
/// face of it.
#[derive(Clone, Copy, Debug)]
pub struct UiFace {
    pub bytes: &'static [u8],
    pub coords: &'static [(&'static [u8; 4], f32)],
}

/// Sofia Sans (Lettersoup: Botio Nikoltchev and Ani Petrova; the SIL Open Font License 1.1, its
/// text in `fonts/OFL.txt`) for where Bahnschrift is missing: Semi Condensed for text, Extra
/// Condensed for titles (as wide as Bahnschrift's condensed bold; Condensed is a sixth wider),
/// each varying by weight. At the same size its lines are as tall as Bahnschrift's (1.2 em), so
/// the layouts made for Bahnschrift hold; its text is 3 to 14% narrower, its capitals 8% shorter.
const SOFIA_SANS_SEMI_CONDENSED: &[u8] = include_bytes!("../fonts/SofiaSansSemiCondensed.ttf");
const SOFIA_SANS_EXTRA_CONDENSED: &[u8] = include_bytes!("../fonts/SofiaSansExtraCondensed.ttf");

/// The interface's face for `weight`: Bahnschrift at that weight (and, for titles, its condensed
/// width) where Windows has it and it can be read, else Sofia Sans's file for it at that weight.
/// `TA_UI_FONT=bundled` takes Sofia Sans anyway, to see the interface as Linux, the Steam Deck and
/// macOS show it.
pub fn ui_face(weight: Weight) -> UiFace {
    face_of(bahnschrift(), weight)
}

/// [`ui_face`] with Bahnschrift's bytes, or without it.
fn face_of(bahnschrift: Option<&'static [u8]>, weight: Weight) -> UiFace {
    let (bytes, coords): (&'static [u8], &'static [(&'static [u8; 4], f32)]) = match (bahnschrift, weight) {
        (Some(bytes), Weight::Regular) => (bytes, &[(b"wght", 400.0)]),
        (Some(bytes), Weight::Strong) => (bytes, &[(b"wght", 600.0)]),
        (Some(bytes), Weight::Display) => (bytes, &[(b"wght", 700.0), (b"wdth", 75.0)]),
        (None, Weight::Regular) => (SOFIA_SANS_SEMI_CONDENSED, &[(b"wght", 400.0)]),
        (None, Weight::Strong) => (SOFIA_SANS_SEMI_CONDENSED, &[(b"wght", 600.0)]),
        (None, Weight::Display) => (SOFIA_SANS_EXTRA_CONDENSED, &[(b"wght", 700.0)]),
    };
    UiFace { bytes, coords }
}

/// Installs the fonts and the dark style. Returns `true` if Bahnschrift, Windows' font, was
/// found (else the interface is in Sofia Sans, bundled).
pub fn install(ctx: &egui::Context) -> bool {
    let system_font = install_fonts(ctx);
    follow_language(ctx);
    // A single, dark identity regardless of the OS theme.
    ctx.set_theme(Theme::Dark);
    as_reborn(|| ctx.style_mut_of(Theme::Dark, apply_style));
    system_font
}

/// Like [`install`], with the dense text and spacing of [`compact`] everywhere (the game:
/// its HUD, tooltips and drop-down lists); [`crate::panel::Panel`]s switch their contents
/// back to [`roomy`].
pub fn install_compact(ctx: &egui::Context) -> bool {
    let system_font = install_fonts(ctx);
    follow_language(ctx);
    ctx.set_theme(Theme::Dark);
    as_reborn(|| {
        ctx.style_mut_of(Theme::Dark, |style| {
            apply_style(style);
            compact(style);
        })
    });
    system_font
}

fn install_fonts(ctx: &egui::Context) -> bool {
    let bahnschrift = bahnschrift();
    let battle = battle_font_of(ctx).map(|font| font.face);
    ctx.set_fonts(definitions(bahnschrift, &crate::i18n::language(), battle.as_ref()));
    bahnschrift.is_some()
}

/// A mod's interface theme's font in a context ([`set_battle_font`]): its file, and the face made
/// of it for the HUD's lettering ([`battle_face`]).
#[derive(Clone)]
struct BattleFont {
    bytes: Arc<Vec<u8>>,
    face: Arc<FontData>,
}

fn battle_font_id() -> egui::Id {
    egui::Id::new("ta_ui::theme::battle_font")
}

fn battle_font_of(ctx: &egui::Context) -> Option<BattleFont> {
    ctx.data(|d| d.get_temp::<BattleFont>(battle_font_id()))
}

/// Whether `ctx` letters its HUD in a mod's interface theme's font ([`set_battle_font`]).
pub fn has_battle_font(ctx: &egui::Context) -> bool {
    battle_font_of(ctx).is_some()
}

/// Whether a font file reads as egui reads it (it panics on one that does not): a face in it that
/// `skrifa` opens, with the tables the interface sets it by (`ta_data::font`, the mod check's
/// rule too).
pub fn font_reads(bytes: &[u8]) -> bool {
    ta_data::font::reads(bytes)
}

/// Sets the font of a mod's interface theme for `ctx`'s HUD (None: the interface's own): first in
/// the HUD's families ([`HUD_STRONG_FAMILY`], [`HUD_DISPLAY_FAMILY`]), the interface's own faces
/// after it for the letters it lacks, so [`hud`]'s titles, numbers and buttons are in it. Only
/// `ctx` has it (the battle's window; the companion screen follows it, [`follow_fonts`]): another
/// context, the loading screen's or the cockpit instruments', never does, and TA Reborn's own
/// families never hold it. Its fonts are made again when it changes. A font that does not read
/// ([`font_reads`]) is not set: false.
pub fn set_battle_font(ctx: &egui::Context, font: Option<Arc<Vec<u8>>>) -> bool {
    if font.as_ref().is_some_and(|bytes| !font_reads(bytes)) {
        return false;
    }
    let same = match (battle_font_of(ctx), &font) {
        (Some(a), Some(b)) => Arc::ptr_eq(&a.bytes, b) || *a.bytes == **b,
        (None, None) => true,
        _ => false,
    };
    if !same {
        put_battle_font(ctx, font.map(|bytes| BattleFont { face: Arc::new(battle_face(&bytes)), bytes }));
    }
    true
}

/// Gives `ctx` the battle font `from` has (none where it has none), its fonts made again when that
/// changed it: a window of the game's beside the battle's (the companion screen) lettered as the
/// battle's HUD is.
pub fn follow_fonts(ctx: &egui::Context, from: &egui::Context) {
    let theirs = battle_font_of(from);
    let same = match (battle_font_of(ctx), &theirs) {
        (Some(a), Some(b)) => Arc::ptr_eq(&a.face, &b.face),
        (None, None) => true,
        _ => false,
    };
    if !same {
        put_battle_font(ctx, theirs);
    }
}

fn put_battle_font(ctx: &egui::Context, font: Option<BattleFont>) {
    ctx.data_mut(|d| match font {
        Some(font) => {
            d.insert_temp(battle_font_id(), font);
        }
        None => d.remove::<BattleFont>(battle_font_id()),
    });
    install_fonts(ctx);
    ctx.request_repaint();
}

/// A theme's font as the HUD letters with it: set on the interface's lines, its line as tall as
/// theirs (egui sets a family's rows by its first face's, and the interface is laid out for its
/// own: Bahnschrift's 1.2 em, Sofia Sans's the same), and its capitals in the middle of them (as
/// the interface's own are, so its letters and those of the faces after it sit level).
fn battle_face(bytes: &[u8]) -> FontData {
    let own = vmetrics(bytes, 0);
    let interface = vmetrics(face_of(bahnschrift(), Weight::Strong).bytes, 0);
    FontData::from_owned(bytes.to_vec()).tweak(FontTweak {
        scale: line_scale(own, interface),
        y_offset_factor: own.map_or(0.0, |m| m.cap_centring()),
        ..Default::default()
    })
}

/// How much a face of metrics `own` is scaled to set it on the interface's lines (its face's
/// metrics `interface`): to a line as tall as theirs. (Within a quarter and four times its size,
/// for metrics that are nonsense.)
fn line_scale(own: Option<VMetrics>, interface: Option<VMetrics>) -> f32 {
    match (own, interface) {
        (Some(own), Some(interface)) if own.row() > 0.0 && interface.row() > 0.0 => (interface.row() / own.row()).clamp(0.25, 4.0),
        _ => 1.0,
    }
}

/// The fonts of the interface in `language`: Bahnschrift's faces (from its bytes, else Sofia
/// Sans's), egui's own behind them for what they lack, then the system's Chinese, Japanese and
/// Korean faces, the language's own first. The HUD's families hold the same, behind `battle`, a
/// themed battle's font ([`battle_face`]), where the context has one.
fn definitions(bahnschrift: Option<&'static [u8]>, language: &str, battle: Option<&Arc<FontData>>) -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    let mut fallbacks = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    // Hack carries the arrows and geometric shapes (▲ ▼ → ●) that Ubuntu-Light lacks.
    if fonts.font_data.contains_key("Hack") {
        fallbacks.push("Hack".to_owned());
    }
    if let Some(proportional) = fonts.families.get_mut(&FontFamily::Proportional) {
        proportional.clone_from(&fallbacks);
    }

    // (Other scripts' faces are set on the interface font's baseline: egui puts each at its
    // own ascent.)
    let ui_metrics = vmetrics(face_of(bahnschrift, Weight::Regular).bytes, 0);
    // egui's own faces (the fallbacks for the symbols the interface's lacks, and the monospace)
    // with their capitals in the middle of their lines too: then they are level with it.
    for name in ["Hack", "Ubuntu-Light"] {
        if let Some(data) = fonts.font_data.get(name)
            && let Some(m) = vmetrics(&data.font, data.index)
        {
            let mut data = FontData::clone(data);
            data.tweak.y_offset_factor = m.cap_centring();
            fonts.font_data.insert(name.to_owned(), Arc::new(data));
        }
    }
    // The interface's faces, each with its capitals in the middle of its line.
    let face = |weight: Weight| {
        let face = face_of(bahnschrift, weight);
        let centring = vmetrics(face.bytes, 0).map_or(0.0, |m| m.cap_centring());
        Arc::new(FontData::from_static(face.bytes).tweak(FontTweak {
            coords: VariationCoords::new(face.coords.iter().copied()),
            y_offset_factor: centring,
            ..Default::default()
        }))
    };
    fonts.font_data.insert("ui-regular".into(), face(Weight::Regular));
    fonts.font_data.insert("ui-strong".into(), face(Weight::Strong));
    fonts.font_data.insert("ui-display".into(), face(Weight::Display));
    if let Some(proportional) = fonts.families.get_mut(&FontFamily::Proportional) {
        proportional.insert(0, "ui-regular".into());
    }
    let mut strong = vec!["ui-strong".to_owned()];
    let mut display = vec!["ui-display".to_owned()];
    strong.extend(fallbacks.iter().cloned());
    display.extend(fallbacks);
    // Chinese, Japanese and Korean after them: the interface language's faces first.
    let (regular, bold) = cjk_faces(language, ui_metrics);
    for (name, data) in regular.iter().chain(&bold) {
        fonts.font_data.insert(name.clone(), data.clone());
    }
    let names = |faces: &[(String, Arc<FontData>)]| faces.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>();
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        if let Some(list) = fonts.families.get_mut(&family) {
            list.extend(names(&regular));
        }
    }
    for family in [&mut strong, &mut display] {
        family.extend(names(&bold));
        family.extend(names(&regular));
    }
    // The HUD's: the same, a themed battle's font before them (they give the letters it lacks).
    let (mut hud_strong, mut hud_display) = (strong.clone(), display.clone());
    if let Some(face) = battle {
        fonts.font_data.insert("battle".into(), face.clone());
        hud_strong.insert(0, "battle".into());
        hud_display.insert(0, "battle".into());
    }
    // Named families must always be bound, or egui panics when they are used.
    fonts.families.insert(FontFamily::Name(STRONG_FAMILY.into()), strong);
    fonts.families.insert(FontFamily::Name(DISPLAY_FAMILY.into()), display);
    fonts.families.insert(FontFamily::Name(HUD_STRONG_FAMILY.into()), hud_strong);
    fonts.families.insert(FontFamily::Name(HUD_DISPLAY_FAMILY.into()), hud_display);
    fonts
}

/// Puts the fonts in again at the start of the frame after the interface's language
/// changes (its own Chinese, Japanese or Korean faces go first).
fn follow_language(ctx: &egui::Context) {
    let id = egui::Id::new("ta-fonts-language");
    let watching = ctx.data_mut(|d| d.get_temp::<u32>(id)).is_some();
    ctx.data_mut(|d| d.insert_temp(id, crate::i18n::generation()));
    if watching {
        return;
    }
    ctx.on_begin_pass(
        "ta-fonts-language",
        Arc::new(move |ui: &mut egui::Ui| {
            let ctx = ui.ctx().clone();
            let now = crate::i18n::generation();
            if ctx.data_mut(|d| d.get_temp::<u32>(id)) != Some(now) {
                ctx.data_mut(|d| d.insert_temp(id, now));
                install_fonts(&ctx);
                ctx.request_repaint();
            }
        }),
    );
}

/// Chinese, Japanese and Korean.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Cjk {
    Simplified,
    Traditional,
    Japanese,
    Korean,
}

impl Cjk {
    /// The script of a language's code ("zh-Hans", "zh-TW", "ja", "ko").
    fn of(language: &str) -> Option<Cjk> {
        let code = language.to_ascii_lowercase();
        let primary = code.split('-').next().unwrap_or("");
        match primary {
            "zh" if code.split('-').skip(1).any(|p| matches!(p, "hant" | "tw" | "hk" | "mo")) => Some(Cjk::Traditional),
            "zh" => Some(Cjk::Simplified),
            "ja" => Some(Cjk::Japanese),
            "ko" => Some(Cjk::Korean),
            _ => None,
        }
    }

    /// Its face in Windows' font folder (file, face), regular or bold: Microsoft YaHei UI,
    /// Microsoft JhengHei UI, Yu Gothic UI, Malgun Gothic.
    fn windows(self, bold: bool) -> (&'static str, u32) {
        match (self, bold) {
            (Cjk::Simplified, false) => ("msyh.ttc", 1),
            (Cjk::Simplified, true) => ("msyhbd.ttc", 1),
            (Cjk::Traditional, false) => ("msjh.ttc", 1),
            (Cjk::Traditional, true) => ("msjhbd.ttc", 1),
            (Cjk::Japanese, false) => ("YuGothM.ttc", 1),
            (Cjk::Japanese, true) => ("YuGothB.ttc", 1),
            (Cjk::Korean, false) => ("malgun.ttf", 0),
            (Cjk::Korean, true) => ("malgunbd.ttf", 0),
        }
    }

    /// Its face in Noto Sans CJK's collection (Linux).
    fn noto(self) -> u32 {
        match self {
            Cjk::Japanese => 0,
            Cjk::Korean => 1,
            Cjk::Simplified => 2,
            Cjk::Traditional => 3,
        }
    }

    /// Its face as a font file from the system's fonts, when it has it: the file's bytes and the
    /// face in it.
    fn file(self, bold: bool) -> Option<(&'static [u8], u32)> {
        const NOTO: [&str; 4] = ["/usr/share/fonts/opentype/noto", "/usr/share/fonts/noto-cjk", "/usr/share/fonts/google-noto-cjk", "/usr/share/fonts/OTF"];
        let (file, index) = self.windows(bold);
        let noto_file = if bold { "NotoSansCJK-Bold.ttc" } else { "NotoSansCJK-Regular.ttc" };
        std::iter::once((windows_fonts().join(file), index)).chain(NOTO.iter().map(|dir| (Path::new(dir).join(noto_file), self.noto()))).find_map(|(path, index)| {
            let bytes = font_file(&path)?;
            has_face(bytes, index).then_some((bytes, index))
        })
    }

    /// The face from the system's fonts, when it has it, on a baseline with the interface
    /// font's (whose metrics are `ui`).
    fn face(self, bold: bool, ui: Option<VMetrics>) -> Option<Arc<FontData>> {
        const NOTO: [&str; 4] = ["/usr/share/fonts/opentype/noto", "/usr/share/fonts/noto-cjk", "/usr/share/fonts/google-noto-cjk", "/usr/share/fonts/OTF"];
        let (file, index) = self.windows(bold);
        let noto_file = if bold { "NotoSansCJK-Bold.ttc" } else { "NotoSansCJK-Regular.ttc" };
        std::iter::once((windows_fonts().join(file), index))
            .chain(NOTO.iter().map(|dir| (Path::new(dir).join(noto_file), self.noto())))
            .find_map(|(path, index)| {
                let bytes = font_file(&path)?;
                // (egui puts its baseline its own ascent below the top of its line, and
                // centres its line on the interface font's: moved onto that font's baseline,
                // as its capitals are centred.)
                let lift = ui.zip(vmetrics(bytes, index)).map_or(0.0, |(ui, own)| ui.ascent + ui.cap_centring() - own.ascent - (ui.row() - own.row()) / 2.0);
                has_face(bytes, index).then(|| Arc::new(FontData { font: Cow::Borrowed(bytes), index, tweak: FontTweak { y_offset_factor: lift, ..Default::default() } }))
            })
    }
}

/// Font faces by the names they are put in under.
type Faces = Vec<(String, Arc<FontData>)>;

/// Font files read so far, by path (none: unreadable).
type FontFiles = Mutex<Vec<(PathBuf, Option<&'static [u8]>)>>;

/// The faces for the scripts Bahnschrift lacks, from the system's fonts (never shipped):
/// (regular, bold). The interface language's first, regular and bold, so the characters
/// Chinese and Japanese share take its shapes; then Chinese and Korean, for names and
/// chat in them (and the languages' own names in the list of languages).
fn cjk_faces(language: &str, ui: Option<VMetrics>) -> (Faces, Faces) {
    let own = Cjk::of(language);
    let mut regular = Vec::new();
    let mut bold = Vec::new();
    let name = |script: Cjk, bold: bool| {
        let (file, index) = script.windows(bold);
        format!("cjk-{file}-{index}")
    };
    if let Some(script) = own
        && let Some(face) = script.face(true, ui)
    {
        bold.push((name(script, true), face));
    }
    for script in own.into_iter().chain([Cjk::Simplified, Cjk::Korean]) {
        if !regular.iter().any(|(n, _)| *n == name(script, false))
            && let Some(face) = script.face(false, ui)
        {
            regular.push((name(script, false), face));
        }
    }
    (regular, bold)
}

/// The system's font folder (Windows').
fn windows_fonts() -> PathBuf {
    std::env::var_os("WINDIR").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from).join("Fonts")
}

/// A font file's bytes, read once and kept: every interface (and every change of language)
/// shares them.
fn font_file(path: &Path) -> Option<&'static [u8]> {
    static FILES: FontFiles = Mutex::new(Vec::new());
    let mut files = FILES.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, bytes)) = files.iter().find(|(p, _)| p == path) {
        return *bytes;
    }
    let bytes = std::fs::read(path).ok().map(|b| &*Box::leak(b.into_boxed_slice()));
    files.push((path.to_path_buf(), bytes));
    bytes
}

/// The system's faces for the scripts Bahnschrift lacks (Chinese, Japanese and Korean), as font
/// files and the face in each, the interface language's own first: for text drawn outside the
/// interface, in the world (the credits' letters, `apps/ta_game/src/world_text.rs`). None when
/// the system has none.
pub fn fallback_faces() -> Vec<(&'static [u8], u32)> {
    let own = Cjk::of(&crate::i18n::language());
    let mut faces: Vec<(&'static [u8], u32)> = Vec::new();
    for script in own.into_iter().chain([Cjk::Simplified, Cjk::Japanese, Cjk::Korean, Cjk::Traditional]) {
        if let Some(face) = script.file(false)
            && !faces.iter().any(|f| std::ptr::eq(f.0.as_ptr(), face.0.as_ptr()) && f.1 == face.1)
        {
            faces.push(face);
        }
    }
    faces
}

/// Bahnschrift from the Windows font folder, if present and parseable (and not passed by:
/// `TA_UI_FONT=bundled`), read once.
fn bahnschrift() -> Option<&'static [u8]> {
    static FOUND: OnceLock<Option<&'static [u8]>> = OnceLock::new();
    *FOUND.get_or_init(|| {
        if std::env::var_os("TA_UI_FONT").is_some_and(|v| v.eq_ignore_ascii_case("bundled")) {
            return None;
        }
        let bytes = font_file(&windows_fonts().join("bahnschrift.ttf"))?;
        // egui panics on font data it cannot parse, so only accept a font whose variation axes
        // can be read — which also proves the weight axis we rely on exists.
        let has_weight_axis = has_face(bytes, 0) && FontData::from_static(bytes).variation_axes().iter().any(|axis| axis.tag.to_be_bytes() == *b"wght");
        if !has_weight_axis {
            log::info!("Bahnschrift unusable; the interface is in Sofia Sans");
            return None;
        }
        Some(bytes)
    })
}

// ---- Style --------------------------------------------------------------------------------

fn apply_style(style: &mut Style) {
    style.visuals = visuals_of(Palette::REBORN);
    roomy(style);
    style.animation_time = 0.12;
}

/// Text sizes and spacing of the launcher and the game's pages and dialogs.
pub fn roomy(style: &mut Style) {
    style.text_styles = [
        (TextStyle::Small, body(12.0)),
        (TextStyle::Body, body(14.5)),
        (TextStyle::Button, strong(14.0)),
        (TextStyle::Heading, display(26.0)),
        (TextStyle::Monospace, mono(12.5)),
    ]
    .into();

    let spacing = &mut style.spacing;
    spacing.item_spacing = vec2(8.0, 7.0);
    spacing.button_padding = vec2(12.0, 5.0);
    spacing.interact_size = vec2(40.0, 26.0);
    spacing.slider_width = 230.0;
    spacing.combo_width = 220.0;
    spacing.text_edit_width = 230.0;
    spacing.icon_width = 16.0;
    spacing.icon_width_inner = 9.0;
    spacing.icon_spacing = 8.0;
    spacing.menu_margin = Margin::same(6);
    spacing.window_margin = Margin::same(12);
    // A scroll bar in sight before the pointer comes near (see `crate::scroll`).
    spacing.scroll = crate::scroll::style();
}

/// Denser text and spacing for the in-game HUD, with the same colours.
pub fn compact(style: &mut Style) {
    style.text_styles = [
        (TextStyle::Small, body(11.0)),
        (TextStyle::Body, body(13.0)),
        (TextStyle::Button, strong(12.5)),
        (TextStyle::Heading, display(22.0)),
        (TextStyle::Monospace, mono(11.5)),
    ]
    .into();
    let spacing = &mut style.spacing;
    spacing.item_spacing = vec2(8.0, 3.0);
    spacing.button_padding = vec2(4.0, 1.0);
    spacing.interact_size = vec2(40.0, 18.0);
    spacing.slider_width = 100.0;
    spacing.combo_width = 100.0;
    spacing.text_edit_width = 280.0;
    spacing.icon_width = 14.0;
    spacing.icon_width_inner = 8.0;
    spacing.icon_spacing = 4.0;
    spacing.menu_margin = Margin::same(6);
    spacing.window_margin = Margin::same(8);
    // egui's own scroll bars in the HUD (its build menu's grid keeps its width).
    spacing.scroll = egui::style::ScrollStyle::default();
}

/// Custom dark visuals: brown-black metal, square-ish corners, brass focus; in `p`'s colours (TA
/// Reborn's own, or a theme's in the HUD).
fn visuals_of(p: Palette) -> Visuals {
    let mut v = Visuals::dark();
    v.weak_text_color = Some(p.steel);
    v.hyperlink_color = p.accent_bright;
    v.faint_bg_color = p.faint_bg;
    v.extreme_bg_color = p.inset;
    v.text_edit_bg_color = Some(p.inset);
    v.code_bg_color = p.panel_hi;
    v.warn_fg_color = p.accent;
    v.error_fg_color = p.bad;

    v.window_corner_radius = CornerRadius::same(3);
    v.window_fill = p.panel;
    v.window_stroke = Stroke::new(1.0, p.line_hi);
    v.window_shadow = Shadow { offset: [0, 8], blur: 24, spread: 0, color: Color32::from_black_alpha(150) };
    v.menu_corner_radius = CornerRadius::same(2);
    v.popup_shadow = Shadow { offset: [0, 6], blur: 16, spread: 0, color: Color32::from_black_alpha(150) };
    v.panel_fill = p.bg;

    v.selection = Selection { bg_fill: p.selection, stroke: Stroke::new(1.0, p.accent) };
    v.widgets = widgets_of(p);
    v.text_cursor.stroke = Stroke::new(2.0, p.accent);
    v.slider_trailing_fill = true;
    v.handle_shape = HandleShape::Rect { aspect_ratio: 0.55 };
    v.interact_cursor = Some(CursorIcon::PointingHand);
    v
}

fn widgets_of(p: Palette) -> Widgets {
    let radius = CornerRadius::same(2);
    let visuals = |fill: Color32, border: Color32, text: Color32, text_width: f32| WidgetVisuals {
        bg_fill: fill,
        weak_bg_fill: fill,
        bg_stroke: Stroke::new(1.0, border),
        corner_radius: radius,
        fg_stroke: Stroke::new(text_width, text),
        expansion: 0.0,
    };
    Widgets {
        // Labels, separators and frames.
        noninteractive: visuals(p.panel, p.line, p.text, 1.0),
        inactive: visuals(p.widget, p.line_hi, p.text, 1.0),
        hovered: visuals(p.widget_hover, p.accent_dim, p.text_bright, 1.5),
        active: visuals(p.accent_deep, p.accent, p.accent_bright, 2.0),
        open: visuals(p.panel_hi, p.accent_dim, p.text_bright, 1.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every face's capitals sit in the middle of their line: Bahnschrift's (where Windows has
    /// it) and Sofia Sans's.
    #[test]
    fn capitals_sit_in_the_middle_of_their_line() {
        for bahnschrift in [bahnschrift(), None] {
            let ctx = egui::Context::default();
            install(&ctx);
            ctx.set_fonts(definitions(bahnschrift, "en", None));
            ctx.run_ui(Default::default(), |ui| {
                for font in [display(30.0), strong(12.0), body(14.5)] {
                    let galley = ui.painter().layout_no_wrap("H".to_owned(), font.clone(), Color32::WHITE);
                    let glyph = &galley.rows[0].row.glyphs[0];
                    let ink = galley.rows[0].pos.y + glyph.pos.y + glyph.uv_rect.offset.y + glyph.uv_rect.size.y / 2.0;
                    assert!((ink - galley.size().y / 2.0).abs() < 0.75, "{font:?} in {bahnschrift:?}: capitals centred at {ink}, the line's middle {}", galley.size().y / 2.0, bahnschrift = bahnschrift.map(|_| "Bahnschrift"));
                }
            })
            .drop_without_applying_deltas();
        }
    }

    /// Without Bahnschrift the interface is in Sofia Sans: its text and titles in their own
    /// files, its lines as tall as Bahnschrift's 1.2 em and its text no wider (where Windows has
    /// it to compare, laid out as the interface lays them out), so the layouts made for
    /// Bahnschrift hold.
    #[test]
    fn without_bahnschrift_the_interface_is_in_sofia_sans() {
        // (By their bytes: a debug build may keep two copies of one embedded file, at two places.)
        assert!(face_of(None, Weight::Regular).bytes == SOFIA_SANS_SEMI_CONDENSED);
        assert!(face_of(None, Weight::Strong).bytes == SOFIA_SANS_SEMI_CONDENSED);
        assert!(face_of(None, Weight::Display).bytes == SOFIA_SANS_EXTRA_CONDENSED);
        for bytes in [SOFIA_SANS_SEMI_CONDENSED, SOFIA_SANS_EXTRA_CONDENSED] {
            assert!(has_face(bytes, 0));
            let axes = FontData::from_static(bytes).variation_axes();
            assert!(axes.iter().any(|axis| axis.tag.to_be_bytes() == *b"wght"), "a weight axis");
            let line = vmetrics(bytes, 0).unwrap().row();
            assert!((line - 1.2).abs() < 0.002, "a line {line} em high");
        }
        let Some(bahnschrift) = bahnschrift() else { return };
        // As tall and as wide, laid out as the interface lays them out.
        let size = |bahnschrift: Option<&'static [u8]>, text: &str, font: FontId| {
            let ctx = egui::Context::default();
            install(&ctx);
            ctx.set_fonts(definitions(bahnschrift, "en", None));
            let mut size = egui::Vec2::ZERO;
            ctx.run_ui(Default::default(), |ui| size = ui.painter().layout_no_wrap(text.to_owned(), font.clone(), Color32::WHITE).size()).drop_without_applying_deltas();
            size
        };
        for (text, font) in [
            ("Set up for this computer. They are yours to change in Settings.", body(14.5)),
            ("Настройки игры, звук и управление", body(14.5)),
            ("RESTORE DEFAULTS", strong(14.0)),
            ("SINGLE PLAYER", display(26.0)),
            ("НАСТРОЙКИ", display(26.0)),
        ] {
            let (ours, sofia) = (size(Some(bahnschrift), text, font.clone()), size(None, text, font.clone()));
            let wide = sofia.x / ours.x;
            assert!((0.84..=1.0).contains(&wide), "{text:?}: Sofia Sans {:.1} points wide, Bahnschrift {:.1} ({wide:.2})", sofia.x, ours.x);
            assert!((sofia.y - ours.y).abs() < 0.3, "{text:?}: a line of Sofia Sans {:.2} points high, of Bahnschrift {:.2}", sofia.y, ours.y);
        }
    }

    /// Sofia Sans draws Cyrillic as Russian and Ukrainian readers know it: the Bulgarian forms
    /// it draws by default (в as tall as b, т as wide as m) are mapped away in the bundled copy
    /// (`tools/fonts/sofia_sans.py`), since egui's shaping never asks for its Russian ones.
    #[test]
    fn sofia_sans_draws_cyrillic_in_the_forms_russian_and_ukrainian_use() {
        let ctx = egui::Context::default();
        install(&ctx);
        ctx.set_fonts(definitions(None, "ru", None));
        ctx.run_ui(Default::default(), |ui| {
            for font in [body(40.0), display(40.0)] {
                // The ink of a letter: its width and height.
                let ink = |letter: &str| {
                    let galley = ui.painter().layout_no_wrap(letter.to_owned(), font.clone(), Color32::WHITE);
                    galley.rows[0].row.glyphs[0].uv_rect.size
                };
                let (ve, b, x) = (ink("в"), ink("b"), ink("x"));
                assert!(ve.y < b.y * 0.85 && (ve.y - x.y).abs() < x.y * 0.1, "{font:?}: в {:.1} high, b {:.1}, x {:.1}: in its Bulgarian form", ve.y, b.y, x.y);
                let (te, m) = (ink("т"), ink("m"));
                assert!(te.x < m.x * 0.8, "{font:?}: т {:.1} wide, m {:.1}: in its Bulgarian form", te.x, m.x);
            }
        })
        .drop_without_applying_deltas();
    }

    /// Where a font's table `tag` starts.
    fn table(font: &[u8], tag: &[u8; 4]) -> usize {
        let tables = u16::from_be_bytes([font[4], font[5]]) as usize;
        let record = (0..tables).map(|i| 12 + 16 * i).find(|&r| &font[r..r + 4] == tag).unwrap_or_else(|| panic!("no {} table", String::from_utf8_lossy(tag)));
        u32::from_be_bytes(font[record + 8..record + 12].try_into().unwrap()) as usize
    }

    /// Whether a font's character map (its Unicode table for the Basic Multilingual Plane,
    /// format 4) has `c`.
    fn maps(font: &[u8], c: char) -> bool {
        let be16 = |at: usize| u16::from_be_bytes([font[at], font[at + 1]]) as usize;
        let be32 = |at: usize| u32::from_be_bytes(font[at..at + 4].try_into().unwrap()) as usize;
        let cmap = table(font, b"cmap");
        let table = (0..be16(cmap + 2)).map(|i| cmap + 4 + 8 * i).find(|&r| (be16(r), be16(r + 2)) == (3, 1)).map(|r| cmap + be32(r + 4)).expect("Windows' Unicode table");
        assert_eq!(be16(table), 4, "format 4");
        let segments = be16(table + 6) / 2;
        let (ends, starts, deltas, offsets) = (table + 14, table + 16 + 2 * segments, table + 16 + 4 * segments, table + 16 + 6 * segments);
        let c = c as usize;
        if c > 0xFFFF {
            return false;
        }
        let Some(i) = (0..segments).find(|&i| be16(ends + 2 * i) >= c) else { return false };
        let start = be16(starts + 2 * i);
        if c < start {
            return false;
        }
        let offset = be16(offsets + 2 * i);
        let glyph = if offset == 0 {
            (c + be16(deltas + 2 * i)) & 0xFFFF
        } else {
            let at = offsets + 2 * i + offset + 2 * (c - start);
            match be16(at) {
                0 => 0,
                g => (g + be16(deltas + 2 * i)) & 0xFFFF,
            }
        };
        glyph != 0
    }

    /// Sofia Sans has every letter of every translation but Chinese, Japanese and Korean (which
    /// come from the system's fonts): Russian and Ukrainian above all (Barlow, first thought of,
    /// has no Cyrillic), Greek too.
    #[test]
    fn sofia_sans_has_every_letter_of_the_translations() {
        let cjk = |c: char| matches!(c as u32, 0x1100..=0x11FF | 0x2E80..=0x9FFF | 0xAC00..=0xD7AF | 0xF900..=0xFAFF | 0xFF00..=0xFFEF);
        let lang = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../lang");
        let mut letters = std::collections::BTreeSet::new();
        for entry in std::fs::read_dir(&lang).expect("the translations") {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "toml") {
                letters.extend(std::fs::read_to_string(&path).unwrap().chars().filter(|&c| c.is_alphanumeric() && !cjk(c)));
            }
        }
        assert!(['д', 'ї', 'ґ', 'ł', 'ğ', 'ã', 'ß'].iter().all(|c| letters.contains(c)), "the translations' letters read");
        // (And the letters of every language's own name, and Greek's, for names and chat.)
        letters.extend("ΑαΒβΓγΔδΕεΖζΗηΘθΙιΚκΛλΜμΝνΞξΟοΠπΡρΣσςΤτΥυΦφΧχΨψΩωάέήίόύώ".chars());
        for (name, font) in [("Sofia Sans Semi Condensed", SOFIA_SANS_SEMI_CONDENSED), ("Sofia Sans Extra Condensed", SOFIA_SANS_EXTRA_CONDENSED)] {
            let missing: String = letters.iter().filter(|&&c| !maps(font, c)).collect();
            assert!(missing.is_empty(), "{name} lacks {missing:?}");
        }
    }

    /// A battle's palette with a blue accent and a panel of its own.
    fn blue() -> Palette {
        Palette::REBORN.themed(&|key| match key {
            "accent" => Some(Color32::from_rgb(90, 180, 255)),
            "panel" => Some(Color32::from_rgb(24, 32, 43)),
            _ => None,
        })
    }

    /// The colour egui's widgets letter what is chosen in: the accent of the look they are in.
    fn chosen(style: &Style) -> Color32 {
        style.visuals.selection.stroke.color
    }

    /// The battle's palette is drawn inside the HUD and nowhere else: egui's own widgets there, and
    /// what egui makes of itself (tooltips, drop-downs: the context's style), with it; outside it
    /// and in a dialog inside it (TA Reborn's plates), TA Reborn's own, all put back as it ends.
    #[test]
    fn the_hud_alone_is_drawn_in_the_battles_palette() {
        let ctx = egui::Context::default();
        install_compact(&ctx);
        let battle = blue();
        set_battle_palette(battle);
        assert_eq!(pal(), Palette::REBORN, "outside the HUD");
        ctx.run_ui(Default::default(), |ui| {
            hud(ui, |ui| {
                assert!(in_hud());
                assert_eq!(pal(), battle);
                assert_eq!(chosen(ui.style()), battle.accent, "the HUD's widgets");
                assert_eq!(chosen(&ui.ctx().global_style()), battle.accent, "what egui makes in it");
                reborn(ui, |ui| {
                    assert_eq!(pal(), Palette::REBORN);
                    assert_eq!(chosen(ui.style()), AMBER);
                    assert_eq!(chosen(&ui.ctx().global_style()), AMBER, "a dialog's hints");
                });
                assert_eq!(pal(), battle, "the HUD again after a dialog in it");
                assert_eq!(chosen(&ui.ctx().global_style()), battle.accent);
                // A panel (a dialog) is TA Reborn's within, wherever it is shown.
                crate::panel::Panel::new("SHARE").show(ui, |ui| {
                    assert_eq!(pal(), Palette::REBORN);
                    assert_eq!(chosen(ui.style()), AMBER);
                });
                assert_eq!(pal(), battle);
            });
            assert!(!in_hud());
            assert_eq!(pal(), Palette::REBORN);
            assert_eq!(chosen(ui.style()), AMBER, "the window's own after the HUD");
            assert_eq!(chosen(&ui.ctx().global_style()), AMBER);
        })
        .drop_without_applying_deltas();
        set_battle_palette(Palette::REBORN);
    }

    /// The style lent to the HUD is made once: frame after frame, the same (no copy a frame), and
    /// TA Reborn's own given back each time.
    #[test]
    fn the_huds_style_is_made_once() {
        let ctx = egui::Context::default();
        install_compact(&ctx);
        set_battle_palette(blue());
        let own = ctx.style_of(Theme::Dark);
        let mut lent = Vec::new();
        for _ in 0..3 {
            ctx.run_ui(Default::default(), |ui| hud(ui, |ui| lent.push(ui.ctx().style_of(Theme::Dark)))).drop_without_applying_deltas();
            assert!(Arc::ptr_eq(&ctx.style_of(Theme::Dark), &own), "the context's own put back");
        }
        assert!(!Arc::ptr_eq(&lent[0], &own));
        assert!(lent.windows(2).all(|w| Arc::ptr_eq(&w[0], &w[1])), "the same style lent each frame");
        // Without a theme there is nothing to lend.
        set_battle_palette(Palette::REBORN);
        ctx.run_ui(Default::default(), |ui| hud(ui, |ui| assert!(Arc::ptr_eq(&ui.ctx().style_of(Theme::Dark), &own)))).drop_without_applying_deltas();
    }

    /// The first family a context's fonts set `family` in.
    fn first_face(ctx: &egui::Context, family: &str) -> String {
        ctx.fonts(|fonts| fonts.definitions().families[&FontFamily::Name(family.into())][0].clone())
    }

    /// A theme's font is only in the contexts given it, the battle's window's and the companion
    /// screen's that follows it: a context made afterwards (the loading screen's, the cockpit
    /// instruments') never has it, nor do TA Reborn's own families anywhere. Only the HUD's
    /// lettering is in it ([`strong`], [`display`] and egui's buttons and headings inside [`hud`]).
    #[test]
    fn a_themes_font_is_only_in_the_huds_of_the_contexts_given_it() {
        let battle = egui::Context::default();
        install_compact(&battle);
        assert!(!set_battle_font(&battle, Some(Arc::new(b"not a font".to_vec()))), "a font that does not read is refused");
        assert!(set_battle_font(&battle, Some(Arc::new(SOFIA_SANS_EXTRA_CONDENSED.to_vec()))));
        let loading = egui::Context::default();
        install_compact(&loading);
        let companion = egui::Context::default();
        install_compact(&companion);
        follow_fonts(&companion, &battle);
        for (ctx, has, name) in [(&battle, true, "the battle's"), (&loading, false, "a context made afterwards"), (&companion, true, "the companion's")] {
            assert_eq!(has_battle_font(ctx), has, "{name}");
            ctx.run_ui(Default::default(), |_| {}).drop_without_applying_deltas();
            assert_eq!(first_face(ctx, STRONG_FAMILY), "ui-strong", "{name}: TA Reborn's own families never hold it");
            assert_eq!(first_face(ctx, DISPLAY_FAMILY), "ui-display", "{name}");
            assert_eq!(first_face(ctx, HUD_STRONG_FAMILY), if has { "battle" } else { "ui-strong" }, "{name}");
            assert_eq!(first_face(ctx, HUD_DISPLAY_FAMILY), if has { "battle" } else { "ui-display" }, "{name}");
        }
        let family = |font: FontId| match font.family {
            FontFamily::Name(name) => name.to_string(),
            other => format!("{other:?}"),
        };
        battle
            .run_ui(Default::default(), |ui| {
                assert_eq!(family(strong(12.0)), STRONG_FAMILY, "outside the HUD");
                hud(ui, |ui| {
                    assert_eq!(family(strong(12.0)), HUD_STRONG_FAMILY);
                    assert_eq!(family(display(20.0)), HUD_DISPLAY_FAMILY);
                    assert_eq!(family(ui.style().text_styles[&TextStyle::Button].clone()), HUD_STRONG_FAMILY, "egui's buttons");
                    assert_eq!(family(ui.style().text_styles[&TextStyle::Body].clone()), "Proportional", "body text stays the interface's");
                    reborn(ui, |ui| {
                        assert_eq!(family(strong(12.0)), STRONG_FAMILY, "a dialog in the HUD");
                        assert_eq!(family(ui.style().text_styles[&TextStyle::Button].clone()), STRONG_FAMILY);
                    });
                });
                assert_eq!(family(ui.style().text_styles[&TextStyle::Button].clone()), STRONG_FAMILY);
            })
            .drop_without_applying_deltas();
        loading.run_ui(Default::default(), |ui| hud(ui, |_| assert_eq!(family(strong(12.0)), STRONG_FAMILY, "no theme's font in this context"))).drop_without_applying_deltas();
        // Put away, and the companion follows.
        assert!(set_battle_font(&battle, None));
        follow_fonts(&companion, &battle);
        assert!(!has_battle_font(&battle) && !has_battle_font(&companion));
    }

    /// `font` (a TrueType font) with `more` ems of line gap: its line that much taller, its letters
    /// as they were.
    fn line_gapped(font: &[u8], more: f32) -> Vec<u8> {
        let mut font = font.to_vec();
        let be16 = |font: &[u8], at: usize| u16::from_be_bytes([font[at], font[at + 1]]);
        let units = f32::from(be16(&font, table(&font, b"head") + 18));
        for at in [table(&font, b"hhea") + 8, table(&font, b"OS/2") + 72] {
            let gap = be16(&font, at) as i16 as f32 + more * units;
            font[at..at + 2].copy_from_slice(&(gap.round() as i16).to_be_bytes());
        }
        font
    }

    /// A theme's font with a taller line than the interface's (half an em more) is set on the
    /// interface's lines: a row of the HUD's lettering in it as tall as one in the interface's own,
    /// so the rows laid out for 1.2 em hold, its capitals in the middle of the row.
    #[test]
    fn a_themes_font_is_set_on_the_interfaces_lines() {
        let tall = line_gapped(SOFIA_SANS_EXTRA_CONDENSED, 0.5);
        let row = vmetrics(&tall, 0).unwrap().row();
        assert!((row - 1.7).abs() < 0.01, "a line {row} em high");
        assert!(font_reads(&tall));
        let interface = vmetrics(face_of(bahnschrift(), Weight::Strong).bytes, 0);
        assert!((line_scale(vmetrics(&tall, 0), interface) - 1.2 / 1.7).abs() < 0.01);
        assert_eq!(line_scale(None, interface), 1.0, "metrics it has not: as it is");
        let ctx = egui::Context::default();
        install_compact(&ctx);
        assert!(set_battle_font(&ctx, Some(Arc::new(tall))));
        ctx.run_ui(Default::default(), |_| {}).drop_without_applying_deltas();
        ctx.run_ui(Default::default(), |ui| {
            for size in [12.0, 20.0, 31.0] {
                let ours = ui.painter().layout_no_wrap("H".to_owned(), strong(size), Color32::WHITE);
                let theirs = hud(ui, |ui| ui.painter().layout_no_wrap("H".to_owned(), strong(size), Color32::WHITE));
                assert!((theirs.size().y - ours.size().y).abs() <= 1.0, "at {size}: a row of the theme's font {} high, of the interface's {}", theirs.size().y, ours.size().y);
                let glyph = &theirs.rows[0].row.glyphs[0];
                let ink = theirs.rows[0].pos.y + glyph.pos.y + glyph.uv_rect.offset.y + glyph.uv_rect.size.y / 2.0;
                assert!((ink - theirs.size().y / 2.0).abs() < 1.0, "at {size}: its capitals centred at {ink}, the row's middle {}", theirs.size().y / 2.0);
            }
        })
        .drop_without_applying_deltas();
    }

    /// The interface's titles and buttons are as tall a line in their two faces (the theme's font
    /// is set to one: [`battle_face`]).
    #[test]
    fn the_interfaces_strong_and_display_faces_have_lines_as_tall() {
        for bahnschrift in [bahnschrift(), None] {
            let row = |weight| vmetrics(face_of(bahnschrift, weight).bytes, 0).unwrap().row();
            assert!((row(Weight::Strong) - row(Weight::Display)).abs() < 0.01);
        }
    }

    /// A colour that goes with a palette colour is itself where a theme leaves that colour, to the
    /// unit, channel by channel; and moves with it where a theme moves it.
    #[test]
    fn a_fixed_shade_follows_its_palette_colour() {
        let brass = Color32::from_rgb(230, 196, 110);
        assert_eq!(retinted(brass, AMBER, AMBER), brass);
        // (Its arithmetic, not only its shortcut: red and green as they were, only blue moved.)
        for c in 0..=255u8 {
            let colour = Color32::from_rgb(c, 255 - c, 9);
            let moved = retinted(colour, AMBER, Color32::from_rgb(AMBER.r(), AMBER.g(), 200));
            assert_eq!((moved.r(), moved.g()), (c, 255 - c));
            assert!(moved.b() > 9);
        }
        let blue = retinted(brass, AMBER, Color32::from_rgb(90, 180, 255));
        assert!(blue.b() > blue.g() && blue.g() > blue.r(), "{blue:?}");
        let darker = retinted(brass, AMBER, Color32::from_rgb(108, 90, 43));
        assert!(darker.r() < brass.r() && darker.g() < brass.g() && darker.b() < brass.b());
    }

    /// What reads on its backdrop is left as it is (TA Reborn's own colours on its dark plates and
    /// frames); what does not is lit or darkened, away from the backdrop, just enough.
    #[test]
    fn legible_colours_read_on_their_backdrop() {
        let frame = Color32::from_rgb(43, 43, 43);
        for colour in [AMBER_BRIGHT, GREEN, TEXT, Color32::from_rgb(255, 90, 70), Color32::from_rgb(255, 132, 112)] {
            assert_eq!(legible(colour, frame, 3.0), colour, "{colour:?}");
        }
        for colour in [TEXT, STEEL, AMBER, GREEN] {
            assert_eq!(legible(colour, PANEL, 4.5), colour, "{colour:?} on TA Reborn's panel");
        }
        let navy = Color32::from_rgb(31, 95, 191);
        let lit = legible(navy, frame, 3.0);
        assert!((3.0..3.3).contains(&contrast(lit, frame)), "lit just enough: {lit:?}");
        let light_panel = Color32::from_rgb(228, 232, 238);
        let green = Color32::from_rgb(120, 220, 140);
        let darker = legible(green, light_panel, 4.5);
        assert!(contrast(darker, light_panel) >= 4.5 && luminance(darker) < luminance(green), "{darker:?}");
        assert_eq!(legible(Color32::from_gray(128), Color32::from_gray(128), 30.0), Color32::BLACK, "as far as it goes");
    }
}
