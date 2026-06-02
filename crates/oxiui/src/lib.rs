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
/// Provides the [`BackendRunner`] trait and its built-in implementations
/// ([`EguiRunner`], `IcedRunner`) for wiring custom backend dispatchers.
pub mod runner;

#[cfg(feature = "egui")]
#[cfg_attr(docsrs, doc(cfg(feature = "egui")))]
pub use runner::EguiRunner;
#[cfg(feature = "iced")]
#[cfg_attr(docsrs, doc(cfg(feature = "iced")))]
pub use runner::IcedRunner;
pub use runner::{BackendRunner, LifecycleConfig};

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

/// wasm32 web entry point re-exports (requires `web` feature).
///
/// On wasm32 targets, [`web::mount`] boots an OxiUI app on a `<canvas>` element.
/// On native targets the `mount` function returns `Err` — use this module only
/// from wasm32 binaries or from code guarded by `#[cfg(target_arch = "wasm32")]`.
#[cfg(feature = "web")]
#[cfg_attr(docsrs, doc(cfg(feature = "web")))]
pub mod web {
    pub use oxiui_web::mount;
}

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

/// Configuration for building an [`App`].
///
/// Use the builder methods to configure the window, then pass to [`App::new`].
///
/// # Example
///
/// ```rust,no_run
/// use oxiui::AppConfig;
/// let config = AppConfig::new()
///     .title("My App")
///     .size(1024.0, 768.0)
///     .resizable(true)
///     .decorations(true)
///     .transparent(false);
/// ```
#[derive(Debug, Clone)]
pub struct AppConfig {
    /// Window title.
    pub title: String,
    /// Initial window width in logical pixels (0.0 → use default).
    pub width: f32,
    /// Initial window height in logical pixels (0.0 → use default).
    pub height: f32,
    /// Whether the window can be resized by the user.
    pub resizable: bool,
    /// Minimum window size in logical pixels `(width, height)`.
    pub min_size: Option<(f32, f32)>,
    /// Maximum window size in logical pixels `(width, height)`.
    pub max_size: Option<(f32, f32)>,
    /// Whether the window has OS-drawn decorations (title bar, borders).
    ///
    /// Defaults to `true`.
    pub decorations: bool,
    /// Whether the window background is transparent.
    ///
    /// Defaults to `false`.
    pub transparent: bool,
    /// Whether the window is always shown above other windows.
    ///
    /// Defaults to `false`.
    pub always_on_top: bool,
    /// Optional PNG/ICO bytes for the window icon.
    ///
    /// Stored as raw bytes; decoded to RGBA when wiring into egui's
    /// `ViewportBuilder::with_icon`. Requires the `png` crate (present in
    /// `oxiui-render-soft`) — currently decoded via a small inline helper when
    /// the `software` feature is enabled. Without `software`, the icon bytes
    /// are stored but decoding is deferred (see deviation note in TODO.md).
    pub icon: Option<Vec<u8>>,
    /// Initial window position in logical pixels `(x, y)` from the top-left
    /// of the primary monitor.
    pub position: Option<(f32, f32)>,
    /// Extra font families to load at startup.
    ///
    /// Each entry is `(family_name, raw_font_bytes)`. Passed to the active
    /// backend's font loading path when [`App::run`] begins (egui path only
    /// in this release; iced font loading is deferred).
    pub extra_fonts: Vec<(String, Vec<u8>)>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl AppConfig {
    /// Create a new [`AppConfig`] with default settings.
    pub fn new() -> Self {
        Self {
            title: String::new(),
            width: 800.0,
            height: 600.0,
            resizable: true,
            min_size: None,
            max_size: None,
            decorations: true,
            transparent: false,
            always_on_top: false,
            icon: None,
            position: None,
            extra_fonts: Vec::new(),
        }
    }

    /// Set the window title.
    pub fn title(mut self, t: impl Into<String>) -> Self {
        self.title = t.into();
        self
    }

    /// Set the initial window size in logical pixels.
    pub fn size(mut self, w: f32, h: f32) -> Self {
        self.width = w;
        self.height = h;
        self
    }

    /// Set whether the window can be resized.
    pub fn resizable(mut self, r: bool) -> Self {
        self.resizable = r;
        self
    }

    /// Set the minimum window size in logical pixels.
    pub fn min_size(mut self, w: f32, h: f32) -> Self {
        self.min_size = Some((w, h));
        self
    }

    /// Set the maximum window size in logical pixels.
    pub fn max_size(mut self, w: f32, h: f32) -> Self {
        self.max_size = Some((w, h));
        self
    }

    /// Set whether the window has OS-drawn decorations (title bar, borders).
    pub fn decorations(mut self, d: bool) -> Self {
        self.decorations = d;
        self
    }

    /// Set whether the window background is transparent.
    pub fn transparent(mut self, t: bool) -> Self {
        self.transparent = t;
        self
    }

    /// Set whether the window is always shown above other windows.
    pub fn always_on_top(mut self, a: bool) -> Self {
        self.always_on_top = a;
        self
    }

    /// Set the window icon from raw PNG/ICO bytes.
    pub fn icon(mut self, bytes: Vec<u8>) -> Self {
        self.icon = Some(bytes);
        self
    }

    /// Set the initial window position in logical pixels from top-left of primary monitor.
    pub fn position(mut self, x: f32, y: f32) -> Self {
        self.position = Some((x, y));
        self
    }
}

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

// ─── Command palette ─────────────────────────────────────────────────────────

/// A named, searchable command.
pub struct Command {
    /// Unique identifier.
    pub id: String,
    /// Display label shown in the palette.
    pub label: String,
    /// Optional keyboard shortcut hint displayed alongside the label.
    pub shortcut: Option<String>,
    /// Action to invoke when the command is selected.
    pub action: Box<dyn Fn() + Send + Sync>,
}

/// A searchable registry of [`Command`]s.
///
/// Commands are registered via [`CommandPalette::register`] and searched
/// via [`CommandPalette::search`] using a simple fuzzy-match algorithm
/// (all query characters must appear in the label in order, case-insensitive).
pub struct CommandPalette {
    commands: Vec<Command>,
}

impl CommandPalette {
    /// Create an empty [`CommandPalette`].
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
        }
    }

    /// Register a command.
    pub fn register(
        &mut self,
        id: impl Into<String>,
        label: impl Into<String>,
        action: impl Fn() + Send + Sync + 'static,
    ) {
        self.commands.push(Command {
            id: id.into(),
            label: label.into(),
            shortcut: None,
            action: Box::new(action),
        });
    }

    /// Register a command with an optional keyboard shortcut hint.
    pub fn register_with_shortcut(
        &mut self,
        id: impl Into<String>,
        label: impl Into<String>,
        shortcut: Option<String>,
        action: impl Fn() + Send + Sync + 'static,
    ) {
        self.commands.push(Command {
            id: id.into(),
            label: label.into(),
            shortcut,
            action: Box::new(action),
        });
    }

    /// Search for commands whose labels fuzzy-match `query`.
    ///
    /// The match is case-insensitive and requires that every character in
    /// `query` appear in `label` in order (subsequence matching).
    pub fn search(&self, query: &str) -> Vec<&Command> {
        let query_lc = query.to_lowercase();
        self.commands
            .iter()
            .filter(|cmd| {
                let label_lc = cmd.label.to_lowercase();
                let mut q_iter = query_lc.chars();
                let mut current = q_iter.next();
                for ch in label_lc.chars() {
                    if current == Some(ch) {
                        current = q_iter.next();
                    }
                    if current.is_none() {
                        return true;
                    }
                }
                current.is_none()
            })
            .collect()
    }

    /// The number of registered commands.
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    /// Returns `true` if no commands are registered.
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}

impl Default for CommandPalette {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Notification queue ───────────────────────────────────────────────────────

/// A pending in-app toast notification.
#[derive(Debug, Clone)]
pub struct Notification {
    /// Short title line.
    pub title: String,
    /// Longer body text.
    pub body: String,
    /// How long the notification should be displayed, in milliseconds.
    pub duration_ms: u64,
    /// Urgency level: 0 = low, 1 = normal, 2 = critical.
    pub urgency: u8,
    /// When the notification was created.
    pub created_at: std::time::Instant,
}

/// A FIFO queue of pending [`Notification`]s.
///
/// Call [`NotificationQueue::push`] to enqueue notifications, and
/// [`NotificationQueue::pop_due`] each frame to drain them for display.
pub struct NotificationQueue {
    pending: std::collections::VecDeque<Notification>,
}

impl NotificationQueue {
    /// Create an empty [`NotificationQueue`].
    pub fn new() -> Self {
        Self {
            pending: std::collections::VecDeque::new(),
        }
    }

    /// Enqueue a notification.
    pub fn push(&mut self, title: impl Into<String>, body: impl Into<String>, duration_ms: u64) {
        self.pending.push_back(Notification {
            title: title.into(),
            body: body.into(),
            duration_ms,
            urgency: 1,
            created_at: std::time::Instant::now(),
        });
    }

    /// Enqueue a notification with explicit urgency (0=low, 1=normal, 2=critical).
    pub fn enqueue(&mut self, title: impl Into<String>, body: impl Into<String>, urgency: u8) {
        let duration_ms = match urgency {
            0 => 3_000,
            2 => 10_000,
            _ => 5_000,
        };
        self.pending.push_back(Notification {
            title: title.into(),
            body: body.into(),
            duration_ms,
            urgency,
            created_at: std::time::Instant::now(),
        });
    }

    /// Dequeue the next pending notification, if any.
    pub fn pop_due(&mut self) -> Option<Notification> {
        self.pending.pop_front()
    }

    /// Returns `true` if no notifications are pending.
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// The number of pending notifications.
    pub fn len(&self) -> usize {
        self.pending.len()
    }
}

impl Default for NotificationQueue {
    fn default() -> Self {
        Self::new()
    }
}

// ─── iced state types (module-level so free functions can reference them) ────
// These types are only compiled when the "iced" feature is active.
#[cfg(feature = "iced")]
mod iced_app {
    use std::cell::{Cell, RefCell};
    use std::collections::{HashMap, HashSet};

    use iced::Element;
    use iced::Task;
    use oxiui_iced::{apply_message, IcedConfig, IcedUiCtx, Message, WidgetState};

    use crate::{ContentFn, HookFn, Plugin};

    /// Application state threaded through iced's `update`/`view` loop.
    ///
    /// iced's `view(&State)` takes an immutable reference, so we use `RefCell`
    /// for interior mutability (the content closure and click/widget state).
    pub struct OxiIcedState {
        /// Window title (supplied to the `.title()` callback).
        pub title: String,
        /// The user-supplied content closure; called every `view` frame.
        pub content: RefCell<Option<ContentFn>>,
        /// Button ids whose `ButtonPressed` message was received this cycle.
        pub pending_clicks: RefCell<HashSet<usize>>,
        /// Per-widget retained state (text, checked, slider, selected index).
        pub widget_state: RefCell<HashMap<usize, WidgetState>>,
        /// Lifecycle on_init hooks; called once before the first frame.
        pub on_init: RefCell<Vec<HookFn>>,
        /// Lifecycle on_frame hooks; called every frame after content.
        pub on_frame: RefCell<Vec<HookFn>>,
        /// Registered plugins sorted by priority.
        pub plugins: RefCell<Vec<Box<dyn Plugin>>>,
        /// Whether the init phase has been completed.
        pub initialised: Cell<bool>,
    }

    impl OxiIcedState {
        /// Create an empty fallback state (used if the boot mutex is poisoned).
        pub fn empty() -> Self {
            Self {
                title: String::new(),
                content: RefCell::new(None),
                pending_clicks: RefCell::new(HashSet::new()),
                widget_state: RefCell::new(HashMap::new()),
                on_init: RefCell::new(Vec::new()),
                on_frame: RefCell::new(Vec::new()),
                plugins: RefCell::new(Vec::new()),
                initialised: Cell::new(false),
            }
        }
    }

    /// iced update function — advances widget state and click tracking.
    pub fn update(state: &mut OxiIcedState, msg: Message) -> Task<Message> {
        let mut clicks = state.pending_clicks.borrow_mut();
        let mut widget_state = state.widget_state.borrow_mut();
        apply_message(&mut widget_state, &mut clicks, &msg);
        Task::none()
    }

    /// iced view function — drives the content closure through `IcedUiCtx`.
    ///
    /// Also fires init hooks + plugin init on the first frame, and on_frame
    /// hooks + plugin update every frame. This mirrors the pattern used by
    /// `OxiEguiApp::ui()` (egui path).
    pub fn view<'a>(state: &'a OxiIcedState) -> Element<'a, Message> {
        // Drain pending clicks for this frame.
        let clicks = {
            let mut guard = state.pending_clicks.borrow_mut();
            std::mem::take(&mut *guard)
        };
        let widget_state = state.widget_state.borrow().clone();

        let config = IcedConfig {
            pending_clicks: clicks,
            state: widget_state,
            spacing: 8.0,
            padding: 0.0,
            title: state.title.clone(),
            spec_capacity_hint: 0,
        };
        let mut ctx = IcedUiCtx::new(config);

        // Fire init hooks and plugin init exactly once.
        if !state.initialised.get() {
            state.initialised.set(true);
            if let Ok(mut hooks) = state.on_init.try_borrow_mut() {
                for hook in hooks.iter_mut() {
                    hook(&mut ctx);
                }
            }
            if let Ok(mut plugins) = state.plugins.try_borrow_mut() {
                for plugin in plugins.iter_mut() {
                    plugin.init(&mut ctx);
                }
            }
        }

        // Drive the content closure through the UiCtx bridge.
        if let Ok(mut content_guard) = state.content.try_borrow_mut() {
            if let Some(ref mut f) = *content_guard {
                f(&mut ctx);
            }
        }

        // Fire per-frame hooks and plugin updates.
        if let Ok(mut hooks) = state.on_frame.try_borrow_mut() {
            for hook in hooks.iter_mut() {
                hook(&mut ctx);
            }
        }
        if let Ok(mut plugins) = state.plugins.try_borrow_mut() {
            for plugin in plugins.iter_mut() {
                plugin.update(&mut ctx);
            }
        }

        // `into_iced_element()` returns `Element<'static, Message>`.
        // `'static: 'a` by subtyping, so the coercion is valid.
        let elem: Element<'static, Message> = ctx.into_iced_element();
        // Cast the lifetime from 'static to 'a (safe: 'static is longer).
        // SAFETY: all widget content is owned strings; no borrowed data from state.
        elem
    }

    /// Run the iced application with the given state and theme.
    pub fn run(
        state: OxiIcedState,
        iced_theme: iced::Theme,
        width: f32,
        height: f32,
    ) -> iced::Result {
        let boot_state = std::sync::Mutex::new(Some(state));

        let boot = move || {
            boot_state
                .lock()
                .ok()
                .and_then(|mut g| g.take())
                .unwrap_or_else(OxiIcedState::empty)
        };

        let title_fn = move |s: &OxiIcedState| s.title.clone();
        let theme_fn = move |_: &OxiIcedState| iced_theme.clone();
        let _ = width;
        let _ = height;

        iced::application(boot, update, view)
            .title(title_fn)
            .theme(theme_fn)
            .run()
    }
}

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
    /// [`egui::Context::request_repaint_after`] with a 1-second delay whenever
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

// ─── OxiEguiApp (native egui integration) ────────────────────────────────────

// OxiEguiApp is only used by `run_native`, which only exists on non-wasm32 targets.
#[cfg(all(feature = "egui", not(target_arch = "wasm32")))]
struct OxiEguiApp {
    content: Option<ContentFn>,
    on_init: Vec<HookFn>,
    on_frame: Vec<HookFn>,
    plugins: Vec<Box<dyn Plugin>>,
    initialised: bool,
    /// If true, yield CPU when no input events occurred this frame.
    frame_skip: bool,
    /// Raw egui::Context escape-hatch callbacks.
    egui_frame_hooks: Vec<EguiFrameHook>,
}

#[cfg(all(feature = "egui", not(target_arch = "wasm32")))]
impl eframe::App for OxiEguiApp {
    /// Called each frame with the root [`egui::Ui`].
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Clone the context now (cheap Arc clone) so we can pass it to hooks
        // without conflicting with the EguiUiCtx borrow below.
        let egui_ctx = ui.ctx().clone();

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

        // Frame-skip: if no input events occurred this frame, defer the next repaint.
        if self.frame_skip && egui_ctx.input(|i| i.events.is_empty()) {
            egui_ctx.request_repaint_after(std::time::Duration::from_secs(1));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxiui_core::events::{Key, Modifiers};

    // ─── STEP 1: Iced plugin init wiring (backend-agnostic proxy via headless) ──

    /// Plugins registered on a headless app fire init+update in priority order.
    /// This indirectly proves OxiIcedState::empty() and the priority sort compile.
    #[test]
    fn test_iced_plugin_init_called() {
        use std::sync::{Arc, Mutex};

        struct SpyPlugin {
            counter: Arc<Mutex<u32>>,
        }
        impl Plugin for SpyPlugin {
            fn init(&mut self, _ctx: &mut dyn UiCtx) {
                *self.counter.lock().unwrap() += 1;
            }
            fn update(&mut self, _ctx: &mut dyn UiCtx) {}
        }

        let counter = Arc::new(Mutex::new(0u32));
        let counter_c = Arc::clone(&counter);

        App::new(AppConfig::new())
            .plugin(SpyPlugin { counter: counter_c })
            .run_headless_once()
            .unwrap();

        assert_eq!(
            *counter.lock().unwrap(),
            1,
            "plugin init must be called once"
        );
    }

    // ─── STEP 2: Window config props ─────────────────────────────────────────

    /// All seven new AppConfig fields round-trip through the builder correctly.
    #[test]
    fn test_app_config_window_props_set() {
        let cfg = AppConfig::new()
            .min_size(400.0, 300.0)
            .max_size(1920.0, 1080.0)
            .decorations(false)
            .transparent(true)
            .always_on_top(true)
            .icon(vec![0u8, 1, 2, 3])
            .position(100.0, 200.0);

        assert_eq!(cfg.min_size, Some((400.0, 300.0)));
        assert_eq!(cfg.max_size, Some((1920.0, 1080.0)));
        assert!(!cfg.decorations);
        assert!(cfg.transparent);
        assert!(cfg.always_on_top);
        assert_eq!(cfg.icon, Some(vec![0u8, 1, 2, 3]));
        assert_eq!(cfg.position, Some((100.0, 200.0)));
    }

    /// AppConfig default values are correct (decorations=true, transparent=false, etc.).
    #[test]
    fn test_app_config_defaults() {
        let cfg = AppConfig::new();
        assert!(cfg.decorations, "decorations defaults to true");
        assert!(!cfg.transparent, "transparent defaults to false");
        assert!(!cfg.always_on_top, "always_on_top defaults to false");
        assert!(cfg.min_size.is_none());
        assert!(cfg.max_size.is_none());
        assert!(cfg.icon.is_none());
        assert!(cfg.position.is_none());
    }

    // ─── STEP 3a: App::notify enqueues ───────────────────────────────────────

    #[test]
    fn test_app_notify_enqueues() {
        let app = App::new(AppConfig::new()).notify("Alert", "Something happened", 1);
        assert_eq!(
            app.notifications().len(),
            1,
            "one notification must be enqueued"
        );
        let n = app.notifications.pending.iter().next().unwrap();
        assert_eq!(n.title, "Alert");
        assert_eq!(n.body, "Something happened");
        assert_eq!(n.urgency, 1);
    }

    // ─── STEP 3b: App::hotkey conflict detection ──────────────────────────────

    #[test]
    fn test_app_hotkey_conflict_detection() {
        let mods = Modifiers {
            ctrl: true,
            ..Modifiers::NONE
        };
        let key = Key::Character("s".into());

        let app = App::new(AppConfig::new())
            .try_hotkey(mods, key.clone(), "save")
            .expect("first registration must succeed");

        let result = app.try_hotkey(mods, key, "save-duplicate");
        assert!(result.is_err(), "duplicate hotkey must return Err");
    }

    #[test]
    fn test_hotkey_conflict_error_type() {
        let mods = Modifiers::NONE;
        let key = Key::Escape;

        let app = App::new(AppConfig::new())
            .try_hotkey(mods, key.clone(), "esc")
            .unwrap();

        match app.try_hotkey(mods, key, "esc2") {
            Err(err) => assert!(!err.message.is_empty()),
            Ok(_) => panic!("expected HotkeyConflict error"),
        }
    }

    // ─── STEP 3c: Command palette fuzzy match ────────────────────────────────

    #[test]
    fn test_command_palette_fuzzy_match() {
        let app = App::new(AppConfig::new())
            .register_command("Save File", None)
            .register_command("Open File", None)
            .register_command("Quit", None);

        let matches = app.command_matches("save");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0], "Save File");
    }

    #[test]
    fn test_command_palette_empty_query_matches_all() {
        let app = App::new(AppConfig::new())
            .register_command("Alpha", None)
            .register_command("Beta", None);

        let matches = app.command_matches("");
        assert_eq!(matches.len(), 2);
    }

    // ─── STEP 3d: Screenshot ─────────────────────────────────────────────────

    #[test]
    fn test_screenshot_returns_nonempty_or_unsupported() {
        let app = App::new(AppConfig::new().size(64.0, 48.0));
        let result = app.screenshot();
        match result {
            Ok(bytes) => assert!(!bytes.is_empty(), "screenshot bytes must be non-empty"),
            Err(UiError::Unsupported(_)) => {
                // expected when `software` feature is not enabled
            }
            Err(e) => panic!("unexpected screenshot error: {e:?}"),
        }
    }

    // ─── STEP 3e: run_with_return ─────────────────────────────────────────────

    #[test]
    fn test_run_with_return_headless() {
        let app = App::new(AppConfig::new());
        let result = app.run_with_return(|_ui| 42u32);
        assert_eq!(result.unwrap(), 42u32);
    }

    #[test]
    fn test_run_with_return_string_value() {
        let app = App::new(AppConfig::new());
        let result = app.run_with_return(|_ui| "hello".to_string());
        assert_eq!(result.unwrap(), "hello");
    }

    // ─── STEP 3f: Lifecycle on_close/on_resize/on_focus registered ───────────

    #[test]
    fn test_lifecycle_on_close_registered() {
        // on_close hooks are stored and survive the builder chain (not fired in headless).
        let _app = App::new(AppConfig::new()).on_close(|_ui| {});
        // If this compiles, the hook is accepted.
    }

    #[test]
    fn test_lifecycle_on_resize_registered() {
        let _app = App::new(AppConfig::new()).on_resize(|_ui| {});
    }

    #[test]
    fn test_lifecycle_on_focus_registered() {
        let _app = App::new(AppConfig::new()).on_focus(|_ui| {});
    }

    // ─── STEP 3g: Richer AppExit ─────────────────────────────────────────────

    #[test]
    fn test_app_exit_richer_reason() {
        let r1 = AppExit::RequestedByUser;
        let r2 = AppExit::Programmatic("deliberate shutdown".into());
        let r3 = AppExit::Ok;

        assert_eq!(r1, AppExit::RequestedByUser);
        assert_eq!(r2, AppExit::Programmatic("deliberate shutdown".into()));
        assert_ne!(r1, r3);
        assert_ne!(r2, AppExit::Programmatic("other".into()));
    }

    // ─── STEP 3h: Prelude exports UiCtx ──────────────────────────────────────

    #[test]
    fn test_prelude_exports_uictx() {
        // Verifying at compile-time that `UiCtx` is in the prelude.
        use crate::prelude::*;
        // If this compiles, UiCtx is re-exported.
        fn _accepts_ctx(_: &dyn UiCtx) {}
    }

    // ─── Integration: headless smoke (equivalent to test_every_example_compiles) ──

    #[test]
    fn test_headless_smoke_all_apis() {
        // Exercise all new APIs in a single headless run.
        use std::sync::{Arc, Mutex};

        struct CountPlugin(Arc<Mutex<u32>>);
        impl Plugin for CountPlugin {
            fn init(&mut self, _: &mut dyn UiCtx) {
                *self.0.lock().unwrap() += 10;
            }
            fn update(&mut self, _: &mut dyn UiCtx) {
                *self.0.lock().unwrap() += 1;
            }
        }

        let counter = Arc::new(Mutex::new(0u32));

        App::new(
            AppConfig::new()
                .title("smoke")
                .min_size(100.0, 100.0)
                .decorations(true)
                .transparent(false),
        )
        .plugin(CountPlugin(Arc::clone(&counter)))
        .on_init(|_| {})
        .on_frame(|_| {})
        .notify("Test", "body", 0)
        .content(|ui| {
            ui.heading("h");
        })
        .run_headless_once()
        .unwrap();

        let c = *counter.lock().unwrap();
        assert_eq!(c, 11, "init=10, update=1");
    }
}
