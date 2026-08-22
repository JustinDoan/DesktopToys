//! Timing curves for the opening sequence.

/// CS:GO's reel deceleration curve, expressed the same way CSS would write it.
pub const REEL_CURVE: CubicBezier = CubicBezier::new(0.08, 0.74, 0.05, 1.0);
/// A softer version used by the legendary wheel so the final stop reads slower.
pub const WHEEL_CURVE: CubicBezier = CubicBezier::new(0.05, 0.82, 0.08, 1.0);

/// A CSS-style `cubic-bezier(x1, y1, x2, y2)` with implicit (0,0) and (1,1) ends.
#[derive(Clone, Copy, Debug)]
pub struct CubicBezier {
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
}

impl CubicBezier {
    pub const fn new(x1: f32, y1: f32, x2: f32, y2: f32) -> Self {
        Self { x1, y1, x2, y2 }
    }

    fn axis(a: f32, b: f32, s: f32) -> f32 {
        let inverse = 1.0 - s;
        3.0 * inverse * inverse * s * a + 3.0 * inverse * s * s * b + s * s * s
    }

    fn axis_slope(a: f32, b: f32, s: f32) -> f32 {
        let inverse = 1.0 - s;
        3.0 * inverse * inverse * a + 6.0 * inverse * s * (b - a) + 3.0 * s * s * (1.0 - b)
    }

    /// Maps a 0..1 time fraction onto the curve's 0..1 output.
    pub fn eval(self, time: f32) -> f32 {
        let time = time.clamp(0.0, 1.0);
        if time <= 0.0 {
            return 0.0;
        }
        if time >= 1.0 {
            return 1.0;
        }

        // Newton first, since the curve is monotonic and well behaved for the
        // control points we ship.
        let mut s = time;
        for _ in 0..8 {
            let error = Self::axis(self.x1, self.x2, s) - time;
            if error.abs() < 1e-5 {
                return Self::axis(self.y1, self.y2, s);
            }
            let slope = Self::axis_slope(self.x1, self.x2, s);
            if slope.abs() < 1e-6 {
                break;
            }
            s -= error / slope;
        }

        // Bisection fallback keeps arbitrary user-authored curves safe.
        let mut low = 0.0f32;
        let mut high = 1.0f32;
        s = time;
        for _ in 0..32 {
            let x = Self::axis(self.x1, self.x2, s);
            if (x - time).abs() < 1e-5 {
                break;
            }
            if x > time {
                high = s;
            } else {
                low = s;
            }
            s = (low + high) * 0.5;
        }
        Self::axis(self.y1, self.y2, s)
    }
}

pub fn clamp01(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

pub fn lerp(from: f32, to: f32, amount: f32) -> f32 {
    from + (to - from) * amount
}

pub fn smoothstep(value: f32) -> f32 {
    let t = clamp01(value);
    t * t * (3.0 - 2.0 * t)
}

pub fn smootherstep(value: f32) -> f32 {
    let t = clamp01(value);
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

pub fn ease_out_cubic(value: f32) -> f32 {
    let inverse = 1.0 - clamp01(value);
    1.0 - inverse * inverse * inverse
}

pub fn ease_in_cubic(value: f32) -> f32 {
    let t = clamp01(value);
    t * t * t
}

pub fn ease_out_quint(value: f32) -> f32 {
    let inverse = 1.0 - clamp01(value);
    1.0 - inverse.powi(5)
}

/// Overshoots past 1 before settling, for the case slamming into place.
pub fn ease_out_back(value: f32, overshoot: f32) -> f32 {
    let t = clamp01(value) - 1.0;
    let c = overshoot + 1.0;
    1.0 + c * t * t * t + overshoot * t * t
}

/// A decaying bounce used for the reveal card's scale pop.
pub fn ease_out_elastic(value: f32) -> f32 {
    let t = clamp01(value);
    if t <= 0.0 {
        return 0.0;
    }
    if t >= 1.0 {
        return 1.0;
    }
    let period = std::f32::consts::TAU / 3.0;
    (2.0f32).powf(-10.0 * t) * ((10.0 * t - 0.75) * period).sin() + 1.0
}

/// Rises to 1 then falls back to 0, for one-shot flashes.
pub fn pulse(value: f32, sharpness: f32) -> f32 {
    let t = clamp01(value);
    let rise = smoothstep(t * sharpness);
    let fall = 1.0 - smoothstep((t - 1.0 / sharpness) / (1.0 - 1.0 / sharpness).max(1e-3));
    (rise * fall).clamp(0.0, 1.0)
}

/// Exponential decay toward zero, normalised so `eval(0) == 1`.
pub fn decay(value: f32, rate: f32) -> f32 {
    (-clamp01(value) * rate).exp()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bezier_pins_both_ends() {
        assert!(REEL_CURVE.eval(0.0).abs() < 1e-4);
        assert!((REEL_CURVE.eval(1.0) - 1.0).abs() < 1e-4);
    }

    #[test]
    fn reel_curve_is_monotonic_and_front_loaded() {
        let mut previous = 0.0;
        for step in 0..=200 {
            let value = REEL_CURVE.eval(step as f32 / 200.0);
            assert!(value >= previous - 1e-4, "curve dipped at step {step}");
            previous = value;
        }
        // The CS:GO feel comes from covering most of the distance early.
        assert!(REEL_CURVE.eval(0.5) > 0.8, "reel should decelerate late");
    }

    #[test]
    fn ease_out_back_overshoots_then_settles() {
        assert!(ease_out_back(0.6, 1.4) > 1.0);
        assert!((ease_out_back(1.0, 1.4) - 1.0).abs() < 1e-4);
    }

    #[test]
    fn pulse_starts_and_ends_dark() {
        assert!(pulse(0.0, 6.0) < 0.05);
        assert!(pulse(1.0, 6.0) < 0.05);
        assert!(pulse(0.4, 6.0) > 0.5);
    }
}
