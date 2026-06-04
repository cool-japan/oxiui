//! Browser error handling utilities for OxiUI web.
//!
//! - Integrates `console_error_panic_hook` to forward Rust panics to the
//!   browser's `console.error`.
//! - Wires `window.onerror` and `window.onunhandledrejection` to surface
//!   unhandled JS errors through the same reporting path.
//!
//! On non-wasm targets all functions are no-ops.

// ── Panic hook ────────────────────────────────────────────────────────────────

/// Install the `console_error_panic_hook` so that Rust panics are forwarded
/// to `console.error` in the browser.
///
/// This is idempotent: calling it more than once has no additional effect
/// (the hook is set via `std::panic::set_hook` which simply replaces any
/// previous hook).
///
/// On non-wasm targets this is a no-op.
///
/// # Note
///
/// This requires the `console_error_panic_hook` crate.  In `oxiui-web` it is
/// included as a wasm32-only dependency so that native builds are not affected.
pub fn install_panic_hook() {
    #[cfg(target_arch = "wasm32")]
    {
        console_error_panic_hook::set_once();
    }
}

// ── window.onerror ────────────────────────────────────────────────────────────

/// Install a `window.onerror` handler that logs unhandled JS errors to the
/// browser console.
///
/// On `wasm32` this sets `window.onerror` to a Rust closure that calls
/// `web_sys::console::error_1` with a formatted error string.
///
/// On non-wasm targets this is a no-op that always returns `Ok(())`.
///
/// # Errors
///
/// Returns `Err` if the DOM operation fails (extremely unlikely).
pub fn install_onerror_handler() -> Result<(), String> {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::{closure::Closure, JsCast, JsValue};

        let window = web_sys::window()
            .ok_or_else(|| "install_onerror_handler: no window available".to_string())?;

        let closure =
            Closure::<dyn FnMut(JsValue, JsValue, JsValue, JsValue, JsValue)>::wrap(Box::new(
                move |message: JsValue,
                      source: JsValue,
                      line: JsValue,
                      col: JsValue,
                      _error: JsValue| {
                    let msg = message.as_string().unwrap_or_default();
                    let src = source.as_string().unwrap_or_default();
                    let ln = line.as_f64().unwrap_or(0.0) as u32;
                    let cn = col.as_f64().unwrap_or(0.0) as u32;
                    let formatted = format!("[oxiui] window.onerror: {msg} @ {src}:{ln}:{cn}");
                    web_sys::console::error_1(&JsValue::from_str(&formatted));
                },
            ));

        js_sys::Reflect::set(
            &window,
            &JsValue::from_str("onerror"),
            closure.as_ref().unchecked_ref(),
        )
        .map_err(|_| "install_onerror_handler: failed to set window.onerror".to_string())?;

        closure.forget();
        Ok(())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Ok(())
    }
}

/// Install a `window.onunhandledrejection` handler that logs unhandled Promise
/// rejections to the browser console.
///
/// On non-wasm targets this is a no-op that always returns `Ok(())`.
///
/// # Errors
///
/// Returns `Err` if the DOM operation fails.
pub fn install_unhandled_rejection_handler() -> Result<(), String> {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::{closure::Closure, JsCast, JsValue};

        let window = web_sys::window().ok_or_else(|| {
            "install_unhandled_rejection_handler: no window available".to_string()
        })?;

        let closure = Closure::<dyn FnMut(web_sys::ErrorEvent)>::wrap(Box::new(
            move |e: web_sys::ErrorEvent| {
                let msg = e.message();
                let formatted = format!("[oxiui] unhandledrejection: {msg}");
                web_sys::console::error_1(&JsValue::from_str(&formatted));
            },
        ));

        js_sys::Reflect::set(
            &window,
            &JsValue::from_str("onunhandledrejection"),
            closure.as_ref().unchecked_ref(),
        )
        .map_err(|_| "install_unhandled_rejection_handler: failed to set handler".to_string())?;

        closure.forget();
        Ok(())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Ok(())
    }
}

/// Install all error handlers in one call.
///
/// Equivalent to calling [`install_panic_hook()`],
/// [`install_onerror_handler()`], and
/// [`install_unhandled_rejection_handler()`] in sequence.
///
/// On non-wasm targets this is a no-op that always returns `Ok(())`.
///
/// # Errors
///
/// Returns the first error encountered, if any.
pub fn install_all_error_handlers() -> Result<(), String> {
    install_panic_hook();
    install_onerror_handler()?;
    install_unhandled_rejection_handler()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_panic_hook_noop_on_native() {
        // Should not panic, crash, or do anything observable on native.
        install_panic_hook();
    }

    #[test]
    fn install_onerror_handler_ok_on_native() {
        assert!(install_onerror_handler().is_ok());
    }

    #[test]
    fn install_unhandled_rejection_handler_ok_on_native() {
        assert!(install_unhandled_rejection_handler().is_ok());
    }

    #[test]
    fn install_all_error_handlers_ok_on_native() {
        assert!(install_all_error_handlers().is_ok());
    }
}
