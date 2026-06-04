//! Drag-and-drop event helpers for OxiUI web.
//!
//! Translates `dragenter`, `dragover`, `dragleave`, and `drop` DOM events
//! into structured Rust types.  `DataTransfer` access provides text content
//! and a list of dropped files.
//!
//! Pure translation helpers are testable on native targets.  The
//! `bind_drag_events` function that actually wires DOM listeners is
//! `wasm32`-only.

// ── Data types ────────────────────────────────────────────────────────────────

/// The type of a drag-and-drop interaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragEventKind {
    /// A dragged item has entered the drop target.
    Enter,
    /// A dragged item is being held over the drop target.
    Over,
    /// A dragged item has left the drop target without being dropped.
    Leave,
    /// The item was dropped onto the target.
    Drop,
}

/// Payload carried by a drag-and-drop event.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DragPayload {
    /// Plain-text content from `DataTransfer.getData("text/plain")`.
    ///
    /// Empty string if the transfer contained no plain text.
    pub text: String,

    /// URL content from `DataTransfer.getData("text/uri-list")`.
    ///
    /// Empty string if the transfer contained no URL.
    pub url: String,

    /// Names of files in `DataTransfer.files`.
    ///
    /// The names are extracted without reading file content (async reads
    /// require the `FileReader` API and are out of scope here).
    pub file_names: Vec<String>,
}

impl DragPayload {
    /// Returns `true` when the payload contains at least one file name.
    pub fn has_files(&self) -> bool {
        !self.file_names.is_empty()
    }

    /// Returns `true` when the payload contains plain text.
    pub fn has_text(&self) -> bool {
        !self.text.is_empty()
    }
}

/// A drag-and-drop event carrying its kind and payload.
#[derive(Clone, Debug)]
pub struct DragEvent {
    /// The interaction kind (enter / over / leave / drop).
    pub kind: DragEventKind,
    /// Payload data (only populated for [`DragEventKind::Drop`]).
    pub payload: DragPayload,
    /// Mouse position in viewport-relative logical pixels at the time of the event.
    pub x: f32,
    /// Mouse position in viewport-relative logical pixels at the time of the event.
    pub y: f32,
}

impl DragEvent {
    /// Create a non-drop drag event (enter / over / leave) with empty payload.
    pub fn navigation(kind: DragEventKind, x: f32, y: f32) -> Self {
        debug_assert!(
            kind != DragEventKind::Drop,
            "use DragEvent::drop() for drop events"
        );
        DragEvent {
            kind,
            payload: DragPayload::default(),
            x,
            y,
        }
    }

    /// Create a drop event with the given payload.
    pub fn drop(payload: DragPayload, x: f32, y: f32) -> Self {
        DragEvent {
            kind: DragEventKind::Drop,
            payload,
            x,
            y,
        }
    }
}

// ── wasm32 DOM binding ────────────────────────────────────────────────────────

/// Attach drag-and-drop event listeners to the canvas element.
///
/// On `wasm32` this wires `dragenter`, `dragover`, `dragleave`, and `drop`
/// listeners on the canvas.  Each event is translated into a [`DragEvent`]
/// and forwarded to the provided callback.
///
/// For `dragover` the default browser action (open file) is prevented so that
/// `drop` fires correctly.
///
/// On non-wasm targets this is always `Ok(())`.
///
/// # Errors
///
/// Returns `Err` if any DOM binding operation fails.
#[allow(unused_variables)]
pub fn bind_drag_events<F>(canvas_id: &str, on_event: F) -> Result<(), String>
where
    F: Fn(DragEvent) + 'static,
{
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::{closure::Closure, JsCast};

        let window =
            web_sys::window().ok_or_else(|| "bind_drag_events: no window available".to_string())?;
        let document = window
            .document()
            .ok_or_else(|| "bind_drag_events: no document available".to_string())?;
        let canvas = document
            .get_element_by_id(canvas_id)
            .ok_or_else(|| format!("bind_drag_events: canvas '{canvas_id}' not found"))?;

        let cb = std::sync::Arc::new(on_event);

        // dragenter
        {
            let cb = std::sync::Arc::clone(&cb);
            let closure = Closure::<dyn FnMut(web_sys::DragEvent)>::wrap(Box::new(
                move |e: web_sys::DragEvent| {
                    e.prevent_default();
                    cb(DragEvent::navigation(
                        DragEventKind::Enter,
                        e.client_x() as f32,
                        e.client_y() as f32,
                    ));
                },
            ));
            canvas
                .add_event_listener_with_callback("dragenter", closure.as_ref().unchecked_ref())
                .map_err(|_| "bind_drag_events: failed to add dragenter listener".to_string())?;
            closure.forget();
        }

        // dragover — must preventDefault to allow drop
        {
            let cb = std::sync::Arc::clone(&cb);
            let closure = Closure::<dyn FnMut(web_sys::DragEvent)>::wrap(Box::new(
                move |e: web_sys::DragEvent| {
                    e.prevent_default();
                    cb(DragEvent::navigation(
                        DragEventKind::Over,
                        e.client_x() as f32,
                        e.client_y() as f32,
                    ));
                },
            ));
            canvas
                .add_event_listener_with_callback("dragover", closure.as_ref().unchecked_ref())
                .map_err(|_| "bind_drag_events: failed to add dragover listener".to_string())?;
            closure.forget();
        }

        // dragleave
        {
            let cb = std::sync::Arc::clone(&cb);
            let closure = Closure::<dyn FnMut(web_sys::DragEvent)>::wrap(Box::new(
                move |e: web_sys::DragEvent| {
                    cb(DragEvent::navigation(
                        DragEventKind::Leave,
                        e.client_x() as f32,
                        e.client_y() as f32,
                    ));
                },
            ));
            canvas
                .add_event_listener_with_callback("dragleave", closure.as_ref().unchecked_ref())
                .map_err(|_| "bind_drag_events: failed to add dragleave listener".to_string())?;
            closure.forget();
        }

        // drop
        {
            let cb = std::sync::Arc::clone(&cb);
            let closure = Closure::<dyn FnMut(web_sys::DragEvent)>::wrap(Box::new(
                move |e: web_sys::DragEvent| {
                    e.prevent_default();
                    let payload = extract_drag_payload(&e);
                    cb(DragEvent::drop(
                        payload,
                        e.client_x() as f32,
                        e.client_y() as f32,
                    ));
                },
            ));
            canvas
                .add_event_listener_with_callback("drop", closure.as_ref().unchecked_ref())
                .map_err(|_| "bind_drag_events: failed to add drop listener".to_string())?;
            closure.forget();
        }

        Ok(())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Ok(())
    }
}

/// Extract a [`DragPayload`] from a DOM `DragEvent`.
///
/// Reads `text/plain` and `text/uri-list` data from `DataTransfer`, plus
/// the names of any transferred files.
///
/// This function is only compiled on wasm32; it is used internally by
/// `bind_drag_events`.
#[cfg(target_arch = "wasm32")]
fn extract_drag_payload(e: &web_sys::DragEvent) -> DragPayload {
    let mut payload = DragPayload::default();

    if let Some(dt) = e.data_transfer() {
        payload.text = dt.get_data("text/plain").unwrap_or_default();
        payload.url = dt.get_data("text/uri-list").unwrap_or_default();

        if let Some(files) = dt.files() {
            for i in 0..files.length() {
                if let Some(file) = files.item(i) {
                    payload.file_names.push(file.name());
                }
            }
        }
    }

    payload
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drag_payload_default_is_empty() {
        let p = DragPayload::default();
        assert!(!p.has_files());
        assert!(!p.has_text());
    }

    #[test]
    fn drag_payload_with_text() {
        let p = DragPayload {
            text: "hello".to_string(),
            ..Default::default()
        };
        assert!(p.has_text());
        assert!(!p.has_files());
    }

    #[test]
    fn drag_payload_with_files() {
        let p = DragPayload {
            file_names: vec!["foo.txt".to_string()],
            ..Default::default()
        };
        assert!(p.has_files());
        assert!(!p.has_text());
    }

    #[test]
    fn drag_event_navigation_kind() {
        let ev = DragEvent::navigation(DragEventKind::Enter, 10.0, 20.0);
        assert_eq!(ev.kind, DragEventKind::Enter);
        assert_eq!(ev.x, 10.0);
        assert_eq!(ev.y, 20.0);
        assert!(!ev.payload.has_text());
    }

    #[test]
    fn drag_event_drop_kind() {
        let payload = DragPayload {
            text: "dropped text".to_string(),
            ..Default::default()
        };
        let ev = DragEvent::drop(payload.clone(), 5.0, 15.0);
        assert_eq!(ev.kind, DragEventKind::Drop);
        assert_eq!(ev.payload.text, "dropped text");
        assert_eq!(ev.x, 5.0);
        assert_eq!(ev.y, 15.0);
    }

    #[test]
    fn bind_drag_events_noop_on_native() {
        let result = bind_drag_events("my-canvas", |_ev| {});
        assert!(result.is_ok());
    }
}
