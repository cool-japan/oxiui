//! Row selection model supporting single, multi, and range selection.

use std::collections::HashSet;

/// The selection mode of a table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionMode {
    /// Selection disabled.
    None,
    /// At most one row selected at a time.
    Single,
    /// Any number of rows selectable.
    Multi,
}

/// Tracks the set of selected row indices and an anchor for range selection.
#[derive(Clone, Debug)]
pub struct SelectionModel {
    mode: SelectionMode,
    selected: HashSet<usize>,
    /// Anchor row for shift-range selection (last primary click).
    anchor: Option<usize>,
}

impl SelectionModel {
    /// Create an empty selection model with the given mode.
    pub fn new(mode: SelectionMode) -> Self {
        Self {
            mode,
            selected: HashSet::new(),
            anchor: None,
        }
    }

    /// The selection mode.
    pub fn mode(&self) -> SelectionMode {
        self.mode
    }

    /// Number of selected rows.
    pub fn len(&self) -> usize {
        self.selected.len()
    }

    /// Returns `true` if nothing is selected.
    pub fn is_empty(&self) -> bool {
        self.selected.is_empty()
    }

    /// Returns `true` if row `index` is selected.
    pub fn is_selected(&self, index: usize) -> bool {
        self.selected.contains(&index)
    }

    /// Selected indices in ascending order.
    pub fn selected_sorted(&self) -> Vec<usize> {
        let mut v: Vec<usize> = self.selected.iter().copied().collect();
        v.sort_unstable();
        v
    }

    /// Clear the entire selection.
    pub fn clear(&mut self) {
        self.selected.clear();
        self.anchor = None;
    }

    /// Handle a plain click on `index`: selects only that row (and sets the
    /// range anchor). No-op in [`SelectionMode::None`].
    pub fn click(&mut self, index: usize) {
        if self.mode == SelectionMode::None {
            return;
        }
        self.selected.clear();
        self.selected.insert(index);
        self.anchor = Some(index);
    }

    /// Handle a ctrl/cmd-click on `index`: toggles that row's membership without
    /// affecting others. In [`SelectionMode::Single`] it behaves like a plain
    /// click; in [`SelectionMode::None`] it is a no-op.
    pub fn ctrl_click(&mut self, index: usize) {
        match self.mode {
            SelectionMode::None => {}
            SelectionMode::Single => self.click(index),
            SelectionMode::Multi => {
                if !self.selected.remove(&index) {
                    self.selected.insert(index);
                }
                self.anchor = Some(index);
            }
        }
    }

    /// Handle a shift-click on `index`: selects the contiguous range from the
    /// current anchor to `index` (inclusive). Falls back to a plain click if
    /// there is no anchor or the mode is not multi.
    pub fn shift_click(&mut self, index: usize) {
        match (self.mode, self.anchor) {
            (SelectionMode::Multi, Some(anchor)) => {
                let (lo, hi) = if anchor <= index {
                    (anchor, index)
                } else {
                    (index, anchor)
                };
                self.selected.clear();
                for i in lo..=hi {
                    self.selected.insert(i);
                }
                // Anchor stays put for chained shift-clicks.
            }
            (SelectionMode::None, _) => {}
            _ => self.click(index),
        }
    }

    /// Select every row in `0..row_count` (multi mode only).
    pub fn select_all(&mut self, row_count: usize) {
        if self.mode != SelectionMode::Multi {
            return;
        }
        self.selected = (0..row_count).collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_click_replaces() {
        let mut s = SelectionModel::new(SelectionMode::Single);
        s.click(2);
        assert!(s.is_selected(2));
        s.click(5);
        assert!(s.is_selected(5));
        assert!(!s.is_selected(2));
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn ctrl_click_toggles_in_multi() {
        let mut s = SelectionModel::new(SelectionMode::Multi);
        s.click(1);
        s.ctrl_click(3);
        s.ctrl_click(5);
        assert_eq!(s.selected_sorted(), vec![1, 3, 5]);
        // Toggle 3 off.
        s.ctrl_click(3);
        assert_eq!(s.selected_sorted(), vec![1, 5]);
    }

    #[test]
    fn shift_click_selects_range() {
        let mut s = SelectionModel::new(SelectionMode::Multi);
        s.click(2); // anchor
        s.shift_click(5);
        assert_eq!(s.selected_sorted(), vec![2, 3, 4, 5]);
        // Shift the other direction from the same anchor.
        s.shift_click(0);
        assert_eq!(s.selected_sorted(), vec![0, 1, 2]);
    }

    #[test]
    fn select_all_and_clear() {
        let mut s = SelectionModel::new(SelectionMode::Multi);
        s.select_all(4);
        assert_eq!(s.len(), 4);
        s.clear();
        assert!(s.is_empty());
    }

    #[test]
    fn none_mode_is_inert() {
        let mut s = SelectionModel::new(SelectionMode::None);
        s.click(1);
        s.ctrl_click(2);
        s.shift_click(3);
        s.select_all(10);
        assert!(s.is_empty());
    }

    #[test]
    fn single_mode_ctrl_click_behaves_like_click() {
        let mut s = SelectionModel::new(SelectionMode::Single);
        s.ctrl_click(2);
        s.ctrl_click(4);
        assert_eq!(s.selected_sorted(), vec![4]);
    }
}
