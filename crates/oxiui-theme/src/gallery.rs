//! Predefined theme gallery — canonical colour-scheme presets.
//!
//! Each constructor returns a ready-to-use [`CooljapanTheme`] built from the
//! design system's published colour values.
//!
//! | Constructor              | Scheme            | Base                                  |
//! |--------------------------|-------------------|---------------------------------------|
//! | [`make_nord_dark`]       | Nord dark         | Polar Night bg, Snow Storm fg         |
//! | [`make_nord_light`]      | Nord light        | Snow Storm bg, Polar Night fg         |
//! | [`make_dracula`]         | Dracula dark      | #282A36 bg, #F8F8F2 fg               |
//! | [`make_solarized_dark`]  | Solarized dark    | base03 bg, base0 fg                   |
//! | [`make_solarized_light`] | Solarized light   | base3 bg, base00 fg                   |
//! | [`make_catppuccin_mocha`]| Catppuccin Mocha  | dark lavender-tinged palette          |
//! | [`make_catppuccin_latte`]| Catppuccin Latte  | light lavender-tinged palette         |
//! | [`make_material_dark`]   | Material dark     | MD3 dark surface tokens               |
//! | [`make_material_light`]  | Material light    | MD3 light surface tokens              |

use crate::CooljapanTheme;
use oxiui_core::{Color, FontSpec, Palette};

fn theme(palette: Palette) -> CooljapanTheme {
    CooljapanTheme::new(palette, FontSpec::new("Inter", 14.0, 400))
}

// ── Nord ─────────────────────────────────────────────────────────────────────
// Palette: https://www.nordtheme.com/
//
// Polar Night: #2E3440 / #3B4252 / #434C5E / #4C566A
// Snow Storm:  #D8DEE9 / #E5E9F0 / #ECEFF4
// Frost:       #8FBCBB / #88C0D0 / #81A1C1 / #5E81AC
// Aurora:      #BF616A / #D08770 / #EBCB8B / #A3BE8C / #B48EAD

/// Nord dark theme — Polar Night background with Snow Storm foreground.
pub fn make_nord_dark() -> CooljapanTheme {
    theme(Palette {
        background: Color(46, 52, 64, 255), // #2E3440 — Polar Night 0
        surface: Color(59, 66, 82, 255),    // #3B4252 — Polar Night 1
        primary: Color(136, 192, 208, 255), // #88C0D0 — Frost 1
        on_primary: Color(46, 52, 64, 255), // Polar Night on Frost
        text: Color(216, 222, 233, 255),    // #D8DEE9 — Snow Storm 0
        muted: Color(76, 86, 106, 255),     // #4C566A — Polar Night 3
    })
}

/// Nord light theme — Snow Storm background with Polar Night foreground.
pub fn make_nord_light() -> CooljapanTheme {
    theme(Palette {
        background: Color(236, 239, 244, 255), // #ECEFF4 — Snow Storm 2
        surface: Color(229, 233, 240, 255),    // #E5E9F0 — Snow Storm 1
        primary: Color(94, 129, 172, 255),     // #5E81AC — Frost 3 (dark blue)
        on_primary: Color(236, 239, 244, 255), // Snow Storm on dark blue
        text: Color(46, 52, 64, 255),          // #2E3440 — Polar Night 0
        muted: Color(76, 86, 106, 255),        // #4C566A — Polar Night 3
    })
}

// ── Dracula ──────────────────────────────────────────────────────────────────
// Palette: https://draculatheme.com/
//
// Background: #282A36, Current Line: #44475A, Foreground: #F8F8F2
// Comment: #6272A4, Cyan: #8BE9FD, Green: #50FA7B, Orange: #FFB86C
// Pink: #FF79C6, Purple: #BD93F9, Red: #FF5555, Yellow: #F1FA8C

/// Dracula dark theme — the canonical purple-dark palette.
pub fn make_dracula() -> CooljapanTheme {
    theme(Palette {
        background: Color(40, 42, 54, 255), // #282A36
        surface: Color(68, 71, 90, 255),    // #44475A — Current Line
        primary: Color(189, 147, 249, 255), // #BD93F9 — Purple
        on_primary: Color(40, 42, 54, 255), // background on purple
        text: Color(248, 248, 242, 255),    // #F8F8F2 — Foreground
        muted: Color(98, 114, 164, 255),    // #6272A4 — Comment
    })
}

// ── Solarized ─────────────────────────────────────────────────────────────────
// Palette: https://ethanschoonover.com/solarized/
//
// base03:  #002B36   base02:  #073642   base01:  #586E75   base00:  #657B83
// base0:   #839496   base1:   #93A1A1   base2:   #EEE8D5   base3:   #FDF6E3
// yellow:  #B58900   orange:  #CB4B16   red:     #DC322F   magenta: #D33682
// violet:  #6C71C4   blue:    #268BD2   cyan:    #2AA198   green:   #859900

/// Solarized dark theme — exact Ethan Schoonover values.
pub fn make_solarized_dark() -> CooljapanTheme {
    theme(Palette {
        background: Color(0, 43, 54, 255), // #002B36 — base03
        surface: Color(7, 54, 66, 255),    // #073642 — base02
        primary: Color(38, 139, 210, 255), // #268BD2 — blue
        on_primary: Color(0, 43, 54, 255), // base03 on blue
        text: Color(131, 148, 150, 255),   // #839496 — base0
        muted: Color(88, 110, 117, 255),   // #586E75 — base01
    })
}

/// Solarized light theme — exact Ethan Schoonover values.
pub fn make_solarized_light() -> CooljapanTheme {
    theme(Palette {
        background: Color(253, 246, 227, 255), // #FDF6E3 — base3
        surface: Color(238, 232, 213, 255),    // #EEE8D5 — base2
        primary: Color(38, 139, 210, 255),     // #268BD2 — blue
        on_primary: Color(253, 246, 227, 255), // base3 on blue
        text: Color(101, 123, 131, 255),       // #657B83 — base00
        muted: Color(147, 161, 161, 255),      // #93A1A1 — base1
    })
}

// ── Catppuccin ────────────────────────────────────────────────────────────────
// Palette: https://github.com/catppuccin/catppuccin
//
// Mocha (dark):
//   Base: #1E1E2E   Mantle: #181825  Crust: #11111B
//   Surface0: #313244  Text: #CDD6F4   Subtext0: #A6ADC8
//   Mauve: #CBA6F7  Blue: #89B4FA
//
// Latte (light):
//   Base: #EFF1F5   Mantle: #E6E9EF  Surface0: #CCD0DA
//   Text: #4C4F69   Subtext0: #6C6F85
//   Mauve: #8839EF  Blue: #1E66F5

/// Catppuccin Mocha (dark) — lavender-tinged dark palette.
pub fn make_catppuccin_mocha() -> CooljapanTheme {
    theme(Palette {
        background: Color(30, 30, 46, 255), // #1E1E2E — Base
        surface: Color(49, 50, 68, 255),    // #313244 — Surface0
        primary: Color(137, 180, 250, 255), // #89B4FA — Blue
        on_primary: Color(30, 30, 46, 255), // Base on Blue
        text: Color(205, 214, 244, 255),    // #CDD6F4 — Text
        muted: Color(166, 173, 200, 255),   // #A6ADC8 — Subtext0
    })
}

/// Catppuccin Latte (light) — lavender-tinged light palette.
pub fn make_catppuccin_latte() -> CooljapanTheme {
    theme(Palette {
        background: Color(239, 241, 245, 255), // #EFF1F5 — Base
        surface: Color(204, 208, 218, 255),    // #CCD0DA — Surface0
        primary: Color(30, 102, 245, 255),     // #1E66F5 — Blue
        on_primary: Color(239, 241, 245, 255), // Base on Blue
        text: Color(76, 79, 105, 255),         // #4C4F69 — Text
        muted: Color(108, 111, 133, 255),      // #6C6F85 — Subtext0
    })
}

// ── Material Design 3 ────────────────────────────────────────────────────────
// Derived from MD3 baseline dark/light colour scheme (purple seed).
//
// Dark:
//   Surface: #141218  Surface Container: #211F26
//   On-Surface: #E6E1E5  Primary: #D0BCFF  On-Primary: #381E72
//
// Light:
//   Surface: #FFFBFE  Surface Container: #F3EDF7
//   On-Surface: #1C1B1F  Primary: #6750A4  On-Primary: #FFFFFF

/// Material Design 3 dark theme — baseline purple scheme, dark variant.
pub fn make_material_dark() -> CooljapanTheme {
    theme(Palette {
        background: Color(20, 18, 24, 255), // #141218 — MD3 surface (dark)
        surface: Color(33, 31, 38, 255),    // #211F26 — MD3 surface container (dark)
        primary: Color(208, 188, 255, 255), // #D0BCFF — MD3 primary (dark)
        on_primary: Color(56, 30, 114, 255), // #381E72 — MD3 on-primary (dark)
        text: Color(230, 225, 229, 255),    // #E6E1E5 — MD3 on-surface (dark)
        muted: Color(147, 143, 153, 255),   // MD3 on-surface-variant (dark)
    })
}

/// Material Design 3 light theme — baseline purple scheme, light variant.
pub fn make_material_light() -> CooljapanTheme {
    theme(Palette {
        background: Color(255, 251, 254, 255), // #FFFBFE — MD3 surface (light)
        surface: Color(243, 237, 247, 255),    // #F3EDF7 — MD3 surface container (light)
        primary: Color(103, 80, 164, 255),     // #6750A4 — MD3 primary (light)
        on_primary: Color(255, 255, 255, 255), // #FFFFFF — MD3 on-primary (light)
        text: Color(28, 27, 31, 255),          // #1C1B1F — MD3 on-surface (light)
        muted: Color(73, 69, 79, 255),         // MD3 on-surface-variant (light)
    })
}

// ── Test ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::high_contrast::wcag_contrast;
    use oxiui_core::Theme;

    fn contrast_text_on_bg(theme: &CooljapanTheme) -> f64 {
        let p = theme.palette();
        wcag_contrast(
            (p.text.0, p.text.1, p.text.2),
            (p.background.0, p.background.1, p.background.2),
        )
    }

    #[test]
    fn nord_dark_constructs_without_panic() {
        let t = make_nord_dark();
        let ratio = contrast_text_on_bg(&t);
        // Snow Storm text on Polar Night bg — visually comfortable, target ≥ 3.0.
        assert!(ratio >= 3.0, "nord dark text/bg contrast: {ratio:.2}");
    }

    #[test]
    fn nord_light_constructs() {
        let _t = make_nord_light();
    }

    #[test]
    fn dracula_constructs() {
        let _t = make_dracula();
    }

    #[test]
    fn solarized_light_constructs_and_is_accessible() {
        let t = make_solarized_light();
        let ratio = contrast_text_on_bg(&t);
        // Solarized light: base00 (#657B83) on base3 (#FDF6E3) ≈ 4.5:1.
        assert!(ratio >= 4.0, "solarized light text/bg contrast: {ratio:.2}");
    }

    #[test]
    fn solarized_dark_constructs() {
        let _t = make_solarized_dark();
    }

    #[test]
    fn catppuccin_mocha_constructs() {
        let _t = make_catppuccin_mocha();
    }

    #[test]
    fn catppuccin_latte_constructs() {
        let _t = make_catppuccin_latte();
    }

    #[test]
    fn material_dark_constructs() {
        let _t = make_material_dark();
    }

    #[test]
    fn material_light_constructs() {
        let _t = make_material_light();
    }

    #[test]
    fn all_gallery_themes_construct() {
        let themes: Vec<CooljapanTheme> = vec![
            make_nord_dark(),
            make_nord_light(),
            make_dracula(),
            make_solarized_dark(),
            make_solarized_light(),
            make_catppuccin_mocha(),
            make_catppuccin_latte(),
            make_material_dark(),
            make_material_light(),
        ];
        assert_eq!(
            themes.len(),
            9,
            "all 9 gallery themes must be constructible"
        );
    }
}
