//! Integration tests for [`oxiui::RecordingUiCtx`] and the A11y snapshot builder.
//!
//! These tests verify that `RecordingUiCtx` captures widget calls correctly
//! and that `build_a11y_tree` returns a populated `A11yTree`.

#![cfg(feature = "a11y")]

use oxiui::RecordingUiCtx;
use oxiui_accessibility::WindowA11yId;
use oxiui_core::{RichTextSpan, UiCtx};

// ── Leaf widget recording ─────────────────────────────────────────────────────

#[test]
fn recording_ctx_records_heading_and_button() {
    let mut ctx = RecordingUiCtx::new();
    ctx.heading("Title");
    ctx.button("Click me");
    assert_eq!(ctx.entries.len(), 2);
    assert!(
        ctx.entries[0].label.contains("Title"),
        "heading label should contain 'Title'"
    );
    assert!(
        ctx.entries[1].label.contains("Click me"),
        "button label should contain 'Click me'"
    );
}

#[test]
fn recording_ctx_records_label() {
    let mut ctx = RecordingUiCtx::new();
    ctx.label("Hello world");
    assert_eq!(ctx.entries.len(), 1);
    assert_eq!(ctx.entries[0].label, "Hello world");
}

#[test]
fn button_returns_default_response_without_delegate() {
    let mut ctx = RecordingUiCtx::new();
    let resp = ctx.button("Submit");
    // Default ButtonResponse has clicked = false.
    assert!(!resp.clicked, "headless button should not report clicked");
}

// ── A11y tree construction ────────────────────────────────────────────────────

#[test]
fn build_a11y_tree_has_correct_node_count() {
    let mut ctx = RecordingUiCtx::new();
    ctx.heading("A");
    ctx.label("B");
    ctx.button("C");
    // build_a11y_tree should not panic; the tree is well-formed.
    let old_tree = ctx.build_a11y_tree(WindowA11yId(1));
    // Diff of the same tree against itself should produce an empty node list
    // (no changes), which verifies the snapshot was populated correctly.
    let delta = oxiui_accessibility::A11yTree::diff(&old_tree, &old_tree);
    assert!(
        delta.nodes.is_empty(),
        "diff of a tree against itself should have no changed nodes"
    );
}

// ── Layout container recording ────────────────────────────────────────────────

#[test]
fn horizontal_content_captured_as_children() {
    let mut ctx = RecordingUiCtx::new();
    ctx.horizontal(&mut |ui| {
        ui.label("child1");
        ui.label("child2");
    });
    assert_eq!(
        ctx.entries.len(),
        1,
        "one Group entry for horizontal layout"
    );
    assert_eq!(
        ctx.entries[0].children.len(),
        2,
        "horizontal group should have 2 child entries"
    );
    assert!(ctx.entries[0].label.contains("horizontal"));
}

#[test]
fn vertical_content_captured_as_children() {
    let mut ctx = RecordingUiCtx::new();
    ctx.vertical(&mut |ui| {
        ui.button("A");
        ui.button("B");
        ui.button("C");
    });
    assert_eq!(ctx.entries.len(), 1);
    assert_eq!(ctx.entries[0].children.len(), 3);
}

#[test]
fn grid_content_captured() {
    let mut ctx = RecordingUiCtx::new();
    ctx.grid(2, &mut |ui| {
        ui.label("cell-1");
        ui.label("cell-2");
    });
    assert_eq!(ctx.entries.len(), 1);
    assert_eq!(ctx.entries[0].children.len(), 2);
}

#[test]
fn menu_bar_content_captured() {
    let mut ctx = RecordingUiCtx::new();
    ctx.menu_bar(&mut |ui| {
        ui.button("File");
        ui.button("Edit");
    });
    assert_eq!(ctx.entries.len(), 1);
    assert!(ctx.entries[0].label.contains("menu_bar"));
    assert_eq!(ctx.entries[0].children.len(), 2);
}

// ── Rich text recording ───────────────────────────────────────────────────────

#[test]
fn recording_ctx_rich_text_captures_span_text() {
    let mut ctx = RecordingUiCtx::new();
    ctx.rich_text(&[RichTextSpan::new("hello"), RichTextSpan::new(" world")]);
    assert_eq!(ctx.entries.len(), 1);
    assert!(
        ctx.entries[0].label.contains("hello"),
        "rich_text entry should contain 'hello'"
    );
    assert!(
        ctx.entries[0].label.contains("world"),
        "rich_text entry should contain 'world'"
    );
}

// ── Delegate forwarding ───────────────────────────────────────────────────────

#[test]
fn with_delegate_records_and_passes_through() {
    use oxiui_core::ButtonResponse;

    // A minimal delegate that counts how many times button() is called.
    struct CountingCtx {
        count: usize,
    }
    impl UiCtx for CountingCtx {
        fn heading(&mut self, _text: &str) {}
        fn label(&mut self, _text: &str) {}
        fn button(&mut self, _label: &str) -> ButtonResponse {
            self.count += 1;
            ButtonResponse::default()
        }
    }

    let mut delegate = CountingCtx { count: 0 };
    {
        let mut ctx = RecordingUiCtx::with_delegate(&mut delegate);
        ctx.button("Save");
        ctx.button("Cancel");
        // Recording captures both.
        assert_eq!(ctx.entries.len(), 2);
    }
    // Delegate received both calls.
    assert_eq!(
        delegate.count, 2,
        "delegate should receive both button calls"
    );
}

// ── Nested layouts ────────────────────────────────────────────────────────────

#[test]
fn nested_horizontal_vertical_captured() {
    let mut ctx = RecordingUiCtx::new();
    ctx.horizontal(&mut |ui| {
        ui.vertical(&mut |ui2| {
            ui2.label("nested");
        });
    });
    assert_eq!(ctx.entries.len(), 1, "one top-level horizontal");
    let horiz = &ctx.entries[0];
    assert_eq!(
        horiz.children.len(),
        1,
        "horizontal has one child (vertical)"
    );
    let vert = &horiz.children[0];
    assert_eq!(vert.children.len(), 1, "vertical has one child (label)");
    assert_eq!(vert.children[0].label, "nested");
}

// ── App::build_a11y_snapshot integration ─────────────────────────────────────

#[test]
fn app_build_a11y_snapshot_returns_stable_tree() {
    use oxiui::{App, AppConfig};

    let mut app = App::new(AppConfig::new()).content(|ui| {
        ui.heading("Snapshot Test");
        ui.button("OK");
    });
    let tree = app.build_a11y_snapshot(WindowA11yId(42));
    // Diff of the snapshot against itself must produce zero changed nodes.
    let delta = oxiui_accessibility::A11yTree::diff(&tree, &tree);
    assert!(
        delta.nodes.is_empty(),
        "snapshot tree diff against itself should be empty"
    );
}

#[test]
fn app_build_a11y_snapshot_no_content_is_well_formed() {
    use oxiui::{App, AppConfig};

    let mut app = App::new(AppConfig::new());
    // No content closure — the returned tree is a no-op root.
    // It should not panic and produce a self-stable diff.
    let tree = app.build_a11y_snapshot(WindowA11yId(1));
    let delta = oxiui_accessibility::A11yTree::diff(&tree, &tree);
    assert!(delta.nodes.is_empty());
}
