//! Focus indicator visual properties for OxiUI accessibility.
//!
//! Provides [`FocusRing`] (the visual spec for the focus outline) and
//! [`FocusIndicator`] (tracks which node currently has focus and what ring
//! spec to use when rendering it).  Renderers consume these types to draw
//! the platform-appropriate focus ring without knowing the full a11y tree.

use accesskit::NodeId;

// ── Focus ring spec ───────────────────────────────────────────────────────────

/// Visual properties for a focus ring, consumed by renderers.
///
/// Describes the outline drawn around the currently-focused widget.
/// All measurements are in logical pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct FocusRing {
    /// Colour of the ring in RGBA byte order `[r, g, b, a]`.
    pub color: [u8; 4],
    /// Stroke width in logical pixels.
    pub width: f32,
    /// Outset distance from the widget's bounding box in logical pixels.
    pub offset: f32,
    /// Corner radius of the ring in logical pixels (`0.0` = sharp corners).
    pub radius: f32,
}

impl Default for FocusRing {
    fn default() -> Self {
        Self {
            // Windows system-highlight blue (#0078D7), fully opaque.
            color: [0, 120, 215, 255],
            width: 2.0,
            offset: 2.0,
            radius: 3.0,
        }
    }
}

// ── Focus indicator ───────────────────────────────────────────────────────────

/// Tracks which node currently holds focus and the visual ring spec to use.
///
/// Renderers query [`FocusIndicator::focused_node`] to know which widget is
/// focused and [`FocusIndicator::ring`] to know how to draw its outline.
///
/// This is intentionally decoupled from [`crate::tree::A11yTree`]'s focus
/// field (which drives the AccessKit `TreeUpdate::focus` field for screen
/// readers).  Both should be kept in sync, but keeping them separate allows
/// the render layer to style the ring independently of the a11y adapter.
pub struct FocusIndicator {
    focused_node: Option<NodeId>,
    ring: FocusRing,
}

impl Default for FocusIndicator {
    fn default() -> Self {
        Self::new()
    }
}

impl FocusIndicator {
    /// Create a new indicator with no focused node and the default ring spec.
    pub fn new() -> Self {
        Self {
            focused_node: None,
            ring: FocusRing::default(),
        }
    }

    /// Set (or clear) the currently focused node.
    ///
    /// Pass `None` to clear the focus — no ring will be rendered.
    pub fn set_focus(&mut self, id: Option<NodeId>) {
        self.focused_node = id;
    }

    /// Return the [`NodeId`] of the currently focused node, if any.
    pub fn focused_node(&self) -> Option<NodeId> {
        self.focused_node
    }

    /// Return a shared reference to the current [`FocusRing`] spec.
    pub fn ring(&self) -> &FocusRing {
        &self.ring
    }

    /// Replace the ring spec with a custom one (builder-style).
    pub fn with_ring(mut self, ring: FocusRing) -> Self {
        self.ring = ring;
        self
    }
}
