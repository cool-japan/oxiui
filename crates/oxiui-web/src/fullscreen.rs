//! Fullscreen API helpers for OxiUI web.
//!
//! Provides wrappers around the W3C Fullscreen API:
//! - `request_fullscreen(canvas_id)` — enter fullscreen for the canvas.
//! - `exit_fullscreen()` — exit fullscreen.
//! - `is_fullscreen()` — query current fullscreen state.
//!
//! On non-wasm targets all functions are no-ops or return sensible defaults.

// ── Request fullscreen ────────────────────────────────────────────────────────

/// Request fullscreen mode for the canvas element with the given DOM `id`.
///
/// On `wasm32` this calls `element.requestFullscreen()` (web-sys exposes it as a
/// synchronous `Result<(), JsValue>`). The browser may show a fullscreen
/// transition animation and may reject the request when there is no active user
/// gesture; such a rejection is silently discarded (there is no good way to
/// surface it synchronously to the caller).
///
/// On non-wasm targets this is always `Ok(())`.
///
/// # Errors
///
/// Returns `Err` if the canvas element is not found in the DOM.
pub fn request_fullscreen(canvas_id: &str) -> Result<(), String> {
    #[cfg(target_arch = "wasm32")]
    {
        let window = web_sys::window()
            .ok_or_else(|| "request_fullscreen: no window available".to_string())?;
        let document = window
            .document()
            .ok_or_else(|| "request_fullscreen: no document available".to_string())?;
        let element = document
            .get_element_by_id(canvas_id)
            .ok_or_else(|| format!("request_fullscreen: canvas '{canvas_id}' not found"))?;

        // web-sys 0.3 exposes `requestFullscreen()` as a synchronous call
        // returning `Result<(), JsValue>`. The browser may reject it (e.g. when
        // there is no active user gesture); that rejection is not fatal to the
        // caller, so it is intentionally discarded here.
        let _ = element.request_fullscreen();

        Ok(())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = canvas_id;
        Ok(())
    }
}

// ── Exit fullscreen ───────────────────────────────────────────────────────────

/// Exit fullscreen mode.
///
/// On `wasm32` this calls `document.exitFullscreen()`.
/// On non-wasm targets this is always `Ok(())`.
///
/// # Errors
///
/// Returns `Err` if the DOM operation fails.
pub fn exit_fullscreen() -> Result<(), String> {
    #[cfg(target_arch = "wasm32")]
    {
        let window =
            web_sys::window().ok_or_else(|| "exit_fullscreen: no window available".to_string())?;
        let document = window
            .document()
            .ok_or_else(|| "exit_fullscreen: no document available".to_string())?;

        // web-sys 0.3 exposes `exitFullscreen()` as a synchronous call
        // returning `()`; it is a no-op when not currently in fullscreen.
        document.exit_fullscreen();

        Ok(())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Ok(())
    }
}

// ── Fullscreen query ──────────────────────────────────────────────────────────

/// Returns `true` if the document is currently in fullscreen mode.
///
/// On `wasm32` this checks `document.fullscreen_element().is_some()`.
/// On non-wasm targets this always returns `false`.
pub fn is_fullscreen() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()
            .and_then(|w| w.document())
            .map(|d| d.fullscreen_element().is_some())
            .unwrap_or(false)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        false
    }
}

/// Toggle fullscreen for the canvas: enters if not fullscreen, exits otherwise.
///
/// On non-wasm targets this is always `Ok(())`.
///
/// # Errors
///
/// Returns the first error encountered from `request_fullscreen` or
/// `exit_fullscreen`.
pub fn toggle_fullscreen(canvas_id: &str) -> Result<(), String> {
    if is_fullscreen() {
        exit_fullscreen()
    } else {
        request_fullscreen(canvas_id)
    }
}

// ── Fullscreen change listener ────────────────────────────────────────────────

/// Listen for fullscreen change events and invoke the callback with the new
/// state (`true` = entered fullscreen, `false` = exited).
///
/// On `wasm32` this wires a `fullscreenchange` event listener on the document.
/// On non-wasm targets this is always `Ok(())`.
///
/// # Errors
///
/// Returns `Err` if the DOM binding fails.
pub fn on_fullscreen_change<F>(callback: F) -> Result<(), String>
where
    F: Fn(bool) + 'static,
{
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::{closure::Closure, JsCast};

        let window = web_sys::window()
            .ok_or_else(|| "on_fullscreen_change: no window available".to_string())?;
        let document = window
            .document()
            .ok_or_else(|| "on_fullscreen_change: no document available".to_string())?;

        let closure = Closure::<dyn FnMut()>::wrap(Box::new(move || {
            let fullscreen = web_sys::window()
                .and_then(|w| w.document())
                .map(|d| d.fullscreen_element().is_some())
                .unwrap_or(false);
            callback(fullscreen);
        }));

        document
            .add_event_listener_with_callback("fullscreenchange", closure.as_ref().unchecked_ref())
            .map_err(|_| {
                "on_fullscreen_change: failed to add fullscreenchange listener".to_string()
            })?;

        closure.forget();
        Ok(())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = callback;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_fullscreen_ok_on_native() {
        assert!(request_fullscreen("any-canvas").is_ok());
    }

    #[test]
    fn exit_fullscreen_ok_on_native() {
        assert!(exit_fullscreen().is_ok());
    }

    #[test]
    fn is_fullscreen_false_on_native() {
        assert!(!is_fullscreen());
    }

    #[test]
    fn toggle_fullscreen_ok_on_native() {
        // On native is_fullscreen() returns false → calls request_fullscreen (no-op).
        assert!(toggle_fullscreen("any-canvas").is_ok());
    }

    #[test]
    fn on_fullscreen_change_ok_on_native() {
        assert!(on_fullscreen_change(|_entered| {}).is_ok());
    }
}
