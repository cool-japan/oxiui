//! Snapshot tests: render a reference UI through `EguiUiCtx`, compare egui
//! output shapes against expected structure.
//!
//! Instead of pixel-level comparison (which is platform-dependent), we verify:
//! - The total number of paint shapes produced is non-zero.
//! - The output contains at least one text or mesh shape (label/button rendered).
//! - Re-rendering the same UI in a second frame produces the same shape count
//!   (deterministic output).
//!
//! This avoids fragile pixel comparisons while still catching regressions where
//! widgets stop being painted at all.

use egui::epaint::{ClippedShape, Shape};
use oxiui_core::{Palette, RichTextSpan, UiCtx};
use oxiui_egui::EguiUiCtx;

fn color(r: u8, g: u8, b: u8) -> oxiui_core::Color {
    oxiui_core::Color(r, g, b, 255)
}

fn test_palette() -> Palette {
    Palette {
        background: color(30, 30, 30),
        surface: color(45, 45, 45),
        primary: color(100, 149, 237),
        on_primary: color(255, 255, 255),
        text: color(240, 240, 240),
        muted: color(160, 160, 160),
    }
}

/// Run two egui headless frames using the same draw function and return the
/// shape counts `(frame1, frame2)` for determinism checks.
fn collect_shapes_two_frames<F>(mut draw_fn: F) -> (usize, usize)
where
    F: FnMut(&mut dyn UiCtx),
{
    let ctx = egui::Context::default();
    let raw_input = egui::RawInput::default();

    // Frame 1
    let out1 = ctx.run_ui(raw_input.clone(), |ui| {
        let mut oxi = EguiUiCtx::new(ui);
        draw_fn(&mut oxi);
    });
    let count1 = shape_count(&out1.shapes);

    // Frame 2 — same content, must be deterministic
    let out2 = ctx.run_ui(raw_input, |ui| {
        let mut oxi = EguiUiCtx::new(ui);
        draw_fn(&mut oxi);
    });
    let count2 = shape_count(&out2.shapes);

    (count1, count2)
}

/// Recursively count all leaf shapes (non-Vec shapes).
fn shape_count(shapes: &[ClippedShape]) -> usize {
    shapes.iter().map(|cs| count_shape(&cs.shape)).sum()
}

fn count_shape(shape: &Shape) -> usize {
    match shape {
        Shape::Vec(inner) => inner.iter().map(count_shape).sum(),
        _ => 1,
    }
}

/// At least one shape must have some kind of text or mesh content.
fn has_text_or_mesh(shapes: &[ClippedShape]) -> bool {
    shapes.iter().any(|cs| shape_has_text_or_mesh(&cs.shape))
}

fn shape_has_text_or_mesh(shape: &Shape) -> bool {
    match shape {
        Shape::Text(_) => true,
        Shape::Mesh(_) => true,
        Shape::Vec(inner) => inner.iter().any(shape_has_text_or_mesh),
        _ => false,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// A label renders at least one shape.
#[test]
fn snapshot_label_produces_shapes() {
    let (c1, c2) = collect_shapes_two_frames(|ui| {
        ui.label("Hello, OxiUI!");
    });
    assert!(c1 > 0, "first frame produced no shapes");
    assert_eq!(c1, c2, "shape count must be deterministic across frames");
}

/// A heading produces shapes.
#[test]
fn snapshot_heading_produces_shapes() {
    let (c1, c2) = collect_shapes_two_frames(|ui| {
        ui.heading("Section Title");
    });
    assert!(c1 > 0, "heading produced no shapes");
    assert_eq!(c1, c2, "heading shape count not deterministic");
}

/// A button produces shapes.
#[test]
fn snapshot_button_produces_shapes() {
    let (c1, c2) = collect_shapes_two_frames(|ui| {
        ui.button("Click Me");
    });
    assert!(c1 > 0, "button produced no shapes");
    assert_eq!(c1, c2, "button shape count not deterministic");
}

/// A reference UI with heading + labels + button + separator yields a non-zero,
/// deterministic shape count that contains at least one text or mesh shape.
#[test]
fn snapshot_reference_ui_shapes() {
    let ctx = egui::Context::default();
    let raw_input = egui::RawInput::default();

    // Apply palette.
    ctx.set_visuals(oxiui_egui::palette_to_egui_visuals(&test_palette()));

    let out1 = ctx.run_ui(raw_input.clone(), |ui| {
        let mut oxi = EguiUiCtx::new(ui);
        oxi.heading("Dashboard");
        oxi.label("Welcome to OxiUI egui adapter.");
        oxi.separator();
        oxi.label("Status: OK");
        oxi.button("Refresh");
    });

    let out2 = ctx.run_ui(raw_input, |ui| {
        let mut oxi = EguiUiCtx::new(ui);
        oxi.heading("Dashboard");
        oxi.label("Welcome to OxiUI egui adapter.");
        oxi.separator();
        oxi.label("Status: OK");
        oxi.button("Refresh");
    });

    let c1 = shape_count(&out1.shapes);
    let c2 = shape_count(&out2.shapes);

    assert!(c1 > 0, "reference UI produced no shapes");
    assert!(
        has_text_or_mesh(&out1.shapes),
        "reference UI has no text or mesh shapes"
    );
    assert_eq!(
        c1, c2,
        "reference UI shape count not deterministic (frame 1={c1}, frame 2={c2})"
    );
}

/// Rich text with multiple spans produces deterministic shapes.
#[test]
fn snapshot_rich_text_shapes_deterministic() {
    let spans = vec![
        RichTextSpan::new("Bold ")
            .font_size(14.0)
            .color([220, 50, 50, 255])
            .bold(),
        RichTextSpan::new("Italic ")
            .font_size(14.0)
            .color([50, 200, 50, 255])
            .italic(),
        RichTextSpan::new("Normal")
            .font_size(14.0)
            .color([200, 200, 200, 255]),
    ];

    let (c1, c2) = collect_shapes_two_frames(|ui| {
        ui.rich_text(&spans);
    });

    assert!(c1 > 0, "rich text produced no shapes");
    assert_eq!(c1, c2, "rich text shape count not deterministic");
}

/// Horizontal layout with two labels produces shapes.
#[test]
fn snapshot_horizontal_layout_shapes() {
    let (c1, c2) = collect_shapes_two_frames(|ui| {
        ui.horizontal(&mut |inner: &mut dyn UiCtx| {
            inner.label("Left");
            inner.label("Right");
        });
    });
    assert!(c1 > 0, "horizontal layout produced no shapes");
    assert_eq!(c1, c2, "horizontal layout not deterministic");
}

/// Separator alone produces at least one shape.
#[test]
fn snapshot_separator_produces_shape() {
    let (c1, _) = collect_shapes_two_frames(|ui| {
        ui.separator();
    });
    assert!(c1 > 0, "separator produced no shapes");
}
