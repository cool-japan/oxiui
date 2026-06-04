#![forbid(unsafe_code)]
#![warn(missing_docs)]
//! iced backend adapter for OxiUI.
//!
//! Bridges OxiUI's `UiCtx` trait (immediate-mode) to iced's retained-mode
//! widget tree (best-effort mapping at M2).
//!
//! # Architecture note
//!
//! iced is a retained-mode framework (Elm-style update/view loop), whereas
//! OxiUI's `UiCtx` is immediate-mode (per-frame closure). `IcedUiCtx` bridges
//! the gap by collecting widget operations from a content closure and building
//! an iced `Column` from the collected elements. The mapping is one-way and
//! best-effort at M2; M3 wired the full message round-trip.
//!
//! # IME CJK note (M4)
//!
//! iced 0.14 does not expose a public per-widget IME injection API at the level
//! of `UiEvent`. IME events are surfaced as `UiEvent::ImePreedit` /
//! `UiEvent::ImeCommit` through the `oxiui-core` event sink, but no direct
//! iced widget action is generated for them at this milestone. See
//! [`forward_ime_event`] for the stub that documents the gap.

#[cfg(feature = "a11y")]
pub mod a11y_bridge;
pub mod adapter;
pub mod theme;

#[cfg(feature = "a11y")]
pub use a11y_bridge::{spec_to_a11y_node, spec_to_a11y_tree, IcedA11yConfig};
pub use adapter::{
    apply_message, map_iced_key, map_iced_keyboard_event, map_iced_modifiers, oxi_widget,
    spec_fingerprint, IcedConfig, IcedNullCtx, IcedSpan, IcedUiCtx, Message, OxiIcedWidget,
    SpecCache, ThemeCache, WidgetSpec, WidgetState,
};
pub use theme::{
    palette_and_tokens_to_iced_theme, palette_to_iced_theme, palette_to_iced_theme_ext,
    scrollable_style_from_palette, scrollable_style_from_theme, text_input_style_from_palette,
    text_input_style_from_theme, DesignTokensAdapter,
};

use oxiui_core::UiEvent;

/// Forward an OxiUI [`UiEvent`] to the iced backend.
///
/// # IME support (M4 — best-effort)
///
/// iced 0.14 does not expose a public API for injecting IME composition events
/// directly into a text-input widget. This function no-ops on IME events and
/// logs a debug message for diagnostics. Full IME support requires iced to
/// expose a `TextInput::ime_preedit` / `ime_commit` API, which is tracked as a
/// future enhancement.
///
/// All other event variants are silently ignored.
pub fn forward_ime_event(event: &UiEvent) {
    match event {
        UiEvent::ImePreedit { text, cursor: _ } => {
            // iced 0.14 has no public IME injection API.
            // Log for diagnostics; a future iced release may expose one.
            let _ = text; // suppress unused-variable warning
        }
        UiEvent::ImeCommit(text) => {
            // iced 0.14: no direct text-input injection — best-effort stub.
            let _ = text;
        }
        _ => {}
    }
}
