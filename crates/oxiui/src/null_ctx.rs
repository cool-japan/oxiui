//! Shared no-op [`UiCtx`] used off-frame.
//!
//! `NullUiCtx` is a [`UiCtx`] implementation whose widget methods do nothing.
//! It is used wherever OxiUI must drive user-supplied closures without a live
//! backend frame:
//!
//! * the headless paths ([`crate::App::run_headless_once`] /
//!   [`crate::App::run_with_return`]), and
//! * lifecycle hooks (`on_close` / `on_resize` / `on_focus`), which fire from a
//!   window event — outside any live `EguiUiCtx` / `IcedUiCtx` frame — where
//!   there is no real drawing surface to bind to.

use oxiui_core::{ButtonResponse, UiCtx};

/// A no-op [`UiCtx`]: every widget call is ignored and buttons never report a click.
///
/// This is deliberately zero-sized and cheap to construct on demand.
pub(crate) struct NullUiCtx;

impl UiCtx for NullUiCtx {
    fn heading(&mut self, _text: &str) {}
    fn label(&mut self, _text: &str) {}
    fn button(&mut self, _label: &str) -> ButtonResponse {
        ButtonResponse::default()
    }
}
