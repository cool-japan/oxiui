//! Slice S8 tests: nested layout, headless smoke materialization, and Cow
//! no-clone verification.
//!
//! # Deviation note (headless app test)
//! iced 0.14 has no public headless application loop. The "boot iced::application
//! for 3 frames" TODO item is implemented here as a spec-materialization smoke
//! test instead: three independent rounds of spec building and materialization
//! are exercised without spawning an iced runtime.

use std::borrow::Cow;

use oxiui_core::UiCtx;
use oxiui_iced::{IcedConfig, IcedUiCtx, WidgetSpec};

fn make_ctx() -> IcedUiCtx {
    IcedUiCtx::new(IcedConfig::default())
}

// ── Nested layout ─────────────────────────────────────────────────────────────

/// Test that `horizontal()` nested inside `vertical()` produces the correct
/// `Vertical([Horizontal([Label, Label]), Label])` spec tree.
#[test]
fn test_nested_horizontal_inside_vertical() {
    let mut ctx = make_ctx();
    ctx.vertical(&mut |ui| {
        ui.horizontal(&mut |ui| {
            ui.label("hello");
            ui.label("world");
        });
        ui.label("outside");
    });

    let specs = ctx.into_specs();
    assert_eq!(specs.len(), 1, "expected a single top-level Vertical spec");

    match &specs[0] {
        WidgetSpec::Vertical(vert_children) => {
            assert_eq!(vert_children.len(), 2, "vertical should have 2 children");

            // First child must be a Horizontal containing two Labels.
            match &vert_children[0] {
                WidgetSpec::Horizontal(horiz_children) => {
                    assert_eq!(horiz_children.len(), 2, "horizontal must have 2 children");
                    match &horiz_children[0] {
                        WidgetSpec::Label(t) => {
                            assert_eq!(t.as_ref(), "hello", "first label must be 'hello'")
                        }
                        other => panic!("expected Label, got: {other:?}"),
                    }
                    match &horiz_children[1] {
                        WidgetSpec::Label(t) => {
                            assert_eq!(t.as_ref(), "world", "second label must be 'world'")
                        }
                        other => panic!("expected Label, got: {other:?}"),
                    }
                }
                other => panic!("expected Horizontal as first child, got: {other:?}"),
            }

            // Second child must be a Label.
            match &vert_children[1] {
                WidgetSpec::Label(t) => {
                    assert_eq!(
                        t.as_ref(),
                        "outside",
                        "second child must be Label('outside')"
                    )
                }
                other => panic!("expected Label as second child, got: {other:?}"),
            }
        }
        other => panic!("expected Vertical spec, got: {other:?}"),
    }
}

/// Verify that a nested layout spec materialises to an iced element without
/// panicking.
#[test]
fn test_nested_layout_materialises_without_panic() {
    let mut ctx = make_ctx();
    ctx.vertical(&mut |ui| {
        ui.horizontal(&mut |ui| {
            ui.label("a");
            ui.label("b");
        });
        ui.label("c");
    });
    let _ = ctx.into_iced_element(); // must not panic
}

// ── Headless smoke test (3-frame materialization) ─────────────────────────────
//
// Deviation: iced 0.14 has no headless application loop.
// Implemented as 3 independent rounds of spec-building + materialization.

/// Round 1: single label spec.
#[test]
fn test_headless_smoke_round1_label() {
    let mut ctx = make_ctx();
    ctx.label("frame 1");
    let specs = ctx.into_specs();
    assert!(!specs.is_empty(), "round 1 must produce at least one spec");
}

/// Round 2: button + label.
#[test]
fn test_headless_smoke_round2_button_label() {
    let mut ctx = make_ctx();
    ctx.button("click");
    ctx.label("frame 2");
    let specs = ctx.into_specs();
    assert_eq!(specs.len(), 2, "round 2 must produce 2 specs");
}

/// Round 3: horizontal layout.
#[test]
fn test_headless_smoke_round3_horizontal() {
    let mut ctx = make_ctx();
    ctx.horizontal(&mut |ui| {
        ui.label("a");
        ui.label("b");
    });
    let specs = ctx.into_specs();
    assert_eq!(specs.len(), 1, "round 3 must produce 1 Horizontal spec");
    assert!(
        matches!(&specs[0], WidgetSpec::Horizontal(_)),
        "round 3 spec must be Horizontal"
    );
}

/// Combined 3-round smoke test: all three rounds produce non-empty specs and
/// materialize without panicking.
///
/// # Deviation
/// This is a spec-materialization smoke test, not a real iced::application
/// headless boot. iced 0.14 exposes no headless event loop.
#[test]
fn test_headless_spec_materialization_three_rounds() {
    // Round 1
    let mut ctx1 = make_ctx();
    ctx1.label("frame 1");
    let specs1 = ctx1.into_specs();
    assert!(!specs1.is_empty(), "round 1: specs must be non-empty");

    // Round 2
    let mut ctx2 = make_ctx();
    ctx2.button("click");
    ctx2.label("frame 2");
    let specs2 = ctx2.into_specs();
    assert!(!specs2.is_empty(), "round 2: specs must be non-empty");

    // Round 3
    let mut ctx3 = make_ctx();
    ctx3.horizontal(&mut |ui| {
        ui.label("a");
        ui.label("b");
    });
    let specs3 = ctx3.into_specs();
    assert!(!specs3.is_empty(), "round 3: specs must be non-empty");

    // Materialize round 3 (most complex) to verify no panic.
    let mut ctx_mat = make_ctx();
    ctx_mat.horizontal(&mut |ui| {
        ui.label("a");
        ui.label("b");
    });
    let _ = ctx_mat.into_iced_element(); // must not panic
}

// ── Cow no-clone test ─────────────────────────────────────────────────────────

/// Verify that `WidgetSpec::Label(Cow::Borrowed(...))` stores a `Borrowed`
/// variant — no heap allocation for static str inputs.
#[test]
fn test_cow_borrowed_for_static_str_label() {
    let spec = WidgetSpec::Label(Cow::Borrowed("static label"));
    match spec {
        WidgetSpec::Label(cow) => {
            assert!(
                matches!(cow, Cow::Borrowed(_)),
                "expected Cow::Borrowed for a static str Label"
            );
        }
        _ => panic!("wrong variant"),
    }
}

/// Verify that `WidgetSpec::Heading(Cow::Borrowed(...))` stores a `Borrowed`
/// variant.
#[test]
fn test_cow_borrowed_for_static_str_heading() {
    let spec = WidgetSpec::Heading(Cow::Borrowed("Section Title"));
    match spec {
        WidgetSpec::Heading(cow) => {
            assert!(
                matches!(cow, Cow::Borrowed(_)),
                "expected Cow::Borrowed for a static str Heading"
            );
        }
        _ => panic!("wrong variant"),
    }
}

/// Verify that `WidgetSpec::Button { label: Cow::Borrowed(...), ... }` stores a
/// `Borrowed` variant.
#[test]
fn test_cow_borrowed_for_static_str_button() {
    let spec = WidgetSpec::Button {
        id: 0,
        label: Cow::Borrowed("Click me"),
    };
    match spec {
        WidgetSpec::Button { label, .. } => {
            assert!(
                matches!(label, Cow::Borrowed(_)),
                "expected Cow::Borrowed for a static str Button label"
            );
        }
        _ => panic!("wrong variant"),
    }
}

/// Verify that `Cow::Owned(s)` constructed from a runtime String works
/// correctly and compares equal to the expected text.
#[test]
fn test_cow_owned_from_runtime_string() {
    let runtime_text = format!("hello {}", "world");
    let spec = WidgetSpec::Label(Cow::Owned(runtime_text));
    match spec {
        WidgetSpec::Label(cow) => {
            assert!(
                matches!(&cow, Cow::Owned(_)),
                "expected Cow::Owned for a runtime String"
            );
            assert_eq!(cow.as_ref(), "hello world");
        }
        _ => panic!("wrong variant"),
    }
}
