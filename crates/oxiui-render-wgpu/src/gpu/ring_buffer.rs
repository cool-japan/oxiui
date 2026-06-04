//! GPU upload ring buffer for streaming vertex/index data.
//!
//! [`RingBuffer`] maintains a single large `VERTEX | COPY_DST` GPU buffer and
//! a write cursor that advances by `align_up(size, alignment)` on each
//! allocation.  When the cursor would overflow the buffer capacity the entire
//! buffer is reset to offset 0 (a "ring" wrap).
//!
//! # Design
//!
//! This avoids the per-frame `create_buffer_init` / `create_buffer` allocations
//! that otherwise show up in GPU driver heap statistics.  Instead, the caller
//! obtains a [`RingAllocation`] describing a byte range within the buffer, and
//! uploads data via `queue.write_buffer`.  The GPU reads from the same buffer
//! in the same frame — because wgpu submits command encoders sequentially,
//! `write_buffer` is guaranteed to be visible before any draw commands issued
//! after the write.
//!
//! # Safety / correctness contract
//!
//! - Allocations are *frame-scoped*: all allocations from a frame must be
//!   consumed (drawn) within that frame's command encoder before the next call
//!   to `reset()`.
//! - `reset()` must be called once per frame *before* any allocations for that
//!   frame.  It does NOT wait for GPU work to finish — the caller is responsible
//!   for ensuring the GPU has consumed the previous frame's commands before
//!   overwriting the buffer (e.g. by submitting and waiting, or by using
//!   double-buffering at the `RingBuffer` level).
//!
//! # Headless / testing
//!
//! The ring buffer wraps a real `wgpu::Buffer`, so tests that need it must
//! acquire a real GPU device.  CPU-only tests can use the `RingBufferStats`
//! type directly without a device.

use oxiui_core::UiError;

// ── RingBufferStats ───────────────────────────────────────────────────────────

/// Lifetime statistics for a [`RingBuffer`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RingBufferStats {
    /// Number of successful allocations since the buffer was created.
    pub total_allocations: u64,
    /// Number of ring wraps (full-buffer resets) performed.
    pub wrap_count: u64,
    /// Number of times the buffer was grown to accommodate a large allocation.
    pub grow_count: u64,
    /// Current byte capacity of the underlying GPU buffer.
    pub capacity_bytes: usize,
    /// Current write cursor offset (bytes from start of buffer).
    pub cursor_bytes: usize,
}

// ── RingAllocation ────────────────────────────────────────────────────────────

/// A sub-range allocation within a [`RingBuffer`].
///
/// The caller uploads data via `queue.write_buffer(buf, alloc.offset, bytes)`
/// and then uses `buf.slice(alloc.offset..alloc.offset + alloc.size)` in the
/// render pass.
#[derive(Clone, Copy, Debug)]
pub struct RingAllocation {
    /// Byte offset from the start of the ring buffer.
    pub offset: u64,
    /// Byte size of the allocation (equal to the requested size, *not* the
    /// aligned stride).
    pub size: u64,
}

// ── RingBuffer ────────────────────────────────────────────────────────────────

/// A streaming GPU vertex/index ring buffer.
///
/// Holds a single `VERTEX | INDEX | COPY_DST` GPU buffer; sub-ranges are
/// handed out sequentially and the cursor wraps back to zero at the end of
/// each frame (or when the remaining space is insufficient for an allocation).
pub struct RingBuffer {
    /// The underlying GPU buffer.
    pub buffer: wgpu::Buffer,
    /// Current write cursor (byte offset from start of buffer).
    cursor: usize,
    /// Alignment requirement for each allocation (typically 4 bytes for
    /// `VERTEX` buffers; use `device.limits().min_uniform_buffer_offset_alignment`
    /// for uniform buffers).
    alignment: u64,
    /// Lifetime statistics.
    stats: RingBufferStats,
}

impl RingBuffer {
    /// Minimum initial buffer capacity in bytes.
    const MIN_CAPACITY: usize = 64 * 1024; // 64 KiB

    /// Create a new ring buffer with an initial capacity of
    /// `max(initial_bytes, MIN_CAPACITY)` bytes.
    ///
    /// `alignment` is the byte alignment applied to every allocation.
    /// For vertex buffers 4 is typical; for uniform buffers use
    /// `device.limits().min_uniform_buffer_offset_alignment`.
    pub fn new(device: &wgpu::Device, initial_bytes: usize, alignment: u64) -> Self {
        let capacity = initial_bytes.max(Self::MIN_CAPACITY).next_power_of_two();
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("oxiui-render-wgpu ring buffer"),
            size: capacity as u64,
            usage: wgpu::BufferUsages::VERTEX
                | wgpu::BufferUsages::INDEX
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let stats = RingBufferStats {
            capacity_bytes: capacity,
            ..Default::default()
        };
        Self {
            buffer,
            cursor: 0,
            alignment: alignment.max(1),
            stats,
        }
    }

    /// Reset the write cursor to zero.
    ///
    /// Must be called once per frame **before** any allocations for that frame.
    /// Does NOT wait for the GPU — the caller must ensure the previous frame's
    /// GPU work has completed before calling `reset()`.
    pub fn reset(&mut self) {
        self.cursor = 0;
    }

    /// Allocate `size` bytes from the ring buffer and upload `data` into the
    /// allocation via `queue.write_buffer`.
    ///
    /// Returns a [`RingAllocation`] describing the offset and size within
    /// `self.buffer`.
    ///
    /// # Wrapping
    ///
    /// If the remaining capacity after the cursor is insufficient, the cursor
    /// wraps to zero (one wrap per frame is normal; multiple wraps in a single
    /// frame indicate the buffer is undersized — consider calling `grow`).
    ///
    /// # Growing
    ///
    /// If even a fresh buffer at offset 0 cannot fit the requested `size` the
    /// buffer is automatically grown to `max(capacity * 2, align_up(size))`
    /// and `grow_count` is incremented.
    ///
    /// # Errors
    ///
    /// Returns [`UiError::Render`] only if the allocation remains impossible
    /// after an attempted grow (e.g. device OOM).  In practice this should
    /// not occur for reasonable data sizes.
    pub fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        data: &[u8],
    ) -> Result<RingAllocation, UiError> {
        let size = data.len();
        if size == 0 {
            return Ok(RingAllocation { offset: 0, size: 0 });
        }

        let aligned_size = align_up(size as u64, self.alignment) as usize;

        // Check whether the remaining tail fits.
        if self.cursor + aligned_size > self.stats.capacity_bytes {
            // Wrap back to zero.
            self.cursor = 0;
            self.stats.wrap_count += 1;
        }

        // Grow if even the full buffer is too small.
        if aligned_size > self.stats.capacity_bytes {
            self.grow(device, aligned_size)?;
        }

        let offset = self.cursor as u64;
        queue.write_buffer(&self.buffer, offset, data);
        self.cursor += aligned_size;
        self.stats.total_allocations += 1;
        self.stats.cursor_bytes = self.cursor;

        Ok(RingAllocation {
            offset,
            size: size as u64,
        })
    }

    /// Explicitly grow the ring buffer to at least `min_size` bytes.
    ///
    /// The new capacity is `max(capacity * 2, next_power_of_two(min_size))`.
    /// The cursor is reset to zero after a grow.
    ///
    /// # Errors
    ///
    /// Returns [`UiError::Render`] on failure (typically OOM).
    pub fn grow(&mut self, device: &wgpu::Device, min_size: usize) -> Result<(), UiError> {
        let new_cap = (self.stats.capacity_bytes * 2)
            .max(min_size.next_power_of_two())
            .max(Self::MIN_CAPACITY);
        let new_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("oxiui-render-wgpu ring buffer (grown)"),
            size: new_cap as u64,
            usage: wgpu::BufferUsages::VERTEX
                | wgpu::BufferUsages::INDEX
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // Replace the buffer and reset the cursor.
        self.buffer = new_buf;
        self.cursor = 0;
        self.stats.capacity_bytes = new_cap;
        self.stats.grow_count += 1;
        self.stats.cursor_bytes = 0;
        Ok(())
    }

    /// Return a snapshot of the ring buffer's lifetime statistics.
    pub fn stats(&self) -> RingBufferStats {
        let mut s = self.stats;
        s.cursor_bytes = self.cursor;
        s
    }

    /// Current byte capacity of the underlying GPU buffer.
    pub fn capacity(&self) -> usize {
        self.stats.capacity_bytes
    }

    /// Current write cursor offset in bytes.
    pub fn cursor(&self) -> usize {
        self.cursor
    }
}

// ── Private helpers ───────────────────────────────────────────────────────────

/// Round `n` up to the next multiple of `align` (which must be ≥ 1).
#[inline]
fn align_up(n: u64, align: u64) -> u64 {
    let a = align.max(1);
    n.div_ceil(a) * a
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // Unit tests for the align_up helper (no GPU needed).
    #[test]
    fn align_up_rounds_correctly() {
        assert_eq!(align_up(0, 4), 0);
        assert_eq!(align_up(1, 4), 4);
        assert_eq!(align_up(4, 4), 4);
        assert_eq!(align_up(5, 4), 8);
        assert_eq!(align_up(256, 256), 256);
        assert_eq!(align_up(257, 256), 512);
    }

    #[test]
    fn ring_buffer_stats_default() {
        let s = RingBufferStats::default();
        assert_eq!(s.total_allocations, 0);
        assert_eq!(s.wrap_count, 0);
        assert_eq!(s.grow_count, 0);
    }

    #[test]
    fn ring_allocation_size_preserved() {
        let alloc = RingAllocation {
            offset: 128,
            size: 56,
        };
        assert_eq!(alloc.offset, 128);
        assert_eq!(alloc.size, 56);
    }
}
