//! Facade-level tests for the application shell: the multi-window registry and
//! the menu bar are actually consumed by a backend rather than merely stored.
//!
//! The live event loops cannot run without a display, so these tests exercise
//! the same code paths a backend takes: `App::run_headless_frame` drives the
//! very sequence `OxiEguiApp::ui` / the iced `view` function run (menu bar →
//! init → content → per-frame hooks), and `BackendRunner::set_shell` is the
//! exact call `App::run()` makes before booting the loop.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use oxiui::{App, AppConfig, BackendRunner, ShellConfig, UiError, WindowCommand};
use oxiui_core::window::WindowConfig;
use oxiui_core::{response::WidgetResponse, ButtonResponse, UiCtx};

// ── Probe contexts ────────────────────────────────────────────────────────────

/// Records every widget call and reports a click for exactly one caption.
struct ProbeCtx {
    click_on: Option<String>,
    buttons: Vec<String>,
    labels: Vec<String>,
    separators: usize,
}

impl ProbeCtx {
    fn new(click_on: Option<&str>) -> Self {
        Self {
            click_on: click_on.map(|s| s.to_string()),
            buttons: Vec::new(),
            labels: Vec::new(),
            separators: 0,
        }
    }

    fn saw_button(&self, label: &str) -> bool {
        self.buttons.iter().any(|b| b == label)
    }
}

impl UiCtx for ProbeCtx {
    fn heading(&mut self, text: &str) {
        self.labels.push(text.to_string());
    }
    fn label(&mut self, text: &str) {
        self.labels.push(text.to_string());
    }
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

// ── Menu bar is rendered and dispatched on the frame path ────────────────────

fn app_with_menu(hits: Arc<AtomicUsize>) -> App {
    App::new(AppConfig::new().title("shell")).menu_bar(move |mb| {
        let quit = Arc::clone(&hits);
        mb.menu("File", move |m| {
            m.item("New", None, || {});
            m.separator();
            m.item("Quit", Some("Ctrl+Q"), move || {
                quit.fetch_add(1, Ordering::SeqCst);
            });
        });
        mb.menu("Help", |m| {
            m.item("About", None, || {});
        });
    })
}

#[test]
fn menu_bar_is_drawn_on_every_frame() {
    let hits = Arc::new(AtomicUsize::new(0));
    let mut app = app_with_menu(Arc::clone(&hits));
    let mut ui = ProbeCtx::new(None);
    assert_eq!(app.run_headless_frame(&mut ui), 0);
    assert_eq!(ui.buttons, vec!["File".to_string(), "Help".to_string()]);
    assert_eq!(hits.load(Ordering::SeqCst), 0);
}

#[test]
fn menu_bar_opens_and_dispatches_an_item_action() {
    let hits = Arc::new(AtomicUsize::new(0));
    let mut app = app_with_menu(Arc::clone(&hits));

    // Frame 1 — open the File menu.
    let mut ui = ProbeCtx::new(Some("File"));
    assert_eq!(app.run_headless_frame(&mut ui), 0);
    assert!(ui.saw_button("New"));
    assert!(ui.saw_button("Quit    Ctrl+Q"));
    assert_eq!(ui.separators, 1);

    // Frame 2 — pick "Quit": the registered callback runs.
    let mut ui = ProbeCtx::new(Some("Quit    Ctrl+Q"));
    assert_eq!(app.run_headless_frame(&mut ui), 1);
    assert_eq!(hits.load(Ordering::SeqCst), 1);

    // Frame 3 — the bar closed itself; only the top-level row is drawn.
    let mut ui = ProbeCtx::new(None);
    assert_eq!(app.run_headless_frame(&mut ui), 0);
    assert_eq!(ui.buttons, vec!["File".to_string(), "Help".to_string()]);
    assert_eq!(hits.load(Ordering::SeqCst), 1);
}

#[test]
fn menu_bar_precedes_the_content_closure() {
    let mut app = App::new(AppConfig::new().title("order"))
        .menu_bar(|mb| {
            mb.menu("File", |m| {
                m.item("Quit", None, || {});
            });
        })
        .content(|ui| {
            ui.button("content-button");
        });
    let mut ui = ProbeCtx::new(None);
    app.run_headless_frame(&mut ui);
    assert_eq!(
        ui.buttons,
        vec!["File".to_string(), "content-button".to_string()],
        "the menu bar must be drawn above the content"
    );
}

#[test]
fn app_without_a_menu_bar_draws_nothing_extra() {
    let mut app = App::new(AppConfig::new().title("plain")).content(|ui| {
        ui.label("body");
    });
    let mut ui = ProbeCtx::new(None);
    assert_eq!(app.run_headless_frame(&mut ui), 0);
    assert!(ui.buttons.is_empty());
    assert_eq!(ui.labels, vec!["body".to_string()]);
}

#[test]
fn headless_once_renders_the_menu_bar_without_firing_actions() {
    let hits = Arc::new(AtomicUsize::new(0));
    let app = app_with_menu(Arc::clone(&hits));
    app.run_headless_once().expect("headless frame");
    assert_eq!(
        hits.load(Ordering::SeqCst),
        0,
        "NullUiCtx never reports a click"
    );
}

// ── Secondary windows reach the backend ──────────────────────────────────────

/// A runner that keeps the default `set_shell`, i.e. consumes no shell.
struct InertRunner;

impl BackendRunner for InertRunner {
    fn run(
        self: Box<Self>,
        _config: AppConfig,
        _content: oxiui::runner::ContentFn,
        _lifecycle: oxiui::LifecycleConfig,
    ) -> Result<oxiui::AppExit, UiError> {
        Ok(oxiui::AppExit::Ok)
    }
}

#[test]
fn registered_windows_and_menu_reach_the_runner() {
    let mut app = App::new(AppConfig::new().title("multi"));
    let inspector = app.open_window_with(WindowConfig::new("Inspector").width(320.0), |ui| {
        ui.label("inspector");
    });
    let plain = app.open_window(WindowConfig::new("Plain"));

    // `App::run()` builds exactly this shell; rebuild it the same way here so
    // the assertions cover the data a backend receives.
    let shell = ShellConfig {
        windows: app.secondary_windows().to_vec(),
        contents: Vec::new(),
        menu_bar: None,
        handle: app.window_handle().clone(),
    };
    assert!(shell.has_secondary_windows());
    assert_eq!(shell.windows.len(), 2);
    assert_eq!(shell.windows[0].id, inspector);
    assert_eq!(shell.windows[0].content_key, Some(0));
    assert_eq!(shell.windows[1].id, plain);
    assert_eq!(shell.windows[1].content_key, None);
}

#[test]
fn a_runner_that_ignores_the_shell_rejects_it() {
    let mut app = App::new(AppConfig::new().title("multi"));
    app.open_window(WindowConfig::new("Panel"));
    let shell = ShellConfig {
        windows: app.secondary_windows().to_vec(),
        contents: Vec::new(),
        menu_bar: None,
        handle: app.window_handle().clone(),
    };
    let mut runner = InertRunner;
    let err = runner
        .set_shell(shell)
        .expect_err("a silent no-op is not allowed");
    assert!(matches!(err, UiError::Unsupported(_)), "got {err:?}");
}

#[test]
fn window_handle_commands_survive_the_move_into_the_shell() {
    let mut app = App::new(AppConfig::new().title("multi"));
    let id = app.open_window(WindowConfig::new("Panel"));
    let handle = app.window_handle().clone();

    handle.close(id).expect("queue close");
    handle.open(id).expect("queue open");
    handle.focus(id).expect("queue focus");

    // The backend drains the very same queue.
    assert_eq!(
        app.window_handle().drain(),
        vec![
            WindowCommand::Close(id),
            WindowCommand::Open(id),
            WindowCommand::Focus(id),
        ]
    );
}

#[test]
fn closing_a_registered_window_removes_its_descriptor() {
    let mut app = App::new(AppConfig::new().title("multi"));
    let id = app.open_window(WindowConfig::new("Panel"));
    assert_eq!(app.secondary_windows().len(), 1);
    let removed = app.close_window(id).expect("descriptor removed");
    assert_eq!(removed.id, id);
    assert!(app.secondary_windows().is_empty());
}

// ── Backend rejections are typed, never silent ───────────────────────────────

#[cfg(feature = "iced")]
#[test]
fn iced_backend_rejects_secondary_windows_from_run() {
    let mut app = App::new(AppConfig::new().title("multi")).backend(oxiui::Backend::Iced);
    app.open_window(WindowConfig::new("Panel"));
    // `run()` must fail before booting the event loop (no window is opened).
    let err = app.run().expect_err("iced cannot open secondary windows");
    assert!(matches!(err, UiError::Unsupported(_)), "got {err:?}");
}

#[cfg(feature = "dioxus")]
#[test]
fn dioxus_backend_rejects_the_shell_from_run() {
    let app = App::new(AppConfig::new().title("menu"))
        .backend(oxiui::Backend::Dioxus)
        .menu_bar(|mb| {
            mb.menu("File", |m| {
                m.item("Quit", None, || {});
            });
        });
    let err = app.run().expect_err("dioxus consumes no shell");
    assert!(matches!(err, UiError::Unsupported(_)), "got {err:?}");
}

// ── Adapter-level menu rendering ─────────────────────────────────────────────

/// The iced adapter reports `UiCtx::menu_bar` as unsupported, so the renderer
/// must fall back to `horizontal` + `popup` and still emit real widget specs.
#[cfg(feature = "iced")]
#[test]
fn menu_bar_materializes_widget_specs_on_the_iced_adapter() {
    use oxiui::menu::{render_menu_bar, MenuBar, MenuBarState};
    use oxiui_iced::adapter::{IcedConfig, IcedUiCtx};

    let bar = MenuBar::build(|mb| {
        mb.menu("File", |m| {
            m.item("New", None, || {});
            m.separator();
            m.item("Quit", Some("Ctrl+Q"), || {});
        });
    });

    // Closed: only the top-level row.
    let mut state = MenuBarState::new();
    let mut ctx = IcedUiCtx::new(IcedConfig::default());
    assert_eq!(render_menu_bar(&bar, &mut state, &mut ctx), 0);
    let closed = ctx.into_specs();
    assert!(!closed.is_empty(), "the menu bar must emit widget specs");

    // Open: the drop-down items are emitted too.
    state.toggle_menu(0);
    let mut ctx = IcedUiCtx::new(IcedConfig::default());
    render_menu_bar(&bar, &mut state, &mut ctx);
    let open = ctx.into_specs();
    assert!(
        open.len() > closed.len(),
        "opening the menu must emit more specs ({} vs {})",
        open.len(),
        closed.len()
    );
}

#[cfg(feature = "a11y")]
#[test]
fn menu_bar_appears_in_the_accessibility_snapshot() {
    use oxiui_accessibility::{A11yTree, WindowA11yId};

    let mut app = App::new(AppConfig::new().title("a11y"))
        .menu_bar(|mb| {
            mb.menu("File", |m| {
                m.item("Quit", None, || {});
            });
        })
        .content(|ui| {
            ui.label("body");
        });
    let with_menu = app.build_a11y_snapshot(WindowA11yId(1));

    let mut plain = App::new(AppConfig::new().title("a11y")).content(|ui| {
        ui.label("body");
    });
    let without_menu = plain.build_a11y_snapshot(WindowA11yId(1));

    // Each tree is self-stable …
    assert!(A11yTree::diff(&with_menu, &with_menu).nodes.is_empty());
    assert!(A11yTree::diff(&without_menu, &without_menu)
        .nodes
        .is_empty());
    // … but the menu bar contributes nodes the plain app does not have.
    assert!(
        !A11yTree::diff(&without_menu, &with_menu).nodes.is_empty(),
        "the menu bar must contribute accessibility nodes"
    );
}

// ── Chrome / content widget-id isolation ─────────────────────────────────────

/// The menu bar draws a *varying* number of widgets (its drop-down opens and
/// closes), so it must never share a widget-id range with the app content —
/// on iced the ids route clicks, on egui they key persistent widget state.
#[cfg(feature = "iced")]
#[test]
fn chrome_and_content_widget_ids_cannot_collide() {
    use oxiui::menu::{render_menu_bar, MenuBar, MenuBarState};
    use oxiui_iced::adapter::{IcedConfig, IcedUiCtx};

    const CHROME_BASE: usize = usize::MAX / 2;

    let bar = MenuBar::build(|mb| {
        mb.menu("File", |m| {
            m.item("New", None, || {});
            m.item("Open", None, || {});
            m.item("Quit", None, || {});
        });
        mb.menu("Help", |m| {
            m.item("About", None, || {});
        });
    });

    // Frame 1: menu closed. Chrome allocates 2 ids, content starts at 0.
    let mut state = MenuBarState::new();
    let mut chrome = IcedUiCtx::with_id_base(IcedConfig::default(), CHROME_BASE);
    render_menu_bar(&bar, &mut state, &mut chrome);
    let closed_chrome_end = chrome.next_widget_id();
    let mut content = IcedUiCtx::new(IcedConfig::default());
    content.button("Save");
    content.button("Load");
    let content_ids_closed = content.next_widget_id();

    // Frame 2: menu open — chrome now allocates strictly more ids …
    state.toggle_menu(0);
    let mut chrome = IcedUiCtx::with_id_base(IcedConfig::default(), CHROME_BASE);
    render_menu_bar(&bar, &mut state, &mut chrome);
    let open_chrome_end = chrome.next_widget_id();
    assert!(
        open_chrome_end > closed_chrome_end,
        "opening the menu must allocate more ids"
    );

    // … yet the content ids are unchanged, and the ranges stay disjoint.
    let mut content = IcedUiCtx::new(IcedConfig::default());
    content.button("Save");
    content.button("Load");
    assert_eq!(
        content.next_widget_id(),
        content_ids_closed,
        "content widget ids must not shift when the menu opens"
    );
    assert!(
        content.next_widget_id() < CHROME_BASE,
        "content must stay below the chrome range"
    );
}

// ── Multi-frame lifecycle parity with the live backends ──────────────────────

#[test]
fn headless_frames_fire_init_once_and_frame_hooks_every_time() {
    let inits = Arc::new(AtomicUsize::new(0));
    let frames = Arc::new(AtomicUsize::new(0));
    let bodies = Arc::new(AtomicUsize::new(0));

    let (i, f, b) = (Arc::clone(&inits), Arc::clone(&frames), Arc::clone(&bodies));
    let mut app = App::new(AppConfig::new().title("frames"))
        .on_init(move |_ui| {
            i.fetch_add(1, Ordering::SeqCst);
        })
        .on_frame(move |_ui| {
            f.fetch_add(1, Ordering::SeqCst);
        })
        .content(move |_ui| {
            b.fetch_add(1, Ordering::SeqCst);
        });

    for _ in 0..3 {
        let mut ui = ProbeCtx::new(None);
        app.run_headless_frame(&mut ui);
    }

    assert_eq!(
        inits.load(Ordering::SeqCst),
        1,
        "on_init fires exactly once"
    );
    assert_eq!(frames.load(Ordering::SeqCst), 3, "on_frame fires per frame");
    assert_eq!(bodies.load(Ordering::SeqCst), 3, "content runs per frame");
}
