//! Native integration tests for `oxiui-web`.
//!
//! These tests run on the host (non-wasm) target and verify that:
//!   - the native stubs return the expected errors,
//!   - [`WebHandle`] state tracking works,
//!   - [`MountOptions`] builder chains set the correct fields,
//!   - [`MountError`] display strings are as documented,
//!   - key-translation helpers produce correct [`oxiui_core::Key`] variants.

// ── feature-flag gate verification ───────────────────────────────────────────

/// Verify that `drag_drop` module is present when the `drag-drop` feature is on.
#[cfg(feature = "drag-drop")]
#[test]
fn drag_drop_module_available_with_feature() {
    // A simple smoke test: constructing a DragPayload should compile and work.
    let p = oxiui_web::drag_drop::DragPayload::default();
    assert!(!p.has_files());
}

/// Verify that `service_worker` module is present when the `service-worker` feature is on.
#[cfg(feature = "service-worker")]
#[test]
fn service_worker_module_available_with_feature() {
    // Smoke test: register_service_worker accepts a callback and completes without panic.
    // On native it calls the callback with Err immediately (no serviceWorker API).
    let called = std::rc::Rc::new(std::cell::Cell::new(false));
    let called_clone = std::rc::Rc::clone(&called);
    oxiui_web::service_worker::register_service_worker(
        "sw.js",
        Box::new(move |result| {
            // On native this is always Err.
            assert!(result.is_err());
            called_clone.set(true);
        }),
    );
    assert!(
        called.get(),
        "callback should have been called synchronously on native"
    );
}

/// Verify that `fullscreen` module is present when the `fullscreen` feature is on.
#[cfg(feature = "fullscreen")]
#[test]
fn fullscreen_module_available_with_feature() {
    // is_fullscreen returns false on native.
    assert!(!oxiui_web::fullscreen::is_fullscreen());
}

/// Verify that `font_loading` module is present when the `font-loading` feature is on.
#[cfg(feature = "font-loading")]
#[test]
fn font_loading_module_available_with_feature() {
    let req = oxiui_web::font_loading::FontLoadRequest::new("TestFont", "url('/test.woff2')");
    assert_eq!(req.family, "TestFont");
}

// ── mount stub tests ─────────────────────────────────────────────────────────

/// On a non-wasm host `mount()` must return `Err(MountError::FeatureNotSupported)`.
#[test]
fn mount_returns_err_on_non_wasm() {
    let result = oxiui_web::mount("anything", oxiui_web::MountOptions::new());
    match result {
        Err(oxiui_web::MountError::FeatureNotSupported) => {}
        other => panic!("expected FeatureNotSupported, got {:?}", other),
    }
}

/// `mount_sync()` mirrors the same stub behaviour.
#[test]
fn mount_sync_returns_err_on_non_wasm() {
    let result = oxiui_web::mount_sync("anything", oxiui_web::MountOptions::new());
    match result {
        Err(oxiui_web::MountError::FeatureNotSupported) => {}
        other => panic!("expected FeatureNotSupported, got {:?}", other),
    }
}

// ── WebHandle tests ──────────────────────────────────────────────────────────

#[test]
fn web_handle_starts_running_then_stops() {
    let h = oxiui_web::WebHandle::new();
    assert!(h.is_running(), "newly created WebHandle should be running");
    h.stop();
    assert!(!h.is_running(), "WebHandle should stop after stop()");
}

#[test]
fn web_handle_inject_event_ok() {
    let h = oxiui_web::WebHandle::new();
    assert!(h.inject_event("{}").is_ok());
}

#[test]
fn web_handle_resize_does_not_panic() {
    let h = oxiui_web::WebHandle::new();
    h.resize(800.0, 600.0); // no-op on native, must not panic
}

#[test]
fn web_handle_default_is_running() {
    let h: oxiui_web::WebHandle = Default::default();
    assert!(h.is_running());
}

// ── MountOptions tests ───────────────────────────────────────────────────────

#[test]
fn mount_options_builder() {
    let opts = oxiui_web::MountOptions::new()
        .with_theme("dark")
        .with_width(800.0)
        .with_height(600.0)
        .with_hidpi(true);
    assert_eq!(opts.theme_name.as_deref(), Some("dark"));
    assert_eq!(opts.width, Some(800.0));
    assert_eq!(opts.height, Some(600.0));
    assert_eq!(opts.hidpi, Some(true));
}

#[test]
fn mount_options_defaults_are_none() {
    let opts = oxiui_web::MountOptions::new();
    assert!(opts.theme_name.is_none());
    assert!(opts.width.is_none());
    assert!(opts.height.is_none());
    assert!(opts.hidpi.is_none());
}

// ── MountError tests ─────────────────────────────────────────────────────────

#[test]
fn mount_error_display() {
    assert_eq!(
        format!("{}", oxiui_web::MountError::FeatureNotSupported),
        "Feature not supported on this target"
    );
    assert_eq!(
        format!("{}", oxiui_web::MountError::CanvasNotFound),
        "Canvas element not found"
    );
    assert_eq!(
        format!("{}", oxiui_web::MountError::InitFailed),
        "Initialization failed"
    );
}

#[test]
fn mount_error_is_error_trait() {
    let e: &dyn std::error::Error = &oxiui_web::MountError::CanvasNotFound;
    assert!(!e.to_string().is_empty());
}

#[test]
fn mount_error_discriminants() {
    // The repr values are part of the ABI (exported to JS as integers).
    assert_eq!(oxiui_web::MountError::CanvasNotFound as u8, 0);
    assert_eq!(oxiui_web::MountError::InitFailed as u8, 1);
    assert_eq!(oxiui_web::MountError::FeatureNotSupported as u8, 2);
}

// ── map_web_key tests ─────────────────────────────────────────────────────────

#[test]
fn map_web_key_letters() {
    // Lowercase letters → Character variant
    assert_eq!(
        oxiui_web::map_web_key("a"),
        oxiui_core::Key::Character("a".to_string())
    );
    assert_eq!(
        oxiui_web::map_web_key("z"),
        oxiui_core::Key::Character("z".to_string())
    );
    // Uppercase (Shift held) → Character variant too
    assert_eq!(
        oxiui_web::map_web_key("A"),
        oxiui_core::Key::Character("A".to_string())
    );
}

#[test]
fn map_web_key_named_keys() {
    assert_eq!(oxiui_web::map_web_key("Enter"), oxiui_core::Key::Enter);
    assert_eq!(oxiui_web::map_web_key("Escape"), oxiui_core::Key::Escape);
    assert_eq!(oxiui_web::map_web_key("Esc"), oxiui_core::Key::Escape);
    assert_eq!(oxiui_web::map_web_key("Tab"), oxiui_core::Key::Tab);
    assert_eq!(oxiui_web::map_web_key(" "), oxiui_core::Key::Space);
    assert_eq!(
        oxiui_web::map_web_key("Backspace"),
        oxiui_core::Key::Backspace
    );
    assert_eq!(oxiui_web::map_web_key("Delete"), oxiui_core::Key::Delete);
}

#[test]
fn map_web_key_arrow_keys() {
    assert_eq!(
        oxiui_web::map_web_key("ArrowLeft"),
        oxiui_core::Key::ArrowLeft
    );
    assert_eq!(
        oxiui_web::map_web_key("ArrowRight"),
        oxiui_core::Key::ArrowRight
    );
    assert_eq!(oxiui_web::map_web_key("ArrowUp"), oxiui_core::Key::ArrowUp);
    assert_eq!(
        oxiui_web::map_web_key("ArrowDown"),
        oxiui_core::Key::ArrowDown
    );
    assert_eq!(oxiui_web::map_web_key("Home"), oxiui_core::Key::Home);
    assert_eq!(oxiui_web::map_web_key("End"), oxiui_core::Key::End);
    assert_eq!(oxiui_web::map_web_key("PageUp"), oxiui_core::Key::PageUp);
    assert_eq!(
        oxiui_web::map_web_key("PageDown"),
        oxiui_core::Key::PageDown
    );
}

#[test]
fn map_web_key_function_keys() {
    assert_eq!(oxiui_web::map_web_key("F1"), oxiui_core::Key::Function(1));
    assert_eq!(oxiui_web::map_web_key("F5"), oxiui_core::Key::Function(5));
    assert_eq!(oxiui_web::map_web_key("F12"), oxiui_core::Key::Function(12));
    assert_eq!(oxiui_web::map_web_key("F24"), oxiui_core::Key::Function(24));
}

#[test]
fn map_web_key_unknown_named() {
    // Multi-character unrecognised name → Named variant
    assert_eq!(
        oxiui_web::map_web_key("CapsLock"),
        oxiui_core::Key::Named("CapsLock".to_string())
    );
    assert_eq!(
        oxiui_web::map_web_key("NumLock"),
        oxiui_core::Key::Named("NumLock".to_string())
    );
}

#[test]
fn map_web_key_digit() {
    // Single-char digit → Character
    assert_eq!(
        oxiui_web::map_web_key("0"),
        oxiui_core::Key::Character("0".to_string())
    );
    assert_eq!(
        oxiui_web::map_web_key("9"),
        oxiui_core::Key::Character("9".to_string())
    );
}

// ── GPU capability detection tests ───────────────────────────────────────────

#[test]
fn detect_gpu_capability_returns_not_applicable_on_native() {
    // On non-wasm targets `detect_gpu_capability()` must return
    // `GpuCapability::NotApplicable` without panicking.
    let cap = oxiui_web::detect_gpu_capability();
    assert_eq!(cap, oxiui_web::GpuCapability::NotApplicable);
}

#[test]
fn gpu_capability_display_is_non_empty() {
    let cases = [
        oxiui_web::GpuCapability::WebGpu,
        oxiui_web::GpuCapability::WebGl2,
        oxiui_web::GpuCapability::WebGl1,
        oxiui_web::GpuCapability::SoftwareFallback,
        oxiui_web::GpuCapability::NotApplicable,
    ];
    for c in &cases {
        assert!(
            !c.to_string().is_empty(),
            "{c:?} display should be non-empty"
        );
    }
}

// ── cursor_css tests ──────────────────────────────────────────────────────────

#[test]
fn cursor_css_pointer_returns_pointer_string() {
    assert_eq!(
        oxiui_web::cursor_css(oxiui_core::CursorShape::Pointer),
        "pointer"
    );
}

#[test]
fn cursor_css_text_returns_text_string() {
    assert_eq!(oxiui_web::cursor_css(oxiui_core::CursorShape::Text), "text");
}

#[test]
fn cursor_css_none_returns_none_string() {
    assert_eq!(oxiui_web::cursor_css(oxiui_core::CursorShape::None), "none");
}

#[test]
fn apply_cursor_noop_on_native() {
    // On native targets `apply_cursor` is a no-op that always returns Ok(()).
    let result = oxiui_web::apply_cursor("any-canvas", oxiui_core::CursorShape::Pointer);
    assert!(result.is_ok());
}

// ── set_theme / send_event / get_state tests ──────────────────────────────────

#[test]
fn set_theme_known_names_are_ok() {
    let h = oxiui_web::WebHandle::new();
    assert!(oxiui_web::set_theme(&h, "dark").is_ok());
    assert!(oxiui_web::set_theme(&h, "light").is_ok());
    assert!(oxiui_web::set_theme(&h, "high-contrast").is_ok());
}

#[test]
fn set_theme_unknown_name_is_ok_noop() {
    // Unknown names are silently ignored — never an error.
    let h = oxiui_web::WebHandle::new();
    assert!(oxiui_web::set_theme(&h, "unicorn").is_ok());
}

#[test]
fn set_theme_case_insensitive() {
    let h = oxiui_web::WebHandle::new();
    assert!(oxiui_web::set_theme(&h, "DARK").is_ok());
    assert!(oxiui_web::set_theme(&h, "Light").is_ok());
}

#[test]
fn send_event_empty_json_is_ok() {
    let h = oxiui_web::WebHandle::new();
    // inject_event with `{}` should not error on the native stub.
    assert!(oxiui_web::send_event(&h, "{}").is_ok());
}

#[test]
fn get_state_returns_json_with_running_key() {
    let h = oxiui_web::WebHandle::new();
    let state = oxiui_web::get_state(&h);
    assert!(
        state.contains("\"running\""),
        "state JSON should contain running key"
    );
    assert!(state.contains("true"), "new handle is running");
    h.stop();
    let stopped = oxiui_web::get_state(&h);
    assert!(
        stopped.contains("false"),
        "stopped handle state should be false"
    );
}
