//! Web event translation helpers.
//!
//! Provides pure functions that convert web-platform event data (described by
//! primitive types mirroring the browser DOM API) into [`oxiui_core::UiEvent`]
//! variants.  All functions operate on plain Rust values so they are testable on
//! native targets without any DOM or browser dependency.
//!
//! On `wasm32` targets the companion `bind_events` function wires `addEventListener`
//! closures for mouse, keyboard, wheel, and touch events onto a canvas element.

use oxiui_core::{
    events::{Modifiers, MouseButton, ScrollDelta},
    UiEvent,
};

use crate::map_web_key;

// ── Modifier helpers ──────────────────────────────────────────────────────────

/// Build a [`Modifiers`] value from the four bool fields present on
/// `MouseEvent`, `KeyboardEvent`, and `WheelEvent`.
pub fn make_modifiers(ctrl: bool, alt: bool, shift: bool, meta: bool) -> Modifiers {
    Modifiers {
        ctrl,
        alt,
        shift,
        meta,
    }
}

// ── Mouse button helpers ──────────────────────────────────────────────────────

/// Map a DOM `button` field (i16) to [`MouseButton`].
///
/// | value | meaning |
/// |-------|---------|
/// | 0     | Left    |
/// | 1     | Middle  |
/// | 2     | Right   |
/// | n     | Other(n)|
pub fn map_mouse_button(button: i16) -> MouseButton {
    match button {
        0 => MouseButton::Left,
        1 => MouseButton::Middle,
        2 => MouseButton::Right,
        n if n >= 0 => MouseButton::Other(n as u16),
        _ => MouseButton::Other(0),
    }
}

// ── Mouse event constructors ──────────────────────────────────────────────────

/// Create a [`UiEvent::MouseDown`] from raw DOM values.
///
/// `client_x` / `client_y` are the logical-pixel coordinates relative to the
/// viewport (as reported by `MouseEvent.clientX` / `.clientY`).
/// `button` is `MouseEvent.button`.
/// Modifier booleans are read from the event's shift/ctrl/alt/meta properties.
pub fn mouse_down_event(
    client_x: f32,
    client_y: f32,
    button: i16,
    ctrl: bool,
    alt: bool,
    shift: bool,
    meta: bool,
) -> UiEvent {
    UiEvent::MouseDown {
        button: map_mouse_button(button),
        x: client_x,
        y: client_y,
        modifiers: make_modifiers(ctrl, alt, shift, meta),
    }
}

/// Create a [`UiEvent::MouseUp`] from raw DOM values.
pub fn mouse_up_event(
    client_x: f32,
    client_y: f32,
    button: i16,
    ctrl: bool,
    alt: bool,
    shift: bool,
    meta: bool,
) -> UiEvent {
    UiEvent::MouseUp {
        button: map_mouse_button(button),
        x: client_x,
        y: client_y,
        modifiers: make_modifiers(ctrl, alt, shift, meta),
    }
}

/// Create a [`UiEvent::MouseMove`] from raw DOM values.
pub fn mouse_move_event(client_x: f32, client_y: f32) -> UiEvent {
    UiEvent::MouseMove {
        x: client_x,
        y: client_y,
    }
}

/// Create a [`UiEvent::Mouse`] (legacy position event) from raw DOM values.
pub fn mouse_position_event(client_x: f32, client_y: f32) -> UiEvent {
    UiEvent::Mouse {
        x: client_x,
        y: client_y,
    }
}

// ── Wheel event constructor ───────────────────────────────────────────────────

/// Web `WheelEvent.deltaMode` constants (matches the DOM spec).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WheelDeltaMode {
    /// `WheelEvent.DOM_DELTA_PIXEL` (0) — pixel-precise smooth scrolling.
    Pixel,
    /// `WheelEvent.DOM_DELTA_LINE` (1) — discrete line scrolling.
    Line,
    /// `WheelEvent.DOM_DELTA_PAGE` (2) — full-page scrolling, treated as lines.
    Page,
}

impl WheelDeltaMode {
    /// Construct from the raw DOM integer value.
    pub fn from_dom(raw: u32) -> Self {
        match raw {
            0 => WheelDeltaMode::Pixel,
            1 => WheelDeltaMode::Line,
            _ => WheelDeltaMode::Page,
        }
    }
}

/// Create a [`UiEvent::Wheel`] from raw DOM `WheelEvent` values.
///
/// `delta_x` / `delta_y` are the raw delta values; `delta_mode` selects the
/// unit.  Page-mode deltas are converted to 3 lines per page.
pub fn wheel_event(delta_x: f64, delta_y: f64, delta_mode: WheelDeltaMode) -> UiEvent {
    let delta = match delta_mode {
        WheelDeltaMode::Pixel => ScrollDelta::Pixels {
            x: delta_x as f32,
            y: delta_y as f32,
        },
        WheelDeltaMode::Line => ScrollDelta::Lines {
            x: delta_x as f32,
            y: delta_y as f32,
        },
        WheelDeltaMode::Page => {
            // Treat 1 page as 3 discrete lines.
            ScrollDelta::Lines {
                x: (delta_x * 3.0) as f32,
                y: (delta_y * 3.0) as f32,
            }
        }
    };
    UiEvent::Wheel(delta)
}

// ── Keyboard event constructors ───────────────────────────────────────────────

/// Create a [`UiEvent::KeyDown`] from raw DOM `KeyboardEvent` values.
///
/// `key` is the DOM `key` string (e.g. `"Enter"`, `"a"`, `"ArrowLeft"`).
/// `repeat` corresponds to `KeyboardEvent.repeat`.
pub fn key_down_event(
    key: &str,
    ctrl: bool,
    alt: bool,
    shift: bool,
    meta: bool,
    repeat: bool,
) -> UiEvent {
    UiEvent::KeyDown {
        key: map_web_key(key),
        modifiers: make_modifiers(ctrl, alt, shift, meta),
        repeat,
    }
}

/// Create a [`UiEvent::KeyUp`] from raw DOM `KeyboardEvent` values.
pub fn key_up_event(key: &str, ctrl: bool, alt: bool, shift: bool, meta: bool) -> UiEvent {
    UiEvent::KeyUp {
        key: map_web_key(key),
        modifiers: make_modifiers(ctrl, alt, shift, meta),
    }
}

/// Create a legacy [`UiEvent::KeyPress`] from a DOM `key` string.
///
/// This is the older event type used by some older adapters; prefer
/// [`key_down_event`] for new code.
pub fn key_press_event(key: &str) -> UiEvent {
    UiEvent::KeyPress(key.to_string())
}

// ── Touch event helpers ───────────────────────────────────────────────────────

/// A single touch point — mirrors the relevant subset of the DOM `Touch`
/// interface, using plain Rust types so it is testable without the DOM.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TouchPoint {
    /// Touch identifier (unique per active touch).
    pub identifier: i32,
    /// X position in logical pixels (relative to the viewport).
    pub client_x: f32,
    /// Y position in logical pixels (relative to the viewport).
    pub client_y: f32,
}

/// Translate the first touch point from a `touchstart`/`touchmove` event into
/// a [`UiEvent::Mouse`] position event.
///
/// The browser fires touch events but OxiUI's core event model currently maps
/// single-touch interactions to mouse equivalents.  Multi-touch is forwarded
/// as independent mouse events per touch point.
pub fn touch_to_mouse_move(touch: TouchPoint) -> UiEvent {
    UiEvent::Mouse {
        x: touch.client_x,
        y: touch.client_y,
    }
}

/// Translate a `touchstart` touch point into a [`UiEvent::MouseDown`].
pub fn touch_start_to_mouse_down(touch: TouchPoint) -> UiEvent {
    UiEvent::MouseDown {
        button: MouseButton::Left,
        x: touch.client_x,
        y: touch.client_y,
        modifiers: Modifiers::NONE,
    }
}

/// Translate a `touchend` touch point into a [`UiEvent::MouseUp`].
pub fn touch_end_to_mouse_up(touch: TouchPoint) -> UiEvent {
    UiEvent::MouseUp {
        button: MouseButton::Left,
        x: touch.client_x,
        y: touch.client_y,
        modifiers: Modifiers::NONE,
    }
}

// ── wasm32 event binding ──────────────────────────────────────────────────────

/// Attach all DOM event listeners to the canvas element.
///
/// On `wasm32` targets this function wires `addEventListener` closures for
/// mouse, keyboard, wheel, and touch events, translating each into a
/// [`UiEvent`] that is forwarded to the provided callback.
///
/// On non-wasm targets this is a no-op stub (always returns `Ok(())`).
///
/// # Errors
///
/// Returns `Err` with a description string if any DOM binding fails.
#[allow(unused_variables)]
pub fn bind_events<F>(canvas_id: &str, on_event: F) -> Result<(), String>
where
    F: Fn(UiEvent) + 'static,
{
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::{closure::Closure, JsCast};

        let window =
            web_sys::window().ok_or_else(|| "bind_events: no window available".to_string())?;
        let document = window
            .document()
            .ok_or_else(|| "bind_events: no document available".to_string())?;
        let canvas = document
            .get_element_by_id(canvas_id)
            .ok_or_else(|| format!("bind_events: canvas '{canvas_id}' not found"))?;
        let canvas: web_sys::HtmlCanvasElement = canvas
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .map_err(|_| format!("bind_events: '{canvas_id}' is not a canvas"))?;

        // Use Arc<F> so callbacks can share the closure without moves.
        let cb = std::sync::Arc::new(on_event);

        // ── mouse events ─────────────────────────────────────────────────────
        {
            let cb = std::sync::Arc::clone(&cb);
            let closure = Closure::<dyn FnMut(web_sys::MouseEvent)>::wrap(Box::new(
                move |e: web_sys::MouseEvent| {
                    let ev = mouse_down_event(
                        e.client_x() as f32,
                        e.client_y() as f32,
                        e.button(),
                        e.ctrl_key(),
                        e.alt_key(),
                        e.shift_key(),
                        e.meta_key(),
                    );
                    cb(ev);
                },
            ));
            canvas
                .add_event_listener_with_callback("mousedown", closure.as_ref().unchecked_ref())
                .map_err(|_| "bind_events: failed to add mousedown listener".to_string())?;
            closure.forget(); // leak intentionally — lives for canvas lifetime
        }

        {
            let cb = std::sync::Arc::clone(&cb);
            let closure = Closure::<dyn FnMut(web_sys::MouseEvent)>::wrap(Box::new(
                move |e: web_sys::MouseEvent| {
                    let ev = mouse_up_event(
                        e.client_x() as f32,
                        e.client_y() as f32,
                        e.button(),
                        e.ctrl_key(),
                        e.alt_key(),
                        e.shift_key(),
                        e.meta_key(),
                    );
                    cb(ev);
                },
            ));
            canvas
                .add_event_listener_with_callback("mouseup", closure.as_ref().unchecked_ref())
                .map_err(|_| "bind_events: failed to add mouseup listener".to_string())?;
            closure.forget();
        }

        {
            let cb = std::sync::Arc::clone(&cb);
            let closure = Closure::<dyn FnMut(web_sys::MouseEvent)>::wrap(Box::new(
                move |e: web_sys::MouseEvent| {
                    cb(mouse_move_event(e.client_x() as f32, e.client_y() as f32));
                },
            ));
            canvas
                .add_event_listener_with_callback("mousemove", closure.as_ref().unchecked_ref())
                .map_err(|_| "bind_events: failed to add mousemove listener".to_string())?;
            closure.forget();
        }

        // ── wheel event ──────────────────────────────────────────────────────
        {
            let cb = std::sync::Arc::clone(&cb);
            let closure = Closure::<dyn FnMut(web_sys::WheelEvent)>::wrap(Box::new(
                move |e: web_sys::WheelEvent| {
                    let mode = WheelDeltaMode::from_dom(e.delta_mode());
                    cb(wheel_event(e.delta_x(), e.delta_y(), mode));
                },
            ));
            canvas
                .add_event_listener_with_callback("wheel", closure.as_ref().unchecked_ref())
                .map_err(|_| "bind_events: failed to add wheel listener".to_string())?;
            closure.forget();
        }

        // ── keyboard events (on window — canvas doesn't receive key events
        //    unless focused with tabindex, so attach to window for reliability)
        {
            let cb = std::sync::Arc::clone(&cb);
            let closure = Closure::<dyn FnMut(web_sys::KeyboardEvent)>::wrap(Box::new(
                move |e: web_sys::KeyboardEvent| {
                    let ev = key_down_event(
                        &e.key(),
                        e.ctrl_key(),
                        e.alt_key(),
                        e.shift_key(),
                        e.meta_key(),
                        e.repeat(),
                    );
                    cb(ev);
                },
            ));
            window
                .add_event_listener_with_callback("keydown", closure.as_ref().unchecked_ref())
                .map_err(|_| "bind_events: failed to add keydown listener".to_string())?;
            closure.forget();
        }

        {
            let cb = std::sync::Arc::clone(&cb);
            let closure = Closure::<dyn FnMut(web_sys::KeyboardEvent)>::wrap(Box::new(
                move |e: web_sys::KeyboardEvent| {
                    let ev = key_up_event(
                        &e.key(),
                        e.ctrl_key(),
                        e.alt_key(),
                        e.shift_key(),
                        e.meta_key(),
                    );
                    cb(ev);
                },
            ));
            window
                .add_event_listener_with_callback("keyup", closure.as_ref().unchecked_ref())
                .map_err(|_| "bind_events: failed to add keyup listener".to_string())?;
            closure.forget();
        }

        // ── touch events ─────────────────────────────────────────────────────
        {
            let cb = std::sync::Arc::clone(&cb);
            let closure = Closure::<dyn FnMut(web_sys::TouchEvent)>::wrap(Box::new(
                move |e: web_sys::TouchEvent| {
                    let touches = e.changed_touches();
                    for i in 0..touches.length() {
                        if let Some(t) = touches.item(i) {
                            let point = TouchPoint {
                                identifier: t.identifier(),
                                client_x: t.client_x() as f32,
                                client_y: t.client_y() as f32,
                            };
                            cb(touch_start_to_mouse_down(point));
                        }
                    }
                },
            ));
            canvas
                .add_event_listener_with_callback("touchstart", closure.as_ref().unchecked_ref())
                .map_err(|_| "bind_events: failed to add touchstart listener".to_string())?;
            closure.forget();
        }

        {
            let cb = std::sync::Arc::clone(&cb);
            let closure = Closure::<dyn FnMut(web_sys::TouchEvent)>::wrap(Box::new(
                move |e: web_sys::TouchEvent| {
                    let touches = e.changed_touches();
                    for i in 0..touches.length() {
                        if let Some(t) = touches.item(i) {
                            let point = TouchPoint {
                                identifier: t.identifier(),
                                client_x: t.client_x() as f32,
                                client_y: t.client_y() as f32,
                            };
                            cb(touch_to_mouse_move(point));
                        }
                    }
                },
            ));
            canvas
                .add_event_listener_with_callback("touchmove", closure.as_ref().unchecked_ref())
                .map_err(|_| "bind_events: failed to add touchmove listener".to_string())?;
            closure.forget();
        }

        {
            let cb = std::sync::Arc::clone(&cb);
            let closure = Closure::<dyn FnMut(web_sys::TouchEvent)>::wrap(Box::new(
                move |e: web_sys::TouchEvent| {
                    let touches = e.changed_touches();
                    for i in 0..touches.length() {
                        if let Some(t) = touches.item(i) {
                            let point = TouchPoint {
                                identifier: t.identifier(),
                                client_x: t.client_x() as f32,
                                client_y: t.client_y() as f32,
                            };
                            cb(touch_end_to_mouse_up(point));
                        }
                    }
                },
            ));
            canvas
                .add_event_listener_with_callback("touchend", closure.as_ref().unchecked_ref())
                .map_err(|_| "bind_events: failed to add touchend listener".to_string())?;
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
    use oxiui_core::{
        events::{MouseButton, ScrollDelta},
        Key, UiEvent,
    };

    #[test]
    fn map_mouse_button_left() {
        assert_eq!(map_mouse_button(0), MouseButton::Left);
    }

    #[test]
    fn map_mouse_button_middle() {
        assert_eq!(map_mouse_button(1), MouseButton::Middle);
    }

    #[test]
    fn map_mouse_button_right() {
        assert_eq!(map_mouse_button(2), MouseButton::Right);
    }

    #[test]
    fn map_mouse_button_other() {
        assert_eq!(map_mouse_button(4), MouseButton::Other(4));
    }

    #[test]
    fn mouse_down_event_fields() {
        let ev = mouse_down_event(10.0, 20.0, 0, false, false, true, false);
        match ev {
            UiEvent::MouseDown {
                button,
                x,
                y,
                modifiers,
            } => {
                assert_eq!(button, MouseButton::Left);
                assert_eq!(x, 10.0);
                assert_eq!(y, 20.0);
                assert!(modifiers.shift);
                assert!(!modifiers.ctrl);
            }
            other => panic!("expected MouseDown, got {other:?}"),
        }
    }

    #[test]
    fn mouse_up_event_fields() {
        let ev = mouse_up_event(5.0, 15.0, 2, true, false, false, false);
        match ev {
            UiEvent::MouseUp {
                button,
                x,
                y,
                modifiers,
            } => {
                assert_eq!(button, MouseButton::Right);
                assert_eq!(x, 5.0);
                assert_eq!(y, 15.0);
                assert!(modifiers.ctrl);
            }
            other => panic!("expected MouseUp, got {other:?}"),
        }
    }

    #[test]
    fn mouse_move_event_fields() {
        let ev = mouse_move_event(100.0, 200.0);
        match ev {
            UiEvent::MouseMove { x, y } => {
                assert_eq!(x, 100.0);
                assert_eq!(y, 200.0);
            }
            other => panic!("expected MouseMove, got {other:?}"),
        }
    }

    #[test]
    fn wheel_event_pixel_mode() {
        let ev = wheel_event(0.0, 120.0, WheelDeltaMode::Pixel);
        match ev {
            UiEvent::Wheel(ScrollDelta::Pixels { x, y }) => {
                assert_eq!(x, 0.0);
                assert_eq!(y, 120.0);
            }
            other => panic!("expected Wheel(Pixels), got {other:?}"),
        }
    }

    #[test]
    fn wheel_event_line_mode() {
        let ev = wheel_event(0.0, 3.0, WheelDeltaMode::Line);
        match ev {
            UiEvent::Wheel(ScrollDelta::Lines { x, y }) => {
                assert_eq!(x, 0.0);
                assert_eq!(y, 3.0);
            }
            other => panic!("expected Wheel(Lines), got {other:?}"),
        }
    }

    #[test]
    fn wheel_event_page_mode_multiplies_by_3() {
        let ev = wheel_event(0.0, 1.0, WheelDeltaMode::Page);
        match ev {
            UiEvent::Wheel(ScrollDelta::Lines { x, y }) => {
                assert_eq!(x, 0.0);
                assert_eq!(y, 3.0);
            }
            other => panic!("expected Wheel(Lines) for page mode, got {other:?}"),
        }
    }

    #[test]
    fn key_down_enter_event() {
        let ev = key_down_event("Enter", false, false, false, false, false);
        match ev {
            UiEvent::KeyDown {
                key,
                modifiers,
                repeat,
            } => {
                assert_eq!(key, Key::Enter);
                assert!(modifiers.is_empty());
                assert!(!repeat);
            }
            other => panic!("expected KeyDown, got {other:?}"),
        }
    }

    #[test]
    fn key_down_with_modifiers() {
        let ev = key_down_event("a", true, false, false, false, false);
        match ev {
            UiEvent::KeyDown { key, modifiers, .. } => {
                assert_eq!(key, Key::Character("a".to_string()));
                assert!(modifiers.ctrl);
                assert!(!modifiers.shift);
            }
            other => panic!("expected KeyDown, got {other:?}"),
        }
    }

    #[test]
    fn key_down_repeat_flag() {
        let ev = key_down_event("ArrowDown", false, false, false, false, true);
        match ev {
            UiEvent::KeyDown { repeat, .. } => {
                assert!(repeat);
            }
            other => panic!("expected KeyDown, got {other:?}"),
        }
    }

    #[test]
    fn key_up_event_fields() {
        let ev = key_up_event("Escape", false, false, false, false);
        match ev {
            UiEvent::KeyUp { key, modifiers } => {
                assert_eq!(key, Key::Escape);
                assert!(modifiers.is_empty());
            }
            other => panic!("expected KeyUp, got {other:?}"),
        }
    }

    #[test]
    fn touch_to_mouse_move_fields() {
        let tp = TouchPoint {
            identifier: 1,
            client_x: 50.0,
            client_y: 75.0,
        };
        let ev = touch_to_mouse_move(tp);
        match ev {
            UiEvent::Mouse { x, y } => {
                assert_eq!(x, 50.0);
                assert_eq!(y, 75.0);
            }
            other => panic!("expected Mouse, got {other:?}"),
        }
    }

    #[test]
    fn touch_start_produces_left_mouse_down() {
        let tp = TouchPoint {
            identifier: 0,
            client_x: 30.0,
            client_y: 40.0,
        };
        let ev = touch_start_to_mouse_down(tp);
        match ev {
            UiEvent::MouseDown {
                button,
                x,
                y,
                modifiers,
            } => {
                assert_eq!(button, MouseButton::Left);
                assert_eq!(x, 30.0);
                assert_eq!(y, 40.0);
                assert!(modifiers.is_empty());
            }
            other => panic!("expected MouseDown, got {other:?}"),
        }
    }

    #[test]
    fn touch_end_produces_left_mouse_up() {
        let tp = TouchPoint {
            identifier: 0,
            client_x: 30.0,
            client_y: 40.0,
        };
        let ev = touch_end_to_mouse_up(tp);
        match ev {
            UiEvent::MouseUp {
                button,
                x,
                y,
                modifiers,
            } => {
                assert_eq!(button, MouseButton::Left);
                assert_eq!(x, 30.0);
                assert_eq!(y, 40.0);
                assert!(modifiers.is_empty());
            }
            other => panic!("expected MouseUp, got {other:?}"),
        }
    }

    #[test]
    fn make_modifiers_all_false() {
        let m = make_modifiers(false, false, false, false);
        assert!(m.is_empty());
    }

    #[test]
    fn make_modifiers_all_true() {
        let m = make_modifiers(true, true, true, true);
        assert!(m.ctrl);
        assert!(m.alt);
        assert!(m.shift);
        assert!(m.meta);
    }

    #[test]
    fn wheel_delta_mode_from_dom() {
        assert_eq!(WheelDeltaMode::from_dom(0), WheelDeltaMode::Pixel);
        assert_eq!(WheelDeltaMode::from_dom(1), WheelDeltaMode::Line);
        assert_eq!(WheelDeltaMode::from_dom(2), WheelDeltaMode::Page);
        // Unknown values treated as Page.
        assert_eq!(WheelDeltaMode::from_dom(99), WheelDeltaMode::Page);
    }

    #[test]
    fn bind_events_noop_on_native() {
        let result = bind_events("any-canvas", |_ev| {});
        assert!(result.is_ok());
    }
}
