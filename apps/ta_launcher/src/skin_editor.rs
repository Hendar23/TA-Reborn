//! The skin editor's state: the skin being made (its manifest, and every image it may use), what
//! was last saved, undo and redo, and the change each control makes. `pages/skin_editor.rs`
//! draws it; the preview paints it as it changes (`skin_preview.rs`).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use image::RgbaImage;
use ta_skins::{Colour, Livery, Manifest, Pack, Pattern, UnitSkin};
use ta_ui::i18n::tf;

/// A texture slot for every armour surface the skin may repaint.
pub const EVERY: &str = "*";
/// A texture slot for armour painted in a plain colour (no texture).
pub const PLAIN: &str = "_untextured";
/// Changes to one control this close together are one step to undo (a slider dragged).
const MERGE_SECONDS: f64 = 1.5;
/// How many steps can be undone.
const UNDO_DEPTH: usize = 200;

pub struct Editor {
    /// The installed skin it edits; none for one not saved yet.
    pub id: Option<String>,
    pub manifest: Manifest,
    /// Every image the skin may use: those it came with and those assigned since (an undo can
    /// map one again). Shared, never copied, with the preview's skin.
    images: BTreeMap<String, Arc<RgbaImage>>,
    /// The images already in its folder.
    on_disk: BTreeSet<String>,
    /// The images this opening of the editor wrote into its folder: those the skin no longer
    /// maps go when it is saved again (the artist's own files never do).
    written: BTreeSet<String>,
    /// Every other file of its folder (in lower case): a new image never takes one's name.
    reserved: BTreeSet<String>,
    /// The manifest as last saved (or as opened).
    saved: Manifest,
    undo: Vec<Manifest>,
    redo: Vec<Manifest>,
    /// The control changed last, and when.
    last: Option<(&'static str, f64)>,
    /// A livery switched off, to switch back on.
    parked: Option<Livery>,
    /// Bumped whenever the mappings or the images change: the preview's models are made again.
    pub textures: u64,
    /// The preview's units, Arm's and Core's (empty: the usual examples).
    pub units: [String; 2],
    /// The unit texture slots map for, or none for every unit.
    pub scope: Option<String>,
    preview: Option<(u64, Arc<SharedPack>)>,
    /// Which opening of the editor this is (pictures made of its images are its own: another
    /// skin's may have the same names).
    pub serial: u64,
}

/// A new skin's first livery: plain olive drab, as Reborn wears it.
pub fn starting_livery() -> Livery {
    Livery::new(Pattern::Solid, &[Colour::rgb(0x6b705c)])
}

/// The name offered for a copy of the skin `name` ("Desert copy"), its own part cut short so
/// the whole fits a skin's name ([`ta_skins::MAX_NAME`] characters).
pub fn copy_name(name: &str) -> String {
    let copy = |name: &str| tf("{name} copy", &[("name", &name)]);
    let whole = copy(name.trim());
    if whole.chars().count() <= ta_skins::MAX_NAME { return whole; }
    let room = ta_skins::MAX_NAME.saturating_sub(copy("").chars().count());
    let cut: String = name.trim().chars().take(room).collect();
    copy(cut.trim_end())
}

/// The editor's skin as the preview paints it: its manifest and the images it maps, shared with
/// the editor (the preview's worker copies them into a pack, off the interface's thread, when it
/// makes its models again).
pub struct SharedPack {
    pub manifest: Manifest,
    pub images: BTreeMap<String, Arc<RgbaImage>>,
}

impl SharedPack {
    /// A pack of its own (its images copied).
    pub fn to_pack(&self) -> Pack {
        Pack { manifest: self.manifest.clone(), images: self.images.iter().map(|(name, image)| (name.clone(), RgbaImage::clone(image))).collect() }
    }
}

impl Editor {
    /// Edits `pack`: the installed skin `id`, or (none) a new one to be saved as new.
    pub fn open(pack: Pack, id: Option<String>) -> Self {
        static OPENED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let on_disk = if id.is_some() { pack.images.keys().cloned().collect() } else { BTreeSet::new() };
        Self {
            id,
            saved: pack.manifest.clone(),
            manifest: pack.manifest,
            images: pack.images.into_iter().map(|(name, image)| (name, Arc::new(image))).collect(),
            on_disk,
            written: BTreeSet::new(),
            reserved: BTreeSet::new(),
            undo: Vec::new(),
            redo: Vec::new(),
            last: None,
            parked: None,
            textures: 0,
            units: [String::new(), String::new()],
            scope: None,
            preview: None,
            serial: OPENED.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        }
    }

    /// The files its folder has besides (`ta_skins::folder_files`): no new image takes their names.
    pub fn reserve(&mut self, files: impl IntoIterator<Item = String>) {
        self.reserved.extend(files.into_iter().map(|file| file.to_ascii_lowercase()));
    }

    /// Whether there is anything to save: changes since it was saved, or a skin never saved.
    pub fn dirty(&self) -> bool {
        self.id.is_none() || self.manifest != self.saved
    }

    /// Changes made since it was opened or last saved (what closing would lose).
    pub fn changed(&self) -> bool {
        self.manifest != self.saved
    }

    pub fn can_undo(&self) -> bool { !self.undo.is_empty() }
    pub fn can_redo(&self) -> bool { !self.redo.is_empty() }

    /// Changes the manifest by `change`, for the control `control` at `now` (seconds): one
    /// step to undo, unless the same control changed a moment ago (a slider being dragged).
    pub fn change(&mut self, control: &'static str, now: f64, change: impl FnOnce(&mut Manifest)) {
        let before = self.manifest.clone();
        change(&mut self.manifest);
        if self.manifest == before { return; }
        let merge = self.last.is_some_and(|(last, at)| last == control && now - at < MERGE_SECONDS);
        if !merge {
            self.undo.push(before.clone());
            if self.undo.len() > UNDO_DEPTH { self.undo.remove(0); }
        }
        self.redo.clear();
        self.last = Some((control, now));
        self.after(&before);
    }

    /// Changes the livery, if it has one.
    pub fn livery(&mut self, control: &'static str, now: f64, change: impl FnOnce(&mut Livery)) {
        self.change(control, now, |manifest| if let Some(livery) = manifest.livery.as_mut() { change(livery) });
    }

    pub fn undo(&mut self) {
        let Some(previous) = self.undo.pop() else { return };
        let before = std::mem::replace(&mut self.manifest, previous);
        self.redo.push(before.clone());
        self.last = None;
        self.after(&before);
    }

    pub fn redo(&mut self) {
        let Some(next) = self.redo.pop() else { return };
        let before = std::mem::replace(&mut self.manifest, next);
        self.undo.push(before.clone());
        self.last = None;
        self.after(&before);
    }

    /// Back to the skin as last saved (itself a step to undo).
    pub fn revert(&mut self, now: f64) {
        let saved = self.saved.clone();
        self.last = None;
        self.change("revert", now, |manifest| *manifest = saved);
        self.last = None;
    }

    /// Paints a livery, or not (the one switched off kept, to come back).
    pub fn set_livery_on(&mut self, on: bool, now: f64) {
        if on == self.manifest.livery.is_some() { return; }
        let parked = self.parked.take();
        let mut taken = None;
        self.change("livery", now, |manifest| {
            if on { manifest.livery = Some(parked.unwrap_or_else(starting_livery)); } else { taken = manifest.livery.take(); }
        });
        self.parked = taken;
    }

    /// The image path a slot maps for `scope` (one unit's, or every unit's), exactly there.
    pub fn mapping(&self, scope: Option<&str>, texture: &str) -> Option<&str> {
        let texture = texture.to_ascii_lowercase();
        match scope {
            None => self.manifest.textures.get(&texture),
            Some(unit) => self.manifest.units.get(&unit.to_ascii_lowercase()).and_then(|skin| skin.textures.get(&texture)),
        }.map(String::as_str)
    }

    /// Maps `texture` (for `scope`) to `image`, kept in the skin under a name of its own;
    /// returns that name.
    pub fn assign(&mut self, scope: Option<&str>, texture: &str, image: RgbaImage, now: f64) -> String {
        let stem = match texture {
            EVERY => "armour".to_string(),
            PLAIN => "plain".to_string(),
            name => name.to_ascii_lowercase(),
        };
        let stem: String = scope.map_or(stem.clone(), |unit| format!("{}-{stem}", unit.to_ascii_lowercase()))
            .chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' }).collect();
        let path = (1..).map(|n| if n == 1 { format!("textures/{stem}.png") } else { format!("textures/{stem}-{n}.png") })
            .find(|path| !self.images.keys().chain(&self.on_disk).any(|taken| taken.eq_ignore_ascii_case(path)) && !self.reserved.contains(&path.to_ascii_lowercase()))
            .expect("some name is free");
        self.images.insert(path.clone(), Arc::new(image));
        let (key, value) = (texture.to_ascii_lowercase(), path.clone());
        let unit = scope.map(str::to_ascii_lowercase);
        self.last = None;
        self.change("assign", now, |manifest| match unit {
            None => { manifest.textures.insert(key, value); }
            Some(unit) => { manifest.units.entry(unit).or_insert_with(UnitSkin::default).textures.insert(key, value); }
        });
        self.last = None;
        path
    }

    /// Takes `texture`'s image away (for `scope`): the livery, or the unit's own paint, shows there again.
    pub fn clear(&mut self, scope: Option<&str>, texture: &str, now: f64) {
        let key = texture.to_ascii_lowercase();
        let unit = scope.map(str::to_ascii_lowercase);
        self.last = None;
        self.change("clear", now, |manifest| match unit {
            None => { manifest.textures.remove(&key); }
            Some(unit) => {
                if let Some(skin) = manifest.units.get_mut(&unit) { skin.textures.remove(&key); }
                if manifest.units.get(&unit).is_some_and(|skin| skin.textures.is_empty()) { manifest.units.remove(&unit); }
            }
        });
        self.last = None;
    }

    /// An image the skin may use, by name.
    pub fn image(&self, path: &str) -> Option<&RgbaImage> {
        self.images.get(path).map(|image| &**image)
    }

    /// The skin as it stands: its manifest and the images it maps.
    pub fn pack(&self) -> Pack {
        self.shared().to_pack()
    }

    /// The skin as it stands, its images shared.
    fn shared(&self) -> SharedPack {
        let names = self.manifest.image_names();
        SharedPack { manifest: self.manifest.clone(), images: self.images.iter().filter(|(name, _)| names.contains(name.as_str())).map(|(n, i)| (n.clone(), i.clone())).collect() }
    }

    /// The skin for the preview, made again only when its mappings or images changed (its
    /// images shared, not copied).
    pub fn preview_pack(&mut self) -> Arc<SharedPack> {
        if let Some((revision, pack)) = &self.preview && *revision == self.textures {
            return pack.clone();
        }
        let pack = Arc::new(self.shared());
        self.preview = Some((self.textures, pack.clone()));
        pack
    }

    /// The images it maps that its folder does not have yet (what saving it writes).
    pub fn added(&self) -> BTreeMap<String, RgbaImage> {
        let names = self.manifest.image_names();
        self.images.iter().filter(|(name, _)| names.contains(name.as_str()) && !self.on_disk.contains(*name)).map(|(n, i)| (n.clone(), RgbaImage::clone(i))).collect()
    }

    /// The images this opening wrote into its folder that `manifest` (about to be saved there) no
    /// longer maps: the save takes them away.
    pub fn stale(&self, manifest: &Manifest) -> BTreeSet<String> {
        let names = manifest.image_names();
        self.written.iter().filter(|name| !names.contains(name.as_str())).cloned().collect()
    }

    /// It was saved as the installed skin `id` (with the manifest `manifest`, its id and name as
    /// saved); the save wrote the images of `renamed` under other names (from, to), and took
    /// those of `removed` out of its folder.
    pub fn saved(&mut self, id: &str, manifest: &Manifest, renamed: &BTreeMap<String, String>, removed: &BTreeSet<String>) {
        let names = manifest.image_names();
        // (Saved as another skin, its folder has only the images mapped then: one mapped again
        // by an undo is written at the next save. Saved again, a folder keeps those it had.)
        if self.id.as_deref() != Some(id) {
            self.on_disk.clear();
            self.written.clear();
            self.reserved.clear();
        }
        if !renamed.is_empty() {
            // Its images under the names the save gave them; the names given are the folder's
            // own files.
            for (from, to) in renamed {
                if let Some(image) = self.images.remove(from) { self.images.insert(to.clone(), image); }
                self.reserved.insert(from.to_ascii_lowercase());
            }
            let rename = |manifest: &mut Manifest| {
                let each = |textures: &mut BTreeMap<String, String>| for path in textures.values_mut() {
                    if let Some(to) = renamed.get(path) { *path = to.clone(); }
                };
                each(&mut manifest.textures);
                manifest.units.values_mut().for_each(|unit| each(&mut unit.textures));
            };
            rename(&mut self.manifest);
            self.undo.iter_mut().chain(self.redo.iter_mut()).for_each(rename);
            self.textures += 1;
        }
        self.written.extend(names.iter().filter(|name| !self.on_disk.contains(**name)).map(|name| name.to_string()));
        self.on_disk.extend(names.iter().map(|name| name.to_string()));
        for name in removed {
            self.on_disk.remove(name);
            self.written.remove(name);
        }
        self.id = Some(id.to_string());
        self.manifest.id = id.to_string();
        self.manifest.name = manifest.name.clone();
        // The worker saved its snapshot, not edits or undo actions made while it
        // was running. Those changes must remain dirty and available to save.
        self.saved = manifest.clone();
        self.saved.id = id.to_string();
        // (The steps to undo stay, under the id it has now.)
        for step in self.undo.iter_mut().chain(self.redo.iter_mut()) {
            step.id = id.to_string();
        }
    }

    /// The preview's models are made again after a change to the mappings.
    fn after(&mut self, before: &Manifest) {
        // (The preview's models are made again for new mappings, or for TA's own shapes in
        // place of Reborn's kit or back.)
        let original = |m: &Manifest| m.livery.as_ref().is_some_and(|l| l.original);
        if before.textures != self.manifest.textures || before.units != self.manifest.units || original(before) != original(&self.manifest) {
            self.textures += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor() -> Editor {
        let pack = ta_skins::load("builtin:desert", std::path::Path::new("unused")).unwrap();
        let mut manifest = pack.manifest;
        manifest.id = "desert-copy".into();
        Editor::open(Pack { manifest, images: BTreeMap::new() }, None)
    }

    fn solid(side: u32) -> RgbaImage { RgbaImage::from_pixel(side, side, image::Rgba([90, 80, 70, 255])) }

    #[test]
    fn edits_made_while_a_save_runs_remain_unsaved_and_revert_to_the_written_snapshot() {
        let mut editor = editor();
        let mut written = editor.manifest.clone();
        written.id = "written-skin".into();
        editor.livery("wear", 0.0, |livery| livery.wear = 0.9);
        editor.saved("written-skin", &written, &BTreeMap::new(), &BTreeSet::new());
        assert!(editor.dirty() && editor.changed(), "the worker did not write the later edit");
        assert_eq!(editor.manifest.livery.as_ref().unwrap().wear, 0.9);
        editor.revert(1.0);
        assert_eq!(editor.manifest, written);
        assert!(!editor.dirty());
    }

    #[test]
    fn saved_as_another_skin_a_picture_mapped_again_is_written_into_its_folder() {
        let mut editor = editor();
        editor.assign(None, EVERY, solid(64), 0.0);
        let manifest = editor.manifest.clone();
        editor.saved("first", &manifest, &BTreeMap::new(), &BTreeSet::new());
        assert!(editor.added().is_empty(), "saved: the picture is in the first skin's folder");
        // Its picture taken away, then saved as another skin: that one's folder has none.
        editor.clear(None, EVERY, 5.0);
        let mut copy = editor.manifest.clone();
        copy.id = "second".into();
        editor.saved("second", &copy, &BTreeMap::new(), &BTreeSet::new());
        assert!(editor.added().is_empty());
        // An undo maps the picture again: the next save writes it into the second folder.
        editor.undo();
        assert_eq!(editor.mapping(None, EVERY), Some("textures/armour.png"));
        assert_eq!(editor.added().len(), 1, "the second skin's folder does not have the picture");
        // Saved again under the same id, a folder keeps what it had.
        let again = editor.manifest.clone();
        editor.saved("second", &again, &BTreeMap::new(), &BTreeSet::new());
        assert!(editor.added().is_empty());
    }

    #[test]
    fn every_opening_of_the_editor_is_told_apart() {
        assert_ne!(editor().serial, editor().serial, "pictures cached for one skin are not shown for another");
    }

    #[test]
    fn a_dragged_slider_is_one_step_to_undo_and_redo_brings_it_back() {
        let mut editor = editor();
        assert!(editor.dirty(), "a skin never saved has something to save");
        let scale = editor.manifest.livery.as_ref().unwrap().scale;
        for (i, at) in [0.0, 0.1, 0.2, 0.3].into_iter().enumerate() {
            editor.livery("scale", at, |livery| livery.scale = 1.5 + i as f32 * 0.1);
        }
        editor.livery("wear", 0.4, |livery| livery.wear = 0.9);
        assert_eq!(editor.undo.len(), 2, "the drag, then the wear");
        editor.undo();
        assert_eq!(editor.manifest.livery.as_ref().unwrap().wear, 0.6);
        editor.undo();
        assert_eq!(editor.manifest.livery.as_ref().unwrap().scale, scale);
        assert!(!editor.can_undo());
        editor.redo();
        assert!((editor.manifest.livery.as_ref().unwrap().scale - 1.8).abs() < 1e-6);
        // (After a pause the same slider is a step of its own.)
        editor.livery("scale", 10.0, |livery| livery.scale = 2.0);
        editor.livery("scale", 20.0, |livery| livery.scale = 3.0);
        editor.undo();
        assert_eq!(editor.manifest.livery.as_ref().unwrap().scale, 2.0);
        assert!(!editor.can_redo() || editor.redo.len() == 1);
    }

    #[test]
    fn switching_the_livery_off_and_on_keeps_it() {
        let mut editor = editor();
        let livery = editor.manifest.livery.clone();
        editor.set_livery_on(false, 0.0);
        assert!(editor.manifest.livery.is_none());
        editor.set_livery_on(true, 5.0);
        assert_eq!(editor.manifest.livery, livery);
        editor.undo();
        editor.undo();
        assert_eq!(editor.manifest.livery, livery);
    }

    /// Painting TA's originals in place of TA Reborn's units (or back) makes the preview's models
    /// again (no kit, or the kit); the livery's other values only paint them.
    #[test]
    fn tas_originals_make_the_preview_models_again() {
        let mut editor = editor();
        let revision = editor.textures;
        editor.livery("wear", 0.0, |livery| livery.wear = 0.9);
        editor.livery("fade", 1.0, |livery| livery.fade = 0.3);
        assert_eq!(editor.textures, revision, "paint alone is one write");
        editor.livery("original", 2.0, |livery| livery.original = true);
        assert!(editor.textures > revision, "TA's own shapes: the models made again");
        let after = editor.textures;
        editor.undo();
        assert!(editor.textures > after && !editor.manifest.livery.as_ref().unwrap().original, "and back");
    }

    #[test]
    fn texture_slots_map_for_every_unit_or_one_and_new_images_are_what_saving_writes() {
        let mut editor = editor();
        let revision = editor.textures;
        let every = editor.assign(None, EVERY, solid(64), 0.0);
        assert_eq!(every, "textures/armour.png");
        let commander = editor.assign(Some("ARMCOM"), "ArmMetal1", solid(128), 1.0);
        assert_eq!(commander, "textures/armcom-armmetal1.png");
        let again = editor.assign(None, EVERY, solid(256), 2.0);
        assert_eq!(again, "textures/armour-2.png", "a new image never takes an old one's name");
        assert!(editor.textures > revision, "the preview's models are made again");
        assert_eq!(editor.mapping(None, "*"), Some("textures/armour-2.png"));
        assert_eq!(editor.mapping(Some("armcom"), "armmetal1"), Some("textures/armcom-armmetal1.png"));
        assert_eq!(editor.manifest.texture_for("armcom", "armmetal1"), Some("textures/armcom-armmetal1.png"));
        assert_eq!(editor.manifest.texture_for("corak", "cormetal"), Some("textures/armour-2.png"));
        let pack = editor.pack();
        assert_eq!(pack.images.len(), 2, "only the images it maps");
        assert_eq!(editor.added().len(), 2);
        editor.undo();
        assert_eq!(editor.mapping(None, EVERY), Some("textures/armour.png"), "undone, the image before maps again");
        editor.clear(Some("armcom"), "ArmMetal1", 3.0);
        assert!(!editor.manifest.units.contains_key("armcom"), "an emptied unit goes");
        let mut manifest = editor.manifest.clone();
        manifest.id = "my-desert".into();
        editor.saved("my-desert", &manifest, &BTreeMap::new(), &BTreeSet::new());
        assert!(!editor.dirty());
        assert!(editor.added().is_empty(), "saved images are in the folder");
        editor.assign(None, PLAIN, solid(64), 9.0);
        assert_eq!(editor.added().len(), 1);
        assert!(editor.dirty());
        editor.revert(10.0);
        assert!(!editor.dirty());
        assert_eq!(editor.mapping(None, PLAIN), None);
    }

    #[test]
    fn the_preview_gets_the_same_skin_until_its_textures_change() {
        let mut editor = editor();
        let first = editor.preview_pack();
        editor.livery("gloss", 0.0, |livery| livery.gloss = 0.9);
        assert!(Arc::ptr_eq(&first, &editor.preview_pack()), "a livery change paints live, with the same models");
        editor.assign(None, EVERY, solid(64), 1.0);
        assert!(!Arc::ptr_eq(&first, &editor.preview_pack()));
    }

    /// The preview's skin shares the editor's images: a change of the mappings copies none (with
    /// a few large pictures, a copy for each change held up the interface).
    #[test]
    fn the_preview_shares_the_editors_images() {
        let mut editor = editor();
        let path = editor.assign(None, EVERY, solid(1024), 0.0);
        let first = editor.preview_pack();
        editor.assign(Some("armcom"), "ArmMetal1", solid(64), 1.0);
        let second = editor.preview_pack();
        assert!(!Arc::ptr_eq(&first, &second), "the mappings changed");
        assert!(Arc::ptr_eq(&first.images[&path], &second.images[&path]), "the same picture, not a copy of it");
        assert!(Arc::ptr_eq(&second.images[&path], &editor.images[&path]));
        assert_eq!(second.to_pack().images[&path], solid(1024), "the worker's copy");
    }

    /// Opened on an installed skin, a new image never takes the name of a file its folder has
    /// (one its skin.toml no longer maps, an artist's own): saving it wrote over nothing and
    /// failed every time.
    #[test]
    fn a_new_image_never_takes_the_name_of_a_file_in_the_folder() {
        let mut manifest = editor().manifest;
        manifest.id = "painted".into();
        let mut editor = Editor::open(Pack { manifest, images: BTreeMap::new() }, Some("painted".into()));
        editor.reserve(["skin.toml".to_string(), "textures/Armour.png".to_string(), "textures/armour-2.png".to_string()]);
        assert_eq!(editor.assign(None, EVERY, solid(64), 0.0), "textures/armour-3.png");
        assert_eq!(editor.added().len(), 1);
    }

    /// An image the save had to write under another name is the editor's under that name: in the
    /// skin, in its steps to undo, and nothing left to save.
    #[test]
    fn an_image_the_save_renamed_is_mapped_under_its_new_name() {
        let mut manifest = editor().manifest;
        manifest.id = "painted".into();
        let mut editor = Editor::open(Pack { manifest, images: BTreeMap::new() }, Some("painted".into()));
        editor.livery("wear", 0.0, |livery| livery.wear = 0.2);
        let given = editor.assign(None, EVERY, solid(64), 1.0);
        let mut written = editor.manifest.clone();
        written.textures.insert("*".into(), "textures/armour-9.png".into());
        let renamed = BTreeMap::from([(given.clone(), "textures/armour-9.png".to_string())]);
        editor.saved("painted", &written, &renamed, &BTreeSet::new());
        assert_eq!(editor.mapping(None, EVERY), Some("textures/armour-9.png"));
        assert!(!editor.dirty() && editor.added().is_empty(), "saved as written");
        assert!(editor.image("textures/armour-9.png").is_some() && editor.image(&given).is_none());
        assert_eq!(editor.assign(None, PLAIN, solid(64), 2.0), "textures/plain.png");
        assert_ne!(editor.assign(None, EVERY, solid(64), 3.0), given, "the name given is the folder's own file");
    }

    /// Pictures this opening wrote that the skin stopped mapping are what the next save takes
    /// away; one taken away and mapped again by an undo is written again.
    #[test]
    fn pictures_written_and_then_replaced_are_stale_at_the_next_save() {
        let mut manifest = editor().manifest;
        manifest.id = "painted".into();
        let mut editor = Editor::open(Pack { manifest, images: BTreeMap::new() }, Some("painted".into()));
        let first = editor.assign(None, EVERY, solid(64), 0.0);
        let snapshot = editor.manifest.clone();
        assert!(editor.stale(&snapshot).is_empty());
        editor.saved("painted", &snapshot, &BTreeMap::new(), &BTreeSet::new());
        let second = editor.assign(None, EVERY, solid(128), 1.0);
        let snapshot = editor.manifest.clone();
        assert_eq!(editor.stale(&snapshot), BTreeSet::from([first.clone()]));
        editor.saved("painted", &snapshot, &BTreeMap::new(), &BTreeSet::from([first.clone()]));
        assert!(editor.stale(&editor.manifest.clone()).is_empty());
        editor.undo();
        assert_eq!(editor.mapping(None, EVERY), Some(first.as_str()));
        assert_eq!(editor.added().keys().collect::<Vec<_>>(), [&first], "taken away from the folder: written again");
        assert_ne!(second, first);
    }

    /// A copy's name fits a skin's name, however long the skin's own.
    #[test]
    fn a_copys_name_fits_a_skins_name() {
        assert_eq!(copy_name("Desert"), "Desert copy");
        let long = "Operation Long Winter Night Camouflage Pattern For The Northern Front Line Units";
        assert!(long.chars().count() >= 75);
        let copy = copy_name(long);
        assert!(copy.chars().count() <= ta_skins::MAX_NAME && copy.ends_with(" copy"), "{copy}");
        assert!(copy.starts_with("Operation Long Winter"));
        let wide = "冬".repeat(80);
        assert!(copy_name(&wide).chars().count() <= ta_skins::MAX_NAME);
    }
}
