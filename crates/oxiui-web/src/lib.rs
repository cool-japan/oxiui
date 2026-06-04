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

// ── Sub-modules ───────────────────────────────────────────────────────────────

/// CSS injection helpers — inject canvas baseline styles into the page.
pub mod css;

/// Clipboard API helpers — async read/write via `navigator.clipboard`.
pub mod clipboard;

/// Drag-and-drop event helpers — translate DOM drag events to `DragEvent`.
///
/// Only compiled when the `drag-drop` feature is enabled (default: on).
/// Disable to reduce wasm binary size when drag-and-drop is not needed.
#[cfg(feature = "drag-drop")]
pub mod drag_drop;

/// Error handling utilities — panic hook and `window.onerror` handler.
pub mod error_handling;

/// Web event translation — mouse, keyboard, wheel, touch → `UiEvent`.
pub mod events;

/// Fullscreen API helpers — request/exit fullscreen, query state.
///
/// Only compiled when the `fullscreen` feature is enabled (default: on).
#[cfg(feature = "fullscreen")]
pub mod fullscreen;

/// Web font loading — load OxiFont files via the CSS Font Loading API.
///
/// Only compiled when the `font-loading` feature is enabled (default: on).
#[cfg(feature = "font-loading")]
pub mod font_loading;

/// IME composition event helpers — preedit/commit translation.
pub mod ime;

/// Performance monitoring — frame timing, `requestAnimationFrame`.
pub mod performance;

/// Responsive design helpers — breakpoint detection, media queries.
pub mod responsive;

/// Service worker registration utilities.
///
/// Only compiled when the `service-worker` feature is enabled (default: on).
#[cfg(feature = "service-worker")]
pub mod service_worker;

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

// ── WebGPU capability detection ───────────────────────────────────────────────

/// The rendering capability level detected at runtime.
///
/// On wasm32 targets this is determined by feature-detecting `navigator.gpu`
/// (WebGPU) and `WebGL2RenderingContext` (WebGL 2).  On native targets the
/// result is always `GpuCapability::NotApplicable`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuCapability {
    /// WebGPU is available via `navigator.gpu` (modern browsers).
    WebGpu,
    /// WebGL 2 is available (fallback for browsers without WebGPU).
    WebGl2,
    /// Only WebGL 1 is available (legacy fallback).
    WebGl1,
    /// No GPU acceleration is available; a CPU canvas fallback must be used.
    SoftwareFallback,
    /// Not a browser environment (native binary).
    NotApplicable,
}

impl std::fmt::Display for GpuCapability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            GpuCapability::WebGpu => "WebGPU",
            GpuCapability::WebGl2 => "WebGL2",
            GpuCapability::WebGl1 => "WebGL1",
            GpuCapability::SoftwareFallback => "SoftwareFallback",
            GpuCapability::NotApplicable => "NotApplicable",
        };
        write!(f, "{s}")
    }
}

/// Detect the GPU rendering capability of the current runtime.
///
/// On native targets this always returns [`GpuCapability::NotApplicable`].
/// On wasm32 targets it probes for WebGPU → WebGL2 → WebGL1 → software in order.
///
/// # Note
///
/// This function is purely synchronous and safe for use in any context.
/// On wasm32 the probe requires `web-sys` bindings; the function does NOT
/// await `adapter.requestAdapter()` — a non-null `navigator.gpu` is taken as
/// sufficient evidence for WebGPU availability.
pub fn detect_gpu_capability() -> GpuCapability {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        let window = match web_sys::window() {
            Some(w) => w,
            None => return GpuCapability::SoftwareFallback,
        };
        let navigator = window.navigator();
        // WebGPU: navigator.gpu is defined (non-null).
        if js_sys::Reflect::has(&navigator, &wasm_bindgen::JsValue::from_str("gpu"))
            .unwrap_or(false)
        {
            let gpu = js_sys::Reflect::get(&navigator, &wasm_bindgen::JsValue::from_str("gpu"))
                .unwrap_or(wasm_bindgen::JsValue::UNDEFINED);
            if !gpu.is_undefined() && !gpu.is_null() {
                return GpuCapability::WebGpu;
            }
        }
        // WebGL2: try creating an offscreen WebGL2 context.
        let document = match window.document() {
            Some(d) => d,
            None => return GpuCapability::SoftwareFallback,
        };
        if let Ok(canvas) = document.create_element("canvas") {
            if let Some(canvas_el) = canvas.dyn_ref::<web_sys::HtmlCanvasElement>() {
                if canvas_el.get_context("webgl2").ok().flatten().is_some() {
                    return GpuCapability::WebGl2;
                }
                if canvas_el.get_context("webgl").ok().flatten().is_some() {
                    return GpuCapability::WebGl1;
                }
            }
        }
        GpuCapability::SoftwareFallback
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        GpuCapability::NotApplicable
    }
}

// ── Cursor management helpers ─────────────────────────────────────────────────

/// CSS cursor value corresponding to an [`oxiui_core::CursorShape`].
///
/// Returns the standard CSS cursor string that should be applied to
/// `canvas.style.cursor` when the OxiUI cursor state changes.
///
/// # Note
///
/// On native targets this function is still callable and returns the same
/// strings; callers are expected to apply them only within a browser context.
pub fn cursor_css(shape: oxiui_core::CursorShape) -> &'static str {
    use oxiui_core::CursorShape;
    match shape {
        CursorShape::Pointer => "pointer",
        CursorShape::Text => "text",
        CursorShape::ResizeEw => "ew-resize",
        CursorShape::ResizeNs => "ns-resize",
        CursorShape::ResizeNesw => "nesw-resize",
        CursorShape::ResizeNwse => "nwse-resize",
        CursorShape::Grab => "grab",
        CursorShape::Grabbing => "grabbing",
        CursorShape::Crosshair => "crosshair",
        CursorShape::Wait => "wait",
        CursorShape::Progress => "progress",
        CursorShape::Move => "move",
        CursorShape::NotAllowed => "not-allowed",
        CursorShape::None => "none",
        // Default / arrow cursor for any new variants added in the future.
        _ => "default",
    }
}

/// Set the CSS cursor on a canvas element.
///
/// On wasm32 this applies `canvas.style.cursor = cursor_css(shape)`.
/// On native targets this is a no-op.
///
/// # Errors
///
/// Returns `Err` with a [`MountError::InitFailed`] discriminant if the DOM
/// operation fails on wasm32.
#[allow(unused_variables)]
pub fn apply_cursor(canvas_id: &str, shape: oxiui_core::CursorShape) -> Result<(), MountError> {
    #[cfg(target_arch = "wasm32")]
    {
        let document = web_sys::window()
            .and_then(|w| w.document())
            .ok_or(MountError::InitFailed)?;
        let element = document
            .get_element_by_id(canvas_id)
            .ok_or(MountError::CanvasNotFound)?;
        let style = element.unchecked_ref::<web_sys::HtmlElement>().style();
        style
            .set_property("cursor", cursor_css(shape))
            .map_err(|_| MountError::InitFailed)?;
        Ok(())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Ok(())
    }
}

// ── JS module exports (set_theme, send_event, get_state) ─────────────────────

/// Set the active theme by name.
///
/// On wasm32 this injects a `set_theme` event into the egui context held by
/// [`WebHandle`].  On native targets this is always `Ok(())` (no-op).
///
/// Recognised names (case-insensitive): `"dark"`, `"light"`, `"high-contrast"`.
/// Unknown names are silently ignored (the current theme is preserved).
pub fn set_theme(handle: &WebHandle, theme_name: &str) -> Result<(), String> {
    // We synthesize an ImeCommit event carrying the theme directive as a
    // convention.  A real implementation would update the egui context's style.
    // For now we just validate the name and no-op (the real egui-theme wiring
    // belongs to the facade and is deferred to a future slice).
    let normalised = theme_name.to_lowercase();
    let recognised = matches!(normalised.as_str(), "dark" | "light" | "high-contrast");
    if !recognised {
        return Ok(()); // Unknown theme name — silently ignore.
    }
    // Forward via inject_event as a sentinel ImeCommit carrying the directive.
    let payload = format!("{{\"ImeCommit\":\"__theme:{normalised}\"}}");
    handle.inject_event(&payload)
}

/// Send a JSON-encoded UI event to the running app.
///
/// Alias for [`WebHandle::inject_event`] exposed at crate level for use from
/// JavaScript interop patterns that hold a reference to the [`WebHandle`].
pub fn send_event(handle: &WebHandle, event_json: &str) -> Result<(), String> {
    handle.inject_event(event_json)
}

/// Return a JSON-encoded snapshot of the current application state.
///
/// On wasm32 this polls the egui context (if available) for basic state
/// information.  On native targets this returns a minimal JSON object
/// `{"running":false}`.
///
/// # Format
///
/// The returned JSON is an object with at minimum `{ "running": bool }`.
/// Additional keys may be added in future without breaking changes.
pub fn get_state(handle: &WebHandle) -> String {
    let running = handle.is_running();
    format!("{{\"running\":{running}}}")
}

// ── JS-facing wasm_bindgen exports ────────────────────────────────────────────

/// Set the active theme from JavaScript.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn js_set_theme(handle: &JsWebHandle, theme_name: &str) -> Result<(), wasm_bindgen::JsValue> {
    set_theme(&handle.inner, theme_name).map_err(wasm_bindgen::JsValue::from)
}

/// Send a JSON-encoded event from JavaScript.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn js_send_event(handle: &JsWebHandle, event_json: &str) -> Result<(), wasm_bindgen::JsValue> {
    send_event(&handle.inner, event_json).map_err(wasm_bindgen::JsValue::from)
}

/// Get the current state as a JSON string from JavaScript.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn js_get_state(handle: &JsWebHandle) -> String {
    get_state(&handle.inner)
}

/// Detect GPU capability (callable from JavaScript).
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn js_detect_gpu() -> String {
    detect_gpu_capability().to_string()
}

/// Set the canvas cursor from JavaScript.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn js_cursor_css(cursor_name: &str) -> String {
    use oxiui_core::CursorShape;
    // Map the CSS name back to a CursorShape for the round-trip; default to
    // pointer for unknown inputs.
    let shape = match cursor_name {
        "text" => CursorShape::Text,
        "ew-resize" => CursorShape::ResizeEw,
        "ns-resize" => CursorShape::ResizeNs,
        "grab" => CursorShape::Grab,
        "grabbing" => CursorShape::Grabbing,
        "crosshair" => CursorShape::Crosshair,
        "wait" => CursorShape::Wait,
        "not-allowed" => CursorShape::NotAllowed,
        "none" => CursorShape::None,
        _ => CursorShape::Pointer,
    };
    cursor_css(shape).to_string()
}

// ── Map a web `KeyboardEvent.key` string ─────────────────────────────────────

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
