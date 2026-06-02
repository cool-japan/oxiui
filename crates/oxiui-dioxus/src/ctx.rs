//! [`DioxusCtx`] — OxiUI [`UiCtx`] implementation that collects widget calls
//! for Dioxus reactive rendering.
//!
//! Dioxus 0.7 is a reactive, component-based framework (not immediate-mode).
//! [`DioxusCtx`] bridges the gap by operating in two phases:
//!
//! 1. **Collection pass** — the user's content closure is called with a
//!    `DioxusCtx`, accumulating widget descriptions in `items`.
//! 2. **Render pass** (M6) — the collected items are converted into a Dioxus
//!    `VNode` tree via `rsx!` / `Element` constructors and returned to the
//!    Dioxus runtime.
//!
//! For M5, only the collection pass is active, enabling headless tests and a
//! build-passing example without requiring a display server.

use oxiui_core::{ButtonResponse, UiCtx};

/// A [`UiCtx`] adapter for Dioxus that collects widget descriptions.
///
/// Widget calls are recorded in `items` in the order they are invoked.
/// Each entry uses the format `"<kind>:<text>"` (e.g. `"label:Hello"`).
///
/// # Example
/// ```
/// use oxiui_dioxus::DioxusCtx;
/// use oxiui_core::UiCtx;
///
/// let mut ctx = DioxusCtx::default();
/// ctx.heading("App Title");
/// ctx.label("Hello, Dioxus!");
/// let _resp = ctx.button("Click me");
/// assert_eq!(ctx.items.len(), 3);
/// ```
#[derive(Debug, Default)]
pub struct DioxusCtx {
    /// Widget descriptions collected during a content-closure pass.
    ///
    /// Format: `"<kind>:<text>"` e.g. `"label:Hello"`, `"button:Quit"`.
    pub items: Vec<String>,
}

impl UiCtx for DioxusCtx {
    fn heading(&mut self, text: &str) {
        self.items.push(format!("heading:{text}"));
    }

    fn label(&mut self, text: &str) {
        self.items.push(format!("label:{text}"));
    }

    fn button(&mut self, label: &str) -> ButtonResponse {
        self.items.push(format!("button:{label}"));
        // In collection/headless mode buttons are never clicked.
        // A real Dioxus component tree would hook up event callbacks.
        ButtonResponse {
            clicked: false,
            hovered: false,
        }
    }
}
