//! Responsive design helpers for OxiUI web.
//!
//! Provides:
//! - `Breakpoint` — a named set of viewport width breakpoints mirroring
//!   common CSS frameworks (xs / sm / md / lg / xl / xxl).
//! - `detect_breakpoint` — synchronously detects the current viewport
//!   breakpoint on wasm32 using `window.innerWidth`.
//! - `on_breakpoint_change` — installs `matchMedia` listeners that fire the
//!   callback whenever the breakpoint changes.
//! - `detect_color_scheme` — detects the user's `prefers-color-scheme`
//!   preference.
//! - `detect_reduced_motion` — detects `prefers-reduced-motion`.
//!
//! All functions are fully testable on native targets via stub implementations.

// ── Breakpoints ───────────────────────────────────────────────────────────────

/// Named viewport width breakpoints.
///
/// The thresholds mirror the CSS framework convention (Bootstrap-style):
///
/// | Variant | Min-width |
/// |---------|-----------|
/// | `Xs`    | 0 px      |
/// | `Sm`    | 576 px    |
/// | `Md`    | 768 px    |
/// | `Lg`    | 992 px    |
/// | `Xl`    | 1200 px   |
/// | `Xxl`   | 1400 px   |
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Breakpoint {
    /// Extra-small — below 576 px.
    Xs,
    /// Small — 576 px and above.
    Sm,
    /// Medium — 768 px and above.
    Md,
    /// Large — 992 px and above.
    Lg,
    /// Extra-large — 1200 px and above.
    Xl,
    /// Extra-extra-large — 1400 px and above.
    Xxl,
}

impl Breakpoint {
    /// Minimum viewport width (in pixels) for this breakpoint.
    pub fn min_width(self) -> u32 {
        match self {
            Breakpoint::Xs => 0,
            Breakpoint::Sm => 576,
            Breakpoint::Md => 768,
            Breakpoint::Lg => 992,
            Breakpoint::Xl => 1200,
            Breakpoint::Xxl => 1400,
        }
    }

    /// Determine the breakpoint that corresponds to a given viewport width.
    pub fn from_width(width: u32) -> Self {
        match width {
            w if w >= 1400 => Breakpoint::Xxl,
            w if w >= 1200 => Breakpoint::Xl,
            w if w >= 992 => Breakpoint::Lg,
            w if w >= 768 => Breakpoint::Md,
            w if w >= 576 => Breakpoint::Sm,
            _ => Breakpoint::Xs,
        }
    }

    /// Returns `true` if this breakpoint is at least as wide as `other`.
    pub fn at_least(self, other: Breakpoint) -> bool {
        self >= other
    }
}

impl std::fmt::Display for Breakpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Breakpoint::Xs => "xs",
            Breakpoint::Sm => "sm",
            Breakpoint::Md => "md",
            Breakpoint::Lg => "lg",
            Breakpoint::Xl => "xl",
            Breakpoint::Xxl => "xxl",
        };
        write!(f, "{s}")
    }
}

// ── Color scheme preference ───────────────────────────────────────────────────

/// The user's `prefers-color-scheme` media query preference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorScheme {
    /// `prefers-color-scheme: dark`
    Dark,
    /// `prefers-color-scheme: light` (or no preference)
    Light,
}

// ── Reduced motion preference ─────────────────────────────────────────────────

/// Whether the user prefers reduced motion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReducedMotion {
    /// `prefers-reduced-motion: reduce`
    Reduce,
    /// No preference or `prefers-reduced-motion: no-preference`
    NoPreference,
}

// ── Detection functions ───────────────────────────────────────────────────────

/// Detect the current viewport breakpoint.
///
/// On `wasm32` this reads `window.innerWidth` and maps it through
/// [`Breakpoint::from_width`].
///
/// On non-wasm targets this returns [`Breakpoint::Md`] as a neutral default.
pub fn detect_breakpoint() -> Breakpoint {
    #[cfg(target_arch = "wasm32")]
    {
        let width = web_sys::window()
            .and_then(|w| w.inner_width().ok())
            .and_then(|v| v.as_f64())
            .map(|f| f as u32)
            .unwrap_or(0);
        Breakpoint::from_width(width)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Breakpoint::Md
    }
}

/// Detect the user's preferred color scheme.
///
/// On `wasm32` this evaluates `(prefers-color-scheme: dark)` via
/// `window.matchMedia`.
///
/// On non-wasm targets this returns [`ColorScheme::Light`].
pub fn detect_color_scheme() -> ColorScheme {
    #[cfg(target_arch = "wasm32")]
    {
        let dark = web_sys::window()
            .and_then(|w| w.match_media("(prefers-color-scheme: dark)").ok())
            .flatten()
            .map(|mq| mq.matches())
            .unwrap_or(false);
        if dark {
            ColorScheme::Dark
        } else {
            ColorScheme::Light
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        ColorScheme::Light
    }
}

/// Detect the user's `prefers-reduced-motion` preference.
///
/// On `wasm32` this evaluates `(prefers-reduced-motion: reduce)`.
/// On non-wasm targets this returns [`ReducedMotion::NoPreference`].
pub fn detect_reduced_motion() -> ReducedMotion {
    #[cfg(target_arch = "wasm32")]
    {
        let reduce = web_sys::window()
            .and_then(|w| w.match_media("(prefers-reduced-motion: reduce)").ok())
            .flatten()
            .map(|mq| mq.matches())
            .unwrap_or(false);
        if reduce {
            ReducedMotion::Reduce
        } else {
            ReducedMotion::NoPreference
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        ReducedMotion::NoPreference
    }
}

// ── Listeners ─────────────────────────────────────────────────────────────────

/// Install a media-query listener that fires `callback` whenever the
/// breakpoint crosses a threshold.
///
/// On `wasm32` this installs one `addListener` / `addEventListener` call per
/// breakpoint boundary.  Each fires when the query match state changes,
/// invoking `callback` with the new current breakpoint (re-detected from
/// `window.innerWidth`).
///
/// On non-wasm targets this is always `Ok(())`.
///
/// # Errors
///
/// Returns `Err` if any media-query API call fails.
#[allow(unused_variables)]
pub fn on_breakpoint_change<F>(callback: F) -> Result<(), String>
where
    F: Fn(Breakpoint) + 'static,
{
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::{closure::Closure, JsCast};

        let window = web_sys::window()
            .ok_or_else(|| "on_breakpoint_change: no window available".to_string())?;

        let cb = std::sync::Arc::new(callback);

        // Breakpoint boundaries as (min-width query, breakpoint at that threshold).
        let boundaries: &[(&str, Breakpoint)] = &[
            ("(min-width: 576px)", Breakpoint::Sm),
            ("(min-width: 768px)", Breakpoint::Md),
            ("(min-width: 992px)", Breakpoint::Lg),
            ("(min-width: 1200px)", Breakpoint::Xl),
            ("(min-width: 1400px)", Breakpoint::Xxl),
        ];

        for &(query, _bp) in boundaries {
            let mq = window
                .match_media(query)
                .map_err(|_| format!("on_breakpoint_change: matchMedia failed for '{query}'"))?
                .ok_or_else(|| {
                    format!("on_breakpoint_change: null MediaQueryList for '{query}'")
                })?;

            let cb_clone = std::sync::Arc::clone(&cb);
            let closure = Closure::<dyn FnMut(web_sys::MediaQueryListEvent)>::wrap(Box::new(
                move |_e: web_sys::MediaQueryListEvent| {
                    cb_clone(detect_breakpoint());
                },
            ));

            mq.add_event_listener_with_callback("change", closure.as_ref().unchecked_ref())
                .map_err(|_| {
                    format!("on_breakpoint_change: addEventListener failed for '{query}'")
                })?;
            closure.forget();
        }

        Ok(())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Ok(())
    }
}

/// Listen for `orientationchange` events and invoke the callback.
///
/// On non-wasm targets this is always `Ok(())`.
#[allow(unused_variables)]
pub fn on_orientation_change<F>(callback: F) -> Result<(), String>
where
    F: Fn() + 'static,
{
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::{closure::Closure, JsCast};

        let window = web_sys::window()
            .ok_or_else(|| "on_orientation_change: no window available".to_string())?;

        let closure = Closure::<dyn FnMut()>::wrap(Box::new(callback));
        window
            .add_event_listener_with_callback("orientationchange", closure.as_ref().unchecked_ref())
            .map_err(|_| {
                "on_orientation_change: failed to add orientationchange listener".to_string()
            })?;
        closure.forget();
        Ok(())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Breakpoint::from_width ─────────────────────────────────────────────────

    #[test]
    fn breakpoint_from_width_xs() {
        assert_eq!(Breakpoint::from_width(0), Breakpoint::Xs);
        assert_eq!(Breakpoint::from_width(575), Breakpoint::Xs);
    }

    #[test]
    fn breakpoint_from_width_sm() {
        assert_eq!(Breakpoint::from_width(576), Breakpoint::Sm);
        assert_eq!(Breakpoint::from_width(767), Breakpoint::Sm);
    }

    #[test]
    fn breakpoint_from_width_md() {
        assert_eq!(Breakpoint::from_width(768), Breakpoint::Md);
        assert_eq!(Breakpoint::from_width(991), Breakpoint::Md);
    }

    #[test]
    fn breakpoint_from_width_lg() {
        assert_eq!(Breakpoint::from_width(992), Breakpoint::Lg);
        assert_eq!(Breakpoint::from_width(1199), Breakpoint::Lg);
    }

    #[test]
    fn breakpoint_from_width_xl() {
        assert_eq!(Breakpoint::from_width(1200), Breakpoint::Xl);
        assert_eq!(Breakpoint::from_width(1399), Breakpoint::Xl);
    }

    #[test]
    fn breakpoint_from_width_xxl() {
        assert_eq!(Breakpoint::from_width(1400), Breakpoint::Xxl);
        assert_eq!(Breakpoint::from_width(2560), Breakpoint::Xxl);
    }

    #[test]
    fn breakpoint_min_widths() {
        assert_eq!(Breakpoint::Xs.min_width(), 0);
        assert_eq!(Breakpoint::Sm.min_width(), 576);
        assert_eq!(Breakpoint::Md.min_width(), 768);
        assert_eq!(Breakpoint::Lg.min_width(), 992);
        assert_eq!(Breakpoint::Xl.min_width(), 1200);
        assert_eq!(Breakpoint::Xxl.min_width(), 1400);
    }

    #[test]
    fn breakpoint_at_least() {
        assert!(Breakpoint::Lg.at_least(Breakpoint::Md));
        assert!(Breakpoint::Md.at_least(Breakpoint::Md));
        assert!(!Breakpoint::Sm.at_least(Breakpoint::Md));
    }

    #[test]
    fn breakpoint_display() {
        assert_eq!(Breakpoint::Xs.to_string(), "xs");
        assert_eq!(Breakpoint::Xxl.to_string(), "xxl");
    }

    #[test]
    fn breakpoint_ordering() {
        assert!(Breakpoint::Xxl > Breakpoint::Xl);
        assert!(Breakpoint::Xs < Breakpoint::Sm);
    }

    // ── Native stubs ──────────────────────────────────────────────────────────

    #[test]
    fn detect_breakpoint_returns_md_on_native() {
        assert_eq!(detect_breakpoint(), Breakpoint::Md);
    }

    #[test]
    fn detect_color_scheme_returns_light_on_native() {
        assert_eq!(detect_color_scheme(), ColorScheme::Light);
    }

    #[test]
    fn detect_reduced_motion_returns_no_preference_on_native() {
        assert_eq!(detect_reduced_motion(), ReducedMotion::NoPreference);
    }

    #[test]
    fn on_breakpoint_change_ok_on_native() {
        assert!(on_breakpoint_change(|_bp| {}).is_ok());
    }

    #[test]
    fn on_orientation_change_ok_on_native() {
        assert!(on_orientation_change(|| {}).is_ok());
    }
}
