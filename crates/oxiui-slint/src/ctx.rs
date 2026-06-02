//! [`SlintCtx`] — OxiUI [`UiCtx`] implementation that collects widget calls.
//!
//! In M5, `SlintCtx` operates in "collection mode": each `UiCtx` call appends
//! a description string to `items`. The production path renders these items
//! through slint's component API; the headless/test path inspects `items`
//! directly without opening a window.

use oxiui_core::{ButtonResponse, UiCtx};

/// A [`UiCtx`] adapter that collects widget descriptions for slint rendering.
///
/// Widget calls are recorded in `items` in the order they are invoked.
/// In headless mode (feature `slint` absent, or when no display is available),
/// the collected items can be inspected directly for testing.
///
/// # Example
/// ```
/// use oxiui_slint::SlintCtx;
/// use oxiui_core::UiCtx;
///
/// let mut ctx = SlintCtx::default();
/// ctx.heading("My Window");
/// ctx.label("Status: ok");
/// let resp = ctx.button("Continue");
/// assert_eq!(ctx.items.len(), 3);
/// ```
#[derive(Debug, Default)]
pub struct SlintCtx {
    /// Widget descriptions collected during a content-closure pass.
    ///
    /// Format: `"<kind>:<text>"` e.g. `"label:Hello"`, `"button:Quit"`.
    pub items: Vec<String>,
}

impl UiCtx for SlintCtx {
    fn heading(&mut self, text: &str) {
        self.items.push(format!("heading:{text}"));
    }

    fn label(&mut self, text: &str) {
        self.items.push(format!("label:{text}"));
    }

    fn button(&mut self, label: &str) -> ButtonResponse {
        self.items.push(format!("button:{label}"));
        // In headless/collection mode buttons are never clicked.
        // A real slint window would return interaction state here.
        ButtonResponse {
            clicked: false,
            hovered: false,
        }
    }
}
