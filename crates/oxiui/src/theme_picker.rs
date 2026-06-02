//! A built-in theme picker widget for choosing between the OxiUI built-in themes.
//!
//! # Example
//!
//! ```rust,no_run
//! use oxiui::theme_picker::{theme_picker, by_name, BUILTIN_THEMES};
//! ```

use oxiui_core::UiCtx;

/// The ordered list of built-in theme names.
///
/// Each name is a valid argument to [`by_name`].
pub const BUILTIN_THEMES: &[&str] = &["light", "dark", "cooljapan_default"];

/// Render a simple theme-picker widget through the given [`UiCtx`].
///
/// Displays one button per built-in theme. When the user clicks a button
/// for a theme that differs from `*current`, the selection is updated and
/// `true` is returned. If nothing changed, returns `false`.
///
/// `*current` must be one of the [`BUILTIN_THEMES`] values (or any `&'static str`).
/// After a change, call `App::theme(by_name(*current))` to apply the selection.
///
/// # Example
///
/// ```rust
/// # use oxiui::theme_picker::{theme_picker, by_name, BUILTIN_THEMES};
/// // In your content closure (assuming a real UiCtx):
/// # struct FakeCtx;
/// # impl oxiui_core::UiCtx for FakeCtx {
/// #     fn heading(&mut self, _: &str) {}
/// #     fn label(&mut self, _: &str) {}
/// #     fn button(&mut self, _: &str) -> oxiui_core::ButtonResponse { Default::default() }
/// # }
/// # let mut ui = FakeCtx;
/// let mut current = "light";
/// let changed = theme_picker(&mut ui, &mut current);
/// if changed { /* apply by_name(current) */ }
/// ```
pub fn theme_picker(ui: &mut dyn UiCtx, current: &mut &'static str) -> bool {
    let mut changed = false;
    for &name in BUILTIN_THEMES {
        let resp = ui.button(name);
        if resp.clicked && *current != name {
            *current = name;
            changed = true;
        }
    }
    changed
}

/// Construct a boxed [`oxiui_core::Theme`] from a built-in theme name.
///
/// Recognised names: `"light"`, `"dark"`, `"cooljapan_default"`.
/// Any other string falls back to the `"light"` theme.
pub fn by_name(name: &str) -> Box<dyn oxiui_core::Theme> {
    match name {
        "dark" => oxiui_theme::dark(),
        "cooljapan_default" => oxiui_theme::cooljapan_default(),
        _ => oxiui_theme::light(),
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use oxiui_core::{ButtonResponse, UiCtx};

    struct NullUiCtx;
    impl UiCtx for NullUiCtx {
        fn heading(&mut self, _text: &str) {}
        fn label(&mut self, _text: &str) {}
        fn button(&mut self, _label: &str) -> ButtonResponse {
            ButtonResponse::default()
        }
    }

    /// Calling theme_picker with a null context must not panic.
    #[test]
    fn test_theme_picker_no_panic() {
        let mut ctx = NullUiCtx;
        let mut current = "light";
        let changed = theme_picker(&mut ctx, &mut current);
        // NullUiCtx never returns clicked=true, so nothing changes.
        assert!(!changed, "null context should return false");
        assert_eq!(current, "light", "selection unchanged");
    }

    /// When a button click is returned, selection changes and true is returned.
    #[test]
    fn test_theme_picker_selection_changes() {
        struct ClickCtx {
            target: &'static str,
        }
        impl UiCtx for ClickCtx {
            fn heading(&mut self, _text: &str) {}
            fn label(&mut self, _text: &str) {}
            fn button(&mut self, label: &str) -> ButtonResponse {
                ButtonResponse {
                    clicked: label == self.target,
                    hovered: false,
                }
            }
        }

        let mut ctx = ClickCtx { target: "dark" };
        let mut current = "light";
        let changed = theme_picker(&mut ctx, &mut current);
        assert!(changed, "clicking 'dark' must return true");
        assert_eq!(current, "dark");
    }

    /// by_name returns a theme for every built-in name without panicking.
    #[test]
    fn test_by_name_all_variants() {
        for &name in BUILTIN_THEMES {
            let _theme = by_name(name);
        }
        // Unknown name falls back to light.
        let _theme = by_name("unknown");
    }
}
