//! A mod's own pictures in the launcher (its manifest's `[media]`, [`ta_vfs::mods::Media`]): its
//! banner, icon and screenshots, read on worker threads (bounded as `ta_data::presentation`
//! checks them) and kept as textures for those on show; its gameplay preview, an animated GIF
//! read whole (its frames made smaller, as many as [`PREVIEW_BUDGET`] holds) and played in its
//! banner, one texture of its own changed as its frames come round.
//!
//! Files are found again every few seconds, so that a mod's author sees a picture changed on the
//! disk without a rescan (a picture is known by its path, size and time).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};

use egui::{ColorImage, Context, TextureHandle, TextureOptions};
use ta_data::presentation::{PICTURE_MAX_BYTES, PICTURE_MAX_SIDE, PREVIEW_MAX_BYTES, PREVIEW_MAX_FRAMES, frame_seconds};
use ta_vfs::mods::{ModInfo, ModKind, media_path};

use crate::library::{Picture, Pictures};

/// The memory a preview's frames may take, decoded: a longer one keeps every other frame (each
/// shown for the two), as often as it must.
pub const PREVIEW_BUDGET: usize = 96 << 20;
/// The largest a preview's frames are kept: the library's banners are never larger.
const PREVIEW_SIZE: (u32, u32) = (640, 360);
/// How many previews are kept read at once (the one on show, the one before it).
const PREVIEWS_KEPT: usize = 2;
/// How long a file found (or not) is believed before it is looked for again, in seconds.
const FOUND_FOR: f64 = 2.0;
/// How long the pointer rests on a card before its preview plays, in seconds.
pub const HOVER_PLAYS_AFTER: f64 = 0.4;

/// What a picture is read for, and so how large it is kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Use {
    /// A card's or the details' banner.
    Banner,
    /// Beside the mod's name.
    Icon,
    /// A screenshot in the details' row.
    Thumb,
    /// A screenshot on the whole window.
    Full,
}

impl Use {
    fn largest(self) -> (u32, u32) {
        match self {
            Use::Banner => (1280, 640),
            Use::Icon => (256, 256),
            Use::Thumb => (480, 270),
            Use::Full => (2560, 1440),
        }
    }
}

/// A file a mod's manifest names, as found a moment ago (`None`: not there).
struct Found {
    at: f64,
    file: Option<(PathBuf, String)>,
}

/// A gameplay preview read: its frames, when each ends (seconds from its start), and as it plays.
struct Preview {
    frames: Vec<Arc<ColorImage>>,
    ends: Vec<f32>,
    texture: Option<TextureHandle>,
    shown: usize,
    /// When it began playing, and the interface's pass it was last on show in (a preview not on
    /// show in the pass before, off the screen a moment, starts again from its beginning).
    since: Option<f64>,
    painted: u64,
}

impl Preview {
    fn length(&self) -> f32 { self.ends.last().copied().unwrap_or(0.0) }

    /// The frame on show `elapsed` seconds into playing, and how long until the next.
    fn frame_at(&self, elapsed: f64) -> (usize, f32) {
        let length = self.length().max(0.01);
        let into = (elapsed % length as f64) as f32;
        let index = self.ends.partition_point(|end| *end <= into).min(self.frames.len().saturating_sub(1));
        (index, (self.ends[index] - into).max(0.0))
    }
}

type Read = (String, Result<(Vec<ColorImage>, Vec<f32>), String>);

/// The mods' own pictures and previews, for the Mods page.
pub struct ModMedia {
    pictures: Pictures,
    found: HashMap<(PathBuf, String), Found>,
    previews: HashMap<String, Preview>,
    reading: HashSet<String>,
    failed: HashSet<String>,
    send: mpsc::Sender<Read>,
    receive: mpsc::Receiver<Read>,
    /// The card the pointer rests on, and since when.
    hover: Option<(String, f64)>,
    now: f64,
}

impl Default for ModMedia {
    fn default() -> Self {
        let (send, receive) = mpsc::channel();
        Self { pictures: Pictures::new("mod media", 2, 96), found: HashMap::new(), previews: HashMap::new(), reading: HashSet::new(),
            failed: HashSet::new(), send, receive, hover: None, now: 0.0 }
    }
}

impl ModMedia {
    /// Takes in what was read since the last frame.
    pub fn poll(&mut self, ctx: &Context) {
        self.now = ctx.input(|i| i.time);
        self.pictures.poll(ctx);
        while let Ok((key, read)) = self.receive.try_recv() {
            self.reading.remove(&key);
            match read {
                Ok((frames, delays)) if !frames.is_empty() => {
                    let mut end = 0.0;
                    let ends = delays.iter().map(|delay| { end += delay; end }).collect();
                    let frames = frames.into_iter().map(Arc::new).collect();
                    self.previews.insert(key, Preview { frames, ends, texture: None, shown: 0, since: None, painted: ctx.cumulative_pass_nr() });
                    // (The least lately shown let go, beyond those kept.)
                    while self.previews.len() > PREVIEWS_KEPT {
                        let Some(oldest) = self.previews.iter().min_by_key(|(_, preview)| preview.painted).map(|(key, _)| key.clone()) else { break };
                        self.previews.remove(&oldest);
                    }
                }
                Ok(_) => { self.failed.insert(key); }
                Err(error) => {
                    log::info!("mod preview {key}: {error}");
                    self.failed.insert(key);
                }
            }
        }
        if !self.reading.is_empty() { ctx.request_repaint_after(std::time::Duration::from_millis(50)); }
    }

    /// Nothing asked for is still being read (a capture waits for it).
    pub fn idle(&self) -> bool { self.pictures.idle() && self.reading.is_empty() }

    /// The file `named` in the mod's folder `dir`, and the key it is known by (its path, size and
    /// time): looked for again every few seconds.
    fn locate(&mut self, dir: &Path, named: &str) -> Option<(PathBuf, String)> {
        let key = (dir.to_path_buf(), named.to_string());
        if let Some(found) = self.found.get(&key) && self.now - found.at < FOUND_FOR {
            return found.file.clone();
        }
        let file = media_path(dir, named).and_then(|path| {
            let meta = std::fs::metadata(&path).ok()?;
            let time = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |t| t.as_nanos());
            let stamp = format!("{}|{}|{time}", path.display(), meta.len());
            Some((path, stamp))
        });
        self.found.insert(key, Found { at: self.now, file: file.clone() });
        file
    }

    /// A picture of a mod's, asked for if it is not made yet.
    fn picture(&mut self, info: &ModInfo, named: &str, using: Use) -> Option<TextureHandle> {
        if info.kind != ModKind::Directory || named.trim().is_empty() { return None; }
        let (path, stamp) = self.locate(&info.path, named)?;
        let key = format!("{using:?}|{stamp}");
        if let Some(texture) = self.pictures.get(&key) { return Some(texture); }
        if self.pictures.wanted(&key) {
            self.pictures.request(key, move || read_picture(&path, using));
        }
        None
    }

    pub fn banner(&mut self, info: &ModInfo) -> Option<TextureHandle> {
        let named = info.manifest.media.banner.clone();
        self.picture(info, &named, Use::Banner)
    }

    pub fn icon(&mut self, info: &ModInfo) -> Option<TextureHandle> {
        let named = info.manifest.media.icon.clone();
        self.picture(info, &named, Use::Icon)
    }

    /// The mod's `index`th screenshot of those shown (`Media::shots`), small for the details' row
    /// or `full` for the window.
    pub fn screenshot(&mut self, info: &ModInfo, index: usize, full: bool) -> Option<TextureHandle> {
        let named = info.manifest.media.shots().nth(index)?.to_string();
        self.picture(info, &named, if full { Use::Full } else { Use::Thumb })
    }

    /// How many screenshots of the mod's are shown (`Media::shots`: those that name a file).
    pub fn screenshots(info: &ModInfo) -> usize {
        if info.kind != ModKind::Directory { return 0; }
        info.manifest.media.shots().count()
    }

    /// Whether the mod has a gameplay preview to play (its file there, not one that failed).
    pub fn has_preview(&mut self, info: &ModInfo) -> bool {
        if info.kind != ModKind::Directory || info.manifest.media.preview.trim().is_empty() { return false; }
        let named = info.manifest.media.preview.clone();
        self.locate(&info.path, &named).is_some_and(|(_, stamp)| !self.failed.contains(&stamp))
    }

    /// Whether the pointer has rested on the card of `id` long enough for its preview to play
    /// (`over`: it is on the card now). Asks for the frame at which it will have.
    pub fn resting(&mut self, ctx: &Context, id: &str, over: bool) -> bool {
        match (&self.hover, over) {
            (Some((on, since)), true) if on == id => {
                let rested = self.now - since >= HOVER_PLAYS_AFTER;
                if !rested { ctx.request_repaint_after(std::time::Duration::from_secs_f64(HOVER_PLAYS_AFTER - (self.now - since))); }
                rested
            }
            (_, true) => {
                self.hover = Some((id.to_string(), self.now));
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(HOVER_PLAYS_AFTER));
                false
            }
            (Some((on, _)), false) if on == id => { self.hover = None; false }
            _ => false,
        }
    }

    /// The frame of the mod's gameplay preview to show now, reading it first if need be (None
    /// until it is read): as it plays (`playing`), else its first; and while it plays, how long
    /// until its next frame.
    pub fn preview(&mut self, ctx: &Context, info: &ModInfo, playing: bool) -> Option<(TextureHandle, Option<f32>)> {
        if info.kind != ModKind::Directory { return None; }
        let named = info.manifest.media.preview.clone();
        let (path, stamp) = self.locate(&info.path, &named)?;
        if self.failed.contains(&stamp) { return None; }
        let (now, pass) = (self.now, ctx.cumulative_pass_nr());
        let Some(preview) = self.previews.get_mut(&stamp) else {
            if self.reading.insert(stamp.clone()) {
                let send = self.send.clone();
                let key = stamp.clone();
                let spawned = std::thread::Builder::new().name("mod preview".into()).spawn(move || {
                    let read = std::panic::catch_unwind(|| read_preview(&path)).unwrap_or_else(|_| Err("the preview could not be read".into()));
                    let _ = send.send((key, read));
                });
                if spawned.is_err() { self.reading.remove(&stamp); self.failed.insert(stamp); }
            }
            return None;
        };
        // (Not on show in the pass before, off the screen a moment, or held: it starts again from
        // its beginning. A frame shown for long has no passes between: it is still on show.)
        if !playing || preview.painted + 1 < pass { preview.since = None; }
        preview.painted = pass;
        let (index, next) = if playing {
            let since = *preview.since.get_or_insert(now);
            let (index, next) = preview.frame_at(now - since);
            (index, Some(next))
        } else {
            (0, None)
        };
        if preview.texture.is_none() {
            preview.texture = Some(ctx.load_texture(format!("mod-preview:{stamp}"), preview.frames[index].clone(), TextureOptions::LINEAR));
        } else if preview.shown != index && let Some(texture) = &mut preview.texture {
            texture.set(preview.frames[index].clone(), TextureOptions::LINEAR);
        }
        preview.shown = index;
        preview.texture.clone().map(|texture| (texture, next))
    }
}

/// Reads a file of at most `most` bytes.
fn read_bounded(path: &Path, most: u64) -> anyhow::Result<Vec<u8>> {
    use std::io::Read;
    let file = std::fs::File::open(path)?;
    anyhow::ensure!(file.metadata()?.len() <= most, "{} is larger than {most} bytes", path.display());
    let mut bytes = Vec::new();
    file.take(most + 1).read_to_end(&mut bytes)?;
    anyhow::ensure!(bytes.len() as u64 <= most, "{} is larger than {most} bytes", path.display());
    Ok(bytes)
}

/// A mod's picture, read (a GIF's first frame) and made no larger than its `using` needs.
fn read_picture(path: &Path, using: Use) -> anyhow::Result<Picture> {
    let bytes = read_bounded(path, PICTURE_MAX_BYTES)?;
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(PICTURE_MAX_SIDE);
    limits.max_image_height = Some(PICTURE_MAX_SIDE);
    limits.max_alloc = Some(128 << 20);
    reader.limits(limits);
    let picture = reader.decode()?;
    let (width, height) = using.largest();
    let picture = if picture.width() > width || picture.height() > height { picture.thumbnail(width, height) } else { picture };
    let picture = picture.to_rgba8();
    Ok(Picture { image: ColorImage::from_rgba_unmultiplied([picture.width() as usize, picture.height() as usize], picture.as_raw()), size: None })
}

/// A gameplay preview's frames, each no larger than [`PREVIEW_SIZE`], and how long each shows (in
/// seconds): as many as [`PREVIEW_BUDGET`] holds, every other one kept (and shown for both) as
/// often as a long one needs. A still picture is one frame.
pub fn read_preview(path: &Path) -> Result<(Vec<ColorImage>, Vec<f32>), String> {
    use image::{AnimationDecoder, ImageDecoder};
    let bytes = read_bounded(path, PREVIEW_MAX_BYTES).map_err(|e| e.to_string())?;
    if image::guess_format(&bytes).map_err(|e| e.to_string())? != image::ImageFormat::Gif {
        let picture = read_picture(path, Use::Banner).map_err(|e| e.to_string())?;
        return Ok((vec![picture.image], vec![1.0]));
    }
    let decoder = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let (width, height) = decoder.dimensions();
    if width == 0 || height == 0 || width > PICTURE_MAX_SIDE || height > PICTURE_MAX_SIDE {
        return Err(format!("a preview of {width} × {height} is not shown"));
    }
    let scale = (PREVIEW_SIZE.0 as f32 / width as f32).min(PREVIEW_SIZE.1 as f32 / height as f32).min(1.0);
    let (w, h) = (((width as f32 * scale).round() as u32).max(1), ((height as f32 * scale).round() as u32).max(1));
    let room = (PREVIEW_BUDGET / (w as usize * h as usize * 4)).max(2);
    let (mut frames, mut delays): (Vec<ColorImage>, Vec<f32>) = (Vec::new(), Vec::new());
    let mut stride = 1usize;
    for (index, frame) in decoder.into_frames().take(PREVIEW_MAX_FRAMES).enumerate() {
        let frame = frame.map_err(|e| e.to_string())?;
        let delay = frame_seconds(frame.delay());
        if index % stride != 0 {
            if let Some(last) = delays.last_mut() { *last += delay; }
            continue;
        }
        let buffer = frame.into_buffer();
        let buffer = if (w, h) != buffer.dimensions() { image::imageops::resize(&buffer, w, h, image::imageops::FilterType::Triangle) } else { buffer };
        frames.push(ColorImage::from_rgba_unmultiplied([w as usize, h as usize], buffer.as_raw()));
        delays.push(delay);
        if frames.len() > room {
            // Every other frame of those kept, each shown for itself and the one let go.
            let mut kept = (Vec::new(), Vec::new());
            for (at, (frame, delay)) in frames.into_iter().zip(delays).enumerate() {
                if at % 2 == 0 { kept.0.push(frame); kept.1.push(delay); } else if let Some(last) = kept.1.last_mut() { *last += delay; }
            }
            (frames, delays) = kept;
            stride *= 2;
        }
    }
    Ok((frames, delays))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An animated GIF of `count` frames of `size`, each `hundredths` of a second, frame `n` a grey
    /// of its own.
    fn gif(path: &Path, size: (u32, u32), count: u32, hundredths: u32) {
        let file = std::fs::File::create(path).unwrap();
        let mut encoder = image::codecs::gif::GifEncoder::new(file);
        encoder.set_repeat(image::codecs::gif::Repeat::Infinite).unwrap();
        let frames = (0..count).map(|n| {
            let grey = (n * 255 / count.max(1)) as u8;
            let buffer = image::RgbaImage::from_pixel(size.0, size.1, image::Rgba([grey, grey, grey, 255]));
            image::Frame::from_parts(buffer, 0, 0, image::Delay::from_numer_denom_ms(hundredths * 10, 1))
        });
        encoder.encode_frames(frames).unwrap();
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ta_mod_media_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A preview plays every frame for as long as it asks, made no larger than the library shows,
    /// and the frames at a time are those due then.
    #[test]
    fn a_preview_plays_its_frames_for_as_long_as_each_asks() {
        let dir = temp("plays");
        let path = dir.join("preview.gif");
        gif(&path, (1280, 720), 6, 20);
        let (frames, delays) = read_preview(&path).unwrap();
        assert_eq!(frames.len(), 6);
        assert_eq!(frames[0].size, [640, 360], "made no larger than a banner shows");
        assert!(delays.iter().all(|d| (d - 0.2).abs() < 1e-4), "{delays:?}");
        let mut end = 0.0;
        let preview = Preview { frames: frames.into_iter().map(Arc::new).collect(), ends: delays.iter().map(|d| { end += d; end }).collect(), texture: None, shown: 0, since: None, painted: 0 };
        assert_eq!(preview.frame_at(0.0).0, 0);
        assert_eq!(preview.frame_at(0.25).0, 1);
        assert_eq!(preview.frame_at(1.19).0, 5);
        assert_eq!(preview.frame_at(1.25).0, 0, "it loops");
        assert!((preview.frame_at(0.25).1 - 0.15).abs() < 1e-3, "the next frame in 0.15 s");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A frame asking for no time at all (as many GIFs do) shows a tenth of a second, as in a
    /// browser; a still picture is a preview of one frame.
    #[test]
    fn frames_without_a_time_show_a_tenth_and_a_still_is_one_frame() {
        let dir = temp("fast");
        let path = dir.join("fast.gif");
        gif(&path, (64, 32), 3, 0);
        let (_, delays) = read_preview(&path).unwrap();
        assert!(delays.iter().all(|d| (d - 0.1).abs() < 1e-4), "{delays:?}");
        let still = dir.join("still.png");
        image::RgbaImage::from_pixel(64, 32, image::Rgba([1, 2, 3, 255])).save(&still).unwrap();
        let (frames, _) = read_preview(&still).unwrap();
        assert_eq!(frames.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A preview longer than its memory holds keeps every other frame, each shown for both: it
    /// lasts as long as it did.
    #[test]
    fn a_long_preview_keeps_every_other_frame_and_its_length() {
        let dir = temp("long");
        let path = dir.join("long.gif");
        // (640 × 360 frames: the budget holds 109 of them.)
        gif(&path, (640, 360), 150, 4);
        let (frames, delays) = read_preview(&path).unwrap();
        assert!(frames.len() <= PREVIEW_BUDGET / (640 * 360 * 4), "{} frames", frames.len());
        assert!(frames.len() >= 60, "{} frames", frames.len());
        let length: f32 = delays.iter().sum();
        assert!((length - 6.0).abs() < 0.05, "it lasts its 6 seconds: {length}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
