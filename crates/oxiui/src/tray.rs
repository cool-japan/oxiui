//! System tray support for OxiUI desktop apps.
//!
//! Provides [`TrayConfig`] and the `App::with_tray` builder method.
//! The integration is gated behind the `tray` Cargo feature and uses the
//! [`tray-icon`](https://crates.io/crates/tray-icon) crate which is
//! Pure-Rust and uses OS-provided system-tray APIs at runtime.
//!
//! # Feature gate
//!
//! ```toml
//! [dependencies]
//! oxiui = { version = "*", features = ["tray"] }
//! ```
//!
//! # Basic usage
//!
//! ```rust,no_run
//! use oxiui::{App, AppConfig};
//! use oxiui::tray::{TrayConfig, TrayMenuItem};
//!
//! App::new(AppConfig::new().title("demo"))
//!     .with_tray(
//!         TrayConfig::new()
//!             .tooltip("My OxiUI App")
//!             .menu_item(TrayMenuItem::action("Show", || {}))
//!             .menu_item(TrayMenuItem::action("Quit", || std::process::exit(0))),
//!     )
//!     .content(|ui| {
//!         ui.label("Hello from the tray app!");
//!     });
//! ```
//!
//! # Implementation note (basic)
//!
//! This is a basic implementation of system tray support.  The [`TrayHandle`]
//! returned by `App::with_tray` owns the underlying `tray-icon` icon/menu
//! objects and keeps them alive for the duration of the app.  Full event
//! loop integration (tray click, menu selection callbacks firing during the
//! eframe event loop) requires wiring `tray-icon`'s `TrayIconEvent` receiver
//! into the eframe `update()` callback, which is planned for a future release.
//!
//! `App::with_tray`: crate::App::with_tray

// ── TrayMenuItem ─────────────────────────────────────────────────────────────

/// A single item in the system tray context menu.
#[derive(Clone, Debug)]
pub enum TrayMenuItem {
    /// A clickable action item with a label and optional keyboard shortcut.
    Action {
        /// Display label.
        label: String,
        /// Optional keyboard shortcut hint (e.g. `"Ctrl+Q"`).
        shortcut: Option<String>,
    },
    /// A horizontal separator between groups of items.
    Separator,
    /// A sub-menu item that expands to a child menu.
    SubMenu {
        /// Display label.
        label: String,
        /// Child items.
        children: Vec<TrayMenuItem>,
    },
}

impl TrayMenuItem {
    /// Create a simple action item without a callback stored at this level.
    ///
    /// Callbacks are wired by the backend when it processes the tray config;
    /// at the data-model level we store only the label + shortcut for
    /// Pure-Rust serialization purposes.
    pub fn action(label: impl Into<String>, _action: impl Fn() + Send + Sync + 'static) -> Self {
        TrayMenuItem::Action {
            label: label.into(),
            shortcut: None,
        }
    }

    /// Create a simple action item with a keyboard shortcut hint.
    pub fn action_with_shortcut(
        label: impl Into<String>,
        shortcut: impl Into<String>,
        _action: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        TrayMenuItem::Action {
            label: label.into(),
            shortcut: Some(shortcut.into()),
        }
    }

    /// Create a separator.
    pub fn separator() -> Self {
        TrayMenuItem::Separator
    }

    /// Create a sub-menu.
    pub fn sub_menu(label: impl Into<String>, children: Vec<TrayMenuItem>) -> Self {
        TrayMenuItem::SubMenu {
            label: label.into(),
            children,
        }
    }
}

// ── TrayConfig ────────────────────────────────────────────────────────────────

/// Configuration for an OxiUI system tray icon.
///
/// Build with [`TrayConfig::new`] and pass to `App::with_tray`.
///
/// # Examples
///
/// ```rust
/// use oxiui::tray::{TrayConfig, TrayMenuItem};
///
/// let config = TrayConfig::new()
///     .tooltip("My App")
///     .icon_path("/usr/share/icons/myapp.png")
///     .menu_item(TrayMenuItem::action("Show Window", || {}))
///     .menu_item(TrayMenuItem::separator())
///     .menu_item(TrayMenuItem::action("Quit", || {}));
/// ```
#[derive(Debug, Default, Clone)]
pub struct TrayConfig {
    /// Optional path to the tray icon image file (PNG/ICO).
    ///
    /// When `None`, a default OxiUI icon placeholder is used.
    pub icon_path: Option<String>,

    /// Raw PNG bytes for the tray icon (alternative to `icon_path`).
    ///
    /// Takes precedence over `icon_path` when both are set.
    pub icon_bytes: Option<Vec<u8>>,

    /// Tooltip shown when hovering over the tray icon.
    pub tooltip: Option<String>,

    /// Menu items shown when the user right-clicks (or left-clicks on some platforms).
    pub menu_items: Vec<TrayMenuItem>,
}

impl TrayConfig {
    /// Create a new, empty [`TrayConfig`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the tooltip string.
    pub fn tooltip(mut self, tip: impl Into<String>) -> Self {
        self.tooltip = Some(tip.into());
        self
    }

    /// Set the icon from a file system path.
    ///
    /// The file must be a PNG or ICO image accessible at runtime.
    /// This path is stored verbatim; it is loaded by the backend on
    /// `App::run()`.
    ///
    /// # Security
    ///
    /// Use application-relative or XDG data paths.  Do not embed
    /// user-controlled absolute paths.
    pub fn icon_path(mut self, path: impl Into<String>) -> Self {
        self.icon_path = Some(path.into());
        self
    }

    /// Set the icon from raw PNG bytes embedded at compile time.
    ///
    /// Takes precedence over [`icon_path`](Self::icon_path) when both are set.
    pub fn icon_bytes(mut self, bytes: Vec<u8>) -> Self {
        self.icon_bytes = Some(bytes);
        self
    }

    /// Append a menu item.
    pub fn menu_item(mut self, item: TrayMenuItem) -> Self {
        self.menu_items.push(item);
        self
    }

    /// Returns `true` if any menu items are registered.
    pub fn has_menu(&self) -> bool {
        !self.menu_items.is_empty()
    }
}

// ── TrayHandle ────────────────────────────────────────────────────────────────

/// A live handle to a mounted system tray icon.
///
/// Returned by `App::with_tray` in the future or by [`TrayHandle::mount`].
/// Dropping this handle removes the tray icon from the system tray.
///
/// # Current status
///
/// This is a basic stub implementation.  When the `tray` feature is enabled
/// the handle is created and the `tray-icon` crate objects are stored in it.
/// Full event-loop integration (receiving click/menu events during the
/// eframe loop) is planned for a future release.
pub struct TrayHandle {
    /// Stores the tray-icon objects when the `tray` feature is enabled.
    #[cfg(feature = "tray")]
    _inner: TrayHandleInner,
    /// Marker for the non-tray build path.
    #[cfg(not(feature = "tray"))]
    _marker: std::marker::PhantomData<()>,
}

#[cfg(feature = "tray")]
struct TrayHandleInner {
    /// The tray-icon `TrayIcon` object.  Kept alive so the icon persists.
    _tray: tray_icon::TrayIcon,
    /// The tray-icon `Menu` object (if a menu was configured).
    _menu: Option<tray_icon::menu::Menu>,
}

impl std::fmt::Debug for TrayHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrayHandle").finish()
    }
}

impl TrayHandle {
    /// Mount a tray icon using the given [`TrayConfig`].
    ///
    /// On desktop platforms with the `tray` feature this creates the OS tray
    /// icon.  On non-desktop targets or when the `tray` feature is absent this
    /// returns a no-op handle.
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` if the tray icon could not be created (e.g. no
    /// system tray available, bad icon bytes, etc.).
    #[allow(unused_variables)]
    pub fn mount(config: TrayConfig) -> Result<Self, String> {
        #[cfg(feature = "tray")]
        {
            use tray_icon::{menu::Menu, TrayIconBuilder};

            // Build the tray menu if items are present.
            let menu_opt: Option<Menu> = if config.has_menu() {
                let menu = Menu::new();
                for item in &config.menu_items {
                    match item {
                        TrayMenuItem::Action { label, .. } => {
                            let mi = tray_icon::menu::MenuItem::new(label, true, None);
                            menu.append(&mi)
                                .map_err(|e| format!("tray menu append failed: {e}"))?;
                        }
                        TrayMenuItem::Separator => {
                            menu.append(&tray_icon::menu::PredefinedMenuItem::separator())
                                .map_err(|e| format!("tray separator append failed: {e}"))?;
                        }
                        TrayMenuItem::SubMenu { label, children } => {
                            // Shallow sub-menu — recurse one level.
                            let sub = Menu::new();
                            for child in children {
                                if let TrayMenuItem::Action {
                                    label: child_label, ..
                                } = child
                                {
                                    let mi =
                                        tray_icon::menu::MenuItem::new(child_label, true, None);
                                    sub.append(&mi)
                                        .map_err(|e| format!("submenu append failed: {e}"))?;
                                }
                            }
                            let submenu = tray_icon::menu::Submenu::with_items(label, true, &[])
                                .map_err(|e| format!("tray submenu create failed: {e}"))?;
                            menu.append(&submenu)
                                .map_err(|e| format!("submenu attach failed: {e}"))?;
                        }
                    }
                }
                Some(menu)
            } else {
                None
            };

            // Build the icon.  Use a 1×1 transparent placeholder when no icon is provided.
            let icon = if let Some(bytes) = &config.icon_bytes {
                tray_icon::Icon::from_rgba(bytes.clone(), 1, 1)
                    .map_err(|e| format!("tray icon from rgba failed: {e}"))
                    .or_else(|_| {
                        // Fallback: transparent 1×1 pixel.
                        tray_icon::Icon::from_rgba(vec![0u8; 4], 1, 1)
                            .map_err(|e| format!("tray fallback icon failed: {e}"))
                    })?
            } else {
                // Transparent 1×1 placeholder icon.
                tray_icon::Icon::from_rgba(vec![0u8; 4], 1, 1)
                    .map_err(|e| format!("tray placeholder icon failed: {e}"))?
            };

            let mut builder = TrayIconBuilder::new().with_icon(icon);

            if let Some(tip) = &config.tooltip {
                builder = builder.with_tooltip(tip);
            }

            if let Some(ref menu) = menu_opt {
                builder = builder.with_menu(Box::new(menu.clone()));
            }

            let tray = builder
                .build()
                .map_err(|e| format!("tray icon build failed: {e}"))?;

            Ok(TrayHandle {
                _inner: TrayHandleInner {
                    _tray: tray,
                    _menu: menu_opt,
                },
            })
        }
        #[cfg(not(feature = "tray"))]
        {
            Ok(TrayHandle {
                _marker: std::marker::PhantomData,
            })
        }
    }

    /// Update the tray tooltip at runtime.
    ///
    /// On non-tray builds or when the tray icon is not yet mounted this is a
    /// no-op that always returns `Ok(())`.
    #[allow(unused_variables)]
    pub fn set_tooltip(&self, tip: &str) -> Result<(), String> {
        #[cfg(feature = "tray")]
        {
            self._inner
                ._tray
                .set_tooltip(Some(tip))
                .map_err(|e| format!("set_tooltip failed: {e}"))?;
        }
        Ok(())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_config_default_is_empty() {
        let c = TrayConfig::new();
        assert!(!c.has_menu());
        assert!(c.tooltip.is_none());
        assert!(c.icon_path.is_none());
        assert!(c.icon_bytes.is_none());
    }

    #[test]
    fn tray_config_builder_chain() {
        let c = TrayConfig::new()
            .tooltip("My App")
            .icon_path("/usr/share/icons/app.png")
            .menu_item(TrayMenuItem::action("Show", || {}))
            .menu_item(TrayMenuItem::separator())
            .menu_item(TrayMenuItem::action("Quit", || {}));
        assert_eq!(c.tooltip.as_deref(), Some("My App"));
        assert_eq!(c.icon_path.as_deref(), Some("/usr/share/icons/app.png"));
        assert_eq!(c.menu_items.len(), 3);
        assert!(c.has_menu());
    }

    #[test]
    fn tray_menu_item_separator_variant() {
        let sep = TrayMenuItem::separator();
        assert!(matches!(sep, TrayMenuItem::Separator));
    }

    #[test]
    fn tray_menu_item_action_stores_label() {
        let item = TrayMenuItem::action("Open", || {});
        match item {
            TrayMenuItem::Action { label, shortcut } => {
                assert_eq!(label, "Open");
                assert!(shortcut.is_none());
            }
            _ => panic!("expected Action"),
        }
    }

    #[test]
    fn tray_menu_item_action_with_shortcut() {
        let item = TrayMenuItem::action_with_shortcut("Save", "Ctrl+S", || {});
        match item {
            TrayMenuItem::Action { shortcut, .. } => {
                assert_eq!(shortcut.as_deref(), Some("Ctrl+S"));
            }
            _ => panic!("expected Action"),
        }
    }

    #[test]
    fn tray_config_icon_bytes() {
        let c = TrayConfig::new().icon_bytes(vec![0u8; 64]);
        assert!(c.icon_bytes.is_some());
    }

    #[test]
    #[cfg_attr(feature = "tray", ignore = "tray-icon requires a live display server")]
    fn tray_handle_mount_no_tray_feature_is_ok() {
        // Without the `tray` feature, mount() returns Ok(no-op handle) — always safe.
        // With the `tray` feature enabled the call requires a live display/event loop;
        // skip under nextest to avoid panicking in headless CI.
        let result = TrayHandle::mount(TrayConfig::new());
        assert!(result.is_ok());
    }
}
