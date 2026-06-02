//! Property primitives for [`crate::tree::A11yNode`].
//!
//! This module defines the small value types that decorate an a11y node beyond
//! its role and label: live-region politeness, three-state toggles, text caret
//! / selection coordinates, and the public `From` mappings to the corresponding
//! AccessKit types. Keeping these in a dedicated module lets `tree.rs` focus on
//! the node-graph plumbing and lets the builder / diff modules import from a
//! single small surface.
//!
//! All conversions are *infallible*: the property types are designed so that
//! every valid OxiUI value has a faithful AccessKit representation. This
//! contract is what allows the tree builder to avoid `unwrap`/`panic` while
//! still emitting fully-typed AccessKit nodes.

use accesskit::{Live, NodeId, Toggled};

// ── Live region politeness ───────────────────────────────────────────────────

/// Live-region politeness for screen-reader announcements.
///
/// Mirrors the W3C ARIA `aria-live` values:
///
/// * [`LiveSetting::Off`] — content updates are not announced.
/// * [`LiveSetting::Polite`] — wait for the screen reader to finish its current
///   utterance, then announce.
/// * [`LiveSetting::Assertive`] — interrupt the current utterance and announce
///   immediately. Reserve for urgent feedback (errors, time-critical alerts).
///
/// The variant ordering matches AccessKit's [`accesskit::Live`] enum so that
/// `From` is a trivial 1:1 mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum LiveSetting {
    /// Updates to this node are not announced.
    #[default]
    Off,
    /// Updates are queued behind the screen reader's current utterance.
    Polite,
    /// Updates interrupt the current utterance.
    Assertive,
}

impl From<LiveSetting> for Live {
    #[inline]
    fn from(value: LiveSetting) -> Self {
        match value {
            LiveSetting::Off => Live::Off,
            LiveSetting::Polite => Live::Polite,
            LiveSetting::Assertive => Live::Assertive,
        }
    }
}

// ── Three-state toggle (checked / mixed / unchecked) ─────────────────────────

/// Three-state toggle for `Checkbox` / `MenuItemCheckBox` / `Tab` selection.
///
/// `bool` cannot encode the *mixed* state needed for tri-state checkboxes
/// (parent rows in a tree view, "select all" toggles, etc.); this enum can.
///
/// Convert from a plain `bool` via `From`:
///
/// ```rust
/// use oxiui_accessibility::props::Toggled3;
/// assert_eq!(Toggled3::from(true),  Toggled3::True);
/// assert_eq!(Toggled3::from(false), Toggled3::False);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum Toggled3 {
    /// The control is in the *off* / *unchecked* state.
    #[default]
    False,
    /// The control is in the *on* / *checked* state.
    True,
    /// The control is in the *indeterminate* / *mixed* state (tri-state).
    Mixed,
}

impl From<bool> for Toggled3 {
    #[inline]
    fn from(b: bool) -> Self {
        if b {
            Toggled3::True
        } else {
            Toggled3::False
        }
    }
}

impl From<Toggled3> for Toggled {
    #[inline]
    fn from(value: Toggled3) -> Self {
        match value {
            Toggled3::False => Toggled::False,
            Toggled3::True => Toggled::True,
            Toggled3::Mixed => Toggled::Mixed,
        }
    }
}

/// Three-state checked state for checkboxes.
///
/// This is a type alias to [`Toggled3`] so that the API is semantically
/// self-documenting where the concept of "checked vs toggled" applies.
pub type CheckedState = Toggled3;

/// Conversion from a `&CheckedState` reference to [`Toggled3`] (identity copy).
impl From<&CheckedState> for Toggled3 {
    #[inline]
    fn from(c: &CheckedState) -> Toggled3 {
        *c
    }
}

// ── Text caret / selection coordinates ───────────────────────────────────────

/// Byte-offset describing the text caret / a text selection on an editable
/// node.
///
/// Offsets are **byte** indices into the UTF-8 representation of the
/// [`crate::tree::A11yNode::text_content`] string. The tree builder
/// translates them into AccessKit's [`accesskit::TextSelection`] /
/// [`accesskit::TextPosition`] coordinates by synthesising a child
/// [`accesskit::Role::TextRun`] node carrying the text's character-length
/// table.
///
/// For a pure caret (no selection), set [`TextCaret::start`] and
/// [`TextCaret::end`] to the same value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TextCaret {
    /// Byte offset of the selection anchor (does not move while extending).
    pub start: usize,
    /// Byte offset of the selection focus (moves while extending).
    pub end: usize,
}

impl TextCaret {
    /// Construct a degenerate selection at byte offset `pos` (i.e. a caret).
    #[inline]
    pub const fn caret(pos: usize) -> Self {
        Self {
            start: pos,
            end: pos,
        }
    }

    /// Construct a selection running between two byte offsets.
    ///
    /// The two offsets may appear in either order; the type stores them as
    /// supplied so that callers can preserve the directional anchor/focus
    /// distinction.
    #[inline]
    pub const fn range(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// Lower bound of the selected range, regardless of direction.
    #[inline]
    pub fn lo(&self) -> usize {
        core::cmp::min(self.start, self.end)
    }

    /// Upper bound of the selected range, regardless of direction.
    #[inline]
    pub fn hi(&self) -> usize {
        core::cmp::max(self.start, self.end)
    }

    /// `true` if this caret has no selected range (start == end).
    #[inline]
    pub fn is_caret(&self) -> bool {
        self.start == self.end
    }
}

/// A text selection expressed as byte offsets (anchor/focus).
///
/// Semantically equivalent to [`TextCaret`] but with different field names to
/// match the spec's `A11yNodeProps::text_selection` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TextSelection {
    /// Byte offset of the anchor (the fixed end).
    pub anchor: usize,
    /// Byte offset of the focus (the moving end / caret position).
    pub focus: usize,
}

impl TextSelection {
    /// A collapsed caret at `pos`.
    #[inline]
    pub const fn caret(pos: usize) -> Self {
        Self {
            anchor: pos,
            focus: pos,
        }
    }

    /// `true` if anchor == focus (no selection range, just a caret).
    #[inline]
    pub fn is_caret(&self) -> bool {
        self.anchor == self.focus
    }
}

// ── Text-run child segment ───────────────────────────────────────────────────

/// A synthesized text-run segment for caret/selection exposure.
///
/// Text nodes that carry a [`TextSelection`] are split into up to three
/// `TextRunChild` segments by [`crate::tree::synthesize_text_run_children`]:
/// the text *before* the selection, the *selected* span, and the text *after*
/// the selection.  Nodes with no selection produce a single segment for the
/// whole text.
///
/// Offsets are expressed both as byte indices (for slicing) and as char
/// indices (for AccessKit's `TextPosition.character_index`).
#[derive(Debug, Clone, Default)]
pub struct TextRunChild {
    /// The UTF-8 text content of this segment.
    pub text: String,
    /// 0-based character index of the first character in this segment.
    pub char_offset: usize,
    /// 0-based byte index of the first byte in this segment.
    pub byte_offset: usize,
    /// `true` if this segment falls within the selection range.
    pub is_selected: bool,
}

// ── Rich property bag ────────────────────────────────────────────────────────

/// Rich property bag attached to every [`crate::tree::A11yNode`].
///
/// All fields are optional / defaulted so that callers only set what they need.
/// The tree builder reads these fields and forwards them to the corresponding
/// AccessKit setters.
#[derive(Debug, Clone, Default)]
pub struct A11yNodeProps {
    // ── Text / description ───────────────────────────────────────────────────
    /// Longer description of the widget (ARIA `aria-describedby`-equivalent text).
    pub description: Option<String>,
    /// Placeholder text for empty text inputs.
    pub placeholder: Option<String>,
    /// Keyboard shortcut that activates this widget (e.g. `"Ctrl+S"`).
    pub key_shortcut: Option<String>,

    // ── State ────────────────────────────────────────────────────────────────
    /// `true` if the widget is non-interactive.
    pub disabled: bool,
    /// Expanded state: `Some(true)` = expanded, `Some(false)` = collapsed,
    /// `None` = not expandable.
    pub expanded: Option<bool>,
    /// Selected state: `Some(true/false)` = selectable, `None` = not selectable.
    pub selected: Option<bool>,
    /// Checked / toggle state; `None` = not checkable.
    pub checked: Option<CheckedState>,

    // ── Range values ─────────────────────────────────────────────────────────
    /// Current numeric value (sliders, progress bars, spinners).
    pub value_now: Option<f64>,
    /// Minimum allowed numeric value.
    pub value_min: Option<f64>,
    /// Maximum allowed numeric value.
    pub value_max: Option<f64>,
    /// Step increment for the numeric value.
    pub value_step: Option<f64>,

    // ── Text content + cursor ─────────────────────────────────────────────────
    /// Text content / string value of the node.
    pub text_value: Option<String>,
    /// Text selection (anchor + focus byte offsets).
    pub text_selection: Option<TextSelection>,

    // ── Relationships ─────────────────────────────────────────────────────────
    /// Nodes that label this node (ARIA `aria-labelledby`).
    pub labelled_by: Vec<NodeId>,
    /// Nodes that describe this node (ARIA `aria-describedby`).
    pub described_by: Vec<NodeId>,
    /// Nodes that this node controls (ARIA `aria-controls`).
    pub controlled_by: Vec<NodeId>,
    /// Nodes that this node logically owns but that are not DOM descendants.
    pub owns: Vec<NodeId>,

    // ── Text run children ─────────────────────────────────────────────────────
    /// Synthesized text-run child segments for caret/selection exposure.
    ///
    /// Populated by [`crate::tree::synthesize_text_run_children`] for text
    /// nodes that carry a [`TextSelection`].  Empty by default.
    pub text_run_children: Vec<TextRunChild>,

    // ── Keyboard navigation ───────────────────────────────────────────────────
    /// Explicit tab index controlling keyboard-focus order.
    ///
    /// `None` / `Some(0)` = natural document order; `Some(n)` where `n > 0` =
    /// explicit position (lower values receive focus first).  Interpreted by
    /// [`crate::nav::TabOrder::compute`].
    pub tab_index: Option<u32>,
}

// ── UTF-8 character-length table ─────────────────────────────────────────────

/// Build the AccessKit `character_lengths` table for `text`.
///
/// AccessKit requires `Role::TextRun` nodes to expose the length, in bytes, of
/// each *grapheme* (here approximated by Unicode scalar values — i.e. each
/// `char`). The runtime cost is `O(text.len())`.
///
/// Returns an empty `Vec` for empty input.
/// Build the AccessKit `character_lengths` table for `text` (public for
/// use by platform adapter integration layers).
pub fn character_lengths_utf8(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    for ch in text.chars() {
        // A single Unicode scalar value is at most 4 bytes in UTF-8 — fits in u8.
        let len = ch.len_utf8() as u8;
        out.push(len);
    }
    out
}

/// Clamp a byte offset to a valid char-boundary index inside `text` and return
/// the matching *character* index suitable for [`accesskit::TextPosition`].
///
/// AccessKit's `character_index` is a count of entries in `character_lengths`
/// (i.e. a 0-based char index, with `text.chars().count()` representing the
/// end-of-line position), not a byte offset. This helper performs the
/// translation while guarding against malformed offsets.
/// Translate a UTF-8 byte offset to a char index (public for platform
/// adapter integration layers).
pub fn byte_offset_to_char_index(text: &str, byte_offset: usize) -> usize {
    if byte_offset == 0 {
        return 0;
    }
    // Walk character boundaries, counting until we reach (or pass) byte_offset.
    let mut chars = 0usize;
    let mut current_byte = 0usize;
    for ch in text.chars() {
        if current_byte >= byte_offset {
            return chars;
        }
        current_byte += ch.len_utf8();
        chars += 1;
    }
    // byte_offset >= text.len(): clamp to end-of-string char index.
    chars
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_setting_maps_to_accesskit_live() {
        assert!(matches!(Live::from(LiveSetting::Off), Live::Off));
        assert!(matches!(Live::from(LiveSetting::Polite), Live::Polite));
        assert!(matches!(
            Live::from(LiveSetting::Assertive),
            Live::Assertive
        ));
    }

    #[test]
    fn toggled3_from_bool() {
        assert_eq!(Toggled3::from(true), Toggled3::True);
        assert_eq!(Toggled3::from(false), Toggled3::False);
    }

    #[test]
    fn toggled3_maps_to_accesskit_toggled() {
        assert!(matches!(Toggled::from(Toggled3::False), Toggled::False));
        assert!(matches!(Toggled::from(Toggled3::True), Toggled::True));
        assert!(matches!(Toggled::from(Toggled3::Mixed), Toggled::Mixed));
    }

    #[test]
    fn text_caret_helpers() {
        let c = TextCaret::caret(5);
        assert!(c.is_caret());
        assert_eq!(c.lo(), 5);
        assert_eq!(c.hi(), 5);

        let s = TextCaret::range(2, 9);
        assert!(!s.is_caret());
        assert_eq!(s.lo(), 2);
        assert_eq!(s.hi(), 9);

        // Reversed anchor/focus still yields correct lo/hi.
        let r = TextCaret::range(9, 2);
        assert_eq!(r.lo(), 2);
        assert_eq!(r.hi(), 9);
    }

    #[test]
    fn text_selection_caret() {
        let sel = TextSelection::caret(10);
        assert!(sel.is_caret());
        assert_eq!(sel.anchor, 10);
        assert_eq!(sel.focus, 10);
    }

    #[test]
    fn text_selection_range() {
        let sel = TextSelection {
            anchor: 3,
            focus: 7,
        };
        assert!(!sel.is_caret());
    }

    #[test]
    fn character_lengths_ascii() {
        let v = character_lengths_utf8("hello");
        assert_eq!(v, vec![1u8, 1, 1, 1, 1]);
    }

    #[test]
    fn character_lengths_multibyte() {
        // "héllo" — é is 2 bytes in UTF-8
        let v = character_lengths_utf8("héllo");
        assert_eq!(v, vec![1u8, 2, 1, 1, 1]);
    }

    #[test]
    fn character_lengths_emoji() {
        // 🦀 is 4 bytes
        let v = character_lengths_utf8("a🦀b");
        assert_eq!(v, vec![1u8, 4, 1]);
    }

    #[test]
    fn character_lengths_empty() {
        let v = character_lengths_utf8("");
        assert!(v.is_empty());
    }

    #[test]
    fn byte_offset_to_char_index_ascii() {
        assert_eq!(byte_offset_to_char_index("hello", 0), 0);
        assert_eq!(byte_offset_to_char_index("hello", 1), 1);
        assert_eq!(byte_offset_to_char_index("hello", 5), 5);
        // Past end clamps to end.
        assert_eq!(byte_offset_to_char_index("hello", 999), 5);
    }

    #[test]
    fn byte_offset_to_char_index_multibyte() {
        // "héllo"  — indexed by char: h=0, é=1, l=2, l=3, o=4, end=5
        // Bytes:    h=0  é=1..2  l=3  l=4  o=5
        assert_eq!(byte_offset_to_char_index("héllo", 0), 0);
        assert_eq!(byte_offset_to_char_index("héllo", 1), 1); // start of é
        assert_eq!(byte_offset_to_char_index("héllo", 3), 2); // start of first 'l'
        assert_eq!(byte_offset_to_char_index("héllo", 6), 5); // end
    }

    #[test]
    fn a11y_node_props_default_is_empty() {
        let props = A11yNodeProps::default();
        assert!(props.description.is_none());
        assert!(props.placeholder.is_none());
        assert!(props.key_shortcut.is_none());
        assert!(!props.disabled);
        assert!(props.expanded.is_none());
        assert!(props.selected.is_none());
        assert!(props.checked.is_none());
        assert!(props.value_now.is_none());
        assert!(props.value_min.is_none());
        assert!(props.value_max.is_none());
        assert!(props.value_step.is_none());
        assert!(props.labelled_by.is_empty());
        assert!(props.described_by.is_empty());
        assert!(props.controlled_by.is_empty());
        assert!(props.owns.is_empty());
    }
}
