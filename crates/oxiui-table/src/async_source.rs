//! Async data source support for OxiUI table.
//!
//! [`AsyncRowSource`] is a `Send + Sync` trait analogous to [`RowSource`] but
//! with an `async fn row()` signature.  This enables remote / IO-bound data
//! sources (databases, REST APIs, file parsing) without blocking the UI thread.
//!
//! A built-in [`PrefetchBuffer`] wraps any `AsyncRowSource` and prefetches
//! rows near the current viewport, serving cached rows synchronously once
//! fetched.

use std::{
    collections::{HashMap, VecDeque},
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};

use crate::{Cell, ColumnDef, RowSource, TableError, DEFAULT_ROW_HEIGHT};

/// A boxed, send-able future.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

// ── AsyncRowSource ────────────────────────────────────────────────────────────

/// Asynchronous variant of [`RowSource`] for IO-bound data backends.
///
/// Implement this trait for data sources that cannot materialise rows
/// synchronously (e.g. remote databases, networked APIs, lazy disk reads).
///
/// # Usage
///
/// Wrap an `AsyncRowSource` in a [`PrefetchBuffer`] to obtain a synchronous
/// [`RowSource`] that serves rows from cache while background prefetch
/// keeps the buffer warm.
///
/// # Thread safety
///
/// Implementors are only required to be `Send`.  The [`PrefetchBuffer`]
/// wraps the source in an `Arc` and accesses it exclusively from the async
/// prefetch path, which runs on a single task at a time.
pub trait AsyncRowSource: Send {
    /// Total number of rows available in the source.
    ///
    /// This count is used to size scroll bars and paginate navigation.
    fn row_count(&self) -> usize;

    /// Return the column definitions for this source.
    fn column_defs(&self) -> &[ColumnDef];

    /// Asynchronously fetch the cells for the row at `index`.
    ///
    /// Implementations must be cancel-safe.  Large in-flight requests may be
    /// cancelled when the viewport moves to a different range.
    fn row_async(&self, index: usize) -> BoxFuture<'_, Result<Vec<Cell>, TableError>>;

    /// Per-row height in logical pixels.
    ///
    /// Defaults to [`DEFAULT_ROW_HEIGHT`] (24 px) for every row.
    fn row_height(&self, _index: usize) -> f32 {
        DEFAULT_ROW_HEIGHT
    }

    /// Optional footer row (aggregate / summary).
    ///
    /// Fetched once and cached; invalidate by rebuilding the `PrefetchBuffer`.
    fn footer_async(&self) -> BoxFuture<'_, Option<Vec<Cell>>> {
        Box::pin(async { None })
    }
}

// ── PrefetchBufferInner ───────────────────────────────────────────────────────

/// Internal mutable state of a [`PrefetchBuffer`].
#[derive(Default)]
struct PrefetchBufferInner {
    /// Cached rows keyed by row index.
    cache: HashMap<usize, Vec<Cell>>,
    /// Maximum number of cached rows (LRU eviction).
    max_rows: usize,
    /// Eviction queue tracking LRU order (oldest at front).
    lru: VecDeque<usize>,
    /// Pending prefetch request indices.
    pending: Vec<usize>,
    /// Cached footer row, if any.
    footer: Option<Vec<Cell>>,
}

impl PrefetchBufferInner {
    fn new(max_rows: usize) -> Self {
        PrefetchBufferInner {
            max_rows,
            ..Default::default()
        }
    }

    /// Insert a row into the cache, evicting the LRU entry if at capacity.
    fn insert(&mut self, index: usize, cells: Vec<Cell>) {
        if self.cache.contains_key(&index) {
            // Move to front of LRU by removing and re-inserting.
            self.lru.retain(|&i| i != index);
        } else if self.cache.len() >= self.max_rows {
            // Evict the least-recently-used row.
            if let Some(evict) = self.lru.pop_front() {
                self.cache.remove(&evict);
            }
        }
        self.cache.insert(index, cells);
        self.lru.push_back(index);
    }

    /// Get a cached row, promoting it in LRU order.
    fn get(&mut self, index: usize) -> Option<&Vec<Cell>> {
        if self.cache.contains_key(&index) {
            // Promote to MRU.
            self.lru.retain(|&i| i != index);
            self.lru.push_back(index);
            self.cache.get(&index)
        } else {
            None
        }
    }

    /// Mark `indices` as pending prefetch (deduplicates).
    fn enqueue_prefetch(&mut self, indices: impl IntoIterator<Item = usize>) {
        for i in indices {
            if !self.cache.contains_key(&i) && !self.pending.contains(&i) {
                self.pending.push(i);
            }
        }
    }

    /// Drain the list of pending prefetch indices.
    fn drain_pending(&mut self) -> Vec<usize> {
        std::mem::take(&mut self.pending)
    }

    /// Number of cached rows.
    fn len(&self) -> usize {
        self.cache.len()
    }

    /// True if the given index is cached.
    fn is_cached(&self, index: usize) -> bool {
        self.cache.contains_key(&index)
    }

    /// Invalidate the entire cache (e.g. on source mutation).
    fn invalidate(&mut self) {
        self.cache.clear();
        self.lru.clear();
        self.pending.clear();
        self.footer = None;
    }
}

// ── PrefetchBuffer ────────────────────────────────────────────────────────────

/// Wraps an [`AsyncRowSource`] and provides a synchronous [`RowSource`] view.
///
/// The buffer caches fetched rows in an LRU cache (capacity `max_rows`).
/// On a cache miss it returns a placeholder row of [`Cell::Empty`] values and
/// enqueues the row index for background prefetch.  Callers are expected to
/// drive the prefetch loop by calling [`flush_pending`](PrefetchBuffer::flush_pending)
/// in an async context (e.g. a Tokio task or a `wasm_bindgen_futures::spawn_local`
/// closure).
///
/// # Placeholder row
///
/// A cache miss returns `N` [`Cell::Empty`] cells where `N` is
/// `column_defs().len()`.  Renderers may style such cells as loading
/// placeholders (spinner, shimmer, etc.).
///
/// # Thread safety
///
/// The inner state is wrapped in `Arc<Mutex<_>>` so `PrefetchBuffer` is
/// `Send + Sync` and can be shared across threads or with async runtimes.
pub struct PrefetchBuffer<S: AsyncRowSource> {
    source: Arc<S>,
    inner: Arc<Mutex<PrefetchBufferInner>>,
    prefetch_ahead: usize,
}

// Manual Clone impl: Arc handles the S without requiring S: Clone.
impl<S: AsyncRowSource> Clone for PrefetchBuffer<S> {
    fn clone(&self) -> Self {
        PrefetchBuffer {
            source: self.source.clone(),
            inner: self.inner.clone(),
            prefetch_ahead: self.prefetch_ahead,
        }
    }
}

impl<S: AsyncRowSource> PrefetchBuffer<S> {
    /// Create a new buffer wrapping `source`.
    ///
    /// - `max_rows`: maximum number of rows to hold in the LRU cache.
    /// - `prefetch_ahead`: number of rows ahead of the viewport to enqueue for
    ///   prefetch when [`request_prefetch`](PrefetchBuffer::request_prefetch) is
    ///   called.
    pub fn new(source: S, max_rows: usize, prefetch_ahead: usize) -> Self {
        PrefetchBuffer {
            source: Arc::new(source),
            inner: Arc::new(Mutex::new(PrefetchBufferInner::new(max_rows))),
            prefetch_ahead,
        }
    }

    /// Request that rows `[start, start + viewport_rows + prefetch_ahead)` be
    /// prefetched.
    ///
    /// Non-cached rows are enqueued internally; call
    /// [`flush_pending`](PrefetchBuffer::flush_pending) in an async context to
    /// actually perform the fetches.
    pub fn request_prefetch(&self, start: usize, viewport_rows: usize) {
        let end = (start + viewport_rows + self.prefetch_ahead).min(self.source.row_count());
        if let Ok(mut inner) = self.inner.lock() {
            inner.enqueue_prefetch(start..end);
        }
    }

    /// Drive the prefetch loop, fetching all pending rows and storing them in
    /// the cache.
    ///
    /// Should be called from an async context (e.g. a spawned task).  Returns
    /// the number of rows successfully fetched in this call.
    pub async fn flush_pending(&self) -> usize {
        let pending = self
            .inner
            .lock()
            .map(|mut g| g.drain_pending())
            .unwrap_or_default();

        let mut fetched = 0usize;
        for idx in pending {
            match self.source.row_async(idx).await {
                Ok(cells) => {
                    if let Ok(mut inner) = self.inner.lock() {
                        inner.insert(idx, cells);
                        fetched += 1;
                    }
                }
                Err(_) => {
                    // Silently skip failed fetches; they will be retried on
                    // the next `request_prefetch` + `flush_pending` cycle.
                }
            }
        }
        fetched
    }

    /// Store a single already-fetched row directly into the cache.
    ///
    /// Useful for callers that manage their own prefetch executor.
    pub fn store_row(&self, index: usize, cells: Vec<Cell>) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.insert(index, cells);
        }
    }

    /// Invalidate the entire cache (e.g. after a source mutation).
    ///
    /// The next call to [`RowSource::row`] will trigger fresh prefetch requests
    /// for every accessed row.
    pub fn invalidate(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.invalidate();
        }
    }

    /// Number of rows currently held in the cache.
    pub fn cached_count(&self) -> usize {
        self.inner.lock().map(|g| g.len()).unwrap_or(0)
    }

    /// True if the given row index is in the cache.
    pub fn is_cached(&self, index: usize) -> bool {
        self.inner
            .lock()
            .map(|g| g.is_cached(index))
            .unwrap_or(false)
    }

    /// Access the underlying async source.
    pub fn source(&self) -> &S {
        &self.source
    }
}

impl<S: AsyncRowSource> RowSource for PrefetchBuffer<S> {
    fn row_count(&self) -> usize {
        self.source.row_count()
    }

    fn column_defs(&self) -> &[ColumnDef] {
        self.source.column_defs()
    }

    fn row(&self, index: usize) -> Vec<Cell> {
        if let Ok(mut inner) = self.inner.lock() {
            if let Some(row) = inner.get(index) {
                return row.clone();
            }
            // Cache miss — enqueue for prefetch and return placeholders.
            inner.enqueue_prefetch(std::iter::once(index));
        }
        // Return an empty-cell placeholder row.
        let ncols = self.source.column_defs().len();
        vec![Cell::Empty; ncols.max(1)]
    }

    fn row_height(&self, index: usize) -> f32 {
        self.source.row_height(index)
    }

    fn footer(&self) -> Option<Vec<Cell>> {
        self.inner.lock().ok()?.footer.clone()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ColumnDef;

    // A simple synchronous source for testing (simulates an instant async source).
    struct InMemoryAsync {
        rows: Vec<Vec<Cell>>,
        cols: Vec<ColumnDef>,
    }

    impl InMemoryAsync {
        fn new(n: usize) -> Self {
            use crate::ColumnDefBuilder;
            let cols = vec![
                ColumnDefBuilder::new("id").width(60.0).build(),
                ColumnDefBuilder::new("value").width(120.0).build(),
            ];
            let rows = (0..n)
                .map(|i| vec![Cell::Int(i as i64), Cell::Text(format!("row-{i}"))])
                .collect();
            InMemoryAsync { rows, cols }
        }
    }

    impl AsyncRowSource for InMemoryAsync {
        fn row_count(&self) -> usize {
            self.rows.len()
        }

        fn column_defs(&self) -> &[ColumnDef] {
            &self.cols
        }

        fn row_async(&self, index: usize) -> BoxFuture<'_, Result<Vec<Cell>, TableError>> {
            let result = if index < self.rows.len() {
                Ok(self.rows[index].clone())
            } else {
                Err(TableError::OutOfBounds { row: index, col: 0 })
            };
            Box::pin(async move { result })
        }
    }

    // Use pollster (Pure Rust) as the test executor — no unsafe, no extra dep
    // beyond what the workspace already carries.
    use pollster::block_on;

    #[test]
    fn async_source_row_count() {
        let src = InMemoryAsync::new(100);
        assert_eq!(src.row_count(), 100);
    }

    #[test]
    fn async_source_row_async_returns_correct_cells() {
        let src = InMemoryAsync::new(5);
        let row = block_on(src.row_async(2)).expect("row ok");
        assert!(matches!(row[0], Cell::Int(2)));
        assert!(matches!(&row[1], Cell::Text(s) if s == "row-2"));
    }

    #[test]
    fn async_source_out_of_bounds() {
        let src = InMemoryAsync::new(3);
        let err = block_on(src.row_async(10)).expect_err("should be err");
        assert!(matches!(err, TableError::OutOfBounds { row: 10, .. }));
    }

    #[test]
    fn prefetch_buffer_cache_miss_returns_placeholder() {
        let buf = PrefetchBuffer::new(InMemoryAsync::new(50), 32, 4);
        // Row is not in cache yet; should return placeholders.
        let row = buf.row(0);
        // 2 columns → 2 empty cells.
        assert_eq!(row.len(), 2);
        for cell in &row {
            assert!(matches!(cell, Cell::Empty));
        }
        assert!(!buf.is_cached(0));
    }

    #[test]
    fn prefetch_buffer_store_and_retrieve_row() {
        let buf = PrefetchBuffer::new(InMemoryAsync::new(50), 32, 4);
        buf.store_row(5, vec![Cell::Int(5), Cell::Text("row-5".to_string())]);
        assert!(buf.is_cached(5));
        let row = buf.row(5);
        assert!(matches!(row[0], Cell::Int(5)));
    }

    #[test]
    fn prefetch_buffer_flush_pending_fetches_rows() {
        let buf = PrefetchBuffer::new(InMemoryAsync::new(20), 32, 0);
        // Access row 3 — triggers a prefetch request.
        let _ = buf.row(3);
        // Now flush the pending requests.
        let fetched = block_on(buf.flush_pending());
        assert_eq!(fetched, 1);
        assert!(buf.is_cached(3));
        // Next access is a cache hit.
        let row = buf.row(3);
        assert!(matches!(row[0], Cell::Int(3)));
    }

    #[test]
    fn prefetch_buffer_request_prefetch_enqueues_range() {
        let buf = PrefetchBuffer::new(InMemoryAsync::new(100), 64, 5);
        // Request viewport rows 0..10, plus 5 ahead = rows 0..15.
        buf.request_prefetch(0, 10);
        let fetched = block_on(buf.flush_pending());
        // Should have fetched at most 15 rows (capped by row_count).
        assert_eq!(fetched, 15);
        for i in 0..15 {
            assert!(buf.is_cached(i), "row {i} should be cached");
        }
    }

    #[test]
    fn prefetch_buffer_lru_eviction() {
        // Cache holds at most 3 rows.
        let buf = PrefetchBuffer::new(InMemoryAsync::new(10), 3, 0);
        // Load rows 0, 1, 2 — fills cache.
        for i in 0..3_usize {
            buf.store_row(i, vec![Cell::Int(i as i64), Cell::Bool(false)]);
        }
        assert_eq!(buf.cached_count(), 3);
        // Inserting row 3 evicts row 0 (LRU).
        buf.store_row(3, vec![Cell::Int(3), Cell::Bool(false)]);
        assert_eq!(buf.cached_count(), 3);
        assert!(!buf.is_cached(0), "row 0 should be evicted");
        assert!(buf.is_cached(3), "row 3 should be cached");
    }

    #[test]
    fn prefetch_buffer_invalidate_clears_cache() {
        let buf = PrefetchBuffer::new(InMemoryAsync::new(10), 32, 0);
        buf.store_row(0, vec![Cell::Int(0), Cell::Bool(false)]);
        assert!(buf.is_cached(0));
        buf.invalidate();
        assert!(!buf.is_cached(0));
        assert_eq!(buf.cached_count(), 0);
    }

    #[test]
    fn prefetch_buffer_implements_row_source() {
        // Compile-time check: PrefetchBuffer<InMemoryAsync> : RowSource.
        fn assert_row_source<T: RowSource>(_: &T) {}
        let buf = PrefetchBuffer::new(InMemoryAsync::new(5), 32, 0);
        assert_row_source(&buf);
    }

    #[test]
    fn prefetch_buffer_row_count_and_column_defs() {
        let buf = PrefetchBuffer::new(InMemoryAsync::new(42), 32, 0);
        assert_eq!(buf.row_count(), 42);
        assert_eq!(buf.column_defs().len(), 2);
    }

    #[test]
    fn prefetch_buffer_is_clone() {
        // Clone shares the same inner state.
        let buf = PrefetchBuffer::new(InMemoryAsync::new(5), 32, 0);
        buf.store_row(1, vec![Cell::Int(1), Cell::Bool(false)]);
        let buf2 = buf.clone();
        assert!(buf2.is_cached(1));
    }

    #[test]
    fn async_source_default_row_height() {
        let src = InMemoryAsync::new(3);
        assert_eq!(src.row_height(0), DEFAULT_ROW_HEIGHT);
    }
}
