//! COOLJAPAN high-contrast palette — WCAG AAA compliant.
//!
//! All foreground/background pairings in this palette exceed the WCAG 2.1
//! AAA contrast threshold of 7.0:1.
//!
//! | Pair                  | Contrast ratio | WCAG level |
//! |-----------------------|----------------|------------|
//! | white on black        | 21.0:1         | AAA ✓      |
//! | yellow (#FFFF00) on black | ~19.6:1    | AAA ✓      |
//! | white on very-dark-blue | ≥ 7.0:1    | AAA ✓      |

use oxiui_core::{Color, Palette};

/// Returns the COOLJAPAN high-contrast palette.
///
/// Background: pure black (`#000000`).
/// Foreground / text: pure white (`#FFFFFF`).
/// Primary / accent: bright yellow (`#FFFF00`) — contrast vs black ≈ 19.6:1.
/// Surface: very-dark navy (`#0A0A1A`) for subtle depth without losing contrast.
/// Muted: light grey (`#AAAAAA`) — contrast vs black ≈ 4.6:1 (AA; AAA for large text).
///
/// All primary foreground/background combinations satisfy WCAG 2.1 AAA (≥ 7.0:1).
pub fn cooljapan_high_contrast() -> Palette {
    Palette {
        background: Color(0, 0, 0, 255),  // #000000 — pure black
        surface: Color(10, 10, 26, 255),  // #0A0A1A — very dark navy
        primary: Color(255, 255, 0, 255), // #FFFF00 — bright yellow, ~19.6:1 on black
        on_primary: Color(0, 0, 0, 255),  // #000000 — text on yellow button
        text: Color(255, 255, 255, 255),  // #FFFFFF — pure white, 21.0:1 on black
        muted: Color(200, 200, 200, 255), // #C8C8C8 — light grey, ~13.1:1 on black
    }
}

/// Returns the COOLJAPAN high-contrast LIGHT palette (AAA light variant).
///
/// All foreground/background pairings exceed the WCAG 2.1 AAA threshold of
/// 7.0:1. The palette uses a near-white background and very-dark foregrounds so
/// that every stated pair satisfies ≥ 7.0 contrast.
///
/// | Pair                         | Contrast (approx) | WCAG level |
/// |------------------------------|-------------------|------------|
/// | `text` (#000000) on `bg`     | ≥ 20.0:1          | AAA ✓      |
/// | `text_secondary` on `bg`     | ≥ 7.0:1           | AAA ✓      |
/// | `primary` (dark blue) on `bg`| ≥ 7.0:1           | AAA ✓      |
pub fn cooljapan_high_contrast_light() -> Palette {
    // background #FAFAFA  L ≈ 0.955
    // #000000 on #FAFAFA → contrast ≈ 20.4:1  (AAA ✓)
    // #1A1A1A on #FAFAFA → contrast ≈ 16.8:1  (AAA ✓)  — text_secondary role via `muted`
    // #00008B on #FAFAFA → contrast ≈ 14.2:1  (AAA ✓)  — dark blue primary
    Palette {
        background: Color(250, 250, 250, 255), // #FAFAFA near-white
        surface: Color(255, 255, 255, 255),    // #FFFFFF white
        primary: Color(0, 0, 139, 255),        // #00008B dark blue, ≈14.2:1 on #FAFAFA
        on_primary: Color(255, 255, 255, 255), // white on dark blue ≈ 14.2:1
        text: Color(0, 0, 0, 255),             // #000000 ≈ 20.4:1 on #FAFAFA
        muted: Color(26, 26, 26, 255),         // #1A1A1A ≈ 16.8:1 on #FAFAFA (secondary text)
    }
}

/// Computes the WCAG 2.1 relative luminance of an sRGB colour (components 0–255).
///
/// Formula from <https://www.w3.org/TR/WCAG21/#dfn-relative-luminance>.
pub fn wcag_luminance(r: u8, g: u8, b: u8) -> f64 {
    let linearize = |c: u8| {
        let v = c as f64 / 255.0;
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linearize(r) + 0.7152 * linearize(g) + 0.0722 * linearize(b)
}

/// Computes the WCAG 2.1 contrast ratio between two sRGB colours.
///
/// Returns a value in [1.0, 21.0]. WCAG AAA normal text requires ≥ 7.0.
///
/// # Example
/// ```
/// use oxiui_theme::high_contrast::wcag_contrast;
/// let ratio = wcag_contrast((255, 255, 255), (0, 0, 0));
/// assert!((ratio - 21.0).abs() < 0.1);
/// ```
pub fn wcag_contrast(fg: (u8, u8, u8), bg: (u8, u8, u8)) -> f64 {
    let l1 = wcag_luminance(fg.0, fg.1, fg.2);
    let l2 = wcag_luminance(bg.0, bg.1, bg.2);
    let (lighter, darker) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
    (lighter + 0.05) / (darker + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contrast(fg: Color, bg: Color) -> f64 {
        wcag_contrast((fg.0, fg.1, fg.2), (bg.0, bg.1, bg.2))
    }

    #[test]
    fn hc_light_text_on_background_is_aaa() {
        let p = cooljapan_high_contrast_light();
        let ratio = contrast(p.text, p.background);
        assert!(ratio >= 7.0, "text on background: {ratio:.2} < 7.0");
    }

    #[test]
    fn hc_light_all_text_bg_pairs_are_aaa() {
        let p = cooljapan_high_contrast_light();
        let bg_rgb = (p.background.0, p.background.1, p.background.2);
        // text vs background
        let r_text = wcag_contrast((p.text.0, p.text.1, p.text.2), bg_rgb);
        assert!(r_text >= 7.0, "text on bg: {r_text:.2}");
        // muted (secondary text) vs background
        let r_muted = wcag_contrast((p.muted.0, p.muted.1, p.muted.2), bg_rgb);
        assert!(r_muted >= 7.0, "muted on bg: {r_muted:.2}");
        // primary vs background
        let r_primary = wcag_contrast((p.primary.0, p.primary.1, p.primary.2), bg_rgb);
        assert!(r_primary >= 7.0, "primary on bg: {r_primary:.2}");
        // on_primary vs primary (text on button)
        let primary_rgb = (p.primary.0, p.primary.1, p.primary.2);
        let r_on_primary = wcag_contrast(
            (p.on_primary.0, p.on_primary.1, p.on_primary.2),
            primary_rgb,
        );
        assert!(
            r_on_primary >= 7.0,
            "on_primary on primary: {r_on_primary:.2}"
        );
    }
}
