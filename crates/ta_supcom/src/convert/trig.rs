//! Sine, cosine and arctangent of our own: only arithmetic whose results IEEE fixes to the bit
//! (square roots among them), so a map or a unit converted on one platform is the same, byte for
//! byte, as on another (the platform's `sin`, `cos` and `atan2` may differ in their last bit).

use std::f64::consts::{FRAC_PI_2, PI};

/// `(sin a, cos a)`, within a unit in the last place or two.
pub fn sin_cos(a: f64) -> (f64, f64) {
    if !a.is_finite() {
        return (0.0, 1.0);
    }
    // Into [-π, π], then into [-π/2, π/2] (sin(π - x) = sin x, cos(π - x) = -cos x).
    let turns = (a / (2.0 * PI)).round();
    let mut x = a - turns * (2.0 * PI);
    let mut flip = 1.0;
    if x > FRAC_PI_2 {
        x = PI - x;
        flip = -1.0;
    } else if x < -FRAC_PI_2 {
        x = -PI - x;
        flip = -1.0;
    }
    let x2 = x * x;
    // Taylor series to the 17th power: within 1e-16 on [-π/2, π/2].
    let mut s = 0.0;
    let mut c = 0.0;
    let mut ts = x;
    let mut tc = 1.0;
    for n in 0..10 {
        s += ts;
        c += tc;
        let k = 2.0 * n as f64;
        ts *= -x2 / ((k + 2.0) * (k + 3.0));
        tc *= -x2 / ((k + 1.0) * (k + 2.0));
    }
    (s, flip * c)
}

/// `atan2(y, x)`: the angle of (x, y) from the x axis, in radians, -π to π, within a unit in the
/// last place or two. The tangent's angle halved three times (`t / (1 + sqrt(1 + t²))`), its series
/// summed, doubled back.
pub fn atan2(y: f64, x: f64) -> f64 {
    if !(x.is_finite() && y.is_finite()) || (x == 0.0 && y == 0.0) {
        return 0.0;
    }
    let (ax, ay) = (x.abs(), y.abs());
    // The angle in [0, π/4] whose tangent is the smaller over the larger.
    let mut t = ax.min(ay) / ax.max(ay);
    for _ in 0..3 {
        t /= 1.0 + (1.0 + t * t).sqrt();
    }
    let t2 = t * t;
    let (mut sum, mut term) = (0.0, t);
    for n in 0..12 {
        sum += term / (2 * n + 1) as f64;
        term *= -t2;
    }
    let mut a = sum * 8.0;
    if ay > ax {
        a = FRAC_PI_2 - a;
    }
    if x < 0.0 {
        a = PI - a;
    }
    if y < 0.0 { -a } else { a }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_arctangent_agrees_with_the_platforms() {
        for i in -40..=40 {
            for j in -40..=40 {
                let (y, x) = (i as f64 * 0.37, j as f64 * 0.29);
                if x == 0.0 && y == 0.0 {
                    continue;
                }
                assert!((atan2(y, x) - y.atan2(x)).abs() < 1e-14, "{y} {x}: {} {}", atan2(y, x), y.atan2(x));
            }
        }
        assert_eq!(atan2(0.0, 0.0), 0.0);
        assert_eq!(atan2(0.0, -1.0), PI);
    }

    #[test]
    fn sine_and_cosine_agree_with_the_platforms() {
        for i in -200..=200 {
            let a = i as f64 * 0.0731;
            let (s, c) = sin_cos(a);
            assert!((s - a.sin()).abs() < 1e-14 && (c - a.cos()).abs() < 1e-14, "{a}: {s} {c}");
        }
        assert_eq!(sin_cos(0.0), (0.0, 1.0));
        assert_eq!(sin_cos(f64::NAN), (0.0, 1.0));
    }
}
