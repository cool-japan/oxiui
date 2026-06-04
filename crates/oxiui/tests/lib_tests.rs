/// Unit tests originally in `src/lib.rs` — extracted to keep lib.rs under 2000 lines.
///
/// These tests exercise the inline (non-integration) paths: builder API, notification
/// queue, hotkey conflict detection, command palette fuzzy matching, screenshot, and the
/// plugin + lifecycle hooks.
use oxiui::{App, AppConfig, AppExit, HotkeyConflict, Plugin, UiCtx};
use oxiui_core::events::{Key, Modifiers};

// ─── STEP 1: Iced plugin init wiring ────────────────────────────────────────

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

// ─── STEP 2: Window config props ────────────────────────────────────────────

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

// ─── STEP 3a: App::notify enqueues ─────────────────────────────────────────

#[test]
fn test_app_notify_enqueues() {
    let app = App::new(AppConfig::new()).notify("Alert", "Something happened", 1);
    // Public API: notifications() returns &NotificationQueue; check count only.
    assert_eq!(
        app.notifications().len(),
        1,
        "one notification must be enqueued"
    );
    assert!(!app.notifications().is_empty());
}

// ─── STEP 3b: App::hotkey conflict detection ────────────────────────────────

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
        Err(HotkeyConflict { message }) => assert!(!message.is_empty()),
        Ok(_) => panic!("expected HotkeyConflict error"),
    }
}

// ─── STEP 3c: Command palette fuzzy match ───────────────────────────────────

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

// ─── STEP 3d: Screenshot ────────────────────────────────────────────────────

#[test]
fn test_screenshot_returns_nonempty_or_unsupported() {
    use oxiui::UiError;
    let app = App::new(AppConfig::new().size(64.0, 48.0));
    match app.screenshot() {
        Ok(bytes) => assert!(!bytes.is_empty(), "screenshot bytes must be non-empty"),
        Err(UiError::Unsupported(_)) => {} // expected when `software` feature is not enabled
        Err(e) => panic!("unexpected screenshot error: {e:?}"),
    }
}

// ─── STEP 3e: run_with_return ────────────────────────────────────────────────

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

// ─── STEP 3f: Lifecycle on_close/on_resize/on_focus ─────────────────────────

#[test]
fn test_lifecycle_on_close_registered() {
    let _app = App::new(AppConfig::new()).on_close(|_ui| {});
}

#[test]
fn test_lifecycle_on_resize_registered() {
    let _app = App::new(AppConfig::new()).on_resize(|_ui| {});
}

#[test]
fn test_lifecycle_on_focus_registered() {
    let _app = App::new(AppConfig::new()).on_focus(|_ui| {});
}

// ─── STEP 3g: Richer AppExit ─────────────────────────────────────────────────

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

// ─── STEP 3h: Prelude exports UiCtx ─────────────────────────────────────────

#[test]
fn test_prelude_exports_uictx() {
    use oxiui::prelude::*;
    fn _accepts_ctx(_: &dyn UiCtx) {}
}

// ─── Integration: headless smoke ────────────────────────────────────────────

#[test]
fn test_headless_smoke_all_apis() {
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
