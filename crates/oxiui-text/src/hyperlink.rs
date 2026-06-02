//! Hyperlink detection in plain text strings.
//!
//! Uses a conservative hand-written matcher (no regex crate) that finds
//! substrings starting with `http://`, `https://`, or `www.` and ending at
//! the next ASCII whitespace or certain common punctuation characters that
//! are unlikely to be part of a URL.

// ── HyperlinkSpan ─────────────────────────────────────────────────────────────

/// A URL-like substring found within a text string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HyperlinkSpan {
    /// Byte offset of the first character of the URL (inclusive).
    pub start: usize,
    /// Byte offset just past the last character of the URL (exclusive).
    pub end: usize,
    /// The matched URL text.
    pub url: String,
}

// ── Punctuation that terminates a URL ────────────────────────────────────────

/// Characters that are considered URL-terminating when they appear at the
/// end of a candidate URL.  Leading occurrences within the URL are kept.
const URL_TERMINATORS: &[char] = &['.', ',', '!', '?', ';', ':', ')', ']', '}', '\'', '"'];

// ── find_hyperlinks ───────────────────────────────────────────────────────────

/// Find all URL-like substrings in `text`.
///
/// A URL-like substring starts with `http://`, `https://`, or `www.` and
/// continues to the next ASCII whitespace (or end-of-string), with trailing
/// punctuation stripped.
pub fn find_hyperlinks(text: &str) -> Vec<HyperlinkSpan> {
    let mut spans: Vec<HyperlinkSpan> = Vec::new();
    let bytes = text.as_bytes();
    let len = bytes.len();

    let mut i = 0usize;
    while i < len {
        // Try to match a URL prefix at position `i`.
        let prefix = try_match_prefix(text, i);
        if let Some(prefix_end) = prefix {
            // Extend to the end of the URL (whitespace-terminated).
            let url_end = extend_url(text, i, prefix_end);
            let url = &text[i..url_end];
            // Strip trailing punctuation that is probably not part of the URL.
            let url = strip_trailing_punct(url);
            let url_end = i + url.len();
            if url_end > i {
                spans.push(HyperlinkSpan {
                    start: i,
                    end: url_end,
                    url: url.to_owned(),
                });
                i = url_end;
                continue;
            }
        }
        // Advance by one character.
        i += char_len_at(bytes, i);
    }

    spans
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Try to match a URL prefix (`http://`, `https://`, `www.`) starting at
/// `pos` in `text`.  Returns the byte offset of the character immediately
/// after the prefix if matched.
fn try_match_prefix(text: &str, pos: usize) -> Option<usize> {
    let rest = &text[pos..];
    for prefix in &["https://", "http://", "www."] {
        if rest.starts_with(prefix) {
            return Some(pos + prefix.len());
        }
    }
    None
}

/// Return the byte offset just past the end of the URL starting at `start`
/// with the prefix ending at `prefix_end`.  Stops at ASCII whitespace.
fn extend_url(text: &str, start: usize, _prefix_end: usize) -> usize {
    let rest = &text[start..];
    let end_local = rest
        .char_indices()
        .find(|(_, c)| c.is_ascii_whitespace())
        .map(|(i, _)| i)
        .unwrap_or(rest.len());
    start + end_local
}

/// Strip trailing punctuation from the candidate URL slice.
fn strip_trailing_punct(url: &str) -> &str {
    let mut end = url.len();
    while end > 0 {
        let ch = url[..end].chars().next_back().unwrap_or('\0');
        if URL_TERMINATORS.contains(&ch) {
            end -= ch.len_utf8();
        } else {
            break;
        }
    }
    &url[..end]
}

/// Return the byte length of the UTF-8 character starting at `bytes[pos]`.
fn char_len_at(bytes: &[u8], pos: usize) -> usize {
    match bytes[pos] {
        b if b < 0x80 => 1,
        b if b < 0xC0 => 1, // continuation byte — shouldn't start a char
        b if b < 0xE0 => 2,
        b if b < 0xF0 => 3,
        _ => 4,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hyperlink_finds_https() {
        let spans = find_hyperlinks("visit https://example.com today");
        assert_eq!(spans.len(), 1);
        assert!(spans[0].url.starts_with("https://"));
    }

    #[test]
    fn hyperlink_finds_http() {
        let spans = find_hyperlinks("see http://example.com/path");
        assert_eq!(spans.len(), 1);
        assert!(spans[0].url.starts_with("http://"));
    }

    #[test]
    fn hyperlink_finds_www() {
        let spans = find_hyperlinks("see www.example.com");
        assert_eq!(spans.len(), 1);
        assert!(spans[0].url.starts_with("www."));
    }

    #[test]
    fn hyperlink_ignores_plain_text() {
        let spans = find_hyperlinks("hello world");
        assert!(spans.is_empty());
    }

    #[test]
    fn hyperlink_multiple_urls() {
        let spans = find_hyperlinks("a https://a.com b http://b.com c");
        assert_eq!(spans.len(), 2);
    }

    #[test]
    fn hyperlink_strips_trailing_punctuation() {
        let spans = find_hyperlinks("visit https://example.com.");
        assert_eq!(spans.len(), 1);
        assert!(
            !spans[0].url.ends_with('.'),
            "trailing dot must be stripped"
        );
    }

    #[test]
    fn hyperlink_correct_byte_offsets() {
        let text = "x https://example.com y";
        let spans = find_hyperlinks(text);
        assert_eq!(spans.len(), 1);
        let span = &spans[0];
        assert_eq!(&text[span.start..span.end], span.url.as_str());
    }

    #[test]
    fn hyperlink_empty_string() {
        assert!(find_hyperlinks("").is_empty());
    }
}
