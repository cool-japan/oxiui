//! Draw-command buffer and render-backend abstraction for OxiUI.
//!
//! This module defines the canonical *paint* layer:
//!
//! - [`DrawCommand`] — a single GPU/CPU-agnostic draw operation.
//! - [`DrawList`] — an ordered buffer of draw commands with clip-stack
//!   tracking and accumulated bounds.
//! - [`RenderBackend`] — a trait that backends implement to consume a
//!   [`DrawList`].
//! - Supporting types: [`PathData`], [`PathVerb`], [`StrokeStyle`],
//!   [`GradientStop`], [`ImageData`], [`ImageFilter`], [`FillRule`],
//!   [`LineJoin`], [`LineCap`].

use crate::geometry::{Point, Rect, Size};
use crate::UiError;
use crate::{Color, FontSpec};

// ── Enums: fill rule, join, cap ─────────────────────────────────────────────

/// The rule used to determine which parts of a self-intersecting path are
/// considered "inside" for filling purposes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum FillRule {
    /// The even-odd rule alternates inside/outside on each crossing.
    EvenOdd,
    /// The non-zero winding-number rule (the default for most renderers).
    #[default]
    NonZero,
}

/// The style used to join two path segments at a corner.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum LineJoin {
    /// A sharp miter join (clipped at [`StrokeStyle::miter_limit`]).
    #[default]
    Miter,
    /// A flat bevel cut across the outside corner.
    Bevel,
    /// A circular arc centered at the corner point.
    Round,
}

/// The style applied to the start and end caps of an open path segment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum LineCap {
    /// No cap — the stroke ends exactly at the path endpoint.
    #[default]
    Butt,
    /// A semicircular cap extending half the stroke width beyond the endpoint.
    Round,
    /// A rectangular cap extending half the stroke width beyond the endpoint.
    Square,
}

// ── StrokeStyle ─────────────────────────────────────────────────────────────

/// Parameters controlling how a path's outline is stroked.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StrokeStyle {
    /// Stroke width in logical pixels.
    pub width: f32,
    /// Corner join style.
    pub join: LineJoin,
    /// End-cap style for open sub-paths.
    pub cap: LineCap,
    /// Maximum ratio of miter length to stroke width before the join is
    /// clipped to a bevel.
    pub miter_limit: f32,
}

impl Default for StrokeStyle {
    fn default() -> Self {
        Self {
            width: 1.0,
            join: LineJoin::Miter,
            cap: LineCap::Butt,
            miter_limit: 4.0,
        }
    }
}

// ── GradientStop ────────────────────────────────────────────────────────────

/// A single colour stop in a gradient ramp.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientStop {
    /// Position within the gradient, clamped to `[0.0, 1.0]`.
    pub offset: f32,
    /// The colour at this stop.
    pub color: Color,
}

impl GradientStop {
    /// Construct a gradient stop, clamping `offset` to `[0.0, 1.0]`.
    pub fn new(offset: f32, color: Color) -> Self {
        Self {
            offset: offset.clamp(0.0, 1.0),
            color,
        }
    }
}

// ── ImageFilter ─────────────────────────────────────────────────────────────

/// The sampling filter applied when scaling an image.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum ImageFilter {
    /// Nearest-neighbour sampling (blocky, no blending).
    #[default]
    Nearest,
    /// Bilinear interpolation (smoother, slightly blurred).
    Bilinear,
}

// ── ImageData ───────────────────────────────────────────────────────────────

/// Owned raw RGBA image data.
#[derive(Clone, Debug, PartialEq)]
pub struct ImageData {
    /// Raw pixel data in row-major RGBA order (`width * height * 4` bytes).
    pub rgba: Vec<u8>,
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
}

impl ImageData {
    /// Construct an [`ImageData`] from a raw RGBA byte vector and dimensions.
    pub fn new(rgba: Vec<u8>, width: u32, height: u32) -> Self {
        Self {
            rgba,
            width,
            height,
        }
    }
}

// ── PathVerb / PathData ─────────────────────────────────────────────────────

/// A single drawing verb in a [`PathData`] sequence.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathVerb {
    /// Begin a new sub-path at the given point.
    MoveTo(Point),
    /// Draw a straight line to the given point.
    LineTo(Point),
    /// Draw a quadratic Bézier curve with one control point.
    QuadTo {
        /// The single control point.
        ctrl: Point,
        /// The end point of the curve.
        end: Point,
    },
    /// Draw a cubic Bézier curve with two control points.
    CubicTo {
        /// The first control point.
        c1: Point,
        /// The second control point.
        c2: Point,
        /// The end point of the curve.
        end: Point,
    },
    /// Close the current sub-path by drawing a line back to the last `MoveTo`.
    Close,
}

/// A resolution-independent path built from [`PathVerb`] segments.
///
/// Paths are the primitive used for arbitrary filled and stroked shapes.
/// Build a path with the chaining builder methods, then pass it to
/// [`DrawList::push_path`] or [`DrawList::push_stroke_path`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PathData {
    /// The ordered sequence of drawing verbs that define this path.
    pub verbs: Vec<PathVerb>,
    /// The fill rule used when rasterising filled versions of this path.
    pub fill_rule: FillRule,
}

impl PathData {
    /// Construct an empty path with `FillRule::NonZero`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the [`FillRule`] on this path (builder-style).
    pub fn with_fill_rule(mut self, rule: FillRule) -> Self {
        self.fill_rule = rule;
        self
    }

    /// Append a `MoveTo` verb.
    pub fn move_to(&mut self, p: Point) -> &mut Self {
        self.verbs.push(PathVerb::MoveTo(p));
        self
    }

    /// Append a `LineTo` verb.
    pub fn line_to(&mut self, p: Point) -> &mut Self {
        self.verbs.push(PathVerb::LineTo(p));
        self
    }

    /// Append a quadratic Bézier `QuadTo` verb.
    pub fn quad_to(&mut self, ctrl: Point, end: Point) -> &mut Self {
        self.verbs.push(PathVerb::QuadTo { ctrl, end });
        self
    }

    /// Append a cubic Bézier `CubicTo` verb.
    pub fn cubic_to(&mut self, c1: Point, c2: Point, end: Point) -> &mut Self {
        self.verbs.push(PathVerb::CubicTo { c1, c2, end });
        self
    }

    /// Append a `Close` verb, closing the current sub-path.
    pub fn close(&mut self) -> &mut Self {
        self.verbs.push(PathVerb::Close);
        self
    }

    /// Returns `true` if this path contains no verbs.
    pub fn is_empty(&self) -> bool {
        self.verbs.is_empty()
    }

    /// Conservative axis-aligned bounding box over all control and anchor
    /// points in the path.
    ///
    /// Returns `None` if the path is empty.  Note this is a *control-point*
    /// AABB, not a tight geometric bounds: Bézier curves can dip outside the
    /// control-point hull.
    pub fn bounds(&self) -> Option<Rect> {
        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;
        let mut found = false;

        let mut update = |p: Point| {
            found = true;
            if p.x < min_x {
                min_x = p.x;
            }
            if p.y < min_y {
                min_y = p.y;
            }
            if p.x > max_x {
                max_x = p.x;
            }
            if p.y > max_y {
                max_y = p.y;
            }
        };

        for verb in &self.verbs {
            match verb {
                PathVerb::MoveTo(p) | PathVerb::LineTo(p) => update(*p),
                PathVerb::QuadTo { ctrl, end } => {
                    update(*ctrl);
                    update(*end);
                }
                PathVerb::CubicTo { c1, c2, end } => {
                    update(*c1);
                    update(*c2);
                    update(*end);
                }
                PathVerb::Close => {}
            }
        }

        if found {
            Some(Rect::new(min_x, min_y, max_x - min_x, max_y - min_y))
        } else {
            None
        }
    }
}

// ── DrawCommand ─────────────────────────────────────────────────────────────

/// A single, backend-agnostic draw operation.
///
/// Commands are stored in a [`DrawList`] and later replayed by a
/// [`RenderBackend`]. The enum is `#[non_exhaustive]` so that new variants
/// can be added without breaking downstream code.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum DrawCommand {
    // ── Clipping ──────────────────────────────────────────────────────────
    /// Push a rectangular clip region onto the clip stack.
    ///
    /// All subsequent commands are clipped to the intersection of active clip
    /// rectangles until the matching [`DrawCommand::PopClip`].
    PushClip {
        /// The clip rectangle in logical pixels.
        rect: Rect,
    },

    /// Pop the most recently pushed clip rectangle from the clip stack.
    PopClip,

    // ── Rectangles ────────────────────────────────────────────────────────
    /// Fill an axis-aligned rectangle with a solid colour.
    FillRect {
        /// The rectangle to fill.
        rect: Rect,
        /// Fill colour.
        color: Color,
    },

    /// Stroke the outline of an axis-aligned rectangle.
    StrokeRect {
        /// The rectangle to stroke.
        rect: Rect,
        /// Stroke width in logical pixels.
        thickness: f32,
        /// Stroke colour.
        color: Color,
    },

    /// Fill a rectangle with uniformly rounded corners.
    FillRoundedRect {
        /// The rectangle to fill.
        rect: Rect,
        /// Corner radius in logical pixels (applied to all four corners).
        radius: f32,
        /// Fill colour.
        color: Color,
    },

    /// Fill a rectangle with per-corner radii.
    ///
    /// `radii` is `[top-left, top-right, bottom-right, bottom-left]`.
    FillRoundedRectPerCorner {
        /// The rectangle to fill.
        rect: Rect,
        /// Per-corner radii `[tl, tr, br, bl]` in logical pixels.
        radii: [f32; 4],
        /// Fill colour.
        color: Color,
    },

    // ── Circles / Ellipses ────────────────────────────────────────────────
    /// Fill a circle with a solid colour.
    FillCircle {
        /// Centre point of the circle.
        center: Point,
        /// Radius in logical pixels.
        radius: f32,
        /// Fill colour.
        color: Color,
    },

    /// Fill an ellipse with a solid colour.
    FillEllipse {
        /// Centre point of the ellipse.
        center: Point,
        /// Horizontal (X-axis) radius in logical pixels.
        rx: f32,
        /// Vertical (Y-axis) radius in logical pixels.
        ry: f32,
        /// Fill colour.
        color: Color,
    },

    // ── Lines ─────────────────────────────────────────────────────────────
    /// Draw a 1-pixel aliased line segment.
    Line {
        /// Start point of the line.
        from: Point,
        /// End point of the line.
        to: Point,
        /// Line colour.
        color: Color,
    },

    /// Draw a 1-pixel anti-aliased line segment.
    LineAa {
        /// Start point of the line.
        from: Point,
        /// End point of the line.
        to: Point,
        /// Line colour.
        color: Color,
    },

    /// Draw a thick, filled line segment.
    LineThick {
        /// Start point of the line.
        from: Point,
        /// End point of the line.
        to: Point,
        /// Width of the line in logical pixels.
        width: f32,
        /// Line colour.
        color: Color,
    },

    /// Draw a dashed line segment.
    LineDashed {
        /// Start point of the line.
        from: Point,
        /// End point of the line.
        to: Point,
        /// Length of each dash in logical pixels.
        dash_len: f32,
        /// Length of each gap in logical pixels.
        gap_len: f32,
        /// Line colour.
        color: Color,
    },

    // ── Paths ─────────────────────────────────────────────────────────────
    /// Fill a path with a solid colour.
    FillPath {
        /// The path to fill.
        path: PathData,
        /// Fill colour.
        color: Color,
    },

    /// Stroke a path with a solid colour and style.
    StrokePath {
        /// The path to stroke.
        path: PathData,
        /// Stroke parameters (width, join, cap, miter limit).
        style: StrokeStyle,
        /// Stroke colour.
        color: Color,
    },

    // ── Gradients ─────────────────────────────────────────────────────────
    /// Fill a rectangular region with a linear gradient.
    LinearGradient {
        /// The destination rectangle (defines the fill area).
        rect: Rect,
        /// Start point of the gradient axis.
        start: Point,
        /// End point of the gradient axis.
        end: Point,
        /// Colour stops defining the ramp.
        stops: Vec<GradientStop>,
    },

    /// Fill a rectangular region with a radial gradient.
    RadialGradient {
        /// The destination rectangle (defines the fill area).
        rect: Rect,
        /// Centre of the radial gradient.
        center: Point,
        /// Outer radius of the gradient in logical pixels.
        radius: f32,
        /// Colour stops defining the ramp.
        stops: Vec<GradientStop>,
    },

    // ── Images ────────────────────────────────────────────────────────────
    /// Blit a raw RGBA image into a destination rectangle.
    Image {
        /// The source image data.
        image: ImageData,
        /// Destination rectangle in logical pixels.
        dest: Rect,
        /// Resampling filter to use when scaling.
        filter: ImageFilter,
    },

    /// Draw an image using 9-slice scaling.
    ///
    /// `insets` is `[top, right, bottom, left]` in pixels of the source image.
    NineSlice {
        /// The source image data.
        image: ImageData,
        /// Destination rectangle in logical pixels.
        dest: Rect,
        /// 9-slice insets `[top, right, bottom, left]` in source pixels.
        insets: [u32; 4],
    },

    // ── Shadows ───────────────────────────────────────────────────────────
    /// Draw a box shadow behind a rectangle.
    BoxShadow {
        /// The rectangle casting the shadow.
        rect: Rect,
        /// Shadow offset relative to `rect`.
        offset: Point,
        /// Blur radius in logical pixels (0 = hard edge).
        blur_radius: f32,
        /// Shadow colour (typically semi-transparent).
        color: Color,
    },

    // ── Text ──────────────────────────────────────────────────────────────
    /// Draw text into a rectangle.
    ///
    /// Full shaping is delegated to the backend; this command is a v1
    /// placeholder. Backends that do not support text return `Err`.
    DrawText {
        /// Bounding rectangle for the text.
        rect: Rect,
        /// The string to render.
        text: String,
        /// Font specification (family, size, weight, style).
        font: FontSpec,
        /// Text colour.
        color: Color,
    },
}

// ── DrawList ─────────────────────────────────────────────────────────────────

/// An ordered buffer of [`DrawCommand`]s, with integrated clip-stack and bounds
/// tracking.
///
/// Build a list with the typed `push_*` helpers (or the low-level [`push`])
/// then pass it to a [`RenderBackend::execute`] call.
///
/// [`push`]: DrawList::push
#[derive(Clone, Debug, Default)]
pub struct DrawList {
    cmds: Vec<DrawCommand>,
    clip_depth: usize,
    bounds: Option<Rect>,
}

impl DrawList {
    /// Construct an empty [`DrawList`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Append an arbitrary [`DrawCommand`], automatically updating clip depth
    /// and accumulated bounds.
    pub fn push(&mut self, cmd: DrawCommand) {
        match &cmd {
            DrawCommand::PushClip { .. } => {
                self.clip_depth = self.clip_depth.saturating_add(1);
            }
            DrawCommand::PopClip => {
                self.clip_depth = self.clip_depth.saturating_sub(1);
            }
            _ => {
                if let Some(b) = Self::cmd_bounds(&cmd) {
                    self.bounds = Some(match self.bounds {
                        None => b,
                        Some(existing) => existing.union(&b),
                    });
                }
            }
        }
        self.cmds.push(cmd);
    }

    /// Return the number of commands in the list.
    pub fn len(&self) -> usize {
        self.cmds.len()
    }

    /// Return `true` if the list contains no commands.
    pub fn is_empty(&self) -> bool {
        self.cmds.is_empty()
    }

    /// Iterate over all commands in submission order.
    pub fn iter(&self) -> std::slice::Iter<'_, DrawCommand> {
        self.cmds.iter()
    }

    /// Remove all commands and reset clip depth and bounds.
    pub fn clear(&mut self) {
        self.cmds.clear();
        self.clip_depth = 0;
        self.bounds = None;
    }

    /// Return the accumulated axis-aligned bounding box of all non-clip draw
    /// commands, or `None` if no draw commands have been pushed.
    pub fn bounds(&self) -> Option<Rect> {
        self.bounds
    }

    /// Return the current clip-stack depth.  Zero means balanced.
    pub fn clip_depth(&self) -> usize {
        self.clip_depth
    }

    /// Return `true` if the clip stack is balanced (depth == 0).
    pub fn is_clip_balanced(&self) -> bool {
        self.clip_depth == 0
    }

    // ── Typed push helpers ───────────────────────────────────────────────

    /// Push a solid-filled rectangle.
    pub fn push_rect(&mut self, rect: Rect, color: Color) {
        self.push(DrawCommand::FillRect { rect, color });
    }

    /// Push a stroked rectangle outline.
    pub fn push_stroke_rect(&mut self, rect: Rect, thickness: f32, color: Color) {
        self.push(DrawCommand::StrokeRect {
            rect,
            thickness,
            color,
        });
    }

    /// Push a filled rectangle with uniform corner radius.
    pub fn push_rounded_rect(&mut self, rect: Rect, radius: f32, color: Color) {
        self.push(DrawCommand::FillRoundedRect {
            rect,
            radius,
            color,
        });
    }

    /// Push a filled rectangle with per-corner radii `[tl, tr, br, bl]`.
    pub fn push_rounded_rect_per_corner(&mut self, rect: Rect, radii: [f32; 4], color: Color) {
        self.push(DrawCommand::FillRoundedRectPerCorner { rect, radii, color });
    }

    /// Push a filled circle.
    pub fn push_circle(&mut self, center: Point, radius: f32, color: Color) {
        self.push(DrawCommand::FillCircle {
            center,
            radius,
            color,
        });
    }

    /// Push a filled ellipse.
    pub fn push_ellipse(&mut self, center: Point, rx: f32, ry: f32, color: Color) {
        self.push(DrawCommand::FillEllipse {
            center,
            rx,
            ry,
            color,
        });
    }

    /// Push a 1-pixel aliased line segment.
    pub fn push_line(&mut self, from: Point, to: Point, color: Color) {
        self.push(DrawCommand::Line { from, to, color });
    }

    /// Push a 1-pixel anti-aliased line segment.
    pub fn push_line_aa(&mut self, from: Point, to: Point, color: Color) {
        self.push(DrawCommand::LineAa { from, to, color });
    }

    /// Push a thick, filled line segment.
    pub fn push_line_thick(&mut self, from: Point, to: Point, width: f32, color: Color) {
        self.push(DrawCommand::LineThick {
            from,
            to,
            width,
            color,
        });
    }

    /// Push a dashed line segment.
    pub fn push_line_dashed(
        &mut self,
        from: Point,
        to: Point,
        dash_len: f32,
        gap_len: f32,
        color: Color,
    ) {
        self.push(DrawCommand::LineDashed {
            from,
            to,
            dash_len,
            gap_len,
            color,
        });
    }

    /// Push a clip rectangle onto the clip stack.
    pub fn push_clip(&mut self, rect: Rect) {
        self.push(DrawCommand::PushClip { rect });
    }

    /// Pop the top clip rectangle from the clip stack.
    pub fn pop_clip(&mut self) {
        self.push(DrawCommand::PopClip);
    }

    /// Push a solid-filled path.
    pub fn push_path(&mut self, path: PathData, color: Color) {
        self.push(DrawCommand::FillPath { path, color });
    }

    /// Push a stroked path.
    pub fn push_stroke_path(&mut self, path: PathData, style: StrokeStyle, color: Color) {
        self.push(DrawCommand::StrokePath { path, style, color });
    }

    /// Push a linear gradient fill over `rect`.
    pub fn push_gradient_linear(
        &mut self,
        rect: Rect,
        start: Point,
        end: Point,
        stops: Vec<GradientStop>,
    ) {
        self.push(DrawCommand::LinearGradient {
            rect,
            start,
            end,
            stops,
        });
    }

    /// Push a radial gradient fill over `rect`.
    pub fn push_gradient_radial(
        &mut self,
        rect: Rect,
        center: Point,
        radius: f32,
        stops: Vec<GradientStop>,
    ) {
        self.push(DrawCommand::RadialGradient {
            rect,
            center,
            radius,
            stops,
        });
    }

    /// Push a scaled image blit.
    pub fn push_image(&mut self, image: ImageData, dest: Rect, filter: ImageFilter) {
        self.push(DrawCommand::Image {
            image,
            dest,
            filter,
        });
    }

    /// Push a 9-slice scaled image.
    pub fn push_nine_slice(&mut self, image: ImageData, dest: Rect, insets: [u32; 4]) {
        self.push(DrawCommand::NineSlice {
            image,
            dest,
            insets,
        });
    }

    /// Push a box shadow.
    pub fn push_shadow(&mut self, rect: Rect, offset: Point, blur_radius: f32, color: Color) {
        self.push(DrawCommand::BoxShadow {
            rect,
            offset,
            blur_radius,
            color,
        });
    }

    /// Push a text draw command.
    pub fn push_text(&mut self, rect: Rect, text: impl Into<String>, font: FontSpec, color: Color) {
        self.push(DrawCommand::DrawText {
            rect,
            text: text.into(),
            font,
            color,
        });
    }

    // ── Private helpers ──────────────────────────────────────────────────

    /// Compute a conservative bounding rect for `cmd`, or `None` for
    /// clip-stack commands (which don't occupy draw-space geometry).
    fn cmd_bounds(cmd: &DrawCommand) -> Option<Rect> {
        match cmd {
            DrawCommand::FillRect { rect, .. }
            | DrawCommand::StrokeRect { rect, .. }
            | DrawCommand::FillRoundedRect { rect, .. }
            | DrawCommand::FillRoundedRectPerCorner { rect, .. }
            | DrawCommand::LinearGradient { rect, .. }
            | DrawCommand::RadialGradient { rect, .. }
            | DrawCommand::Image { dest: rect, .. }
            | DrawCommand::NineSlice { dest: rect, .. }
            | DrawCommand::DrawText { rect, .. } => Some(*rect),

            DrawCommand::BoxShadow {
                rect,
                offset,
                blur_radius,
                ..
            } => {
                let pad = *blur_radius;
                Some(Rect::new(
                    rect.left() + offset.x - pad,
                    rect.top() + offset.y - pad,
                    rect.width() + 2.0 * pad,
                    rect.height() + 2.0 * pad,
                ))
            }

            DrawCommand::FillCircle { center, radius, .. } => Some(Rect::new(
                center.x - radius,
                center.y - radius,
                radius * 2.0,
                radius * 2.0,
            )),

            DrawCommand::FillEllipse { center, rx, ry, .. } => {
                Some(Rect::new(center.x - rx, center.y - ry, rx * 2.0, ry * 2.0))
            }

            DrawCommand::Line { from, to, .. } | DrawCommand::LineAa { from, to, .. } => {
                let x = from.x.min(to.x);
                let y = from.y.min(to.y);
                Some(Rect::new(
                    x,
                    y,
                    (from.x - to.x).abs(),
                    (from.y - to.y).abs(),
                ))
            }

            DrawCommand::LineThick {
                from, to, width, ..
            } => {
                let pad = width / 2.0;
                let x = from.x.min(to.x) - pad;
                let y = from.y.min(to.y) - pad;
                let w = (from.x - to.x).abs() + *width;
                let h = (from.y - to.y).abs() + *width;
                Some(Rect::new(x, y, w, h))
            }

            DrawCommand::LineDashed { from, to, .. } => {
                let x = from.x.min(to.x);
                let y = from.y.min(to.y);
                Some(Rect::new(
                    x,
                    y,
                    (from.x - to.x).abs(),
                    (from.y - to.y).abs(),
                ))
            }

            DrawCommand::FillPath { path, .. } => path.bounds(),

            DrawCommand::StrokePath { path, style, .. } => path.bounds().map(|b| {
                let pad = style.width / 2.0;
                Rect::new(
                    b.left() - pad,
                    b.top() - pad,
                    b.width() + style.width,
                    b.height() + style.width,
                )
            }),

            DrawCommand::PushClip { .. } | DrawCommand::PopClip => None,
        }
    }
}

// ── RenderBackend ─────────────────────────────────────────────────────────────

/// A surface that can consume and render a [`DrawList`].
///
/// Implementors are the concrete rendering backends — software rasteriser,
/// GPU pipeline, SVG emitter, etc.  The trait is intentionally minimal: a
/// backend need only implement [`execute`] and [`RenderBackend::surface_size`].  The
/// `supports_*` probes default to `false`; override them to advertise real
/// capabilities so callers can avoid emitting unsupported commands.
///
/// [`execute`]: RenderBackend::execute
pub trait RenderBackend {
    /// Replay an entire [`DrawList`] onto the backend's surface.
    ///
    /// The whole list is submitted in one call (rather than command-by-command)
    /// so the backend can guarantee clip-stack continuity across the sequence.
    fn execute(&mut self, list: &DrawList) -> Result<(), UiError>;

    /// Return the target surface dimensions in *physical* pixels.
    fn surface_size(&self) -> Size;

    /// Return `true` if this backend can render blur effects (e.g. box shadows).
    fn supports_blur(&self) -> bool {
        false
    }

    /// Return `true` if this backend can render gradient fills.
    fn supports_gradients(&self) -> bool {
        false
    }

    /// Return `true` if this backend can render arbitrary vector paths.
    fn supports_paths(&self) -> bool {
        false
    }

    /// Return `true` if this backend can blit [`ImageData`].
    fn supports_images(&self) -> bool {
        false
    }

    /// Return `true` if this backend can render text via [`DrawCommand::DrawText`].
    fn supports_text(&self) -> bool {
        false
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Point, Rect};
    use crate::Color;

    fn red() -> Color {
        Color(255, 0, 0, 255)
    }
    fn blue() -> Color {
        Color(0, 0, 255, 255)
    }

    #[test]
    fn draw_list_builder_records_command_sequence() {
        let mut dl = DrawList::new();
        dl.push_rect(Rect::new(0.0, 0.0, 10.0, 10.0), red());
        dl.push_clip(Rect::new(0.0, 0.0, 5.0, 5.0));
        dl.push_rect(Rect::new(1.0, 1.0, 3.0, 3.0), blue());
        dl.pop_clip();
        assert_eq!(dl.len(), 4);
        // verify order: FillRect, PushClip, FillRect, PopClip
        let cmds: Vec<_> = dl.iter().collect();
        assert!(matches!(cmds[0], DrawCommand::FillRect { .. }));
        assert!(matches!(cmds[1], DrawCommand::PushClip { .. }));
        assert!(matches!(cmds[2], DrawCommand::FillRect { .. }));
        assert!(matches!(cmds[3], DrawCommand::PopClip));
    }

    #[test]
    fn draw_list_len_and_is_empty() {
        let mut dl = DrawList::new();
        assert!(dl.is_empty());
        assert_eq!(dl.len(), 0);
        dl.push_rect(Rect::new(0.0, 0.0, 1.0, 1.0), red());
        assert!(!dl.is_empty());
        assert_eq!(dl.len(), 1);
    }

    #[test]
    fn clip_push_pop_balance() {
        let mut dl = DrawList::new();
        assert!(dl.is_clip_balanced());
        dl.push_clip(Rect::new(0.0, 0.0, 10.0, 10.0));
        assert_eq!(dl.clip_depth(), 1);
        assert!(!dl.is_clip_balanced());
        dl.pop_clip();
        assert_eq!(dl.clip_depth(), 0);
        assert!(dl.is_clip_balanced());
        // Extra pop saturates to 0 — no panic, no underflow
        dl.pop_clip();
        assert_eq!(dl.clip_depth(), 0);
    }

    #[test]
    fn bounds_union_of_draw_commands() {
        let mut dl = DrawList::new();
        dl.push_rect(Rect::new(0.0, 0.0, 10.0, 10.0), red());
        dl.push_rect(Rect::new(20.0, 20.0, 5.0, 5.0), blue());
        let b = dl.bounds().expect("bounds should be Some");
        // union of [0,0,10,10] and [20,20,5,5] = [0,0,25,25]
        assert!((b.left() - 0.0).abs() < 0.001);
        assert!((b.top() - 0.0).abs() < 0.001);
        assert!((b.width() - 25.0).abs() < 0.001);
        assert!((b.height() - 25.0).abs() < 0.001);
    }

    #[test]
    fn bounds_excludes_clip_commands() {
        let mut dl = DrawList::new();
        dl.push_clip(Rect::new(0.0, 0.0, 100.0, 100.0));
        dl.pop_clip();
        assert!(
            dl.bounds().is_none(),
            "clip commands must not contribute to bounds"
        );
    }

    #[test]
    fn clear_resets_bounds_and_depth() {
        let mut dl = DrawList::new();
        dl.push_clip(Rect::new(0.0, 0.0, 10.0, 10.0));
        dl.push_rect(Rect::new(0.0, 0.0, 10.0, 10.0), red());
        dl.clear();
        assert!(dl.is_empty());
        assert!(dl.bounds().is_none());
        assert_eq!(dl.clip_depth(), 0);
    }

    #[test]
    fn path_data_builder_and_bounds() {
        let mut p = PathData::new();
        p.move_to(Point::new(0.0, 0.0));
        p.line_to(Point::new(10.0, 0.0));
        p.line_to(Point::new(5.0, 8.0));
        p.close();
        let b = p.bounds().expect("triangle has bounds");
        assert!((b.left() - 0.0).abs() < 0.001);
        assert!((b.top() - 0.0).abs() < 0.001);
        assert!((b.width() - 10.0).abs() < 0.001);
        assert!((b.height() - 8.0).abs() < 0.001);
        assert_eq!(p.fill_rule, FillRule::NonZero);
        let p2 = PathData::new().with_fill_rule(FillRule::EvenOdd);
        assert_eq!(p2.fill_rule, FillRule::EvenOdd);
    }

    #[test]
    fn empty_list_iter_is_empty() {
        let dl = DrawList::new();
        assert!(dl.iter().next().is_none());
    }

    #[test]
    fn gradient_stop_clamps_offset() {
        let s = GradientStop::new(-0.5, red());
        assert!((s.offset - 0.0).abs() < 0.001);
        let s2 = GradientStop::new(1.5, blue());
        assert!((s2.offset - 1.0).abs() < 0.001);
    }

    #[test]
    fn stroke_style_defaults() {
        let s = StrokeStyle::default();
        assert!((s.width - 1.0).abs() < 0.001);
        assert!(matches!(s.join, LineJoin::Miter));
        assert!(matches!(s.cap, LineCap::Butt));
    }
}
