//! Theme serialization and deserialization via [`oxicode`].
//!
//! Provides a compact, Pure-Rust binary representation of a theme snapshot
//! (design tokens + typography scale) that can be saved to disk or transmitted
//! over the network and reconstructed without loss.
//!
//! # Format
//!
//! The serialized form is a length-prefixed [`ThemeSnapshot`] encoded with
//! [`oxicode`]'s standard configuration (variable-length integers, little-endian
//! floats, no external schema).  The format is opaque binary; for human-editable
//! theme files use [`serde_json`](https://crates.io/crates/serde_json) on top of
//! these types instead.
//!
//! # Example
//!
//! ```rust
//! use oxiui_theme::serial::{deserialize_theme, serialize_theme, ThemeSnapshot};
//!
//! let snapshot = ThemeSnapshot::default();
//! let bytes = serialize_theme(&snapshot).expect("serialize");
//! let restored = deserialize_theme(&bytes).expect("deserialize");
//! assert_eq!(snapshot, restored);
//! ```

use oxicode::{Decode, Encode};
use oxiui_core::{FontSpec, Palette, UiError};

use crate::{DesignTokens, TypographyScale};

// ── Snapshot type ────────────────────────────────────────────────────────────

/// A serializable snapshot of the design-token, typography, palette, and font
/// layers of a theme.
///
/// This is the primary type persisted by [`serialize_theme`] /
/// [`deserialize_theme`].  All fields round-trip faithfully through the
/// [`oxicode`] binary format.
#[derive(Clone, Debug, Default, PartialEq, Encode, Decode)]
pub struct ThemeSnapshot {
    /// Design tokens (spacing / radius / elevation / opacity).
    pub tokens: DesignTokens,
    /// Typographic scale (six named text-style roles).
    pub typography: TypographyScale,
    /// Semantic colour palette for this theme.
    pub palette: Palette,
    /// Body font specification.
    pub body_font: FontSpec,
}

// ── Public API ───────────────────────────────────────────────────────────────

/// Serialise a [`ThemeSnapshot`] to a compact binary blob via [`oxicode`].
///
/// The returned bytes can be saved to disk and later restored with
/// [`deserialize_theme`].
///
/// # Errors
///
/// Returns [`UiError::Other`] if the [`oxicode`] encoder encounters an
/// unexpected error (e.g. out-of-memory when allocating the output buffer).
pub fn serialize_theme(snapshot: &ThemeSnapshot) -> Result<Vec<u8>, UiError> {
    oxicode::encode_to_vec(snapshot)
        .map_err(|e| UiError::Other(format!("theme serialization failed: {e}")))
}

/// Deserialise a [`ThemeSnapshot`] from bytes produced by [`serialize_theme`].
///
/// # Errors
///
/// Returns [`UiError::Other`] if the bytes are truncated, corrupted, or were
/// not produced by [`serialize_theme`] with a compatible [`oxicode`] version.
pub fn deserialize_theme(bytes: &[u8]) -> Result<ThemeSnapshot, UiError> {
    let (snapshot, _consumed) = oxicode::decode_from_slice::<ThemeSnapshot>(bytes)
        .map_err(|e| UiError::Other(format!("theme deserialization failed: {e}")))?;
    Ok(snapshot)
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokens::{RadiusStep, SpacingStep};

    #[test]
    fn serialize_deserialize_default_theme_roundtrip() {
        let original = ThemeSnapshot::default();
        let bytes = serialize_theme(&original).expect("serialize should succeed");
        assert!(!bytes.is_empty(), "serialized bytes must be non-empty");

        let restored = deserialize_theme(&bytes).expect("deserialize should succeed");
        assert_eq!(
            original, restored,
            "round-tripped snapshot must equal original"
        );

        // Re-serialise and verify determinism.
        let bytes2 = serialize_theme(&restored).expect("re-serialize should succeed");
        assert_eq!(
            bytes, bytes2,
            "re-serialized bytes must be identical (deterministic encoding)"
        );
    }

    #[test]
    fn deserialize_invalid_bytes_returns_error() {
        let bad = b"this is not a valid oxicode-encoded theme snapshot";
        let result = deserialize_theme(bad);
        assert!(result.is_err(), "invalid bytes must return an error");
    }

    #[test]
    fn serialize_is_not_empty() {
        let snapshot = ThemeSnapshot::default();
        let bytes = serialize_theme(&snapshot).expect("serialize");
        assert!(
            bytes.len() > 10,
            "serialized theme should be non-trivial in size (got {} bytes)",
            bytes.len()
        );
    }

    #[test]
    fn roundtrip_custom_tokens() {
        let custom = ThemeSnapshot {
            tokens: DesignTokens {
                spacing: [2.0, 4.0, 8.0, 16.0, 24.0, 32.0, 64.0],
                radius: [0.0, 3.0, 6.0, 12.0, 24.0, 999.0],
                elevation: [0.0, 2.0, 4.0, 8.0, 16.0, 32.0],
                opacity: [0.2, 0.5, 0.7, 0.9, 1.0],
            },
            typography: TypographyScale::default(),
            ..ThemeSnapshot::default()
        };

        let bytes = serialize_theme(&custom).expect("serialize custom tokens");
        let restored = deserialize_theme(&bytes).expect("deserialize custom tokens");
        assert_eq!(custom, restored);
    }

    #[test]
    fn named_token_lookup_survives_roundtrip() {
        let original = ThemeSnapshot::default();
        let bytes = serialize_theme(&original).expect("serialize");
        let restored = deserialize_theme(&bytes).expect("deserialize");

        // Verify named step access still works on the restored value.
        assert_eq!(
            original.tokens.spacing(SpacingStep::Md),
            restored.tokens.spacing(SpacingStep::Md),
            "spacing Md must survive round-trip"
        );
        assert_eq!(
            original.tokens.radius(RadiusStep::Full),
            restored.tokens.radius(RadiusStep::Full),
            "radius Full must survive round-trip"
        );
    }

    #[test]
    fn typography_survives_roundtrip() {
        let original = ThemeSnapshot::default();
        let bytes = serialize_theme(&original).expect("serialize");
        let restored = deserialize_theme(&bytes).expect("deserialize");

        assert_eq!(
            original.typography.body.size, restored.typography.body.size,
            "body size must survive round-trip"
        );
        assert_eq!(
            original.typography.display.weight, restored.typography.display.weight,
            "display weight must survive round-trip"
        );
    }

    #[test]
    fn palette_round_trip() {
        use oxiui_core::{Color, Palette};
        let snapshot = ThemeSnapshot {
            palette: Palette {
                background: Color(10, 20, 30, 255),
                surface: Color(11, 21, 31, 255),
                primary: Color(12, 22, 32, 255),
                on_primary: Color(13, 23, 33, 255),
                text: Color(14, 24, 34, 255),
                muted: Color(15, 25, 35, 255),
            },
            ..ThemeSnapshot::default()
        };
        let bytes = serialize_theme(&snapshot).expect("serialize");
        let decoded = deserialize_theme(&bytes).expect("deserialize");
        assert_eq!(snapshot, decoded);
    }

    #[test]
    fn font_style_oblique_round_trip() {
        use oxiui_core::{FontSpec, FontStyle};
        let snapshot = ThemeSnapshot {
            body_font: FontSpec {
                family: "MyFont".to_string(),
                size: 18.0,
                weight: 700,
                style: FontStyle::Oblique { degrees: 12.5 },
                letter_spacing: 0.5,
                line_height: Some(1.6),
                features: vec![],
            },
            ..ThemeSnapshot::default()
        };
        let bytes = serialize_theme(&snapshot).expect("serialize");
        let decoded = deserialize_theme(&bytes).expect("deserialize");
        assert_eq!(snapshot, decoded);
    }

    #[test]
    fn full_snapshot_round_trip() {
        use oxiui_core::{Color, FontSpec, FontStyle, Palette};
        let snapshot = ThemeSnapshot {
            palette: Palette {
                background: Color(255, 255, 255, 255),
                surface: Color(245, 245, 245, 255),
                primary: Color(99, 102, 241, 255),
                on_primary: Color(255, 255, 255, 255),
                text: Color(15, 23, 42, 255),
                muted: Color(100, 116, 139, 255),
            },
            body_font: FontSpec {
                family: "Inter".to_string(),
                size: 16.0,
                weight: 400,
                style: FontStyle::Normal,
                letter_spacing: 0.0,
                line_height: Some(1.5),
                features: vec![],
            },
            ..ThemeSnapshot::default()
        };
        let bytes = serialize_theme(&snapshot).expect("serialize");
        let decoded = deserialize_theme(&bytes).expect("deserialize");
        assert_eq!(snapshot, decoded);
    }
}
