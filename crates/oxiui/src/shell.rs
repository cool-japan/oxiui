//! The application *shell*: the parts of an [`crate::App`] that live outside
//! the primary window's content closure.
//!
//! A [`ShellConfig`] bundles everything a backend needs in order to honour the
//! facade's window- and menu-level builder APIs:
//!
//! * the [`SecondaryWindow`] descriptors registered with
//!   [`crate::App::open_window`] / [`crate::App::open_window_with`],
//! * their per-window content closures ([`crate::multiwindow::SharedContent`]),
//! * the [`crate::MenuBar`] attached with [`crate::App::menu_bar`], and
//! * the [`WindowHandle`] the app kept, so runtime open / close / focus
//!   requests reach the running event loop.
//!
//! `App::run()` builds the shell and passes it to the selected backend through
//! [`crate::BackendRunner::set_shell`] **before** the event loop starts. A
//! backend that cannot honour part of the shell must reject it with
//! [`oxiui_core::UiError::Unsupported`]; the default
//! [`crate::BackendRunner::set_shell`] implementation does exactly that, so a
//! third-party runner can never silently swallow a menu bar or a window.
//!
//! # Backend support matrix
//!
//! | Backend | secondary windows | menu bar |
//! |---|---|---|
//! | `egui` (native) | ✅ deferred viewports (real OS windows) | ✅ rendered at the top of the primary frame |
//! | `egui` (wasm32) | ❌ `UiError::Unsupported` (whole `App::run` is unsupported on wasm) | ❌ same |
//! | `iced` | ❌ `UiError::Unsupported` (iced 0.14 multi-window needs the `daemon` entry point) | ✅ rendered at the top of the primary view |
//! | `dioxus` | ❌ `UiError::Unsupported` (no native event loop yet) | ❌ same |

use crate::menu::MenuBar;
use crate::multiwindow::{SecondaryWindow, SharedContent, WindowHandle};

/// Backend-visible application shell: secondary windows plus the menu bar.
///
/// Constructed by `App::run()` via
/// [`crate::multiwindow::WindowRegistry::take_shell`].
#[derive(Default)]
pub struct ShellConfig {
    /// Registered secondary window descriptors, in registration order.
    pub windows: Vec<SecondaryWindow>,
    /// Per-window content closures, indexed by [`SecondaryWindow::content_key`].
    pub contents: Vec<SharedContent>,
    /// The application menu bar, if one was attached.
    pub menu_bar: Option<MenuBar>,
    /// Runtime command handle shared with [`crate::App::window_handle`].
    pub handle: WindowHandle,
}

impl ShellConfig {
    /// Returns `true` when the shell carries nothing a backend has to honour.
    ///
    /// An empty shell is always accepted, including by backends that support
    /// neither secondary windows nor menu bars.
    pub fn is_empty(&self) -> bool {
        self.windows.is_empty() && self.menu_bar.is_none()
    }

    /// Returns `true` when at least one secondary window was registered.
    pub fn has_secondary_windows(&self) -> bool {
        !self.windows.is_empty()
    }

    /// Returns `true` when a menu bar was attached.
    pub fn has_menu_bar(&self) -> bool {
        self.menu_bar.is_some()
    }

    /// The shared content closure for `window`, if it was registered with one.
    pub fn content_for(&self, window: &SecondaryWindow) -> Option<&SharedContent> {
        window.content_key.and_then(|key| self.contents.get(key))
    }

    /// A human-readable summary used in `Unsupported` error messages.
    pub(crate) fn describe(&self) -> String {
        format!(
            "{} secondary window(s), menu bar: {}",
            self.windows.len(),
            self.has_menu_bar()
        )
    }
}

impl std::fmt::Debug for ShellConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShellConfig")
            .field("windows", &self.windows)
            .field("contents", &self.contents.len())
            .field("menu_bar", &self.menu_bar)
            .finish()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::multiwindow::WindowRegistry;
    use oxiui_core::window::WindowConfig;

    #[test]
    fn default_shell_is_empty() {
        let shell = ShellConfig::default();
        assert!(shell.is_empty());
        assert!(!shell.has_secondary_windows());
        assert!(!shell.has_menu_bar());
    }

    #[test]
    fn shell_with_menu_bar_is_not_empty() {
        let shell = ShellConfig {
            menu_bar: Some(MenuBar::build(|mb| {
                mb.menu("File", |m| {
                    m.item("Quit", None, || {});
                });
            })),
            ..Default::default()
        };
        assert!(!shell.is_empty());
        assert!(shell.has_menu_bar());
        assert!(!shell.has_secondary_windows());
    }

    #[test]
    fn take_shell_moves_descriptors_and_contents() {
        let mut reg = WindowRegistry::new();
        reg.open_window(WindowConfig::new("plain"));
        reg.open_window_with(WindowConfig::new("rich"), |ui| ui.label("hi"));
        let shell = reg.take_shell(None);
        assert_eq!(shell.windows.len(), 2);
        assert_eq!(shell.contents.len(), 1);
        assert!(shell.has_secondary_windows());
        // The registry no longer holds the moved-out descriptors.
        assert_eq!(reg.secondary_count(), 0);
    }

    #[test]
    fn content_for_resolves_the_right_closure() {
        let mut reg = WindowRegistry::new();
        reg.open_window(WindowConfig::new("plain"));
        reg.open_window_with(WindowConfig::new("rich"), |ui| ui.label("hi"));
        let shell = reg.take_shell(None);
        assert!(shell.content_for(&shell.windows[0]).is_none());
        assert!(shell.content_for(&shell.windows[1]).is_some());
    }

    #[test]
    fn describe_reports_counts() {
        let mut reg = WindowRegistry::new();
        reg.open_window(WindowConfig::new("a"));
        let shell = reg.take_shell(Some(MenuBar::build(|_| {})));
        assert_eq!(shell.describe(), "1 secondary window(s), menu bar: true");
    }
}
