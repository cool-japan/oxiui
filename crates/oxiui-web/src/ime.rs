//! IME (Input Method Editor) composition event helpers.
//!
//! Translates `compositionstart`, `compositionupdate`, and `compositionend`
//! DOM events into [`oxiui_core::UiEvent::ImePreedit`] and
//! [`oxiui_core::UiEvent::ImeCommit`] variants.
//!
//! All translation functions operate on plain Rust types (strings + booleans)
//! so they are fully testable on native targets without any DOM or browser
//! dependency.

use oxiui_core::UiEvent;

// ── IME event constructors ────────────────────────────────────────────────────

/// Create a [`UiEvent::ImePreedit`] for the in-progress composition text.
///
/// Called on `compositionstart` (with empty `text`) and `compositionupdate`
/// (with the current preedit string).
///
/// `text` is the current composition string (may be empty on `compositionstart`).
/// `cursor` is an optional byte-range `(start, end)` that should be highlighted
/// as the composition cursor; pass `None` when not provided by the platform.
pub fn ime_preedit_event(text: impl Into<String>, cursor: Option<(usize, usize)>) -> UiEvent {
    UiEvent::ImePreedit {
        text: text.into(),
        cursor,
    }
}

/// Create a [`UiEvent::ImeCommit`] for the finalised composition text.
///
/// Called on `compositionend`. `text` is the committed string that should be
/// inserted into the active text-input field.
pub fn ime_commit_event(text: impl Into<String>) -> UiEvent {
    UiEvent::ImeCommit(text.into())
}

/// Synthesise the sequence of events for a complete IME composition cycle.
///
/// Given a `preedit` string (the in-progress text) and a `commit` string
/// (the finalised text), returns the three events that represent:
/// 1. `compositionstart` — empty preedit.
/// 2. `compositionupdate` — preedit string with an optional cursor hint.
/// 3. `compositionend` → `ImeCommit` — the committed text.
///
/// This is primarily useful for tests and for simulating IME input.
pub fn ime_full_cycle(
    preedit: impl Into<String>,
    commit: impl Into<String>,
    cursor: Option<(usize, usize)>,
) -> [UiEvent; 3] {
    let preedit = preedit.into();
    let commit = commit.into();
    [
        ime_preedit_event("", None),
        ime_preedit_event(preedit, cursor),
        ime_commit_event(commit),
    ]
}

// ── wasm32 IME binding ────────────────────────────────────────────────────────

/// Attach IME composition event listeners to the document's active element or
/// the canvas element.
///
/// On `wasm32` this wires `compositionstart`, `compositionupdate`, and
/// `compositionend` listeners on the `window`'s body (composition events
/// bubble up from any focused element).  Each event is translated into the
/// appropriate [`UiEvent`] and forwarded to the provided callback.
///
/// On non-wasm targets this is a no-op stub that always returns `Ok(())`.
///
/// # Errors
///
/// Returns `Err` with a description string if any DOM binding fails.
#[allow(unused_variables)]
pub fn bind_ime_events<F>(on_event: F) -> Result<(), String>
where
    F: Fn(UiEvent) + 'static,
{
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::{closure::Closure, JsCast};

        let window =
            web_sys::window().ok_or_else(|| "bind_ime_events: no window available".to_string())?;

        let cb = std::sync::Arc::new(on_event);

        // compositionstart
        {
            let cb = std::sync::Arc::clone(&cb);
            let closure = Closure::<dyn FnMut(web_sys::CompositionEvent)>::wrap(Box::new(
                move |e: web_sys::CompositionEvent| {
                    let text = e.data().unwrap_or_default();
                    cb(ime_preedit_event(text, None));
                },
            ));
            window
                .add_event_listener_with_callback(
                    "compositionstart",
                    closure.as_ref().unchecked_ref(),
                )
                .map_err(|_| {
                    "bind_ime_events: failed to add compositionstart listener".to_string()
                })?;
            closure.forget();
        }

        // compositionupdate
        {
            let cb = std::sync::Arc::clone(&cb);
            let closure = Closure::<dyn FnMut(web_sys::CompositionEvent)>::wrap(Box::new(
                move |e: web_sys::CompositionEvent| {
                    let text = e.data().unwrap_or_default();
                    let len = text.len();
                    // Provide a cursor hint at the end of the preedit text.
                    let cursor = if len > 0 { Some((len, len)) } else { None };
                    cb(ime_preedit_event(text, cursor));
                },
            ));
            window
                .add_event_listener_with_callback(
                    "compositionupdate",
                    closure.as_ref().unchecked_ref(),
                )
                .map_err(|_| {
                    "bind_ime_events: failed to add compositionupdate listener".to_string()
                })?;
            closure.forget();
        }

        // compositionend
        {
            let cb = std::sync::Arc::clone(&cb);
            let closure = Closure::<dyn FnMut(web_sys::CompositionEvent)>::wrap(Box::new(
                move |e: web_sys::CompositionEvent| {
                    let text = e.data().unwrap_or_default();
                    cb(ime_commit_event(text));
                },
            ));
            window
                .add_event_listener_with_callback(
                    "compositionend",
                    closure.as_ref().unchecked_ref(),
                )
                .map_err(|_| {
                    "bind_ime_events: failed to add compositionend listener".to_string()
                })?;
            closure.forget();
        }

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
    use oxiui_core::UiEvent;

    #[test]
    fn ime_preedit_event_empty() {
        let ev = ime_preedit_event("", None);
        match ev {
            UiEvent::ImePreedit { text, cursor } => {
                assert!(text.is_empty());
                assert!(cursor.is_none());
            }
            other => panic!("expected ImePreedit, got {other:?}"),
        }
    }

    #[test]
    fn ime_preedit_event_with_text_and_cursor() {
        let ev = ime_preedit_event("hello", Some((3, 5)));
        match ev {
            UiEvent::ImePreedit { text, cursor } => {
                assert_eq!(text, "hello");
                assert_eq!(cursor, Some((3, 5)));
            }
            other => panic!("expected ImePreedit, got {other:?}"),
        }
    }

    #[test]
    fn ime_commit_event_fields() {
        let ev = ime_commit_event("確定");
        match ev {
            UiEvent::ImeCommit(text) => {
                assert_eq!(text, "確定");
            }
            other => panic!("expected ImeCommit, got {other:?}"),
        }
    }

    #[test]
    fn ime_full_cycle_produces_three_events() {
        let [start, update, commit] = ime_full_cycle("にほんご", "日本語", Some((4, 4)));
        // First event: empty preedit (compositionstart)
        match start {
            UiEvent::ImePreedit { text, cursor } => {
                assert!(text.is_empty());
                assert!(cursor.is_none());
            }
            other => panic!("expected ImePreedit for start, got {other:?}"),
        }
        // Second event: preedit text (compositionupdate)
        match update {
            UiEvent::ImePreedit { text, cursor } => {
                assert_eq!(text, "にほんご");
                assert_eq!(cursor, Some((4, 4)));
            }
            other => panic!("expected ImePreedit for update, got {other:?}"),
        }
        // Third event: commit (compositionend)
        match commit {
            UiEvent::ImeCommit(text) => {
                assert_eq!(text, "日本語");
            }
            other => panic!("expected ImeCommit for commit, got {other:?}"),
        }
    }

    #[test]
    fn bind_ime_events_noop_on_native() {
        let result = bind_ime_events(|_ev| {});
        assert!(result.is_ok());
    }
}
