#![forbid(unsafe_code)]
#![warn(missing_docs)]
//! `oxiui-web` — wasm32 entry point for OxiUI.
//!
//! # Usage (JavaScript / TypeScript)
//!
//! After building with `wasm-pack build --target web`:
//!
//! ```js
//! import init, { mount } from './pkg/oxiui_web.js';
//! await init();
//! await mount('my-canvas');
//! ```
//!
//! Where `my-canvas` is the `id` of an `<canvas>` element in the page.
//!
//! # Native stub
//!
//! On non-wasm targets this crate compiles to a stub that always returns `Err`.
//! This allows `--all-features` native builds to succeed without pulling in any
//! browser-specific dependencies.

#[cfg(target_arch = "wasm32")]
mod wasm;

// ── WebHandle ────────────────────────────────────────────────────────────────

/// A handle to a mounted OxiUI web app, allowing control from JS.
///
/// Returned by `mount()`. On non-wasm targets this handle always reports
/// "not running" and all control methods are no-ops.
pub struct WebHandle {
    running: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Shared egui context — populated by [`wasm::WasmApp`] on first paint.
    /// `None` before the first frame and on non-wasm targets.
    #[cfg(target_arch = "wasm32")]
    ctx_slot: std::sync::Arc<std::sync::Mutex<Option<egui::Context>>>,
}

impl std::fmt::Debug for WebHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebHandle")
            .field(
                "running",
                &self.running.load(std::sync::atomic::Ordering::SeqCst),
            )
            .finish()
    }
}

impl WebHandle {
    /// Create a new [`WebHandle`] in the running state (native / non-wasm).
    pub fn new() -> Self {
        WebHandle {
            running: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)),
            #[cfg(target_arch = "wasm32")]
            ctx_slot: std::sync::Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// Create a [`WebHandle`] backed by a live egui context slot (wasm32 only).
    ///
    /// Called internally by `mount()` after `WebRunner::start()` succeeds.
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn new_wasm(
        ctx_slot: std::sync::Arc<std::sync::Mutex<Option<egui::Context>>>,
    ) -> Self {
        WebHandle {
            running: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)),
            ctx_slot,
        }
    }

    /// Stop the mounted application.
    pub fn stop(&self) {
        self.running
            .store(false, std::sync::atomic::Ordering::SeqCst);
    }

    /// Signal a canvas resize to the egui event loop.
    ///
    /// On wasm32, calls `egui::Context::request_repaint()` so the next frame
    /// picks up the new canvas dimensions that the browser ResizeObserver
    /// (installed by eframe) has already reported to WebGL/wgpu.
    ///
    /// On native this is a no-op.
    pub fn resize(&self, _width: f32, _height: f32) {
        #[cfg(target_arch = "wasm32")]
        {
            if let Ok(guard) = self.ctx_slot.lock() {
                if let Some(ctx) = guard.as_ref() {
                    ctx.request_repaint();
                }
            }
        }
    }

    /// Inject a JSON-encoded [`oxiui_core::UiEvent`] into the egui event loop.
    ///
    /// On wasm32 the string is deserialised via `serde_json` and forwarded to
    /// egui through [`oxiui_egui::forward_event_to_egui`].
    ///
    /// On native this is a no-op (the argument is ignored) and always returns
    /// `Ok(())`.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the JSON cannot be deserialised as a `UiEvent`.
    pub fn inject_event(&self, _ev_json: &str) -> Result<(), String> {
        #[cfg(target_arch = "wasm32")]
        {
            let event: oxiui_core::UiEvent = serde_json::from_str(_ev_json)
                .map_err(|e| format!("inject_event: invalid event JSON: {e}"))?;
            if let Ok(guard) = self.ctx_slot.lock() {
                if let Some(ctx) = guard.as_ref() {
                    oxiui_egui::forward_event_to_egui(ctx, &event);
                }
            }
        }
        Ok(())
    }

    /// Returns `true` while the mounted app is running.
    pub fn is_running(&self) -> bool {
        self.running.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl Default for WebHandle {
    fn default() -> Self {
        Self::new()
    }
}

// wasm_bindgen JS-facing wrapper — only on wasm32.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
/// JS-facing wrapper for [`WebHandle`].
pub struct JsWebHandle {
    inner: WebHandle,
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
impl JsWebHandle {
    /// Stop the mounted application.
    pub fn stop(&self) {
        self.inner.stop();
    }

    /// Resize the canvas.
    pub fn resize(&self, width: f32, height: f32) {
        self.inner.resize(width, height);
    }

    /// Inject a JSON-encoded event.
    pub fn inject_event(&self, ev_json: &str) -> Result<(), wasm_bindgen::JsValue> {
        self.inner
            .inject_event(ev_json)
            .map_err(|e| wasm_bindgen::JsValue::from_str(&e))
    }

    /// Returns `true` while the app is running.
    pub fn is_running(&self) -> bool {
        self.inner.is_running()
    }
}

// ── MountOptions ─────────────────────────────────────────────────────────────

/// Configuration passed to [`mount()`].
#[derive(Default, Clone, Debug)]
pub struct MountOptions {
    /// Optional theme name (e.g. `"dark"`, `"light"`).
    pub theme_name: Option<String>,
    /// Optional canvas width in logical pixels.
    pub width: Option<f32>,
    /// Optional canvas height in logical pixels.
    pub height: Option<f32>,
    /// Whether to enable HiDPI / Retina rendering.
    pub hidpi: Option<bool>,
}

impl MountOptions {
    /// Create a [`MountOptions`] with all fields set to `None`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the theme name.
    pub fn with_theme(mut self, t: &str) -> Self {
        self.theme_name = Some(t.to_string());
        self
    }

    /// Set the canvas width.
    pub fn with_width(mut self, w: f32) -> Self {
        self.width = Some(w);
        self
    }

    /// Set the canvas height.
    pub fn with_height(mut self, h: f32) -> Self {
        self.height = Some(h);
        self
    }

    /// Set the HiDPI flag.
    pub fn with_hidpi(mut self, h: bool) -> Self {
        self.hidpi = Some(h);
        self
    }
}

/// JS-facing builder for [`MountOptions`] — wasm32 only.
#[cfg(target_arch = "wasm32")]
#[derive(Default)]
#[wasm_bindgen::prelude::wasm_bindgen]
pub struct JsMountOptions {
    inner: MountOptions,
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
impl JsMountOptions {
    /// Create a new [`JsMountOptions`] with defaults.
    pub fn new() -> Self {
        JsMountOptions::default()
    }

    /// Set the theme name.
    pub fn with_theme(mut self, t: &str) -> JsMountOptions {
        self.inner = self.inner.with_theme(t);
        self
    }

    /// Set canvas width (pass `-1.0` to leave unset).
    pub fn with_width(mut self, w: f32) -> JsMountOptions {
        self.inner = self.inner.with_width(w);
        self
    }

    /// Set canvas height (pass `-1.0` to leave unset).
    pub fn with_height(mut self, h: f32) -> JsMountOptions {
        self.inner = self.inner.with_height(h);
        self
    }

    /// Set the HiDPI flag.
    pub fn with_hidpi(mut self, h: bool) -> JsMountOptions {
        self.inner = self.inner.with_hidpi(h);
        self
    }
}

// ── MountError ───────────────────────────────────────────────────────────────

/// Typed error returned by [`mount()`] and [`mount_sync()`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MountError {
    /// The requested canvas element was not found in the DOM.
    CanvasNotFound = 0,
    /// The canvas was found but the runner could not be initialised.
    InitFailed = 1,
    /// This feature is not supported on the current target.
    FeatureNotSupported = 2,
}

impl std::fmt::Display for MountError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MountError::CanvasNotFound => {
                write!(f, "Canvas element not found")
            }
            MountError::InitFailed => write!(f, "Initialization failed"),
            MountError::FeatureNotSupported => {
                write!(f, "Feature not supported on this target")
            }
        }
    }
}

impl std::error::Error for MountError {}

#[cfg(target_arch = "wasm32")]
impl From<MountError> for wasm_bindgen::JsValue {
    fn from(e: MountError) -> wasm_bindgen::JsValue {
        wasm_bindgen::JsValue::from_str(&e.to_string())
    }
}

// ── mount / mount_sync — non-wasm stubs ──────────────────────────────────────

/// Mount an OxiUI app on the canvas with `canvas_id`.
///
/// # Non-wasm targets
///
/// Always returns `Err(MountError::FeatureNotSupported)`. This stub allows
/// `--all-features` native builds to compile without browser dependencies.
///
/// # wasm32 targets
///
/// See the wasm module for the real implementation. The wasm entry point is
/// async and returns a `JsWebHandle` usable from JavaScript.
#[cfg(not(target_arch = "wasm32"))]
pub fn mount(_canvas_id: &str, _opts: MountOptions) -> Result<WebHandle, MountError> {
    Err(MountError::FeatureNotSupported)
}

/// Synchronous variant of [`mount()`].
///
/// On non-wasm targets this always returns `Err(MountError::FeatureNotSupported)`.
#[cfg(not(target_arch = "wasm32"))]
pub fn mount_sync(_canvas_id: &str, _opts: MountOptions) -> Result<WebHandle, MountError> {
    Err(MountError::FeatureNotSupported)
}

// Re-export the wasm32 async mount under the same name.
#[cfg(target_arch = "wasm32")]
pub use wasm::mount;

// ── Web key mapping ───────────────────────────────────────────────────────────

/// Map a web `KeyboardEvent.key` string to an [`oxiui_core::Key`] variant.
///
/// Single printable characters (letters, digits, symbols) are returned as
/// [`oxiui_core::Key::Character`]. Non-printable named keys are mapped to
/// the corresponding named variant. Unknown names become
/// [`oxiui_core::Key::Named`] containing the original string.
pub fn map_web_key(key: &str) -> oxiui_core::Key {
    // Single-character printable keys → Character variant.
    // The Web `key` attribute for letter keys is a single letter (upper or
    // lower-case depending on Shift), or a symbol / digit.
    match key {
        // Named non-printable keys:
        "Enter" => oxiui_core::Key::Enter,
        "Tab" => oxiui_core::Key::Tab,
        " " => oxiui_core::Key::Space,
        "Backspace" => oxiui_core::Key::Backspace,
        "Delete" => oxiui_core::Key::Delete,
        "Escape" | "Esc" => oxiui_core::Key::Escape,
        "ArrowLeft" => oxiui_core::Key::ArrowLeft,
        "ArrowRight" => oxiui_core::Key::ArrowRight,
        "ArrowUp" => oxiui_core::Key::ArrowUp,
        "ArrowDown" => oxiui_core::Key::ArrowDown,
        "Home" => oxiui_core::Key::Home,
        "End" => oxiui_core::Key::End,
        "PageUp" => oxiui_core::Key::PageUp,
        "PageDown" => oxiui_core::Key::PageDown,
        // Function keys F1–F24:
        "F1" => oxiui_core::Key::Function(1),
        "F2" => oxiui_core::Key::Function(2),
        "F3" => oxiui_core::Key::Function(3),
        "F4" => oxiui_core::Key::Function(4),
        "F5" => oxiui_core::Key::Function(5),
        "F6" => oxiui_core::Key::Function(6),
        "F7" => oxiui_core::Key::Function(7),
        "F8" => oxiui_core::Key::Function(8),
        "F9" => oxiui_core::Key::Function(9),
        "F10" => oxiui_core::Key::Function(10),
        "F11" => oxiui_core::Key::Function(11),
        "F12" => oxiui_core::Key::Function(12),
        "F13" => oxiui_core::Key::Function(13),
        "F14" => oxiui_core::Key::Function(14),
        "F15" => oxiui_core::Key::Function(15),
        "F16" => oxiui_core::Key::Function(16),
        "F17" => oxiui_core::Key::Function(17),
        "F18" => oxiui_core::Key::Function(18),
        "F19" => oxiui_core::Key::Function(19),
        "F20" => oxiui_core::Key::Function(20),
        "F21" => oxiui_core::Key::Function(21),
        "F22" => oxiui_core::Key::Function(22),
        "F23" => oxiui_core::Key::Function(23),
        "F24" => oxiui_core::Key::Function(24),
        // Any other key: if it is a single code point it is a Character, else
        // it is a Named key (forward-compatible escape hatch).
        other => {
            let mut chars = other.chars();
            let first = chars.next();
            if first.is_some() && chars.next().is_none() {
                // Single code point → printable character.
                oxiui_core::Key::Character(other.to_string())
            } else {
                // Multi-character name not explicitly listed above.
                oxiui_core::Key::Named(other.to_string())
            }
        }
    }
}
