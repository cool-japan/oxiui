#![forbid(unsafe_code)]
#![warn(missing_docs)]
//! `oxiui-slint` — Slint adapter for OxiUI.
//!
//! Provides [`SlintCtx`] which implements [`UiCtx`] by collecting widget calls,
//! and [`run_slint`] which drives a content closure through slint rendering.
//!
//! # Feature gate
//!
//! This crate is useful even with `default = []`:
//! - [`SlintCtx`] can be constructed and tested without the `slint` feature
//!   (collection mode only).
//! - Enable the `slint` feature to activate slint rendering in [`run_slint`].
//!
//! # License note
//!
//! The `slint` crate is licensed under GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0
//! OR LicenseRef-Slint-Software-3.0. Enabling the `slint` feature of this crate
//! brings in that dependency. Downstream consumers must ensure their project's
//! license is compatible with one of slint's license options.
//!
//! # Purity note (COOLJAPAN Pure Rust Policy v2)
//!
//! **NON-PURE adapter.** Enabling the `slint` feature pulls
//! `slint` -> parley/fontique -> `yeslogic-fontconfig-sys` (a C fontconfig
//! binding) on Linux. This is slint-upstream font discovery with no pure
//! opt-out today, so `oxiui-slint` is **NOT** part of the OxiUI Pure-Rust L1
//! set. Depend on it directly only if you accept that boundary.
//!
//! # Palette mapping note (M5)
//!
//! slint 1.16.1 exposes a `Color::from_argb_u8(a, r, g, b)` constructor and
//! per-component accessors through the `slint::Color` type (available under
//! `renderer-software` feature, no `backend-winit` required). However,
//! slint's global style/theme API does not expose a pluggable external palette
//! injection seam in 1.16.1: the `StyleMetrics` struct (lightly documented)
//! is set internally and not public. For M5 we document this as a known gap
//! and proceed with default slint styling. Full palette mapping is planned for
//! M6 once a public API seam is confirmed.
//!
//! # Usage
//!
//! Native window rendering is not yet wired, so [`run_slint`] currently returns
//! [`UiError::Unsupported`]. For headless widget collection, drive a
//! [`SlintCtx`] directly:
//!
//! ```rust
//! use oxiui_core::UiCtx;
//! use oxiui_slint::SlintCtx;
//!
//! let mut ctx = SlintCtx::default();
//! ctx.heading("Hello from Slint");
//! ctx.label("OxiUI + slint backend");
//! assert_eq!(ctx.items.len(), 2);
//! ```

pub mod ctx;

pub use ctx::SlintCtx;

use oxiui_core::{Palette, UiCtx, UiError};

/// Run a slint-backed UI frame with the given palette and content closure.
///
/// # Palette mapping
///
/// slint 1.16.1 does not expose a public pluggable palette/theme injection API.
/// The `palette` argument is available for downstream consumers who use
/// `slint::Color::from_argb_u8` directly in their component definitions; it
/// is not automatically applied to slint's global style in M5 (see the crate-level
/// note).
///
/// # Window behaviour
///
/// This function is contracted to open a native slint window and run its event
/// loop. That integration (`slint::run_event_loop` after building slint
/// components from the collected widget tree) is **not yet wired**: it requires
/// a live display at runtime (not exercise-able in headless CI) and the `slint`
/// dependency is GPL-gated and off by default. Until it lands, `run_slint`
/// returns a typed [`UiError::Unsupported`] rather than silently reporting a
/// successful run that never opened a window.
///
/// For headless widget collection, construct a [`SlintCtx`] directly and drive
/// it with your content closure — that path is fully supported and testable
/// without a display.
///
/// # Errors
///
/// Always returns [`UiError::Unsupported`]: native slint window rendering is not
/// yet implemented. Once the event loop is wired, this will instead return
/// [`UiError::Backend`] if slint's event loop reports an error.
pub fn run_slint<F>(palette: &dyn oxiui_core::Theme, content: F) -> Result<(), UiError>
where
    F: FnOnce(&mut dyn UiCtx),
{
    // Do NOT run the content closure or fabricate a successful exit: opening a
    // native slint window is not implemented (see the doc comment). Surface an
    // explicit typed error so callers can tell a real window run apart from a
    // no-op. The parameters are consumed here only to keep the signature stable.
    let _pal: &Palette = palette.palette();
    let _ = content;

    Err(UiError::Unsupported(
        "oxiui-slint: native window rendering via slint::run_event_loop is not \
         yet implemented; construct a SlintCtx directly for headless widget \
         collection"
            .to_string(),
    ))
}
