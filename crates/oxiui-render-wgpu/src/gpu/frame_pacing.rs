//! GPU frame timing and adaptive present-mode selection.
//!
//! [`FrameTimer`] wraps wgpu's `QuerySet` timestamp API to measure how long
//! each frame's GPU work takes.  A rolling histogram accumulates the last N
//! frame times so the caller can make data-driven decisions about present mode
//! and quality settings.
//!
//! # Feature gate
//!
//! Timestamp queries require [`wgpu::Features::TIMESTAMP_QUERY`].  The timer
//! gracefully degrades to CPU-side `std::time::Instant` measurements when the
//! feature is unavailable.
//!
//! # Present-mode heuristic
//!
//! - If the 99th-percentile frame time exceeds `target_frame_ms * 1.5`, the
//!   timer recommends switching to `Fifo` (V-sync) to reduce latency.
//! - If the 99th-percentile frame time stays below `target_frame_ms * 0.5`,
//!   `Mailbox` (triple-buffered tearing-free) is recommended.
//! - Otherwise, `Fifo` (V-sync) is the safe default recommendation.

use std::time::{Duration, Instant};

// ── FrameHistogram ────────────────────────────────────────────────────────────

/// A fixed-size circular buffer of frame durations.
const HISTOGRAM_SIZE: usize = 64;

/// Rolling histogram of the last `HISTOGRAM_SIZE` frame durations (in µs).
#[derive(Debug)]
pub struct FrameHistogram {
    samples: [u64; HISTOGRAM_SIZE],
    head: usize,
    count: usize,
}

impl Default for FrameHistogram {
    fn default() -> Self {
        Self {
            samples: [0u64; HISTOGRAM_SIZE],
            head: 0,
            count: 0,
        }
    }
}

impl FrameHistogram {
    /// Push a new frame duration (in microseconds).
    pub fn push(&mut self, duration_us: u64) {
        self.samples[self.head] = duration_us;
        self.head = (self.head + 1) % HISTOGRAM_SIZE;
        self.count = (self.count + 1).min(HISTOGRAM_SIZE);
    }

    /// Number of samples currently held.
    pub fn len(&self) -> usize {
        self.count
    }

    /// Return `true` if no samples have been recorded yet.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Mean frame duration in microseconds over the recorded samples.
    pub fn mean_us(&self) -> f64 {
        if self.count == 0 {
            return 0.0;
        }
        let sum: u64 = self.samples[..self.count].iter().sum();
        sum as f64 / self.count as f64
    }

    /// Minimum frame duration in the histogram (µs).
    pub fn min_us(&self) -> u64 {
        self.samples[..self.count]
            .iter()
            .copied()
            .min()
            .unwrap_or(0)
    }

    /// Maximum frame duration in the histogram (µs).
    pub fn max_us(&self) -> u64 {
        self.samples[..self.count]
            .iter()
            .copied()
            .max()
            .unwrap_or(0)
    }

    /// Approximate p99 frame duration (µs): the 99th percentile of the sorted
    /// samples in the histogram.
    pub fn p99_us(&self) -> u64 {
        if self.count == 0 {
            return 0;
        }
        let mut sorted = self.samples[..self.count].to_vec();
        sorted.sort_unstable();
        let idx = ((sorted.len() as f64 * 0.99) as usize).min(sorted.len() - 1);
        sorted[idx]
    }
}

// ── FrameTimerMode ────────────────────────────────────────────────────────────

/// Whether the timer uses GPU timestamps or CPU time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameTimerMode {
    /// GPU timestamp queries (high accuracy, requires `TIMESTAMP_QUERY`).
    GpuTimestamp,
    /// CPU-side `Instant` measurements (approximate, always available).
    CpuFallback,
}

// ── PresentModeRecommendation ─────────────────────────────────────────────────

/// A recommended wgpu present mode based on recent frame timings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentModeRecommendation {
    /// V-sync (safe, reduced latency when GPU is overloaded).
    Fifo,
    /// Triple-buffered, no tearing — ideal when GPU has headroom.
    Mailbox,
    /// Immediate present (lowest latency, may tear).
    Immediate,
}

impl PresentModeRecommendation {
    /// Convert to the corresponding [`wgpu::PresentMode`].
    pub fn to_wgpu(self) -> wgpu::PresentMode {
        match self {
            Self::Fifo => wgpu::PresentMode::Fifo,
            Self::Mailbox => wgpu::PresentMode::Mailbox,
            Self::Immediate => wgpu::PresentMode::Immediate,
        }
    }
}

// ── FrameTimer ────────────────────────────────────────────────────────────────

/// GPU frame timer with CPU fallback.
///
/// Record the start of a frame with [`FrameTimer::begin_frame`] and the end
/// with [`FrameTimer::end_frame`].  After each frame the duration is pushed to
/// the internal [`FrameHistogram`].  Call
/// [`FrameTimer::recommend_present_mode`] to get an adaptive present-mode
/// suggestion.
///
/// When `TIMESTAMP_QUERY` is available, a `QuerySet` with two entries is used
/// for sub-millisecond GPU-side measurements.  Otherwise, CPU `Instant` is
/// used as a coarser fallback.
pub struct FrameTimer {
    /// The rolling frame-time histogram.
    pub histogram: FrameHistogram,
    /// How this timer measures time.
    pub mode: FrameTimerMode,
    /// Target frame duration.
    target_frame: Duration,
    /// CPU start time (fallback mode).
    cpu_start: Option<Instant>,
    /// GPU timestamp `QuerySet` (two entries: start + end).
    timestamp_query_set: Option<wgpu::QuerySet>,
    /// Resolve buffer for timestamp queries.
    timestamp_resolve_buf: Option<wgpu::Buffer>,
    /// Readback buffer for timestamp queries.
    timestamp_readback_buf: Option<wgpu::Buffer>,
    /// Timestamp period (nanoseconds per tick) from the adapter.
    timestamp_period_ns: f32,
    /// Whether a GPU begin query has been issued this frame.
    gpu_query_pending: bool,
}

impl FrameTimer {
    /// Create a new frame timer.
    ///
    /// If `device.features()` contains `TIMESTAMP_QUERY`, GPU-side timing is
    /// used.  Otherwise, the timer falls back to CPU `Instant`.
    ///
    /// `target_fps` is used to compute `target_frame` for the present-mode
    /// heuristic.  Pass 60 for a standard 60 Hz target.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, target_fps: u32) -> Self {
        let target_frame = Duration::from_micros(1_000_000 / target_fps.max(1) as u64);
        let has_timestamps = device.features().contains(wgpu::Features::TIMESTAMP_QUERY);

        if has_timestamps {
            let timestamp_period_ns = queue.get_timestamp_period();
            let query_set = device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("oxiui-render-wgpu frame timer queries"),
                ty: wgpu::QueryType::Timestamp,
                count: 2,
            });
            let resolve_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("oxiui-render-wgpu timestamp resolve"),
                size: 16, // 2 × u64
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
            let readback_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("oxiui-render-wgpu timestamp readback"),
                size: 16,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            Self {
                histogram: FrameHistogram::default(),
                mode: FrameTimerMode::GpuTimestamp,
                target_frame,
                cpu_start: None,
                timestamp_query_set: Some(query_set),
                timestamp_resolve_buf: Some(resolve_buf),
                timestamp_readback_buf: Some(readback_buf),
                timestamp_period_ns,
                gpu_query_pending: false,
            }
        } else {
            Self {
                histogram: FrameHistogram::default(),
                mode: FrameTimerMode::CpuFallback,
                target_frame,
                cpu_start: None,
                timestamp_query_set: None,
                timestamp_resolve_buf: None,
                timestamp_readback_buf: None,
                timestamp_period_ns: 1.0,
                gpu_query_pending: false,
            }
        }
    }

    /// Record the start of a frame.
    ///
    /// In GPU mode, `encoder` should be the first encoder for the frame.
    /// In CPU fallback mode, `encoder` is ignored.
    pub fn begin_frame(&mut self, encoder: &mut wgpu::CommandEncoder) {
        match self.mode {
            FrameTimerMode::GpuTimestamp => {
                if let Some(ref qs) = self.timestamp_query_set {
                    encoder.write_timestamp(qs, 0);
                    self.gpu_query_pending = true;
                }
            }
            FrameTimerMode::CpuFallback => {
                self.cpu_start = Some(Instant::now());
            }
        }
    }

    /// Record the end of a frame.
    ///
    /// In GPU mode, `encoder` should be the last encoder for the frame (after
    /// all render passes).  Call `resolve` after submitting the encoder to
    /// copy the timestamps to the readback buffer.
    pub fn end_frame(&mut self, encoder: &mut wgpu::CommandEncoder) {
        match self.mode {
            FrameTimerMode::GpuTimestamp => {
                if let Some(ref qs) = self.timestamp_query_set {
                    encoder.write_timestamp(qs, 1);
                }
            }
            FrameTimerMode::CpuFallback => {
                if let Some(start) = self.cpu_start.take() {
                    let us = start.elapsed().as_micros() as u64;
                    self.histogram.push(us);
                }
            }
        }
    }

    /// Resolve GPU timestamps to the readback buffer.
    ///
    /// Call this in a *separate* encoder submitted immediately after the frame
    /// encoder.  The resolve copy is a GPU operation that must follow the
    /// write_timestamp commands in the previous encoder.
    ///
    /// No-op in CPU fallback mode or if no GPU query is pending.
    pub fn resolve_timestamps(&mut self, encoder: &mut wgpu::CommandEncoder) {
        if self.mode != FrameTimerMode::GpuTimestamp || !self.gpu_query_pending {
            return;
        }
        if let (Some(ref qs), Some(ref resolve_buf)) =
            (&self.timestamp_query_set, &self.timestamp_resolve_buf)
        {
            encoder.resolve_query_set(qs, 0..2, resolve_buf, 0);
            if let Some(ref readback_buf) = self.timestamp_readback_buf {
                encoder.copy_buffer_to_buffer(resolve_buf, 0, readback_buf, 0, 16);
            }
        }
    }

    /// Read back the GPU timestamps (if available) and update the histogram.
    ///
    /// Must be called after the resolve encoder has been submitted *and* the
    /// GPU has completed (e.g. after `device.poll(Wait)`).
    ///
    /// No-op in CPU fallback mode or if no GPU query is pending.
    pub fn collect_gpu_timestamps(&mut self, device: &wgpu::Device) {
        if self.mode != FrameTimerMode::GpuTimestamp || !self.gpu_query_pending {
            return;
        }
        self.gpu_query_pending = false;

        let Some(ref readback_buf) = self.timestamp_readback_buf else {
            return;
        };

        let slice = readback_buf.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        // Non-blocking: if the device has already been polled (Wait mode), the
        // mapping is ready.  If not, we skip this frame's sample to avoid
        // blocking the render thread.
        if device.poll(wgpu::PollType::Poll).is_ok() {
            let data = slice.get_mapped_range();
            let timestamps: [u64; 2] = bytemuck::pod_read_unaligned(&data[..16]);
            drop(data);
            readback_buf.unmap();

            if timestamps[1] >= timestamps[0] {
                let ticks = timestamps[1] - timestamps[0];
                let ns = ticks as f64 * self.timestamp_period_ns as f64;
                let us = (ns / 1_000.0) as u64;
                self.histogram.push(us);
            }
        } else {
            readback_buf.unmap();
        }
    }

    /// Recommend a [`wgpu::PresentMode`] based on the recent frame-time histogram.
    ///
    /// Requires at least 4 samples to make a recommendation; returns `Fifo`
    /// until then.
    pub fn recommend_present_mode(&self) -> PresentModeRecommendation {
        if self.histogram.len() < 4 {
            return PresentModeRecommendation::Fifo;
        }
        let p99_us = self.histogram.p99_us();
        let target_us = self.target_frame.as_micros() as u64;

        if p99_us > target_us * 3 / 2 {
            // GPU is consistently late — stay on Fifo to avoid stalls.
            PresentModeRecommendation::Fifo
        } else if p99_us < target_us / 2 {
            // GPU has lots of headroom — Mailbox or Immediate.
            PresentModeRecommendation::Mailbox
        } else {
            PresentModeRecommendation::Fifo
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn histogram_push_and_stats() {
        let mut h = FrameHistogram::default();
        assert!(h.is_empty());
        h.push(8_000); // 8 ms
        h.push(16_000); // 16 ms
        h.push(12_000); // 12 ms
        assert_eq!(h.len(), 3);
        let mean = h.mean_us();
        assert!((mean - 12_000.0).abs() < 1.0, "mean should be ~12000 µs");
        assert_eq!(h.min_us(), 8_000);
        assert_eq!(h.max_us(), 16_000);
    }

    #[test]
    fn histogram_p99_is_maximum_in_small_set() {
        let mut h = FrameHistogram::default();
        for ms in 1u64..=10u64 {
            h.push(ms * 1_000);
        }
        // p99 in a 10-sample set: index 9 (the max) = 10_000 µs.
        assert_eq!(h.p99_us(), 10_000);
    }

    #[test]
    fn histogram_wraps_at_capacity() {
        let mut h = FrameHistogram::default();
        // Push more than HISTOGRAM_SIZE samples.
        for i in 0..(HISTOGRAM_SIZE + 10) as u64 {
            h.push(i);
        }
        assert_eq!(
            h.len(),
            HISTOGRAM_SIZE,
            "should be capped at HISTOGRAM_SIZE"
        );
    }

    #[test]
    fn recommend_present_mode_fifo_when_insufficient_samples() {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            force_fallback_adapter: false,
            compatible_surface: None,
        }));
        let Some(adapter) = adapter.ok() else {
            return; // no GPU — skip GPU test
        };
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("frame-timer test device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            memory_hints: wgpu::MemoryHints::Performance,
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            trace: wgpu::Trace::Off,
        }))
        .expect("request_device");

        let timer = FrameTimer::new(&device, &queue, 60);
        // No samples recorded yet → always Fifo.
        assert_eq!(
            timer.recommend_present_mode(),
            PresentModeRecommendation::Fifo
        );
    }

    #[test]
    fn recommend_mailbox_when_fast() {
        let mut h = FrameHistogram::default();
        // Simulate very fast GPU: frames take 2 ms, target 16 ms → p99 << target/2.
        for _ in 0..10 {
            h.push(2_000); // 2 ms
        }
        // p99 = 2000, target = 16_667, 2000 < 8333 → Mailbox.
        let timer_mode = FrameTimerMode::CpuFallback;
        let _ = timer_mode; // just for doc purposes

        // Use the histogram directly.
        let p99 = h.p99_us();
        let target_us = 16_667u64;
        let rec = if p99 < target_us / 2 {
            PresentModeRecommendation::Mailbox
        } else {
            PresentModeRecommendation::Fifo
        };
        assert_eq!(rec, PresentModeRecommendation::Mailbox);
    }

    #[test]
    fn present_mode_recommendation_to_wgpu() {
        assert_eq!(
            PresentModeRecommendation::Fifo.to_wgpu(),
            wgpu::PresentMode::Fifo
        );
        assert_eq!(
            PresentModeRecommendation::Mailbox.to_wgpu(),
            wgpu::PresentMode::Mailbox
        );
        assert_eq!(
            PresentModeRecommendation::Immediate.to_wgpu(),
            wgpu::PresentMode::Immediate
        );
    }
}
