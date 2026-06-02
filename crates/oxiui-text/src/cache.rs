//! Hand-rolled LRU shaping cache — no external `lru` crate.
//!
//! Uses [`std::collections::HashMap`] for O(1) lookup and a
//! [`std::collections::VecDeque`] as a recency queue (front = most recently
//! used).

use std::collections::{HashMap, VecDeque};

use crate::ShapedText;

// ── CacheKey ──────────────────────────────────────────────────────────────────

/// Key that uniquely identifies a shaping request.
#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub struct CacheKey {
    /// The text to shape.
    pub text: String,
    /// The font family, if any.
    pub font_family: Option<String>,
    /// `font_size.to_bits()` — bit-exact f32 for hashing.
    pub font_size_bits: u32,
    /// `max_width.to_bits()`.
    pub max_width_bits: u32,
}

// ── ShapingCache ──────────────────────────────────────────────────────────────

/// LRU cache mapping [`CacheKey`] → [`ShapedText`].
///
/// Capacity is the maximum number of entries held in memory.  When capacity
/// is exceeded the least-recently-used entry is evicted.
pub struct ShapingCache {
    capacity: usize,
    entries: HashMap<CacheKey, ShapedText>,
    /// Front = most recently used, back = least recently used.
    order: VecDeque<CacheKey>,
    hits: u64,
    misses: u64,
    evictions: u64,
}

impl ShapingCache {
    /// Create a new cache with the given maximum `capacity`.
    ///
    /// A capacity of 0 is treated as effectively disabled (every lookup is a
    /// miss and nothing is stored).
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            entries: HashMap::new(),
            order: VecDeque::new(),
            hits: 0,
            misses: 0,
            evictions: 0,
        }
    }

    /// Look up `key` and return a reference to the cached [`ShapedText`] if
    /// present.
    ///
    /// On a hit the key is promoted to the front (most-recently-used)
    /// position.
    pub fn get(&mut self, key: &CacheKey) -> Option<&ShapedText> {
        if self.entries.contains_key(key) {
            self.hits += 1;
            // Promote key to the front of the recency queue.
            if let Some(pos) = self.order.iter().position(|k| k == key) {
                self.order.remove(pos);
            }
            self.order.push_front(key.clone());
            self.entries.get(key)
        } else {
            self.misses += 1;
            None
        }
    }

    /// Insert a `value` for `key`.
    ///
    /// If the cache is at capacity the least-recently-used entry is evicted
    /// first.  If `capacity == 0` the entry is never stored.
    pub fn insert(&mut self, key: CacheKey, value: ShapedText) {
        if self.capacity == 0 {
            return;
        }
        // If already present, update in place and promote.
        if self.entries.contains_key(&key) {
            if let Some(pos) = self.order.iter().position(|k| k == &key) {
                self.order.remove(pos);
            }
            self.order.push_front(key.clone());
            self.entries.insert(key, value);
            return;
        }
        // Evict LRU if at capacity.
        while self.entries.len() >= self.capacity {
            self.evict_lru();
        }
        self.order.push_front(key.clone());
        self.entries.insert(key, value);
    }

    /// Return the cache hit rate as `hits / (hits + misses)`, or `0.0` when
    /// no lookups have been performed.
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }

    /// Return `(hits, misses, evictions)`.
    pub fn stats(&self) -> (u64, u64, u64) {
        (self.hits, self.misses, self.evictions)
    }

    /// Clear all entries and reset statistics.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
        self.hits = 0;
        self.misses = 0;
        self.evictions = 0;
    }

    /// Return the number of entries currently in the cache.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` when the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Remove the least-recently-used entry (back of `order`).
    fn evict_lru(&mut self) {
        if let Some(lru_key) = self.order.pop_back() {
            self.entries.remove(&lru_key);
            self.evictions += 1;
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_key(text: &str) -> CacheKey {
        CacheKey {
            text: text.to_owned(),
            font_family: None,
            font_size_bits: 16.0_f32.to_bits(),
            max_width_bits: 0.0_f32.to_bits(),
        }
    }

    fn shaped(w: f32, h: f32) -> ShapedText {
        ShapedText {
            lines: Vec::new(),
            total_width: w,
            total_height: h,
        }
    }

    #[test]
    fn cache_hit_after_insert() {
        let mut cache = ShapingCache::new(8);
        let k = make_key("hello");
        cache.insert(k.clone(), shaped(60.0, 16.0));
        assert!(cache.get(&k).is_some());
    }

    #[test]
    fn cache_miss_after_eviction() {
        // Capacity=1: insert A then B → A is evicted, get(A) returns None.
        let mut cache = ShapingCache::new(1);
        let a = make_key("A");
        let b = make_key("B");
        cache.insert(a.clone(), shaped(10.0, 16.0));
        cache.insert(b.clone(), shaped(10.0, 16.0));
        assert!(cache.get(&a).is_none(), "A should have been evicted");
        assert!(cache.get(&b).is_some(), "B should still be present");
    }

    #[test]
    fn cache_hit_rate_tracking() {
        let mut cache = ShapingCache::new(8);
        let k = make_key("x");
        cache.insert(k.clone(), shaped(5.0, 16.0));

        // 3 hits
        cache.get(&k);
        cache.get(&k);
        cache.get(&k);
        // 2 misses
        cache.get(&make_key("missing1"));
        cache.get(&make_key("missing2"));

        let rate = cache.hit_rate();
        assert!(
            (rate - 0.6).abs() < 1e-9,
            "hit rate should be 0.6, got {rate}"
        );
    }

    #[test]
    fn cache_clear_resets_everything() {
        let mut cache = ShapingCache::new(4);
        let k = make_key("hello");
        cache.insert(k.clone(), shaped(10.0, 16.0));
        cache.get(&k);
        cache.clear();
        assert!(cache.is_empty());
        let (h, m, e) = cache.stats();
        assert_eq!((h, m, e), (0, 0, 0));
        assert!(cache.get(&k).is_none());
    }

    #[test]
    fn cache_lru_order_promotion() {
        // Capacity=2: insert A, B; get(A) promotes A; insert C → evicts B not A.
        let mut cache = ShapingCache::new(2);
        let a = make_key("A");
        let b = make_key("B");
        let c = make_key("C");

        cache.insert(a.clone(), shaped(1.0, 1.0));
        cache.insert(b.clone(), shaped(2.0, 2.0));
        // Promote A
        cache.get(&a);
        // Insert C — should evict B (LRU)
        cache.insert(c.clone(), shaped(3.0, 3.0));

        assert!(
            cache.get(&a).is_some(),
            "A was promoted, must still be present"
        );
        assert!(
            cache.get(&b).is_none(),
            "B is LRU after A was promoted, must be evicted"
        );
        assert!(
            cache.get(&c).is_some(),
            "C was just inserted, must be present"
        );
    }

    #[test]
    fn cache_zero_capacity_never_stores() {
        let mut cache = ShapingCache::new(0);
        let k = make_key("hello");
        cache.insert(k.clone(), shaped(10.0, 16.0));
        assert!(cache.get(&k).is_none());
        assert!(cache.is_empty());
    }

    #[test]
    fn cache_evictions_count() {
        let mut cache = ShapingCache::new(1);
        cache.insert(make_key("A"), shaped(1.0, 1.0));
        cache.insert(make_key("B"), shaped(1.0, 1.0));
        cache.insert(make_key("C"), shaped(1.0, 1.0));
        let (_, _, e) = cache.stats();
        assert_eq!(e, 2, "two evictions should have occurred");
    }
}
