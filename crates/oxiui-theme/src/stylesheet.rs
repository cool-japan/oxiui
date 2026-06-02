//! CSS-subset stylesheet parser with cascading specificity resolution.
//!
//! Implements a hand-written recursive-descent parser for a CSS-like syntax
//! supporting type, class, and id selectors plus compound and grouped
//! selectors.  Malformed rules are skipped with diagnostics rather than
//! panicking, ensuring forward progress through the input.
//!
//! # Supported grammar subset
//! ```text
//! stylesheet   = rule*
//! rule         = selector_list '{' declarations '}'
//! selector_list= selector (',' selector)*
//! selector     = simple_selector+
//! simple_selector = type_selector? (class | id)*
//! type_selector= IDENT
//! class        = '.' IDENT
//! id           = '#' IDENT
//! declarations = declaration*
//! declaration  = property ':' value ';'
//! property     = 'color' | 'background' | 'background-color' | 'padding' |
//!                'margin' | 'font-size' | 'font-weight' | 'border-color' |
//!                'border-width' | 'opacity'
//! value        = hex_color | rgb() | rgba() | number | IDENT
//! ```

use oxiui_core::Color;

// ── Value types ────────────────────────────────────────────────────────────────

/// A parsed CSS property value.
#[derive(Debug, Clone, PartialEq)]
pub enum CssValue {
    /// A colour value (e.g. `#ff0000`, `rgb(255, 0, 0)`).
    Color(Color),
    /// A numeric value in pixels or unitless (e.g. `14`, `8px`).
    Number(f32),
    /// An unrecognised keyword (e.g. `bold`, `auto`).
    Keyword(String),
    /// The CSS `inherit` keyword.
    Inherit,
    /// The CSS `initial` keyword.
    Initial,
    /// The CSS `unset` keyword.
    Unset,
}

/// A set of parsed CSS declarations for a single element or rule block.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ComputedStyle {
    /// The `color` property (text / foreground colour).
    pub color: Option<CssValue>,
    /// The `background-color` / `background` property.
    pub background_color: Option<CssValue>,
    /// The `padding` property in logical pixels.
    pub padding: Option<f32>,
    /// The `margin` property in logical pixels.
    pub margin: Option<f32>,
    /// The `font-size` property in logical pixels.
    pub font_size: Option<f32>,
    /// The `font-weight` property (e.g. `400`, `700`).
    pub font_weight: Option<f32>,
    /// The `border-color` property.
    pub border_color: Option<CssValue>,
    /// The `border-width` property in logical pixels.
    pub border_width: Option<f32>,
    /// The `opacity` property in the range `[0.0, 1.0]`.
    pub opacity: Option<f32>,
}

// ── Selector types ─────────────────────────────────────────────────────────────

/// Selector specificity expressed as `(id_count, class_count, type_count)`.
///
/// Higher tuples win in a CSS cascade.  The ordering is lexicographic, which
/// matches the CSS specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Specificity(pub u32, pub u32, pub u32);

impl Specificity {
    /// Increment the id component (each `#id` selector part).
    pub fn add_id(&mut self) {
        self.0 += 1;
    }

    /// Increment the class component (each `.class` selector part).
    pub fn add_class(&mut self) {
        self.1 += 1;
    }

    /// Increment the type component (each element-type selector part).
    pub fn add_type(&mut self) {
        self.2 += 1;
    }
}

/// A single part of a compound selector.
#[derive(Debug, Clone)]
pub enum SelectorPart {
    /// An element-type selector (e.g. `button`).
    Type(String),
    /// A class selector (e.g. `.primary`).
    Class(String),
    /// An id selector (e.g. `#submit`).
    Id(String),
}

/// A parsed CSS selector with pre-computed specificity.
#[derive(Debug, Clone)]
pub struct Selector {
    /// The ordered parts that make up this compound selector.
    pub parts: Vec<SelectorPart>,
    /// Pre-computed specificity used for cascade ordering.
    pub specificity: Specificity,
}

/// A parsed CSS rule: one or more selectors paired with a declarations block.
#[derive(Debug, Clone)]
pub struct Rule {
    /// The selectors that trigger this rule.
    pub selectors: Vec<Selector>,
    /// The computed style declared in the rule block.
    pub style: ComputedStyle,
    /// Insertion index within the stylesheet; used to break specificity ties.
    pub source_order: usize,
}

/// A parsed stylesheet containing zero or more rules.
#[derive(Debug, Clone, Default)]
pub struct StyleSheet {
    /// The rules parsed from the CSS source, in source order.
    pub rules: Vec<Rule>,
}

/// A single non-fatal parse error.
#[derive(Debug, Clone)]
pub struct ParseDiagnostic {
    /// Byte offset in the source where the error was detected.
    pub offset: usize,
    /// Human-readable description of the problem.
    pub message: String,
}

/// The result of parsing a stylesheet.
#[derive(Debug, Clone, Default)]
pub struct ParseResult {
    /// The successfully parsed stylesheet (may contain fewer rules than the
    /// source if some rules triggered diagnostics).
    pub stylesheet: StyleSheet,
    /// Non-fatal parse errors encountered during parsing.
    pub diagnostics: Vec<ParseDiagnostic>,
}

// ── StyleSheet impl ────────────────────────────────────────────────────────────

impl StyleSheet {
    /// Parse a CSS-subset string.
    ///
    /// Malformed rules are skipped with [`ParseDiagnostic`]s rather than
    /// causing a panic.  Valid rules following a malformed one are still
    /// parsed.
    pub fn parse(input: &str) -> ParseResult {
        let mut parser = Parser::new(input);
        parser.parse_stylesheet()
    }

    /// Find all rules that match a widget described by type, classes, and id.
    ///
    /// Returns rules sorted ascending by `(specificity, source_order)` so that
    /// the caller can apply them in order and the last write wins (standard CSS
    /// cascade).
    pub fn matching_rules<'a>(
        &'a self,
        widget_type: &str,
        classes: &[&str],
        id: Option<&str>,
    ) -> Vec<(&'a Rule, Specificity)> {
        let mut matches = Vec::new();
        for rule in &self.rules {
            for selector in &rule.selectors {
                if selector_matches(selector, widget_type, classes, id) {
                    matches.push((rule, selector.specificity));
                    break; // first matching selector for this rule is sufficient
                }
            }
        }
        matches.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.source_order.cmp(&b.0.source_order)));
        matches
    }

    /// Compute the final [`ComputedStyle`] for a widget by cascading all
    /// matching rules.
    ///
    /// Rules are applied in ascending specificity / source-order, so the last
    /// (most specific) rule wins for each property.
    pub fn compute_style(
        &self,
        widget_type: &str,
        classes: &[&str],
        id: Option<&str>,
    ) -> ComputedStyle {
        let mut result = ComputedStyle::default();
        for (rule, _) in self.matching_rules(widget_type, classes, id) {
            apply_rule(&mut result, &rule.style);
        }
        result
    }
}

/// Returns `true` if every part of `sel` matches the described widget.
pub(crate) fn selector_matches(
    sel: &Selector,
    widget_type: &str,
    classes: &[&str],
    id: Option<&str>,
) -> bool {
    for part in &sel.parts {
        match part {
            SelectorPart::Type(t) => {
                if t != widget_type {
                    return false;
                }
            }
            SelectorPart::Class(c) => {
                if !classes.contains(&c.as_str()) {
                    return false;
                }
            }
            SelectorPart::Id(i) => {
                if id != Some(i.as_str()) {
                    return false;
                }
            }
        }
    }
    true
}

/// Apply all set properties from `source` into `target` (last-writer-wins).
pub(crate) fn apply_rule(target: &mut ComputedStyle, source: &ComputedStyle) {
    if let Some(v) = &source.color {
        target.color = Some(v.clone());
    }
    if let Some(v) = &source.background_color {
        target.background_color = Some(v.clone());
    }
    if let Some(v) = source.padding {
        target.padding = Some(v);
    }
    if let Some(v) = source.margin {
        target.margin = Some(v);
    }
    if let Some(v) = source.font_size {
        target.font_size = Some(v);
    }
    if let Some(v) = source.font_weight {
        target.font_weight = Some(v);
    }
    if let Some(v) = &source.border_color {
        target.border_color = Some(v.clone());
    }
    if let Some(v) = source.border_width {
        target.border_width = Some(v);
    }
    if let Some(v) = source.opacity {
        target.opacity = Some(v);
    }
}

// ── Parser ─────────────────────────────────────────────────────────────────────

struct Parser<'a> {
    input: &'a str,
    pos: usize,
    source_order: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input,
            pos: 0,
            source_order: 0,
        }
    }

    fn remaining(&self) -> &str {
        &self.input[self.pos..]
    }

    fn is_eof(&self) -> bool {
        self.pos >= self.input.len()
    }

    fn skip_whitespace(&mut self) {
        while !self.is_eof() {
            let ch = self.remaining().chars().next().unwrap_or('\0');
            if ch.is_whitespace() {
                self.pos += ch.len_utf8();
            } else if self.remaining().starts_with("/*") {
                if let Some(end) = self.remaining().find("*/") {
                    self.pos += end + 2;
                } else {
                    self.pos = self.input.len();
                }
            } else {
                break;
            }
        }
    }

    fn parse_ident(&mut self) -> Option<String> {
        self.skip_whitespace();
        let start = self.pos;
        let mut end = start;
        for (i, ch) in self.remaining().char_indices() {
            if ch.is_alphanumeric() || ch == '-' || ch == '_' {
                end = start + i + ch.len_utf8();
            } else {
                break;
            }
        }
        if end > start {
            let ident = self.input[start..end].to_owned();
            self.pos = end;
            Some(ident)
        } else {
            None
        }
    }

    fn consume_char(&mut self, expected: char) -> bool {
        self.skip_whitespace();
        if self.remaining().starts_with(expected) {
            self.pos += expected.len_utf8();
            true
        } else {
            false
        }
    }

    fn parse_stylesheet(&mut self) -> ParseResult {
        let mut rules = Vec::new();
        let mut diagnostics = Vec::new();
        self.skip_whitespace();
        while !self.is_eof() {
            let before = self.pos;
            match self.parse_rule() {
                Ok(Some(rule)) => rules.push(rule),
                Ok(None) => {}
                Err(d) => {
                    diagnostics.push(d);
                    self.recover_to_next_rule();
                }
            }
            // Guarantee forward progress so we never loop on unmovable input.
            if self.pos == before {
                let step = self
                    .remaining()
                    .chars()
                    .next()
                    .map(char::len_utf8)
                    .unwrap_or(1);
                self.pos += step;
            }
            self.skip_whitespace();
        }
        ParseResult {
            stylesheet: StyleSheet { rules },
            diagnostics,
        }
    }

    fn recover_to_next_rule(&mut self) {
        while !self.is_eof() {
            if self.remaining().starts_with('}') {
                self.pos += 1;
                break;
            }
            self.pos += 1;
        }
    }

    fn parse_rule(&mut self) -> Result<Option<Rule>, ParseDiagnostic> {
        let selectors = self.parse_selector_list()?;
        if selectors.is_empty() {
            return Ok(None);
        }
        if !self.consume_char('{') {
            return Err(ParseDiagnostic {
                offset: self.pos,
                message: "expected '{'".into(),
            });
        }
        let style = self.parse_declarations();
        self.consume_char('}');
        let order = self.source_order;
        self.source_order += 1;
        Ok(Some(Rule {
            selectors,
            style,
            source_order: order,
        }))
    }

    fn parse_selector_list(&mut self) -> Result<Vec<Selector>, ParseDiagnostic> {
        let mut selectors = Vec::new();
        loop {
            self.skip_whitespace();
            if self.is_eof() || self.remaining().starts_with('{') {
                break;
            }
            if let Some(sel) = self.parse_selector() {
                selectors.push(sel);
            }
            self.skip_whitespace();
            if self.remaining().starts_with(',') {
                self.pos += 1;
            } else {
                break;
            }
        }
        Ok(selectors)
    }

    fn parse_selector(&mut self) -> Option<Selector> {
        self.skip_whitespace();
        let mut parts = Vec::new();
        let mut spec = Specificity::default();
        loop {
            self.skip_whitespace();
            if self.is_eof()
                || self.remaining().starts_with('{')
                || self.remaining().starts_with(',')
            {
                break;
            }
            if self.remaining().starts_with('.') {
                self.pos += 1;
                if let Some(class) = self.parse_ident() {
                    spec.add_class();
                    parts.push(SelectorPart::Class(class));
                }
            } else if self.remaining().starts_with('#') {
                self.pos += 1;
                if let Some(id) = self.parse_ident() {
                    spec.add_id();
                    parts.push(SelectorPart::Id(id));
                }
            } else if let Some(ident) = self.parse_ident() {
                spec.add_type();
                parts.push(SelectorPart::Type(ident));
            } else {
                break;
            }
        }
        if parts.is_empty() {
            None
        } else {
            Some(Selector {
                parts,
                specificity: spec,
            })
        }
    }

    fn parse_declarations(&mut self) -> ComputedStyle {
        let mut style = ComputedStyle::default();
        loop {
            self.skip_whitespace();
            if self.is_eof() || self.remaining().starts_with('}') {
                break;
            }
            self.parse_declaration(&mut style);
        }
        style
    }

    fn parse_declaration(&mut self, style: &mut ComputedStyle) {
        self.skip_whitespace();
        let prop = match self.parse_ident() {
            Some(p) => p,
            None => {
                self.skip_to_semicolon();
                return;
            }
        };
        self.skip_whitespace();
        if !self.consume_char(':') {
            self.skip_to_semicolon();
            return;
        }
        self.skip_whitespace();
        let value = self.parse_value();
        self.skip_to_semicolon();
        if let Some(v) = value {
            match prop.as_str() {
                "color" => style.color = Some(v),
                "background" | "background-color" => style.background_color = Some(v),
                "padding" => {
                    if let CssValue::Number(n) = &v {
                        style.padding = Some(*n);
                    }
                }
                "margin" => {
                    if let CssValue::Number(n) = &v {
                        style.margin = Some(*n);
                    }
                }
                "font-size" => {
                    if let CssValue::Number(n) = &v {
                        style.font_size = Some(*n);
                    }
                }
                "font-weight" => {
                    if let CssValue::Number(n) = &v {
                        style.font_weight = Some(*n);
                    }
                }
                "border-color" => style.border_color = Some(v),
                "border-width" => {
                    if let CssValue::Number(n) = &v {
                        style.border_width = Some(*n);
                    }
                }
                "opacity" => {
                    if let CssValue::Number(n) = &v {
                        style.opacity = Some(*n);
                    }
                }
                _ => {} // unknown property silently ignored
            }
        }
    }

    fn parse_value(&mut self) -> Option<CssValue> {
        self.skip_whitespace();
        if self.remaining().starts_with('#') {
            self.pos += 1;
            return self.parse_hex_color();
        }
        if self.remaining().starts_with("rgba(") || self.remaining().starts_with("rgb(") {
            return self.parse_rgb_color();
        }
        // Try number (with optional `px` suffix).
        let start = self.pos;
        let mut end = start;
        let mut has_digit = false;
        let mut has_dot = false;
        for (i, ch) in self.remaining().char_indices() {
            if ch.is_ascii_digit() {
                has_digit = true;
                end = start + i + 1;
            } else if ch == '.' && !has_dot {
                has_dot = true;
                end = start + i + 1;
            } else if ch == 'p' || ch == 'x' {
                end = start + i + 1; // consume 'px' suffix
            } else {
                break;
            }
        }
        if has_digit {
            let num_str: String = self.input[start..end]
                .chars()
                .filter(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            self.pos = end;
            return num_str.parse::<f32>().ok().map(CssValue::Number);
        }
        // Keyword (inherit / initial / unset / other).
        if let Some(ident) = self.parse_ident() {
            return Some(match ident.as_str() {
                "inherit" => CssValue::Inherit,
                "initial" => CssValue::Initial,
                "unset" => CssValue::Unset,
                _ => CssValue::Keyword(ident),
            });
        }
        None
    }

    fn parse_hex_color(&mut self) -> Option<CssValue> {
        let start = self.pos;
        let hex: String = self
            .remaining()
            .chars()
            .take_while(|c| c.is_ascii_hexdigit())
            .collect();
        self.pos += hex.len();
        let color = match hex.len() {
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                Color(r, g, b, 255)
            }
            8 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
                Color(r, g, b, a)
            }
            3 => {
                let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?;
                let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?;
                let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?;
                Color(r, g, b, 255)
            }
            _ => {
                self.pos = start;
                return None;
            }
        };
        Some(CssValue::Color(color))
    }

    fn parse_rgb_color(&mut self) -> Option<CssValue> {
        let skip = if self.remaining().starts_with("rgba(") {
            5
        } else {
            4
        };
        self.pos += skip;
        let r = self.parse_number_u8()?;
        self.consume_char(',');
        let g = self.parse_number_u8()?;
        self.consume_char(',');
        let b = self.parse_number_u8()?;
        let a = if self.remaining().trim_start().starts_with(',') {
            self.consume_char(',');
            self.skip_whitespace();
            let alpha_str: String = self
                .remaining()
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            self.pos += alpha_str.len();
            (alpha_str.parse::<f32>().unwrap_or(1.0) * 255.0) as u8
        } else {
            255
        };
        self.consume_char(')');
        Some(CssValue::Color(Color(r, g, b, a)))
    }

    fn parse_number_u8(&mut self) -> Option<u8> {
        self.skip_whitespace();
        let digits: String = self
            .remaining()
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if digits.is_empty() {
            return None;
        }
        self.pos += digits.len();
        digits.parse::<u8>().ok()
    }

    fn skip_to_semicolon(&mut self) {
        while !self.is_eof() {
            if self.remaining().starts_with(';') {
                self.pos += 1;
                break;
            }
            if self.remaining().starts_with('}') {
                break; // do not consume '}'
            }
            self.pos += 1;
        }
    }
}
