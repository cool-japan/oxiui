use oxiui_core::Color;
use oxiui_theme::icons::{BuiltinIcons, IconName, IconSet, IconVariant};
use oxiui_theme::stylesheet::{CssValue, StyleSheet};
use oxiui_theme::Breakpoint;

// ── Selector parsing ─────────────────────────────────────────────────────────

#[test]
fn parse_simple_type_selector() {
    let result = StyleSheet::parse("button { color: #ff0000; }");
    assert!(result.diagnostics.is_empty());
    let style = result.stylesheet.compute_style("button", &[], None);
    assert_eq!(style.color, Some(CssValue::Color(Color(255, 0, 0, 255))));
}

#[test]
fn parse_class_selector() {
    let result = StyleSheet::parse(".primary { background-color: #0000ff; }");
    let style = result
        .stylesheet
        .compute_style("button", &["primary"], None);
    assert_eq!(
        style.background_color,
        Some(CssValue::Color(Color(0, 0, 255, 255)))
    );
}

#[test]
fn parse_id_selector() {
    let result = StyleSheet::parse("#submit { font-size: 14; }");
    let style = result
        .stylesheet
        .compute_style("button", &[], Some("submit"));
    assert!((style.font_size.unwrap_or(0.0) - 14.0).abs() < 0.01);
}

#[test]
fn parse_compound_selector() {
    let result = StyleSheet::parse("button.primary { padding: 8; }");
    let style = result
        .stylesheet
        .compute_style("button", &["primary"], None);
    assert!((style.padding.unwrap_or(0.0) - 8.0).abs() < 0.01);
    // no match without class
    let style2 = result.stylesheet.compute_style("button", &[], None);
    assert!(style2.padding.is_none());
}

#[test]
fn parse_grouped_selectors() {
    let result = StyleSheet::parse("h1, h2 { font-size: 20; }");
    let s1 = result.stylesheet.compute_style("h1", &[], None);
    let s2 = result.stylesheet.compute_style("h2", &[], None);
    assert!(s1.font_size.is_some());
    assert!(s2.font_size.is_some());
}

// ── Cascade and specificity ───────────────────────────────────────────────────

#[test]
fn cascade_specificity_id_over_class() {
    let result = StyleSheet::parse(".btn { color: #aaaaaa; } #submit { color: #bbbbbb; }");
    let style = result
        .stylesheet
        .compute_style("button", &["btn"], Some("submit"));
    assert_eq!(
        style.color,
        Some(CssValue::Color(Color(0xbb, 0xbb, 0xbb, 255)))
    );
}

#[test]
fn cascade_later_rule_wins_at_equal_specificity() {
    let result = StyleSheet::parse("button { color: #111111; } button { color: #222222; }");
    let style = result.stylesheet.compute_style("button", &[], None);
    assert_eq!(
        style.color,
        Some(CssValue::Color(Color(0x22, 0x22, 0x22, 255)))
    );
}

// ── Hex color variants ────────────────────────────────────────────────────────

#[test]
fn parse_hex_shorthand_3_digits() {
    let result = StyleSheet::parse("p { color: #f00; }");
    let style = result.stylesheet.compute_style("p", &[], None);
    assert_eq!(style.color, Some(CssValue::Color(Color(255, 0, 0, 255))));
}

#[test]
fn parse_hex_8_digits_with_alpha() {
    let result = StyleSheet::parse("p { color: #ff000080; }");
    let style = result.stylesheet.compute_style("p", &[], None);
    assert_eq!(style.color, Some(CssValue::Color(Color(255, 0, 0, 128))));
}

// ── rgb() function ───────────────────────────────────────────────────────────

#[test]
fn parse_rgb_function() {
    let result = StyleSheet::parse("p { color: rgb(10, 20, 30); }");
    let style = result.stylesheet.compute_style("p", &[], None);
    assert_eq!(style.color, Some(CssValue::Color(Color(10, 20, 30, 255))));
}

// ── Keywords ─────────────────────────────────────────────────────────────────

#[test]
fn parse_inherit_keyword() {
    let result = StyleSheet::parse("span { color: inherit; }");
    let style = result.stylesheet.compute_style("span", &[], None);
    assert_eq!(style.color, Some(CssValue::Inherit));
}

// ── Malformed input recovery ──────────────────────────────────────────────────

#[test]
fn malformed_rule_skipped_with_diagnostic() {
    let result = StyleSheet::parse("@@invalid { color: red } button { font-size: 12; }");
    // button rule should still parse
    let s = result.stylesheet.compute_style("button", &[], None);
    assert!(
        s.font_size.is_some(),
        "button font-size should parse despite malformed leading rule"
    );
}

// ── Property tests ────────────────────────────────────────────────────────────

#[test]
fn parse_all_numeric_properties() {
    let result = StyleSheet::parse(
        "div { padding: 4; margin: 8; font-size: 16; font-weight: 700; \
         border-width: 2; opacity: 1; }",
    );
    let s = result.stylesheet.compute_style("div", &[], None);
    assert!((s.padding.unwrap_or(0.0) - 4.0).abs() < 0.01);
    assert!((s.margin.unwrap_or(0.0) - 8.0).abs() < 0.01);
    assert!((s.font_size.unwrap_or(0.0) - 16.0).abs() < 0.01);
    assert!((s.font_weight.unwrap_or(0.0) - 700.0).abs() < 0.01);
    assert!((s.border_width.unwrap_or(0.0) - 2.0).abs() < 0.01);
    assert!((s.opacity.unwrap_or(0.0) - 1.0).abs() < 0.01);
}

#[test]
fn parse_background_shorthand() {
    let result = StyleSheet::parse("div { background: #123456; }");
    let s = result.stylesheet.compute_style("div", &[], None);
    assert!(s.background_color.is_some());
}

// ── Breakpoints ───────────────────────────────────────────────────────────────

#[test]
fn breakpoint_xs_below_576() {
    assert_eq!(Breakpoint::for_width(400.0), Breakpoint::Xs);
}

#[test]
fn breakpoint_md_768_to_991() {
    assert_eq!(Breakpoint::for_width(900.0), Breakpoint::Md);
}

#[test]
fn breakpoint_xxl_at_1536() {
    assert_eq!(Breakpoint::for_width(1536.0), Breakpoint::Xxl);
}

#[test]
fn breakpoint_ordering() {
    assert!(Breakpoint::Xs < Breakpoint::Sm);
    assert!(Breakpoint::Sm < Breakpoint::Md);
    assert!(Breakpoint::Md < Breakpoint::Lg);
    assert!(Breakpoint::Xl < Breakpoint::Xxl);
}

// ── IconSet ───────────────────────────────────────────────────────────────────

#[test]
fn icon_set_close_all_sizes() {
    let icons = BuiltinIcons::new();
    for size in [16u32, 20, 24, 32] {
        assert!(
            icons
                .path_data(IconName::Close, IconVariant::Outline, size)
                .is_some(),
            "Close outline missing for size {size}"
        );
        assert!(
            icons
                .path_data(IconName::Close, IconVariant::Filled, size)
                .is_some(),
            "Close filled missing for size {size}"
        );
    }
}

#[test]
fn icon_set_all_icons_have_24px_outline() {
    let icons = BuiltinIcons::new();
    let all = [
        IconName::Close,
        IconName::Menu,
        IconName::ArrowRight,
        IconName::ArrowLeft,
        IconName::ArrowUp,
        IconName::ArrowDown,
        IconName::Check,
        IconName::Search,
    ];
    for icon in all {
        assert!(
            icons.path_data(icon, IconVariant::Outline, 24).is_some(),
            "{icon:?} missing 24px outline"
        );
    }
}

#[test]
fn icon_set_rounded_aliases_outline() {
    let icons = BuiltinIcons::new();
    let outline = icons.path_data(IconName::Close, IconVariant::Outline, 24);
    let rounded = icons.path_data(IconName::Close, IconVariant::Rounded, 24);
    assert_eq!(outline, rounded);
}
