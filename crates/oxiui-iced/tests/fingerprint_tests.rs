//! Tests for per-spec dirty fingerprinting in `oxiui-iced`.
//!
//! Covers [`spec_fingerprint`] stability, discriminant sensitivity, and the
//! [`SpecCache`] rebuild-count tracking that tells callers whether the spec
//! list changed between frames.

use std::borrow::Cow;

use oxiui_core::UiCtx;
use oxiui_iced::adapter::{spec_fingerprint, IcedConfig, IcedUiCtx, SpecCache, WidgetSpec};

// ── spec_fingerprint stability ────────────────────────────────────────────────

/// Two identical [`WidgetSpec`] values must produce the same fingerprint.
#[test]
fn test_fingerprint_same_spec_stable() {
    let s1 = WidgetSpec::Label(Cow::Borrowed("hello"));
    let s2 = WidgetSpec::Label(Cow::Borrowed("hello"));
    assert_eq!(
        spec_fingerprint(&s1),
        spec_fingerprint(&s2),
        "identical specs must produce identical fingerprints"
    );
}

/// Button("OK") and Button("Cancel") must have different fingerprints.
#[test]
fn test_fingerprint_different_spec_differs() {
    let ok = WidgetSpec::Button {
        id: 0,
        label: Cow::Borrowed("OK"),
    };
    let cancel = WidgetSpec::Button {
        id: 0,
        label: Cow::Borrowed("Cancel"),
    };
    assert_ne!(
        spec_fingerprint(&ok),
        spec_fingerprint(&cancel),
        "Button(\"OK\") and Button(\"Cancel\") must have different fingerprints"
    );
}

/// A [`WidgetSpec::Button`] and a [`WidgetSpec::Label`] with the same text
/// must have different fingerprints (variant discriminant must affect the hash).
#[test]
fn test_fingerprint_variant_differs() {
    let btn = WidgetSpec::Button {
        id: 0,
        label: Cow::Borrowed("Hi"),
    };
    let lbl = WidgetSpec::Label(Cow::Borrowed("Hi"));
    assert_ne!(
        spec_fingerprint(&btn),
        spec_fingerprint(&lbl),
        "different variants with the same text must produce different fingerprints"
    );
}

/// Calling `spec_fingerprint` twice on the same value must return the same
/// result (deterministic within a run).
#[test]
fn test_fingerprint_is_deterministic() {
    let spec = WidgetSpec::Heading(Cow::Borrowed("Title"));
    let fp1 = spec_fingerprint(&spec);
    let fp2 = spec_fingerprint(&spec);
    assert_eq!(fp1, fp2, "fingerprint must be deterministic across calls");
}

/// Specs that differ only in id produce different fingerprints.
#[test]
fn test_fingerprint_id_sensitivity() {
    let s1 = WidgetSpec::Button {
        id: 0,
        label: Cow::Borrowed("OK"),
    };
    let s2 = WidgetSpec::Button {
        id: 1,
        label: Cow::Borrowed("OK"),
    };
    assert_ne!(
        spec_fingerprint(&s1),
        spec_fingerprint(&s2),
        "specs differing only in id must produce different fingerprints"
    );
}

// ── SpecCache rebuild_count ───────────────────────────────────────────────────

/// A freshly constructed [`SpecCache`] has `rebuild_count == 0`.
#[test]
fn test_rebuild_count_starts_at_zero() {
    let cache = SpecCache::default();
    assert_eq!(
        cache.rebuild_count(),
        0,
        "new cache must start at rebuild_count 0"
    );
}

/// `rebuild_count` increments after a `sync` call with a non-empty spec list
/// (first call always sees a change relative to an empty cache).
#[test]
fn test_rebuild_count_increments_on_change() {
    let mut cache = SpecCache::default();

    // Build a spec list via IcedUiCtx so we exercise real adapter paths.
    let mut ctx = IcedUiCtx::new(IcedConfig::default());
    ctx.label("Hello");
    ctx.button("Go");
    let specs = ctx.into_specs();

    let changed = cache.sync(&specs);
    assert!(changed, "first sync must return true (cache was empty)");
    assert!(
        cache.rebuild_count() > 0,
        "rebuild_count must be > 0 after a change was detected"
    );
}

/// Building with identical specs twice must only count one rebuild.
#[test]
fn test_identical_specs_no_extra_rebuild() {
    let mut cache = SpecCache::default();

    let make_specs = || {
        let mut ctx = IcedUiCtx::new(IcedConfig::default());
        ctx.label("Stable");
        ctx.into_specs()
    };

    let specs = make_specs();
    let first = cache.sync(&specs);
    assert!(first, "first sync (empty cache) must trigger a rebuild");
    assert_eq!(cache.rebuild_count(), 1);

    // Second sync with identical content must not trigger a new rebuild.
    let specs2 = make_specs();
    let second = cache.sync(&specs2);
    assert!(!second, "identical specs must NOT trigger a second rebuild");
    assert_eq!(
        cache.rebuild_count(),
        1,
        "rebuild_count must remain 1 for identical specs"
    );
}

/// When the spec list changes (different widget), `sync` returns `true` and
/// `rebuild_count` advances again.
#[test]
fn test_rebuild_count_increments_when_spec_changes() {
    let mut cache = SpecCache::default();

    let specs_a = vec![WidgetSpec::Label(Cow::Borrowed("A"))];
    let specs_b = vec![WidgetSpec::Label(Cow::Borrowed("B"))];

    cache.sync(&specs_a);
    assert_eq!(cache.rebuild_count(), 1);

    let changed = cache.sync(&specs_b);
    assert!(changed, "different spec content must trigger a rebuild");
    assert_eq!(cache.rebuild_count(), 2);
}

/// Adding a widget to the spec list (length change) must be detected as a change.
#[test]
fn test_rebuild_detected_on_length_change() {
    let mut cache = SpecCache::default();

    let short = vec![WidgetSpec::Label(Cow::Borrowed("one"))];
    let long = vec![
        WidgetSpec::Label(Cow::Borrowed("one")),
        WidgetSpec::Label(Cow::Borrowed("two")),
    ];

    cache.sync(&short);
    assert_eq!(cache.rebuild_count(), 1);

    let changed = cache.sync(&long);
    assert!(changed, "added widget must trigger rebuild");
    assert_eq!(cache.rebuild_count(), 2);
}

/// Empty spec list synced twice must count exactly one rebuild.
#[test]
fn test_empty_specs_sync_twice_counts_one() {
    let mut cache = SpecCache::default();

    // Empty cache vs empty specs — lengths match (both 0), fingerprints vacuously
    // identical, so this is NOT a change.
    let changed_first = cache.sync(&[]);
    assert!(
        !changed_first,
        "syncing empty specs against empty cache must not count as a change"
    );
    assert_eq!(cache.rebuild_count(), 0);

    let changed_second = cache.sync(&[]);
    assert!(!changed_second);
    assert_eq!(cache.rebuild_count(), 0);
}
