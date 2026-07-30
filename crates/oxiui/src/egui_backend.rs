//! OxiEguiApp — native egui/eframe integration (non-wasm32 only).
//!
//! This module provides `OxiEguiApp`, the `eframe::App` implementation that
//! drives the user's content closure, lifecycle hooks, and plugins through
//! egui's immediate-mode frame loop.

use crate::null_ctx::NullUiCtx;
use crate::runner::{LifecycleEvent, LifecycleSnapshot, LifecycleTracker};
use crate::{ContentFn, EguiFrameHook, HookFn, Plugin};

/// The eframe application struct that drives the OxiUI content closure.
///
/// Constructed inside `EguiRunner::run_native` and passed to
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
    /// Hooks fired once when the window is closing (`eframe::App::on_exit`).
    pub on_close: Vec<HookFn>,
    /// Hooks fired when the viewport size changes.
    pub on_resize: Vec<HookFn>,
    /// Hooks fired when the window focus state flips.
    pub on_focus: Vec<HookFn>,
    /// Deduplicates raw size / focus / close snapshots into fired events.
    pub tracker: LifecycleTracker,
}

impl OxiEguiApp {
    /// Fire the hooks corresponding to a batch of deduplicated lifecycle events.
    ///
    /// Hooks run against a [`NullUiCtx`]: lifecycle events fire outside a live
    /// drawing frame, so there is no `EguiUiCtx` to bind them to.
    fn dispatch_lifecycle(&mut self, events: &[LifecycleEvent]) {
        if events.is_empty() {
            return;
        }
        let mut null = NullUiCtx;
        for ev in events {
            match ev {
                LifecycleEvent::Resized(_, _) => {
                    for hook in self.on_resize.iter_mut() {
                        hook(&mut null);
                    }
                }
                LifecycleEvent::Focus(_) => {
                    for hook in self.on_focus.iter_mut() {
                        hook(&mut null);
                    }
                }
                LifecycleEvent::Close => {
                    for hook in self.on_close.iter_mut() {
                        hook(&mut null);
                    }
                }
            }
        }
    }
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

        // Drop the UiCtx borrow before polling window lifecycle state.
        drop(ctx_bridge);

        // Poll viewport size + focus and fire resize / focus hooks on change.
        let (size, focused) = egui_ctx.input(|i| {
            let rect = i.viewport_rect();
            ((rect.width(), rect.height()), i.focused)
        });
        let events = self.tracker.observe(LifecycleSnapshot {
            size: Some(size),
            focused: Some(focused),
            close_requested: false,
        });
        self.dispatch_lifecycle(&events);

        // Frame-skip: if no input events occurred this frame, defer the next repaint.
        if self.frame_skip && egui_ctx.input(|i| i.events.is_empty()) {
            egui_ctx.request_repaint_after(std::time::Duration::from_secs(1));
        }
    }

    /// Called once on shutdown (non-glow eframe signature).
    ///
    /// Fires the `on_close` hooks exactly once via the shared
    /// [`LifecycleTracker`] close guard.
    fn on_exit(&mut self) {
        let events = self.tracker.observe(LifecycleSnapshot {
            size: None,
            focused: None,
            close_requested: true,
        });
        self.dispatch_lifecycle(&events);
    }
}
