//! Easing curves, spring physics, and a transition animator.
//!
//! The unit of time throughout is **seconds** (`f32`). Progress `t` and eased
//! output are normalised to `[0, 1]` unless a spring overshoots.
//!
//! - [`Easing`] evaluates the standard timing functions. The CSS `cubic-bezier`
//!   case is the hard one: the curve is parametric in an internal variable `u`,
//!   but we are given the *x* (time) coordinate and must solve for `u` to read
//!   off *y* (progress). We invert with **Newton–Raphson** and fall back to
//!   **bisection** when the derivative is near zero (degenerate control points
//!   such as `(0,1,1,0)` would otherwise diverge to `NaN`).
//! - [`Spring`] is a damped harmonic oscillator solved in closed form for the
//!   under-, over-, and critically-damped regimes (no Euler stepping, so it is
//!   stable at any frame rate).
//! - [`Transition`] bundles duration/delay/easing; [`Animator`] tracks active
//!   transitions and samples them at a given elapsed time.

/// A timing function mapping linear progress `t ∈ [0, 1]` to eased progress.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Easing {
    /// Identity: output equals input.
    Linear,
    /// Slow start (`cubic-bezier(0.42, 0, 1, 1)`).
    EaseIn,
    /// Slow end (`cubic-bezier(0, 0, 0.58, 1)`).
    EaseOut,
    /// Slow start and end (`cubic-bezier(0.42, 0, 0.58, 1)`).
    EaseInOut,
    /// An arbitrary cubic Bézier with control points `(x1, y1)` and `(x2, y2)`;
    /// endpoints are fixed at `(0,0)` and `(1,1)` as in CSS.
    CubicBezier {
        /// First control-point x.
        x1: f32,
        /// First control-point y.
        y1: f32,
        /// Second control-point x.
        x2: f32,
        /// Second control-point y.
        y2: f32,
    },
}

impl Easing {
    /// Evaluate the curve at linear progress `t` (clamped to `[0, 1]`).
    pub fn eval(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match *self {
            Easing::Linear => t,
            Easing::EaseIn => cubic_bezier_eval(0.42, 0.0, 1.0, 1.0, t),
            Easing::EaseOut => cubic_bezier_eval(0.0, 0.0, 0.58, 1.0, t),
            Easing::EaseInOut => cubic_bezier_eval(0.42, 0.0, 0.58, 1.0, t),
            Easing::CubicBezier { x1, y1, x2, y2 } => cubic_bezier_eval(x1, y1, x2, y2, t),
        }
    }
}

/// One coordinate of a cubic Bézier with fixed endpoints 0 and 1, given the two
/// inner control values `c1`, `c2` and curve parameter `u ∈ [0, 1]`.
///
/// `B(u) = 3(1-u)²u·c1 + 3(1-u)u²·c2 + u³` (the `(1-u)³·0` term vanishes).
#[inline]
fn bezier_axis(c1: f32, c2: f32, u: f32) -> f32 {
    let one_minus = 1.0 - u;
    3.0 * one_minus * one_minus * u * c1 + 3.0 * one_minus * u * u * c2 + u * u * u
}

/// Derivative of [`bezier_axis`] with respect to `u`.
#[inline]
fn bezier_axis_deriv(c1: f32, c2: f32, u: f32) -> f32 {
    let one_minus = 1.0 - u;
    3.0 * one_minus * one_minus * c1 + 6.0 * one_minus * u * (c2 - c1) + 3.0 * u * u * (1.0 - c2)
}

/// Evaluate a CSS-style cubic Bézier easing at time `x ∈ [0, 1]`.
///
/// Solves `bezier_x(u) = x` for the curve parameter `u`, then returns
/// `bezier_y(u)`. Uses Newton–Raphson with a bisection fallback for robustness
/// when the x-derivative is tiny.
fn cubic_bezier_eval(x1: f32, y1: f32, x2: f32, y2: f32, x: f32) -> f32 {
    // Endpoints are exact regardless of control points.
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let u = solve_bezier_u_for_x(x1, x2, x);
    bezier_axis(y1, y2, u)
}

/// Solve `bezier_axis(x1, x2, u) == x` for `u ∈ [0, 1]`.
fn solve_bezier_u_for_x(x1: f32, x2: f32, x: f32) -> f32 {
    const NEWTON_ITERS: usize = 8;
    const EPS: f32 = 1e-6;

    // Newton–Raphson seeded at u = x (a good guess since x ∈ [0,1]).
    let mut u = x;
    for _ in 0..NEWTON_ITERS {
        let fx = bezier_axis(x1, x2, u) - x;
        if fx.abs() < EPS {
            return u.clamp(0.0, 1.0);
        }
        let d = bezier_axis_deriv(x1, x2, u);
        if d.abs() < 1e-6 {
            // Derivative ~ 0: Newton would explode. Hand off to bisection.
            break;
        }
        u -= fx / d;
        // Keep the iterate inside the valid domain.
        u = u.clamp(0.0, 1.0);
    }

    // Bisection fallback — guaranteed to converge because bezier_x is monotone
    // non-decreasing in u for valid CSS control points (x1, x2 ∈ [0, 1]).
    let mut lo = 0.0_f32;
    let mut hi = 1.0_f32;
    let mut mid = u.clamp(lo, hi);
    for _ in 0..32 {
        mid = 0.5 * (lo + hi);
        let fx = bezier_axis(x1, x2, mid);
        if (fx - x).abs() < EPS {
            return mid;
        }
        if fx < x {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    mid
}

/// A damped-harmonic-oscillator spring, solved in closed form.
///
/// Parameters follow the common "physical" convention: `mass`, `stiffness` (k)
/// and `damping` (c). The angular frequency is `ω₀ = √(k/m)` and the damping
/// ratio is `ζ = c / (2√(k·m))`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spring {
    /// Oscillator mass (`> 0`).
    pub mass: f32,
    /// Spring stiffness `k` (`> 0`).
    pub stiffness: f32,
    /// Damping coefficient `c` (`>= 0`).
    pub damping: f32,
}

impl Default for Spring {
    /// A snappy, slightly-underdamped UI spring.
    fn default() -> Self {
        Self {
            mass: 1.0,
            stiffness: 170.0,
            damping: 26.0,
        }
    }
}

impl Spring {
    /// Construct a spring from physical parameters.
    pub fn new(mass: f32, stiffness: f32, damping: f32) -> Self {
        Self {
            mass,
            stiffness,
            damping,
        }
    }

    /// Build a spring with the given natural frequency `ω₀` (rad/s) and damping
    /// ratio `ζ` (unit mass). `ζ = 1` is critically damped.
    pub fn from_frequency(omega0: f32, zeta: f32) -> Self {
        let mass = 1.0;
        let stiffness = omega0 * omega0 * mass;
        let damping = 2.0 * zeta * omega0 * mass;
        Self {
            mass,
            stiffness,
            damping,
        }
    }

    /// The undamped natural angular frequency `ω₀ = √(k/m)`.
    pub fn natural_frequency(&self) -> f32 {
        (self.stiffness / self.mass.max(f32::EPSILON)).sqrt()
    }

    /// The damping ratio `ζ`. `< 1` under-, `== 1` critically-, `> 1` over-damped.
    pub fn damping_ratio(&self) -> f32 {
        let denom = 2.0 * (self.stiffness * self.mass).max(f32::EPSILON).sqrt();
        self.damping / denom
    }

    /// Position at time `t` (seconds) for a spring released from displacement
    /// `from - to` with initial velocity `v0`, settling towards `to`.
    ///
    /// Returns the absolute position (already offset by `to`). The solution is
    /// the exact analytic response of `m·x'' + c·x' + k·x = 0`, so it is
    /// numerically stable at any `t` and frame rate.
    pub fn position(&self, from: f32, to: f32, v0: f32, t: f32) -> f32 {
        if t <= 0.0 {
            return from;
        }
        let x0 = from - to; // displacement from the rest position
        let omega0 = self.natural_frequency();
        if omega0 <= f32::EPSILON {
            return to + x0; // no restoring force: stays put
        }
        let zeta = self.damping_ratio();

        let offset = if (zeta - 1.0).abs() < 1e-4 {
            // Critically damped: x(t) = (x0 + (v0 + ω₀·x0)·t)·e^(−ω₀·t).
            let c2 = v0 + omega0 * x0;
            (x0 + c2 * t) * (-omega0 * t).exp()
        } else if zeta < 1.0 {
            // Under-damped: decaying oscillation.
            let omega_d = omega0 * (1.0 - zeta * zeta).sqrt();
            let decay = (-zeta * omega0 * t).exp();
            let a = x0;
            let b = (v0 + zeta * omega0 * x0) / omega_d;
            decay * (a * (omega_d * t).cos() + b * (omega_d * t).sin())
        } else {
            // Over-damped: sum of two real exponentials.
            let disc = (zeta * zeta - 1.0).sqrt();
            let r1 = -omega0 * (zeta - disc);
            let r2 = -omega0 * (zeta + disc);
            let c1 = (v0 - r2 * x0) / (r1 - r2);
            let c2 = x0 - c1;
            c1 * (r1 * t).exp() + c2 * (r2 * t).exp()
        };
        to + offset
    }

    /// Whether the spring has effectively settled at `to` by time `t`, within
    /// `tolerance` of the rest position.
    pub fn is_settled(&self, from: f32, to: f32, v0: f32, t: f32, tolerance: f32) -> bool {
        (self.position(from, to, v0, t) - to).abs() <= tolerance
    }
}

/// A property transition: an [`Easing`] applied over `duration` after `delay`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transition {
    /// Animation duration in seconds (`> 0`).
    pub duration: f32,
    /// Delay before the animation begins, in seconds (`>= 0`).
    pub delay: f32,
    /// The timing function.
    pub easing: Easing,
}

impl Transition {
    /// A transition over `duration` seconds with the given easing and no delay.
    pub fn new(duration: f32, easing: Easing) -> Self {
        Self {
            duration,
            delay: 0.0,
            easing,
        }
    }

    /// Builder: set the start delay.
    pub fn with_delay(mut self, delay: f32) -> Self {
        self.delay = delay;
        self
    }

    /// The eased progress `[0, 1]` at `elapsed` seconds since the transition was
    /// scheduled (accounts for `delay`). Returns `0` during the delay and `1`
    /// once `delay + duration` has passed.
    pub fn progress(&self, elapsed: f32) -> f32 {
        let active = elapsed - self.delay;
        if active <= 0.0 {
            return 0.0;
        }
        if self.duration <= 0.0 || active >= self.duration {
            return 1.0;
        }
        self.easing.eval(active / self.duration)
    }

    /// Interpolate a scalar from `start` to `end` at `elapsed` seconds.
    pub fn sample(&self, start: f32, end: f32, elapsed: f32) -> f32 {
        let p = self.progress(elapsed);
        start + (end - start) * p
    }

    /// Returns `true` once the transition (delay + duration) has completed.
    pub fn is_finished(&self, elapsed: f32) -> bool {
        elapsed >= self.delay + self.duration
    }
}

/// A single tracked animation from `start` to `end` over a [`Transition`].
#[derive(Clone, Copy, Debug)]
struct ActiveTransition {
    key: u64,
    start: f32,
    end: f32,
    transition: Transition,
    /// Total elapsed time the animation has been advanced.
    elapsed: f32,
}

/// Tracks a set of keyed transitions and advances them frame by frame.
///
/// Each animation is identified by a caller-chosen `u64` key; starting a new
/// animation with an existing key replaces it (so a re-triggered hover doesn't
/// stack). [`Animator::advance`] adds `dt` seconds to every active animation and
/// drops the finished ones.
#[derive(Debug, Default)]
pub struct Animator {
    active: Vec<ActiveTransition>,
}

impl Animator {
    /// Create an empty animator.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of currently-active animations.
    pub fn active_count(&self) -> usize {
        self.active.len()
    }

    /// Returns `true` if any animation is in flight.
    pub fn is_animating(&self) -> bool {
        !self.active.is_empty()
    }

    /// Start (or restart) the animation under `key`, interpolating
    /// `start → end` over `transition`. Any existing animation with the same
    /// key is replaced and its elapsed time reset.
    pub fn start(&mut self, key: u64, start: f32, end: f32, transition: Transition) {
        let entry = ActiveTransition {
            key,
            start,
            end,
            transition,
            elapsed: 0.0,
        };
        if let Some(slot) = self.active.iter_mut().find(|a| a.key == key) {
            *slot = entry;
        } else {
            self.active.push(entry);
        }
    }

    /// The current value of the animation under `key`, or `None` if no such
    /// animation is active.
    pub fn value(&self, key: u64) -> Option<f32> {
        self.active
            .iter()
            .find(|a| a.key == key)
            .map(|a| a.transition.sample(a.start, a.end, a.elapsed))
    }

    /// Advance every active animation by `dt` seconds, removing any that have
    /// finished. Returns the number still active afterwards.
    pub fn advance(&mut self, dt: f32) -> usize {
        for a in &mut self.active {
            a.elapsed += dt;
        }
        self.active.retain(|a| !a.transition.is_finished(a.elapsed));
        self.active.len()
    }

    /// Cancel the animation under `key`. Returns `true` if one was removed.
    pub fn cancel(&mut self, key: u64) -> bool {
        let before = self.active.len();
        self.active.retain(|a| a.key != key);
        self.active.len() != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    #[test]
    fn linear_easing_endpoints_and_midpoint() {
        assert_eq!(Easing::Linear.eval(0.0), 0.0);
        assert_eq!(Easing::Linear.eval(1.0), 1.0);
        assert!(close(Easing::Linear.eval(0.5), 0.5, 1e-6));
    }

    #[test]
    fn ease_in_out_is_symmetric_about_half() {
        let e = Easing::EaseInOut;
        // Endpoints exact.
        assert!(close(e.eval(0.0), 0.0, 1e-6));
        assert!(close(e.eval(1.0), 1.0, 1e-6));
        // Midpoint of a symmetric ease should be ~0.5.
        assert!(close(e.eval(0.5), 0.5, 1e-3), "got {}", e.eval(0.5));
        // Symmetry: f(t) + f(1-t) ≈ 1.
        for t in [0.1f32, 0.25, 0.4] {
            assert!(close(e.eval(t) + e.eval(1.0 - t), 1.0, 2e-3), "t={t}");
        }
    }

    #[test]
    fn ease_in_starts_slow() {
        // EaseIn output should lag behind linear in the first half.
        let e = Easing::EaseIn;
        assert!(
            e.eval(0.25) < 0.25,
            "ease-in should be below the diagonal early"
        );
        assert!(close(e.eval(1.0), 1.0, 1e-6));
    }

    #[test]
    fn cubic_bezier_recovers_linear() {
        // A bezier with collinear controls on the diagonal is the identity.
        let lin = Easing::CubicBezier {
            x1: 0.25,
            y1: 0.25,
            x2: 0.75,
            y2: 0.75,
        };
        for t in [0.0f32, 0.2, 0.5, 0.8, 1.0] {
            assert!(close(lin.eval(t), t, 2e-3), "t={t} got {}", lin.eval(t));
        }
    }

    #[test]
    fn cubic_bezier_degenerate_does_not_nan() {
        // (0,1,1,0): zero x-derivative at both ends — the Newton fallback path.
        let e = Easing::CubicBezier {
            x1: 0.0,
            y1: 1.0,
            x2: 1.0,
            y2: 0.0,
        };
        for i in 0..=10 {
            let t = i as f32 / 10.0;
            let v = e.eval(t);
            assert!(v.is_finite(), "value at t={t} must be finite, got {v}");
            assert!((0.0..=1.0).contains(&v) || close(v, 0.0, 1e-3) || close(v, 1.0, 1e-3));
        }
    }

    #[test]
    fn spring_critically_damped_converges_without_overshoot() {
        let s = Spring::from_frequency(20.0, 1.0); // zeta = 1
        assert!(close(s.damping_ratio(), 1.0, 1e-3));
        // From 0 to 1, no initial velocity.
        let mut prev = s.position(0.0, 1.0, 0.0, 0.0);
        assert!(close(prev, 0.0, 1e-4));
        // Monotone approach (critically damped never overshoots).
        for i in 1..=60 {
            let t = i as f32 / 60.0;
            let p = s.position(0.0, 1.0, 0.0, t);
            assert!(p <= 1.0 + 1e-3, "overshoot at t={t}: {p}");
            assert!(p >= prev - 1e-4, "should be monotone increasing at t={t}");
            prev = p;
        }
        assert!(s.is_settled(0.0, 1.0, 0.0, 1.5, 1e-2));
    }

    #[test]
    fn spring_underdamped_overshoots_then_settles() {
        let s = Spring::from_frequency(30.0, 0.3); // lightly damped
        assert!(s.damping_ratio() < 1.0);
        let mut max = f32::MIN;
        for i in 0..=200 {
            let t = i as f32 / 100.0;
            max = max.max(s.position(0.0, 1.0, 0.0, t));
        }
        assert!(
            max > 1.0,
            "underdamped spring should overshoot the target, max={max}"
        );
        // And it should be settled after enough time.
        assert!(s.is_settled(0.0, 1.0, 0.0, 5.0, 2e-2));
    }

    #[test]
    fn spring_overdamped_no_overshoot() {
        let s = Spring::from_frequency(10.0, 2.0); // zeta = 2
        assert!(s.damping_ratio() > 1.0);
        for i in 0..=100 {
            let t = i as f32 / 50.0;
            let p = s.position(0.0, 1.0, 0.0, t);
            assert!(
                p <= 1.0 + 1e-3,
                "overdamped must not overshoot, t={t} p={p}"
            );
        }
    }

    #[test]
    fn transition_progress_respects_delay_and_duration() {
        let tr = Transition::new(2.0, Easing::Linear).with_delay(1.0);
        assert_eq!(tr.progress(0.5), 0.0); // still in delay
        assert!(close(tr.progress(2.0), 0.5, 1e-6)); // 1s into a 2s anim
        assert_eq!(tr.progress(3.0), 1.0); // finished
        assert!(tr.is_finished(3.0));
        assert!(!tr.is_finished(2.5));
        assert!(close(tr.sample(10.0, 20.0, 2.0), 15.0, 1e-4));
    }

    #[test]
    fn animator_tracks_and_drops_finished() {
        let mut anim = Animator::new();
        anim.start(1, 0.0, 100.0, Transition::new(1.0, Easing::Linear));
        assert!(anim.is_animating());
        assert!(close(anim.value(1).expect("active"), 0.0, 1e-4));
        anim.advance(0.5);
        assert!(close(anim.value(1).expect("active"), 50.0, 1e-3));
        // Restart with same key resets elapsed.
        anim.start(1, 0.0, 100.0, Transition::new(1.0, Easing::Linear));
        assert!(close(anim.value(1).expect("active"), 0.0, 1e-4));
        // Advance past the end -> dropped.
        anim.advance(1.5);
        assert_eq!(anim.active_count(), 0);
        assert!(anim.value(1).is_none());
    }

    #[test]
    fn animator_cancel() {
        let mut anim = Animator::new();
        anim.start(7, 0.0, 1.0, Transition::new(1.0, Easing::Linear));
        assert!(anim.cancel(7));
        assert!(!anim.cancel(7));
    }
}
