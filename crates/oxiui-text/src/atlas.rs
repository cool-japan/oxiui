//! Glyph atlas: LRU cache of pre-rasterized glyph bitmaps.
//!
//! [`GlyphAtlas`] maps [`GlyphKey`] (glyph ID + size + subpixel offset) to
//! [`GlyphEntry`] (bitmap + advance + bearing), providing a shared bitmap
//! store so CPU render backends avoid re-rasterizing the same glyph on every
//! frame.

use crate::{TextPipeline, TextStyle};
use oxitext::Bitmap;
use oxiui_core::UiError;
use std::collections::{HashMap, VecDeque};

// ── GlyphKey ──────────────────────────────────────────────────────────────────

/// Cache key identifying a specific rendered glyph at a given size and
/// subpixel position.
///
/// Subpixel offsets are quantized to 1/16 pixel steps to bound the number of
/// distinct cache entries per glyph while still allowing subpixel-accurate
/// rendering.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct GlyphKey {
    /// OpenType glyph ID.
    pub glyph_id: u16,
    /// Font size in 1/16-pixel units (`font_size * 16.0` as `u32`).
    pub font_size_pixels: u32,
    /// Quantized subpixel offset as `(x_16ths, y_16ths)` where each component
    /// is in `0..16`.
    pub subpixel_offset_16ths: (u8, u8),
}

impl GlyphKey {
    /// Construct a [`GlyphKey`] from a glyph ID, a font size in pixels, and
    /// the sub-pixel fractional pen position.
    ///
    /// `subpixel_x` and `subpixel_y` should be the fractional parts of the
    /// pen position (i.e. `pen_x.fract()` and `pen_y.fract()`).
    pub fn new(glyph_id: u16, font_size: f32, subpixel_x: f32, subpixel_y: f32) -> Self {
        GlyphKey {
            glyph_id,
            font_size_pixels: (font_size * 16.0) as u32,
            subpixel_offset_16ths: (
                ((subpixel_x.fract() * 16.0) as u8).min(15),
                ((subpixel_y.fract() * 16.0) as u8).min(15),
            ),
        }
    }
}

// ── GlyphEntry ────────────────────────────────────────────────────────────────

/// A cached rasterized glyph with its bitmap and positioning metrics.
#[derive(Clone, Debug)]
pub struct GlyphEntry {
    /// Greyscale alpha-coverage bitmap for this glyph.
    pub bitmap: Bitmap,
    /// Horizontal advance width in pixels.
    pub advance_x: f32,
    /// `(bearing_x, bearing_y)` in pixels — distance from pen position to the
    /// left and top edges of the bitmap, respectively.
    pub bearing: (i32, i32),
}

// ── GlyphAtlas ────────────────────────────────────────────────────────────────

/// LRU cache of pre-rasterized glyph bitmaps.
///
/// The atlas is designed to be shared across frames by CPU render backends.
/// When the cache is full the least-recently-used entry is evicted to make
/// room for new glyphs.
///
/// # Example
/// ```ignore
/// let mut atlas = GlyphAtlas::new(1024);
/// let key = GlyphKey::new(42, 16.0, 0.0, 0.0);
/// let entry = atlas.get_or_rasterize(&mut pipeline, key, "A", &style)?;
/// // use entry.bitmap …
/// ```
pub struct GlyphAtlas {
    /// Glyph entries keyed by [`GlyphKey`].
    cache: HashMap<GlyphKey, GlyphEntry>,
    /// Tracks insertion order for LRU eviction (front = oldest, back = newest).
    lru: VecDeque<GlyphKey>,
    /// Maximum number of entries before eviction starts.
    max_entries: usize,
}

impl GlyphAtlas {
    /// Create a new [`GlyphAtlas`] with the given capacity.
    ///
    /// When the number of cached entries reaches `max_entries`, the
    /// least-recently-used entry is evicted before inserting a new one.
    pub fn new(max_entries: usize) -> Self {
        GlyphAtlas {
            cache: HashMap::new(),
            lru: VecDeque::new(),
            max_entries,
        }
    }

    /// Return the cached [`GlyphEntry`] for `key`, or `None` if not present.
    ///
    /// This is a pure lookup and does **not** update the LRU order.  Call
    /// [`Self::get_or_rasterize`] when you need automatic rasterization and
    /// LRU promotion.
    pub fn get(&self, key: &GlyphKey) -> Option<&GlyphEntry> {
        self.cache.get(key)
    }

    /// Return or rasterize the glyph described by `key`.
    ///
    /// - On a cache hit the entry is promoted to the back of the LRU queue and
    ///   returned by reference.
    /// - On a cache miss `pipeline.render(text, style)` is called to rasterize
    ///   the glyph.  If the atlas is at capacity the oldest entry is evicted
    ///   before inserting the new one.
    ///
    /// # Errors
    /// Returns [`UiError::Render`] if the pipeline fails to rasterize, or
    /// [`UiError::Other`] if the requested glyph ID is not present in the
    /// render result.
    pub fn get_or_rasterize(
        &mut self,
        pipeline: &mut TextPipeline,
        key: GlyphKey,
        text: &str,
        style: &TextStyle,
    ) -> Result<&GlyphEntry, UiError> {
        if self.cache.contains_key(&key) {
            // Promote to back of LRU queue (most recently used).
            if let Some(pos) = self.lru.iter().position(|k| k == &key) {
                self.lru.remove(pos);
            }
            self.lru.push_back(key.clone());
            return self
                .cache
                .get(&key)
                .ok_or_else(|| UiError::Other("atlas: key confirmed but missing in map".into()));
        }

        // Rasterize via the pipeline.
        let result = pipeline.render(text, style)?;

        // Find the glyph matching our requested glyph_id.
        let entry = result
            .glyphs
            .iter()
            .zip(result.bitmaps.iter())
            .find(|(g, _)| g.gid == key.glyph_id)
            .map(|(g, bm)| GlyphEntry {
                bitmap: bm.clone(),
                advance_x: g.advance_x,
                bearing: (0, 0),
            })
            .ok_or_else(|| {
                UiError::Other(format!(
                    "atlas: glyph id {} not found in render result",
                    key.glyph_id
                ))
            })?;

        // Evict oldest entries until we have room for one more.
        if self.max_entries > 0 {
            self.evict_to(self.max_entries - 1);
        }

        self.lru.push_back(key.clone());
        self.cache.insert(key.clone(), entry);

        self.cache
            .get(&key)
            .ok_or_else(|| UiError::Other("atlas: entry missing immediately after insert".into()))
    }

    /// Evict entries from the front of the LRU queue until `cache.len() <= max`.
    pub fn evict_to(&mut self, max: usize) {
        while self.cache.len() > max {
            match self.lru.pop_front() {
                Some(oldest) => {
                    self.cache.remove(&oldest);
                }
                None => break,
            }
        }
    }

    /// Return the number of cached glyph entries.
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Return `true` when the atlas contains no cached entries.
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    /// Return the fraction of capacity in use as a value in `0.0..=1.0`.
    ///
    /// Returns `0.0` when `max_entries` is zero to avoid division by zero.
    pub fn utilization(&self) -> f32 {
        if self.max_entries == 0 {
            return 0.0;
        }
        self.cache.len() as f32 / self.max_entries as f32
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal `GlyphEntry` for tests that don't touch the pipeline.
    fn dummy_entry(advance: f32) -> GlyphEntry {
        GlyphEntry {
            bitmap: Bitmap {
                width: 1,
                height: 1,
                pixels: vec![128],
            },
            advance_x: advance,
            bearing: (0, 0),
        }
    }

    /// Insert `count` dummy entries into the atlas without using the pipeline.
    fn fill_atlas(atlas: &mut GlyphAtlas, count: u16) {
        for i in 0..count {
            let key = GlyphKey {
                glyph_id: i,
                font_size_pixels: 256,
                subpixel_offset_16ths: (0, 0),
            };
            atlas.cache.insert(key.clone(), dummy_entry(8.0));
            atlas.lru.push_back(key);
        }
    }

    // 1. Utilization formula: 5 entries in a capacity-10 atlas → 0.5.
    #[test]
    fn utilization_half_capacity() {
        let mut atlas = GlyphAtlas::new(10);
        fill_atlas(&mut atlas, 5);
        let u = atlas.utilization();
        assert!((u - 0.5).abs() < f32::EPSILON, "expected 0.5, got {u}");
    }

    // 2. Utilization is 0 for empty atlas.
    #[test]
    fn utilization_empty() {
        let atlas = GlyphAtlas::new(10);
        assert!((atlas.utilization() - 0.0).abs() < f32::EPSILON);
    }

    // 3. evict_to reduces length to the target.
    #[test]
    fn evict_to_reduces_length() {
        let mut atlas = GlyphAtlas::new(10);
        fill_atlas(&mut atlas, 5);
        assert_eq!(atlas.len(), 5);

        atlas.evict_to(2);
        assert_eq!(atlas.len(), 2);
    }

    // 4. LRU eviction: adding (max+1) entries leaves only the newest `max`.
    #[test]
    fn lru_eviction_drops_oldest() {
        let mut atlas = GlyphAtlas::new(10);
        fill_atlas(&mut atlas, 11); // 11 entries, capacity = 10

        // The oldest key (glyph_id = 0) was pushed first.
        let oldest_key = GlyphKey {
            glyph_id: 0,
            font_size_pixels: 256,
            subpixel_offset_16ths: (0, 0),
        };
        // We haven't called evict_to yet (fill_atlas bypasses it), so manually evict.
        atlas.evict_to(10);

        assert_eq!(atlas.len(), 10);
        assert!(
            atlas.get(&oldest_key).is_none(),
            "oldest entry should have been evicted"
        );
    }

    // 5. GlyphKey::new encodes font_size_pixels correctly.
    #[test]
    fn glyph_key_font_size_encoding() {
        let key = GlyphKey::new(7, 16.0, 0.0, 0.0);
        // 16.0 * 16 = 256
        assert_eq!(key.font_size_pixels, 256u32);
        assert_eq!(key.glyph_id, 7);
    }

    // 6. GlyphKey::new encodes subpixel offsets in 0..16.
    #[test]
    fn glyph_key_subpixel_quantization() {
        let key = GlyphKey::new(1, 12.0, 0.5, 0.25);
        // 0.5 * 16 = 8 → 8; 0.25 * 16 = 4 → 4
        assert_eq!(key.subpixel_offset_16ths, (8, 4));
    }

    // 7. get() returns None for missing key.
    #[test]
    fn get_returns_none_for_missing_key() {
        let atlas = GlyphAtlas::new(10);
        let key = GlyphKey::new(99, 16.0, 0.0, 0.0);
        assert!(atlas.get(&key).is_none());
    }

    // 8. is_empty() reflects cache state.
    #[test]
    fn is_empty_reflects_state() {
        let mut atlas = GlyphAtlas::new(10);
        assert!(atlas.is_empty());
        fill_atlas(&mut atlas, 1);
        assert!(!atlas.is_empty());
    }

    // 9. get() returns inserted entry with matching advance_x.
    #[test]
    fn get_returns_inserted_entry() {
        let mut atlas = GlyphAtlas::new(10);
        let key = GlyphKey {
            glyph_id: 42,
            font_size_pixels: 256,
            subpixel_offset_16ths: (0, 0),
        };
        let entry = dummy_entry(12.5);
        atlas.cache.insert(key.clone(), entry);
        atlas.lru.push_back(key.clone());

        let result = atlas.get(&key);
        assert!(result.is_some());
        let e = result.expect("entry present");
        assert!((e.advance_x - 12.5).abs() < f32::EPSILON);
    }

    // 10. evict_to with max >= len is a no-op.
    #[test]
    fn evict_to_noop_when_within_capacity() {
        let mut atlas = GlyphAtlas::new(10);
        fill_atlas(&mut atlas, 3);
        atlas.evict_to(5); // target > current len
        assert_eq!(atlas.len(), 3);
    }
}
