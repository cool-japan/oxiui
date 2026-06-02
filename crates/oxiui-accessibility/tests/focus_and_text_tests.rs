//! Integration tests for focus indicator and text cursor/selection synthesis.
//!
//! Tests:
//! 1.  `test_focus_indicator_default`        — new indicator has no focused node.
//! 2.  `test_focus_indicator_set_focus`      — set_focus captures the node id.
//! 3.  `test_focus_indicator_clear_focus`    — set_focus(None) clears it.
//! 4.  `test_focus_ring_default_values`      — default ring has expected properties.
//! 5.  `test_focus_ring_custom`              — with_ring replaces properties.
//! 6.  `test_text_selection_cursor`          — cursor() is collapsed.
//! 7.  `test_text_selection_range`           — range(3,8).len() == 5.
//! 8.  `test_text_selection_is_not_collapsed`— range selection is not collapsed.
//! 9.  `test_build_text_input_a11y`          — node carries content and description.
//! 10. `test_update_text_cursor`             — updating cursor changes description.

use accesskit::NodeId;
use oxiui_accessibility::text_a11y::{build_text_input_a11y, update_text_cursor, TextSelection};
use oxiui_accessibility::{FocusIndicator, FocusRing};

// ── Focus indicator tests ─────────────────────────────────────────────────────

#[test]
fn test_focus_indicator_default() {
    let fi = FocusIndicator::new();
    assert_eq!(
        fi.focused_node(),
        None,
        "new FocusIndicator must have no focused node"
    );
}

#[test]
fn test_focus_indicator_set_focus() {
    let mut fi = FocusIndicator::new();
    fi.set_focus(Some(NodeId(42)));
    assert_eq!(
        fi.focused_node(),
        Some(NodeId(42)),
        "set_focus should store the given NodeId"
    );
}

#[test]
fn test_focus_indicator_clear_focus() {
    let mut fi = FocusIndicator::new();
    fi.set_focus(Some(NodeId(7)));
    fi.set_focus(None);
    assert_eq!(
        fi.focused_node(),
        None,
        "set_focus(None) should clear the focused node"
    );
}

#[test]
fn test_focus_ring_default_values() {
    let ring = FocusRing::default();
    // Default color: Windows-style highlight blue #0078D7, fully opaque.
    assert_eq!(
        ring.color,
        [0, 120, 215, 255],
        "default ring color should be #0078D7FF"
    );
    assert_eq!(ring.width, 2.0, "default ring width should be 2.0 px");
    assert_eq!(ring.offset, 2.0, "default ring offset should be 2.0 px");
    assert_eq!(ring.radius, 3.0, "default ring radius should be 3.0 px");
}

#[test]
fn test_focus_ring_custom() {
    let custom = FocusRing {
        color: [255, 0, 0, 128],
        width: 3.5,
        offset: 1.0,
        radius: 0.0,
    };
    let fi = FocusIndicator::new().with_ring(custom.clone());
    assert_eq!(*fi.ring(), custom, "with_ring should replace the ring spec");
}

// ── TextSelection tests ───────────────────────────────────────────────────────

#[test]
fn test_text_selection_cursor() {
    let sel = TextSelection::cursor(5);
    assert!(sel.is_collapsed(), "cursor(5) should be collapsed");
    assert_eq!(sel.start(), 5);
    assert_eq!(sel.end(), 5);
    assert_eq!(sel.len(), 0);
    assert!(sel.is_empty());
}

#[test]
fn test_text_selection_range() {
    let sel = TextSelection::range(3, 8);
    assert_eq!(sel.len(), 5, "range(3,8).len() should be 5");
    assert_eq!(sel.start(), 3);
    assert_eq!(sel.end(), 8);
}

#[test]
fn test_text_selection_is_not_collapsed() {
    let sel = TextSelection::range(3, 8);
    assert!(
        !sel.is_collapsed(),
        "a range selection must not be collapsed"
    );
    assert!(!sel.is_empty(), "a range selection must not be empty");
}

// ── Text input a11y synthesis tests ──────────────────────────────────────────

#[test]
fn test_build_text_input_a11y() {
    let content = "hello world";
    let sel = TextSelection::cursor(5);
    let node = build_text_input_a11y(content, sel, true);

    assert_eq!(
        node.text_content.as_deref(),
        Some("hello world"),
        "node should carry the content string"
    );

    let desc = node.props.description.as_deref().unwrap_or("");
    assert!(
        desc.contains("5"),
        "description should mention cursor position 5, got: {desc}"
    );
    assert!(
        desc.contains("cursor"),
        "description should mention 'cursor', got: {desc}"
    );
}

#[test]
fn test_update_text_cursor() {
    let content = "hello world";
    let initial_sel = TextSelection::cursor(0);
    let mut node = build_text_input_a11y(content, initial_sel, true);

    // Description reflects cursor at 0.
    let before = node.props.description.clone().unwrap_or_default();
    assert!(
        before.contains('0'),
        "initial description should mention position 0"
    );

    // Update to a range selection.
    let new_sel = TextSelection::range(3, 8);
    update_text_cursor(&mut node, new_sel);

    let after = node.props.description.as_deref().unwrap_or("");
    assert!(
        after.contains("3") && after.contains("8"),
        "updated description should mention bytes 3 and 8, got: {after}"
    );
    assert!(
        after != before,
        "description must change after update_text_cursor"
    );
}
