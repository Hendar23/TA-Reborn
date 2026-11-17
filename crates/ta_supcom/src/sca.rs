//! Supreme Commander's animations (`.sca`, version 5): a unit's walk, death or build movement as
//! keyframes of its skeleton's bones. Laid out as the files are, little-endian
//! (docs/SUPCOM_UNITS.md has the table):
//!
//! - the header: `ANIM`, the version (5), the number of frames, the length in seconds, the number
//!   of bones, and where the bones' names, their parents and the first frame are, and how long a
//!   frame is;
//! - the bones' names, NUL-ended (after a `NAME` mark), and each bone's parent, an i32 (-1 for the
//!   root; after a `LINK` mark);
//! - the root's place and turn at the start (a place, then a quaternion, w first);
//! - the frames: each its time (seconds) and flags, then every bone's place and turn relative to
//!   its parent.
//!
//! The bones are named as the mesh's are, not necessarily in its order: an animation is matched
//! to a mesh by name.

use crate::{Error, Result};

/// The most frames or bones an animation may claim (the game's longest has 900 frames).
const MAX_ITEMS: u32 = 100_000;

/// A bone's place and turn relative to its parent in one frame (a unit quaternion, `[w, x, y, z]`).
pub type Key = ([f32; 3], [f32; 4]);

/// One frame: its time in seconds, its flags, and a key per bone (in the animation's order).
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pub time: f32,
    pub flags: u32,
    pub keys: Vec<Key>,
}

/// An animation: its length in seconds, its bones (names and parents), where the root starts, and
/// its frames in order of time.
#[derive(Clone, Debug, PartialEq)]
pub struct Animation {
    pub duration: f32,
    pub bones: Vec<String>,
    pub parents: Vec<Option<usize>>,
    pub root: Key,
    pub frames: Vec<Frame>,
}

impl Animation {
    /// The animation's bone named `name` (any case).
    pub fn bone(&self, name: &str) -> Option<usize> {
        self.bones.iter().position(|b| b.eq_ignore_ascii_case(name))
    }
}

/// Reads an animation.
pub fn parse(b: &[u8]) -> Result<Animation> {
    let u32_at = |at: usize| -> Result<u32> { b.get(at..at + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap())).ok_or_else(|| bad(&format!("ends early (at byte {at})"))) };
    let f32_at = |at: usize| -> Result<f32> { u32_at(at).map(f32::from_bits) };
    if b.len() < 36 || &b[0..4] != b"ANIM" {
        return Err(bad("not an animation"));
    }
    let version = u32_at(4)?;
    if version != 5 {
        return Err(bad(&format!("version {version} (5 is read)")));
    }
    let frame_count = u32_at(8)?;
    let duration = f32_at(12)?;
    let bone_count = u32_at(16)?;
    let (names_at, links_at, first_at, frame_size) = (u32_at(20)? as usize, u32_at(24)? as usize, u32_at(28)? as usize, u32_at(32)? as usize);
    if bone_count == 0 || bone_count > MAX_ITEMS || frame_count > MAX_ITEMS {
        return Err(bad(&format!("{frame_count} frames of {bone_count} bones")));
    }
    let bones_n = bone_count as usize;
    if frame_size != 8 + bones_n * 28 {
        return Err(bad(&format!("frames of {frame_size} bytes for {bones_n} bones")));
    }
    // (Its frames must be in the file before room is made for them.)
    if first_at.checked_add(28).and_then(|at| at.checked_add((frame_count as usize).saturating_mul(frame_size))).is_none_or(|end| end > b.len()) {
        return Err(bad(&format!("{frame_count} frames claimed, past the end of its {} bytes", b.len())));
    }
    // The names: NUL-ended, one after another from `names_at` (any `NAME` mark before them).
    let mut bones = Vec::with_capacity(bones_n);
    let mut at = names_at;
    if b.get(at..at + 4) == Some(b"NAME") {
        at += 4;
    }
    for _ in 0..bones_n {
        let rest = b.get(at..).ok_or_else(|| bad("its names run past the end"))?;
        let len = rest.iter().take(256).position(|&c| c == 0).ok_or_else(|| bad("a name without its end"))?;
        bones.push(String::from_utf8_lossy(&rest[..len]).into_owned());
        at += len + 1;
    }
    let mut parents = Vec::with_capacity(bones_n);
    for (i, name) in bones.iter().enumerate() {
        let p = u32_at(links_at + i * 4)? as i32;
        parents.push(match p {
            -1 => None,
            p if p >= 0 && (p as usize) < bones_n && p as usize != i => Some(p as usize),
            p => return Err(bad(&format!("bone {i} ({name}) has parent {p}"))),
        });
    }
    let key_at = |at: usize| -> Result<Key> { Ok(([f32_at(at)?, f32_at(at + 4)?, f32_at(at + 8)?], [f32_at(at + 12)?, f32_at(at + 16)?, f32_at(at + 20)?, f32_at(at + 24)?])) };
    let root = key_at(first_at)?;
    let mut frames = Vec::with_capacity(frame_count as usize);
    for f in 0..frame_count as usize {
        let at = first_at + 28 + f * frame_size;
        let time = f32_at(at)?;
        let flags = u32_at(at + 4)?;
        let keys = (0..bones_n).map(|k| key_at(at + 8 + k * 28)).collect::<Result<Vec<Key>>>()?;
        if frames.last().is_some_and(|last: &Frame| time < last.time) {
            return Err(bad(&format!("frame {f} goes back in time")));
        }
        frames.push(Frame { time, flags, keys });
    }
    Ok(Animation { duration, bones, parents, root, frames })
}

fn bad(what: &str) -> Error {
    Error::Format(format!("sca: {what}"))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// An animation file as the game writes one: `bones` (name, parent) and `frames` (time, then
    /// a key per bone).
    pub(crate) fn write(bones: &[(&str, i32)], frames: &[(f32, Vec<Key>)], duration: f32) -> Vec<u8> {
        let mut out = vec![0u8; 36];
        while !out.len().is_multiple_of(32) {
            out.push(0);
        }
        // (As the game's: each mark just before where the header points.)
        out.extend_from_slice(b"NAME");
        let names_at = out.len();
        for (name, _) in bones {
            out.extend_from_slice(name.as_bytes());
            out.push(0);
        }
        while !out.len().is_multiple_of(4) {
            out.push(0xC5);
        }
        out.extend_from_slice(b"LINK");
        let links_at = out.len();
        for (_, parent) in bones {
            out.extend_from_slice(&parent.to_le_bytes());
        }
        let first_at = out.len();
        for v in [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        for (time, keys) in frames {
            out.extend_from_slice(&time.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes());
            for (place, turn) in keys {
                for v in place.iter().chain(turn.iter()) {
                    out.extend_from_slice(&v.to_le_bytes());
                }
            }
        }
        let frame_size = 8 + bones.len() as u32 * 28;
        let header = [5u32, frames.len() as u32, duration.to_bits(), bones.len() as u32, names_at as u32, links_at as u32, first_at as u32, frame_size];
        out[0..4].copy_from_slice(b"ANIM");
        for (k, v) in header.iter().enumerate() {
            out[4 + k * 4..8 + k * 4].copy_from_slice(&v.to_le_bytes());
        }
        out
    }

    const STILL: [f32; 4] = [1.0, 0.0, 0.0, 0.0];

    /// A two-legged walk: the body bobbing, a leg swinging forward and back.
    pub(crate) fn walk() -> Vec<u8> {
        let swing = [0.966, 0.259, 0.0, 0.0];
        write(
            &[("Body", -1), ("Leg", 0)],
            &[
                (0.0, vec![([0.0, 10.0, 0.0], STILL), ([1.0, -2.0, 0.0], STILL)]),
                (0.5, vec![([0.0, 10.5, 0.0], STILL), ([1.0, -2.0, 0.0], swing)]),
                (1.0, vec![([0.0, 10.0, 0.0], STILL), ([1.0, -2.0, 0.0], STILL)]),
            ],
            1.0,
        )
    }

    #[test]
    fn an_animation_reads_its_bones_and_frames() {
        let anim = parse(&walk()).unwrap();
        assert_eq!(anim.bones, ["Body", "Leg"]);
        assert_eq!(anim.parents, [None, Some(0)]);
        assert_eq!(anim.duration, 1.0);
        assert_eq!(anim.frames.len(), 3);
        assert_eq!(anim.frames[1].time, 0.5);
        assert_eq!(anim.frames[1].keys[0].0, [0.0, 10.5, 0.0]);
        assert_eq!(anim.frames[1].keys[1].1, [0.966, 0.259, 0.0, 0.0]);
        assert_eq!(anim.bone("leg"), Some(1));
    }

    #[test]
    fn a_broken_animation_is_refused_not_misread() {
        let good = walk();
        assert!(parse(&good[..30]).is_err(), "cut short");
        assert!(parse(&good[..good.len() - 4]).is_err(), "its last frame cut");
        let mut other = good.clone();
        other[4] = 4;
        assert!(parse(&other).is_err(), "another version");
        let backwards = write(&[("Body", -1)], &[(0.5, vec![([0.0; 3], STILL)]), (0.25, vec![([0.0; 3], STILL)])], 1.0);
        assert!(parse(&backwards).is_err(), "time going back");
        let own_parent = write(&[("Body", 0)], &[], 1.0);
        assert!(parse(&own_parent).is_err(), "a bone its own parent");
        // Ninety thousand frames claimed in a short file: refused before room is made for them.
        let mut claims = good.clone();
        claims[8..12].copy_from_slice(&90_000u32.to_le_bytes());
        assert!(parse(&claims).unwrap_err().to_string().contains("past the end"));
    }
}
