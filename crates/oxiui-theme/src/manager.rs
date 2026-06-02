//! [`ThemeManager`] — runtime theme switching with observer notifications.
//!
//! ```rust
//! use oxiui_theme::{CooljapanTheme, dark, light};
//! use oxiui_theme::manager::ThemeManager;
//! use std::sync::{Arc, Mutex};
//!
//! // Wrap the initial theme (must be a concrete Clone-able type).
//! let initial = oxiui_theme::cooljapan_default();
//! ```

use crate::CooljapanTheme;
use std::sync::atomic::{AtomicU64, Ordering};

/// A callback invoked whenever the active theme changes.
pub type ThemeListener = Box<dyn Fn(&CooljapanTheme) + Send + Sync>;

/// A unique handle returned by [`ThemeManager::subscribe`].
///
/// Pass this to [`ThemeManager::unsubscribe`] to stop receiving notifications.
pub type ListenerId = u64;

static NEXT_LISTENER_ID: AtomicU64 = AtomicU64::new(1);

/// Runtime theme manager with observer notifications.
///
/// Holds one active [`CooljapanTheme`] and notifies all registered listeners
/// whenever [`set_theme`](ThemeManager::set_theme) is called.
///
/// # Example
/// ```rust
/// use oxiui_core::{Color, FontSpec, Palette};
/// use oxiui_theme::{CooljapanTheme};
/// use oxiui_theme::manager::ThemeManager;
/// use std::sync::{Arc, Mutex};
///
/// let initial = CooljapanTheme::new(
///     Palette {
///         background: Color(0, 0, 0, 255),
///         surface: Color(10, 10, 26, 255),
///         primary: Color(255, 255, 0, 255),
///         on_primary: Color(0, 0, 0, 255),
///         text: Color(255, 255, 255, 255),
///         muted: Color(200, 200, 200, 255),
///     },
///     FontSpec::new("Inter", 14.0, 400),
/// );
/// let mut manager = ThemeManager::new(initial.clone());
/// let called = Arc::new(Mutex::new(false));
/// let c = called.clone();
/// manager.subscribe(Box::new(move |_| { *c.lock().unwrap() = true; }));
/// manager.set_theme(initial);
/// assert!(*called.lock().unwrap());
/// ```
pub struct ThemeManager {
    active: CooljapanTheme,
    listeners: Vec<(ListenerId, ThemeListener)>,
}

impl ThemeManager {
    /// Construct a manager starting with `initial` as the active theme.
    pub fn new(initial: CooljapanTheme) -> Self {
        Self {
            active: initial,
            listeners: Vec::new(),
        }
    }

    /// Return a reference to the currently active theme.
    pub fn theme(&self) -> &CooljapanTheme {
        &self.active
    }

    /// Switch the active theme and notify every registered listener.
    pub fn set_theme(&mut self, theme: CooljapanTheme) {
        self.active = theme;
        for (_, listener) in &self.listeners {
            listener(&self.active);
        }
    }

    /// Register a listener and return its [`ListenerId`].
    ///
    /// The listener is called synchronously inside [`set_theme`](ThemeManager::set_theme)
    /// with a reference to the new theme.
    pub fn subscribe(&mut self, f: ThemeListener) -> ListenerId {
        let id = NEXT_LISTENER_ID.fetch_add(1, Ordering::Relaxed);
        self.listeners.push((id, f));
        id
    }

    /// Remove the listener registered with `id`.
    ///
    /// If `id` is not found, this is a no-op.
    pub fn unsubscribe(&mut self, id: ListenerId) {
        self.listeners.retain(|(lid, _)| *lid != id);
    }

    /// Returns the number of currently registered listeners.
    pub fn listener_count(&self) -> usize {
        self.listeners.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxiui_core::{Color, FontSpec, Palette};
    use std::sync::{Arc, Mutex};

    fn make_theme(bg: u8) -> CooljapanTheme {
        CooljapanTheme::new(
            Palette {
                background: Color(bg, bg, bg, 255),
                surface: Color(bg, bg, bg, 255),
                primary: Color(0, 0, 200, 255),
                on_primary: Color(255, 255, 255, 255),
                text: Color(0, 0, 0, 255),
                muted: Color(60, 60, 60, 255),
            },
            FontSpec::new("Inter", 14.0, 400),
        )
    }

    #[test]
    fn theme_manager_set_fires_listeners() {
        let mut manager = ThemeManager::new(make_theme(0));
        let called = Arc::new(Mutex::new(0u32));
        let c = called.clone();
        manager.subscribe(Box::new(move |_| {
            *c.lock().unwrap() += 1;
        }));
        manager.set_theme(make_theme(255));
        assert_eq!(
            *called.lock().unwrap(),
            1,
            "listener should be called exactly once"
        );
    }

    #[test]
    fn theme_manager_unsubscribe() {
        let mut manager = ThemeManager::new(make_theme(0));
        let called = Arc::new(Mutex::new(0u32));
        let c = called.clone();
        let id = manager.subscribe(Box::new(move |_| {
            *c.lock().unwrap() += 1;
        }));
        manager.unsubscribe(id);
        manager.set_theme(make_theme(128));
        assert_eq!(
            *called.lock().unwrap(),
            0,
            "unsubscribed listener must not be called"
        );
    }

    #[test]
    fn theme_manager_multiple_listeners() {
        let mut manager = ThemeManager::new(make_theme(0));
        let counts: Vec<Arc<Mutex<u32>>> = (0..3).map(|_| Arc::new(Mutex::new(0u32))).collect();
        for c in &counts {
            let c = c.clone();
            manager.subscribe(Box::new(move |_| {
                *c.lock().unwrap() += 1;
            }));
        }
        manager.set_theme(make_theme(42));
        for (i, c) in counts.iter().enumerate() {
            assert_eq!(*c.lock().unwrap(), 1, "listener {i} must be called once");
        }
    }

    #[test]
    fn theme_manager_theme_getter() {
        use oxiui_core::Theme;
        let theme = make_theme(100);
        let manager = ThemeManager::new(theme.clone());
        let active = manager.theme();
        assert_eq!(active.palette().background, Color(100, 100, 100, 255));
    }
}
