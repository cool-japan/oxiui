//! COOLJAPAN ecosystem integration — table state persistence via `oxicode`.
//!
//! Provides [`TableState`], a serialisable snapshot of the mutable UI state of
//! a [`Table`](crate::table::Table): column widths, column order, active sort,
//! per-column filter text, current page, pinned columns, and zebra-striping
//! flag.
//!
//! `TableState` implements `oxicode::Encode` + `oxicode::Decode` so it can be
//! serialised to the COOLJAPAN binary codec and persisted to disk, sent over
//! the network, or stored in user preferences.
//!
//! # COOLJAPAN policies
//!
//! - **No `bincode`**: all serialisation uses `oxicode` (the COOLJAPAN binary
//!   codec).
//! - **No `zip`/`flate2`/`zstd`**: compressed export must use `oxiarc-*`.
//! - **No CSV crate**: CSV export already uses the manual RFC-4180 builder in
//!   `crate::csv`.
//! - **Pure Rust default features**: the `persist-table` feature gates the
//!   `oxicode` dependency so downstream crates without it pay zero overhead.
//!
//! # Example
//!
//! ```rust
//! # #[cfg(feature = "persist-table")]
//! # {
//! use oxiui_table::persistence::TableState;
//! # use oxicode::{Encode, Decode};
//!
//! let state = TableState {
//!     column_widths: vec![120.0, 80.0, 200.0],
//!     column_order: vec![0, 2, 1],
//!     sort_column: Some(0),
//!     sort_ascending: true,
//!     column_filters: vec!["".into(), "".into(), "Alice".into()],
//!     current_page: 0,
//!     page_size: 25,
//!     pinned_columns: 1,
//!     zebra_striping: true,
//! };
//!
//! let bytes = state.encode_to_vec().expect("encode must not fail");
//! let restored = TableState::decode_from_slice(&bytes).expect("decode must not fail");
//! assert_eq!(state.column_order, restored.column_order);
//! # }
//! ```
//!
//! # Feature flag
//!
//! `TableState::encode_to_vec` and `TableState::decode_from_slice` are only
//! available when the `persist-table` feature is enabled.  The struct and
//! builder conversions always compile.

// ── TableState ────────────────────────────────────────────────────────────────

/// Serialisable snapshot of the mutable UI state of a `Table`.
///
/// Only the *view* state is captured — not the data source itself.  This is
/// deliberately scoped to what a user would expect to have restored between
/// sessions: column layout, sort, filters, pagination, and display flags.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "persist-table", derive(oxicode::Encode, oxicode::Decode))]
pub struct TableState {
    /// Runtime column widths (logical pixels) in logical column order.
    pub column_widths: Vec<f32>,
    /// Column render order: `column_order[i]` is the logical column index at
    /// render position `i`.
    pub column_order: Vec<usize>,
    /// Active sort column index, or `None` for no sort.
    pub sort_column: Option<usize>,
    /// `true` = ascending, `false` = descending.  Ignored when `sort_column`
    /// is `None`.
    pub sort_ascending: bool,
    /// Per-column filter text strings (empty = no filter).  Indexed by logical
    /// column index.
    pub column_filters: Vec<String>,
    /// Current page (0-based) in paginated mode.
    pub current_page: usize,
    /// Rows per page.  `0` disables pagination.
    pub page_size: usize,
    /// Number of leftmost columns pinned (frozen) during horizontal scrolling.
    pub pinned_columns: usize,
    /// Whether alternate rows are rendered with a different background colour.
    pub zebra_striping: bool,
}

impl Default for TableState {
    fn default() -> Self {
        Self {
            column_widths: Vec::new(),
            column_order: Vec::new(),
            sort_column: None,
            sort_ascending: true,
            column_filters: Vec::new(),
            current_page: 0,
            page_size: 50,
            pinned_columns: 0,
            zebra_striping: false,
        }
    }
}

impl TableState {
    /// Create a `TableState` from the current settings of a
    /// [`Table`](crate::table::Table).
    ///
    /// This is a non-generic helper that accepts the individual fields directly
    /// to avoid a trait bound cycle between this module and `table.rs`.
    #[allow(clippy::too_many_arguments)]
    pub fn from_table_fields(
        column_widths: Vec<f32>,
        column_order: Vec<usize>,
        sort_column: Option<usize>,
        sort_ascending: bool,
        column_filters: Vec<String>,
        current_page: usize,
        page_size: usize,
        pinned_columns: usize,
        zebra_striping: bool,
    ) -> Self {
        Self {
            column_widths,
            column_order,
            sort_column,
            sort_ascending,
            column_filters,
            current_page,
            page_size,
            pinned_columns,
            zebra_striping,
        }
    }

    /// Serialise this state to a `Vec<u8>` using `oxicode`.
    ///
    /// # Errors
    ///
    /// Returns a `String` error if encoding fails.
    ///
    /// Only available when the `persist-table` feature is enabled.
    #[cfg(feature = "persist-table")]
    pub fn encode_to_vec(&self) -> Result<Vec<u8>, String> {
        oxicode::encode_to_vec(self).map_err(|e| e.to_string())
    }

    /// Deserialise a `TableState` from a byte slice using `oxicode`.
    ///
    /// # Errors
    ///
    /// Returns a `String` error if the bytes are invalid.
    ///
    /// Only available when the `persist-table` feature is enabled.
    #[cfg(feature = "persist-table")]
    pub fn decode_from_slice(bytes: &[u8]) -> Result<Self, String> {
        let (state, _consumed) =
            oxicode::decode_from_slice::<Self>(bytes).map_err(|e| e.to_string())?;
        Ok(state)
    }
}

// ── Diff / apply helpers ──────────────────────────────────────────────────────

/// The result of comparing two [`TableState`] snapshots.
///
/// Useful for efficiently propagating only the changed fields when restoring
/// state across sessions or syncing distributed views.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TableStateDiff {
    /// New column widths if they changed.
    pub column_widths: Option<Vec<f32>>,
    /// New column order if it changed.
    pub column_order: Option<Vec<usize>>,
    /// New sort column if it changed (or `Some(None)` to clear).
    pub sort_column: Option<Option<usize>>,
    /// New ascending flag if it changed.
    pub sort_ascending: Option<bool>,
    /// New filter strings if any changed.
    pub column_filters: Option<Vec<String>>,
    /// New page number if it changed.
    pub current_page: Option<usize>,
    /// New page size if it changed.
    pub page_size: Option<usize>,
    /// New pinned-column count if it changed.
    pub pinned_columns: Option<usize>,
    /// New zebra-striping flag if it changed.
    pub zebra_striping: Option<bool>,
}

/// Compute the diff from `old` to `new`.
///
/// Only fields that differ are set in the returned [`TableStateDiff`].
pub fn diff(old: &TableState, new: &TableState) -> TableStateDiff {
    TableStateDiff {
        column_widths: if old.column_widths != new.column_widths {
            Some(new.column_widths.clone())
        } else {
            None
        },
        column_order: if old.column_order != new.column_order {
            Some(new.column_order.clone())
        } else {
            None
        },
        sort_column: if old.sort_column != new.sort_column {
            Some(new.sort_column)
        } else {
            None
        },
        sort_ascending: if old.sort_ascending != new.sort_ascending {
            Some(new.sort_ascending)
        } else {
            None
        },
        column_filters: if old.column_filters != new.column_filters {
            Some(new.column_filters.clone())
        } else {
            None
        },
        current_page: if old.current_page != new.current_page {
            Some(new.current_page)
        } else {
            None
        },
        page_size: if old.page_size != new.page_size {
            Some(new.page_size)
        } else {
            None
        },
        pinned_columns: if old.pinned_columns != new.pinned_columns {
            Some(new.pinned_columns)
        } else {
            None
        },
        zebra_striping: if old.zebra_striping != new.zebra_striping {
            Some(new.zebra_striping)
        } else {
            None
        },
    }
}

/// Apply a [`TableStateDiff`] to a [`TableState`], mutating it in place.
///
/// Fields that are `None` in the diff are left unchanged.
pub fn apply_diff(state: &mut TableState, d: &TableStateDiff) {
    if let Some(ref v) = d.column_widths {
        state.column_widths = v.clone();
    }
    if let Some(ref v) = d.column_order {
        state.column_order = v.clone();
    }
    if let Some(v) = d.sort_column {
        state.sort_column = v;
    }
    if let Some(v) = d.sort_ascending {
        state.sort_ascending = v;
    }
    if let Some(ref v) = d.column_filters {
        state.column_filters = v.clone();
    }
    if let Some(v) = d.current_page {
        state.current_page = v;
    }
    if let Some(v) = d.page_size {
        state.page_size = v;
    }
    if let Some(v) = d.pinned_columns {
        state.pinned_columns = v;
    }
    if let Some(v) = d.zebra_striping {
        state.zebra_striping = v;
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_state() -> TableState {
        TableState {
            column_widths: vec![120.0, 80.0, 200.0],
            column_order: vec![0, 2, 1],
            sort_column: Some(0),
            sort_ascending: true,
            column_filters: vec!["".into(), "".into(), "Alice".into()],
            current_page: 2,
            page_size: 25,
            pinned_columns: 1,
            zebra_striping: true,
        }
    }

    #[test]
    fn default_state_has_sensible_values() {
        let state = TableState::default();
        assert!(state.column_widths.is_empty());
        assert!(state.column_order.is_empty());
        assert!(state.sort_column.is_none());
        assert!(state.sort_ascending); // default ascending
        assert_eq!(state.page_size, 50);
        assert!(!state.zebra_striping);
    }

    #[test]
    fn from_table_fields_round_trips() {
        let state = TableState::from_table_fields(
            vec![100.0, 200.0],
            vec![1, 0],
            Some(1),
            false,
            vec!["filter".into(), "".into()],
            3,
            10,
            2,
            true,
        );
        assert_eq!(state.column_widths, vec![100.0, 200.0]);
        assert_eq!(state.column_order, vec![1, 0]);
        assert_eq!(state.sort_column, Some(1));
        assert!(!state.sort_ascending);
        assert_eq!(state.column_filters[0], "filter");
        assert_eq!(state.current_page, 3);
        assert_eq!(state.page_size, 10);
        assert_eq!(state.pinned_columns, 2);
        assert!(state.zebra_striping);
    }

    #[test]
    fn diff_identical_states_is_empty() {
        let a = sample_state();
        let b = a.clone();
        let d = diff(&a, &b);
        assert_eq!(d, TableStateDiff::default());
    }

    #[test]
    fn diff_sort_column_changed() {
        let a = sample_state();
        let mut b = a.clone();
        b.sort_column = Some(2);
        let d = diff(&a, &b);
        assert_eq!(d.sort_column, Some(Some(2)));
        assert!(d.column_widths.is_none(), "column_widths must be unchanged");
    }

    #[test]
    fn diff_column_widths_changed() {
        let a = sample_state();
        let mut b = a.clone();
        b.column_widths = vec![150.0, 80.0, 200.0];
        let d = diff(&a, &b);
        assert_eq!(
            d.column_widths.as_deref(),
            Some(&[150.0_f32, 80.0, 200.0][..])
        );
    }

    #[test]
    fn apply_diff_modifies_state() {
        let mut state = sample_state();
        let d = TableStateDiff {
            sort_column: Some(Some(2)),
            sort_ascending: Some(false),
            zebra_striping: Some(false),
            ..Default::default()
        };
        apply_diff(&mut state, &d);
        assert_eq!(state.sort_column, Some(2));
        assert!(!state.sort_ascending);
        assert!(!state.zebra_striping);
        // Unchanged fields must survive.
        assert_eq!(state.column_order, vec![0, 2, 1]);
    }

    #[test]
    fn apply_diff_none_fields_unchanged() {
        let original = sample_state();
        let mut state = original.clone();
        apply_diff(&mut state, &TableStateDiff::default());
        assert_eq!(state, original);
    }

    #[test]
    fn diff_apply_roundtrip() {
        let old = sample_state();
        let mut new = old.clone();
        new.sort_column = None;
        new.page_size = 100;
        new.zebra_striping = false;

        let d = diff(&old, &new);
        let mut reconstructed = old.clone();
        apply_diff(&mut reconstructed, &d);
        assert_eq!(reconstructed, new);
    }

    #[cfg(feature = "persist-table")]
    #[test]
    fn encode_decode_roundtrip() {
        let state = sample_state();
        let bytes = state.encode_to_vec().expect("encode must succeed");
        assert!(!bytes.is_empty(), "encoded bytes must not be empty");
        let decoded = TableState::decode_from_slice(&bytes).expect("decode must succeed");
        assert_eq!(decoded, state);
    }

    #[cfg(feature = "persist-table")]
    #[test]
    fn encode_decode_default_state() {
        let state = TableState::default();
        let bytes = state.encode_to_vec().expect("encode");
        let decoded = TableState::decode_from_slice(&bytes).expect("decode");
        assert_eq!(decoded, state);
    }

    #[cfg(feature = "persist-table")]
    #[test]
    fn decode_invalid_bytes_returns_err() {
        let result = TableState::decode_from_slice(&[0xFF, 0x00, 0xAB]);
        assert!(result.is_err(), "invalid bytes must return Err");
    }

    #[cfg(feature = "persist-table")]
    #[test]
    fn encode_produces_non_trivial_bytes() {
        let state = sample_state();
        let bytes = state.encode_to_vec().expect("encode");
        // At minimum, 3 f32 column widths × 4 bytes = 12 bytes for widths alone.
        assert!(
            bytes.len() >= 12,
            "encoded bytes too short: {}",
            bytes.len()
        );
    }

    #[test]
    fn diff_clear_sort_column() {
        let mut a = sample_state();
        a.sort_column = Some(0);
        let mut b = a.clone();
        b.sort_column = None;
        let d = diff(&a, &b);
        assert_eq!(
            d.sort_column,
            Some(None),
            "clearing sort should produce Some(None)"
        );
    }

    #[test]
    fn diff_filter_change() {
        let a = sample_state();
        let mut b = a.clone();
        b.column_filters = vec!["new".into(), "".into(), "Alice".into()];
        let d = diff(&a, &b);
        assert!(d.column_filters.is_some());
        assert_eq!(d.column_filters.as_ref().unwrap()[0], "new");
    }
}
