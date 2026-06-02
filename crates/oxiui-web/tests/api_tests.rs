//! Native integration tests for `oxiui-web`.
//!
//! These tests run on the host (non-wasm) target and verify that:
//!   - the native stubs return the expected errors,
//!   - [`WebHandle`] state tracking works,
//!   - [`MountOptions`] builder chains set the correct fields,
//!   - [`MountError`] display strings are as documented,
//!   - key-translation helpers produce correct [`oxiui_core::Key`] variants.

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
