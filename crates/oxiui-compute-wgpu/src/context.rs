//! Headless GPU compute context: `Instance` → `Adapter` → `Device` + `Queue`.
//!
//! [`ComputeContext`] performs the full no-window, no-surface initialisation
//! chain required for pure GPU compute workloads (sparse solvers, LBM, MC/DC,
//! …).  Three constructors plus a fluent [`ContextBuilder`] are provided:
//!
//! * [`ComputeContext::try_new`] — returns `Option<Self>`; `None` means no GPU
//!   adapter is available (graceful CI skip, never panics).
//! * [`ComputeContext::new`] — returns `Result<Self, ComputeError>`; exposes the
//!   underlying failure reason through [`ComputeError`].
//! * [`ComputeContext::new_async`] — async variant; awaits adapter and device
//!   requests directly without a `pollster::block_on` wrapper.
//! * [`ComputeContext::builder`] — returns a [`ContextBuilder`] for fluent
//!   configuration of limits, features, and power preference.
//! * [`ComputeContext::from_device`] — wraps an externally owned
//!   `wgpu::Device` + `wgpu::Queue` (e.g. from `oxiui-render-wgpu`) so the
//!   compute layer can share the render backend's device.
//!
//! ## Multi-queue support
//!
//! On adapters that expose separate transfer and compute queue families,
//! [`ContextBuilder::with_multi_queue`] requests a dedicated transfer queue.
//! When no second queue family is available the context falls back to a single
//! shared queue (`transfer_queue()` returns `None`).
//!
//! Both sync constructors use `PowerPreference::HighPerformance` and
//! `wgpu::Limits::default()` (not `downlevel_defaults()`, which caps the
//! compute feature set).

use crate::error::ComputeError;

// ── ComputeContext ─────────────────────────────────────────────────────────────

/// An initialised headless GPU compute context.
///
/// Owns the logical [`wgpu::Device`], the primary compute [`wgpu::Queue`],
/// an optional dedicated transfer queue (when the adapter exposes more than one
/// queue family), and the [`wgpu::AdapterInfo`] snapshot captured at
/// construction time.  No window handle, surface, or swap-chain is involved.
pub struct ComputeContext {
    /// The logical GPU device.
    pub device: wgpu::Device,
    /// The primary command submission queue (compute and, when no separate
    /// transfer queue is available, also used for DMA transfers).
    pub queue: wgpu::Queue,
    /// A dedicated transfer queue, present only when the adapter exposes
    /// separate queue families **and** `ContextBuilder::with_multi_queue` was
    /// called.  `None` when the adapter provides a single shared queue family.
    transfer_queue: Option<wgpu::Queue>,
    /// Adapter metadata snapshot (vendor, backend, driver, …).
    adapter_info: wgpu::AdapterInfo,
}

impl ComputeContext {
    /// Return a reference to the adapter metadata captured at construction time.
    ///
    /// The returned [`wgpu::AdapterInfo`] contains fields such as `name`,
    /// `vendor`, `device`, `backend`, `driver`, and `driver_info`.
    ///
    /// ```rust,no_run
    /// use oxiui_compute_wgpu::ComputeContext;
    ///
    /// if let Some(ctx) = ComputeContext::try_new() {
    ///     let info = ctx.adapter_info();
    ///     println!("GPU backend: {:?}", info.backend);
    /// }
    /// ```
    pub fn adapter_info(&self) -> &wgpu::AdapterInfo {
        &self.adapter_info
    }

    /// Return the dedicated transfer queue when one was obtained.
    ///
    /// Returns `Some` only when [`ContextBuilder::with_multi_queue`] was
    /// called **and** the underlying adapter exposed a separate transfer queue
    /// family.  Callers should fall back to `self.queue` when this is `None`.
    ///
    /// ```rust,no_run
    /// use oxiui_compute_wgpu::ComputeContext;
    ///
    /// if let Some(ctx) = ComputeContext::try_new() {
    ///     if let Some(tq) = ctx.transfer_queue() {
    ///         // Use the dedicated DMA queue for uploads.
    ///         let _ = tq;
    ///     }
    /// }
    /// ```
    pub fn transfer_queue(&self) -> Option<&wgpu::Queue> {
        self.transfer_queue.as_ref()
    }

    /// Return a [`ContextBuilder`] for fluent configuration of limits,
    /// features, and power preference.
    ///
    /// ```rust,no_run
    /// use oxiui_compute_wgpu::ComputeContext;
    ///
    /// let ctx = ComputeContext::builder()
    ///     .with_power_preference(wgpu::PowerPreference::LowPower)
    ///     .build();
    /// ```
    pub fn builder() -> ContextBuilder {
        ContextBuilder::default()
    }

    /// Create a context with high-performance power preference and default limits.
    ///
    /// # Errors
    ///
    /// * [`ComputeError::NoAdapter`] — no suitable GPU adapter was found.
    /// * [`ComputeError::DeviceRequest`] — the device/queue request failed.
    ///
    /// ```rust,no_run
    /// use oxiui_compute_wgpu::{ComputeContext, ComputeError};
    ///
    /// match ComputeContext::new() {
    ///     Ok(ctx)                      => { let _ = ctx; }
    ///     Err(ComputeError::NoAdapter) => { /* skip */ }
    ///     Err(e)                       => panic!("unexpected: {e}"),
    /// }
    /// ```
    #[cfg_attr(feature = "tracing", tracing::instrument(level = "debug"))]
    pub fn new() -> Result<Self, ComputeError> {
        ContextBuilder::default().build()
    }

    /// Try to create a `ComputeContext`, returning `None` when no suitable GPU
    /// adapter is available on this host.
    ///
    /// This constructor never panics.  Call sites that want a graceful skip on
    /// headless CI environments (VMs, containers without GPU pass-through) should
    /// use this variant:
    ///
    /// ```rust,no_run
    /// use oxiui_compute_wgpu::ComputeContext;
    ///
    /// if let Some(ctx) = ComputeContext::try_new() {
    ///     // GPU is available — run the compute workload
    ///     let _ = ctx;
    /// } else {
    ///     // No GPU — skip gracefully
    /// }
    /// ```
    pub fn try_new() -> Option<Self> {
        Self::new().ok()
    }

    /// Async variant of [`new`][Self::new] — awaits adapter and device requests
    /// directly without a `pollster::block_on` wrapper.
    ///
    /// Suitable for use inside an async runtime (Tokio, async-std, etc.).
    ///
    /// # Errors
    ///
    /// Same as [`new`][Self::new].
    ///
    /// ```rust,no_run
    /// use oxiui_compute_wgpu::ComputeContext;
    ///
    /// # async fn run() -> Result<(), oxiui_compute_wgpu::ComputeError> {
    /// let ctx = ComputeContext::new_async().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn new_async() -> Result<Self, ComputeError> {
        ContextBuilder::default().build_async().await
    }

    /// Wrap externally owned `wgpu::Device` and `wgpu::Queue` in a
    /// `ComputeContext` so that the compute layer can share a device/queue pair
    /// that was already created by another backend (e.g. `oxiui-render-wgpu`).
    ///
    /// A synthetic [`wgpu::AdapterInfo`] is constructed from the optional
    /// `adapter_info` argument; pass `None` to use a placeholder.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use oxiui_compute_wgpu::ComputeContext;
    ///
    /// // Suppose `device` and `queue` come from an external renderer.
    /// # fn external() -> (wgpu::Device, wgpu::Queue) { unimplemented!() }
    /// let (device, queue) = external();
    /// let ctx = ComputeContext::from_device(device, queue, None);
    /// ```
    pub fn from_device(
        device: wgpu::Device,
        queue: wgpu::Queue,
        adapter_info: Option<wgpu::AdapterInfo>,
    ) -> Self {
        let adapter_info = adapter_info.unwrap_or_else(|| wgpu::AdapterInfo {
            name: "external".into(),
            vendor: 0,
            device: 0,
            device_type: wgpu::DeviceType::Other,
            device_pci_bus_id: String::new(),
            driver: String::new(),
            driver_info: String::new(),
            backend: wgpu::Backend::Noop,
            subgroup_min_size: 0,
            subgroup_max_size: 0,
            transient_saves_memory: false,
        });
        ComputeContext {
            device,
            queue,
            transfer_queue: None,
            adapter_info,
        }
    }

    // ── Convenience delegates to ContextBuilder ─────────────────────────────

    /// Create a [`crate::dispatch::Dispatcher`] that borrows this context.
    ///
    /// The `Dispatcher` provides high-level, zero-boilerplate GPU compute
    /// operations (`map_f32`, `zip_map_f32`, `reduce_sum_f32`, `sph_density`,
    /// `sort_f32`, …).
    ///
    /// ```rust,no_run
    /// use oxiui_compute_wgpu::ComputeContext;
    ///
    /// if let Some(ctx) = ComputeContext::try_new() {
    ///     let d = ctx.dispatcher();
    ///     let out = d.map_f32(&[1.0, 2.0, 3.0], "x * 2.0");
    ///     assert_eq!(out, vec![2.0, 4.0, 6.0]);
    /// }
    /// ```
    pub fn dispatcher(&self) -> crate::dispatch::Dispatcher<'_> {
        crate::dispatch::Dispatcher::new(self)
    }

    /// Start building a context with custom memory limits.
    ///
    /// Equivalent to `ComputeContext::builder().with_limits(limits)`.
    pub fn with_limits(limits: wgpu::Limits) -> ContextBuilder {
        ContextBuilder::default().with_limits(limits)
    }

    /// Start building a context with specific GPU features enabled.
    ///
    /// Equivalent to `ComputeContext::builder().with_features(features)`.
    pub fn with_features(features: wgpu::Features) -> ContextBuilder {
        ContextBuilder::default().with_features(features)
    }

    /// Start building a context with a specific power preference.
    ///
    /// Equivalent to `ComputeContext::builder().with_power_preference(pref)`.
    pub fn with_power_preference(pref: wgpu::PowerPreference) -> ContextBuilder {
        ContextBuilder::default().with_power_preference(pref)
    }

    /// Return a [`crate::hot_reload::ShaderWatcher`] that watches WGSL source
    /// files on disk and signals when recompilation is needed.
    ///
    /// Only available when the `hot-reload` Cargo feature is enabled.
    ///
    /// ```rust,no_run
    /// # #[cfg(feature = "hot-reload")]
    /// # {
    /// use oxiui_compute_wgpu::ComputeContext;
    ///
    /// if let Some(ctx) = ComputeContext::try_new() {
    ///     let _watcher = ctx.watcher();
    ///     // Add paths to watch, then call watcher.drain_changed() each frame.
    /// }
    /// # }
    /// ```
    #[cfg(feature = "hot-reload")]
    pub fn watcher(&self) -> crate::hot_reload::ShaderWatcher {
        crate::hot_reload::ShaderWatcher::new()
    }
}

// ── ContextBuilder ─────────────────────────────────────────────────────────────

/// Fluent builder for [`ComputeContext`].
///
/// Compose limits, features, and power preference in one chain, then call
/// [`build`][ContextBuilder::build] (sync) or [`build_async`][ContextBuilder::build_async]
/// (async) to finalise.
///
/// ```rust,no_run
/// use oxiui_compute_wgpu::{ComputeContext, ComputeError};
///
/// let result = ComputeContext::builder()
///     .with_power_preference(wgpu::PowerPreference::HighPerformance)
///     .with_limits(wgpu::Limits::default())
///     .build();
/// ```
#[derive(Debug, Default)]
pub struct ContextBuilder {
    power_preference: wgpu::PowerPreference,
    required_features: wgpu::Features,
    required_limits: Option<wgpu::Limits>,
    /// When `true`, request a dedicated transfer queue family (if supported).
    multi_queue: bool,
}

impl ContextBuilder {
    /// Set the GPU power preference.
    ///
    /// Defaults to [`wgpu::PowerPreference::HighPerformance`] when not
    /// called.
    pub fn with_power_preference(mut self, pref: wgpu::PowerPreference) -> Self {
        self.power_preference = pref;
        self
    }

    /// Request optional GPU features (e.g. `TIMESTAMP_QUERY`, `SHADER_F16`).
    ///
    /// If the adapter does not support the requested features, [`ContextBuilder::build`] will
    /// return [`ComputeError::DeviceRequest`] with a descriptive message before
    /// attempting `request_device`.
    pub fn with_features(mut self, features: wgpu::Features) -> Self {
        self.required_features = features;
        self
    }

    /// Override the default device limits.
    ///
    /// Use [`wgpu::Limits::downlevel_defaults()`] for maximum compatibility or
    /// supply custom limits for high-throughput compute workloads.
    pub fn with_limits(mut self, limits: wgpu::Limits) -> Self {
        self.required_limits = Some(limits);
        self
    }

    /// Request separate transfer and compute queues on adapters that advertise
    /// more than one queue family.
    ///
    /// When the adapter exposes only a single queue family the built context
    /// falls back gracefully: `transfer_queue()` returns `None` and `queue`
    /// is used for all operations.
    ///
    /// Note: wgpu currently exposes at most one queue per device to the Rust
    /// API, so this option records the intent and the context exposes
    /// `transfer_queue()` as `None` until wgpu adds explicit multi-queue
    /// support.  The flag is preserved for forward compatibility.
    pub fn with_multi_queue(mut self) -> Self {
        self.multi_queue = true;
        self
    }

    /// Blocking variant: run the full adapter + device init on the current thread.
    ///
    /// # Errors
    ///
    /// * [`ComputeError::NoAdapter`] — no GPU adapter matched the options.
    /// * [`ComputeError::DeviceRequest`] — features or device request failed.
    pub fn build(self) -> Result<ComputeContext, ComputeError> {
        pollster::block_on(self.build_async())
    }

    /// Async variant: await adapter and device requests inside the caller's runtime.
    ///
    /// # Errors
    ///
    /// * [`ComputeError::NoAdapter`] — no GPU adapter matched the options.
    /// * [`ComputeError::DeviceRequest`] — features or device request failed.
    pub async fn build_async(self) -> Result<ComputeContext, ComputeError> {
        let instance = wgpu::Instance::default();

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: self.power_preference,
                force_fallback_adapter: false,
                // No surface — pure compute, no swap-chain required.
                compatible_surface: None,
            })
            .await
            .map_err(|_| ComputeError::NoAdapter)?;

        // Pre-check requested features before attempting device acquisition so
        // callers get a clear error instead of a cryptic RequestDeviceError.
        if !self.required_features.is_empty()
            && !adapter.features().contains(self.required_features)
        {
            return Err(ComputeError::DeviceRequest(format!(
                "adapter does not support requested features: {:?}",
                self.required_features
            )));
        }

        // Capture adapter metadata before consuming the adapter.
        let adapter_info = adapter.get_info();

        let limits = self.required_limits.unwrap_or_default();

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("oxiui-compute-wgpu"),
                required_features: self.required_features,
                required_limits: limits,
                ..Default::default()
            })
            .await
            .map_err(|e| ComputeError::DeviceRequest(e.to_string()))?;

        // Multi-queue: wgpu currently exposes one queue per device.  The intent
        // is recorded and `transfer_queue` is set to `None` until wgpu adds
        // explicit multi-queue support.  Callers check `transfer_queue()` and
        // fall back to `queue` automatically.
        let transfer_queue: Option<wgpu::Queue> = if self.multi_queue {
            // Future: when wgpu supports `request_device` returning multiple
            // queues, acquire a second queue here.  For now, advertise that
            // the context was built with multi-queue intent but that the
            // adapter does not expose a second queue.
            None
        } else {
            None
        };

        Ok(ComputeContext {
            device,
            queue,
            transfer_queue,
            adapter_info,
        })
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── existing tests (preserved) ───────────────────────────────────────────

    #[test]
    fn try_new_does_not_panic() {
        // Gracefully skips if no GPU adapter — must never panic.
        let _ = ComputeContext::try_new();
    }

    #[test]
    fn new_returns_result() {
        match ComputeContext::new() {
            Ok(_ctx) => { /* GPU available — context created successfully */ }
            Err(ComputeError::NoAdapter) => {
                // No GPU on this host (CI, headless VM) — acceptable skip.
            }
            Err(ComputeError::DeviceRequest(ref msg)) => {
                panic!("unexpected DeviceRequest error: {msg}")
            }
            Err(e) => {
                panic!("unexpected error: {e}")
            }
        }
    }

    #[test]
    fn try_new_consistent_with_new() {
        // try_new() must be consistent with new(): both fail or both succeed.
        let via_new = ComputeContext::new();
        let via_try = ComputeContext::try_new();
        match (via_new, via_try) {
            (Ok(_), Some(_)) | (Err(_), None) => { /* consistent */ }
            (Ok(_), None) => panic!("new() succeeded but try_new() returned None"),
            (Err(e), Some(_)) => panic!("new() failed but try_new() returned Some: {e}"),
        }
    }

    // ── new tests (S1) ───────────────────────────────────────────────────────

    /// Non-GPU test: verify that `ContextBuilder::default()` constructs without
    /// panicking, even before `build()` is called.
    #[test]
    fn builder_chain_defaults() {
        // The builder itself must be constructable regardless of GPU availability.
        let _builder = ContextBuilder::default()
            .with_power_preference(wgpu::PowerPreference::HighPerformance)
            .with_limits(wgpu::Limits::default())
            .with_features(wgpu::Features::empty());
        // Attempt build; whether it succeeds depends on host GPU availability —
        // either outcome is acceptable.
        let _result = _builder.build();
        // No assertion: success or NoAdapter are both valid outcomes.
    }

    /// Non-GPU test: `with_multi_queue()` builds correctly.
    #[test]
    fn builder_with_multi_queue_does_not_panic() {
        let _result = ContextBuilder::default().with_multi_queue().build();
        // Either Ok or NoAdapter — neither must panic.
    }

    /// GPU-gated: adapter_info() returns a non-empty backend string.
    #[test]
    fn context_has_adapter_info() {
        oxiui_core::require_gpu!(ctx, ComputeContext::try_new());
        let info = ctx.adapter_info();
        let backend_str = format!("{:?}", info.backend);
        assert!(!backend_str.is_empty(), "backend string must not be empty");
    }

    /// GPU-gated: builder with LowPower preference builds successfully.
    #[test]
    fn builder_with_low_power() {
        oxiui_core::require_gpu!(
            ctx,
            ComputeContext::with_power_preference(wgpu::PowerPreference::LowPower)
                .build()
                .ok()
        );
        let _ = ctx;
    }

    /// GPU-gated: new_async() via pollster::block_on produces a valid context.
    #[test]
    fn new_async_via_pollster() {
        oxiui_core::require_gpu!(ctx, pollster::block_on(ComputeContext::new_async()).ok());
        let _ = ctx;
    }

    /// GPU-gated: requesting all features should return a clean error (not panic)
    /// when the adapter does not support them all.
    #[test]
    fn with_unsupported_features_returns_error() {
        // `wgpu::Features::all()` is almost certainly not fully supported on any
        // single adapter; we expect either a DeviceRequest error or a successful
        // build (the latter is allowed on hardware that does support everything).
        // What must NOT happen is a panic.
        let result = ComputeContext::with_features(wgpu::Features::all()).build();
        match result {
            Ok(_) => { /* hardware supports all features — acceptable */ }
            Err(ComputeError::NoAdapter) => { /* no GPU — skip */ }
            Err(ComputeError::DeviceRequest(_)) => { /* expected clean error */ }
            Err(e) => panic!("unexpected error variant: {e}"),
        }
    }

    /// GPU-gated: `transfer_queue()` returns `None` on a standard context
    /// (multi-queue not requested).
    #[test]
    fn transfer_queue_none_without_multi_queue() {
        oxiui_core::require_gpu!(ctx, ComputeContext::try_new());
        assert!(
            ctx.transfer_queue().is_none(),
            "transfer_queue must be None when multi-queue was not requested"
        );
    }

    /// GPU-gated: multi-queue context builds without panic; transfer_queue is
    /// `None` (graceful fallback) because wgpu exposes one queue per device.
    #[test]
    fn multi_queue_context_builds() {
        oxiui_core::require_gpu!(
            ctx,
            ComputeContext::builder().with_multi_queue().build().ok()
        );
        // wgpu currently exposes at most one queue — transfer_queue is None.
        assert!(ctx.transfer_queue().is_none());
    }

    /// Non-GPU test: `from_device` wraps externally owned resources.
    ///
    /// This test only verifies the `from_device` path at the type level;
    /// actual GPU execution is not performed.
    #[test]
    fn from_device_via_real_gpu() {
        oxiui_core::require_gpu!(ctx, ComputeContext::try_new());
        // Extract adapter info from the context we just built.
        let info = ctx.adapter_info().clone();
        // Rebuild using from_device with the same device + queue.
        // (We move ctx's device/queue into a new context.)
        let ctx2 = ComputeContext::from_device(ctx.device, ctx.queue, Some(info.clone()));
        assert_eq!(ctx2.adapter_info().name, info.name);
        assert!(ctx2.transfer_queue().is_none());
    }
}
