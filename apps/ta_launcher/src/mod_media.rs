//! A mod's own pictures in the launcher (its manifest's `[media]`, [`ta_vfs::mods::Media`], and its
//! interface theme's): its banner, icon and screenshots, read on worker threads (bounded as
//! `ta_data::presentation` checks them) and kept as textures for those on show, a screenshot on
//! the whole window in a few of its own, let go as it closes; its theme's pictures repeated as the
//! HUD repeats them; its gameplay preview, an animated GIF read whole on a worker of its own (its
//! frames made smaller, as many as [`PREVIEW_BUDGET`](ta_data::presentation::PREVIEW_BUDGET) holds;
//! the newest asked for first, and its reading stopped once it is no longer on show) and played in
//! its banner, one texture of its own changed as its frames come round. A picture that is not
//! there or does not read says so (its row shows a quiet cross, never a rail at work for ever); a
//! still picture, or a GIF of one frame, named as the preview is no preview.
//!
//! Files are found again every few seconds, on a thread of their own (a mod's folder may be on a
//! network share), so that a mod's author sees a picture changed on the disk without a rescan (a
//! picture is known by its path, size and time).

use std::collections::{HashMap, HashSet};
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::thread::JoinHandle;

use egui::{ColorImage, Context, TextureHandle, TextureOptions, TextureWrapMode};
use ta_data::presentation::{PICTURE_MAX_BYTES, PICTURE_MAX_SIDE, PREVIEW_MAX_BYTES, PREVIEW_MAX_FRAMES, decode_picture, frame_seconds, preview_decoder, preview_frame_size, preview_room, read_bounded, read_theme_picture};
use ta_vfs::mods::{ModInfo, ModKind, media_path};

use crate::library::{Picture, Pictures};

/// How many previews are kept read at once (the one on show, the one before it).
const PREVIEWS_KEPT: usize = 2;
/// How many screenshots are kept at the window's size (the one on show, those just before it).
const FULL_KEPT: usize = 3;
/// How many of a theme's pictures are kept (a mod's bar and panels).
const TILES_KEPT: usize = 2;
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
    /// A picture of its interface theme, repeated as the HUD repeats it (as large as it is: the
    /// HUD draws it a texel a point).
    Tile,
}

impl Use {
    fn largest(self) -> (u32, u32) {
        match self {
            Use::Banner => (1280, 640),
            Use::Icon => (256, 256),
            Use::Thumb => (480, 270),
            Use::Full => (2560, 1440),
            Use::Tile => (u32::MAX, u32::MAX),
        }
    }
}

/// A picture of a mod's as it stands: made, on its way, or not to be had (its file not there, or
/// not a picture the launcher reads).
#[derive(Clone)]
pub enum Got {
    Ready(TextureHandle),
    Waiting,
    Failed,
}

impl Got {
    pub fn ready(self) -> Option<TextureHandle> {
        match self { Got::Ready(texture) => Some(texture), _ => None }
    }

    pub fn texture(&self) -> Option<&TextureHandle> {
        match self { Got::Ready(texture) => Some(texture), _ => None }
    }

    pub fn failed(&self) -> bool { matches!(self, Got::Failed) }
}

/// A mod's screenshots on the whole window: the mod, the one on show, whether the keys are yet to
/// be put on its close button (it has just opened), and the screenshot of the details' row it was
/// opened from (the keys go back to it as it closes).
#[derive(Clone, Debug, PartialEq)]
pub struct Lightbox {
    pub id: String,
    pub index: usize,
    pub focus: bool,
    pub from: Option<egui::Id>,
}

impl Lightbox {
    pub fn new(id: &str, index: usize, from: Option<egui::Id>) -> Lightbox {
        Lightbox { id: id.to_string(), index, focus: true, from }
    }
}

/// A file a mod's manifest names, as found: where it is, the key it is known by (its path, size
/// and time), and for a gameplay preview, whether it moves (a GIF of two frames or more).
#[derive(Clone, Debug, PartialEq)]
struct File {
    path: PathBuf,
    stamp: String,
    moves: bool,
}

/// A file as last found (`Some(None)`: not there; `None`: not known yet), when, and whether it is
/// being looked for again.
struct Found {
    at: f64,
    file: Option<Option<File>>,
    asked: bool,
}

/// A file to find: the mod's folder, the name its manifest gives, and whether it is the gameplay
/// preview (whether it moves is found too).
type Ask = (PathBuf, String, bool);

/// The thread that finds the files the pages ask for, and answers.
struct Locator {
    asks: mpsc::Sender<Ask>,
    answers: mpsc::Receiver<(Ask, Option<File>)>,
}

impl Locator {
    fn start() -> Option<Locator> {
        let (asks, asked) = mpsc::channel::<Ask>();
        let (answer, answers) = mpsc::channel();
        std::thread::Builder::new().name("mod media files".into()).spawn(move || {
            // (Whether a preview moves, by its stamp: a GIF is walked through once, not each time
            // it is found again.)
            let mut moving = HashMap::new();
            for ask in asked {
                // (A file that could not be looked into is not there: the thread goes on.)
                let file = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| find(&ask.0, &ask.1, ask.2, &mut moving))).unwrap_or(None);
                if answer.send((ask, file)).is_err() { return; }
            }
        }).ok()?;
        Some(Locator { asks, answers })
    }
}

/// The file `named` in the mod's folder `dir`, as it is now (`preview`: whether it moves too).
fn find(dir: &Path, named: &str, preview: bool, moving: &mut HashMap<String, bool>) -> Option<File> {
    let path = media_path(dir, named)?;
    let meta = std::fs::metadata(&path).ok()?;
    let time = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |t| t.as_nanos());
    let stamp = format!("{}|{}|{time}", path.display(), meta.len());
    let moves = preview && {
        if moving.len() > 256 { moving.clear(); }
        *moving.entry(stamp.clone()).or_insert_with(|| moves(&path))
    };
    Some(File { path, stamp, moves })
}

/// Whether the picture at `path` moves: a GIF of two frames or more, read only as far as its
/// second frame's start (nothing decoded). A still picture, or a GIF of one frame, does not.
fn moves(path: &Path) -> bool {
    std::fs::File::open(path).ok().is_some_and(|file| gif_moves(std::io::BufReader::new(file)).unwrap_or(false))
}

/// [`moves`] of a GIF's bytes: its blocks walked through to a second image.
fn gif_moves(mut reader: impl std::io::Read) -> std::io::Result<bool> {
    fn skip(reader: &mut impl std::io::Read, bytes: u64) -> std::io::Result<()> {
        if std::io::copy(&mut reader.by_ref().take(bytes), &mut std::io::sink())? < bytes {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        Ok(())
    }
    // (A run of blocks, each its length and its bytes, to one of none.)
    fn skip_blocks(reader: &mut impl std::io::Read) -> std::io::Result<()> {
        loop {
            let mut length = [0u8];
            reader.read_exact(&mut length)?;
            if length[0] == 0 { return Ok(()); }
            skip(reader, length[0] as u64)?;
        }
    }
    // Its signature and its screen's description, then its colours if it gives them.
    let mut head = [0u8; 13];
    reader.read_exact(&mut head)?;
    if &head[..3] != b"GIF" { return Ok(false); }
    if head[10] & 0x80 != 0 { skip(&mut reader, 3 << ((head[10] & 7) + 1))?; }
    let mut images = 0;
    loop {
        let mut kind = [0u8];
        reader.read_exact(&mut kind)?;
        match kind[0] {
            // An extension: its label, its blocks.
            0x21 => {
                reader.read_exact(&mut kind)?;
                skip_blocks(&mut reader)?;
            }
            // An image: its description, its colours, its code size and its data.
            0x2c => {
                images += 1;
                if images > 1 { return Ok(true); }
                let mut description = [0u8; 9];
                reader.read_exact(&mut description)?;
                if description[8] & 0x80 != 0 { skip(&mut reader, 3 << ((description[8] & 7) + 1))?; }
                reader.read_exact(&mut kind)?;
                skip_blocks(&mut reader)?;
            }
            // Its end (or what no GIF holds).
            _ => return Ok(false),
        }
    }
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

type Frames = Result<(Vec<ColorImage>, Vec<f32>), String>;
type Read = (String, Frames);

/// A preview to read: the key it is kept by, its file, and whether it is still wanted.
struct Job {
    key: String,
    path: PathBuf,
    cancel: Arc<AtomicBool>,
}

/// The previews waiting to be read (the newest first), and whether the worker is to stop.
struct Jobs {
    waiting: Mutex<Vec<Job>>,
    ready: Condvar,
    stop: AtomicBool,
}

/// The worker that reads previews, one at a time.
struct Decoder {
    jobs: Arc<Jobs>,
    results: mpsc::Receiver<Read>,
    worker: Option<JoinHandle<()>>,
}

impl Decoder {
    fn start() -> Option<Decoder> {
        let jobs = Arc::new(Jobs { waiting: Mutex::new(Vec::new()), ready: Condvar::new(), stop: AtomicBool::new(false) });
        let (send, results) = mpsc::channel();
        let worker = {
            let jobs = jobs.clone();
            std::thread::Builder::new().name("mod previews".into()).spawn(move || decode_previews(&jobs, &send)).ok()?
        };
        Some(Decoder { jobs, results, worker: Some(worker) })
    }

    fn ask(&self, job: Job) {
        self.jobs.waiting.lock().unwrap_or_else(|e| e.into_inner()).push(job);
        self.jobs.ready.notify_one();
    }

    /// Those waiting that are no longer wanted leave the queue.
    fn prune(&self) {
        self.jobs.waiting.lock().unwrap_or_else(|e| e.into_inner()).retain(|job| !job.cancel.load(Ordering::Relaxed));
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        self.jobs.stop.store(true, Ordering::Relaxed);
        self.jobs.waiting.lock().unwrap_or_else(|e| e.into_inner()).clear();
        self.jobs.ready.notify_all();
        if let Some(worker) = self.worker.take() { let _ = worker.join(); }
    }
}

/// The preview worker: the newest preview asked for read first, one let go before its turn
/// skipped, and one let go as it is read stopped between its frames (and not sent).
fn decode_previews(jobs: &Jobs, send: &mpsc::Sender<Read>) {
    loop {
        let job = {
            let mut waiting = jobs.waiting.lock().unwrap_or_else(|e| e.into_inner());
            loop {
                if jobs.stop.load(Ordering::Relaxed) { return; }
                match waiting.pop() {
                    Some(job) if job.cancel.load(Ordering::Relaxed) => continue,
                    Some(job) => break job,
                    None => waiting = jobs.ready.wait(waiting).unwrap_or_else(|e| e.into_inner()),
                }
            }
        };
        let let_go = || job.cancel.load(Ordering::Relaxed) || jobs.stop.load(Ordering::Relaxed);
        let read = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| read_preview(&job.path, &let_go)))
            .unwrap_or_else(|_| Err("the preview could not be read".into()));
        if let_go() { continue; }
        if send.send((job.key, read)).is_err() { return; }
    }
}

/// A preview being read: its job's flag, and the interface's pass it was last asked for in.
struct Reading {
    cancel: Arc<AtomicBool>,
    asked: u64,
}

/// The mods' own pictures and previews, for the Mods page.
pub struct ModMedia {
    /// Banners, icons and the screenshots' row.
    pictures: Pictures,
    /// Screenshots on the whole window: the few lately on show, let go as it closes.
    full: Pictures,
    /// Interface themes' pictures, repeated.
    tiles: Pictures,
    found: HashMap<Ask, Found>,
    locator: Option<Locator>,
    /// How many files asked for are not found yet.
    locating: usize,
    previews: HashMap<String, Preview>,
    decoder: Option<Decoder>,
    reading: HashMap<String, Reading>,
    /// Previews that could not be read, or turned out not to move.
    failed: HashSet<String>,
    /// The card the pointer rests on, and since when.
    hover: Option<(String, f64)>,
    now: f64,
}

impl Default for ModMedia {
    fn default() -> Self {
        let repeat = TextureOptions { wrap_mode: TextureWrapMode::Repeat, ..TextureOptions::LINEAR };
        Self {
            pictures: Pictures::new("mod media", 2, 96),
            full: Pictures::new("mod screenshots", 1, FULL_KEPT),
            tiles: Pictures::new("mod theme", 1, TILES_KEPT).options(repeat),
            found: HashMap::new(), locator: None, locating: 0, previews: HashMap::new(), decoder: None, reading: HashMap::new(),
            failed: HashSet::new(), hover: None, now: 0.0,
        }
    }
}

impl ModMedia {
    /// Takes in what was read and found since the last frame, and stops reading the previews no
    /// longer on show (not asked for in the pass before).
    pub fn poll(&mut self, ctx: &Context) {
        self.now = ctx.input(|i| i.time);
        for pictures in [&mut self.pictures, &mut self.full, &mut self.tiles] { pictures.poll(ctx); }
        if let Some(locator) = &self.locator {
            while let Ok((ask, file)) = locator.answers.try_recv() {
                self.locating = self.locating.saturating_sub(1);
                self.found.insert(ask, Found { at: self.now, file: Some(file), asked: false });
            }
        }
        self.let_go_unwanted(ctx.cumulative_pass_nr());
        let read: Vec<Read> = self.decoder.as_ref().map(|decoder| std::iter::from_fn(|| decoder.results.try_recv().ok()).collect()).unwrap_or_default();
        for (key, frames) in read { self.take_read(key, frames); }
        if !self.reading.is_empty() || self.locating > 0 { ctx.request_repaint_after(std::time::Duration::from_millis(50)); }
    }

    /// The previews being read that were not asked for in the pass before `pass` let go.
    fn let_go_unwanted(&mut self, pass: u64) {
        let before = self.reading.len();
        self.reading.retain(|_, reading| {
            let wanted = reading.asked + 1 >= pass;
            if !wanted { reading.cancel.store(true, Ordering::Relaxed); }
            wanted
        });
        if self.reading.len() < before && let Some(decoder) = &self.decoder { decoder.prune(); }
    }

    /// A preview read: kept if it is still wanted (one let go meanwhile is not), as last on show
    /// when it was last asked for (so never ahead of those on show since, which it cannot push
    /// out); one of a single frame is a still, never played.
    fn take_read(&mut self, key: String, frames: Frames) {
        let Some(reading) = self.reading.remove(&key) else { return };
        match frames {
            Ok((frames, delays)) if frames.len() > 1 => {
                let mut end = 0.0;
                let ends = delays.iter().map(|delay| { end += delay; end }).collect();
                let frames = frames.into_iter().map(Arc::new).collect();
                self.previews.insert(key.clone(), Preview { frames, ends, texture: None, shown: 0, since: None, painted: reading.asked });
                // (The least lately shown let go, beyond those kept: of two on show as lately, the
                // one just read.)
                while self.previews.len() > PREVIEWS_KEPT {
                    let Some(oldest) = self.previews.iter().min_by_key(|(other, preview)| (preview.painted, **other != key)).map(|(other, _)| other.clone()) else { break };
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

    /// How many previews are read or being read (for the tests).
    #[cfg(test)]
    pub fn previews_held(&self) -> usize { self.previews.len() + self.reading.len() }

    /// Nothing asked for is still being read or looked for (a capture waits for it).
    pub fn idle(&self) -> bool {
        self.pictures.idle() && self.full.idle() && self.tiles.idle() && self.reading.is_empty() && self.locating == 0
    }

    /// Lets go of what only the Mods library shows: the previews read and being read, the
    /// screenshots on the whole window and the themes' pictures; the pointer's rest on a card
    /// forgotten. (The library left: a few small pictures, the cards', are kept for its return.)
    pub fn release(&mut self) {
        self.hover = None;
        if !self.reading.is_empty() {
            for reading in self.reading.values() { reading.cancel.store(true, Ordering::Relaxed); }
            self.reading.clear();
            if let Some(decoder) = &self.decoder { decoder.prune(); }
        }
        self.previews.clear();
        self.full.clear();
        self.tiles.clear();
    }

    /// The screenshots on the whole window let go (it closed).
    pub fn close_full(&mut self) { self.full.clear(); }

    /// Every file looked for again, and what could not be read tried again (RESCAN: a picture that
    /// was locked a moment, or put right since).
    pub fn forget_failures(&mut self) {
        for pictures in [&mut self.pictures, &mut self.full, &mut self.tiles] { pictures.forget_failures(); }
        self.failed.clear();
        self.found.clear();
    }

    /// The file `named` in the mod's folder `dir` as last found (`None`: not known yet;
    /// `Some(None)`: not there), looked for again on its thread when that is a few seconds old.
    fn locate(&mut self, dir: &Path, named: &str, preview: bool) -> Option<Option<File>> {
        let ask: Ask = (dir.to_path_buf(), named.to_string(), preview);
        let found = self.found.entry(ask.clone()).or_insert(Found { at: f64::NEG_INFINITY, file: None, asked: false });
        if !found.asked && self.now - found.at >= FOUND_FOR {
            if self.locator.is_none() { self.locator = Locator::start(); }
            match &self.locator {
                Some(locator) if locator.asks.send(ask).is_ok() => {
                    found.asked = true;
                    self.locating += 1;
                }
                // (No thread to look for it: looked for here.)
                _ => *found = Found { at: self.now, file: Some(find(dir, named, preview, &mut HashMap::new())), asked: false },
            }
        }
        found.file.clone()
    }

    /// A picture of a mod's, asked for if it is not made yet.
    fn picture(&mut self, info: &ModInfo, named: &str, using: Use) -> Got {
        if info.kind != ModKind::Directory || named.trim().is_empty() { return Got::Failed; }
        let file = match self.locate(&info.path, named, false) {
            None => return Got::Waiting,
            Some(None) => return Got::Failed,
            Some(Some(file)) => file,
        };
        let key = format!("{using:?}|{}", file.stamp);
        let pictures = match using { Use::Full => &mut self.full, Use::Tile => &mut self.tiles, _ => &mut self.pictures };
        if let Some(texture) = pictures.get(&key) { return Got::Ready(texture); }
        if pictures.failed(&key) { return Got::Failed; }
        if pictures.wanted(&key) {
            let path = file.path;
            pictures.request(key, move || read_picture(&path, using));
        }
        Got::Waiting
    }

    pub fn banner(&mut self, info: &ModInfo) -> Option<TextureHandle> {
        let named = info.manifest.media.banner.clone();
        self.picture(info, &named, Use::Banner).ready()
    }

    pub fn icon(&mut self, info: &ModInfo) -> Option<TextureHandle> {
        let named = info.manifest.media.icon.clone();
        self.picture(info, &named, Use::Icon).ready()
    }

    /// The picture its interface theme puts behind the HUD's plates (`panel`) or top bar (`bar`),
    /// read as the HUD reads it, to be repeated as the HUD repeats it (a texel a point).
    pub fn theme_picture(&mut self, info: &ModInfo, which: &str) -> Option<TextureHandle> {
        let named = info.manifest.theme.texture("", which)?.to_string();
        self.picture(info, &named, Use::Tile).ready()
    }

    /// The mod's `index`th screenshot of those shown (`Media::shots`), small for the details' row
    /// or `full` for the window.
    pub fn screenshot(&mut self, info: &ModInfo, index: usize, full: bool) -> Got {
        let Some(named) = info.manifest.media.shots().nth(index).map(str::to_string) else { return Got::Failed };
        self.picture(info, &named, if full { Use::Full } else { Use::Thumb })
    }

    /// How many screenshots of the mod's are shown (`Media::shots`: those that name a file).
    pub fn screenshots(info: &ModInfo) -> usize {
        if info.kind != ModKind::Directory { return 0; }
        info.manifest.media.shots().count()
    }

    /// Whether the mod has a gameplay preview to play: its file there, one that moves (a still is
    /// no preview), not one that failed.
    pub fn has_preview(&mut self, info: &ModInfo) -> bool {
        if info.kind != ModKind::Directory || info.manifest.media.preview.trim().is_empty() { return false; }
        let named = info.manifest.media.preview.clone();
        matches!(self.locate(&info.path, &named, true), Some(Some(file)) if file.moves && !self.failed.contains(&file.stamp))
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

    /// The frame of the mod's gameplay preview to show now as it plays, and how long until its
    /// next: None until it is read (asked for to be read, on its worker, newest first; it goes on
    /// being read only while it is asked for, pass after pass).
    pub fn preview(&mut self, ctx: &Context, info: &ModInfo) -> Option<(TextureHandle, f32)> {
        if info.kind != ModKind::Directory { return None; }
        let named = info.manifest.media.preview.clone();
        let file = self.locate(&info.path, &named, true)??;
        if !file.moves || self.failed.contains(&file.stamp) { return None; }
        let (now, pass) = (self.now, ctx.cumulative_pass_nr());
        let Some(preview) = self.previews.get_mut(&file.stamp) else {
            self.read(file, pass);
            return None;
        };
        // (Not on show in the pass before, off the screen a moment: it starts again from its
        // beginning. A frame shown for long has no passes between: it is still on show.)
        if preview.painted + 1 < pass { preview.since = None; }
        preview.painted = pass;
        let since = *preview.since.get_or_insert(now);
        let (index, next) = preview.frame_at(now - since);
        if preview.texture.is_none() {
            preview.texture = Some(ctx.load_texture(format!("mod-preview:{}", file.stamp), preview.frames[index].clone(), TextureOptions::LINEAR));
        } else if preview.shown != index && let Some(texture) = &mut preview.texture {
            texture.set(preview.frames[index].clone(), TextureOptions::LINEAR);
        }
        preview.shown = index;
        preview.texture.clone().map(|texture| (texture, next))
    }

    /// Asks for the preview in `file` to be read (asked for in `pass`; one being read already is
    /// still wanted).
    fn read(&mut self, file: File, pass: u64) {
        if let Some(reading) = self.reading.get_mut(&file.stamp) {
            reading.asked = pass;
            return;
        }
        if self.decoder.is_none() { self.decoder = Decoder::start(); }
        let Some(decoder) = &self.decoder else {
            self.failed.insert(file.stamp);
            return;
        };
        let cancel = Arc::new(AtomicBool::new(false));
        decoder.ask(Job { key: file.stamp.clone(), path: file.path, cancel: cancel.clone() });
        self.reading.insert(file.stamp, Reading { cancel, asked: pass });
    }
}

/// A mod's picture, read (a GIF's first frame) and made no larger than its `using` needs; a
/// theme's as the HUD reads it (a picture it would not draw is not shown).
fn read_picture(path: &Path, using: Use) -> anyhow::Result<Picture> {
    if using == Use::Tile {
        let picture = read_theme_picture(path).map_err(anyhow::Error::msg)?;
        return Ok(Picture { image: ColorImage::from_rgba_unmultiplied([picture.width() as usize, picture.height() as usize], picture.as_raw()), size: None });
    }
    let bytes = read_bounded(path, PICTURE_MAX_BYTES).map_err(anyhow::Error::msg)?;
    let picture = decode_picture(bytes).map_err(anyhow::Error::msg)?;
    let (width, height) = using.largest();
    let picture = if picture.width() > width || picture.height() > height { picture.thumbnail(width, height) } else { picture };
    let picture = picture.to_rgba8();
    Ok(Picture { image: ColorImage::from_rgba_unmultiplied([picture.width() as usize, picture.height() as usize], picture.as_raw()), size: None })
}

/// A gameplay preview's frames, each no larger than [`PREVIEW_SIZE`](ta_data::presentation::PREVIEW_SIZE), and how long each shows (in
/// seconds): as many as [`PREVIEW_BUDGET`](ta_data::presentation::PREVIEW_BUDGET) holds, every other one kept (and shown for both) as
/// often as a long one needs. A still picture is one frame. `let_go` is asked between frames:
/// true, it stops there.
pub fn read_preview(path: &Path, let_go: &dyn Fn() -> bool) -> Result<(Vec<ColorImage>, Vec<f32>), String> {
    use image::{AnimationDecoder, ImageDecoder};
    let bytes = read_bounded(path, PREVIEW_MAX_BYTES)?;
    if image::guess_format(&bytes).ok() != Some(image::ImageFormat::Gif) {
        let picture = read_picture(path, Use::Banner).map_err(|e| e.to_string())?;
        return Ok((vec![picture.image], vec![1.0]));
    }
    // (Within limits: a frame's own size is checked before it is decoded.)
    let decoder = preview_decoder(bytes)?;
    let (width, height) = decoder.dimensions();
    if width == 0 || height == 0 || width > PICTURE_MAX_SIDE || height > PICTURE_MAX_SIDE {
        return Err(format!("a preview of {width} × {height} is not shown"));
    }
    let (w, h) = preview_frame_size(width, height);
    let room = preview_room((w, h));
    let (mut frames, mut delays): (Vec<ColorImage>, Vec<f32>) = (Vec::new(), Vec::new());
    let mut stride = 1usize;
    for (index, frame) in decoder.into_frames().take(PREVIEW_MAX_FRAMES).enumerate() {
        if let_go() { return Err("no longer wanted".into()); }
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

    fn keep() -> bool { false }

    /// A preview plays every frame for as long as it asks, made no larger than the library shows,
    /// and the frames at a time are those due then.
    #[test]
    fn a_preview_plays_its_frames_for_as_long_as_each_asks() {
        let dir = temp("plays");
        let path = dir.join("preview.gif");
        gif(&path, (1280, 720), 6, 20);
        let (frames, delays) = read_preview(&path, &keep).unwrap();
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
        // Let go, it stops at its next frame.
        assert!(read_preview(&path, &|| true).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A frame asking for no time at all (as many GIFs do) shows a tenth of a second, as in a
    /// browser; a still picture is a preview of one frame.
    #[test]
    fn frames_without_a_time_show_a_tenth_and_a_still_is_one_frame() {
        let dir = temp("fast");
        let path = dir.join("fast.gif");
        gif(&path, (64, 32), 3, 0);
        let (_, delays) = read_preview(&path, &keep).unwrap();
        assert!(delays.iter().all(|d| (d - 0.1).abs() < 1e-4), "{delays:?}");
        let still = dir.join("still.png");
        image::RgbaImage::from_pixel(64, 32, image::Rgba([1, 2, 3, 255])).save(&still).unwrap();
        let (frames, _) = read_preview(&still, &keep).unwrap();
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
        let (frames, delays) = read_preview(&path, &keep).unwrap();
        assert!(frames.len() <= ta_data::presentation::PREVIEW_BUDGET / (640 * 360 * 4), "{} frames", frames.len());
        assert!(frames.len() >= 60, "{} frames", frames.len());
        let length: f32 = delays.iter().sum();
        assert!((length - 6.0).abs() < 0.05, "it lasts its 6 seconds: {length}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Only a GIF of two frames or more moves: one of a single frame, a still picture, a GIF cut
    /// short before its second frame and what is no picture at all do not (and nothing of a
    /// frame is decoded to tell).
    #[test]
    fn only_a_gif_of_two_frames_or_more_moves() {
        let dir = temp("moves");
        let path = |name: &str| dir.join(name);
        // (Each frame with colours of its own: a local colour table to step over.)
        gif(&path("two.gif"), (64, 32), 2, 10);
        gif(&path("many.gif"), (640, 360), 12, 4);
        gif(&path("one.gif"), (64, 32), 1, 0);
        image::RgbaImage::from_pixel(64, 32, image::Rgba([1, 2, 3, 255])).save(path("still.png")).unwrap();
        // A GIF of one global colour table, a looping application block and a frame's timing
        // before each frame of a single pixel.
        let gif_of = |frames: usize| {
            let mut bytes = b"GIF89a\x01\x00\x01\x00\x80\x00\x00\x00\x00\x00\xff\xff\xff".to_vec();
            bytes.extend_from_slice(b"\x21\xff\x0bNETSCAPE2.0\x03\x01\x00\x00\x00");
            for _ in 0..frames {
                bytes.extend_from_slice(b"\x21\xf9\x04\x00\x0a\x00\x00\x00");
                bytes.extend_from_slice(b"\x2c\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02\x44\x01\x00");
            }
            bytes.push(0x3b);
            bytes
        };
        std::fs::write(path("global_two.gif"), gif_of(2)).unwrap();
        std::fs::write(path("global_one.gif"), gif_of(1)).unwrap();
        let two = std::fs::read(path("two.gif")).unwrap();
        // (Cut in its first frame's data: never known to move.)
        std::fs::write(path("cut.gif"), &two[..two.len() / 2]).unwrap();
        std::fs::write(path("text.gif"), b"GIF89a, said a text file").unwrap();
        for (name, moving) in [("two.gif", true), ("many.gif", true), ("global_two.gif", true), ("one.gif", false), ("global_one.gif", false),
            ("still.png", false), ("cut.gif", false), ("text.gif", false), ("none.gif", false)] {
            assert_eq!(moves(&path(name)), moving, "{name}");
        }
        // (The hand-made ones are GIFs the decoder reads: two frames, and one.)
        assert_eq!(read_preview(&path("global_two.gif"), &keep).unwrap().0.len(), 2);
        assert_eq!(read_preview(&path("global_one.gif"), &keep).unwrap().0.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A mod of `files` (name, bytes) in a folder of its own, its manifest naming them.
    fn module(dir: &Path, manifest: &str) -> ModInfo {
        std::fs::write(dir.join("mod.toml"), format!("id = \"shown\"\nname = \"Shown\"\n{manifest}")).unwrap();
        ModInfo { manifest: toml::from_str(&std::fs::read_to_string(dir.join("mod.toml")).unwrap()).unwrap(), path: dir.to_path_buf(), kind: ModKind::Directory }
    }

    /// Runs frames until `done` says so (or ten seconds pass), the media polled in each.
    fn frames_until(ctx: &Context, media: &mut ModMedia, mut done: impl FnMut(&Context, &mut ModMedia) -> bool) -> bool {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            let mut finished = false;
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                media.poll(ui.ctx());
                finished = done(ui.ctx(), media);
            });
            output.textures_delta.clear();
            if finished { return true; }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        false
    }

    /// A screenshot that is not there, one of a kind the launcher does not read, and one over
    /// its size each come to nothing, said so (never a rail at work for ever); a good one comes
    /// in; and RESCAN tries again one that failed.
    #[test]
    fn a_picture_not_to_be_had_says_so_and_a_rescan_tries_it_again() {
        let dir = temp("states");
        std::fs::create_dir_all(dir.join("media")).unwrap();
        image::RgbImage::from_pixel(64, 36, image::Rgb([200, 0, 0])).save(dir.join("media/picture.bmp")).unwrap();
        image::GrayImage::new(5000, 20).save(dir.join("media/huge.png")).unwrap();
        image::RgbImage::from_pixel(640, 360, image::Rgb([0, 90, 0])).save(dir.join("media/fine.png")).unwrap();
        let info = module(&dir, "[media]\nscreenshots = [\"media/mistyped.png\", \"media/picture.bmp\", \"media/huge.png\", \"media/fine.png\"]\n");
        let ctx = Context::default();
        let mut media = ModMedia::default();
        assert_eq!(ModMedia::screenshots(&info), 4);
        let mut states = Vec::new();
        assert!(frames_until(&ctx, &mut media, |_, media| {
            states = (0..4).map(|index| media.screenshot(&info, index, false)).collect();
            !states.iter().any(|got| matches!(got, Got::Waiting))
        }), "every screenshot comes to something");
        assert!(states[0].failed() && states[1].failed() && states[2].failed(), "missing, a BMP, over 4096 pixels a side");
        assert!(states[3].texture().is_some_and(|texture| texture.size() == [480, 270]), "made no larger than the row shows");
        assert!(frames_until(&ctx, &mut media, |_, media| media.idle()), "nothing left at work");
        // Put right as it was (a PNG in the BMP's place, of its very size and time: as a file read
        // while it was being written, or locked a moment), it shows only after a rescan.
        let bmp = dir.join("media/picture.bmp");
        let (length, time) = std::fs::metadata(&bmp).map(|meta| (meta.len() as usize, meta.modified().unwrap())).unwrap();
        let mut png = std::io::Cursor::new(Vec::new());
        image::RgbImage::from_pixel(64, 36, image::Rgb([0, 0, 200])).write_to(&mut png, image::ImageFormat::Png).unwrap();
        let mut bytes = png.into_inner();
        assert!(bytes.len() < length, "a plain PNG is the smaller");
        bytes.resize(length, 0);
        std::fs::write(&bmp, &bytes).unwrap();
        std::fs::File::options().write(true).open(&bmp).unwrap().set_modified(time).unwrap();
        for _ in 0..3 { frames_until(&ctx, &mut media, |_, _| true); }
        assert!(media.screenshot(&info, 1, false).failed(), "known by its path, size and time: still failed");
        media.forget_failures();
        assert!(frames_until(&ctx, &mut media, |_, media| media.screenshot(&info, 1, false).texture().is_some()), "RESCAN reads it again");
        drop(media);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A preview asked for is read on its worker and plays; one no longer asked for (not on show
    /// in the pass before) is let go, its job's flag raised, and the worker skips a job let go
    /// before its turn (nothing comes back of it).
    #[test]
    fn a_preview_is_read_while_it_is_asked_for_and_let_go_after() {
        let dir = temp("let_go");
        std::fs::create_dir_all(dir.join("media")).unwrap();
        gif(&dir.join("media/long.gif"), (640, 360), 30, 4);
        let info = module(&dir, "[media]\npreview = \"media/long.gif\"\n");
        let ctx = Context::default();
        let mut media = ModMedia::default();
        assert!(frames_until(&ctx, &mut media, |_, media| media.has_preview(&info)), "a GIF of many frames moves");
        // Asked for pass after pass: read, and it plays.
        assert!(frames_until(&ctx, &mut media, |ctx, media| media.preview(ctx, &info).is_some()), "it plays");
        assert!(media.reading.is_empty() && media.previews.len() == 1);
        // A reading asked for in a pass goes on in the next, and is let go in the one after.
        let cancel = Arc::new(AtomicBool::new(false));
        media.reading.insert("asked".into(), Reading { cancel: cancel.clone(), asked: 10 });
        media.let_go_unwanted(11);
        assert!(media.reading.contains_key("asked") && !cancel.load(Ordering::Relaxed), "asked for in the pass before: wanted");
        media.let_go_unwanted(12);
        assert!(!media.reading.contains_key("asked") && cancel.load(Ordering::Relaxed), "not asked for since: let go, its job told");
        // The worker skips a job let go before its turn, and reads the next.
        let decoder = Decoder::start().unwrap();
        let path = dir.join("media/long.gif");
        decoder.ask(Job { key: "wanted".into(), path: path.clone(), cancel: Arc::new(AtomicBool::new(false)) });
        decoder.ask(Job { key: "let go".into(), path, cancel: Arc::new(AtomicBool::new(true)) });
        let (key, read) = decoder.results.recv_timeout(std::time::Duration::from_secs(20)).expect("the wanted one read");
        assert_eq!(key, "wanted");
        assert!(read.is_ok_and(|(frames, _)| frames.len() == 30));
        assert!(decoder.results.recv_timeout(std::time::Duration::from_millis(300)).is_err(), "nothing of the one let go");
        drop(decoder);
        // The library left: everything of the previews let go.
        media.release();
        assert!(media.previews.is_empty() && media.reading.is_empty());
        drop(media);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A preview read late, last asked for long ago, never pushes out one on show: it is let go
    /// itself, beyond those kept; one no longer wanted is not kept at all; one of a single frame
    /// is a still, no preview.
    #[test]
    fn a_late_preview_pushes_out_none_on_show() {
        let mut media = ModMedia::default();
        let frame = || Arc::new(ColorImage::filled([2, 2], egui::Color32::RED));
        let frames = |n: usize| Ok((vec![ColorImage::filled([2, 2], egui::Color32::RED); n], vec![0.1; n]));
        for (key, painted) in [("details", 100), ("card", 100)] {
            media.previews.insert(key.into(), Preview { frames: vec![frame(), frame()], ends: vec![0.1, 0.2], texture: None, shown: 0, since: None, painted });
        }
        media.reading.insert("late".into(), Reading { cancel: Arc::new(AtomicBool::new(false)), asked: 40 });
        media.take_read("late".into(), frames(2));
        let mut kept: Vec<&String> = media.previews.keys().collect();
        kept.sort();
        assert_eq!(kept, ["card", "details"], "those on show stay");
        // Asked for as lately as those on show (one of them gone from the screen meanwhile).
        media.previews.get_mut("card").unwrap().painted = 90;
        media.reading.insert("next".into(), Reading { cancel: Arc::new(AtomicBool::new(false)), asked: 100 });
        media.take_read("next".into(), frames(2));
        let mut kept: Vec<&String> = media.previews.keys().collect();
        kept.sort();
        assert_eq!(kept, ["details", "next"], "the one off the screen let go");
        media.take_read("unwanted".into(), frames(2));
        assert!(!media.previews.contains_key("unwanted"), "not asked for: not kept");
        media.reading.insert("still".into(), Reading { cancel: Arc::new(AtomicBool::new(false)), asked: 200 });
        media.take_read("still".into(), frames(1));
        assert!(!media.previews.contains_key("still") && media.failed.contains("still"), "one frame: no preview");
    }

    /// A still picture or a GIF of one frame named as the preview is no preview: no mark, nothing
    /// played, no frames asked for.
    #[test]
    fn a_still_named_as_the_preview_is_no_preview() {
        let dir = temp("still_preview");
        std::fs::create_dir_all(dir.join("media")).unwrap();
        gif(&dir.join("media/one.gif"), (64, 32), 1, 0);
        image::RgbaImage::from_pixel(64, 32, image::Rgba([1, 2, 3, 255])).save(dir.join("media/still.png")).unwrap();
        let ctx = Context::default();
        for named in ["media/one.gif", "media/still.png"] {
            let info = module(&dir, &format!("[media]\npreview = \"{named}\"\n"));
            let mut media = ModMedia::default();
            assert!(frames_until(&ctx, &mut media, |_, media| media.idle() && media.locate(&info.path, named, true).is_some()));
            assert!(!media.has_preview(&info), "{named}");
            frames_until(&ctx, &mut media, |ctx, media| media.preview(ctx, &info).is_none());
            assert!(media.reading.is_empty(), "{named}: nothing read");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
