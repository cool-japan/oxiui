//! CSS injection utilities for the OxiUI web layer.
//!
//! Provides helpers that inject minimal CSS into the page's `<head>` to:
//! - Remove scrollbars from `body` so the canvas fills the viewport.
//! - Make the canvas fill its container (`width: 100%; height: 100%`).
//! - Prevent accidental text selection on the canvas (UX improvement).
//! - Scope styles to a data attribute to avoid polluting global CSS.
//!
//! On non-wasm targets all functions compile to no-ops that always return `Ok(())`.

/// Minimal CSS injected by [`inject_canvas_styles`].
///
/// The styles are scoped to the canvas element via the `[data-oxiui]` attribute
/// and to `body` directly.  No third-party CSS classes or reset frameworks are
/// assumed.
pub const CANVAS_BASE_CSS: &str = r#"
/* oxiui-web: baseline canvas styles — injected once on mount */
body {
  margin: 0;
  padding: 0;
  overflow: hidden;
}
canvas[data-oxiui] {
  display: block;
  width: 100%;
  height: 100%;
  user-select: none;
  -webkit-user-select: none;
  -webkit-tap-highlight-color: transparent;
  outline: none;
}
"#;

/// Inject minimal CSS into `document.head` to set up the canvas container.
///
/// Creates a `<style data-oxiui-css>` element and appends it to `<head>`.
/// A second call is idempotent: the function checks for an existing element
/// with the `data-oxiui-css` attribute and skips injection if found.
///
/// On non-wasm targets this is always `Ok(())`.
///
/// # Errors
///
/// Returns `Err` if any DOM operation fails (e.g. `head` is not available).
pub fn inject_canvas_styles() -> Result<(), String> {
    inject_css(CANVAS_BASE_CSS, "data-oxiui-css")
}

/// Inject arbitrary CSS into `document.head` guarded by a unique marker
/// attribute.
///
/// `css_text` is the raw CSS to inject.  `marker_attr` is the attribute name
/// used both to mark the injected `<style>` element and to detect duplicates —
/// choose a unique name per call site (e.g. `"data-oxiui-css"`,
/// `"data-oxiui-theme-css"`).
///
/// On non-wasm targets this is always `Ok(())`.
///
/// # Errors
///
/// Returns `Err` if any DOM operation fails.
#[allow(unused_variables)]
pub fn inject_css(css_text: &str, marker_attr: &str) -> Result<(), String> {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;

        let window =
            web_sys::window().ok_or_else(|| "inject_css: no window available".to_string())?;
        let document = window
            .document()
            .ok_or_else(|| "inject_css: no document available".to_string())?;

        // Idempotency check: bail if already injected.
        if document
            .query_selector(&format!("[{marker_attr}]"))
            .ok()
            .flatten()
            .is_some()
        {
            return Ok(());
        }

        let head = document
            .head()
            .ok_or_else(|| "inject_css: no <head> element in document".to_string())?;

        let style = document
            .create_element("style")
            .map_err(|_| "inject_css: failed to create <style> element".to_string())?;

        style
            .set_attribute(marker_attr, "")
            .map_err(|_| "inject_css: failed to set marker attribute".to_string())?;

        style.set_text_content(Some(css_text));

        head.append_child(&style)
            .map_err(|_| "inject_css: failed to append <style> to <head>".to_string())?;

        Ok(())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Ok(())
    }
}

/// Mark the canvas element with `data-oxiui` so the scoped CSS applies.
///
/// Call once after mounting, passing the canvas element's DOM id.
/// On non-wasm targets this is always `Ok(())`.
///
/// # Errors
///
/// Returns `Err` if the canvas is not found or the attribute cannot be set.
#[allow(unused_variables)]
pub fn mark_canvas(canvas_id: &str) -> Result<(), String> {
    #[cfg(target_arch = "wasm32")]
    {
        let window =
            web_sys::window().ok_or_else(|| "mark_canvas: no window available".to_string())?;
        let document = window
            .document()
            .ok_or_else(|| "mark_canvas: no document available".to_string())?;
        let element = document
            .get_element_by_id(canvas_id)
            .ok_or_else(|| format!("mark_canvas: canvas '{canvas_id}' not found"))?;
        element
            .set_attribute("data-oxiui", "")
            .map_err(|_| "mark_canvas: failed to set data-oxiui attribute".to_string())?;
        Ok(())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canvas_base_css_is_nonempty() {
        assert!(!CANVAS_BASE_CSS.is_empty());
    }

    #[test]
    fn canvas_base_css_contains_body() {
        assert!(CANVAS_BASE_CSS.contains("body"));
    }

    #[test]
    fn canvas_base_css_contains_user_select_none() {
        assert!(CANVAS_BASE_CSS.contains("user-select: none"));
    }

    #[test]
    fn canvas_base_css_contains_overflow_hidden() {
        assert!(CANVAS_BASE_CSS.contains("overflow: hidden"));
    }

    #[test]
    fn inject_canvas_styles_noop_on_native() {
        let result = inject_canvas_styles();
        assert!(result.is_ok());
    }

    #[test]
    fn inject_css_noop_on_native() {
        let result = inject_css("body { color: red; }", "data-test-marker");
        assert!(result.is_ok());
    }

    #[test]
    fn mark_canvas_noop_on_native() {
        let result = mark_canvas("some-canvas");
        assert!(result.is_ok());
    }
}
