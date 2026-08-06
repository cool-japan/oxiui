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
//! # Usage
//!
//! Native window rendering is not yet wired, so [`run_dioxus`] currently returns
//! [`UiError::Unsupported`]. For headless widget collection, drive a
//! [`DioxusCtx`] directly:
//!
//! ```rust
//! use oxiui_core::UiCtx;
//! use oxiui_dioxus::DioxusCtx;
//!
//! let mut ctx = DioxusCtx::default();
//! ctx.heading("Hello from Dioxus");
//! ctx.label("OxiUI + dioxus backend");
//! assert_eq!(ctx.items.len(), 2);
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
/// # Window behaviour
///
/// This function is contracted to launch a Dioxus runtime and open a window.
/// That path (translating the collected widget tree into an `rsx!` element tree
/// and calling `dioxus::launch`) is **not yet wired**: the Pure-Rust desktop
/// renderer `dioxus-native` (Blitz/Vello) is not yet stable, and the `desktop`
/// feature pulls C/C++ system deps that violate the Pure Rust policy. Until a
/// Pure-Rust launch path lands, `run_dioxus` returns a typed
/// [`UiError::Unsupported`] rather than silently reporting a successful run that
/// never opened a window.
///
/// For headless widget collection, construct a [`DioxusCtx`] directly and drive
/// it with your content closure — that path is fully supported and testable
/// without a display or any heavy dependencies.
///
/// # Errors
///
/// Always returns [`UiError::Unsupported`]: native Dioxus window rendering is not
/// yet implemented. Once the launch path is wired, this will instead return
/// [`UiError::Backend`] if the Dioxus runtime reports an error.
pub fn run_dioxus<F>(palette: &dyn oxiui_core::Theme, content: F) -> Result<(), UiError>
where
    F: FnOnce(&mut dyn UiCtx),
{
    // Do NOT run the content closure or fabricate a successful exit: launching a
    // Dioxus window is not implemented (see the doc comment). Surface an explicit
    // typed error so callers can tell a real window run apart from a no-op. The
    // parameters are consumed here only to keep the signature stable.
    let _pal: &Palette = palette.palette();
    let _ = content;

    Err(UiError::Unsupported(
        "oxiui-dioxus: native window rendering via dioxus::launch is not yet \
         implemented (dioxus-native is not yet stable); construct a DioxusCtx \
         directly for headless widget collection"
            .to_string(),
    ))
}
