//! Keyboard navigation state for the table widget.
//!
//! [`TableNav`] tracks the currently active (focused) cell position and
//! provides pure movement methods that clamp to valid bounds.  None of the
//! methods directly mutate the `RowSource` or interact with the renderer;
//! they are pure state transitions that callers apply to the UI each frame.

/// Keyboard navigation state — tracks the active (focused) cell.
///
/// Row and column indices here are **visible** indices (after filter / sort),
/// not source indices.  The caller is responsible for converting to source
/// indices when fetching cell data.
#[derive(Debug, Clone, Default)]
pub struct TableNav {
    /// The currently focused row in the visible (filtered/sorted) row set.
    pub active_row: usize,
    /// The currently focused column in the visible column order.
    pub active_col: usize,
}

impl TableNav {
    /// Create a new [`TableNav`] positioned at the top-left cell.
    pub fn new() -> Self {
        Self::default()
    }

    /// Move focus one row up.  Returns `true` if the position changed.
    ///
    /// Clamps at row 0; no-op when `total_rows` is zero.
    pub fn move_up(&mut self, total_rows: usize) -> bool {
        if total_rows == 0 {
            return false;
        }
        if self.active_row > 0 {
            self.active_row -= 1;
            true
        } else {
            false
        }
    }

    /// Move focus one row down.  Returns `true` if the position changed.
    ///
    /// Clamps at `total_rows - 1`; no-op when `total_rows` is zero.
    pub fn move_down(&mut self, total_rows: usize) -> bool {
        if total_rows == 0 {
            return false;
        }
        if self.active_row + 1 < total_rows {
            self.active_row += 1;
            true
        } else {
            false
        }
    }

    /// Move focus one column left.  Returns `true` if the position changed.
    ///
    /// Clamps at column 0; no-op when `total_cols` is zero.
    pub fn move_left(&mut self, total_cols: usize) -> bool {
        if total_cols == 0 {
            return false;
        }
        if self.active_col > 0 {
            self.active_col -= 1;
            true
        } else {
            false
        }
    }

    /// Move focus one column right.  Returns `true` if the position changed.
    ///
    /// Clamps at `total_cols - 1`; no-op when `total_cols` is zero.
    pub fn move_right(&mut self, total_cols: usize) -> bool {
        if total_cols == 0 {
            return false;
        }
        if self.active_col + 1 < total_cols {
            self.active_col += 1;
            true
        } else {
            false
        }
    }

    /// Move focus to the first visible row.  Returns `true` if the position changed.
    pub fn move_home_row(&mut self) -> bool {
        if self.active_row != 0 {
            self.active_row = 0;
            true
        } else {
            false
        }
    }

    /// Move focus to the last visible row.  Returns `true` if the position changed.
    ///
    /// No-op when `total_rows` is zero.
    pub fn move_end_row(&mut self, total_rows: usize) -> bool {
        let last = total_rows.saturating_sub(1);
        if self.active_row != last {
            self.active_row = last;
            true
        } else {
            false
        }
    }

    /// Scroll focus up by `page_size` rows (Page Up).
    ///
    /// Clamps at row 0.  Returns `true` if the position changed.
    pub fn page_up(&mut self, page_size: usize) -> bool {
        let new_row = self.active_row.saturating_sub(page_size);
        if new_row != self.active_row {
            self.active_row = new_row;
            true
        } else {
            false
        }
    }

    /// Scroll focus down by `page_size` rows (Page Down).
    ///
    /// Clamps at `total_rows - 1`.  Returns `true` if the position changed.
    /// No-op when `total_rows` is zero.
    pub fn page_down(&mut self, total_rows: usize, page_size: usize) -> bool {
        if total_rows == 0 {
            return false;
        }
        let new_row = (self.active_row + page_size).min(total_rows.saturating_sub(1));
        if new_row != self.active_row {
            self.active_row = new_row;
            true
        } else {
            false
        }
    }

    /// Set the active cell position explicitly (e.g. on mouse click).
    pub fn set_position(&mut self, row: usize, col: usize) {
        self.active_row = row;
        self.active_col = col;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_top_left() {
        let nav = TableNav::new();
        assert_eq!(nav.active_row, 0);
        assert_eq!(nav.active_col, 0);
    }

    #[test]
    fn move_down_increments_row() {
        let mut nav = TableNav::new();
        assert!(nav.move_down(5));
        assert_eq!(nav.active_row, 1);
    }

    #[test]
    fn move_down_clamps_at_last_row() {
        let mut nav = TableNav::new();
        nav.active_row = 4;
        assert!(!nav.move_down(5)); // row 4 is the last (5 total)
        assert_eq!(nav.active_row, 4);
    }

    #[test]
    fn move_up_clamps_at_zero() {
        let mut nav = TableNav::new();
        assert!(!nav.move_up(5));
        assert_eq!(nav.active_row, 0);
    }

    #[test]
    fn move_left_right() {
        let mut nav = TableNav::new();
        assert!(!nav.move_left(3));
        assert!(nav.move_right(3));
        assert_eq!(nav.active_col, 1);
        nav.active_col = 2;
        assert!(!nav.move_right(3));
        assert_eq!(nav.active_col, 2);
    }

    #[test]
    fn page_up_down() {
        let mut nav = TableNav::new();
        nav.active_row = 10;
        assert!(nav.page_up(4));
        assert_eq!(nav.active_row, 6);
        assert!(nav.page_down(20, 4));
        assert_eq!(nav.active_row, 10);
        // Page down past end clamps.
        assert!(nav.page_down(12, 100));
        assert_eq!(nav.active_row, 11);
    }

    #[test]
    fn home_end() {
        let mut nav = TableNav::new();
        nav.active_row = 7;
        assert!(nav.move_home_row());
        assert_eq!(nav.active_row, 0);
        // Already at home — no change.
        assert!(!nav.move_home_row());

        assert!(nav.move_end_row(10));
        assert_eq!(nav.active_row, 9);
        // Already at end — no change.
        assert!(!nav.move_end_row(10));
    }

    #[test]
    fn no_op_on_zero_rows() {
        let mut nav = TableNav::new();
        assert!(!nav.move_up(0));
        assert!(!nav.move_down(0));
        assert!(!nav.page_down(0, 5));
        assert!(!nav.move_end_row(0));
        assert_eq!(nav.active_row, 0);
    }

    #[test]
    fn set_position_updates() {
        let mut nav = TableNav::new();
        nav.set_position(5, 3);
        assert_eq!(nav.active_row, 5);
        assert_eq!(nav.active_col, 3);
    }
}
