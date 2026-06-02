#![forbid(unsafe_code)]
#![warn(missing_docs)]
//! `oxiui-dioxus` — Dioxus adapter for OxiUI.
//!
//! Provides [`DioxusCtx`] which implements [`UiCtx`] by collecting widget calls,
//! and [`run_dioxus`] which drives a content closure through the Dioxus rendering
//! pipeline.
//!
//! # Feature gate
//!
//! This crate is usable with `default = []`:
//! - [`DioxusCtx`] can be constructed and tested without the `dioxus` feature
//!   (collection mode only, no heavy dependencies).
//! - Enable the `dioxus` feature to activate Dioxus rendering in [`run_dioxus`].
//!
//! # Architecture note (M5)
//!
//! Dioxus 0.7 is a reactive, component-based framework: components are functions
//! that return `Element` via the `rsx!` macro. This contrasts with OxiUI's
//! immediate-mode `UiCtx` closure approach. The M5 bridge operates as follows:
//!
//! 1. The content closure is executed against a [`DioxusCtx`], collecting widget
//!    descriptions in `DioxusCtx::items`.
//! 2. (M6) Those items are translated into a Dioxus `rsx!` element tree and
//!    passed to `dioxus::launch()` as the root component.
//!
//! The `desktop` feature of dioxus (wry/tao, WebKit, Chromium) is intentionally
//! **not** used: it pulls in C/C++ system dependencies that violate the
//! Pure Rust policy. Instead the `minimal` feature set is used for M5:
//! `["macro", "html", "signals", "hooks", "launch"]` — all Pure Rust.
//!
//! Full desktop rendering (M6) will use `dioxus-native` (the Pure Rust Vello/
//! Blitz-based renderer) once it stabilises.
//!
//! # Palette mapping note (M5)
//!
//! Dioxus 0.7 renders via CSS-in-Rust (inline `style=""` attributes).
//! The `palette` argument passed to [`run_dioxus`] is available for downstream
//! consumers who format `style` strings from the palette colours; it is not
//! automatically injected in M5. A helper `palette_to_css_vars()` is planned
//! for M6 to emit `:root { --background: #rrggbb; ... }` global CSS.
//!
//! # Usage (headless)
//!
//! ```rust,ignore
//! use oxiui_dioxus::run_dioxus;
//! use oxiui_theme::cooljapan_dark;
//!
//! run_dioxus(&*cooljapan_dark(), |ui| {
//!     ui.heading("Hello from Dioxus");
//!     ui.label("OxiUI + dioxus backend");
//! }).expect("run_dioxus should be Ok");
//! ```

pub mod ctx;

pub use ctx::DioxusCtx;

use oxiui_core::{Palette, UiCtx, UiError};

/// Run a Dioxus-backed UI frame with the given theme and content closure.
///
/// # Palette mapping
///
/// Dioxus renders via CSS `style` attributes. The `palette` argument's
/// colours are available for inline-style use in M6+; they are not
/// automatically applied in M5 (see the crate-level note).
///
/// # Behaviour in M5
///
/// In M5 this function executes the content closure in headless collection
/// mode via [`DioxusCtx`] and returns `Ok(())`. No window is opened, no
/// Dioxus runtime is started. This satisfies the "example builds" acceptance
/// criterion without requiring a display or any C/C++ deps (wry/tao are not
/// used).
///
/// The full Dioxus launch path (M6) will use `dioxus-native` (Pure Rust Blitz
/// renderer) and looks like:
///
/// ```rust,ignore
/// #[cfg(feature = "dioxus")]
/// {
///     let items = ctx.items.clone();
///     dioxus::launch(move || {
///         // translate `items` into rsx! element tree
///         rsx! { /* ... */ }
///     });
/// }
/// ```
///
/// # Errors
///
/// Returns [`UiError::Backend`] if the Dioxus launch reports an error (M6+).
/// In M5 this function is always `Ok(())`.
pub fn run_dioxus<F>(palette: &dyn oxiui_core::Theme, content: F) -> Result<(), UiError>
where
    F: FnOnce(&mut dyn UiCtx),
{
    // Access the palette for future CSS variable generation.
    let _pal: &Palette = palette.palette();

    // Execute the content closure in collection mode.
    let mut ctx = DioxusCtx::default();
    content(&mut ctx);

    // Production path (M6): translate ctx.items into a dioxus `rsx!` tree
    // and call `dioxus::launch()`. Requires dioxus-native (Pure Rust renderer).
    // This is deferred from M5 — see crate doc comment for rationale.

    Ok(())
}
