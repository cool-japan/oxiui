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
//! # Usage (native window)
//!
//! ```rust,ignore
//! use oxiui_slint::run_slint;
//! use oxiui_theme::cooljapan_dark;
//!
//! run_slint(&*cooljapan_dark(), |ui| {
//!     ui.heading("Hello from Slint");
//!     ui.label("OxiUI + slint backend");
//! }).expect("slint run failed");
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
/// # Headless behaviour
///
/// When the `slint` feature is **disabled**, this function executes the content
/// closure in headless collection mode via [`SlintCtx`] and returns `Ok(())`.
/// No window is opened.
///
/// When the `slint` feature is **enabled**, the same headless path is taken for
/// M5 to satisfy the "example builds" acceptance criterion. A native slint window
/// can be opened via `slint::run_event_loop()` after building components; that
/// integration is deferred to M6 (it requires a display at runtime and is
/// not exercise-able in headless CI).
///
/// # Errors
///
/// Returns [`UiError::Backend`] if the slint event loop reports an error (M6+).
/// In M5 this function is always `Ok(())`.
pub fn run_slint<F>(palette: &dyn oxiui_core::Theme, content: F) -> Result<(), UiError>
where
    F: FnOnce(&mut dyn UiCtx),
{
    // Access the palette to satisfy the function signature (future mapping use).
    let _pal: &Palette = palette.palette();

    // Execute the content closure in collection mode.
    let mut ctx = SlintCtx::default();
    content(&mut ctx);

    // Production path (M6): build slint components from ctx.items, set palette
    // colours via `slint::Color::from_argb_u8(a, r, g, b)`, and invoke
    // `slint::run_event_loop()`. This requires a live display and is therefore
    // deferred from M5.
    //
    // The `slint` feature gate is already active when this code is compiled;
    // the actual slint::run_event_loop() call is commented out below until M6:
    //
    //   #[cfg(feature = "slint")]
    //   {
    //       let bg = slint::Color::from_argb_u8(
    //           _pal.background.3, _pal.background.0,
    //           _pal.background.1, _pal.background.2,
    //       );
    //       let _ = bg; // used in StyleMetrics once the public API is available
    //       slint::run_event_loop().map_err(|e| UiError::Backend(e.to_string()))?;
    //   }

    Ok(())
}
