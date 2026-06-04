//! Web font loading utilities for OxiUI web.
//!
//! Wraps the CSS Font Loading API (`FontFace`, `document.fonts`) to load
//! OxiFont files from URLs and register them with the browser so CSS
//! `font-family` can reference them.
//!
//! On non-wasm targets all functions compile to no-ops.

// ── Font load request ─────────────────────────────────────────────────────────

/// A font load request describing a single font face to be loaded from a URL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontLoadRequest {
    /// The CSS font-family name to register (e.g. `"OxiSans"`).
    pub family: String,
    /// The URL of the font file.  Can be a relative path or an absolute URL.
    pub url: String,
    /// Optional CSS `font-weight` descriptor (e.g. `"400"`, `"bold"`).
    /// Defaults to `"normal"` when `None`.
    pub weight: Option<String>,
    /// Optional CSS `font-style` descriptor (e.g. `"italic"`).
    /// Defaults to `"normal"` when `None`.
    pub style: Option<String>,
}

impl FontLoadRequest {
    /// Create a minimal font load request with family name and URL only.
    pub fn new(family: impl Into<String>, url: impl Into<String>) -> Self {
        FontLoadRequest {
            family: family.into(),
            url: url.into(),
            weight: None,
            style: None,
        }
    }

    /// Set the font weight.
    pub fn with_weight(mut self, weight: impl Into<String>) -> Self {
        self.weight = Some(weight.into());
        self
    }

    /// Set the font style.
    pub fn with_style(mut self, style: impl Into<String>) -> Self {
        self.style = Some(style.into());
        self
    }

    /// Build the CSS `src` string for use with `new FontFace(...)`.
    pub fn css_src(&self) -> String {
        format!("url('{}')", self.url)
    }

    /// Build the CSS FontFace descriptor string (weight + style).
    pub fn css_descriptors(&self) -> String {
        let weight = self.weight.as_deref().unwrap_or("normal");
        let style = self.style.as_deref().unwrap_or("normal");
        format!("font-weight: {weight}; font-style: {style};")
    }
}

// ── Font loading ──────────────────────────────────────────────────────────────

/// A callback invoked once a font face has been loaded (or failed to load).
///
/// Receives `Ok(family_name)` on success or `Err(description)` on failure.
pub type FontLoadCallback = Box<dyn FnOnce(Result<String, String>) + 'static>;

/// Load a font face from a URL and add it to `document.fonts`.
///
/// On `wasm32` this:
/// 1. Creates a new `FontFace(family, src, descriptors)` object.
/// 2. Calls `font_face.load()` which returns a Promise.
/// 3. Calls `document.fonts.add(font_face)` once the promise resolves.
/// 4. Invokes `callback` with the result.
///
/// On non-wasm targets the callback is invoked synchronously with
/// `Ok(family_name)`.
///
/// # Errors (callback)
///
/// The callback receives `Err` if the font URL is unreachable or the font
/// format is unsupported.
#[allow(unused_variables)]
pub fn load_font(request: FontLoadRequest, callback: FontLoadCallback) {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsValue;
        use wasm_bindgen_futures::spawn_local;

        let family_clone = request.family.clone();

        spawn_local(async move {
            let result: Result<String, String> = async {
                let window = web_sys::window()
                    .ok_or_else(|| "load_font: no window available".to_string())?;
                let document = window
                    .document()
                    .ok_or_else(|| "load_font: no document available".to_string())?;

                let src = request.css_src();
                let descriptors_str = request.css_descriptors();

                // Build FontFaceDescriptors object.
                let descriptors = js_sys::Object::new();
                let weight_val = JsValue::from_str(request.weight.as_deref().unwrap_or("normal"));
                let style_val = JsValue::from_str(request.style.as_deref().unwrap_or("normal"));
                js_sys::Reflect::set(&descriptors, &JsValue::from_str("weight"), &weight_val)
                    .map_err(|_| "load_font: failed to set weight descriptor".to_string())?;
                js_sys::Reflect::set(&descriptors, &JsValue::from_str("style"), &style_val)
                    .map_err(|_| "load_font: failed to set style descriptor".to_string())?;

                let font_face = web_sys::FontFace::new_with_str_and_str_sequence_or_descriptors(
                    &request.family,
                    &src,
                    &descriptors.into(),
                )
                .map_err(|e| {
                    e.as_string().unwrap_or_else(|| {
                        format!(
                            "load_font: FontFace construction failed for '{}'",
                            request.family
                        )
                    })
                })?;

                // Load the font (async).
                let loaded_face = wasm_bindgen_futures::JsFuture::from(font_face.load())
                    .await
                    .map_err(|e| {
                        e.as_string().unwrap_or_else(|| {
                            format!("load_font: failed to load '{}'", request.family)
                        })
                    })?;

                // Add to document.fonts.
                let fonts = document.fonts();
                let loaded_face_typed: web_sys::FontFace = loaded_face.into();
                fonts.add(&loaded_face_typed).map_err(|_| {
                    format!(
                        "load_font: document.fonts.add failed for '{}'",
                        request.family
                    )
                })?;

                Ok(request.family)
            }
            .await;

            callback(result);
        });
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let family = request.family.clone();
        callback(Ok(family));
    }
}

/// Load multiple font faces in parallel, delivering results via a single
/// combined callback once all have finished (or any fails).
///
/// On non-wasm targets the callback is invoked synchronously with an empty
/// `Vec` of loaded families (no fonts to load on the server side).
///
/// On failure the callback receives the first error encountered; successful
/// faces loaded before the failure may already be registered.
#[allow(unused_variables)]
pub fn load_fonts_parallel(requests: Vec<FontLoadRequest>, callback: FontLoadCallback) {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen_futures::spawn_local;

        if requests.is_empty() {
            callback(Ok(String::new()));
            return;
        }

        let total = requests.len();
        let results =
            std::sync::Arc::new(std::sync::Mutex::new(Vec::<Result<String, String>>::new()));
        let callback = std::sync::Arc::new(std::sync::Mutex::new(Some(callback)));

        for request in requests {
            let results_clone = std::sync::Arc::clone(&results);
            let callback_clone = std::sync::Arc::clone(&callback);
            let total = total;

            load_font(
                request,
                Box::new(move |res| {
                    let mut results = results_clone.lock().unwrap_or_else(|e| e.into_inner());
                    results.push(res);
                    if results.len() == total {
                        let combined: Result<Vec<String>, String> =
                            results.drain(..).collect::<Result<Vec<_>, _>>();
                        let summary = combined.map(|names| names.join(", "));
                        if let Some(cb) = callback_clone
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .take()
                        {
                            cb(summary);
                        }
                    }
                }),
            );
        }
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
    fn font_load_request_new() {
        let req = FontLoadRequest::new("OxiSans", "/fonts/oxi-sans.woff2");
        assert_eq!(req.family, "OxiSans");
        assert_eq!(req.url, "/fonts/oxi-sans.woff2");
        assert!(req.weight.is_none());
        assert!(req.style.is_none());
    }

    #[test]
    fn font_load_request_with_weight_and_style() {
        let req = FontLoadRequest::new("OxiSans", "/fonts/oxi-sans-bold.woff2")
            .with_weight("700")
            .with_style("italic");
        assert_eq!(req.weight.as_deref(), Some("700"));
        assert_eq!(req.style.as_deref(), Some("italic"));
    }

    #[test]
    fn css_src_wraps_url() {
        let req = FontLoadRequest::new("F", "https://example.com/font.woff2");
        assert_eq!(req.css_src(), "url('https://example.com/font.woff2')");
    }

    #[test]
    fn css_descriptors_default() {
        let req = FontLoadRequest::new("F", "f.woff2");
        let desc = req.css_descriptors();
        assert!(desc.contains("font-weight: normal"));
        assert!(desc.contains("font-style: normal"));
    }

    #[test]
    fn css_descriptors_custom() {
        let req = FontLoadRequest::new("F", "f.woff2")
            .with_weight("bold")
            .with_style("italic");
        let desc = req.css_descriptors();
        assert!(desc.contains("font-weight: bold"));
        assert!(desc.contains("font-style: italic"));
    }

    #[test]
    fn load_font_calls_callback_on_native() {
        let received = std::rc::Rc::new(std::cell::RefCell::new(
            Option::<Result<String, String>>::None,
        ));
        let received_clone = std::rc::Rc::clone(&received);
        load_font(
            FontLoadRequest::new("OxiSans", "/fonts/test.woff2"),
            Box::new(move |res| {
                *received_clone.borrow_mut() = Some(res);
            }),
        );
        // On native the callback is synchronous.
        let guard = received.borrow();
        assert!(guard.is_some());
        assert_eq!(*guard.as_ref().unwrap(), Ok("OxiSans".to_string()));
    }

    #[test]
    fn load_fonts_parallel_empty_ok_on_native() {
        let received = std::rc::Rc::new(std::cell::RefCell::new(
            Option::<Result<String, String>>::None,
        ));
        let received_clone = std::rc::Rc::clone(&received);
        load_fonts_parallel(
            vec![],
            Box::new(move |res| {
                *received_clone.borrow_mut() = Some(res);
            }),
        );
        let guard = received.borrow();
        assert!(guard.is_some());
        assert!(guard.as_ref().unwrap().is_ok());
    }

    #[test]
    fn load_fonts_parallel_multiple_ok_on_native() {
        let received = std::rc::Rc::new(std::cell::RefCell::new(
            Option::<Result<String, String>>::None,
        ));
        let received_clone = std::rc::Rc::clone(&received);
        let reqs = vec![
            FontLoadRequest::new("A", "a.woff2"),
            FontLoadRequest::new("B", "b.woff2"),
        ];
        load_fonts_parallel(
            reqs,
            Box::new(move |res| {
                *received_clone.borrow_mut() = Some(res);
            }),
        );
        let guard = received.borrow();
        assert!(guard.is_some());
        assert!(guard.as_ref().unwrap().is_ok());
    }
}
