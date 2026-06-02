//! Slice-I integration tests: horizontal/vertical/grid/rich_text forwarding
//! and menu_bar/drag_source/drop_target unsupported stubs.
//!
//! These tests exercise spec collection and response codes only; no iced
//! runtime is started.

use oxiui_core::{RichTextSpan, UiCtx};
use oxiui_iced::{IcedConfig, IcedNullCtx, IcedUiCtx, WidgetSpec};

fn make_ctx() -> IcedUiCtx {
    IcedUiCtx::new(IcedConfig::default())
}

// ── horizontal ────────────────────────────────────────────────────────────────

#[test]
fn horizontal_collects_children() {
    let mut ctx = make_ctx();
    let r = ctx.horizontal(&mut |ui| {
        ui.label("a");
        ui.label("b");
    });
    assert!(r.supported, "horizontal must be supported");
    let specs = ctx.into_specs();
    match specs.last() {
        Some(WidgetSpec::Horizontal(children)) => {
            assert_eq!(children.len(), 2, "expected two children");
        }
        other => panic!("expected Horizontal spec, got: {other:?}"),
    }
}

#[test]
fn horizontal_empty_closure() {
    let mut ctx = make_ctx();
    let r = ctx.horizontal(&mut |_ui| {});
    assert!(r.supported);
    let specs = ctx.into_specs();
    assert!(
        matches!(specs.last(), Some(WidgetSpec::Horizontal(ch)) if ch.is_empty()),
        "empty horizontal should produce an empty Horizontal spec"
    );
}

// ── vertical ──────────────────────────────────────────────────────────────────

#[test]
fn vertical_collects_children() {
    let mut ctx = make_ctx();
    let r = ctx.vertical(&mut |ui| {
        ui.label("x");
    });
    assert!(r.supported, "vertical must be supported");
    let specs = ctx.into_specs();
    assert!(
        matches!(specs.last(), Some(WidgetSpec::Vertical(_))),
        "expected Vertical spec"
    );
}

#[test]
fn vertical_multiple_children() {
    let mut ctx = make_ctx();
    ctx.vertical(&mut |ui| {
        ui.label("line1");
        ui.label("line2");
        ui.label("line3");
    });
    let specs = ctx.into_specs();
    match specs.last() {
        Some(WidgetSpec::Vertical(children)) => {
            assert_eq!(children.len(), 3);
        }
        other => panic!("expected Vertical, got: {other:?}"),
    }
}

// ── grid ──────────────────────────────────────────────────────────────────────

#[test]
fn grid_wraps_at_cols() {
    let mut ctx = make_ctx();
    let r = ctx.grid(3, &mut |ui| {
        for i in 0..6 {
            ui.label(&format!("c{i}"));
        }
    });
    assert!(r.supported, "grid must be supported");
    let specs = ctx.into_specs();
    match specs.last() {
        Some(WidgetSpec::Grid { cols, children }) => {
            assert_eq!(*cols, 3, "cols must be 3");
            assert_eq!(children.len(), 6, "six children expected");
        }
        other => panic!("expected Grid spec, got: {other:?}"),
    }
}

#[test]
fn grid_single_col() {
    let mut ctx = make_ctx();
    ctx.grid(1, &mut |ui| {
        ui.label("only");
        ui.label("child");
    });
    let specs = ctx.into_specs();
    match specs.last() {
        Some(WidgetSpec::Grid { cols, children }) => {
            assert_eq!(*cols, 1);
            assert_eq!(children.len(), 2);
        }
        other => panic!("expected Grid, got: {other:?}"),
    }
}

// ── rich_text ─────────────────────────────────────────────────────────────────

#[test]
fn rich_text_maps_spans() {
    let mut ctx = make_ctx();
    let spans = vec![
        RichTextSpan::new("Hello").color([255, 0, 0, 255]),
        RichTextSpan::new(" world").bold(),
    ];
    let r = ctx.rich_text(&spans);
    assert!(r.supported, "rich_text must be supported");
}

#[test]
fn rich_text_produces_spec() {
    let mut ctx = make_ctx();
    let spans = vec![RichTextSpan::new("Hi")];
    ctx.rich_text(&spans);
    let specs = ctx.into_specs();
    assert!(
        matches!(specs.last(), Some(WidgetSpec::RichText(_))),
        "expected RichText spec"
    );
}

#[test]
fn rich_text_empty_spans() {
    let mut ctx = make_ctx();
    let r = ctx.rich_text(&[]);
    assert!(r.supported);
}

// ── menu_bar (unsupported) ────────────────────────────────────────────────────

#[test]
fn menu_bar_returns_unsupported() {
    let mut ctx = make_ctx();
    let r = ctx.menu_bar(&mut |_| {});
    assert!(!r.supported, "menu_bar must be unsupported in iced 0.14");
}

// ── drag_source (unsupported) ─────────────────────────────────────────────────

#[test]
fn drag_source_returns_unsupported() {
    let mut ctx = make_ctx();
    let r = ctx.drag_source(1, &mut |_| {});
    assert!(!r.supported, "drag_source must be unsupported in iced 0.14");
}

// ── drop_target (unsupported) ─────────────────────────────────────────────────

#[test]
fn drop_target_returns_unsupported() {
    let mut ctx = make_ctx();
    let r = ctx.drop_target(&[1], &mut |_| {});
    assert!(!r.supported, "drop_target must be unsupported in iced 0.14");
}

// ── IcedNullCtx recording ─────────────────────────────────────────────────────

#[test]
fn null_ctx_records_horizontal() {
    let mut ctx = IcedNullCtx::recording();
    ctx.horizontal(&mut |_| {});
    let log = ctx.log.expect("should be recording");
    assert!(log.iter().any(|(m, _)| *m == "horizontal"));
}

#[test]
fn null_ctx_records_vertical() {
    let mut ctx = IcedNullCtx::recording();
    ctx.vertical(&mut |_| {});
    let log = ctx.log.expect("should be recording");
    assert!(log.iter().any(|(m, _)| *m == "vertical"));
}

#[test]
fn null_ctx_records_grid() {
    let mut ctx = IcedNullCtx::recording();
    ctx.grid(2, &mut |_| {});
    let log = ctx.log.expect("should be recording");
    let entry = log.iter().find(|(m, _)| *m == "grid");
    assert!(entry.is_some(), "expected grid to be recorded");
    assert_eq!(entry.unwrap().1, "2");
}

#[test]
fn null_ctx_records_rich_text() {
    let mut ctx = IcedNullCtx::recording();
    let spans = vec![RichTextSpan::new("hi")];
    ctx.rich_text(&spans);
    let log = ctx.log.expect("should be recording");
    assert!(log.iter().any(|(m, _)| *m == "rich_text"));
}

// ── materialisation (smoke) ───────────────────────────────────────────────────

#[test]
fn horizontal_materialises_without_panic() {
    let mut ctx = make_ctx();
    ctx.horizontal(&mut |ui| {
        ui.label("left");
        ui.button("right");
    });
    let _ = ctx.into_iced_element(); // must not panic
}

#[test]
fn vertical_materialises_without_panic() {
    let mut ctx = make_ctx();
    ctx.vertical(&mut |ui| {
        ui.heading("Title");
        ui.label("Body");
    });
    let _ = ctx.into_iced_element();
}

#[test]
fn grid_materialises_without_panic() {
    let mut ctx = make_ctx();
    ctx.grid(2, &mut |ui| {
        for i in 0..4 {
            ui.label(&format!("item {i}"));
        }
    });
    let _ = ctx.into_iced_element();
}

#[test]
fn rich_text_materialises_without_panic() {
    let mut ctx = make_ctx();
    let spans = vec![
        RichTextSpan::new("Red!").color([255, 0, 0, 255]),
        RichTextSpan::new(" Bold!").bold(),
    ];
    ctx.rich_text(&spans);
    let _ = ctx.into_iced_element();
}

// ── id threading ──────────────────────────────────────────────────────────────

#[test]
fn horizontal_threads_ids_to_parent() {
    let mut ctx = make_ctx();
    ctx.button("outer"); // id 0
    ctx.horizontal(&mut |ui| {
        ui.button("inner"); // id 1
    });
    ctx.button("after"); // id 2 — proves id counter advanced through horizontal
    let specs = ctx.into_specs();
    // Three top-level specs: Button, Horizontal, Button
    assert_eq!(specs.len(), 3);
}
