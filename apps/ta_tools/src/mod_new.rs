//! `ta-tools mod-new <folder>`: a new mod's folder, ready to fill. Its `mod.toml` names it and
//! writes out every other key a mod may have, commented, as docs/MODDING.md tells them; its
//! `media` folder is for the launcher's pictures and the interface theme's; a read-me tells what
//! goes where and how to try it.

use std::path::Path;

use anyhow::{Context, Result};

/// Makes the mod's folder `dir` (new, or empty), named `name` (its folder's name by default), its
/// id `id` (made from the name by default).
pub fn run(dir: &Path, id: Option<String>, name: Option<String>) -> Result<()> {
    if dir.exists() && std::fs::read_dir(dir).with_context(|| dir.display().to_string())?.next().is_some() {
        anyhow::bail!("{} is not empty: give a new folder for the mod", dir.display());
    }
    let folder = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "My Mod".into());
    let name = name.unwrap_or(folder);
    let id = id.unwrap_or_else(|| id_of(&name)).to_lowercase();
    anyhow::ensure!(ta_vfs::mods::valid_id(&id), "the id {id:?} is not one a mod may have: 1 to 64 letters, digits, - or _, not base or reborn (give one with --id)");
    std::fs::create_dir_all(dir.join("media")).with_context(|| dir.display().to_string())?;
    std::fs::write(dir.join("mod.toml"), manifest(&id, &name))?;
    std::fs::write(dir.join("README.md"), readme(&name))?;
    // (What it was given reads as the game reads it.)
    let read = ta_vfs::mods::read_mod(dir).map_err(|e| anyhow::anyhow!("{e}"))?.context("the new mod does not read")?;
    println!("Made {} ({}) in {}", read.manifest.name, read.manifest.id, dir.display());
    println!("Put it in one of TA Reborn's mod folders (the launcher's MODS page, MOD FOLDERS...) and press RESCAN.");
    Ok(())
}

/// An id made from a name: its letters and digits in lower case, other runs of characters one
/// hyphen (`"My Naval Mod 2"` is `my-naval-mod-2`).
fn id_of(name: &str) -> String {
    let mut id = String::new();
    for c in name.chars() {
        if c.is_alphanumeric() {
            id.extend(c.to_lowercase());
        } else if !id.is_empty() && !id.ends_with('-') {
            id.push('-');
        }
    }
    let id = id.trim_end_matches('-').chars().take(64).collect::<String>();
    if id.is_empty() { "my-mod".into() } else { id }
}

fn manifest(id: &str, name: &str) -> String {
    let quoted = |s: &str| toml::Value::String(s.to_string()).to_string();
    format!(r##"# {name}: a TA Reborn mod. Every key is in docs/MODDING.md ("The mod.toml manifest").
id = {id}                          # its identity: keep it from version to version
name = {name_q}
version = "0.1"
description = ""
authors = []
# depends = ["other-mod"]          # mods that must be on too; they load before this one
# load_after = ["other-mod"]       # mods this one loads after, when they are on
# conflicts = ["other-mod"]        # mods that must not be on with this one
# requires_reborn = "{current}"     # the oldest TA Reborn it plays in
# tags = ["units"]                 # words the launcher's library finds it by
# source = "https://example.org"   # its web page

# How the launcher presents it ("Presenting a mod in the launcher"): pictures in its media folder.
# [media]
# banner = "media/banner.png"          # 1280 x 640: its card, and the top of its page
# preview = "media/gameplay.gif"       # an animated GIF of it in play, a few seconds
# icon = "media/icon.png"              # 256 x 256
# screenshots = ["media/shot1.png"]

# [links]                              # links on its page, by their words
# Discord = "https://discord.gg/example"

# The colours its battles' HUD is drawn in ("Theming the interface").
# [theme]
# accent = "#5ab4ff"
# text = "#dfe7ef"
# panel = "#18202b"
# panel_texture = "media/hud/panel.png"
# bar_texture = "media/hud/bar.png"
# font = "media/hud/font.ttf"
"##, id = quoted(id), name_q = quoted(name), current = ta_vfs::mods::REBORN_VERSION)
}

fn readme(name: &str) -> String {
    format!(r#"# {name}

A TA Reborn mod. Its files go in Total Annihilation's own folders, beside this read-me:

- `units/` (unit definitions, `.fbi`), `objects3d/` (models, `.3do`), `scripts/` (`.cob`),
  `weapons/` and `gamedata/` (`.tdf`), `anims/` (`.gaf`), `sounds/` (`.wav`), `maps/`
  (`.tnt` and `.ota`), `guis/`, `unitpics/`, `textures/`, `features/`...
- or TA archives (`.ufo`, `.hpi`, `.ccx`, `.gpf`, `.gp3`) holding those folders;
- `media/`: the launcher's pictures of the mod and its interface theme's (see `mod.toml`).

To try it, put this folder in one of TA Reborn's mod folders (the launcher's **MODS** page,
**MOD FOLDERS…**), press **RESCAN**, turn it on and **PLAY**. **MOD INFO** checks it, as
`ta-tools validate-mod <this folder>` does.

Making mods: https://github.com/Jared-woodruff/TA-Reborn/blob/main/docs/MODDING.md
"#)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A new mod's folder reads as the game reads mods: its id made from its name, its keys
    /// those it was given, its media folder there; a folder holding anything is never written in.
    #[test]
    fn a_new_mod_reads_as_the_game_reads_mods() {
        let root = std::env::temp_dir().join(format!("ta_mod_new_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dir = root.join("Naval Additions");
        run(&dir, None, None).unwrap();
        let read = ta_vfs::mods::read_mod(&dir).unwrap().unwrap();
        assert_eq!((read.manifest.id.as_str(), read.manifest.name.as_str(), read.manifest.version.as_str()), ("naval-additions", "Naval Additions", "0.1"));
        assert!(read.manifest.media.is_empty() && read.manifest.theme.is_empty(), "the rest is commented");
        assert!(dir.join("media").is_dir() && dir.join("README.md").is_file());
        assert!(run(&dir, None, None).is_err(), "never over a folder holding anything");
        run(&root.join("x"), Some("My_Id".into()), Some("A \"quoted\" name".into())).unwrap();
        let read = ta_vfs::mods::read_mod(&root.join("x")).unwrap().unwrap();
        assert_eq!((read.manifest.id.as_str(), read.manifest.name.as_str()), ("my_id", "A \"quoted\" name"));
        assert!(run(&root.join("y"), Some("base".into()), None).is_err(), "a reserved id");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_id_is_made_from_a_name() {
        assert_eq!(id_of("My Naval Mod 2"), "my-naval-mod-2");
        assert_eq!(id_of("  TA: Escalation!  "), "ta-escalation");
        assert_eq!(id_of("!!!"), "my-mod");
    }
}
