//! Bridge between `oxiui-theme` design tokens and `oxiui-render-wgpu` draw primitives.
//!
//! Enabled by the `theme` Cargo feature.  All functions are pure-Rust, zero-copy
//! converters — no GPU work happens here.
//!
//! # Functions
//!
//! | Function | Purpose |
//! |---|---|
//! | [`shadow_spec_to_desc`] | Convert a `ShadowSpec` + rect to a [`ShadowDesc`] |
//! | [`push_shadow_spec`] | Push a `BoxShadow` command derived from a `ShadowSpec` |
//! | [`push_border_spec`] | Push stroke commands derived from a `BorderSpec` |
//! | [`push_border_specs`] | Push stroke commands for each visible side of a `BorderSpecs` |
//! | [`primary_gradient_stops`] | Two-stop gradient ramp from a palette's background→primary |
//! | [`surface_gradient_stops`] | Two-stop gradient ramp from background→surface |
//! | [`status_gradient_stops`] | Three-stop ramp using the extended-palette status colours |
//! | [`push_theme_gradient`] | Push a themed linear gradient onto a `DrawList` |

use oxiui_core::{
    geometry::{Point, Rect},
    paint::{DrawList, GradientStop},
    Color,
};
use oxiui_theme::{BorderSpec, BorderSpecs, BorderStyle, ExtendedPalette, ShadowSpec};

use crate::gpu::shadow::ShadowDesc;

// ── ShadowSpec → ShadowDesc ───────────────────────────────────────────────────

/// Convert an [`oxiui_theme::ShadowSpec`] plus a source rectangle into a
/// [`ShadowDesc`] ready for the render-wgpu shadow pipeline.
///
/// The shadow rectangle is computed by shifting `rect` by the spec's
/// (`offset_x`, `offset_y`).  `ShadowSpec::spread` grows the shadow rect
/// outward by `spread` pixels on all sides (negative spread shrinks it).
///
/// Returns `None` when the shadow is invisible (i.e. `spec.is_invisible()`).
///
/// # Example
///
/// ```rust
/// # use oxiui_core::geometry::Rect;
/// # use oxiui_theme::ShadowSpec;
/// # use oxiui_render_wgpu::theme_bridge::shadow_spec_to_desc;
/// let rect = Rect::new(10.0, 10.0, 100.0, 50.0);
/// let spec = ShadowSpec::drop_shadow(2.0, 4.0, 8.0);
/// let desc = shadow_spec_to_desc(rect, &spec);
/// assert!(desc.is_some());
/// ```
pub fn shadow_spec_to_desc(rect: Rect, spec: &ShadowSpec) -> Option<ShadowDesc> {
    if spec.is_invisible() {
        return None;
    }
    let spread = spec.spread;
    let shadow_rect = Rect::new(
        rect.left() + spec.offset_x - spread,
        rect.top() + spec.offset_y - spread,
        (rect.width() + 2.0 * spread).max(0.0),
        (rect.height() + 2.0 * spread).max(0.0),
    );
    Some(ShadowDesc {
        shadow_rect,
        color: spec.color,
        blur_radius: spec.blur,
    })
}

// ── push_shadow_spec ──────────────────────────────────────────────────────────

/// Push a [`oxiui_core::paint::DrawCommand::BoxShadow`] derived from a [`ShadowSpec`] onto `list`.
///
/// The `offset` point from the spec is mapped to `DrawCommand::BoxShadow`'s
/// `offset` field.  `spread` is applied by adjusting the source `rect` outward
/// before pushing.  Invisible specs (alpha == 0) are silently skipped.
///
/// # Example
///
/// ```rust
/// # use oxiui_core::{geometry::Rect, paint::DrawList};
/// # use oxiui_theme::ShadowSpec;
/// # use oxiui_render_wgpu::theme_bridge::push_shadow_spec;
/// let mut list = DrawList::new();
/// let rect = Rect::new(0.0, 0.0, 80.0, 40.0);
/// let spec = ShadowSpec::drop_shadow(1.0, 2.0, 4.0);
/// push_shadow_spec(&mut list, rect, &spec);
/// assert_eq!(list.len(), 1);
/// ```
pub fn push_shadow_spec(list: &mut DrawList, rect: Rect, spec: &ShadowSpec) {
    if spec.is_invisible() {
        return;
    }
    let spread = spec.spread;
    let effective_rect = Rect::new(
        rect.left() - spread,
        rect.top() - spread,
        (rect.width() + 2.0 * spread).max(0.0),
        (rect.height() + 2.0 * spread).max(0.0),
    );
    list.push_shadow(
        effective_rect,
        Point::new(spec.offset_x, spec.offset_y),
        spec.blur,
        spec.color,
    );
}

// ── BorderSpec → DrawList stroke commands ─────────────────────────────────────

/// Push a single uniform `StrokeRect` derived from a [`BorderSpec`] onto `list`.
///
/// The stroke is placed on the outside of `rect` (expanded by `spec.width / 2`
/// so the inner edge of the stroke aligns with `rect`'s edge).  Invisible specs
/// (width ≤ 0, alpha == 0, or `BorderStyle::None`) are silently skipped.
///
/// Dashed and dotted styles are rendered using `push_line_dashed` along each
/// edge.  Double-line style is rendered as two strokes at `width / 3` each, with
/// a gap of `width / 3` between them.
///
/// # Example
///
/// ```rust
/// # use oxiui_core::{geometry::Rect, paint::DrawList, Color};
/// # use oxiui_theme::{BorderSpec, BorderStyle};
/// # use oxiui_render_wgpu::theme_bridge::push_border_spec;
/// let mut list = DrawList::new();
/// let rect = Rect::new(10.0, 10.0, 100.0, 50.0);
/// let spec = BorderSpec::solid(2.0, Color(0, 0, 0, 255));
/// push_border_spec(&mut list, rect, &spec);
/// assert_eq!(list.len(), 1);
/// ```
pub fn push_border_spec(list: &mut DrawList, rect: Rect, spec: &BorderSpec) {
    if spec.is_invisible() {
        return;
    }
    match spec.style {
        BorderStyle::None => {}
        BorderStyle::Solid => {
            list.push_stroke_rect(rect, spec.width, spec.color);
        }
        BorderStyle::Dashed => {
            let dash_len = (spec.width * 3.0).max(4.0);
            let gap_len = (spec.width * 2.0).max(2.0);
            let tl = Point::new(rect.left(), rect.top());
            let tr = Point::new(rect.right(), rect.top());
            let br = Point::new(rect.right(), rect.bottom());
            let bl = Point::new(rect.left(), rect.bottom());
            list.push_line_dashed(tl, tr, dash_len, gap_len, spec.color);
            list.push_line_dashed(tr, br, dash_len, gap_len, spec.color);
            list.push_line_dashed(br, bl, dash_len, gap_len, spec.color);
            list.push_line_dashed(bl, tl, dash_len, gap_len, spec.color);
        }
        BorderStyle::Dotted => {
            let dot = spec.width;
            let gap = spec.width;
            let tl = Point::new(rect.left(), rect.top());
            let tr = Point::new(rect.right(), rect.top());
            let br = Point::new(rect.right(), rect.bottom());
            let bl = Point::new(rect.left(), rect.bottom());
            list.push_line_dashed(tl, tr, dot, gap, spec.color);
            list.push_line_dashed(tr, br, dot, gap, spec.color);
            list.push_line_dashed(br, bl, dot, gap, spec.color);
            list.push_line_dashed(bl, tl, dot, gap, spec.color);
        }
        BorderStyle::Double => {
            // Two strokes at width/3, separated by width/3 gap.
            let stroke_w = (spec.width / 3.0).max(1.0);
            let half_gap = stroke_w;
            // Inner rect (inset by stroke_w + half_gap).
            let inset = stroke_w + half_gap;
            let inner = Rect::new(
                rect.left() + inset,
                rect.top() + inset,
                (rect.width() - 2.0 * inset).max(0.0),
                (rect.height() - 2.0 * inset).max(0.0),
            );
            // Outer stroke (border of `rect` itself).
            list.push_stroke_rect(rect, stroke_w, spec.color);
            // Inner stroke.
            if inner.width() > 0.0 && inner.height() > 0.0 {
                list.push_stroke_rect(inner, stroke_w, spec.color);
            }
        }
    }
}

/// Push stroke commands for each visible side of a [`BorderSpecs`] per-side border.
///
/// When all four sides are identical and visible, a single `StrokeRect` is
/// pushed (fast path).  Otherwise each edge is rendered individually as a line
/// along that side.  Sides with `is_invisible()` are skipped.
///
/// Dashed/dotted/double styles are applied per-edge using the same rules as
/// [`push_border_spec`].
///
/// # Example
///
/// ```rust
/// # use oxiui_core::{geometry::Rect, paint::DrawList, Color};
/// # use oxiui_theme::{BorderSpec, BorderSpecs, BorderStyle};
/// # use oxiui_render_wgpu::theme_bridge::push_border_specs;
/// let mut list = DrawList::new();
/// let rect = Rect::new(0.0, 0.0, 80.0, 40.0);
/// let specs = BorderSpecs::solid(1.0, Color(50, 50, 50, 255));
/// push_border_specs(&mut list, rect, &specs);
/// assert_eq!(list.len(), 1); // uniform → single StrokeRect
/// ```
pub fn push_border_specs(list: &mut DrawList, rect: Rect, specs: &BorderSpecs) {
    if specs.is_invisible() {
        return;
    }
    // Fast path: uniform borders of the same style/color collapse to one command.
    if specs.is_uniform() {
        push_border_spec(list, rect, &specs.top);
        return;
    }
    // Per-edge path: push a line for each visible edge.
    let tl = Point::new(rect.left(), rect.top());
    let tr = Point::new(rect.right(), rect.top());
    let br = Point::new(rect.right(), rect.bottom());
    let bl = Point::new(rect.left(), rect.bottom());

    push_edge_line(list, tl, tr, &specs.top);
    push_edge_line(list, tr, br, &specs.right);
    push_edge_line(list, br, bl, &specs.bottom);
    push_edge_line(list, bl, tl, &specs.left);
}

/// Push a single line for one edge of a per-side border.
fn push_edge_line(list: &mut DrawList, from: Point, to: Point, spec: &BorderSpec) {
    if spec.is_invisible() {
        return;
    }
    match spec.style {
        BorderStyle::None => {}
        BorderStyle::Solid => {
            list.push_line_thick(from, to, spec.width, spec.color);
        }
        BorderStyle::Dashed => {
            let dash = (spec.width * 3.0).max(4.0);
            let gap = (spec.width * 2.0).max(2.0);
            list.push_line_dashed(from, to, dash, gap, spec.color);
        }
        BorderStyle::Dotted => {
            let dot = spec.width;
            let gap = spec.width;
            list.push_line_dashed(from, to, dot, gap, spec.color);
        }
        BorderStyle::Double => {
            // Two parallel lines; approximate with two thin lines in sequence.
            let thin = (spec.width / 3.0).max(1.0);
            list.push_line_thick(from, to, thin, spec.color);
            list.push_line_thick(from, to, thin, spec.color);
        }
    }
}

// ── Gradient stop builders from theme color scale ─────────────────────────────

/// Build a two-stop linear gradient ramp from a palette's `background` (offset
/// 0.0) to `primary` (offset 1.0).
///
/// Use with [`push_theme_gradient`] or [`DrawList::push_gradient_linear`] to
/// render a background→primary gradient derived directly from the active theme.
///
/// # Example
///
/// ```rust
/// # use oxiui_theme::ExtendedPalette;
/// # use oxiui_core::Palette;
/// # use oxiui_core::Color;
/// # use oxiui_render_wgpu::theme_bridge::primary_gradient_stops;
/// let palette = ExtendedPalette::derive(Palette {
///     background: Color(26, 27, 38, 255),
///     surface: Color(36, 40, 59, 255),
///     primary: Color(122, 162, 247, 255),
///     on_primary: Color(26, 27, 38, 255),
///     text: Color(192, 202, 245, 255),
///     muted: Color(86, 95, 137, 255),
/// }, true);
/// let stops = primary_gradient_stops(&palette);
/// assert_eq!(stops.len(), 2);
/// assert_eq!(stops[0].offset, 0.0);
/// assert_eq!(stops[1].offset, 1.0);
/// ```
pub fn primary_gradient_stops(palette: &ExtendedPalette) -> Vec<GradientStop> {
    vec![
        GradientStop::new(0.0, palette.base.background),
        GradientStop::new(1.0, palette.base.primary),
    ]
}

/// Build a two-stop linear gradient ramp from `background` (0.0) to `surface`
/// (1.0).
///
/// Useful for subtle panel/card backgrounds derived from the theme palette.
pub fn surface_gradient_stops(palette: &ExtendedPalette) -> Vec<GradientStop> {
    vec![
        GradientStop::new(0.0, palette.base.background),
        GradientStop::new(1.0, palette.base.surface),
    ]
}

/// Build a three-stop gradient ramp using the extended-palette status colours.
///
/// The stops are spaced evenly: `error` at 0.0, `warning` at 0.5, `success`
/// at 1.0.  This is useful for status-indicator bars (e.g. health bars, build
/// status strips) where the full semantic range should be shown.
pub fn status_gradient_stops(palette: &ExtendedPalette) -> Vec<GradientStop> {
    vec![
        GradientStop::new(0.0, palette.error),
        GradientStop::new(0.5, palette.warning),
        GradientStop::new(1.0, palette.success),
    ]
}

/// Build a two-stop gradient from `surface_variant` (0.0) to `outline` (1.0).
///
/// Useful for separator lines and dividers that follow the theme's neutral tones.
pub fn outline_gradient_stops(palette: &ExtendedPalette) -> Vec<GradientStop> {
    vec![
        GradientStop::new(0.0, palette.surface_variant),
        GradientStop::new(1.0, palette.outline),
    ]
}

// ── push_theme_gradient ───────────────────────────────────────────────────────

/// Direction for a themed gradient.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GradientDirection {
    /// Left-to-right horizontal gradient.
    Horizontal,
    /// Top-to-bottom vertical gradient.
    Vertical,
    /// Diagonal gradient from top-left to bottom-right.
    Diagonal,
}

/// Push a themed linear gradient onto `list` using the provided colour stops.
///
/// The gradient axis is derived from `direction` and the bounding `rect`.
/// This is a convenience wrapper around [`DrawList::push_gradient_linear`] that
/// avoids callers computing start/end points manually.
///
/// # Example
///
/// ```rust
/// # use oxiui_core::{geometry::Rect, paint::DrawList, Palette, Color};
/// # use oxiui_theme::ExtendedPalette;
/// # use oxiui_render_wgpu::theme_bridge::{
/// #     push_theme_gradient, primary_gradient_stops, GradientDirection,
/// # };
/// let mut list = DrawList::new();
/// let rect = Rect::new(0.0, 0.0, 200.0, 60.0);
/// let palette = ExtendedPalette::derive(Palette {
///     background: Color(26, 27, 38, 255),
///     surface: Color(36, 40, 59, 255),
///     primary: Color(122, 162, 247, 255),
///     on_primary: Color(26, 27, 38, 255),
///     text: Color(192, 202, 245, 255),
///     muted: Color(86, 95, 137, 255),
/// }, true);
/// let stops = primary_gradient_stops(&palette);
/// push_theme_gradient(&mut list, rect, &stops, GradientDirection::Horizontal);
/// assert_eq!(list.len(), 1);
/// ```
pub fn push_theme_gradient(
    list: &mut DrawList,
    rect: Rect,
    stops: &[GradientStop],
    direction: GradientDirection,
) {
    if stops.is_empty() {
        return;
    }
    let (start, end) = match direction {
        GradientDirection::Horizontal => (
            Point::new(rect.left(), rect.top()),
            Point::new(rect.right(), rect.top()),
        ),
        GradientDirection::Vertical => (
            Point::new(rect.left(), rect.top()),
            Point::new(rect.left(), rect.bottom()),
        ),
        GradientDirection::Diagonal => (
            Point::new(rect.left(), rect.top()),
            Point::new(rect.right(), rect.bottom()),
        ),
    };
    list.push_gradient_linear(rect, start, end, stops.to_vec());
}

// ── elevation_to_shadow_spec helper ──────────────────────────────────────────

/// Convert a continuous elevation value (dp) into a [`ShadowDesc`] for the
/// given `rect`, using the `oxiui_theme::elevation_to_shadow` mapping.
///
/// Returns `None` for elevation `<= 0` (no shadow at ground level).
pub fn elevation_shadow_desc(rect: Rect, elevation: f32) -> Option<ShadowDesc> {
    if elevation <= 0.0 {
        return None;
    }
    let spec = oxiui_theme::elevation_to_shadow(elevation);
    shadow_spec_to_desc(rect, &spec)
}

/// Convert a discrete elevation level (0–5) into a [`ShadowDesc`] for the
/// given `rect`, using [`oxiui_theme::elevation_shadow`].
///
/// Returns `None` when `level == 0` or the spec is invisible.
pub fn elevation_level_shadow_desc(rect: Rect, level: usize) -> Option<ShadowDesc> {
    let spec = oxiui_theme::elevation_shadow(level)?;
    shadow_spec_to_desc(rect, &spec)
}

/// Push both the ambient and key shadow layers from
/// [`oxiui_theme::elevation_shadows`] onto `list` for the given `rect` and
/// `elevation`.
///
/// When `elevation == 0` no commands are pushed.
pub fn push_elevation_shadows(list: &mut DrawList, rect: Rect, elevation: u32) {
    let stack = oxiui_theme::elevation_shadows(elevation);
    for spec in &stack {
        push_shadow_spec(list, rect, spec);
    }
}

// ── theme_color_to_draw_color helper ─────────────────────────────────────────

/// Blend a theme colour toward a fully transparent black by `alpha_factor` in
/// `[0.0, 1.0]`.
///
/// Useful when a caller wants a translucent tint derived from a palette colour
/// without modifying the palette itself.  `alpha_factor = 1.0` is a no-op.
pub fn tinted_color(color: Color, alpha_factor: f32) -> Color {
    let a = (color.3 as f32 * alpha_factor.clamp(0.0, 1.0)).round() as u8;
    Color(color.0, color.1, color.2, a)
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxiui_core::{geometry::Rect, paint::DrawList, Color, Palette};
    use oxiui_theme::ExtendedPalette;

    fn test_palette() -> ExtendedPalette {
        ExtendedPalette::derive(
            Palette {
                background: Color(26, 27, 38, 255),
                surface: Color(36, 40, 59, 255),
                primary: Color(122, 162, 247, 255),
                on_primary: Color(26, 27, 38, 255),
                text: Color(192, 202, 245, 255),
                muted: Color(86, 95, 137, 255),
            },
            true,
        )
    }

    fn test_rect() -> Rect {
        Rect::new(10.0, 10.0, 100.0, 50.0)
    }

    // ── shadow_spec_to_desc ────────────────────────────────────────────────────

    #[test]
    fn shadow_spec_to_desc_invisible_returns_none() {
        let spec = ShadowSpec::new(0.0, 0.0, 0.0, [0, 0, 0, 0]);
        assert!(shadow_spec_to_desc(test_rect(), &spec).is_none());
    }

    #[test]
    fn shadow_spec_to_desc_visible_returns_some() {
        let spec = ShadowSpec::drop_shadow(2.0, 4.0, 8.0);
        let desc = shadow_spec_to_desc(test_rect(), &spec);
        assert!(desc.is_some());
        let d = desc.unwrap();
        // shadow_rect should be shifted by offset_x/offset_y
        assert!((d.shadow_rect.left() - (test_rect().left() + spec.offset_x)).abs() < 1e-6);
        assert!((d.shadow_rect.top() - (test_rect().top() + spec.offset_y)).abs() < 1e-6);
        assert!((d.blur_radius - spec.blur).abs() < 1e-6);
    }

    #[test]
    fn shadow_spec_to_desc_with_positive_spread_grows_rect() {
        let spec = ShadowSpec::drop_shadow(0.0, 0.0, 4.0).with_spread(5.0);
        let desc = shadow_spec_to_desc(test_rect(), &spec).unwrap();
        // width should be original + 2 * spread
        let expected_w = test_rect().width() + 2.0 * 5.0;
        assert!((desc.shadow_rect.width() - expected_w).abs() < 1e-6);
    }

    #[test]
    fn shadow_spec_to_desc_with_negative_spread_shrinks_rect() {
        let spec = ShadowSpec::drop_shadow(0.0, 0.0, 2.0).with_spread(-3.0);
        let desc = shadow_spec_to_desc(test_rect(), &spec).unwrap();
        // width = original + 2 * (-3) = 100 - 6 = 94
        let expected_w = (test_rect().width() - 6.0).max(0.0);
        assert!((desc.shadow_rect.width() - expected_w).abs() < 1e-6);
    }

    // ── push_shadow_spec ───────────────────────────────────────────────────────

    #[test]
    fn push_shadow_spec_invisible_pushes_nothing() {
        let mut list = DrawList::new();
        let spec = ShadowSpec::new(0.0, 0.0, 0.0, [0, 0, 0, 0]);
        push_shadow_spec(&mut list, test_rect(), &spec);
        assert_eq!(list.len(), 0);
    }

    #[test]
    fn push_shadow_spec_visible_pushes_one_command() {
        let mut list = DrawList::new();
        let spec = ShadowSpec::drop_shadow(1.0, 2.0, 4.0);
        push_shadow_spec(&mut list, test_rect(), &spec);
        assert_eq!(list.len(), 1);
    }

    // ── gradient stops ─────────────────────────────────────────────────────────

    #[test]
    fn primary_gradient_stops_has_two_stops() {
        let p = test_palette();
        let stops = primary_gradient_stops(&p);
        assert_eq!(stops.len(), 2);
        assert_eq!(stops[0].offset, 0.0);
        assert_eq!(stops[1].offset, 1.0);
        assert_eq!(stops[0].color, p.base.background);
        assert_eq!(stops[1].color, p.base.primary);
    }

    #[test]
    fn surface_gradient_stops_has_two_stops() {
        let p = test_palette();
        let stops = surface_gradient_stops(&p);
        assert_eq!(stops.len(), 2);
        assert_eq!(stops[0].color, p.base.background);
        assert_eq!(stops[1].color, p.base.surface);
    }

    #[test]
    fn status_gradient_stops_has_three_stops() {
        let p = test_palette();
        let stops = status_gradient_stops(&p);
        assert_eq!(stops.len(), 3);
        assert_eq!(stops[0].color, p.error);
        assert_eq!(stops[1].color, p.warning);
        assert_eq!(stops[2].color, p.success);
    }

    #[test]
    fn outline_gradient_stops_has_two_stops() {
        let p = test_palette();
        let stops = outline_gradient_stops(&p);
        assert_eq!(stops.len(), 2);
    }

    // ── push_theme_gradient ────────────────────────────────────────────────────

    #[test]
    fn push_theme_gradient_horizontal_pushes_one_command() {
        let mut list = DrawList::new();
        let p = test_palette();
        let stops = primary_gradient_stops(&p);
        push_theme_gradient(
            &mut list,
            test_rect(),
            &stops,
            GradientDirection::Horizontal,
        );
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn push_theme_gradient_vertical_pushes_one_command() {
        let mut list = DrawList::new();
        let p = test_palette();
        let stops = surface_gradient_stops(&p);
        push_theme_gradient(&mut list, test_rect(), &stops, GradientDirection::Vertical);
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn push_theme_gradient_diagonal_pushes_one_command() {
        let mut list = DrawList::new();
        let p = test_palette();
        let stops = status_gradient_stops(&p);
        push_theme_gradient(&mut list, test_rect(), &stops, GradientDirection::Diagonal);
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn push_theme_gradient_empty_stops_pushes_nothing() {
        let mut list = DrawList::new();
        push_theme_gradient(&mut list, test_rect(), &[], GradientDirection::Horizontal);
        assert_eq!(list.len(), 0);
    }

    // ── elevation helpers ──────────────────────────────────────────────────────

    #[test]
    fn elevation_shadow_desc_zero_returns_none() {
        assert!(elevation_shadow_desc(test_rect(), 0.0).is_none());
    }

    #[test]
    fn elevation_shadow_desc_positive_returns_some() {
        let desc = elevation_shadow_desc(test_rect(), 4.0);
        assert!(desc.is_some());
        let d = desc.unwrap();
        assert!(d.blur_radius > 0.0);
    }

    #[test]
    fn elevation_level_shadow_desc_level_zero_returns_none() {
        assert!(elevation_level_shadow_desc(test_rect(), 0).is_none());
    }

    #[test]
    fn elevation_level_shadow_desc_level_three_returns_some() {
        let desc = elevation_level_shadow_desc(test_rect(), 3);
        assert!(desc.is_some());
    }

    #[test]
    fn push_elevation_shadows_zero_pushes_nothing() {
        let mut list = DrawList::new();
        push_elevation_shadows(&mut list, test_rect(), 0);
        assert_eq!(list.len(), 0);
    }

    #[test]
    fn push_elevation_shadows_four_pushes_two_shadows() {
        let mut list = DrawList::new();
        push_elevation_shadows(&mut list, test_rect(), 4);
        // elevation_shadows(4) returns 2 specs (ambient + key), both non-invisible
        assert_eq!(list.len(), 2);
    }

    // ── tinted_color ───────────────────────────────────────────────────────────

    #[test]
    fn tinted_color_full_alpha_is_noop() {
        let c = Color(255, 128, 64, 200);
        let t = tinted_color(c, 1.0);
        assert_eq!(t, c);
    }

    #[test]
    fn tinted_color_zero_alpha_makes_transparent() {
        let c = Color(255, 128, 64, 200);
        let t = tinted_color(c, 0.0);
        assert_eq!(t.3, 0);
    }

    #[test]
    fn tinted_color_half_alpha_halves_alpha_channel() {
        let c = Color(255, 128, 64, 200);
        let t = tinted_color(c, 0.5);
        // 200 * 0.5 = 100
        assert_eq!(t.3, 100);
        assert_eq!(t.0, c.0);
        assert_eq!(t.1, c.1);
        assert_eq!(t.2, c.2);
    }

    // ── push_border_spec ───────────────────────────────────────────────────────

    #[test]
    fn push_border_spec_invisible_pushes_nothing() {
        let mut list = DrawList::new();
        let spec = BorderSpec::none();
        push_border_spec(&mut list, test_rect(), &spec);
        assert_eq!(list.len(), 0);
    }

    #[test]
    fn push_border_spec_solid_pushes_one_command() {
        let mut list = DrawList::new();
        let spec = BorderSpec::solid(2.0, Color(0, 0, 0, 255));
        push_border_spec(&mut list, test_rect(), &spec);
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn push_border_spec_dashed_pushes_four_lines() {
        let mut list = DrawList::new();
        let spec = BorderSpec {
            width: 1.0,
            style: BorderStyle::Dashed,
            color: Color(255, 0, 0, 255),
        };
        push_border_spec(&mut list, test_rect(), &spec);
        // 4 edges → 4 dashed-line commands
        assert_eq!(list.len(), 4);
    }

    #[test]
    fn push_border_spec_dotted_pushes_four_lines() {
        let mut list = DrawList::new();
        let spec = BorderSpec {
            width: 1.0,
            style: BorderStyle::Dotted,
            color: Color(0, 255, 0, 255),
        };
        push_border_spec(&mut list, test_rect(), &spec);
        assert_eq!(list.len(), 4);
    }

    #[test]
    fn push_border_spec_double_pushes_two_strokes() {
        let mut list = DrawList::new();
        let spec = BorderSpec {
            width: 6.0,
            style: BorderStyle::Double,
            color: Color(0, 0, 255, 255),
        };
        push_border_spec(&mut list, test_rect(), &spec);
        // outer + inner stroke = 2 commands
        assert_eq!(list.len(), 2);
    }

    // ── push_border_specs ──────────────────────────────────────────────────────

    #[test]
    fn push_border_specs_invisible_pushes_nothing() {
        let mut list = DrawList::new();
        let specs = BorderSpecs::none();
        push_border_specs(&mut list, test_rect(), &specs);
        assert_eq!(list.len(), 0);
    }

    #[test]
    fn push_border_specs_uniform_pushes_one_command() {
        let mut list = DrawList::new();
        let specs = BorderSpecs::solid(1.0, Color(50, 50, 50, 255));
        push_border_specs(&mut list, test_rect(), &specs);
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn push_border_specs_non_uniform_pushes_per_edge() {
        use oxiui_theme::{BorderSpec, BorderSpecs};
        let mut list = DrawList::new();
        // Top and right solid, bottom and left invisible.
        let specs = BorderSpecs {
            top: BorderSpec::solid(1.0, Color(0, 0, 0, 255)),
            right: BorderSpec::solid(2.0, Color(0, 0, 0, 255)),
            bottom: BorderSpec::none(),
            left: BorderSpec::none(),
        };
        push_border_specs(&mut list, test_rect(), &specs);
        // Two visible edges → 2 line commands.
        assert_eq!(list.len(), 2);
    }
}
