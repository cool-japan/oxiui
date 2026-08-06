//! Pluggable backend runner infrastructure for OxiUI.
//!
//! The [`BackendRunner`] trait decouples backend selection from
//! [`crate::App::run`] dispatch logic. Each backend provides its own
//! [`BackendRunner`] implementation; the app moves its configuration into the
//! appropriate runner at run time and delegates the live event loop to it.
//!
//! # Live delegation
//! [`EguiRunner`] (feature `egui`) and [`IcedRunner`] (feature `iced`) own the
//! real rendering paths. [`crate::App::run`] takes the app's fields
//! (theme, hooks, plugins, …) with [`std::mem::take`], builds the matching
//! runner, assembles a [`LifecycleConfig`], and calls [`BackendRunner::run`].
//! The runner then drives `eframe::run_native` / `iced::application` to
//! completion.
//!
//! # Lifecycle tracking
//! [`crate::runner::LifecycleTracker`] deduplicates raw window snapshots into
//! [`crate::runner::LifecycleEvent`]s so that `on_resize` / `on_focus` fire only on real
//! changes and `on_close` fires at most once. See
//! [`crate::runner::LifecycleTracker::observe`].

use crate::shell::ShellConfig;
use crate::{AppConfig, AppExit, HookFn};
use oxiui_core::UiError;

/// Content closure type passed to a backend runner.
///
/// A boxed, send-capable, frame-driven closure that receives a mutable reference
/// to a [`oxiui_core::UiCtx`] on each frame.
pub type ContentFn = Box<dyn FnMut(&mut dyn oxiui_core::UiCtx) + Send>;

/// Lifecycle callbacks handed to a [`BackendRunner`] at startup.
///
/// Each field is the exact hook vector registered on the [`crate::App`] via
/// [`crate::App::on_close`] / [`crate::App::on_resize`] / [`crate::App::on_focus`].
/// Backends fire these against a `NullUiCtx` when the matching
/// window event fires, because lifecycle events happen outside a live drawing
/// frame (there is no `EguiUiCtx` / `IcedUiCtx` bound at that point).
///
/// The size / focus values that drive the event stream are consumed by the
/// [`LifecycleTracker`] for change-detection; the hooks themselves take the
/// established `FnMut(&mut dyn UiCtx)` shape. A future typed-argument lifecycle
/// API (passing the new size / focus state into the hook) is out of scope for
/// this release.
#[derive(Default)]
pub struct LifecycleConfig {
    /// Hooks fired once when the window is closing.
    pub on_close: Vec<HookFn>,
    /// Hooks fired whenever the window size changes.
    pub on_resize: Vec<HookFn>,
    /// Hooks fired whenever the window gains or loses focus.
    pub on_focus: Vec<HookFn>,
}

/// A raw, per-poll snapshot of window lifecycle state.
///
/// Fields are `Option` so a backend can report only what it observed this poll
/// (e.g. iced's `resize_events` subscription reports a size but no focus).
#[derive(Debug, Clone, Copy, Default)]
pub struct LifecycleSnapshot {
    /// The current window size in logical pixels, if known this poll.
    pub size: Option<(f32, f32)>,
    /// The current focus state, if known this poll.
    pub focused: Option<bool>,
    /// Whether a window-close was requested this poll.
    pub close_requested: bool,
}

/// A deduplicated lifecycle event emitted by [`LifecycleTracker::observe`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LifecycleEvent {
    /// The window was resized to `(width, height)` logical pixels.
    Resized(f32, f32),
    /// The window gained (`true`) or lost (`false`) focus.
    Focus(bool),
    /// The window is closing (emitted at most once).
    Close,
}

/// Deduplicates raw [`LifecycleSnapshot`]s into change-only [`LifecycleEvent`]s.
///
/// * `Resized` is emitted only when the size differs from the last observed size.
/// * `Focus` is emitted only when the focus state flips.
/// * `Close` is emitted at most once for the lifetime of the tracker.
///
/// The first snapshot that carries a size / focus value is treated as a change
/// (`None` → `Some`), so a backend that reports its initial geometry receives
/// one seeding `Resized` / `Focus` event.
#[derive(Debug, Default)]
pub struct LifecycleTracker {
    last_size: Option<(f32, f32)>,
    last_focused: Option<bool>,
    close_fired: bool,
}

impl LifecycleTracker {
    /// Fold a raw snapshot into the tracker, returning the events it produced.
    pub fn observe(&mut self, snap: LifecycleSnapshot) -> Vec<LifecycleEvent> {
        let mut events = Vec::new();
        if let Some(size) = snap.size {
            if self.last_size != Some(size) {
                self.last_size = Some(size);
                events.push(LifecycleEvent::Resized(size.0, size.1));
            }
        }
        if let Some(focused) = snap.focused {
            if self.last_focused != Some(focused) {
                self.last_focused = Some(focused);
                events.push(LifecycleEvent::Focus(focused));
            }
        }
        if snap.close_requested && !self.close_fired {
            self.close_fired = true;
            events.push(LifecycleEvent::Close);
        }
        events
    }
}

/// Trait for pluggable backend runners.
///
/// Implement this trait to integrate a new GUI backend with OxiUI. The trait is
/// object-safe; callers box the implementor and invoke [`BackendRunner::run`].
pub trait BackendRunner: Send + 'static {
    /// Launch the backend event loop.
    ///
    /// This call blocks until the event loop terminates, then returns the
    /// exit status or a [`UiError`] describing why the backend failed to start.
    fn run(
        self: Box<Self>,
        config: AppConfig,
        content: ContentFn,
        lifecycle: LifecycleConfig,
    ) -> Result<AppExit, UiError>;

    /// Hand the runner the application shell (secondary windows + menu bar).
    ///
    /// [`crate::App::run`] calls this once, before [`BackendRunner::run`], with
    /// everything the facade's window / menu builder APIs registered. A runner
    /// that stores the shell must actually render it.
    ///
    /// # Errors
    ///
    /// The default implementation accepts an empty shell and rejects a
    /// non-empty one with [`UiError::Unsupported`], so a backend that cannot
    /// open secondary windows or draw a menu bar reports that to the caller
    /// instead of silently dropping the request. Override it to consume the
    /// shell (or to reject only the parts the backend cannot honour).
    fn set_shell(&mut self, shell: ShellConfig) -> Result<(), UiError> {
        if shell.is_empty() {
            Ok(())
        } else {
            Err(UiError::Unsupported(format!(
                "this backend runner does not consume the application shell ({})",
                shell.describe()
            )))
        }
    }
}

/// Live [`BackendRunner`] for the egui backend.
///
/// Carries the app configuration that the egui event loop needs (theme, hooks,
/// plugins, frame-skip flag, and raw-context escape hooks). [`BackendRunner::run`]
/// assembles the `eframe::ViewportBuilder` / `NativeOptions` and boots
/// `eframe::run_native`, constructing an `OxiEguiApp` wired with the
/// supplied [`LifecycleConfig`].
#[cfg(feature = "egui")]
pub struct EguiRunner {
    pub(crate) theme: Box<dyn oxiui_core::Theme>,
    pub(crate) on_init: Vec<HookFn>,
    pub(crate) on_frame: Vec<HookFn>,
    pub(crate) plugins: Vec<Box<dyn crate::Plugin>>,
    pub(crate) frame_skip: bool,
    pub(crate) egui_frame_hooks: Vec<crate::EguiFrameHook>,
    /// Secondary windows + menu bar handed over by [`BackendRunner::set_shell`].
    pub(crate) shell: ShellConfig,
}

#[cfg(feature = "egui")]
impl Default for EguiRunner {
    fn default() -> Self {
        Self {
            theme: oxiui_theme::cooljapan_default(),
            on_init: Vec::new(),
            on_frame: Vec::new(),
            plugins: Vec::new(),
            frame_skip: false,
            egui_frame_hooks: Vec::new(),
            shell: ShellConfig::default(),
        }
    }
}

#[cfg(feature = "egui")]
impl EguiRunner {
    /// Create an [`EguiRunner`] with default state (COOLJAPAN theme, no hooks).
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the theme the egui backend applies at startup.
    pub fn theme(mut self, theme: Box<dyn oxiui_core::Theme>) -> Self {
        self.theme = theme;
        self
    }
}

#[cfg(feature = "egui")]
impl BackendRunner for EguiRunner {
    /// Store the shell; the native egui backend honours all of it.
    ///
    /// Secondary windows become deferred viewports (real OS windows) and the
    /// menu bar is drawn at the top of the primary frame. On wasm32 the whole
    /// [`BackendRunner::run`] path is unsupported, so nothing is silently lost.
    fn set_shell(&mut self, shell: ShellConfig) -> Result<(), UiError> {
        self.shell = shell;
        Ok(())
    }

    fn run(
        self: Box<Self>,
        config: AppConfig,
        content: ContentFn,
        lifecycle: LifecycleConfig,
    ) -> Result<AppExit, UiError> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            (*self).run_native(config, content, lifecycle)
        }
        #[cfg(target_arch = "wasm32")]
        {
            // On wasm32, `eframe::run_native` does not exist; the browser path
            // is driven by `oxiui_web::mount`. Reference the moved-in state so
            // no field is flagged as dead on this cfg.
            let _ = (self, config, content, lifecycle);
            Err(UiError::Unsupported(
                "On wasm32, use `oxiui_web::mount(canvas_id)` instead of App::run().".to_string(),
            ))
        }
    }
}

#[cfg(all(feature = "egui", not(target_arch = "wasm32")))]
impl EguiRunner {
    fn run_native(
        self,
        mut config: AppConfig,
        content: ContentFn,
        lifecycle: LifecycleConfig,
    ) -> Result<AppExit, UiError> {
        use eframe::NativeOptions;
        use oxiui_egui::palette_to_egui_visuals;

        let EguiRunner {
            theme,
            on_init,
            on_frame,
            mut plugins,
            frame_skip,
            egui_frame_hooks,
            shell,
        } = self;

        let palette = theme.palette().clone();
        let title = config.title.clone();
        let width = config.width;
        let height = config.height;
        let visuals = palette_to_egui_visuals(&palette);
        let extra_fonts = std::mem::take(&mut config.extra_fonts);

        // Sort plugins by priority (ascending).
        plugins.sort_by_key(|p| p.priority());

        // Decode the window icon (if provided) to egui::IconData.
        let icon_data: Option<std::sync::Arc<egui::IconData>> =
            if let Some(icon_bytes) = &config.icon {
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
            .with_resizable(config.resizable)
            .with_decorations(config.decorations)
            .with_transparent(config.transparent);

        if config.always_on_top {
            vp = vp.with_always_on_top();
        }
        if let Some((min_w, min_h)) = config.min_size {
            vp = vp.with_min_inner_size([min_w, min_h]);
        }
        if let Some((max_w, max_h)) = config.max_size {
            vp = vp.with_max_inner_size([max_w, max_h]);
        }
        if let Some((px, py)) = config.position {
            vp = vp.with_position([px, py]);
        }
        if let Some(icon) = icon_data {
            vp = vp.with_icon(icon);
        }

        let native_opts = NativeOptions {
            viewport: vp,
            ..Default::default()
        };

        let LifecycleConfig {
            on_close,
            on_resize,
            on_focus,
        } = lifecycle;

        let ShellConfig {
            windows,
            contents,
            menu_bar,
            handle: window_handle,
        } = shell;
        let session = crate::multiwindow::WindowSession::from_descriptors(&windows);

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
                Ok(Box::new(crate::egui_backend::OxiEguiApp {
                    content: Some(content),
                    on_init,
                    on_frame,
                    plugins,
                    initialised: false,
                    frame_skip,
                    egui_frame_hooks,
                    on_close,
                    on_resize,
                    on_focus,
                    tracker: LifecycleTracker::default(),
                    menu_bar,
                    menu_state: crate::menu::MenuBarState::new(),
                    windows,
                    window_contents: contents,
                    window_handle,
                    session,
                    os_closed: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
                }))
            }),
        )
        .map(|()| AppExit::Ok)
        .map_err(|e| UiError::Backend(e.to_string()))
    }
}

/// Live [`BackendRunner`] for the iced backend.
///
/// Carries the app configuration the iced event loop needs (theme, hooks,
/// plugins). [`BackendRunner::run`] builds the retained-mode
/// `OxiIcedState`, wires the lifecycle subscription, and
/// boots `iced::application`.
#[cfg(feature = "iced")]
pub struct IcedRunner {
    pub(crate) theme: Box<dyn oxiui_core::Theme>,
    pub(crate) on_init: Vec<HookFn>,
    pub(crate) on_frame: Vec<HookFn>,
    pub(crate) plugins: Vec<Box<dyn crate::Plugin>>,
    /// Menu bar handed over by [`BackendRunner::set_shell`]; iced 0.14 cannot
    /// open secondary windows from the single-window `iced::application`
    /// entry point, so those are rejected rather than stored.
    pub(crate) menu_bar: Option<crate::menu::MenuBar>,
}

#[cfg(feature = "iced")]
impl Default for IcedRunner {
    fn default() -> Self {
        Self {
            theme: oxiui_theme::cooljapan_default(),
            on_init: Vec::new(),
            on_frame: Vec::new(),
            plugins: Vec::new(),
            menu_bar: None,
        }
    }
}

#[cfg(feature = "iced")]
impl IcedRunner {
    /// Create an [`IcedRunner`] with default state (COOLJAPAN theme, no hooks).
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the theme the iced backend applies at startup.
    pub fn theme(mut self, theme: Box<dyn oxiui_core::Theme>) -> Self {
        self.theme = theme;
        self
    }
}

#[cfg(feature = "iced")]
impl BackendRunner for IcedRunner {
    /// Accept the menu bar; reject secondary windows.
    ///
    /// # Errors
    ///
    /// Returns [`UiError::Unsupported`] when the app registered secondary
    /// windows: iced 0.14's `iced::application` entry point drives exactly one
    /// window, and multi-window support requires the `iced::daemon` runtime,
    /// which OxiUI does not target in this release. Reporting the error keeps
    /// `App::run()` honest instead of dropping the windows on the floor.
    fn set_shell(&mut self, shell: ShellConfig) -> Result<(), UiError> {
        if shell.has_secondary_windows() {
            return Err(UiError::Unsupported(format!(
                "the iced backend cannot open secondary windows ({} registered); \
                 use the default egui backend for multi-window apps",
                shell.windows.len()
            )));
        }
        self.menu_bar = shell.menu_bar;
        Ok(())
    }

    fn run(
        self: Box<Self>,
        config: AppConfig,
        content: ContentFn,
        lifecycle: LifecycleConfig,
    ) -> Result<AppExit, UiError> {
        use std::cell::{Cell, RefCell};
        use std::collections::{HashMap, HashSet};

        use oxiui_iced::palette_to_iced_theme;

        let IcedRunner {
            theme,
            on_init,
            on_frame,
            mut plugins,
            menu_bar,
        } = *self;

        let iced_theme = palette_to_iced_theme(&theme.palette().clone());

        // Sort plugins by priority before handing off to the iced state.
        plugins.sort_by_key(|p| p.priority());

        let LifecycleConfig {
            on_close,
            on_resize,
            on_focus,
        } = lifecycle;

        let state = crate::iced_backend::OxiIcedState {
            title: config.title.clone(),
            content: RefCell::new(Some(content)),
            pending_clicks: RefCell::new(HashSet::new()),
            widget_state: RefCell::new(HashMap::new()),
            on_init: RefCell::new(on_init),
            on_frame: RefCell::new(on_frame),
            plugins: RefCell::new(plugins),
            initialised: Cell::new(false),
            on_close: RefCell::new(on_close),
            on_resize: RefCell::new(on_resize),
            on_focus: RefCell::new(on_focus),
            tracker: RefCell::new(LifecycleTracker::default()),
            menu_bar,
            menu_state: RefCell::new(crate::menu::MenuBarState::new()),
        };

        crate::iced_backend::run(state, iced_theme, config.width, config.height)
            .map(|()| AppExit::Ok)
            .map_err(|e| UiError::Backend(e.to_string()))
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menu::MenuBar;
    use crate::multiwindow::WindowRegistry;
    use oxiui_core::window::WindowConfig;

    /// A runner that keeps the default `set_shell` implementation.
    struct InertRunner;

    impl BackendRunner for InertRunner {
        fn run(
            self: Box<Self>,
            _config: AppConfig,
            _content: ContentFn,
            _lifecycle: LifecycleConfig,
        ) -> Result<AppExit, UiError> {
            Ok(AppExit::Ok)
        }
    }

    fn shell_with_window() -> ShellConfig {
        let mut reg = WindowRegistry::new();
        reg.open_window(WindowConfig::new("secondary"));
        reg.take_shell(None)
    }

    fn shell_with_menu() -> ShellConfig {
        WindowRegistry::new().take_shell(Some(MenuBar::build(|mb| {
            mb.menu("File", |m| {
                m.item("Quit", None, || {});
            });
        })))
    }

    #[test]
    fn default_set_shell_accepts_an_empty_shell() {
        let mut runner = InertRunner;
        assert!(runner.set_shell(ShellConfig::default()).is_ok());
    }

    #[test]
    fn default_set_shell_rejects_secondary_windows() {
        let mut runner = InertRunner;
        let err = runner
            .set_shell(shell_with_window())
            .expect_err("a runner that ignores the shell must not accept it");
        assert!(matches!(err, UiError::Unsupported(_)), "got {err:?}");
        assert!(err.to_string().contains("1 secondary window"));
    }

    #[test]
    fn default_set_shell_rejects_a_menu_bar() {
        let mut runner = InertRunner;
        let err = runner
            .set_shell(shell_with_menu())
            .expect_err("a runner that ignores the shell must not accept it");
        assert!(matches!(err, UiError::Unsupported(_)), "got {err:?}");
        assert!(err.to_string().contains("menu bar: true"));
    }

    #[cfg(feature = "egui")]
    #[test]
    fn egui_runner_accepts_windows_and_menu_bar() {
        let mut reg = WindowRegistry::new();
        let id = reg.open_window_with(WindowConfig::new("panel"), |ui| ui.label("panel"));
        let shell = reg.take_shell(Some(MenuBar::build(|mb| {
            mb.menu("File", |m| {
                m.item("Quit", None, || {});
            });
        })));
        let mut runner = EguiRunner::new();
        runner.set_shell(shell).expect("egui consumes the shell");
        assert_eq!(runner.shell.windows.len(), 1);
        assert_eq!(runner.shell.windows[0].id, id);
        assert!(runner.shell.has_menu_bar());
    }

    #[cfg(feature = "iced")]
    #[test]
    fn iced_runner_accepts_a_menu_bar() {
        let mut runner = IcedRunner::new();
        runner
            .set_shell(shell_with_menu())
            .expect("iced renders the menu bar through IcedUiCtx");
        assert!(runner.menu_bar.is_some());
    }

    #[cfg(feature = "iced")]
    #[test]
    fn iced_runner_rejects_secondary_windows() {
        let mut runner = IcedRunner::new();
        let err = runner
            .set_shell(shell_with_window())
            .expect_err("iced 0.14 single-window runtime cannot open them");
        assert!(matches!(err, UiError::Unsupported(_)), "got {err:?}");
        assert!(runner.menu_bar.is_none());
    }
}
