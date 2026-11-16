//! Supreme Commander: Forged Alliance's four factions, imported (`ta_supcom::factions`): the
//! playable units of the copy of the game in use counted by faction, and those of the factions
//! chosen made into TA units on a worker, into one mod (`supcom-factions`), which then plays in
//! skirmish, a side each. As with the maps (`crate::supcom`), nothing of the game's is shipped:
//! each player's units are made from their own copy, the same on every computer. The mod is made
//! whole each time, of the factions chosen: a stop, or a failure, leaves the one installed as it
//! was.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::JoinHandle;

use anyhow::{Context, Result, bail};
use ta_supcom::factions::{faction, make, pictures, units as made_units};
use ta_supcom::files::Files;
use ta_supcom::install::{self, Install};
use ta_supcom::unit::{self, Faction, Motion, UnitBlueprint};
use ta_ui::i18n::{n, t, tf};

use crate::supcom::{State, Went};

pub use make::MOD_ID;

/// The mod's name and words as its manifest keeps them (`make`'s), marked to be translated: the
/// interface shows them in the language in use ([`mod_name`]; the Mods page translates a
/// description).
const MOD_NAME: &str = n("Supreme Commander factions");
const MOD_DESCRIPTION: &str = n("Supreme Commander: Forged Alliance's four factions, their units and buildings, made from your own copy of the game.");
/// What a unit takes on the disk once made, about (its paint the most): 402 MB for the game's 348.
const UNIT_BYTES: u64 = 1_160_000;

/// The mod's name as the interface shows it, in the language in use.
pub fn mod_name() -> &'static str {
    t(MOD_NAME)
}

/// A faction of the copy of the game in use: its playable units by kind, and whether the mod
/// installed holds it.
#[derive(Clone, Debug)]
pub struct Entry {
    pub faction: Faction,
    pub buildings: usize,
    pub land: usize,
    pub air: usize,
    pub naval: usize,
    pub state: State,
}

impl Entry {
    pub fn units(&self) -> usize {
        self.buildings + self.land + self.air + self.naval
    }

    /// Its side's name, as the game's sides are named (UEF, CYBRAN, AEON, SERAPHIM): what it is
    /// chosen by.
    pub fn side(&self) -> &'static str {
        made_units::side(&self.faction)
    }
}

/// What a look for the game's units found.
#[derive(Clone, Default)]
pub struct Found {
    /// The copy of the game in use: the player's choice, else the first found.
    pub install: Option<Install>,
    /// Every copy of the game found (GOG's, Steam's...), in the order looked for.
    pub copies: Vec<Install>,
    pub factions: Vec<Entry>,
    /// Why the game's units could not be listed.
    pub error: Option<String>,
}

/// The factions' commanders' build pictures (as the game will show them), by side.
pub type Pictures = BTreeMap<&'static str, egui::ColorImage>;

/// How an import is going.
#[derive(Clone, Default)]
pub struct Progress {
    pub done: usize,
    pub total: usize,
    /// The unit being made (its faction's side and its name).
    pub current: String,
    /// Every unit made: the mod is being installed (that cannot be stopped).
    pub installing: bool,
}

/// What an import came to.
#[derive(Clone, Debug, Default)]
pub struct Outcome {
    /// The factions installed, by side.
    pub factions: Vec<String>,
    pub units: usize,
    /// Units that could not be made, and why.
    pub skipped: Vec<(String, String)>,
    pub stopped: bool,
    /// Why nothing was installed, when it failed.
    pub error: Option<String>,
}

impl Outcome {
    pub fn went(&self) -> Went {
        if self.error.is_some() {
            Went::Badly
        } else if self.stopped || self.factions.is_empty() {
            Went::Nothing
        } else if self.skipped.is_empty() {
            Went::Well
        } else {
            Went::Partly
        }
    }

    /// What it came to, as the dialog says it.
    pub fn summary(&self) -> String {
        let names = self.factions.join(", ");
        match (&self.error, self.went()) {
            (Some(why), _) => tf("The factions could not be imported: {why}", &[("why", why)]),
            (None, Went::Nothing) => t("Stopped: nothing was installed, and the factions are as they were.").to_string(),
            (None, Went::Well) => tf("Imported {factions}, {units} units in all: each is a side to play in skirmish.", &[("factions", &names), ("units", &self.units)]),
            (None, _) => tf("Imported {factions}, {units} units in all, but {failed} could not be made: each faction is a side to play in skirmish.", &[("factions", &names), ("units", &self.units), ("failed", &self.skipped.len())]),
        }
    }

    /// What a notice says of it with the dialog closed (nothing for a stop).
    pub fn notice(&self) -> Option<String> {
        let names = self.factions.join(", ");
        Some(match (&self.error, self.went()) {
            (Some(why), _) => tf("The Supreme Commander factions could not be imported: {why}", &[("why", why)]),
            (None, Went::Nothing) => return None,
            (None, _) => tf("Imported the Supreme Commander factions ({factions}): each is a side to play in skirmish.", &[("factions", &names)]),
        })
    }
}

struct Job {
    handle: JoinHandle<Outcome>,
    stop: Arc<AtomicBool>,
}

#[derive(Default)]
pub struct Factions {
    scan: Option<mpsc::Receiver<(Found, Pictures)>>,
    /// What the last look found, shared: the dialog draws from it every frame.
    pub found: Option<Arc<Found>>,
    /// The factions chosen, by side. None until the game's units are first listed, when those the
    /// mod holds are chosen, or every faction where it holds none ([`first_choice`]); after that
    /// what the player chose.
    pub chosen: Option<BTreeSet<String>>,
    job: Option<Job>,
    pub progress: Arc<Mutex<Progress>>,
    pub outcome: Option<Outcome>,
    /// The commanders' pictures not shown yet, and those shown, as textures, by side.
    pictures: Pictures,
    textures: BTreeMap<&'static str, egui::TextureHandle>,
    /// Factions to choose and import as soon as the game's units are listed (sides, or `all`): a
    /// capture's (`--show supcom-factions:uef,cybran`).
    pub auto_start: Option<Vec<String>>,
}

impl Factions {
    pub fn scanning(&self) -> bool {
        self.scan.is_some()
    }

    pub fn importing(&self) -> bool {
        self.job.is_some()
    }

    /// Looks for the game and its units (in the player's chosen folder `picked`, else wherever a
    /// copy is), and for the mod in `mods_dir`.
    pub fn rescan(&mut self, picked: Option<PathBuf>, mods_dir: &Path) {
        let (send, receive) = mpsc::channel();
        let mods_dir = mods_dir.to_path_buf();
        std::thread::Builder::new().name("supcom factions scan".into()).spawn(move || {
            let _ = send.send(find(picked.as_deref(), &mods_dir));
        }).ok();
        self.scan = Some(receive);
    }

    /// Whether the faction of side `side` is chosen.
    pub fn is_chosen(&self, side: &str) -> bool {
        self.chosen.as_ref().is_some_and(|chosen| chosen.contains(side))
    }

    /// Chooses the faction of side `side` (`on`), or not.
    pub fn choose(&mut self, side: &str, on: bool) {
        let chosen = self.chosen.get_or_insert_with(BTreeSet::new);
        if on {
            chosen.insert(side.to_string());
        } else {
            chosen.remove(side);
        }
    }

    /// The factions chosen, as listed.
    pub fn chosen_entries(&self) -> Vec<Entry> {
        self.found.iter().flat_map(|found| found.factions.iter().filter(|e| self.is_chosen(e.side())).cloned()).collect()
    }

    /// The commander's picture of the faction of side `side` as a texture, made from its pixels
    /// the first time it is shown.
    pub fn texture(&mut self, ctx: &egui::Context, side: &'static str) -> Option<egui::TextureHandle> {
        if let Some(texture) = self.textures.get(side) {
            return Some(texture.clone());
        }
        let image = self.pictures.remove(side)?;
        let texture = ctx.load_texture(format!("supcom-faction:{side}"), image, egui::TextureOptions::NEAREST);
        self.textures.insert(side, texture.clone());
        Some(texture)
    }

    /// Starts making the factions chosen into the mod, the launcher's settings in `store_dir`.
    pub fn start(&mut self, store_dir: PathBuf) {
        let Some(found) = self.found.clone() else { return };
        let Some(game) = found.install.clone() else { return };
        if self.importing() {
            return;
        }
        let sides: BTreeSet<String> = found.factions.iter().map(|e| e.side().to_string()).filter(|s| self.is_chosen(s)).collect();
        if sides.is_empty() {
            return;
        }
        let stop = Arc::new(AtomicBool::new(false));
        *self.progress.lock().unwrap() = Progress::default();
        let (progress, asked) = (self.progress.clone(), stop.clone());
        self.outcome = None;
        let handle = std::thread::Builder::new().name("supcom factions import".into()).spawn(move || {
            import(&game, &sides, &store_dir, &progress, &asked).unwrap_or_else(|e| Outcome { error: Some(format!("{e:#}")), ..Default::default() })
        });
        match handle {
            Ok(handle) => self.job = Some(Job { handle, stop }),
            Err(e) => self.outcome = Some(Outcome { error: Some(e.to_string()), ..Default::default() }),
        }
    }

    /// The factions of `auto_start`, chosen, once the game's units are listed: true when they are
    /// to be imported now.
    pub fn take_auto_start(&mut self) -> bool {
        if self.scanning() || self.importing() {
            return false;
        }
        let (Some(names), Some(found)) = (&self.auto_start, &self.found) else { return false };
        let all = names.iter().any(|n| n.eq_ignore_ascii_case("all"));
        let chosen: BTreeSet<String> = found.factions.iter().map(|e| e.side()).filter(|s| all || names.iter().any(|n| n.eq_ignore_ascii_case(s))).map(str::to_string).collect();
        let any = !chosen.is_empty();
        self.chosen = Some(chosen);
        self.auto_start = None;
        any
    }

    /// Whether a capture may take its picture: the units looked for, a capture's factions
    /// imported (or, with `TA_SUPCOM_SHOT_IMPORTING=1`, while they are).
    pub fn capture_ready(&self) -> bool {
        let during = std::env::var_os("TA_SUPCOM_SHOT_IMPORTING").is_some_and(|v| v == "1");
        !self.scanning() && self.auto_start.is_none() && (during || !self.importing())
    }

    /// Stops the import at the next unit: nothing is installed. (Once installing, it finishes.)
    pub fn stop(&self) {
        if let Some(job) = &self.job {
            job.stop.store(true, Ordering::Relaxed);
        }
    }

    /// Takes in what the workers found and made; `Some` when an import has ended (the mod
    /// installed, when it holds factions).
    pub fn poll(&mut self, ctx: &egui::Context, open: bool) -> Option<Outcome> {
        if let Some(scan) = &self.scan {
            match scan.try_recv() {
                Ok((found, pictures)) => {
                    let same_copy = self.found.as_ref().is_some_and(|f| f.install == found.install);
                    match &mut self.chosen {
                        Some(chosen) if same_copy => chosen.retain(|c| found.factions.iter().any(|e| e.side() == c)),
                        _ => self.chosen = Some(first_choice(&found)),
                    }
                    self.textures.clear();
                    self.pictures = if open { pictures } else { Pictures::new() };
                    self.found = Some(Arc::new(found));
                    self.scan = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => self.scan = None,
                Err(mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(100)),
            }
        }
        if !open && (!self.pictures.is_empty() || !self.textures.is_empty()) {
            self.pictures.clear();
            self.textures.clear();
        }
        if self.job.as_ref().is_some_and(|j| j.handle.is_finished()) {
            let job = self.job.take().unwrap();
            let outcome = job.handle.join().unwrap_or_else(|_| Outcome { error: Some(t("The import stopped unexpectedly.").to_string()), ..Default::default() });
            self.outcome = Some(outcome.clone());
            return Some(outcome);
        }
        if self.job.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        None
    }

    /// Stops an import under way at shutdown and waits for it (a unit's making, or the install
    /// once begun): its outcome.
    pub fn finish_on_exit(&mut self) -> Option<Outcome> {
        self.stop();
        let job = self.job.take()?;
        job.handle.join().ok()
    }

    /// An import under way that ends, stopped, once asked to stop: for tests.
    #[cfg(test)]
    pub fn pretend_importing(&mut self) {
        let stop = Arc::new(AtomicBool::new(false));
        let asked = stop.clone();
        let handle = std::thread::spawn(move || {
            while !asked.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            Outcome { stopped: true, ..Default::default() }
        });
        self.job = Some(Job { handle, stop });
    }

    /// A look that found `found`, taken in at the next [`Factions::poll`]: for tests.
    #[cfg(test)]
    pub fn pretend_found(&mut self, found: Found, pictures: Pictures) {
        let (send, receive) = mpsc::channel();
        send.send((found, pictures)).unwrap();
        self.scan = Some(receive);
    }
}

/// The factions chosen for the player when the game's units are first listed: those the mod
/// holds (made again, it keeps them), or every one where it holds none.
fn first_choice(found: &Found) -> BTreeSet<String> {
    let held: BTreeSet<String> = found.factions.iter().filter(|e| e.state != State::New).map(|e| e.side().to_string()).collect();
    if held.is_empty() { found.factions.iter().map(|e| e.side().to_string()).collect() } else { held }
}

/// What the factions chosen take on the disk once made, about (bytes).
pub fn size_estimate(entries: &[Entry]) -> u64 {
    entries.iter().map(|e| e.units() as u64 * UNIT_BYTES).sum()
}

/// How a unit is counted: a building, or by how it moves.
fn kind_of(u: &UnitBlueprint) -> usize {
    match u.motion {
        _ if u.is("STRUCTURE") => 0,
        Motion::None => 0,
        Motion::Air => 2,
        Motion::Water | Motion::Sub => 3,
        Motion::Land | Motion::Amphibious | Motion::Hover => 1,
    }
}

/// The converter's version that made the mod in `dir`, and the sides it holds.
fn installed_state(dir: &Path) -> (u32, BTreeSet<String>) {
    let version = ta_vfs::mods::read_mod(dir).ok().flatten().and_then(|m| m.manifest.version.trim().parse().ok()).unwrap_or(0);
    let sides = std::fs::read(dir.join("gamedata").join("sidedata_supcom.tdf")).map(|bytes| {
        let doc = ta_formats::tdf::parse_bytes(&bytes);
        doc.sections.iter().filter(|s| s.name.to_ascii_lowercase().starts_with("side")).filter_map(|s| s.get("name")).map(|n| n.trim().to_ascii_uppercase()).collect()
    }).unwrap_or_default();
    (version, sides)
}

/// The game, its playable units counted by faction (with what the mod in `mods_dir` holds), and
/// each faction's commander's build picture.
fn find(picked: Option<&Path>, mods_dir: &Path) -> (Found, Pictures) {
    let copies = install::discover();
    let install = crate::supcom::in_use(&copies, picked);
    let mut found = Found { install: install.clone(), copies, ..Default::default() };
    let mut pictures = Pictures::new();
    let Some(game) = install else { return (found, pictures) };
    let (version, held) = crate::supcom::mod_dir(mods_dir, MOD_ID).as_deref().map(installed_state).unwrap_or_default();
    let units = match Files::own(&game).and_then(|files| Ok((unit::catalogue(&files)?, files))) {
        Ok(units) => units,
        Err(e) => {
            found.error = Some(tf("The game's units could not be read: {why}", &[("why", &e)]));
            return (found, pictures);
        }
    };
    let (units, files) = units;
    for f in Faction::PLAYABLE {
        let theirs: Vec<&UnitBlueprint> = units.iter().filter(|u| u.playable() && u.faction == f).collect();
        if theirs.is_empty() {
            continue;
        }
        let mut counts = [0usize; 4];
        for u in &theirs {
            counts[kind_of(u)] += 1;
        }
        let side = made_units::side(&f);
        let state = if !held.contains(side) { State::New } else if version >= make::VERSION { State::Imported } else { State::Outdated };
        if let Some(commander) = faction::commander(&theirs)
            && let Some(picture) = picture_of(&files, commander)
        {
            pictures.insert(side, picture);
        }
        found.factions.push(Entry { faction: f, buildings: counts[0], land: counts[1], air: counts[2], naval: counts[3], state });
    }
    if found.factions.is_empty() {
        found.error = Some(t("No units of the four factions were found in this copy of the game.").to_string());
    }
    (found, pictures)
}

/// A unit's build picture as the game will show it (the converter's own).
fn picture_of(files: &Files, u: &UnitBlueprint) -> Option<egui::ColorImage> {
    let icon = files.read(&pictures::icon_path(u)).ok()??;
    let frame = pictures::frame_path(u).and_then(|p| files.read(&p).ok().flatten());
    let pcx = ta_formats::pcx::PcxImage::parse(&pictures::picture(&icon, frame.as_deref()).ok()?).ok()?;
    Some(egui::ColorImage::from_rgba_unmultiplied([pcx.width as usize, pcx.height as usize], &pcx.to_rgba()))
}

/// What a unit is called while it is made: its side, and its name in the game.
fn called(u: &UnitBlueprint) -> String {
    let name = [&u.name, &u.description].into_iter().find(|n| !n.trim().is_empty()).map_or(u.id.as_str(), |n| n.as_str());
    format!("{} {}", made_units::side(&u.faction), unit::strip_loc(name))
}

/// Makes the factions of `sides` of the copy `game` into the mod and installs it in the
/// launcher's mods folder (`store_dir`'s), in place of the one there: nothing is installed when
/// stopped (`stop`) before every unit is made.
fn import(game: &Install, sides: &BTreeSet<String>, store_dir: &Path, progress: &Arc<Mutex<Progress>>, stop: &AtomicBool) -> Result<Outcome> {
    let files = Files::own(game).context(t("Supreme Commander: Forged Alliance's units could not be read."))?;
    let units: Vec<UnitBlueprint> = unit::catalogue(&files)?.into_iter().filter(|u| u.playable() && sides.contains(made_units::side(&u.faction))).collect();
    if units.is_empty() {
        bail!("{}", t("None of the factions chosen has units in this copy of the game."));
    }
    let names: BTreeMap<String, String> = units.iter().map(|u| (u.id.clone(), called(u))).collect();
    progress.lock().unwrap().total = units.len();
    let work = crate::maps::Workspace::new(&store_dir.join("cache").join("faction-imports"))?;
    let payload = work.0.join("payload");
    let made = make::make(&files, &units, env!("CARGO_PKG_VERSION"), &mut |path, bytes| write_into(&payload, path, bytes), &mut |done, id| {
        let mut p = progress.lock().unwrap();
        p.done = done;
        p.current = names.get(id).cloned().unwrap_or_else(|| id.to_string());
        !stop.load(Ordering::Relaxed)
    })?;
    if made.stopped {
        return Ok(Outcome { stopped: true, ..Default::default() });
    }
    if made.units == 0 {
        bail!("{}", t("None of the units could be made."));
    }
    {
        let mut p = progress.lock().unwrap();
        p.done = units.len();
        p.installing = true;
    }
    install_made(&payload, &store_dir.join("mods"))?;
    let factions: Vec<String> = Faction::PLAYABLE.iter().map(made_units::side).filter(|s| sides.contains(*s)).map(str::to_string).collect();
    Ok(Outcome { factions, units: made.units, skipped: made.skipped, ..Default::default() })
}

/// Writes a file the converter made into the mod's folder being made.
fn write_into(payload: &Path, path: &str, bytes: &[u8]) -> ta_supcom::Result<()> {
    let at = payload.join(path);
    if let Some(dir) = at.parent() {
        std::fs::create_dir_all(dir).map_err(|e| ta_supcom::Error::Format(format!("{}: {e}", dir.display())))?;
    }
    std::fs::write(&at, bytes).map_err(|e| ta_supcom::Error::Format(format!("{}: {e}", at.display())))
}

/// Installs the mod made in `payload` (its own manifest, `make`'s) in `mods_dir`, in place of the
/// one installed: its folder.
fn install_made(payload: &Path, mods_dir: &Path) -> Result<PathBuf> {
    let manifest = ta_vfs::mods::read_mod(payload)?.context(t("The mod made has no manifest."))?.manifest;
    Ok(ta_vfs::package::install(&ta_vfs::package::plan(payload)?, &manifest, mods_dir)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ta-supcom-factions-{name}-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The words the interface translates are the converter's own.
    #[test]
    fn the_mods_words_are_the_converters() {
        assert_eq!(MOD_NAME, make::MOD_NAME);
        assert_eq!(MOD_DESCRIPTION, make::DESCRIPTION);
    }

    /// A mod made is installed under its own manifest, in place of the one there: what the new
    /// one does not make is gone; the factions it holds and the converter that made it are read
    /// back from it.
    #[test]
    fn a_mod_made_replaces_the_one_installed() {
        let dir = temp("install");
        let mods = dir.join("mods");
        let payload = |name: &str, sides: &[&str]| {
            let p = dir.join(name);
            for (path, bytes) in [("mod.toml", make::manifest("1.0.7")), ("units/uel0201.fbi", "[UNITINFO]{UnitName=UEL0201;}".to_string())] {
                write_into(&p, path, bytes.as_bytes()).unwrap();
            }
            let add_on: String = sides.iter().enumerate().map(|(i, s)| format!("[SIDE{i}]{{name={s};commander={s}0001;}}")).collect();
            write_into(&p, "gamedata/sidedata_supcom.tdf", add_on.as_bytes()).unwrap();
            p
        };
        let first = payload("first", &["UEF", "CYBRAN"]);
        write_into(&first, "units/url0107.fbi", b"[UNITINFO]{UnitName=URL0107;}").unwrap();
        let installed = install_made(&first, &mods).unwrap();
        assert_eq!(crate::supcom::mod_dir(&mods, MOD_ID), Some(installed.clone()));
        let info = ta_vfs::mods::read_mod(&installed).unwrap().unwrap();
        assert_eq!((info.manifest.id.as_str(), info.manifest.version.as_str(), info.manifest.requires_reborn.as_str()), (MOD_ID, "1", "1.0.7"));
        assert_eq!(installed_state(&installed), (make::VERSION, ["CYBRAN".to_string(), "UEF".to_string()].into()));
        // Made again of the UEF alone: the Cybran's unit is gone.
        let again = install_made(&payload("second", &["UEF"]), &mods).unwrap();
        assert_eq!(again, installed);
        assert!(installed.join("units/uel0201.fbi").exists() && !installed.join("units/url0107.fbi").exists());
        assert_eq!(installed_state(&installed).1, ["UEF".to_string()].into());
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn entry(faction: Faction, state: State) -> Entry {
        Entry { faction, buildings: 47, land: 20, air: 12, naval: 10, state }
    }

    /// At first the factions the mod holds are chosen (made again, it keeps them), or all four
    /// where it holds none; a look after keeps the player's choice.
    #[test]
    fn the_first_choice_is_what_the_mod_holds_or_every_faction() {
        let ctx = egui::Context::default();
        let found = |states: [State; 4]| Found {
            install: Some(Install { root: PathBuf::from("C:/Game"), store: ta_supcom::install::Store::Gog }),
            factions: Faction::PLAYABLE.into_iter().zip(states).map(|(f, s)| entry(f, s)).collect(),
            ..Default::default()
        };
        let mut factions = Factions::default();
        factions.pretend_found(found([State::New; 4]), Pictures::new());
        factions.poll(&ctx, true);
        assert_eq!(factions.chosen_entries().len(), 4);
        let mut held = Factions::default();
        held.pretend_found(found([State::Imported, State::New, State::Outdated, State::New]), Pictures::new());
        held.poll(&ctx, true);
        assert_eq!(held.chosen_entries().iter().map(Entry::side).collect::<Vec<_>>(), ["UEF", "AEON"]);
        held.choose("SERAPHIM", true);
        held.choose("UEF", false);
        held.pretend_found(found([State::Imported; 4]), Pictures::new());
        held.poll(&ctx, true);
        assert_eq!(held.chosen_entries().iter().map(Entry::side).collect::<Vec<_>>(), ["AEON", "SERAPHIM"], "the player's choice is kept");
        assert_eq!(size_estimate(&held.chosen_entries()), 2 * 89 * UNIT_BYTES);
        // A capture's factions, chosen once listed.
        held.auto_start = Some(vec!["cybran".into()]);
        assert!(held.take_auto_start());
        assert_eq!(held.chosen_entries().iter().map(Entry::side).collect::<Vec<_>>(), ["CYBRAN"]);
    }

    /// An import is told as it went: every unit made, some not (amber), failed (red), stopped (no
    /// notice).
    #[test]
    fn an_import_is_told_as_it_went() {
        let well = Outcome { factions: vec!["UEF".into(), "AEON".into()], units: 180, ..Default::default() };
        let partly = Outcome { skipped: vec![("UEL0401".into(), "no mesh".into())], ..well.clone() };
        let broken = Outcome { error: Some("disk full".into()), ..Default::default() };
        let stopped = Outcome { stopped: true, ..Default::default() };
        assert_eq!([&well, &partly, &broken, &stopped].map(Outcome::went), [Went::Well, Went::Partly, Went::Badly, Went::Nothing]);
        assert!(well.summary().starts_with("Imported UEF, AEON, 180 units in all"), "{}", well.summary());
        assert!(partly.summary().contains("but 1 could not be made"), "{}", partly.summary());
        assert!(broken.notice().unwrap().ends_with("disk full"));
        assert!(stopped.notice().is_none());
    }
}
