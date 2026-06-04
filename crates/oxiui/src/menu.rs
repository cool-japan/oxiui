//! Native menu bar builder.
//!
//! Provides a cross-platform, backend-agnostic menu definition API for the
//! [`App`] facade.  Menus are built with a closure-based DSL and stored as a
//! data tree; backend adapters convert the tree to platform-specific menus
//! when `App::run()` starts.
//!
//! **Pure Rust data model.**  This module does NOT call any OS menu APIs —
//! it only constructs an in-memory tree of [`MenuItem`] / [`Menu`] nodes.
//! Backends that support native menu bars (egui via `egui-menu`, iced via
//! `iced::widget::menu`) should traverse the [`MenuBar`] tree returned by
//! [`App::menu_bar`] and map it to their native representation.
//!
//! # Usage
//!
//! ```rust
//! use oxiui::menu::{MenuBar, MenuBarBuilder};
//!
//! let bar = MenuBar::build(|mb| {
//!     mb.menu("File", |m| {
//!         m.item("New",  None, || {});
//!         m.item("Open", Some("Ctrl+O"), || {});
//!         m.separator();
//!         m.item("Quit", Some("Ctrl+Q"), || {});
//!     });
//!     mb.menu("Edit", |m| {
//!         m.item("Undo", Some("Ctrl+Z"), || {});
//!         m.item("Redo", Some("Ctrl+Y"), || {});
//!     });
//! });
//!
//! assert_eq!(bar.menus().len(), 2);
//! assert_eq!(bar.menus()[0].label(), "File");
//! assert_eq!(bar.menus()[1].label(), "Edit");
//! ```

// ── MenuItem ─────────────────────────────────────────────────────────────────

/// A single item in a [`Menu`].
pub enum MenuItem {
    /// A clickable action item.
    Action {
        /// Display label.
        label: String,
        /// Optional keyboard shortcut hint (e.g. `"Ctrl+S"`).
        shortcut: Option<String>,
        /// Callback invoked when the item is selected.
        action: Box<dyn Fn() + Send + Sync>,
    },
    /// A visual separator (horizontal rule between groups of actions).
    Separator,
    /// A nested submenu.
    Submenu {
        /// Label for the submenu root item.
        label: String,
        /// The nested menu.
        menu: Menu,
    },
}

impl std::fmt::Debug for MenuItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MenuItem::Action {
                label, shortcut, ..
            } => f
                .debug_struct("MenuItem::Action")
                .field("label", label)
                .field("shortcut", shortcut)
                .finish(),
            MenuItem::Separator => write!(f, "MenuItem::Separator"),
            MenuItem::Submenu { label, menu } => f
                .debug_struct("MenuItem::Submenu")
                .field("label", label)
                .field("menu", menu)
                .finish(),
        }
    }
}

// ── Menu ─────────────────────────────────────────────────────────────────────

/// A drop-down menu containing [`MenuItem`]s.
#[derive(Debug)]
pub struct Menu {
    label: String,
    items: Vec<MenuItem>,
}

impl Menu {
    /// Create an empty menu with the given label.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            items: Vec::new(),
        }
    }

    /// The label shown on the menu root button.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// The items in this menu in definition order.
    pub fn items(&self) -> &[MenuItem] {
        &self.items
    }

    /// Add a clickable action item to this menu.
    pub fn item(
        &mut self,
        label: impl Into<String>,
        shortcut: Option<&str>,
        action: impl Fn() + Send + Sync + 'static,
    ) -> &mut Self {
        self.items.push(MenuItem::Action {
            label: label.into(),
            shortcut: shortcut.map(|s| s.to_string()),
            action: Box::new(action),
        });
        self
    }

    /// Add a visual separator to this menu.
    pub fn separator(&mut self) -> &mut Self {
        self.items.push(MenuItem::Separator);
        self
    }

    /// Add a nested submenu.
    pub fn submenu<F>(&mut self, label: impl Into<String>, build: F) -> &mut Self
    where
        F: FnOnce(&mut Menu),
    {
        let mut sub = Menu::new(label);
        build(&mut sub);
        self.items.push(MenuItem::Submenu {
            label: sub.label.clone(),
            menu: sub,
        });
        self
    }
}

// ── MenuBar ────────────────────────────────────────────────────────────────────

/// The application-level menu bar.
///
/// A menu bar is a horizontal row of top-level [`Menu`]s, each opening a
/// drop-down when clicked.  Build it with [`MenuBar::build`] and register it
/// via `App::with_menu_bar`.
#[derive(Debug)]
pub struct MenuBar {
    menus: Vec<Menu>,
}

impl MenuBar {
    /// Build a [`MenuBar`] using a closure that receives a [`MenuBarBuilder`].
    ///
    /// # Example
    ///
    /// ```rust
    /// use oxiui::menu::MenuBar;
    ///
    /// let bar = MenuBar::build(|mb| {
    ///     mb.menu("File", |m| {
    ///         m.item("Open", Some("Ctrl+O"), || {});
    ///         m.item("Quit", Some("Ctrl+Q"), || {});
    ///     });
    /// });
    /// assert_eq!(bar.menus().len(), 1);
    /// ```
    pub fn build<F>(build: F) -> Self
    where
        F: FnOnce(&mut MenuBarBuilder),
    {
        let mut builder = MenuBarBuilder { menus: Vec::new() };
        build(&mut builder);
        MenuBar {
            menus: builder.menus,
        }
    }

    /// Returns the top-level menus in definition order.
    pub fn menus(&self) -> &[Menu] {
        &self.menus
    }

    /// Returns the total number of top-level menus.
    pub fn menu_count(&self) -> usize {
        self.menus.len()
    }

    /// Returns the top-level menu with the given label, if any.
    pub fn find_menu(&self, label: &str) -> Option<&Menu> {
        self.menus.iter().find(|m| m.label() == label)
    }
}

// ── MenuBarBuilder ────────────────────────────────────────────────────────────

/// Builder context passed to the closure in [`MenuBar::build`].
pub struct MenuBarBuilder {
    menus: Vec<Menu>,
}

impl MenuBarBuilder {
    /// Append a top-level menu with the given label.
    ///
    /// The `build` closure receives a [`Menu`] that can be populated with
    /// [`Menu::item`], [`Menu::separator`], and [`Menu::submenu`] calls.
    pub fn menu<F>(&mut self, label: impl Into<String>, build: F) -> &mut Self
    where
        F: FnOnce(&mut Menu),
    {
        let mut menu = Menu::new(label);
        build(&mut menu);
        self.menus.push(menu);
        self
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_empty_menu_bar() {
        let bar = MenuBar::build(|_mb| {});
        assert_eq!(bar.menu_count(), 0);
        assert!(bar.menus().is_empty());
    }

    #[test]
    fn build_single_menu() {
        let bar = MenuBar::build(|mb| {
            mb.menu("File", |m| {
                m.item("Quit", None, || {});
            });
        });
        assert_eq!(bar.menu_count(), 1);
        let file = &bar.menus()[0];
        assert_eq!(file.label(), "File");
        assert_eq!(file.items().len(), 1);
    }

    #[test]
    fn build_multiple_menus() {
        let bar = MenuBar::build(|mb| {
            mb.menu("File", |m| {
                m.item("Open", Some("Ctrl+O"), || {});
                m.item("Quit", Some("Ctrl+Q"), || {});
            });
            mb.menu("Edit", |m| {
                m.item("Undo", Some("Ctrl+Z"), || {});
            });
            mb.menu("Help", |m| {
                m.item("About", None, || {});
            });
        });
        assert_eq!(bar.menu_count(), 3);
        assert_eq!(bar.menus()[0].label(), "File");
        assert_eq!(bar.menus()[1].label(), "Edit");
        assert_eq!(bar.menus()[2].label(), "Help");
    }

    #[test]
    fn item_shortcut_stored() {
        let bar = MenuBar::build(|mb| {
            mb.menu("File", |m| {
                m.item("Save", Some("Ctrl+S"), || {});
            });
        });
        let item = &bar.menus()[0].items()[0];
        if let MenuItem::Action { shortcut, .. } = item {
            assert_eq!(shortcut.as_deref(), Some("Ctrl+S"));
        } else {
            panic!("expected Action item");
        }
    }

    #[test]
    fn separator_item() {
        let bar = MenuBar::build(|mb| {
            mb.menu("File", |m| {
                m.item("New", None, || {});
                m.separator();
                m.item("Quit", None, || {});
            });
        });
        let items = bar.menus()[0].items();
        assert_eq!(items.len(), 3);
        assert!(matches!(items[1], MenuItem::Separator));
    }

    #[test]
    fn nested_submenu() {
        let bar = MenuBar::build(|mb| {
            mb.menu("View", |m| {
                m.submenu("Theme", |sub| {
                    sub.item("Dark", None, || {});
                    sub.item("Light", None, || {});
                });
            });
        });
        let items = bar.menus()[0].items();
        assert_eq!(items.len(), 1);
        if let MenuItem::Submenu { label, menu } = &items[0] {
            assert_eq!(label, "Theme");
            assert_eq!(menu.items().len(), 2);
        } else {
            panic!("expected Submenu item");
        }
    }

    #[test]
    fn find_menu_by_label() {
        let bar = MenuBar::build(|mb| {
            mb.menu("File", |m| {
                m.item("Quit", None, || {});
            });
            mb.menu("Help", |m| {
                m.item("About", None, || {});
            });
        });
        assert!(bar.find_menu("File").is_some());
        assert!(bar.find_menu("Help").is_some());
        assert!(bar.find_menu("Missing").is_none());
    }

    #[test]
    fn action_callback_is_callable() {
        use std::sync::{Arc, Mutex};
        let counter = Arc::new(Mutex::new(0usize));
        let c = counter.clone();
        let bar = MenuBar::build(|mb| {
            mb.menu("File", move |m| {
                let c2 = c.clone();
                m.item("Click", None, move || {
                    *c2.lock().unwrap() += 1;
                });
            });
        });
        if let MenuItem::Action { action, .. } = &bar.menus()[0].items()[0] {
            action();
        }
        assert_eq!(*counter.lock().unwrap(), 1);
    }
}
