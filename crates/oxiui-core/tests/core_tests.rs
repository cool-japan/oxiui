use oxiui_core::{Color, FontSpec, Palette, UiError};

#[test]
fn ui_error_display_backend() {
    let e = UiError::Backend("test".to_string());
    assert!(e.to_string().contains("test"));
}

#[test]
fn ui_error_display_render() {
    let e = UiError::Render("render problem".to_string());
    assert!(e.to_string().contains("render problem"));
}

#[test]
fn ui_error_display_unsupported() {
    let e = UiError::Unsupported("no backend".to_string());
    assert!(e.to_string().contains("no backend"));
}

#[test]
fn palette_fields() {
    let p = Palette {
        background: Color(26, 27, 38, 255),
        surface: Color(36, 40, 59, 255),
        primary: Color(122, 162, 247, 255),
        on_primary: Color(26, 27, 38, 255),
        text: Color(192, 202, 245, 255),
        muted: Color(86, 95, 137, 255),
    };
    assert_eq!(p.background.0, 26);
    assert_eq!(p.primary.2, 247);
}

#[test]
fn font_spec_default() {
    let f = FontSpec::default();
    assert!(f.size > 0.0);
    assert!(f.weight > 0);
    assert_eq!(f.family, "Inter");
}

#[test]
fn font_spec_new() {
    let f = FontSpec::new("JetBrains Mono", 12.0, 700);
    assert_eq!(f.family, "JetBrains Mono");
    assert_eq!(f.size, 12.0);
    assert_eq!(f.weight, 700);
}

#[test]
fn color_equality() {
    let a = Color(255, 128, 0, 255);
    let b = Color(255, 128, 0, 255);
    let c = Color(0, 0, 0, 255);
    assert_eq!(a, b);
    assert_ne!(a, c);
}
