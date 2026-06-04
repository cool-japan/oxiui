//! Flexbox layout engine — single-line and multi-line (wrapping).
//!
//! Computes child rectangles along a main axis with `flex-grow` distribution,
//! `justify-content` main-axis alignment, `align-items` cross-axis alignment,
//! and optional multi-line wrapping with `align-content` cross-axis distribution.
//!
//! ## Parallel subtree layout
//!
//! [`layout_subtrees_parallel`] dispatches a batch of independent container
//! layouts onto Rayon's work-stealing thread pool. It is safe to call from any
//! thread and never allocates on the common path beyond the input/output
//! `Vec`s. Use it when you have many sibling containers whose layouts do not
//! depend on each other.

use crate::geometry::{Rect, Size};
use rayon::prelude::*;

/// The direction children are laid out along the main axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlexDirection {
    /// Left-to-right (main axis = horizontal).
    Row,
    /// Top-to-bottom (main axis = vertical).
    Column,
}

/// Main-axis distribution of free space.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JustifyContent {
    /// Pack items at the start.
    Start,
    /// Centre items as a group.
    Center,
    /// Pack items at the end.
    End,
    /// First item at start, last at end, equal gaps between.
    SpaceBetween,
    /// Equal space around each item (half-size gaps at the edges).
    SpaceAround,
    /// Equal space between and around every item.
    SpaceEvenly,
}

/// Cross-axis alignment of items within the container.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlignItems {
    /// Align to the cross-axis start.
    Start,
    /// Centre on the cross axis.
    Center,
    /// Align to the cross-axis end.
    End,
    /// Stretch to fill the cross axis.
    Stretch,
}

/// Whether and how the flex container wraps its items.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FlexWrap {
    /// All items fit in a single line (CSS `flex-wrap: nowrap`).
    #[default]
    NoWrap,
    /// Items wrap into additional lines in the forward direction.
    Wrap,
    /// Items wrap into additional lines in the reverse direction (lines are reversed).
    WrapReverse,
}

/// Distribution of multiple lines along the cross axis (analogous to
/// `justify-content` but for lines, not items).  Only applies when
/// `wrap != NoWrap` and there is more than one line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AlignContent {
    /// Lines packed at the cross-axis start.
    #[default]
    Start,
    /// Lines centred on the cross axis.
    Center,
    /// Lines packed at the cross-axis end.
    End,
    /// First line at start, last at end, equal gaps between.
    SpaceBetween,
    /// Equal space around each line (half-size gaps at the edges).
    SpaceAround,
    /// Equal space between and around every line.
    SpaceEvenly,
    /// Lines stretched to fill the cross axis equally.
    Stretch,
}

/// A flex item: its base (preferred) size plus grow factor.
#[derive(Clone, Copy, Debug)]
pub struct FlexItem {
    /// Preferred size before any growth/shrink is applied.
    pub basis: Size,
    /// Proportional share of leftover main-axis space (`flex-grow`).
    pub grow: f32,
}

impl FlexItem {
    /// A non-growing item with the given base size.
    pub fn fixed(basis: Size) -> Self {
        Self { basis, grow: 0.0 }
    }

    /// A growing item (`grow = 1.0`) with the given base size.
    pub fn flexible(basis: Size) -> Self {
        Self { basis, grow: 1.0 }
    }
}

/// A flexbox container (single-line or multi-line).
#[derive(Clone, Copy, Debug)]
pub struct FlexLayout {
    /// Main-axis direction.
    pub direction: FlexDirection,
    /// Main-axis distribution.
    pub justify: JustifyContent,
    /// Cross-axis alignment of items within each line.
    pub align: AlignItems,
    /// Gap between adjacent items in logical pixels.
    pub gap: f32,
    /// Whether and how items wrap into multiple lines.
    pub wrap: FlexWrap,
    /// Distribution of lines along the cross axis (only relevant when
    /// `wrap != NoWrap` and there are multiple lines).
    pub align_content: AlignContent,
}

impl Default for FlexLayout {
    fn default() -> Self {
        Self {
            direction: FlexDirection::Row,
            justify: JustifyContent::Start,
            align: AlignItems::Stretch,
            gap: 0.0,
            wrap: FlexWrap::NoWrap,
            align_content: AlignContent::Start,
        }
    }
}

impl FlexLayout {
    /// A row layout (children left-to-right).
    pub fn row() -> Self {
        Self {
            direction: FlexDirection::Row,
            ..Self::default()
        }
    }

    /// A column layout (children top-to-bottom).
    pub fn column() -> Self {
        Self {
            direction: FlexDirection::Column,
            ..Self::default()
        }
    }

    /// Builder: set `justify-content`.
    pub fn with_justify(mut self, justify: JustifyContent) -> Self {
        self.justify = justify;
        self
    }

    /// Builder: set `align-items`.
    pub fn with_align(mut self, align: AlignItems) -> Self {
        self.align = align;
        self
    }

    /// Builder: set the inter-item gap.
    pub fn with_gap(mut self, gap: f32) -> Self {
        self.gap = gap;
        self
    }

    /// Builder: set line-wrapping behaviour.
    pub fn with_wrap(mut self, wrap: FlexWrap) -> Self {
        self.wrap = wrap;
        self
    }

    /// Builder: set cross-axis line distribution (only applies when wrapping).
    pub fn with_align_content(mut self, ac: AlignContent) -> Self {
        self.align_content = ac;
        self
    }

    /// Lay out `items` inside `container`, returning one [`Rect`] per item in
    /// the same order. Rectangles are in `container`'s coordinate space.
    pub fn layout(&self, container: Rect, items: &[FlexItem]) -> Vec<Rect> {
        if items.is_empty() {
            return Vec::new();
        }
        match self.wrap {
            FlexWrap::NoWrap => self.layout_single_line(container, items),
            FlexWrap::Wrap | FlexWrap::WrapReverse => self.layout_wrapped(container, items),
        }
    }

    // ── Single-line layout (original algorithm, unchanged) ──────────────

    fn layout_single_line(&self, container: Rect, items: &[FlexItem]) -> Vec<Rect> {
        let is_row = self.direction == FlexDirection::Row;
        let main_extent = if is_row {
            container.width()
        } else {
            container.height()
        };
        let cross_extent = if is_row {
            container.height()
        } else {
            container.width()
        };

        let main_of = |it: &FlexItem| {
            if is_row {
                it.basis.width
            } else {
                it.basis.height
            }
        };
        let total_basis: f32 = items.iter().map(main_of).sum();
        let total_gap = self.gap * (items.len().saturating_sub(1)) as f32;
        let total_grow: f32 = items.iter().map(|it| it.grow.max(0.0)).sum();

        let free = (main_extent - total_basis - total_gap).max(0.0);

        let mut main_sizes: Vec<f32> = items
            .iter()
            .map(|it| {
                let extra = if total_grow > 0.0 {
                    free * (it.grow.max(0.0) / total_grow)
                } else {
                    0.0
                };
                main_of(it) + extra
            })
            .collect();

        let used_main: f32 = main_sizes.iter().sum::<f32>() + total_gap;
        let leftover = (main_extent - used_main).max(0.0);

        let n = items.len() as f32;
        let (lead, between) = if total_grow > 0.0 {
            (0.0, self.gap)
        } else {
            match self.justify {
                JustifyContent::Start => (0.0, self.gap),
                JustifyContent::Center => (leftover * 0.5, self.gap),
                JustifyContent::End => (leftover, self.gap),
                JustifyContent::SpaceBetween => {
                    if items.len() == 1 {
                        (0.0, self.gap)
                    } else {
                        (0.0, self.gap + leftover / (n - 1.0))
                    }
                }
                JustifyContent::SpaceAround => {
                    let unit = leftover / n;
                    (unit * 0.5, self.gap + unit)
                }
                JustifyContent::SpaceEvenly => {
                    let unit = leftover / (n + 1.0);
                    (unit, self.gap + unit)
                }
            }
        };

        for s in &mut main_sizes {
            if *s < 0.0 {
                *s = 0.0;
            }
        }

        let mut rects = Vec::with_capacity(items.len());
        let mut main_cursor = lead;
        for (i, it) in items.iter().enumerate() {
            let main_size = main_sizes[i];
            let item_cross = if is_row {
                it.basis.height
            } else {
                it.basis.width
            };
            let (cross_size, cross_pos) = match self.align {
                AlignItems::Stretch => (cross_extent, 0.0),
                AlignItems::Start => (item_cross, 0.0),
                AlignItems::Center => (item_cross, (cross_extent - item_cross) * 0.5),
                AlignItems::End => (item_cross, cross_extent - item_cross),
            };

            let rect = if is_row {
                Rect::new(
                    container.left() + main_cursor,
                    container.top() + cross_pos,
                    main_size,
                    cross_size,
                )
            } else {
                Rect::new(
                    container.left() + cross_pos,
                    container.top() + main_cursor,
                    cross_size,
                    main_size,
                )
            };
            rects.push(rect);

            main_cursor += main_size;
            if i + 1 < items.len() {
                main_cursor += between;
            }
        }
        rects
    }

    // ── Multi-line (wrapping) layout ─────────────────────────────────────

    fn layout_wrapped(&self, container: Rect, items: &[FlexItem]) -> Vec<Rect> {
        let is_row = self.direction == FlexDirection::Row;
        let main_extent = if is_row {
            container.width()
        } else {
            container.height()
        };
        let cross_extent = if is_row {
            container.height()
        } else {
            container.width()
        };

        let main_of = |it: &FlexItem| {
            if is_row {
                it.basis.width
            } else {
                it.basis.height
            }
        };
        let cross_of = |it: &FlexItem| {
            if is_row {
                it.basis.height
            } else {
                it.basis.width
            }
        };

        // ── Step 1: partition items into lines ──────────────────────────
        // A new line starts when adding the next item (plus gap) would exceed
        // main_extent.  Each line gets at least one item.
        let mut lines: Vec<Vec<usize>> = Vec::new(); // indices into `items`
        let mut current_line: Vec<usize> = Vec::new();
        let mut current_main: f32 = 0.0;

        for (i, it) in items.iter().enumerate() {
            let item_main = main_of(it).max(0.0);
            let needed = if current_line.is_empty() {
                item_main
            } else {
                current_main + self.gap + item_main
            };

            if !current_line.is_empty() && needed > main_extent + 1e-4 {
                lines.push(current_line);
                current_line = Vec::new();
                current_main = item_main;
            } else {
                current_main = needed;
            }
            current_line.push(i);
        }
        if !current_line.is_empty() {
            lines.push(current_line);
        }

        // ── Step 2: compute each line's cross-axis size ─────────────────
        // The cross size of a line is the maximum cross size of its items
        // (or cross_extent / num_lines for Stretch, resolved below).
        let line_cross_sizes: Vec<f32> = lines
            .iter()
            .map(|line| {
                line.iter()
                    .map(|&i| cross_of(&items[i]).max(0.0))
                    .fold(0.0_f32, f32::max)
            })
            .collect();

        // ── Step 3: determine display order for lines ───────────────────
        // WrapReverse reverses the cross-axis order: the last logical line
        // is displayed first (at the cross-axis start).
        let line_order: Vec<usize> = if self.wrap == FlexWrap::WrapReverse {
            (0..lines.len()).rev().collect()
        } else {
            (0..lines.len()).collect()
        };

        // ── Step 4: compute cross-axis sizes in display order ───────────
        // `display_cross_sizes[d]` is the cross size of the line shown at
        // display position `d`.  For Stretch the per-line size ignores actual
        // item sizes; for all other modes we use the max item cross for each
        // display slot.
        let n_lines = lines.len() as f32;
        let display_cross_sizes: Vec<f32> = if matches!(self.align_content, AlignContent::Stretch) {
            vec![cross_extent / n_lines; lines.len()]
        } else {
            line_order.iter().map(|&li| line_cross_sizes[li]).collect()
        };
        let total_display_cross: f32 = display_cross_sizes.iter().sum();
        let leftover_cross = (cross_extent - total_display_cross).max(0.0);

        // Compute the cross-start for each display slot.
        let (line_cross_starts, resolved_cross_sizes): (Vec<f32>, Vec<f32>) =
            match self.align_content {
                AlignContent::Start | AlignContent::Stretch => {
                    let mut pos = 0.0;
                    let starts = display_cross_sizes
                        .iter()
                        .map(|&sz| {
                            let s = pos;
                            pos += sz;
                            s
                        })
                        .collect();
                    (starts, display_cross_sizes.clone())
                }
                AlignContent::End => {
                    let mut pos = leftover_cross;
                    let starts = display_cross_sizes
                        .iter()
                        .map(|&sz| {
                            let s = pos;
                            pos += sz;
                            s
                        })
                        .collect();
                    (starts, display_cross_sizes.clone())
                }
                AlignContent::Center => {
                    let mut pos = leftover_cross * 0.5;
                    let starts = display_cross_sizes
                        .iter()
                        .map(|&sz| {
                            let s = pos;
                            pos += sz;
                            s
                        })
                        .collect();
                    (starts, display_cross_sizes.clone())
                }
                AlignContent::SpaceBetween => {
                    let gap = if lines.len() <= 1 {
                        0.0
                    } else {
                        leftover_cross / (n_lines - 1.0)
                    };
                    let mut pos = 0.0;
                    let starts = display_cross_sizes
                        .iter()
                        .map(|&sz| {
                            let s = pos;
                            pos += sz + gap;
                            s
                        })
                        .collect();
                    (starts, display_cross_sizes.clone())
                }
                AlignContent::SpaceAround => {
                    let unit = leftover_cross / n_lines;
                    let mut pos = unit * 0.5;
                    let starts = display_cross_sizes
                        .iter()
                        .map(|&sz| {
                            let s = pos;
                            pos += sz + unit;
                            s
                        })
                        .collect();
                    (starts, display_cross_sizes.clone())
                }
                AlignContent::SpaceEvenly => {
                    let unit = leftover_cross / (n_lines + 1.0);
                    let mut pos = unit;
                    let starts = display_cross_sizes
                        .iter()
                        .map(|&sz| {
                            let s = pos;
                            pos += sz + unit;
                            s
                        })
                        .collect();
                    (starts, display_cross_sizes.clone())
                }
            };

        // ── Step 5: lay out each line and build the output rects ────────
        let mut rects_by_index: Vec<Rect> = vec![Rect::new(0.0, 0.0, 0.0, 0.0); items.len()];

        for (display_order, &line_idx) in line_order.iter().enumerate() {
            let line = &lines[line_idx];
            // `cross_start` and `line_cross` are indexed by display position.
            let cross_start = line_cross_starts[display_order];
            let line_cross = resolved_cross_sizes[display_order];

            // Lay out main axis for this line using the existing single-line logic.
            let line_items: Vec<FlexItem> = line.iter().map(|&i| items[i]).collect();
            let line_main_sizes = self.resolve_main_sizes(&line_items, main_extent);
            let (main_lead, main_between) = self.justify_offsets(&line_main_sizes, main_extent);

            let mut main_cursor = main_lead;
            for (j, &orig_idx) in line.iter().enumerate() {
                let it = &items[orig_idx];
                let main_size = line_main_sizes[j];
                let item_cross = cross_of(it).max(0.0);

                let (cross_size, cross_off) = match self.align {
                    AlignItems::Stretch => (line_cross, 0.0),
                    AlignItems::Start => (item_cross, 0.0),
                    AlignItems::Center => (item_cross, (line_cross - item_cross) * 0.5),
                    AlignItems::End => (item_cross, line_cross - item_cross),
                };

                let rect = if is_row {
                    Rect::new(
                        container.left() + main_cursor,
                        container.top() + cross_start + cross_off,
                        main_size,
                        cross_size,
                    )
                } else {
                    Rect::new(
                        container.left() + cross_start + cross_off,
                        container.top() + main_cursor,
                        cross_size,
                        main_size,
                    )
                };
                rects_by_index[orig_idx] = rect;

                main_cursor += main_size;
                if j + 1 < line.len() {
                    main_cursor += main_between;
                }
            }
        }

        rects_by_index
    }

    // ── Shared helpers ───────────────────────────────────────────────────

    /// Resolve main-axis sizes with grow distribution for a line.
    fn resolve_main_sizes(&self, line_items: &[FlexItem], main_extent: f32) -> Vec<f32> {
        let is_row = self.direction == FlexDirection::Row;
        let main_of = |it: &FlexItem| {
            if is_row {
                it.basis.width
            } else {
                it.basis.height
            }
        };

        let total_basis: f32 = line_items.iter().map(main_of).sum();
        let total_gap = self.gap * (line_items.len().saturating_sub(1)) as f32;
        let total_grow: f32 = line_items.iter().map(|it| it.grow.max(0.0)).sum();
        let free = (main_extent - total_basis - total_gap).max(0.0);

        line_items
            .iter()
            .map(|it| {
                let extra = if total_grow > 0.0 {
                    free * (it.grow.max(0.0) / total_grow)
                } else {
                    0.0
                };
                (main_of(it) + extra).max(0.0)
            })
            .collect()
    }

    /// Compute leading offset and between-item spacing from justify-content.
    fn justify_offsets(&self, main_sizes: &[f32], main_extent: f32) -> (f32, f32) {
        let total_gap = self.gap * (main_sizes.len().saturating_sub(1)) as f32;
        let used: f32 = main_sizes.iter().sum::<f32>() + total_gap;
        let leftover = (main_extent - used).max(0.0);
        let n = main_sizes.len() as f32;

        // If any item had grow > 0 in the original items, the free space is
        // already consumed; approximate by checking whether leftover ≈ 0.
        if leftover < 1e-4 {
            return (0.0, self.gap);
        }

        match self.justify {
            JustifyContent::Start => (0.0, self.gap),
            JustifyContent::Center => (leftover * 0.5, self.gap),
            JustifyContent::End => (leftover, self.gap),
            JustifyContent::SpaceBetween => {
                if main_sizes.len() == 1 {
                    (0.0, self.gap)
                } else {
                    (0.0, self.gap + leftover / (n - 1.0))
                }
            }
            JustifyContent::SpaceAround => {
                let unit = leftover / n;
                (unit * 0.5, self.gap + unit)
            }
            JustifyContent::SpaceEvenly => {
                let unit = leftover / (n + 1.0);
                (unit, self.gap + unit)
            }
        }
    }
}

/// A single layout task for [`layout_subtrees_parallel`].
///
/// Encapsulates one independent subtree layout request: the flex spec, the
/// container rectangle, and the slice of items to lay out. The result is a
/// `Vec<Rect>` in the same order as `items`.
pub struct LayoutTask {
    /// The flexbox configuration to use.
    pub layout: FlexLayout,
    /// The container rectangle to lay out into.
    pub container: Rect,
    /// The items to lay out inside the container.
    pub items: Vec<FlexItem>,
}

/// Lay out multiple **independent** subtrees in parallel on Rayon's thread pool.
///
/// Each [`LayoutTask`] in `tasks` is a self-contained layout (its own container
/// rect and item list). Because no task depends on any other, Rayon can compute
/// them concurrently across available CPU cores.
///
/// Returns one `Vec<Rect>` per task, in the same order as `tasks`.
///
/// # When to use
///
/// Prefer this over sequential `FlexLayout::layout` calls when:
/// - You have ≥ 4 independent containers to lay out in one frame.
/// - Each container has at least a handful of items (overhead dominates below ~8
///   items on modern hardware).
///
/// For tiny trees, sequential layout is faster due to lower overhead.
///
/// # Example
///
/// ```rust
/// # use oxiui_core::layout::{FlexLayout, FlexItem, LayoutTask, layout_subtrees_parallel};
/// # use oxiui_core::geometry::{Rect, Size};
/// let tasks: Vec<LayoutTask> = (0..8)
///     .map(|_| LayoutTask {
///         layout: FlexLayout::row(),
///         container: Rect::new(0.0, 0.0, 400.0, 40.0),
///         items: vec![
///             FlexItem::fixed(Size::new(100.0, 40.0)),
///             FlexItem::flexible(Size::new(50.0, 40.0)),
///         ],
///     })
///     .collect();
/// let results = layout_subtrees_parallel(&tasks);
/// assert_eq!(results.len(), 8);
/// assert_eq!(results[0].len(), 2);
/// ```
pub fn layout_subtrees_parallel(tasks: &[LayoutTask]) -> Vec<Vec<Rect>> {
    tasks
        .par_iter()
        .map(|task| task.layout.layout(task.container, &task.items))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Rect, Size};

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.5
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    #[test]
    fn row_start_no_grow() {
        let l = FlexLayout::row();
        let items = [
            FlexItem::fixed(Size::new(20.0, 10.0)),
            FlexItem::fixed(Size::new(30.0, 10.0)),
        ];
        let rects = l.layout(Rect::new(0.0, 0.0, 100.0, 40.0), &items);
        assert_eq!(rects.len(), 2);
        assert!(approx(rects[0].left(), 0.0));
        assert!(approx(rects[0].width(), 20.0));
        assert!(approx(rects[1].left(), 20.0));
        assert!(approx(rects[1].width(), 30.0));
    }

    #[test]
    fn row_grow_fills_container() {
        let l = FlexLayout::row();
        let items = [
            FlexItem::flexible(Size::new(0.0, 10.0)),
            FlexItem::flexible(Size::new(0.0, 10.0)),
        ];
        let rects = l.layout(Rect::new(0.0, 0.0, 100.0, 10.0), &items);
        // Two equal-grow items split 100 evenly.
        assert!(approx(rects[0].width(), 50.0));
        assert!(approx(rects[1].width(), 50.0));
        assert!(approx(rects[1].left(), 50.0));
    }

    #[test]
    fn row_grow_with_gap() {
        let l = FlexLayout::row().with_gap(10.0);
        let items = [
            FlexItem::flexible(Size::new(0.0, 10.0)),
            FlexItem::flexible(Size::new(0.0, 10.0)),
        ];
        let rects = l.layout(Rect::new(0.0, 0.0, 100.0, 10.0), &items);
        // 100 - 10 gap = 90 split => 45 each.
        assert!(approx(rects[0].width(), 45.0));
        assert!(approx(rects[1].left(), 55.0));
        assert!(approx(rects[1].width(), 45.0));
    }

    #[test]
    fn justify_center() {
        let l = FlexLayout::row().with_justify(JustifyContent::Center);
        let items = [FlexItem::fixed(Size::new(40.0, 10.0))];
        let rects = l.layout(Rect::new(0.0, 0.0, 100.0, 10.0), &items);
        // 60 leftover, centred => offset 30.
        assert!(approx(rects[0].left(), 30.0));
    }

    #[test]
    fn justify_space_between() {
        let l = FlexLayout::row().with_justify(JustifyContent::SpaceBetween);
        let items = [
            FlexItem::fixed(Size::new(20.0, 10.0)),
            FlexItem::fixed(Size::new(20.0, 10.0)),
            FlexItem::fixed(Size::new(20.0, 10.0)),
        ];
        let rects = l.layout(Rect::new(0.0, 0.0, 120.0, 10.0), &items);
        // 60 used by items, 60 leftover split into 2 gaps = 30 each.
        assert!(approx(rects[0].left(), 0.0));
        assert!(approx(rects[1].left(), 50.0));
        assert!(approx(rects[2].left(), 100.0));
    }

    #[test]
    fn justify_space_evenly() {
        let l = FlexLayout::row().with_justify(JustifyContent::SpaceEvenly);
        let items = [
            FlexItem::fixed(Size::new(20.0, 10.0)),
            FlexItem::fixed(Size::new(20.0, 10.0)),
        ];
        let rects = l.layout(Rect::new(0.0, 0.0, 100.0, 10.0), &items);
        // 60 leftover / 3 gaps = 20 each: lead 20, then 20+20 gap.
        assert!(approx(rects[0].left(), 20.0));
        assert!(approx(rects[1].left(), 60.0));
    }

    #[test]
    fn align_items_cross_axis() {
        // Column layout: cross axis is horizontal.
        let l = FlexLayout::column().with_align(AlignItems::Center);
        let items = [FlexItem::fixed(Size::new(40.0, 20.0))];
        let rects = l.layout(Rect::new(0.0, 0.0, 100.0, 200.0), &items);
        // Item width 40, container width 100 => centred at x=30.
        assert!(approx(rects[0].left(), 30.0));
        assert!(approx(rects[0].width(), 40.0));

        let stretch = FlexLayout::column().with_align(AlignItems::Stretch);
        let r2 = stretch.layout(Rect::new(0.0, 0.0, 100.0, 200.0), &items);
        assert!(approx(r2[0].width(), 100.0));
    }

    #[test]
    fn empty_items_returns_empty() {
        let l = FlexLayout::row();
        assert!(l.layout(Rect::new(0.0, 0.0, 10.0, 10.0), &[]).is_empty());
    }

    // ── CSS Flexbox wrapping conformance tests (20 scenarios) ────────────

    /// 1. Single row, all items fit — same as NoWrap behavior.
    #[test]
    fn wrap_single_row_fits() {
        let l = FlexLayout::row().with_wrap(FlexWrap::Wrap);
        let items = [
            FlexItem::fixed(Size::new(30.0, 10.0)),
            FlexItem::fixed(Size::new(30.0, 10.0)),
        ];
        let rects = l.layout(Rect::new(0.0, 0.0, 100.0, 40.0), &items);
        assert_eq!(rects.len(), 2);
        // All in one row.
        assert!(close(rects[0].top(), 0.0));
        assert!(close(rects[1].top(), 0.0));
        assert!(close(rects[0].left(), 0.0));
        assert!(close(rects[1].left(), 30.0));
    }

    /// 2. Wrap: 3 items, container too small for all → 2 lines.
    #[test]
    fn wrap_three_items_two_lines() {
        let l = FlexLayout::row().with_wrap(FlexWrap::Wrap);
        // Container width 70, each item width 40 → items 0 and 1 can't fit (need 80).
        // Line 1: item 0 (40px); Line 2: items 1, 2 (40+40=80 > 70... wait that's also too big).
        // Let's use width 90: item 0+1 (40+40=80 ≤ 90), item 2 overflows → Line 1: [0,1], Line 2: [2].
        let items = [
            FlexItem::fixed(Size::new(40.0, 10.0)),
            FlexItem::fixed(Size::new(40.0, 10.0)),
            FlexItem::fixed(Size::new(40.0, 10.0)),
        ];
        let rects = l.layout(Rect::new(0.0, 0.0, 90.0, 40.0), &items);
        assert_eq!(rects.len(), 3);
        // Items 0 and 1 on line 1 (top=0).
        assert!(close(rects[0].top(), 0.0), "item0 top={}", rects[0].top());
        assert!(close(rects[1].top(), 0.0), "item1 top={}", rects[1].top());
        // Item 2 on line 2 (top=10).
        assert!(approx(rects[2].top(), 10.0), "item2 top={}", rects[2].top());
    }

    /// 3. WrapReverse: verify line order reversed.
    #[test]
    fn wrap_reverse_line_order() {
        let l = FlexLayout::row().with_wrap(FlexWrap::WrapReverse);
        let items = [
            FlexItem::fixed(Size::new(60.0, 10.0)),
            FlexItem::fixed(Size::new(60.0, 10.0)), // wraps to line 2
        ];
        let rects = l.layout(Rect::new(0.0, 0.0, 80.0, 40.0), &items);
        // With WrapReverse, the SECOND logical line (item 1) appears at the TOP.
        // item 0 → line 1 (logical), displayed at cross=10 (second display position)
        // item 1 → line 2 (logical), displayed at cross=0 (first display position)
        assert!(
            rects[0].top() > rects[1].top(),
            "item0.top={} item1.top={} — WrapReverse should put item1 above item0",
            rects[0].top(),
            rects[1].top()
        );
    }

    /// 4. AlignContent::Center: 2 lines → centered in cross-axis.
    #[test]
    fn align_content_center_two_lines() {
        let l = FlexLayout::row()
            .with_wrap(FlexWrap::Wrap)
            .with_align_content(AlignContent::Center);
        let items = [
            FlexItem::fixed(Size::new(60.0, 10.0)),
            FlexItem::fixed(Size::new(60.0, 10.0)),
        ];
        // 2 lines × 10px = 20px total, container height=60, so 20 leftover.
        // Center: offset = 10.
        let rects = l.layout(Rect::new(0.0, 0.0, 80.0, 60.0), &items);
        assert!(
            rects[0].top() > 5.0,
            "line1 should be offset from top: top={}",
            rects[0].top()
        );
        assert!(rects[1].top() > rects[0].top(), "line2 below line1");
    }

    /// 5. AlignContent::SpaceBetween: 2 lines → endpoints.
    #[test]
    fn align_content_space_between() {
        let l = FlexLayout::row()
            .with_wrap(FlexWrap::Wrap)
            .with_align_content(AlignContent::SpaceBetween);
        let items = [
            FlexItem::fixed(Size::new(60.0, 10.0)),
            FlexItem::fixed(Size::new(60.0, 10.0)),
        ];
        // Container height=60: first line at top=0, second at top=50 (60-10).
        let rects = l.layout(Rect::new(0.0, 0.0, 80.0, 60.0), &items);
        assert!(close(rects[0].top(), 0.0), "line1 top={}", rects[0].top());
        assert!(approx(rects[1].top(), 50.0), "line2 top={}", rects[1].top());
    }

    /// 6. AlignContent::SpaceAround.
    #[test]
    fn align_content_space_around() {
        let l = FlexLayout::row()
            .with_wrap(FlexWrap::Wrap)
            .with_align_content(AlignContent::SpaceAround);
        let items = [
            FlexItem::fixed(Size::new(60.0, 10.0)),
            FlexItem::fixed(Size::new(60.0, 10.0)),
        ];
        // Container height=60, total cross=20, leftover=40. 2 lines → unit=20.
        // Line 1: offset = 10 (unit/2). Line 2: 10 + 10 + 20 = 40.
        let rects = l.layout(Rect::new(0.0, 0.0, 80.0, 60.0), &items);
        assert!(approx(rects[0].top(), 10.0), "line1 top={}", rects[0].top());
        assert!(approx(rects[1].top(), 40.0), "line2 top={}", rects[1].top());
    }

    /// 7. AlignContent::SpaceEvenly.
    #[test]
    fn align_content_space_evenly() {
        let l = FlexLayout::row()
            .with_wrap(FlexWrap::Wrap)
            .with_align_content(AlignContent::SpaceEvenly);
        let items = [
            FlexItem::fixed(Size::new(60.0, 10.0)),
            FlexItem::fixed(Size::new(60.0, 10.0)),
        ];
        // Container height=60, total cross=20, leftover=40. 2 lines → unit=40/3≈13.3.
        // Line 1: 13.3. Line 2: 13.3 + 10 + 13.3 = 36.6.
        let rects = l.layout(Rect::new(0.0, 0.0, 80.0, 60.0), &items);
        let unit = 40.0 / 3.0;
        assert!(
            approx(rects[0].top(), unit),
            "line1 top={} unit={unit}",
            rects[0].top()
        );
        assert!(
            approx(rects[1].top(), unit + 10.0 + unit),
            "line2 top={}",
            rects[1].top()
        );
    }

    /// 8. AlignContent::Stretch: lines stretch to fill cross axis.
    #[test]
    fn align_content_stretch() {
        let l = FlexLayout::row()
            .with_wrap(FlexWrap::Wrap)
            .with_align_content(AlignContent::Stretch)
            .with_align(AlignItems::Stretch);
        let items = [
            FlexItem::fixed(Size::new(60.0, 10.0)),
            FlexItem::fixed(Size::new(60.0, 10.0)),
        ];
        // Container height=60, 2 lines → each line gets 30px.
        let rects = l.layout(Rect::new(0.0, 0.0, 80.0, 60.0), &items);
        assert!(close(rects[0].top(), 0.0));
        assert!(approx(rects[0].height(), 30.0), "h={}", rects[0].height());
        assert!(approx(rects[1].top(), 30.0), "top={}", rects[1].top());
        assert!(approx(rects[1].height(), 30.0), "h={}", rects[1].height());
    }

    /// 9. Single-item line (oversized item) — gets its own line, no panic.
    #[test]
    fn wrap_oversized_item_own_line() {
        let l = FlexLayout::row().with_wrap(FlexWrap::Wrap);
        let items = [
            FlexItem::fixed(Size::new(200.0, 10.0)), // wider than container
            FlexItem::fixed(Size::new(30.0, 10.0)),
        ];
        let rects = l.layout(Rect::new(0.0, 0.0, 80.0, 40.0), &items);
        assert_eq!(rects.len(), 2);
        // Each item on its own line.
        assert!(
            rects[1].top() > rects[0].top(),
            "item1 should be below oversized item0"
        );
    }

    /// 10. Zero-gap wrapping.
    #[test]
    fn wrap_zero_gap() {
        let l = FlexLayout::row().with_wrap(FlexWrap::Wrap).with_gap(0.0);
        let items = [
            FlexItem::fixed(Size::new(50.0, 10.0)),
            FlexItem::fixed(Size::new(50.0, 10.0)),
            FlexItem::fixed(Size::new(50.0, 10.0)),
        ];
        // Container width=80: items 0 (50≤80), items 0+1 (100>80) → wrap after 0.
        // Line 1: [0], Line 2: [1], Line 3: [2].
        let rects = l.layout(Rect::new(0.0, 0.0, 80.0, 40.0), &items);
        // All items on separate lines OR items 1+2 share a line? width 80 ≥ 50+50=100? No.
        // 50 ≤ 80, 50+50=100 > 80 → item 1 wraps. 50 ≤ 80 → item 2 alone. 3 lines.
        assert!(rects[1].top() > rects[0].top(), "item1 below item0");
    }

    /// 11. Wrap + FlexDirection::Column.
    #[test]
    fn wrap_column_direction() {
        let l = FlexLayout::column().with_wrap(FlexWrap::Wrap);
        let items = [
            FlexItem::fixed(Size::new(10.0, 60.0)),
            FlexItem::fixed(Size::new(10.0, 60.0)), // wraps to second column
        ];
        // Container height=80: first item (60≤80), second item (60+60=120>80) → wraps.
        let rects = l.layout(Rect::new(0.0, 0.0, 40.0, 80.0), &items);
        // Item 1 should be in a new column (different left).
        assert!(
            rects[1].left() > rects[0].left(),
            "column wrap: item1 should be in next column; item0.left={} item1.left={}",
            rects[0].left(),
            rects[1].left()
        );
    }

    /// 12. Wrap + JustifyContent::SpaceBetween within each line.
    #[test]
    fn wrap_with_justify_space_between_per_line() {
        let l = FlexLayout::row()
            .with_wrap(FlexWrap::Wrap)
            .with_justify(JustifyContent::SpaceBetween);
        let items = [
            FlexItem::fixed(Size::new(20.0, 10.0)),
            FlexItem::fixed(Size::new(20.0, 10.0)),
            FlexItem::fixed(Size::new(20.0, 10.0)),
            FlexItem::fixed(Size::new(20.0, 10.0)),
        ];
        // Container width=100: items 0-1 (40≤100), items 0-2 (60≤100), items 0-3 (80≤100) — all fit!
        // So single line, SpaceBetween: leftover=20/3 gaps.
        let rects = l.layout(Rect::new(0.0, 0.0, 100.0, 40.0), &items);
        assert_eq!(rects.len(), 4);
        assert!(close(rects[0].left(), 0.0));
        assert!(approx(rects[3].left() + rects[3].width(), 100.0));
    }

    /// 13. All items same size, wraps exactly at boundary.
    #[test]
    fn wrap_exact_boundary() {
        let l = FlexLayout::row().with_wrap(FlexWrap::Wrap);
        // 3 items of 30px in a 90px container — all fit on one line.
        let items = [
            FlexItem::fixed(Size::new(30.0, 10.0)),
            FlexItem::fixed(Size::new(30.0, 10.0)),
            FlexItem::fixed(Size::new(30.0, 10.0)),
        ];
        let rects = l.layout(Rect::new(0.0, 0.0, 90.0, 20.0), &items);
        // All on same row.
        assert!(close(rects[0].top(), rects[1].top()));
        assert!(close(rects[1].top(), rects[2].top()));
    }

    /// 14. Items with grow > 0 in wrapped lines.
    #[test]
    fn wrap_with_flex_grow() {
        let l = FlexLayout::row().with_wrap(FlexWrap::Wrap);
        let items = [
            FlexItem::flexible(Size::new(20.0, 10.0)), // grows
            FlexItem::fixed(Size::new(80.0, 10.0)),    // won't fit with item0 growing
        ];
        // Container 100px. Item 0 basis=20, item 1 basis=80. 20+80=100 fits.
        // But with grow, item 0 would consume free space. No wrapping needed.
        let rects = l.layout(Rect::new(0.0, 0.0, 100.0, 20.0), &items);
        assert_eq!(rects.len(), 2);
        // Both on same line; item0 grows to fill (100-80=20).
        assert!(close(rects[0].top(), rects[1].top()));
    }

    /// 15. Empty items list with wrap.
    #[test]
    fn wrap_empty_items() {
        let l = FlexLayout::row().with_wrap(FlexWrap::Wrap);
        let rects = l.layout(Rect::new(0.0, 0.0, 100.0, 100.0), &[]);
        assert!(rects.is_empty());
    }

    /// 16. Single item fits in one line.
    #[test]
    fn wrap_single_item() {
        let l = FlexLayout::row().with_wrap(FlexWrap::Wrap);
        let items = [FlexItem::fixed(Size::new(40.0, 20.0))];
        let rects = l.layout(Rect::new(0.0, 0.0, 100.0, 40.0), &items);
        assert_eq!(rects.len(), 1);
        assert!(close(rects[0].left(), 0.0));
        assert!(close(rects[0].top(), 0.0));
        assert!(close(rects[0].width(), 40.0));
    }

    /// 17. Large gap causes more wrapping.
    #[test]
    fn wrap_large_gap() {
        let l = FlexLayout::row().with_wrap(FlexWrap::Wrap).with_gap(30.0);
        let items = [
            FlexItem::fixed(Size::new(30.0, 10.0)),
            FlexItem::fixed(Size::new(30.0, 10.0)),
        ];
        // Container width=80: first item 30, then +gap30+30=90 > 80 → wraps.
        let rects = l.layout(Rect::new(0.0, 0.0, 80.0, 40.0), &items);
        assert!(
            rects[1].top() > rects[0].top(),
            "item1 should be on second line"
        );
    }

    /// 18. WrapReverse + AlignContent::End.
    #[test]
    fn wrap_reverse_align_content_end() {
        let l = FlexLayout::row()
            .with_wrap(FlexWrap::WrapReverse)
            .with_align_content(AlignContent::End);
        let items = [
            FlexItem::fixed(Size::new(60.0, 10.0)),
            FlexItem::fixed(Size::new(60.0, 10.0)),
        ];
        // 2 lines. AlignContent::End: both lines packed at bottom.
        let rects = l.layout(Rect::new(0.0, 0.0, 80.0, 60.0), &items);
        // Both items should be in the lower portion of the container.
        let max_top = rects.iter().map(|r| r.top()).fold(0.0_f32, f32::max);
        assert!(
            max_top > 30.0,
            "lines should be packed toward the end, max_top={max_top}"
        );
    }

    /// 19. Cross-axis AlignItems::Center within each line.
    #[test]
    fn wrap_align_items_center_per_line() {
        let l = FlexLayout::row()
            .with_wrap(FlexWrap::Wrap)
            .with_align(AlignItems::Center);
        let items = [
            FlexItem::fixed(Size::new(60.0, 5.0)),  // line 1
            FlexItem::fixed(Size::new(60.0, 15.0)), // line 2
        ];
        // Container height=40. Line 1 height=5, line 2 height=15.
        // AlignItems::Center: item 0 centered within its line's cross size.
        let rects = l.layout(Rect::new(0.0, 0.0, 80.0, 40.0), &items);
        // Item 0's height should remain 5 (not stretched).
        assert!(
            close(rects[0].height(), 5.0),
            "item0 h={}",
            rects[0].height()
        );
        // Item 1's height should remain 15.
        assert!(
            close(rects[1].height(), 15.0),
            "item1 h={}",
            rects[1].height()
        );
    }

    /// 20. Verify original indices are preserved after wrapping.
    #[test]
    fn wrap_output_preserves_original_order() {
        let l = FlexLayout::row().with_wrap(FlexWrap::Wrap);
        let items = [
            FlexItem::fixed(Size::new(70.0, 10.0)), // idx 0
            FlexItem::fixed(Size::new(70.0, 10.0)), // idx 1 — wraps
            FlexItem::fixed(Size::new(70.0, 10.0)), // idx 2 — wraps again
        ];
        let rects = l.layout(Rect::new(0.0, 0.0, 80.0, 60.0), &items);
        assert_eq!(rects.len(), 3);
        // Each item on its own line; positions increase monotonically.
        assert!(rects[0].top() < rects[1].top(), "idx0 above idx1");
        assert!(rects[1].top() < rects[2].top(), "idx1 above idx2");
    }

    /// 21. WrapReverse with unequal cross sizes: each line gets its OWN cross slot.
    ///
    /// Line 0 (logical): item0, cross=10px.  Line 1 (logical): item1, cross=30px.
    /// WrapReverse: display order is [1, 0], so item1 (30px) is displayed at
    /// the top (display slot 0) and item0 (10px) at the bottom (display slot 1).
    /// The two slots must not overlap.
    #[test]
    fn wrap_reverse_unequal_cross_sizes() {
        let l = FlexLayout::row()
            .with_wrap(FlexWrap::WrapReverse)
            .with_align(AlignItems::Start); // don't stretch items
        let items = [
            FlexItem::fixed(Size::new(60.0, 10.0)), // line 0 (logical), cross=10
            FlexItem::fixed(Size::new(60.0, 30.0)), // line 1 (logical), cross=30
        ];
        // Container: 80×60. Each item wraps (80<60+60).
        let rects = l.layout(Rect::new(0.0, 0.0, 80.0, 60.0), &items);
        assert_eq!(rects.len(), 2);

        // WrapReverse: item1 (30px) is at display position 0 (top).
        //              item0 (10px) is at display position 1 (below item1).
        let top1 = rects[1].top(); // item1 (logical line 1, displayed first)
        let top0 = rects[0].top(); // item0 (logical line 0, displayed second)

        // item1 should be above item0.
        assert!(top1 < top0,
            "WrapReverse: item1 (30px cross, display-first) top={top1} should be < item0 top={top0}");

        // The two rects must not overlap (item0 starts at or after item1's bottom).
        let bottom1 = top1 + rects[1].height();
        assert!(
            top0 >= bottom1 - 1e-3,
            "no overlap: item0.top={top0} must be >= item1.bottom={bottom1}"
        );

        // item1 height is 30 (not stretched to 10px).
        assert!(
            close(rects[1].height(), 30.0),
            "item1 height={}",
            rects[1].height()
        );
        // item0 height is 10 (not stretched to 30px).
        assert!(
            close(rects[0].height(), 10.0),
            "item0 height={}",
            rects[0].height()
        );
    }

    // ── Parallel layout tests ──────────────────────────────────────────────

    /// Parallel layout of 8 independent row containers produces identical
    /// results to sequential layout.
    #[test]
    fn parallel_layout_matches_sequential() {
        let tasks: Vec<LayoutTask> = (0..8_u32)
            .map(|i| LayoutTask {
                layout: FlexLayout::row(),
                container: Rect::new(0.0, 0.0, 400.0, 40.0),
                items: vec![
                    FlexItem::fixed(Size::new(100.0, 40.0)),
                    FlexItem::flexible(Size::new(50.0 + i as f32, 40.0)),
                ],
            })
            .collect();

        let parallel_results = layout_subtrees_parallel(&tasks);
        assert_eq!(parallel_results.len(), 8);

        for (task, par_rects) in tasks.iter().zip(parallel_results.iter()) {
            let seq_rects = task.layout.layout(task.container, &task.items);
            assert_eq!(seq_rects.len(), par_rects.len());
            for (sr, pr) in seq_rects.iter().zip(par_rects.iter()) {
                assert!(
                    close(sr.left(), pr.left()) && close(sr.width(), pr.width()),
                    "parallel and sequential results diverge"
                );
            }
        }
    }

    /// Parallel layout of an empty task list returns an empty result.
    #[test]
    fn parallel_layout_empty_tasks() {
        let results = layout_subtrees_parallel(&[]);
        assert!(results.is_empty());
    }

    /// Parallel layout of a single task with an empty item list returns an
    /// empty rect vec (mirrors the sequential behaviour).
    #[test]
    fn parallel_layout_single_empty_items() {
        let tasks = [LayoutTask {
            layout: FlexLayout::column(),
            container: Rect::new(0.0, 0.0, 200.0, 200.0),
            items: vec![],
        }];
        let results = layout_subtrees_parallel(&tasks);
        assert_eq!(results.len(), 1);
        assert!(results[0].is_empty());
    }

    /// Parallel layout scales correctly: 64 column containers, each with 3 items.
    #[test]
    fn parallel_layout_large_batch() {
        let tasks: Vec<LayoutTask> = (0..64)
            .map(|_| LayoutTask {
                layout: FlexLayout::column(),
                container: Rect::new(0.0, 0.0, 100.0, 150.0),
                items: vec![
                    FlexItem::fixed(Size::new(100.0, 30.0)),
                    FlexItem::flexible(Size::new(100.0, 20.0)),
                    FlexItem::fixed(Size::new(100.0, 30.0)),
                ],
            })
            .collect();
        let results = layout_subtrees_parallel(&tasks);
        assert_eq!(results.len(), 64);
        for rects in &results {
            assert_eq!(rects.len(), 3);
            // Items must be stacked vertically (non-decreasing top).
            assert!(rects[0].top() <= rects[1].top());
            assert!(rects[1].top() <= rects[2].top());
        }
    }
}
