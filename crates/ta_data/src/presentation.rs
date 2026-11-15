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
/// open as meant. `dir` is the mod's folder; the findings are on `layer`'s (`mod:<id>`).
pub fn check(dir: &Path, manifest: &ModManifest, layer: &str) -> Vec<Issue> {
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
    issues
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
        let issues = check(&dir, &manifest, "mod:m");
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
        let issues = check(&dir, &manifest, "mod:example-mod");
        assert!(issues.is_empty(), "{:?}", codes(&issues));
        let (frames, seconds) = preview_frames(&dir.join(&manifest.media.preview)).unwrap();
        assert!(frames > 2 && seconds > 1.0, "{frames} frames, {seconds} s");
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
        let issues = check(&dir, &manifest, "mod:m");
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
