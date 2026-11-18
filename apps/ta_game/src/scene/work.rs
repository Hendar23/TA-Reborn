//! Buildings at work (for the eye alone): the clips a model has for its work (`objects3d/<name>
//! .anims`, ta_formats' `anims`: the Supreme Commander buildings' own, made from their game's
//! animations, or a mod's), played over what the unit's script does with its pieces:
//!
//! * `active` plays round and round while the unit works ([`works`]): finished, switched on or
//!   with no switch, its upkeep paid, neither stunned nor carried (`Sim::working`), not upgrading
//!   or installing anything, and (a factory or a constructor) building something. As it stops
//!   working the clip slows to a stop where it is, over [`EASE`], and goes on from there when it
//!   starts again; as it starts upgrading the clip fades out over [`FADE`] (Forged Alliance stops
//!   an extractor's pump to play its upgrade), and back in if the upgrade is called off. A unit
//!   first seen at work is somewhere in its cycle (by its slot), so that a field of extractors
//!   put down at once does not pump in step; one first seen idle (a frame being built) is at its
//!   cycle's start, where its model rests, and sets off from there.
//! * `open` plays forward as the unit starts working, holding at its end, and back as it stops.
//! * `upgrade` is posed by how far the unit's upgrade in place has come (the simulation's share of
//!   the work done, between its ticks), faded in over [`FADE`] as it starts (and out if it is
//!   called off). Done, the unit it becomes takes its place, standing as its model rests, and
//!   starts its own work: the old one's last pose is the morph Forged Alliance shows before the
//!   swap.
//!
//! A clip's moves and turns are added to the script's (a part the script spins goes on spinning
//! as a clip lifts it), its turns taken to run on from frame to frame (`Clip::unwrap_turns`) so
//! that a share of one, fading, never jumps half a turn. Nothing here feeds back into the game.

use ta_formats::anims::{ACTIVE, Anims, Clip, OPEN, UPGRADE};
use ta_sim::{SelfJob, Sim, Unit, UnitType};

use super::PieceState;

/// Seconds a loop takes to slow to a stop as its unit stops working, and to come up to its pace.
pub(super) const EASE: f32 = 0.6;
/// Seconds a loop or an upgrade takes to fade in or out.
pub(super) const FADE: f32 = 0.4;

/// One of a model's work clips: the clip (its turns run on), and the channels it moves (its
/// piece's place in the clip, and 0..3 moves along x, y, z, 3..6 turns about them).
pub(super) struct Layer {
    clip: Clip,
    channels: Vec<(usize, usize)>,
}

impl Layer {
    fn of(clip: &Clip) -> Option<Layer> {
        if clip.times.is_empty() || clip.seconds <= 0.0 && clip.times.len() < 2 {
            return None;
        }
        let mut clip = clip.clone();
        clip.unwrap_turns();
        let value = |f: usize, k: usize, ch: usize| {
            let key = clip.frame(f)[k];
            if ch < 3 { key.moved[ch] } else { key.turned[ch - 3] }
        };
        let channels: Vec<(usize, usize)> = (0..clip.pieces.len()).flat_map(|k| (0..6).map(move |ch| (k, ch))).filter(|&(k, ch)| (0..clip.times.len()).any(|f| value(f, k, ch) != 0)).collect();
        (!channels.is_empty()).then_some(Layer { clip, channels })
    }

    /// The clip's moves and turns `at` seconds in, `weight` of them, added to `states` (the
    /// script's, at the tick before and this one).
    fn add(&self, at: f32, cycle: bool, weight: f32, keys: &mut Vec<[f32; 6]>, states: [&mut [PieceState]; 2]) {
        self.clip.sample(at, cycle, keys);
        for states in states {
            for &(k, ch) in &self.channels {
                let Some(s) = states.get_mut(self.clip.pieces[k] as usize) else { continue };
                let v = (keys[k][ch] * weight).round();
                if ch < 3 {
                    s.0[ch] = s.0[ch].saturating_add(v as i32);
                } else {
                    // (Turns wrap round, as the script's do.)
                    s.1[ch - 3] = (s.1[ch - 3] as i64 + v as i64) as i16 as i32;
                }
            }
        }
    }
}

/// A model's work clips (see the module's notes).
pub(super) struct Clips {
    active: Option<Layer>,
    open: Option<Layer>,
    upgrade: Option<Layer>,
}

impl Clips {
    /// The work clips among a model's animations, if it has any (that move something).
    pub(super) fn of(anims: &Anims) -> Option<Clips> {
        let layer = |name: &str| anims.clip(name).and_then(Layer::of);
        let clips = Clips { active: layer(ACTIVE), open: layer(OPEN), upgrade: layer(UPGRADE) };
        (clips.active.is_some() || clips.open.is_some() || clips.upgrade.is_some()).then_some(clips)
    }
}

/// Whether a unit works, as its work clips have it (see the module's notes).
pub(super) fn works(sim: &Sim, u: &Unit, t: &UnitType) -> bool {
    sim.working(u) && u.self_work.is_none() && (!(t.is_factory || t.def.builder) || u.build.is_some())
}

/// A unit's work clips as they play: what the game says of it at each tick, and where each clip is.
#[derive(Default)]
pub(super) struct Worker {
    /// At the last tick: whether it works, how far its upgrade has come at the tick before and this
    /// one (none: not upgrading), and where its loop starts when it is first seen working (a share
    /// of its cycle).
    working: bool,
    upgrade: [Option<f32>; 2],
    start: f32,
    /// When it was last posed (game seconds).
    last: Option<f64>,
    /// `active`: seconds into its cycle, its pace (0 stopped, 1 its own), how far it is shown.
    phase: f32,
    pace: f32,
    active: f32,
    /// `open`: seconds into it.
    open: f32,
    /// `upgrade`: how far it is shown, and the share of the upgrade it was last posed at (held as
    /// it fades out, the upgrade called off).
    shown: f32,
    share: f32,
    keys: Vec<[f32; 6]>,
    before: Vec<PieceState>,
    now: Vec<PieceState>,
}

impl Worker {
    /// What the game says of unit `u` at its tick (`jump`: the game went somewhere else, or the
    /// unit is new: nothing to go on from).
    pub(super) fn tick(&mut self, sim: &Sim, u: &Unit, t: &UnitType, jump: bool) {
        self.working = works(sim, u, t);
        let upgrade = match sim.self_work_progress(u) {
            Some((SelfJob::Upgrade(_), done)) => Some(done.to_f32().clamp(0.0, 1.0)),
            _ => None,
        };
        self.upgrade = [if jump { upgrade } else { self.upgrade[1] }, upgrade];
        // (Somewhere in its cycle by its slot: the golden ratio's spread.)
        self.start = (u.id.slot as f32 * 0.618_034).fract();
    }

    /// The script states to pose the unit with this frame, `time` game seconds in and `alpha` of
    /// the way to the next tick, given its script's (or its walk's) at the tick before and this
    /// one: with its work clips added where they play.
    pub(super) fn states<'a>(&'a mut self, clips: &Clips, time: f64, alpha: f32, prev: &'a [PieceState], cur: &'a [PieceState]) -> (&'a [PieceState], &'a [PieceState]) {
        let since = self.last.filter(|&t| time >= t && time - t < 1.0).map(|t| (time - t) as f32);
        self.last = Some(time);
        let upgrading = self.upgrade[1].is_some();
        let (pace, active, shown) = (if self.working { 1.0 } else { 0.0 }, if upgrading { 0.0 } else { 1.0 }, if upgrading { 1.0 } else { 0.0 });
        let open = clips.open.as_ref().map_or(0.0, |o| o.clip.seconds);
        let cycle = clips.active.as_ref().map_or(0.0, |a| a.clip.seconds);
        match since {
            // Seen again after a while (or the game went back): as it is now; at work somewhere in
            // its loop, else at its start (its model at rest, as it is built).
            None => {
                (self.pace, self.active, self.shown) = (pace, active, shown);
                self.open = if self.working { open } else { 0.0 };
                self.phase = if self.working { self.start * cycle } else { 0.0 };
            }
            Some(dt) => {
                let approach = |x: f32, to: f32, step: f32| if to > x { (x + step).min(to) } else { (x - step).max(to) };
                self.pace = approach(self.pace, pace, dt / EASE);
                self.active = approach(self.active, active, dt / FADE);
                self.shown = approach(self.shown, shown, dt / FADE);
                if cycle > 0.0 {
                    self.phase = (self.phase + dt * self.pace).rem_euclid(cycle);
                }
                self.open = (self.open + if self.working { dt } else { -dt }).clamp(0.0, open);
            }
        }
        match self.upgrade {
            [Some(a), Some(b)] => self.share = a + (b - a) * alpha.clamp(0.0, 1.0),
            [None, Some(b)] => self.share = b,
            _ => {}
        }
        let ease = |x: f32| x * x * (3.0 - 2.0 * x);
        let playing: [Option<(&Layer, f32, bool, f32)>; 3] = [
            clips.active.as_ref().filter(|_| self.active > 0.0).map(|a| (a, self.phase, true, ease(self.active))),
            clips.open.as_ref().filter(|_| self.open > 0.0).map(|o| (o, self.open, false, 1.0)),
            clips.upgrade.as_ref().filter(|_| self.shown > 0.0).map(|u| (u, self.share * u.clip.seconds, false, ease(self.shown))),
        ];
        if playing.iter().all(Option::is_none) || prev.len() != cur.len() {
            return (prev, cur);
        }
        self.before.clear();
        self.before.extend_from_slice(prev);
        self.now.clear();
        self.now.extend_from_slice(cur);
        for (layer, at, cycle, weight) in playing.into_iter().flatten() {
            layer.add(at, cycle, weight, &mut self.keys, [&mut self.before, &mut self.now]);
        }
        (&self.before, &self.now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ta_formats::anims::Key;

    /// A pump of our own: its piece 1 rising 4 px and back over 2 s, and turning round; a hatch
    /// (piece 0) opening a quarter turn in a second; an upgrade lifting piece 2 by 10 px.
    fn clips() -> Clips {
        let up = |y: i32, t: i32| Key { moved: [0, y * 65536, 0], turned: [0, t, 0] };
        let pump = Clip { name: ACTIVE.into(), seconds: 2.0, pieces: vec![1], times: vec![0.0, 1.0], keys: vec![up(0, 0), up(4, 30000)] };
        let hatch = Clip { name: OPEN.into(), seconds: 1.0, pieces: vec![0], times: vec![0.0, 1.0], keys: vec![Key::default(), Key { moved: [0; 3], turned: [16384, 0, 0] }] };
        let upgrade = Clip { name: UPGRADE.into(), seconds: 4.0, pieces: vec![2], times: vec![0.0, 4.0], keys: vec![Key::default(), up(10, 0)] };
        Clips::of(&Anims { clips: vec![pump, hatch, upgrade, Clip { name: "walk".into(), ..Default::default() }] }).unwrap()
    }

    fn rest() -> Vec<PieceState> {
        vec![([0; 3], [0; 3], true); 3]
    }

    /// Posed at `time` (the frame's own `alpha`): piece `p`'s state.
    fn pose(w: &mut Worker, c: &Clips, time: f64, alpha: f32) -> Vec<PieceState> {
        let (prev, cur) = (rest(), rest());
        let (_, now) = w.states(c, time, alpha, &prev, &cur);
        now.to_vec()
    }

    /// Working, the loop plays at its pace from where it starts; stopped, it slows to a stop over
    /// its ease and holds, going on from there as it starts again; its turns run on.
    #[test]
    fn a_loop_plays_while_its_unit_works_and_stops_where_it_is() {
        let c = clips();
        let mut w = Worker { working: true, ..Worker::default() };
        assert_eq!(pose(&mut w, &c, 10.0, 0.0)[1].0[1], 0, "first seen, at the start of its cycle (its slot's: 0)");
        assert_eq!(pose(&mut w, &c, 10.5, 0.0)[1].0[1], 2 * 65536, "half way up");
        w.working = false;
        let mut at = 10.5;
        let mut last = 0;
        for _ in 0..12 {
            at += 0.1;
            last = pose(&mut w, &c, at, 0.0)[1].0[1];
        }
        let held = pose(&mut w, &c, at + 0.1, 0.0)[1].0[1];
        assert_eq!(held, last, "stopped: held");
        assert!(held > 2 * 65536 && held < 4 * 65536, "it slowed to a stop past where it was: {held}");
        w.working = true;
        assert!(pose(&mut w, &c, at + 0.5, 0.0)[1].0[1] > held, "on from there");
        // Its turn, 30,000 at the top: posed as it runs on (wrapped as a script's turn).
        let mut w = Worker { working: true, ..Worker::default() };
        pose(&mut w, &c, 0.0, 0.0);
        pose(&mut w, &c, 0.5, 0.0);
        assert_eq!(pose(&mut w, &c, 1.0, 0.0)[1].1[1], 30000);
    }

    /// The hatch opens as its unit starts working and closes as it stops; seen first at work, open.
    #[test]
    fn an_opening_opens_while_its_unit_works() {
        let c = clips();
        let mut w = Worker::default();
        assert_eq!(pose(&mut w, &c, 0.0, 0.0)[0].1[0], 0, "shut while it does not work");
        w.working = true;
        assert_eq!(pose(&mut w, &c, 0.5, 0.0)[0].1[0], 8192, "half open half a second on");
        assert_eq!(pose(&mut w, &c, 1.0, 0.0)[0].1[0], 16384, "open");
        assert_eq!(pose(&mut w, &c, 1.5, 0.0)[0].1[0], 16384, "held");
        assert_eq!(pose(&mut w, &c, 2.5, 0.0)[0].1[0], 16384, "held");
        w.working = false;
        assert_eq!(pose(&mut w, &c, 2.75, 0.0)[0].1[0], 12288, "closing");
        let mut seen = Worker { working: true, ..Worker::default() };
        assert_eq!(pose(&mut seen, &c, 9.0, 0.0)[0].1[0], 16384);
    }

    /// Upgrading, the loop fades out and the upgrade fades in, posed by the share done between
    /// the ticks; called off, the upgrade fades out where it was and the loop comes back.
    #[test]
    fn an_upgrade_is_posed_by_how_far_it_has_come() {
        let c = clips();
        let mut w = Worker { working: true, ..Worker::default() };
        pose(&mut w, &c, 0.0, 0.0);
        // (The unit upgrading: no longer at work.)
        (w.working, w.upgrade) = (false, [Some(0.5), Some(0.5)]);
        let faded = pose(&mut w, &c, FADE as f64, 0.0);
        assert_eq!(faded[2].0[1], 5 * 65536, "half way: 5 px up");
        assert_eq!(faded[1].0[1], 0, "the pump faded out");
        w.upgrade = [Some(0.5), Some(0.6)];
        assert_eq!(pose(&mut w, &c, FADE as f64 + 0.01, 0.5)[2].0[1], (5.5 * 65536.0) as i32, "between the ticks");
        // Called off: the upgrade fades out from where it was, the pump fades back in.
        (w.working, w.upgrade) = (true, [Some(0.6), None]);
        let mid = pose(&mut w, &c, FADE as f64 + 0.21, 0.0);
        assert!(mid[2].0[1] > 0 && mid[2].0[1] < 6 * 65536, "{}", mid[2].0[1]);
        assert_eq!(pose(&mut w, &c, FADE as f64 + 0.7, 0.0)[2].0[1], 0, "gone");
    }

    /// A model without work clips has none; one whose clips move nothing neither.
    #[test]
    fn only_clips_that_move_something_play() {
        assert!(Clips::of(&Anims { clips: vec![Clip { name: "walk".into(), seconds: 1.0, pieces: vec![0], times: vec![0.0], keys: vec![Key { moved: [1, 0, 0], turned: [0; 3] }] }] }).is_none());
        let still = Clip { name: ACTIVE.into(), seconds: 1.0, pieces: vec![0], times: vec![0.0, 0.5], keys: vec![Key::default(); 2] };
        assert!(Clips::of(&Anims { clips: vec![still] }).is_none());
    }
}
