//! Font fallback chain with Unicode-range based script detection.
//!
//! When oxifont does not expose per-glyph availability queries, this module
//! implements a conservative Unicode-range heuristic so that CJK, emoji, and
//! Latin text can be routed to appropriate font families without linking a
//! full font parser.

// ── Unicode range helpers ─────────────────────────────────────────────────────

/// Returns `true` when `ch` is in one of the CJK Unicode blocks:
/// CJK Unified Ideographs, Hiragana, Katakana, Hangul, Bopomofo, CJK
/// Compatibility, and related extension blocks.
pub fn is_cjk(ch: char) -> bool {
    matches!(ch,
        // Hiragana
        '\u{3040}'..='\u{309F}' |
        // Katakana
        '\u{30A0}'..='\u{30FF}' |
        // Bopomofo
        '\u{02EA}'..='\u{02EB}' |
        '\u{3105}'..='\u{312F}' |
        '\u{31A0}'..='\u{31BF}' |
        // Hangul
        '\u{1100}'..='\u{11FF}' |
        '\u{302E}'..='\u{302F}' |
        '\u{3131}'..='\u{318E}' |
        '\u{3200}'..='\u{321E}' |
        '\u{3260}'..='\u{327E}' |
        '\u{A960}'..='\u{A97C}' |
        '\u{AC00}'..='\u{D7A3}' |
        '\u{D7B0}'..='\u{D7C6}' |
        '\u{D7CB}'..='\u{D7FB}' |
        '\u{FFA0}'..='\u{FFBE}' |
        '\u{FFC2}'..='\u{FFC7}' |
        '\u{FFCA}'..='\u{FFCF}' |
        '\u{FFD2}'..='\u{FFD7}' |
        '\u{FFDA}'..='\u{FFDC}' |
        // CJK Radicals Supplement
        '\u{2E80}'..='\u{2EFF}' |
        // Kangxi Radicals
        '\u{2F00}'..='\u{2FDF}' |
        // Ideographic Description Characters
        '\u{2FF0}'..='\u{2FFF}' |
        // CJK Symbols and Punctuation
        '\u{3000}'..='\u{303F}' |
        // CJK Unified Ideographs Extension A
        '\u{3400}'..='\u{4DBF}' |
        // CJK Unified Ideographs
        '\u{4E00}'..='\u{9FFF}' |
        // Yi Syllables
        '\u{A000}'..='\u{A48F}' |
        // Yi Radicals
        '\u{A490}'..='\u{A4CF}' |
        // CJK Compatibility Ideographs
        '\u{F900}'..='\u{FAFF}' |
        // CJK Compatibility Forms
        '\u{FE30}'..='\u{FE4F}' |
        // CJK Unified Ideographs Extension B-H
        '\u{20000}'..='\u{3134F}'
    )
}

/// Returns `true` when `ch` is in a standard emoji Unicode block.
pub fn is_emoji(ch: char) -> bool {
    matches!(ch,
        // Emoticons
        '\u{1F600}'..='\u{1F64F}' |
        // Miscellaneous Symbols and Pictographs
        '\u{1F300}'..='\u{1F5FF}' |
        // Transport and Map Symbols
        '\u{1F680}'..='\u{1F6FF}' |
        // Supplemental Symbols and Pictographs
        '\u{1F900}'..='\u{1F9FF}' |
        // Symbols and Pictographs Extended-A
        '\u{1FA00}'..='\u{1FA6F}' |
        '\u{1FA70}'..='\u{1FAFF}' |
        // Enclosed Alphanumeric Supplement (emoji subset including Regional Indicators)
        '\u{1F1E0}'..='\u{1F1FF}' |
        // Dingbats
        '\u{2702}'..='\u{27B0}' |
        // Miscellaneous Symbols
        '\u{2600}'..='\u{26FF}'
    )
}

/// A font family paired with the Unicode-range predicate that covers it.
pub type FamilyEntry = (String, fn(char) -> bool);

// ── FallbackChain ─────────────────────────────────────────────────────────────

/// An ordered list of font families used for glyph-level fallback.
///
/// The resolver walks the chain in order and returns the first family whose
/// Unicode-range heuristic covers `ch`.  The last entry acts as a universal
/// fallback ("tofu" / .notdef).
pub struct FallbackChain {
    /// Entries in priority order.  Each entry is `(family, predicate)`.
    families: Vec<FamilyEntry>,
}

/// Universal accept predicate (tofu / last-resort).
fn accept_all(_ch: char) -> bool {
    true
}

impl FallbackChain {
    /// Construct the default fallback chain:
    ///
    /// 1. CJK fonts  — covers Unified Ideographs, Kana, Hangul …
    /// 2. Emoji font — covers emoji symbol ranges
    /// 3. Latin / universal fallback
    pub fn default_chain() -> Self {
        Self {
            families: vec![
                ("Noto Sans CJK".to_owned(), is_cjk as fn(char) -> bool),
                ("Noto Emoji".to_owned(), is_emoji as fn(char) -> bool),
                ("DejaVu Sans".to_owned(), accept_all as fn(char) -> bool),
            ],
        }
    }

    /// Append a new family at the end of the chain (before the universal
    /// fallback if one is present — this is handled automatically via the
    /// chain-walk in [`Self::resolve_glyph`]).
    pub fn add_family(&mut self, family: String) {
        // Insert before the last (tofu) entry so it takes priority.
        let len = self.families.len();
        let insert_pos = if len > 0 { len - 1 } else { 0 };
        self.families
            .insert(insert_pos, (family, accept_all as fn(char) -> bool));
    }

    /// Return the name of the first family in the chain that can render `ch`.
    ///
    /// Returns `None` only when the chain is empty (which the default chain
    /// never produces).
    pub fn resolve_glyph(&self, ch: char) -> Option<&str> {
        for (family, predicate) in &self.families {
            if predicate(ch) {
                return Some(family.as_str());
            }
        }
        None
    }

    /// Borrow the family list.
    pub fn families(&self) -> &[FamilyEntry] {
        &self.families
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_cjk_detected() {
        assert!(is_cjk('中'), "'中' must be CJK");
        assert!(is_cjk('あ'), "'あ' (Hiragana) must be CJK");
        assert!(!is_cjk('a'), "'a' must not be CJK");
        assert!(!is_cjk('!'), "'!' must not be CJK");
    }

    #[test]
    fn fallback_emoji_detected() {
        assert!(is_emoji('😀'), "'😀' must be emoji");
        assert!(is_emoji('🎉'), "'🎉' must be emoji");
        assert!(!is_emoji('a'), "'a' must not be emoji");
    }

    #[test]
    fn fallback_chain_has_entries() {
        let chain = FallbackChain::default_chain();
        assert!(
            !chain.families().is_empty(),
            "default chain must have entries"
        );
    }

    #[test]
    fn fallback_resolves_cjk_to_cjk_family() {
        let chain = FallbackChain::default_chain();
        let family = chain.resolve_glyph('中').unwrap();
        assert!(
            family.contains("CJK"),
            "CJK char should resolve to a CJK family"
        );
    }

    #[test]
    fn fallback_resolves_emoji() {
        let chain = FallbackChain::default_chain();
        let family = chain.resolve_glyph('😀').unwrap();
        assert!(
            family.to_lowercase().contains("emoji"),
            "emoji should resolve to emoji family"
        );
    }

    #[test]
    fn fallback_resolves_latin_to_last_resort() {
        let chain = FallbackChain::default_chain();
        // 'a' is not CJK or emoji → falls through to the last universal entry.
        let family = chain.resolve_glyph('a').unwrap();
        assert!(!family.is_empty());
    }

    #[test]
    fn fallback_add_family_inserts_before_tofu() {
        let mut chain = FallbackChain::default_chain();
        let original_len = chain.families().len();
        chain.add_family("My Custom Font".to_owned());
        assert_eq!(chain.families().len(), original_len + 1);
    }
}
