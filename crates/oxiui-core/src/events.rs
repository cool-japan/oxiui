//! Input event types: mouse buttons, keyboard keys, and modifier state.
//!
//! These types back the richer [`UiEvent`](crate::UiEvent) variants
//! (`MouseDown`, `KeyDown`, …) added on top of the original immediate-mode
//! event set. The original variants (`Resize`, `Mouse`, `KeyPress`, IME …)
//! are retained for backward compatibility with existing adapters.

/// A mouse button.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum MouseButton {
    /// Primary (usually left) button.
    Left,
    /// Secondary (usually right) button.
    Right,
    /// Middle / wheel button.
    Middle,
    /// Any other button, identified by platform index.
    Other(u16),
}

/// Keyboard modifier-key state.
///
/// `meta` is the Command key on macOS and the Super/Windows key elsewhere.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Modifiers {
    /// Control key held.
    pub ctrl: bool,
    /// Alt / Option key held.
    pub alt: bool,
    /// Shift key held.
    pub shift: bool,
    /// Meta (Command / Super / Windows) key held.
    pub meta: bool,
}

impl Modifiers {
    /// No modifiers held.
    pub const NONE: Modifiers = Modifiers {
        ctrl: false,
        alt: false,
        shift: false,
        meta: false,
    };

    /// Returns `true` if no modifier keys are held.
    pub fn is_empty(self) -> bool {
        !self.ctrl && !self.alt && !self.shift && !self.meta
    }

    /// Returns `true` if the platform "command" modifier is held: `meta` on
    /// macOS-style platforms, `ctrl` elsewhere. Callers that don't track the
    /// platform can treat either as the accelerator modifier.
    pub fn command(self) -> bool {
        self.ctrl || self.meta
    }
}

/// A logical key, abstracted away from physical scan codes.
///
/// `Character` carries the produced text for printable keys; named variants
/// cover the common non-printable keys needed for navigation and editing.
///
/// The enum is `#[non_exhaustive]`: new variants added in future releases will
/// deserialise as a serde unknown-field error rather than silently mismatching.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Key {
    /// A printable character (already mapped through the active layout/IME).
    Character(String),
    /// Enter / Return.
    Enter,
    /// Tab.
    Tab,
    /// Space bar.
    Space,
    /// Backspace.
    Backspace,
    /// Delete (forward delete).
    Delete,
    /// Escape.
    Escape,
    /// Left arrow.
    ArrowLeft,
    /// Right arrow.
    ArrowRight,
    /// Up arrow.
    ArrowUp,
    /// Down arrow.
    ArrowDown,
    /// Home.
    Home,
    /// End.
    End,
    /// Page Up.
    PageUp,
    /// Page Down.
    PageDown,
    /// A function key `F1`–`F24` (the `u8` is the number, 1-based).
    Function(u8),
    /// Any other named key, identified by string (forward-compatible escape).
    Named(String),
}

impl Key {
    /// If this key produces text, return it; otherwise `None`.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Key::Character(s) => Some(s.as_str()),
            Key::Space => Some(" "),
            _ => None,
        }
    }
}

/// Mouse scroll-wheel delta.
///
/// Positive `y` scrolls content up (wheel away from the user) by convention.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ScrollDelta {
    /// Discrete line-based scrolling (mouse wheel notches).
    Lines {
        /// Horizontal lines.
        x: f32,
        /// Vertical lines.
        y: f32,
    },
    /// Smooth pixel-precise scrolling (trackpads).
    Pixels {
        /// Horizontal pixels.
        x: f32,
        /// Vertical pixels.
        y: f32,
    },
}

// ── Rich pointer / keyboard / touch events (dispatch layer) ─────────────────
//
// The variants below feed the [`EventDispatcher`](crate::dispatch::EventDispatcher)
// capture/bubble pipeline. They are richer than the immediate-mode
// [`UiEvent`](crate::UiEvent) set (which adapters emit) and carry geometry in
// [`Point`](crate::geometry::Point) form for hit testing.

use crate::geometry::Point;

/// A pointer (mouse / trackpad) event in tree-local coordinates.
#[derive(Clone, Debug, PartialEq)]
pub enum MouseEvent {
    /// A button was pressed.
    Down {
        /// Position in tree-local logical pixels.
        pos: Point,
        /// Which button.
        button: MouseButton,
        /// Modifier state.
        modifiers: Modifiers,
    },
    /// A button was released.
    Up {
        /// Position in tree-local logical pixels.
        pos: Point,
        /// Which button.
        button: MouseButton,
        /// Modifier state.
        modifiers: Modifiers,
    },
    /// The pointer moved without a press/release.
    Move {
        /// New position.
        pos: Point,
        /// Modifier state.
        modifiers: Modifiers,
    },
    /// The pointer entered a widget's bounds.
    Enter {
        /// Position at entry.
        pos: Point,
    },
    /// The pointer left a widget's bounds.
    Leave {
        /// Position at exit (last known).
        pos: Point,
    },
    /// A double click (two `Down`s within the platform double-click window).
    DoubleClick {
        /// Position.
        pos: Point,
        /// Which button.
        button: MouseButton,
        /// Modifier state.
        modifiers: Modifiers,
    },
    /// A triple click (three rapid `Down`s; selects a paragraph by convention).
    TripleClick {
        /// Position.
        pos: Point,
        /// Which button.
        button: MouseButton,
        /// Modifier state.
        modifiers: Modifiers,
    },
    /// A scroll-wheel / trackpad scroll, discrete or smooth (see [`ScrollDelta`]).
    Scroll {
        /// Position of the pointer when the scroll occurred.
        pos: Point,
        /// Scroll delta.
        delta: ScrollDelta,
        /// Modifier state.
        modifiers: Modifiers,
    },
    /// A drag gesture began (button held and moved past the start threshold).
    DragStart {
        /// Position where the drag started.
        pos: Point,
        /// Which button initiated the drag.
        button: MouseButton,
    },
    /// The pointer moved while dragging.
    DragMove {
        /// Current position.
        pos: Point,
        /// Movement since the previous `DragMove`/`DragStart`.
        delta: Point,
    },
    /// A drag gesture ended (button released).
    DragEnd {
        /// Position where the drag ended.
        pos: Point,
        /// Which button was released.
        button: MouseButton,
    },
}

impl MouseEvent {
    /// The pointer position carried by this event.
    pub fn position(&self) -> Point {
        match self {
            MouseEvent::Down { pos, .. }
            | MouseEvent::Up { pos, .. }
            | MouseEvent::Move { pos, .. }
            | MouseEvent::Enter { pos }
            | MouseEvent::Leave { pos }
            | MouseEvent::DoubleClick { pos, .. }
            | MouseEvent::TripleClick { pos, .. }
            | MouseEvent::Scroll { pos, .. }
            | MouseEvent::DragStart { pos, .. }
            | MouseEvent::DragMove { pos, .. }
            | MouseEvent::DragEnd { pos, .. } => *pos,
        }
    }
}

/// A physical key location, independent of the active keyboard layout.
///
/// Identified by a USB-HID-style code name (e.g. `"KeyA"`, `"Digit1"`). This
/// distinguishes the *position* pressed from the *character produced* (which
/// lives in the logical [`Key`]); a French AZERTY `Q` key and a US QWERTY `A`
/// key share the same physical code `"KeyA"`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PhysicalKey(pub String);

impl PhysicalKey {
    /// Construct from a code name.
    pub fn new(code: impl Into<String>) -> Self {
        Self(code.into())
    }

    /// Borrow the code name.
    pub fn code(&self) -> &str {
        &self.0
    }
}

/// A keyboard event carrying both logical and physical key information.
#[derive(Clone, Debug, PartialEq)]
pub enum KeyboardEvent {
    /// A key was pressed (or auto-repeated; see `repeat`).
    Down {
        /// The logical key (layout/IME mapped).
        key: Key,
        /// The physical key location, if known.
        physical: Option<PhysicalKey>,
        /// Modifier state.
        modifiers: Modifiers,
        /// `true` if this is an OS auto-repeat of a held key.
        repeat: bool,
    },
    /// A key was released.
    Up {
        /// The logical key.
        key: Key,
        /// The physical key location, if known.
        physical: Option<PhysicalKey>,
        /// Modifier state.
        modifiers: Modifiers,
    },
    /// Committed text input (post-IME). Distinct from `Down` so a text field can
    /// ignore navigation keys while still receiving typed characters.
    CharInput {
        /// The committed text (may be multiple code points).
        text: String,
    },
}

impl KeyboardEvent {
    /// Returns `true` if this is an auto-repeat `Down` event.
    pub fn is_repeat(&self) -> bool {
        matches!(self, KeyboardEvent::Down { repeat: true, .. })
    }
}

/// A recognised multi-touch gesture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GestureKind {
    /// A two-finger pinch; `scale` is the relative scale factor (`1.0` = none).
    Pinch {
        /// Relative scale since gesture start (`> 1` zoom in, `< 1` zoom out).
        scale: f32,
    },
    /// A two-finger rotation; `radians` is the signed angle delta.
    Rotate {
        /// Rotation delta in radians (positive = counter-clockwise).
        radians: f32,
    },
    /// A swipe; `delta` is the translation vector of the swipe.
    Swipe {
        /// Swipe translation in logical pixels.
        delta: Point,
    },
}

/// A touch event from a single contact point, identified by a stable touch id.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TouchEvent {
    /// A finger touched down.
    Start {
        /// Stable identifier for this contact for its lifetime.
        id: u64,
        /// Contact position.
        pos: Point,
    },
    /// A touched finger moved.
    Move {
        /// Contact identifier.
        id: u64,
        /// New position.
        pos: Point,
    },
    /// A finger lifted off.
    End {
        /// Contact identifier.
        id: u64,
        /// Position at release.
        pos: Point,
    },
    /// The contact was cancelled by the system (e.g. palm rejection).
    Cancel {
        /// Contact identifier.
        id: u64,
    },
    /// A recognised gesture spanning one or more contacts.
    Gesture {
        /// The recognised gesture.
        kind: GestureKind,
        /// The gesture centroid.
        center: Point,
    },
}

impl TouchEvent {
    /// The stable touch id, or `None` for centroid-only gesture events.
    pub fn touch_id(&self) -> Option<u64> {
        match self {
            TouchEvent::Start { id, .. }
            | TouchEvent::Move { id, .. }
            | TouchEvent::End { id, .. }
            | TouchEvent::Cancel { id } => Some(*id),
            TouchEvent::Gesture { .. } => None,
        }
    }
}

/// Propagation control flags an event handler can set to influence dispatch.
///
/// Defaults are `false`/`false`: the event continues through the
/// capture/bubble phases and the default action is allowed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Propagation {
    /// Stop the event travelling to further nodes in the current and
    /// subsequent phases (capture/bubble).
    pub stop_propagation: bool,
    /// Suppress the framework's default action for this event.
    pub prevent_default: bool,
}

impl Propagation {
    /// No control set — continue propagation, allow default.
    pub const CONTINUE: Propagation = Propagation {
        stop_propagation: false,
        prevent_default: false,
    };

    /// A [`Propagation`] that stops further propagation.
    pub const fn stop() -> Self {
        Self {
            stop_propagation: true,
            prevent_default: false,
        }
    }

    /// A [`Propagation`] that prevents the default action.
    pub const fn prevent() -> Self {
        Self {
            stop_propagation: false,
            prevent_default: true,
        }
    }

    /// Merge two propagation results, OR-ing each flag (sticky once set).
    pub fn merge(self, other: Propagation) -> Propagation {
        Propagation {
            stop_propagation: self.stop_propagation || other.stop_propagation,
            prevent_default: self.prevent_default || other.prevent_default,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifiers_command() {
        assert!(Modifiers::NONE.is_empty());
        let ctrl = Modifiers {
            ctrl: true,
            ..Modifiers::NONE
        };
        assert!(ctrl.command());
        let meta = Modifiers {
            meta: true,
            ..Modifiers::NONE
        };
        assert!(meta.command());
        assert!(!Modifiers::NONE.command());
    }

    #[test]
    fn key_as_text() {
        assert_eq!(Key::Character("ä".into()).as_text(), Some("ä"));
        assert_eq!(Key::Space.as_text(), Some(" "));
        assert_eq!(Key::Enter.as_text(), None);
        assert_eq!(Key::Function(5), Key::Function(5));
    }

    #[test]
    fn mouse_button_eq() {
        assert_eq!(MouseButton::Left, MouseButton::Left);
        assert_ne!(MouseButton::Left, MouseButton::Right);
        assert_eq!(MouseButton::Other(7), MouseButton::Other(7));
    }

    #[test]
    fn mouse_event_position() {
        let e = MouseEvent::Down {
            pos: Point::new(3.0, 4.0),
            button: MouseButton::Left,
            modifiers: Modifiers::NONE,
        };
        assert_eq!(e.position(), Point::new(3.0, 4.0));
        let drag = MouseEvent::DragMove {
            pos: Point::new(9.0, 9.0),
            delta: Point::new(1.0, 1.0),
        };
        assert_eq!(drag.position(), Point::new(9.0, 9.0));
    }

    #[test]
    fn keyboard_event_repeat_and_char() {
        let down = KeyboardEvent::Down {
            key: Key::Character("a".into()),
            physical: Some(PhysicalKey::new("KeyA")),
            modifiers: Modifiers::NONE,
            repeat: true,
        };
        assert!(down.is_repeat());
        let ci = KeyboardEvent::CharInput { text: "x".into() };
        assert!(!ci.is_repeat());
        if let KeyboardEvent::Down {
            physical: Some(p), ..
        } = &down
        {
            assert_eq!(p.code(), "KeyA");
        } else {
            panic!("expected Down with physical key");
        }
    }

    #[test]
    fn touch_event_id_and_gesture() {
        assert_eq!(
            TouchEvent::Start {
                id: 7,
                pos: Point::ZERO
            }
            .touch_id(),
            Some(7)
        );
        let g = TouchEvent::Gesture {
            kind: GestureKind::Pinch { scale: 1.5 },
            center: Point::new(10.0, 10.0),
        };
        assert_eq!(g.touch_id(), None);
        assert_eq!(g, g);
    }

    #[test]
    fn propagation_merge_is_sticky() {
        let a = Propagation::stop();
        let b = Propagation::prevent();
        let m = a.merge(b);
        assert!(m.stop_propagation);
        assert!(m.prevent_default);
        // CONTINUE merged with stop still stops.
        assert!(
            Propagation::CONTINUE
                .merge(Propagation::stop())
                .stop_propagation
        );
        assert_eq!(Propagation::default(), Propagation::CONTINUE);
    }
}
