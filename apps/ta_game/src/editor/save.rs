//! Saving: every map is written as a mod of its own in the user's mods folder
//! (`%APPDATA%\TA Reborn\mods\<map name>\`: `mod.toml`, `maps\<name>.tnt`,
//! `maps\<name>.ota`), never into the TA install. The mod is enabled in the settings, and
//! the running game mounts it at once so the skirmish page lists the map.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, bail, ensure};
use ta_formats::image::Palette;
use ta_vfs::Vfs;

use super::doc::MapDoc;

/// Where a map was saved.
#[derive(Clone, Debug)]
pub struct Saved {
    /// The mod's folder.
    pub dir: PathBuf,
    /// The mod's id (in `mod.toml` and the settings' enabled mods).
    pub id: String,
    /// The map's file stem (`maps/<stem>.tnt`), the name the game lists it by.
    pub stem: String,
}

/// The user's mods folder (the launcher's "User folder").
pub fn user_mods_dir() -> PathBuf {
    ta_config::config_dir().join("mods")
}

/// A file stem for a map name: no characters Windows forbids in file names, no leading or
/// trailing dots and spaces, at most 64 characters.
pub fn file_stem(name: &str) -> String {
    let cleaned: String = name.chars().filter(|&c| !c.is_control() && !"<>:\"/\\|?*".contains(c)).take(64).collect();
    cleaned.trim_matches(|c: char| c == '.' || c.is_whitespace()).to_string()
}

/// Stable across saves and distinct for names that share an ASCII slug (including
/// entirely non-Latin names). Existing editor folders retain their previous IDs.
pub fn mod_id(stem: &str) -> String {
    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(stem.to_lowercase().as_bytes());
    let suffix: String = hash[..8].iter().map(|byte| format!("{byte:02x}")).collect();
    format!("{}-{suffix}", legacy_mod_id(stem))
}

fn legacy_mod_id(stem: &str) -> String {
    let mut id = String::from("map-");
    for c in stem.chars() {
        if c.is_ascii_alphanumeric() {
            id.push(c.to_ascii_lowercase());
        } else if !id.ends_with('-') {
            id.push('-');
        }
    }
    id.trim_end_matches('-').to_string()
}

const EDITOR_HEADER: &str = "# Written by the TA Reborn map editor.";

fn is_link(meta: &std::fs::Metadata) -> bool {
    #[cfg(windows)] { use std::os::windows::fs::MetadataExt; meta.file_attributes() & 0x400 != 0 }
    #[cfg(not(windows))] { meta.file_type().is_symlink() }
}

/// Whether `dir` holds a map the editor saved as `stem` (its manifest written by the editor).
pub fn editor_map(dir: &Path, stem: &str) -> bool {
    existing_id(dir, stem).ok().flatten().is_some()
}

/// Refuse unrelated content, even when its mod is disabled and absent from the VFS.
fn existing_id(dir: &Path, stem: &str) -> Result<Option<String>, NotSaveable> {
    Ok(existing_manifest(dir, stem)?.map(|existing| existing.manifest.id))
}

/// The editor's earlier save of a map: its manifest as the game reads it, and as written.
struct Existing {
    manifest: ta_vfs::mods::ModManifest,
    text: String,
}

/// Why the editor does not save a map into a folder.
#[derive(Debug)]
enum NotSaveable {
    /// The folder is anything else's: another mod, a file, a link...
    Other(anyhow::Error),
    /// The editor's own earlier save, its `mod.toml` (`file`) edited by hand so that it does not
    /// read (`error`): said as that, for the author to put right, never taken for another's.
    Unreadable { file: PathBuf, error: String },
}

impl From<NotSaveable> for anyhow::Error {
    fn from(why: NotSaveable) -> anyhow::Error {
        match why {
            NotSaveable::Other(error) => error,
            NotSaveable::Unreadable { file, error } => anyhow::anyhow!("{} does not read: {}", file.display(), error.trim()),
        }
    }
}

/// The editor's earlier save of the map `stem` in `dir` (None: never saved); refused when `dir`
/// is anything else's, or when its manifest does not read.
fn existing_manifest(dir: &Path, stem: &str) -> Result<Option<Existing>, NotSaveable> {
    let other = |error: anyhow::Error| NotSaveable::Other(error);
    let meta = match std::fs::symlink_metadata(dir) {
        Ok(meta) => meta,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(other(error.into())),
    };
    if !meta.is_dir() || is_link(&meta) {
        return Err(other(anyhow::anyhow!("{} is not an ordinary map directory", dir.display())));
    }
    let manifest = dir.join("mod.toml");
    let meta = std::fs::symlink_metadata(&manifest).map_err(|_| other(anyhow::anyhow!("the destination is not an editor map; choose another map name")))?;
    if !meta.is_file() || is_link(&meta) || meta.len() > 1 << 20 {
        return Err(other(anyhow::anyhow!("the destination has no ordinary editor manifest")));
    }
    use std::io::Read;
    let mut text = String::new();
    std::fs::File::open(&manifest).and_then(|file| file.take((1 << 20) + 1).read_to_string(&mut text))
        .map_err(|error| NotSaveable::Unreadable { file: manifest.clone(), error: error.to_string() })?;
    if text.len() > 1 << 20 || !text.starts_with(EDITOR_HEADER) {
        return Err(other(anyhow::anyhow!("the destination belongs to another mod; choose another map name")));
    }
    // (Read as the game reads it: what only presents the map, written by hand in another form than
    // the editor's, reads all the same.)
    let read: ta_vfs::mods::ModManifest = toml::from_str(&text).map_err(|error| NotSaveable::Unreadable { file: manifest.clone(), error: error.to_string() })?;
    if read.id != legacy_mod_id(stem) && read.id != mod_id(stem) {
        return Err(other(anyhow::anyhow!("the destination belongs to a different editor map")));
    }
    Ok(Some(Existing { manifest: read, text }))
}

/// The description the editor writes for the map `name` (one written otherwise is the author's,
/// kept).
fn editor_description(name: &str) -> String {
    format!("The map {}, made with the TA Reborn map editor.", name.trim())
}

/// Whether a description is one the editor wrote (for whatever name the map had then).
fn is_editor_description(text: &str) -> bool {
    text.starts_with("The map ") && text.ends_with(", made with the TA Reborn map editor.")
}

/// Why a map can't be saved under `name`: an empty name, or another map of that name (the
/// base game's, or another mod's) that ours would hide. The user's own earlier save of the
/// same name is replaced.
pub fn name_problem(vfs: &Vfs, name: &str, mods_dir: &Path) -> Option<String> {
    use ta_ui::i18n::{t, tf};
    let stem = file_stem(name);
    if stem.is_empty() {
        return Some(t("Give the map a name first (Properties).").into());
    }
    if !ta_vfs::package::safe_component(&stem) {
        return Some(t("This map name is reserved by the operating system; choose another name.").into());
    }
    if crate::setup::campaign_stem(&stem) {
        return Some(tf("Names like \"{name}\" belong to the campaigns' maps, which Skirmish doesn't list; choose another name.", &[("name", &stem)]));
    }
    match existing_id(&mods_dir.join(&stem), &stem) {
        // (The map's own, its manifest edited so that it does not read: said as that.)
        Err(NotSaveable::Unreadable { file, error }) => {
            return Some(tf("The manifest {file} does not read: {error}", &[("file", &ta_vfs::mods::shown_path(&file)), ("error", &error.trim())]));
        }
        Err(NotSaveable::Other(error)) => {
            log::warn!("map editor: {}: {error:#}", mods_dir.join(&stem).display());
            return Some(tf("Your mods folder already has a \"{name}\" the editor did not save; choose another name.", &[("name", &stem)]));
        }
        Ok(_) => {}
    }
    let path = format!("maps/{stem}.tnt");
    let origin = vfs.origin(&path)?;
    let ours = mods_dir.join(&stem);
    let same = |a: &Path, b: &Path| a.to_string_lossy().to_lowercase().trim_end_matches(['\\', '/']) == b.to_string_lossy().to_lowercase().trim_end_matches(['\\', '/']);
    if std::fs::canonicalize(&origin.path).ok().zip(std::fs::canonicalize(&ours).ok()).is_some_and(|(a, b)| a == b)
        || same(&origin.path, &ours) {
        return None;
    }
    Some(if origin.layer == "base" {
        tf("A map called \"{name}\" is already in Total Annihilation; choose another name.", &[("name", &stem)])
    } else {
        tf("A map called \"{name}\" is already in another mod; choose another name.", &[("name", &stem)])
    })
}

/// Writes the map as a mod in `mods_dir`. `author` goes into `mod.toml`.
pub fn write_mod(doc: &MapDoc, palette: &Palette, mods_dir: &Path, author: &str) -> Result<Saved> {
    let stem = file_stem(&doc.props.name);
    ensure!(ta_vfs::package::safe_component(&stem), "the map needs an ordinary, non-reserved file name");
    std::fs::create_dir_all(mods_dir)?;
    ensure!(!is_link(&std::fs::symlink_metadata(mods_dir)?), "the map library must not be a link");
    // The stable lock spans validation and publication, so two editor windows
    // cannot interleave terrain/metadata or race their ownership checks.
    let lock_path = mods_dir.join(ta_vfs::package::EDITOR_SAVE_LOCK);
    if let Ok(meta) = std::fs::symlink_metadata(&lock_path) {
        ensure!(meta.is_file() && !is_link(&meta), "the editor save lock must be an ordinary file");
    }
    let lock = std::fs::OpenOptions::new().create(true).truncate(false).read(true).write(true).open(&lock_path)?;
    lock.lock()?;
    // (A save stopped half way, between its two renames, left a map's only copy aside, where
    // discovery does not see it: it goes back first.)
    for done in ta_vfs::package::restore_editor_backups(mods_dir) {
        log::info!("map editor: {done}");
    }
    let dir = mods_dir.join(&stem);
    let existing = existing_manifest(&dir, &stem)?;
    let id = existing.as_ref().map_or_else(|| mod_id(&stem), |existing| existing.manifest.id.clone());
    let (other, _) = ta_vfs::mods::scan(mods_dir);
    let canonical = std::fs::canonicalize(&dir).ok();
    ensure!(!other.iter().any(|entry| entry.manifest.id == id
        && !(canonical.is_some() && std::fs::canonicalize(&entry.path).ok() == canonical)), "another mod uses this map's identity; choose another map name");
    let tnt = doc.compacted(palette).write().context("writing the TNT")?;
    let ota = doc.to_ota().to_bytes();
    // The editor's own (the map's identity and name, its author, its description while it is the
    // editor's) over the manifest as it was, written over the file: the rest of it, added by hand
    // (a banner, links, tags, a theme, what it needs, keys this TA Reborn does not know, comments),
    // is kept as written.
    let clean = |s: &str| s.trim().chars().filter(|c| !c.is_control()).collect::<String>();
    let (mut manifest, before) = match existing {
        Some(Existing { manifest, text }) => (manifest, Some(text)),
        None => (ta_vfs::mods::ModManifest::default(), None),
    };
    manifest.id = id.clone();
    manifest.name = clean(&format!("{} (map)", doc.props.name.trim()));
    if manifest.version.trim().is_empty() { manifest.version = "1.0".into(); }
    if manifest.description.trim().is_empty() || is_editor_description(&manifest.description) {
        manifest.description = clean(&editor_description(&doc.props.name));
    }
    // (Who saves it is one of its authors: after those it names, its maker first, as the launcher
    // shows them; co-authors added by hand stay.)
    let author = clean(author);
    if !author.is_empty() && !manifest.authors.iter().any(|named| named.trim().to_lowercase() == author.to_lowercase()) {
        manifest.authors.push(author);
    }
    let manifest = match &before {
        Some(text) => ta_vfs::mods::write_manifest(Some(text), &manifest).map_err(|error| anyhow::anyhow!(error)).context("writing the map's mod.toml")?,
        None => format!("{EDITOR_HEADER}\n{}", toml::to_string_pretty(&manifest).context("writing the map's mod.toml")?),
    };
    let stage = SaveStage::new(mods_dir, &stem)?;
    if dir.exists() { copy_existing(&dir, &stage.path, 0, &mut (0, 0))?; }
    std::fs::create_dir_all(stage.path.join("maps"))?;
    write_file(&stage.path.join("maps").join(format!("{stem}.tnt")), &tnt)?;
    write_file(&stage.path.join("maps").join(format!("{stem}.ota")), &ota)?;
    if let Some(features) = &doc.features {
        write_file(&stage.path.join("maps").join(format!("{stem}.features")), features)?;
    }
    write_file(&stage.path.join("mod.toml"), manifest.as_bytes())?;
    // The swap: the previous copy aside (named after the map's folder: a swap stopped half way is
    // put right, `ta_vfs::package::restore_editor_backups`), the new one in its place. Each step
    // tried again for a few seconds where Windows refuses it for a file open a moment (an
    // antivirus, the launcher reading the map).
    let backup = mods_dir.join(ta_vfs::package::editor_backup_name(&stage.work, &stem));
    let existed = dir.exists();
    if existed { ta_vfs::package::rename_patiently(&dir, &backup).context("preserving the previous map")?; }
    if let Err(error) = ta_vfs::package::rename_patiently(&stage.path, &dir) {
        if existed && let Err(restore) = ta_vfs::package::rename_patiently(&backup, &dir) {
            bail!("could not publish map ({error}); previous map remains at {} ({restore})", backup.display());
        }
        return Err(error).context("publishing the complete map");
    }
    // (Out of the backup's name first, then deleted: never left half deleted where it would be
    // taken for the map's only copy.)
    if existed { let _ = ta_vfs::package::discard(mods_dir, &backup); }
    Ok(Saved { dir, id, stem })
}

/// A save's working folder (`<work>.installing`), removed with this.
struct SaveStage { path: PathBuf, work: String }
impl SaveStage {
    /// One for saving the map of the folder `stem` in `parent`.
    fn new(parent: &Path, stem: &str) -> Result<Self> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        for _ in 0..100 {
            let work = format!(".editor-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed));
            let path = parent.join(format!("{work}.installing"));
            // (Not a name whose copy aside of a map is still there, from an earlier run.)
            if parent.join(ta_vfs::package::editor_backup_name(&work, stem)).exists() || parent.join(format!("{work}.old")).exists() { continue; }
            match std::fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path, work }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {},
                Err(error) => return Err(error.into()),
            }
        }
        bail!("could not create a private map save directory")
    }
}
impl Drop for SaveStage {
    fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.path); }
}

/// Preserve extra map assets and documentation while rejecting linked/unbounded
/// trees. A failure affects only the private staging directory.
fn copy_existing(from: &Path, to: &Path, depth: usize, work: &mut (usize, u64)) -> Result<()> {
    let meta = std::fs::symlink_metadata(from)?;
    work.0 += 1;
    work.1 = work.1.saturating_add(if meta.is_file() { meta.len() } else { 0 });
    ensure!(!is_link(&meta) && depth <= 64 && work.0 <= 100_000 && work.1 <= 8 << 30, "map folder contains links or exceeds save limits");
    if meta.is_dir() {
        std::fs::create_dir_all(to)?;
        for child in std::fs::read_dir(from)? {
            let child = child?;
            copy_existing(&child.path(), &to.join(child.file_name()), depth + 1, work)?;
        }
    } else {
        ensure!(meta.is_file(), "map folder contains a non-regular file");
        std::fs::copy(from, to)?;
    }
    Ok(())
}

/// Writes through a temporary file and a rename, so a failure never leaves half a file.
fn write_file(path: &Path, bytes: &[u8]) -> Result<()> {
    ta_config::write_atomic_bytes(path, bytes).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

/// Makes a saved map playable now and in later sessions: enables its mod in the settings
/// (saved to config.toml), mounts it over the running game's data unless it already is, and
/// lists it in the skirmish catalogue.
pub fn publish(saved: &Saved, assets: &mut crate::setup::Assets, settings: &mut ta_config::Config) -> Result<()> {
    let app_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf));
    let folders = ta_vfs::mods::default_folders(app_dir.as_deref(), &ta_config::config_dir(), settings.mods.extra_dir.as_deref());
    publish_in(saved, assets, settings, &folders, true)
}

/// [`publish`] with the mod folders to look in, saving the settings or not (tests).
pub fn publish_in(saved: &Saved, assets: &mut crate::setup::Assets, settings: &mut ta_config::Config, folders: &[PathBuf], save_settings: bool) -> Result<()> {
    let mounted: Vec<String> = assets.vfs.sources().map(|s| s.layer.clone()).fold(Vec::new(), |mut v, l| {
        if l != "base" && l != ta_vfs::BUILTIN_LAYER && !v.contains(&l) {
            v.push(l);
        }
        v
    });
    if !mounted.contains(&saved.id) {
        let mut order = mounted;
        order.push(saved.id.clone());
        let (fresh, in_play) = remount(&assets.vfs, &order, settings, folders)?;
        assets.compatibility = ta_data::Compatibility::load(&fresh);
        assets.data_hash = assets.compatibility.global();
        assets.vfs = Arc::new(fresh);
        // (Saved games and replays made from now on name the map's mod too.)
        assets.mods = crate::record_mods::InPlay::new(in_play);
    } else {
        // Loose files were replaced in place: the VFS still reads them, but records and
        // subsequent network games must no longer advertise the old terrain's hash.
        assets.compatibility = ta_data::Compatibility::load(&assets.vfs);
        assets.data_hash = assets.compatibility.global();
    }
    assets.maps = crate::setup::MapEntry::catalogue(&assets.vfs);
    assets.selectable_maps = crate::setup::MapEntry::enabled_catalogue(&assets.vfs, &settings.maps);
    if !settings.mods.enabled.iter().any(|m| m == &saved.id) {
        settings.mods.enabled.push(saved.id.clone());
    }
    // A previous attempt may have enabled the map in memory before its settings
    // write failed. Always retry persistence, and never report a failed save as done.
    if save_settings { settings.save().context("saving the enabled map in settings")?; }
    Ok(())
}

/// A new file system like `vfs` (the same install) with the mods `order` (found in
/// `folders`) mounted on it, and those mods, in their load order.
fn remount(vfs: &Vfs, order: &[String], settings: &ta_config::Config, folders: &[PathBuf]) -> Result<(Vfs, Vec<ta_vfs::mods::ModInfo>)> {
    let root = vfs
        .sources()
        .find(|s| s.layer == "base")
        .map(|s| if s.path.is_dir() { s.path.clone() } else { s.path.parent().map(Path::to_path_buf).unwrap_or_default() })
        .filter(|p| p.is_dir());
    let root = match root {
        Some(r) => r,
        None => ta_vfs::install::discover(settings.game_dir.as_deref())?.root,
    };
    // Built-in content is a special layer, not a discovered user mod. Preserve the
    // actual mounted directory (also works for an explicit TA_REBORN_ASSETS location).
    let builtin = vfs.sources()
        .filter(|s| s.layer == ta_vfs::BUILTIN_LAYER)
        .find_map(|s| if s.path.is_dir() { Some(s.path.clone()) } else { s.path.parent().map(Path::to_path_buf) });
    let (available, problems) = ta_vfs::mods::scan_folders(folders);
    for p in problems {
        log::warn!("mods: {p}");
    }
    // (Mounted as the game mounts them: what the mods replace of the install left out.)
    let mounted = ta_vfs::mods::mount_enabled(Some(&root), builtin.as_deref(), order, &available);
    mounted.install?;
    mounted.builtin?;
    let ids = mounted.mods.map_err(|e| anyhow::anyhow!("{e}"))?;
    log::info!("mods mounted after saving a map: {ids:?}");
    let in_play = ids.iter().filter_map(|id| available.iter().find(|m| m.manifest.id == *id)).cloned().collect();
    Ok((mounted.vfs, in_play))
}
