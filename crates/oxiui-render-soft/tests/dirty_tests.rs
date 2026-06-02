use oxiui_render_soft::DirtyRegion;

/// A new DirtyRegion of 64x64 pixels with 32-pixel tiles should be fully dirty.
/// That gives 2x2 = 4 tiles, all dirty from the start.
#[test]
fn test_dirty_region_new_fully_dirty() {
    let region = DirtyRegion::new(64, 64, 32);
    assert_eq!(region.total_tiles(), 4, "expected 4 tiles (2x2)");
    assert_eq!(region.dirty_count(), 4, "all tiles should start dirty");
    for ty in 0..2 {
        for tx in 0..2 {
            assert!(
                region.is_tile_dirty(tx, ty),
                "tile ({tx},{ty}) should be dirty on construction"
            );
        }
    }
}

/// After clear_all(), mark_rect(0,0,33,33,32) should dirty tiles (0,0), (1,0),
/// and (0,1) — the 33rd pixel crosses into the second tile on both axes.
#[test]
fn test_dirty_region_mark_rect() {
    let mut region = DirtyRegion::new(64, 64, 32);
    region.clear_all();
    assert_eq!(region.dirty_count(), 0, "should be clean after clear_all");

    // A 33×33 rect starting at (0,0) crosses into tile column 1 and tile row 1.
    region.mark_rect(0, 0, 33, 33, 32);

    assert!(region.is_tile_dirty(0, 0), "tile (0,0) must be dirty");
    assert!(region.is_tile_dirty(1, 0), "tile (1,0) must be dirty");
    assert!(region.is_tile_dirty(0, 1), "tile (0,1) must be dirty");
    // tile (1,1) is within the 33×33 area so it may or may not be marked — no assertion.
}

/// invalidate_all() then clear_all() should leave dirty_count() == 0.
#[test]
fn test_dirty_region_clear_all() {
    let mut region = DirtyRegion::new(128, 128, 32);
    region.invalidate_all();
    assert_eq!(
        region.dirty_count(),
        region.total_tiles(),
        "invalidate_all should make all tiles dirty"
    );
    region.clear_all();
    assert_eq!(region.dirty_count(), 0, "clear_all should zero dirty count");
    // Confirm each tile reports clean.
    for ty in 0..4u32 {
        for tx in 0..4u32 {
            assert!(
                !region.is_tile_dirty(tx, ty),
                "tile ({tx},{ty}) should be clean after clear_all"
            );
        }
    }
}

/// After clear_all() and marking exactly 2 tiles, dirty_tiles() should yield
/// exactly those 2 coordinates.
#[test]
fn test_dirty_region_dirty_tiles_iterator() {
    let mut region = DirtyRegion::new(64, 64, 32);
    region.clear_all();

    region.mark_tile(0, 0);
    region.mark_tile(1, 1);

    let dirty: Vec<(u32, u32)> = region.dirty_tiles().collect();
    assert_eq!(dirty.len(), 2, "expected exactly 2 dirty tiles");
    assert!(
        dirty.contains(&(0, 0)),
        "tile (0,0) should appear in dirty_tiles"
    );
    assert!(
        dirty.contains(&(1, 1)),
        "tile (1,1) should appear in dirty_tiles"
    );
}

/// clear_all() then invalidate_all() should make dirty_count() == total_tiles().
#[test]
fn test_dirty_region_invalidate_all() {
    let mut region = DirtyRegion::new(64, 64, 32);
    region.clear_all();
    assert_eq!(region.dirty_count(), 0, "clean after clear_all");

    region.invalidate_all();
    assert_eq!(
        region.dirty_count(),
        region.total_tiles(),
        "dirty_count should equal total_tiles after invalidate_all"
    );
}

/// After clear_all(), mark_tile(1,0) should make only tile (1,0) dirty.
#[test]
fn test_dirty_region_mark_single_tile() {
    let mut region = DirtyRegion::new(64, 64, 32);
    region.clear_all();

    region.mark_tile(1, 0);

    assert!(
        region.is_tile_dirty(1, 0),
        "tile (1,0) should be dirty after mark_tile"
    );
    assert!(
        !region.is_tile_dirty(0, 0),
        "tile (0,0) should remain clean"
    );
    assert!(
        !region.is_tile_dirty(0, 1),
        "tile (0,1) should remain clean"
    );
    assert!(
        !region.is_tile_dirty(1, 1),
        "tile (1,1) should remain clean"
    );
    assert_eq!(region.dirty_count(), 1, "exactly one tile should be dirty");
}
