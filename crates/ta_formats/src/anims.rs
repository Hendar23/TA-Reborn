//! A model's animations (`objects3d/<name>.anims`, beside its 3DO): keyframed movements of its
//! pieces that TA Reborn plays on the screen alone (the simulation never sees them, as it never
//! sees how its walkers' legs are posed by the ground they cover). Each is a **clip**, named for
//! what it is played for:
//!
//! - `walk`: a cycle, its last frame leading back into its first, played as the unit walks, in
//!   step with the ground it covers (as TA's walk scripts are: apps/ta_game's `scene/walk.rs`),
//!   over whatever its script does to the other pieces;
//! - `death`: played where the unit dies, its wreck shown once it ends (more than one: one of them
//!   at random, `death`, `death2`...).
//!
//! The models TA Reborn makes of Supreme Commander's units have them (crates/ta_supcom, from the
//! game's `.sca` animations); docs/MODDING.md tells mod makers how to give their models some.
//!
//! A clip's keys are at its frames' times (seconds from its start, rising), each piece it moves
//! held as a TA script holds a piece: a move from where the piece stands in the model, along x, y
//! and z in 65,536ths of a pixel (x negated, as scripts give it), and a turn about x, y and z in
//! TA's angle units (65,536 a whole turn; z negated), applied about y, then x, then z, as the
//! engine composes a script's (docs/FORMATS.md). Between keys a piece moves in a straight line
//! and turns the short way.
//!
//! Laid out little-endian:
//! - `TAANIMS`, a zero byte, the version (1), the number of clips (u32s);
//! - each clip: its name (a u16 length and that many bytes of UTF-8), its length in seconds (f32),
//!   the number of pieces it moves and of its frames (u32s); the pieces, by their index in the
//!   3DO's order (as [`Model3do::parse`](crate::model3do::Model3do::parse) lists them, u32s); the
//!   frames' times (f32s); then each frame's keys, piece by piece in that order, each its move
//!   and its turn (six i32s).

use crate::{FormatError, Result};

const MAGIC: &[u8; 8] = b"TAANIMS\0";
const VERSION: u32 = 1;
/// The most clips, pieces or frames an animation file may claim.
const MAX_ITEMS: u32 = 1 << 16;

/// A piece's place in one frame: its move (65,536ths of a pixel, x negated) and its turn (TA's
/// angle units, z negated) from where it stands in the model.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Key {
    pub moved: [i32; 3],
    pub turned: [i32; 3],
}

/// A clip: what it is played for, how long it lasts, the pieces it moves and their keys.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Clip {
    pub name: String,
    pub seconds: f32,
    /// The pieces it moves, by their index in the model.
    pub pieces: Vec<u32>,
    /// Its frames' times (seconds from its start, rising).
    pub times: Vec<f32>,
    /// Each frame's keys, a key for each of [`Clip::pieces`] in order (`times.len() × pieces.len()`).
    pub keys: Vec<Key>,
}

impl Clip {
    /// The keys of frame `frame`, piece by piece.
    pub fn frame(&self, frame: usize) -> &[Key] {
        let n = self.pieces.len();
        &self.keys[frame * n..(frame + 1) * n]
    }

    /// Each moved piece's move and turn `at` seconds into the clip (a cycle wraps round from its
    /// last frame to its first over the time left to its length; otherwise it holds its ends), in
    /// [`Clip::pieces`]' order: moves straight, turns the short way, as fractions of their units.
    pub fn sample(&self, at: f32, cycle: bool, out: &mut Vec<[f32; 6]>) {
        out.clear();
        let n = self.pieces.len();
        if self.times.is_empty() {
            out.resize(n, [0.0; 6]);
            return;
        }
        let at = if cycle && self.seconds > 0.0 { at.rem_euclid(self.seconds) } else { at.clamp(0.0, self.seconds.max(0.0)) };
        let last = self.times.len() - 1;
        let k = self.times.partition_point(|&t| t <= at);
        // The keys either side, and how far between them.
        let (a, b, f) = if k == 0 {
            if cycle && self.seconds > self.times[last] {
                let span = self.seconds - self.times[last] + self.times[0];
                (last, 0, ((at + self.seconds - self.times[last]) / span).clamp(0.0, 1.0))
            } else {
                (0, 0, 0.0)
            }
        } else if k > last {
            if cycle && self.seconds > self.times[last] {
                let span = self.seconds - self.times[last] + self.times[0];
                (last, 0, ((at - self.times[last]) / span).clamp(0.0, 1.0))
            } else {
                (last, last, 0.0)
            }
        } else {
            let (t0, t1) = (self.times[k - 1], self.times[k]);
            (k - 1, k, if t1 > t0 { ((at - t0) / (t1 - t0)).clamp(0.0, 1.0) } else { 0.0 })
        };
        let (ka, kb) = (self.frame(a), self.frame(b));
        out.extend(ka.iter().zip(kb).map(|(x, y)| {
            let mut v = [0.0f32; 6];
            for c in 0..3 {
                v[c] = x.moved[c] as f32 + (y.moved[c] as f32 - x.moved[c] as f32) * f;
                let turn = y.turned[c].wrapping_sub(x.turned[c]) as i16 as f32;
                v[c + 3] = x.turned[c] as f32 + turn * f;
            }
            v
        }));
    }
}

/// A model's animations.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Anims {
    pub clips: Vec<Clip>,
}

impl Anims {
    /// The clip named `name` (any case).
    pub fn clip(&self, name: &str) -> Option<&Clip> {
        self.clips.iter().find(|c| c.name.eq_ignore_ascii_case(name))
    }

    /// The death clips (`death`, `death2`...). (Their names are read as bytes: a clip named in any
    /// script, `歩く`, is no death, and no trouble.)
    pub fn deaths(&self) -> impl Iterator<Item = &Clip> {
        self.clips.iter().filter(|c| {
            let name = c.name.as_bytes();
            name.len() >= 5 && name[..5].eq_ignore_ascii_case(b"death") && name[5..].iter().all(u8::is_ascii_digit)
        })
    }

    /// Whether it fits a model of `pieces` pieces: every clip moves pieces it has, each once, and
    /// holds a key for each at each frame, at times rising within its length.
    pub fn fits(&self, pieces: usize) -> bool {
        self.clips.iter().all(|c| {
            let mut seen = vec![false; pieces];
            c.pieces.iter().all(|&p| (p as usize) < pieces && !std::mem::replace(&mut seen[p as usize], true))
                && c.keys.len() == c.times.len() * c.pieces.len()
                && c.times.windows(2).all(|w| w[0] <= w[1])
                && c.times.iter().all(|t| t.is_finite() && *t >= 0.0)
        })
    }

    pub fn parse(b: &[u8]) -> Result<Anims> {
        let mut r = Reader { b, at: 0 };
        if r.take(8)? != MAGIC {
            return Err(FormatError::invalid("anims", "not an animation file"));
        }
        let version = r.u32()?;
        if version != VERSION {
            return Err(FormatError::invalid("anims", format!("version {version} ({VERSION} is read)")));
        }
        let count = r.u32()?;
        if count > MAX_ITEMS {
            return Err(FormatError::invalid("anims", format!("{count} clips")));
        }
        let mut clips = Vec::with_capacity(count.min(64) as usize);
        for _ in 0..count {
            let name = r.string()?;
            let seconds = r.f32()?;
            let (pieces, frames) = (r.u32()?, r.u32()?);
            let keys = (pieces as usize).saturating_mul(frames as usize);
            if pieces > MAX_ITEMS || frames > MAX_ITEMS || keys.saturating_mul(24) > b.len().saturating_sub(r.at) || !seconds.is_finite() || seconds < 0.0 {
                return Err(FormatError::invalid("anims", format!("a clip of {pieces} pieces and {frames} frames, {seconds} s")));
            }
            let pieces = (0..pieces).map(|_| r.u32()).collect::<Result<Vec<u32>>>()?;
            let times = (0..frames).map(|_| r.f32()).collect::<Result<Vec<f32>>>()?;
            let keys = (0..keys)
                .map(|_| {
                    let v = r.take(24)?;
                    let i = |k: usize| i32::from_le_bytes([v[4 * k], v[4 * k + 1], v[4 * k + 2], v[4 * k + 3]]);
                    Ok(Key { moved: [i(0), i(1), i(2)], turned: [i(3), i(4), i(5)] })
                })
                .collect::<Result<Vec<Key>>>()?;
            clips.push(Clip { name, seconds, pieces, times, keys });
        }
        Ok(Anims { clips })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        for v in [VERSION, self.clips.len() as u32] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        for c in &self.clips {
            let bytes = c.name.as_bytes();
            let len = bytes.len().min(u16::MAX as usize);
            out.extend_from_slice(&(len as u16).to_le_bytes());
            out.extend_from_slice(&bytes[..len]);
            out.extend_from_slice(&c.seconds.to_le_bytes());
            out.extend_from_slice(&(c.pieces.len() as u32).to_le_bytes());
            out.extend_from_slice(&(c.times.len() as u32).to_le_bytes());
            for p in &c.pieces {
                out.extend_from_slice(&p.to_le_bytes());
            }
            for t in &c.times {
                out.extend_from_slice(&t.to_le_bytes());
            }
            for k in &c.keys {
                for v in k.moved.iter().chain(&k.turned) {
                    out.extend_from_slice(&v.to_le_bytes());
                }
            }
        }
        out
    }
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.at.checked_add(n).filter(|&e| e <= self.b.len()).ok_or(FormatError::Eof { offset: self.at, needed: n })?;
        let s = &self.b[self.at..end];
        self.at = end;
        Ok(s)
    }

    fn u32(&mut self) -> Result<u32> {
        let s = self.take(4)?;
        Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }

    fn f32(&mut self) -> Result<f32> {
        self.u32().map(f32::from_bits)
    }

    fn string(&mut self) -> Result<String> {
        let len = self.take(2)?;
        let len = u16::from_le_bytes([len[0], len[1]]) as usize;
        String::from_utf8(self.take(len)?.to_vec()).map_err(|_| FormatError::invalid("anims", "a name not in UTF-8"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Anims {
        let key = |m: i32, t: i32| Key { moved: [m, 0, -m], turned: [0, t, 0] };
        Anims {
            clips: vec![
                Clip { name: "walk".into(), seconds: 1.0, pieces: vec![2, 0], times: vec![0.0, 0.5], keys: vec![key(0, 0), key(65536, 30000), key(131072, 32000), key(0, -30000)] },
                Clip { name: "death".into(), seconds: 2.0, pieces: vec![1], times: vec![0.0, 2.0], keys: vec![key(0, 0), key(0, 16384)] },
                Clip { name: "Death2".into(), ..Default::default() },
                Clip { name: "歩く".into(), ..Default::default() },
                Clip { name: "deathé".into(), ..Default::default() },
            ],
        }
    }

    #[test]
    fn animations_read_back_as_written() {
        let a = sample();
        let back = Anims::parse(&a.to_bytes()).unwrap();
        assert_eq!(back, a);
        assert!(back.fits(3) && !back.fits(2), "the walk moves piece 2");
        assert_eq!(back.clip("WALK").map(|c| c.pieces.len()), Some(2));
        assert_eq!(back.deaths().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["death", "Death2"], "names in other scripts are no deaths");
    }

    #[test]
    fn a_clip_is_sampled_between_its_keys_and_round_its_cycle() {
        let a = sample();
        let walk = a.clip("walk").unwrap();
        let mut out = Vec::new();
        walk.sample(0.25, true, &mut out);
        assert_eq!(out[0], [65536.0, 0.0, -65536.0, 0.0, 16000.0, 0.0], "half way to the second key");
        // Turns go the short way: 30,000 to -30,000 through a half turn, not through none.
        assert_eq!(out[1][4], 32768.0);
        // From the last key (0.5 s) back round to the first, over the half second left.
        walk.sample(0.75, true, &mut out);
        assert_eq!((out[0][0], out[0][4]), (65536.0, 16000.0));
        assert_eq!(out[1][4], -32768.0);
        walk.sample(1.25, true, &mut out);
        assert_eq!(out[0][4], 16000.0, "a second on is the same");
        // Not a cycle: it holds its last key.
        let death = a.clip("death").unwrap();
        death.sample(5.0, false, &mut out);
        assert_eq!(out[0][4], 16384.0);
        death.sample(1.0, false, &mut out);
        assert_eq!(out[0][4], 8192.0);
    }

    #[test]
    fn a_broken_animation_file_is_refused() {
        let bytes = sample().to_bytes();
        assert!(Anims::parse(&bytes[..bytes.len() - 1]).is_err(), "cut short");
        assert!(Anims::parse(b"TASURF\0\0").is_err());
        let mut huge = bytes.clone();
        huge[16 + 2 + 4 + 4..16 + 2 + 4 + 8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(Anims::parse(&huge).is_err(), "pieces claimed past the file");
    }
}
