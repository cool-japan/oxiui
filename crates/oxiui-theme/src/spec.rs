//! Border and shadow specifications, plus elevation → shadow presets.
//!
//! The single-edge [`BorderSpec`] is paired with [`BorderSpecs`] for per-side
//! borders (top/right/bottom/left). Shadows come in two flavours: a single
//! representative [`ShadowSpec`] via [`elevation_shadow`], and the full
//! ambient + key pair stack via [`elevation_shadows`] following Material
//! Design's two-layer elevation model.

use oxiui_core::Color;

/// Border line style.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BorderStyle {
    /// No border drawn.
    None,
    /// A solid line.
    Solid,
    /// A dashed line.
    Dashed,
    /// A dotted line.
    Dotted,
    /// A double line.
    Double,
}

/// A border specification: width, style, and colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BorderSpec {
    /// Border width in logical pixels.
    pub width: f32,
    /// Line style.
    pub style: BorderStyle,
    /// Border colour.
    pub color: Color,
}

impl BorderSpec {
    /// A solid border of the given width and colour.
    pub const fn solid(width: f32, color: Color) -> Self {
        Self {
            width,
            style: BorderStyle::Solid,
            color,
        }
    }

    /// No border.
    pub const fn none() -> Self {
        Self {
            width: 0.0,
            style: BorderStyle::None,
            color: Color(0, 0, 0, 0),
        }
    }

    /// Returns `true` if this border would draw nothing.
    pub fn is_invisible(&self) -> bool {
        self.style == BorderStyle::None || self.width <= 0.0 || self.color.3 == 0
    }
}

/// Per-side border specification: each edge of a rectangle may carry its own
/// [`BorderSpec`]. Use [`BorderSpecs::uniform`] when all four edges agree.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BorderSpecs {
    /// Top edge.
    pub top: BorderSpec,
    /// Right edge.
    pub right: BorderSpec,
    /// Bottom edge.
    pub bottom: BorderSpec,
    /// Left edge.
    pub left: BorderSpec,
}

impl BorderSpecs {
    /// Build a per-side spec where every edge is the same [`BorderSpec`].
    pub const fn uniform(spec: BorderSpec) -> Self {
        Self {
            top: spec,
            right: spec,
            bottom: spec,
            left: spec,
        }
    }

    /// Build a per-side spec with no border on any edge.
    pub const fn none() -> Self {
        Self::uniform(BorderSpec::none())
    }

    /// Build a per-side spec with the same `width`, `style`, and `color`.
    pub const fn solid(width: f32, color: Color) -> Self {
        Self::uniform(BorderSpec::solid(width, color))
    }

    /// `true` if every edge would draw nothing.
    pub fn is_invisible(&self) -> bool {
        self.top.is_invisible()
            && self.right.is_invisible()
            && self.bottom.is_invisible()
            && self.left.is_invisible()
    }

    /// `true` if every edge is identical (allowing renderers to take a fast
    /// uniform-border path).
    pub fn is_uniform(&self) -> bool {
        self.top == self.right && self.right == self.bottom && self.bottom == self.left
    }
}

/// A box-shadow specification.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowSpec {
    /// Horizontal offset in logical pixels.
    pub offset_x: f32,
    /// Vertical offset in logical pixels.
    pub offset_y: f32,
    /// Blur radius in logical pixels.
    pub blur: f32,
    /// Spread radius in logical pixels (grows the shadow before blurring).
    pub spread: f32,
    /// Shadow colour (usually semi-transparent black).
    pub color: Color,
    /// Whether the shadow is drawn inside the box (inset) rather than outside.
    pub inset: bool,
}

impl ShadowSpec {
    /// A drop shadow (outset) with the given parameters.
    pub const fn drop(offset_y: f32, blur: f32, color: Color) -> Self {
        Self {
            offset_x: 0.0,
            offset_y,
            blur,
            spread: 0.0,
            color,
            inset: false,
        }
    }

    /// Construct a shadow from explicit offset, blur, and RGBA byte array.
    ///
    /// `color_rgba` is `[r, g, b, a]` where each channel is `0..=255`.
    /// `spread` defaults to 0 and `inset` defaults to `false`.
    pub fn new(offset_x: f32, offset_y: f32, blur_radius: f32, color_rgba: [u8; 4]) -> Self {
        Self {
            offset_x,
            offset_y,
            blur: blur_radius,
            spread: 0.0,
            color: Color(color_rgba[0], color_rgba[1], color_rgba[2], color_rgba[3]),
            inset: false,
        }
    }

    /// Construct a directional drop shadow with a default semi-transparent black colour.
    ///
    /// Equivalent to `ShadowSpec::new(offset_x, offset_y, blur, [0, 0, 0, 160])`.
    pub fn drop_shadow(offset_x: f32, offset_y: f32, blur: f32) -> Self {
        Self::new(offset_x, offset_y, blur, [0, 0, 0, 160])
    }

    /// Builder: set the spread radius (positive expands, negative contracts).
    pub fn with_spread(mut self, spread: f32) -> Self {
        self.spread = spread;
        self
    }

    /// Builder: set the `inset` flag.
    pub fn with_inset(mut self, inset: bool) -> Self {
        self.inset = inset;
        self
    }

    /// Encode the shadow colour as a packed `0xAARRGGBB` `u32`.
    ///
    /// # Example
    /// ```
    /// # use oxiui_theme::ShadowSpec;
    /// let spec = ShadowSpec::new(0.0, 0.0, 0.0, [255, 0, 0, 128]);
    /// assert_eq!(spec.to_pixel_color(), 0x80FF_0000);
    /// ```
    pub fn to_pixel_color(&self) -> u32 {
        let Color(r, g, b, a) = self.color;
        ((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
    }

    /// Returns `true` if this shadow would render nothing.
    pub fn is_invisible(&self) -> bool {
        self.color.3 == 0
    }
}

/// Convert a continuous elevation value (in logical dp, typical range 0–24) to a
/// single representative [`ShadowSpec`].
///
/// Elevation 0 produces a fully transparent, zero-blur shadow. Higher values yield
/// progressively larger blur and downward offset following a Material Design-style
/// curve.
///
/// | elevation | blur (approx) | offset_y (approx) |
/// |-----------|---------------|--------------------|
/// | 0         | 0             | 0                  |
/// | 2         | 4             | 2                  |
/// | 4         | 8             | 4                  |
/// | 8         | 16            | 8                  |
///
/// # Examples
/// ```
/// # use oxiui_theme::elevation_to_shadow;
/// let s = elevation_to_shadow(0.0);
/// assert_eq!(s.to_pixel_color() & 0xFF, 0); // alpha == 0 at elevation 0
/// let s4 = elevation_to_shadow(4.0);
/// assert!(s4.blur > 0.0);
/// ```
pub fn elevation_to_shadow(elevation: f32) -> ShadowSpec {
    if elevation <= 0.0 {
        return ShadowSpec {
            offset_x: 0.0,
            offset_y: 0.0,
            blur: 0.0,
            spread: 0.0,
            color: Color(0, 0, 0, 0),
            inset: false,
        };
    }
    let e = elevation.max(0.0);
    let blur = e * 2.0;
    let offset_y = e * 1.0;
    // Alpha scales from ~48 at elevation 1 up to ~160 at elevation 24.
    let alpha = (40.0 + e * 5.0).min(160.0) as u8;
    ShadowSpec {
        offset_x: 0.0,
        offset_y,
        blur,
        spread: 0.0,
        color: Color(0, 0, 0, alpha),
        inset: false,
    }
}

/// Returns the conventional drop-shadow stack for elevation `level` (0..=5).
///
/// Level 0 is no shadow; higher levels are larger and softer. The colour is a
/// semi-transparent black scaled with elevation. Many shadows in real design
/// systems stack two layers (ambient + key); here we return a single
/// representative shadow per level for simplicity.
pub fn elevation_shadow(level: usize) -> Option<ShadowSpec> {
    match level {
        0 => None,
        1 => Some(ShadowSpec::drop(1.0, 2.0, Color(0, 0, 0, 56))),
        2 => Some(ShadowSpec::drop(2.0, 4.0, Color(0, 0, 0, 64))),
        3 => Some(ShadowSpec::drop(4.0, 8.0, Color(0, 0, 0, 72))),
        4 => Some(ShadowSpec::drop(8.0, 16.0, Color(0, 0, 0, 84))),
        _ => Some(ShadowSpec::drop(12.0, 24.0, Color(0, 0, 0, 96))),
    }
}

/// Returns a Material Design-style **ambient + key** two-shadow stack for the
/// given `elevation` (logical dp, typical range 0–24).
///
/// Material Design uses two shadows per elevated surface:
/// - **Ambient** — diffuse, spread wide, low opacity.
/// - **Key** — directional, smaller, higher opacity.
///
/// The returned `Vec` always contains exactly **two** [`ShadowSpec`]s
/// `[ambient, key]` for elevation > 0, or two invisible (transparent) shadows
/// for elevation == 0.
///
/// # Examples
/// ```
/// # use oxiui_theme::spec::elevation_shadows;
/// let stack = elevation_shadows(4);
/// assert_eq!(stack.len(), 2);
/// let (ambient, key) = (&stack[0], &stack[1]);
/// assert!(ambient.blur > key.blur); // ambient is broader
/// ```
pub fn elevation_shadows(elevation: u32) -> Vec<ShadowSpec> {
    // Formulas approximate the Material Design 2 dp → shadow mapping.
    // Ambient: large blur, low alpha; Key: smaller blur, higher alpha.
    if elevation == 0 {
        return vec![
            ShadowSpec {
                offset_x: 0.0,
                offset_y: 0.0,
                blur: 0.0,
                spread: 0.0,
                color: Color(0, 0, 0, 0),
                inset: false,
            },
            ShadowSpec {
                offset_x: 0.0,
                offset_y: 0.0,
                blur: 0.0,
                spread: 0.0,
                color: Color(0, 0, 0, 0),
                inset: false,
            },
        ];
    }
    let dp = elevation as f32;
    // Ambient shadow: grows as sqrt of elevation, wide blur, low alpha.
    let ambient_blur = (dp * 3.0).max(1.0);
    let ambient_y = dp * 0.5;
    let ambient_alpha = ((12.0 + dp * 1.5).min(60.0)) as u8;
    // Key shadow: proportional to elevation, tighter blur, higher alpha.
    let key_blur = (dp * 1.5).max(1.0);
    let key_y = dp * 1.0;
    let key_alpha = ((20.0 + dp * 2.5).min(100.0)) as u8;
    vec![
        ShadowSpec {
            // ambient
            offset_x: 0.0,
            offset_y: ambient_y,
            blur: ambient_blur,
            spread: 0.0,
            color: Color(0, 0, 0, ambient_alpha),
            inset: false,
        },
        ShadowSpec {
            // key
            offset_x: 0.0,
            offset_y: key_y,
            blur: key_blur,
            spread: 0.0,
            color: Color(0, 0, 0, key_alpha),
            inset: false,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn border_visibility() {
        assert!(BorderSpec::none().is_invisible());
        assert!(!BorderSpec::solid(1.0, Color(255, 255, 255, 255)).is_invisible());
        // Zero alpha is invisible.
        assert!(BorderSpec::solid(2.0, Color(255, 255, 255, 0)).is_invisible());
    }

    #[test]
    fn shadow_visibility() {
        assert!(!ShadowSpec::drop(2.0, 4.0, Color(0, 0, 0, 100)).is_invisible());
        assert!(ShadowSpec::drop(2.0, 4.0, Color(0, 0, 0, 0)).is_invisible());
    }

    #[test]
    fn elevation_grows_with_level() {
        assert!(elevation_shadow(0).is_none());
        let s1 = elevation_shadow(1).expect("level 1 has a shadow");
        let s5 = elevation_shadow(5).expect("level 5 has a shadow");
        assert!(s5.blur > s1.blur);
        assert!(s5.offset_y > s1.offset_y);
        // Out-of-range clamps to the strongest.
        assert_eq!(elevation_shadow(99), elevation_shadow(5));
    }

    #[test]
    fn elevation_shadow_count() {
        let stack = elevation_shadows(4);
        assert_eq!(stack.len(), 2, "must return exactly 2 ShadowSpec values");
    }

    #[test]
    fn elevation_zero_returns_invisible_pair() {
        let stack = elevation_shadows(0);
        assert_eq!(stack.len(), 2);
        assert!(stack[0].is_invisible());
        assert!(stack[1].is_invisible());
    }

    #[test]
    fn elevation_shadows_increases_with_level() {
        let stack_low = elevation_shadows(2);
        let stack_high = elevation_shadows(8);
        // The ambient (index 0) blur must grow with elevation.
        assert!(
            stack_high[0].blur > stack_low[0].blur,
            "ambient blur must increase: {} vs {}",
            stack_high[0].blur,
            stack_low[0].blur,
        );
    }

    #[test]
    fn border_style_double_exists() {
        let b = BorderSpec {
            width: 2.0,
            style: BorderStyle::Double,
            color: Color(0, 0, 0, 255),
        };
        assert_eq!(b.style, BorderStyle::Double);
    }
}
