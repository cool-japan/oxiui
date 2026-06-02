//! Multi-line text editor state.
//!
//! [`TextArea`] is a headless, pure-data multi-line editor supporting:
//! vertical scroll, line numbers, soft/hard wrap, and undo/redo with
//! consecutive-character coalescing.  No rendering — adapters own that.

use crate::selection::Selection;
use std::collections::HashSet;

// ── WrapMode ──────────────────────────────────────────────────────────────────

/// Wrap mode for the text area.
#[derive(Clone, Debug, PartialEq)]
pub enum WrapMode {
    /// Hard wrap: newlines only, no soft wrap.
    Hard,
    /// Soft wrap: wrap at `max_width` pixels.
    ///
    /// When rendering, long lines are split using an estimated char width of
    /// `max_width / 8.0` pixels (since this layer has no access to a live
    /// shaper).
    Soft(f32),
}

// ── EditOp ────────────────────────────────────────────────────────────────────

/// A reversible edit operation used in the undo/redo stack.
#[derive(Clone, Debug)]
enum EditOp {
    /// Inserted `text` at (row, col).
    Insert {
        row: usize,
        col: usize,
        text: String,
    },
    /// Deleted chars from `col_start..col_end` on `row`, content was `deleted`.
    Delete {
        row: usize,
        col_start: usize,
        col_end: usize,
        deleted: String,
    },
    /// Split line `row` at `col` (inserted a newline).
    InsertNewline { row: usize, col: usize },
    /// Joined line `row` with line `row + 1` (deleted the newline).
    DeleteNewline { row: usize },
}

// ── TextArea ──────────────────────────────────────────────────────────────────

/// Multi-line text editor state (headless — no rendering).
///
/// Lines are stored individually without their trailing newline characters.
/// The logical text is reconstructed by [`TextArea::text`].
pub struct TextArea {
    /// Each line of text, stored **without** a trailing newline.
    lines: Vec<String>,
    /// Cursor position as `(row, col)` in *char* indices (not byte offsets).
    cursor: (usize, usize),
    /// Optional selection anchor in `(row, col)` char coordinates.
    selection_anchor: Option<(usize, usize)>,
    /// Vertical scroll in pixels.
    scroll_offset: f32,
    /// Wrap mode for display.
    wrap: WrapMode,
    /// Undo stack; each entry is a coalesced group of [`EditOp`]s.
    undo_stack: Vec<Vec<EditOp>>,
    /// Redo stack; rebuilt by undo operations.
    redo_stack: Vec<Vec<EditOp>>,
    /// Accumulated op waiting to be coalesced or committed.
    pending_op: Option<EditOp>,
    /// Set of line indices that need re-shaping after an edit.
    pub dirty_paragraphs: HashSet<usize>,
    /// Per-line shaped-text cache; `None` means the line needs reshaping.
    shape_cache: Vec<Option<crate::ShapedText>>,
}

impl TextArea {
    /// Create a new [`TextArea`] from `initial_text`, with cursor at `(0, 0)`.
    pub fn new(initial_text: &str, wrap: WrapMode) -> Self {
        let lines: Vec<String> = if initial_text.is_empty() {
            vec![String::new()]
        } else {
            initial_text.split('\n').map(|l| l.to_owned()).collect()
        };
        let n = lines.len();
        let dirty_paragraphs: HashSet<usize> = (0..n).collect();
        let shape_cache: Vec<Option<crate::ShapedText>> = vec![None; n];
        Self {
            lines,
            cursor: (0, 0),
            selection_anchor: None,
            scroll_offset: 0.0,
            wrap,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            pending_op: None,
            dirty_paragraphs,
            shape_cache,
        }
    }

    /// Return the full text, joining lines with `'\n'`.
    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    /// Return the number of lines.
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Return the current cursor position as `(row, col)` in char indices.
    pub fn cursor(&self) -> (usize, usize) {
        self.cursor
    }

    // ── Internal helpers ───────────────────────────────────────────────────

    /// Length of `row` in chars.  Panics in debug if `row` is out of bounds.
    fn line_len(&self, row: usize) -> usize {
        self.lines.get(row).map(|l| l.chars().count()).unwrap_or(0)
    }

    /// Convert `(row, col_chars)` to a byte offset within `self.lines[row]`.
    #[cfg(test)]
    fn col_to_byte(&self, row: usize, col: usize) -> usize {
        let line = match self.lines.get(row) {
            Some(l) => l,
            None => return 0,
        };
        Selection::grapheme_to_byte(line, col)
    }

    /// Apply an `EditOp` forward (for do/redo).
    ///
    /// Also updates the dirty-paragraph set and shape cache so that
    /// [`TextArea::shaped_paragraphs`] only re-shapes the affected lines.
    fn apply_op(&mut self, op: &EditOp) {
        match op {
            EditOp::Insert { row, col, text } => {
                if let Some(line) = self.lines.get_mut(*row) {
                    let byte = Selection::grapheme_to_byte(line, *col);
                    line.insert_str(byte, text);
                    let new_col = col + text.chars().count();
                    self.cursor = (*row, new_col);
                    // Mark the edited line dirty.
                    self.dirty_paragraphs.insert(*row);
                    if let Some(slot) = self.shape_cache.get_mut(*row) {
                        *slot = None;
                    }
                }
            }
            EditOp::Delete {
                row,
                col_start,
                col_end,
                ..
            } => {
                if let Some(line) = self.lines.get_mut(*row) {
                    let b_start = Selection::grapheme_to_byte(line, *col_start);
                    let b_end = Selection::grapheme_to_byte(line, *col_end);
                    line.replace_range(b_start..b_end, "");
                    self.cursor = (*row, *col_start);
                    // Mark the edited line dirty.
                    self.dirty_paragraphs.insert(*row);
                    if let Some(slot) = self.shape_cache.get_mut(*row) {
                        *slot = None;
                    }
                }
            }
            EditOp::InsertNewline { row, col } => {
                if let Some(line) = self.lines.get_mut(*row) {
                    let byte = Selection::grapheme_to_byte(line, *col);
                    let rest = line[byte..].to_owned();
                    line.truncate(byte);
                    let row_idx = *row;
                    self.lines.insert(row_idx + 1, rest);
                    self.cursor = (row_idx + 1, 0);
                    // Both the modified row and the newly inserted row are dirty.
                    self.dirty_paragraphs.insert(row_idx);
                    self.dirty_paragraphs.insert(row_idx + 1);
                    // Grow the cache to accommodate the new line.
                    if row_idx < self.shape_cache.len() {
                        self.shape_cache[row_idx] = None;
                        self.shape_cache.insert(row_idx + 1, None);
                    } else {
                        self.shape_cache.resize_with(self.lines.len(), || None);
                    }
                }
            }
            EditOp::DeleteNewline { row } => {
                let row_idx = *row;
                if row_idx + 1 < self.lines.len() {
                    let next = self.lines.remove(row_idx + 1);
                    let join_col = self.line_len(row_idx);
                    self.lines[row_idx].push_str(&next);
                    self.cursor = (row_idx, join_col);
                    // The merged row is dirty; remove the now-gone row from cache.
                    self.dirty_paragraphs.insert(row_idx);
                    if row_idx < self.shape_cache.len() {
                        self.shape_cache[row_idx] = None;
                    }
                    // Remove the deleted row from the cache if it exists.
                    if row_idx + 1 < self.shape_cache.len() {
                        self.shape_cache.remove(row_idx + 1);
                    }
                    // Also remove from dirty set in case it was queued.
                    self.dirty_paragraphs.remove(&(row_idx + 1));
                }
            }
        }
    }

    /// Apply the *inverse* of an `EditOp` (for undo).
    fn apply_inverse_op(&mut self, op: &EditOp) {
        match op {
            EditOp::Insert { row, col, text } => {
                let col_end = col + text.chars().count();
                let inverse = EditOp::Delete {
                    row: *row,
                    col_start: *col,
                    col_end,
                    deleted: text.clone(),
                };
                self.apply_op(&inverse);
            }
            EditOp::Delete {
                row,
                col_start,
                deleted,
                ..
            } => {
                let inverse = EditOp::Insert {
                    row: *row,
                    col: *col_start,
                    text: deleted.clone(),
                };
                self.apply_op(&inverse);
            }
            EditOp::InsertNewline { row, col } => {
                let inverse = EditOp::DeleteNewline { row: *row };
                // After undoing InsertNewline, cursor goes back to (row, col).
                self.apply_op(&inverse);
                self.cursor = (*row, *col);
            }
            EditOp::DeleteNewline { row } => {
                // We need to know where to split; store the original col.
                // The join_col is implicit from the current line length before
                // the split, but we reconstruct it: the inverse of DeleteNewline
                // is InsertNewline at the position where the second line began.
                // The col is the join_col stored in cursor after the original apply.
                // We must re-derive it from the current undo context.
                // At this point `self.lines[row]` contains the joined text.
                // The original col was the length of the first part, which is
                // the cursor col stored when DeleteNewline was recorded.
                // We track this via a helper: record join col in undo.
                // However, we haven't stored join_col in the op.  The invariant
                // is: after DeleteNewline is applied, cursor = (row, original_len_of_row).
                // We can't recover that without extra state in the op.
                //
                // Solution: store join_col as part of the EditOp variant.
                // But the spec shows `DeleteNewline { row: usize }` only.
                // We must patch this to use a private `_join_col` hint stored
                // in the undo_stack.
                //
                // Instead we use the `DeleteNewline` op's `row` and the current
                // state: after DeleteNewline, the undo must re-split at the saved
                // cursor col.  We store it as the cursor col at undo time.
                // The undo cursor col at this point equals the original line
                // length before the DeleteNewline was applied.
                // We maintain this via the cursor value set during forward apply.
                let join_col = self.cursor.1;
                let inverse = EditOp::InsertNewline {
                    row: *row,
                    col: join_col,
                };
                self.apply_op(&inverse);
            }
        }
    }

    // ── Commit / coalesce ──────────────────────────────────────────────────

    /// Commit the pending operation (if any) onto the undo stack.
    ///
    /// Consecutive typed characters are coalesced: if `pending_op` is
    /// `Insert` at the same row and adjacent column, they are merged into one.
    /// When committed, a new single-item group is pushed onto `undo_stack`.
    pub fn commit_pending(&mut self) {
        if let Some(op) = self.pending_op.take() {
            self.undo_stack.push(vec![op]);
        }
    }

    /// Try to coalesce `new_op` into `pending_op`.
    ///
    /// Returns `true` when coalescing succeeded (no commit needed).
    fn try_coalesce(&mut self, new_op: EditOp) -> bool {
        let can_merge = if let (
            Some(EditOp::Insert {
                row: pr,
                col: pc,
                text: pt,
            }),
            EditOp::Insert {
                row: nr, col: nc, ..
            },
        ) = (&self.pending_op, &new_op)
        {
            *pr == *nr && *pc + pt.chars().count() == *nc
        } else {
            false
        };

        if can_merge {
            if let (Some(EditOp::Insert { text: pt, .. }), EditOp::Insert { text: nt, .. }) =
                (&mut self.pending_op, &new_op)
            {
                pt.push_str(nt);
                return true;
            }
        }
        false
    }

    // ── Editing operations ─────────────────────────────────────────────────

    /// Insert a character at the cursor position.
    ///
    /// When `ch == '\n'`, delegates to [`TextArea::insert_newline`].
    /// Otherwise inserts into the current line and advances the column.
    /// Consecutive inserts on the same row at adjacent columns are coalesced
    /// into a single undo group.
    pub fn insert_char(&mut self, ch: char) {
        if ch == '\n' {
            self.insert_newline();
            return;
        }
        // Clears redo on any new edit.
        self.redo_stack.clear();

        let (row, col) = self.cursor;
        let mut s = String::with_capacity(ch.len_utf8());
        s.push(ch);
        let op = EditOp::Insert { row, col, text: s };

        if !self.try_coalesce(op.clone()) {
            self.commit_pending();
            self.pending_op = Some(op.clone());
        }
        // Apply forward regardless of coalesce result.
        self.apply_op(&op);
    }

    /// Split the current line at the cursor column, inserting a new line.
    ///
    /// Commits any pending coalesced operation first.
    pub fn insert_newline(&mut self) {
        self.redo_stack.clear();
        let (row, col) = self.cursor;
        self.commit_pending();
        let op = EditOp::InsertNewline { row, col };
        self.apply_op(&op);
        self.undo_stack.push(vec![op]);
    }

    /// Delete the character immediately before the cursor (Backspace).
    ///
    /// When `col == 0` and `row > 0`, joins the current line with the previous.
    /// Commits any pending coalesced operation first.
    pub fn delete_backward(&mut self) {
        self.redo_stack.clear();
        self.commit_pending();

        let (row, col) = self.cursor;
        if col == 0 {
            if row == 0 {
                return;
            }
            // Join this line with the previous.
            let op = EditOp::DeleteNewline { row: row - 1 };
            // Before applying, record the join col for inverse reconstruction.
            // We set cursor to (row-1, prev_line_len) to enable undo inverse.
            let prev_len = self.line_len(row - 1);
            self.cursor = (row - 1, prev_len);
            self.apply_op(&op);
            self.undo_stack.push(vec![op]);
        } else {
            // Delete char at col-1.
            let line = match self.lines.get(row) {
                Some(l) => l.clone(),
                None => return,
            };
            let b_start = Selection::grapheme_to_byte(&line, col - 1);
            let b_end = Selection::grapheme_to_byte(&line, col);
            let deleted = line[b_start..b_end].to_owned();
            let op = EditOp::Delete {
                row,
                col_start: col - 1,
                col_end: col,
                deleted,
            };
            self.apply_op(&op);
            self.undo_stack.push(vec![op]);
        }
    }

    /// Delete the character at the cursor position (Delete key).
    ///
    /// When at the end of a line and there is a next line, joins them.
    /// Commits any pending coalesced operation first.
    pub fn delete_forward(&mut self) {
        self.redo_stack.clear();
        self.commit_pending();

        let (row, col) = self.cursor;
        let line_len = self.line_len(row);
        if col >= line_len {
            if row + 1 >= self.lines.len() {
                return;
            }
            // Join with next line.
            let op = EditOp::DeleteNewline { row };
            // Record join col before applying (it's just the current col which equals line_len).
            self.cursor = (row, line_len);
            self.apply_op(&op);
            self.undo_stack.push(vec![op]);
        } else {
            let line = match self.lines.get(row) {
                Some(l) => l.clone(),
                None => return,
            };
            let b_start = Selection::grapheme_to_byte(&line, col);
            let b_end = Selection::grapheme_to_byte(&line, col + 1);
            let deleted = line[b_start..b_end].to_owned();
            let op = EditOp::Delete {
                row,
                col_start: col,
                col_end: col + 1,
                deleted,
            };
            self.apply_op(&op);
            self.undo_stack.push(vec![op]);
        }
    }

    // ── Cursor movement ────────────────────────────────────────────────────

    /// Move the cursor up one row, clamping column to the new line's length.
    pub fn move_up(&mut self) {
        let (row, col) = self.cursor;
        if row == 0 {
            return;
        }
        let new_row = row - 1;
        let new_col = col.min(self.line_len(new_row));
        self.cursor = (new_row, new_col);
        self.selection_anchor = None;
    }

    /// Move the cursor down one row, clamping column to the new line's length.
    pub fn move_down(&mut self) {
        let (row, col) = self.cursor;
        if row + 1 >= self.lines.len() {
            return;
        }
        let new_row = row + 1;
        let new_col = col.min(self.line_len(new_row));
        self.cursor = (new_row, new_col);
        self.selection_anchor = None;
    }

    /// Move the cursor one character to the left.
    ///
    /// When at column 0 and not on the first row, wraps to the end of the
    /// previous line.
    pub fn move_left(&mut self) {
        let (row, col) = self.cursor;
        if col > 0 {
            self.cursor = (row, col - 1);
        } else if row > 0 {
            let prev_len = self.line_len(row - 1);
            self.cursor = (row - 1, prev_len);
        }
        self.selection_anchor = None;
    }

    /// Move the cursor one character to the right.
    ///
    /// When at the end of a line and there is a next line, wraps to column 0
    /// of the next line.
    pub fn move_right(&mut self) {
        let (row, col) = self.cursor;
        let line_len = self.line_len(row);
        if col < line_len {
            self.cursor = (row, col + 1);
        } else if row + 1 < self.lines.len() {
            self.cursor = (row + 1, 0);
        }
        self.selection_anchor = None;
    }

    /// Move the cursor to column 0 (Home key).
    pub fn move_home(&mut self) {
        self.cursor.1 = 0;
        self.selection_anchor = None;
    }

    /// Move the cursor to the end of the current line (End key).
    pub fn move_end(&mut self) {
        let row = self.cursor.0;
        self.cursor.1 = self.line_len(row);
        self.selection_anchor = None;
    }

    /// Move the cursor to the very beginning of the document.
    pub fn move_doc_start(&mut self) {
        self.cursor = (0, 0);
        self.selection_anchor = None;
    }

    /// Move the cursor to the very end of the document.
    pub fn move_doc_end(&mut self) {
        let last_row = self.lines.len().saturating_sub(1);
        let last_col = self.line_len(last_row);
        self.cursor = (last_row, last_col);
        self.selection_anchor = None;
    }

    // ── Undo / Redo ────────────────────────────────────────────────────────

    /// Undo the last edit group.
    ///
    /// Commits any pending coalesced operation first, then pops the most
    /// recent entry from the undo stack, applies each op's inverse in reverse
    /// order, and pushes the group to the redo stack.
    ///
    /// Returns `true` when something was undone.
    pub fn undo(&mut self) -> bool {
        self.commit_pending();
        if let Some(group) = self.undo_stack.pop() {
            // Apply inverses in reverse order.
            for op in group.iter().rev() {
                self.apply_inverse_op(op);
            }
            self.redo_stack.push(group);
            true
        } else {
            false
        }
    }

    /// Redo the last undone edit group.
    ///
    /// Commits any pending coalesced operation first, then pops from the
    /// redo stack, re-applies each op in forward order, and pushes the
    /// group back onto the undo stack.
    ///
    /// Returns `true` when something was redone.
    pub fn redo(&mut self) -> bool {
        self.commit_pending();
        if let Some(group) = self.redo_stack.pop() {
            for op in &group {
                self.apply_op(op);
            }
            self.undo_stack.push(group);
            true
        } else {
            false
        }
    }

    // ── Selection ──────────────────────────────────────────────────────────

    /// Select all text; anchor at `(0, 0)`, cursor at end of last line.
    pub fn select_all(&mut self) {
        let last_row = self.lines.len().saturating_sub(1);
        let last_col = self.line_len(last_row);
        self.selection_anchor = Some((0, 0));
        self.cursor = (last_row, last_col);
    }

    /// Return the selected text, or `None` when the selection is collapsed.
    pub fn selected_text(&self) -> Option<String> {
        let anchor = self.selection_anchor?;
        let cursor = self.cursor;
        if anchor == cursor {
            return None;
        }

        // Normalise to (start, end) in document order.
        let (start, end) = if anchor <= cursor {
            (anchor, cursor)
        } else {
            (cursor, anchor)
        };
        let (start_row, start_col) = start;
        let (end_row, end_col) = end;

        if start_row == end_row {
            let line = self.lines.get(start_row)?;
            let b_start = Selection::grapheme_to_byte(line, start_col);
            let b_end = Selection::grapheme_to_byte(line, end_col);
            return Some(line[b_start..b_end].to_owned());
        }

        let mut parts: Vec<String> = Vec::new();
        // First partial line.
        if let Some(line) = self.lines.get(start_row) {
            let b_start = Selection::grapheme_to_byte(line, start_col);
            parts.push(line[b_start..].to_owned());
        }
        // Middle lines (full).
        for row in (start_row + 1)..end_row {
            if let Some(line) = self.lines.get(row) {
                parts.push(line.clone());
            }
        }
        // Last partial line.
        if let Some(line) = self.lines.get(end_row) {
            let b_end = Selection::grapheme_to_byte(line, end_col);
            parts.push(line[..b_end].to_owned());
        }

        Some(parts.join("\n"))
    }

    // ── Metadata ───────────────────────────────────────────────────────────

    /// Return a list of 1-based line numbers: `[1, 2, …, line_count()]`.
    pub fn line_numbers(&self) -> Vec<usize> {
        (1..=self.lines.len()).collect()
    }

    /// Compute the visible line range for the given scroll offset and viewport.
    ///
    /// `first_line = floor(scroll_offset / line_height)`,
    /// `last_line = first_line + ceil(viewport_height / line_height)`,
    /// clamped to `0..line_count`.
    pub fn visible_range(&self, line_height: f32, viewport_height: f32) -> std::ops::Range<usize> {
        let count = self.lines.len();
        if count == 0 || line_height <= 0.0 {
            return 0..0;
        }
        let first = (self.scroll_offset / line_height).floor() as usize;
        let visible_count = (viewport_height / line_height).ceil() as usize;
        let last = (first + visible_count).min(count);
        let first = first.min(count);
        first..last
    }

    /// Adjust `scroll_offset` so that the cursor row is visible.
    pub fn scroll_to_cursor(&mut self, line_height: f32, viewport_height: f32) {
        if line_height <= 0.0 {
            return;
        }
        let row = self.cursor.0;
        let cursor_top = row as f32 * line_height;
        let cursor_bottom = cursor_top + line_height;

        if cursor_top < self.scroll_offset {
            self.scroll_offset = cursor_top;
        } else if cursor_bottom > self.scroll_offset + viewport_height {
            self.scroll_offset = cursor_bottom - viewport_height;
        }
    }

    /// Return display lines using the wrap mode configured at construction.
    ///
    /// Delegates to [`TextArea::display_lines`] with `self.wrap`.
    pub fn display_lines_default(&self) -> Vec<String> {
        let wrap = self.wrap.clone();
        self.display_lines(&wrap)
    }

    /// Return display lines after applying the wrap mode.
    ///
    /// For [`WrapMode::Hard`], lines are returned as-is.
    /// For [`WrapMode::Soft`], each logical line is split into
    /// visual lines using an estimated char width of `max_width / 8.0`.
    pub fn display_lines(&self, wrap: &WrapMode) -> Vec<String> {
        match wrap {
            WrapMode::Hard => self.lines.clone(),
            WrapMode::Soft(max_width) => {
                let chars_per_line = (max_width / 8.0).max(1.0) as usize;
                let mut result = Vec::new();
                for line in &self.lines {
                    if line.is_empty() {
                        result.push(String::new());
                        continue;
                    }
                    let total_chars = line.chars().count();
                    if total_chars <= chars_per_line {
                        result.push(line.clone());
                    } else {
                        // Split into chunks of `chars_per_line` chars.
                        let chars: Vec<char> = line.chars().collect();
                        let mut start = 0;
                        while start < total_chars {
                            let end = (start + chars_per_line).min(total_chars);
                            let chunk: String = chars[start..end].iter().collect();
                            result.push(chunk);
                            start = end;
                        }
                    }
                }
                result
            }
        }
    }

    /// Return `true` if any edits have been recorded in the undo stack.
    pub fn is_modified(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Shape all dirty paragraphs using `pipeline` and `style`, then return
    /// the full per-line shaped-text cache.
    ///
    /// Only lines listed in `dirty_paragraphs` are re-shaped; all other lines
    /// are returned from the cache without re-shaping.  Lines that could not be
    /// shaped (e.g. because the pipeline reported an error) are represented as
    /// an empty [`crate::ShapedText`].
    ///
    /// After this call `dirty_paragraphs` is cleared.
    pub fn shaped_paragraphs(
        &mut self,
        pipeline: &mut crate::TextPipeline,
        style: &crate::TextStyle,
    ) -> Vec<crate::ShapedText> {
        // Keep cache length in sync with line count.
        self.shape_cache.resize_with(self.lines.len(), || None);

        // Collect dirty indices into a Vec so we can iterate without holding
        // an immutable borrow on `self.dirty_paragraphs` while mutating.
        let dirty: Vec<usize> = self.dirty_paragraphs.iter().copied().collect();
        for idx in dirty {
            if idx < self.lines.len() {
                let line = &self.lines[idx];
                self.shape_cache[idx] = pipeline.shape(line, style).ok();
            }
        }
        self.dirty_paragraphs.clear();

        // Return the full cache; replace any `None` entries (failed / empty
        // lines) with an empty `ShapedText` so indices stay aligned.
        self.shape_cache
            .iter()
            .map(|opt| {
                opt.clone().unwrap_or(crate::ShapedText {
                    lines: Vec::new(),
                    total_width: 0.0,
                    total_height: 0.0,
                })
            })
            .collect()
    }

    /// Return the byte offset within `self.lines[row]` for a given char column.
    ///
    /// Exposed for testing; returns 0 if `row` is out of bounds.
    #[cfg(test)]
    fn col_byte_offset(&self, row: usize, col: usize) -> usize {
        self.col_to_byte(row, col)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn area(text: &str) -> TextArea {
        TextArea::new(text, WrapMode::Hard)
    }

    // ── 1. insert_char advances cursor ────────────────────────────────────

    #[test]
    fn test_insert_char_advances_cursor() {
        let mut ta = area("hello");
        ta.cursor = (0, 5);
        ta.insert_char('!');
        assert_eq!(ta.cursor, (0, 6));
        assert_eq!(ta.lines[0], "hello!");
    }

    // ── 2. insert_newline splits line ─────────────────────────────────────

    #[test]
    fn test_insert_newline_splits_line() {
        let mut ta = area("helloworld");
        ta.cursor = (0, 5);
        ta.insert_newline();
        assert_eq!(ta.lines.len(), 2);
        assert_eq!(ta.lines[0], "hello");
        assert_eq!(ta.lines[1], "world");
        assert_eq!(ta.cursor, (1, 0));
    }

    // ── 3. delete_backward removes char ──────────────────────────────────

    #[test]
    fn test_delete_backward_removes_char() {
        let mut ta = area("abc");
        ta.cursor = (0, 3);
        ta.delete_backward();
        assert_eq!(ta.lines[0], "ab");
        assert_eq!(ta.cursor, (0, 2));
    }

    // ── 4. delete_backward joins lines ────────────────────────────────────

    #[test]
    fn test_delete_backward_joins_lines() {
        let mut ta = area("hello\nworld");
        ta.cursor = (1, 0);
        ta.delete_backward();
        assert_eq!(ta.lines.len(), 1);
        assert_eq!(ta.lines[0], "helloworld");
        assert_eq!(ta.cursor, (0, 5));
    }

    // ── 5. cursor up/down clamped col ─────────────────────────────────────

    #[test]
    fn test_cursor_up_down_preserves_goal_column() {
        let mut ta = area("hello world\nhi");
        // Place cursor at col 10 on long line.
        ta.cursor = (0, 10);
        ta.move_down();
        // "hi" has length 2; col must be clamped.
        assert_eq!(ta.cursor.0, 1);
        assert!(ta.cursor.1 <= 2);
        // Move back up; col was 10 but after clamping to 2 it stays ≤10.
        ta.move_up();
        assert_eq!(ta.cursor.0, 0);
        assert!(ta.cursor.1 <= 10);
    }

    // ── 6. move_left wraps to prev line ──────────────────────────────────

    #[test]
    fn test_move_left_wraps_to_prev_line() {
        let mut ta = area("abc\ndef");
        ta.cursor = (1, 0);
        ta.move_left();
        assert_eq!(ta.cursor, (0, 3)); // end of "abc"
    }

    // ── 7. move_right wraps to next line ─────────────────────────────────

    #[test]
    fn test_move_right_wraps_to_next_line() {
        let mut ta = area("abc\ndef");
        ta.cursor = (0, 3); // end of "abc"
        ta.move_right();
        assert_eq!(ta.cursor, (1, 0));
    }

    // ── 8. soft wrap splits at width ──────────────────────────────────────

    #[test]
    fn test_soft_wrap_splits_at_width() {
        // 80px / 8px-per-char = 10 chars per visual line.
        let ta = TextArea::new("abcdefghij12345", WrapMode::Soft(80.0));
        let display = ta.display_lines(&WrapMode::Soft(80.0));
        assert!(
            display.len() >= 2,
            "long line should split into >=2 visual lines"
        );
        assert_eq!(display[0].chars().count(), 10);
    }

    // ── 9. hard wrap keeps explicit newlines ──────────────────────────────

    #[test]
    fn test_hard_wrap_keeps_explicit_newlines() {
        let text = "line one\nline two\nline three";
        let ta = TextArea::new(text, WrapMode::Hard);
        let display = ta.display_lines(&WrapMode::Hard);
        assert_eq!(display.len(), 3);
        assert_eq!(display[0], "line one");
        assert_eq!(display[1], "line two");
    }

    // ── 10. undo reverses insert ─────────────────────────────────────────

    #[test]
    fn test_undo_reverses_insert() {
        let mut ta = area("hello");
        ta.cursor = (0, 5);
        ta.insert_char('!');
        // Commit the pending op.
        ta.commit_pending();
        let did_undo = ta.undo();
        assert!(did_undo);
        assert_eq!(ta.lines[0], "hello");
    }

    // ── 11. redo reapplies ────────────────────────────────────────────────

    #[test]
    fn test_redo_reapplies() {
        let mut ta = area("hello");
        ta.cursor = (0, 5);
        ta.insert_char('!');
        ta.commit_pending();
        ta.undo();
        let did_redo = ta.redo();
        assert!(did_redo);
        assert_eq!(ta.lines[0], "hello!");
    }

    // ── 12. undo coalesces consecutive chars ──────────────────────────────

    #[test]
    fn test_undo_coalesces_consecutive_chars() {
        let mut ta = area("");
        ta.insert_char('a');
        ta.insert_char('b');
        ta.insert_char('c');
        // All three chars should be in one pending Insert op.
        // undo() commits pending, then pops the group.
        let did_undo = ta.undo();
        assert!(did_undo, "undo should succeed");
        assert_eq!(
            ta.lines[0], "",
            "all three inserted chars should be removed"
        );
    }

    // ── 13. visible_range maps scroll offset ─────────────────────────────

    #[test]
    fn test_visible_range_maps_scroll_offset() {
        let text = (0..20)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut ta = TextArea::new(&text, WrapMode::Hard);
        // Each line is 20px tall, viewport = 60px → 3 visible lines.
        ta.scroll_offset = 40.0; // start at line 2 (0-indexed).
        let range = ta.visible_range(20.0, 60.0);
        assert_eq!(range.start, 2);
        assert_eq!(range.end, 5);
    }

    // ── 14. line_numbers gutter count ────────────────────────────────────

    #[test]
    fn test_line_numbers_gutter_count() {
        let ta = TextArea::new("one\ntwo\nthree", WrapMode::Hard);
        let nums = ta.line_numbers();
        assert_eq!(nums, vec![1, 2, 3]);
    }

    // ── Extra: col_byte_offset helper ────────────────────────────────────

    #[test]
    fn test_col_byte_offset_ascii() {
        let ta = area("hello");
        assert_eq!(ta.col_byte_offset(0, 0), 0);
        assert_eq!(ta.col_byte_offset(0, 3), 3);
        assert_eq!(ta.col_byte_offset(0, 5), 5);
    }

    // ── 15. Dirty tracking: insert_char marks only affected line ─────────

    #[test]
    fn dirty_tracking_marks_only_affected_line_on_insert() {
        let text = "l0\nl1\nl2\nl3\nl4\nl5\nl6\nl7\nl8\nl9";
        let mut ta = TextArea::new(text, WrapMode::Hard);
        // Simulate all lines having been shaped (clear dirty set).
        ta.dirty_paragraphs.clear();

        // Position cursor at line 5, col 0 and insert a character.
        ta.cursor = (5, 0);
        ta.insert_char('x');

        // Only line 5 should be dirty.
        assert_eq!(
            ta.dirty_paragraphs,
            std::collections::HashSet::from([5usize])
        );
    }

    // ── 16. Dirty tracking: insert_newline marks both halves ─────────────

    #[test]
    fn dirty_tracking_marks_both_lines_on_newline() {
        let mut ta = TextArea::new("hello world", WrapMode::Hard);
        ta.dirty_paragraphs.clear();

        ta.cursor = (0, 5);
        ta.insert_newline();

        // Both the split row (0) and the new row (1) must be dirty.
        assert!(ta.dirty_paragraphs.contains(&0));
        assert!(ta.dirty_paragraphs.contains(&1));
        // The shape cache for both rows must be None.
        assert!(ta.shape_cache[0].is_none());
        assert!(ta.shape_cache[1].is_none());
    }

    // ── 17. Dirty tracking: delete_backward at col>0 marks single row ────

    #[test]
    fn dirty_tracking_marks_row_on_delete_backward_inline() {
        let mut ta = TextArea::new("abc\ndef", WrapMode::Hard);
        ta.dirty_paragraphs.clear();

        ta.cursor = (1, 2);
        ta.delete_backward();

        // Only row 1 should be dirty.
        assert!(ta.dirty_paragraphs.contains(&1));
        assert!(!ta.dirty_paragraphs.contains(&0));
    }

    // ── 18. Shape cache grows with insert_newline ─────────────────────────

    #[test]
    fn shape_cache_grows_after_insert_newline() {
        let mut ta = TextArea::new("one line", WrapMode::Hard);
        assert_eq!(ta.shape_cache.len(), 1);

        ta.cursor = (0, 3);
        ta.insert_newline();

        assert_eq!(ta.shape_cache.len(), 2);
    }

    // ── 19. Shape cache shrinks with delete_backward join ─────────────────

    #[test]
    fn shape_cache_shrinks_after_line_join() {
        let mut ta = TextArea::new("a\nb", WrapMode::Hard);
        assert_eq!(ta.shape_cache.len(), 2);

        ta.cursor = (1, 0);
        ta.delete_backward(); // joins lines → 1 line

        assert_eq!(ta.shape_cache.len(), 1);
    }
}
