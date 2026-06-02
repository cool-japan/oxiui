use oxiui_core::{Color, Palette};
use oxiui_egui::palette_to_egui_visuals;

#[test]
fn palette_to_visuals_maps_text_color() {
    let palette = Palette {
        text: Color(192, 202, 245, 255),
        background: Color(26, 27, 38, 255),
        surface: Color(36, 40, 59, 255),
        primary: Color(122, 162, 247, 255),
        on_primary: Color(26, 27, 38, 255),
        muted: Color(86, 95, 137, 255),
    };
    let visuals = palette_to_egui_visuals(&palette);
    let text_c = visuals
        .override_text_color
        .expect("text color should be set");
    assert_eq!(text_c.r(), 192);
    assert_eq!(text_c.g(), 202);
    assert_eq!(text_c.b(), 245);
}

#[test]
fn palette_to_visuals_maps_panel_fill() {
    let palette = Palette {
        text: Color(192, 202, 245, 255),
        background: Color(26, 27, 38, 255),
        surface: Color(36, 40, 59, 255),
        primary: Color(122, 162, 247, 255),
        on_primary: Color(26, 27, 38, 255),
        muted: Color(86, 95, 137, 255),
    };
    let visuals = palette_to_egui_visuals(&palette);
    assert_eq!(visuals.panel_fill.r(), 26);
    assert_eq!(visuals.panel_fill.g(), 27);
    assert_eq!(visuals.panel_fill.b(), 38);
}

#[test]
fn palette_to_visuals_maps_selection_color() {
    let palette = Palette {
        text: Color(192, 202, 245, 255),
        background: Color(26, 27, 38, 255),
        surface: Color(36, 40, 59, 255),
        primary: Color(122, 162, 247, 255),
        on_primary: Color(26, 27, 38, 255),
        muted: Color(86, 95, 137, 255),
    };
    let visuals = palette_to_egui_visuals(&palette);
    assert_eq!(visuals.selection.bg_fill.r(), 122);
    assert_eq!(visuals.selection.bg_fill.g(), 162);
}
