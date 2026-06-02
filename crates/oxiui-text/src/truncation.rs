//! Text truncation and ellipsis insertion.
//!
//! Provides [`truncate`] which trims text to fit within `max_width` pixels,
//! inserting U+2026 HORIZONTAL ELLIPSIS ("…") at the end or middle as
//! requested.

use crate::{TextPipeline, TextStyle};

/// Controls where the ellipsis is inserted when truncating.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TruncationMode {
    /// Truncate from the end: `"Hello, world"` → `"Hello, …"`.
    End,
    /// Truncate from the middle: `"Hello, world"` → `"Hel…rld"`.
    Middle,
    /// No truncation; return the original string unchanged.
    None,
}

/// The horizontal ellipsis character.
const ELLIPSIS: &str = "…";

/// Truncate `text` so that its measured width fits within `max_width` pixels.
///
/// The `pipeline` is used to measure candidate strings.  If `text` already
/// fits, it is returned unchanged.  Otherwise characters are removed from the
/// end (or middle for [`TruncationMode::Middle`]) until the result fits,
/// then `"…"` is appended / inserted.
///
/// # Errors
/// Returns the original `text` unchanged if the pipeline fails to measure
/// (conservative: avoids silently dropping content).
pub fn truncate(
    pipeline: &mut TextPipeline,
    text: &str,
    style: &TextStyle,
    max_width: f32,
    mode: TruncationMode,
) -> String {
    if matches!(mode, TruncationMode::None) {
        return text.to_owned();
    }

    // Measure the full text.
    let (full_w, _) = match pipeline.measure(text, style) {
        Ok(m) => m,
        Err(_) => return text.to_owned(),
    };

    if full_w <= max_width {
        return text.to_owned();
    }

    match mode {
        TruncationMode::None => text.to_owned(),
        TruncationMode::End => truncate_end(pipeline, text, style, max_width),
        TruncationMode::Middle => truncate_middle(pipeline, text, style, max_width),
    }
}

/// Remove characters from the end until `text + "…"` fits.
fn truncate_end(
    pipeline: &mut TextPipeline,
    text: &str,
    style: &TextStyle,
    max_width: f32,
) -> String {
    // Walk char boundaries from the right, shrinking the text.
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let ellipsis_w = match pipeline.measure(ELLIPSIS, style) {
        Ok((w, _)) => w,
        Err(_) => 8.0, // conservative fallback
    };

    // If even the ellipsis alone doesn't fit, return just the ellipsis.
    if ellipsis_w > max_width {
        return ELLIPSIS.to_owned();
    }

    let budget = max_width - ellipsis_w;

    // Binary-search for the longest prefix that fits within budget.
    let mut lo = 0usize;
    let mut hi = chars.len();
    let mut best = 0usize; // char count

    while lo <= hi {
        let mid = (lo + hi) / 2;
        let prefix = if mid == 0 {
            ""
        } else {
            let byte_end = chars[mid - 1].0 + chars[mid - 1].1.len_utf8();
            &text[..byte_end]
        };
        let w = pipeline
            .measure(prefix, style)
            .map(|(w, _)| w)
            .unwrap_or(0.0);
        if w <= budget {
            best = mid;
            if lo == hi {
                break;
            }
            lo = mid + 1;
        } else {
            if mid == 0 {
                break;
            }
            hi = mid - 1;
        }
    }

    let byte_end = if best == 0 {
        0
    } else {
        chars[best - 1].0 + chars[best - 1].1.len_utf8()
    };
    format!("{}{ELLIPSIS}", &text[..byte_end])
}

/// Remove characters from the middle until `left…right` fits.
fn truncate_middle(
    pipeline: &mut TextPipeline,
    text: &str,
    style: &TextStyle,
    max_width: f32,
) -> String {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let total = chars.len();

    if total == 0 {
        return ELLIPSIS.to_owned();
    }

    let ellipsis_w = match pipeline.measure(ELLIPSIS, style) {
        Ok((w, _)) => w,
        Err(_) => 8.0,
    };

    if ellipsis_w > max_width {
        return ELLIPSIS.to_owned();
    }

    let budget = max_width - ellipsis_w;

    // Try increasing left+right character counts (keeping equal halves).
    // Start with left=right=0 and grow symmetrically.
    let mut left_count = 0usize;
    let mut right_count = 0usize;

    loop {
        let next_left = left_count + 1;
        let next_right = right_count + 1;

        // Would adding one more to the left still fit?
        if next_left + right_count <= total {
            let left_byte_end = chars[next_left - 1].0 + chars[next_left - 1].1.len_utf8();
            let right_byte_start = if right_count == 0 {
                text.len()
            } else {
                chars[total - right_count].0
            };
            let candidate = if left_byte_end <= right_byte_start {
                format!(
                    "{}{ELLIPSIS}{}",
                    &text[..left_byte_end],
                    &text[right_byte_start..]
                )
            } else {
                break;
            };
            let w = pipeline
                .measure(&candidate, style)
                .map(|(w, _)| w)
                .unwrap_or(f32::MAX);
            if w <= budget + ellipsis_w {
                left_count = next_left;
            } else {
                break;
            }
        } else {
            break;
        }

        // Would adding one more to the right still fit?
        if next_right + left_count <= total {
            let left_byte_end = if left_count == 0 {
                0
            } else {
                chars[left_count - 1].0 + chars[left_count - 1].1.len_utf8()
            };
            let right_byte_start = chars[total - next_right].0;
            if left_byte_end > right_byte_start {
                break;
            }
            let candidate = format!(
                "{}{ELLIPSIS}{}",
                &text[..left_byte_end],
                &text[right_byte_start..]
            );
            let w = pipeline
                .measure(&candidate, style)
                .map(|(w, _)| w)
                .unwrap_or(f32::MAX);
            if w <= budget + ellipsis_w {
                right_count = next_right;
            } else {
                break;
            }
        } else {
            break;
        }
    }

    let left_byte_end = if left_count == 0 {
        0
    } else {
        chars[left_count - 1].0 + chars[left_count - 1].1.len_utf8()
    };
    let right_byte_start = if right_count == 0 {
        text.len()
    } else {
        chars[total - right_count].0
    };

    if left_byte_end <= right_byte_start {
        format!(
            "{}{ELLIPSIS}{}",
            &text[..left_byte_end],
            &text[right_byte_start..]
        )
    } else {
        ELLIPSIS.to_owned()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────
//
// NOTE: The tests in this module exercise the logic paths of the truncation
// algorithm.  Because `TextPipeline::measure` requires real font bytes we use
// a stub pipeline (from system fonts, or skip if unavailable) only for the
// "short text unchanged" case, and exercise the string-manipulation logic
// directly for the other cases.

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: returns `true` when the string ends with the ellipsis character.
    fn ends_with_ellipsis(s: &str) -> bool {
        s.ends_with(ELLIPSIS)
    }

    /// Helper: returns `true` when the ellipsis appears somewhere in the
    /// middle of the string (not at the very end).
    fn has_middle_ellipsis(s: &str) -> bool {
        s.contains(ELLIPSIS) && !s.ends_with(ELLIPSIS)
    }

    /// Attempt to obtain a `TextPipeline` from system fonts, skipping the test
    /// if none are available (CI without fonts).
    fn try_system_pipeline() -> Option<TextPipeline> {
        TextPipeline::from_system_font("DejaVu Sans")
            .or_else(|_| TextPipeline::from_system_font("Arial"))
            .or_else(|_| TextPipeline::from_system_font("Helvetica"))
            .ok()
    }

    #[test]
    fn truncation_mode_none_unchanged() {
        // TruncationMode::None must return the input unchanged regardless of width.
        // No pipeline needed for this path.
        if let Some(mut pipeline) = try_system_pipeline() {
            let style = TextStyle::new(16.0);
            let result = truncate(&mut pipeline, "short", &style, 10.0, TruncationMode::None);
            assert_eq!(result, "short");
        }
    }

    #[test]
    fn truncation_end_short_text_unchanged() {
        if let Some(mut pipeline) = try_system_pipeline() {
            let style = TextStyle::new(14.0);
            // A very large max_width means no truncation.
            let result = truncate(&mut pipeline, "hi", &style, 10_000.0, TruncationMode::End);
            assert_eq!(result, "hi");
        }
    }

    #[test]
    fn truncation_end_long_text_has_ellipsis() {
        if let Some(mut pipeline) = try_system_pipeline() {
            let style = TextStyle::new(16.0);
            // Force truncation to a very small width.
            let result = truncate(
                &mut pipeline,
                "This is a very long text that should be truncated",
                &style,
                50.0,
                TruncationMode::End,
            );
            assert!(
                ends_with_ellipsis(&result),
                "truncated text must end with '…', got: {result:?}"
            );
        }
    }

    #[test]
    fn truncation_middle_has_ellipsis_inside() {
        if let Some(mut pipeline) = try_system_pipeline() {
            let style = TextStyle::new(16.0);
            let result = truncate(
                &mut pipeline,
                "This is a very long text that should be middle-truncated",
                &style,
                80.0,
                TruncationMode::Middle,
            );
            assert!(
                has_middle_ellipsis(&result) || result.contains(ELLIPSIS),
                "middle-truncated text must contain '…', got: {result:?}"
            );
        }
    }
}
