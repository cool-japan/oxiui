//! Render-layer cache for GPU-backed subtree caching.
//!
//! [`LayerCache`] manages a pool of [`RenderTarget`]s keyed by a caller-provided
//! `u64` `layer_id`.  When a widget subtree is rendered, its output is saved to
//! a `RenderTarget`; subsequent frames can composite the cached layer texture
//! directly without replaying the full draw list, as long as the layer hasn't
//! been invalidated.
//!
//! # Invalidation model
//!
//! - Each layer has a generation counter.  The caller bumps the generation (via
//!   [`LayerCache::invalidate`]) when the subtree's content changes.
//! - Layers that haven't been accessed for `max_idle_frames` frames are evicted
//!   to reclaim GPU memory.
//!
//! # Usage
//!
//! ```rust,ignore
//! let mut cache = LayerCache::new(16);    // keep up to 16 layers
//! cache.begin_frame();
//!
//! let layer_id = widget.stable_id();
//! if let Some(target) = cache.get(layer_id) {
//!     if !target.is_dirty() {
//!         // composite cached layer texture into the parent pass
//!         composite_layer(target.texture_view(), ...);
//!         return;
//!     }
//! }
//! // Layer is dirty or absent — render the subtree into it.
//! let target = cache.get_or_create(device, layer_id, width, height, 1)?;
//! render_subtree_into(target, ...);
//! target.mark_clean();
//! ```

use oxiui_core::UiError;

use crate::gpu::render_target::RenderTarget;

// ── LayerEntry ────────────────────────────────────────────────────────────────

struct LayerEntry {
    target: RenderTarget,
    /// Frame number of the last access (used for LRU eviction).
    last_frame: u64,
    /// Content generation — bumped by `invalidate()`.
    generation: u64,
}

// ── LayerCache ────────────────────────────────────────────────────────────────

/// A pool of off-screen render targets, keyed by `layer_id: u64`.
pub struct LayerCache {
    entries: std::collections::HashMap<u64, LayerEntry>,
    /// Maximum number of idle frames before a layer is evicted.
    max_idle_frames: u64,
    /// Current frame counter, advanced by `begin_frame()`.
    current_frame: u64,
    /// Maximum number of live layers before the oldest is evicted.
    max_layers: usize,
}

impl LayerCache {
    /// Create a new cache that holds at most `max_layers` layers simultaneously.
    pub fn new(max_layers: usize) -> Self {
        Self {
            entries: std::collections::HashMap::new(),
            max_idle_frames: 4,
            current_frame: 0,
            max_layers: max_layers.max(1),
        }
    }

    /// Set the number of idle frames after which a layer is evicted.
    ///
    /// A larger value trades GPU memory for reduced re-render cost when a layer
    /// goes temporarily off-screen.
    pub fn set_max_idle_frames(&mut self, frames: u64) {
        self.max_idle_frames = frames.max(1);
    }

    /// Advance the internal frame counter and evict any layers that have not
    /// been accessed for `max_idle_frames` frames.
    ///
    /// **Must be called once per frame**, before any `get`/`get_or_create` calls
    /// for that frame.
    pub fn begin_frame(&mut self) {
        self.current_frame += 1;
        // Evict idle layers.
        let max_idle = self.max_idle_frames;
        let current = self.current_frame;
        self.entries
            .retain(|_, entry| current - entry.last_frame <= max_idle);
    }

    /// Look up an existing layer by `layer_id`.
    ///
    /// Updates the last-access frame counter.  Returns `None` if no layer with
    /// this id is currently cached.
    pub fn get(&mut self, layer_id: u64) -> Option<&mut RenderTarget> {
        let current = self.current_frame;
        let entry = self.entries.get_mut(&layer_id)?;
        entry.last_frame = current;
        Some(&mut entry.target)
    }

    /// Return the [`RenderTarget`] for `layer_id`, creating a new one if absent.
    ///
    /// If a new target is created it is `width × height` pixels with the given
    /// `sample_count`.  If the layer already exists but has different dimensions
    /// it is **not** resized — the existing target is returned as-is.  Call
    /// [`LayerCache::invalidate`] + [`LayerCache::remove`] then re-create if
    /// you need a different size.
    ///
    /// When the cache is at capacity (`max_layers`), the LRU layer is evicted
    /// before creating the new entry.
    ///
    /// # Errors
    ///
    /// Propagates [`RenderTarget::new`] errors.
    pub fn get_or_create(
        &mut self,
        device: &wgpu::Device,
        layer_id: u64,
        width: u32,
        height: u32,
        sample_count: u32,
    ) -> Result<&mut RenderTarget, UiError> {
        let current = self.current_frame;

        if !self.entries.contains_key(&layer_id) {
            // Evict LRU entry if at capacity.
            if self.entries.len() >= self.max_layers {
                self.evict_lru();
            }
            let target = RenderTarget::new(device, width, height, sample_count)?;
            self.entries.insert(
                layer_id,
                LayerEntry {
                    target,
                    last_frame: current,
                    generation: 0,
                },
            );
        }

        let entry = self.entries.get_mut(&layer_id).expect("just inserted");
        entry.last_frame = current;
        Ok(&mut entry.target)
    }

    /// Invalidate a layer by `layer_id`, marking it dirty and bumping its
    /// generation counter.  If the layer is not cached, this is a no-op.
    pub fn invalidate(&mut self, layer_id: u64) {
        if let Some(entry) = self.entries.get_mut(&layer_id) {
            entry.target.mark_dirty();
            entry.generation += 1;
        }
    }

    /// Invalidate all cached layers.
    pub fn invalidate_all(&mut self) {
        for entry in self.entries.values_mut() {
            entry.target.mark_dirty();
            entry.generation += 1;
        }
    }

    /// Remove a layer by `layer_id`, freeing the associated GPU texture.
    pub fn remove(&mut self, layer_id: u64) {
        self.entries.remove(&layer_id);
    }

    /// Remove all cached layers, freeing all associated GPU textures.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Return the number of live layers currently in the cache.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Return `true` if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Return the current frame counter.
    pub fn current_frame(&self) -> u64 {
        self.current_frame
    }

    // ── Private helpers ──────────────────────────────────────────────────────

    /// Evict the layer that was accessed least recently.
    fn evict_lru(&mut self) {
        // Find the key with the smallest `last_frame`.
        let lru_key = self
            .entries
            .iter()
            .min_by_key(|(_, e)| e.last_frame)
            .map(|(&k, _)| k);
        if let Some(k) = lru_key {
            self.entries.remove(&k);
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn try_device() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            force_fallback_adapter: false,
            compatible_surface: None,
        }))
        .ok()?;
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("layer-cache test device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            memory_hints: wgpu::MemoryHints::Performance,
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            trace: wgpu::Trace::Off,
        }))
        .ok()
    }

    #[test]
    fn layer_cache_empty_on_creation() {
        let cache = LayerCache::new(8);
        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
        assert_eq!(cache.current_frame(), 0);
    }

    #[test]
    fn layer_cache_get_or_create_and_get() {
        let Some((device, _queue)) = try_device() else {
            return;
        };
        let mut cache = LayerCache::new(4);
        cache.begin_frame();

        let target = cache
            .get_or_create(&device, 42, 32, 32, 1)
            .expect("create layer 42");
        assert!(target.is_dirty(), "fresh layer must be dirty");
        target.mark_clean();

        assert_eq!(cache.len(), 1);
        let t = cache.get(42).expect("layer 42 must exist");
        assert!(!t.is_dirty(), "layer must be clean after mark_clean");
    }

    #[test]
    fn layer_cache_invalidate_marks_dirty() {
        let Some((device, _queue)) = try_device() else {
            return;
        };
        let mut cache = LayerCache::new(4);
        cache.begin_frame();

        let target = cache.get_or_create(&device, 1, 16, 16, 1).expect("create");
        target.mark_clean();
        cache.invalidate(1);
        let t = cache.get(1).expect("layer 1 must exist");
        assert!(t.is_dirty(), "invalidated layer must be dirty");
    }

    #[test]
    fn layer_cache_evicts_idle_layers() {
        let Some((device, _queue)) = try_device() else {
            return;
        };
        let mut cache = LayerCache::new(8);
        cache.set_max_idle_frames(2);

        cache.begin_frame(); // frame 1
        cache.get_or_create(&device, 10, 16, 16, 1).expect("create");

        // Advance 3 frames without accessing layer 10.
        cache.begin_frame(); // frame 2
        cache.begin_frame(); // frame 3
        cache.begin_frame(); // frame 4 — idle >= 3 > max_idle_frames(2) → evicted

        assert!(
            cache.get(10).is_none(),
            "layer 10 must be evicted after {n} idle frames",
            n = 3
        );
    }

    #[test]
    fn layer_cache_lru_eviction_at_capacity() {
        let Some((device, _queue)) = try_device() else {
            return;
        };
        let mut cache = LayerCache::new(2); // capacity = 2 layers

        cache.begin_frame();
        cache.get_or_create(&device, 1, 8, 8, 1).expect("layer 1");
        cache.get_or_create(&device, 2, 8, 8, 1).expect("layer 2");
        assert_eq!(cache.len(), 2);

        // Accessing layer 2 makes layer 1 the LRU.
        cache.begin_frame();
        let _ = cache.get(2); // access layer 2 again

        // Creating layer 3 must evict LRU = layer 1.
        cache.get_or_create(&device, 3, 8, 8, 1).expect("layer 3");
        assert_eq!(cache.len(), 2, "cache should still hold 2 entries");
        assert!(
            cache.get(1).is_none(),
            "layer 1 (LRU) must have been evicted"
        );
        assert!(cache.get(2).is_some(), "layer 2 must survive");
        assert!(cache.get(3).is_some(), "layer 3 must be present");
    }

    #[test]
    fn layer_cache_clear_removes_all() {
        let Some((device, _queue)) = try_device() else {
            return;
        };
        let mut cache = LayerCache::new(8);
        cache.begin_frame();
        cache.get_or_create(&device, 1, 8, 8, 1).expect("layer 1");
        cache.get_or_create(&device, 2, 8, 8, 1).expect("layer 2");
        assert_eq!(cache.len(), 2);
        cache.clear();
        assert!(cache.is_empty());
    }

    #[test]
    fn layer_cache_invalid_layer_id_returns_none() {
        let mut cache = LayerCache::new(4);
        cache.begin_frame();
        assert!(cache.get(9999).is_none());
    }

    #[test]
    fn layer_cache_invalidate_all() {
        let Some((device, _queue)) = try_device() else {
            return;
        };
        let mut cache = LayerCache::new(4);
        cache.begin_frame();
        for id in 1u64..=3 {
            let t = cache.get_or_create(&device, id, 8, 8, 1).expect("create");
            t.mark_clean();
        }
        cache.invalidate_all();
        for id in 1u64..=3 {
            let t = cache.get(id).expect("layer must exist");
            assert!(
                t.is_dirty(),
                "all layers must be dirty after invalidate_all"
            );
        }
    }
}
