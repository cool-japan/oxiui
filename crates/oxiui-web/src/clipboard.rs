//! Clipboard API integration for OxiUI web.
//!
//! Provides async clipboard read/write via the modern `navigator.clipboard` API
//! with a synchronous `document.execCommand` fallback for older browsers.
//!
//! All wasm-only code is behind `#[cfg(target_arch = "wasm32")]`; native targets
//! compile to no-op stubs that always return `Ok(())` / `Ok(String::new())`.

// ── Clipboard write ───────────────────────────────────────────────────────────

/// Write `text` to the system clipboard.
///
/// On `wasm32` this calls `navigator.clipboard.writeText(text)` (modern
/// Clipboard API).  The future is spawned with `spawn_local` so it runs
/// without the caller needing to be `async`.
///
/// On non-wasm targets this is a no-op that always returns `Ok(())`.
///
/// # Errors
///
/// Returns `Err` if clipboard access is denied by the browser.
#[allow(unused_variables)]
pub fn write_to_clipboard(text: &str) -> Result<(), String> {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen_futures::spawn_local;

        let text_owned = text.to_string();
        spawn_local(async move {
            let window = match web_sys::window() {
                Some(w) => w,
                None => return,
            };
            let navigator = window.navigator();
            let clipboard = navigator.clipboard();
            let _ = wasm_bindgen_futures::JsFuture::from(clipboard.write_text(&text_owned)).await;
        });
        Ok(())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Ok(())
    }
}

/// Write `text` to the clipboard using the legacy `document.execCommand('copy')`
/// path for browsers that do not support the Clipboard API.
///
/// On `wasm32` this creates a temporary `<textarea>`, selects its contents, and
/// calls `document.execCommand('copy')`.
///
/// On non-wasm targets this is always `Ok(())`.
///
/// # Errors
///
/// Returns `Err` if the DOM operations fail.
#[allow(unused_variables)]
pub fn write_to_clipboard_exec_command(text: &str) -> Result<(), String> {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;

        let window = web_sys::window()
            .ok_or_else(|| "write_to_clipboard_exec_command: no window".to_string())?;
        let document = window
            .document()
            .ok_or_else(|| "write_to_clipboard_exec_command: no document".to_string())?;

        // Create a temporary <textarea>, set its value, select all, copy, remove.
        let textarea = document
            .create_element("textarea")
            .map_err(|_| "write_to_clipboard_exec_command: create_element failed".to_string())?;
        let textarea: web_sys::HtmlElement = textarea.dyn_into().map_err(|_| {
            "write_to_clipboard_exec_command: dyn_into HtmlElement failed".to_string()
        })?;

        // Position off-screen so it doesn't flash.
        let style = textarea.style();
        style
            .set_property("position", "fixed")
            .map_err(|_| "write_to_clipboard_exec_command: style failed".to_string())?;
        style
            .set_property("left", "-9999px")
            .map_err(|_| "write_to_clipboard_exec_command: style failed".to_string())?;
        style
            .set_property("top", "-9999px")
            .map_err(|_| "write_to_clipboard_exec_command: style failed".to_string())?;

        textarea.set_inner_text(text);

        let body = document
            .body()
            .ok_or_else(|| "write_to_clipboard_exec_command: no body".to_string())?;
        body.append_child(&textarea)
            .map_err(|_| "write_to_clipboard_exec_command: append_child failed".to_string())?;

        // Select all text and copy.
        let textarea_as_el = &textarea;
        if let Some(input) = textarea_as_el.dyn_ref::<web_sys::HtmlInputElement>() {
            input.select();
        }
        let _ = document.exec_command("copy");

        body.remove_child(&textarea)
            .map_err(|_| "write_to_clipboard_exec_command: remove_child failed".to_string())?;

        Ok(())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Ok(())
    }
}

// ── Clipboard read ────────────────────────────────────────────────────────────

/// A callback type invoked with the clipboard text once it has been read.
///
/// The callback receives `Ok(text)` on success and `Err(description)` on
/// failure.  On wasm32 this is called asynchronously after `readText()` resolves.
pub type ClipboardReadCallback = Box<dyn FnOnce(Result<String, String>) + 'static>;

/// Read text from the system clipboard, delivering the result via a callback.
///
/// On `wasm32` this calls `navigator.clipboard.readText()` asynchronously and
/// invokes `callback` once the promise resolves.
///
/// On non-wasm targets the callback is invoked synchronously with
/// `Ok(String::new())`.
///
/// # Note on permissions
///
/// The Clipboard read API requires user permission or a user gesture in
/// modern browsers.  `readText()` may reject in contexts where permission has
/// not been granted.
#[allow(unused_variables)]
pub fn read_from_clipboard(callback: ClipboardReadCallback) {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsValue;
        use wasm_bindgen_futures::spawn_local;

        spawn_local(async move {
            let result: Result<String, String> = async {
                let window = web_sys::window()
                    .ok_or_else(|| "read_from_clipboard: no window".to_string())?;
                let navigator = window.navigator();
                let clipboard = navigator.clipboard();
                let promise = clipboard.read_text();
                let js_val = wasm_bindgen_futures::JsFuture::from(promise)
                    .await
                    .map_err(|e: JsValue| {
                        e.as_string()
                            .unwrap_or_else(|| "clipboard readText failed".to_string())
                    })?;
                Ok(js_val.as_string().unwrap_or_default())
            }
            .await;
            callback(result);
        });
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        callback(Ok(String::new()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_to_clipboard_ok_on_native() {
        assert!(write_to_clipboard("hello clipboard").is_ok());
    }

    #[test]
    fn write_to_clipboard_exec_command_ok_on_native() {
        assert!(write_to_clipboard_exec_command("hello legacy").is_ok());
    }

    #[test]
    fn read_from_clipboard_calls_callback_on_native() {
        // Use Rc<RefCell<>> to allow capture into a 'static closure on native.
        let received = std::rc::Rc::new(std::cell::RefCell::new(
            Option::<Result<String, String>>::None,
        ));
        let received_clone = std::rc::Rc::clone(&received);
        // On native the callback is called synchronously.
        read_from_clipboard(Box::new(move |res| {
            *received_clone.borrow_mut() = Some(res);
        }));
        let guard = received.borrow();
        assert!(
            guard.is_some(),
            "callback should have been called synchronously on native"
        );
        assert!(guard.as_ref().unwrap().is_ok());
    }
}
