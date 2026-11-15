//! The checks of how a mod presents itself in the launcher (its manifest's `[media]`, `[links]`
//! and `source`): pictures that are in the mod's folder, read, and are of a size and a shape the
//! launcher shows well; a gameplay preview that is an animated GIF it plays; links it opens.
//! Notes of their own (`presentation.*`), never errors or warnings: a mod plays the same whatever
//! its pictures, and its badge is how it plays.

use std::path::Path;

use ta_vfs::mods::{MAX_SCREENSHOTS, ModManifest, media_path, web_address};

use crate::validation::{Issue, Severity};
use crate::words::{Fill, Words, en, enf};

/// The most a mod's picture (its banner, icon or a screenshot) may weigh for the launcher to
/// show it.
pub const PICTURE_MAX_BYTES: u64 = 16 << 20;
/// The most a mod's gameplay preview may weigh for the launcher to play it.
pub const PREVIEW_MAX_BYTES: u64 = 32 << 20;
/// The widest and the tallest a picture the launcher reads may be.
pub const PICTURE_MAX_SIDE: u32 = 4096;
/// How many of a preview's frames the launcher reads (it plays a longer one to there).
pub const PREVIEW_MAX_FRAMES: usize = 1000;
/// A preview's frame shown for less than this (as many GIFs ask, 0 or 1 hundredth of a second)
/// is shown this long, as web browsers show it.
pub const PREVIEW_LEAST_DELAY: f32 = 0.1;

/// The kinds of picture the launcher shows (extensions, lower case).
const PICTURES: &[&str] = &["png", "jpg", "jpeg", "gif"];

/// What a mod's manifest names of its pictures and links that the launcher would not show or
/// open as meant, and of its interface theme that the game would not draw as meant (`theme.*`).
/// `dir` is the mod's folder; the findings are on `layer`'s (`mod:<id>`); `sides` are the sides
/// of the game with the mod (`ta_data::side_names`), where known: a theme's side of no such name
/// is told.
pub fn check(dir: &Path, manifest: &ModManifest, layer: &str, sides: Option<&[String]>) -> Vec<Issue> {
    let mut issues = Vec::new();
    let mut note = |code: &str, path: &str, words: Words, suggestion: &'static str| {
        issues.push(Issue {
            severity: Severity::Info, code: code.into(), path: path.into(), layer: layer.into(),
            message: words.english(), words, suggestion: suggestion.into(), ..Issue::default()
        });
    };
    let media = &manifest.media;
    let pictures = [("banner", &media.banner, Kind::Banner), ("preview", &media.preview, Kind::Preview), ("icon", &media.icon, Kind::Icon)];
    let screenshots = media.shots().map(|file| ("screenshots", file, Kind::Screenshot));
    for (key, named, kind) in pictures.into_iter().map(|(key, named, kind)| (key, named.as_str(), kind)).chain(screenshots) {
        if named.trim().is_empty() { continue; }
        let shown = named.trim().replace('\\', "/");
        let Some(path) = media_path(dir, named) else {
            let leaves = shown.starts_with('/') || shown.contains(':') || shown.split('/').any(|part| part == "..");
            if leaves {
                note("presentation.outside", "mod.toml", enf("mod.toml's {key} names {path}, which is outside the mod's folder: the launcher shows pictures of the mod's own folder only.", [("key", Fill::name(key)), ("path", Fill::name(&shown))]),
                    en("Put the picture in the mod's media folder and name it from the mod's folder (media/banner.png)."));
            } else {
                note("presentation.missing", "mod.toml", enf("mod.toml's {key} names {path}, which is not in the mod's folder: the launcher shows the mod without it.", [("key", Fill::name(key)), ("path", Fill::name(&shown))]),
                    en("Add the picture, or correct its path in mod.toml: it is read from the mod's folder (media/banner.png)."));
            }
            continue;
        };
        let at = path.strip_prefix(dir).map_or(shown.clone(), |p| p.to_string_lossy().replace('\\', "/"));
        let extension = at.rsplit_once('.').map_or(String::new(), |(_, e)| e.to_ascii_lowercase());
        if !PICTURES.contains(&extension.as_str()) {
            note("presentation.format", &at, enf("{path} is not a PNG, JPEG or GIF picture: the launcher shows only those.", [("path", Fill::name(&at))]),
                en("Save the picture as a PNG or a JPEG (an animated GIF for the gameplay preview)."));
            continue;
        }
        let bytes = std::fs::metadata(&path).map_or(0, |m| m.len());
        let most = if kind == Kind::Preview { PREVIEW_MAX_BYTES } else { PICTURE_MAX_BYTES };
        if bytes > most {
            note("presentation.too_large", &at, enf("{path} weighs {size}: the launcher shows pictures of up to {most} only.", [("path", Fill::name(&at)), ("size", Fill::name(megabytes(bytes))), ("most", Fill::name(megabytes(most)))]),
                en("Make it smaller: fewer pixels, a JPEG for a photograph, fewer frames or colours for a GIF."));
            continue;
        }
        let (width, height) = match dimensions(&path) {
            Ok(size) => size,
            Err(error) => {
                note("presentation.unreadable", &at, enf("{path} does not read as a picture ({error}): the launcher shows the mod without it.", [("path", Fill::name(&at)), ("error", Fill::Words(Words::raw(error)))]),
                    en("Save the picture again with an ordinary picture editor."));
                continue;
            }
        };
        if width > PICTURE_MAX_SIDE || height > PICTURE_MAX_SIDE {
            note("presentation.too_large", &at, enf("{path} is {width} × {height}: the launcher shows pictures of up to {most} pixels a side only.", [("path", Fill::name(&at)), ("width", Fill::number(width)), ("height", Fill::number(height)), ("most", Fill::number(PICTURE_MAX_SIDE))]),
                en("Make it smaller: the launcher never shows a picture larger than the screen."));
            continue;
        }
        let ratio = width as f32 / height.max(1) as f32;
        match kind {
            Kind::Banner => {
                if width < 640 || height < 320 {
                    note("presentation.small", &at, enf("The banner is {width} × {height}: the library enlarges it, and it shows blurred. A banner of 1280 × 640 shows sharp.", [("width", Fill::number(width)), ("height", Fill::number(height))]),
                        en("Give it more pixels: 1280 × 640 at best."));
                }
                if !(1.5..=3.2).contains(&ratio) {
                    note("presentation.shape", &at, enf("The banner is {width} × {height}: the library shows it at twice as wide as tall, so much of it is cut off.", [("width", Fill::number(width)), ("height", Fill::number(height))]),
                        en("Make the banner twice as wide as it is tall (1280 × 640), its subject in the middle."));
                }
            }
            Kind::Icon => {
                if !(0.85..=1.18).contains(&ratio) {
                    note("presentation.shape", &at, enf("The icon is {width} × {height}: it is shown square, its sides cut off.", [("width", Fill::number(width)), ("height", Fill::number(height))]),
                        en("Make the icon square (256 × 256)."));
                }
                if width.min(height) < 64 {
                    note("presentation.small", &at, enf("The icon is {width} × {height}: it shows blurred. One of 256 × 256 shows sharp.", [("width", Fill::number(width)), ("height", Fill::number(height))]),
                        en("Give it more pixels: 256 × 256 at best."));
                }
            }
            Kind::Preview => match preview_frames(&path) {
                Ok((frames, _)) if frames < 2 => {
                    note("presentation.still", &at, enf("The gameplay preview {path} is a still picture: the launcher plays animated GIFs.", [("path", Fill::name(&at))]),
                        en("Save the gameplay preview as an animated GIF (a few seconds of the mod in play)."));
                }
                Ok((frames, seconds)) if frames > PREVIEW_MAX_FRAMES || seconds > 30.0 => {
                    note("presentation.long", &at, enf("The gameplay preview has {frames} frames ({seconds} seconds): the launcher keeps a preview's frames in memory, so a long one plays choppier, in fewer of them.", [("frames", Fill::number(frames)), ("seconds", Fill::number(seconds.round() as u32))]),
                        en("Keep the gameplay preview to a few seconds (5 to 15), about 640 pixels wide."));
                }
                Ok(_) => {}
                Err(error) => {
                    note("presentation.unreadable", &at, enf("{path} does not read as a picture ({error}): the launcher shows the mod without it.", [("path", Fill::name(&at)), ("error", Fill::Words(Words::raw(error)))]),
                        en("Save the picture again with an ordinary picture editor."));
                }
            },
            Kind::Screenshot => {
                if width < 640 || height < 360 {
                    note("presentation.small", &at, enf("The screenshot {path} is {width} × {height}: shown large, it shows blurred.", [("path", Fill::name(&at)), ("width", Fill::number(width)), ("height", Fill::number(height))]),
                        en("Take the screenshot at the game's full size (1920 × 1080 at best)."));
                }
            }
        }
    }
    let named = media.screenshots.iter().filter(|file| !file.trim().is_empty()).count();
    if named > MAX_SCREENSHOTS {
        note("presentation.many", "mod.toml", enf("mod.toml names {count} screenshots: the launcher shows the first {most}.", [("count", Fill::number(named)), ("most", Fill::number(MAX_SCREENSHOTS))]),
            en("Keep the best of them, in the order they are to be seen."));
    }
    let links = (!manifest.source.trim().is_empty()).then_some(("source", &manifest.source)).into_iter()
        .chain(manifest.links.iter().map(|(label, url)| (label.as_str(), url)));
    for (label, url) in links {
        if !web_address(url) {
            note("presentation.link", "mod.toml", enf("mod.toml's link {label} ({url}) is not a web address: the launcher opens http:// and https:// addresses only.", [("label", Fill::name(label)), ("url", Fill::name(url.trim()))]),
                en("Give the page's whole address, starting https://."));
        }
    }
    check_theme(dir, &manifest.theme, sides, &mut note);
    issues
}

/// The most a theme's picture may be a side (the HUD reads up to it).
const THEME_PICTURE_SIDE: u32 = 2048;
/// The most a theme's font may weigh.
const THEME_FONT_MAX_BYTES: u64 = 16 << 20;

/// The checks of a mod's interface theme (`[theme]`): its colours colours, its pictures pictures
/// the HUD reads, its font one the game reads (as its interface does, `skrifa`), its text and
/// accent clear on its panels, its sides the game's.
fn check_theme(dir: &Path, theme: &ta_vfs::mods::Theme, sides: Option<&[String]>, note: &mut dyn FnMut(&str, &str, Words, &'static str)) {
    if theme.is_empty() { return; }
    use ta_vfs::mods::LOOK_COLOURS;
    // Its colours: the theme's, each side's, the classic HUD's.
    let looks = std::iter::once((String::new(), &theme.look)).chain(theme.sides.iter().map(|(side, look)| (format!("sides.{side}."), look)));
    let mut colours: Vec<(String, &str)> = Vec::new();
    for (prefix, look) in looks {
        for key in LOOK_COLOURS {
            if let Some(value) = look.colour(key) { colours.push((format!("{prefix}{key}"), value)); }
        }
    }
    let classic = &theme.classic;
    for (key, value) in [("classic.text", &classic.text), ("classic.button_text", &classic.button_text), ("classic.dim", &classic.dim), ("classic.pressed", &classic.pressed)] {
        if !value.trim().is_empty() { colours.push((key.to_string(), value.trim())); }
    }
    for (key, value) in &colours {
        if parse_colour(value).is_none() {
            note("theme.colour", "mod.toml", enf("The theme's {key} is {value}, which is not a colour as a theme writes them: the HUD keeps TA Reborn's own there.", [("key", Fill::name(key)), ("value", Fill::name(*value))]),
                en("Write the colour as # and six hexadecimal digits, red, green and blue (#5ab4ff)."));
        }
    }
    // Its pictures and its font.
    let pictures = std::iter::once(("", &theme.look)).chain(theme.sides.iter().map(|(side, look)| (side.as_str(), look)))
        .flat_map(|(side, look)| [(side, "panel_texture", &look.panel_texture), (side, "bar_texture", &look.bar_texture)]);
    for (side, key, named) in pictures {
        if named.trim().is_empty() { continue; }
        let key = if side.is_empty() { key.to_string() } else { format!("sides.{side}.{key}") };
        let Some((path, at)) = theme_file(dir, &key, named, note) else { continue };
        let extension = at.rsplit_once('.').map_or(String::new(), |(_, e)| e.to_ascii_lowercase());
        if !matches!(extension.as_str(), "png" | "jpg" | "jpeg") {
            note("theme.format", &at, enf("The theme's picture {path} is not a PNG or a JPEG: the HUD draws its own metal there.", [("path", Fill::name(&at))]),
                en("Save the picture as a PNG or a JPEG."));
            continue;
        }
        if std::fs::metadata(&path).map_or(0, |m| m.len()) > PICTURE_MAX_BYTES {
            note("theme.too_large", &at, enf("The theme's picture {path} weighs more than {most}: the HUD draws its own metal there.", [("path", Fill::name(&at)), ("most", Fill::name(megabytes(PICTURE_MAX_BYTES)))]),
                en("Make it smaller: a tile of a few hundred pixels is repeated across the panels."));
            continue;
        }
        match dimensions(&path) {
            Ok((w, h)) if w > THEME_PICTURE_SIDE || h > THEME_PICTURE_SIDE => {
                note("theme.too_large", &at, enf("The theme's picture {path} is {width} × {height}: the HUD reads pictures of up to {most} pixels a side, and draws its own metal there.", [("path", Fill::name(&at)), ("width", Fill::number(w)), ("height", Fill::number(h)), ("most", Fill::number(THEME_PICTURE_SIDE))]),
                    en("Make it smaller: a tile of a few hundred pixels is repeated across the panels."));
            }
            Ok(_) => {}
            Err(error) => {
                note("theme.unreadable", &at, enf("The theme's picture {path} does not read as a picture ({error}): the HUD draws its own metal there.", [("path", Fill::name(&at)), ("error", Fill::Words(Words::raw(error)))]),
                    en("Save the picture again with an ordinary picture editor."));
            }
        }
    }
    if !theme.font.trim().is_empty() && let Some((path, at)) = theme_file(dir, "font", &theme.font, note) {
        let extension = at.rsplit_once('.').map_or(String::new(), |(_, e)| e.to_ascii_lowercase());
        let bytes = std::fs::metadata(&path).map_or(0, |m| m.len());
        let reads = matches!(extension.as_str(), "ttf" | "otf") && bytes <= THEME_FONT_MAX_BYTES
            && std::fs::read(&path).is_ok_and(|font| skrifa::FontRef::from_index(&font, 0).is_ok());
        if !reads {
            note("theme.font", &at, enf("The theme's font {path} does not read as a TrueType or OpenType font of up to {most}: the HUD keeps TA Reborn's own.", [("path", Fill::name(&at)), ("most", Fill::name(megabytes(THEME_FONT_MAX_BYTES)))]),
                en("Name a .ttf or an .otf font file, as a font's makers publish it."));
        }
    }
    // Its text and its accent on its panels (TA Reborn's own where it leaves one out), as
    // readable as the interface's own are.
    let colour = |key: &str, default: [u8; 3]| theme.look.colour(key).and_then(parse_colour).unwrap_or(default);
    let panel = colour("panel", [31, 30, 26]);
    let given = |key: &str| theme.look.colour(key).is_some_and(|v| parse_colour(v).is_some());
    for (key, default, least) in [("text", [216, 211, 196], 4.5f32), ("accent", [216, 180, 86], 3.0)] {
        if !given(key) && !given("panel") { continue; }
        let ratio = contrast(colour(key, default), panel);
        if ratio < least {
            note("theme.contrast", "mod.toml", enf("The theme's {key} on its panels has a contrast of {ratio}:1: hard to read. {least}:1 reads well.", [("key", Fill::name(key)), ("ratio", Fill::name(format!("{ratio:.1}"))), ("least", Fill::name(format!("{least:.1}")))]),
                en("Make the panels darker, or the text and the accent lighter."));
        }
    }
    // Its sides: each of the game's (the mod's own) by name.
    if let Some(sides) = sides {
        for side in theme.sides.keys() {
            if !sides.iter().any(|known| known.eq_ignore_ascii_case(side.trim())) {
                note("theme.side", "mod.toml", enf("The theme gives the side {side} colours of its own, but the game has no side of that name: they are never shown. Its sides: {sides}.", [("side", Fill::name(side)), ("sides", Fill::List(sides.iter().map(Fill::name).collect()))]),
                    en("Name the side as gamedata/sidedata.tdf does ([theme.sides.arm])."));
            }
        }
    }
}

/// A file a theme names, found in the mod's folder (with where it is in it, as shown), or its
/// finding: not there, or outside the folder.
fn theme_file(dir: &Path, key: &str, named: &str, note: &mut dyn FnMut(&str, &str, Words, &'static str)) -> Option<(std::path::PathBuf, String)> {
    let shown = named.trim().replace('\\', "/");
    match media_path(dir, named) {
        Some(path) => {
            let at = path.strip_prefix(dir).map_or(shown, |p| p.to_string_lossy().replace('\\', "/"));
            Some((path, at))
        }
        None => {
            note("theme.missing", "mod.toml", enf("The theme's {key} names {path}, which is not in the mod's folder: the HUD does without it.", [("key", Fill::name(key)), ("path", Fill::name(&shown))]),
                en("Add the file, or correct its path in mod.toml: it is read from the mod's folder (media/hud/panel.png)."));
            None
        }
    }
}

/// A colour as a theme writes it (`#rgb`, `#rrggbb`, `#rrggbbaa`, the `#` optional): its red,
/// green and blue.
fn parse_colour(text: &str) -> Option<[u8; 3]> {
    let hex = text.trim().trim_start_matches('#');
    if !hex.chars().all(|c| c.is_ascii_hexdigit()) { return None; }
    let byte = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).ok();
    match hex.len() {
        3 => { let d = |at: usize| u8::from_str_radix(&hex[at..at + 1], 16).ok().map(|v| v * 17); Some([d(0)?, d(1)?, d(2)?]) }
        6 | 8 => Some([byte(0)?, byte(2)?, byte(4)?]),
        _ => None,
    }
}

/// The contrast of two colours, as the Web's accessibility guidelines measure it (1 to 21).
fn contrast(a: [u8; 3], b: [u8; 3]) -> f32 {
    let luminance = |c: [u8; 3]| {
        let channel = |v: u8| { let v = v as f32 / 255.0; if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) } };
        0.2126 * channel(c[0]) + 0.7152 * channel(c[1]) + 0.0722 * channel(c[2])
    };
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind { Banner, Preview, Icon, Screenshot }

fn megabytes(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
}

/// A picture's size, read from its header (no pixels).
fn dimensions(path: &Path) -> Result<(u32, u32), String> {
    image::ImageReader::open(path).map_err(|e| e.to_string())?
        .with_guessed_format().map_err(|e| e.to_string())?
        .into_dimensions().map_err(|e| e.to_string())
}

/// How many frames a GIF has (up to [`PREVIEW_MAX_FRAMES`] and one), and for how long they show,
/// in seconds (each at least [`PREVIEW_LEAST_DELAY`]). A picture of another kind is one frame.
pub fn preview_frames(path: &Path) -> Result<(usize, f32), String> {
    use image::AnimationDecoder;
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let format = image::guess_format(&bytes).map_err(|e| e.to_string())?;
    if format != image::ImageFormat::Gif { return Ok((1, 0.0)); }
    let decoder = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let (mut frames, mut seconds) = (0usize, 0.0f32);
    for frame in decoder.into_frames().take(PREVIEW_MAX_FRAMES + 1) {
        let frame = frame.map_err(|e| e.to_string())?;
        frames += 1;
        seconds += frame_seconds(frame.delay());
    }
    Ok((frames, seconds))
}

/// How long a GIF's frame shows, in seconds, as the launcher plays it.
pub fn frame_seconds(delay: image::Delay) -> f32 {
    let (numerator, denominator) = delay.numer_denom_ms();
    let seconds = numerator as f32 / denominator.max(1) as f32 / 1000.0;
    if seconds < 0.02 { PREVIEW_LEAST_DELAY } else { seconds }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ta_presentation_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("media")).unwrap();
        dir
    }

    fn png(path: &Path, width: u32, height: u32) {
        image::RgbaImage::from_pixel(width, height, image::Rgba([90, 120, 160, 255])).save(path).unwrap();
    }

    fn gif(path: &Path, frames: u32) {
        let mut encoder = image::codecs::gif::GifEncoder::new(std::fs::File::create(path).unwrap());
        encoder.encode_frames((0..frames).map(|n| image::Frame::from_parts(image::RgbaImage::from_pixel(64, 36, image::Rgba([n as u8 * 40, 0, 0, 255])), 0, 0, image::Delay::from_numer_denom_ms(100, 1)))).unwrap();
    }

    fn manifest(text: &str) -> ModManifest {
        toml::from_str(&format!("id = \"m\"\nname = \"M\"\n{text}")).unwrap()
    }

    fn codes(issues: &[Issue]) -> Vec<(&str, &str)> {
        issues.iter().map(|i| (i.code.as_str(), i.path.as_str())).collect()
    }

    /// Every finding's words, and what to do, are marked for translation here.
    fn assert_words(issues: &[Issue]) {
        let source = include_str!("presentation.rs");
        let marked = |text: &str| source.contains(&format!("en(\"{text}\")")) || source.contains(&format!("enf(\"{text}\""));
        for issue in issues {
            assert_eq!(issue.words.english(), issue.message, "{}", issue.code);
            for text in issue.words.texts() {
                assert!(marked(text), "{}: {text:?} is not marked for translation", issue.code);
            }
            assert!(marked(&issue.suggestion), "{}: {:?} is not marked for translation", issue.code, issue.suggestion);
            assert_eq!((issue.severity, issue.layer.as_str()), (Severity::Info, "mod:m"), "notes of the mod's own");
        }
    }

    /// A mod that presents itself as the launcher shows best has nothing told.
    #[test]
    fn a_well_presented_mod_has_nothing_told() {
        let dir = temp("good");
        png(&dir.join("media/banner.png"), 1280, 640);
        png(&dir.join("media/icon.png"), 256, 256);
        png(&dir.join("media/shot.png"), 1920, 1080);
        gif(&dir.join("media/gameplay.gif"), 4);
        let manifest = manifest("source = \"https://www.moddb.com/mods/m\"\n[media]\nbanner = \"media/banner.png\"\npreview = \"media/gameplay.gif\"\nicon = \"Media/Icon.png\"\nscreenshots = [\"media/shot.png\"]\n[links]\nDiscord = \"https://discord.gg/m\"\n");
        let issues = check(&dir, &manifest, "mod:m", None);
        assert!(issues.is_empty(), "{:?}", codes(&issues));
        assert_eq!(preview_frames(&dir.join("media/gameplay.gif")).unwrap().0, 4);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The repository's example mod presents itself with nothing told (its pictures as the
    /// launcher shows best, its links web addresses).
    #[test]
    fn the_example_mod_presents_itself_with_nothing_told() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/examples/example-mod");
        let manifest: ModManifest = toml::from_str(&std::fs::read_to_string(dir.join("mod.toml")).unwrap()).unwrap();
        assert!(!manifest.media.preview.is_empty() && manifest.media.screenshots.len() == 2);
        let issues = check(&dir, &manifest, "mod:example-mod", None);
        assert!(issues.is_empty(), "{:?}", codes(&issues));
        let (frames, seconds) = preview_frames(&dir.join(&manifest.media.preview)).unwrap();
        assert!(frames > 2 && seconds > 1.0, "{frames} frames, {seconds} s");
    }

    /// A sound interface theme has nothing told: its colours colours, its pictures pictures, its
    /// font a font, its text and accent clear on its panels, its side the game's.
    #[test]
    fn a_sound_theme_has_nothing_told() {
        let dir = temp("theme_good");
        std::fs::create_dir_all(dir.join("media/hud")).unwrap();
        png(&dir.join("media/hud/panel.png"), 128, 128);
        let font = Path::new(env!("CARGO_MANIFEST_DIR")).join("../ta_ui/fonts/SofiaSansSemiCondensed.ttf");
        std::fs::copy(&font, dir.join("media/hud/Font.ttf")).unwrap();
        let manifest = manifest("[theme]\naccent = \"#5ab4ff\"\ntext = \"#e0e8f0\"\npanel = \"#151b24\"\npanel_texture = \"media/hud/panel.png\"\nfont = \"media/hud/Font.ttf\"\n[theme.classic]\ntext = \"#eee\"\n[theme.sides.core]\naccent = \"#ce4834\"\n");
        let issues = check(&dir, &manifest, "mod:m", Some(&["ARM".to_string(), "CORE".to_string()]));
        assert!(issues.is_empty(), "{:?}", codes(&issues));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Each way a theme falls short is told: a colour that is none, a picture not there or of
    /// another kind, a font that does not read, text hard to read on its panels, a side the game
    /// has not.
    #[test]
    fn each_shortfall_of_a_theme_is_told() {
        let dir = temp("theme_short");
        std::fs::write(dir.join("media/bar.bmp"), b"BM").unwrap();
        std::fs::write(dir.join("media/font.ttf"), b"not a font").unwrap();
        let manifest = manifest("[theme]\naccent = \"blue\"\ntext = \"#333333\"\npanel = \"#2a2a2a\"\npanel_texture = \"media/none.png\"\nbar_texture = \"media/bar.bmp\"\nfont = \"media/font.ttf\"\n[theme.sides.tll]\naccent = \"#ff0000\"\n");
        let issues = check(&dir, &manifest, "mod:m", Some(&["ARM".to_string(), "CORE".to_string()]));
        let found = codes(&issues);
        for expected in [("theme.colour", "mod.toml"), ("theme.missing", "mod.toml"), ("theme.format", "media/bar.bmp"), ("theme.font", "media/font.ttf"), ("theme.contrast", "mod.toml"), ("theme.side", "mod.toml")] {
            assert!(found.contains(&expected), "{expected:?} in {found:?}");
        }
        assert_words(&issues);
        // (Without the game's sides, none is told of them.)
        assert!(!codes(&check(&dir, &manifest, "mod:m", None)).iter().any(|(code, _)| *code == "theme.side"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Each way a mod's presentation falls short is told: a picture not there, one outside its
    /// folder, one of another kind, one small or of the wrong shape, a preview that does not move,
    /// screenshots beyond those shown, a link that is no web address.
    #[test]
    fn each_shortfall_of_a_mods_presentation_is_told() {
        let dir = temp("short");
        png(&dir.join("media/banner.png"), 400, 400);
        png(&dir.join("media/icon.png"), 48, 32);
        png(&dir.join("media/still.png"), 640, 360);
        std::fs::write(dir.join("media/shot.bmp"), b"BM").unwrap();
        let shots: Vec<String> = (0..14).map(|n| format!("\"media/s{n}.png\"")).collect();
        let manifest = manifest(&format!("source = \"www.example.com\"\n[media]\nbanner = \"media/banner.png\"\npreview = \"media/still.png\"\nicon = \"media/icon.png\"\nscreenshots = [\"media/shot.bmp\", \"../outside.png\", {}]\n[links]\nForum = \"ftp://example.com\"\n", shots.join(", ")));
        let issues = check(&dir, &manifest, "mod:m", None);
        let found = codes(&issues);
        for expected in [("presentation.small", "media/banner.png"), ("presentation.shape", "media/banner.png"), ("presentation.shape", "media/icon.png"), ("presentation.small", "media/icon.png"),
            ("presentation.still", "media/still.png"), ("presentation.format", "media/shot.bmp"), ("presentation.outside", "mod.toml"), ("presentation.missing", "mod.toml"),
            ("presentation.many", "mod.toml"), ("presentation.link", "mod.toml")] {
            assert!(found.contains(&expected), "{expected:?} in {found:?}");
        }
        assert_eq!(found.iter().filter(|(code, _)| *code == "presentation.link").count(), 2, "the source and the forum's link");
        assert_words(&issues);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
