//! OxiEguiApp — native egui/eframe integration (non-wasm32 only).
//!
//! This module provides `OxiEguiApp`, the `eframe::App` implementation that
//! drives the user's content closure, lifecycle hooks, and plugins through
//! egui's immediate-mode frame loop.

use crate::{ContentFn, EguiFrameHook, HookFn, Plugin};

/// The eframe application struct that drives the OxiUI content closure.
///
/// Constructed inside `App::run_egui_or_fallback` and passed to
/// `eframe::run_native`.  Not part of the public API.
pub struct OxiEguiApp {
    pub content: Option<ContentFn>,
    pub on_init: Vec<HookFn>,
    pub on_frame: Vec<HookFn>,
    pub plugins: Vec<Box<dyn Plugin>>,
    pub initialised: bool,
    /// If true, yield CPU when no input events occurred this frame.
    pub frame_skip: bool,
    /// Raw egui::Context escape-hatch callbacks.
    pub egui_frame_hooks: Vec<EguiFrameHook>,
}

impl eframe::App for OxiEguiApp {
    /// Called each frame with the root [`egui::Ui`].
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Clone the context now (cheap Arc clone) so we can pass it to hooks
        // without conflicting with the EguiUiCtx borrow below.
        let egui_ctx = ui.ctx().clone();

        let mut ctx_bridge = oxiui_egui::EguiUiCtx::new(ui);

        // Fire init hooks exactly once.
        if !self.initialised {
            self.initialised = true;
            for hook in self.on_init.iter_mut() {
                hook(&mut ctx_bridge);
            }
            for plugin in self.plugins.iter_mut() {
                plugin.init(&mut ctx_bridge);
            }
        }

        // Content closure.
        if let Some(ref mut f) = self.content {
            f(&mut ctx_bridge);
        }

        // Per-frame hooks and plugin updates.
        for hook in self.on_frame.iter_mut() {
            hook(&mut ctx_bridge);
        }
        for plugin in self.plugins.iter_mut() {
            plugin.update(&mut ctx_bridge);
        }

        // egui escape-hatch callbacks.
        for hook in &mut self.egui_frame_hooks {
            hook(&egui_ctx);
        }

        // Frame-skip: if no input events occurred this frame, defer the next repaint.
        if self.frame_skip && egui_ctx.input(|i| i.events.is_empty()) {
            egui_ctx.request_repaint_after(std::time::Duration::from_secs(1));
        }
    }
}
