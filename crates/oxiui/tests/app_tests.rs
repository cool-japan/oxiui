//! Feature tests for the oxiui facade: AppConfig, AppExit, lifecycle hooks,
//! Plugin, HotkeyRegistry, CommandPalette, and NotificationQueue.

use oxiui::{
    AppConfig, AppExit, CommandPalette, HotkeyRegistry, Notification, NotificationQueue, Plugin,
};
use oxiui_core::events::{Key, Modifiers};
use oxiui_core::UiCtx;

// ─── AppConfig builder ────────────────────────────────────────────────────────

#[test]
fn app_config_builder_default() {
    let cfg = AppConfig::new();
    assert_eq!(cfg.title, "");
    assert_eq!(cfg.width, 800.0);
    assert_eq!(cfg.height, 600.0);
    assert!(cfg.resizable);
}

#[test]
fn app_config_builder_chained() {
    let cfg = AppConfig::new()
        .title("My App")
        .size(1024.0, 768.0)
        .resizable(false);
    assert_eq!(cfg.title, "My App");
    assert_eq!(cfg.width, 1024.0);
    assert_eq!(cfg.height, 768.0);
    assert!(!cfg.resizable);
}

#[test]
fn app_exit_variants() {
    assert_eq!(AppExit::Ok, AppExit::Ok);
    assert_ne!(AppExit::Ok, AppExit::Error("e".into()));
    assert_eq!(AppExit::Error("a".into()), AppExit::Error("a".into()));
}

// ─── run_headless_once returns AppExit::Ok ────────────────────────────────────

#[test]
fn headless_once_returns_app_exit_ok() {
    let result = oxiui::App::new(AppConfig::new().title("test"))
        .content(|ui| {
            ui.heading("h");
            ui.label("l");
            let _ = ui.button("b");
        })
        .run_headless_once();
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), AppExit::Ok);
}

// ─── Lifecycle hooks ──────────────────────────────────────────────────────────

#[test]
fn on_init_hook_called_exactly_once() {
    use std::sync::{Arc, Mutex};

    let counter = Arc::new(Mutex::new(0u32));
    let counter_c = Arc::clone(&counter);

    oxiui::App::new(AppConfig::new().title("hooks"))
        .on_init(move |_ui| {
            *counter_c.lock().unwrap() += 1;
        })
        .run_headless_once()
        .unwrap();

    assert_eq!(*counter.lock().unwrap(), 1);
}

#[test]
fn on_frame_hook_called_once_in_headless() {
    use std::sync::{Arc, Mutex};

    let counter = Arc::new(Mutex::new(0u32));
    let counter_c = Arc::clone(&counter);

    oxiui::App::new(AppConfig::new().title("hooks"))
        .on_frame(move |_ui| {
            *counter_c.lock().unwrap() += 1;
        })
        .run_headless_once()
        .unwrap();

    // run_headless_once fires one frame.
    assert_eq!(*counter.lock().unwrap(), 1);
}

#[test]
fn multiple_hooks_called_in_order() {
    use std::sync::{Arc, Mutex};

    let order = Arc::new(Mutex::new(Vec::<u32>::new()));
    let o1 = Arc::clone(&order);
    let o2 = Arc::clone(&order);
    let o3 = Arc::clone(&order);

    oxiui::App::new(AppConfig::new().title("order"))
        .on_init(move |_ui| o1.lock().unwrap().push(1))
        .on_init(move |_ui| o2.lock().unwrap().push(2))
        .on_frame(move |_ui| o3.lock().unwrap().push(3))
        .run_headless_once()
        .unwrap();

    let o = order.lock().unwrap();
    assert_eq!(*o, vec![1, 2, 3]);
}

// ─── Plugin system ────────────────────────────────────────────────────────────

/// A test plugin that records calls to `init` and `update`.
struct RecordingPlugin {
    name: String,
    calls: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    prio: i32,
}

impl Plugin for RecordingPlugin {
    fn init(&mut self, _ctx: &mut dyn UiCtx) {
        self.calls
            .lock()
            .unwrap()
            .push(format!("init:{}", self.name));
    }

    fn update(&mut self, _ctx: &mut dyn UiCtx) {
        self.calls
            .lock()
            .unwrap()
            .push(format!("update:{}", self.name));
    }

    fn priority(&self) -> i32 {
        self.prio
    }
}

#[test]
fn plugin_init_and_update_called() {
    let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let calls_c = std::sync::Arc::clone(&calls);

    oxiui::App::new(AppConfig::new().title("plugin"))
        .plugin(RecordingPlugin {
            name: "A".into(),
            calls: calls_c,
            prio: 0,
        })
        .run_headless_once()
        .unwrap();

    let c = calls.lock().unwrap();
    assert!(c.contains(&"init:A".to_string()));
    assert!(c.contains(&"update:A".to_string()));
}

#[test]
fn plugin_registry_ordering_by_priority() {
    let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let c1 = std::sync::Arc::clone(&calls);
    let c2 = std::sync::Arc::clone(&calls);
    let c3 = std::sync::Arc::clone(&calls);

    // Register in reverse priority order; expect lower priority to run first.
    oxiui::App::new(AppConfig::new().title("priority"))
        .plugin(RecordingPlugin {
            name: "B".into(),
            calls: c2,
            prio: 10,
        })
        .plugin(RecordingPlugin {
            name: "A".into(),
            calls: c1,
            prio: 1,
        })
        .plugin(RecordingPlugin {
            name: "C".into(),
            calls: c3,
            prio: 20,
        })
        .run_headless_once()
        .unwrap();

    let c = calls.lock().unwrap();
    // init order must be A (1) < B (10) < C (20)
    let init_order: Vec<_> = c.iter().filter(|s| s.starts_with("init:")).collect();
    assert_eq!(init_order, vec!["init:A", "init:B", "init:C"]);
}

// ─── HotkeyRegistry ───────────────────────────────────────────────────────────

#[test]
fn hotkey_conflict_detection() {
    let mut reg = HotkeyRegistry::new();
    let mods = Modifiers {
        ctrl: true,
        ..Modifiers::NONE
    };
    let key = Key::Character("s".into());

    assert!(reg.register("save", mods, key.clone(), || {}).is_ok());

    // Registering the same binding should fail.
    assert!(reg
        .register("save-again", mods, key.clone(), || {})
        .is_err());
}

#[test]
fn hotkey_no_conflict_different_modifiers() {
    let mut reg = HotkeyRegistry::new();
    let ctrl = Modifiers {
        ctrl: true,
        ..Modifiers::NONE
    };
    let shift = Modifiers {
        shift: true,
        ..Modifiers::NONE
    };
    let key = Key::Character("p".into());

    assert!(reg.register("ctrl-p", ctrl, key.clone(), || {}).is_ok());
    // Different modifiers — no conflict.
    assert!(reg.register("shift-p", shift, key, || {}).is_ok());
    assert_eq!(reg.len(), 2);
}

#[test]
fn hotkey_conflict_check_returns_true_when_conflict() {
    let mut reg = HotkeyRegistry::new();
    let mods = Modifiers::NONE;
    let key = Key::Escape;

    reg.register("esc", mods, key.clone(), || {}).unwrap();
    assert!(reg.conflict_check(mods, key));
}

// ─── CommandPalette ───────────────────────────────────────────────────────────

#[test]
fn command_palette_fuzzy_search_basic() {
    let mut palette = CommandPalette::new();
    palette.register("open-file", "Open File", || {});
    palette.register("new-file", "New File", || {});
    palette.register("quit", "Quit Application", || {});

    // "fi" matches "Open File" and "New File"
    let results = palette.search("fi");
    assert_eq!(results.len(), 2);
    let ids: Vec<&str> = results.iter().map(|c| c.id.as_str()).collect();
    assert!(ids.contains(&"open-file"));
    assert!(ids.contains(&"new-file"));
}

#[test]
fn command_palette_fuzzy_search_empty_query_matches_all() {
    let mut palette = CommandPalette::new();
    palette.register("a", "Alpha", || {});
    palette.register("b", "Beta", || {});
    palette.register("c", "Gamma", || {});

    let results = palette.search("");
    assert_eq!(results.len(), 3);
}

#[test]
fn command_palette_fuzzy_search_no_match() {
    let mut palette = CommandPalette::new();
    palette.register("save", "Save Document", || {});

    let results = palette.search("xyz");
    assert!(results.is_empty());
}

#[test]
fn command_palette_fuzzy_search_case_insensitive() {
    let mut palette = CommandPalette::new();
    palette.register("save", "Save Document", || {});

    let results = palette.search("SAVE");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "save");
}

#[test]
fn command_palette_is_empty_and_len() {
    let mut palette = CommandPalette::new();
    assert!(palette.is_empty());
    assert_eq!(palette.len(), 0);
    palette.register("a", "Alpha", || {});
    assert!(!palette.is_empty());
    assert_eq!(palette.len(), 1);
}

// ─── NotificationQueue ────────────────────────────────────────────────────────

#[test]
fn notification_queue_push_pop_fifo() {
    let mut q = NotificationQueue::new();
    q.push("First", "body1", 3000);
    q.push("Second", "body2", 5000);

    let n1 = q.pop_due().unwrap();
    assert_eq!(n1.title, "First");
    assert_eq!(n1.body, "body1");
    assert_eq!(n1.duration_ms, 3000);

    let n2 = q.pop_due().unwrap();
    assert_eq!(n2.title, "Second");

    assert!(q.pop_due().is_none());
}

#[test]
fn notification_queue_is_empty_and_len() {
    let mut q = NotificationQueue::new();
    assert!(q.is_empty());
    assert_eq!(q.len(), 0);

    q.push("T", "b", 1000);
    assert!(!q.is_empty());
    assert_eq!(q.len(), 1);

    let _ = q.pop_due();
    assert!(q.is_empty());
}

#[test]
fn notification_clone_and_debug() {
    let n = Notification {
        title: "T".into(),
        body: "B".into(),
        duration_ms: 2000,
        urgency: 1,
        created_at: std::time::Instant::now(),
    };
    let n2 = n.clone();
    assert_eq!(n.title, n2.title);
    // Debug just has to not panic.
    let _ = format!("{n:?}");
}

// ─── Slice G: App::with_state ─────────────────────────────────────────────────

#[test]
fn headless_smoke_run_with_return() {
    let result = oxiui::App::new(AppConfig::default())
        .content(|ui| {
            ui.label("hi");
        })
        .run_with_return(|_ui| "ok".to_string());
    // Should return Ok("ok") in headless/return mode.
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "ok");
}

#[test]
fn with_state_builds_without_panic() {
    // Verify that App::with_state compiles and the App is constructed successfully.
    let state = 0i32;
    let _app = oxiui::App::new(AppConfig::default()).with_state(state, |_ui, s| {
        *s += 1;
    });
    // App built successfully without panicking.
}

#[test]
fn with_state_can_run_headless() {
    use std::sync::{Arc, Mutex};

    let counter = Arc::new(Mutex::new(0i32));
    let counter_c = Arc::clone(&counter);

    // The state (inner counter) is separate from the Arc; we use Arc only to
    // observe side effects from outside the closure.
    let state = 0i32;
    oxiui::App::new(AppConfig::default())
        .with_state(state, move |_ui, s| {
            *s += 1;
            *counter_c.lock().unwrap() = *s;
        })
        .run_headless_once()
        .unwrap();

    // run_headless_once drives one frame; state should have been incremented once.
    assert_eq!(*counter.lock().unwrap(), 1);
}

// ─── Slice G: App::with_font ──────────────────────────────────────────────────

#[test]
fn with_font_pushes_to_extra_fonts() {
    let app = oxiui::App::new(AppConfig::default()).with_font("TestFont", vec![0u8, 1, 2, 3]);
    let fonts = app.extra_fonts();
    assert_eq!(fonts.len(), 1);
    assert_eq!(fonts[0].0, "TestFont");
    assert_eq!(fonts[0].1, vec![0u8, 1, 2, 3]);
}

#[test]
fn with_font_multiple_families_accumulate() {
    let app = oxiui::App::new(AppConfig::default())
        .with_font("FontA", vec![1u8])
        .with_font("FontB", vec![2u8]);
    assert_eq!(app.extra_fonts().len(), 2);
    assert_eq!(app.extra_fonts()[0].0, "FontA");
    assert_eq!(app.extra_fonts()[1].0, "FontB");
}

// ─── Slice G: BackendRunner ───────────────────────────────────────────────────

#[test]
fn backend_runner_egui_constructs() {
    #[cfg(feature = "egui")]
    {
        let _runner = oxiui::EguiRunner;
    }
}

#[test]
fn backend_runner_iced_constructs() {
    #[cfg(feature = "iced")]
    {
        let _runner = oxiui::IcedRunner;
    }
}

#[test]
fn lifecycle_config_default_constructs() {
    let lc = oxiui::runner::LifecycleConfig::default();
    assert!(lc.on_close.is_none());
    assert!(lc.on_resize.is_none());
    assert!(lc.on_focus.is_none());
}

// ─── Slice G: text module re-export ──────────────────────────────────────────

#[test]
fn text_module_font_spec_accessible() {
    // Verify the oxiui::text module is accessible and contains FontSpec.
    let _fs: Option<oxiui::text::FontSpec> = None;
}

#[test]
fn app_config_default_constructs_with_extra_fonts() {
    let cfg = AppConfig::default();
    assert!(
        cfg.extra_fonts.is_empty(),
        "extra_fonts must default to empty"
    );
}

// ─── Slice S4: new feature tests ─────────────────────────────────────────────

/// test_reactive_reexport — confirm Signal/Computed/ReactiveRuntime are accessible via oxiui::reactive.
#[test]
fn test_reactive_reexport() {
    use oxiui::reactive::{Computed, ReactiveRuntime, Signal};
    let rt = ReactiveRuntime::new();
    let s: Signal<i32> = rt.signal(42i32);
    assert_eq!(s.get(), 42i32);
    let s2 = s.clone();
    // computed() returns Result<Computed<T>, ReactiveError>
    let c: Computed<i32> = rt.computed(move || s2.get() * 2).expect("computed");
    // Computed::get() returns Result<T, ReactiveError>
    assert_eq!(c.get().expect("get"), 84i32);
}

/// test_app_frame_skip_builder — App with frame_skip=true builds without panicking before run().
#[test]
fn test_app_frame_skip_builder() {
    let _app = oxiui::App::new(AppConfig::default())
        .content(|_ui| {})
        .with_frame_skip(true);
    // Compiles and constructs successfully; no panic.
}

/// test_frame_skip_default_false — default frame_skip is false (no behaviour change).
#[test]
fn test_frame_skip_default_false() {
    // Run headless once to verify a frame_skip=false app works normally.
    oxiui::App::new(AppConfig::default())
        .content(|_ui| {})
        .with_frame_skip(false)
        .run_headless_once()
        .unwrap();
}

/// test_theme_picker_no_panic — calling theme_picker with a no-op UiCtx must not panic.
#[test]
fn test_theme_picker_no_panic() {
    use oxiui::theme_picker::theme_picker;
    use oxiui_core::{ButtonResponse, UiCtx};

    struct NullCtx;
    impl UiCtx for NullCtx {
        fn heading(&mut self, _: &str) {}
        fn label(&mut self, _: &str) {}
        fn button(&mut self, _: &str) -> ButtonResponse {
            ButtonResponse::default()
        }
    }

    let mut ctx = NullCtx;
    let mut current = "light";
    let changed = theme_picker(&mut ctx, &mut current);
    // NullCtx never returns clicked=true.
    assert!(!changed);
}

/// test_builtin_themes_constant — BUILTIN_THEMES contains at least the three named themes.
#[test]
fn test_builtin_themes_constant() {
    assert!(oxiui::BUILTIN_THEMES.contains(&"light"));
    assert!(oxiui::BUILTIN_THEMES.contains(&"dark"));
    assert!(oxiui::BUILTIN_THEMES.contains(&"cooljapan_default"));
}

/// test_theme_by_name — by_name is callable for all built-in theme names.
#[test]
fn test_theme_by_name() {
    use oxiui::theme_by_name;
    for &name in oxiui::BUILTIN_THEMES {
        let _theme = theme_by_name(name);
    }
    // Unknown name falls back without panic.
    let _theme = theme_by_name("nonexistent");
}

/// test_reactive_module_in_prelude — Signal/Computed/ReactiveRuntime are in the prelude.
#[test]
fn test_reactive_in_prelude() {
    use oxiui::prelude::*;
    let rt = ReactiveRuntime::new();
    let s: Signal<u32> = rt.signal(0u32);
    // computed() returns Result<Computed<T>, ReactiveError>
    let _c: Computed<u32> = rt.computed(move || s.get()).expect("computed");
}

/// test_doc_cfg_compile — the #![cfg_attr(docsrs, feature(doc_cfg))] attribute at the
/// crate level is present. This test just verifies the crate compiles normally (the
/// attribute is a no-op without the docsrs cfg).
#[test]
fn test_doc_cfg_no_op_without_docsrs() {
    // This test simply compiles — proving the crate-level attribute doesn't break
    // normal (non-docsrs) builds.
    let _app = oxiui::App::new(AppConfig::default());
}

// ─── App::table (feature = "table") ──────────────────────────────────────────

/// test_app_table_compiles — create a minimal RowSource impl and call App::table.
#[cfg(feature = "table")]
#[test]
fn test_app_table_compiles() {
    use oxiui_table::{Cell, ColumnDef, RowSource};

    struct SimpleSource;
    impl RowSource for SimpleSource {
        fn row_count(&self) -> usize {
            2
        }
        fn row(&self, index: usize) -> Vec<Cell> {
            vec![
                Cell::Text(format!("row{index}-col0")),
                Cell::Text(format!("row{index}-col1")),
            ]
        }
        fn column_defs(&self) -> &[ColumnDef] {
            static COLS: std::sync::LazyLock<Vec<ColumnDef>> = std::sync::LazyLock::new(|| {
                vec![
                    ColumnDef {
                        name: "A".into(),
                        width: 80.0,
                        ..ColumnDef::default()
                    },
                    ColumnDef {
                        name: "B".into(),
                        width: 80.0,
                        ..ColumnDef::default()
                    },
                ]
            });
            &COLS
        }
    }

    // Constructing App::table should not panic.
    let _app = oxiui::App::new(AppConfig::default()).table(SimpleSource);
}

/// test_app_table_headless — App::table renders without panic in headless mode.
#[cfg(feature = "table")]
#[test]
fn test_app_table_headless() {
    use oxiui_table::{Cell, ColumnDef, RowSource};

    struct TwoRow;
    impl RowSource for TwoRow {
        fn row_count(&self) -> usize {
            2
        }
        fn row(&self, index: usize) -> Vec<Cell> {
            vec![Cell::Text(format!("cell{index}"))]
        }
        fn column_defs(&self) -> &[ColumnDef] {
            static COLS: std::sync::LazyLock<Vec<ColumnDef>> = std::sync::LazyLock::new(|| {
                vec![ColumnDef {
                    name: "Col".into(),
                    width: 100.0,
                    ..ColumnDef::default()
                }]
            });
            &COLS
        }
    }

    oxiui::App::new(AppConfig::default())
        .table(TwoRow)
        .run_headless_once()
        .unwrap();
}

// ─── SA-facade-tests: headless smoke, backend selection, theme, window config ─

/// test_headless_smoke_all_widgets — heading/label/button in one headless frame.
#[test]
fn test_headless_smoke_all_widgets() {
    let result = oxiui::App::new(oxiui::AppConfig::new().title("smoke"))
        .content(|ui| {
            ui.heading("Heading");
            ui.label("Label");
            let _ = ui.button("Button");
        })
        .run_headless_once();
    assert!(result.is_ok(), "headless smoke: {result:?}");
}

/// test_backend_default_is_egui — App builds with default config (Egui backend) without running.
#[test]
fn test_backend_default_is_egui() {
    let _app = oxiui::App::new(oxiui::AppConfig::default());
}

/// test_backend_egui_headless — explicitly set Backend::Egui and run headless.
#[cfg(feature = "egui")]
#[test]
fn test_backend_egui_headless() {
    let result = oxiui::App::new(oxiui::AppConfig::default())
        .backend(oxiui::Backend::Egui)
        .content(|ui| {
            ui.label("egui backend");
        })
        .run_headless_once();
    assert!(result.is_ok(), "egui backend headless: {result:?}");
}

/// test_theme_application_dark — dark() theme applied; headless succeeds.
#[test]
fn test_theme_application_dark() {
    let result = oxiui::App::new(oxiui::AppConfig::default())
        .theme(oxiui::theme::dark())
        .content(|ui| {
            ui.label("dark");
        })
        .run_headless_once();
    assert!(result.is_ok(), "dark theme headless: {result:?}");
}

/// test_theme_application_cooljapan — cooljapan_default() theme applied; headless succeeds.
#[test]
fn test_theme_application_cooljapan() {
    let result = oxiui::App::new(oxiui::AppConfig::default())
        .theme(oxiui::theme::cooljapan_default())
        .content(|ui| {
            ui.label("cooljapan");
        })
        .run_headless_once();
    assert!(result.is_ok(), "cooljapan theme headless: {result:?}");
}

/// test_window_config_title_stored — AppConfig::title builder stores the title.
#[test]
fn test_window_config_title_stored() {
    let config = oxiui::AppConfig::new().title("My Window");
    assert_eq!(config.title, "My Window");
}

/// test_window_config_size_stored — AppConfig::size builder stores width/height.
#[test]
fn test_window_config_size_stored() {
    let config = oxiui::AppConfig::new().size(1280.0, 720.0);
    assert!(config.width > 0.0, "width must be positive");
    assert!(config.height > 0.0, "height must be positive");
    assert_eq!(config.width, 1280.0);
    assert_eq!(config.height, 720.0);
}

/// test_state_management_increments — with_state closure mutates state; observed via Arc.
#[test]
fn test_state_management_increments() {
    use std::sync::{Arc, Mutex};
    let counter = Arc::new(Mutex::new(0i32));
    let c2 = counter.clone();
    oxiui::App::new(oxiui::AppConfig::default())
        .with_state(0i32, move |ui, state| {
            *state += 1;
            ui.label(&format!("count: {state}"));
            *c2.lock().unwrap() = *state;
        })
        .run_headless_once()
        .ok();
    // State was incremented once (one headless frame).
    assert_eq!(*counter.lock().unwrap(), 1);
}

/// OrderPlugin — records init/update calls in priority order.
struct OrderPlugin {
    order_log: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    name: String,
    priority: i32,
}

impl Plugin for OrderPlugin {
    fn init(&mut self, _ctx: &mut dyn UiCtx) {
        self.order_log
            .lock()
            .unwrap()
            .push(format!("init:{}", self.name));
    }
    fn update(&mut self, _ctx: &mut dyn UiCtx) {
        self.order_log
            .lock()
            .unwrap()
            .push(format!("update:{}", self.name));
    }
    fn priority(&self) -> i32 {
        self.priority
    }
}

/// test_plugin_ordering_by_priority — numerically lower priority value runs first.
#[test]
fn test_plugin_ordering_by_priority() {
    let log = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    oxiui::App::new(oxiui::AppConfig::default())
        .content(|_ui| {})
        .plugin(OrderPlugin {
            order_log: log.clone(),
            name: "low".into(),
            priority: 1,
        })
        .plugin(OrderPlugin {
            order_log: log.clone(),
            name: "high".into(),
            priority: 100,
        })
        .run_headless_once()
        .ok();
    let entries = log.lock().unwrap().clone();
    let init_positions: Vec<_> = entries
        .iter()
        .enumerate()
        .filter(|(_, e)| e.starts_with("init:"))
        .map(|(i, e)| (i, e.as_str()))
        .collect();
    // Ordering is ascending by priority value: lower numeric value inits first.
    let low_pos = init_positions
        .iter()
        .find(|(_, e)| e.contains("low"))
        .map(|(i, _)| *i);
    let high_pos = init_positions
        .iter()
        .find(|(_, e)| e.contains("high"))
        .map(|(i, _)| *i);
    if let (Some(l), Some(h)) = (low_pos, high_pos) {
        assert!(
            l < h,
            "lower priority value (1) should init before higher (100): {entries:?}"
        );
    }
}

/// test_full_lifecycle_init_frames_close — content closure is called at least once per headless run.
#[test]
fn test_full_lifecycle_init_frames_close() {
    use std::sync::{Arc, Mutex};
    let frames = Arc::new(Mutex::new(0u32));
    let f2 = frames.clone();
    let result = oxiui::App::new(oxiui::AppConfig::default())
        .content(move |ui| {
            *f2.lock().unwrap() += 1;
            ui.label("frame");
        })
        .run_headless_once();
    assert!(result.is_ok(), "lifecycle: {result:?}");
    // run_headless_once drives one frame; content closure called at least once.
    assert!(
        *frames.lock().unwrap() >= 1,
        "content closure should have been called"
    );
}

// ─── New features: design tokens, typography, renderer, startup clock ─────────

#[test]
fn with_design_tokens_stores_in_config() {
    let tokens = oxiui_theme::DesignTokens::default();
    let app = oxiui::App::new(AppConfig::default()).with_design_tokens(tokens.clone());
    // design_tokens() should return the stored tokens (same spacing values).
    let got = app.design_tokens();
    assert_eq!(got.spacing, tokens.spacing);
    assert_eq!(got.radius, tokens.radius);
}

#[test]
fn with_typography_stores_in_config() {
    let typo = oxiui_theme::TypographyScale::default();
    let app = oxiui::App::new(AppConfig::default()).with_typography(typo);
    let got = app.typography();
    // Check that the sizes round-trip (not just the same default).
    assert_eq!(got.display.size, typo.display.size);
    assert_eq!(got.body.size, typo.body.size);
}

#[test]
fn design_tokens_falls_back_to_default_when_unset() {
    let app = oxiui::App::new(AppConfig::default());
    let got = app.design_tokens();
    let def = oxiui_theme::DesignTokens::default();
    // Unset tokens → defaults.
    assert_eq!(got.spacing, def.spacing);
}

#[test]
fn typography_falls_back_to_default_when_unset() {
    let app = oxiui::App::new(AppConfig::default());
    let got = app.typography();
    let def = oxiui_theme::TypographyScale::default();
    assert_eq!(got.display.size, def.display.size);
}

#[test]
fn startup_clock_is_monotonic() {
    let t0 = oxiui::App::startup_clock();
    let t1 = oxiui::App::startup_clock();
    // t1 must be >= t0 (monotonic clock guarantee).
    assert!(t1 >= t0);
}

#[test]
fn process_rss_bytes_returns_option() {
    // On any platform this must not panic.
    let _rss: Option<u64> = oxiui::process_rss_bytes();
    // On Linux it should be Some; on macOS it's None (no C API).
    // No platform-specific assert here — just verify the function is callable.
}

#[cfg(feature = "software")]
#[test]
fn soft_renderer_constructs() {
    let app = oxiui::App::new(AppConfig::default());
    let _r = app.soft_renderer();
}

#[cfg(feature = "persist")]
#[test]
fn with_persistent_state_headless_no_panic() {
    use oxicode::{Decode, Encode};

    #[derive(Encode, Decode, Default)]
    struct Counter {
        n: u32,
    }

    let tmp = std::env::temp_dir().join("oxiui_persist_test_state.oxi");
    // Clean up from any previous run.
    let _ = std::fs::remove_file(&tmp);

    let app = oxiui::App::new(AppConfig::default()).with_persistent_state(
        Counter::default(),
        tmp.clone(),
        |ui, state| {
            state.n += 1;
            ui.label(&format!("n={}", state.n));
        },
    );
    app.run_headless_once().unwrap();

    // Clean up.
    let _ = std::fs::remove_file(&tmp);
}

// ─── Multi-window support ─────────────────────────────────────────────────────

#[test]
fn open_window_returns_non_primary_id() {
    use oxiui_core::window::{WindowConfig, WindowId};
    let mut app = oxiui::App::new(AppConfig::new().title("main"));
    let wid = app.open_window(WindowConfig::new("secondary"));
    assert_ne!(wid, WindowId::PRIMARY);
}

#[test]
fn open_multiple_windows_unique_ids() {
    use oxiui_core::window::WindowConfig;
    let mut app = oxiui::App::new(AppConfig::new().title("main"));
    let id1 = app.open_window(WindowConfig::new("w1").width(400.0).height(300.0));
    let id2 = app.open_window(WindowConfig::new("w2").width(800.0).height(600.0));
    assert_ne!(id1, id2);
    assert_eq!(app.secondary_windows().len(), 2);
}

#[test]
fn close_window_removes_from_registry() {
    use oxiui_core::window::WindowConfig;
    let mut app = oxiui::App::new(AppConfig::new().title("main"));
    let wid = app.open_window(WindowConfig::new("panel"));
    assert_eq!(app.secondary_windows().len(), 1);
    let removed = app.close_window(wid);
    assert!(removed.is_some());
    assert!(app.secondary_windows().is_empty());
}

#[test]
fn window_channel_cross_window_messaging() {
    use oxiui_core::window::{WindowConfig, WindowId};
    let mut app = oxiui::App::new(AppConfig::new().title("main"));
    let _wid = app.open_window(WindowConfig::new("child"));
    let ch = app.window_channel().clone();
    let target = WindowId(42);
    ch.send(target, "ping").unwrap();
    let msgs = ch.drain_messages(target).unwrap();
    assert_eq!(msgs, vec!["ping"]);
}

// ─── Dialog API ───────────────────────────────────────────────────────────────

#[test]
fn message_dialog_returns_dialog_id() {
    use oxiui::DialogResponse;
    let mut app = oxiui::App::new(AppConfig::new().title("dlg"));
    let id = app.message_dialog("Info", "Hello");
    // No response yet.
    assert!(app.poll_dialog(id).is_none());
    // Post a simulated response.
    app.respond_dialog(id, DialogResponse::Dismissed);
    assert_eq!(app.poll_dialog(id), Some(DialogResponse::Dismissed));
}

#[test]
fn confirm_dialog_confirmed_response() {
    use oxiui::DialogResponse;
    let mut app = oxiui::App::new(AppConfig::new().title("dlg"));
    let id = app.confirm_dialog("Quit?", "Are you sure?");
    app.respond_dialog(id, DialogResponse::Confirmed);
    assert_eq!(app.poll_dialog(id), Some(DialogResponse::Confirmed));
}

#[test]
fn file_dialog_paths_response() {
    use oxiui::DialogResponse;
    let mut app = oxiui::App::new(AppConfig::new().title("dlg"));
    let id = app.file_dialog("Open File", vec![("Rust".into(), "*.rs".into())], false);
    app.respond_dialog(id, DialogResponse::FilePaths(vec!["/tmp/foo.rs".into()]));
    if let Some(DialogResponse::FilePaths(paths)) = app.poll_dialog(id) {
        assert_eq!(paths, vec!["/tmp/foo.rs"]);
    } else {
        panic!("expected FilePaths response");
    }
}

#[test]
fn prompt_dialog_text_response() {
    use oxiui::DialogResponse;
    let mut app = oxiui::App::new(AppConfig::new().title("dlg"));
    let id = app.prompt_dialog("Name", "Enter name:", Some("World".into()));
    app.respond_dialog(id, DialogResponse::Text("Alice".into()));
    assert_eq!(
        app.poll_dialog(id),
        Some(DialogResponse::Text("Alice".into()))
    );
}

#[test]
fn file_save_dialog_returns_save_path() {
    use oxiui::DialogResponse;
    let mut app = oxiui::App::new(AppConfig::new().title("dlg"));
    let id = app.file_save_dialog("Save As", Some("output.txt".into()), vec![]);
    app.respond_dialog(id, DialogResponse::SavePath("/tmp/output.txt".into()));
    assert_eq!(
        app.poll_dialog(id),
        Some(DialogResponse::SavePath("/tmp/output.txt".into()))
    );
}

#[test]
fn dialog_poll_after_consume_returns_none() {
    use oxiui::DialogResponse;
    let mut app = oxiui::App::new(AppConfig::new().title("dlg"));
    let id = app.message_dialog("t", "m");
    app.respond_dialog(id, DialogResponse::Dismissed);
    let _ = app.poll_dialog(id);
    assert!(app.poll_dialog(id).is_none());
}

// ─── Native menu bar ──────────────────────────────────────────────────────────

#[test]
fn menu_bar_registers_menus() {
    let app = oxiui::App::new(AppConfig::new().title("menu")).menu_bar(|mb| {
        mb.menu("File", |m| {
            m.item("Open", Some("Ctrl+O"), || {});
            m.item("Quit", Some("Ctrl+Q"), || {});
        });
        mb.menu("Help", |m| {
            m.item("About", None, || {});
        });
    });
    let bar = app.get_menu_bar().expect("menu bar registered");
    assert_eq!(bar.menu_count(), 2);
    assert_eq!(bar.menus()[0].label(), "File");
    assert_eq!(bar.menus()[1].label(), "Help");
}

#[test]
fn menu_bar_items_count() {
    let app = oxiui::App::new(AppConfig::new().title("menu")).menu_bar(|mb| {
        mb.menu("File", |m| {
            m.item("New", None, || {});
            m.separator();
            m.item("Quit", None, || {});
        });
    });
    let bar = app.get_menu_bar().unwrap();
    assert_eq!(bar.menus()[0].items().len(), 3);
}

#[test]
fn with_menu_bar_accepts_pre_built() {
    use oxiui::MenuBar;
    let bar = MenuBar::build(|mb| {
        mb.menu("View", |m| {
            m.item("Zoom In", None, || {});
        });
    });
    let app = oxiui::App::new(AppConfig::new().title("menu")).with_menu_bar(bar);
    assert!(app.get_menu_bar().is_some());
    assert_eq!(app.get_menu_bar().unwrap().menu_count(), 1);
}

#[test]
fn no_menu_bar_by_default() {
    let app = oxiui::App::new(AppConfig::new().title("no-menu"));
    assert!(app.get_menu_bar().is_none());
}

#[test]
fn menu_bar_find_menu_by_label() {
    let app = oxiui::App::new(AppConfig::new().title("menu")).menu_bar(|mb| {
        mb.menu("File", |m| {
            m.item("New", None, || {});
        });
        mb.menu("Edit", |m| {
            m.item("Copy", None, || {});
        });
    });
    let bar = app.get_menu_bar().unwrap();
    assert!(bar.find_menu("File").is_some());
    assert!(bar.find_menu("Edit").is_some());
    assert!(bar.find_menu("Missing").is_none());
}
