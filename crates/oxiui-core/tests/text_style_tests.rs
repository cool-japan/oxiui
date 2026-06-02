use oxiui_core::{ButtonResponse, TextStyle, UiCtx, WidgetResponse};

// ── TextStyle unit tests ─────────────────────────────────────────────────────

#[test]
fn test_text_style_default() {
    let s = TextStyle::default();
    assert_eq!(s.font_weight, 400);
    assert!(!s.italic);
    assert!(!s.underline);
    assert!(!s.strikethrough);
    assert!(s.font_size.is_none());
    assert!(s.color.is_none());
    assert!(s.line_height.is_none());
    assert_eq!(s.letter_spacing, 0.0);
}

#[test]
fn test_text_style_bold() {
    let s = TextStyle::bold();
    assert_eq!(s.font_weight, 700);
    assert!(!s.italic);
}

#[test]
fn test_text_style_italic() {
    let s = TextStyle::italic();
    assert!(s.italic);
    assert_eq!(s.font_weight, 400);
}

#[test]
fn test_text_style_with_size() {
    let s = TextStyle::default().with_size(18.0);
    assert_eq!(s.font_size, Some(18.0));
}

#[test]
fn test_text_style_heading_preset() {
    let s = TextStyle::heading();
    assert_eq!(s.font_size, Some(24.0));
    assert_eq!(s.font_weight, 700);
}

#[test]
fn test_text_style_caption_preset() {
    let s = TextStyle::caption();
    assert_eq!(s.font_size, Some(11.0));
    assert_eq!(s.font_weight, 400);
}

#[test]
fn test_text_style_chained_builders() {
    let s = TextStyle::default()
        .with_size(16.0)
        .with_weight(600)
        .with_color([0, 0, 0, 255]);
    assert_eq!(s.font_size, Some(16.0));
    assert_eq!(s.font_weight, 600);
    assert_eq!(s.color, Some([0, 0, 0, 255]));
}

// ── UiCtx integration tests ──────────────────────────────────────────────────

/// Minimal adapter implementing only the three required UiCtx methods.
struct BareCtx {
    label_calls: u32,
    heading_calls: u32,
}

impl BareCtx {
    fn new() -> Self {
        Self {
            label_calls: 0,
            heading_calls: 0,
        }
    }
}

impl UiCtx for BareCtx {
    fn heading(&mut self, _text: &str) {
        self.heading_calls += 1;
    }

    fn label(&mut self, _text: &str) {
        self.label_calls += 1;
    }

    fn button(&mut self, _label: &str) -> ButtonResponse {
        ButtonResponse::default()
    }
}

#[test]
fn test_label_styled_no_panic() {
    let mut ui = BareCtx::new();
    let style = TextStyle::default().with_size(14.0);
    let response: WidgetResponse = ui.label_styled("Hello, world!", style);
    assert!(
        response.supported,
        "label_styled should return supported=true"
    );
    assert_eq!(ui.label_calls, 1, "label should have been called once");
}

#[test]
fn test_heading_styled_no_panic() {
    let mut ui = BareCtx::new();
    let style = TextStyle::heading();
    let response: WidgetResponse = ui.heading_styled("Section Title", style);
    assert!(
        response.supported,
        "heading_styled should return supported=true"
    );
    assert_eq!(ui.heading_calls, 1, "heading should have been called once");
}
