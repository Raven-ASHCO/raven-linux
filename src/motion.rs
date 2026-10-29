//! Honest motion policy (DESIGN.md, "Honest motion") and the Calm curves.
//!
//! Every animation is caused by an engine event or by the user's own action
//! (a Settings section, a switch, a segmented control); the only decorative
//! motion is the logo reveal on the welcome screen. Reduced motion (the
//! system's `gtk-enable-animations = false`, or Raven's own setting) means no
//! motion at all: texts still update.
//!
//! [`CubicBezier`] evaluates the tokens' CSS curves (`motion.curves`, some of
//! which overshoot like a spring) for the animations the app drives itself
//! frame by frame: the Settings glide, the segmented thumb, the group ring's
//! arcs, the lock's shackle and the route line's packets.

/// The two inputs that decide whether anything may move.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MotionPolicy {
    /// GTK's `gtk-enable-animations` (GNOME "Animations" switch).
    pub system_animations: bool,
    /// Raven's own "Reduced motion" setting.
    pub app_reduced_motion: bool,
}

impl MotionPolicy {
    pub fn allowed(&self) -> bool {
        self.system_animations && !self.app_reduced_motion
    }

    /// A transition length in ms, or 0 when nothing may move.
    pub fn duration(&self, ms: u32) -> u32 {
        if self.allowed() {
            ms
        } else {
            0
        }
    }
}

/// A CSS `cubic-bezier(x1, y1, x2, y2)` timing curve. `y` may leave 0..1
/// (the curve overshoots, like a spring); `x` is clamped to 0..1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CubicBezier {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
}

impl CubicBezier {
    pub fn new(p: [f64; 4]) -> Self {
        Self {
            x1: p[0].clamp(0.0, 1.0),
            y1: p[1],
            x2: p[2].clamp(0.0, 1.0),
            y2: p[3],
        }
    }

    /// A curve from the tokens by name (`lift`, `glide`, `thumb`, …).
    pub fn named(name: &str) -> Self {
        Self::new(crate::theme::tokens().curve(name))
    }

    fn axis(a: f64, b: f64, s: f64) -> f64 {
        // B(s) for P0 = 0, P1 = a, P2 = b, P3 = 1.
        let u = 1.0 - s;
        3.0 * u * u * s * a + 3.0 * u * s * s * b + s * s * s
    }

    fn axis_slope(a: f64, b: f64, s: f64) -> f64 {
        let u = 1.0 - s;
        3.0 * u * u * a + 6.0 * u * s * (b - a) + 3.0 * s * s * (1.0 - b)
    }

    /// The eased value at time `t` (0..1): solve x(s) = t, return y(s).
    pub fn ease(&self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        if t == 0.0 || t == 1.0 {
            return t;
        }
        // Newton's method, then bisection if it wanders off.
        let mut s = t;
        for _ in 0..8 {
            let x = Self::axis(self.x1, self.x2, s) - t;
            if x.abs() < 1e-6 {
                return Self::axis(self.y1, self.y2, s);
            }
            let d = Self::axis_slope(self.x1, self.x2, s);
            if d.abs() < 1e-6 {
                break;
            }
            s -= x / d;
        }
        let (mut lo, mut hi) = (0.0, 1.0);
        s = t;
        for _ in 0..60 {
            let x = Self::axis(self.x1, self.x2, s);
            if (x - t).abs() < 1e-7 {
                break;
            }
            if x < t {
                lo = s;
            } else {
                hi = s;
            }
            s = (lo + hi) / 2.0;
        }
        Self::axis(self.y1, self.y2, s)
    }
}

/// Where a value is at `elapsed_ms` of an animation from `from` to `to`
/// over `duration_ms` on `curve`, and whether it has arrived.
pub fn tween(
    from: f64,
    to: f64,
    elapsed_ms: f64,
    duration_ms: f64,
    curve: &CubicBezier,
) -> (f64, bool) {
    if duration_ms <= 0.0 || elapsed_ms >= duration_ms {
        return (to, true);
    }
    let t = (elapsed_ms / duration_ms).max(0.0);
    (from + (to - from) * curve.ease(t), false)
}

/// What the route line under the chat toolbar shows (one at a time, queued).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Packet {
    /// `send_started`: a ▸ travels left to right.
    Out,
    /// `receipt`: a ✓ travels right to left.
    ReceiptBack,
    /// `incoming`: a ◂ travels in, right to left.
    In,
}

impl Packet {
    /// Duration from the tokens.
    pub fn duration_ms(self, d: &crate::theme::Durations) -> u32 {
        match self {
            Packet::Out => d.route_packet,
            Packet::ReceiptBack => d.receipt_back,
            Packet::In => d.route_packet,
        }
    }

    /// Horizontal position (0 = left, 1 = right) at progress `t` (0..1),
    /// on the tokens' `packet` curve.
    pub fn position(self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        let eased = CubicBezier::named("packet").ease(t);
        match self {
            Packet::Out => eased,
            Packet::ReceiptBack | Packet::In => 1.0 - eased,
        }
    }

    /// Opacity at progress `t`: fades in over the first 15 %, out over the
    /// last 15 %.
    pub fn opacity(t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        if t < 0.15 {
            t / 0.15
        } else if t > 0.85 {
            (1.0 - t) / 0.15
        } else {
            1.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reduced_motion_from_either_side() {
        assert!(MotionPolicy {
            system_animations: true,
            app_reduced_motion: false
        }
        .allowed());
        assert!(!MotionPolicy {
            system_animations: false,
            app_reduced_motion: false
        }
        .allowed());
        assert!(!MotionPolicy {
            system_animations: true,
            app_reduced_motion: true
        }
        .allowed());
        assert_eq!(
            MotionPolicy {
                system_animations: true,
                app_reduced_motion: true
            }
            .duration(350),
            0
        );
        assert_eq!(
            MotionPolicy {
                system_animations: true,
                app_reduced_motion: false
            }
            .duration(350),
            350
        );
    }

    #[test]
    fn packets_travel_the_right_way() {
        assert_eq!(Packet::Out.position(0.0), 0.0);
        assert_eq!(Packet::Out.position(1.0), 1.0);
        assert_eq!(Packet::ReceiptBack.position(0.0), 1.0);
        assert_eq!(Packet::ReceiptBack.position(1.0), 0.0);
        assert_eq!(Packet::In.position(1.0), 0.0);
        assert!(Packet::Out.position(0.5) > 0.4, "eased");
        assert_eq!(Packet::opacity(0.5), 1.0);
        assert_eq!(Packet::opacity(0.0), 0.0);
        let d = crate::theme::tokens().durations;
        assert_eq!(Packet::Out.duration_ms(&d), 700);
        assert_eq!(Packet::ReceiptBack.duration_ms(&d), 600);
    }

    #[test]
    fn cubic_bezier_curves() {
        let linear = CubicBezier::new([0.0, 0.0, 1.0, 1.0]);
        for t in [0.0, 0.1, 0.5, 0.9, 1.0] {
            assert!((linear.ease(t) - t).abs() < 1e-4, "{t}");
        }
        // CSS ease-out: fast start, slow end.
        let out = CubicBezier::new([0.0, 0.0, 0.58, 1.0]);
        assert!(out.ease(0.3) > 0.3);
        // The tokens' springs overshoot past 1 before they settle.
        let glide = CubicBezier::named("glide");
        let peak = (1..100)
            .map(|i| glide.ease(i as f64 / 100.0))
            .fold(0.0_f64, f64::max);
        assert!(peak > 1.0, "glide overshoots: {peak}");
        assert_eq!(glide.ease(1.0), 1.0);
        assert_eq!(glide.ease(0.0), 0.0);
        // Monotone time: x stays in 0..1 even for odd control points.
        let odd = CubicBezier::new([1.4, 0.0, -0.2, 1.0]);
        assert!((0.0..=1.0).contains(&odd.ease(0.5)));
    }

    #[test]
    fn tweens_arrive() {
        let c = CubicBezier::named("thumb");
        assert_eq!(tween(10.0, 50.0, 0.0, 420.0, &c), (10.0, false));
        assert_eq!(tween(10.0, 50.0, 420.0, 420.0, &c), (50.0, true));
        assert_eq!(tween(10.0, 50.0, 5.0, 0.0, &c), (50.0, true), "no motion");
        let (mid, done) = tween(0.0, 100.0, 210.0, 420.0, &c);
        assert!(!done && mid > 50.0, "{mid}");
    }
}
