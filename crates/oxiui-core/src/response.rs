//! Response structs returned by the extended [`UiCtx`](crate::UiCtx) widgets.
//!
//! ## The `supported` contract
//!
//! Every extended widget method on [`UiCtx`](crate::UiCtx) (`text_input`,
//! `checkbox`, `slider`, …) ships with a **default implementation that returns
//! a response whose `supported` field is `false`**. An adapter that has not
//! overridden a given widget therefore reports `supported == false` to the
//! caller, instead of silently drawing nothing and lying about success. Callers
//! can branch on `supported` to fall back gracefully (e.g. render their own
//! control, or warn). Each response type carries a zero/identity state for its
//! payload alongside the flag, and an [`unsupported`](CheckboxResponse::unsupported)
//! constructor the defaults use.

/// Result of a [`text_input`](crate::UiCtx::text_input) widget.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextInputResponse {
    /// Whether the text changed this frame.
    pub changed: bool,
    /// The current text contents.
    pub text: String,
    /// Whether the active adapter actually rendered this widget.
    pub supported: bool,
    /// Whether this text input currently has keyboard focus.
    ///
    /// # Headless-approximate note
    ///
    /// This field is a best-effort approximation. Backends that lack a real
    /// focus mechanism (e.g. headless state-machine contexts) always report
    /// `false`. Backends that wire iced focus events may set this accurately
    /// in a future milestone.
    pub focused: bool,
}

impl TextInputResponse {
    /// The "not implemented by this adapter" response: empty text, not changed,
    /// `supported = false`.
    pub fn unsupported() -> Self {
        Self {
            changed: false,
            text: String::new(),
            supported: false,
            focused: false,
        }
    }

    /// A supported response carrying the current `text` and `changed` flag.
    ///
    /// `focused` defaults to `false` (headless-approximate). Use
    /// [`TextInputResponse::supported_focused`] if you have accurate focus state.
    pub fn supported(text: impl Into<String>, changed: bool) -> Self {
        Self {
            changed,
            text: text.into(),
            supported: true,
            focused: false,
        }
    }

    /// A supported response with an explicit `focused` state.
    pub fn supported_focused(text: impl Into<String>, changed: bool, focused: bool) -> Self {
        Self {
            changed,
            text: text.into(),
            supported: true,
            focused,
        }
    }
}

/// Result of a [`checkbox`](crate::UiCtx::checkbox) widget.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CheckboxResponse {
    /// Whether the checked state toggled this frame.
    pub changed: bool,
    /// The current checked state.
    pub checked: bool,
    /// Whether the active adapter actually rendered this widget.
    pub supported: bool,
}

impl CheckboxResponse {
    /// The "not implemented by this adapter" response.
    pub fn unsupported() -> Self {
        Self {
            changed: false,
            checked: false,
            supported: false,
        }
    }

    /// A supported response carrying the current `checked` and `changed` flags.
    pub fn supported(checked: bool, changed: bool) -> Self {
        Self {
            changed,
            checked,
            supported: true,
        }
    }
}

/// Result of a [`slider`](crate::UiCtx::slider) widget.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SliderResponse {
    /// Whether the value changed this frame.
    pub changed: bool,
    /// The current value.
    pub value: f64,
    /// Whether the active adapter actually rendered this widget.
    pub supported: bool,
}

impl SliderResponse {
    /// The "not implemented by this adapter" response: value `0.0`, not changed.
    pub fn unsupported() -> Self {
        Self {
            changed: false,
            value: 0.0,
            supported: false,
        }
    }

    /// A supported response carrying the current `value` and `changed` flag.
    pub fn supported(value: f64, changed: bool) -> Self {
        Self {
            changed,
            value,
            supported: true,
        }
    }
}

/// Result of a [`dropdown`](crate::UiCtx::dropdown) widget.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DropdownResponse {
    /// Whether the selection changed this frame.
    pub changed: bool,
    /// The index of the currently selected option.
    pub selected: usize,
    /// Whether the active adapter actually rendered this widget.
    pub supported: bool,
}

impl DropdownResponse {
    /// The "not implemented by this adapter" response: selection `0`, not changed.
    pub fn unsupported() -> Self {
        Self {
            changed: false,
            selected: 0,
            supported: false,
        }
    }

    /// A supported response carrying the current `selected` and `changed` flag.
    pub fn supported(selected: usize, changed: bool) -> Self {
        Self {
            changed,
            selected,
            supported: true,
        }
    }
}

/// A generic response for widgets with no interaction payload
/// (`separator`, `spacer`, `image`, `tooltip`, `popup`, `modal`, `scroll_area`).
///
/// Carries only whether the adapter rendered it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WidgetResponse {
    /// Whether the active adapter actually rendered this widget.
    pub supported: bool,
}

impl WidgetResponse {
    /// The "not implemented by this adapter" response (`supported = false`).
    pub fn unsupported() -> Self {
        Self { supported: false }
    }

    /// A response indicating the widget was rendered (`supported = true`).
    pub fn supported() -> Self {
        Self { supported: true }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_constructors_set_flag_false() {
        assert!(!TextInputResponse::unsupported().supported);
        assert!(!CheckboxResponse::unsupported().supported);
        assert!(!SliderResponse::unsupported().supported);
        assert!(!DropdownResponse::unsupported().supported);
        assert!(!WidgetResponse::unsupported().supported);
    }

    #[test]
    fn unsupported_payloads_are_zeroed() {
        assert_eq!(TextInputResponse::unsupported().text, "");
        assert!(!TextInputResponse::unsupported().changed);
        assert!(!CheckboxResponse::unsupported().checked);
        assert_eq!(SliderResponse::unsupported().value, 0.0);
        assert_eq!(DropdownResponse::unsupported().selected, 0);
    }

    #[test]
    fn supported_constructors_carry_payload() {
        let t = TextInputResponse::supported("hi", true);
        assert!(t.supported && t.changed && t.text == "hi");
        let c = CheckboxResponse::supported(true, true);
        assert!(c.supported && c.checked && c.changed);
        let s = SliderResponse::supported(0.5, false);
        assert!(s.supported && !s.changed && (s.value - 0.5).abs() < 1e-9);
        let d = DropdownResponse::supported(3, true);
        assert!(d.supported && d.changed && d.selected == 3);
        assert!(WidgetResponse::supported().supported);
    }

    #[test]
    fn defaults_match_unsupported_for_flag() {
        // Derived Default leaves supported=false, matching unsupported()'s flag.
        assert_eq!(
            CheckboxResponse::default().supported,
            CheckboxResponse::unsupported().supported
        );
        assert_eq!(WidgetResponse::default(), WidgetResponse::unsupported());
    }
}
