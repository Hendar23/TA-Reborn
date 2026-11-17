//! The game's files as a map names them (`/env/evergreen/layers/eg_gravel01_albedo.dds`,
//! `/maps/scmp_019/scmp_019.scmap`), found whatever their case, as the game finds them: a map's
//! own files in the folders holding maps (the players' own folders first, then the game's), the
//! rest in the game's archives (`gamedata/*.scd`) and Forged Alliance Forever's (`.nx2`, where
//! it is installed), then loose in the game's folder. For a player's own map FAF's archives are
//! read first (newer textures a map of its vault may use); for the game's own maps the game's are
//! ([`Files::for_map`]), so that they come out the same on every computer, FAF or not.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use ta_vfs::install::{find_dir_ci, find_file_ci};

use crate::install::Install;
use crate::scd::{Scd, normalise};
use crate::{Error, Result};

#[derive(Clone)]
pub struct Files {
    /// Folders holding a folder per map, the first found winning.
    map_dirs: Vec<PathBuf>,
    /// The game's own archives, and Forged Alliance Forever's.
    own: Arc<Vec<Scd>>,
    faf: Arc<Vec<Scd>>,
    /// Whether FAF's archives are read before the game's (a player's own maps), not after.
    faf_first: bool,
    /// The game's own folder.
    root: Option<PathBuf>,
}

impl Files {
    /// The files of the game at `install`, with the players' map folders `user_maps` first (and
    /// Forged Alliance Forever's archives before the game's: see [`Files::for_map`]).
    pub fn open(install: &Install, user_maps: &[PathBuf]) -> Result<Files> {
        // Forged Alliance Forever's own archives, where it is installed beside the game (newer
        // textures its vault's maps may use).
        #[cfg(windows)]
        let faf = {
            let program_data = std::env::var_os("ProgramData").map_or_else(|| PathBuf::from(r"C:\ProgramData"), PathBuf::from);
            open_all(&program_data.join("FAForever").join("gamedata"), "nx2")
        };
        #[cfg(not(windows))]
        let faf = Vec::new();
        let gamedata = install.gamedata();
        let own = open_all(&gamedata, "scd");
        if own.is_empty() {
            return Err(Error::Missing(format!("{}: none of the game's archives", gamedata.display())));
        }
        let mut map_dirs: Vec<PathBuf> = user_maps.to_vec();
        map_dirs.push(install.maps());
        Ok(Files { map_dirs, own: Arc::new(own), faf: Arc::new(faf), faf_first: true, root: Some(install.root.clone()) })
    }

    /// The game's own files alone (its `gamedata/*.scd` and its folder): what its units are, as
    /// Forged Alliance plays them (Forged Alliance Forever's archives, which rebalance them, are
    /// not read).
    pub fn own(install: &Install) -> Result<Files> {
        let gamedata = install.gamedata();
        let archives = open_all(&gamedata, "scd");
        if archives.is_empty() {
            return Err(Error::Missing(format!("{}: none of the game's archives", gamedata.display())));
        }
        Ok(Files { map_dirs: Vec::new(), own: Arc::new(archives), faf: Arc::new(Vec::new()), faf_first: false, root: Some(install.root.clone()) })
    }

    /// These files as a map of the game's own (`players_own` false) or a player's finds them: the
    /// game's own archives first for the game's maps, Forged Alliance Forever's first for a
    /// player's (made with FAF's textures at hand). The archives are shared, not opened again.
    pub fn for_map(&self, players_own: bool) -> Files {
        Files { faf_first: players_own, ..self.clone() }
    }

    /// Files from the archives and folders given, for the tests.
    #[cfg(test)]
    pub(crate) fn of(map_dirs: Vec<PathBuf>, archives: Vec<Scd>) -> Files {
        Files { map_dirs, own: Arc::new(archives), faf: Arc::new(Vec::new()), faf_first: false, root: None }
    }

    /// Files from the game's archives and FAF's given, for the tests.
    #[cfg(test)]
    pub(crate) fn with_faf(map_dirs: Vec<PathBuf>, own: Vec<Scd>, faf: Vec<Scd>) -> Files {
        Files { map_dirs, own: Arc::new(own), faf: Arc::new(faf), faf_first: true, root: None }
    }

    /// The archives in the order they are read.
    fn archives(&self) -> impl Iterator<Item = &Scd> {
        let (first, then) = if self.faf_first { (&self.faf, &self.own) } else { (&self.own, &self.faf) };
        first.iter().chain(then.iter())
    }

    /// The archives' files under `prefix` (normalised paths, in order, each once).
    pub fn list(&self, prefix: &str) -> Vec<String> {
        let mut all: Vec<String> = self.archives().flat_map(|a| a.list(prefix).map(str::to_string)).collect();
        all.sort();
        all.dedup();
        all
    }

    /// The bytes of `path` (a path as a map names it), or `None` where the game has none.
    pub fn read(&self, path: &str) -> Result<Option<Vec<u8>>> {
        let norm = normalise(path);
        if let Some(rest) = norm.strip_prefix("maps/")
            && let Some(file) = self.map_file(rest)
        {
            return std::fs::read(&file).map(Some).map_err(|e| Error::io(&file, e));
        }
        for archive in self.archives() {
            if let Some(bytes) = archive.read(&norm)? {
                return Ok(Some(bytes));
            }
        }
        if let Some(root) = &self.root
            && let Some(file) = loose(root, &norm)
        {
            return std::fs::read(&file).map(Some).map_err(|e| Error::io(&file, e));
        }
        Ok(None)
    }

    /// Where a map's file is: `rest` (`<map folder>/<file>`) in the first folder of maps holding
    /// it.
    pub fn map_file(&self, rest: &str) -> Option<PathBuf> {
        self.map_dirs.iter().find_map(|dir| loose(dir, rest))
    }

    /// The folders holding maps, players' first.
    pub fn map_dirs(&self) -> &[PathBuf] {
        &self.map_dirs
    }
}

/// `rel` (`a/b/c.dds`, lower case) under `base`, each name found whatever its case.
fn loose(base: &Path, rel: &str) -> Option<PathBuf> {
    let mut dir = base.to_path_buf();
    let parts: Vec<&str> = rel.split('/').filter(|p| !p.is_empty()).collect();
    let (file, folders) = parts.split_last()?;
    for folder in folders {
        if *folder == ".." || *folder == "." {
            return None;
        }
        dir = find_dir_ci(&dir, folder)?;
    }
    find_file_ci(&dir, file)
}

/// Every archive with extension `ext` in `dir`, in name order (any it cannot open left out).
fn open_all(dir: &Path, ext: &str) -> Vec<Scd> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.is_file() && p.extension().is_some_and(|x| x.eq_ignore_ascii_case(ext))).collect();
    paths.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()));
    paths.into_iter().filter_map(|p| Scd::open(&p).map_err(|e| log::warn!("{e}")).ok()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_maps_files_come_from_the_players_folders_first_and_the_rest_from_the_archives() {
        let dir = crate::tests::Temp::new("files");
        let game_maps = dir.0.join("game/maps");
        let user_maps = dir.0.join("user/maps");
        std::fs::create_dir_all(game_maps.join("SCMP_019")).unwrap();
        std::fs::write(game_maps.join("SCMP_019/SCMP_019.scmap"), b"game's").unwrap();
        std::fs::create_dir_all(user_maps.join("Theirs.v0002/env")).unwrap();
        std::fs::write(user_maps.join("Theirs.v0002/env/Ground_albedo.dds"), b"own texture").unwrap();
        std::fs::write(dir.0.join("env.scd"), crate::scd::tests::zip(&[("env/Evergreen/Layers/Grass_albedo.dds", b"from the archive", false)])).unwrap();
        let files = Files::of(vec![user_maps, game_maps], vec![Scd::open(&dir.0.join("env.scd")).unwrap()]);
        assert_eq!(files.read("/maps/scmp_019/scmp_019.scmap").unwrap().unwrap(), b"game's");
        assert_eq!(files.read("/maps/theirs.v0002/ENV/ground_albedo.dds").unwrap().unwrap(), b"own texture");
        assert_eq!(files.read("/env/evergreen/layers/grass_albedo.dds").unwrap().unwrap(), b"from the archive");
        assert!(files.read("/env/evergreen/layers/missing.dds").unwrap().is_none());
        assert!(files.read("/maps/../secret.txt").unwrap().is_none());
    }

    /// Where Forged Alliance Forever is installed and changes a file of the game's: the game's own
    /// maps read the game's (the same maps on every computer, FAF or not), a player's own maps
    /// FAF's; a file FAF alone has is read either way.
    #[test]
    fn the_games_own_maps_read_the_games_files_first() {
        let dir = crate::tests::Temp::new("files-faf");
        std::fs::write(dir.0.join("env.scd"), crate::scd::tests::zip(&[("env/evergreen/layers/grass_albedo.dds", b"the game's", false)])).unwrap();
        std::fs::write(dir.0.join("env.nx2"), crate::scd::tests::zip(&[("env/evergreen/layers/grass_albedo.dds", b"FAF's", false), ("env/evergreen/layers/new_albedo.dds", b"FAF's own", false)])).unwrap();
        let files = Files::with_faf(Vec::new(), vec![Scd::open(&dir.0.join("env.scd")).unwrap()], vec![Scd::open(&dir.0.join("env.nx2")).unwrap()]);
        let read = |f: &Files, p: &str| f.read(p).unwrap().unwrap();
        assert_eq!(read(&files.for_map(false), "/env/evergreen/layers/grass_albedo.dds"), b"the game's");
        assert_eq!(read(&files.for_map(true), "/env/evergreen/layers/grass_albedo.dds"), b"FAF's");
        assert_eq!(read(&files.for_map(false), "/env/evergreen/layers/new_albedo.dds"), b"FAF's own");
    }
}
