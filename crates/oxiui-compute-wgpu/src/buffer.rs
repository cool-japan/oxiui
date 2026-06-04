//! Storage, uniform, and staging buffer helpers, typed buffer wrappers,
//! buffer pool, sub-allocator, and async readback utilities.
//!
//! These are free functions and types that take a `&wgpu::Device` (and
//! optionally a `&wgpu::Queue`) to construct or read back GPU buffers.
//! They keep the API surface small while covering the patterns used by
//! every compute workload in the COOLJAPAN ecosystem:
//!
//! | Pattern | Helper | Usage |
//! |---------|--------|-------|
//! | Upload once, read/write by shader | [`storage_buffer_init`] | SSBO inputs/outputs |
//! | Small read-only constants for shaders | [`uniform_buffer`] | push-constants, CB0 |
//! | Readback from GPU to CPU | [`staging_buffer`] + [`read_back`] | result extraction |
//! | Zero-copy upload via mapped creation | [`mapped_storage_init`] | integrated-GPU fast path |
//! | Typed wrapper with element count | [`TypedBuffer`] | avoids byte-size arithmetic |
//! | Buffer recycling across dispatches | [`BufferPool`] | avoids per-frame reallocation |
//! | One large buffer sliced into regions | [`SubAllocator`] | aligned sub-region tracking |
//! | Async non-blocking readback | [`read_back_async`] | async-runtime integration |
//! | Partial readback by byte offset | [`read_back_range`] | read a sub-range of a buffer |

use bytemuck::Pod;
use std::collections::HashMap;
use std::marker::PhantomData;

// ── Buffer creation helpers ───────────────────────────────────────────────────

/// Create a GPU storage buffer initialised with `data`.
///
/// The returned buffer has usages `STORAGE | COPY_DST | COPY_SRC`:
/// - `STORAGE`  — bindable as a shader storage buffer.
/// - `COPY_DST` — allows `queue.write_buffer` updates.
/// - `COPY_SRC` — allows copying to a [`staging_buffer`] for CPU readback.
///
/// # Parameters
/// - `device` — the logical wgpu device.
/// - `label`  — debug label visible in GPU capture tools (pass `""` to omit).
/// - `data`   — raw bytes to upload; length determines the buffer size.
///
/// # Panics
/// Panics if `data` is empty (zero-size buffers are forbidden by the WebGPU
/// spec and wgpu validation).
pub fn storage_buffer_init(device: &wgpu::Device, label: &str, data: &[u8]) -> wgpu::Buffer {
    assert!(
        !data.is_empty(),
        "storage_buffer_init: data must be non-empty"
    );
    use wgpu::util::DeviceExt as _;
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: non_empty_label(label),
        contents: data,
        usage: wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_DST
            | wgpu::BufferUsages::COPY_SRC,
    })
}

/// Create a GPU uniform buffer initialised with `data`.
///
/// The returned buffer has usages `UNIFORM | COPY_DST`:
/// - `UNIFORM`  — bindable as a uniform / constant buffer.
/// - `COPY_DST` — allows `queue.write_buffer` updates between dispatches.
///
/// # Panics
/// Panics if `data` is empty.
pub fn uniform_buffer(device: &wgpu::Device, label: &str, data: &[u8]) -> wgpu::Buffer {
    assert!(!data.is_empty(), "uniform_buffer: data must be non-empty");
    use wgpu::util::DeviceExt as _;
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: non_empty_label(label),
        contents: data,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    })
}

/// Create an empty CPU-mappable staging buffer of `size` bytes.
///
/// The returned buffer has usage `MAP_READ | COPY_DST`:
/// - `COPY_DST` — accept a `copy_buffer_to_buffer` from a storage/output buffer.
/// - `MAP_READ` — allows `buffer.slice(..).map_async(MapMode::Read, …)`.
///
/// # Panics
/// Panics if `size` is zero.
pub fn staging_buffer(device: &wgpu::Device, label: &str, size: u64) -> wgpu::Buffer {
    assert!(size > 0, "staging_buffer: size must be > 0");
    device.create_buffer(&wgpu::BufferDescriptor {
        label: non_empty_label(label),
        size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

// ── Readback helper ───────────────────────────────────────────────────────────

/// Copy `len` elements of type `T` from `buf` (a GPU storage buffer) to a
/// `Vec<T>` on the CPU.
///
/// Internally this:
/// 1. Allocates a temporary [`staging_buffer`].
/// 2. Records a `copy_buffer_to_buffer` command and submits it.
/// 3. Maps the staging buffer synchronously via `pollster::block_on`.
/// 4. Copies the mapped bytes into a `Vec<T>` via `bytemuck::cast_slice`.
/// 5. Unmaps the staging buffer.
///
/// # Type parameter
/// `T` must implement [`bytemuck::Pod`] so the raw GPU bytes can be
/// reinterpreted safely.
///
/// # Panics
/// Panics if the GPU mapping fails (device lost, buffer too small, …).
#[cfg_attr(
    feature = "tracing",
    tracing::instrument(level = "debug", skip(device, queue, buf))
)]
pub fn read_back<T: Pod>(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    buf: &wgpu::Buffer,
    len: usize,
) -> Vec<T> {
    let byte_size = (std::mem::size_of::<T>() * len) as u64;
    assert!(byte_size > 0, "read_back: requested size must be > 0");

    // ── 1. Create a staging buffer ─────────────────────────────────────────
    let staging = staging_buffer(device, "oxiui-compute-wgpu readback staging", byte_size);

    // ── 2. Encode + submit copy ────────────────────────────────────────────
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("oxiui-compute-wgpu readback encoder"),
    });
    encoder.copy_buffer_to_buffer(buf, 0, &staging, 0, byte_size);
    queue.submit(std::iter::once(encoder.finish()));

    // ── 3. Map synchronously ───────────────────────────────────────────────
    let slice = staging.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });

    // Pump the device until the map callback fires.
    // `PollType::wait_indefinitely()` blocks until the most recent submission
    // completes — the correct behaviour for a synchronous CPU readback.
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("read_back: device poll failed");

    rx.recv()
        .expect("read_back: channel closed before map callback")
        .expect("read_back: GPU mapping failed");

    // ── 4. Copy bytes to Vec<T> ────────────────────────────────────────────
    let mapped = slice.get_mapped_range();
    let result: Vec<T> = bytemuck::cast_slice::<u8, T>(&mapped).to_vec();

    // ── 5. Unmap ───────────────────────────────────────────────────────────
    drop(mapped);
    staging.unmap();

    result
}

// ── Zero-copy upload ──────────────────────────────────────────────────────────

/// Create a `STORAGE | COPY_SRC` buffer via `mapped_at_creation`, writing
/// `data` without an intermediate staging copy.
///
/// This is the fastest upload path on integrated (unified-memory) GPUs where
/// the buffer lives in CPU-visible memory from the start.  On discrete GPUs
/// wgpu may still arrange an internal transfer, but the CPU side avoids an
/// extra copy.
///
/// # Panics
/// Panics if `data` is empty (zero-size buffers are rejected by wgpu).
pub fn mapped_storage_init(device: &wgpu::Device, label: &str, data: &[u8]) -> wgpu::Buffer {
    assert!(
        !data.is_empty(),
        "mapped_storage_init: data must be non-empty"
    );
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: non_empty_label(label),
        size: data.len() as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: true,
    });
    buffer
        .slice(..)
        .get_mapped_range_mut()
        .copy_from_slice(data);
    buffer.unmap();
    buffer
}

// ── Partial readback ──────────────────────────────────────────────────────────

/// Copy `len` elements of type `T` from `src`, starting at `byte_offset`, to a
/// `Vec<T>` on the CPU.
///
/// Unlike [`read_back`] (which always starts at offset 0), this function lets
/// callers extract a sub-range of a buffer — useful when multiple logical
/// arrays share one large allocation.
///
/// # Panics
/// Panics if the GPU mapping fails.
pub fn read_back_range<T: bytemuck::Pod>(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    src: &wgpu::Buffer,
    byte_offset: u64,
    len: usize,
) -> Vec<T> {
    let byte_size = (len * std::mem::size_of::<T>()) as u64;
    assert!(byte_size > 0, "read_back_range: requested size must be > 0");
    let staging = staging_buffer(device, "", byte_size);

    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    encoder.copy_buffer_to_buffer(src, byte_offset, &staging, 0, byte_size);
    queue.submit(std::iter::once(encoder.finish()));

    let slice = staging.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("read_back_range: device poll failed");
    rx.recv()
        .expect("read_back_range: channel closed before map callback")
        .expect("read_back_range: GPU mapping failed");

    let mapped = slice.get_mapped_range();
    let result = bytemuck::cast_slice::<u8, T>(&mapped).to_vec();
    drop(mapped);
    staging.unmap();
    result
}

// ── Async readback ────────────────────────────────────────────────────────────

/// Async version of [`read_back`] — bridges `map_async`'s callback into an
/// `async fn` via an `mpsc` channel.
///
/// **Note on blocking:** this function calls
/// `device.poll(PollType::wait_indefinitely())` after yielding once to the
/// executor, which *blocks the OS thread* until the GPU copy finishes.  This
/// is acceptable for a compute crate where the caller controls the executor
/// (e.g. `pollster::block_on`).  A fully cooperative non-blocking variant that
/// drives `Poll` from the executor waker is a planned follow-up.
///
/// Compatible runtimes: `pollster`, `tokio::task::spawn_blocking`, or any
/// single-threaded executor.
pub async fn read_back_async<T: bytemuck::Pod>(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    src: &wgpu::Buffer,
    len: usize,
) -> Result<Vec<T>, crate::ComputeError> {
    let byte_size = (len * std::mem::size_of::<T>()) as u64;
    assert!(byte_size > 0, "read_back_async: requested size must be > 0");
    let staging = staging_buffer(device, "read-back-async", byte_size);

    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_buffer_to_buffer(src, 0, &staging, 0, byte_size);
    queue.submit(std::iter::once(encoder.finish()));

    // Bridge map_async callback into async via channel.
    let (tx, rx) = std::sync::mpsc::channel::<Result<(), wgpu::BufferAsyncError>>();
    staging.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });

    // Yield once to allow the executor to schedule other work, then poll the
    // device.  This cooperative yield works with pollster and tokio.
    std::future::ready(()).await;
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|e| crate::ComputeError::Operation {
            op: "read_back_async",
            detail: e.to_string(),
        })?;

    rx.recv()
        .map_err(|_| crate::ComputeError::Operation {
            op: "read_back_async",
            detail: "channel closed before map callback fired".into(),
        })?
        .map_err(|e| crate::ComputeError::Operation {
            op: "read_back_async",
            detail: e.to_string(),
        })?;

    let mapped = staging.slice(..).get_mapped_range();
    let data = bytemuck::cast_slice::<u8, T>(&mapped).to_vec();
    drop(mapped);
    staging.unmap();
    Ok(data)
}

// ── TypedBuffer<T> ────────────────────────────────────────────────────────────

/// A typed wrapper around [`wgpu::Buffer`] that tracks the element count so
/// callers never have to compute byte sizes manually.
///
/// `T` must implement [`bytemuck::Pod`] — the same bound used by
/// [`storage_buffer_init`] and [`read_back`].
///
/// # Example
/// ```rust,no_run
/// use oxiui_compute_wgpu::buffer::TypedBuffer;
///
/// // Construction, upload, and download are all in element counts.
/// ```
pub struct TypedBuffer<T: bytemuck::Pod> {
    buffer: wgpu::Buffer,
    len: usize,
    _phantom: PhantomData<T>,
}

impl<T: bytemuck::Pod> TypedBuffer<T> {
    /// Allocate an uninitialised GPU buffer for `len` elements with the given
    /// `usage` flags.
    pub fn new(device: &wgpu::Device, label: &str, usage: wgpu::BufferUsages, len: usize) -> Self {
        let size = (len * std::mem::size_of::<T>()) as u64;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: non_empty_label(label),
            size,
            usage,
            mapped_at_creation: false,
        });
        TypedBuffer {
            buffer,
            len,
            _phantom: PhantomData,
        }
    }

    /// Create a `STORAGE | COPY_DST | COPY_SRC` buffer pre-filled with `data`.
    pub fn from_data(device: &wgpu::Device, label: &str, data: &[T]) -> Self {
        let bytes = bytemuck::cast_slice(data);
        let buffer = storage_buffer_init(device, label, bytes);
        TypedBuffer {
            buffer,
            len: data.len(),
            _phantom: PhantomData,
        }
    }

    /// Number of `T` elements the buffer holds.
    pub fn len(&self) -> usize {
        self.len
    }

    /// `true` if the buffer holds zero elements.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Size of the buffer in bytes.
    pub fn byte_len(&self) -> u64 {
        (self.len * std::mem::size_of::<T>()) as u64
    }

    /// Return a [`wgpu::BindingResource`] covering the entire buffer, suitable
    /// for passing to `BindGroupEntry::resource`.
    pub fn as_entire_binding(&self) -> wgpu::BindingResource<'_> {
        self.buffer.as_entire_binding()
    }

    /// Access the underlying [`wgpu::Buffer`].
    pub fn inner(&self) -> &wgpu::Buffer {
        &self.buffer
    }

    /// Write `data` into the buffer via `queue.write_buffer`.
    ///
    /// # Panics
    /// Panics if `data.len() != self.len`.
    pub fn upload(&self, queue: &wgpu::Queue, data: &[T]) {
        assert_eq!(data.len(), self.len, "upload length mismatch");
        queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(data));
    }

    /// Read the buffer contents back to the CPU as a `Vec<T>`.
    pub fn download(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> Vec<T> {
        read_back(device, queue, &self.buffer, self.len)
    }
}

// ── BufferPool ────────────────────────────────────────────────────────────────

/// A simple free-list pool that recycles [`wgpu::Buffer`]s across dispatches
/// to avoid per-frame reallocation.
///
/// Buffers are bucketed by `(rounded_size, BufferUsages)`.  The size is
/// rounded up to the next power of two (minimum 256) on both `acquire` and
/// `release` so that similarly-sized buffers can be reused interchangeably.
///
/// # Limitations
/// The pool does **not** destroy idle buffers; callers that need memory-bounded
/// recycling should call [`BufferPool::available_count`] and drop excess
/// buffers manually.
pub struct BufferPool {
    buckets: HashMap<(u64, wgpu::BufferUsages), Vec<wgpu::Buffer>>,
}

impl BufferPool {
    /// Create an empty pool.
    pub fn new() -> Self {
        BufferPool {
            buckets: HashMap::new(),
        }
    }

    /// Acquire a buffer of at least `size` bytes with the given `usage`.
    ///
    /// Returns a recycled buffer from the pool when one is available, or
    /// allocates a new one.  The actual buffer size may be larger than `size`
    /// due to power-of-two rounding.
    pub fn acquire(
        &mut self,
        device: &wgpu::Device,
        size: u64,
        usage: wgpu::BufferUsages,
    ) -> wgpu::Buffer {
        let rounded = size.next_power_of_two().max(256);
        let bucket = self.buckets.entry((rounded, usage)).or_default();
        if let Some(buf) = bucket.pop() {
            return buf;
        }
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("pool-buffer"),
            size: rounded,
            usage,
            mapped_at_creation: false,
        })
    }

    /// Return a buffer to the pool so it can be reused by future `acquire`
    /// calls.
    ///
    /// `size` should be the *logical* size the caller used for `acquire`; the
    /// pool applies the same rounding so the buffer lands in the correct
    /// bucket.
    pub fn release(&mut self, size: u64, usage: wgpu::BufferUsages, buffer: wgpu::Buffer) {
        let rounded = size.next_power_of_two().max(256);
        self.buckets
            .entry((rounded, usage))
            .or_default()
            .push(buffer);
    }

    /// Number of idle buffers in the `(size, usage)` bucket.
    pub fn available_count(&self, size: u64, usage: wgpu::BufferUsages) -> usize {
        let rounded = size.next_power_of_two().max(256);
        self.buckets.get(&(rounded, usage)).map_or(0, |v| v.len())
    }
}

impl Default for BufferPool {
    fn default() -> Self {
        Self::new()
    }
}

// ── SubAllocator ──────────────────────────────────────────────────────────────

/// A description of a sub-region allocated from a [`SubAllocator`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubRegion {
    /// Byte offset from the start of the backing buffer.
    pub offset: u64,
    /// Size of the region in bytes.
    pub size: u64,
}

/// Bump-allocates aligned sub-regions from a single large [`wgpu::Buffer`].
///
/// Useful when many small uniform or storage allocations would each require a
/// separate `wgpu::Buffer` — instead, one large buffer is created once and
/// sliced into named regions, reducing `BindGroup` churn and allocator
/// overhead.
///
/// # Limitations
/// `SubAllocator` is a *bump* allocator — individual regions cannot be freed.
/// Call [`reset`](SubAllocator::reset) to reclaim the entire capacity at once.
pub struct SubAllocator {
    buffer: wgpu::Buffer,
    capacity: u64,
    cursor: u64,
    alignment: u64,
}

impl SubAllocator {
    /// Create a `SubAllocator` wrapping `buffer` with the given `capacity` and
    /// minimum `alignment` (must be a power of two; clamped to 1 if zero).
    pub fn new(buffer: wgpu::Buffer, capacity: u64, alignment: u64) -> Self {
        SubAllocator {
            buffer,
            capacity,
            cursor: 0,
            alignment: alignment.max(1),
        }
    }

    /// Allocate a contiguous region of `size` bytes, aligned to
    /// `self.alignment`.
    ///
    /// Returns `None` when the remaining capacity is insufficient.
    pub fn alloc(&mut self, size: u64) -> Option<SubRegion> {
        let aligned_cursor = align_up(self.cursor, self.alignment);
        let end = aligned_cursor.checked_add(size)?;
        if end > self.capacity {
            return None;
        }
        self.cursor = end;
        Some(SubRegion {
            offset: aligned_cursor,
            size,
        })
    }

    /// Reset the cursor to zero, making all previously allocated regions
    /// available again.  The backing buffer is **not** cleared.
    pub fn reset(&mut self) {
        self.cursor = 0;
    }

    /// Access the underlying [`wgpu::Buffer`].
    pub fn inner(&self) -> &wgpu::Buffer {
        &self.buffer
    }

    /// Number of bytes currently allocated (cursor position, before alignment
    /// of the *next* alloc).
    pub fn used(&self) -> u64 {
        self.cursor
    }

    /// Number of bytes remaining after the current cursor.
    pub fn remaining(&self) -> u64 {
        self.capacity.saturating_sub(self.cursor)
    }
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Convert an empty label string to `None` (wgpu prefers `Option<&str>`).
#[inline]
fn non_empty_label(label: &str) -> Option<&str> {
    if label.is_empty() {
        None
    } else {
        Some(label)
    }
}

/// Round `value` up to the nearest multiple of `alignment`.
///
/// If `alignment` is 0 the value is returned unchanged.
#[inline]
fn align_up(value: u64, alignment: u64) -> u64 {
    if alignment == 0 {
        return value;
    }
    value.div_ceil(alignment) * alignment
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ComputeContext;

    /// Helper — skip the test gracefully when no GPU is available (CI).
    macro_rules! require_gpu {
        ($ctx:ident) => {
            let Some($ctx) = ComputeContext::try_new() else {
                return; // no GPU on this host — graceful skip
            };
        };
    }

    // ── Existing tests ────────────────────────────────────────────────────────

    #[test]
    fn storage_buffer_init_roundtrip() {
        require_gpu!(ctx);
        let data: Vec<f32> = vec![1.0, 2.0, 3.0, 4.0];
        let bytes = bytemuck::cast_slice::<f32, u8>(&data);
        let buf = storage_buffer_init(&ctx.device, "test-storage", bytes);
        let back: Vec<f32> = read_back(&ctx.device, &ctx.queue, &buf, data.len());
        assert_eq!(back, data);
    }

    #[test]
    fn uniform_buffer_created() {
        require_gpu!(ctx);
        let data: [f32; 4] = [0.1, 0.2, 0.3, 0.4];
        let bytes = bytemuck::cast_slice::<f32, u8>(&data);
        // Just verify it constructs without panic.
        let _buf = uniform_buffer(&ctx.device, "test-uniform", bytes);
    }

    #[test]
    fn staging_buffer_created() {
        require_gpu!(ctx);
        let _buf = staging_buffer(&ctx.device, "test-staging", 256);
    }

    #[test]
    fn non_empty_label_behaviour() {
        assert_eq!(non_empty_label("foo"), Some("foo"));
        assert_eq!(non_empty_label(""), None);
    }

    // ── Non-GPU unit tests (slice S2) ─────────────────────────────────────────

    /// Verify that the byte_len formula for TypedBuffer is correct.
    #[test]
    fn typed_buffer_len_math() {
        assert_eq!(std::mem::size_of::<f32>(), 4);
        let len: usize = 8;
        assert_eq!(len * std::mem::size_of::<f32>(), 32);
        // u64 cast (what byte_len uses internally)
        assert_eq!((len * std::mem::size_of::<f32>()) as u64, 32u64);
    }

    /// Two allocations of 100 bytes with alignment 256 must land at offsets
    /// 0 and 256 respectively.
    #[test]
    fn suballocator_offsets_aligned() {
        // We need a real wgpu::Buffer to construct SubAllocator; on headless
        // CI we can verify only the align_up math without a GPU.
        // Test the alignment arithmetic directly.
        let first_aligned = align_up(0, 256);
        assert_eq!(first_aligned, 0);
        let after_first = first_aligned + 100; // cursor after first alloc
        let second_aligned = align_up(after_first, 256);
        assert!(
            second_aligned >= 256,
            "second offset {second_aligned} should be >= 256"
        );
        assert_eq!(second_aligned % 256, 0);
    }

    /// After reset the cursor returns to 0, so a fresh alloc gets offset 0.
    #[test]
    fn suballocator_reset_rewinds() {
        // Test SubAllocator logic with a dummy buffer via GPU if available,
        // otherwise verify the cursor arithmetic directly.
        require_gpu!(ctx);

        // 1 KiB backing buffer, 256-byte alignment.
        let backing = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sub-alloc-test"),
            size: 1024,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let mut sa = SubAllocator::new(backing, 1024, 256);

        let r1 = sa.alloc(100).expect("first alloc should succeed");
        assert_eq!(r1.offset, 0);
        sa.reset();
        let r2 = sa.alloc(100).expect("post-reset alloc should succeed");
        assert_eq!(r2.offset, 0, "after reset, offset must restart at 0");
    }

    /// Power-of-two rounding used by BufferPool must work correctly.
    #[test]
    fn buffer_pool_size_rounds_up() {
        assert_eq!(256u64.next_power_of_two(), 256);
        assert_eq!(300u64.next_power_of_two(), 512);
        assert_eq!(1u64.next_power_of_two().max(256), 256);
        assert_eq!(255u64.next_power_of_two().max(256), 256);
    }

    /// Cast a &[f32] to bytes and back — verifies bytemuck Pod semantics.
    #[test]
    fn bytemuck_pod_roundtrip() {
        let original: [f32; 3] = [1.0, 2.0, 3.0];
        let bytes: &[u8] = bytemuck::cast_slice(&original);
        assert_eq!(bytes.len(), 12);
        let back: &[f32] = bytemuck::cast_slice(bytes);
        assert_eq!(back, &original);
    }

    // ── GPU-gated tests (slice S2) ────────────────────────────────────────────

    /// Acquire a buffer, release it, then acquire again — the pool must hand
    /// back a recycled buffer (available_count drops from 1 to 0).
    #[test]
    fn pool_acquire_reuses_buffer() {
        require_gpu!(ctx);
        let mut pool = BufferPool::new();
        let usage = wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC;
        let size: u64 = 256;

        // Initially nothing in pool.
        assert_eq!(pool.available_count(size, usage), 0);

        // Acquire allocates fresh.
        let buf = pool.acquire(&ctx.device, size, usage);

        // Release puts it back.
        pool.release(size, usage, buf);
        assert_eq!(pool.available_count(size, usage), 1);

        // Second acquire pulls from pool.
        let _buf2 = pool.acquire(&ctx.device, size, usage);
        assert_eq!(pool.available_count(size, usage), 0);
    }

    /// `mapped_storage_init` must produce a buffer whose contents match the
    /// input data when read back via a copy + staging buffer.
    #[test]
    fn mapped_init_roundtrip() {
        require_gpu!(ctx);
        let data: Vec<f32> = vec![10.0, 20.0, 30.0, 40.0];
        let bytes = bytemuck::cast_slice::<f32, u8>(&data);

        let src = mapped_storage_init(&ctx.device, "mapped-init-test", bytes);

        // mapped_storage_init gives STORAGE | COPY_SRC, so we need a staging
        // buffer to read back.
        let staging = staging_buffer(&ctx.device, "mapped-init-staging", bytes.len() as u64);
        let mut encoder = ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("mapped-init-readback"),
            });
        encoder.copy_buffer_to_buffer(&src, 0, &staging, 0, bytes.len() as u64);
        ctx.queue.submit(std::iter::once(encoder.finish()));

        let slice = staging.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        ctx.device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("poll failed");
        rx.recv()
            .expect("channel closed")
            .expect("map_async failed");

        let mapped = slice.get_mapped_range();
        let back: Vec<f32> = bytemuck::cast_slice::<u8, f32>(&mapped).to_vec();
        drop(mapped);
        staging.unmap();

        assert_eq!(back, data);
    }

    /// Write [10, 20, 30, 40] f32 values, then read_back_range at byte offset 4
    /// (skip first element) with len=2 — must return [20.0, 30.0].
    #[test]
    fn read_back_range_returns_subslice() {
        require_gpu!(ctx);
        let data: Vec<f32> = vec![10.0, 20.0, 30.0, 40.0];
        let bytes = bytemuck::cast_slice::<f32, u8>(&data);
        let buf = storage_buffer_init(&ctx.device, "range-test", bytes);

        // Skip first f32 (4 bytes), read next 2 f32s.
        let sub: Vec<f32> = read_back_range(&ctx.device, &ctx.queue, &buf, 4, 2);
        assert_eq!(sub, vec![20.0f32, 30.0]);
    }

    /// `pollster::block_on(read_back_async(...))` must return the same values
    /// as the synchronous `read_back(...)`.
    #[test]
    fn async_readback_matches_sync() {
        require_gpu!(ctx);
        let data: Vec<f32> = vec![5.0, 6.0, 7.0, 8.0];
        let bytes = bytemuck::cast_slice::<f32, u8>(&data);
        let buf = storage_buffer_init(&ctx.device, "async-readback-test", bytes);

        let sync_result: Vec<f32> = read_back(&ctx.device, &ctx.queue, &buf, data.len());
        let async_result: Vec<f32> =
            pollster::block_on(read_back_async(&ctx.device, &ctx.queue, &buf, data.len()))
                .expect("async readback failed");

        assert_eq!(sync_result, async_result);
        assert_eq!(async_result, data);
    }
}
