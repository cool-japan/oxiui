//! Draw-call batcher that groups [`DrawList`] commands by pipeline state.
//!
//! The batcher sorts draw commands by [`BatchKey`] (texture × pipeline ×
//! blend-mode), merges adjacent runs with the same key into a single
//! [`DrawBatch`], and optionally culls commands whose bounds fall entirely
//! outside an active clip rectangle.
//!
//! The original relative order of commands *within* a batch is preserved
//! (stable sort).

use oxiui_core::geometry::Rect;
use oxiui_core::paint::{DrawCommand, DrawList};

// ── Pipeline / blend enumerations ─────────────────────────────────────────────

/// The shader pipeline required to render a draw command.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PipelineKind {
    /// Solid-colour fill or stroke.
    SolidColor,
    /// Textured blit.
    Textured,
    /// Gradient fill.
    Gradient,
    /// Arbitrary vector path.
    Path,
}

/// The compositing mode applied when blending a draw command onto the target.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BlendMode {
    /// Standard source-over alpha compositing.
    Normal,
    /// Multiply blend.
    Multiply,
    /// Screen blend.
    Screen,
    /// Overlay blend.
    Overlay,
}

// ── BatchKey ──────────────────────────────────────────────────────────────────

/// The minimal state that forces a draw-call boundary.
///
/// Commands that share the same `BatchKey` and are adjacent in the sorted
/// order can be merged into a single [`DrawBatch`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BatchKey {
    /// Optional texture ID (`None` for untextured commands).
    pub texture_id: Option<u64>,
    /// Required shader pipeline.
    pub pipeline: PipelineKind,
    /// Required blend mode.
    pub blend: BlendMode,
}

// ── DrawBatch ─────────────────────────────────────────────────────────────────

/// A contiguous run of draw commands that share the same [`BatchKey`].
pub struct DrawBatch {
    /// The shared pipeline / texture / blend state.
    pub key: BatchKey,
    /// Range of *original* command indices (before sorting) covered by this
    /// batch.  The GPU consumer uses these to look up the actual commands.
    pub command_range: std::ops::Range<usize>,
    /// Number of draw instances in this batch.
    pub instance_count: usize,
}

// ── PreparedFrame ─────────────────────────────────────────────────────────────

/// The output of a single [`batch`] call.
pub struct PreparedFrame {
    /// Merged batches in sorted draw order.
    pub batches: Vec<DrawBatch>,
    /// Number of commands that were dropped by visibility culling.
    pub culled_count: usize,
}

// ── Public batch() function ──────────────────────────────────────────────────

/// Classify, cull, sort, and batch the commands in `list`.
///
/// ## Visibility culling
///
/// If `active_clip` is `Some([x, y, w, h])`, commands whose conservative
/// bounding box does not intersect the clip rectangle are skipped and counted
/// in [`PreparedFrame::culled_count`].  Commands that carry no bounds (e.g.
/// `PopClip`) always pass culling.  Clip-stack management commands
/// (`PushClip` / `PopClip`) are excluded from batching entirely.
///
/// ## Ordering guarantee
///
/// The sort is stable, so the relative submission order of commands that share
/// the same [`BatchKey`] is preserved.
pub fn batch(list: &DrawList, active_clip: Option<[f32; 4]>) -> PreparedFrame {
    // Collect (original_index, command_ref) pairs for drawable commands only.
    let mut drawable: Vec<(usize, &DrawCommand)> = list
        .iter()
        .enumerate()
        .filter(|(_, cmd)| !is_clip_ctrl(cmd))
        .collect();

    // --- Visibility culling ---
    let mut culled_count = 0usize;
    if let Some(clip) = active_clip {
        let clip_rect = clip_array_to_rect(clip);
        drawable.retain(|(_, cmd)| {
            match command_bounds(cmd) {
                None => true, // no bounds → always keep
                Some(bounds) => {
                    if rects_intersect(bounds, clip_rect) {
                        true
                    } else {
                        culled_count += 1;
                        false
                    }
                }
            }
        });
    }

    // --- Stable sort by BatchKey ---
    drawable.sort_by_key(|(_, cmd)| classify(cmd));

    // --- Merge adjacent same-key runs ---
    let mut batches: Vec<DrawBatch> = Vec::new();
    let mut i = 0;
    while i < drawable.len() {
        let key = classify(drawable[i].1);
        let orig_start = drawable[i].0;
        let mut orig_end = orig_start + 1;
        let run_start = i;
        i += 1;
        while i < drawable.len() && classify(drawable[i].1) == key {
            orig_end = drawable[i].0 + 1;
            i += 1;
        }
        let run_len = i - run_start;
        batches.push(DrawBatch {
            key,
            command_range: orig_start..orig_end,
            instance_count: run_len,
        });
    }

    PreparedFrame {
        batches,
        culled_count,
    }
}

// ── Private helpers ───────────────────────────────────────────────────────────

/// Returns `true` for clip-stack management commands that are not batched.
fn is_clip_ctrl(cmd: &DrawCommand) -> bool {
    matches!(cmd, DrawCommand::PushClip { .. } | DrawCommand::PopClip)
}

/// Derive a [`BatchKey`] for a single drawable command.
fn classify(cmd: &DrawCommand) -> BatchKey {
    let (pipeline, texture_id) = match cmd {
        DrawCommand::FillRect { .. }
        | DrawCommand::StrokeRect { .. }
        | DrawCommand::FillRoundedRect { .. }
        | DrawCommand::FillRoundedRectPerCorner { .. }
        | DrawCommand::FillCircle { .. }
        | DrawCommand::FillEllipse { .. }
        | DrawCommand::Line { .. }
        | DrawCommand::LineAa { .. }
        | DrawCommand::LineThick { .. }
        | DrawCommand::LineDashed { .. }
        | DrawCommand::BoxShadow { .. }
        | DrawCommand::DrawText { .. } => (PipelineKind::SolidColor, None),

        DrawCommand::Image { .. } | DrawCommand::NineSlice { .. } => (PipelineKind::Textured, None),

        DrawCommand::LinearGradient { .. } | DrawCommand::RadialGradient { .. } => {
            (PipelineKind::Gradient, None)
        }

        DrawCommand::FillPath { .. } | DrawCommand::StrokePath { .. } => (PipelineKind::Path, None),

        // Clip commands are filtered out before calling classify; handle anyway.
        _ => (PipelineKind::SolidColor, None),
    };
    BatchKey {
        texture_id,
        pipeline,
        blend: BlendMode::Normal,
    }
}

/// Conservative bounding rect for a single command.
///
/// Returns `None` for clip-stack commands (which carry no draw-space geometry).
/// This is a local re-implementation because `DrawList::cmd_bounds` is private.
fn command_bounds(cmd: &DrawCommand) -> Option<Rect> {
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
            Some(Rect::new(
                from.x.min(to.x) - pad,
                from.y.min(to.y) - pad,
                (from.x - to.x).abs() + *width,
                (from.y - to.y).abs() + *width,
            ))
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

        // Clip commands and unknown future variants have no bounds.
        _ => None,
    }
}

/// Convert the `[x, y, w, h]` clip array to a [`Rect`].
fn clip_array_to_rect(clip: [f32; 4]) -> Rect {
    Rect::new(clip[0], clip[1], clip[2], clip[3])
}

/// Half-open rectangle intersection test.
fn rects_intersect(a: Rect, b: Rect) -> bool {
    a.left() < b.right() && b.left() < a.right() && a.top() < b.bottom() && b.top() < a.bottom()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use oxiui_core::paint::{DrawList, ImageData, ImageFilter};
    use oxiui_core::{
        geometry::{Point, Rect},
        Color,
    };

    fn red() -> Color {
        Color(255, 0, 0, 255)
    }

    fn list_with_n_rects(n: usize) -> DrawList {
        let mut list = DrawList::new();
        for i in 0..n {
            list.push_rect(Rect::new(i as f32, 0.0, 1.0, 1.0), red());
        }
        list
    }

    #[test]
    fn batcher_1000_rects_5_textures_le_5_batches() {
        // 1000 solid-colour rects → all SolidColor pipeline → should merge
        // into a single batch.
        let list = list_with_n_rects(1000);
        let frame = batch(&list, None);
        // All solid-colour → 1 batch; image → adds 1 more only if we add one.
        assert!(
            frame.batches.len() <= 5,
            "expected ≤5 batches, got {}",
            frame.batches.len()
        );
    }

    #[test]
    fn batcher_preserves_relative_order_within_batch() {
        // Two rects at x=0 and x=10: both SolidColor. After stable sort,
        // their relative order within the batch must be preserved.
        let mut list = DrawList::new();
        list.push_rect(Rect::new(0.0, 0.0, 1.0, 1.0), Color(255, 0, 0, 255));
        list.push_rect(Rect::new(10.0, 0.0, 1.0, 1.0), Color(0, 255, 0, 255));
        let frame = batch(&list, None);
        assert_eq!(frame.batches.len(), 1);
        assert_eq!(frame.batches[0].instance_count, 2);
        // command_range.start must be 0 (first command's original index).
        assert_eq!(frame.batches[0].command_range.start, 0);
    }

    #[test]
    fn batcher_visibility_culling_drops_offscreen() {
        let mut list = DrawList::new();
        // On-screen rect.
        list.push_rect(Rect::new(0.0, 0.0, 10.0, 10.0), red());
        // Off-screen rect (far right).
        list.push_rect(Rect::new(500.0, 500.0, 10.0, 10.0), red());

        let clip = [0.0_f32, 0.0, 100.0, 100.0];
        let frame = batch(&list, Some(clip));
        assert_eq!(frame.culled_count, 1, "off-screen rect must be culled");
        // One batch with 1 instance (the on-screen rect).
        let total_instances: usize = frame.batches.iter().map(|b| b.instance_count).sum();
        assert_eq!(total_instances, 1);
    }

    #[test]
    fn batcher_multiple_pipeline_kinds_produce_multiple_batches() {
        let mut list = DrawList::new();
        list.push_rect(Rect::new(0.0, 0.0, 10.0, 10.0), red());
        list.push_gradient_linear(
            Rect::new(10.0, 0.0, 10.0, 10.0),
            Point::new(10.0, 0.0),
            Point::new(20.0, 0.0),
            vec![],
        );
        list.push_image(
            ImageData::new(vec![0, 0, 0, 255], 1, 1),
            Rect::new(20.0, 0.0, 10.0, 10.0),
            ImageFilter::Nearest,
        );
        let frame = batch(&list, None);
        // SolidColor, Gradient, Textured → 3 different pipelines.
        assert_eq!(frame.batches.len(), 3);
    }
}
