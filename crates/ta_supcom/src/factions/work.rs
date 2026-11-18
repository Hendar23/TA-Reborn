//! What a building does as it works, made for TA Reborn (docs/SUPCOM_UNITS.md): the clips its
//! model plays (`active`, `open`, `upgrade`: ta_formats' `anims`, played by the game as the unit
//! works) and the parts its script spins and slides (TA's own `spin` and `move`, in the simulation:
//! [`super::script`]), from Forged Alliance's classes and the unit's own script
//! ([`crate::motions`]).
//!
//! Forged Alliance's classes play some of a building's animations themselves, whatever its script:
//!
//! - every structure plays its `AnimationUpgrade` as it upgrades, its frame the share of the
//!   upgrade done (`SetAnimationFraction`), the building it becomes hidden until it is done
//!   (`StructureUnit`'s `UpgradingState`): its `upgrade` clip;
//! - every unit plays its `AnimationPermOpen` once, as it is finished, and holds it: its `open`;
//! - the Cybran land and air factories play their `AnimationBuild` round and round while they
//!   build (`CLandFactoryUnit`, `CAirFactoryUnit`): their `active`;
//! - the Seraphim shield generators play their `AnimationActivate` once as their shield comes
//!   up (`SShieldStructureUnit`): their `open`, the shields being always up.
//!
//! Its script plays the rest: an extractor pumps once it is finished (`PlayAnim(...,
//! true)`: `active`), stopping where it is when paused, a Seraphim hydrocarbon plant at half
//! speed (`SetRate(0.5)`), a fabricator opens before it spins (`PlayAnim(...)`: `open`).
//!
//! The parts its script spins and slides become its TA script's, each about the model's axis its
//! bone's turns about (a piece's axes are the model's: [`super::model`]). Turned by a clip as well
//! as its script, a part takes both. A clip that would turn a part a weapon aims (or anything that
//! carries one) is not made: the turret unfolding before it aims (the Aeon's point defences) would
//! aim from where it was. Mobile units are left as they were.

use crate::motions::{Spin, When};
use crate::scm::{self, Mesh};
use crate::unit::{Motion, UnitBlueprint};

/// A clip to make: what it is played for, the animation it is made of (the game's path), how fast
/// it plays (its times divided by it), and whether it is a cycle.
#[derive(Clone, Debug, PartialEq)]
pub struct Source {
    pub name: &'static str,
    pub path: String,
    pub rate: f64,
    pub cycle: bool,
}

/// The clips a unit's model plays as it works (see the module's notes): the first its script
/// plays round and round (`active`) and once (`open`), else its classes'; its upgrade's, where it
/// `upgrades` in place. A structure's only.
pub fn clips(u: &UnitBlueprint, upgrades: bool) -> Vec<Source> {
    let mut out: Vec<Source> = Vec::new();
    if u.motion != Motion::None {
        return out;
    }
    let path = |animation: &str| -> Option<String> {
        if animation.ends_with(".sca") { Some(animation.to_string()) } else { u.display.animation(animation).map(str::to_string) }
    };
    let mut put = |name: &'static str, path: Option<String>, rate: f64, cycle: bool| {
        if let Some(path) = path
            && !out.iter().any(|s| s.name == name)
            && rate > 0.0
        {
            out.push(Source { name, path, rate, cycle });
        }
    };
    for p in &u.motions.plays {
        put(if p.looped { ta_formats::anims::ACTIVE } else { ta_formats::anims::OPEN }, path(&p.animation), p.rate, p.looped);
    }
    if matches!(u.motions.base.as_str(), "CLandFactoryUnit" | "CAirFactoryUnit") && u.is("FACTORY") {
        put(ta_formats::anims::ACTIVE, path("AnimationBuild"), 1.0, true);
    }
    put(ta_formats::anims::OPEN, path("AnimationPermOpen"), 1.0, false);
    if u.motions.base == "SShieldStructureUnit" {
        put(ta_formats::anims::OPEN, path("AnimationActivate"), 1.0, false);
    }
    if upgrades {
        put(ta_formats::anims::UPGRADE, path("AnimationUpgrade"), 1.0, false);
    }
    out
}

/// Whether a clip turns a part one of the unit's weapons aims, or one carrying it (`pieces`: each
/// model piece's bone): such a clip is not made (see the module's notes).
pub fn turns_an_aim(u: &UnitBlueprint, mesh: &Mesh, pieces: &[usize], clip: &ta_formats::anims::Clip) -> bool {
    // (The aimed bones and everything over them.)
    let mut aimed = vec![false; mesh.bones.len()];
    for w in &u.weapons {
        for name in [&w.yaw_bone, &w.pitch_bone].into_iter().flatten() {
            let mut b = mesh.bone(name);
            while let Some(k) = b {
                aimed[k] = true;
                b = mesh.bones[k].parent;
            }
        }
    }
    clip.pieces.iter().enumerate().any(|(k, &p)| {
        pieces.get(p as usize).is_some_and(|&b| aimed[b]) && (0..clip.times.len()).any(|f| clip.frame(f)[k].turned.iter().any(|t| t.abs() > TURNED))
    })
}

/// A turn (TA's angle units) a clip may give a part without moving it to speak of: about 1°.
const TURNED: i32 = 182;

/// TA's angle units a turn.
const TURN: f64 = 65536.0;

/// A spin as the unit's TA script makes it: the piece, and about each of its axes (0 x, 1 y, 2 z)
/// how fast (angle units a second, signed as scripts turn) and how fast it gathers speed (angle
/// units a second each frame, 30 a second; 0 at once).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spinning {
    pub when: When,
    pub piece: u32,
    pub axes: Vec<(u32, i32, i32)>,
    /// It swings to and fro: how long each half of a swing gathers speed and loses it again
    /// (milliseconds), and the longest pause at each end.
    pub sway: Option<(i32, i32)>,
}

/// A slide as the unit's TA script makes it (moves in 65,536ths of a pixel, x negated as scripts
/// move): where the piece is put as the unit is made, the places it goes, and how fast along each
/// axis (65,536ths of a pixel a second; 0 at once) as it goes and as it goes back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sliding {
    pub when: When,
    pub piece: u32,
    pub made: [i32; 3],
    pub goals: Vec<[i32; 3]>,
    pub shuttle: bool,
    pub speed: [i32; 3],
    pub back: [i32; 3],
}

/// What the unit's TA script spins and slides.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Motions {
    pub spins: Vec<Spinning>,
    pub slides: Vec<Sliding>,
    /// What could not be made so (a spin about an axis none of the model's: a bone named that the
    /// mesh has not), for the record.
    pub left_out: Vec<String>,
}

impl Motions {
    pub fn any(&self, when: When) -> bool {
        self.spins.iter().any(|s| s.when == when) || self.slides.iter().any(|s| s.when == when)
    }
}

/// A move under this (65,536ths of a pixel: a thousandth of one) is none.
const ROUNDING: i32 = 66;

/// The share of an axis a bone's axis must lie along to be taken as that axis alone.
const ALONG: f64 = 0.99;
/// The least share of an axis a swing's axis is taken to turn about (a swing about a slanted axis
/// is made of turns about the model's axes, each by its share: close for the swings' few tens of
/// degrees, as the guns' pitches are made of turns about x and z: [`super::script::RestAim`]).
const SHARE: f64 = 0.05;

/// The unit's spins and slides (its script's: [`crate::motions`]) as its TA script makes them, for
/// its model of `pieces` (each piece's name, in the model's order) of `mesh`, its mesh units
/// `pixels` pixels. A structure's only.
pub fn motions(u: &UnitBlueprint, mesh: &Mesh, pieces: &[String], pixels: f64) -> Motions {
    let mut out = Motions::default();
    if u.motion != Motion::None {
        return out;
    }
    let rest = mesh.rest_poses();
    let found = |bone: &str| -> Option<(u32, usize)> {
        let piece = pieces.iter().position(|p| p.eq_ignore_ascii_case(bone))? as u32;
        Some((piece, mesh.bone(bone)?))
    };
    for s in &u.motions.spins {
        let Some((piece, b)) = found(&s.bone) else {
            out.left_out.push(format!("a spin of {}, not its mesh's", s.bone));
            continue;
        };
        match spinning(s, piece, model_axis(&rest[b].1, s.axis)) {
            Some(spin) => out.spins.push(spin),
            None => out.left_out.push(format!("{}'s spin about its {} axis, along none of the model's", s.bone, ["x", "y", "z"][s.axis])),
        }
    }
    for s in &u.motions.slides {
        // (A factory's pad moved is where its units are built: left where the simulation has it.)
        if s.bone.starts_with('@') {
            out.left_out.push(format!("a slide of its {}", &s.bone[1..]));
            continue;
        }
        let Some((piece, b)) = found(&s.bone) else {
            out.left_out.push(format!("a slide of {}, not its mesh's", s.bone));
            continue;
        };
        let turn = rest[b].1;
        let place = |v: [f64; 3]| -> [i32; 3] {
            let m = scm::rotate(turn, [v[0] as f32, v[1] as f32, v[2] as f32]);
            // (What the bone's turn leaves along another axis by its rounding, under a thousandth
            // of a pixel, is none.)
            let fixed = |x: f64| {
                let v = (x * pixels * 65536.0).round().clamp(i32::MIN as f64, i32::MAX as f64) as i32;
                if v.abs() < ROUNDING { 0 } else { v }
            };
            [-fixed(m[0] as f64), fixed(m[1] as f64), fixed(m[2] as f64)]
        };
        let made = s.made.map_or([0; 3], place);
        let goals: Vec<[i32; 3]> = s.goals.iter().map(|&g| place(g)).collect();
        // Along each axis as fast as it goes there along the way it goes (its first leg: from where
        // it is made to its first place, or between the places it shuttles between).
        let leg = match goals.as_slice() {
            [a, b, ..] if s.shuttle => [b[0] as f64 - a[0] as f64, b[1] as f64 - a[1] as f64, b[2] as f64 - a[2] as f64],
            [a, ..] => [a[0] as f64 - made[0] as f64, a[1] as f64 - made[1] as f64, a[2] as f64 - made[2] as f64],
            [] => [0.0; 3],
        };
        let length = (leg[0] * leg[0] + leg[1] * leg[1] + leg[2] * leg[2]).sqrt();
        let along = |speed: f64| -> [i32; 3] {
            std::array::from_fn(|k| {
                if speed <= 0.0 || length <= 0.0 || leg[k] == 0.0 {
                    return 0;
                }
                ((speed * pixels * 65536.0 * leg[k].abs() / length).round() as i64).clamp(1, i32::MAX as i64) as i32
            })
        };
        out.slides.push(Sliding { when: s.when, piece, made, goals, shuttle: s.shuttle, speed: along(s.speed), back: along(s.back) });
    }
    out
}

/// The model's axis (`[x, y, z]`) a bone resting turned by `turn` turns about when it turns about
/// its own `axis`.
fn model_axis(turn: &[f32; 4], axis: usize) -> [f64; 3] {
    let mut v = [0.0f32; 3];
    v[axis] = 1.0;
    let m = scm::rotate(*turn, v);
    [m[0] as f64, m[1] as f64, m[2] as f64]
}

/// A spin about the model's `axis` (Forged Alliance's sense: the right hand's) as the script's:
/// about the one axis it lies along, or (a swing) the axes it has a share of. TA's scripts turn
/// about x and y as the model's turn, about z the other way round (docs/FORMATS.md).
fn spinning(s: &Spin, piece: u32, axis: [f64; 3]) -> Option<Spinning> {
    let units = |degrees: f64| degrees * TURN / 360.0;
    let along = (0..3).find(|&k| axis[k].abs() >= ALONG);
    let shares: Vec<(usize, f64)> = match along {
        Some(k) => vec![(k, axis[k].signum())],
        None if s.sway.is_some() => (0..3).filter(|&k| axis[k].abs() >= SHARE).map(|k| (k, axis[k])).collect(),
        None => return None,
    };
    // (A swing without an acceleration of its own gathers speed over a second.)
    let accel = if s.accel > 0.0 { s.accel } else if s.sway.is_some() { s.speed.abs() } else { 0.0 };
    let axes = shares
        .iter()
        .map(|&(k, share)| {
            let sense = if k == 2 { -1.0 } else { 1.0 };
            let speed = units(s.speed * share * sense).round() as i32;
            let per_frame = units(accel * share.abs()) / 30.0;
            let accel = if accel > 0.0 { (per_frame.round() as i32).max(1) } else { 0 };
            (k as u32, speed, accel)
        })
        .filter(|&(_, speed, _)| speed != 0)
        .collect::<Vec<_>>();
    if axes.is_empty() {
        return None;
    }
    let sway = s.sway.map(|pause| (((s.speed.abs() / accel) * 1000.0).round() as i32, (pause * 1000.0).round() as i32));
    Some(Spinning { when: s.when, piece, axes, sway })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::motions::{Motions as Read, Play, Slide};
    use crate::unit::Display;

    fn building(read: Read, animations: &[(&str, &str)]) -> UnitBlueprint {
        UnitBlueprint {
            id: "PLANT".into(),
            categories: vec!["STRUCTURE".into()],
            display: Display { uniform_scale: 0.05, animations: animations.iter().map(|(k, p)| (k.to_string(), p.to_string())).collect(), ..Display::default() },
            motions: read,
            ..UnitBlueprint::default()
        }
    }

    /// The clips a building plays: its script's loop (at its rate) and opening, its classes'
    /// upgrade when it upgrades, an opening held from its finishing; a mobile unit none.
    #[test]
    fn a_buildings_clips_come_from_its_script_and_its_classes() {
        let read = Read {
            plays: vec![
                Play { animation: "AnimationPump".into(), looped: true, rate: 0.5, when: When::Working },
                Play { animation: "units/x/x_hatch.sca".into(), looped: false, rate: 1.0, when: When::Working },
            ],
            ..Read::default()
        };
        let u = building(read.clone(), &[("AnimationPump", "units/x/x_pump.sca"), ("AnimationUpgrade", "units/x/x_up.sca"), ("AnimationPermOpen", "units/x/x_perm.sca")]);
        let names = |c: Vec<Source>| c.iter().map(|s| (s.name, s.path.clone(), s.rate, s.cycle)).collect::<Vec<_>>();
        assert_eq!(names(clips(&u, true)), [
            ("active", "units/x/x_pump.sca".to_string(), 0.5, true),
            ("open", "units/x/x_hatch.sca".to_string(), 1.0, false),
            ("upgrade", "units/x/x_up.sca".to_string(), 1.0, false),
        ], "the script's opening before the class's");
        assert!(!clips(&u, false).iter().any(|s| s.name == "upgrade"), "no upgrade where it upgrades into nothing");
        let held = building(Read::default(), &[("AnimationPermOpen", "units/x/x_perm.sca")]);
        assert_eq!(names(clips(&held, false)), [("open", "units/x/x_perm.sca".to_string(), 1.0, false)]);
        let walker = UnitBlueprint { motion: Motion::Land, ..u };
        assert!(clips(&walker, true).is_empty());
    }

    /// A mesh: a base, a mast standing straight, and a dish on it resting turned a quarter about
    /// x (its own z the model's up) and a flap resting turned a twelfth about y (its own x slanting).
    fn mesh() -> Mesh {
        let still = [1.0, 0.0, 0.0, 0.0];
        let s = std::f32::consts::FRAC_1_SQRT_2;
        let (c, n) = ((std::f32::consts::PI / 12.0).cos(), (std::f32::consts::PI / 12.0).sin());
        let file = scm::tests::write(
            &[("Base", -1, [0.0; 3], still), ("Mast", 0, [0.0, 2.0, 0.0], still), ("Dish", 1, [0.0, 1.0, 0.0], [s, -s, 0.0, 0.0]), ("Flap", 0, [1.0, 0.0, 0.0], [c, 0.0, n, 0.0])],
            &[([0.0; 3], [0.0; 2], 0)],
            &[[0, 0, 0]],
        );
        scm::parse(&file).unwrap()
    }

    /// Spins about the model's axes as a script turns: a mast about y as Forged Alliance turns it,
    /// a dish about its own z (the model's up), a swing about a slanted axis made of turns about
    /// x and z; a continuous spin about a slanted axis is left out.
    #[test]
    fn spins_turn_about_the_models_axes() {
        let spin = |bone: &str, axis: usize, speed: f64, sway: Option<f64>| Spin { bone: bone.into(), axis, speed, accel: 30.0, when: When::Working, sway };
        let read = Read { spins: vec![spin("Mast", 1, 90.0, None), spin("Dish", 2, 45.0, None), spin("Flap", 0, -15.0, Some(1.0)), spin("Flap", 0, 60.0, None), spin("Nowhere", 1, 10.0, None)], ..Read::default() };
        let u = building(read, &[]);
        let m = mesh();
        let pieces: Vec<String> = m.bones.iter().map(|b| b.name.clone()).collect();
        let made = motions(&u, &m, &pieces, 1.0);
        // 90° a second, 16,384 angle units; 30° a second gathered each second, 182 a frame.
        assert_eq!(made.spins[0], Spinning { when: When::Working, piece: 1, axes: vec![(1, 16384, 182)], sway: None });
        // The dish's z is the model's up (a quarter turn about x the other way): about y.
        assert_eq!(made.spins[1].axes, [(1, 8192, 182)]);
        // The flap's x slants: 15° of its axis's share of x and of z (z turned the other way).
        let swing = &made.spins[2];
        assert_eq!(swing.sway, Some((500, 1000)), "half a second to its speed and back");
        assert_eq!(swing.axes.iter().map(|a| a.0).collect::<Vec<_>>(), [0, 2]);
        // (Its x the model's turned a twelfth about y: 0.866 of x and -0.5 of z.)
        let (x, z) = (swing.axes[0].1 as f64, swing.axes[1].1 as f64);
        assert!((x - -2730.67 * 0.866).abs() < 2.0 && (z - -2730.67 * 0.5).abs() < 2.0, "{x} {z}");
        assert_eq!((swing.axes[0].2, swing.axes[1].2), (158, 91));
        assert_eq!(made.spins.len(), 3);
        assert_eq!(made.left_out.len(), 2, "{:?}", made.left_out);
        // Slides: along the bone's own axes, in the script's units (x negated), as fast along each
        // axis as along the way; a factory's pad moved left where it is.
        let slide = |bone: &str, made: Option<[f64; 3]>, goals: Vec<[f64; 3]>| Slide { bone: bone.into(), made, goals, shuttle: false, speed: 2.0, back: 4.0, when: When::Working };
        let read = Read { slides: vec![slide("Mast", Some([0.0, -1.0, 0.0]), vec![[0.0, 0.0, 0.0]]), slide("Dish", None, vec![[0.0, 0.0, 2.0]]), slide("@BuildAttachBone", None, vec![[1.0, 0.0, 0.0]])], ..Read::default() };
        let made = motions(&building(read, &[]), &m, &pieces, 2.0);
        assert_eq!(made.slides[0], Sliding { when: When::Working, piece: 1, made: [0, -131072, 0], goals: vec![[0; 3]], shuttle: false, speed: [0, 262144, 0], back: [0, 524288, 0] });
        // The dish's z is the model's up: 2 up.
        assert_eq!(made.slides[1].goals, [[0, 262144, 0]]);
        assert_eq!(made.slides.len(), 2);
    }
}
