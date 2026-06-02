//! Colour-space conversions and palette construction/validation.
//!
//! The base [`crate::Color`] type is 8-bit sRGB RGBA. This module adds
//! round-trip-correct conversions to and from the colour spaces a real UI layer
//! needs:
//!
//! - **Linear RGB** — gamma-decoded sRGB, the space in which light actually
//!   adds. Required for correct blending and for WCAG luminance.
//! - **HSL** — hue/saturation/lightness, convenient for theme tweaks.
//! - **Oklch** — a perceptually-uniform polar space (Björn Ottosson's Oklab in
//!   lightness/chroma/hue form). Lightness ramps in Oklch look perceptually even,
//!   which is why [`PaletteBuilder`] derives tints/shades there.
//!
//! All conversions are designed so that `srgb → space → srgb` reproduces the
//! original within rounding (verified by tests). The transfer functions use the
//! exact piecewise sRGB curve, not the `2.2` approximation.

use crate::{Color, Palette};

/// The sRGB→linear transfer function applied to one channel in `[0, 1]`.
fn srgb_to_linear_channel(c: f32) -> f32 {
    if c <= 0.040_448_237 {
        // The threshold 0.04045 mapped through the inverse is used on encode; on
        // decode the canonical break is 0.04045.
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// The linear→sRGB transfer function applied to one channel in `[0, 1]`.
fn linear_to_srgb_channel(c: f32) -> f32 {
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

#[inline]
fn clamp01(v: f32) -> f32 {
    v.clamp(0.0, 1.0)
}

#[inline]
fn to_u8(v: f32) -> u8 {
    (clamp01(v) * 255.0 + 0.5) as u8
}

/// A colour expressed in linear (gamma-decoded) RGB with `f32` channels in
/// `[0, 1]` plus straight alpha.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LinearRgba {
    /// Linear red.
    pub r: f32,
    /// Linear green.
    pub g: f32,
    /// Linear blue.
    pub b: f32,
    /// Straight (non-premultiplied) alpha.
    pub a: f32,
}

impl LinearRgba {
    /// Construct from explicit channels (not clamped).
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Decode an 8-bit sRGB [`Color`] into linear space.
    pub fn from_color(c: Color) -> Self {
        Self {
            r: srgb_to_linear_channel(c.0 as f32 / 255.0),
            g: srgb_to_linear_channel(c.1 as f32 / 255.0),
            b: srgb_to_linear_channel(c.2 as f32 / 255.0),
            a: c.3 as f32 / 255.0,
        }
    }

    /// Encode back to an 8-bit sRGB [`Color`].
    pub fn to_color(self) -> Color {
        Color(
            to_u8(linear_to_srgb_channel(clamp01(self.r))),
            to_u8(linear_to_srgb_channel(clamp01(self.g))),
            to_u8(linear_to_srgb_channel(clamp01(self.b))),
            to_u8(self.a),
        )
    }

    /// The WCAG 2.x relative luminance of this colour (alpha ignored).
    ///
    /// `L = 0.2126 R + 0.7152 G + 0.0722 B` on linear channels.
    pub fn relative_luminance(self) -> f32 {
        0.2126 * self.r + 0.7152 * self.g + 0.0722 * self.b
    }

    /// Linearly interpolate towards `other` by `t` in `[0, 1]` (in linear
    /// space, which is the physically-correct place to blend light).
    pub fn lerp(self, other: LinearRgba, t: f32) -> LinearRgba {
        let t = clamp01(t);
        LinearRgba {
            r: self.r + (other.r - self.r) * t,
            g: self.g + (other.g - self.g) * t,
            b: self.b + (other.b - self.b) * t,
            a: self.a + (other.a - self.a) * t,
        }
    }
}

/// A colour in HSL (hue degrees `[0, 360)`, saturation/lightness `[0, 1]`) plus
/// straight alpha.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hsla {
    /// Hue in degrees `[0, 360)`.
    pub h: f32,
    /// Saturation `[0, 1]`.
    pub s: f32,
    /// Lightness `[0, 1]`.
    pub l: f32,
    /// Straight alpha `[0, 1]`.
    pub a: f32,
}

impl Hsla {
    /// Construct from explicit components.
    pub const fn new(h: f32, s: f32, l: f32, a: f32) -> Self {
        Self { h, s, l, a }
    }

    /// Convert an sRGB [`Color`] to HSL.
    pub fn from_color(c: Color) -> Self {
        let r = c.0 as f32 / 255.0;
        let g = c.1 as f32 / 255.0;
        let b = c.2 as f32 / 255.0;
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let delta = max - min;
        let l = (max + min) * 0.5;

        let s = if delta.abs() < f32::EPSILON {
            0.0
        } else {
            delta / (1.0 - (2.0 * l - 1.0).abs())
        };

        let h = if delta.abs() < f32::EPSILON {
            0.0
        } else if (max - r).abs() < f32::EPSILON {
            60.0 * (((g - b) / delta) % 6.0)
        } else if (max - g).abs() < f32::EPSILON {
            60.0 * (((b - r) / delta) + 2.0)
        } else {
            60.0 * (((r - g) / delta) + 4.0)
        };
        let h = if h < 0.0 { h + 360.0 } else { h };

        Self {
            h,
            s,
            l,
            a: c.3 as f32 / 255.0,
        }
    }

    /// Convert this HSL colour back to an sRGB [`Color`].
    pub fn to_color(self) -> Color {
        let s = clamp01(self.s);
        let l = clamp01(self.l);
        let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
        let h_prime = self.h.rem_euclid(360.0) / 60.0;
        let x = c * (1.0 - (h_prime % 2.0 - 1.0).abs());
        let (r1, g1, b1) = match h_prime as u32 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        let m = l - c * 0.5;
        Color(to_u8(r1 + m), to_u8(g1 + m), to_u8(b1 + m), to_u8(self.a))
    }

    /// Return a copy with lightness multiplied by `factor` (clamped).
    pub fn scale_lightness(self, factor: f32) -> Hsla {
        Hsla {
            l: clamp01(self.l * factor),
            ..self
        }
    }
}

/// A colour in the Oklch perceptual space: lightness `[0, 1]`, chroma `>= 0`,
/// hue degrees `[0, 360)`, plus straight alpha.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Oklcha {
    /// Perceptual lightness `[0, 1]`.
    pub l: f32,
    /// Chroma (colourfulness), unbounded but typically `< 0.4`.
    pub c: f32,
    /// Hue in degrees `[0, 360)`.
    pub h: f32,
    /// Straight alpha `[0, 1]`.
    pub a: f32,
}

impl Oklcha {
    /// Construct from explicit components.
    pub const fn new(l: f32, c: f32, h: f32, a: f32) -> Self {
        Self { l, c, h, a }
    }

    /// Convert an sRGB [`Color`] to Oklch.
    pub fn from_color(color: Color) -> Self {
        let lin = LinearRgba::from_color(color);
        // Linear sRGB -> LMS (Oklab matrix).
        let l = 0.412_221_46 * lin.r + 0.536_332_55 * lin.g + 0.051_445_995 * lin.b;
        let m = 0.211_903_5 * lin.r + 0.680_699_5 * lin.g + 0.107_396_96 * lin.b;
        let s = 0.088_302_46 * lin.r + 0.281_718_85 * lin.g + 0.629_978_7 * lin.b;
        let l_ = l.cbrt();
        let m_ = m.cbrt();
        let s_ = s.cbrt();
        // LMS' -> Oklab.
        let ok_l = 0.210_454_26 * l_ + 0.793_617_8 * m_ - 0.004_072_047 * s_;
        let ok_a = 1.977_998_5 * l_ - 2.428_592_2 * m_ + 0.450_593_7 * s_;
        let ok_b = 0.025_904_037 * l_ + 0.782_771_77 * m_ - 0.808_675_77 * s_;
        // Oklab -> Oklch (polar).
        let c = (ok_a * ok_a + ok_b * ok_b).sqrt();
        let mut h = ok_b.atan2(ok_a).to_degrees();
        if h < 0.0 {
            h += 360.0;
        }
        Self {
            l: ok_l,
            c,
            h,
            a: lin.a,
        }
    }

    /// Convert this Oklch colour back to an sRGB [`Color`].
    pub fn to_color(self) -> Color {
        let h_rad = self.h.to_radians();
        let ok_a = self.c * h_rad.cos();
        let ok_b = self.c * h_rad.sin();
        // Oklab -> LMS'.
        let l_ = self.l + 0.396_337_78 * ok_a + 0.215_803_76 * ok_b;
        let m_ = self.l - 0.105_561_346 * ok_a - 0.063_854_17 * ok_b;
        let s_ = self.l - 0.089_484_18 * ok_a - 1.291_485_5 * ok_b;
        let l = l_ * l_ * l_;
        let m = m_ * m_ * m_;
        let s = s_ * s_ * s_;
        // LMS -> linear sRGB.
        let r = 4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s;
        let g = -1.268_438 * l + 2.609_757_4 * m - 0.341_319_38 * s;
        let b = -0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s;
        LinearRgba { r, g, b, a: self.a }.to_color()
    }

    /// Return a copy with lightness set to `l` (clamped to `[0, 1]`).
    pub fn with_lightness(self, l: f32) -> Oklcha {
        Oklcha {
            l: clamp01(l),
            ..self
        }
    }
}

/// The WCAG 2.x contrast ratio between two colours.
///
/// Returns a value in `[1.0, 21.0]`; `(L1 + 0.05) / (L2 + 0.05)` with `L1` the
/// lighter luminance. AA body text wants `>= 4.5`, AA large text / AAA-relaxed
/// `>= 3.0`, AAA body `>= 7.0`.
pub fn contrast_ratio(a: Color, b: Color) -> f32 {
    let la = LinearRgba::from_color(a).relative_luminance();
    let lb = LinearRgba::from_color(b).relative_luminance();
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// WCAG conformance levels for a foreground/background pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WcagLevel {
    /// Below AA even for large text (`< 3.0`).
    Fail,
    /// Meets AA for large text only (`>= 3.0`, `< 4.5`).
    AaLarge,
    /// Meets AA for normal text (`>= 4.5`, `< 7.0`).
    Aa,
    /// Meets AAA for normal text (`>= 7.0`).
    Aaa,
}

impl WcagLevel {
    /// Classify a contrast ratio into a [`WcagLevel`].
    pub fn from_ratio(ratio: f32) -> Self {
        if ratio >= 7.0 {
            WcagLevel::Aaa
        } else if ratio >= 4.5 {
            WcagLevel::Aa
        } else if ratio >= 3.0 {
            WcagLevel::AaLarge
        } else {
            WcagLevel::Fail
        }
    }
}

/// A non-fatal accessibility warning produced by [`PaletteBuilder::validate`].
#[derive(Clone, Debug, PartialEq)]
pub struct ContrastWarning {
    /// Human-readable name of the foreground role (e.g. `"text"`).
    pub foreground: &'static str,
    /// Human-readable name of the background role (e.g. `"background"`).
    pub background: &'static str,
    /// The measured contrast ratio.
    pub ratio: f32,
    /// The classified WCAG level for the pair.
    pub level: WcagLevel,
}

/// Builder for a semantic [`Palette`] with optional WCAG validation.
///
/// Unset roles are derived from the ones that *are* set, using Oklch lightness
/// adjustments so the derived shades are perceptually even. `validate` returns
/// **warnings, never hard errors** — a low-contrast palette is still usable.
#[derive(Clone, Debug, Default)]
pub struct PaletteBuilder {
    background: Option<Color>,
    surface: Option<Color>,
    primary: Option<Color>,
    on_primary: Option<Color>,
    text: Option<Color>,
    muted: Option<Color>,
}

impl PaletteBuilder {
    /// Start an empty builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the window/page background.
    pub fn background(mut self, c: Color) -> Self {
        self.background = Some(c);
        self
    }

    /// Set the card/panel surface colour.
    pub fn surface(mut self, c: Color) -> Self {
        self.surface = Some(c);
        self
    }

    /// Set the primary accent colour.
    pub fn primary(mut self, c: Color) -> Self {
        self.primary = Some(c);
        self
    }

    /// Set the text colour drawn on the primary accent.
    pub fn on_primary(mut self, c: Color) -> Self {
        self.on_primary = Some(c);
        self
    }

    /// Set the main body text colour.
    pub fn text(mut self, c: Color) -> Self {
        self.text = Some(c);
        self
    }

    /// Set the de-emphasised text colour.
    pub fn muted(mut self, c: Color) -> Self {
        self.muted = Some(c);
        self
    }

    /// Build the [`Palette`], deriving any unset roles.
    ///
    /// Defaults assume a dark theme when no background is given. Derivations:
    /// `surface` lightens `background` in Oklch; `on_primary` picks black or
    /// white for best contrast against `primary`; `text` is near-white/near-
    /// black opposite the background; `muted` is `text` at reduced lightness.
    pub fn build(self) -> Palette {
        let background = self.background.unwrap_or(Color(26, 27, 38, 255));
        let bg_ok = Oklcha::from_color(background);
        let is_dark = bg_ok.l < 0.5;

        let surface = self.surface.unwrap_or_else(|| {
            // Nudge the surface a step away from the background in lightness.
            let target = if is_dark {
                bg_ok.l + 0.06
            } else {
                bg_ok.l - 0.06
            };
            bg_ok.with_lightness(target).to_color()
        });

        let primary = self.primary.unwrap_or(Color(122, 162, 247, 255));

        let on_primary = self.on_primary.unwrap_or_else(|| {
            let white = Color(255, 255, 255, 255);
            let black = Color(0, 0, 0, 255);
            if contrast_ratio(primary, white) >= contrast_ratio(primary, black) {
                white
            } else {
                black
            }
        });

        let default_text = if is_dark {
            Color(230, 233, 245, 255)
        } else {
            Color(20, 22, 30, 255)
        };
        let text = self.text.unwrap_or(default_text);

        let muted = self.muted.unwrap_or_else(|| {
            let t = Oklcha::from_color(text);
            // Pull muted lightness towards the background by ~35%.
            let target = t.l + (bg_ok.l - t.l) * 0.35;
            t.with_lightness(target).to_color()
        });

        Palette {
            background,
            surface,
            primary,
            on_primary,
            text,
            muted,
        }
    }

    /// Validate the contrast of the key foreground/background pairs in a built
    /// palette. Returns one [`ContrastWarning`] per pair whose ratio is below
    /// the AA threshold (`4.5`). An empty vector means every pair passes AA.
    pub fn validate(palette: &Palette) -> Vec<ContrastWarning> {
        let pairs: [(&'static str, &'static str, Color, Color); 4] = [
            ("text", "background", palette.text, palette.background),
            ("text", "surface", palette.text, palette.surface),
            ("muted", "background", palette.muted, palette.background),
            ("on_primary", "primary", palette.on_primary, palette.primary),
        ];
        let mut warnings = Vec::new();
        for (fg_name, bg_name, fg, bg) in pairs {
            let ratio = contrast_ratio(fg, bg);
            let level = WcagLevel::from_ratio(ratio);
            if ratio < 4.5 {
                warnings.push(ContrastWarning {
                    foreground: fg_name,
                    background: bg_name,
                    ratio,
                    level,
                });
            }
        }
        warnings
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    fn roundtrip_within(c: Color, tol: u8) {
        // sRGB -> linear -> sRGB
        let l = LinearRgba::from_color(c).to_color();
        assert!(c.0.abs_diff(l.0) <= tol, "linear r {:?} -> {:?}", c, l);
        assert!(c.1.abs_diff(l.1) <= tol, "linear g {:?} -> {:?}", c, l);
        assert!(c.2.abs_diff(l.2) <= tol, "linear b {:?} -> {:?}", c, l);
        // sRGB -> HSL -> sRGB
        let h = Hsla::from_color(c).to_color();
        assert!(c.0.abs_diff(h.0) <= tol, "hsl r {:?} -> {:?}", c, h);
        assert!(c.1.abs_diff(h.1) <= tol, "hsl g {:?} -> {:?}", c, h);
        assert!(c.2.abs_diff(h.2) <= tol, "hsl b {:?} -> {:?}", c, h);
        // sRGB -> Oklch -> sRGB
        let o = Oklcha::from_color(c).to_color();
        assert!(c.0.abs_diff(o.0) <= tol, "oklch r {:?} -> {:?}", c, o);
        assert!(c.1.abs_diff(o.1) <= tol, "oklch g {:?} -> {:?}", c, o);
        assert!(c.2.abs_diff(o.2) <= tol, "oklch b {:?} -> {:?}", c, o);
    }

    #[test]
    fn roundtrip_primaries_and_grays() {
        for c in [
            Color(0, 0, 0, 255),
            Color(255, 255, 255, 255),
            Color(255, 0, 0, 255),
            Color(0, 255, 0, 255),
            Color(0, 0, 255, 255),
            Color(128, 128, 128, 255),
            Color(122, 162, 247, 200),
            Color(33, 77, 199, 255),
        ] {
            roundtrip_within(c, 2);
        }
    }

    #[test]
    fn linear_luminance_extremes() {
        assert!(close(
            LinearRgba::from_color(Color(0, 0, 0, 255)).relative_luminance(),
            0.0,
            1e-4
        ));
        assert!(close(
            LinearRgba::from_color(Color(255, 255, 255, 255)).relative_luminance(),
            1.0,
            1e-3
        ));
    }

    #[test]
    fn hsl_known_values() {
        // Pure red: hue 0, full saturation, half lightness.
        let red = Hsla::from_color(Color(255, 0, 0, 255));
        assert!(close(red.h, 0.0, 0.5));
        assert!(close(red.s, 1.0, 0.01));
        assert!(close(red.l, 0.5, 0.01));
        // Pure green: hue 120.
        let green = Hsla::from_color(Color(0, 255, 0, 255));
        assert!(close(green.h, 120.0, 0.5));
    }

    #[test]
    fn oklch_white_is_light() {
        let w = Oklcha::from_color(Color(255, 255, 255, 255));
        assert!(close(w.l, 1.0, 0.01), "white L={}", w.l);
        assert!(w.c < 0.01, "white chroma should be ~0, got {}", w.c);
    }

    #[test]
    fn contrast_black_white_is_21() {
        let r = contrast_ratio(Color(0, 0, 0, 255), Color(255, 255, 255, 255));
        assert!(close(r, 21.0, 0.05), "ratio {r}");
        assert_eq!(WcagLevel::from_ratio(r), WcagLevel::Aaa);
        // Symmetric.
        let r2 = contrast_ratio(Color(255, 255, 255, 255), Color(0, 0, 0, 255));
        assert!(close(r2, 21.0, 0.05));
    }

    #[test]
    fn wcag_thresholds() {
        assert_eq!(WcagLevel::from_ratio(1.0), WcagLevel::Fail);
        assert_eq!(WcagLevel::from_ratio(3.5), WcagLevel::AaLarge);
        assert_eq!(WcagLevel::from_ratio(5.0), WcagLevel::Aa);
        assert_eq!(WcagLevel::from_ratio(10.0), WcagLevel::Aaa);
    }

    #[test]
    fn builder_derives_and_validates() {
        let palette = PaletteBuilder::new()
            .background(Color(26, 27, 38, 255))
            .primary(Color(122, 162, 247, 255))
            .build();
        // Surface should differ from background (derived lighter for dark theme).
        assert_ne!(palette.surface, palette.background);
        // on_primary should be readable on primary.
        assert!(contrast_ratio(palette.on_primary, palette.primary) >= 3.0);
        // Derived dark-theme text should pass AA against the background.
        let warnings = PaletteBuilder::validate(&palette);
        assert!(
            !warnings
                .iter()
                .any(|w| w.foreground == "text" && w.background == "background"),
            "derived text/background should pass AA: {warnings:?}"
        );
    }

    #[test]
    fn builder_flags_low_contrast() {
        // Light-grey text on white: should warn.
        let bad = Palette {
            background: Color(255, 255, 255, 255),
            surface: Color(255, 255, 255, 255),
            primary: Color(200, 200, 200, 255),
            on_primary: Color(210, 210, 210, 255),
            text: Color(200, 200, 200, 255),
            muted: Color(220, 220, 220, 255),
        };
        let warnings = PaletteBuilder::validate(&bad);
        assert!(
            !warnings.is_empty(),
            "expected contrast warnings for low-contrast palette"
        );
        assert!(warnings.iter().all(|w| w.ratio < 4.5));
    }
}
