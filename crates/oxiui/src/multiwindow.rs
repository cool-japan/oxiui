//! Multi-window support for the [`App`] facade.
//!
//! This module provides the window registry that tracks secondary windows
//! alongside the core [`oxiui_core::WindowManager`], plus the runtime plumbing
//! that the backends consume:
//!
//! * [`SecondaryWindow`] — a window descriptor (config + optional content key).
//! * [`crate::multiwindow::WindowRegistry`] — registration-time storage owned
//!   by [`crate::App`].
//! * [`WindowHandle`] — a cloneable handle that content closures capture to
//!   open / close / focus secondary windows **while the event loop runs**.
//! * [`WindowSession`] — the backend-side state machine that folds
//!   [`WindowCommand`]s into an open-window set and a focus queue.
//!
//! The descriptors, contents and handle are moved into a
//! [`crate::shell::ShellConfig`] by `App::run()` and handed to the active
//! [`crate::BackendRunner`]. The native egui backend opens one deferred
//! viewport (a real OS window) per open descriptor; backends that cannot do so
//! reject the shell with [`oxiui_core::UiError::Unsupported`] instead of
//! silently dropping it.
//!
//! # Usage
//!
//! ```rust
//! use oxiui::{App, AppConfig};
//! use oxiui_core::window::WindowConfig;
//!
//! let mut app = App::new(AppConfig::new().title("main"));
//! let wid = app.open_window(WindowConfig::new("Secondary").width(400.0).height(300.0));
//! assert_ne!(wid, oxiui_core::window::WindowId::PRIMARY);
//!
//! // Drive the window from anywhere (including a content closure) at runtime.
//! let handle = app.window_handle();
//! handle.focus(wid).expect("queue focus request");
//! ```

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use oxiui_core::window::{WindowChannel, WindowConfig, WindowId, WindowManager};
use oxiui_core::UiError;

use crate::runner::ContentFn;

// ── SharedContent ─────────────────────────────────────────────────────────────

/// A per-window content closure shared between the facade and a backend.
///
/// Secondary-window content is driven from a deferred viewport callback that
/// egui requires to be `Fn + Send + Sync + 'static`, so the `FnMut` closure is
/// wrapped in an [`Arc`] + [`Mutex`] rather than moved by value.
pub type SharedContent = Arc<Mutex<ContentFn>>;

// ── SecondaryWindow ───────────────────────────────────────────────────────────

/// A secondary window descriptor registered via `App::open_window`.
///
/// Backends consume this list on `App::run()` and open OS windows accordingly.
#[derive(Clone, Debug)]
pub struct SecondaryWindow {
    /// Stable window identifier assigned at registration time.
    pub id: WindowId,
    /// Configuration for the window (title, size, flags).
    pub config: WindowConfig,
    /// Index into the registry's content table
    /// ([`WindowRegistry::contents`]) when the window was registered with
    /// [`WindowRegistry::open_window_with`]; `None` when the window has no
    /// content closure (backends then render a placeholder label).
    pub content_key: Option<usize>,
}

// ── WindowCommand ─────────────────────────────────────────────────────────────

/// A runtime request to change a secondary window's state.
///
/// Commands are queued through a [`WindowHandle`] and folded into a
/// [`WindowSession`] by the active backend once per frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowCommand {
    /// (Re-)open the registered window with this id.
    Open(WindowId),
    /// Close the window with this id (the descriptor stays registered, so it
    /// can be re-opened with [`WindowCommand::Open`]).
    Close(WindowId),
    /// Raise and focus the window with this id.
    Focus(WindowId),
}

// ── WindowHandle ──────────────────────────────────────────────────────────────

/// A cheap, cloneable handle for driving secondary windows at runtime.
///
/// Obtain one with [`crate::App::window_handle`] before `App::run()` and move
/// clones into content closures / hooks / worker threads. Commands are queued
/// and applied by the backend at the start of the next frame.
///
/// The handle is `Send + Sync`; queueing from a background thread is allowed.
#[derive(Clone, Debug, Default)]
pub struct WindowHandle {
    queue: Arc<Mutex<VecDeque<WindowCommand>>>,
}

impl WindowHandle {
    /// Create an empty handle (not connected to any registry).
    pub fn new() -> Self {
        Self::default()
    }

    /// Queue a raw [`WindowCommand`].
    ///
    /// # Errors
    ///
    /// Returns [`UiError::Window`] if the internal queue lock is poisoned
    /// (only possible after a panic while another thread held the lock).
    pub fn push(&self, command: WindowCommand) -> Result<(), UiError> {
        let mut guard = self
            .queue
            .lock()
            .map_err(|_| UiError::Window("window command queue lock poisoned".to_string()))?;
        guard.push_back(command);
        Ok(())
    }

    /// Queue an [`WindowCommand::Open`] request for `id`.
    ///
    /// # Errors
    ///
    /// Returns [`UiError::Window`] if the internal queue lock is poisoned.
    pub fn open(&self, id: WindowId) -> Result<(), UiError> {
        self.push(WindowCommand::Open(id))
    }

    /// Queue a [`WindowCommand::Close`] request for `id`.
    ///
    /// # Errors
    ///
    /// Returns [`UiError::Window`] if the internal queue lock is poisoned.
    pub fn close(&self, id: WindowId) -> Result<(), UiError> {
        self.push(WindowCommand::Close(id))
    }

    /// Queue a [`WindowCommand::Focus`] request for `id`.
    ///
    /// # Errors
    ///
    /// Returns [`UiError::Window`] if the internal queue lock is poisoned.
    pub fn focus(&self, id: WindowId) -> Result<(), UiError> {
        self.push(WindowCommand::Focus(id))
    }

    /// Take every queued command, leaving the queue empty.
    ///
    /// Called by the backend once per frame. A poisoned lock yields an empty
    /// batch rather than an error so a frame can never be aborted by it.
    pub fn drain(&self) -> Vec<WindowCommand> {
        match self.queue.lock() {
            Ok(mut guard) => guard.drain(..).collect(),
            Err(_) => Vec::new(),
        }
    }

    /// Number of commands currently queued (0 if the lock is poisoned).
    pub fn pending(&self) -> usize {
        self.queue.lock().map(|g| g.len()).unwrap_or(0)
    }
}

// ── WindowSession ─────────────────────────────────────────────────────────────

/// Backend-side open/closed/focus state for the registered secondary windows.
///
/// Every registered descriptor starts **open**: `App::run()` opens each window
/// the app registered before start-up. [`WindowSession::apply`] folds runtime
/// [`WindowCommand`]s into that state; [`WindowSession::mark_closed`] records a
/// close initiated by the OS (the user clicking the window's close button).
#[derive(Clone, Debug, Default)]
pub struct WindowSession {
    known: Vec<WindowId>,
    open: Vec<WindowId>,
    focus_requests: Vec<WindowId>,
}

impl WindowSession {
    /// Build a session from the registered descriptors, all initially open.
    pub fn from_descriptors(windows: &[SecondaryWindow]) -> Self {
        let known: Vec<WindowId> = windows.iter().map(|w| w.id).collect();
        Self {
            open: known.clone(),
            known,
            focus_requests: Vec::new(),
        }
    }

    /// Fold a runtime command into the session state.
    ///
    /// # Errors
    ///
    /// Returns [`UiError::Window`] when `id` was never registered on the
    /// [`WindowRegistry`], or when a focus is requested for a closed window.
    pub fn apply(&mut self, command: WindowCommand) -> Result<(), UiError> {
        let id = match command {
            WindowCommand::Open(id) | WindowCommand::Close(id) | WindowCommand::Focus(id) => id,
        };
        if !self.known.contains(&id) {
            return Err(UiError::Window(format!(
                "window {} is not registered; call App::open_window before driving it",
                id.0
            )));
        }
        match command {
            WindowCommand::Open(id) => {
                if !self.open.contains(&id) {
                    self.open.push(id);
                }
                Ok(())
            }
            WindowCommand::Close(id) => {
                self.open.retain(|w| *w != id);
                self.focus_requests.retain(|w| *w != id);
                Ok(())
            }
            WindowCommand::Focus(id) => {
                if !self.open.contains(&id) {
                    return Err(UiError::Window(format!(
                        "cannot focus window {}: it is closed",
                        id.0
                    )));
                }
                if !self.focus_requests.contains(&id) {
                    self.focus_requests.push(id);
                }
                Ok(())
            }
        }
    }

    /// Record a close that the windowing system initiated (user close button).
    ///
    /// Unlike [`WindowSession::apply`] this never fails: the id comes from the
    /// backend's own descriptor list.
    pub fn mark_closed(&mut self, id: WindowId) {
        self.open.retain(|w| *w != id);
        self.focus_requests.retain(|w| *w != id);
    }

    /// Whether the window with this id should currently be shown.
    pub fn is_open(&self, id: WindowId) -> bool {
        self.open.contains(&id)
    }

    /// The ids of all currently open secondary windows, in registration order.
    pub fn open_windows(&self) -> &[WindowId] {
        &self.open
    }

    /// Take the pending focus requests, leaving the queue empty.
    pub fn take_focus_requests(&mut self) -> Vec<WindowId> {
        std::mem::take(&mut self.focus_requests)
    }
}

// ── WindowRegistry ─────────────────────────────────────────────────────────────

/// Facade-level window registry.
///
/// Wraps the core [`WindowManager`] and maintains an ordered list of secondary
/// window descriptors for back-end dispatch.  The primary window is implicit
/// (always id = `WindowId::PRIMARY`) and is not tracked here.
pub struct WindowRegistry {
    manager: WindowManager,
    secondary: Vec<SecondaryWindow>,
    contents: Vec<SharedContent>,
    handle: WindowHandle,
}

impl WindowRegistry {
    /// Create a new, empty registry with only the primary window.
    pub fn new() -> Self {
        Self {
            manager: WindowManager::default(),
            secondary: Vec::new(),
            contents: Vec::new(),
            handle: WindowHandle::new(),
        }
    }

    /// Register a new secondary window with the given configuration.
    ///
    /// Returns the [`WindowId`] assigned to the new window.  The backend will
    /// open the corresponding OS window when the event loop starts.
    ///
    /// The window has no content closure; backends render a placeholder label
    /// with the window title.  Use [`WindowRegistry::open_window_with`] to draw
    /// real content into the window.
    pub fn open_window(&mut self, config: WindowConfig) -> WindowId {
        let id = self.manager.create_window(config.clone());
        self.secondary.push(SecondaryWindow {
            id,
            config,
            content_key: None,
        });
        id
    }

    /// Register a secondary window together with its per-window content closure.
    ///
    /// The closure is called once per frame of that window with a backend
    /// [`oxiui_core::UiCtx`], exactly like the primary window's content closure.
    pub fn open_window_with<F>(&mut self, config: WindowConfig, content: F) -> WindowId
    where
        F: FnMut(&mut dyn oxiui_core::UiCtx) + Send + 'static,
    {
        let id = self.manager.create_window(config.clone());
        let key = self.contents.len();
        self.contents
            .push(Arc::new(Mutex::new(Box::new(content) as ContentFn)));
        self.secondary.push(SecondaryWindow {
            id,
            config,
            content_key: Some(key),
        });
        id
    }

    /// The content closure table, indexed by [`SecondaryWindow::content_key`].
    ///
    /// Entries are never removed (so keys stay stable); closing a window leaves
    /// its closure in place for a later re-open.
    pub fn contents(&self) -> &[SharedContent] {
        &self.contents
    }

    /// The runtime command handle shared with the active backend.
    pub fn handle(&self) -> &WindowHandle {
        &self.handle
    }

    /// Move the registry's backend-visible state into a [`crate::shell::ShellConfig`].
    ///
    /// Called by `App::run()`; the registry keeps its [`WindowManager`] and
    /// handle so `App` stays usable (queued commands still reach the backend).
    pub fn take_shell(
        &mut self,
        menu_bar: Option<crate::menu::MenuBar>,
    ) -> crate::shell::ShellConfig {
        crate::shell::ShellConfig {
            windows: std::mem::take(&mut self.secondary),
            contents: std::mem::take(&mut self.contents),
            menu_bar,
            handle: self.handle.clone(),
        }
    }

    /// Remove a secondary window from the registry.
    ///
    /// Returns the removed descriptor if `id` was found, or `None` if the
    /// window was not registered (or `id` is the primary window).
    pub fn close_window(&mut self, id: WindowId) -> Option<SecondaryWindow> {
        if id == WindowId::PRIMARY {
            return None;
        }
        let _ = self.manager.destroy_window(id); // best-effort; ignore PRIMARY error
        let pos = self.secondary.iter().position(|w| w.id == id)?;
        Some(self.secondary.remove(pos))
    }

    /// Returns a shared reference to the cross-window communication channel.
    pub fn channel(&self) -> &WindowChannel {
        self.manager.channel()
    }

    /// Returns the list of all registered secondary windows in registration order.
    pub fn secondary_windows(&self) -> &[SecondaryWindow] {
        &self.secondary
    }

    /// Returns the number of open secondary windows (not counting the primary).
    pub fn secondary_count(&self) -> usize {
        self.secondary.len()
    }
}

impl Default for WindowRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_window_returns_non_primary_id() {
        let mut reg = WindowRegistry::new();
        let id = reg.open_window(WindowConfig::new("secondary"));
        assert_ne!(id, WindowId::PRIMARY);
    }

    #[test]
    fn open_window_increments_secondary_count() {
        let mut reg = WindowRegistry::new();
        assert_eq!(reg.secondary_count(), 0);
        reg.open_window(WindowConfig::new("w1"));
        assert_eq!(reg.secondary_count(), 1);
        reg.open_window(WindowConfig::new("w2"));
        assert_eq!(reg.secondary_count(), 2);
    }

    #[test]
    fn close_window_removes_secondary() {
        let mut reg = WindowRegistry::new();
        let id = reg.open_window(WindowConfig::new("w"));
        assert_eq!(reg.secondary_count(), 1);
        let removed = reg.close_window(id);
        assert!(removed.is_some());
        assert_eq!(reg.secondary_count(), 0);
    }

    #[test]
    fn close_primary_window_is_noop() {
        let mut reg = WindowRegistry::new();
        let result = reg.close_window(WindowId::PRIMARY);
        assert!(result.is_none());
    }

    #[test]
    fn channel_send_and_drain() {
        let reg = WindowRegistry::new();
        let ch = reg.channel().clone();
        let wid = WindowId(42);
        ch.send(wid, "hello").unwrap();
        let msgs = ch.drain_messages(wid).unwrap();
        assert_eq!(msgs, vec!["hello"]);
    }

    #[test]
    fn secondary_windows_slice_matches_registration_order() {
        let mut reg = WindowRegistry::new();
        let id1 = reg.open_window(WindowConfig::new("first"));
        let id2 = reg.open_window(WindowConfig::new("second"));
        let windows = reg.secondary_windows();
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].id, id1);
        assert_eq!(windows[1].id, id2);
    }

    #[test]
    fn close_nonexistent_window_returns_none() {
        let mut reg = WindowRegistry::new();
        let result = reg.close_window(WindowId(999));
        assert!(result.is_none());
    }

    // ── Runtime handle / session state machine ────────────────────────────────

    #[test]
    fn handle_queues_and_drains_commands() {
        let handle = WindowHandle::new();
        assert_eq!(handle.pending(), 0);
        handle.open(WindowId(7)).expect("queue open");
        handle.focus(WindowId(7)).expect("queue focus");
        handle.close(WindowId(7)).expect("queue close");
        assert_eq!(handle.pending(), 3);
        let drained = handle.drain();
        assert_eq!(
            drained,
            vec![
                WindowCommand::Open(WindowId(7)),
                WindowCommand::Focus(WindowId(7)),
                WindowCommand::Close(WindowId(7)),
            ]
        );
        assert_eq!(handle.pending(), 0);
    }

    #[test]
    fn handle_clone_shares_one_queue() {
        let handle = WindowHandle::new();
        let clone = handle.clone();
        clone.open(WindowId(3)).expect("queue open");
        assert_eq!(handle.pending(), 1);
        assert_eq!(handle.drain().len(), 1);
        assert_eq!(clone.pending(), 0);
    }

    #[test]
    fn registry_handle_is_shared_with_take_shell() {
        let mut reg = WindowRegistry::new();
        let id = reg.open_window(WindowConfig::new("w"));
        let handle = reg.handle().clone();
        let shell = reg.take_shell(None);
        handle.close(id).expect("queue close");
        // The shell's handle observes commands queued through the app's handle.
        assert_eq!(shell.handle.drain(), vec![WindowCommand::Close(id)]);
    }

    #[test]
    fn open_window_with_registers_content_key() {
        let mut reg = WindowRegistry::new();
        let plain = reg.open_window(WindowConfig::new("plain"));
        let with_content = reg.open_window_with(WindowConfig::new("rich"), |ui| {
            ui.label("secondary");
        });
        assert_ne!(plain, with_content);
        let windows = reg.secondary_windows();
        assert_eq!(windows[0].content_key, None);
        assert_eq!(windows[1].content_key, Some(0));
        assert_eq!(reg.contents().len(), 1);
    }

    #[test]
    fn registered_content_closure_is_callable() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&calls);
        let mut reg = WindowRegistry::new();
        reg.open_window_with(WindowConfig::new("rich"), move |_ui| {
            counter.fetch_add(1, Ordering::SeqCst);
        });
        let shared = reg.contents()[0].clone();
        {
            let mut guard = shared.lock().expect("content lock");
            let mut null = crate::null_ctx::NullUiCtx;
            (guard)(&mut null);
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn session_starts_with_every_descriptor_open() {
        let mut reg = WindowRegistry::new();
        let a = reg.open_window(WindowConfig::new("a"));
        let b = reg.open_window(WindowConfig::new("b"));
        let session = WindowSession::from_descriptors(reg.secondary_windows());
        assert_eq!(session.open_windows(), &[a, b]);
        assert!(session.is_open(a));
        assert!(session.is_open(b));
    }

    #[test]
    fn session_close_then_reopen() {
        let mut reg = WindowRegistry::new();
        let a = reg.open_window(WindowConfig::new("a"));
        let mut session = WindowSession::from_descriptors(reg.secondary_windows());
        session.apply(WindowCommand::Close(a)).expect("close");
        assert!(!session.is_open(a));
        session.apply(WindowCommand::Open(a)).expect("reopen");
        assert!(session.is_open(a));
        // Re-opening an already open window is idempotent.
        session.apply(WindowCommand::Open(a)).expect("reopen twice");
        assert_eq!(session.open_windows(), &[a]);
    }

    #[test]
    fn session_rejects_unregistered_window() {
        let mut session = WindowSession::from_descriptors(&[]);
        let err = session
            .apply(WindowCommand::Open(WindowId(42)))
            .expect_err("unregistered id must be rejected");
        assert!(matches!(err, UiError::Window(_)), "got {err:?}");
        assert!(err.to_string().contains("42"));
    }

    #[test]
    fn session_focus_queue_is_drained_once() {
        let mut reg = WindowRegistry::new();
        let a = reg.open_window(WindowConfig::new("a"));
        let mut session = WindowSession::from_descriptors(reg.secondary_windows());
        session.apply(WindowCommand::Focus(a)).expect("focus");
        session.apply(WindowCommand::Focus(a)).expect("focus again");
        assert_eq!(session.take_focus_requests(), vec![a]);
        assert!(session.take_focus_requests().is_empty());
    }

    #[test]
    fn session_focus_on_closed_window_errors() {
        let mut reg = WindowRegistry::new();
        let a = reg.open_window(WindowConfig::new("a"));
        let mut session = WindowSession::from_descriptors(reg.secondary_windows());
        session.apply(WindowCommand::Close(a)).expect("close");
        let err = session
            .apply(WindowCommand::Focus(a))
            .expect_err("focusing a closed window must fail");
        assert!(matches!(err, UiError::Window(_)), "got {err:?}");
    }

    #[test]
    fn session_mark_closed_drops_pending_focus() {
        let mut reg = WindowRegistry::new();
        let a = reg.open_window(WindowConfig::new("a"));
        let mut session = WindowSession::from_descriptors(reg.secondary_windows());
        session.apply(WindowCommand::Focus(a)).expect("focus");
        session.mark_closed(a);
        assert!(!session.is_open(a));
        assert!(session.take_focus_requests().is_empty());
    }
}
