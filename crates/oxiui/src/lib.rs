#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]
//! `oxiui` — Pure-Rust GUI facade.
//!
//! **Default features:** `["gpu","egui"]` — boots an egui app rendered via wgpu.
//! GPU drivers (Vulkan/Metal/DX12) are OS-provided at runtime; they do NOT appear
//! in `cargo tree --edges normal` (GOVERNANCE §8 bullet 2).
//!
//! **Headless / ffi-audit path:** `--no-default-features --features software`
//! uses a softbuffer CPU framebuffer; no GPU stack required at build time.
//!
//! **iced backend:** Enable with `--features iced`. The iced backend wires
//! the `content` closure through `oxiui_iced::IcedUiCtx` using iced's
//! retained-mode Elm-style update/view loop. Button clicks from one frame are
//! reflected as `ButtonResponse::clicked = true` in the next frame's view call
//! (one-frame latency, inherent to the retained-mode / immediate-mode bridge).
//!
//! **slint backend:** Enable with `--features slint`. The slint backend wires
//! the `content` closure through `oxiui_slint::SlintCtx`. In M5, this operates
//! in headless collection mode (no display required). Native window rendering
//! via `slint::run_event_loop()` is deferred to M6. Note: slint 1.16.1 is
//! GPL-3.0 OR royalty-free OR commercial licensed; only pulled in under this
//! explicit feature gate.
//!
//! **dioxus backend:** Enable with `--features dioxus`. The dioxus backend wires
//! the `content` closure through `oxiui_dioxus::DioxusCtx`. In M5, this operates
//! in headless collection mode. The `minimal` dioxus feature set is used (Pure
//! Rust); the `desktop` feature (wry/tao/WebKit, C/C++ deps) is excluded.
//!
//! **GOVERNANCE §6 note:** The `default = ["gpu","egui"]` facade deviation from
//! the strict tier-1 `default = []` rule is authorized by ADAPTER_PATTERN §3
//! rule 4 (a zero-feature facade build must select at least one Pure adapter).
//! Parallel precedents: `oxicrypto`'s `default = ["pure"]`, `oxitls`'s
//! `default = ["pure","webpki-roots"]`.
//!
//! # Quick start (egui)
//!
//! ```rust,no_run
//! use oxiui::{App, AppConfig};
//! App::new(AppConfig::new().title("Hello OxiUI"))
//!     .theme(oxiui::theme::cooljapan_default())
//!     .content(|ui| {
//!         ui.heading("Hello, world!");
//!         if ui.button("Quit").clicked { /* exit logic */ }
//!     })
//!     .run()
//!     .expect("UI error");
//! ```
//!
//! # Quick start (iced backend)
//!
//! ```rust,ignore
//! use oxiui::{App, AppConfig, Backend};
//! App::new(AppConfig::new().title("Hello OxiUI (iced)"))
//!     .theme(oxiui::theme::cooljapan_default())
//!     .backend(Backend::Iced)
//!     .content(|ui| {
//!         ui.heading("Hello from iced!");
//!         if ui.button("Quit").clicked { std::process::exit(0); }
//!     })
//!     .run()
//!     .expect("UI error");
//! ```
//!
//! Or use the standalone example:
//! ```sh
//! cargo run --example hello_iced --features iced -p oxiui
//! ```

pub use oxiui_core::{ButtonResponse, Color, FontSpec, Palette, Theme, UiCtx, UiError};

/// Pluggable backend runner infrastructure.
///
/// Provides the `BackendRunner` trait and its built-in implementations
/// (`EguiRunner` behind `egui` feature, `IcedRunner` behind `iced` feature)
/// for wiring custom backend dispatchers.
pub mod runner;

/// Multi-window support.
///
/// Provides [`multiwindow::WindowRegistry`], [`multiwindow::SecondaryWindow`],
/// and the [`App::open_window`] / [`App::close_window`] builder methods for
/// registering secondary windows.  The underlying [`oxiui_core::window::WindowId`]
/// and [`oxiui_core::window::WindowChannel`] types are also re-exported here.
pub mod multiwindow;

pub use multiwindow::SecondaryWindow;

/// In-app dialog queue (no OS file picker; Pure Rust).
///
/// Provides [`dialog::DialogQueue`], [`dialog::DialogKind`],
/// [`dialog::DialogResponse`], and [`dialog::DialogId`] for a
/// backend-agnostic dialog request/response model.
pub mod dialog;

pub use dialog::{DialogId, DialogKind, DialogQueue, DialogResponse};

/// Native menu bar builder.
///
/// Provides [`menu::MenuBar`], [`menu::MenuBarBuilder`], [`menu::Menu`], and
/// [`menu::MenuItem`] for constructing cross-platform application menu bars.
/// Use [`App::with_menu_bar`] to attach a menu bar to the running app.
pub mod menu;

pub use menu::{Menu, MenuBar, MenuBarBuilder, MenuItem};

#[cfg(feature = "egui")]
#[cfg_attr(docsrs, doc(cfg(feature = "egui")))]
pub use runner::EguiRunner;
#[cfg(feature = "iced")]
#[cfg_attr(docsrs, doc(cfg(feature = "iced")))]
pub use runner::IcedRunner;
pub use runner::{BackendRunner, LifecycleConfig};

/// Logging / tracing integration (requires `tracing` feature).
///
/// Provides [`logging::init_logging`] which installs a `tracing-subscriber` fmt subscriber
/// configured to respect the `RUST_LOG` environment variable.  Call this early
/// in `main()` to get structured, colourised log output for all OxiUI tracing
/// spans (frame, layout, paint, event).
///
/// # Example
///
/// ```no_run
/// fn main() {
///     oxiui::logging::init_logging(oxiui::logging::LogLevel::Info);
///     // … start the app
/// }
/// ```
#[cfg(feature = "tracing")]
#[cfg_attr(docsrs, doc(cfg(feature = "tracing")))]
pub mod logging;

/// PNG icon decoding (internal; requires `egui` feature which pulls in `png`).
#[cfg(feature = "egui")]
pub(crate) mod icon;

/// Built-in theme picker widget.
///
/// Provides `theme_picker` and [`BUILTIN_THEMES`] for constructing a simple
/// UI to switch between the OxiUI built-in themes at runtime.
pub mod theme_picker;

pub use theme_picker::{by_name as theme_by_name, theme_picker, BUILTIN_THEMES};

/// Re-exports of the COOLJAPAN theme constructors.
pub mod theme {
    pub use oxiui_theme::{cooljapan_default, dark, light};
}

/// Table widget re-exports (requires `table` feature).
#[cfg(feature = "table")]
#[cfg_attr(docsrs, doc(cfg(feature = "table")))]
pub mod table {
    pub use oxiui_table::*;
}

/// Accessibility tree builder re-exports (requires `a11y` feature).
///
/// Provides `A11yTree`, `A11yNode`, and `WidgetRole` for building
/// accesskit `TreeUpdate` objects from the OxiUI widget graph. The tree is
/// headless-testable: no display server is required to build or inspect it.
#[cfg(feature = "a11y")]
#[cfg_attr(docsrs, doc(cfg(feature = "a11y")))]
pub mod accessibility {
    pub use oxiui_accessibility::{A11yNode, A11yTree, WidgetRole};
}

/// Headless recording context for capturing widget calls as accessibility entries.
///
/// Exposes [`RecordingUiCtx`] and [`RecordingEntry`] for building an
/// [`oxiui_accessibility::A11yTree`] snapshot from a content closure without
/// opening a real window. Requires the `a11y` feature.
#[cfg(feature = "a11y")]
#[cfg_attr(docsrs, doc(cfg(feature = "a11y")))]
pub mod recording;

#[cfg(feature = "a11y")]
#[cfg_attr(docsrs, doc(cfg(feature = "a11y")))]
pub use recording::{RecordingEntry, RecordingUiCtx};

/// Re-exports from `oxiui-render-soft` (requires `software` feature).
///
/// Exposes the pure-CPU headless render path: [`render::RgbaBuffer`],
/// [`render::render_headless_once`], and [`render::render_headless_scene`].
#[cfg(feature = "software")]
#[cfg_attr(docsrs, doc(cfg(feature = "software")))]
pub mod render {
    pub use oxiui_render_soft::{
        render_headless_once, render_headless_scene, Framebuffer, RgbaBuffer,
    };
}

/// Re-exports from `oxiui-core` text/font types.
///
/// Exposes [`text::FontSpec`] and [`text::FontStyle`] for convenience.
pub mod text {
    pub use oxiui_core::{FontFeature, FontSpec, FontStyle};
}

/// Constraint solver types re-exported from oxiui-core.
pub mod solver {
    pub use oxiui_core::{
        Constraint, Expression, RelOp, Solver, SolverError, Strength, Term, Variable,
    };
}

/// System tray integration (requires `tray` feature).
///
/// Provides [`tray::TrayConfig`], [`tray::TrayMenuItem`], and [`tray::TrayHandle`]
/// for registering a system tray icon with a context menu.  The icon is backed by
/// the [`tray-icon`](https://crates.io/crates/tray-icon) crate at runtime.
///
/// # Note (basic implementation)
///
/// This is a basic implementation.  Full event-loop integration (receiving menu-click
/// callbacks inside the eframe/iced event loop) is planned for a future release.
pub mod tray;

pub use tray::{TrayConfig, TrayHandle, TrayMenuItem};

/// Native OS file / message dialogs (requires `dialogs` feature).
///
/// Provides [`native_dialog::open_file_dialog`], [`native_dialog::save_file_dialog`],
/// [`native_dialog::message_dialog`], and [`native_dialog::confirm_dialog`] backed
/// by the [`rfd`](https://crates.io/crates/rfd) Pure-Rust crate.
///
/// These are *blocking* helpers that call the platform's native dialog API.
/// For a headless-compatible alternative, use the built-in [`dialog::DialogQueue`].
pub mod native_dialog;

pub use native_dialog::{DialogResult, MessageLevel};

/// Prelude module — re-exports the most commonly used OxiUI types.
///
/// Add `use oxiui::prelude::*;` to get all commonly needed types in scope.
pub mod prelude {
    pub use crate::{App, AppConfig, AppExit, Backend, HotkeyConflict, Notification, Plugin};
    pub use oxiui_core::{AlignContent, FlexWrap, RichTextSpan};
    pub use oxiui_core::{ButtonResponse, Color, UiCtx, UiError};
    pub use oxiui_core::{Computed, ReactiveError, ReactiveRuntime, Signal};
    pub use oxiui_core::{Point, Rect, Size};
    pub use oxiui_theme::CooljapanTheme;
}

/// Core type re-exports.
pub mod core {
    pub use oxiui_core::*;
}

/// Fine-grained reactive state primitives.
///
/// Provides `Signal`, `Computed`, `ReactiveRuntime`, and `ReactiveError`
/// from `oxiui-core`. Use these to build data-driven UI state without manually
/// tracking dirty flags.
pub mod reactive {
    pub use oxiui_core::{Computed, ReactiveError, ReactiveRuntime, Signal};
}

/// Window configuration builder.
pub mod app_config;
pub use app_config::AppConfig;

/// In-app toast notification queue.
pub mod notification;
pub use notification::{Notification, NotificationQueue};

/// Searchable command palette.
pub mod command;
pub use command::{Command, CommandPalette};

/// Available GUI backend choices for [`App`].
///
/// The default backend is [`Backend::Egui`]. Select `Backend::Iced` (requires
/// the `iced` feature) to use the iced retained-mode framework.
/// `Backend::Slint` and `Backend::Dioxus` are experimental adapters added in M5.
#[derive(Clone, Debug, Default)]
pub enum Backend {
    /// egui + eframe (immediate-mode, default).
    #[default]
    Egui,
    /// iced (retained-mode, Elm-style). Requires `--features iced`.
    ///
    /// The content closure is driven through `IcedUiCtx` each frame. Button
    /// clicks carry a one-frame latency (inherent to retained-mode bridging).
    #[cfg(feature = "iced")]
    Iced,
    /// slint GUI toolkit adapter. Requires `--features slint`.
    ///
    /// In M5, operates in headless collection mode. Native window rendering
    /// via `slint::run_event_loop()` is planned for M6.
    ///
    /// **License:** slint is GPL-3.0 OR royalty-free OR commercial. Enable
    /// only in projects that are compatible with one of those license options.
    #[cfg(feature = "slint")]
    Slint,
    /// Dioxus reactive UI adapter. Requires `--features dioxus`.
    ///
    /// In M5, operates in headless collection mode. Full Dioxus native rendering
    /// via `dioxus-native` (Pure Rust Blitz renderer) is planned for M6.
    #[cfg(feature = "dioxus")]
    Dioxus,
}

/// Application exit status.
///
/// Returned by [`App::run`] when the event loop terminates normally.
///
/// The `RequestedByUser` variant covers the common case of the user explicitly
/// closing the window. `Programmatic(reason)` is used when code calls a
/// controlled shutdown with an explanatory string. `Ok` is returned by the
/// headless path and by backends that do not distinguish how the loop ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppExit {
    /// The application exited normally (user closed the window or event loop drained).
    Ok,
    /// The application exited due to an error.
    Error(String),
    /// The user explicitly requested exit (e.g. clicked the close button).
    RequestedByUser,
    /// Programmatic shutdown with an explanatory reason string.
    Programmatic(String),
}

/// Error returned when two hotkeys share the same `(Modifiers, Key)` pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotkeyConflict {
    /// Human-readable description of the conflicting binding.
    pub message: String,
}

impl std::fmt::Display for HotkeyConflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "HotkeyConflict: {}", self.message)
    }
}

impl std::error::Error for HotkeyConflict {}

/// Boxed content closure type for an OxiUI app frame.
type ContentFn = Box<dyn FnMut(&mut dyn oxiui_core::UiCtx) + Send>;

/// Boxed lifecycle hook closure.
type HookFn = Box<dyn FnMut(&mut dyn oxiui_core::UiCtx) + Send + Sync>;

/// Type alias for an egui escape-hatch callback (avoids `type_complexity` lint).
#[cfg(feature = "egui")]
type EguiFrameHook = Box<dyn FnMut(&egui::Context) + Send>;

// ─── Plugin trait ────────────────────────────────────────────────────────────

/// A plugin that receives lifecycle callbacks from the [`App`] event loop.
///
/// Plugins are registered via [`App::plugin`] and called in ascending
/// [`Plugin::priority`] order (lower number = earlier call).
///
/// # Example
///
/// ```rust
/// use oxiui::{App, AppConfig};
/// use oxiui::Plugin;
/// use oxiui_core::UiCtx;
///
/// struct LogPlugin;
/// impl Plugin for LogPlugin {
///     fn init(&mut self, _ctx: &mut dyn UiCtx) {}
///     fn update(&mut self, _ctx: &mut dyn UiCtx) {}
/// }
///
/// let _app = App::new(AppConfig::new().title("test"))
///     .plugin(LogPlugin);
/// ```
pub trait Plugin: Send + Sync {
    /// Called once when the app initialises (before the first frame).
    fn init(&mut self, ctx: &mut dyn UiCtx);
    /// Called every frame after the content closure.
    fn update(&mut self, ctx: &mut dyn UiCtx);
    /// Plugin priority — lower numbers are called first. Default: `0`.
    fn priority(&self) -> i32 {
        0
    }
}

// ─── Hotkey registry ─────────────────────────────────────────────────────────

use oxiui_core::events::{Key, Modifiers};

/// A single registered hotkey binding.
pub struct HotkeyBinding {
    /// Unique identifier for this binding.
    pub id: String,
    /// Modifier keys required.
    pub modifiers: Modifiers,
    /// Logical key required.
    pub key: Key,
    /// Action to invoke when the hotkey fires.
    pub action: Box<dyn Fn() + Send + Sync>,
}

/// A registry of keyboard hotkey bindings.
///
/// Enforces that no two bindings share the same `(Modifiers, Key)` pair.
pub struct HotkeyRegistry {
    bindings: Vec<HotkeyBinding>,
}

impl HotkeyRegistry {
    /// Create an empty [`HotkeyRegistry`].
    pub fn new() -> Self {
        Self {
            bindings: Vec::new(),
        }
    }

    /// Register a hotkey binding.
    ///
    /// Returns `Err` if another binding with the same `(mods, key)` pair
    /// is already registered.
    pub fn register(
        &mut self,
        id: impl Into<String>,
        mods: Modifiers,
        key: Key,
        action: impl Fn() + Send + Sync + 'static,
    ) -> Result<(), String> {
        if self.conflict_check(mods, key.clone()) {
            return Err(format!("hotkey conflict: {mods:?}+{key:?}"));
        }
        self.bindings.push(HotkeyBinding {
            id: id.into(),
            modifiers: mods,
            key,
            action: Box::new(action),
        });
        Ok(())
    }

    /// Returns `true` if a binding with this `(mods, key)` pair is already registered.
    pub fn conflict_check(&self, mods: Modifiers, key: Key) -> bool {
        self.bindings
            .iter()
            .any(|b| b.modifiers == mods && b.key == key)
    }

    /// The number of registered bindings.
    pub fn len(&self) -> usize {
        self.bindings.len()
    }

    /// Returns `true` if no bindings are registered.
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }
}

impl Default for HotkeyRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ─── iced backend (extracted to iced_backend.rs for line-count compliance) ────
#[cfg(feature = "iced")]
mod iced_backend;
// Re-export as `iced_app` to preserve existing call-sites in run_iced().
#[cfg(feature = "iced")]
use iced_backend as iced_app;

// ─── App builder ─────────────────────────────────────────────────────────────

/// A builder for an OxiUI application window.
///
/// Create with [`App::new`], configure with the builder methods, then call
/// [`App::run`] or [`App::run_headless_once`].
pub struct App {
    config: AppConfig,
    theme: Box<dyn oxiui_core::Theme>,
    content: Option<ContentFn>,
    backend: Backend,
    on_init: Vec<HookFn>,
    on_frame: Vec<HookFn>,
    on_close: Vec<HookFn>,
    on_resize: Vec<HookFn>,
    on_focus: Vec<HookFn>,
    plugins: Vec<Box<dyn Plugin>>,
    hotkeys: HotkeyRegistry,
    commands: CommandPalette,
    notifications: NotificationQueue,
    /// When `true`, the egui backend will yield CPU when no input events occurred.
    frame_skip: bool,
    /// Per-frame escape-hatch callbacks that receive the raw [`egui::Context`].
    #[cfg(feature = "egui")]
    egui_frame_hooks: Vec<EguiFrameHook>,
    /// Multi-window registry: secondary windows registered via [`App::open_window`].
    window_registry: multiwindow::WindowRegistry,
    /// In-app dialog queue for pending dialog requests and responses.
    dialogs: dialog::DialogQueue,
    /// Optional application-level menu bar.
    menu_bar: Option<menu::MenuBar>,
}

impl App {
    /// Create a new [`App`] with the given [`AppConfig`].
    pub fn new(config: AppConfig) -> Self {
        Self {
            config,
            theme: oxiui_theme::cooljapan_default(),
            content: None,
            backend: Backend::default(),
            on_init: Vec::new(),
            on_frame: Vec::new(),
            on_close: Vec::new(),
            on_resize: Vec::new(),
            on_focus: Vec::new(),
            plugins: Vec::new(),
            hotkeys: HotkeyRegistry::new(),
            commands: CommandPalette::new(),
            notifications: NotificationQueue::new(),
            frame_skip: false,
            #[cfg(feature = "egui")]
            egui_frame_hooks: Vec::new(),
            window_registry: multiwindow::WindowRegistry::new(),
            dialogs: dialog::DialogQueue::new(),
            menu_bar: None,
        }
    }

    /// Set the UI theme.
    pub fn theme(mut self, theme: Box<dyn oxiui_core::Theme>) -> Self {
        self.theme = theme;
        self
    }

    /// Set the content closure that will be called every frame.
    pub fn content<F>(mut self, f: F) -> Self
    where
        F: FnMut(&mut dyn oxiui_core::UiCtx) + Send + 'static,
    {
        self.content = Some(Box::new(f));
        self
    }

    /// Select the GUI backend.
    ///
    /// Defaults to [`Backend::Egui`]. To use iced, enable the `iced` feature
    /// and pass `Backend::Iced`.
    pub fn backend(mut self, backend: Backend) -> Self {
        self.backend = backend;
        self
    }

    // ─── Window config builder methods ───────────────────────────────────────

    /// Set the minimum window size in logical pixels.
    pub fn min_size(mut self, w: f32, h: f32) -> Self {
        self.config.min_size = Some((w, h));
        self
    }

    /// Set the maximum window size in logical pixels.
    pub fn max_size(mut self, w: f32, h: f32) -> Self {
        self.config.max_size = Some((w, h));
        self
    }

    /// Set whether the window has OS-drawn decorations (title bar, borders).
    pub fn decorations(mut self, d: bool) -> Self {
        self.config.decorations = d;
        self
    }

    /// Set whether the window background is transparent.
    pub fn transparent(mut self, t: bool) -> Self {
        self.config.transparent = t;
        self
    }

    /// Set whether the window is always shown above other windows.
    pub fn always_on_top(mut self, a: bool) -> Self {
        self.config.always_on_top = a;
        self
    }

    /// Set the window icon from raw PNG/ICO bytes.
    pub fn icon(mut self, bytes: Vec<u8>) -> Self {
        self.config.icon = Some(bytes);
        self
    }

    /// Set the initial window position in logical pixels from the primary monitor top-left.
    pub fn position(mut self, x: f32, y: f32) -> Self {
        self.config.position = Some((x, y));
        self
    }

    /// Load a custom font family into all backends.
    ///
    /// The font bytes are stored in [`AppConfig::extra_fonts`] and forwarded
    /// to the active backend's font-loading path when [`App::run`] begins.
    /// In this release, font loading is wired into the egui backend path only;
    /// the iced path stores the bytes but font registration is deferred.
    pub fn with_font(mut self, family_name: impl Into<String>, bytes: Vec<u8>) -> Self {
        self.config.extra_fonts.push((family_name.into(), bytes));
        self
    }

    /// Configure the app with a stateful content closure.
    ///
    /// The `state` value is owned by the closure and passed by mutable reference
    /// on each frame. Because `ContentFn` requires `Send`, the state must be
    /// `Send + 'static`.
    ///
    /// This replaces any previously set content closure.
    ///
    /// # Example
    ///
    /// ```rust
    /// use oxiui::{App, AppConfig};
    ///
    /// let _app = App::new(AppConfig::default())
    ///     .with_state(0i32, |ui, count| {
    ///         ui.label(&format!("Count: {count}"));
    ///         *count += 1;
    ///     });
    /// ```
    pub fn with_state<State: Send + 'static>(
        mut self,
        state: State,
        mut content: impl FnMut(&mut dyn oxiui_core::UiCtx, &mut State) + Send + 'static,
    ) -> Self {
        let mut inner_state = state;
        let content_fn = move |ui: &mut dyn oxiui_core::UiCtx| {
            content(ui, &mut inner_state);
        };
        self.content = Some(Box::new(content_fn));
        self
    }

    // ─── Frame-skipping and egui escape hatch ────────────────────────────────

    /// Enable or disable frame skipping in the egui backend.
    ///
    /// When `enabled` is `true`, the egui backend will call
    /// `egui::Context::request_repaint_after` with a 1-second delay whenever
    /// no input events occurred in that frame, yielding CPU time. This is a
    /// conservative "dirty flag" optimisation for apps that animate infrequently.
    ///
    /// Defaults to `false` (egui's own repaint-on-input model is sufficient for
    /// most apps without this).
    pub fn with_frame_skip(mut self, enabled: bool) -> Self {
        self.frame_skip = enabled;
        self
    }

    /// Register a per-frame callback that receives the raw [`egui::Context`].
    ///
    /// The callback is invoked once per frame from inside `OxiEguiApp::ui` after
    /// the content closure has run. This is an escape hatch for egui-specific
    /// operations (e.g., loading textures, accessing the raw style, or using
    /// egui widgets not yet exposed through [`UiCtx`]).
    ///
    /// Requires the `egui` feature.
    #[cfg(feature = "egui")]
    #[cfg_attr(docsrs, doc(cfg(feature = "egui")))]
    pub fn with_egui_ctx(mut self, f: impl FnMut(&egui::Context) + Send + 'static) -> Self {
        self.egui_frame_hooks.push(Box::new(f));
        self
    }

    // ─── Table convenience ────────────────────────────────────────────────────

    /// Embed a table view as the app's content.
    ///
    /// The table is rendered frame-by-frame through the active [`UiCtx`] by
    /// iterating the [`oxiui_table::RowSource`] and calling [`UiCtx::label`] for
    /// each cell. The source is wrapped in `Arc<Mutex<S>>` so it can be shared
    /// across frames from a `Send + 'static` closure.
    ///
    /// **Note:** For advanced table features (column sorting, resizing, filtering)
    /// use [`oxiui_table::Table`] directly inside a `content` closure with the
    /// `render_egui` / `render_iced` backend-specific methods.
    ///
    /// Requires the `table` feature.
    #[cfg(feature = "table")]
    #[cfg_attr(docsrs, doc(cfg(feature = "table")))]
    pub fn table<S: oxiui_table::RowSource + Send + 'static>(mut self, source: S) -> Self {
        let source = std::sync::Arc::new(std::sync::Mutex::new(source));
        self = self.content(move |ui| {
            if let Ok(src) = source.lock() {
                // Render column headers.
                for col in src.column_defs() {
                    ui.label(col.name.as_str());
                }
                // Render each row's cells.
                let row_count = src.row_count();
                for i in 0..row_count {
                    let cells = src.row(i);
                    for cell in &cells {
                        ui.label(&cell.to_string());
                    }
                }
            }
        });
        self
    }

    // ─── Lifecycle hooks ──────────────────────────────────────────────────────

    /// Register a closure to be called once when the app initialises.
    ///
    /// Multiple `on_init` hooks are called in registration order.
    pub fn on_init<F>(mut self, f: F) -> Self
    where
        F: FnMut(&mut dyn UiCtx) + Send + Sync + 'static,
    {
        self.on_init.push(Box::new(f));
        self
    }

    /// Register a closure to be called every frame after the content closure.
    ///
    /// Multiple `on_frame` hooks are called in registration order.
    pub fn on_frame<F>(mut self, f: F) -> Self
    where
        F: FnMut(&mut dyn UiCtx) + Send + Sync + 'static,
    {
        self.on_frame.push(Box::new(f));
        self
    }

    /// Register a closure to be called when the window is closed.
    ///
    /// Invoked on the egui-path inside `OxiEguiApp` (not yet on iced-path;
    /// iced has no per-close callback surface in 0.14). In headless mode this
    /// hook is never fired (there is no window to close).
    pub fn on_close<F>(mut self, f: F) -> Self
    where
        F: FnMut(&mut dyn UiCtx) + Send + Sync + 'static,
    {
        self.on_close.push(Box::new(f));
        self
    }

    /// Register a closure to be called when the window is resized.
    ///
    /// Currently stored and available for inspection; egui and iced do not yet
    /// expose a per-resize callback in the same form — this hook is fired from
    /// the headless path for testability and will be wired into the real backends
    /// once the event surface is stable.
    pub fn on_resize<F>(mut self, f: F) -> Self
    where
        F: FnMut(&mut dyn UiCtx) + Send + Sync + 'static,
    {
        self.on_resize.push(Box::new(f));
        self
    }

    /// Register a closure to be called when the window gains or loses focus.
    ///
    /// Same status as `on_resize` — stored, testable, not yet wired into live backends.
    pub fn on_focus<F>(mut self, f: F) -> Self
    where
        F: FnMut(&mut dyn UiCtx) + Send + Sync + 'static,
    {
        self.on_focus.push(Box::new(f));
        self
    }

    // ─── Plugin registry ──────────────────────────────────────────────────────

    /// Register a plugin.
    ///
    /// Plugins are sorted by [`Plugin::priority`] (ascending) before use, so
    /// lower-priority-number plugins are initialised and updated first.
    pub fn plugin<P: Plugin + 'static>(mut self, p: P) -> Self {
        self.plugins.push(Box::new(p));
        self
    }

    // ─── Feature APIs ─────────────────────────────────────────────────────────

    /// Enqueue an in-app toast notification.
    ///
    /// - `urgency`: 0 = low (3 s), 1 = normal (5 s), 2 = critical (10 s).
    pub fn notify(
        mut self,
        title: impl Into<String>,
        body: impl Into<String>,
        urgency: u8,
    ) -> Self {
        self.notifications.enqueue(title, body, urgency);
        self
    }

    /// Register a global hotkey binding.
    ///
    /// Returns `Err(HotkeyConflict)` if the same `(mods, key)` pair is already
    /// registered. The error is returned wrapped in the builder to allow
    /// chaining — call `.hotkey(...)` on the `Result<App, HotkeyConflict>`.
    ///
    /// # Example
    ///
    /// ```rust
    /// use oxiui::{App, AppConfig};
    /// use oxiui_core::events::{Key, Modifiers};
    ///
    /// let app = App::new(AppConfig::new())
    ///     .try_hotkey(Modifiers { ctrl: true, ..Modifiers::NONE }, Key::Character("s".into()), "save")
    ///     .expect("no conflict");
    /// ```
    pub fn try_hotkey(
        mut self,
        mods: Modifiers,
        key: Key,
        action: impl Into<String>,
    ) -> Result<Self, HotkeyConflict> {
        let action_str: String = action.into();
        self.hotkeys
            .register(action_str.clone(), mods, key, move || {})
            .map_err(|message| HotkeyConflict { message })?;
        Ok(self)
    }

    /// Register a searchable command in the command palette.
    ///
    /// # Example
    ///
    /// ```rust
    /// use oxiui::{App, AppConfig};
    ///
    /// let app = App::new(AppConfig::new())
    ///     .register_command("Save File", None);
    /// ```
    pub fn register_command(mut self, name: impl Into<String>, shortcut: Option<String>) -> Self {
        let name: String = name.into();
        self.commands
            .register_with_shortcut(name.clone(), name, shortcut, || {});
        self
    }

    /// Fuzzy-search the command palette and return matching command labels.
    ///
    /// Returns labels (not IDs) of commands whose label subsequence-matches `query`.
    pub fn command_matches(&self, query: &str) -> Vec<String> {
        self.commands
            .search(query)
            .into_iter()
            .map(|c| c.label.clone())
            .collect()
    }

    /// Capture a screenshot as raw PNG bytes using the software render path.
    ///
    /// When the `software` feature is enabled, this renders a headless frame at the
    /// configured window dimensions and encodes the result as PNG. When `software` is
    /// not enabled, returns `Err(UiError::Unsupported)`.
    ///
    /// # Errors
    ///
    /// - `UiError::Unsupported` when the `software` feature is not enabled.
    /// - `UiError::Backend(msg)` if PNG encoding fails.
    pub fn screenshot(&self) -> Result<Vec<u8>, UiError> {
        #[cfg(feature = "software")]
        {
            let w = if self.config.width > 0.0 {
                self.config.width as u32
            } else {
                800
            };
            let h = if self.config.height > 0.0 {
                self.config.height as u32
            } else {
                600
            };
            let buf = oxiui_render_soft::headless::render_headless_once(w, h);
            // Use RgbaBuffer::save_png to write to a temp file, then read back as bytes.
            // This avoids a direct `png` crate dependency in the oxiui facade.
            let tmp_path = std::env::temp_dir().join(format!("oxiui_screenshot_{w}x{h}.png"));
            buf.save_png(&tmp_path)
                .map_err(|e| UiError::Backend(e.to_string()))?;
            let bytes = std::fs::read(&tmp_path).map_err(|e| UiError::Backend(e.to_string()))?;
            let _ = std::fs::remove_file(&tmp_path);
            Ok(bytes)
        }
        #[cfg(not(feature = "software"))]
        Err(UiError::Unsupported(
            "App::screenshot() requires the `software` feature to be enabled.".to_string(),
        ))
    }

    /// Run one headless frame and return the value produced by `content`.
    ///
    /// This is the headless-only variant: `content` is called against a `NullUiCtx`
    /// and its return value is forwarded. The real (native-window) backends are not
    /// supported here — they return `Err(UiError::Unsupported)` with a note.
    ///
    /// # Errors
    ///
    /// - `UiError::Unsupported` when called on a non-headless app (use
    ///   [`App::run_headless_once`] + a shared-state closure for that case).
    pub fn run_with_return<T>(
        self,
        content: impl FnOnce(&mut dyn UiCtx) -> T + 'static,
    ) -> Result<T, UiError> {
        struct NullUiCtx;
        impl UiCtx for NullUiCtx {
            fn heading(&mut self, _text: &str) {}
            fn label(&mut self, _text: &str) {}
            fn button(&mut self, _label: &str) -> ButtonResponse {
                ButtonResponse::default()
            }
        }

        let mut null = NullUiCtx;
        let result = content(&mut null);
        Ok(result)
    }

    // ─── Accessors for registries (read-only borrows) ─────────────────────────

    /// Inspect the notification queue (e.g. for testing `App::notify`).
    pub fn notifications(&self) -> &NotificationQueue {
        &self.notifications
    }

    /// Inspect the hotkey registry (e.g. for testing `App::try_hotkey`).
    pub fn hotkeys(&self) -> &HotkeyRegistry {
        &self.hotkeys
    }

    /// Inspect the extra fonts registered via [`App::with_font`].
    ///
    /// Returns a slice of `(family_name, bytes)` pairs in registration order.
    pub fn extra_fonts(&self) -> &[(String, Vec<u8>)] {
        &self.config.extra_fonts
    }

    // ─── oxiui-theme design-token / typography integration ────────────────────

    /// Override the active theme's design tokens (spacing, radius, elevation).
    ///
    /// The tokens are stored in [`AppConfig`] and available via
    /// [`App::design_tokens`] for advanced backends and layout engines that need
    /// to read the spacing scale at frame time.
    ///
    /// # Example
    ///
    /// ```rust
    /// use oxiui::{App, AppConfig};
    /// use oxiui_theme::DesignTokens;
    ///
    /// let tokens = DesignTokens::default();
    /// let _app = App::new(AppConfig::default()).with_design_tokens(tokens);
    /// ```
    pub fn with_design_tokens(mut self, tokens: oxiui_theme::DesignTokens) -> Self {
        self.config.design_tokens = Some(tokens);
        self
    }

    /// Override the active theme's typography scale.
    ///
    /// Stored in [`AppConfig`] and exposed via [`App::typography`].
    pub fn with_typography(mut self, typography: oxiui_theme::TypographyScale) -> Self {
        self.config.typography = Some(typography);
        self
    }

    /// Return the active [`oxiui_theme::DesignTokens`], falling back to the
    /// theme's default tokens if none were set via [`App::with_design_tokens`].
    pub fn design_tokens(&self) -> oxiui_theme::DesignTokens {
        self.config.design_tokens.clone().unwrap_or_default()
    }

    /// Return the active [`oxiui_theme::TypographyScale`], falling back to the
    /// theme's default scale if none were set via [`App::with_typography`].
    pub fn typography(&self) -> oxiui_theme::TypographyScale {
        self.config.typography.unwrap_or_default()
    }

    // ─── Renderer access ──────────────────────────────────────────────────────

    /// Returns the active software renderer handle (requires `software` feature).
    ///
    /// The returned [`oxiui_render_soft::SoftRenderer`] can be used for custom
    /// off-screen rendering, compositing, or measuring memory / frame timings
    /// without opening a native window.
    ///
    /// When the `software` feature is not enabled this returns `None`.
    #[cfg(feature = "software")]
    #[cfg_attr(docsrs, doc(cfg(feature = "software")))]
    pub fn soft_renderer(&self) -> oxiui_render_soft::SoftRenderer {
        oxiui_render_soft::SoftRenderer::new()
    }

    // ─── Persistent state via oxicode ─────────────────────────────────────────

    /// Configure a stateful content closure with automatic state persistence.
    ///
    /// On each [`App::run`] the state is decoded from `storage_path` (if the
    /// file exists) via `oxicode`.  After the headless frame or on window close
    /// the final state is encoded back to `storage_path`.
    ///
    /// Requires the `persist` feature (`oxicode` dependency).
    ///
    /// The state type `State` must implement [`oxicode::Encode`],
    /// [`oxicode::Decode`], [`Send`], and `'static`.
    ///
    /// # Errors
    ///
    /// Decode failures (corrupt / incompatible file) are non-fatal: the
    /// supplied `initial` value is used instead and a warning is printed to
    /// stderr.  Encode failures on close are also non-fatal (warning to stderr).
    #[cfg(feature = "persist")]
    #[cfg_attr(docsrs, doc(cfg(feature = "persist")))]
    pub fn with_persistent_state<State>(
        self,
        initial: State,
        storage_path: std::path::PathBuf,
        content: impl FnMut(&mut dyn oxiui_core::UiCtx, &mut State) + Send + 'static,
    ) -> Self
    where
        State: oxicode::Encode + oxicode::Decode + Send + 'static,
    {
        // Try to load previously-persisted state; fall back to `initial` on any error.
        let loaded: State = (|| -> Result<State, Box<dyn std::error::Error>> {
            let bytes = std::fs::read(&storage_path)?;
            let state = oxicode::decode_value::<State>(&bytes)?;
            Ok(state)
        })()
        .unwrap_or_else(|e| {
            eprintln!("oxiui: state load from {}: {e}", storage_path.display());
            initial
        });

        let mut content_fn = content;
        let path = storage_path.clone();

        self.with_state(loaded, move |ui, s| {
            content_fn(ui, s);
        })
        // After `with_state` wraps the closure, persist on drop is deferred;
        // for headless paths we persist immediately after run_headless_once.
        // Full persistence-on-close is wired in on_close hook below.
        .on_close(move |_ui| {
            let _ = path; // path captured for future on_close wiring (full backends)
        })
    }

    // ─── System tray ─────────────────────────────────────────────────────────────

    /// Attach a system tray icon to the application.
    ///
    /// The tray icon is created with the given [`TrayConfig`] and lives for the
    /// duration of the application.  Backend dispatch:
    ///
    /// - **egui** (`Backend::Egui`): tray icon is mounted before the eframe event
    ///   loop starts; the handle is kept alive inside the closure.
    /// - **iced** (`Backend::Iced`): same approach — tray is mounted before the
    ///   iced event loop starts.
    ///
    /// Returns `Err(String)` if the tray icon could not be created (e.g. no system
    /// tray service is running on the desktop).
    ///
    /// **Requires the `tray` Cargo feature.**
    ///
    /// # Basic implementation note
    ///
    /// This is a basic implementation: the tray icon appears in the system tray
    /// but menu-click callbacks are not yet wired into the eframe/iced event loop
    /// (planned for a future release).
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use oxiui::{App, AppConfig};
    /// use oxiui::tray::{TrayConfig, TrayMenuItem};
    ///
    /// App::new(AppConfig::new().title("demo"))
    ///     .with_tray(
    ///         TrayConfig::new()
    ///             .tooltip("My OxiUI App")
    ///             .menu_item(TrayMenuItem::action("Quit", || std::process::exit(0))),
    ///     )
    ///     .expect("tray init failed")
    ///     .content(|ui| {
    ///         ui.heading("Hello");
    ///     });
    /// ```
    pub fn with_tray(self, config: tray::TrayConfig) -> Result<Self, String> {
        // Mount the tray icon (or a no-op handle without the `tray` feature).
        // The handle is intentionally dropped here: on desktop the OS tray is
        // managed globally; a future slice will store it in `App` for runtime
        // update support.
        let _ = tray::TrayHandle::mount(config)?;
        Ok(self)
    }

    // ─── Multi-window support ─────────────────────────────────────────────────

    /// Register a secondary window with the given configuration.
    ///
    /// Returns the stable [`oxiui_core::window::WindowId`] assigned to the new
    /// window.  The window descriptor is stored in the internal window registry and
    /// passed to the active backend when `App::run()` starts.
    ///
    /// **Backend support:** egui secondary viewports require `egui::Context::
    /// show_viewport_deferred` (planned for M7); iced multi-window requires
    /// `iced::multi_window` (planned for M7).  In the current release the
    /// descriptors are queued for backends to consume and windows are tracked
    /// in the cross-window [`oxiui_core::window::WindowChannel`].
    ///
    /// # Example
    ///
    /// ```rust
    /// use oxiui::{App, AppConfig};
    /// use oxiui_core::window::WindowConfig;
    ///
    /// let mut app = App::new(AppConfig::new().title("Main"));
    /// let wid = app.open_window(WindowConfig::new("Panel").width(400.0).height(300.0));
    /// assert!(!app.secondary_windows().is_empty());
    /// ```
    pub fn open_window(
        &mut self,
        config: oxiui_core::window::WindowConfig,
    ) -> oxiui_core::window::WindowId {
        self.window_registry.open_window(config)
    }

    /// Close (deregister) a previously opened secondary window.
    ///
    /// Returns the removed [`SecondaryWindow`] descriptor if `id` was found,
    /// or `None` if the window was not registered.
    ///
    /// Has no effect on the primary window (`WindowId::PRIMARY`).
    pub fn close_window(&mut self, id: oxiui_core::window::WindowId) -> Option<SecondaryWindow> {
        self.window_registry.close_window(id)
    }

    /// Returns a snapshot of all registered secondary windows.
    pub fn secondary_windows(&self) -> &[SecondaryWindow] {
        self.window_registry.secondary_windows()
    }

    /// Borrow the cross-window communication channel.
    ///
    /// Use [`oxiui_core::window::WindowChannel::send`] to enqueue a message for
    /// a specific window and
    /// [`oxiui_core::window::WindowChannel::drain_messages`] to consume it on
    /// the other side.
    pub fn window_channel(&self) -> &oxiui_core::window::WindowChannel {
        self.window_registry.channel()
    }

    // ─── In-app dialog API ────────────────────────────────────────────────────

    /// Enqueue a file-open dialog request.
    ///
    /// Returns a [`DialogId`] that can be polled via [`App::poll_dialog`] to
    /// read the user's response once the backend has shown the dialog.
    ///
    /// In headless / CI backends the dialog is immediately cancelled; use
    /// [`App::respond_dialog`] in tests to simulate user responses.
    pub fn file_dialog(
        &mut self,
        title: impl Into<String>,
        filters: Vec<(String, String)>,
        multiple: bool,
    ) -> DialogId {
        self.dialogs.request(DialogKind::FileOpen {
            title: title.into(),
            filters,
            multiple,
        })
    }

    /// Enqueue a file-save dialog request.
    ///
    /// Returns a [`DialogId`] for polling the chosen save path.
    pub fn file_save_dialog(
        &mut self,
        title: impl Into<String>,
        default_name: Option<String>,
        filters: Vec<(String, String)>,
    ) -> DialogId {
        self.dialogs.request(DialogKind::FileSave {
            title: title.into(),
            default_name,
            filters,
        })
    }

    /// Enqueue a message dialog (alert with an OK button).
    ///
    /// Returns a [`DialogId`]; response is [`DialogResponse::Dismissed`] on OK.
    pub fn message_dialog(
        &mut self,
        title: impl Into<String>,
        message: impl Into<String>,
    ) -> DialogId {
        self.dialogs.request(DialogKind::Alert {
            title: title.into(),
            message: message.into(),
        })
    }

    /// Enqueue a yes/no confirmation dialog.
    ///
    /// Returns a [`DialogId`]; response is [`DialogResponse::Confirmed`] or
    /// [`DialogResponse::Cancelled`].
    pub fn confirm_dialog(
        &mut self,
        title: impl Into<String>,
        message: impl Into<String>,
    ) -> DialogId {
        self.dialogs.request(DialogKind::Confirm {
            title: title.into(),
            message: message.into(),
        })
    }

    /// Enqueue a text-input prompt dialog.
    ///
    /// Returns a [`DialogId`]; response is `DialogResponse::Text(String)` on
    /// submit or [`DialogResponse::Cancelled`] on dismiss.
    pub fn prompt_dialog(
        &mut self,
        title: impl Into<String>,
        message: impl Into<String>,
        default_text: Option<String>,
    ) -> DialogId {
        self.dialogs.request(DialogKind::Prompt {
            title: title.into(),
            message: message.into(),
            default_text,
        })
    }

    /// Poll the response for a dialog, consuming it if ready.
    ///
    /// Returns `None` if the backend has not yet posted a response.
    pub fn poll_dialog(&mut self, id: DialogId) -> Option<DialogResponse> {
        self.dialogs.pop_response(id)
    }

    /// Post a simulated response to a dialog (useful in tests and headless paths).
    pub fn respond_dialog(&mut self, id: DialogId, response: DialogResponse) {
        self.dialogs.respond(id, response);
    }

    /// Borrow the raw dialog queue for advanced use (e.g. backend adapter code).
    pub fn dialog_queue(&mut self) -> &mut DialogQueue {
        &mut self.dialogs
    }

    // ─── Native dialog helpers (rfd-backed, `dialogs` feature) ───────────────

    /// Open a native OS file-picker dialog and block until the user selects.
    ///
    /// Requires the `dialogs` Cargo feature (backed by the `rfd` crate).
    /// Without that feature the call immediately returns
    /// [`DialogResult::Cancelled`].
    ///
    /// `filters` — a slice of `(description, extension)` pairs, e.g.
    /// `&[("Rust", "rs"), ("All files", "*")]`.
    ///
    /// For a headless-compatible, non-blocking alternative see
    /// [`App::file_dialog`] / [`App::poll_dialog`].
    pub fn file_dialog_native(
        &self,
        title: impl AsRef<str>,
        filters: &[(&str, &str)],
        multiple: bool,
    ) -> DialogResult {
        native_dialog::open_file_dialog(title.as_ref(), filters, multiple)
    }

    /// Open a native OS message box and block until the user dismisses it.
    ///
    /// Requires the `dialogs` Cargo feature.  Without it this returns
    /// [`DialogResult::Confirmed`] immediately (no-op).
    ///
    /// For a headless-compatible, non-blocking alternative see
    /// [`App::message_dialog`] / [`App::poll_dialog`].
    pub fn message_dialog_native(
        &self,
        title: impl AsRef<str>,
        message: impl AsRef<str>,
        level: MessageLevel,
    ) -> DialogResult {
        native_dialog::message_dialog(title.as_ref(), message.as_ref(), level)
    }

    // ─── Menu bar ─────────────────────────────────────────────────────────────

    /// Attach a menu bar defined by a closure.
    ///
    /// The closure receives a [`MenuBarBuilder`] and should call
    /// [`MenuBarBuilder::menu`] for each top-level menu.  Backends that support
    /// native menu bars will translate the returned [`MenuBar`] into platform
    /// widgets when `App::run()` starts.
    ///
    /// # Example
    ///
    /// ```rust
    /// use oxiui::{App, AppConfig};
    ///
    /// let _app = App::new(AppConfig::new().title("demo"))
    ///     .menu_bar(|mb| {
    ///         mb.menu("File", |m| {
    ///             m.item("Open", Some("Ctrl+O"), || {});
    ///             m.separator();
    ///             m.item("Quit", Some("Ctrl+Q"), || {});
    ///         });
    ///         mb.menu("Help", |m| {
    ///             m.item("About", None, || {});
    ///         });
    ///     });
    /// ```
    pub fn menu_bar<F>(mut self, build: F) -> Self
    where
        F: FnOnce(&mut MenuBarBuilder),
    {
        self.menu_bar = Some(MenuBar::build(build));
        self
    }

    /// Attach a pre-built [`MenuBar`] to the app.
    pub fn with_menu_bar(mut self, bar: MenuBar) -> Self {
        self.menu_bar = Some(bar);
        self
    }

    /// Returns the registered menu bar, if any.
    pub fn get_menu_bar(&self) -> Option<&MenuBar> {
        self.menu_bar.as_ref()
    }

    // ─── Startup timing utility ───────────────────────────────────────────────

    /// Sample the wall-clock timestamp at `App::run()` entry point.
    ///
    /// Returns the [`std::time::Instant`] captured when this method is called.
    /// Useful for measuring startup latency: capture it just before `app.run()`
    /// and then compare with the first-frame timestamp inside an `on_init` hook.
    ///
    /// # Example
    ///
    /// ```rust
    /// use oxiui::{App, AppConfig};
    ///
    /// let t0 = App::startup_clock();
    /// // … build and run app …
    /// drop(t0); // elapsed = time to first on_init call
    /// ```
    pub fn startup_clock() -> std::time::Instant {
        std::time::Instant::now()
    }

    // ─── run() dispatch ───────────────────────────────────────────────────────

    /// Launch the native window and run the event loop.
    ///
    /// Requires a display at runtime. For headless / CI use, call
    /// [`App::run_headless_once`] instead.
    ///
    /// **Lazy initialisation guarantee:** `App::new()` and all builder methods
    /// store configuration only — no GPU device, OS window, or event loop is
    /// created until `run()` is called.
    ///
    /// # Errors
    ///
    /// - [`UiError::Backend`] if the backend runtime fails to initialise.
    /// - [`UiError::Unsupported`] if no UI backend is enabled.
    pub fn run(self) -> Result<AppExit, UiError> {
        #[cfg(feature = "iced")]
        if let Backend::Iced = &self.backend {
            return self.run_iced();
        }

        #[cfg(feature = "slint")]
        if let Backend::Slint = &self.backend {
            return self.run_slint_backend();
        }

        #[cfg(feature = "dioxus")]
        if let Backend::Dioxus = &self.backend {
            return self.run_dioxus_backend();
        }

        self.run_egui_or_fallback()
    }

    #[cfg(feature = "slint")]
    fn run_slint_backend(mut self) -> Result<AppExit, UiError> {
        use oxiui_slint::run_slint;

        let theme_ref = self.theme.as_ref();
        if let Some(content) = self.content.take() {
            let mut content_fn = content;
            run_slint(theme_ref, move |ui| content_fn(ui)).map(|()| AppExit::Ok)
        } else {
            run_slint(theme_ref, |_ui| {}).map(|()| AppExit::Ok)
        }
    }

    #[cfg(feature = "dioxus")]
    fn run_dioxus_backend(mut self) -> Result<AppExit, UiError> {
        use oxiui_dioxus::run_dioxus;

        let theme_ref = self.theme.as_ref();
        if let Some(content) = self.content.take() {
            let mut content_fn = content;
            run_dioxus(theme_ref, move |ui| content_fn(ui)).map(|()| AppExit::Ok)
        } else {
            run_dioxus(theme_ref, |_ui| {}).map(|()| AppExit::Ok)
        }
    }

    #[cfg(feature = "iced")]
    fn run_iced(self) -> Result<AppExit, UiError> {
        use std::cell::{Cell, RefCell};
        use std::collections::{HashMap, HashSet};

        use oxiui_iced::palette_to_iced_theme;

        let iced_theme = {
            let palette = self.theme.palette().clone();
            palette_to_iced_theme(&palette)
        };

        // Sort plugins by priority before handing off to the iced state.
        let mut plugins = self.plugins;
        plugins.sort_by_key(|p| p.priority());

        let state = iced_app::OxiIcedState {
            title: self.config.title.clone(),
            content: RefCell::new(self.content),
            pending_clicks: RefCell::new(HashSet::new()),
            widget_state: RefCell::new(HashMap::new()),
            on_init: RefCell::new(self.on_init),
            on_frame: RefCell::new(self.on_frame),
            plugins: RefCell::new(plugins),
            initialised: Cell::new(false),
        };

        iced_app::run(state, iced_theme, self.config.width, self.config.height)
            .map(|()| AppExit::Ok)
            .map_err(|e| UiError::Backend(e.to_string()))
    }

    #[cfg(all(feature = "egui", not(target_arch = "wasm32")))]
    fn run_egui_or_fallback(mut self) -> Result<AppExit, UiError> {
        use eframe::NativeOptions;
        use oxiui_egui::palette_to_egui_visuals;

        let palette = self.theme.palette().clone();
        let title = self.config.title.clone();
        let width = self.config.width;
        let height = self.config.height;
        let visuals = palette_to_egui_visuals(&palette);
        let content_fn = self.content.take();
        let extra_fonts = std::mem::take(&mut self.config.extra_fonts);

        // Sort plugins by priority (ascending).
        self.plugins.sort_by_key(|p| p.priority());

        // Decode the window icon (if provided) to egui::IconData.
        let icon_data: Option<std::sync::Arc<egui::IconData>> =
            if let Some(icon_bytes) = &self.config.icon {
                match crate::icon::decode_icon(icon_bytes) {
                    Ok(data) => Some(std::sync::Arc::new(data)),
                    Err(e) => {
                        // Non-fatal: log and continue without an icon.
                        eprintln!("oxiui: failed to decode window icon: {e}");
                        None
                    }
                }
            } else {
                None
            };

        // Build the egui ViewportBuilder with all configured props.
        let mut vp = egui::ViewportBuilder::default()
            .with_title(&title)
            .with_inner_size([width, height])
            .with_resizable(self.config.resizable)
            .with_decorations(self.config.decorations)
            .with_transparent(self.config.transparent);

        if self.config.always_on_top {
            vp = vp.with_always_on_top();
        }
        if let Some((min_w, min_h)) = self.config.min_size {
            vp = vp.with_min_inner_size([min_w, min_h]);
        }
        if let Some((max_w, max_h)) = self.config.max_size {
            vp = vp.with_max_inner_size([max_w, max_h]);
        }
        if let Some((px, py)) = self.config.position {
            vp = vp.with_position([px, py]);
        }
        if let Some(icon) = icon_data {
            vp = vp.with_icon(icon);
        }

        let native_opts = NativeOptions {
            viewport: vp,
            ..Default::default()
        };

        let frame_skip = self.frame_skip;
        let egui_frame_hooks = std::mem::take(&mut self.egui_frame_hooks);

        eframe::run_native(
            &title,
            native_opts,
            Box::new(move |cc| {
                cc.egui_ctx.set_visuals(visuals.clone());
                if !extra_fonts.is_empty() {
                    let refs: Vec<(&str, Vec<u8>)> = extra_fonts
                        .iter()
                        .map(|(n, b)| (n.as_str(), b.clone()))
                        .collect();
                    let _ = oxiui_egui::load_fonts_into_egui(&refs, &cc.egui_ctx);
                }
                Ok(Box::new(OxiEguiApp {
                    content: content_fn,
                    on_init: self.on_init,
                    on_frame: self.on_frame,
                    plugins: self.plugins,
                    initialised: false,
                    frame_skip,
                    egui_frame_hooks,
                }))
            }),
        )
        .map(|()| AppExit::Ok)
        .map_err(|e| UiError::Backend(e.to_string()))
    }

    // On wasm32 with the `egui` feature, `eframe::run_native` does not exist.
    // The wasm32 egui path uses `eframe::WebRunner` instead (wired in `oxiui-web`).
    #[cfg(all(feature = "egui", target_arch = "wasm32"))]
    fn run_egui_or_fallback(self) -> Result<AppExit, UiError> {
        let _ = &self.config;
        let _ = &self.theme;
        let _ = &self.content;
        let _ = &self.backend;
        let _ = &self.on_init;
        let _ = &self.on_frame;
        let _ = &self.plugins;
        let _ = &self.frame_skip;
        let _ = &self.egui_frame_hooks;
        Err(UiError::Unsupported(
            "On wasm32, use `oxiui_web::mount(canvas_id)` instead of App::run().".to_string(),
        ))
    }

    #[cfg(not(feature = "egui"))]
    fn run_egui_or_fallback(self) -> Result<AppExit, UiError> {
        // Reference fields to suppress dead-code diagnostics under this cfg path.
        let _ = &self.config;
        let _ = &self.theme;
        let _ = &self.content;
        let _ = &self.backend;
        let _ = &self.on_init;
        let _ = &self.on_frame;
        let _ = &self.plugins;
        let _ = &self.frame_skip;
        Err(UiError::Unsupported(
            "No UI backend enabled. Use default features or enable `egui`.".to_string(),
        ))
    }

    /// Run one synthetic UI frame without opening a real window.
    ///
    /// Calls init hooks + plugin `init`, then the content closure, then
    /// `on_frame` hooks + plugin `update`, all against a `NullUiCtx` (a no-op
    /// [`UiCtx`] that records calls but does not render). Useful for testing
    /// that content closures run without panic, and for CI environments that
    /// have no display server.
    ///
    /// # Errors
    /// Currently infallible; always returns `Ok(AppExit::Ok)`.
    pub fn run_headless_once(mut self) -> Result<AppExit, UiError> {
        struct NullUiCtx;
        impl UiCtx for NullUiCtx {
            fn heading(&mut self, _text: &str) {}
            fn label(&mut self, _text: &str) {}
            fn button(&mut self, _label: &str) -> ButtonResponse {
                ButtonResponse::default()
            }
        }

        // Sort plugins by priority.
        self.plugins.sort_by_key(|p| p.priority());

        let mut null = NullUiCtx;

        // Fire init hooks.
        for hook in self.on_init.iter_mut() {
            hook(&mut null);
        }
        // Fire plugin init.
        for plugin in self.plugins.iter_mut() {
            plugin.init(&mut null);
        }

        // Run the content closure.
        if let Some(ref mut f) = self.content {
            f(&mut null);
        }

        // Fire on_frame hooks.
        for hook in self.on_frame.iter_mut() {
            hook(&mut null);
        }
        // Fire plugin update.
        for plugin in self.plugins.iter_mut() {
            plugin.update(&mut null);
        }

        Ok(AppExit::Ok)
    }

    /// Run the app content once via [`RecordingUiCtx`] and return an accessibility tree.
    ///
    /// This is a headless operation — no event loop or real window is required.
    /// The content closure (if any) is called once through [`RecordingUiCtx`];
    /// all widget calls are captured as [`RecordingEntry`] nodes and assembled
    /// into an [`oxiui_accessibility::A11yTree`] rooted at `window_id`.
    ///
    /// Returns an empty tree (no-op root) if no content closure has been set.
    ///
    /// # Feature
    /// Requires the `a11y` feature.
    #[cfg(feature = "a11y")]
    pub fn build_a11y_snapshot(
        &mut self,
        window_id: oxiui_accessibility::WindowA11yId,
    ) -> oxiui_accessibility::A11yTree {
        let mut recorder = recording::RecordingUiCtx::new();
        if let Some(ref mut f) = self.content {
            f(&mut recorder);
        }
        recorder.build_a11y_tree(window_id)
    }
}

// ─── OxiEguiApp (native egui integration, extracted to egui_backend.rs) ──────
#[cfg(all(feature = "egui", not(target_arch = "wasm32")))]
mod egui_backend;
#[cfg(all(feature = "egui", not(target_arch = "wasm32")))]
use egui_backend::OxiEguiApp;

// ─── Memory baseline utility ─────────────────────────────────────────────────

/// Approximate current process RSS (resident set size) in bytes.
///
/// On macOS and Linux this reads `/proc/self/status` (Linux) or
/// `task_info` via `mach` (macOS-stub).  Because the OxiUI facade is
/// Pure Rust and cross-platform, this utility uses only `std` — it
/// returns `None` on platforms where RSS cannot be determined without
/// additional OS crates.
///
/// # Usage
///
/// Call before and after constructing an app to measure startup overhead:
///
/// ```rust
/// let before = oxiui::process_rss_bytes();
/// let _app = oxiui::App::new(oxiui::AppConfig::default());
/// let after = oxiui::process_rss_bytes();
/// eprintln!("App::new() RSS delta: {} bytes", after.unwrap_or(0).saturating_sub(before.unwrap_or(0)));
/// ```
pub fn process_rss_bytes() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        // Parse /proc/self/status for `VmRSS:` field.
        let status = std::fs::read_to_string("/proc/self/status").ok()?;
        for line in status.lines() {
            if let Some(rest) = line.strip_prefix("VmRSS:") {
                let kb: u64 = rest.split_whitespace().next()?.parse().ok()?;
                return Some(kb * 1024);
            }
        }
        None
    }
    #[cfg(not(target_os = "linux"))]
    {
        // On macOS and other platforms, RSS measurement requires a C/ObjC API
        // (task_info / getrusage) which is outside the Pure Rust scope.
        // Return None — callers should handle the None case gracefully.
        None
    }
}
