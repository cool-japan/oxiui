//! Generation-checked resource handles and reference-counted registry.
//!
//! Resources (textures, shaders) are identified by [`TextureHandle`] and
//! [`ShaderHandle`] — lightweight generation-checked tokens.  The
//! [`ResourceRegistry`] tracks reference counts and recycles slots so the
//! same index can be safely reused without stale-handle confusion.
//!
//! RAII wrappers ([`TextureGuard`], [`ShaderGuard`]) automatically decrement
//! the reference count when dropped.

use std::cell::RefCell;
use std::rc::Rc;

// ── ResourceId ────────────────────────────────────────────────────────────────

/// A generation-checked resource identifier.
///
/// The `gen` field is bumped each time a slot is recycled so that a stale
/// handle cannot accidentally alias a newly allocated resource.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ResourceId {
    /// Generation counter for this slot.
    pub gen: u32,
    /// Index into the backing store.
    pub idx: u32,
}

// ── Handle newtypes ───────────────────────────────────────────────────────────

/// An opaque handle to a GPU texture resource.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextureHandle(
    /// The underlying resource identifier.
    pub ResourceId,
);

/// An opaque handle to a GPU shader resource.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ShaderHandle(
    /// The underlying resource identifier.
    pub ResourceId,
);

// ── ResourceEntry ─────────────────────────────────────────────────────────────

/// A slot in the registry backing store.
struct ResourceEntry {
    /// Stored value (e.g. an asset name or path).
    value: String,
    /// Live reference count.  Zero means the slot is free.
    ref_count: u32,
    /// Current slot generation — incremented when the slot is recycled.
    gen: u32,
}

// ── ResourceRegistry ──────────────────────────────────────────────────────────

/// Shared registry of GPU textures and shaders, with generation-checked handles
/// and reference counting.
///
/// Slots are recycled via a free list when their reference count reaches zero.
/// The generation counter prevents stale handles from aliasing new allocations.
pub struct ResourceRegistry {
    /// Storage for texture entries.
    texture_store: Vec<ResourceEntry>,
    /// Storage for shader entries.
    shader_store: Vec<ResourceEntry>,
    /// Indices of free texture slots (ref_count == 0).
    free_texture: Vec<usize>,
    /// Indices of free shader slots (ref_count == 0).
    free_shader: Vec<usize>,
}

impl ResourceRegistry {
    /// Construct an empty registry.
    pub fn new() -> Self {
        Self {
            texture_store: Vec::new(),
            shader_store: Vec::new(),
            free_texture: Vec::new(),
            free_shader: Vec::new(),
        }
    }

    // ── Texture API ──────────────────────────────────────────────────────────

    /// Allocate a texture slot with an initial reference count of 1.
    pub fn alloc_texture(&mut self, name: String) -> TextureHandle {
        let id = if let Some(idx) = self.free_texture.pop() {
            let entry = &mut self.texture_store[idx];
            entry.value = name;
            entry.ref_count = 1;
            ResourceId {
                gen: entry.gen,
                idx: idx as u32,
            }
        } else {
            let idx = self.texture_store.len();
            self.texture_store.push(ResourceEntry {
                value: name,
                ref_count: 1,
                gen: 0,
            });
            ResourceId {
                gen: 0,
                idx: idx as u32,
            }
        };
        TextureHandle(id)
    }

    /// Increment the reference count of `h`.
    ///
    /// Returns `false` if the handle is stale (generation mismatch or
    /// out-of-bounds); returns `true` on success.
    pub fn retain_texture(&mut self, h: TextureHandle) -> bool {
        let id = h.0;
        let idx = id.idx as usize;
        match self.texture_store.get_mut(idx) {
            Some(entry) if entry.gen == id.gen && entry.ref_count > 0 => {
                entry.ref_count += 1;
                true
            }
            _ => false,
        }
    }

    /// Decrement the reference count of `h`.
    ///
    /// When the count reaches zero the slot's generation is bumped and the
    /// slot is pushed onto the free list for reuse.  Stale or already-freed
    /// handles are silently ignored.
    pub fn release_texture(&mut self, h: TextureHandle) {
        let id = h.0;
        let idx = id.idx as usize;
        if let Some(entry) = self.texture_store.get_mut(idx) {
            if entry.gen == id.gen && entry.ref_count > 0 {
                entry.ref_count -= 1;
                if entry.ref_count == 0 {
                    entry.gen = entry.gen.wrapping_add(1);
                    self.free_texture.push(idx);
                }
            }
        }
    }

    /// Return the name associated with `h`, or `None` if stale.
    pub fn get_texture(&self, h: TextureHandle) -> Option<&str> {
        let id = h.0;
        let idx = id.idx as usize;
        let entry = self.texture_store.get(idx)?;
        if entry.gen == id.gen && entry.ref_count > 0 {
            Some(&entry.value)
        } else {
            None
        }
    }

    // ── Shader API ───────────────────────────────────────────────────────────

    /// Allocate a shader slot with an initial reference count of 1.
    pub fn alloc_shader(&mut self, name: String) -> ShaderHandle {
        let id = if let Some(idx) = self.free_shader.pop() {
            let entry = &mut self.shader_store[idx];
            entry.value = name;
            entry.ref_count = 1;
            ResourceId {
                gen: entry.gen,
                idx: idx as u32,
            }
        } else {
            let idx = self.shader_store.len();
            self.shader_store.push(ResourceEntry {
                value: name,
                ref_count: 1,
                gen: 0,
            });
            ResourceId {
                gen: 0,
                idx: idx as u32,
            }
        };
        ShaderHandle(id)
    }

    /// Increment the reference count of `h`.
    ///
    /// Returns `false` if the handle is stale; returns `true` on success.
    pub fn retain_shader(&mut self, h: ShaderHandle) -> bool {
        let id = h.0;
        let idx = id.idx as usize;
        match self.shader_store.get_mut(idx) {
            Some(entry) if entry.gen == id.gen && entry.ref_count > 0 => {
                entry.ref_count += 1;
                true
            }
            _ => false,
        }
    }

    /// Decrement the reference count of `h`.
    ///
    /// When the count reaches zero the slot is freed and the generation is
    /// bumped.  Stale handles are silently ignored.
    pub fn release_shader(&mut self, h: ShaderHandle) {
        let id = h.0;
        let idx = id.idx as usize;
        if let Some(entry) = self.shader_store.get_mut(idx) {
            if entry.gen == id.gen && entry.ref_count > 0 {
                entry.ref_count -= 1;
                if entry.ref_count == 0 {
                    entry.gen = entry.gen.wrapping_add(1);
                    self.free_shader.push(idx);
                }
            }
        }
    }

    /// Return the name associated with `h`, or `None` if stale.
    pub fn get_shader(&self, h: ShaderHandle) -> Option<&str> {
        let id = h.0;
        let idx = id.idx as usize;
        let entry = self.shader_store.get(idx)?;
        if entry.gen == id.gen && entry.ref_count > 0 {
            Some(&entry.value)
        } else {
            None
        }
    }
}

impl Default for ResourceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ── RAII guards ───────────────────────────────────────────────────────────────

/// RAII wrapper that releases a [`TextureHandle`] when dropped.
pub struct TextureGuard {
    /// The guarded texture handle.
    pub handle: TextureHandle,
    /// Shared access to the registry that owns `handle`.
    registry: Rc<RefCell<ResourceRegistry>>,
}

impl TextureGuard {
    /// Construct a guard that will release `handle` on drop.
    pub fn new(handle: TextureHandle, registry: Rc<RefCell<ResourceRegistry>>) -> Self {
        Self { handle, registry }
    }
}

impl Drop for TextureGuard {
    fn drop(&mut self) {
        self.registry.borrow_mut().release_texture(self.handle);
    }
}

/// RAII wrapper that releases a [`ShaderHandle`] when dropped.
pub struct ShaderGuard {
    /// The guarded shader handle.
    pub handle: ShaderHandle,
    /// Shared access to the registry that owns `handle`.
    registry: Rc<RefCell<ResourceRegistry>>,
}

impl ShaderGuard {
    /// Construct a guard that will release `handle` on drop.
    pub fn new(handle: ShaderHandle, registry: Rc<RefCell<ResourceRegistry>>) -> Self {
        Self { handle, registry }
    }
}

impl Drop for ShaderGuard {
    fn drop(&mut self) {
        self.registry.borrow_mut().release_shader(self.handle);
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_alloc_retain_release_raii() {
        let registry = Rc::new(RefCell::new(ResourceRegistry::new()));

        // Allocate with ref_count = 1.
        let handle = registry.borrow_mut().alloc_texture("tex_a".to_string());
        assert_eq!(registry.borrow().get_texture(handle), Some("tex_a"));

        // retain bumps to 2.
        assert!(registry.borrow_mut().retain_texture(handle));

        // First release → count 1, still live.
        registry.borrow_mut().release_texture(handle);
        assert_eq!(registry.borrow().get_texture(handle), Some("tex_a"));

        // RAII guard releases the second ref on drop.
        {
            let _guard = TextureGuard::new(handle, Rc::clone(&registry));
        }
        // After guard drop, count == 0 → slot freed, handle stale.
        assert_eq!(registry.borrow().get_texture(handle), None);
    }

    #[test]
    fn resource_double_release_is_safe() {
        let mut reg = ResourceRegistry::new();
        let h = reg.alloc_texture("tex_b".to_string());
        reg.release_texture(h);
        // Second release on the now-stale handle must not panic.
        reg.release_texture(h);
        // Slot is freed; a new alloc should reuse it.
        let h2 = reg.alloc_texture("tex_c".to_string());
        assert_eq!(reg.get_texture(h2), Some("tex_c"));
        // The old handle is still stale.
        assert_eq!(reg.get_texture(h), None);
    }
}
