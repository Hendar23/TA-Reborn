//! Supreme Commander: Forged Alliance's maps, imported (`ta_supcom`): the game found (GOG's or
//! Steam's copy, or a folder the player picks), its skirmish maps and the player's own listed
//! with the game's own pictures of them, and those chosen made into TA maps, one after another
//! on a worker, into one map pack (`map-pack-supreme-commander`), which then shows in the map
//! library and in skirmish. Nothing of the game's is shipped: each player's maps are made from
//! their own copy, the same on every computer. The dialog's other tab imports the game's
//! factions (`crate::supcom_factions`).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::JoinHandle;

use anyhow::{Context, Result};
use ta_formats::image::Palette;
use ta_supcom::convert::{self, Options, Stage, kit::Templates};
use ta_supcom::files::Files;
use ta_supcom::install::{self, Install};
use ta_supcom::map::{self, Map, Scenario};
use ta_ui::i18n::{n, t, tf};
use ta_vfs::mods::ModManifest;

/// The map pack the maps go in (a map pack: the library offers to remove it as one).
pub const PACK_ID: &str = "map-pack-supreme-commander";
/// The pack's name and words as its manifest keeps them, in English (a mod's own words): the
/// interface shows them translated ([`pack_name`]; the Mods page translates a description).
const PACK_NAME: &str = n("Supreme Commander maps");
const PACK_DESCRIPTION: &str = n("Maps of Supreme Commander: Forged Alliance, made into TA maps from your own copy of the game.");
/// A map's picture in the list is made at most this many pixels a side (the game's are 256,
/// shown 56 points across).
const PREVIEW_SIDE: u32 = 128;
/// The largest picture of a map read for the list, a side: a player's map may hold one of any
/// size, and one larger is left out (no picture), never decoded.
const PREVIEW_LARGEST: u32 = 2048;

/// The pack's name as the interface shows it, in the language in use.
pub fn pack_name() -> &'static str {
    t(PACK_NAME)
}

/// A mod's name as the interface shows it: this pack's and the factions' in the language in use
/// (their manifests keep them in English), any other's as its manifest has it.
pub fn mod_name(manifest: &ModManifest) -> &str {
    match manifest.id.as_str() {
        PACK_ID => pack_name(),
        crate::supcom_factions::MOD_ID => crate::supcom_factions::mod_name(),
        _ => &manifest.name,
    }
}

/// The dialog's tabs: the game's maps, or its factions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Maps,
    Factions,
}

/// A map that can be imported.
#[derive(Clone)]
pub struct Entry {
    pub folder: PathBuf,
    /// One of the player's own (downloaded, Forged Alliance Forever's), not the game's.
    pub players_own: bool,
    pub scenario: Scenario,
    pub state: State,
}

/// The game's pictures of the maps a look found, made small (RGBA), by map folder.
pub type Previews = BTreeMap<PathBuf, egui::ColorImage>;

/// Whether a map is in the pack already.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    New,
    Imported,
    /// Imported by an older version of the converter: importing it again makes it better.
    Outdated,
}

impl State {
    pub fn label(self) -> Option<&'static str> {
        match self {
            State::New => None,
            State::Imported => Some(n("IMPORTED")),
            State::Outdated => Some(n("UPDATE")),
        }
    }
}

/// What a look for the game found.
#[derive(Clone, Default)]
pub struct Found {
    /// The copy of the game in use: the player's choice, else the first found.
    pub install: Option<Install>,
    /// Every copy of the game found (GOG's, Steam's...), in the order looked for.
    pub copies: Vec<Install>,
    /// The folders of the player's own maps there are.
    pub own_folders: Vec<PathBuf>,
    pub maps: Vec<Entry>,
    /// Why the game's maps could not be listed (its maps folder would not open).
    pub error: Option<String>,
    /// What could not be read and is not listed: a folder of the player's own maps, a map whose
    /// scenario file would not read (translated, a line each).
    pub skipped: Vec<String>,
}

/// How an import is going.
#[derive(Clone, Default)]
pub struct Progress {
    pub done: usize,
    pub total: usize,
    pub current: String,
    /// The share of the current map painted.
    pub share: f32,
}

/// What an import came to.
#[derive(Clone, Debug, Default)]
pub struct Outcome {
    pub imported: Vec<String>,
    /// The folders (their names, lower case) of the maps made.
    pub made: Vec<String>,
    /// Maps that could not be made, and why.
    pub failed: Vec<(String, String)>,
    pub stopped: bool,
    /// Why nothing was installed, when nothing was.
    pub error: Option<String>,
}

/// How an import went, as it is told.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Went {
    /// Every map made.
    Well,
    /// Some made, some not.
    Partly,
    /// Nothing installed: the import failed, or none of the maps could be made.
    Badly,
    /// Stopped before a map was made.
    Nothing,
}

impl Went {
    pub fn colour(self) -> egui::Color32 {
        match self {
            Went::Well => crate::theme::GREEN,
            Went::Partly => crate::theme::AMBER,
            Went::Badly => crate::theme::RED,
            Went::Nothing => crate::theme::STEEL,
        }
    }
}

impl Outcome {
    pub fn went(&self) -> Went {
        if self.error.is_some() || (self.imported.is_empty() && !self.failed.is_empty()) {
            Went::Badly
        } else if self.imported.is_empty() {
            Went::Nothing
        } else if self.failed.is_empty() {
            Went::Well
        } else {
            Went::Partly
        }
    }

    /// What it came to, as the dialog says it.
    pub fn summary(&self) -> String {
        let (made, failed) = (self.imported.len(), self.failed.len());
        match (&self.error, self.went()) {
            (Some(why), _) => tf("The maps could not be imported: {why}", &[("why", why)]),
            (None, Went::Badly) => tf("None of the {n} maps could be made.", &[("n", &failed)]),
            (None, Went::Nothing) => t("Nothing was imported.").to_string(),
            (None, Went::Well) => tf("Imported {n} maps: they are in the map library, and in skirmish.", &[("n", &made)]),
            (None, Went::Partly) => tf("Imported {n} maps, but {failed} could not be made: those made are in the map library, and in skirmish.", &[("n", &made), ("failed", &failed)]),
        }
    }

    /// What a notice says of it with the dialog closed, failures and all (nothing for a stop
    /// before any map was made).
    pub fn notice(&self) -> Option<String> {
        let (made, failed) = (self.imported.len(), self.failed.len());
        Some(match (&self.error, self.went()) {
            (Some(why), _) => tf("The Supreme Commander maps could not be imported: {why}", &[("why", why)]),
            (None, Went::Badly) => tf("None of the {n} Supreme Commander maps could be made: SUPREME COMMANDER… on the Maps page says why.", &[("n", &failed)]),
            (None, Went::Nothing) => return None,
            (None, Went::Well) => tf("Imported {n} Supreme Commander maps: they are in the map library, and in skirmish.", &[("n", &made)]),
            (None, Went::Partly) => tf("Imported {n} Supreme Commander maps, but {failed} could not be made: SUPREME COMMANDER… on the Maps page says why.", &[("n", &made), ("failed", &failed)]),
        })
    }
}

struct Job {
    handle: JoinHandle<Outcome>,
    ending: Arc<Ending>,
}

/// How an import is asked to end: after the map in hand (`stop`: the player's STOP), or at once,
/// the maps made installed and the one in hand left out (`abandon`: the launcher closing).
#[derive(Default)]
struct Ending {
    stop: AtomicBool,
    abandon: AtomicBool,
}

#[derive(Default)]
pub struct Import {
    pub open: bool,
    /// The game's folder as the player chose it (else the copy found).
    pub picked: Option<PathBuf>,
    scan: Option<mpsc::Receiver<(Found, Previews)>>,
    /// What the last look found, shared: the dialog draws from it every frame.
    pub found: Option<Arc<Found>>,
    /// The maps chosen (their folders). None until the game's maps are first listed, when the
    /// game's own not imported yet are chosen for the player ([`first_choice`]); after that what
    /// the player chose, none at all if so.
    pub chosen: Option<BTreeSet<PathBuf>>,
    job: Option<Job>,
    pub progress: Arc<Mutex<Progress>>,
    pub outcome: Option<Outcome>,
    /// The maps' pictures not shown yet (their pixels, let go of once on the GPU), and those
    /// shown, as textures, by map folder: all let go of when the dialog closes.
    previews: Previews,
    textures: BTreeMap<PathBuf, egui::TextureHandle>,
    /// Maps to choose and import as soon as the game's maps are listed (their folders' names, in
    /// lower case): a capture's (`--show supcom:<folder>,<folder>`).
    pub auto_start: Option<Vec<String>>,
    pub tab: Tab,
    /// The game's factions, the other tab's.
    pub factions: crate::supcom_factions::Factions,
}

impl Import {
    pub fn scanning(&self) -> bool {
        self.scan.is_some()
    }

    pub fn importing(&self) -> bool {
        self.job.is_some()
    }

    /// Opens the dialog and looks for the game (again): the maps' pictures were let go of when it
    /// closed. (An import under way goes on meanwhile.)
    pub fn show(&mut self, mods_dir: &Path) {
        self.open = true;
        self.tab = Tab::Maps;
        if !self.scanning() {
            self.rescan(mods_dir);
        }
    }

    /// Opens the dialog at its factions and looks for the game's units (again).
    pub fn show_factions(&mut self, mods_dir: &Path) {
        self.open = true;
        self.tab = Tab::Factions;
        if !self.factions.scanning() {
            self.factions.rescan(self.picked.clone(), mods_dir);
        }
    }

    /// Shows the tab `tab`: the maps' looked for where they have not been yet, the factions' each
    /// time (a second or so: what the mods folder holds may have changed, and their pictures were
    /// let go of with the dialog).
    pub fn set_tab(&mut self, tab: Tab, mods_dir: &Path) {
        self.tab = tab;
        match tab {
            Tab::Maps if self.found.is_none() && !self.scanning() => self.rescan(mods_dir),
            Tab::Factions if !self.factions.scanning() && !self.factions.importing() => self.factions.rescan(self.picked.clone(), mods_dir),
            _ => {}
        }
    }

    /// Looks for the game and its maps (in the player's chosen folder, else wherever a copy is).
    pub fn rescan(&mut self, mods_dir: &Path) {
        let (send, receive) = mpsc::channel();
        let picked = self.picked.clone();
        let mods_dir = mods_dir.to_path_buf();
        std::thread::Builder::new().name("supcom scan".into()).spawn(move || {
            let _ = send.send(find(picked.as_deref(), pack_dir(&mods_dir).as_deref()));
        }).ok();
        self.scan = Some(receive);
    }

    /// Whether the map in `folder` is chosen.
    pub fn is_chosen(&self, folder: &Path) -> bool {
        self.chosen.as_ref().is_some_and(|chosen| chosen.contains(folder))
    }

    /// Chooses the map in `folder` (`on`), or not.
    pub fn choose(&mut self, folder: &Path, on: bool) {
        let chosen = self.chosen.get_or_insert_with(BTreeSet::new);
        if on {
            chosen.insert(folder.to_path_buf());
        } else {
            chosen.remove(folder);
        }
    }

    /// Chooses every map listed (`on`), or none.
    pub fn choose_all(&mut self, on: bool) {
        let all = || self.found.iter().flat_map(|found| found.maps.iter().map(|e| e.folder.clone())).collect();
        self.chosen = Some(if on { all() } else { BTreeSet::new() });
    }

    /// The picture of the map in `folder` as a texture, made from its pixels the first time it is
    /// shown (they are let go of then).
    pub fn texture(&mut self, ctx: &egui::Context, folder: &Path) -> Option<egui::TextureHandle> {
        if let Some(texture) = self.textures.get(folder) {
            return Some(texture.clone());
        }
        let image = self.previews.remove(folder)?;
        let texture = ctx.load_texture(format!("supcom:{}", folder.display()), image, egui::TextureOptions::LINEAR);
        self.textures.insert(folder.to_path_buf(), texture.clone());
        Some(texture)
    }

    /// How many pictures are held as pixels, and as textures.
    #[cfg(test)]
    pub fn pictures_held(&self) -> (usize, usize) {
        (self.previews.len(), self.textures.len())
    }

    /// Uses the game in folder `picked` (or one level down in it); false when it is not there.
    /// Both tabs list that copy's then (the factions' where they have been looked at).
    pub fn pick(&mut self, picked: &Path, mods_dir: &Path) -> bool {
        match install::validate_picked(picked) {
            Some(root) => {
                self.picked = Some(root);
                if self.found.is_some() || self.tab == Tab::Maps {
                    self.rescan(mods_dir);
                }
                if self.factions.found.is_some() || self.tab == Tab::Factions {
                    self.factions.rescan(self.picked.clone(), mods_dir);
                }
                true
            }
            None => false,
        }
    }

    /// Starts making the chosen maps into TA maps, the original game at `ta_root`.
    pub fn start(&mut self, ta_root: PathBuf, store_dir: PathBuf) {
        let Some(found) = self.found.clone() else { return };
        let Some(game) = found.install.clone() else { return };
        if self.importing() {
            return;
        }
        let maps: Vec<(PathBuf, bool, String)> = found.maps.iter().filter(|e| self.is_chosen(&e.folder)).map(|e| (e.folder.clone(), e.players_own, e.scenario.name.clone())).collect();
        if maps.is_empty() {
            return;
        }
        let own = found.own_folders.clone();
        let ending = Arc::new(Ending::default());
        *self.progress.lock().unwrap() = Progress { total: maps.len(), ..Default::default() };
        let progress = self.progress.clone();
        let asked = ending.clone();
        self.outcome = None;
        let handle = std::thread::Builder::new().name("supcom import".into()).spawn(move || {
            import(&game, &own, &maps, &ta_root, &store_dir, &progress, &asked).unwrap_or_else(|e| Outcome { error: Some(format!("{e:#}")), ..Default::default() })
        });
        match handle {
            Ok(handle) => self.job = Some(Job { handle, ending }),
            Err(e) => self.outcome = Some(Outcome { error: Some(e.to_string()), ..Default::default() }),
        }
    }

    /// The maps of `auto_start`, chosen, once the game's maps are listed: true when they are to
    /// be imported now.
    pub fn take_auto_start(&mut self) -> bool {
        if self.scanning() || self.importing() {
            return false;
        }
        let (Some(names), Some(found)) = (&self.auto_start, &self.found) else { return false };
        let chosen: BTreeSet<PathBuf> = found.maps.iter().filter(|e| e.folder.file_name().is_some_and(|f| names.iter().any(|n| f.to_string_lossy().eq_ignore_ascii_case(n)))).map(|e| e.folder.clone()).collect();
        let any = !chosen.is_empty();
        self.chosen = Some(chosen);
        self.auto_start = None;
        any
    }

    /// Whether a capture may take its picture: the game looked for, a capture's maps imported
    /// (or, with `TA_SUPCOM_SHOT_IMPORTING=1`, while they are).
    pub fn capture_ready(&self) -> bool {
        let during = std::env::var_os("TA_SUPCOM_SHOT_IMPORTING").is_some_and(|v| v == "1");
        !self.open || (!self.scanning() && self.auto_start.is_none() && (during || !self.importing()) && self.factions.capture_ready())
    }

    /// Stops the import after the map being made.
    pub fn stop(&self) {
        if let Some(job) = &self.job {
            job.ending.stop.store(true, Ordering::Relaxed);
        }
    }

    /// Ends the import at once (the launcher closing): the maps made are installed, the one being
    /// made is left out.
    pub fn abandon(&self) {
        if let Some(job) = &self.job {
            job.ending.abandon.store(true, Ordering::Relaxed);
            job.ending.stop.store(true, Ordering::Relaxed);
        }
    }

    /// Takes in what the workers found and made; `Some` when an import has just installed the
    /// pack (its maps to be looked through again, the pack turned on).
    pub fn poll(&mut self, ctx: &egui::Context) -> Option<Outcome> {
        if let Some(scan) = &self.scan {
            match scan.try_recv() {
                Ok((found, previews)) => {
                    let same_copy = self.found.as_ref().is_some_and(|f| f.install == found.install);
                    match &mut self.chosen {
                        // (What the player chose, less the maps no longer there: none if so.)
                        Some(chosen) if same_copy => chosen.retain(|c| found.maps.iter().any(|e| &e.folder == c)),
                        // (The game's maps listed for the first time, or another copy's.)
                        _ => self.chosen = Some(first_choice(&found)),
                    }
                    self.textures.clear();
                    // (The pictures are kept only for a dialog open to show them.)
                    self.previews = if self.open { previews } else { Previews::new() };
                    self.found = Some(Arc::new(found));
                    self.scan = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => self.scan = None,
                Err(mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(100)),
            }
        }
        // Closed, the dialog lets go of the maps' pictures, on the GPU and off it.
        if !self.open && (!self.previews.is_empty() || !self.textures.is_empty()) {
            self.previews.clear();
            self.textures.clear();
        }
        if self.job.as_ref().is_some_and(|j| j.handle.is_finished()) {
            let job = self.job.take().unwrap();
            let outcome = job.handle.join().unwrap_or_else(|_| Outcome { error: Some(t("The import stopped unexpectedly.").to_string()), ..Default::default() });
            // (The maps made are chosen no more: what is left chosen is what was not made, those
            // that could not be among them.)
            if outcome.error.is_none()
                && let Some(chosen) = &mut self.chosen
            {
                let made: BTreeSet<&str> = outcome.made.iter().map(|f| f.as_str()).collect();
                chosen.retain(|f| !made.contains(m_source(f).as_str()));
            }
            self.outcome = Some(outcome.clone());
            return Some(outcome);
        }
        if self.job.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        None
    }

    /// Ends an import under way at shutdown and waits for it: the maps made are installed (the
    /// pack whole or not at all), the one being made left out. Its outcome, the maps made to be
    /// turned on.
    pub fn finish_on_exit(&mut self) -> Option<Outcome> {
        self.abandon();
        let job = self.job.take()?;
        job.handle.join().ok()
    }

    /// An import under way that ends, stopped, once asked to stop, having made the maps of
    /// folders `made`: for tests.
    #[cfg(test)]
    pub fn pretend_importing(&mut self, made: &[&str]) {
        let ending = Arc::new(Ending::default());
        let asked = ending.clone();
        let made: Vec<String> = made.iter().map(|m| m.to_string()).collect();
        let handle = std::thread::spawn(move || {
            while !asked.stop.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            Outcome { imported: made.clone(), made, stopped: true, ..Default::default() }
        });
        self.job = Some(Job { handle, ending });
    }

    /// An import under way that comes to `outcome` at once: for tests.
    #[cfg(test)]
    pub fn pretend_import_ending(&mut self, outcome: Outcome) {
        let handle = std::thread::spawn(move || outcome);
        self.job = Some(Job { handle, ending: Arc::new(Ending::default()) });
    }

    /// A look that found `found` and the pictures `previews`, taken in at the next
    /// [`Import::poll`] as a look's are: for tests.
    #[cfg(test)]
    pub fn pretend_found(&mut self, found: Found, previews: Previews) {
        let (send, receive) = mpsc::channel();
        send.send((found, previews)).unwrap();
        self.scan = Some(receive);
    }
}

/// The maps chosen for the player when the game's maps are first listed: the game's own not
/// imported yet (or made by an older converter). The player's own maps are theirs to choose: a
/// vault holds hundreds, gigabytes once made.
fn first_choice(found: &Found) -> BTreeSet<PathBuf> {
    found.maps.iter().filter(|e| !e.players_own && e.state != State::Imported).map(|e| e.folder.clone()).collect()
}

/// The folder the pack is installed in, if it is.
fn pack_dir(mods_dir: &Path) -> Option<PathBuf> {
    mod_dir(mods_dir, PACK_ID)
}

/// The folder the mod `id` is installed in, found by its id (where its name was taken the
/// installer named it `<id>-2`...), if it is.
pub(crate) fn mod_dir(mods_dir: &Path, id: &str) -> Option<PathBuf> {
    ta_vfs::mods::scan(mods_dir).0.into_iter().find(|m| m.manifest.id == id && m.kind == ta_vfs::mods::ModKind::Directory).map(|m| m.path)
}

/// The game, the player's own map folders and every map that can be imported, with which are in
/// the pack (`pack`, its folder) already; and the game's pictures of them.
fn find(picked: Option<&Path>, pack: Option<&Path>) -> (Found, Previews) {
    let copies = install::discover();
    let install = in_use(&copies, picked);
    let own_folders = install::user_map_folders();
    let mut found = Found { install: install.clone(), copies, own_folders: own_folders.clone(), ..Default::default() };
    let mut previews = Previews::new();
    let Some(game) = install else { return (found, previews) };
    // What the pack holds: each map's source folder and the converter that made it.
    let mut made: BTreeMap<String, u32> = BTreeMap::new();
    if let Some(pack) = pack
        && let Ok(entries) = std::fs::read_dir(pack.join("maps"))
    {
        for e in entries.flatten() {
            if e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("ota"))
                && let Ok(bytes) = std::fs::read(e.path())
            {
                let doc = ta_formats::tdf::parse_bytes(&bytes);
                if let Some(h) = doc.section("GlobalHeader")
                    && let Some(source) = h.get("supcommap")
                {
                    made.insert(source.trim().to_ascii_lowercase(), h.get_i64_or("supcomversion", 0) as u32);
                }
            }
        }
    }
    let mut dirs: Vec<(PathBuf, bool)> = own_folders.iter().map(|d| (d.clone(), true)).collect();
    dirs.push((game.maps(), false));
    let mut seen = BTreeSet::new();
    for (dir, players_own) in dirs {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) => {
                let why = tf("{folder} could not be read: {why}", &[("folder", &dir.display()), ("why", &e)]);
                if players_own {
                    found.skipped.push(why);
                } else {
                    found.error = Some(why);
                }
                continue;
            }
        };
        let mut folders: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
        folders.sort();
        for folder in folders {
            let scenario = match map::read_scenario(&folder) {
                Ok(scenario) => scenario,
                // (A folder without a scenario is no map.)
                Err(ta_supcom::Error::Missing(_)) => continue,
                Err(e) => {
                    let name = folder.file_name().map_or_else(|| folder.display().to_string(), |n| n.to_string_lossy().into_owned());
                    found.skipped.push(tf("{map}: its scenario file could not be read: {why}", &[("map", &name), ("why", &e)]));
                    continue;
                }
            };
            if !scenario.skirmish() || !seen.insert(scenario.folder.to_ascii_lowercase()) {
                continue;
            }
            if let Some(preview) = preview_of(&folder) {
                previews.insert(folder.clone(), preview);
            }
            let state = match made.get(&scenario.folder.to_ascii_lowercase()) {
                Some(&v) if v >= convert::VERSION => State::Imported,
                Some(_) => State::Outdated,
                None => State::New,
            };
            found.maps.push(Entry { folder, players_own, scenario, state });
        }
    }
    found.maps.sort_by(|a, b| a.scenario.name.to_lowercase().cmp(&b.scenario.name.to_lowercase()).then(a.folder.cmp(&b.folder)));
    (found, previews)
}

/// The game's picture of the map in `folder`, made small for the list: none where it has none, or
/// one too large to read (a player's map may hold any size: it is not decoded).
fn preview_of(folder: &Path) -> Option<egui::ColorImage> {
    let (_, _, dds) = ta_supcom::scmap::ScMap::read_preview(&scmap_of(folder)?).ok()?;
    let header = ta_supcom::dds::Header::parse(&dds).ok()?;
    // (Its first level bounded: a smaller one missing, that is the one decoded.)
    if header.width.max(header.height) > PREVIEW_LARGEST {
        return None;
    }
    let image = ta_supcom::dds::decode(&dds, header.level_for(PREVIEW_SIDE)).ok()?;
    (image.rgba.len() == image.width as usize * image.height as usize).then(|| shrink(&image, PREVIEW_SIDE))
}

/// `image` made at most `side` pixels along its longer side (each pixel the mean of those it
/// covers), opaque: the game's pictures leave their alpha unset.
fn shrink(image: &ta_supcom::dds::Image, side: u32) -> egui::ColorImage {
    let (w, h) = (image.width.max(1) as usize, image.height.max(1) as usize);
    let (longer, side) = (w.max(h), side.max(1) as usize);
    let (to_w, to_h) = if longer <= side { (w, h) } else { ((w * side).div_ceil(longer).max(1), (h * side).div_ceil(longer).max(1)) };
    let mut pixels = Vec::with_capacity(to_w * to_h);
    for y in 0..to_h {
        let (top, bottom) = (y * h / to_h, ((y + 1) * h / to_h).max(y * h / to_h + 1));
        for x in 0..to_w {
            let (left, right) = (x * w / to_w, ((x + 1) * w / to_w).max(x * w / to_w + 1));
            let mut sum = [0u32; 3];
            for row in top..bottom {
                for texel in &image.rgba[row * w + left..row * w + right] {
                    for (s, v) in sum.iter_mut().zip(texel) {
                        *s += u32::from(*v);
                    }
                }
            }
            let count = ((bottom - top) * (right - left)) as u32;
            pixels.push(egui::Color32::from_rgb((sum[0] / count) as u8, (sum[1] / count) as u8, (sum[2] / count) as u8));
        }
    }
    egui::ColorImage::new([to_w, to_h], pixels)
}

/// The copy of the game in use: the folder picked (named by its store when it is one of the
/// `copies` found, GOG's or Steam's), else the first found.
pub(crate) fn in_use(copies: &[Install], picked: Option<&Path>) -> Option<Install> {
    match picked {
        Some(root) => Some(copies.iter().find(|copy| install::same_folder(&copy.root, root)).cloned().unwrap_or_else(|| Install { root: root.to_path_buf(), store: install::Store::Folder })),
        None => copies.first().cloned(),
    }
}

/// A map folder's map file (its own `<name>.scmap`, any case; else the first there).
fn scmap_of(folder: &Path) -> Option<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(folder).ok()?.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("scmap"))).collect();
    files.sort();
    files.into_iter().next()
}

/// Makes `maps` (folder, the player's own, name) into TA maps and installs the pack: the maps
/// already in it kept, those made again replaced.
fn import(game: &Install, own: &[PathBuf], maps: &[(PathBuf, bool, String)], ta_root: &Path, store_dir: &Path, progress: &Arc<Mutex<Progress>>, ending: &Ending) -> Result<Outcome> {
    // TA's own features and palette: from the original game alone, as every player has it.
    let mut vfs = ta_vfs::Vfs::new();
    vfs.mount_game_dir(ta_root, "base").context(t("Total Annihilation's own data could not be read."))?;
    let palette = Arc::new(Palette::parse(&vfs.read("palettes/palette.pal")?)?);
    let templates = Arc::new(Templates::load(&vfs));
    let files = Arc::new(Files::open(game, own)?);
    let mods_dir = store_dir.join("mods");
    let work = crate::maps::Workspace::new(&store_dir.join("cache").join("map-imports"))?;
    let payload = work.0.join("payload");
    let maps_out = payload.join("maps");
    std::fs::create_dir_all(&maps_out)?;
    // The maps the pack has, kept (those made again are written over).
    if let Some(pack) = pack_dir(&mods_dir)
        && let Ok(entries) = std::fs::read_dir(pack.join("maps"))
    {
        for e in entries.flatten() {
            if e.path().is_file() {
                std::fs::copy(e.path(), maps_out.join(e.file_name()))?;
            }
        }
    }
    let mut outcome = Outcome::default();
    for (i, (folder, players_own, name)) in maps.iter().enumerate() {
        if ending.stop.load(Ordering::Relaxed) {
            outcome.stopped = true;
            break;
        }
        {
            let mut p = progress.lock().unwrap();
            p.done = i;
            p.current = name.clone();
            p.share = 0.0;
        }
        let Some(made) = convert_one(&files, &templates, &palette, folder, *players_own, progress, ending) else {
            // (Abandoned, the launcher closing: the maps made are installed now.)
            outcome.stopped = true;
            break;
        };
        match made {
            Ok(c) => {
                // (An older version of this map, under another name, goes.)
                remove_made_from(&maps_out, &m_source(folder));
                // (Another map's files of the same name are never written over.)
                if taken(&maps_out, &c.stem) {
                    outcome.failed.push((name.clone(), tf("Another map in the pack has the file name {stem}.", &[("stem", &c.stem)]).to_string()));
                    continue;
                }
                std::fs::write(maps_out.join(format!("{}.tnt", c.stem)), &c.tnt)?;
                std::fs::write(maps_out.join(format!("{}.ota", c.stem)), &c.ota)?;
                if let Some(f) = &c.features {
                    std::fs::write(maps_out.join(format!("{}.features", c.stem)), f)?;
                }
                outcome.imported.push(c.title);
                outcome.made.push(m_source(folder));
            }
            // (The converter's own words, about the map's files, in ours.)
            Err(e) => outcome.failed.push((name.clone(), tf("It could not be made into a TA map: {why}", &[("why", &format!("{e:#}"))]))),
        }
    }
    progress.lock().unwrap().done = outcome.imported.len() + outcome.failed.len();
    if outcome.imported.is_empty() {
        return Ok(outcome);
    }
    let manifest = ModManifest {
        id: PACK_ID.into(),
        name: PACK_NAME.into(),
        version: convert::VERSION.to_string(),
        description: PACK_DESCRIPTION.into(),
        authors: Vec::new(),
        depends: Vec::new(),
        load_after: Vec::new(),
        source: String::new(),
        rename: BTreeMap::new(),
        replaces: Vec::new(), ..Default::default()
    };
    ta_vfs::package::install(&ta_vfs::package::plan(&payload)?, &manifest, &mods_dir)?;
    Ok(outcome)
}

/// Makes the map in `folder` into a TA map on a thread of its own, the import waiting for it:
/// `None` when the import is abandoned meanwhile (the thread is left to the launcher's closing:
/// a large map takes minutes, and it writes nothing).
fn convert_one(files: &Arc<Files>, templates: &Arc<Templates>, palette: &Arc<Palette>, folder: &Path, players_own: bool, progress: &Arc<Mutex<Progress>>, ending: &Ending) -> Option<Result<convert::Converted>> {
    let (send, receive) = mpsc::channel();
    let (files, templates, palette, folder, progress) = (files.clone(), templates.clone(), palette.clone(), folder.to_path_buf(), progress.clone());
    let spawned = std::thread::Builder::new().name("supcom map".into()).spawn(move || {
        let made = Map::read(&files, &folder).map_err(anyhow::Error::from).and_then(|m| {
            Ok(convert::convert(&m, &files, &templates, &palette, players_own, &Options::default(), &|s| {
                if let Stage::Painting(share) = s {
                    progress.lock().unwrap().share = share;
                }
            })?)
        });
        let _ = send.send(made);
    });
    if let Err(e) = spawned {
        return Some(Err(e.into()));
    }
    loop {
        match receive.recv_timeout(std::time::Duration::from_millis(50)) {
            Ok(made) => return Some(made),
            Err(mpsc::RecvTimeoutError::Timeout) if ending.abandon.load(Ordering::Relaxed) => return None,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            // (Its thread ended without a word: it failed.)
            Err(mpsc::RecvTimeoutError::Disconnected) => return Some(Err(anyhow::anyhow!(t("The map's conversion stopped unexpectedly.").to_string()))),
        }
    }
}

/// The source folder's name as a converted map's OTA records it (`supcommap`).
fn m_source(folder: &Path) -> String {
    folder.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default()
}

/// Removes the files of the map made from `source` (its OTA saying so) from `maps`.
fn remove_made_from(maps: &Path, source: &str) {
    let Ok(entries) = std::fs::read_dir(maps) else { return };
    for e in entries.flatten() {
        let path = e.path();
        if !path.extension().is_some_and(|x| x.eq_ignore_ascii_case("ota")) {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let doc = ta_formats::tdf::parse_bytes(&bytes);
        if doc.section("GlobalHeader").and_then(|h| h.get("supcommap")).is_some_and(|s| s.trim().eq_ignore_ascii_case(source)) {
            for ext in ["tnt", "ota", "features"] {
                let _ = std::fs::remove_file(path.with_extension(ext));
            }
        }
    }
}

/// Whether `maps` holds a map of file stem `stem` (whatever its case: the game finds maps so).
fn taken(maps: &Path, stem: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(maps) else { return false };
    entries.flatten().any(|e| {
        let path = e.path();
        path.extension().is_some_and(|x| x.eq_ignore_ascii_case("tnt") || x.eq_ignore_ascii_case("ota"))
            && path.file_stem().is_some_and(|s| s.to_string_lossy().eq_ignore_ascii_case(stem))
    })
}

/// What importing `entries` takes on the disk, about (bytes): a map's art grows with its size.
pub fn size_estimate(entries: &[&Entry]) -> u64 {
    entries.iter().map(|e| match e.scenario.size.0.max(e.scenario.size.1) {
        0..=300 => 17_000_000u64,
        301..=600 => 21_000_000,
        _ => 27_000_000,
    }).sum()
}

/// The size a map is called by in the game (`20 km`).
pub fn km(scenario: &Scenario) -> String {
    let side = scenario.size.0.max(scenario.size.1);
    tf("{n} km", &[("n", &(side / 50))]).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_map_made_again_replaces_its_older_files_only() {
        let dir = std::env::temp_dir().join(format!("ta-supcom-import-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let maps = dir.join("maps");
        std::fs::create_dir_all(&maps).unwrap();
        for (stem, source) in [("Old Name (SC)", "SCMP_001"), ("Other (SC)", "SCMP_002")] {
            std::fs::write(maps.join(format!("{stem}.ota")), format!("[GlobalHeader]{{missionname={stem};supcommap={source};supcomversion=1;}}")).unwrap();
            std::fs::write(maps.join(format!("{stem}.tnt")), b"tnt").unwrap();
        }
        remove_made_from(&maps, "scmp_001");
        assert!(!maps.join("Old Name (SC).ota").exists() && !maps.join("Old Name (SC).tnt").exists());
        assert!(maps.join("Other (SC).ota").exists());
        // Another map's name is taken, whatever its case; the one gone is free.
        assert!(taken(&maps, "other (sc)"));
        assert!(!taken(&maps, "Old Name (SC)"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// A copy of the game's maps listed: its skirmish maps (not its campaign's), in name order,
    /// those the pack holds marked as imported (or as made by an older converter).
    #[test]
    fn the_games_skirmish_maps_are_listed_with_what_the_pack_holds() {
        let dir = std::env::temp_dir().join(format!("ta-supcom-find-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let game = dir.join("Supreme Commander Forged Alliance");
        std::fs::create_dir_all(game.join("gamedata")).unwrap();
        std::fs::write(game.join("gamedata/env.scd"), b"PK").unwrap();
        let scenario = |folder: &str, name: &str, kind: &str| {
            let f = game.join("maps").join(folder);
            std::fs::create_dir_all(&f).unwrap();
            std::fs::write(f.join(format!("{folder}_scenario.lua")), format!("ScenarioInfo = {{ name = '{name}', type = '{kind}', size = {{512, 512}}, Configurations = {{ ['standard'] = {{ teams = {{ {{ name = 'FFA', armies = {{'ARMY_1','ARMY_2'}} }} }} }} }} }}")).unwrap();
        };
        scenario("SCMP_002", "Open Palms", "skirmish");
        scenario("SCMP_001", "Zulu Ridge", "skirmish");
        scenario("SCMP_003", "Beta Fork", "skirmish");
        scenario("X1CA_001", "X1CA_001", "campaign");
        // (A scenario that does not read is said, not passed over in silence; a folder with none
        // is no map.)
        std::fs::create_dir_all(game.join("maps/BROKEN")).unwrap();
        std::fs::write(game.join("maps/BROKEN/BROKEN_scenario.lua"), "ScenarioInfo = {{{").unwrap();
        std::fs::create_dir_all(game.join("maps/notes")).unwrap();
        // The pack where the installer put it, its first name taken: found by its id.
        let mods = dir.join("mods");
        std::fs::create_dir_all(mods.join(PACK_ID)).unwrap();
        std::fs::write(mods.join(PACK_ID).join("mod.toml"), "id = \"someone-elses\"\nname = \"Not ours\"\n").unwrap();
        let pack = mods.join(format!("{PACK_ID}-2"));
        std::fs::create_dir_all(pack.join("maps")).unwrap();
        std::fs::write(pack.join("mod.toml"), format!("id = \"{PACK_ID}\"\nname = \"Supreme Commander maps\"\n")).unwrap();
        std::fs::write(pack.join("maps/Open Palms (SC).ota"), format!("[GlobalHeader]{{supcommap=SCMP_002;supcomversion={};}}", convert::VERSION)).unwrap();
        std::fs::write(pack.join("maps/Beta Fork (SC).ota"), "[GlobalHeader]{supcommap=scmp_003;supcomversion=0;}").unwrap();
        assert_eq!(pack_dir(&mods), Some(pack.clone()));
        let (found, previews) = find(Some(&game), pack_dir(&mods).as_deref());
        let listed: Vec<(&str, State)> = found.maps.iter().map(|e| (e.scenario.name.as_str(), e.state)).collect();
        assert_eq!(listed, [("Beta Fork", State::Outdated), ("Open Palms", State::Imported), ("Zulu Ridge", State::New)]);
        assert_eq!(found.install.as_ref().map(|i| i.root.clone()), Some(game.clone()));
        assert!(found.maps.iter().all(|e| !e.players_own && e.scenario.armies.len() == 2));
        assert!(previews.is_empty(), "no map file, no picture");
        assert_eq!(found.skipped.len(), 1, "{:?}", found.skipped);
        assert!(found.skipped[0].starts_with("BROKEN: "), "{:?}", found.skipped);
        assert!(found.error.is_none());
        // No game there: nothing to list, and said so.
        let (none, _) = find(Some(&dir.join("elsewhere")), Some(&pack));
        assert!(none.maps.is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// A map of the list, for the tests.
    fn entry(folder: &str, players_own: bool, state: State) -> Entry {
        let name = Path::new(folder).file_name().unwrap().to_string_lossy().into_owned();
        Entry { folder: PathBuf::from(folder), players_own, scenario: Scenario { folder: name.clone(), name, ..Default::default() }, state }
    }

    fn chosen(import: &Import) -> Vec<String> {
        import.chosen.iter().flatten().map(|f| f.to_string_lossy().replace('\\', "/")).collect()
    }

    /// At first the game's own maps not imported yet are chosen (not the player's own, perhaps
    /// hundreds); after an import only what was not made stays chosen, and the look that follows
    /// keeps the player's choice, even none at all; another copy of the game chooses afresh.
    #[test]
    fn the_first_choice_is_the_games_maps_and_an_import_leaves_what_was_not_made() {
        use ta_supcom::install::Store;
        let ctx = egui::Context::default();
        let gog = Install { root: PathBuf::from("C:/Game"), store: Store::Gog };
        let found = |install: &Install, states: [State; 3]| Found {
            install: Some(install.clone()),
            maps: vec![entry("C:/Game/maps/SCMP_001", false, states[0]), entry("C:/Game/maps/SCMP_002", false, states[1]), entry("C:/Game/maps/SCMP_003", false, states[2]),
                entry("D:/Own/maps/mine.v0001", true, State::New)],
            ..Default::default()
        };
        let mut import = Import { open: true, ..Default::default() };
        import.pretend_found(found(&gog, [State::New, State::Imported, State::Outdated]), Previews::new());
        import.poll(&ctx);
        assert_eq!(chosen(&import), ["C:/Game/maps/SCMP_001", "C:/Game/maps/SCMP_003"]);
        // The player adds their own map; it and SCMP_001 are made, SCMP_003 is not.
        import.choose(Path::new("D:/Own/maps/mine.v0001"), true);
        import.pretend_importing(&["scmp_001", "mine.v0001"]);
        import.stop();
        let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while import.poll(&ctx).is_none() && std::time::Instant::now() < until {
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(chosen(&import), ["C:/Game/maps/SCMP_003"], "only what was not made stays chosen");
        // The look after the import does not choose every map not imported again.
        import.pretend_found(found(&gog, [State::Imported, State::Imported, State::Outdated]), Previews::new());
        import.poll(&ctx);
        assert_eq!(chosen(&import), ["C:/Game/maps/SCMP_003"]);
        import.choose_all(false);
        import.pretend_found(found(&gog, [State::New, State::New, State::New]), Previews::new());
        import.poll(&ctx);
        assert!(chosen(&import).is_empty(), "none chosen is the player's choice too");
        import.choose_all(true);
        assert_eq!(chosen(&import).len(), 4);
        // Another copy of the game: its own maps not imported yet.
        let steam = Install { root: PathBuf::from("C:/Steam/Game"), store: Store::Steam };
        import.pretend_found(found(&steam, [State::Imported, State::New, State::Imported]), Previews::new());
        import.poll(&ctx);
        assert_eq!(chosen(&import), ["C:/Game/maps/SCMP_002"]);
    }

    /// The game's pictures are made small for the list, a player's map's picture too large to
    /// read left out undecoded; each is uploaded once (its pixels let go of then), and all of them
    /// are let go of when the dialog closes, a look that comes in meanwhile keeping none.
    #[test]
    fn map_pictures_are_bounded_made_small_and_let_go_of_with_the_dialog() {
        let dir = std::env::temp_dir().join(format!("ta-supcom-pictures-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        // A map file holding a picture: an uncompressed DDS (BGRA, 32 bits) or a DXT1 one.
        let map_file = |folder: &str, width: u32, height: u32, dxt1: bool| {
            let mut dds = vec![0u8; 128];
            let put = |dds: &mut Vec<u8>, at: usize, v: u32| dds[at..at + 4].copy_from_slice(&v.to_le_bytes());
            dds[..4].copy_from_slice(b"DDS ");
            put(&mut dds, 4, 124);
            put(&mut dds, 12, height);
            put(&mut dds, 16, width);
            put(&mut dds, 28, 1);
            put(&mut dds, 76, 32);
            if dxt1 {
                put(&mut dds, 80, 0x4);
                dds[84..88].copy_from_slice(b"DXT1");
                dds.resize(128 + (width as usize / 4) * (height as usize / 4) * 8, 0x55);
            } else {
                put(&mut dds, 80, 0x41);
                for (at, mask) in [(88, 32), (92, 0x00ff_0000), (96, 0x0000_ff00), (100, 0x0000_00ff), (104, 0xff00_0000)] {
                    put(&mut dds, at, mask);
                }
                // (Left half white, right half black; the alpha unset, as the game leaves it.)
                for _y in 0..height {
                    for x in 0..width {
                        dds.extend_from_slice(if x < width / 2 { &[255, 255, 255, 0] } else { &[0, 0, 0, 0] });
                    }
                }
            }
            let f = dir.join(folder);
            std::fs::create_dir_all(&f).unwrap();
            let mut file = b"Map\x1a".to_vec();
            file.extend_from_slice(&[0; 26]);
            file.extend_from_slice(&(dds.len() as i32).to_le_bytes());
            file.extend_from_slice(&dds);
            for v in [60i32, 512, 512, 0] {
                file.extend_from_slice(&v.to_le_bytes());
            }
            std::fs::write(f.join(format!("{folder}.scmap")), file).unwrap();
            f
        };
        let game = map_file("SCMP_001", 256, 256, false);
        let preview = preview_of(&game).expect("the game's picture");
        assert_eq!(preview.size, [128, 128]);
        assert_eq!((preview.pixels[0], preview.pixels[127]), (egui::Color32::WHITE, egui::Color32::BLACK), "opaque, halves kept");
        let wide = map_file("wide.v0001", 64, 16, false);
        assert_eq!(preview_of(&wide).unwrap().size, [64, 16], "a small picture is not made larger");
        let huge = map_file("huge.v0001", 4096, 2048, true);
        assert!(preview_of(&huge).is_none(), "a picture far larger than the list's is not decoded");
        // Uploaded once, and its pixels let go of; all let go of when the dialog closes.
        let ctx = egui::Context::default();
        let mut import = Import { open: true, ..Default::default() };
        let previews: Previews = [(game.clone(), preview.clone()), (wide.clone(), preview.clone())].into();
        import.pretend_found(Found { maps: vec![entry("SCMP_001", false, State::New)], ..Default::default() }, previews.clone());
        import.poll(&ctx);
        assert_eq!(import.pictures_held(), (2, 0));
        let texture = import.texture(&ctx, &game).expect("its texture");
        assert_eq!(import.texture(&ctx, &game).map(|t| t.id()), Some(texture.id()), "made once");
        assert_eq!(import.pictures_held(), (1, 1));
        import.open = false;
        import.poll(&ctx);
        assert_eq!(import.pictures_held(), (0, 0), "closed, the dialog holds no picture");
        import.pretend_found(Found::default(), previews);
        import.poll(&ctx);
        assert_eq!(import.pictures_held(), (0, 0), "nor from a look that came in meanwhile");
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// An import is told as it went: every map made, some not (amber, not green), none made or
    /// the import failed (red), a stop before any map was made (no notice).
    #[test]
    fn an_import_is_told_as_it_went() {
        let failed = || vec![("Seton's Clutch".to_string(), "It could not be made into a TA map: bad".to_string())];
        let well = Outcome { imported: vec!["Open Palms".into()], ..Default::default() };
        let partly = Outcome { imported: vec!["Open Palms".into()], failed: failed(), ..Default::default() };
        let badly = Outcome { failed: failed(), ..Default::default() };
        let broken = Outcome { error: Some("disk full".into()), ..Default::default() };
        let nothing = Outcome { stopped: true, ..Default::default() };
        assert_eq!([&well, &partly, &badly, &broken, &nothing].map(Outcome::went), [Went::Well, Went::Partly, Went::Badly, Went::Badly, Went::Nothing]);
        assert_eq!(partly.went().colour(), crate::theme::AMBER);
        assert!(partly.notice().unwrap().contains("but 1 could not be made"));
        assert!(badly.notice().unwrap().starts_with("None of the 1 Supreme Commander maps"));
        assert!(broken.notice().unwrap().ends_with("disk full") && broken.summary().ends_with("disk full"));
        assert!(nothing.notice().is_none());
        assert!(well.notice().unwrap().starts_with("Imported 1 Supreme Commander maps"));
    }

    /// With GOG's and Steam's copies both found: the first in use unless one is picked, a copy
    /// picked keeping its store's name, any other folder picked used as a folder.
    #[test]
    fn the_copy_in_use_is_the_one_picked_with_its_stores_name() {
        use ta_supcom::install::Store;
        let gog = Install { root: PathBuf::from("C:/GOG Games/Supreme Commander Forged Alliance"), store: Store::Gog };
        let steam = Install { root: PathBuf::from("C:/Program Files (x86)/Steam/steamapps/common/Supreme Commander Forged Alliance"), store: Store::Steam };
        let copies = [gog.clone(), steam.clone()];
        assert_eq!(in_use(&copies, None), Some(gog));
        assert_eq!(in_use(&copies, Some(&steam.root)), Some(steam));
        let elsewhere = PathBuf::from("D:/Games/Forged Alliance");
        assert_eq!(in_use(&copies, Some(&elsewhere)), Some(Install { root: elsewhere, store: Store::Folder }));
        assert_eq!(in_use(&[], None), None);
    }

    #[test]
    fn sizes_are_told_as_the_game_tells_them() {
        let s = Scenario { size: (1024, 1024), ..Default::default() };
        assert_eq!(km(&s), "20 km");
        let e = Entry { folder: PathBuf::new(), players_own: false, scenario: s, state: State::New };
        assert_eq!(size_estimate(&[&e, &e]), 54_000_000);
    }
}
