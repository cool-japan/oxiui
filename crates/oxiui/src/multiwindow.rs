//! Multi-window support for the [`App`] facade.
//!
//! This module provides a headless window registry that tracks secondary windows
//! alongside the core [`oxiui_core::WindowManager`].  At the facade level the
//! registry keeps a list of pending [`SecondaryWindow`] descriptors; each backend
//! is responsible for actually opening OS windows when `App::run()` starts.
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
//! ```

use oxiui_core::window::{WindowChannel, WindowConfig, WindowId, WindowManager};

// ── SecondaryWindow ───────────────────────────────────────────────────────────

/// A pending secondary window descriptor registered via `App::open_window`.
///
/// Backends consume this list on `App::run()` and open OS windows accordingly.
#[derive(Clone, Debug)]
pub struct SecondaryWindow {
    /// Stable window identifier assigned at registration time.
    pub id: WindowId,
    /// Configuration for the window (title, size, flags).
    pub config: WindowConfig,
    /// Optional content closure index (reserved for M7 multi-content routing).
    pub content_key: Option<usize>,
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
}

impl WindowRegistry {
    /// Create a new, empty registry with only the primary window.
    pub fn new() -> Self {
        Self {
            manager: WindowManager::default(),
            secondary: Vec::new(),
        }
    }

    /// Register a new secondary window with the given configuration.
    ///
    /// Returns the [`WindowId`] assigned to the new window.  The backend will
    /// open the corresponding OS window when the event loop starts.
    pub fn open_window(&mut self, config: WindowConfig) -> WindowId {
        let id = self.manager.create_window(config.clone());
        self.secondary.push(SecondaryWindow {
            id,
            config,
            content_key: None,
        });
        id
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
}
