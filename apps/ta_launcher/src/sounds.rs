//! The launcher's sounds: TA's own interface sounds from the player's install (the buttons,
//! the tabs, the big Play button, as TA's menus had them; `gamedata/allsound.tdf`), and its
//! music: TA's calmer tracks (the briefing's drone, the building music; or the player's own,
//! as the settings have it), quietly, at a share of the music volume. The music fades in as
//! the launcher opens, out while the game plays or the launcher is minimised, and back after.
//! The install's archives are opened on a thread of their own, so the launcher shows at once
//! and the sounds join a moment later; with no install, or the settings off, it stays silent.
//! A track plays on by itself: the interface is woken for the music only as it ends (its length
//! read as it starts), never drawn over and over for it.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use ta_audio::{AudioEngine, AudioSettings, Mood, MusicConfig, SoundBank, SoundSource, UiSounds};
use ta_vfs::Vfs;

/// Seconds the music takes to fade in or out.
const FADE: f32 = 1.6;

/// The launcher's music: TA's calmer tracks, the briefing's drone and the building music,
/// shuffled, coming and going slowly.
fn playlist() -> MusicConfig {
    let mut config = MusicConfig { crossfade: 4.0, fade_in: 4.0, gap: 4.0, shuffle: true, ..Default::default() };
    config.playlists.briefing = [17, 11, 12, 13, 14, 15, 16].into_iter().map(ta_audio::music::TrackRef::Number).collect();
    config
}

/// How loud the music is now (a share of what it is set to), eased toward where it should be.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MusicFade {
    pub level: f32,
}

impl MusicFade {
    /// `dt` seconds on toward `target`, over [`FADE`] seconds for the whole way.
    pub fn step(&mut self, target: f32, dt: f32) {
        let step = (dt.max(0.0) / FADE).min(1.0);
        self.level += (target - self.level).clamp(-step, step);
    }
}

/// The install's archives, opened, and its interface sounds.
type Opened = Option<(Arc<Vfs>, UiSounds)>;

/// When the music next needs the engine looked at, if it does (`None`: it plays on by itself
/// until something else asks for a frame). Fading in or out: smoothly. A track playing: once it
/// is due to end (`playing`: the seconds left; `None` within it: not known), a second at a time
/// once it is (its end may come a little late), and now and then meanwhile (a track that fails
/// mid-way, a device gone), more often when how long it has left is not known. None playing
/// (`silent_for` so long): a second at a time, as the gap between two tracks runs out (the
/// engine takes at most a second of its clock a look); seldom once it has gone on (no device,
/// no track that reads).
fn music_wake(fading: bool, on: bool, playing: Option<Option<f64>>, silent_for: Duration) -> Option<Duration> {
    if fading {
        return Some(Duration::from_millis(50));
    }
    if !on {
        return None;
    }
    let seconds = match playing {
        Some(Some(left)) => (left + 0.25).clamp(1.0, 30.0),
        Some(None) => 5.0,
        None if silent_for < Duration::from_secs(15) => 1.0,
        None => 5.0,
    };
    Some(Duration::from_secs_f64(seconds))
}

/// How long a track plays (seconds), from its file: a VFS path from `source`, or `file:` and a
/// path (the player's own music). `None` when it cannot be told.
fn track_seconds(source: &dyn SoundSource, track: &str) -> Option<f64> {
    match track.strip_prefix("file:") {
        Some(file) => ta_audio::length::file_seconds(Path::new(file)),
        None => source.read(track).as_deref().and_then(ta_audio::length::seconds),
    }
}

/// The length of the track playing, read on a thread of its own as it starts (a track is read
/// whole to tell it): the music is looked at again as it ends.
#[derive(Default)]
struct TrackLength {
    /// The track measured, and its length (`None`: it cannot be told).
    known: Option<(String, Option<f64>)>,
    /// The track being measured.
    measuring: Option<(String, Receiver<Option<f64>>)>,
}

impl TrackLength {
    /// `track`'s length: `None` while it is being measured (a frame is asked for once it is),
    /// `Some(None)` when it cannot be told.
    fn of(&mut self, track: &str, source: &Arc<dyn SoundSource>, ctx: &egui::Context) -> Option<Option<f64>> {
        if let Some((measured, rx)) = &self.measuring {
            // (A measure that went wrong cannot tell it.)
            let length = match rx.try_recv() {
                Ok(length) => Some(length),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(None),
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
            };
            if let Some(length) = length {
                self.known = Some((measured.clone(), length));
                self.measuring = None;
            }
        }
        if let Some((measured, length)) = &self.known
            && measured == track
        {
            return Some(*length);
        }
        if self.measuring.as_ref().is_none_or(|(measured, _)| measured != track) {
            let (tx, rx) = channel();
            let (source, path, ctx) = (source.clone(), track.to_string(), ctx.clone());
            let spawned = std::thread::Builder::new().name("music length".into()).spawn(move || {
                let _ = tx.send(track_seconds(source.as_ref(), &path));
                ctx.request_repaint();
            });
            match spawned {
                Ok(_) => self.measuring = Some((track.to_string(), rx)),
                Err(e) => {
                    log::warn!("launcher music: {e}");
                    self.known = Some((track.to_string(), None));
                    return Some(None);
                }
            }
        }
        None
    }
}

pub struct Sounds {
    /// The install's archives being opened, and its interface sounds read.
    loading: Option<JoinHandle<Opened>>,
    playing: Option<(AudioEngine, UiSounds)>,
    /// `--film`: the sounds are named for the film's sound log, never played.
    filming: bool,
    named: Option<UiSounds>,
    last: Instant,
    /// The volumes last applied.
    applied: Option<ta_config::Audio>,
    /// The music: how loud it is now, whether a playlist is asked for, a track skip asked for.
    music: MusicFade,
    music_on: bool,
    skip: bool,
    /// The track playing (a VFS path, or `file:` and a path), and whether the install has
    /// music at all.
    playing_track: Option<String>,
    has_music: bool,
    /// How long the track playing is, and since when none has played (between two tracks).
    length: TrackLength,
    silent_since: Option<Instant>,
}

impl Sounds {
    /// Opens the install at `root` (when there is one) for its sounds; `filming`: names them
    /// for the film instead of playing them.
    pub fn start(root: Option<PathBuf>, filming: bool) -> Sounds {
        let loading = root.map(|root| {
            std::thread::spawn(move || {
                let mut vfs = Vfs::new();
                if let Err(e) = vfs.mount_game_dir(&root, "base") {
                    log::warn!("launcher sounds: {e}");
                    return None;
                }
                let ui = UiSounds::load(&vfs).ok()?;
                Some((Arc::new(vfs), ui))
            })
        });
        Sounds { loading, playing: None, filming, named: None, last: Instant::now(), applied: None, music: MusicFade::default(), music_on: false, skip: false, playing_track: None, has_music: false, length: TrackLength::default(), silent_since: None }
    }

    /// The install's sounds are known (or there is no install to know them from).
    pub fn ready(&self) -> bool {
        self.loading.is_none()
    }

    /// `--film`: the sound the interface asked for this frame, by its path in the game's files.
    pub fn take_named(&self, ctx: &egui::Context) -> Option<String> {
        let sound = ta_ui::sound::take(ctx)?;
        self.named.as_ref()?.get(sound.event()).map(ta_audio::sound_path)
    }

    /// Before drawing widgets: finish loading and let them request sounds for this frame.
    pub fn begin_frame(&mut self, ctx: &egui::Context, audio: &ta_config::Audio) {
        if self.loading.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
        if self.filming {
            if self.loading.as_ref().is_some_and(|l| l.is_finished()) {
                self.named = self.loading.take().and_then(|l| l.join().ok()).flatten().map(|(_, ui)| ui);
            }
            ta_ui::sound::enable(ctx, audio.launcher_sounds && self.named.is_some());
            return;
        }
        if self.loading.as_ref().is_some_and(|l| l.is_finished())
            && let Some(Ok(Some((vfs, ui)))) = self.loading.take().map(|l| l.join())
        {
            let bank = Arc::new(SoundBank::from_vfs(vfs));
            // (Silent to begin with: the music fades in, see `update`.)
            let quiet = ta_config::Audio { music: 0.0, ..audio.clone() };
            match AudioEngine::new(AudioSettings::from(&quiet), bank) {
                Ok(mut engine) => {
                    engine.set_music_config(playlist());
                    engine.music_mut().set_user_tracks(&ta_config::music_files(audio), audio.user_music);
                    self.has_music = !engine.music().playlist(Mood::Briefing).is_empty();
                    self.playing = Some((engine, ui));
                }
                Err(e) => log::warn!("launcher sounds: {e}"),
            }
        }
        let on = audio.launcher_sounds && self.playing.is_some();
        ta_ui::sound::enable(ctx, on);
    }

    /// The track playing, if any (a VFS path, or `file:` and a path).
    pub fn now_playing(&self) -> Option<&str> {
        self.playing_track.as_deref()
    }

    /// The install has music the launcher can play.
    pub fn has_music(&self) -> bool {
        self.has_music
    }

    /// Captures (`--show music`): as if `track` played (they are silent).
    pub fn preview_music(&mut self, track: &str) {
        self.has_music = true;
        self.playing_track = Some(track.to_string());
    }

    /// On to the next track.
    pub fn skip_track(&mut self) {
        self.skip = true;
    }

    /// After drawing widgets: this frame's interface sound, played at once.
    pub fn update(&mut self, ctx: &egui::Context, audio: &ta_config::Audio) {
        if self.filming {
            if !audio.launcher_sounds {
                let _ = ta_ui::sound::take(ctx);
            }
            return;
        }
        let sound = ta_ui::sound::take(ctx);
        let Some((engine, ui)) = self.playing.as_mut() else { return };
        if let Some(sound) = sound
            && audio.launcher_sounds
            && let Some(name) = ui.get(sound.event())
        {
            engine.play_ui(name, 0.8);
        }
    }

    /// Every frame, from `logic` (which runs minimised too, when nothing is drawn): the music
    /// follows the settings, keeps `quiet` (the game playing, the launcher minimised) fading in
    /// and out, and goes on to its next track, a frame asked for when that is due ([`music_wake`]).
    pub fn music(&mut self, ctx: &egui::Context, audio: &ta_config::Audio, quiet: bool) {
        if self.filming {
            return;
        }
        let dt = self.last.elapsed().as_secs_f32();
        self.last = Instant::now();
        let Some((engine, _)) = self.playing.as_mut() else { return };
        // The player's own music turned on or off, or its folder changed: its tracks again.
        if self.applied.as_ref().is_some_and(|applied| applied.user_music != audio.user_music || applied.music_folder != audio.music_folder) {
            engine.music_mut().set_user_tracks(&ta_config::music_files(audio), audio.user_music);
            self.has_music = !engine.music().playlist(Mood::Briefing).is_empty();
        }
        // The music: faded toward its share of the music volume, or out (and stopped once
        // silent); a playlist asked for as it comes back.
        let target = if audio.launcher_music && !quiet && self.has_music { audio.launcher_music_volume.clamp(0.0, 1.0) } else { 0.0 };
        let before = self.music.level;
        self.music.step(target, dt);
        if target > 0.0 && !self.music_on {
            engine.set_mood(Mood::Briefing);
            self.music_on = true;
        } else if target <= 0.0 && self.music.level <= 0.0 && self.music_on {
            engine.music_mut().stop_with_fade(0.0);
            self.music_on = false;
        }
        if std::mem::take(&mut self.skip) && self.music_on {
            engine.music_mut().skip();
        }
        if self.applied.as_ref() != Some(audio) || before != self.music.level {
            engine.apply_config(&ta_config::Audio { music: audio.music * self.music.level, ..audio.clone() });
            self.applied = Some(audio.clone());
        }
        engine.update(dt);
        self.playing_track = engine.music().now_playing().map(str::to_string);
        // (A track ending starts the next, though nothing else asks for a frame: one is asked for
        // as it ends, not over and over while it plays.)
        let playing = self.playing_track.as_deref().map(|track| {
            self.length.of(track, engine.bank().source(), ctx).flatten().zip(engine.music_position()).map(|(length, at)| length - at)
        });
        let silent_for = match &self.playing_track {
            None if self.music_on => self.silent_since.get_or_insert_with(Instant::now).elapsed(),
            _ => {
                self.silent_since = None;
                Duration::ZERO
            }
        };
        if let Some(after) = music_wake(self.music.level != target, self.music_on, playing, silent_for) {
            ctx.request_repaint_after(after);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filming_respects_sound_settings_and_discards_a_muted_click() {
        let ctx = egui::Context::default();
        let mut sounds = Sounds::start(None, true);
        sounds.named = Some(UiSounds::parse(b"[SMLBUTTON] { sound=click; }"));
        let mut audio = ta_config::Audio::default();
        sounds.begin_frame(&ctx, &audio);
        ta_ui::sound::play(&ctx, ta_ui::sound::UiSound::Button);
        sounds.update(&ctx, &audio);
        assert_eq!(sounds.take_named(&ctx).as_deref(), Some("sounds/click.wav"));

        ta_ui::sound::play(&ctx, ta_ui::sound::UiSound::Button);
        audio.launcher_sounds = false;
        sounds.update(&ctx, &audio);
        assert_eq!(sounds.take_named(&ctx), None);
        sounds.begin_frame(&ctx, &audio);
        ta_ui::sound::play(&ctx, ta_ui::sound::UiSound::Button);
        assert_eq!(sounds.take_named(&ctx), None);
    }

    #[test]
    fn the_music_fades_in_and_out_over_its_time_and_never_past() {
        let mut fade = MusicFade::default();
        for _ in 0..10 {
            fade.step(0.45, 0.1);
        }
        assert!((fade.level - 0.45).abs() < 1e-6, "in by a second: {}", fade.level);
        fade.step(0.0, 0.8);
        assert!((fade.level - 0.0).abs() < 1e-6, "out over its time: {}", fade.level);
        fade.step(0.0, 1.0);
        assert_eq!(fade.level, 0.0);
        // (A long frame never overshoots.)
        fade.step(1.0, 10.0);
        assert_eq!(fade.level, 1.0);
    }

    /// The music asks for a frame only as it needs one: smoothly while it fades, as the track
    /// playing is due to end (and now and then meanwhile), a second at a time between two
    /// tracks, then seldom; never while it is off. Not four times a second as a track plays.
    #[test]
    fn the_music_wakes_the_launcher_only_as_it_needs_to() {
        let s = Duration::from_secs_f64;
        assert_eq!(music_wake(true, true, Some(Some(100.0)), Duration::ZERO), Some(Duration::from_millis(50)), "fading");
        assert_eq!(music_wake(false, false, None, Duration::ZERO), None, "off");
        assert_eq!(music_wake(false, true, Some(Some(12.0)), Duration::ZERO), Some(s(12.25)), "as the track ends");
        assert_eq!(music_wake(false, true, Some(Some(200.0)), Duration::ZERO), Some(s(30.0)), "now and then meanwhile");
        assert_eq!(music_wake(false, true, Some(Some(-3.0)), Duration::ZERO), Some(s(1.0)), "past its end: a second at a time");
        assert_eq!(music_wake(false, true, Some(None), Duration::ZERO), Some(s(5.0)), "its end not known");
        assert_eq!(music_wake(false, true, None, Duration::from_secs(2)), Some(s(1.0)), "between two tracks");
        assert_eq!(music_wake(false, true, None, Duration::from_secs(60)), Some(s(5.0)), "silent for long");
    }

    /// With the install's music playing, the launcher is woken as the track ends: once its length
    /// is known, the next frame asked for is tens of seconds away (it was a quarter of one).
    #[test]
    fn a_playing_track_wakes_the_launcher_only_as_it_ends() {
        let Ok(install) = ta_vfs::install::discover(None) else {
            eprintln!("skipping: no TA install");
            return;
        };
        let mut vfs = Vfs::new();
        if vfs.mount_game_dir(&install.root, "base").is_err() {
            return;
        }
        let bank = Arc::new(SoundBank::from_vfs(Arc::new(vfs)));
        let mut engine = AudioEngine::new_offline(AudioSettings::default(), bank, 48_000);
        engine.set_music_config(playlist());
        if engine.music().playlist(Mood::Briefing).is_empty() {
            eprintln!("skipping: no music in the install");
            return;
        }
        let mut sounds = Sounds::start(None, false);
        sounds.playing = Some((engine, UiSounds::parse(b"")));
        sounds.has_music = true;
        let audio = ta_config::Audio::default();
        // (Faded in already.)
        sounds.music.level = audio.launcher_music_volume;
        let ctx = egui::Context::default();
        let next_frame = |sounds: &mut Sounds| {
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| sounds.music(ui.ctx(), &audio, false));
            output.textures_delta.clear();
            output.viewport_output[&egui::ViewportId::ROOT].repaint_delay
        };
        let until = Instant::now() + Duration::from_secs(30);
        next_frame(&mut sounds);
        while sounds.length.known.is_none() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(20));
            next_frame(&mut sounds);
        }
        assert!(sounds.now_playing().is_some(), "a track plays");
        assert!(matches!(&sounds.length.known, Some((_, Some(seconds))) if *seconds > 30.0), "its length: {:?}", sounds.length.known);
        // (The measure's own frame taken first.)
        std::thread::sleep(Duration::from_millis(50));
        let delays: Vec<Duration> = (0..3).map(|_| next_frame(&mut sounds)).collect();
        assert!(*delays.last().unwrap() >= Duration::from_secs(25), "woken as it ends, not every quarter second: {delays:?}");
    }

    #[test]
    fn the_launcher_plays_the_calm_tracks() {
        let tracks = playlist().playlists.briefing;
        assert_eq!(tracks.first(), Some(&ta_audio::music::TrackRef::Number(17)));
        assert!(!tracks.iter().any(|t| matches!(t, ta_audio::music::TrackRef::Number(2..=8))), "no battle music: {tracks:?}");
    }

    /// With an install: the launcher's playlist plays one of TA's calm tracks, and is heard
    /// (mixed offline: no speakers needed).
    #[test]
    fn the_calm_tracks_play_from_the_install() {
        let Ok(install) = ta_vfs::install::discover(None) else {
            eprintln!("skipping: no TA install");
            return;
        };
        let mut vfs = Vfs::new();
        if vfs.mount_game_dir(&install.root, "base").is_err() {
            return;
        }
        let bank = Arc::new(SoundBank::from_vfs(Arc::new(vfs)));
        let mut engine = AudioEngine::new_offline(AudioSettings::default(), bank, 48_000);
        engine.set_music_config(playlist());
        engine.set_mood(Mood::Briefing);
        let mut out = vec![0.0f32; 48_000 * 2 / 10];
        let mut loudest = 0.0f32;
        for _ in 0..60 {
            engine.update(0.1);
            engine.render_offline(&mut out);
            loudest = out.iter().fold(loudest, |m, v| m.max(v.abs()));
        }
        let track = engine.music().now_playing().unwrap_or_default().to_string();
        let calm: Vec<String> = [17, 11, 12, 13, 14, 15, 16].iter().map(|n| format!("music/{n}.mp3")).collect();
        assert!(calm.contains(&track), "a calm track: {track}");
        assert!(loudest > 0.001, "and heard: {loudest}");
    }
}
