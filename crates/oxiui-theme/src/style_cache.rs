//! Memoized style cache for resolved [`ComputedStyle`] values.
//!
//! [`StyleCache`] wraps a [`CompiledStyleSheet`] and caches the result of
//! [`CompiledStyleSheet::compute_style`] per `(widget_type, classes, id,
//! generation)` key.  When the stylesheet's generation changes the cache is
//! automatically invalidated and rebuilt on the next access.

use std::collections::HashMap;

use crate::compile::CompiledStyleSheet;
use crate::stylesheet::ComputedStyle;

// ── Cache key ─────────────────────────────────────────────────────────────────

/// The key used to look up a previously computed style.
///
/// Classes are sorted so that `["primary", "disabled"]` and
/// `["disabled", "primary"]` produce the same key.
#[derive(Hash, Eq, PartialEq, Clone)]
struct StyleCacheKey {
    widget_type: String,
    classes: Vec<String>,
    id: Option<String>,
    generation: u64,
}

impl StyleCacheKey {
    fn new(widget_type: &str, classes: &[&str], id: Option<&str>, generation: u64) -> Self {
        let mut sorted_classes: Vec<String> = classes.iter().map(|s| s.to_string()).collect();
        sorted_classes.sort();
        Self {
            widget_type: widget_type.to_owned(),
            classes: sorted_classes,
            id: id.map(ToOwned::to_owned),
            generation,
        }
    }
}

// ── StyleCache ────────────────────────────────────────────────────────────────

/// Memoizes [`ComputedStyle`] per `(widget_type, classes, id, stylesheet_generation)`.
///
/// When [`CompiledStyleSheet::generation`] advances, all cached entries from
/// the previous generation are discarded before the new lookup.
///
/// # Example
/// ```rust
/// use oxiui_theme::{StyleCache, stylesheet::StyleSheet};
/// use oxiui_theme::compile::CompiledStyleSheet;
///
/// let sheet = StyleSheet::parse("button { color: #ff0000; }").stylesheet;
/// let compiled = CompiledStyleSheet::compile(&sheet, 1);
/// let mut cache = StyleCache::new();
/// let style = cache.get_or_compute(&compiled, "button", &[], None);
/// assert!(style.color.is_some());
/// ```
pub struct StyleCache {
    cache: HashMap<StyleCacheKey, ComputedStyle>,
    current_generation: u64,
}

impl StyleCache {
    /// Create an empty cache.
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            current_generation: 0,
        }
    }

    /// Look up or compute a style.
    ///
    /// If `compiled.generation` differs from the last seen generation, the
    /// entire cache is cleared before the lookup proceeds.  On a cache miss the
    /// style is computed via [`CompiledStyleSheet::compute_style`], inserted,
    /// and returned.
    pub fn get_or_compute(
        &mut self,
        compiled: &CompiledStyleSheet,
        widget_type: &str,
        classes: &[&str],
        id: Option<&str>,
    ) -> &ComputedStyle {
        // Invalidate on generation change.
        if compiled.generation != self.current_generation {
            self.cache.clear();
            self.current_generation = compiled.generation;
        }

        let key = StyleCacheKey::new(widget_type, classes, id, compiled.generation);
        self.cache
            .entry(key)
            .or_insert_with(|| compiled.compute_style(widget_type, classes, id))
    }
}

impl Default for StyleCache {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::CompiledStyleSheet;
    use crate::stylesheet::StyleSheet;

    fn make_compiled(css: &str, generation: u64) -> CompiledStyleSheet {
        let sheet = StyleSheet::parse(css).stylesheet;
        CompiledStyleSheet::compile(&sheet, generation)
    }

    #[test]
    fn test_style_cache_miss_computes() {
        let compiled = make_compiled("button { color: #ff0000; }", 1);
        let mut cache = StyleCache::new();
        let style = cache.get_or_compute(&compiled, "button", &[], None);
        assert!(
            style.color.is_some(),
            "cache miss should compute and return the correct style"
        );
    }

    #[test]
    fn test_style_cache_hit_returns_identical() {
        let compiled = make_compiled("button { color: #ff0000; padding: 8px; }", 1);
        let mut cache = StyleCache::new();

        let first = cache.get_or_compute(&compiled, "button", &[], None).clone();
        let second = cache.get_or_compute(&compiled, "button", &[], None).clone();

        assert_eq!(
            first, second,
            "second call (cache hit) must return identical style"
        );
    }

    #[test]
    fn test_style_cache_invalidates_on_generation_change() {
        // Gen 1: button has red color.
        let compiled_v1 = make_compiled("button { color: #ff0000; }", 1);
        let mut cache = StyleCache::new();

        let style_v1 = cache
            .get_or_compute(&compiled_v1, "button", &[], None)
            .clone();
        assert!(style_v1.color.is_some(), "v1 should have color");

        // Gen 2: button now has no color set at all.
        let compiled_v2 = make_compiled("button { padding: 4px; }", 2);

        // Before accessing with v2, the cache still holds v1's entry.
        // After the call with v2's compiled sheet, cache is invalidated.
        let style_v2 = cache
            .get_or_compute(&compiled_v2, "button", &[], None)
            .clone();
        assert!(
            style_v2.color.is_none(),
            "after generation change the old cached value must not be returned"
        );
        assert!(style_v2.padding.is_some(), "v2 should have padding");
    }

    #[test]
    fn test_style_cache_class_order_irrelevant() {
        // Classes are sorted when building the key, so order must not matter.
        let compiled = make_compiled(".primary { color: #7aa2f7; }", 1);
        let mut cache = StyleCache::new();

        let a = cache
            .get_or_compute(&compiled, "button", &["disabled", "primary"], None)
            .clone();
        let b = cache
            .get_or_compute(&compiled, "button", &["primary", "disabled"], None)
            .clone();

        assert_eq!(a, b, "class order must not affect cache lookup");
    }
}
