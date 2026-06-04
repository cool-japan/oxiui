//! Typographic scale: named text roles with size, line-height, weight, spacing.

/// A resolved text style for one typographic role.
#[derive(Clone, Copy, Debug, PartialEq, oxicode::Encode, oxicode::Decode)]
pub struct TextStyleToken {
    /// Font size in logical pixels.
    pub size: f32,
    /// Line height in logical pixels (total line box height).
    pub line_height: f32,
    /// Letter spacing in logical pixels (may be negative for tight display text).
    pub letter_spacing: f32,
    /// Font weight (100 thin … 900 black; 400 regular).
    pub weight: u16,
}

impl TextStyleToken {
    /// Construct a text-style token.
    pub const fn new(size: f32, line_height: f32, letter_spacing: f32, weight: u16) -> Self {
        Self {
            size,
            line_height,
            letter_spacing,
            weight,
        }
    }
}

/// The set of named typographic roles for a theme, largest to smallest.
#[derive(Clone, Copy, Debug, PartialEq, oxicode::Encode, oxicode::Decode)]
pub struct TypographyScale {
    /// Largest display text (hero headings).
    pub display: TextStyleToken,
    /// Section headline.
    pub headline: TextStyleToken,
    /// Subsection title.
    pub title: TextStyleToken,
    /// Body / paragraph text.
    pub body: TextStyleToken,
    /// Small caption / helper text.
    pub caption: TextStyleToken,
    /// Smallest overline / label text (often uppercased).
    pub overline: TextStyleToken,
}

impl Default for TypographyScale {
    /// A conventional Material-ish scale anchored at a 14-px body.
    fn default() -> Self {
        Self {
            display: TextStyleToken::new(32.0, 40.0, -0.5, 700),
            headline: TextStyleToken::new(24.0, 32.0, -0.25, 600),
            title: TextStyleToken::new(18.0, 24.0, 0.0, 600),
            body: TextStyleToken::new(14.0, 20.0, 0.0, 400),
            caption: TextStyleToken::new(12.0, 16.0, 0.2, 400),
            overline: TextStyleToken::new(10.0, 14.0, 1.0, 500),
        }
    }
}

impl TypographyScale {
    /// All roles ordered largest → smallest by size.
    pub fn roles_descending(&self) -> [TextStyleToken; 6] {
        [
            self.display,
            self.headline,
            self.title,
            self.body,
            self.caption,
            self.overline,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_are_monotonic_descending() {
        let s = TypographyScale::default();
        let roles = s.roles_descending();
        for w in roles.windows(2) {
            assert!(
                w[0].size >= w[1].size,
                "scale must not increase as it descends"
            );
        }
        // Strict ordering of the key roles.
        assert!(s.caption.size < s.body.size);
        assert!(s.body.size < s.title.size);
        assert!(s.title.size < s.headline.size);
        assert!(s.headline.size < s.display.size);
    }

    #[test]
    fn line_height_at_least_size() {
        let s = TypographyScale::default();
        for role in s.roles_descending() {
            assert!(
                role.line_height >= role.size,
                "line height must cover the glyph size"
            );
        }
    }
}
