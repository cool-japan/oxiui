//! Multi-window support for OxiUI.
//!
//! Provides [`WindowId`] handles, per-window [`WidgetTree`] management via
//! [`WindowManager`], a typed [`WindowConfig`] builder, [`WindowEvent`]
//! lifecycle events, and a thread-safe [`WindowChannel`] for cross-window
//! message passing.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::{Rect, UiError, WidgetTree};

// ── WindowId ─────────────────────────────────────────────────────────────────

/// Opaque handle identifying a window.
///
/// Obtained from [`WindowManager::create_window`] and used to address
/// per-window widget trees and route events.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WindowId(pub u64);

impl WindowId {
    /// The implicit primary / root window (id = 1).
    pub const PRIMARY: Self = WindowId(1);
}

impl std::fmt::Display for WindowId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Window({})", self.0)
    }
}

// ── WindowConfig ─────────────────────────────────────────────────────────────

/// Configuration for creating a new window.
#[derive(Clone, Debug)]
pub struct WindowConfig {
    /// Window title string.
    pub title: String,
    /// Logical width in pixels.
    pub width: f32,
    /// Logical height in pixels.
    pub height: f32,
    /// Whether the window may be resized by the user.
    pub resizable: bool,
    /// Whether to show the OS window decorations (title bar, borders).
    pub decorations: bool,
    /// Whether the window background is transparent.
    pub transparent: bool,
    /// Whether the window floats above all other windows.
    pub always_on_top: bool,
}

impl Default for WindowConfig {
    fn default() -> Self {
        WindowConfig {
            title: String::new(),
            width: 800.0,
            height: 600.0,
            resizable: true,
            decorations: true,
            transparent: false,
            always_on_top: false,
        }
    }
}

impl WindowConfig {
    /// Create a config with the given title and default dimensions.
    pub fn new(title: impl Into<String>) -> Self {
        WindowConfig {
            title: title.into(),
            ..Default::default()
        }
    }

    /// Set the logical width.
    pub fn width(mut self, w: f32) -> Self {
        self.width = w;
        self
    }

    /// Set the logical height.
    pub fn height(mut self, h: f32) -> Self {
        self.height = h;
        self
    }

    /// Set whether the window is resizable.
    pub fn resizable(mut self, r: bool) -> Self {
        self.resizable = r;
        self
    }

    /// Set whether window decorations are shown.
    pub fn decorations(mut self, d: bool) -> Self {
        self.decorations = d;
        self
    }

    /// Set whether the window background is transparent.
    pub fn transparent(mut self, t: bool) -> Self {
        self.transparent = t;
        self
    }

    /// Set whether the window is always on top.
    pub fn always_on_top(mut self, a: bool) -> Self {
        self.always_on_top = a;
        self
    }
}

// ── WindowEvent ──────────────────────────────────────────────────────────────

/// Events related to window lifecycle and state.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum WindowEvent {
    /// A new window was successfully created.
    Created(WindowId),
    /// A window was closed and its resources freed.
    Closed(WindowId),
    /// A window was resized to new logical dimensions.
    Resized {
        /// The window that was resized.
        id: WindowId,
        /// New logical width in pixels.
        width: f32,
        /// New logical height in pixels.
        height: f32,
    },
    /// A window gained keyboard/pointer focus.
    FocusGained(WindowId),
    /// A window lost keyboard/pointer focus.
    FocusLost(WindowId),
    /// A cross-window message was dispatched from one window to another.
    Message {
        /// Originating window.
        from: WindowId,
        /// Target window.
        to: WindowId,
        /// Arbitrary UTF-8 payload.
        payload: String,
    },
}

// ── WindowChannel ─────────────────────────────────────────────────────────────

/// Thread-safe cross-window communication channel.
///
/// Messages sent to a `WindowId` are queued until the window drains them via
/// [`drain_messages`](WindowChannel::drain_messages).  The channel is cheaply
/// cloneable via [`Clone`] (backed by an [`Arc`]).
#[derive(Clone, Debug, Default)]
pub struct WindowChannel {
    queues: Arc<Mutex<HashMap<WindowId, Vec<String>>>>,
}

impl WindowChannel {
    /// Create a new, empty channel.
    pub fn new() -> Self {
        WindowChannel::default()
    }

    /// Enqueue a message for the target window.
    ///
    /// # Errors
    ///
    /// Returns [`UiError::Focus`] when the internal lock is poisoned
    /// (extremely rare; only occurs after a panic inside `drain_messages`).
    pub fn send(&self, to: WindowId, payload: impl Into<String>) -> Result<(), UiError> {
        let mut guard = self
            .queues
            .lock()
            .map_err(|_| UiError::Focus("window-channel lock poisoned".into()))?;
        guard.entry(to).or_default().push(payload.into());
        Ok(())
    }

    /// Drain all queued messages for the given window, returning them in
    /// arrival order.  The internal queue for that window is cleared.
    ///
    /// # Errors
    ///
    /// Returns [`UiError::Focus`] when the lock is poisoned.
    pub fn drain_messages(&self, id: WindowId) -> Result<Vec<String>, UiError> {
        let mut guard = self
            .queues
            .lock()
            .map_err(|_| UiError::Focus("window-channel lock poisoned".into()))?;
        Ok(guard.remove(&id).unwrap_or_default())
    }

    /// Return the number of pending messages for a window without consuming them.
    pub fn pending_count(&self, id: WindowId) -> usize {
        self.queues
            .lock()
            .map(|g| g.get(&id).map_or(0, Vec::len))
            .unwrap_or(0)
    }
}

// ── WindowManager ─────────────────────────────────────────────────────────────

/// Manages per-window [`WidgetTree`]s and window lifecycle.
///
/// The primary window ([`WindowId::PRIMARY`]) is always present and cannot
/// be destroyed.  Secondary windows are created via
/// [`create_window`](WindowManager::create_window) and removed via
/// [`destroy_window`](WindowManager::destroy_window).
pub struct WindowManager {
    next_id: u64,
    trees: HashMap<WindowId, WidgetTree>,
    configs: HashMap<WindowId, WindowConfig>,
    channel: WindowChannel,
}

impl WindowManager {
    /// Create a new manager with the primary window having the given root rect.
    pub fn new(primary_rect: Rect) -> Self {
        let mut trees = HashMap::new();
        let mut configs = HashMap::new();
        trees.insert(WindowId::PRIMARY, WidgetTree::new(primary_rect));
        configs.insert(WindowId::PRIMARY, WindowConfig::new("Main"));
        WindowManager {
            next_id: 2,
            trees,
            configs,
            channel: WindowChannel::new(),
        }
    }

    /// Create a secondary window with the given configuration, returning its id.
    ///
    /// The new window's widget tree is initialised with a root rect matching
    /// the config's width/height dimensions.
    pub fn create_window(&mut self, config: WindowConfig) -> WindowId {
        let id = WindowId(self.next_id);
        self.next_id += 1;
        let rect = Rect::new(0.0, 0.0, config.width, config.height);
        self.trees.insert(id, WidgetTree::new(rect));
        self.configs.insert(id, config);
        id
    }

    /// Destroy a secondary window, freeing its widget tree.
    ///
    /// # Errors
    ///
    /// Returns an error if `id` is [`WindowId::PRIMARY`] or does not exist.
    pub fn destroy_window(&mut self, id: WindowId) -> Result<(), UiError> {
        if id == WindowId::PRIMARY {
            return Err(UiError::Focus("cannot destroy the primary window".into()));
        }
        if self.trees.remove(&id).is_none() {
            return Err(UiError::Focus(format!("window {id} not found")));
        }
        self.configs.remove(&id);
        Ok(())
    }

    /// Borrow the [`WidgetTree`] for a window.
    pub fn tree(&self, id: WindowId) -> Option<&WidgetTree> {
        self.trees.get(&id)
    }

    /// Mutably borrow the [`WidgetTree`] for a window.
    pub fn tree_mut(&mut self, id: WindowId) -> Option<&mut WidgetTree> {
        self.trees.get_mut(&id)
    }

    /// Borrow the [`WindowConfig`] for a window.
    pub fn config(&self, id: WindowId) -> Option<&WindowConfig> {
        self.configs.get(&id)
    }

    /// Return a sorted list of all open window ids.
    pub fn window_ids(&self) -> Vec<WindowId> {
        let mut ids: Vec<WindowId> = self.trees.keys().copied().collect();
        ids.sort();
        ids
    }

    /// Return the number of currently open windows.
    pub fn window_count(&self) -> usize {
        self.trees.len()
    }

    /// Borrow the shared cross-window communication channel.
    pub fn channel(&self) -> &WindowChannel {
        &self.channel
    }

    /// Update a window's logical dimensions and reinitialise its widget tree.
    ///
    /// This is a blunt resize: all existing widget nodes are dropped and the
    /// tree is recreated with only the root node.  Adapters that cache the
    /// tree contents should rebuild after calling this method.
    pub fn resize_window(&mut self, id: WindowId, width: f32, height: f32) {
        if let Some(cfg) = self.configs.get_mut(&id) {
            cfg.width = width;
            cfg.height = height;
        }
        if let Some(tree) = self.trees.get_mut(&id) {
            *tree = WidgetTree::new(Rect::new(0.0, 0.0, width, height));
        }
    }
}

impl Default for WindowManager {
    fn default() -> Self {
        Self::new(Rect::new(0.0, 0.0, 800.0, 600.0))
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_window_always_present() {
        let mgr = WindowManager::default();
        assert!(mgr.tree(WindowId::PRIMARY).is_some());
        assert_eq!(mgr.window_count(), 1);
    }

    #[test]
    fn create_window_returns_unique_ids() {
        let mut mgr = WindowManager::default();
        let id1 = mgr.create_window(WindowConfig::new("w1"));
        let id2 = mgr.create_window(WindowConfig::new("w2"));
        assert_ne!(id1, id2);
        assert_ne!(id1, WindowId::PRIMARY);
        assert_ne!(id2, WindowId::PRIMARY);
    }

    #[test]
    fn create_and_destroy_window() {
        let mut mgr = WindowManager::default();
        let id = mgr.create_window(WindowConfig::new("secondary"));
        assert_eq!(mgr.window_count(), 2);
        mgr.destroy_window(id).expect("destroy ok");
        assert_eq!(mgr.window_count(), 1);
        assert!(mgr.tree(id).is_none());
    }

    #[test]
    fn destroy_primary_window_is_err() {
        let mut mgr = WindowManager::default();
        assert!(mgr.destroy_window(WindowId::PRIMARY).is_err());
    }

    #[test]
    fn destroy_nonexistent_window_is_err() {
        let mut mgr = WindowManager::default();
        assert!(mgr.destroy_window(WindowId(99)).is_err());
    }

    #[test]
    fn window_ids_sorted() {
        let mut mgr = WindowManager::default();
        let id2 = mgr.create_window(WindowConfig::default());
        let id3 = mgr.create_window(WindowConfig::default());
        let ids = mgr.window_ids();
        assert_eq!(ids[0], WindowId::PRIMARY);
        assert!(ids.contains(&id2));
        assert!(ids.contains(&id3));
        // Verify sorted order.
        for w in ids.windows(2) {
            assert!(w[0] < w[1]);
        }
    }

    #[test]
    fn window_channel_send_and_drain() {
        let ch = WindowChannel::new();
        let wid = WindowId(10);
        ch.send(wid, "hello").unwrap();
        ch.send(wid, "world").unwrap();
        let msgs = ch.drain_messages(wid).unwrap();
        assert_eq!(msgs, vec!["hello", "world"]);
        // Queue is empty after drain.
        assert_eq!(ch.pending_count(wid), 0);
    }

    #[test]
    fn window_channel_pending_count() {
        let ch = WindowChannel::new();
        let wid = WindowId(20);
        assert_eq!(ch.pending_count(wid), 0);
        ch.send(wid, "a").unwrap();
        ch.send(wid, "b").unwrap();
        assert_eq!(ch.pending_count(wid), 2);
        ch.drain_messages(wid).unwrap();
        assert_eq!(ch.pending_count(wid), 0);
    }

    #[test]
    fn window_channel_clone_shares_state() {
        let ch = WindowChannel::new();
        let ch2 = ch.clone();
        let wid = WindowId(30);
        ch.send(wid, "msg").unwrap();
        // Cloned handle sees the same queue.
        assert_eq!(ch2.pending_count(wid), 1);
        let msgs = ch2.drain_messages(wid).unwrap();
        assert_eq!(msgs, vec!["msg"]);
    }

    #[test]
    fn window_channel_separate_queues_per_window() {
        let ch = WindowChannel::new();
        let a = WindowId(40);
        let b = WindowId(41);
        ch.send(a, "for-a").unwrap();
        ch.send(b, "for-b").unwrap();
        assert_eq!(ch.pending_count(a), 1);
        assert_eq!(ch.pending_count(b), 1);
        let drained_a = ch.drain_messages(a).unwrap();
        assert_eq!(drained_a, vec!["for-a"]);
        assert_eq!(ch.pending_count(b), 1); // b unaffected
    }

    #[test]
    fn window_config_builder() {
        let cfg = WindowConfig::new("Test")
            .width(1024.0)
            .height(768.0)
            .resizable(false)
            .decorations(false)
            .transparent(true)
            .always_on_top(true);
        assert_eq!(cfg.title, "Test");
        assert_eq!(cfg.width, 1024.0);
        assert_eq!(cfg.height, 768.0);
        assert!(!cfg.resizable);
        assert!(!cfg.decorations);
        assert!(cfg.transparent);
        assert!(cfg.always_on_top);
    }

    #[test]
    fn resize_window_updates_config_and_tree() {
        let mut mgr = WindowManager::default();
        mgr.resize_window(WindowId::PRIMARY, 1920.0, 1080.0);
        let cfg = mgr.config(WindowId::PRIMARY).unwrap();
        assert_eq!(cfg.width, 1920.0);
        assert_eq!(cfg.height, 1080.0);
    }

    #[test]
    fn window_event_debug_is_non_empty() {
        let e = WindowEvent::Created(WindowId::PRIMARY);
        let s = format!("{e:?}");
        assert!(!s.is_empty());
    }

    #[test]
    fn window_id_display() {
        assert_eq!(format!("{}", WindowId::PRIMARY), "Window(1)");
        assert_eq!(format!("{}", WindowId(42)), "Window(42)");
    }

    #[test]
    fn window_id_ordering() {
        let mut ids = vec![WindowId(3), WindowId(1), WindowId(2)];
        ids.sort();
        assert_eq!(ids, vec![WindowId(1), WindowId(2), WindowId(3)]);
    }
}
