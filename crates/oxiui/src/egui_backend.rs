//! OxiEguiApp — native egui/eframe integration (non-wasm32 only).
//!
//! This module provides `OxiEguiApp`, the `eframe::App` implementation that
//! drives the user's content closure, lifecycle hooks, and plugins through
//! egui's immediate-mode frame loop.

use std::sync::{Arc, Mutex};

use oxiui_core::window::{WindowConfig, WindowId};

use crate::menu::{MenuBar, MenuBarState};
use crate::multiwindow::{SecondaryWindow, SharedContent, WindowHandle, WindowSession};
use crate::null_ctx::NullUiCtx;
use crate::runner::{LifecycleEvent, LifecycleSnapshot, LifecycleTracker};
use crate::{ContentFn, EguiFrameHook, HookFn, Plugin};

/// The [`egui::ViewportId`] that hosts the secondary window `id`.
///
/// Derived from the stable [`WindowId`], so the same descriptor always maps to
/// the same viewport across frames (egui requires a stable id to keep the OS
/// window alive between passes).
pub(crate) fn viewport_id_for(id: WindowId) -> egui::ViewportId {
    egui::ViewportId::from_hash_of(("oxiui_secondary_window", id.0))
}

/// Widget-id base reserved for OxiUI's own chrome (currently the menu bar).
///
/// The menu bar draws a variable number of widgets — the drop-down appears and
/// disappears — so it must not share an id range with the app's content, whose
/// ids have to stay stable frame to frame for egui's persistent widget state
/// (dropdown selection, popup position, grid column widths) to survive.
/// Content keeps the natural `0, 1, 2, …` range; chrome starts here.
pub(crate) const CHROME_ID_BASE: usize = usize::MAX / 2;

/// Translate a core [`WindowConfig`] into an [`egui::ViewportBuilder`].
pub(crate) fn viewport_builder_for(config: &WindowConfig) -> egui::ViewportBuilder {
    let mut builder = egui::ViewportBuilder::default()
        .with_title(&config.title)
        .with_inner_size([config.width, config.height])
        .with_resizable(config.resizable)
        .with_decorations(config.decorations)
        .with_transparent(config.transparent);
    if config.always_on_top {
        builder = builder.with_always_on_top();
    }
    builder
}

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
    /// The application menu bar, drawn at the top of every primary frame.
    pub menu_bar: Option<MenuBar>,
    /// Which menu / submenu of [`OxiEguiApp::menu_bar`] is currently open.
    pub menu_state: MenuBarState,
    /// Secondary window descriptors registered on the facade.
    pub windows: Vec<SecondaryWindow>,
    /// Per-window content closures, indexed by `SecondaryWindow::content_key`.
    pub window_contents: Vec<SharedContent>,
    /// Runtime open / close / focus command queue shared with the app.
    pub window_handle: WindowHandle,
    /// Which secondary windows are currently open, plus pending focus requests.
    pub session: WindowSession,
    /// Ids whose viewport reported an OS close request, filled in from the
    /// deferred viewport callbacks (which cannot touch `&mut self`).
    pub os_closed: Arc<Mutex<Vec<u64>>>,
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

    /// Open, close and focus the registered secondary windows for this frame.
    ///
    /// Each open descriptor is shown as an egui *deferred viewport* — a real OS
    /// window on every platform where eframe supports multiple viewports; egui
    /// falls back to an embedded `egui::Window` when it does not (checked via
    /// [`egui::Context::embed_viewports`]), so the window is always visible.
    ///
    /// A window disappears from the next frame as soon as either the user
    /// closes it (its viewport reports `close_requested`) or the app queues a
    /// [`crate::multiwindow::WindowCommand::Close`]; not calling
    /// `show_viewport_deferred` for an id is what tears the OS window down.
    fn drive_secondary_windows(&mut self, ctx: &egui::Context) {
        if self.windows.is_empty() {
            return;
        }

        // 1. Fold the app's runtime commands into the session state.
        for command in self.window_handle.drain() {
            if let Err(e) = self.session.apply(command) {
                eprintln!("oxiui: ignoring window command: {e}");
            }
        }

        // 2. Apply the closes the OS reported through the viewport callbacks.
        if let Ok(mut closed) = self.os_closed.lock() {
            for raw in closed.drain(..) {
                self.session.mark_closed(WindowId(raw));
            }
        }

        // 3. Raise the windows the app asked to focus.
        for id in self.session.take_focus_requests() {
            ctx.send_viewport_cmd_to(viewport_id_for(id), egui::ViewportCommand::Focus);
        }

        // 4. Show every window that is still open.
        for window in &self.windows {
            if !self.session.is_open(window.id) {
                continue;
            }
            let content: Option<SharedContent> = window
                .content_key
                .and_then(|key| self.window_contents.get(key))
                .map(Arc::clone);
            let closed = Arc::clone(&self.os_closed);
            let raw_id = window.id.0;
            let title = window.config.title.clone();
            ctx.show_viewport_deferred(
                viewport_id_for(window.id),
                viewport_builder_for(&window.config),
                move |ui, _class| {
                    if ui.ctx().input(|i| i.viewport().close_requested()) {
                        if let Ok(mut guard) = closed.lock() {
                            if !guard.contains(&raw_id) {
                                guard.push(raw_id);
                            }
                        }
                    }
                    match content.as_ref() {
                        Some(shared) => match shared.lock() {
                            Ok(mut draw) => {
                                let mut bridge = oxiui_egui::EguiUiCtx::new(ui);
                                (draw)(&mut bridge);
                            }
                            Err(_) => {
                                ui.label("oxiui: window content unavailable (closure panicked)");
                            }
                        },
                        None => {
                            ui.label(&title);
                        }
                    }
                },
            );
        }
    }
}

impl eframe::App for OxiEguiApp {
    /// Called each frame with the root [`egui::Ui`].
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Clone the context now (cheap Arc clone) so we can pass it to hooks
        // without conflicting with the EguiUiCtx borrow below.
        let egui_ctx = ui.ctx().clone();

        // Draw the application menu bar above everything else, and dispatch the
        // actions of any item the user picked this frame.
        // The chrome uses a reserved id range so the content's widget ids stay
        // stable when the drop-down opens or closes (see `CHROME_ID_BASE`).
        if let Some(bar) = self.menu_bar.as_ref() {
            let mut menu_bridge = oxiui_egui::EguiUiCtx::with_id_base(ui, CHROME_ID_BASE);
            crate::menu::render_menu_bar(bar, &mut self.menu_state, &mut menu_bridge);
        }

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

        // Open / close / focus the registered secondary windows.
        self.drive_secondary_windows(&egui_ctx);

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

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::multiwindow::{WindowCommand, WindowRegistry};

    #[test]
    fn viewport_id_is_stable_per_window_and_unique_across_windows() {
        let a = viewport_id_for(WindowId(1));
        let b = viewport_id_for(WindowId(2));
        assert_eq!(a, viewport_id_for(WindowId(1)));
        assert_ne!(a, b);
        assert_ne!(a, egui::ViewportId::ROOT);
    }

    #[test]
    fn viewport_builder_carries_the_window_config() {
        let config = WindowConfig::new("Panel")
            .width(320.0)
            .height(240.0)
            .resizable(false)
            .decorations(false)
            .transparent(true)
            .always_on_top(true);
        let builder = viewport_builder_for(&config);
        assert_eq!(builder.title.as_deref(), Some("Panel"));
        assert_eq!(builder.inner_size, Some(egui::vec2(320.0, 240.0)));
        assert_eq!(builder.resizable, Some(false));
        assert_eq!(builder.decorations, Some(false));
        assert_eq!(builder.transparent, Some(true));
        assert_eq!(builder.window_level, Some(egui::WindowLevel::AlwaysOnTop));
    }

    #[test]
    fn viewport_builder_defaults_stay_unset() {
        let builder = viewport_builder_for(&WindowConfig::new("Plain"));
        assert_eq!(builder.resizable, Some(true));
        assert_eq!(builder.transparent, Some(false));
        assert_eq!(builder.window_level, None);
    }

    /// The session state machine as the egui frame loop drives it: the app
    /// queues commands on its handle, the backend folds them in each frame.
    #[test]
    fn handle_commands_reach_the_session() {
        let mut reg = WindowRegistry::new();
        let id = reg.open_window(WindowConfig::new("panel"));
        let handle = reg.handle().clone();
        let mut session = WindowSession::from_descriptors(reg.secondary_windows());

        handle.close(id).expect("queue close");
        for command in handle.drain() {
            session.apply(command).expect("apply");
        }
        assert!(!session.is_open(id));

        handle.open(id).expect("queue open");
        handle.focus(id).expect("queue focus");
        for command in handle.drain() {
            session.apply(command).expect("apply");
        }
        assert!(session.is_open(id));
        assert_eq!(session.take_focus_requests(), vec![id]);
    }

    #[test]
    fn os_close_flags_close_the_window() {
        let mut reg = WindowRegistry::new();
        let id = reg.open_window(WindowConfig::new("panel"));
        let mut session = WindowSession::from_descriptors(reg.secondary_windows());
        let os_closed: Arc<Mutex<Vec<u64>>> = Arc::new(Mutex::new(Vec::new()));

        // Simulate the deferred viewport callback observing `close_requested`.
        os_closed.lock().expect("lock").push(id.0);
        if let Ok(mut closed) = os_closed.lock() {
            for raw in closed.drain(..) {
                session.mark_closed(WindowId(raw));
            }
        }
        assert!(!session.is_open(id));
        assert!(os_closed.lock().expect("lock").is_empty());
        // A stale command for the closed window is still recognised (registered).
        assert!(session.apply(WindowCommand::Open(id)).is_ok());
    }
}
