//! Lines laid on the ground as the screen shows them: a range's rim, an area order's ring. Only
//! what lies in front of the eye and in the view (and a margin round it) is made into points,
//! a point every few screen points there and none elsewhere, however large the circle. (A rim
//! made of a fixed number of points, the points behind the eye left out, was drawn as one line:
//! seen from low over the ground, a radar reaching across the map ran its rim out to where the
//! eye's plane cut it, millions of points off the screen, and its dashes, laid all along it,
//! were counted in hundreds of thousands a frame.)

use egui::{Painter, Pos2, Shape, Stroke};
use glam::{Mat4, Vec2, Vec3, Vec4};

/// How the battlefield projects to the screen (in points, as the interface draws).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Lens {
    view_proj: Mat4,
    /// The view's size in points.
    size: Vec2,
}

/// How far past the view's edges lines are kept (normalised device units: 1 is the edge), so
/// that they run on out of sight where the painter's clip ends them.
const MARGIN: f32 = 1.2;
/// The least distance in front of the eye a point is kept at (`w`: world units for the
/// perspective camera, always 1 for the classic one).
const NEAR: f32 = 0.01;
/// Sides a circle is first cut into, before those in view are cut finer.
const SIDES: usize = 128;
/// Screen points between the points of a line in view, at most (unless [`DEPTH`] says).
const STEP: f32 = 6.0;
/// How many times a side in view is halved at most: into 64, a point every 10 px of the ground
/// round a radar's rim 13,000 px out.
const DEPTH: u32 = 6;

impl Lens {
    pub(crate) fn new(camera: &ta_render::Camera, pixels_per_point: f32) -> Lens {
        Lens { view_proj: camera.view_proj(), size: camera.viewport / pixels_per_point.max(1e-3) }
    }

    fn clip(&self, p: Vec3) -> Vec4 {
        self.view_proj * p.extend(1.0)
    }

    /// Where a point in front of the eye (clip space) lands on the screen.
    fn screen(&self, c: Vec4) -> Pos2 {
        let w = c.w.max(NEAR);
        Pos2::new((c.x / w + 1.0) * 0.5 * self.size.x, (1.0 - c.y / w) * 0.5 * self.size.y)
    }

    /// Where a world point lands on the screen, if it is in front of the eye.
    pub(crate) fn project(&self, p: Vec3) -> Option<Pos2> {
        let c = self.clip(p);
        (c.w > NEAR).then(|| self.screen(c))
    }
}

/// The part of the straight line `a` to `b` (clip space) in front of the eye and in the view with
/// its margin: the fractions of the way along it where that part begins and ends.
fn visible(a: Vec4, b: Vec4) -> Option<(f32, f32)> {
    let sides = |c: Vec4| [c.w - NEAR, MARGIN * c.w - c.x, MARGIN * c.w + c.x, MARGIN * c.w - c.y, MARGIN * c.w + c.y];
    let (fa, fb) = (sides(a), sides(b));
    let (mut t0, mut t1) = (0.0f32, 1.0f32);
    for (p, q) in fa.into_iter().zip(fb) {
        if p < 0.0 && q < 0.0 {
            return None;
        }
        if p < 0.0 {
            t0 = t0.max(p / (p - q));
        } else if q < 0.0 {
            t1 = t1.min(p / (p - q));
        }
    }
    (t0 < t1).then_some((t0, t1))
}

/// Points of a line on the screen, unbroken, and how far along the whole line it begins (screen
/// points): dashes laid on it keep their places where the view cuts the line short.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Run {
    pub start: f64,
    pub points: Vec<Pos2>,
}

/// The runs of a line being made, side by side.
#[derive(Default)]
struct Runs {
    done: Vec<Run>,
    open: Option<Run>,
    /// How far along the line (screen points) the next side begins.
    along: f64,
    /// The first run began where the line does (so that a run still open at its end goes on
    /// into it: the circle closed).
    from_start: bool,
}

impl Runs {
    fn end(&mut self) {
        if let Some(run) = self.open.take().filter(|r| r.points.len() >= 2) {
            self.done.push(run);
        }
    }

    /// The side from angle `a0` to `a1` (`c0`, `c1` its ends in clip space): what of it is in
    /// view, halved while it is longer than a few points on the screen.
    fn side(&mut self, lens: &Lens, point: &dyn Fn(f32) -> Vec4, (a0, a1): (f32, f32), (c0, c1): (Vec4, Vec4), depth: u32) {
        let Some((t0, t1)) = visible(c0, c1) else {
            self.end();
            // (Its length on the screen, where both ends are in front of the eye, keeps the dashes
            // that come after in their places as the view moves.)
            if c0.w > NEAR && c1.w > NEAR {
                self.along += f64::from(lens.screen(c0).distance(lens.screen(c1)));
            }
            return;
        };
        let (p0, p1) = (lens.screen(c0.lerp(c1, t0)), lens.screen(c0.lerp(c1, t1)));
        if depth < DEPTH && p0.distance(p1) > STEP {
            let am = (a0 + a1) * 0.5;
            let cm = point(am);
            self.side(lens, point, (a0, am), (c0, cm), depth + 1);
            self.side(lens, point, (am, a1), (cm, c1), depth + 1);
            return;
        }
        let cut = |t: f32| if c0.w > NEAR && c1.w > NEAR { f64::from(lens.screen(c0).distance(lens.screen(c1)) * t) } else { 0.0 };
        if t0 > 0.0 {
            self.end();
            self.along += cut(t0);
        }
        if self.open.is_none() {
            if self.done.is_empty() && a0 == 0.0 && t0 == 0.0 {
                self.from_start = true;
            }
            self.open = Some(Run { start: self.along, points: vec![p0] });
        }
        if let Some(run) = self.open.as_mut() {
            run.points.push(p1);
        }
        self.along += f64::from(p0.distance(p1));
        if t1 < 1.0 {
            self.end();
            self.along += cut(1.0 - t1);
        }
    }

    /// The runs, a run still open at the circle's end joined to the first where that began at its
    /// start.
    fn finish(mut self, closed: bool) -> Vec<Run> {
        if let Some(mut last) = self.open.take() {
            if closed && self.from_start && !self.done.is_empty() {
                let first = self.done.remove(0);
                last.points.extend(first.points.into_iter().skip(1));
            }
            if last.points.len() >= 2 {
                self.done.push(last);
            }
        }
        self.done
    }
}

/// The circle of `radius` round `centre` laid on the ground, `lift` over it (`ground(x, z)` its
/// height), as `lens` shows it: in runs of screen points where it is in front of the eye and in
/// view, a point every few screen points there. However large the circle and wherever the eye,
/// a few thousand points at most.
pub(crate) fn circle(lens: &Lens, ground: &dyn Fn(f32, f32) -> f32, centre: Vec3, radius: f32, lift: f32) -> Vec<Run> {
    let point = |a: f32| {
        let (x, z) = (centre.x + a.cos() * radius, centre.z + a.sin() * radius);
        lens.clip(Vec3::new(x, ground(x, z) + lift, z))
    };
    let step = std::f32::consts::TAU / SIDES as f32;
    let mut runs = Runs::default();
    let mut last = point(0.0);
    for k in 0..SIDES {
        let (a0, a1) = (k as f32 * step, (k + 1) as f32 * step);
        let next = point(a1);
        runs.side(lens, &point, (a0, a1), (last, next), 0);
        last = next;
    }
    runs.finish(true)
}

/// The circle's rim at `sides` points, each with its place on the screen where it is in front of
/// the eye (else None), for a shading fan: unlike [`circle`], these may lie far off the screen.
pub(crate) fn fan(lens: &Lens, ground: &dyn Fn(f32, f32) -> f32, centre: Vec3, radius: f32, lift: f32, sides: usize) -> Vec<Option<Pos2>> {
    (0..=sides)
        .map(|k| {
            let a = k as f32 / sides as f32 * std::f32::consts::TAU;
            let (x, z) = (centre.x + a.cos() * radius, centre.z + a.sin() * radius);
            lens.project(Vec3::new(x, ground(x, z) + lift, z))
        })
        .collect()
}

/// Strokes the runs.
pub(crate) fn stroke(painter: &Painter, runs: &[Run], stroke: Stroke) {
    for run in runs {
        painter.add(Shape::line(run.points.clone(), stroke));
    }
}

/// Dashes along the runs, `dash` long and `gap` apart, starting `crawl` along the whole line
/// (moving it moves them along).
pub(crate) fn dashes(painter: &Painter, runs: &[Run], stroke: Stroke, dash: f32, gap: f32, crawl: f32) {
    let period = f64::from(dash + gap);
    for run in runs {
        let offset = (f64::from(crawl) - run.start).rem_euclid(period) as f32;
        painter.extend(Shape::dashed_line_with_offset(&run.points, stroke, &[dash], &[gap], offset));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ta_render::Camera;

    fn camera(focus: Vec3, zoom: f32, yaw: f32, pitch_degrees: f32) -> Camera {
        let mut camera = Camera::new(Vec2::new(1920.0, 1080.0));
        camera.focus = focus;
        camera.zoom = zoom;
        camera.yaw = yaw;
        camera.pitch = pitch_degrees.to_radians();
        camera
    }

    fn points(runs: &[Run]) -> usize {
        runs.iter().map(|r| r.points.len()).sum()
    }

    /// Every point is in the view or just past its edges, nowhere near the millions of points
    /// away a rim cut by the eye's plane reached.
    fn kept_near_the_view(runs: &[Run]) {
        for p in runs.iter().flat_map(|r| &r.points) {
            assert!(p.x.is_finite() && p.y.is_finite());
            assert!(p.x > -1920.0 * 0.15 && p.x < 1920.0 * 1.15 && p.y > -1080.0 * 0.15 && p.y < 1080.0 * 1.15, "{p:?}");
        }
    }

    /// A circle wholly in view: one run, closed, its points on the circle a few points apart.
    #[test]
    fn a_circle_in_view_is_one_closed_run() {
        let lens = Lens::new(&camera(Vec3::ZERO, 1.0, 0.0, 63.0), 1.0);
        let runs = circle(&lens, &|_, _| 0.0, Vec3::ZERO, 300.0, 0.0);
        assert_eq!(runs.len(), 1, "{runs:?}");
        let run = &runs[0];
        assert!(run.points.first().unwrap().distance(*run.points.last().unwrap()) < 0.01, "closed");
        let longest = run.points.windows(2).map(|w| w[0].distance(w[1])).fold(0.0, f32::max);
        assert!(longest <= STEP, "a point every few screen points: {longest}");
        assert!(points(&runs) < 600, "{}", points(&runs));
        assert_eq!(run.start, 0.0);
    }

    /// A radar reaching far past the view, seen from close and low over the ground, from many
    /// sides: what is drawn stays a few thousand points near the view, and it is still there.
    #[test]
    fn a_huge_circle_from_low_down_is_cut_to_the_view() {
        for (zoom, pitch) in [(1.0, 30.0), (2.0, 25.0), (4.0, 20.0), (8.0, 15.0), (6.0, 35.0), (3.0, 12.0), (8.0, 6.0)] {
            for yaw in [0.0f32, 0.4, 1.3, 2.9, 4.4] {
                for (radius, off) in [(13440.0, Vec3::ZERO), (2688.0, Vec3::new(400.0, 0.0, 900.0)), (30000.0, Vec3::new(-2000.0, 0.0, 500.0))] {
                    let lens = Lens::new(&camera(Vec3::new(2000.0, 0.0, 2000.0), zoom, yaw, pitch), 1.0);
                    let runs = circle(&lens, &|x, z| (x * 0.01).sin() * 40.0 + (z * 0.013).cos() * 30.0, Vec3::new(2000.0, 0.0, 2000.0) + off, radius, 2.0);
                    kept_near_the_view(&runs);
                    assert!(points(&runs) < 8_000, "zoom {zoom} pitch {pitch} yaw {yaw} radius {radius}: {} points", points(&runs));
                    let along: f32 = runs.iter().flat_map(|r| r.points.windows(2)).map(|w| w[0].distance(w[1])).sum();
                    assert!(along < 30_000.0, "{along} points of line");
                }
            }
        }
    }

    /// Off the view altogether: nothing to draw. Wholly round the view (the eye inside a radar's
    /// reach, close in): nothing either.
    #[test]
    fn a_circle_out_of_view_draws_nothing() {
        let lens = Lens::new(&camera(Vec3::ZERO, 2.0, 0.0, 63.0), 1.0);
        assert!(circle(&lens, &|_, _| 0.0, Vec3::new(9000.0, 0.0, 0.0), 200.0, 0.0).is_empty());
        assert!(circle(&lens, &|_, _| 0.0, Vec3::ZERO, 13440.0, 0.0).is_empty());
    }

    /// Dashes on a run cut short by the view begin where they would on the whole line: the run
    /// says how far along it begins.
    #[test]
    fn runs_cut_by_the_view_know_where_they_begin() {
        // (The view on the rim's far side from where it begins.)
        let lens = Lens::new(&camera(Vec3::new(-600.0, 0.0, 0.0), 2.0, 0.0, 63.0), 1.0);
        let runs = circle(&lens, &|_, _| 0.0, Vec3::ZERO, 600.0, 0.0);
        assert!(!runs.is_empty());
        assert!(runs.iter().all(|r| r.start > 0.0), "the line begins out of view: {:?}", runs.iter().map(|r| r.start).collect::<Vec<_>>());
    }

    /// The classic camera (an orthographic projection, w always 1) cuts the same way.
    #[test]
    fn the_classic_camera_cuts_too() {
        let mut classic = camera(Vec3::new(2000.0, 0.0, 2000.0), 4.0, 0.0, 63.0);
        classic.mode = ta_render::camera::CameraMode::Classic;
        let lens = Lens::new(&classic, 1.0);
        // (Its rim through the middle of the view.)
        let runs = circle(&lens, &|_, _| 0.0, Vec3::new(2000.0, 0.0, 2000.0 + 2688.0), 2688.0, 2.0);
        kept_near_the_view(&runs);
        assert!(points(&runs) < 4_000, "{}", points(&runs));
        assert!(!runs.is_empty(), "the rim crosses the view");
    }
}
