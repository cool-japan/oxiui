//! Cell formatting: convert a [`Cell`] to a display string with configurable
//! number formatting, date formatting, or a custom closure.

use crate::Cell;

/// Format a [`Cell`] value as a display string.
///
/// Implementations are `Send + Sync` so they can be stored in column
/// definitions that cross thread boundaries.
pub trait CellFormatter: Send + Sync {
    /// Render `cell` as a display string.
    fn format(&self, cell: &Cell) -> String;
}

// ── Default formatter ────────────────────────────────────────────────────────

/// Uses the cell's [`Display`](std::fmt::Display) impl without modification.
pub struct DefaultFormatter;

impl CellFormatter for DefaultFormatter {
    fn format(&self, cell: &Cell) -> String {
        cell.to_string()
    }
}

// ── Number formatter ─────────────────────────────────────────────────────────

/// Formats `Cell::Int` and `Cell::Float` with configurable decimal places and
/// optional thousands separators. Non-numeric cells fall back to [`DefaultFormatter`].
pub struct NumberFormatter {
    /// Number of decimal places shown for float values (and int values cast to
    /// float when `decimal_places > 0`).
    pub decimal_places: usize,
    /// If `true`, inserts `','` every three integer digits.
    pub thousands_separator: bool,
}

impl NumberFormatter {
    /// Format an integer value with optional thousands separator.
    fn format_integer(n: i64, thousands: bool) -> String {
        let s = n.unsigned_abs().to_string();
        let with_sep = if thousands {
            insert_thousands(s)
        } else {
            n.unsigned_abs().to_string()
        };
        if n < 0 {
            format!("-{with_sep}")
        } else {
            with_sep
        }
    }

    /// Format a float with the requested decimal places and optional thousands
    /// separator on the integer part.
    fn format_float(&self, v: f64) -> String {
        let formatted = format!("{v:.prec$}", prec = self.decimal_places);
        if !self.thousands_separator {
            return formatted;
        }
        // Split at the decimal point (if present).
        match formatted.split_once('.') {
            Some((int_part, dec_part)) => {
                let neg = int_part.starts_with('-');
                let digits = if neg { &int_part[1..] } else { int_part };
                let int_sep = insert_thousands(digits.to_owned());
                let sign = if neg { "-" } else { "" };
                format!("{sign}{int_sep}.{dec_part}")
            }
            None => insert_thousands(formatted),
        }
    }
}

/// Insert commas every three digits from the right of a purely-digit string.
fn insert_thousands(s: String) -> String {
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len();
    let mut out = String::with_capacity(len + len / 3);
    for (i, c) in chars.iter().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(*c);
    }
    out
}

impl CellFormatter for NumberFormatter {
    fn format(&self, cell: &Cell) -> String {
        match cell {
            Cell::Int(n) => {
                if self.decimal_places == 0 {
                    Self::format_integer(*n, self.thousands_separator)
                } else {
                    self.format_float(*n as f64)
                }
            }
            Cell::Float(v) => self.format_float(*v),
            other => DefaultFormatter.format(other),
        }
    }
}

// ── Date formatter ───────────────────────────────────────────────────────────

/// Formats cell values using a strftime-style format string.
///
/// Since `oxiui-table` has no `chrono` dependency, the format is applied to the
/// cell's string representation via simple token substitution. For real date
/// formatting add `chrono` as an optional dependency and gate on a feature flag.
///
/// Currently only `%Y-%m-%d` token pass-through is supported: it returns the
/// cell's display string unchanged (the caller is expected to store the date as a
/// pre-formatted `Cell::Text`).
pub struct DateFormatter {
    /// A strftime-style format string (kept for future chrono integration).
    pub fmt: String,
}

impl CellFormatter for DateFormatter {
    fn format(&self, cell: &Cell) -> String {
        // Without chrono, we can only return the cell's existing representation.
        // When `chrono` is integrated, parse and re-format here.
        cell.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_formatter_strings() {
        let f = DefaultFormatter;
        assert_eq!(f.format(&Cell::Text("hi".into())), "hi");
        assert_eq!(f.format(&Cell::Int(42)), "42");
        assert_eq!(f.format(&Cell::Empty), "");
    }

    #[test]
    fn number_formatter_decimal() {
        let f = NumberFormatter {
            decimal_places: 2,
            thousands_separator: false,
        };
        assert_eq!(f.format(&Cell::Float(1.5)), "1.50");
        assert_eq!(f.format(&Cell::Float(9.876_54)), "9.88");
    }

    #[test]
    fn number_formatter_thousands() {
        let f = NumberFormatter {
            decimal_places: 0,
            thousands_separator: true,
        };
        assert_eq!(f.format(&Cell::Int(1_000_000)), "1,000,000");
        assert_eq!(f.format(&Cell::Int(1_234)), "1,234");
        assert_eq!(f.format(&Cell::Int(999)), "999");
    }

    #[test]
    fn number_formatter_negative() {
        let f = NumberFormatter {
            decimal_places: 0,
            thousands_separator: true,
        };
        assert_eq!(f.format(&Cell::Int(-1_000)), "-1,000");
    }

    #[test]
    fn number_formatter_float_thousands() {
        let f = NumberFormatter {
            decimal_places: 2,
            thousands_separator: true,
        };
        assert_eq!(f.format(&Cell::Float(1234567.891)), "1,234,567.89");
    }

    #[test]
    fn date_formatter_passthrough() {
        let f = DateFormatter {
            fmt: "%Y-%m-%d".into(),
        };
        assert_eq!(f.format(&Cell::Text("2026-05-29".into())), "2026-05-29");
    }
}
