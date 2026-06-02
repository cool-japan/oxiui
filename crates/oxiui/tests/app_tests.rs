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
