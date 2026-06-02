//! 64×64 render-tile iterator.
//!
//! Splits the framebuffer into a grid of `TILE_SIZE × TILE_SIZE` tiles,
//! clamped to the framebuffer boundary. Tiles are independent of each other
//! and can be rendered in parallel (rayon-ready); this run uses a serial
//! driver.

use crate::clip::ClipRect;

/// Default tile side length in pixels.
pub const DEFAULT_TILE_SIZE: u32 = 64;

/// A single render tile, identified by its top-left corner and its pixel
/// extents (which may be smaller than `DEFAULT_TILE_SIZE` at boundaries).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tile {
    /// Left edge of the tile (column index, inclusive).
    pub x: u32,
    /// Top edge of the tile (row index, inclusive).
    pub y: u32,
    /// Width in pixels (1–`DEFAULT_TILE_SIZE`).
    pub w: u32,
    /// Height in pixels (1–`DEFAULT_TILE_SIZE`).
    pub h: u32,
}

impl Tile {
    /// Returns the `ClipRect` corresponding to this tile.
    pub fn clip_rect(&self) -> ClipRect {
        ClipRect {
            x0: self.x as i64,
            y0: self.y as i64,
            x1: (self.x + self.w) as i64,
            y1: (self.y + self.h) as i64,
        }
    }
}

/// Iterator over the tiles covering a [`ClipRect`], clamped to the
/// framebuffer size.
pub struct TileIter {
    rect: ClipRect,
    fb_w: u32,
    fb_h: u32,
    tile_size: u32,
    cur_x: u32,
    cur_y: u32,
    done: bool,
}

impl TileIter {
    fn new(rect: ClipRect, fb_w: u32, fb_h: u32, tile_size: u32) -> Self {
        let ts = tile_size.max(1);
        // Clamp the iteration rectangle to the framebuffer.
        let rx0 = rect.x0.max(0) as u32;
        let ry0 = rect.y0.max(0) as u32;
        let clamped = ClipRect {
            x0: rx0 as i64,
            y0: ry0 as i64,
            x1: (rect.x1 as u32).min(fb_w) as i64,
            y1: (rect.y1 as u32).min(fb_h) as i64,
        };
        let done = clamped.is_empty();
        Self {
            rect: clamped,
            fb_w,
            fb_h,
            tile_size: ts,
            cur_x: rx0,
            cur_y: ry0,
            done,
        }
    }
}

impl Iterator for TileIter {
    type Item = Tile;

    fn next(&mut self) -> Option<Tile> {
        if self.done {
            return None;
        }
        let x0 = self.cur_x;
        let y0 = self.cur_y;
        if x0 >= self.fb_w || y0 >= self.fb_h {
            return None;
        }
        if (x0 as i64) >= self.rect.x1 || (y0 as i64) >= self.rect.y1 {
            return None;
        }

        let x_end = ((x0 + self.tile_size) as i64).min(self.rect.x1) as u32;
        let y_end = ((y0 + self.tile_size) as i64).min(self.rect.y1) as u32;
        let w = x_end.saturating_sub(x0);
        let h = y_end.saturating_sub(y0);

        if w == 0 || h == 0 {
            return None;
        }

        let tile = Tile { x: x0, y: y0, w, h };

        // Advance to the next position.
        let next_x = x0 + self.tile_size;
        if (next_x as i64) < self.rect.x1 {
            self.cur_x = next_x;
        } else {
            // Move to the next row.
            self.cur_x = self.rect.x0 as u32;
            self.cur_y = y0 + self.tile_size;
            if (self.cur_y as i64) >= self.rect.y1 {
                self.done = true;
            }
        }

        Some(tile)
    }
}

/// Return an iterator over `DEFAULT_TILE_SIZE`-sized tiles covering `rect`,
/// clamped to the framebuffer dimensions `fb_w × fb_h`.
pub fn tiles_for(rect: ClipRect, fb_w: u32, fb_h: u32) -> TileIter {
    TileIter::new(rect, fb_w, fb_h, DEFAULT_TILE_SIZE)
}

/// Apply closure `f` to each tile covering `rect`.
///
/// Currently serial. Designed to be swapped for a rayon parallel iterator
/// when `rayon` becomes a workspace dependency.
pub fn render_tiles<F>(rect: ClipRect, fb_w: u32, fb_h: u32, mut f: F)
where
    F: FnMut(Tile),
{
    for tile in tiles_for(rect, fb_w, fb_h) {
        f(tile);
    }
}

/// Collect all tiles covering `rect` into an owned `Vec<Tile>`.
///
/// Useful when you need to iterate multiple times or share the list across
/// threads.
pub fn collect_tiles(rect: ClipRect, fb_w: u32, fb_h: u32) -> Vec<Tile> {
    tiles_for(rect, fb_w, fb_h).collect()
}

/// Render tiles in parallel using rayon, collecting per-tile pixel buffers.
///
/// Each tile is rendered independently into its own `Vec<u32>` scratch buffer
/// by calling `render_fn(tile)`.  The results are returned in the same order
/// as `tiles`, ready for the caller to merge back into the main framebuffer.
///
/// The merge pass is the caller's responsibility.  A typical merge:
/// ```ignore
/// for (tile, pixels) in results {
///     for row in 0..tile.h {
///         let src_start = (row * tile.w) as usize;
///         let dst_start = ((tile.y + row) * fb_w + tile.x) as usize;
///         fb_pixels[dst_start..dst_start + tile.w as usize]
///             .copy_from_slice(&pixels[src_start..src_start + tile.w as usize]);
///     }
/// }
/// ```
///
/// # Behaviour when `parallel` feature is disabled
///
/// This function is only available under `#[cfg(feature = "parallel")]`.
/// The sequential equivalent is [`render_tiles`] / [`collect_tiles`].
#[cfg(feature = "parallel")]
pub fn render_parallel<F>(tiles: &[Tile], render_fn: F) -> Vec<(Tile, Vec<u32>)>
where
    F: Fn(&Tile) -> Vec<u32> + Send + Sync,
{
    use rayon::prelude::*;
    tiles
        .par_iter()
        .map(|tile| (*tile, render_fn(tile)))
        .collect()
}

// ---------------------------------------------------------------------------
// DirtyRegion
// ---------------------------------------------------------------------------

/// Tracks which tiles of the framebuffer have been modified since the last
/// clear, enabling callers to skip re-compositing unchanged tiles.
///
/// On construction every tile is marked dirty so the first repaint always
/// paints the full surface. After [`DirtyRegion::clear_all`] only tiles
/// touched by [`DirtyRegion::mark_rect`] or [`DirtyRegion::mark_tile`] are
/// reported as dirty.
#[derive(Debug, Clone)]
pub struct DirtyRegion {
    /// Width of the framebuffer in tiles.
    tiles_wide: u32,
    /// Height of the framebuffer in tiles.
    tiles_tall: u32,
    /// Bit set: `dirty[ty * tiles_wide + tx]` is `true` if that tile is dirty.
    dirty: Vec<bool>,
    /// `true` if every tile is dirty (avoids per-tile iteration on full invalidation).
    all_dirty: bool,
}

impl DirtyRegion {
    /// Create a new [`DirtyRegion`] for a framebuffer of `fb_width × fb_height`
    /// pixels using `tile_size`-pixel tiles.
    ///
    /// All tiles start dirty so the first frame is always fully rendered.
    pub fn new(fb_width: u32, fb_height: u32, tile_size: u32) -> Self {
        let ts = tile_size.max(1);
        let tw = fb_width.div_ceil(ts);
        let th = fb_height.div_ceil(ts);
        Self {
            tiles_wide: tw,
            tiles_tall: th,
            dirty: vec![true; (tw * th) as usize],
            all_dirty: true,
        }
    }

    /// Mark the pixel rectangle `(x, y, width, height)` as dirty, dirtying
    /// every tile that overlaps it.
    ///
    /// If all tiles are already dirty this is a no-op.
    pub fn mark_rect(&mut self, x: u32, y: u32, width: u32, height: u32, tile_size: u32) {
        if self.all_dirty {
            return;
        }
        let ts = tile_size.max(1);
        let tx_start = x / ts;
        let ty_start = y / ts;
        let tx_end = (x + width).div_ceil(ts);
        let ty_end = (y + height).div_ceil(ts);
        for ty in ty_start..ty_end.min(self.tiles_tall) {
            for tx in tx_start..tx_end.min(self.tiles_wide) {
                let idx = (ty * self.tiles_wide + tx) as usize;
                if idx < self.dirty.len() {
                    self.dirty[idx] = true;
                }
            }
        }
    }

    /// Mark the single tile at `(tx, ty)` as dirty.
    ///
    /// If all tiles are already dirty this is a no-op.
    pub fn mark_tile(&mut self, tx: u32, ty: u32) {
        if self.all_dirty {
            return;
        }
        let idx = (ty * self.tiles_wide + tx) as usize;
        if idx < self.dirty.len() {
            self.dirty[idx] = true;
        }
    }

    /// Return `true` if the tile at `(tx, ty)` needs to be re-rendered.
    pub fn is_tile_dirty(&self, tx: u32, ty: u32) -> bool {
        if self.all_dirty {
            return true;
        }
        let idx = (ty * self.tiles_wide + tx) as usize;
        self.dirty.get(idx).copied().unwrap_or(false)
    }

    /// Clear all dirty flags after a completed repaint.
    ///
    /// After this call [`DirtyRegion::dirty_count`] returns `0` until a tile
    /// is explicitly marked dirty again.
    pub fn clear_all(&mut self) {
        self.dirty.fill(false);
        self.all_dirty = false;
    }

    /// Mark every tile as dirty.
    ///
    /// Use after a resize or any full framebuffer invalidation.
    pub fn invalidate_all(&mut self) {
        self.dirty.fill(true);
        self.all_dirty = true;
    }

    /// Iterate over the `(tx, ty)` coordinates of all dirty tiles.
    pub fn dirty_tiles(&self) -> impl Iterator<Item = (u32, u32)> + '_ {
        (0..self.tiles_tall).flat_map(move |ty| {
            (0..self.tiles_wide).filter_map(move |tx| {
                if self.is_tile_dirty(tx, ty) {
                    Some((tx, ty))
                } else {
                    None
                }
            })
        })
    }

    /// Return the number of tiles currently marked dirty.
    pub fn dirty_count(&self) -> usize {
        if self.all_dirty {
            (self.tiles_wide * self.tiles_tall) as usize
        } else {
            self.dirty.iter().filter(|&&d| d).count()
        }
    }

    /// Return the total number of tiles in the framebuffer.
    pub fn total_tiles(&self) -> usize {
        (self.tiles_wide * self.tiles_tall) as usize
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiles_cover_full_frame() {
        let w = 200u32;
        let h = 150u32;
        let rect = ClipRect::full(w, h);
        let tiles: Vec<Tile> = tiles_for(rect, w, h).collect();

        // All tiles combined must cover the full framebuffer with no overlaps.
        let mut coverage = vec![0u32; (w * h) as usize];
        for tile in &tiles {
            for ty in tile.y..tile.y + tile.h {
                for tx in tile.x..tile.x + tile.w {
                    coverage[(ty * w + tx) as usize] += 1;
                }
            }
        }
        for (i, &c) in coverage.iter().enumerate() {
            assert_eq!(c, 1, "pixel {i} covered {c} times (expected exactly 1)");
        }
    }

    #[test]
    fn tile_covers_frame() {
        // The tiles for a full framebuffer should have no gaps and no overlaps.
        let w = 64u32;
        let h = 64u32;
        let rect = ClipRect::full(w, h);
        let tiles: Vec<Tile> = tiles_for(rect, w, h).collect();
        // A 64x64 buffer with 64x64 tiles should yield exactly 1 tile.
        assert_eq!(tiles.len(), 1);
        assert_eq!(
            tiles[0],
            Tile {
                x: 0,
                y: 0,
                w: 64,
                h: 64
            }
        );
    }

    #[test]
    fn tiles_non_overlapping() {
        // 128x128 buffer → 4 tiles of 64x64.
        let w = 128u32;
        let h = 128u32;
        let rect = ClipRect::full(w, h);
        let tiles: Vec<Tile> = tiles_for(rect, w, h).collect();
        assert_eq!(tiles.len(), 4);
        // Verify no two tiles share a pixel.
        let mut coverage = vec![0u32; (w * h) as usize];
        for tile in &tiles {
            for ty in tile.y..tile.y + tile.h {
                for tx in tile.x..tile.x + tile.w {
                    coverage[(ty * w + tx) as usize] += 1;
                }
            }
        }
        for &c in &coverage {
            assert_eq!(c, 1, "every pixel must be covered exactly once");
        }
    }

    #[test]
    fn partial_boundary_tiles() {
        // 70x70 buffer → boundary tiles are smaller than 64x64.
        let w = 70u32;
        let h = 70u32;
        let rect = ClipRect::full(w, h);
        let tiles: Vec<Tile> = tiles_for(rect, w, h).collect();
        // Should have 4 tiles: 64x64, 6x64, 64x6, 6x6.
        assert_eq!(tiles.len(), 4);
        let mut coverage = vec![0u32; (w * h) as usize];
        for tile in &tiles {
            for ty in tile.y..tile.y + tile.h {
                for tx in tile.x..tile.x + tile.w {
                    coverage[(ty * w + tx) as usize] += 1;
                }
            }
        }
        for (i, &c) in coverage.iter().enumerate() {
            assert_eq!(c, 1, "pixel {i} covered {c} times");
        }
    }

    #[test]
    fn tile_clip_rect_correct() {
        let tile = Tile {
            x: 64,
            y: 128,
            w: 32,
            h: 16,
        };
        let clip = tile.clip_rect();
        assert_eq!(clip.x0, 64);
        assert_eq!(clip.y0, 128);
        assert_eq!(clip.x1, 96);
        assert_eq!(clip.y1, 144);
    }

    #[test]
    fn render_tiles_visits_all() {
        let w = 100u32;
        let h = 100u32;
        let rect = ClipRect::full(w, h);
        let mut count = 0u32;
        render_tiles(rect, w, h, |_tile| {
            count += 1;
        });
        // 100x100 with 64x64 tiles: 2×2 = 4 tiles.
        assert_eq!(count, 4);
    }

    #[test]
    fn collect_tiles_returns_same_as_iterator() {
        let w = 128u32;
        let h = 128u32;
        let rect = ClipRect::full(w, h);
        let via_iter: Vec<Tile> = tiles_for(rect, w, h).collect();
        let via_collect = collect_tiles(rect, w, h);
        assert_eq!(via_iter, via_collect);
    }

    /// Parallel rendering produces the same output as sequential for a
    /// deterministic render function (fill each tile with a constant colour).
    #[cfg(feature = "parallel")]
    #[test]
    fn parallel_output_matches_sequential() {
        let w = 128u32;
        let h = 128u32;
        let rect = ClipRect::full(w, h);

        // A deterministic render function: fill with a per-tile constant
        // derived from the tile's (x, y) position — no shared mutable state.
        let render_fn = |tile: &Tile| -> Vec<u32> {
            let colour: u32 =
                0xFF000000 | ((tile.x & 0xFF) << 16) | ((tile.y & 0xFF) << 8) | (tile.w & 0xFF);
            vec![colour; (tile.w * tile.h) as usize]
        };

        // Sequential baseline.
        let tiles = collect_tiles(rect, w, h);
        let sequential: Vec<(Tile, Vec<u32>)> = tiles.iter().map(|t| (*t, render_fn(t))).collect();

        // Parallel run.
        let parallel = super::render_parallel(&tiles, render_fn);

        // Sort both by tile position to ensure order independence.
        let mut seq_sorted = sequential;
        let mut par_sorted = parallel;
        seq_sorted.sort_by_key(|(t, _)| (t.y, t.x));
        par_sorted.sort_by_key(|(t, _)| (t.y, t.x));

        assert_eq!(seq_sorted.len(), par_sorted.len(), "tile count mismatch");
        for ((st, sp), (pt, pp)) in seq_sorted.iter().zip(par_sorted.iter()) {
            assert_eq!(st, pt, "tile metadata mismatch");
            assert_eq!(sp, pp, "pixel data mismatch for tile ({},{})", st.x, st.y);
        }
    }
}
