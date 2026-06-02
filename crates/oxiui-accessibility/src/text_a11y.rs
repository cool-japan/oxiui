//! Text cursor and selection synthesis for accessible text inputs.
//!
//! Provides:
//! - [`TextPosition`] — a byte-offset cursor position in a UTF-8 string.
//! - [`TextSelection`] — a selection range (anchor + active) or collapsed cursor.
//! - [`build_text_input_a11y`] — synthesise an [`crate::tree::A11yNode`] for a
//!   text input field, pre-populated with content and a selection description.
//! - [`update_text_cursor`] — update an existing text input node with a new
//!   cursor/selection position.
//!
//! # Name collision note
//!
//! `crate::props` already exports a `TextSelection { anchor: usize, focus: usize }`
//! for internal byte-offset bookkeeping used by [`crate::tree::synthesize_text_run_children`].
//! The types in *this* module use an explicit [`TextPosition`] newtype wrapper
//! and the field name `active` (rather than `focus`) to follow the AT-SPI /
//! ARIA convention and to avoid confusion with the lower-level props type.
//! Import explicitly via `oxiui_accessibility::text_a11y::*` — do **not**
//! glob-import at the crate root alongside `props::TextSelection`.

use accesskit::NodeId;

use crate::tree::{A11yNode, WidgetRole};

// ── TextPosition ──────────────────────────────────────────────────────────────

/// A byte offset into the UTF-8 content of a text field.
///
/// Offsets are 0-based indices into the raw bytes of the field's content
/// string.  Callers are responsible for providing offsets that land on valid
/// UTF-8 character boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TextPosition(
    /// The 0-based byte offset.
    pub usize,
);

// ── TextSelection ─────────────────────────────────────────────────────────────

/// A text selection range, expressed as a pair of [`TextPosition`] byte offsets.
///
/// When `anchor == active` the selection is *collapsed* (a bare cursor with no
/// selected text).  When they differ, the selection spans `start()..end()`.
///
/// The `anchor` is the fixed end (where the selection *started*) and `active`
/// is the moving end (where the cursor currently is).  Either end may be
/// numerically larger.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextSelection {
    /// The fixed end of the selection (where the user started selecting).
    pub anchor: TextPosition,
    /// The active end of the selection (where the cursor is right now).
    pub active: TextPosition,
}

impl TextSelection {
    /// Construct a collapsed cursor at byte offset `pos`.
    ///
    /// `is_collapsed()` returns `true` for this variant.
    pub fn cursor(pos: usize) -> Self {
        Self {
            anchor: TextPosition(pos),
            active: TextPosition(pos),
        }
    }

    /// Construct a selection from byte offset `from` to byte offset `to`.
    ///
    /// The anchor is set to `from` and the active end to `to`.  They may be
    /// supplied in any order — use [`Self::start`] / [`Self::end`] for the
    /// normalized bounds.
    pub fn range(from: usize, to: usize) -> Self {
        Self {
            anchor: TextPosition(from),
            active: TextPosition(to),
        }
    }

    /// `true` if the selection is a bare cursor (no selected text).
    pub fn is_collapsed(&self) -> bool {
        self.anchor == self.active
    }

    /// The lower byte offset of the selection, regardless of direction.
    pub fn start(&self) -> usize {
        self.anchor.0.min(self.active.0)
    }

    /// The upper byte offset of the selection, regardless of direction.
    pub fn end(&self) -> usize {
        self.anchor.0.max(self.active.0)
    }

    /// The number of bytes in the selection.
    ///
    /// Returns `0` for a collapsed cursor.
    pub fn len(&self) -> usize {
        self.end() - self.start()
    }

    /// `true` if the selection spans no bytes (equivalent to `is_collapsed()`).
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

// ── Internal helper ───────────────────────────────────────────────────────────

/// Build a human-readable description of `selection` for screen readers.
///
/// Returns strings of the form `"cursor at position 5"` or
/// `"selected text: bytes 3..8"`.
fn describe_selection(selection: TextSelection) -> String {
    if selection.is_collapsed() {
        format!("cursor at position {}", selection.active.0)
    } else {
        format!(
            "selected text: bytes {}..{}",
            selection.start(),
            selection.end()
        )
    }
}

// ── Public synthesis functions ────────────────────────────────────────────────

/// Synthesise an [`A11yNode`] for a text input field with cursor/selection.
///
/// The returned node has:
/// - Role [`WidgetRole::TextInput`] when `is_editable` is `true`, otherwise
///   [`WidgetRole::Label`] (read-only text).
/// - `text_content` set to `content`.
/// - `props.description` set to a human-readable selection description
///   (e.g. `"cursor at position 5"` or `"selected text: bytes 3..8"`).
///
/// The node is minted with `NodeId(0)`.  Callers that embed this node in a
/// larger id space must renumber it (the same convention used by
/// [`crate::tree::build_table_a11y`]).
pub fn build_text_input_a11y(
    content: &str,
    selection: TextSelection,
    is_editable: bool,
) -> A11yNode {
    let role = if is_editable {
        WidgetRole::TextInput
    } else {
        WidgetRole::Label
    };

    let mut node = A11yNode::simple(NodeId(0), role, None);
    node.text_content = Some(content.to_string());
    node.props.description = Some(describe_selection(selection));
    node
}

/// Update the cursor/selection description on an existing text input node.
///
/// Reads `node.text_content` for the content (no change) and overwrites
/// `node.props.description` with a fresh description derived from `selection`.
pub fn update_text_cursor(node: &mut A11yNode, selection: TextSelection) {
    node.props.description = Some(describe_selection(selection));
}
