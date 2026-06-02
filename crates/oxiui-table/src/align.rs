//! Per-column cell alignment.

use crate::Cell;

/// Horizontal alignment of a cell's content within its column.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CellAlign {
    /// Align content to the left (default for text).
    #[default]
    Left,
    /// Centre content horizontally.
    Center,
    /// Align content to the right (default for numeric cells).
    Right,
}

impl CellAlign {
    /// The conventional default alignment for a given cell: numbers are
    /// right-aligned, booleans centred, everything else left-aligned.
    pub fn default_for(cell: &Cell) -> CellAlign {
        match cell {
            Cell::Int(_) | Cell::Float(_) => CellAlign::Right,
            Cell::Bool(_) => CellAlign::Center,
            _ => CellAlign::Left,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_defaults_right() {
        assert_eq!(CellAlign::default_for(&Cell::Int(1)), CellAlign::Right);
        assert_eq!(CellAlign::default_for(&Cell::Float(1.5)), CellAlign::Right);
    }

    #[test]
    fn bool_defaults_center() {
        assert_eq!(CellAlign::default_for(&Cell::Bool(true)), CellAlign::Center);
    }

    #[test]
    fn text_and_empty_default_left() {
        assert_eq!(CellAlign::default_for(&Cell::from("x")), CellAlign::Left);
        assert_eq!(CellAlign::default_for(&Cell::Empty), CellAlign::Left);
        assert_eq!(CellAlign::default(), CellAlign::Left);
    }
}
