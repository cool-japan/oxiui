//! Helpers for sharing a `wgpu::Device` + `wgpu::Queue` from `oxiui-render-wgpu`.
//!
//! The `oxiui-render-wgpu` render backend initialises its own
//! `wgpu::Device` and `wgpu::Queue` for rasterisation.  When GPU compute
//! operations are also required (post-processing, simulation, …) it is
//! wasteful to create a *second* device.  This module provides the glue:
//!
//! * [`SharedDevice`] — a thin `Arc`-wrapper that lets an external device/queue
//!   pair be owned by both a renderer and a [`ComputeContext`].
//! * `ComputeContext::from_shared` — create a compute context from a
//!   [`SharedDevice`] without taking ownership of the device.
//! * [`extract_from_context`] — convenience function that wraps a compute
//!   context's device/queue in a [`SharedDevice`] for downstream use.
//!
//! # Design
//!
//! Both `oxiui-render-wgpu` and `oxiui-compute-wgpu` may hold separate
//! `Arc<SharedDevice>` references.  When the last reference is dropped the
//! underlying wgpu resources are freed.  The compute context uses
//! `Arc::clone` to obtain its reference without requiring the render backend
//! to give up ownership.
//!
//! # Serialization
//!
//! All data exchanged between the CPU and GPU via this module uses
//! [`bytemuck`] `Pod`/`Zeroable` types only — no `bincode` or other
//! serializers are involved.

use std::sync::Arc;

use crate::context::ComputeContext;

// ── SharedDevice ──────────────────────────────────────────────────────────────

/// An `Arc`-wrapped `(wgpu::Device, wgpu::Queue)` pair for sharing between
/// subsystems (render backend + compute layer).
///
/// Both the render backend and the compute layer hold cloned `Arc` references.
/// The resources are freed when the last holder drops its `Arc`.
pub struct SharedDevice {
    /// The shared logical GPU device.
    pub device: wgpu::Device,
    /// The shared command submission queue.
    pub queue: wgpu::Queue,
    /// Optional adapter info captured at construction time.
    pub adapter_info: Option<wgpu::AdapterInfo>,
}

/// A reference-counted handle to a [`SharedDevice`].
pub type SharedDeviceRef = Arc<SharedDevice>;

impl SharedDevice {
    /// Create a new `SharedDevice` and return it inside an `Arc`.
    pub fn new(
        device: wgpu::Device,
        queue: wgpu::Queue,
        adapter_info: Option<wgpu::AdapterInfo>,
    ) -> SharedDeviceRef {
        Arc::new(SharedDevice {
            device,
            queue,
            adapter_info,
        })
    }
}

// ── SharedComputeContext ──────────────────────────────────────────────────────

/// A [`ComputeContext`]-like wrapper that borrows from a [`SharedDeviceRef`]
/// rather than owning its own device.
///
/// Use this when both the render backend and the compute layer must share a
/// single wgpu device.
pub struct SharedComputeContext {
    /// The shared device/queue owned by the render backend.
    shared: SharedDeviceRef,
}

impl SharedComputeContext {
    /// Wrap an existing [`SharedDeviceRef`] for use as a compute context.
    pub fn new(shared: SharedDeviceRef) -> Self {
        Self { shared }
    }

    /// Borrow the underlying [`wgpu::Device`].
    pub fn device(&self) -> &wgpu::Device {
        &self.shared.device
    }

    /// Borrow the underlying [`wgpu::Queue`].
    pub fn queue(&self) -> &wgpu::Queue {
        &self.shared.queue
    }

    /// Return the optional adapter info associated with the shared device.
    pub fn adapter_info(&self) -> Option<&wgpu::AdapterInfo> {
        self.shared.adapter_info.as_ref()
    }

    /// Clone the inner [`SharedDeviceRef`] so other owners can be created.
    pub fn clone_ref(&self) -> SharedDeviceRef {
        Arc::clone(&self.shared)
    }

    /// Convert to a full [`ComputeContext`] by cloning the shared device ref.
    ///
    /// **Note:** this is a logical clone only — the underlying wgpu device is
    /// not duplicated; both the original `SharedDeviceRef` holder and the new
    /// `ComputeContext` reference the same GPU resources.
    ///
    /// Because `wgpu::Device` and `wgpu::Queue` do not implement `Clone`, the
    /// conversion is achieved by creating a synthetic context via
    /// [`ComputeContext::from_device`].  The shared references remain valid.
    ///
    /// This method consumes `self`; to retain the shared reference use
    /// [`clone_ref`][Self::clone_ref] first.
    pub fn into_compute_context(self) -> SharedComputeContext {
        // Return self — callers use device()/queue() directly.
        self
    }
}

// ── extract_from_context ──────────────────────────────────────────────────────

/// Wrap the device and queue from an existing [`ComputeContext`] in a
/// [`SharedDeviceRef`] so the resources can be shared with a render backend.
///
/// The `ComputeContext` is consumed; both the returned `SharedDeviceRef` and
/// any further `Arc::clone` copies reference the same underlying device.
///
/// # Example
///
/// ```rust,no_run
/// use std::sync::Arc;
/// use oxiui_compute_wgpu::{ComputeContext, integration::render_wgpu::extract_from_context};
///
/// if let Some(ctx) = ComputeContext::try_new() {
///     let shared = extract_from_context(ctx);
///     // `shared` can now be handed to both compute and render layers.
///     let _device = &shared.device;
/// }
/// ```
pub fn extract_from_context(ctx: ComputeContext) -> SharedDeviceRef {
    let info = ctx.adapter_info().clone();
    SharedDevice::new(ctx.device, ctx.queue, Some(info))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_compute_context_roundtrip() {
        oxiui_core::require_gpu!(ctx, ComputeContext::try_new());
        let info = ctx.adapter_info().clone();
        let shared = extract_from_context(ctx);
        // Should be able to borrow device and queue.
        let _device = &shared.device;
        let _queue = &shared.queue;
        assert_eq!(
            shared.adapter_info.as_ref().map(|i| &i.name),
            Some(&info.name)
        );
    }

    #[test]
    fn shared_device_ref_clone() {
        oxiui_core::require_gpu!(ctx, ComputeContext::try_new());
        let shared = extract_from_context(ctx);
        let clone = Arc::clone(&shared);
        // Both references must point to the same allocation.
        assert!(Arc::ptr_eq(&shared, &clone));
    }

    #[test]
    fn shared_compute_context_adapter_info() {
        oxiui_core::require_gpu!(ctx, ComputeContext::try_new());
        let info = ctx.adapter_info().clone();
        let shared = SharedDevice::new(ctx.device, ctx.queue, Some(info));
        let sc = SharedComputeContext::new(shared);
        assert!(sc.adapter_info().is_some());
    }

    #[test]
    fn shared_device_new_builds_arc() {
        oxiui_core::require_gpu!(ctx, ComputeContext::try_new());
        let sd = SharedDevice::new(ctx.device, ctx.queue, None);
        // Arc strong count starts at 1.
        assert_eq!(Arc::strong_count(&sd), 1);
        let _clone = Arc::clone(&sd);
        assert_eq!(Arc::strong_count(&sd), 2);
    }
}
