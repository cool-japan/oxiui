//! Menu bar model **and** its backend-agnostic renderer.
//!
//! Provides a cross-platform menu definition API for the [`App`] facade.  Menus
//! are built with a closure-based DSL ([`MenuBar::build`]) and stored as a data
//! tree.  [`crate::menu::render_menu_bar`] then draws that tree through any
//! [`oxiui_core::UiCtx`] and dispatches [`MenuItem::Action`] callbacks when the
//! user clicks an item, so every backend that implements `UiCtx` gets a working
//! menu bar without writing menu code of its own.
//!
//! **Pure Rust, no OS menu APIs.**  The bar is drawn with ordinary widgets
//! (`menu_bar` / `button` / `popup` / `separator`); on a backend whose `UiCtx`
//! reports a container as unsupported the renderer degrades gracefully
//! (`menu_bar` → `horizontal` → inline, `popup` → `vertical` → inline) instead
//! of dropping the menu.
//!
//! `App::run()` moves the attached [`MenuBar`] into a
//! [`crate::shell::ShellConfig`]; the native egui backend renders it at the top
//! of the primary frame and the iced backend at the top of the primary view.
//! Interaction state (which menu is open) lives in [`MenuBarState`], which the
//! backend owns across frames.
//!
//! # Widget id ranges
//!
//! Because the bar draws a *varying* number of widgets — the drop-down appears
//! and disappears — backends must render it through a context whose widget-id
//! range is **disjoint** from the app content's
//! ([`oxiui_egui::EguiUiCtx::with_id_base`] /
//! `oxiui_iced::adapter::IcedUiCtx::with_id_base`). Sharing one range would
//! shift every content widget id whenever a menu opened, breaking egui's
//! persistent widget state and iced's click routing.
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

use oxiui_core::UiCtx;

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

// ── MenuBarState ──────────────────────────────────────────────────────────────

/// Cross-frame interaction state for a rendered [`MenuBar`].
///
/// Tracks which menu (and which chain of submenus) is currently open. Backends
/// own one instance per menu bar and pass it to [`render_menu_bar`] each frame.
///
/// `open_path[0]` is the index of the open top-level menu, `open_path[1]` the
/// index of the open submenu inside it, and so on. An empty path means the bar
/// is closed.
#[derive(Clone, Debug, Default)]
pub struct MenuBarState {
    open_path: Vec<usize>,
}

impl MenuBarState {
    /// Create a closed menu-bar state.
    pub fn new() -> Self {
        Self::default()
    }

    /// The index of the currently open top-level menu, if any.
    pub fn open_menu(&self) -> Option<usize> {
        self.open_path.first().copied()
    }

    /// The full open chain: top-level index, then one index per open submenu.
    pub fn open_path(&self) -> &[usize] {
        &self.open_path
    }

    /// Whether any menu is currently open.
    pub fn is_open(&self) -> bool {
        !self.open_path.is_empty()
    }

    /// Open the top-level menu at `index`, or close it if it is already open.
    ///
    /// Switching to a different top-level menu also closes every open submenu.
    pub fn toggle_menu(&mut self, index: usize) {
        if self.open_menu() == Some(index) {
            self.open_path.clear();
        } else {
            self.open_path.clear();
            self.open_path.push(index);
        }
    }

    /// Open or close the submenu at `index` nested at `depth`.
    ///
    /// `depth` is the position the submenu occupies in the open path
    /// (`1` for a submenu of a top-level menu). Deeper entries are discarded,
    /// so opening a sibling submenu closes the previously open one.
    pub fn toggle_submenu(&mut self, depth: usize, index: usize) {
        if self.open_path.len() > depth && self.open_path[depth] == index {
            self.open_path.truncate(depth);
        } else {
            self.open_path.truncate(depth);
            self.open_path.push(index);
        }
    }

    /// Close the whole bar.
    pub fn close(&mut self) {
        self.open_path.clear();
    }
}

// ── Rendering ─────────────────────────────────────────────────────────────────

/// Format a menu item's caption, appending its keyboard shortcut hint.
pub fn item_label(label: &str, shortcut: Option<&str>) -> String {
    match shortcut {
        Some(sc) if !sc.is_empty() => format!("{label}    {sc}"),
        _ => label.to_string(),
    }
}

/// Render `bar` through `ui`, updating `state` and firing item actions.
///
/// Returns the number of [`MenuItem::Action`] callbacks invoked in this frame
/// (`0` in the common case where the user clicked nothing).
///
/// The renderer is fully backend-agnostic: it uses only `UiCtx` primitives and
/// falls back to weaker containers when the active adapter reports one as
/// unsupported ([`oxiui_core::response::WidgetResponse::supported`] is `false`),
/// so the menu is always drawn.
///
/// # Example
///
/// ```rust
/// use oxiui::menu::{render_menu_bar, MenuBar, MenuBarState};
/// use oxiui_core::{ButtonResponse, UiCtx};
///
/// /// A minimal probe context: records button captions, never reports a click.
/// struct Probe {
///     labels: Vec<String>,
/// }
/// impl UiCtx for Probe {
///     fn heading(&mut self, _text: &str) {}
///     fn label(&mut self, _text: &str) {}
///     fn button(&mut self, label: &str) -> ButtonResponse {
///         self.labels.push(label.to_string());
///         ButtonResponse::default()
///     }
/// }
///
/// let bar = MenuBar::build(|mb| {
///     mb.menu("File", |m| {
///         m.item("Quit", Some("Ctrl+Q"), || {});
///     });
/// });
/// let mut state = MenuBarState::new();
/// let mut ui = Probe { labels: Vec::new() };
///
/// // Closed bar: only the top-level menu button is drawn.
/// assert_eq!(render_menu_bar(&bar, &mut state, &mut ui), 0);
/// assert_eq!(ui.labels, vec!["File".to_string()]);
/// ```
pub fn render_menu_bar(bar: &MenuBar, state: &mut MenuBarState, ui: &mut dyn UiCtx) -> usize {
    // ── 1. The top-level row of menu buttons ──────────────────────────────────
    let labels: Vec<String> = bar.menus().iter().map(|m| m.label().to_string()).collect();
    let mut clicked_top: Option<usize> = None;
    {
        let mut row = |ui: &mut dyn UiCtx| {
            for (index, label) in labels.iter().enumerate() {
                if ui.button(label).clicked {
                    clicked_top = Some(index);
                }
            }
        };
        if !ui.menu_bar(&mut row).supported && !ui.horizontal(&mut row).supported {
            // Neither container exists on this adapter: draw the buttons inline.
            row(ui);
        }
    }
    if let Some(index) = clicked_top {
        state.toggle_menu(index);
    }

    // ── 2. The drop-down for the open menu (if any) ───────────────────────────
    let Some(top) = state.open_menu() else {
        return 0;
    };
    let Some(menu) = bar.menus().get(top) else {
        // The bar shrank since the menu was opened — drop the stale path.
        state.close();
        return 0;
    };

    let path = state.open_path().to_vec();
    let mut outcome = ItemOutcome::default();
    {
        let mut body = |ui: &mut dyn UiCtx| {
            render_menu_items(menu, &path, 1, ui, &mut outcome);
        };
        if !ui.popup(&mut body).supported && !ui.vertical(&mut body).supported {
            body(ui);
        }
    }

    if let Some((depth, index)) = outcome.toggle_submenu {
        state.toggle_submenu(depth, index);
    }
    if outcome.close_requested {
        state.close();
    }
    outcome.fired
}

/// Accumulated result of rendering one drop-down's items.
#[derive(Default)]
struct ItemOutcome {
    /// A submenu the user toggled this frame (`(depth, index)`).
    toggle_submenu: Option<(usize, usize)>,
    /// Number of action callbacks invoked.
    fired: usize,
    /// Whether an action was chosen (which closes the bar).
    close_requested: bool,
}

/// Draw the items of `menu`, recursing into the submenu named by `path[depth]`.
fn render_menu_items(
    menu: &Menu,
    path: &[usize],
    depth: usize,
    ui: &mut dyn UiCtx,
    outcome: &mut ItemOutcome,
) {
    for (index, item) in menu.items().iter().enumerate() {
        match item {
            MenuItem::Action {
                label,
                shortcut,
                action,
            } => {
                if ui.button(&item_label(label, shortcut.as_deref())).clicked {
                    action();
                    outcome.fired += 1;
                    outcome.close_requested = true;
                }
            }
            MenuItem::Separator => {
                ui.separator();
            }
            MenuItem::Submenu { label, menu: sub } => {
                let expanded = path.get(depth).copied() == Some(index);
                let marker = if expanded { '-' } else { '+' };
                if ui.button(&format!("{label} {marker}")).clicked {
                    outcome.toggle_submenu = Some((depth, index));
                }
                if expanded {
                    let mut nested = |ui: &mut dyn UiCtx| {
                        render_menu_items(sub, path, depth + 1, ui, outcome);
                    };
                    if !ui.vertical(&mut nested).supported {
                        nested(ui);
                    }
                }
            }
        }
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

    // ── Renderer ──────────────────────────────────────────────────────────────

    use oxiui_core::response::WidgetResponse;
    use oxiui_core::ButtonResponse;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    /// A `UiCtx` that records every widget call and reports a click for one
    /// caption. Container methods are left at their `UiCtx` defaults
    /// (`unsupported`), which exercises the renderer's inline fallback path.
    struct MinimalCtx {
        click_on: Option<String>,
        buttons: Vec<String>,
        separators: usize,
    }

    impl MinimalCtx {
        fn new(click_on: Option<&str>) -> Self {
            Self {
                click_on: click_on.map(|s| s.to_string()),
                buttons: Vec::new(),
                separators: 0,
            }
        }
    }

    impl UiCtx for MinimalCtx {
        fn heading(&mut self, _text: &str) {}
        fn label(&mut self, _text: &str) {}
        fn button(&mut self, label: &str) -> ButtonResponse {
            self.buttons.push(label.to_string());
            ButtonResponse {
                clicked: self.click_on.as_deref() == Some(label),
                hovered: false,
            }
        }
        fn separator(&mut self) -> WidgetResponse {
            self.separators += 1;
            WidgetResponse::supported()
        }
    }

    /// A `UiCtx` whose containers are supported, so the renderer nests calls.
    /// Records the container names it was asked for, in order.
    struct ContainerCtx {
        click_on: Option<String>,
        buttons: Vec<String>,
        containers: Vec<&'static str>,
    }

    impl ContainerCtx {
        fn new(click_on: Option<&str>) -> Self {
            Self {
                click_on: click_on.map(|s| s.to_string()),
                buttons: Vec::new(),
                containers: Vec::new(),
            }
        }
    }

    impl UiCtx for ContainerCtx {
        fn heading(&mut self, _text: &str) {}
        fn label(&mut self, _text: &str) {}
        fn button(&mut self, label: &str) -> ButtonResponse {
            self.buttons.push(label.to_string());
            ButtonResponse {
                clicked: self.click_on.as_deref() == Some(label),
                hovered: false,
            }
        }
        fn menu_bar(&mut self, content: &mut dyn FnMut(&mut dyn UiCtx)) -> WidgetResponse {
            self.containers.push("menu_bar");
            content(self);
            WidgetResponse::supported()
        }
        fn popup(&mut self, content: &mut dyn FnMut(&mut dyn UiCtx)) -> WidgetResponse {
            self.containers.push("popup");
            content(self);
            WidgetResponse::supported()
        }
        fn vertical(&mut self, content: &mut dyn FnMut(&mut dyn UiCtx)) -> WidgetResponse {
            self.containers.push("vertical");
            content(self);
            WidgetResponse::supported()
        }
    }

    fn demo_bar(counter: Arc<AtomicUsize>) -> MenuBar {
        MenuBar::build(move |mb| {
            let hit = Arc::clone(&counter);
            mb.menu("File", move |m| {
                m.item("New", None, || {});
                m.separator();
                m.item("Open", Some("Ctrl+O"), move || {
                    hit.fetch_add(1, Ordering::SeqCst);
                });
                m.submenu("Recent", |sub| {
                    sub.item("a.txt", None, || {});
                    sub.item("b.txt", None, || {});
                });
            });
            mb.menu("Help", |m| {
                m.item("About", None, || {});
            });
        })
    }

    #[test]
    fn item_label_appends_shortcut() {
        assert_eq!(item_label("Open", Some("Ctrl+O")), "Open    Ctrl+O");
        assert_eq!(item_label("Open", None), "Open");
        assert_eq!(item_label("Open", Some("")), "Open");
    }

    #[test]
    fn closed_bar_renders_only_top_level_buttons() {
        let bar = demo_bar(Arc::new(AtomicUsize::new(0)));
        let mut state = MenuBarState::new();
        let mut ui = MinimalCtx::new(None);
        assert_eq!(render_menu_bar(&bar, &mut state, &mut ui), 0);
        assert_eq!(ui.buttons, vec!["File".to_string(), "Help".to_string()]);
        assert!(!state.is_open());
    }

    #[test]
    fn clicking_a_menu_opens_it_and_renders_its_items() {
        let bar = demo_bar(Arc::new(AtomicUsize::new(0)));
        let mut state = MenuBarState::new();

        // Frame 1: click "File" — the drop-down opens in the same frame.
        let mut ui = MinimalCtx::new(Some("File"));
        assert_eq!(render_menu_bar(&bar, &mut state, &mut ui), 0);
        assert_eq!(state.open_menu(), Some(0));
        assert!(ui.buttons.contains(&"Open    Ctrl+O".to_string()));
        assert_eq!(ui.separators, 1);

        // Frame 2: no click — the menu stays open.
        let mut ui = MinimalCtx::new(None);
        assert_eq!(render_menu_bar(&bar, &mut state, &mut ui), 0);
        assert!(ui.buttons.contains(&"New".to_string()));
        assert_eq!(state.open_menu(), Some(0));
    }

    #[test]
    fn clicking_an_item_fires_its_action_and_closes_the_bar() {
        let counter = Arc::new(AtomicUsize::new(0));
        let bar = demo_bar(Arc::clone(&counter));
        let mut state = MenuBarState::new();
        state.toggle_menu(0);

        let mut ui = MinimalCtx::new(Some("Open    Ctrl+O"));
        assert_eq!(render_menu_bar(&bar, &mut state, &mut ui), 1);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
        assert!(!state.is_open(), "choosing an item must close the bar");

        // The next frame renders only the top-level row again.
        let mut ui = MinimalCtx::new(None);
        assert_eq!(render_menu_bar(&bar, &mut state, &mut ui), 0);
        assert_eq!(ui.buttons, vec!["File".to_string(), "Help".to_string()]);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn clicking_the_open_menu_again_closes_it() {
        let bar = demo_bar(Arc::new(AtomicUsize::new(0)));
        let mut state = MenuBarState::new();
        state.toggle_menu(0);
        let mut ui = MinimalCtx::new(Some("File"));
        render_menu_bar(&bar, &mut state, &mut ui);
        assert!(!state.is_open());
    }

    #[test]
    fn switching_menus_closes_the_previous_one() {
        let bar = demo_bar(Arc::new(AtomicUsize::new(0)));
        let mut state = MenuBarState::new();
        state.toggle_submenu(0, 0);
        state.toggle_submenu(1, 3);
        assert_eq!(state.open_path(), &[0, 3]);
        let mut ui = MinimalCtx::new(Some("Help"));
        render_menu_bar(&bar, &mut state, &mut ui);
        assert_eq!(state.open_path(), &[1], "submenu chain must be dropped");
    }

    #[test]
    fn submenu_expands_and_collapses() {
        let bar = demo_bar(Arc::new(AtomicUsize::new(0)));
        let mut state = MenuBarState::new();
        state.toggle_menu(0);

        // Collapsed: the submenu shows a '+' marker and no children.
        let mut ui = MinimalCtx::new(None);
        render_menu_bar(&bar, &mut state, &mut ui);
        assert!(ui.buttons.contains(&"Recent +".to_string()));
        assert!(!ui.buttons.contains(&"a.txt".to_string()));

        // Click it: expands to '-' and renders the children.
        let mut ui = MinimalCtx::new(Some("Recent +"));
        render_menu_bar(&bar, &mut state, &mut ui);
        assert_eq!(state.open_path(), &[0, 3]);

        let mut ui = MinimalCtx::new(None);
        render_menu_bar(&bar, &mut state, &mut ui);
        assert!(ui.buttons.contains(&"Recent -".to_string()));
        assert!(ui.buttons.contains(&"a.txt".to_string()));
        assert!(ui.buttons.contains(&"b.txt".to_string()));

        // Click again: collapses.
        let mut ui = MinimalCtx::new(Some("Recent -"));
        render_menu_bar(&bar, &mut state, &mut ui);
        assert_eq!(state.open_path(), &[0]);
    }

    #[test]
    fn submenu_item_action_fires() {
        let bar = MenuBar::build(|mb| {
            mb.menu("View", |m| {
                m.submenu("Theme", |sub| {
                    sub.item("Dark", None, || {});
                });
            });
        });
        let mut state = MenuBarState::new();
        state.toggle_menu(0);
        state.toggle_submenu(1, 0);
        let mut ui = MinimalCtx::new(Some("Dark"));
        assert_eq!(render_menu_bar(&bar, &mut state, &mut ui), 1);
        assert!(!state.is_open());
    }

    #[test]
    fn supported_containers_are_preferred_over_the_inline_fallback() {
        let bar = demo_bar(Arc::new(AtomicUsize::new(0)));
        let mut state = MenuBarState::new();
        state.toggle_menu(0);
        state.toggle_submenu(1, 3);
        let mut ui = ContainerCtx::new(None);
        render_menu_bar(&bar, &mut state, &mut ui);
        assert_eq!(ui.containers, vec!["menu_bar", "popup", "vertical"]);
        assert!(ui.buttons.contains(&"a.txt".to_string()));
    }

    #[test]
    fn stale_open_index_is_dropped() {
        let empty = MenuBar::build(|_| {});
        let mut state = MenuBarState::new();
        state.toggle_menu(5);
        let mut ui = MinimalCtx::new(None);
        assert_eq!(render_menu_bar(&empty, &mut state, &mut ui), 0);
        assert!(!state.is_open());
    }
}
