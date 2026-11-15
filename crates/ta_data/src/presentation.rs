//! The checks of how a mod presents itself in the launcher (its manifest's `[media]`, `[links]`
//! and `source`): pictures that are in the mod's folder, read, and are of a size and a shape the
//! launcher shows well; a gameplay preview that is an animated GIF it plays; links it opens.
//! Notes of their own (`presentation.*`, `theme.*`), never errors or warnings: a mod plays the
//! same whatever its pictures, and its badge is how it plays. And what its `mod.toml` holds that
//! is read otherwise than written (`manifest.*`, [`manifest_keys`]): notes, but for a key that
//! decides what the game mounts, a warning.

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
/// The memory a preview's frames may take in the launcher, decoded: a longer one keeps every
/// other frame (each shown for the two), as often as it must ([`preview_stride`]).
pub const PREVIEW_BUDGET: usize = 96 << 20;
/// The largest a preview's frames are kept: the library's banners are never larger.
pub const PREVIEW_SIZE: (u32, u32) = (640, 360);
/// The most decoding a mod's picture may take (a still picture; a preview's frames each).
const PICTURE_MAX_ALLOC: u64 = 128 << 20;
/// The most decoding a gameplay preview may take at once (its canvas and a frame, each up to
/// [`PICTURE_MAX_SIDE`] a side).
const PREVIEW_MAX_ALLOC: u64 = 256 << 20;
/// The most a theme's picture may weigh for the HUD to draw it.
pub const THEME_PICTURE_MAX_BYTES: u64 = 16 << 20;
/// The most a theme's picture may be a side (the HUD reads up to it).
pub const THEME_PICTURE_SIDE: u32 = 2048;
/// The most a theme's font may weigh.
pub const THEME_FONT_MAX_BYTES: u64 = 16 << 20;


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
        let most = if kind == Kind::Preview { PREVIEW_MAX_BYTES } else { PICTURE_MAX_BYTES };
        let bytes = std::fs::metadata(&path).map_or(0, |m| m.len());
        if bytes > most {
            note("presentation.too_large", &at, enf("{path} weighs {size}: the launcher shows pictures of up to {most} only.", [("path", Fill::name(&at)), ("size", Fill::name(megabytes(bytes))), ("most", Fill::name(megabytes(most)))]),
                en("Make it smaller: fewer pixels, a JPEG for a photograph, fewer frames or colours for a GIF."));
            continue;
        }
        let unreadable = |note: &mut dyn FnMut(&str, &str, Words, &'static str), error: String| {
            note("presentation.unreadable", &at, enf("{path} does not read as a picture ({error}): the launcher shows the mod without it.", [("path", Fill::name(&at)), ("error", Fill::Words(Words::raw(error)))]),
                en("Save the picture again with an ordinary picture editor."));
        };
        let bytes = match read_bounded(&path, most) {
            Ok(bytes) => bytes,
            Err(error) => { unreadable(&mut note, error); continue; }
        };
        // (By its content, as the launcher reads it: a picture's name says nothing.)
        let Some(format) = picture_format(&bytes) else {
            note("presentation.format", &at, enf("{path} is not a PNG, JPEG or GIF picture: the launcher shows only those.", [("path", Fill::name(&at))]),
                en("Save the picture as a PNG or a JPEG (an animated GIF for the gameplay preview)."));
            continue;
        };
        let too_large = |note: &mut dyn FnMut(&str, &str, Words, &'static str), (width, height): (u32, u32)| {
            note("presentation.too_large", &at, enf("{path} is {width} × {height}: the launcher shows pictures of up to {most} pixels a side only.", [("path", Fill::name(&at)), ("width", Fill::number(width)), ("height", Fill::number(height)), ("most", Fill::number(PICTURE_MAX_SIDE))]),
                en("Make it smaller: the launcher never shows a picture larger than the screen."));
        };
        // An animated gameplay preview: its frames counted as the launcher plays them, their
        // pixels not decoded.
        if kind == Kind::Preview && format == image::ImageFormat::Gif {
            match gif_frames(&bytes) {
                Ok(gif) if gif.width > PICTURE_MAX_SIDE || gif.height > PICTURE_MAX_SIDE => too_large(&mut note, (gif.width, gif.height)),
                Ok(gif) if gif.largest.0 > PICTURE_MAX_SIDE || gif.largest.1 > PICTURE_MAX_SIDE => too_large(&mut note, gif.largest),
                Ok(gif) if gif.width == 0 || gif.height == 0 => unreadable(&mut note, "a picture of no pixels".into()),
                Ok(gif) if gif.frames < 2 => still(&mut note, &at),
                Ok(gif) => preview_length(&mut note, &at, &gif),
                Err(error) => unreadable(&mut note, error),
            }
            continue;
        }
        let size = image::ImageReader::with_format(std::io::Cursor::new(&bytes), format).into_dimensions();
        let (width, height) = match size {
            Ok(size) => size,
            Err(error) => { unreadable(&mut note, error.to_string()); continue; }
        };
        if width > PICTURE_MAX_SIDE || height > PICTURE_MAX_SIDE {
            too_large(&mut note, (width, height));
            continue;
        }
        // (Read whole, as the launcher reads it: a picture cut short reads its size and no more.)
        if let Err(error) = decode_picture(bytes) {
            unreadable(&mut note, error);
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
            // (A still picture, or a GIF's first frame: an animated one is counted above.)
            Kind::Preview => still(&mut note, &at),
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
    let requires = manifest.requires_reborn.trim();
    if !requires.is_empty() && ta_vfs::mods::version_older("0", requires).is_none() {
        note("presentation.requires", "mod.toml", enf("mod.toml's requires_reborn is {value}, which is not a version: the mod is mounted in any TA Reborn.", [("value", Fill::name(requires))]),
            en("Write the oldest TA Reborn the mod plays in, as 1.0.7."));
    }
    check_theme(dir, &manifest.theme, sides, &mut note);
    issues.extend(manifest_keys(dir, layer));
    issues
}

/// What the mod's `mod.toml` holds that TA Reborn reads otherwise than it is written
/// (`ta_vfs::mods::manifest_notes`, `manifest.*`): keys it does not know, keys written in a table
/// where they are not read (the usual slip in TOML: every key below a `[table]` line is that
/// table's), values of another kind than their keys take, one text where a list is wanted. A key
/// that decides what the game mounts (`ta_vfs::mods::MOUNTING_KEYS`), misplaced or mistyped, is a
/// warning: the mod is mounted as if it were not given. The rest are notes.
pub fn manifest_keys(dir: &Path, layer: &str) -> Vec<Issue> {
    use ta_vfs::mods::{MOUNTING_KEYS, ManifestNoteKind, ValueKind, closest_key, manifest_notes};
    let Ok(text) = std::fs::read_to_string(ta_vfs::mods::manifest_path(dir)) else { return Vec::new() };
    let kind = |kind: ValueKind| Fill::Words(Words::text(match kind {
        ValueKind::String => en("text in quotes"),
        ValueKind::Integer => en("a whole number"),
        ValueKind::Float => en("a number"),
        ValueKind::Boolean => en("true or false"),
        ValueKind::Datetime => en("a date"),
        ValueKind::Array => en("a list in square brackets"),
        ValueKind::Table => en("a table"),
    }));
    // (The key's own name: its path's last part, as written.)
    let own = |key: &str| key.rsplit_once('.').map_or(key, |(_, last)| last).trim_matches('"').to_string();
    let mut issues = Vec::new();
    for note in manifest_notes(&text) {
        let key = Fill::name(&note.key);
        let mounting = |name: &str| MOUNTING_KEYS.contains(&name);
        let (severity, code, words, suggestion) = match &note.kind {
            ManifestNoteKind::Misplaced { belongs } if belongs.is_empty() && mounting(&own(&note.key)) => (Severity::Warning, "manifest.misplaced",
                enf("mod.toml's {key} is below a [table] line, so it is read as part of that table, and TA Reborn mounts the mod as if it were not given. It belongs at the top of mod.toml, above the first [table] line.", [("key", key)]),
                en("Move the line up, above the manifest's first [table] line (with its id and name).")),
            ManifestNoteKind::Misplaced { belongs } if belongs.is_empty() => (Severity::Info, "manifest.misplaced",
                enf("mod.toml's {key} is below a [table] line, so it is read as part of that table and TA Reborn does not read it. It belongs at the top of mod.toml, above the first [table] line.", [("key", key)]),
                en("Move the line up, above the manifest's first [table] line (with its id and name).")),
            ManifestNoteKind::Misplaced { belongs } => (Severity::Info, "manifest.misplaced",
                enf("mod.toml's {key} is written where TA Reborn does not read it: it belongs in [{table}].", [("key", key), ("table", Fill::name(belongs))]),
                en("Move the line under its own table's [table] line.")),
            ManifestNoteKind::Unknown => match closest_key(&note.key) {
                Some(meant) => (if !note.key.contains('.') && mounting(meant) { Severity::Warning } else { Severity::Info }, "manifest.unknown",
                    enf("mod.toml's {key} is not a key TA Reborn knows, and it reads past it: did you mean {meant}?", [("key", key), ("meant", Fill::name(meant))]),
                    en("Correct the key's name (docs/MODDING.md lists every key).")),
                None => (Severity::Info, "manifest.unknown",
                    enf("mod.toml's {key} is not a key TA Reborn knows, and it reads past it (a key of a later TA Reborn's, or a slip of the pen).", [("key", key)]),
                    en("Correct the key's name, or take it out: a key of a later TA Reborn's is kept as written.")),
            },
            ManifestNoteKind::WrongKind { expected, found } => (Severity::Info, "manifest.wrong_kind",
                enf("mod.toml's {key} is {found} where it takes {expected}: TA Reborn leaves it out.", [("key", key), ("found", kind(*found)), ("expected", kind(*expected))]),
                en("Write the value as its key takes it (docs/MODDING.md lists every key).")),
            ManifestNoteKind::OneForList => (Severity::Info, "manifest.one_for_list",
                enf("mod.toml's {key} is one text where a list is wanted: TA Reborn reads it as a list of that one, but older TA Reborns leave the whole mod out for it.", [("key", key)]),
                en("Write it as a list, in square brackets: tags = [\"units\"].")),
        };
        issues.push(Issue {
            severity, code: code.into(), path: "mod.toml".into(), line: u32::try_from(note.line).ok(), layer: layer.into(),
            message: words.english(), words, suggestion: suggestion.into(), ..Issue::default()
        });
    }
    issues
}

/// The note that the gameplay preview at `at` is a still picture (the launcher plays animated
/// GIFs).
fn still(note: &mut dyn FnMut(&str, &str, Words, &'static str), at: &str) {
    note("presentation.still", at, enf("The gameplay preview {path} is a still picture: the launcher plays animated GIFs.", [("path", Fill::name(at))]),
        en("Save the gameplay preview as an animated GIF (a few seconds of the mod in play)."));
}

/// The notes of a gameplay preview the launcher plays otherwise than it was made: past its first
/// [`PREVIEW_MAX_FRAMES`] frames, or with fewer of its frames than it has, the frames it keeps of
/// one this long and this large ([`preview_stride`]: [`PREVIEW_BUDGET`] holds no more).
fn preview_length(note: &mut dyn FnMut(&str, &str, Words, &'static str), at: &str, gif: &GifFrames) {
    if gif.frames > PREVIEW_MAX_FRAMES {
        note("presentation.long", at, enf("The gameplay preview has more than {most} frames: the launcher plays its first {most}, {seconds} seconds of it.", [("most", Fill::number(PREVIEW_MAX_FRAMES)), ("seconds", Fill::number(gif.seconds.round() as u32))]),
            en("Keep the gameplay preview to a few seconds (5 to 15), about 640 × 360 pixels."));
    }
    let size = preview_frame_size(gif.width, gif.height);
    let stride = preview_stride(gif.frames, size);
    if stride > 1 {
        let kept = gif.frames.min(PREVIEW_MAX_FRAMES).div_ceil(stride);
        let rate = kept as f32 / gif.seconds.max(0.1);
        note("presentation.long", at, enf("The gameplay preview's {frames} frames take more memory than the launcher keeps for a preview at {width} × {height}: it keeps one frame in {stride}, so it plays at {rate} frames a second, choppier than it was made.", [("frames", Fill::number(gif.frames.min(PREVIEW_MAX_FRAMES))), ("width", Fill::number(size.0)), ("height", Fill::number(size.1)), ("stride", Fill::number(stride)), ("rate", Fill::name(format!("{rate:.1}")))]),
            en("Keep the gameplay preview to a few seconds (5 to 15), about 640 × 360 pixels."));
    }
}

/// A GIF gameplay preview as the launcher plays it: counted without its frames' pixels decoded.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GifFrames {
    /// Its size, the canvas its frames are drawn on.
    pub width: u32,
    pub height: u32,
    /// How many frames it has (counted to [`PREVIEW_MAX_FRAMES`] and one).
    pub frames: usize,
    /// How long the frames the launcher plays (its first [`PREVIEW_MAX_FRAMES`]) show, in seconds,
    /// each at least [`PREVIEW_LEAST_DELAY`].
    pub seconds: f32,
    /// Its largest frame's size.
    pub largest: (u32, u32),
}

/// A GIF's frames counted as the launcher plays them, read without decoding their pixels (and
/// within limits: a GIF that says it is vast is refused).
pub fn gif_frames(bytes: &[u8]) -> Result<GifFrames, String> {
    let mut options = gif::DecodeOptions::new();
    options.skip_frame_decoding(true);
    options.set_memory_limit(gif::MemoryLimit::Bytes(std::num::NonZeroU64::new(PREVIEW_MAX_ALLOC).expect("a limit")));
    let mut decoder = options.read_info(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let (width, height) = (u32::from(decoder.width()), u32::from(decoder.height()));
    let mut gif = GifFrames { width, height, frames: 0, seconds: 0.0, largest: (0, 0) };
    while gif.frames <= PREVIEW_MAX_FRAMES {
        let Some(frame) = decoder.next_frame_info().map_err(|e| e.to_string())? else { break };
        if gif.frames < PREVIEW_MAX_FRAMES { gif.seconds += hundredths_seconds(frame.delay); }
        gif.largest = (gif.largest.0.max(u32::from(frame.width)), gif.largest.1.max(u32::from(frame.height)));
        gif.frames += 1;
    }
    Ok(gif)
}

/// How long a GIF frame of `delay` hundredths of a second shows, in seconds, as the launcher plays
/// it ([`frame_seconds`]'s rule).
fn hundredths_seconds(delay: u16) -> f32 {
    let seconds = f32::from(delay) / 100.0;
    if seconds < 0.02 { PREVIEW_LEAST_DELAY } else { seconds }
}

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
        if std::fs::metadata(&path).map_or(0, |m| m.len()) > THEME_PICTURE_MAX_BYTES {
            note("theme.too_large", &at, enf("The theme's picture {path} weighs more than {most}: the HUD draws its own metal there.", [("path", Fill::name(&at)), ("most", Fill::name(megabytes(THEME_PICTURE_MAX_BYTES)))]),
                en("Make it smaller: a tile of a few hundred pixels is repeated across the panels."));
            continue;
        }
        // (By its content, as the HUD reads it.)
        let bytes = std::fs::read(&path).unwrap_or_default();
        if picture_format(&bytes).is_none() {
            note("theme.format", &at, enf("The theme's picture {path} is not a PNG, JPEG or GIF picture: the HUD draws its own metal there.", [("path", Fill::name(&at))]),
                en("Save the picture as a PNG or a JPEG."));
            continue;
        }
        let size = picture_format(&bytes).map(|format| image::ImageReader::with_format(std::io::Cursor::new(&bytes), format).into_dimensions());
        match size {
            Some(Ok((w, h))) if w > THEME_PICTURE_SIDE || h > THEME_PICTURE_SIDE => {
                note("theme.too_large", &at, enf("The theme's picture {path} is {width} × {height}: the HUD reads pictures of up to {most} pixels a side, and draws its own metal there.", [("path", Fill::name(&at)), ("width", Fill::number(w)), ("height", Fill::number(h)), ("most", Fill::number(THEME_PICTURE_SIDE))]),
                    en("Make it smaller: a tile of a few hundred pixels is repeated across the panels."));
            }
            _ => if let Err(error) = read_theme_picture(&path) {
                note("theme.unreadable", &at, enf("The theme's picture {path} does not read as a picture ({error}): the HUD draws its own metal there.", [("path", Fill::name(&at)), ("error", Fill::Words(Words::raw(error)))]),
                    en("Save the picture again with an ordinary picture editor."));
            },
        }
    }
    if !theme.font.trim().is_empty() && let Some((path, at)) = theme_file(dir, "font", &theme.font, note) {
        // (As the game reads it: a font of the size and the tables its interface sets text by.)
        match read_theme_font(&path) {
            Err(_) => note("theme.font", &at, enf("The theme's font {path} does not read as a TrueType or OpenType font of up to {most}: the HUD keeps TA Reborn's own.", [("path", Fill::name(&at)), ("most", Fill::name(megabytes(THEME_FONT_MAX_BYTES)))]),
                en("Name a .ttf or an .otf font file, as a font's makers publish it.")),
            // (The HUD sets it on its own lines, 1.2 times the size: one of other spacing is
            // scaled to fit them, as `ta_ui::theme` scales it.)
            Ok(bytes) => if let Some(metrics) = crate::font::vmetrics(&bytes, 0).filter(|m| m.row() > 0.0) {
                let scale = (HUD_LINE / metrics.row()).clamp(0.25, 4.0);
                if !(0.85..=1.15).contains(&scale) {
                    note("theme.font_scaled", &at, enf("The theme's font {path} sets its lines {line} times its letters' size, where the HUD's are {hud}: the HUD fits it to its lines, so its letters show at {percent}% of their size.", [("path", Fill::name(&at)), ("line", Fill::name(format!("{:.2}", metrics.row()))), ("hud", Fill::name(format!("{HUD_LINE:.1}"))), ("percent", Fill::number((scale * 100.0).round() as u32))]),
                        en("Nothing to do if it looks right in battle. A font of ordinary line spacing shows its letters at the size the HUD asks for."));
                }
            },
        }
    }
    // Its text and its accent on what they are drawn over (TA Reborn's own colours where it
    // leaves one out), as readable as the interface's own: its panels' colour (its cards and
    // plates of no picture), its plates' picture (its average colour), or, where it gives the
    // plates no picture, the side's own metal (dark, TA's tiles). The theme's own and each side's
    // that changes one of them.
    let contexts = std::iter::once((String::new(), &theme.look)).chain(theme.sides.iter().map(|(side, look)| (side.clone(), look)));
    for (side, own) in contexts {
        let changes = |key: &str| own.colour(key).is_some_and(|v| parse_colour(v).is_some());
        if !["text", "accent", "panel"].into_iter().any(changes) && own.panel_texture.trim().is_empty() { continue; }
        let prefix = if side.is_empty() { String::new() } else { format!("sides.{side}.") };
        let colour = |key: &str, default: [u8; 3]| theme.colour(&side, key).and_then(parse_colour).unwrap_or(default);
        let panel = colour("panel", REBORN_PANEL);
        let picture = theme.texture(&side, "panel").and_then(|named| media_path(dir, named)).and_then(|path| Some((read_theme_picture(&path).ok().and_then(|p| mean_colour(&p))?, path)));
        let background = changes("panel") || !own.panel_texture.trim().is_empty();
        for (key, default, least) in [("text", REBORN_TEXT, 4.5f32), ("accent", REBORN_ACCENT, 3.0)] {
            if !side.is_empty() && !changes(key) && !background { continue; }
            let shown = format!("{prefix}{key}");
            let ink = colour(key, default);
            // (Shown floored, never rounded up past the least it is held to.)
            let ratio = |behind: [u8; 3]| Some(contrast(ink, behind)).filter(|ratio| *ratio < least).map(|ratio| format!("{:.1}", (ratio * 10.0).floor() / 10.0));
            let least = format!("{least:.1}");
            if let Some(ratio) = ratio(panel) {
                note("theme.contrast", "mod.toml", enf("The theme's {key} on its panels has a contrast of {ratio}:1: hard to read. {least}:1 reads well.", [("key", Fill::name(&shown)), ("ratio", Fill::name(&ratio)), ("least", Fill::name(&least))]),
                    en("Make the panels darker, or the text and the accent lighter."));
            }
            match &picture {
                Some((average, path)) => if let Some(ratio) = ratio(*average) {
                    let at = path.strip_prefix(dir).map_or(String::new(), |p| p.to_string_lossy().replace('\\', "/"));
                    note("theme.contrast", "mod.toml", enf("The theme's {key} on its plates' picture {path} has a contrast of {ratio}:1 with its average colour: hard to read. {least}:1 reads well.", [("key", Fill::name(&shown)), ("path", Fill::name(&at)), ("ratio", Fill::name(&ratio)), ("least", Fill::name(&least))]),
                        en("Make the picture darker, or the text and the accent lighter."));
                },
                // (The side's own metal: what TA Reborn's own panel colour stands for.)
                None if panel != REBORN_PANEL => if let Some(ratio) = ratio(REBORN_PANEL) {
                    note("theme.contrast", "mod.toml", enf("The theme's {key} on the side's own metal plates (the theme gives them no picture) has a contrast of {ratio}:1: hard to read. {least}:1 reads well.", [("key", Fill::name(&shown)), ("ratio", Fill::name(&ratio)), ("least", Fill::name(&least))]),
                        en("Give the plates a picture of the theme's own (panel_texture), or make the text and the accent lighter."));
                },
                None => {}
            }
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

/// How tall a line of the HUD's text is, in ems (TA Reborn's own fonts' lines: Bahnschrift's, and
/// Sofia Sans's made to match).
const HUD_LINE: f32 = 1.2;

/// TA Reborn's own panel, text and accent colours (`ta_ui::theme::Palette::REBORN`'s), where a
/// theme leaves them as they are. Its panel is as dark as the sides' metal plates.
const REBORN_PANEL: [u8; 3] = [31, 30, 26];
const REBORN_TEXT: [u8; 3] = [216, 211, 196];
const REBORN_ACCENT: [u8; 3] = [216, 180, 86];

/// A picture's average colour, as it shows over the battlefield (its clear pixels weigh less);
/// None for one of no opaque pixels.
fn mean_colour(picture: &image::RgbaImage) -> Option<[u8; 3]> {
    let (mut sum, mut weight) = ([0u64; 3], 0u64);
    for pixel in picture.pixels() {
        let alpha = u64::from(pixel[3]);
        for (total, channel) in sum.iter_mut().zip(&pixel.0[..3]) { *total += u64::from(*channel) * alpha; }
        weight += alpha;
    }
    (weight > 0).then(|| sum.map(|total| (total / weight) as u8))
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

/// A file's bytes, where it weighs no more than `most`.
pub fn read_bounded(path: &Path, most: u64) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let weighs = |bytes: u64| format!("it weighs {} (the most is {})", megabytes(bytes), megabytes(most));
    let size = file.metadata().map_err(|e| e.to_string())?.len();
    if size > most { return Err(weighs(size)); }
    let mut bytes = Vec::new();
    file.take(most + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > most { return Err(weighs(bytes.len() as u64)); }
    Ok(bytes)
}

/// The limits a mod's pictures are decoded within: no more than `side` pixels a side, nor
/// `alloc` bytes taken.
fn limits(side: u32, alloc: u64) -> image::Limits {
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(side);
    limits.max_image_height = Some(side);
    limits.max_alloc = Some(alloc);
    limits
}

/// What a picture's bytes are, by their content: PNG, JPEG and GIF are what TA Reborn reads (a
/// picture's name says nothing). None for anything else.
pub fn picture_format(bytes: &[u8]) -> Option<image::ImageFormat> {
    image::guess_format(bytes).ok().filter(|format| matches!(format, image::ImageFormat::Png | image::ImageFormat::Jpeg | image::ImageFormat::Gif))
}

/// A mod's picture (its banner, icon or a screenshot; a GIF's first frame) as the launcher reads
/// it: by its content, no more than [`PICTURE_MAX_SIDE`] a side. Its bytes are read whole: the
/// check reads it so too, so that a picture it passes is one the launcher shows.
pub fn decode_picture(bytes: Vec<u8>) -> Result<image::DynamicImage, String> {
    decode(bytes, PICTURE_MAX_SIDE, PICTURE_MAX_ALLOC)
}

fn decode(bytes: Vec<u8>, side: u32, alloc: u64) -> Result<image::DynamicImage, String> {
    let format = picture_format(&bytes).ok_or_else(|| "not a PNG, JPEG or GIF picture".to_string())?;
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(bytes), format);
    reader.limits(limits(side, alloc));
    reader.decode().map_err(|e| e.to_string())
}

/// A gameplay preview's decoder, its frames read within limits (each frame's own size checked
/// before it is decoded: a GIF may say it is small and that a frame is vast).
pub fn preview_decoder(bytes: Vec<u8>) -> Result<image::codecs::gif::GifDecoder<std::io::Cursor<Vec<u8>>>, String> {
    use image::ImageDecoder;
    let mut decoder = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
    decoder.set_limits(limits(PICTURE_MAX_SIDE, PREVIEW_MAX_ALLOC)).map_err(|e| e.to_string())?;
    Ok(decoder)
}

/// The size a preview of `width` × `height` is kept at: as large as fits [`PREVIEW_SIZE`], never
/// larger than it is.
pub fn preview_frame_size(width: u32, height: u32) -> (u32, u32) {
    let scale = (PREVIEW_SIZE.0 as f32 / width.max(1) as f32).min(PREVIEW_SIZE.1 as f32 / height.max(1) as f32).min(1.0);
    (((width as f32 * scale).round() as u32).max(1), ((height as f32 * scale).round() as u32).max(1))
}

/// How many frames kept at `size` [`PREVIEW_BUDGET`] holds (two at least).
pub fn preview_room(size: (u32, u32)) -> usize {
    (PREVIEW_BUDGET / (size.0 as usize * size.1 as usize * 4).max(1)).max(2)
}

/// How many frames apart those the launcher keeps of a preview of `frames` frames at `size` are
/// (it plays the first [`PREVIEW_MAX_FRAMES`]): 1 when it keeps every one, else the power of two
/// that leaves no more than [`preview_room`] of them.
pub fn preview_stride(frames: usize, size: (u32, u32)) -> usize {
    let (frames, room) = (frames.min(PREVIEW_MAX_FRAMES), preview_room(size));
    let mut stride = 1usize;
    while frames.div_ceil(stride) > room { stride *= 2; }
    stride
}

/// A theme's picture as the HUD reads it: by its content (a PNG, a JPEG, a GIF's first frame),
/// no more than [`THEME_PICTURE_MAX_BYTES`] nor [`THEME_PICTURE_SIDE`] a side.
pub fn read_theme_picture(path: &Path) -> Result<image::RgbaImage, String> {
    let bytes = read_bounded(path, THEME_PICTURE_MAX_BYTES)?;
    Ok(decode(bytes, THEME_PICTURE_SIDE, 64 << 20)?.to_rgba8())
}

/// A theme's font as the game reads it: its bytes, where it weighs no more than
/// [`THEME_FONT_MAX_BYTES`] and reads ([`crate::font::reads`]).
pub fn read_theme_font(path: &Path) -> Result<Vec<u8>, String> {
    let bytes = read_bounded(path, THEME_FONT_MAX_BYTES)?;
    if !crate::font::reads(&bytes) { return Err("not a TrueType or OpenType font the interface reads".into()); }
    Ok(bytes)
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
        png_of(path, width, height, [90, 120, 160]);
    }

    fn png_of(path: &Path, width: u32, height: u32, [r, g, b]: [u8; 3]) {
        image::RgbaImage::from_pixel(width, height, image::Rgba([r, g, b, 255])).save(path).unwrap();
    }

    fn gif(path: &Path, frames: u32) {
        let mut encoder = image::codecs::gif::GifEncoder::new(std::fs::File::create(path).unwrap());
        encoder.encode_frames((0..frames).map(|n| image::Frame::from_parts(image::RgbaImage::from_pixel(64, 36, image::Rgba([n as u8 * 40, 0, 0, 255])), 0, 0, image::Delay::from_numer_denom_ms(100, 1)))).unwrap();
    }

    fn manifest(text: &str) -> ModManifest {
        toml::from_str(&format!("id = \"m\"\nname = \"M\"\n{text}")).unwrap()
    }

    fn manifest_with(text: &str) -> ModManifest { manifest(text) }

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
        let gif = gif_frames(&std::fs::read(dir.join("media/gameplay.gif")).unwrap()).unwrap();
        assert_eq!((gif.width, gif.height, gif.frames, gif.largest), (64, 36, 4, (64, 36)));
        assert!((gif.seconds - 0.4).abs() < 1e-4, "{} s", gif.seconds);
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
        let gif = gif_frames(&std::fs::read(dir.join(&manifest.media.preview)).unwrap()).unwrap();
        assert!(gif.frames > 2 && gif.seconds > 1.0, "{gif:?}");
    }

    /// A sound interface theme has nothing told: its colours colours, its pictures pictures, its
    /// font a font, its text and accent clear on its panels, its side the game's.
    #[test]
    fn a_sound_theme_has_nothing_told() {
        let dir = temp("theme_good");
        std::fs::create_dir_all(dir.join("media/hud")).unwrap();
        png_of(&dir.join("media/hud/panel.png"), 128, 128, [21, 27, 36]);
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
        let odd = manifest_with("requires_reborn = \"latest\"\n");
        assert_eq!(codes(&check(&dir, &odd, "mod:m", None)), [("presentation.requires", "mod.toml")]);
        assert_words(&issues);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Pictures are judged by their content, read whole, as the launcher reads them: a GIF named
    /// .png shows, a picture cut off after its header does not, a GIF that says a frame of it is
    /// vast is refused before it is decoded (by the check and by the launcher's decoder alike).
    #[test]
    fn pictures_are_judged_by_their_content_read_whole() {
        let dir = temp("content");
        gif(&dir.join("media/banner.png"), 1);
        png(&dir.join("media/whole.png"), 1280, 640);
        let whole = std::fs::read(dir.join("media/whole.png")).unwrap();
        std::fs::write(dir.join("media/cut.png"), &whole[..whole.len().min(80)]).unwrap();
        // A GIF of 64 × 36 whose one frame says it is 65535 × 65535 (a few bytes).
        let mut vast = b"GIF89a".to_vec();
        vast.extend([64, 0, 36, 0, 0x80, 0, 0, 0, 0, 0, 255, 255, 255]);
        vast.extend([0x2C, 0, 0, 0, 0, 0xFF, 0xFF, 0xFF, 0xFF, 0]);
        vast.extend([2, 2, 0x4C, 0x01, 0, 0x3B]);
        std::fs::write(dir.join("media/vast.gif"), &vast).unwrap();
        let named = manifest("[media]\nscreenshots = [\"media/cut.png\"]\npreview = \"media/vast.gif\"\n");
        let issues = check(&dir, &named, "mod:m", None);
        let found = codes(&issues);
        assert!(found.contains(&("presentation.unreadable", "media/cut.png")), "{found:?}");
        assert!(found.contains(&("presentation.too_large", "media/vast.gif")), "{found:?}");
        let banner = manifest("[media]\nbanner = \"media/banner.png\"\n");
        let issues = check(&dir, &banner, "mod:m", None);
        let found = codes(&issues);
        assert!(!found.iter().any(|(code, _)| *code == "presentation.format" || *code == "presentation.unreadable"), "{found:?}");
        // (The launcher's decoder: its frame refused, never allocated.)
        use image::AnimationDecoder;
        let frame = preview_decoder(vast).unwrap().into_frames().next().unwrap();
        assert!(frame.is_err(), "a vast frame is refused");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A preview's length is judged by the launcher's memory for it: one of more frames than it
    /// holds at its size plays choppier, one of more than it plays is cut off, a small one of many
    /// frames plays whole.
    #[test]
    fn a_previews_length_is_judged_by_the_launchers_budget() {
        let size = preview_frame_size(1920, 1080);
        assert_eq!(size, PREVIEW_SIZE);
        assert_eq!((preview_stride(100, size), preview_stride(200, size), preview_stride(500, size)), (1, 2, 8));
        assert_eq!(preview_stride(5000, preview_frame_size(160, 90)), 1, "a small one is kept whole (its first frames)");
        let told = |gif: GifFrames| {
            let mut found = Vec::new();
            preview_length(&mut |code: &str, _: &str, words: Words, _: &'static str| found.push((code.to_string(), words.english())), "media/p.gif", &gif);
            found
        };
        let long = told(GifFrames { width: 640, height: 360, frames: 200, seconds: 20.0, largest: (640, 360) });
        assert_eq!(long.len(), 1, "{long:?}");
        assert!(long[0].1.contains("one frame in 2") && long[0].1.contains("5.0 frames a second"), "{long:?}");
        assert!(told(GifFrames { width: 160, height: 90, frames: 600, seconds: 60.0, largest: (160, 90) }).is_empty());
        let cut = told(GifFrames { width: 160, height: 90, frames: 1001, seconds: 100.0, largest: (160, 90) });
        assert!(cut.iter().any(|(_, words)| words.contains("plays its first 1000")), "{cut:?}");
        // Frames asking for no time at all show for a tenth of a second, as browsers show them.
        assert_eq!((hundredths_seconds(0), hundredths_seconds(1), hundredths_seconds(2), hundredths_seconds(50)), (0.1, 0.1, 0.02, 0.5));
    }

    /// A theme's font whose lines are far taller or shorter than the HUD's is told: the HUD fits
    /// it to its lines, so its letters show smaller or larger than meant. One of ordinary spacing
    /// is not.
    #[test]
    fn a_theme_font_of_other_line_spacing_is_told() {
        let dir = temp("font_line");
        let sofia = std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../ta_ui/fonts/SofiaSansSemiCondensed.ttf")).unwrap();
        std::fs::write(dir.join("media/even.ttf"), &sofia).unwrap();
        // The same font with its line gap made a whole em: lines of 2.2.
        let mut tall = sofia.clone();
        let (tables, mut gap_at) = (u16::from_be_bytes([tall[4], tall[5]]) as usize, None);
        for i in 0..tables {
            let record = 12 + 16 * i;
            if &tall[record..record + 4] == b"hhea" { gap_at = Some(u32::from_be_bytes(tall[record + 8..record + 12].try_into().unwrap()) as usize + 8); }
        }
        let units = crate::font::vmetrics(&sofia, 0).map(|m| m.row()).unwrap();
        assert!((units - 1.2).abs() < 0.01);
        let gap_at = gap_at.unwrap();
        // (Whichever metrics the font sets its lines by: hhea's, or OS/2's typographic ones.)
        let os2 = (0..tables).map(|i| 12 + 16 * i).find(|r| &tall[*r..*r + 4] == b"OS/2").map(|r| u32::from_be_bytes(tall[r + 8..r + 12].try_into().unwrap()) as usize);
        let em = { let head = (0..tables).map(|i| 12 + 16 * i).find(|r| &tall[*r..*r + 4] == b"head").map(|r| u32::from_be_bytes(tall[r + 8..r + 12].try_into().unwrap()) as usize).unwrap(); u16::from_be_bytes([tall[head + 18], tall[head + 19]]) };
        tall[gap_at..gap_at + 2].copy_from_slice(&(em as i16).to_be_bytes());
        if let Some(os2) = os2 { tall[os2 + 72..os2 + 74].copy_from_slice(&(em as i16).to_be_bytes()); }
        std::fs::write(dir.join("media/tall.ttf"), &tall).unwrap();
        let told = |font: &str| check(&dir, &manifest(&format!("[theme]
font = \"media/{font}\"
")), "mod:m", None);
        assert!(told("even.ttf").is_empty(), "{:?}", codes(&told("even.ttf")));
        let issues = told("tall.ttf");
        assert_eq!(codes(&issues), [("theme.font_scaled", "media/tall.ttf")]);
        assert!(issues[0].message.contains("2.20 times") && issues[0].message.contains("55%"), "{}", issues[0].message);
        assert_words(&issues);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// What a mod.toml holds that is read otherwise than written is told: a key that decides what
    /// is mounted, misplaced or mistyped, as a warning (the mod is mounted as if it were not
    /// given); the rest as notes, with the key likely meant.
    #[test]
    fn a_manifests_keys_read_otherwise_than_written_are_told() {
        let dir = temp("keys");
        std::fs::write(dir.join("mod.toml"), "id = \"m\"\nname = \"M\"\ndepend = [\"x\"]\ntags = \"units\"\n\n[media]\nbaner = \"media/b.png\"\nrequires_reborn = \"1.0.7\"\n\n[theme]\naccent = 5\n").unwrap();
        let issues = manifest_keys(&dir, "mod:m");
        let found: Vec<(&str, Severity, Option<u32>)> = issues.iter().map(|i| (i.code.as_str(), i.severity, i.line)).collect();
        assert_eq!(found, [
            ("manifest.unknown", Severity::Warning, Some(3)),
            ("manifest.one_for_list", Severity::Info, Some(4)),
            ("manifest.unknown", Severity::Info, Some(7)),
            ("manifest.misplaced", Severity::Warning, Some(8)),
            ("manifest.wrong_kind", Severity::Info, Some(11)),
        ], "{:?}", issues.iter().map(|i| &i.message).collect::<Vec<_>>());
        assert!(issues[0].message.contains("did you mean depends?"), "{}", issues[0].message);
        assert!(issues[2].message.contains("did you mean banner?"), "{}", issues[2].message);
        assert!(issues[4].message.contains("a whole number where it takes text in quotes"), "{}", issues[4].message);
        let source = include_str!("presentation.rs");
        for issue in &issues {
            for text in issue.words.texts() { assert!(source.contains(&format!("en(\"{text}\")")) || source.contains(&format!("enf(\"{text}\"")), "{text:?}"); }
            assert_eq!(issue.words.english(), issue.message);
        }
        // (A sound manifest: nothing.)
        std::fs::write(dir.join("mod.toml"), "id = \"m\"\nname = \"M\"\ndepends = [\"x\"]\n[media]\nbanner = \"media/b.png\"\n").unwrap();
        assert!(manifest_keys(&dir, "mod:m").is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A theme's contrast is judged on every side and on what its text is drawn over: its panels'
    /// colour, its plates' picture, or the sides' own dark metal where it gives them none; shown
    /// floored, never as the least it is held to.
    #[test]
    fn a_themes_contrast_is_judged_where_its_text_is_drawn() {
        let dir = temp("contrast");
        png_of(&dir.join("media/light.png"), 64, 64, [230, 230, 220]);
        // Dark text on light panels: fine on the panels, not on the sides' own metal.
        let light = manifest("[theme]\ntext = \"#202020\"\naccent = \"#7a4a00\"\npanel = \"#f0f0e8\"\n");
        let issues = check(&dir, &light, "mod:m", None);
        assert_eq!(issues.iter().filter(|i| i.code == "theme.contrast").count(), 2, "{:?}", issues.iter().map(|i| &i.message).collect::<Vec<_>>());
        assert!(issues.iter().all(|i| i.message.contains("metal")), "{:?}", issues.iter().map(|i| &i.message).collect::<Vec<_>>());
        assert_words(&issues);
        // With a light picture of its own for the plates, it reads.
        let pictured = manifest("[theme]\ntext = \"#202020\"\naccent = \"#7a4a00\"\npanel = \"#f0f0e8\"\npanel_texture = \"media/light.png\"\n");
        assert!(check(&dir, &pictured, "mod:m", None).is_empty());
        // A side's own text, hard to read on TA Reborn's panels: told as the side's.
        let side = manifest("[theme.sides.arm]\ntext = \"#303030\"\n");
        let found: Vec<String> = check(&dir, &side, "mod:m", None).into_iter().map(|i| i.message).collect();
        assert!(found.len() == 1 && found[0].contains("sides.arm.text"), "{found:?}");
        // (Shown floored: 2.96 is "2.9:1", below the 3.0:1 it is held to.)
        let edge = manifest("[theme]\naccent = \"#767676\"\npanel = \"#191919\"\n");
        for issue in check(&dir, &edge, "mod:m", None) { assert!(!issue.message.contains("3.0:1: hard"), "{}", issue.message); }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
