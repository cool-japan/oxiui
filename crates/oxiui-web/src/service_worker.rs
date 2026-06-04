//! Service worker registration utilities for OxiUI web.
//!
//! Provides `register_service_worker` to register a service worker script
//! for offline caching of the wasm binary and assets.
//!
//! On non-wasm targets all functions are no-ops.

// ── Registration ──────────────────────────────────────────────────────────────

/// A callback invoked with the result of service worker registration.
///
/// Receives `Ok(scope)` where `scope` is the scope URL string on success, or
/// `Err(description)` on failure.
pub type ServiceWorkerCallback = Box<dyn FnOnce(Result<String, String>) + 'static>;

/// Register a service worker at the given script URL.
///
/// On `wasm32` this calls `navigator.serviceWorker.register(script_url)`.
/// The promise is awaited and the result forwarded to `callback`.
///
/// On non-wasm targets `callback` is invoked synchronously with
/// `Err("service workers not available on this target")`.
///
/// # Arguments
///
/// * `script_url` — path to the service worker JS file (e.g. `"/sw.js"`).
///   Must be accessible from the page's origin.
///
/// # Errors (callback)
///
/// `Err` is delivered if:
/// - `navigator.serviceWorker` is not available (non-secure context or old browser).
/// - The browser rejects the registration (network error, scope conflict, etc.).
#[allow(unused_variables)]
pub fn register_service_worker(script_url: &str, callback: ServiceWorkerCallback) {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsValue;
        use wasm_bindgen_futures::spawn_local;

        let url_owned = script_url.to_string();

        spawn_local(async move {
            let result: Result<String, String> = async {
                let window = web_sys::window()
                    .ok_or_else(|| "register_service_worker: no window".to_string())?;
                let navigator = window.navigator();
                let sw_container = navigator.service_worker();

                let promise = sw_container.register(&url_owned);
                let reg = wasm_bindgen_futures::JsFuture::from(promise)
                    .await
                    .map_err(|e: JsValue| {
                        e.as_string()
                            .unwrap_or_else(|| "service worker registration failed".to_string())
                    })?;

                // Extract the scope from the registration object.
                let scope = js_sys::Reflect::get(&reg, &JsValue::from_str("scope"))
                    .ok()
                    .and_then(|v| v.as_string())
                    .unwrap_or_else(|| url_owned.clone());

                Ok(scope)
            }
            .await;

            callback(result);
        });
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        callback(Err(
            "service workers not available on this target".to_string()
        ));
    }
}

/// Unregister all service workers for the current origin.
///
/// On `wasm32` this calls `navigator.serviceWorker.getRegistrations()` then
/// unregisters each one.  The callback is invoked once all unregistrations
/// complete.
///
/// On non-wasm targets the callback is invoked synchronously with `Ok(0)`
/// (zero registrations removed).
///
/// # Callback value
///
/// `Ok(count)` — the number of service workers that were successfully
/// unregistered.  `Err(description)` if the API is unavailable or fails.
pub type UnregisterCallback = Box<dyn FnOnce(Result<u32, String>) + 'static>;

/// Unregister all active service workers for the current origin.
///
/// See the module-level documentation for details.
///
/// # Errors (callback)
///
/// Returns `Err` if the service worker API is unavailable or fails.
#[allow(unused_variables)]
pub fn unregister_all_service_workers(callback: UnregisterCallback) {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsValue;
        use wasm_bindgen_futures::spawn_local;

        spawn_local(async move {
            let result: Result<u32, String> = async {
                let window = web_sys::window()
                    .ok_or_else(|| "unregister_all_service_workers: no window".to_string())?;
                let sw_container = window.navigator().service_worker();

                let regs_js =
                    wasm_bindgen_futures::JsFuture::from(sw_container.get_registrations())
                        .await
                        .map_err(|e: JsValue| {
                            e.as_string()
                                .unwrap_or_else(|| "getRegistrations failed".to_string())
                        })?;

                let regs: js_sys::Array = regs_js.into();
                let mut count = 0u32;

                for i in 0..regs.length() {
                    let reg = regs.get(i);
                    let reg_typed: web_sys::ServiceWorkerRegistration = reg.into();
                    let ok = wasm_bindgen_futures::JsFuture::from(reg_typed.unregister())
                        .await
                        .map(|v| v.as_bool().unwrap_or(false))
                        .unwrap_or(false);

                    if ok {
                        count += 1;
                    }
                }

                Ok(count)
            }
            .await;

            callback(result);
        });
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        callback(Ok(0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_service_worker_err_on_native() {
        let received = std::rc::Rc::new(std::cell::RefCell::new(
            Option::<Result<String, String>>::None,
        ));
        let received_clone = std::rc::Rc::clone(&received);
        register_service_worker(
            "/sw.js",
            Box::new(move |res| {
                *received_clone.borrow_mut() = Some(res);
            }),
        );
        // On native the callback is synchronous.
        let guard = received.borrow();
        assert!(guard.is_some());
        assert!(
            guard.as_ref().unwrap().is_err(),
            "should return Err on non-wasm targets"
        );
    }

    #[test]
    fn unregister_all_service_workers_ok_zero_on_native() {
        let received =
            std::rc::Rc::new(std::cell::RefCell::new(Option::<Result<u32, String>>::None));
        let received_clone = std::rc::Rc::clone(&received);
        unregister_all_service_workers(Box::new(move |res| {
            *received_clone.borrow_mut() = Some(res);
        }));
        let guard = received.borrow();
        assert!(guard.is_some());
        assert_eq!(*guard.as_ref().unwrap(), Ok(0));
    }
}
