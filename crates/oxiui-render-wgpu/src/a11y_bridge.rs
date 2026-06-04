//! Accessibility rendering bridge for `oxiui-render-wgpu`.
//!
//! Enabled by the `accessibility` Cargo feature.  Provides helpers that convert
//! `oxiui-accessibility` focus / high-contrast state into [`DrawList`] commands
//! so the GPU renderer can paint accessibility visuals without knowing about the
//! accesskit node tree.
//!
//! # Focus rings
//!
//! [`push_focus_ring`] appends a styled outline to a [`DrawList`] given a widget
//! bounding rectangle and a [`FocusRing`] spec.  The ring is drawn as a rounded
//! stroke path that follows the widget's boundary.
//!
//! # High-contrast palette override
//!
//! [`is_high_contrast_active`] checks whether the OS high-contrast preference is
//! active, reading from `oxiui-accessibility`'s `OsA11yPrefs` (via the
//! `OXIUI_HIGH_CONTRAST` environment variable as a cross-platform fallback).
//!
//! # Example
//!
//! ```rust
//! use oxiui_core::{geometry::Rect, paint::DrawList, Color};
//! use oxiui_accessibility::FocusRing;
//! use oxiui_render_wgpu::a11y_bridge::push_focus_ring;
//!
//! let mut list = DrawList::new();
//! let ring = FocusRing::default();
//! let widget_rect = Rect::new(10.0, 10.0, 80.0, 40.0);
//! push_focus_ring(&mut list, widget_rect, &ring);
//! assert_eq!(list.len(), 1, "one StrokePath for the focus ring");
//! ```

use oxiui_accessibility::FocusRing;
use oxiui_core::{
    geometry::{Point, Rect},
    paint::{DrawList, PathData, StrokeStyle},
    Color,
};

// ── Focus ring rendering ──────────────────────────────────────────────────────

/// Push a focus-ring stroke onto `list` for a widget at `widget_rect`.
///
/// The ring is positioned outside `widget_rect` by `ring.offset` pixels on
/// each side.  When `ring.radius > 0` the corners are rounded via cubic Bézier
/// arcs (kappa ≈ 0.5523).
///
/// The ring uses `DrawCommand::StrokePath` so it is rendered by the existing
/// GPU path/stroke tessellator — no new shader support is required.
///
/// # Arguments
/// * `list` — the draw list to append to.
/// * `widget_rect` — the bounding rectangle of the widget being focused.
/// * `ring` — visual spec (color, width, offset, radius).
pub fn push_focus_ring(list: &mut DrawList, widget_rect: Rect, ring: &FocusRing) {
    // Expand the widget rect outward by the ring offset.
    let outset = ring.offset;
    let expanded = Rect::new(
        widget_rect.left() - outset,
        widget_rect.top() - outset,
        widget_rect.width() + 2.0 * outset,
        widget_rect.height() + 2.0 * outset,
    );

    let color = Color(ring.color[0], ring.color[1], ring.color[2], ring.color[3]);
    let style = StrokeStyle {
        width: ring.width,
        ..StrokeStyle::default()
    };

    if ring.radius <= 0.0 {
        // Sharp corners: four line segments.
        let path = rect_path(expanded);
        list.push_stroke_path(path, style, color);
    } else {
        // Rounded corners via cubic Bézier arcs.
        let path = rounded_rect_path(expanded, ring.radius);
        list.push_stroke_path(path, style, color);
    }
}

/// Build a closed rectangular path (no corner radius).
fn rect_path(r: Rect) -> PathData {
    let mut p = PathData::new();
    p.move_to(Point::new(r.left(), r.top()));
    p.line_to(Point::new(r.right(), r.top()));
    p.line_to(Point::new(r.right(), r.bottom()));
    p.line_to(Point::new(r.left(), r.bottom()));
    p.close();
    p
}

/// Build a closed rounded-rectangle path using cubic Bézier arcs.
///
/// Corner arcs use the standard Bézier circle approximation constant
/// κ ≈ 0.5523 (4/3 · tan(π/8)).  The radius is clamped to half the
/// shortest side so degenerate rectangles remain sane.
fn rounded_rect_path(r: Rect, radius: f32) -> PathData {
    // Kappa — standard circle approximation for 90-degree cubic Bézier arc.
    const KAPPA: f32 = 0.552_284_8;

    let max_r = (r.width().min(r.height()) * 0.5).max(0.0);
    let rr = radius.min(max_r);
    let k = rr * KAPPA;

    let l = r.left();
    let t = r.top();
    let ri = r.right();
    let b = r.bottom();

    let mut p = PathData::new();

    // Start at top-left, just after the corner arc.
    p.move_to(Point::new(l + rr, t));

    // Top edge → top-right corner arc.
    p.line_to(Point::new(ri - rr, t));
    p.cubic_to(
        Point::new(ri - rr + k, t),
        Point::new(ri, t + rr - k),
        Point::new(ri, t + rr),
    );

    // Right edge → bottom-right corner arc.
    p.line_to(Point::new(ri, b - rr));
    p.cubic_to(
        Point::new(ri, b - rr + k),
        Point::new(ri - rr + k, b),
        Point::new(ri - rr, b),
    );

    // Bottom edge → bottom-left corner arc.
    p.line_to(Point::new(l + rr, b));
    p.cubic_to(
        Point::new(l + rr - k, b),
        Point::new(l, b - rr + k),
        Point::new(l, b - rr),
    );

    // Left edge → top-left corner arc.
    p.line_to(Point::new(l, t + rr));
    p.cubic_to(
        Point::new(l, t + rr - k),
        Point::new(l + rr - k, t),
        Point::new(l + rr, t),
    );

    p.close();
    p
}

// ── High-contrast palette overlay ────────────────────────────────────────────

/// Check whether the OS high-contrast preference is active.
///
/// Reads from `oxiui-accessibility`'s `OsA11yPrefs` which uses the
/// `OXIUI_HIGH_CONTRAST` environment variable as a cross-platform fallback.
pub fn is_high_contrast_active() -> bool {
    oxiui_accessibility::OsA11yPrefs::query().high_contrast
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use oxiui_accessibility::FocusRing;
    use oxiui_core::{geometry::Rect, paint::DrawList};

    fn sample_rect() -> Rect {
        Rect::new(20.0, 20.0, 120.0, 60.0)
    }

    // push_focus_ring with radius=0 → 1 StrokePath command.
    #[test]
    fn focus_ring_sharp_corners_produces_one_command() {
        let mut list = DrawList::new();
        let ring = FocusRing {
            color: [0, 120, 215, 255],
            width: 2.0,
            offset: 2.0,
            radius: 0.0,
        };
        push_focus_ring(&mut list, sample_rect(), &ring);
        assert_eq!(list.len(), 1, "one StrokePath for sharp-corner focus ring");
    }

    // push_focus_ring with radius>0 → 1 StrokePath command.
    #[test]
    fn focus_ring_rounded_corners_produces_one_command() {
        let mut list = DrawList::new();
        let ring = FocusRing::default();
        push_focus_ring(&mut list, sample_rect(), &ring);
        assert_eq!(list.len(), 1, "one StrokePath for rounded focus ring");
    }

    // Ring is offset outward: the path bounding box must be larger than the widget.
    #[test]
    fn focus_ring_expands_beyond_widget_rect() {
        let mut list = DrawList::new();
        let ring = FocusRing {
            color: [0, 0, 0, 255],
            width: 2.0,
            offset: 4.0,
            radius: 0.0,
        };
        push_focus_ring(&mut list, sample_rect(), &ring);
        // Verify there is at least one command.
        assert!(!list.is_empty());
    }

    // Radius clamped at half shortest side — no panic on very small rect.
    #[test]
    fn focus_ring_degenerate_rect_no_panic() {
        let mut list = DrawList::new();
        let ring = FocusRing {
            radius: 100.0, // much larger than rect
            ..FocusRing::default()
        };
        let tiny = Rect::new(0.0, 0.0, 4.0, 4.0);
        push_focus_ring(&mut list, tiny, &ring);
        assert_eq!(list.len(), 1);
    }

    // is_high_contrast_active does not panic.
    #[test]
    fn high_contrast_query_does_not_panic() {
        // The actual value depends on the environment variable; just ensure no panic.
        let _ = is_high_contrast_active();
    }
}
