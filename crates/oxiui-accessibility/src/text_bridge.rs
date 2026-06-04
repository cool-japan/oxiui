//! Bridge between [`oxiui_text`] text widgets and the OxiUI accessibility tree.
//!
//! Provides conversions from `oxiui-text`'s [`TextInput`] and [`TextArea`] types
//! into [`A11yNode`] values ready for the AccessKit pipeline.
//!
//! # Feature gate
//!
//! This module is compiled only when the `text-bridge` Cargo feature is enabled:
//!
//! ```toml
//! oxiui-accessibility = { version = "0.1.1", features = ["text-bridge"] }
//! ```
//!
//! # Overview
//!
//! * [`text_input_to_a11y`] — converts a single-line [`TextInput`] to an
//!   [`A11yNode`] with role [`WidgetRole::TextInput`], text content, and a
//!   cursor/selection description in `props.description`.
//! * [`text_area_to_a11y`] — converts a multi-line [`TextArea`] to an
//!   [`A11yNode`] with the same role, full content, and cursor row/col description.
//! * [`TextInputA11yParams`] — optional label / description overrides.

use accesskit::NodeId;
use oxiui_text::{TextArea, TextInput};

use crate::{
    text_a11y::{TextPosition, TextSelection},
    tree::{A11yNode, WidgetRole},
};

// ── Parameter structs ─────────────────────────────────────────────────────────

/// Optional accessibility overrides for text widget bridge functions.
///
/// All fields default to `None`, meaning the bridge will synthesise sensible
/// defaults (role, content, selection description) without additional metadata.
#[derive(Debug, Default, Clone)]
pub struct TextInputA11yParams {
    /// Accessible name (visible label) for the text field.
    ///
    /// Typically the label text displayed adjacent to the input.
    pub label: Option<String>,
    /// Accessible description (longer hint) for the text field.
    ///
    /// If provided this overrides the auto-generated cursor/selection description.
    pub description: Option<String>,
    /// `NodeId` to assign to the produced node.
    ///
    /// Defaults to `NodeId(0)`; callers embedding the node in a larger id-space
    /// should provide a unique id here.
    pub id: Option<NodeId>,
    /// When `true`, `props.disabled` is set on the returned node.
    pub disabled: bool,
}

// ── Single-line TextInput bridge ──────────────────────────────────────────────

/// Convert an [`oxiui_text::TextInput`] into an [`A11yNode`].
///
/// The returned node has:
/// - Role [`WidgetRole::TextInput`].
/// - `text_content` set to the input's current (un-masked) text.
/// - `props.description` set to a cursor/selection description derived from the
///   input's current selection, **unless** `params.description` is provided.
/// - `label`, `disabled` taken from `params` when set.
///
/// Password inputs produce the same node structure; the text content is the
/// *raw* text (callers should decide whether to expose it).  If the intent is
/// to hide password characters from the a11y tree, pass an empty or masked
/// content string by constructing the node manually.
///
/// # Example
///
/// ```rust
/// use accesskit::NodeId;
/// use oxiui_text::TextInput;
/// use oxiui_accessibility::text_bridge::{text_input_to_a11y, TextInputA11yParams};
///
/// let input = TextInput::with_text("hello");
/// let params = TextInputA11yParams {
///     label: Some("Username".to_string()),
///     id: Some(NodeId(42)),
///     ..Default::default()
/// };
/// let node = text_input_to_a11y(&input, &params);
/// assert_eq!(node.label.as_deref(), Some("Username"));
/// assert_eq!(node.text_content.as_deref(), Some("hello"));
/// ```
pub fn text_input_to_a11y(input: &TextInput, params: &TextInputA11yParams) -> A11yNode {
    let id = params.id.unwrap_or(NodeId(0));
    let sel = input.selection();
    let a11y_sel = TextSelection {
        anchor: TextPosition(sel.anchor),
        active: TextPosition(sel.focus),
    };

    let description = params
        .description
        .clone()
        .unwrap_or_else(|| build_selection_description(a11y_sel));

    let mut node = A11yNode::simple(id, WidgetRole::TextInput, params.label.clone());
    node.text_content = Some(input.text().to_string());
    node.props.description = Some(description);
    node.props.disabled = params.disabled;
    node
}

// ── Multi-line TextArea bridge ────────────────────────────────────────────────

/// Convert an [`oxiui_text::TextArea`] into an [`A11yNode`].
///
/// The returned node has:
/// - Role [`WidgetRole::TextInput`] (no dedicated multi-line role in WidgetRole;
///   the text content clearly communicates the multi-line nature).
/// - `text_content` set to the full text content.
/// - `props.description` set to a `"cursor at row R, col C"` description,
///   **unless** `params.description` is provided.
/// - `label`, `disabled` taken from `params` when set.
///
/// # Example
///
/// ```rust
/// use accesskit::NodeId;
/// use oxiui_text::{TextArea, WrapMode};
/// use oxiui_accessibility::text_bridge::{text_area_to_a11y, TextInputA11yParams};
///
/// let area = TextArea::new("line one\nline two", WrapMode::Hard);
/// let params = TextInputA11yParams {
///     label: Some("Notes".to_string()),
///     id: Some(NodeId(7)),
///     ..Default::default()
/// };
/// let node = text_area_to_a11y(&area, &params);
/// assert_eq!(node.label.as_deref(), Some("Notes"));
/// assert!(node.text_content.as_ref().map(|t| t.contains("line one")).unwrap_or(false));
/// ```
pub fn text_area_to_a11y(area: &TextArea, params: &TextInputA11yParams) -> A11yNode {
    let id = params.id.unwrap_or(NodeId(0));
    let (row, col) = area.cursor();

    let description = params
        .description
        .clone()
        .unwrap_or_else(|| build_cursor_description(row, col));

    let mut node = A11yNode::simple(id, WidgetRole::TextInput, params.label.clone());
    node.text_content = Some(area.text());
    node.props.description = Some(description);
    node.props.disabled = params.disabled;
    node
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Build a screen-reader–friendly description of a selection/cursor.
fn build_selection_description(sel: TextSelection) -> String {
    if sel.is_collapsed() {
        format!("cursor at position {}", sel.active.0)
    } else {
        format!("selected text: bytes {}..{}", sel.start(), sel.end())
    }
}

/// Build a screen-reader–friendly cursor description from row/col (0-based).
fn build_cursor_description(row: usize, col: usize) -> String {
    format!("cursor at row {}, column {}", row + 1, col + 1)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use accesskit::NodeId;
    use oxiui_text::{TextArea, TextInput, WrapMode};

    // ── TextInput bridge ──────────────────────────────────────────────────────

    #[test]
    fn test_text_input_to_a11y_label_and_content() {
        let input = TextInput::with_text("hello");
        let params = TextInputA11yParams {
            label: Some("Username".to_string()),
            id: Some(NodeId(1)),
            ..Default::default()
        };
        let node = text_input_to_a11y(&input, &params);

        assert_eq!(node.id, NodeId(1));
        assert_eq!(node.role, WidgetRole::TextInput);
        assert_eq!(node.label.as_deref(), Some("Username"));
        assert_eq!(node.text_content.as_deref(), Some("hello"));
    }

    #[test]
    fn test_text_input_to_a11y_cursor_at_end() {
        // TextInput::with_text places the cursor at the end of the text.
        let input = TextInput::with_text("abc");
        let params = TextInputA11yParams::default();
        let node = text_input_to_a11y(&input, &params);

        let desc = node.props.description.as_deref().unwrap_or("");
        // Cursor at position 3 (byte length of "abc")
        assert!(
            desc.contains("cursor at position 3"),
            "expected cursor description, got: {desc}"
        );
    }

    #[test]
    fn test_text_input_to_a11y_description_override() {
        let input = TextInput::with_text("data");
        let params = TextInputA11yParams {
            description: Some("Enter your login name".to_string()),
            ..Default::default()
        };
        let node = text_input_to_a11y(&input, &params);

        assert_eq!(
            node.props.description.as_deref(),
            Some("Enter your login name"),
            "params.description should override auto-generated description"
        );
    }

    #[test]
    fn test_text_input_to_a11y_disabled_flag() {
        let input = TextInput::new();
        let params = TextInputA11yParams {
            disabled: true,
            ..Default::default()
        };
        let node = text_input_to_a11y(&input, &params);
        assert!(node.props.disabled, "disabled flag should be propagated");
    }

    #[test]
    fn test_text_input_to_a11y_empty_input() {
        let input = TextInput::new();
        let params = TextInputA11yParams::default();
        let node = text_input_to_a11y(&input, &params);

        assert_eq!(node.text_content.as_deref(), Some(""));
        let desc = node.props.description.as_deref().unwrap_or("");
        assert!(
            desc.contains("cursor at position 0"),
            "empty input should have cursor at 0, got: {desc}"
        );
    }

    #[test]
    fn test_text_input_to_a11y_default_id_is_zero() {
        let input = TextInput::new();
        let params = TextInputA11yParams::default();
        let node = text_input_to_a11y(&input, &params);
        assert_eq!(node.id, NodeId(0));
    }

    // ── TextArea bridge ───────────────────────────────────────────────────────

    #[test]
    fn test_text_area_to_a11y_label_and_content() {
        let area = TextArea::new("line one\nline two", WrapMode::Hard);
        let params = TextInputA11yParams {
            label: Some("Notes".to_string()),
            id: Some(NodeId(7)),
            ..Default::default()
        };
        let node = text_area_to_a11y(&area, &params);

        assert_eq!(node.id, NodeId(7));
        assert_eq!(node.role, WidgetRole::TextInput);
        assert_eq!(node.label.as_deref(), Some("Notes"));
        let content = node.text_content.as_deref().unwrap_or("");
        assert!(
            content.contains("line one"),
            "content should include first line, got: {content}"
        );
    }

    #[test]
    fn test_text_area_to_a11y_cursor_description() {
        let area = TextArea::new("hello", WrapMode::Hard);
        let params = TextInputA11yParams::default();
        let node = text_area_to_a11y(&area, &params);

        let desc = node.props.description.as_deref().unwrap_or("");
        // Cursor starts at row 0, col 0 (new editor, cursor at start).
        assert!(
            desc.contains("row"),
            "description should mention row, got: {desc}"
        );
        assert!(
            desc.contains("column"),
            "description should mention column, got: {desc}"
        );
    }

    #[test]
    fn test_text_area_to_a11y_description_override() {
        let area = TextArea::new("text", WrapMode::Hard);
        let params = TextInputA11yParams {
            description: Some("Multi-line notes field".to_string()),
            ..Default::default()
        };
        let node = text_area_to_a11y(&area, &params);

        assert_eq!(
            node.props.description.as_deref(),
            Some("Multi-line notes field"),
        );
    }

    #[test]
    fn test_text_area_to_a11y_disabled_flag() {
        let area = TextArea::new("", WrapMode::Hard);
        let params = TextInputA11yParams {
            disabled: true,
            ..Default::default()
        };
        let node = text_area_to_a11y(&area, &params);
        assert!(node.props.disabled);
    }

    // ── build_selection_description (unit) ───────────────────────────────────

    #[test]
    fn test_build_selection_description_cursor() {
        let sel = TextSelection::cursor(5);
        let desc = build_selection_description(sel);
        assert_eq!(desc, "cursor at position 5");
    }

    #[test]
    fn test_build_selection_description_range() {
        let sel = TextSelection::range(2, 7);
        let desc = build_selection_description(sel);
        assert!(desc.contains("2..7"), "expected 2..7 in: {desc}");
    }

    // ── build_cursor_description (unit) ──────────────────────────────────────

    #[test]
    fn test_build_cursor_description_first_cell() {
        let desc = build_cursor_description(0, 0);
        // 0-based internally, 1-based in output
        assert_eq!(desc, "cursor at row 1, column 1");
    }

    #[test]
    fn test_build_cursor_description_third_row() {
        let desc = build_cursor_description(2, 4);
        assert_eq!(desc, "cursor at row 3, column 5");
    }
}
