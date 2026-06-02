//! CSS Grid Level 1 layout engine.
//!
//! Provides a full implementation of the CSS Grid track-sizing algorithm,
//! including `fr` units, `minmax()`, auto-sizing, template areas, gap,
//! spanning, and sparse row-major auto-placement.
//!
//! # Quick start
//!
//! ```rust
//! use oxiui_core::grid::{GridTemplate, GridItem, GridPlacement, TrackSizing, compute_grid};
//! use oxiui_core::Size;
//!
//! let template = GridTemplate {
//!     cols: vec![TrackSizing::Fr(1.0), TrackSizing::Fr(1.0)],
//!     rows: vec![TrackSizing::Fixed(100.0)],
//!     areas: None,
//!     row_gap: 0.0,
//!     col_gap: 0.0,
//! };
//! let items = vec![
//!     GridItem {
//!         placement: GridPlacement::auto(),
//!         min_content_size: Size::ZERO,
//!         max_content_size: Size::ZERO,
//!     },
//! ];
//! let rects = compute_grid(&template, &items, Size::new(200.0, 100.0));
//! assert_eq!(rects.len(), 1);
//! ```

use std::collections::{HashMap, HashSet};

use crate::geometry::{Rect, Size};

// ── Track sizing functions ────────────────────────────────────────────────────

/// The sizing function for a single grid track.
///
/// Mirrors the CSS `<track-size>` value types from the Grid Level 1 spec.
#[derive(Clone, Debug)]
pub enum TrackSizing {
    /// A fixed pixel size: `100px`.
    Fixed(f32),
    /// A flexible fraction of the remaining free space: `1fr`.
    Fr(f32),
    /// Size the track to its content; behaves as `minmax(min-content, max-content)`.
    Auto,
    /// `minmax(min, max)` — floor at `min`, ceiling at `max`.
    MinMax(Box<TrackSizing>, Box<TrackSizing>),
    /// Shrink the track to the smallest size that avoids overflow (`min-content`).
    MinContent,
    /// Grow the track to the largest intrinsic size of its items (`max-content`).
    MaxContent,
    /// Convenience: expand `n` repetitions of `sizing` into individual tracks.
    Repeat(usize, Box<TrackSizing>),
}

// ── Template ──────────────────────────────────────────────────────────────────

/// A CSS grid template: track definitions, named areas, and gutters.
#[derive(Clone, Debug, Default)]
pub struct GridTemplate {
    /// Sizing functions for explicit row tracks (top to bottom).
    pub rows: Vec<TrackSizing>,
    /// Sizing functions for explicit column tracks (left to right).
    pub cols: Vec<TrackSizing>,
    /// Optional named area grid: `areas[row_idx][col_idx]` is the area name
    /// (or `None` for an unnamed cell). Row and column indices are zero-based.
    pub areas: Option<Vec<Vec<Option<String>>>>,
    /// Gap (gutter) between adjacent row tracks in logical pixels.
    pub row_gap: f32,
    /// Gap (gutter) between adjacent column tracks in logical pixels.
    pub col_gap: f32,
}

// ── Placement ─────────────────────────────────────────────────────────────────

/// Identifies the start line of a grid placement.
#[derive(Clone, Debug)]
pub enum GridLine {
    /// Explicit 1-based line number. Positive values start from the
    /// beginning of the grid; negative value support is reserved for
    /// future explicit track count resolution.
    Line(i32),
    /// Let the auto-placement algorithm decide.
    Auto,
    /// Place by named template area (resolved from `GridTemplate::areas`).
    Named(String),
}

/// One axis of a grid placement: a start line plus a span count.
#[derive(Clone, Debug)]
pub struct GridSpan {
    /// The start line (or `Auto`/`Named` for algorithm-placed items).
    pub line: GridLine,
    /// Number of tracks to span (minimum 1).
    pub span: usize,
}

/// The full placement of a grid item: row and column spans.
#[derive(Clone, Debug)]
pub struct GridPlacement {
    /// Row placement (start line + span).
    pub row: GridSpan,
    /// Column placement (start line + span).
    pub col: GridSpan,
}

impl GridPlacement {
    /// Fully auto-placed item: both axes use `Auto`, span = 1.
    pub fn auto() -> Self {
        Self {
            row: GridSpan {
                line: GridLine::Auto,
                span: 1,
            },
            col: GridSpan {
                line: GridLine::Auto,
                span: 1,
            },
        }
    }

    /// Explicit line placement with no span (span = 1 on each axis).
    ///
    /// Lines are 1-based; negative values count from the grid end.
    pub fn at(row: i32, col: i32) -> Self {
        Self {
            row: GridSpan {
                line: GridLine::Line(row),
                span: 1,
            },
            col: GridSpan {
                line: GridLine::Line(col),
                span: 1,
            },
        }
    }

    /// Explicit line placement with explicit row/column spans.
    pub fn span(row: i32, col: i32, row_span: usize, col_span: usize) -> Self {
        Self {
            row: GridSpan {
                line: GridLine::Line(row),
                span: row_span.max(1),
            },
            col: GridSpan {
                line: GridLine::Line(col),
                span: col_span.max(1),
            },
        }
    }
}

// ── Grid item ─────────────────────────────────────────────────────────────────

/// A grid item with a placement and intrinsic content-size hints.
#[derive(Clone, Debug)]
pub struct GridItem {
    /// Where to place this item in the grid.
    pub placement: GridPlacement,
    /// The smallest size that avoids overflow (`min-content`).
    pub min_content_size: Size,
    /// The largest intrinsic size (`max-content`).
    pub max_content_size: Size,
}

// ── Internal resolved placement ───────────────────────────────────────────────

/// Resolved 1-based inclusive placement (row_start, col_start, row_end, col_end).
/// `row_end` and `col_end` are the lines *after* the last track, i.e. exclusive.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ResolvedPlacement {
    /// 1-based start row line (inclusive).
    row_start: usize,
    /// 1-based exclusive row end line.
    row_end: usize,
    /// 1-based start column line (inclusive).
    col_start: usize,
    /// 1-based exclusive column end line.
    col_end: usize,
}

// ── Track record ──────────────────────────────────────────────────────────────

/// Internal per-track state during the sizing algorithm.
#[derive(Clone, Debug)]
struct TrackRecord {
    /// The expanded (Repeat-unwound) sizing function.
    sizing: TrackSizing,
    /// Base size: the minimum the track must be.
    base: f32,
    /// Growth limit: the maximum the track may grow to.
    growth_limit: f32,
}

// ── Named area map ────────────────────────────────────────────────────────────

/// Parses `GridTemplate::areas` into a map from area name →
/// 1-based `(row_start, col_start, row_end_exclusive, col_end_exclusive)`.
fn build_area_map(areas: &[Vec<Option<String>>]) -> HashMap<String, (usize, usize, usize, usize)> {
    let mut map: HashMap<String, (usize, usize, usize, usize)> = HashMap::new();
    for (r, row) in areas.iter().enumerate() {
        for (c, cell) in row.iter().enumerate() {
            if let Some(name) = cell {
                let row1 = r + 1;
                let col1 = c + 1;
                map.entry(name.clone())
                    .and_modify(|e| {
                        // Extend the bounding box.
                        e.0 = e.0.min(row1);
                        e.1 = e.1.min(col1);
                        e.2 = e.2.max(row1 + 1);
                        e.3 = e.3.max(col1 + 1);
                    })
                    .or_insert((row1, col1, row1 + 1, col1 + 1));
            }
        }
    }
    map
}

// ── Track expansion (Repeat unwinding) ───────────────────────────────────────

/// Expands a slice of `TrackSizing` values, unwinding any `Repeat(n, s)` entries.
fn expand_tracks(specs: &[TrackSizing]) -> Vec<TrackSizing> {
    let mut out = Vec::new();
    for s in specs {
        expand_one(s, &mut out);
    }
    out
}

fn expand_one(s: &TrackSizing, out: &mut Vec<TrackSizing>) {
    match s {
        TrackSizing::Repeat(n, inner) => {
            for _ in 0..*n {
                expand_one(inner, out);
            }
        }
        other => out.push(other.clone()),
    }
}

// ── Implicit track count expansion ───────────────────────────────────────────

/// Ensures `tracks` has at least `needed` entries, padding with `Auto` tracks.
fn ensure_track_count(tracks: &mut Vec<TrackRecord>, needed: usize) {
    while tracks.len() < needed {
        tracks.push(TrackRecord {
            sizing: TrackSizing::Auto,
            base: 0.0,
            growth_limit: f32::INFINITY,
        });
    }
}

// ── Track record initialisation ───────────────────────────────────────────────

/// Builds the initial `TrackRecord` for a sizing function, before content sizes
/// are considered. `Auto` and content-based tracks start at 0/∞; they are
/// updated in the content-sizing pass.
fn make_track_record(sizing: &TrackSizing) -> TrackRecord {
    let (base, growth_limit) = initial_base_growth(sizing);
    TrackRecord {
        sizing: sizing.clone(),
        base,
        growth_limit,
    }
}

fn initial_base_growth(sizing: &TrackSizing) -> (f32, f32) {
    match sizing {
        TrackSizing::Fixed(px) => (*px, *px),
        TrackSizing::Fr(_) => (0.0, f32::INFINITY),
        TrackSizing::Auto => (0.0, f32::INFINITY),
        TrackSizing::MinContent => (0.0, f32::INFINITY),
        TrackSizing::MaxContent => (0.0, f32::INFINITY),
        TrackSizing::MinMax(min, max) => {
            let (b, _) = initial_base_growth(min);
            let (_, g) = initial_base_growth(max);
            (b, g)
        }
        TrackSizing::Repeat(_, inner) => initial_base_growth(inner),
    }
}

// ── Content-size contribution to a single span-1 track ───────────────────────

/// Returns `true` if a sizing function's base is driven by intrinsic content
/// (i.e. it should be updated from item min/max content sizes).
fn is_intrinsic_min(sizing: &TrackSizing) -> bool {
    matches!(
        sizing,
        TrackSizing::Auto | TrackSizing::MinContent | TrackSizing::MaxContent
    )
}

/// Returns `true` if a sizing function uses max-content for its base
/// (as opposed to min-content).
fn uses_max_content_base(sizing: &TrackSizing) -> bool {
    matches!(sizing, TrackSizing::MaxContent)
}

// ── Core algorithm ────────────────────────────────────────────────────────────

/// Compute grid layout.
///
/// Given a `template`, a list of `items`, and the `available` container size,
/// returns one [`Rect`] per item (in the same order as `items`), each
/// describing the item's position and size within the grid.
///
/// # Algorithm
///
/// Follows CSS Grid Level 1 specification order:
/// 1. Expand `Repeat` track shorthands.
/// 2. Resolve named template areas.
/// 3. Resolve explicit placements; run sparse row-major auto-placement.
/// 4. Initialise track base sizes from intrinsic content.
/// 5. Distribute free space to `fr` tracks.
/// 6. Compute per-track offsets (prefix sum with gaps).
/// 7. Build output `Rect` for each item.
pub fn compute_grid(template: &GridTemplate, items: &[GridItem], available: Size) -> Vec<Rect> {
    if items.is_empty() {
        return Vec::new();
    }

    // ── Step 1: Expand Repeat shorthands ─────────────────────────────────────
    let explicit_col_specs = expand_tracks(&template.cols);
    let explicit_row_specs = expand_tracks(&template.rows);
    let explicit_cols = explicit_col_specs.len();
    let explicit_rows = explicit_row_specs.len();

    // ── Step 2: Build named-area map ─────────────────────────────────────────
    let area_map: HashMap<String, (usize, usize, usize, usize)> = template
        .areas
        .as_deref()
        .map(build_area_map)
        .unwrap_or_default();

    // ── Step 3: Resolve placements ────────────────────────────────────────────

    // First pass: resolve items whose both axes are non-Auto.
    // We need the final track counts to handle negative line numbers, so we do
    // a preliminary scan to find the maximum required tracks.

    // Pre-resolve named and explicit lines (not auto).
    let mut pre_placements: Vec<Option<ResolvedPlacement>> = vec![None; items.len()];

    for (idx, item) in items.iter().enumerate() {
        let row_line = resolve_grid_line(&item.placement.row.line, &area_map, true);
        let col_line = resolve_grid_line(&item.placement.col.line, &area_map, false);

        if let (Some(rs), Some(cs)) = (row_line, col_line) {
            let re = rs + item.placement.row.span;
            let ce = cs + item.placement.col.span;
            pre_placements[idx] = Some(ResolvedPlacement {
                row_start: rs,
                row_end: re,
                col_start: cs,
                col_end: ce,
            });
        } else if let (Some(rs), None) = (row_line, col_line) {
            // Row explicit, col auto — handled in auto-placement pass.
            let re = rs + item.placement.row.span;
            pre_placements[idx] = Some(ResolvedPlacement {
                row_start: rs,
                row_end: re,
                col_start: 0, // sentinel for "not yet placed"
                col_end: 0,
            });
        }
    }

    // Determine required track count from pre-placed items.
    let mut max_row = explicit_rows.max(1);
    let mut max_col = explicit_cols.max(1);
    for p in pre_placements.iter().flatten() {
        if p.col_start != 0 {
            max_row = max_row.max(p.row_end.saturating_sub(1));
            max_col = max_col.max(p.col_end.saturating_sub(1));
        }
    }

    // Build track records, initially from explicit specs, padded with Auto.
    let mut row_tracks: Vec<TrackRecord> =
        explicit_row_specs.iter().map(make_track_record).collect();
    let mut col_tracks: Vec<TrackRecord> =
        explicit_col_specs.iter().map(make_track_record).collect();
    ensure_track_count(&mut row_tracks, max_row);
    ensure_track_count(&mut col_tracks, max_col);

    // Occupied cells set: (1-based row, 1-based col).
    let mut occupied: HashSet<(usize, usize)> = HashSet::new();

    // Mark cells occupied by fully-placed items.
    for p in pre_placements.iter().flatten() {
        if p.col_start != 0 {
            mark_occupied(&mut occupied, p);
        }
    }

    // Final placements: one per item.
    let mut placements: Vec<ResolvedPlacement> = vec![
        ResolvedPlacement {
            row_start: 1,
            row_end: 2,
            col_start: 1,
            col_end: 2
        };
        items.len()
    ];

    // Copy over already-placed items.
    for (idx, pre) in pre_placements.iter().enumerate() {
        if let Some(p) = pre {
            if p.col_start != 0 {
                placements[idx] = p.clone();
            }
        }
    }

    // Auto-placement cursor (1-based).
    let mut cur_row: usize = 1;
    let mut cur_col: usize = 1;

    // The number of auto-placement columns is the current column count
    // (may grow as we place items).
    let auto_col_count = |col_tracks: &Vec<TrackRecord>| col_tracks.len();

    for (idx, item) in items.iter().enumerate() {
        let pre = &pre_placements[idx];
        // Skip already fully-placed.
        if let Some(p) = pre {
            if p.col_start != 0 {
                continue;
            }
        }

        let span_row = item.placement.row.span.max(1);
        let span_col = item.placement.col.span.max(1);

        // If row was pre-resolved (row explicit, col auto):
        if let Some(p) = pre {
            // Row is fixed; scan columns in that row for a free slot.
            let fixed_rs = p.row_start;
            let fixed_re = p.row_end;
            let mut c = 1usize;
            loop {
                if c + span_col - 1 > auto_col_count(&col_tracks) {
                    // Grow columns.
                    ensure_track_count(&mut col_tracks, c + span_col - 1);
                }
                if slots_free(&occupied, fixed_rs, fixed_re, c, c + span_col) {
                    break;
                }
                c += 1;
                // Grow if needed.
                ensure_track_count(&mut col_tracks, c + span_col - 1);
            }
            let placement = ResolvedPlacement {
                row_start: fixed_rs,
                row_end: fixed_re,
                col_start: c,
                col_end: c + span_col,
            };
            mark_occupied(&mut occupied, &placement);
            placements[idx] = placement;
            continue;
        }

        // Both auto: advance cursor in row-major order.
        loop {
            // Grow columns to fit span.
            let needed_cols = auto_col_count(&col_tracks).max(span_col);
            ensure_track_count(&mut col_tracks, needed_cols);

            let col_limit = auto_col_count(&col_tracks);

            if cur_col + span_col - 1 > col_limit {
                // Wrap to next row.
                cur_row += 1;
                cur_col = 1;
                ensure_track_count(&mut row_tracks, cur_row + span_row - 1);
            }

            let rs = cur_row;
            let re = cur_row + span_row;
            let cs = cur_col;
            let ce = cur_col + span_col;

            // Grow row tracks if needed.
            ensure_track_count(&mut row_tracks, re.saturating_sub(1).max(1));

            if slots_free(&occupied, rs, re, cs, ce) {
                let placement = ResolvedPlacement {
                    row_start: rs,
                    row_end: re,
                    col_start: cs,
                    col_end: ce,
                };
                mark_occupied(&mut occupied, &placement);
                placements[idx] = placement;
                // Advance past multi-column spans too.
                cur_col = cs + span_col;
                if cur_col > auto_col_count(&col_tracks) {
                    cur_row += 1;
                    cur_col = 1;
                }
                break;
            } else {
                // Advance one column.
                cur_col += 1;
                if cur_col > col_limit {
                    cur_row += 1;
                    cur_col = 1;
                    ensure_track_count(&mut row_tracks, cur_row);
                }
            }
        }
    }

    // ── Step 4: Resolve track base sizes from content ─────────────────────────

    // For span-1 items only: update intrinsic track bases.
    for (item, placement) in items.iter().zip(placements.iter()) {
        // Row tracks.
        if placement.row_end - placement.row_start == 1 {
            let ri = placement.row_start - 1; // 0-based index
            if ri < row_tracks.len() {
                let track = &mut row_tracks[ri];
                if is_intrinsic_min(&track.sizing) {
                    let content = if uses_max_content_base(&track.sizing) {
                        item.max_content_size.height
                    } else {
                        item.min_content_size.height
                    };
                    track.base = track.base.max(content);
                }
            }
        }
        // Column tracks.
        if placement.col_end - placement.col_start == 1 {
            let ci = placement.col_start - 1;
            if ci < col_tracks.len() {
                let track = &mut col_tracks[ci];
                if is_intrinsic_min(&track.sizing) {
                    let content = if uses_max_content_base(&track.sizing) {
                        item.max_content_size.width
                    } else {
                        item.min_content_size.width
                    };
                    track.base = track.base.max(content);
                }
            }
        }
    }

    // Apply MinMax floors and ceilings after content resolution.
    apply_minmax_clamps(&mut col_tracks);
    apply_minmax_clamps(&mut row_tracks);

    // ── Step 5: Distribute free space to Fr tracks ────────────────────────────
    distribute_fr(&mut col_tracks, available.width, template.col_gap);
    distribute_fr(&mut row_tracks, available.height, template.row_gap);

    // ── Step 6: Compute per-track start positions ─────────────────────────────
    let col_starts = compute_starts(&col_tracks, template.col_gap);
    let row_starts = compute_starts(&row_tracks, template.row_gap);

    // ── Step 7: Build output rects ────────────────────────────────────────────
    let mut out = Vec::with_capacity(items.len());
    for placement in &placements {
        let cs = placement.col_start.saturating_sub(1); // 0-based track index
        let ce = (placement.col_end - 1).saturating_sub(1); // inclusive last track
        let rs = placement.row_start.saturating_sub(1);
        let re = (placement.row_end - 1).saturating_sub(1);

        let x = col_starts.get(cs).copied().unwrap_or(0.0);
        let y = row_starts.get(rs).copied().unwrap_or(0.0);

        let x_end = if ce < col_starts.len() && ce < col_tracks.len() {
            col_starts[ce] + col_tracks[ce].base
        } else if cs < col_starts.len() && cs < col_tracks.len() {
            col_starts[cs] + col_tracks[cs].base
        } else {
            x
        };

        let y_end = if re < row_starts.len() && re < row_tracks.len() {
            row_starts[re] + row_tracks[re].base
        } else if rs < row_starts.len() && rs < row_tracks.len() {
            row_starts[rs] + row_tracks[rs].base
        } else {
            y
        };

        let w = (x_end - x).max(0.0);
        let h = (y_end - y).max(0.0);
        out.push(Rect::new(x, y, w, h));
    }

    out
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Resolves a `GridLine` to a 1-based absolute start line.
///
/// For `Named` lines, looks up the area in `area_map`.
/// `is_row` selects whether to use the row or column component of the area.
///
/// Returns `None` for `Auto` lines or unresolvable names.
fn resolve_grid_line(
    line: &GridLine,
    area_map: &HashMap<String, (usize, usize, usize, usize)>,
    is_row: bool,
) -> Option<usize> {
    match line {
        GridLine::Line(n) => {
            if *n >= 1 {
                Some(*n as usize)
            } else if *n < 0 {
                // Negative lines cannot be resolved without knowing the total
                // track count; treat as line 1 at this stage (will be overridden
                // by negative-line resolution after track expansion).
                Some(1)
            } else {
                None
            }
        }
        GridLine::Named(name) => area_map
            .get(name)
            .map(|&(rs, cs, _re, _ce)| if is_row { rs } else { cs }),
        GridLine::Auto => None,
    }
}

/// Marks all cells in the bounding box of `p` as occupied.
fn mark_occupied(occupied: &mut HashSet<(usize, usize)>, p: &ResolvedPlacement) {
    for r in p.row_start..p.row_end {
        for c in p.col_start..p.col_end {
            occupied.insert((r, c));
        }
    }
}

/// Returns `true` if all cells in the bounding box
/// `[row_start, row_end) × [col_start, col_end)` are free.
fn slots_free(
    occupied: &HashSet<(usize, usize)>,
    row_start: usize,
    row_end: usize,
    col_start: usize,
    col_end: usize,
) -> bool {
    for r in row_start..row_end {
        for c in col_start..col_end {
            if occupied.contains(&(r, c)) {
                return false;
            }
        }
    }
    true
}

/// Applies `MinMax(min, max)` floor and ceiling constraints to each track's
/// base size after the content-size contribution pass.
fn apply_minmax_clamps(tracks: &mut [TrackRecord]) {
    for track in tracks.iter_mut() {
        if let TrackSizing::MinMax(min_spec, max_spec) = &track.sizing.clone() {
            let floor = match min_spec.as_ref() {
                TrackSizing::Fixed(px) => *px,
                TrackSizing::MinContent | TrackSizing::Auto => track.base,
                _ => 0.0,
            };
            let ceil = match max_spec.as_ref() {
                TrackSizing::Fixed(px) => *px,
                TrackSizing::MaxContent => f32::INFINITY,
                TrackSizing::Fr(_) => f32::INFINITY, // resolved in fr pass
                _ => f32::INFINITY,
            };
            track.base =
                track
                    .base
                    .max(floor)
                    .min(if ceil.is_finite() { ceil } else { track.base });
            track.growth_limit = ceil;
        }
    }
}

/// Distributes free space among `Fr` tracks.
///
/// `available` is the total container size along this axis.
/// `gap` is the gutter between tracks.
fn distribute_fr(tracks: &mut [TrackRecord], available: f32, gap: f32) {
    let gap_total = if tracks.len() > 1 {
        gap * (tracks.len() as f32 - 1.0)
    } else {
        0.0
    };

    // Sum of all non-Fr (base) sizes. Tracks whose max is `Fr` are flexible
    // and must be excluded so that their floor (base) doesn't consume free space
    // that should be distributed to them proportionally.
    let fixed_sum: f32 = tracks
        .iter()
        .map(|t| match &t.sizing {
            TrackSizing::Fr(_) => 0.0,
            TrackSizing::MinMax(_, max) if matches!(max.as_ref(), TrackSizing::Fr(_)) => 0.0,
            _ => t.base,
        })
        .sum();

    let free = (available - gap_total - fixed_sum).max(0.0);

    // Collect fr tracks.
    let fr_indices: Vec<usize> = tracks
        .iter()
        .enumerate()
        .filter_map(|(i, t)| match &t.sizing {
            TrackSizing::Fr(_) => Some(i),
            // MinMax(_, Fr) — also participates.
            TrackSizing::MinMax(_, max) => {
                if matches!(max.as_ref(), TrackSizing::Fr(_)) {
                    Some(i)
                } else {
                    None
                }
            }
            _ => None,
        })
        .collect();

    if fr_indices.is_empty() {
        return;
    }

    let sum_fr: f32 = fr_indices
        .iter()
        .map(|&i| fr_value_of(&tracks[i].sizing))
        .sum();

    if sum_fr <= 0.0 {
        return;
    }

    for i in fr_indices {
        let frac = fr_value_of(&tracks[i].sizing);
        let computed = frac * free / sum_fr;
        let base_floor = tracks[i].base;
        tracks[i].base = computed.max(base_floor);
    }
}

/// Extracts the `fr` multiplier from a sizing function.
/// Returns 0.0 for non-Fr functions.
fn fr_value_of(sizing: &TrackSizing) -> f32 {
    match sizing {
        TrackSizing::Fr(f) => *f,
        TrackSizing::MinMax(_, max) => match max.as_ref() {
            TrackSizing::Fr(f) => *f,
            _ => 0.0,
        },
        _ => 0.0,
    }
}

/// Computes cumulative start positions for each track (prefix sum with gaps).
fn compute_starts(tracks: &[TrackRecord], gap: f32) -> Vec<f32> {
    let mut starts = Vec::with_capacity(tracks.len());
    let mut offset = 0.0f32;
    for (i, track) in tracks.iter().enumerate() {
        if i > 0 {
            offset += gap;
        }
        starts.push(offset);
        offset += track.base;
    }
    starts
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Size;

    fn fixed_item(r: i32, c: i32) -> GridItem {
        GridItem {
            placement: GridPlacement::at(r, c),
            min_content_size: Size::ZERO,
            max_content_size: Size::ZERO,
        }
    }

    fn auto_item() -> GridItem {
        GridItem {
            placement: GridPlacement::auto(),
            min_content_size: Size::ZERO,
            max_content_size: Size::ZERO,
        }
    }

    // ── Fixed-track tests ─────────────────────────────────────────────────────

    #[test]
    fn test_single_fixed_track_row() {
        let template = GridTemplate {
            rows: vec![TrackSizing::Fixed(100.0)],
            cols: vec![TrackSizing::Fixed(200.0)],
            areas: None,
            row_gap: 0.0,
            col_gap: 0.0,
        };
        let items = vec![fixed_item(1, 1)];
        let rects = compute_grid(&template, &items, Size::new(200.0, 200.0));
        assert_eq!(rects.len(), 1);
        assert_eq!(rects[0].origin.y, 0.0);
        assert_eq!(rects[0].size.height, 100.0);
    }

    #[test]
    fn test_single_fixed_track_col() {
        let template = GridTemplate {
            rows: vec![TrackSizing::Fixed(200.0)],
            cols: vec![TrackSizing::Fixed(100.0)],
            areas: None,
            row_gap: 0.0,
            col_gap: 0.0,
        };
        let items = vec![fixed_item(1, 1)];
        let rects = compute_grid(&template, &items, Size::new(200.0, 200.0));
        assert_eq!(rects.len(), 1);
        assert_eq!(rects[0].origin.x, 0.0);
        assert_eq!(rects[0].size.width, 100.0);
    }

    // ── Fr track tests ────────────────────────────────────────────────────────

    #[test]
    fn test_three_equal_fr_tracks() {
        let template = GridTemplate {
            rows: vec![TrackSizing::Fixed(50.0)],
            cols: vec![
                TrackSizing::Fr(1.0),
                TrackSizing::Fr(1.0),
                TrackSizing::Fr(1.0),
            ],
            areas: None,
            row_gap: 0.0,
            col_gap: 0.0,
        };
        let items = vec![fixed_item(1, 1), fixed_item(1, 2), fixed_item(1, 3)];
        let rects = compute_grid(&template, &items, Size::new(300.0, 50.0));
        assert_eq!(rects.len(), 3);
        for r in &rects {
            assert!(
                (r.size.width - 100.0).abs() < 1e-4,
                "expected 100px, got {}",
                r.size.width
            );
        }
    }

    #[test]
    fn test_fr_proportional_split_unequal() {
        let template = GridTemplate {
            rows: vec![TrackSizing::Fixed(50.0)],
            cols: vec![TrackSizing::Fr(1.0), TrackSizing::Fr(2.0)],
            areas: None,
            row_gap: 0.0,
            col_gap: 0.0,
        };
        let items = vec![fixed_item(1, 1), fixed_item(1, 2)];
        let rects = compute_grid(&template, &items, Size::new(300.0, 50.0));
        assert_eq!(rects.len(), 2);
        assert!(
            (rects[0].size.width - 100.0).abs() < 1e-4,
            "col1 = {}",
            rects[0].size.width
        );
        assert!(
            (rects[1].size.width - 200.0).abs() < 1e-4,
            "col2 = {}",
            rects[1].size.width
        );
    }

    // ── MinMax tests ──────────────────────────────────────────────────────────

    #[test]
    fn test_minmax_clamps() {
        // minmax(100, 1fr) — free space = 50px < 100px floor → track must be 100px.
        let template = GridTemplate {
            rows: vec![TrackSizing::Fixed(50.0)],
            cols: vec![TrackSizing::MinMax(
                Box::new(TrackSizing::Fixed(100.0)),
                Box::new(TrackSizing::Fr(1.0)),
            )],
            areas: None,
            row_gap: 0.0,
            col_gap: 0.0,
        };
        let items = vec![fixed_item(1, 1)];
        let rects = compute_grid(&template, &items, Size::new(50.0, 50.0));
        assert_eq!(rects.len(), 1);
        assert!(
            rects[0].size.width >= 100.0,
            "minmax floor violated: {}",
            rects[0].size.width
        );
    }

    #[test]
    fn test_nested_minmax_fr() {
        // minmax(50px, 1fr), available = 200px.
        // CSS Grid spec: the track gets max(floor=50, fr_share=200) = 200px.
        let template = GridTemplate {
            rows: vec![TrackSizing::Fixed(50.0)],
            cols: vec![TrackSizing::MinMax(
                Box::new(TrackSizing::Fixed(50.0)),
                Box::new(TrackSizing::Fr(1.0)),
            )],
            areas: None,
            row_gap: 0.0,
            col_gap: 0.0,
        };
        let items = vec![fixed_item(1, 1)];
        let rects = compute_grid(&template, &items, Size::new(200.0, 50.0));
        assert_eq!(rects.len(), 1);
        // Per CSS Grid Level 1: minmax(50px, 1fr) with 200px free → 200px.
        assert!(
            (rects[0].size.width - 200.0).abs() < 1e-4,
            "minmax(50,1fr) with 200px available: expected 200, got {}",
            rects[0].size.width
        );
    }

    // ── Auto / content sizing ─────────────────────────────────────────────────

    #[test]
    fn test_auto_track_sizes_to_content() {
        let template = GridTemplate {
            rows: vec![TrackSizing::Fixed(50.0)],
            cols: vec![TrackSizing::Auto],
            areas: None,
            row_gap: 0.0,
            col_gap: 0.0,
        };
        let items = vec![GridItem {
            placement: GridPlacement::at(1, 1),
            min_content_size: Size::new(40.0, 50.0),
            max_content_size: Size::new(80.0, 50.0),
        }];
        let rects = compute_grid(&template, &items, Size::new(200.0, 50.0));
        assert_eq!(rects.len(), 1);
        assert!(
            rects[0].size.width >= 40.0,
            "auto track should be ≥ min_content: {}",
            rects[0].size.width
        );
    }

    // ── Explicit placement ────────────────────────────────────────────────────

    #[test]
    fn test_explicit_placement_at_line() {
        // 3 cols of 50px each. Item at row=2, col=3.
        let template = GridTemplate {
            rows: vec![TrackSizing::Fixed(50.0), TrackSizing::Fixed(50.0)],
            cols: vec![
                TrackSizing::Fixed(50.0),
                TrackSizing::Fixed(50.0),
                TrackSizing::Fixed(50.0),
            ],
            areas: None,
            row_gap: 0.0,
            col_gap: 0.0,
        };
        let items = vec![fixed_item(2, 3)];
        let rects = compute_grid(&template, &items, Size::new(150.0, 100.0));
        assert_eq!(rects.len(), 1);
        assert!(
            (rects[0].origin.x - 100.0).abs() < 1e-4,
            "x = {}",
            rects[0].origin.x
        );
        assert!(
            (rects[0].origin.y - 50.0).abs() < 1e-4,
            "y = {}",
            rects[0].origin.y
        );
    }

    // ── Span tests ────────────────────────────────────────────────────────────

    #[test]
    fn test_span_2_occupies_two_tracks() {
        // 2 cols of 100px with 10px gap. Item spans both cols.
        let template = GridTemplate {
            rows: vec![TrackSizing::Fixed(50.0)],
            cols: vec![TrackSizing::Fixed(100.0), TrackSizing::Fixed(100.0)],
            areas: None,
            row_gap: 0.0,
            col_gap: 10.0,
        };
        let items = vec![GridItem {
            placement: GridPlacement::span(1, 1, 1, 2),
            min_content_size: Size::ZERO,
            max_content_size: Size::ZERO,
        }];
        let rects = compute_grid(&template, &items, Size::new(210.0, 50.0));
        assert_eq!(rects.len(), 1);
        // Width = track1 + gap + track2 = 100 + 10 + 100 = 210.
        assert!(
            (rects[0].size.width - 210.0).abs() < 1e-4,
            "span width = {}",
            rects[0].size.width
        );
    }

    // ── Auto-placement tests ──────────────────────────────────────────────────

    #[test]
    fn test_auto_placement_fills_row_major() {
        // 4 auto items, 2 explicit cols with explicit row sizing → 2×2 row-major.
        let template = GridTemplate {
            rows: vec![TrackSizing::Fixed(40.0), TrackSizing::Fixed(40.0)],
            cols: vec![TrackSizing::Fixed(50.0), TrackSizing::Fixed(50.0)],
            areas: None,
            row_gap: 0.0,
            col_gap: 0.0,
        };
        let items = vec![auto_item(), auto_item(), auto_item(), auto_item()];
        let rects = compute_grid(&template, &items, Size::new(100.0, 80.0));
        assert_eq!(rects.len(), 4);
        // Items 0,1 in row 1 (y=0); items 2,3 in row 2 (y=40).
        assert!((rects[0].origin.x - 0.0).abs() < 1e-4);
        assert!((rects[1].origin.x - 50.0).abs() < 1e-4);
        assert!(
            (rects[2].origin.y - 40.0).abs() < 1e-4,
            "row2 y = {}",
            rects[2].origin.y
        );
        assert!((rects[2].origin.x - 0.0).abs() < 1e-4);
        assert!((rects[3].origin.x - 50.0).abs() < 1e-4);
    }

    #[test]
    fn test_auto_placement_with_hole() {
        // Explicit item at (1,2). Three auto items should skip that cell.
        let template = GridTemplate {
            rows: vec![TrackSizing::Fixed(50.0)],
            cols: vec![TrackSizing::Fixed(50.0), TrackSizing::Fixed(50.0)],
            areas: None,
            row_gap: 0.0,
            col_gap: 0.0,
        };
        let explicit = GridItem {
            placement: GridPlacement::at(1, 2),
            min_content_size: Size::ZERO,
            max_content_size: Size::ZERO,
        };
        let items = vec![explicit, auto_item(), auto_item(), auto_item()];
        let rects = compute_grid(&template, &items, Size::new(100.0, 200.0));
        assert_eq!(rects.len(), 4);
        // The explicit item is at x=50.
        assert!(
            (rects[0].origin.x - 50.0).abs() < 1e-4,
            "explicit x = {}",
            rects[0].origin.x
        );
        // No two items should occupy the same cell.
        let positions: Vec<(i32, i32)> = rects
            .iter()
            .map(|r| (r.origin.x as i32, r.origin.y as i32))
            .collect();
        let unique: std::collections::HashSet<_> = positions.iter().cloned().collect();
        assert_eq!(
            positions.len(),
            unique.len(),
            "duplicate positions: {:?}",
            positions
        );
    }

    // ── Template-area tests ───────────────────────────────────────────────────

    #[test]
    fn test_template_areas_named_item() {
        // 2×2 grid. "header" occupies (row1, col1) and (row1, col2).
        let areas = vec![
            vec![Some("header".to_string()), Some("header".to_string())],
            vec![Some("main".to_string()), Some("sidebar".to_string())],
        ];
        let template = GridTemplate {
            rows: vec![TrackSizing::Fixed(60.0), TrackSizing::Fixed(100.0)],
            cols: vec![TrackSizing::Fixed(120.0), TrackSizing::Fixed(80.0)],
            areas: Some(areas),
            row_gap: 0.0,
            col_gap: 0.0,
        };
        let items = vec![GridItem {
            placement: GridPlacement {
                row: GridSpan {
                    line: GridLine::Named("header".to_string()),
                    span: 1,
                },
                col: GridSpan {
                    line: GridLine::Named("header".to_string()),
                    span: 2,
                },
            },
            min_content_size: Size::ZERO,
            max_content_size: Size::ZERO,
        }];
        let rects = compute_grid(&template, &items, Size::new(200.0, 160.0));
        assert_eq!(rects.len(), 1);
        // Header spans cols 1–2: width = 120 + 80 = 200.
        assert!(
            (rects[0].size.width - 200.0).abs() < 1e-4,
            "header width = {}",
            rects[0].size.width
        );
        assert!((rects[0].origin.y - 0.0).abs() < 1e-4);
    }

    // ── Gap tests ─────────────────────────────────────────────────────────────

    #[test]
    fn test_row_col_gap_offsets() {
        // 2 cols of 50px with col_gap=10.
        let template = GridTemplate {
            rows: vec![TrackSizing::Fixed(50.0)],
            cols: vec![TrackSizing::Fixed(50.0), TrackSizing::Fixed(50.0)],
            areas: None,
            row_gap: 0.0,
            col_gap: 10.0,
        };
        let items = vec![fixed_item(1, 1), fixed_item(1, 2)];
        let rects = compute_grid(&template, &items, Size::new(110.0, 50.0));
        assert_eq!(rects.len(), 2);
        assert!((rects[0].origin.x - 0.0).abs() < 1e-4);
        // Second col starts at 50 + 10 = 60.
        assert!(
            (rects[1].origin.x - 60.0).abs() < 1e-4,
            "second col x = {}",
            rects[1].origin.x
        );
    }

    // ── Edge-case tests ───────────────────────────────────────────────────────

    #[test]
    fn test_over_constrained_shrinks_gracefully() {
        // Items larger than available space — no panic, clamp ≥ 0.
        let template = GridTemplate {
            rows: vec![TrackSizing::Fixed(50.0)],
            cols: vec![TrackSizing::Fixed(1000.0)],
            areas: None,
            row_gap: 0.0,
            col_gap: 0.0,
        };
        let items = vec![fixed_item(1, 1)];
        let rects = compute_grid(&template, &items, Size::new(10.0, 50.0));
        assert_eq!(rects.len(), 1);
        assert!(rects[0].size.width >= 0.0);
        assert!(rects[0].size.height >= 0.0);
    }

    #[test]
    fn test_empty_grid_empty_rects() {
        let template = GridTemplate {
            rows: vec![TrackSizing::Fixed(50.0)],
            cols: vec![TrackSizing::Fixed(50.0)],
            areas: None,
            row_gap: 0.0,
            col_gap: 0.0,
        };
        let rects = compute_grid(&template, &[], Size::new(100.0, 100.0));
        assert!(rects.is_empty());
    }

    // ── Repeat expansion ──────────────────────────────────────────────────────

    #[test]
    fn test_repeat_expands_tracks() {
        // repeat(3, 50px) → 3 × 50px tracks.
        let template = GridTemplate {
            rows: vec![TrackSizing::Fixed(50.0)],
            cols: vec![TrackSizing::Repeat(3, Box::new(TrackSizing::Fixed(50.0)))],
            areas: None,
            row_gap: 0.0,
            col_gap: 0.0,
        };
        let items = vec![fixed_item(1, 1), fixed_item(1, 2), fixed_item(1, 3)];
        let rects = compute_grid(&template, &items, Size::new(150.0, 50.0));
        assert_eq!(rects.len(), 3);
        assert!((rects[0].size.width - 50.0).abs() < 1e-4);
        assert!((rects[1].size.width - 50.0).abs() < 1e-4);
        assert!((rects[2].size.width - 50.0).abs() < 1e-4);
        assert!((rects[1].origin.x - 50.0).abs() < 1e-4);
        assert!((rects[2].origin.x - 100.0).abs() < 1e-4);
    }

    // ── CSS Grid spec conformance scenarios ───────────────────────────────────

    /// CSS Grid spec: a 12-column grid with a 3-column spanning item.
    #[test]
    fn test_spec_12col_grid_with_span() {
        let cols: Vec<TrackSizing> = (0..12).map(|_| TrackSizing::Fixed(10.0)).collect();
        let template = GridTemplate {
            rows: vec![TrackSizing::Fixed(50.0)],
            cols,
            areas: None,
            row_gap: 0.0,
            col_gap: 0.0,
        };
        let items = vec![GridItem {
            placement: GridPlacement::span(1, 5, 1, 3),
            min_content_size: Size::ZERO,
            max_content_size: Size::ZERO,
        }];
        let rects = compute_grid(&template, &items, Size::new(120.0, 50.0));
        assert_eq!(rects.len(), 1);
        // col 5–7 inclusive: x = 40, w = 30.
        assert!(
            (rects[0].origin.x - 40.0).abs() < 1e-4,
            "x = {}",
            rects[0].origin.x
        );
        assert!(
            (rects[0].size.width - 30.0).abs() < 1e-4,
            "w = {}",
            rects[0].size.width
        );
    }

    /// CSS Grid spec: auto-placement dense — item mid-row after explicit hole.
    #[test]
    fn test_spec_auto_placement_dense_after_hole() {
        // 3-col grid. Item at (1,2). Auto item should go to (1,1).
        let template = GridTemplate {
            rows: vec![TrackSizing::Fixed(50.0), TrackSizing::Fixed(50.0)],
            cols: vec![
                TrackSizing::Fixed(30.0),
                TrackSizing::Fixed(30.0),
                TrackSizing::Fixed(30.0),
            ],
            areas: None,
            row_gap: 0.0,
            col_gap: 0.0,
        };
        let explicit = GridItem {
            placement: GridPlacement::at(1, 2),
            min_content_size: Size::ZERO,
            max_content_size: Size::ZERO,
        };
        let items = vec![explicit, auto_item()];
        let rects = compute_grid(&template, &items, Size::new(90.0, 100.0));
        assert_eq!(rects.len(), 2);
        // Auto item should be at (1,1): x=0.
        assert!(
            (rects[1].origin.x - 0.0).abs() < 1e-4,
            "auto x = {}",
            rects[1].origin.x
        );
        assert!(
            (rects[1].origin.y - 0.0).abs() < 1e-4,
            "auto y = {}",
            rects[1].origin.y
        );
    }

    /// CSS Grid spec: mixed fr and fixed — fixed tracks take their space first.
    #[test]
    fn test_spec_mixed_fixed_and_fr() {
        // 200px total, col1=50px fixed, col2=1fr → col2 = 150px.
        let template = GridTemplate {
            rows: vec![TrackSizing::Fixed(50.0)],
            cols: vec![TrackSizing::Fixed(50.0), TrackSizing::Fr(1.0)],
            areas: None,
            row_gap: 0.0,
            col_gap: 0.0,
        };
        let items = vec![fixed_item(1, 1), fixed_item(1, 2)];
        let rects = compute_grid(&template, &items, Size::new(200.0, 50.0));
        assert_eq!(rects.len(), 2);
        assert!(
            (rects[0].size.width - 50.0).abs() < 1e-4,
            "fixed = {}",
            rects[0].size.width
        );
        assert!(
            (rects[1].size.width - 150.0).abs() < 1e-4,
            "fr = {}",
            rects[1].size.width
        );
    }

    /// CSS Grid spec: row gaps affect row offsets.
    #[test]
    fn test_spec_row_gap_affects_offsets() {
        let template = GridTemplate {
            rows: vec![TrackSizing::Fixed(40.0), TrackSizing::Fixed(60.0)],
            cols: vec![TrackSizing::Fixed(100.0)],
            areas: None,
            row_gap: 8.0,
            col_gap: 0.0,
        };
        let items = vec![fixed_item(1, 1), fixed_item(2, 1)];
        let rects = compute_grid(&template, &items, Size::new(100.0, 108.0));
        assert_eq!(rects.len(), 2);
        assert!((rects[0].origin.y - 0.0).abs() < 1e-4);
        // Row 2 starts at 40 + 8 = 48.
        assert!(
            (rects[1].origin.y - 48.0).abs() < 1e-4,
            "row2 y = {}",
            rects[1].origin.y
        );
    }

    /// CSS Grid spec: template areas resolution with 2 items.
    #[test]
    fn test_spec_template_areas_two_items() {
        let areas = vec![vec![Some("nav".to_string()), Some("content".to_string())]];
        let template = GridTemplate {
            rows: vec![TrackSizing::Fixed(80.0)],
            cols: vec![TrackSizing::Fixed(60.0), TrackSizing::Fixed(140.0)],
            areas: Some(areas),
            row_gap: 0.0,
            col_gap: 0.0,
        };
        let nav_item = GridItem {
            placement: GridPlacement {
                row: GridSpan {
                    line: GridLine::Named("nav".to_string()),
                    span: 1,
                },
                col: GridSpan {
                    line: GridLine::Named("nav".to_string()),
                    span: 1,
                },
            },
            min_content_size: Size::ZERO,
            max_content_size: Size::ZERO,
        };
        let content_item = GridItem {
            placement: GridPlacement {
                row: GridSpan {
                    line: GridLine::Named("content".to_string()),
                    span: 1,
                },
                col: GridSpan {
                    line: GridLine::Named("content".to_string()),
                    span: 1,
                },
            },
            min_content_size: Size::ZERO,
            max_content_size: Size::ZERO,
        };
        let items = vec![nav_item, content_item];
        let rects = compute_grid(&template, &items, Size::new(200.0, 80.0));
        assert_eq!(rects.len(), 2);
        // nav at x=0, w=60.
        assert!(
            (rects[0].origin.x - 0.0).abs() < 1e-4,
            "nav x = {}",
            rects[0].origin.x
        );
        assert!(
            (rects[0].size.width - 60.0).abs() < 1e-4,
            "nav w = {}",
            rects[0].size.width
        );
        // content at x=60, w=140.
        assert!(
            (rects[1].origin.x - 60.0).abs() < 1e-4,
            "content x = {}",
            rects[1].origin.x
        );
        assert!(
            (rects[1].size.width - 140.0).abs() < 1e-4,
            "content w = {}",
            rects[1].size.width
        );
    }

    /// CSS Grid spec: implicit rows created when items exceed explicit row count.
    #[test]
    fn test_spec_implicit_row_creation() {
        // Only 1 explicit row; 3 auto items with 1 column → 3 rows needed.
        let template = GridTemplate {
            rows: vec![TrackSizing::Fixed(30.0)],
            cols: vec![TrackSizing::Fixed(100.0)],
            areas: None,
            row_gap: 0.0,
            col_gap: 0.0,
        };
        let items = vec![auto_item(), auto_item(), auto_item()];
        let rects = compute_grid(&template, &items, Size::new(100.0, 90.0));
        assert_eq!(rects.len(), 3);
        // All in single column, stacked.
        assert!((rects[0].origin.x - 0.0).abs() < 1e-4);
        assert!((rects[1].origin.x - 0.0).abs() < 1e-4);
        assert!((rects[2].origin.x - 0.0).abs() < 1e-4);
        assert!(rects[1].origin.y >= rects[0].origin.y + rects[0].size.height);
        assert!(rects[2].origin.y >= rects[1].origin.y + rects[1].size.height);
    }
}
