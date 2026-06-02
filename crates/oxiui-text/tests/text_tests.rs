// Font fixture — shared with the oxitext test suite.
// Path relative to this tests/ directory:
//   oxiui/crates/oxiui-text/tests/ → ../../../../oxitext/tests/fixtures/test-font.ttf
static FONT_BYTES: &[u8] = include_bytes!("../../../../oxitext/tests/fixtures/test-font.ttf");

#[test]
fn shape_hello_returns_glyphs() {
    use oxiui_text::{TextPipeline, TextStyle};
    let mut pipeline = TextPipeline::from_bytes(FONT_BYTES).expect("pipeline creation failed");
    let style = TextStyle::default();
    let result = pipeline.shape("Hello", &style).expect("shape failed");
    // The pipeline must return at least one line with glyphs.
    let total_glyphs: usize = result.lines.iter().map(|l| l.len()).sum();
    assert!(total_glyphs > 0, "expected at least 1 glyph for 'Hello'");
}

#[test]
fn shape_produces_nonzero_metrics() {
    use oxiui_text::{TextPipeline, TextStyle};
    let mut pipeline = TextPipeline::from_bytes(FONT_BYTES).expect("pipeline creation failed");
    let style = TextStyle::default();
    let result = pipeline.shape("Hello", &style).expect("shape failed");
    assert!(
        result.total_width > 0.0,
        "shaped text must have nonzero width"
    );
    assert!(
        result.total_height > 0.0,
        "shaped text must have nonzero height"
    );
}

#[test]
fn text_pipeline_error_maps_to_ui_error() {
    // Empty text should not panic; it should either return Ok with 0 glyphs
    // or an Err — both are acceptable. This test just verifies no panic.
    use oxiui_text::{TextPipeline, TextStyle};
    let mut pipeline = TextPipeline::from_bytes(FONT_BYTES).expect("pipeline creation failed");
    let style = TextStyle::default();
    // Not checking the value, just that it doesn't panic.
    let _result = pipeline.shape("", &style);
}

#[test]
fn render_hello_produces_bitmaps() {
    use oxiui_text::{TextPipeline, TextStyle};
    let mut pipeline = TextPipeline::from_bytes(FONT_BYTES).expect("pipeline creation failed");
    let style = TextStyle::default();
    let result = pipeline.render("Hello", &style).expect("render failed");
    // Both glyphs and bitmaps slices must be the same length.
    assert_eq!(
        result.glyphs.len(),
        result.bitmaps.len(),
        "glyphs and bitmaps must have the same length"
    );
    assert!(
        !result.glyphs.is_empty(),
        "expected at least one positioned glyph"
    );
}
