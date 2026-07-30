//! iced application state and entry-point (requires `iced` feature).
//!
//! This module provides the `OxiIcedState` struct that threads the user's
//! content closure, lifecycle hooks, and plugins through iced's retained-mode
//! `update`/`view` event loop.  The `run` function boots the iced application.
//!
//! ## Lifecycle wiring
//! Widget interaction and window lifecycle share a single iced message type,
//! [`AppMessage`]. Widget messages flow through `apply_message` unchanged;
//! lifecycle messages ([`LifecycleMsg`]) come from an
//! [`iced::event::listen_with`] subscription and drive the `on_resize` /
//! `on_focus` / `on_close` hooks. Close is handled by the app (rather than iced)
//! so the hooks run before the window is destroyed: the application is booted
//! with `exit_on_close_request(false)` and issues `iced::window::close` itself.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};

use iced::{Element, Subscription, Task};
use oxiui_iced::{apply_message, IcedConfig, IcedUiCtx, Message, WidgetState};

use crate::null_ctx::NullUiCtx;
use crate::runner::{LifecycleEvent, LifecycleSnapshot, LifecycleTracker};
use crate::{ContentFn, HookFn, Plugin};

/// Combined iced message: widget interactions plus window lifecycle events.
#[derive(Debug, Clone)]
pub enum AppMessage {
    /// A widget-level message produced by the `IcedUiCtx` bridge.
    Widget(Message),
    /// A window lifecycle event captured by the subscription.
    Lifecycle(LifecycleMsg),
}

/// Window lifecycle messages captured from the iced event stream.
#[derive(Debug, Clone)]
pub enum LifecycleMsg {
    /// The window was resized to `(width, height)` logical pixels.
    Resized(f32, f32),
    /// The window gained (`true`) or lost (`false`) focus.
    Focus(bool),
    /// The user requested the window (`id`) be closed.
    Close(iced::window::Id),
}

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
    /// Hooks fired once when the window is closing.
    pub on_close: RefCell<Vec<HookFn>>,
    /// Hooks fired when the window size changes.
    pub on_resize: RefCell<Vec<HookFn>>,
    /// Hooks fired when the window focus state flips.
    pub on_focus: RefCell<Vec<HookFn>>,
    /// Deduplicates raw size / focus / close snapshots into fired events.
    pub tracker: RefCell<LifecycleTracker>,
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
            on_close: RefCell::new(Vec::new()),
            on_resize: RefCell::new(Vec::new()),
            on_focus: RefCell::new(Vec::new()),
            tracker: RefCell::new(LifecycleTracker::default()),
        }
    }

    /// Fire the given hook vector against a no-op [`UiCtx`].
    fn fire(&self, hooks: &RefCell<Vec<HookFn>>) {
        let mut null = NullUiCtx;
        if let Ok(mut hooks) = hooks.try_borrow_mut() {
            for hook in hooks.iter_mut() {
                hook(&mut null);
            }
        }
    }
}

/// iced update function — advances widget state, click tracking, and lifecycle.
pub fn update(state: &mut OxiIcedState, msg: AppMessage) -> Task<AppMessage> {
    match msg {
        AppMessage::Widget(m) => {
            let mut clicks = state.pending_clicks.borrow_mut();
            let mut widget_state = state.widget_state.borrow_mut();
            apply_message(&mut widget_state, &mut clicks, &m);
            Task::none()
        }
        AppMessage::Lifecycle(LifecycleMsg::Resized(w, h)) => {
            let events = state.tracker.borrow_mut().observe(LifecycleSnapshot {
                size: Some((w, h)),
                focused: None,
                close_requested: false,
            });
            if events
                .iter()
                .any(|e| matches!(e, LifecycleEvent::Resized(..)))
            {
                state.fire(&state.on_resize);
            }
            Task::none()
        }
        AppMessage::Lifecycle(LifecycleMsg::Focus(f)) => {
            let events = state.tracker.borrow_mut().observe(LifecycleSnapshot {
                size: None,
                focused: Some(f),
                close_requested: false,
            });
            if events.iter().any(|e| matches!(e, LifecycleEvent::Focus(_))) {
                state.fire(&state.on_focus);
            }
            Task::none()
        }
        AppMessage::Lifecycle(LifecycleMsg::Close(id)) => {
            let events = state.tracker.borrow_mut().observe(LifecycleSnapshot {
                size: None,
                focused: None,
                close_requested: true,
            });
            if events.iter().any(|e| matches!(e, LifecycleEvent::Close)) {
                state.fire(&state.on_close);
            }
            // We disabled `exit_on_close_request`, so close the window ourselves
            // now that the on_close hooks have run.
            iced::window::close(id)
        }
    }
}

/// iced view function — drives the content closure through `IcedUiCtx`.
///
/// Also fires init hooks + plugin init on the first frame, and on_frame
/// hooks + plugin update every frame. This mirrors the pattern used by
/// `OxiEguiApp::ui()` (egui path).
pub fn view(state: &OxiIcedState) -> Element<'_, AppMessage> {
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

    // `into_iced_element()` returns `Element<'static, Message>`; tag every
    // widget message as `AppMessage::Widget` so lifecycle messages can coexist.
    let elem: Element<'static, Message> = ctx.into_iced_element();
    elem.map(AppMessage::Widget)
}

/// Map raw iced events to lifecycle messages, discarding everything else.
///
/// `listen_with` takes a plain `fn` pointer (no captured state), so this is a
/// free function. Returning `None` drops the event.
fn lifecycle_filter(
    event: iced::Event,
    _status: iced::event::Status,
    id: iced::window::Id,
) -> Option<AppMessage> {
    use iced::window::Event as WindowEvent;
    match event {
        iced::Event::Window(WindowEvent::Resized(size)) => Some(AppMessage::Lifecycle(
            LifecycleMsg::Resized(size.width, size.height),
        )),
        iced::Event::Window(WindowEvent::Focused) => {
            Some(AppMessage::Lifecycle(LifecycleMsg::Focus(true)))
        }
        iced::Event::Window(WindowEvent::Unfocused) => {
            Some(AppMessage::Lifecycle(LifecycleMsg::Focus(false)))
        }
        iced::Event::Window(WindowEvent::CloseRequested) => {
            Some(AppMessage::Lifecycle(LifecycleMsg::Close(id)))
        }
        _ => None,
    }
}

/// Lifecycle subscription: window resize / focus / close events.
fn subscription(_state: &OxiIcedState) -> Subscription<AppMessage> {
    iced::event::listen_with(lifecycle_filter)
}

/// Run the iced application with the given state and theme.
pub fn run(state: OxiIcedState, iced_theme: iced::Theme, width: f32, height: f32) -> iced::Result {
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

    iced::application(boot, update, view)
        .title(title_fn)
        .theme(theme_fn)
        .subscription(subscription)
        // We fire on_close hooks ourselves, then close the window explicitly.
        .exit_on_close_request(false)
        // Honour the configured initial window size (previously ignored).
        .window_size(iced::Size::new(width, height))
        .run()
}
