//! A mod's `mod.toml` as TA Reborn reads it and writes it back ([`super::ModManifest`]).
//!
//! What only presents a mod (its words, its pictures, its links, its interface theme) is read
//! leniently: a value of another kind than its key takes is left out and the rest of the manifest
//! read, one string where a list is wanted a list of one. What decides whether the mod is mounted
//! (its id and name, what it needs and loads after, what it must not be on with, the TA Reborn it
//! plays in, its renamed folders and the archives it replaces) is read strictly: a wrong kind
//! there is a manifest that does not read, and the mod is left out (a lone string for a list of
//! ids is still the list of that one id). [`manifest_notes`] tells what a manifest holds that is
//! read past: keys this TA Reborn does not know (one it knows in another table, the usual slip in
//! TOML, told as that) and values left out. [`write_manifest`] writes a manifest over the file it
//! was, keeping as written all it does not change: a later TA Reborn's keys survive an older one's
//! install.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer};
use toml::Value;

use super::{ClassicLook, LOOK_COLOURS, Look, Media, ModManifest, Theme, web_address};

/// The keys of a manifest's top level, in the order TA Reborn writes them.
const TOP_KEYS: &[&str] = &[
    "id", "name", "version", "description", "authors", "depends", "load_after", "conflicts", "requires_reborn", "source", "rename",
    "replaces", "tags", "media", "links", "theme",
];
/// The keys of `[media]`.
const MEDIA_KEYS: &[&str] = &["banner", "preview", "icon", "screenshots"];
/// A look's pictures, beside its colours ([`LOOK_COLOURS`]).
const LOOK_PICTURES: &[&str] = &["panel_texture", "bar_texture"];
/// What `[theme]` has beyond a look's keys.
const THEME_OWN: &[&str] = &["font", "classic", "sides"];
/// The keys of `[theme.classic]`.
const CLASSIC_KEYS: &[&str] = &["text", "button_text", "dim", "pressed"];

fn look_key(key: &str) -> bool {
    LOOK_COLOURS.contains(&key) || LOOK_PICTURES.contains(&key)
}

/// The keys of a manifest's top level that decide whether the mod is mounted and with what: one
/// of them misplaced or mistyped mounts the mod as if it were not given.
pub const MOUNTING_KEYS: &[&str] = &["id", "depends", "load_after", "conflicts", "requires_reborn", "rename", "replaces"];

/// The key this TA Reborn knows that the unknown key at `path` (a [`ManifestNote`]'s, dotted as
/// written) was most likely meant to be: one of its table's, a letter or two from it (or written
/// with `-` for `_`). None where none is that near, or the table takes any key (`[links]`).
pub fn closest_key(path: &str) -> Option<&'static str> {
    // Its parts, split on the dots outside quotes, each without its quotes.
    let (mut parts, mut part, mut quoted) = (Vec::new(), String::new(), false);
    for c in path.chars() {
        match c {
            '"' => quoted = !quoted,
            '.' if !quoted => parts.push(std::mem::take(&mut part)),
            _ => part.push(c),
        }
    }
    parts.push(part);
    let key = parts.pop()?.to_lowercase().replace('-', "_");
    let theme: Vec<&'static str> = LOOK_COLOURS.iter().chain(LOOK_PICTURES).chain(THEME_OWN).copied().collect();
    let look: Vec<&'static str> = LOOK_COLOURS.iter().chain(LOOK_PICTURES).copied().collect();
    let known: &[&'static str] = match parts.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        [] => TOP_KEYS,
        ["media"] => MEDIA_KEYS,
        ["theme"] => &theme,
        ["theme", "classic"] => CLASSIC_KEYS,
        ["theme", "sides", _] => &look,
        _ => return None,
    };
    // (Letters added, left out, changed or two swapped: each one edit.)
    let distance = |a: &str, b: &str| {
        let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
        let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
        for (i, row) in d.iter_mut().enumerate() { row[0] = i; }
        for j in 0..=b.len() { d[0][j] = j; }
        for i in 1..=a.len() {
            for j in 1..=b.len() {
                let cost = usize::from(a[i - 1] != b[j - 1]);
                d[i][j] = (d[i - 1][j] + 1).min(d[i][j - 1] + 1).min(d[i - 1][j - 1] + cost);
                if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] { d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1); }
            }
        }
        d[a.len()][b.len()]
    };
    known.iter().copied().filter(|k| *k != key).map(|k| (distance(&key, k), k))
        .filter(|(d, k)| *d <= if k.len() > 5 { 2 } else { 1 }).min().map(|(_, k)| k)
        // (Or the same with its words run together or plural: `loadafter`, `dependencies`.)
        .or_else(|| known.iter().copied().find(|k| k.replace('_', "") == key.replace('_', "") || key.starts_with(k.trim_end_matches('s')) && k.len() > 5))
}

/// Something of a `mod.toml` this TA Reborn reads past ([`manifest_notes`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManifestNote {
    /// The key, by its dotted path as written: `media.requires_reborn`, `theme.sides.arm.font`
    /// (a part that is no bare key quoted, as TOML writes it: `links."Home page"`; an item of a
    /// list by its place in it, from 0: `media.screenshots[1]`).
    pub key: String,
    /// The line it is written on, from 1 (an item of a list: the item's).
    pub line: usize,
    pub kind: ManifestNoteKind,
}

/// What a [`ManifestNote`] tells of its key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManifestNoteKind {
    /// A key this TA Reborn does not know (a later TA Reborn's, or a slip of the pen): read past,
    /// and kept as written when TA Reborn writes the manifest again ([`write_manifest`]).
    Unknown,
    /// A key this TA Reborn knows in another table, `belongs` (its dotted path: `""` the top
    /// level, `"theme"`, `"media"`), written in this one: TOML puts every key below a `[table]`
    /// line in that table, so the key is read as this table's (unknown to it, or a link's words
    /// under `[links]`), never as the one it was meant for. A key of the top level goes above the
    /// manifest's first `[table]` line; one of a table, under that table's own line.
    Misplaced { belongs: String },
    /// A value of another kind than its key takes (`expected`): left out, as if the key were not
    /// given. (In the keys that decide whether the mod is mounted, a wrong kind is the manifest's
    /// error instead, never a note: it does not read.)
    WrongKind { expected: ValueKind, found: ValueKind },
    /// One string where a list is wanted (`tags = "units"`): read as a list of that one. TA Reborn
    /// before this rule refused the whole manifest for it.
    OneForList,
}

/// The kinds of value TOML has, as a [`ManifestNote`] tells them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueKind {
    String,
    Integer,
    Float,
    Boolean,
    Datetime,
    /// A list (`[...]`).
    Array,
    /// A table (`[name]`, or `{ ... }`).
    Table,
}

fn kind_of(value: &Value) -> ValueKind {
    match value {
        Value::String(_) => ValueKind::String,
        Value::Integer(_) => ValueKind::Integer,
        Value::Float(_) => ValueKind::Float,
        Value::Boolean(_) => ValueKind::Boolean,
        Value::Datetime(_) => ValueKind::Datetime,
        Value::Array(_) => ValueKind::Array,
        Value::Table(_) => ValueKind::Table,
    }
}

/// A part of a key's path: a key, or an item of a list (by its place, from 0).
#[derive(Clone, Debug, PartialEq, Eq)]
enum Part {
    Key(String),
    Item(usize),
}

fn at(path: &[Part], part: Part) -> Vec<Part> {
    let mut path = path.to_vec();
    path.push(part);
    path
}

fn key_at(path: &[Part], key: &str) -> Vec<Part> {
    at(path, Part::Key(key.to_string()))
}

/// What a lenient reading met that it read past, when it is asked to tell ([`manifest_notes`]).
struct Notes(Option<Vec<(Vec<Part>, ManifestNoteKind)>>);

impl Notes {
    fn off() -> Notes {
        Notes(None)
    }

    fn add(&mut self, path: &[Part], kind: ManifestNoteKind) {
        if let Some(found) = &mut self.0 {
            found.push((path.to_vec(), kind));
        }
    }
}

fn wrong(expected: ValueKind, found: &Value) -> ManifestNoteKind {
    ManifestNoteKind::WrongKind { expected, found: kind_of(found) }
}

/// The tables a key may be meant for when it is written in another: each by its dotted path.
#[derive(Clone, Copy)]
enum Home {
    Top,
    Media,
    Theme,
}

impl Home {
    fn path(self) -> &'static str {
        match self {
            Home::Top => "",
            Home::Media => "media",
            Home::Theme => "theme",
        }
    }

    fn knows(self, key: &str) -> bool {
        match self {
            Home::Top => TOP_KEYS.contains(&key),
            Home::Media => MEDIA_KEYS.contains(&key),
            Home::Theme => look_key(key) || THEME_OWN.contains(&key),
        }
    }

    /// The first of `homes` that knows `key`, as a note's kind; else an unknown key.
    fn of(key: &str, homes: &[Home]) -> ManifestNoteKind {
        homes.iter().find(|home| home.knows(key))
            .map_or(ManifestNoteKind::Unknown, |home| ManifestNoteKind::Misplaced { belongs: home.path().to_string() })
    }
}

/// A key a table does not know, at `path`: one of `homes` knows it, or no table does.
fn stray(path: &[Part], key: &str, homes: &[Home], notes: &mut Notes) {
    notes.add(path, Home::of(key, homes));
}

/// A string's value; anything else is left out.
fn text(value: &Value, path: &[Part], notes: &mut Notes) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => {
            notes.add(path, wrong(ValueKind::String, other));
            String::new()
        }
    }
}

/// A list of strings: one string is a list of one; an item of a list that is no string is left
/// out, as anything else is.
fn texts(value: &Value, path: &[Part], notes: &mut Notes) -> Vec<String> {
    match value {
        Value::String(text) => {
            notes.add(path, ManifestNoteKind::OneForList);
            vec![text.clone()]
        }
        Value::Array(items) => items.iter().enumerate().filter_map(|(index, item)| match item {
            Value::String(text) => Some(text.clone()),
            other => {
                notes.add(&at(path, Part::Item(index)), wrong(ValueKind::String, other));
                None
            }
        }).collect(),
        other => {
            notes.add(path, wrong(ValueKind::Array, other));
            Vec::new()
        }
    }
}

fn media(value: &Value, path: &[Part], notes: &mut Notes) -> Media {
    let mut media = Media::default();
    let Value::Table(table) = value else {
        notes.add(path, wrong(ValueKind::Table, value));
        return media;
    };
    for (key, value) in table {
        let path = key_at(path, key);
        match key.as_str() {
            "banner" => media.banner = text(value, &path, notes),
            "preview" => media.preview = text(value, &path, notes),
            "icon" => media.icon = text(value, &path, notes),
            "screenshots" => media.screenshots = texts(value, &path, notes),
            _ => stray(&path, key, &[Home::Top, Home::Theme], notes),
        }
    }
    media
}

/// The links of `[links]`: each by its words, an address (an address that is no web address
/// stays, as written: the launcher opens none but web addresses). A key of the manifest's own
/// written there with something that is no web address is told as that.
fn links(value: &Value, path: &[Part], notes: &mut Notes) -> BTreeMap<String, String> {
    let mut links = BTreeMap::new();
    let Value::Table(table) = value else {
        notes.add(path, wrong(ValueKind::Table, value));
        return links;
    };
    for (words, value) in table {
        let path = key_at(path, words);
        let misplaced = Home::Top.knows(words) && !matches!(value, Value::String(address) if web_address(address));
        if misplaced {
            notes.add(&path, ManifestNoteKind::Misplaced { belongs: String::new() });
        }
        match value {
            Value::String(address) => {
                links.insert(words.clone(), address.clone());
            }
            other if !misplaced => notes.add(&path, wrong(ValueKind::String, other)),
            _ => {}
        }
    }
    links
}

/// A look's field of `key` (one of its colours or pictures).
fn look_field<'a>(look: &'a mut Look, key: &str) -> Option<&'a mut String> {
    Some(match key {
        "accent" => &mut look.accent,
        "accent_bright" => &mut look.accent_bright,
        "text" => &mut look.text,
        "text_bright" => &mut look.text_bright,
        "dim" => &mut look.dim,
        "panel" => &mut look.panel,
        "inset" => &mut look.inset,
        "line" => &mut look.line,
        "good" => &mut look.good,
        "bad" => &mut look.bad,
        "info" => &mut look.info,
        "button_text" => &mut look.button_text,
        "metal" => &mut look.metal,
        "energy" => &mut look.energy,
        "panel_texture" => &mut look.panel_texture,
        "bar_texture" => &mut look.bar_texture,
        _ => return None,
    })
}

/// A side's look (`[theme.sides.<side>]`).
fn look(value: &Value, path: &[Part], notes: &mut Notes) -> Look {
    let mut look = Look::default();
    let Value::Table(table) = value else {
        notes.add(path, wrong(ValueKind::Table, value));
        return look;
    };
    for (key, value) in table {
        let path = key_at(path, key);
        match look_field(&mut look, key) {
            Some(field) => *field = text(value, &path, notes),
            None => stray(&path, key, &[Home::Theme, Home::Top, Home::Media], notes),
        }
    }
    look
}

fn classic(value: &Value, path: &[Part], notes: &mut Notes) -> ClassicLook {
    let mut classic = ClassicLook::default();
    let Value::Table(table) = value else {
        notes.add(path, wrong(ValueKind::Table, value));
        return classic;
    };
    for (key, value) in table {
        let path = key_at(path, key);
        match key.as_str() {
            "text" => classic.text = text(value, &path, notes),
            "button_text" => classic.button_text = text(value, &path, notes),
            "dim" => classic.dim = text(value, &path, notes),
            "pressed" => classic.pressed = text(value, &path, notes),
            _ => stray(&path, key, &[Home::Theme, Home::Top, Home::Media], notes),
        }
    }
    classic
}

/// The sides' own looks, by the side's name; an entry that is no table is no side's (a key of the
/// theme's own written after `[theme.sides]` is told as that).
fn sides(value: &Value, path: &[Part], notes: &mut Notes) -> BTreeMap<String, Look> {
    let mut sides = BTreeMap::new();
    let Value::Table(table) = value else {
        notes.add(path, wrong(ValueKind::Table, value));
        return sides;
    };
    for (side, value) in table {
        let path = key_at(path, side);
        if value.is_table() {
            sides.insert(side.clone(), look(value, &path, notes));
            continue;
        }
        match Home::of(side, &[Home::Theme, Home::Top, Home::Media]) {
            ManifestNoteKind::Unknown => notes.add(&path, wrong(ValueKind::Table, value)),
            misplaced => notes.add(&path, misplaced),
        }
    }
    sides
}

fn theme(value: &Value, path: &[Part], notes: &mut Notes) -> Theme {
    let mut theme = Theme::default();
    let Value::Table(table) = value else {
        notes.add(path, wrong(ValueKind::Table, value));
        return theme;
    };
    for (key, value) in table {
        let path = key_at(path, key);
        if let Some(field) = look_field(&mut theme.look, key) {
            *field = text(value, &path, notes);
            continue;
        }
        match key.as_str() {
            "font" => theme.font = text(value, &path, notes),
            "classic" => theme.classic = classic(value, &path, notes),
            "sides" => theme.sides = sides(value, &path, notes),
            _ => stray(&path, key, &[Home::Top, Home::Media], notes),
        }
    }
    theme
}

fn top(key: &str) -> Vec<Part> {
    vec![Part::Key(key.to_string())]
}

/// `version`, `description` and `source`: a string, else nothing.
pub(super) fn loose_text<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Ok(text(&Value::deserialize(d)?, &[], &mut Notes::off()))
}

/// `authors` and `tags`: a list of strings (one string a list of one), what is not a string left out.
pub(super) fn loose_texts<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    Ok(texts(&Value::deserialize(d)?, &[], &mut Notes::off()))
}

/// `[links]`: its entries whose addresses are strings.
pub(super) fn loose_links<'de, D: Deserializer<'de>>(d: D) -> Result<BTreeMap<String, String>, D::Error> {
    Ok(links(&Value::deserialize(d)?, &top("links"), &mut Notes::off()))
}

impl<'de> Deserialize<'de> for Media {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(media(&Value::deserialize(d)?, &top("media"), &mut Notes::off()))
    }
}

impl<'de> Deserialize<'de> for Theme {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(theme(&Value::deserialize(d)?, &top("theme"), &mut Notes::off()))
    }
}

impl<'de> Deserialize<'de> for Look {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(look(&Value::deserialize(d)?, &[], &mut Notes::off()))
    }
}

impl<'de> Deserialize<'de> for ClassicLook {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(classic(&Value::deserialize(d)?, &[], &mut Notes::off()))
    }
}

/// Mod ids (or archives' file names) as read, in lower case: a list of strings, or one string (the
/// list of that one). Anything else does not read: these decide whether the mod is mounted.
pub(super) fn lower_case_list<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    struct Names;
    impl<'de> serde::de::Visitor<'de> for Names {
        type Value = Vec<String>;

        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a list of strings, or one string")
        }

        fn visit_str<E: serde::de::Error>(self, name: &str) -> Result<Vec<String>, E> {
            Ok(vec![name.to_lowercase()])
        }

        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<String>, A::Error> {
            let mut names = Vec::new();
            while let Some(name) = seq.next_element::<String>()? {
                names.push(name.to_lowercase());
            }
            Ok(names)
        }
    }
    d.deserialize_any(Names)
}

/// What the manifest `text` holds that this TA Reborn reads past, in the order it is written:
/// keys it does not know (those it knows in another table told as that), values of another kind
/// than their keys take (left out), and one string where a list is wanted (read as a list of
/// one). Nothing when `text` is not TOML: that is the manifest's error, and the mod is left out.
/// (A value of a wrong kind in a key that decides whether the mod is mounted, `depends = 5`, is
/// the manifest's error too: it does not read.)
pub fn manifest_notes(text: &str) -> Vec<ManifestNote> {
    let Ok(table) = toml::from_str::<toml::Table>(text) else { return Vec::new() };
    let mut notes = Notes(Some(Vec::new()));
    for (key, value) in &table {
        let path = top(key);
        match key.as_str() {
            // (Read strictly, or a string of their own: a wrong kind does not read.)
            "id" | "name" | "requires_reborn" | "rename" => {}
            "depends" | "load_after" | "conflicts" | "replaces" => {
                if value.is_str() {
                    notes.add(&path, ManifestNoteKind::OneForList);
                }
            }
            "version" | "description" | "source" => {
                self::text(value, &path, &mut notes);
            }
            "authors" | "tags" => {
                texts(value, &path, &mut notes);
            }
            "media" => {
                media(value, &path, &mut notes);
            }
            "links" => {
                links(value, &path, &mut notes);
            }
            "theme" => {
                theme(value, &path, &mut notes);
            }
            _ => stray(&path, key, &[Home::Media, Home::Theme], &mut notes),
        }
    }
    let document = toml_edit::Document::parse(text).ok();
    let mut found: Vec<ManifestNote> = notes.0.unwrap_or_default().into_iter()
        .map(|(path, kind)| ManifestNote { key: dotted(&path), line: document.as_ref().map_or(1, |document| line_of(document, &path)), kind })
        .collect();
    found.sort_by(|a, b| a.line.cmp(&b.line).then_with(|| a.key.cmp(&b.key)));
    found
}

/// A key's path as TOML writes it: its parts between dots, those that are no bare keys quoted, the
/// items of lists by their places (`links."Home page"`, `media.screenshots[1]`).
fn dotted(path: &[Part]) -> String {
    let mut out = String::new();
    for part in path {
        match part {
            Part::Key(key) => {
                if !out.is_empty() {
                    out.push('.');
                }
                if !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
                    out.push_str(key);
                } else {
                    out.push_str(&Value::String(key.clone()).to_string());
                }
            }
            Part::Item(index) => out.push_str(&format!("[{index}]")),
        }
    }
    out
}

/// The line (from 1) the key at `path` is written on in `document` (an item of a list: the item's).
fn line_of(document: &toml_edit::Document<&str>, path: &[Part]) -> usize {
    enum Node<'a> {
        Item(&'a toml_edit::Item),
        Value(&'a toml_edit::Value),
    }
    let mut node = Node::Item(document.as_item());
    let mut span = None;
    for part in path {
        let next = match (part, &node) {
            (Part::Key(key), Node::Item(item)) => item.as_table_like().and_then(|table| {
                span = table.key(key).and_then(toml_edit::Key::span).or(span.clone());
                table.get(key).map(Node::Item)
            }),
            (Part::Key(key), Node::Value(value)) => value.as_inline_table().and_then(|table| {
                span = toml_edit::TableLike::key(table, key).and_then(toml_edit::Key::span).or(span.clone());
                toml_edit::TableLike::get(table, key).map(Node::Item)
            }),
            (Part::Item(index), Node::Item(item)) => item.as_array().and_then(|list| list.get(*index)).map(|value| {
                span = value.span().or(span.clone());
                Node::Value(value)
            }),
            (Part::Item(index), Node::Value(value)) => value.as_array().and_then(|list| list.get(*index)).map(|value| {
                span = value.span().or(span.clone());
                Node::Value(value)
            }),
        };
        match next {
            Some(next) => node = next,
            None => break,
        }
    }
    span.map_or(1, |span| document.raw().get(..span.start).map_or(1, |before| before.matches('\n').count() + 1))
}

/// How [`write_manifest`] treats a table of the manifest: which of its keys are TA Reborn's (one
/// of those not written is removed; the others are kept as written), and what its keys hold.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Spec {
    Root,
    Media,
    Theme,
    Classic,
    /// `[theme.sides]`: each key a side's look.
    Sides,
    Look,
    /// `[links]`, `[rename]`: every key an entry.
    Map,
    /// A value of its own.
    Value,
}

impl Spec {
    fn keys(self) -> &'static [&'static [&'static str]] {
        match self {
            Spec::Root => &[TOP_KEYS],
            Spec::Media => &[MEDIA_KEYS],
            Spec::Theme => &[LOOK_COLOURS, LOOK_PICTURES, THEME_OWN],
            Spec::Classic => &[CLASSIC_KEYS],
            Spec::Look => &[LOOK_COLOURS, LOOK_PICTURES],
            Spec::Sides | Spec::Map | Spec::Value => &[],
        }
    }

    /// Whether `key` is TA Reborn's in this table.
    fn knows(self, key: &str) -> bool {
        matches!(self, Spec::Sides | Spec::Map) || self.keys().iter().any(|keys| keys.contains(&key))
    }

    /// Where `key` goes among those of the table, for one written anew (the order TA Reborn
    /// writes them in; entries by their names).
    fn rank(self, key: &str) -> usize {
        self.keys().iter().flat_map(|keys| keys.iter()).position(|known| *known == key).unwrap_or(usize::MAX)
    }

    fn child(self, key: &str) -> Spec {
        match (self, key) {
            (Spec::Root, "media") => Spec::Media,
            (Spec::Root, "theme") => Spec::Theme,
            (Spec::Root, "links" | "rename") => Spec::Map,
            (Spec::Theme, "classic") => Spec::Classic,
            (Spec::Theme, "sides") => Spec::Sides,
            (Spec::Sides, _) => Spec::Look,
            _ => Spec::Value,
        }
    }
}

/// An item of a document as a plain value (None: no value).
fn plain_item(item: &toml_edit::Item) -> Option<Value> {
    match item {
        toml_edit::Item::None => None,
        toml_edit::Item::Value(value) => Some(plain(value)),
        toml_edit::Item::Table(table) => Some(Value::Table(table.iter().filter_map(|(key, item)| Some((key.to_string(), plain_item(item)?))).collect())),
        toml_edit::Item::ArrayOfTables(tables) => Some(Value::Array(tables.iter()
            .map(|table| Value::Table(table.iter().filter_map(|(key, item)| Some((key.to_string(), plain_item(item)?))).collect()))
            .collect())),
    }
}

fn plain(value: &toml_edit::Value) -> Value {
    match value {
        toml_edit::Value::String(text) => Value::String(text.value().clone()),
        toml_edit::Value::Integer(number) => Value::Integer(*number.value()),
        toml_edit::Value::Float(number) => Value::Float(*number.value()),
        toml_edit::Value::Boolean(yes) => Value::Boolean(*yes.value()),
        toml_edit::Value::Datetime(when) => Value::Datetime(*when.value()),
        toml_edit::Value::Array(items) => Value::Array(items.iter().map(plain).collect()),
        toml_edit::Value::InlineTable(table) => Value::Table(table.iter().map(|(key, value)| (key.to_string(), plain(value))).collect()),
    }
}

/// A plain value as one of a document's, written as TOML writes it.
fn edit_value(value: &Value) -> toml_edit::Value {
    match value {
        Value::String(text) => text.as_str().into(),
        Value::Integer(number) => (*number).into(),
        Value::Float(number) => (*number).into(),
        Value::Boolean(yes) => (*yes).into(),
        Value::Datetime(when) => (*when).into(),
        Value::Array(items) => toml_edit::Value::Array(items.iter().map(edit_value).collect()),
        Value::Table(table) => {
            let mut inline = toml_edit::InlineTable::new();
            for (key, value) in table {
                inline.insert(key, edit_value(value));
            }
            toml_edit::Value::InlineTable(inline)
        }
    }
}

/// A plain value as an item of a document's table: a table under a table of its own as one of
/// its own (`[theme.sides.core]`, no line of its own for one holding only tables, as
/// `[theme.sides]`); in an inline table, a value.
fn edit_item(value: &Value, inline: bool) -> toml_edit::Item {
    match value {
        Value::Table(table) if !inline => {
            let mut new = toml_edit::Table::new();
            for (key, value) in table {
                new.insert(key, edit_item(value, false));
            }
            new.set_implicit(!table.is_empty() && table.values().all(Value::is_table));
            toml_edit::Item::Table(new)
        }
        other => toml_edit::Item::Value(edit_value(other)),
    }
}

/// Makes the key `key` of the document's `table` hold `target` (None: not given, the key removed),
/// keeping as written what of it holds that already: the keys of a table TA Reborn does not know,
/// and the comments and the layout of all it leaves as they were.
fn put(table: &mut dyn toml_edit::TableLike, key: &str, target: Option<&Value>, spec: Spec, inline: bool) {
    let Some(target) = target else {
        // (A table of TA Reborn's: its own keys go, and it with them, unless it holds keys TA
        // Reborn does not know.)
        if spec != Spec::Value && let Some(item) = table.get_mut(key) {
            let inner_inline = inline || item.as_inline_table().is_some();
            if let Some(inner) = item.as_table_like_mut() {
                merge(inner, &toml::Table::new(), spec, inner_inline);
                if !inner.is_empty() {
                    return;
                }
            }
        }
        table.remove(key);
        return;
    };
    let Some(item) = table.get_mut(key) else {
        if inline {
            insert_inline(table, key, edit_item(target, true));
        } else {
            table.insert(key, edit_item(target, false));
        }
        return;
    };
    if plain_item(item).as_ref() == Some(target) {
        return;
    }
    if let Value::Table(target) = target {
        let inner_inline = inline || item.as_inline_table().is_some();
        if let Some(inner) = item.as_table_like_mut() {
            merge(inner, target, spec, inner_inline);
            return;
        }
    }
    // (In its place, with the comments about its value.)
    let decor = item.as_value().map(|value| value.decor().clone());
    *item = edit_item(target, inline);
    if let (Some(decor), Some(value)) = (decor, item.as_value_mut()) {
        *value.decor_mut() = decor;
    }
}

/// A key put at the end of an inline table: the space before its closing brace, written after what
/// was its last value, goes after the new one (`{ a = 1, b = 2 }`, not `{ a = 1 , b = 2 }`).
fn insert_inline(table: &mut dyn toml_edit::TableLike, key: &str, item: toml_edit::Item) {
    if let Some((_, last)) = table.iter_mut().last()
        && let Some(value) = last.as_value_mut()
        && value.decor().suffix().and_then(toml_edit::RawString::as_str).is_some_and(|suffix| suffix.trim().is_empty())
    {
        value.decor_mut().set_suffix("");
    }
    table.insert(key, item);
}

/// Makes the document's `table` hold the table `target`: its keys of TA Reborn's that `target`
/// does not give removed, each it gives put ([`put`]), those TA Reborn does not know kept.
fn merge(table: &mut dyn toml_edit::TableLike, target: &toml::Table, spec: Spec, inline: bool) {
    let given: Vec<String> = table.iter().map(|(key, _)| key.to_string()).collect();
    for key in given.iter().filter(|key| spec.knows(key) && !target.contains_key(key.as_str())) {
        put(table, key, None, spec.child(key), inline);
    }
    let mut keys: Vec<&String> = target.keys().collect();
    keys.sort_by(|a, b| spec.rank(a).cmp(&spec.rank(b)).then_with(|| a.cmp(b)));
    for key in keys {
        put(table, key, target.get(key.as_str()), spec.child(key), inline);
    }
}

/// `manifest` as the text of its `mod.toml`, written over `base`, the file's text as it was (None:
/// a file of its own, as TOML writes it): each key of TA Reborn's whose value `manifest` changes
/// from what `base` reads as is written anew (in a table, only what of it changes), and the rest
/// of `base` is kept as written: keys this TA Reborn does not know (a later TA Reborn's survive
/// an older one's install), comments and layout. Err when `base` is not TOML.
pub fn write_manifest(base: Option<&str>, manifest: &ModManifest) -> Result<String, String> {
    let Some(base) = base else { return toml::to_string_pretty(manifest).map_err(|e| e.to_string()) };
    let mut document: toml_edit::DocumentMut = base.parse().map_err(|e: toml_edit::TomlError| e.to_string())?;
    let target = toml::Table::try_from(manifest).map_err(|e| e.to_string())?;
    // (What the file says now, as the game reads it: what it says already is left as written,
    // however it is written.)
    let was: Option<ModManifest> = toml::from_str(base).ok();
    let ModManifest { id, name, version, description, authors, depends, load_after, conflicts, requires_reborn, source, rename, replaces, tags, media, links, theme } = manifest;
    let same = |key: &str| -> bool {
        let Some(was) = &was else { return false };
        match key {
            "id" => was.id == *id,
            "name" => was.name == *name,
            "version" => was.version == *version,
            "description" => was.description == *description,
            "authors" => was.authors == *authors,
            "depends" => was.depends == *depends,
            "load_after" => was.load_after == *load_after,
            "conflicts" => was.conflicts == *conflicts,
            "requires_reborn" => was.requires_reborn == *requires_reborn,
            "source" => was.source == *source,
            "rename" => was.rename == *rename,
            "replaces" => was.replaces == *replaces,
            "tags" => was.tags == *tags,
            "media" => was.media == *media,
            "links" => was.links == *links,
            "theme" => was.theme == *theme,
            _ => false,
        }
    };
    for key in TOP_KEYS.iter().filter(|key| !same(key)) {
        put(document.as_table_mut(), key, target.get(*key), Spec::Root.child(key), false);
    }
    Ok(document.to_string())
}

#[cfg(test)]
mod tests {
    use super::closest_key;

    /// A mistyped key is told by the one it was likely meant to be, in its own table: and none
    /// where no known key is near, or where any key goes (`[links]`).
    #[test]
    fn a_mistyped_key_is_told_by_the_one_it_was_meant_to_be() {
        assert_eq!(closest_key("depend"), Some("depends"));
        assert_eq!(closest_key("dependencies"), Some("depends"));
        assert_eq!(closest_key("load-after"), Some("load_after"));
        assert_eq!(closest_key("requires_rebron"), Some("requires_reborn"));
        assert_eq!(closest_key("media.baner"), Some("banner"));
        assert_eq!(closest_key("theme.acent"), Some("accent"));
        assert_eq!(closest_key("theme.sides.arm.panle"), Some("panel"));
        assert_eq!(closest_key("theme.classic.txt"), Some("text"));
        assert_eq!(closest_key("homepage"), None);
        assert_eq!(closest_key("links.\"Home page\""), None);
    }

    use super::*;

    fn read(text: &str) -> ModManifest {
        toml::from_str(text).unwrap_or_else(|e| panic!("{e}\n{text}"))
    }

    fn notes(text: &str) -> Vec<(String, usize, ManifestNoteKind)> {
        manifest_notes(text).into_iter().map(|note| (note.key, note.line, note.kind)).collect()
    }

    fn misplaced(belongs: &str) -> ManifestNoteKind {
        ManifestNoteKind::Misplaced { belongs: belongs.to_string() }
    }

    /// A wrong kind in what only presents the mod is left out, the rest read: a mod is never left
    /// out for its screenshots, its tags or a colour written as a number. One string where a list
    /// is wanted is a list of one.
    #[test]
    fn a_wrong_kind_in_what_presents_a_mod_is_left_out_and_the_rest_read() {
        let manifest = read("id = \"m\"\nname = \"M\"\nversion = 1.2\ndescription = [\"a\"]\nauthors = \"A. Modder\"\nsource = 5\ntags = \"units\"\n\n[media]\nbanner = \"media/banner.png\"\nscreenshots = \"media/a.png\"\nicon = 7\n\n[links]\nDiscord = \"https://discord.gg/x\"\nBroken = 5\n\n[theme]\naccent = 0x5ab4ff\ntext = \"#dfe7ef\"\nfont = [\"x\"]\n\n[theme.classic]\ntext = 1\npressed = \"#ffffff\"\n\n[theme.sides]\narm = \"#123456\"\n\n[theme.sides.core]\naccent = \"#ce4834\"\nbar_texture = false\n");
        assert_eq!((manifest.version.as_str(), manifest.description.as_str(), manifest.source.as_str()), ("", "", ""));
        assert_eq!(manifest.authors, ["A. Modder"]);
        assert_eq!(manifest.tags, ["units"]);
        assert_eq!(manifest.media, Media { banner: "media/banner.png".into(), screenshots: vec!["media/a.png".into()], ..Default::default() });
        assert_eq!(manifest.links, BTreeMap::from([("Discord".to_string(), "https://discord.gg/x".to_string())]));
        assert_eq!((manifest.theme.look.accent.as_str(), manifest.theme.look.text.as_str(), manifest.theme.font.as_str()), ("", "#dfe7ef", ""));
        assert_eq!(manifest.theme.classic, ClassicLook { pressed: "#ffffff".into(), ..Default::default() });
        assert_eq!(manifest.theme.sides.keys().collect::<Vec<_>>(), ["core"], "a side that is no table is none");
        assert_eq!(manifest.theme.colour("core", "accent"), Some("#ce4834"));
        // An item of a list that is no string is left out, the others kept.
        let manifest = read("id = \"m\"\nname = \"M\"\ntags = [\"units\", 5, \"naval\"]\n[media]\nscreenshots = [\"a.png\", true, \"b.png\"]\n");
        assert_eq!((manifest.tags.as_slice(), manifest.media.screenshots.as_slice()), (["units".to_string(), "naval".to_string()].as_slice(), ["a.png".to_string(), "b.png".to_string()].as_slice()));
        // A table that is no table: left out.
        let manifest = read("id = \"m\"\nname = \"M\"\nmedia = \"banner.png\"\nlinks = [\"https://example.com\"]\ntheme = \"#5ab4ff\"\n");
        assert!(manifest.media.is_empty() && manifest.links.is_empty() && manifest.theme.is_empty());
    }

    /// What decides whether the mod is mounted is read strictly: a wrong kind there does not read
    /// (a mod must not mount without what it needs, or in a TA Reborn too old for it); a lone
    /// string for a list of ids is that one id.
    #[test]
    fn what_decides_mounting_is_read_strictly() {
        for wrong in ["depends = 5", "load_after = { a = 1 }", "conflicts = [1]", "requires_reborn = 1.1", "replaces = true", "[rename]\nguim = 5"] {
            let text = format!("id = \"m\"\nname = \"M\"\n{wrong}\n");
            assert!(toml::from_str::<ModManifest>(&text).is_err(), "{text}");
        }
        for wrong in ["id = 5\nname = \"M\"\n", "id = \"m\"\nname = [\"M\"]\n", "id = \"m\"\n", "name = \"M\"\n"] {
            assert!(toml::from_str::<ModManifest>(wrong).is_err(), "{wrong}");
        }
        let manifest = read("id = \"M\"\nname = \"M\"\ndepends = \"Base_Units\"\nload_after = \"X\"\nconflicts = \"Old\"\nreplaces = \"BTDATA.CCX\"\n");
        assert_eq!((manifest.depends, manifest.load_after, manifest.conflicts, manifest.replaces), (vec!["base_units".to_string()], vec!["x".to_string()], vec!["old".to_string()], vec!["btdata.ccx".to_string()]));
    }

    /// The notes name each key read past by its dotted path and line: one TA Reborn does not know,
    /// one of the top level written under a table (TOML's usual slip), one of `[theme]` written
    /// under a side, a wrong kind left out, one string for a list.
    #[test]
    fn the_notes_name_what_is_read_past_by_its_path_and_line() {
        let text = "id = \"m\"\nname = \"M\"\nhomepage = \"https://example.com\"\ntags = \"units\"\n\n[media]\nbanner = \"media/banner.png\"\nrequires_reborn = \"1.0.7\"\ndepends = [\"core\"]\nscreenshots = [\"a.png\", 5]\n\n[links]\n\"Home page\" = \"https://example.com\"\nconflicts = \"old-navy\"\nsource = \"https://example.com/mod\"\nBroken = 5\n\n[theme]\naccent = 0x5ab4ff\nrequires_reborn = \"1.0.7\"\nbanner = \"media/b.png\"\nsparkle = true\n\n[theme.sides.arm]\nfont = \"media/a.ttf\"\nshine = \"#fff\"\n\n[theme.sides]\naccent = \"#fff\"\n";
        assert_eq!(notes(text), [
            ("homepage".to_string(), 3, ManifestNoteKind::Unknown),
            ("tags".to_string(), 4, ManifestNoteKind::OneForList),
            ("media.requires_reborn".to_string(), 8, misplaced("")),
            ("media.depends".to_string(), 9, misplaced("")),
            ("media.screenshots[1]".to_string(), 10, ManifestNoteKind::WrongKind { expected: ValueKind::String, found: ValueKind::Integer }),
            ("links.conflicts".to_string(), 14, misplaced("")),
            ("links.Broken".to_string(), 16, ManifestNoteKind::WrongKind { expected: ValueKind::String, found: ValueKind::Integer }),
            ("theme.accent".to_string(), 19, ManifestNoteKind::WrongKind { expected: ValueKind::String, found: ValueKind::Integer }),
            ("theme.requires_reborn".to_string(), 20, misplaced("")),
            ("theme.banner".to_string(), 21, misplaced("media")),
            ("theme.sparkle".to_string(), 22, ManifestNoteKind::Unknown),
            ("theme.sides.arm.font".to_string(), 25, misplaced("theme")),
            ("theme.sides.arm.shine".to_string(), 26, ManifestNoteKind::Unknown),
            ("theme.sides.accent".to_string(), 29, misplaced("theme")),
        ]);
        // (A link of words like a key's, to a web address, is a link.)
        assert!(!notes(text).iter().any(|(key, ..)| key == "links.source"));
        // Keys of tables written at the top, a quoted key, an inline table, dotted keys.
        let text = "id = \"m\"\nname = \"M\"\nbanner = \"media/banner.png\"\naccent = \"#fff\"\nmedia.icon = 5\n\"odd key\" = 1\nlinks = { \"Home page\" = 5 }\n";
        assert_eq!(notes(text), [
            ("banner".to_string(), 3, misplaced("media")),
            ("accent".to_string(), 4, misplaced("theme")),
            ("media.icon".to_string(), 5, ManifestNoteKind::WrongKind { expected: ValueKind::String, found: ValueKind::Integer }),
            ("\"odd key\"".to_string(), 6, ManifestNoteKind::Unknown),
            ("links.\"Home page\"".to_string(), 7, ManifestNoteKind::WrongKind { expected: ValueKind::String, found: ValueKind::Integer }),
        ]);
        // A manifest that reads as it is meant: nothing to tell; one that is not TOML: its error, not notes.
        assert!(manifest_notes("id = \"m\"\nname = \"M\"\nversion = \"1\"\ndepends = [\"a\"]\n[rename]\nguim = \"guis\"\n[media]\nbanner = \"b.png\"\n[links]\nDiscord = \"https://discord.gg/x\"\n[theme]\naccent = \"#fff\"\n[theme.classic]\ntext = \"#fff\"\n[theme.sides.core]\naccent = \"#fff\"\n").is_empty());
        assert!(manifest_notes("not TOML").is_empty());
        // (Strict keys of a wrong kind are the manifest's error: no note of theirs.)
        assert!(manifest_notes("id = \"m\"\nname = \"M\"\ndepends = 5\n").is_empty());
    }

    /// Every note the lenient reading takes into account matches what it read: a key told as left
    /// out reads as not given, one told as one for a list reads as that list of one.
    #[test]
    fn what_the_notes_tell_is_what_is_read() {
        let text = "id = \"m\"\nname = \"M\"\nversion = 2\nauthors = \"A\"\n[media]\nicon = 5\n[theme]\nfont = 1\n[theme.sides.core]\naccent = 2\n";
        let manifest = read(text);
        assert_eq!((manifest.version.as_str(), manifest.authors.as_slice(), manifest.media.icon.as_str(), manifest.theme.font.as_str()), ("", ["A".to_string()].as_slice(), "", ""));
        assert_eq!(manifest.theme.side("core"), Some(&Look::default()));
        let keys: Vec<String> = manifest_notes(text).into_iter().map(|note| note.key).collect();
        assert_eq!(keys, ["version", "authors", "media.icon", "theme.font", "theme.sides.core.accent"]);
    }

    /// A manifest written over the file it was keeps what it does not change as written: keys TA
    /// Reborn does not know (a later version's), comments, layout, and the form of a value that
    /// reads the same; what changes is written in place, in a table only what of it changes.
    #[test]
    fn a_manifest_written_over_its_file_keeps_what_it_does_not_change() {
        let base = "# My mod, by me.\nid = \"MyMod\"   # its identity\nname = \"My Mod\"\nversion = \"1.0\"\nauthors = \"A. Modder\"\nfuture_key = { since = \"2.0\" }\n\n[media]\nbanner = \"media/banner.png\" # 1280 x 640\nsparkle = true\n\n[theme]\naccent = \"#5ab4ff\"\n# the sides\n[theme.sides.core]\naccent = \"#ce4834\"\n";
        let mut manifest = read(base);
        assert_eq!(write_manifest(Some(base), &manifest).unwrap(), base, "nothing changed: as it was, to the byte");
        manifest.id = "other".into();
        manifest.name = "Other \"Mod\"".into();
        manifest.rename.insert("guim".into(), "guis".into());
        manifest.replaces = vec!["btdata.ccx".into()];
        manifest.media.icon = "media/icon.png".into();
        manifest.theme.sides.insert("arm".into(), Look { accent: "#00ff00".into(), ..Default::default() });
        let written = write_manifest(Some(base), &manifest).unwrap();
        assert_eq!(read(&written), manifest, "{written}");
        for kept in ["# My mod, by me.", "# its identity", "authors = \"A. Modder\"", "future_key = { since = \"2.0\" }", "# 1280 x 640", "sparkle = true", "# the sides"] {
            assert!(written.contains(kept), "{kept} kept:\n{written}");
        }
        assert!(written.starts_with("# My mod, by me.\nid = \"other\"   # its identity\nname = "), "{written}");
        assert!(written.contains("[rename]\nguim = \"guis\"") && written.contains("replaces = [\"btdata.ccx\"]") && written.contains("[theme.sides.arm]"), "{written}");
        assert!(!written.contains("[theme.sides]\n"), "no line of its own for a table of tables: {written}");
        // What is no longer given goes; what TA Reborn does not know stays.
        manifest.media = Media::default();
        manifest.theme = Theme::default();
        let written = write_manifest(Some(&written), &manifest).unwrap();
        assert_eq!(read(&written), manifest, "{written}");
        assert!(written.contains("sparkle = true") && !written.contains("banner") && !written.contains("accent"), "{written}");
        // A new file: as TOML writes the manifest; a base that is no TOML: an error.
        assert_eq!(write_manifest(None, &manifest).unwrap(), toml::to_string_pretty(&manifest).unwrap());
        assert!(write_manifest(Some("not TOML"), &manifest).is_err());
    }

    /// A key written in a form the game reads leniently (one string for a list) is left as written
    /// while it says what the manifest says; one TA Reborn changes is written as TOML writes it.
    #[test]
    fn a_manifest_written_over_a_lenient_file_changes_only_what_differs() {
        let base = "id = \"m\"\nname = \"M\"\ntags = \"units\"\nlinks = { Discord = \"https://discord.gg/x\" }\n";
        let mut manifest = read(base);
        manifest.links.insert("Forum".into(), "https://example.com".into());
        let written = write_manifest(Some(base), &manifest).unwrap();
        assert!(written.contains("tags = \"units\""), "{written}");
        assert!(written.contains("links = { Discord = \"https://discord.gg/x\", Forum = \"https://example.com\" }"), "{written}");
        assert_eq!(read(&written), manifest);
        manifest.tags.push("naval".into());
        let written = write_manifest(Some(&written), &manifest).unwrap();
        assert!(written.contains("tags = [\"units\", \"naval\"]"), "{written}");
    }
}
