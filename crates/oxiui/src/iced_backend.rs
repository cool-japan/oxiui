//! iced application state and entry-point (requires `iced` feature).
//!
//! This module provides the `OxiIcedState` struct that threads the user's
//! content closure, lifecycle hooks, and plugins through iced's retained-mode
//! `update`/`view` event loop.  The `run` function boots the iced application.

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
    let _ = width;
    let _ = height;

    iced::application(boot, update, view)
        .title(title_fn)
        .theme(theme_fn)
        .run()
}
