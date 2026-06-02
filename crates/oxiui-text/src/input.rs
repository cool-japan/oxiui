//! Single-line editable text input widget state.
//!
//! [`TextInput`] is a pure data structure — rendering is handled by adapters.

use crate::layout::TextLayout;
use crate::selection::Selection;

// ── Helper ────────────────────────────────────────────────────────────────────

/// Find the largest index `≤ i` that is a char boundary in `s`.
///
/// This is a stable-toolchain substitute for the nightly `str::floor_char_boundary`.
fn floor_char_boundary(s: &str, i: usize) -> usize {
    let mut pos = i.min(s.len());
    while pos > 0 && !s.is_char_boundary(pos) {
        pos -= 1;
    }
    pos
}

// ── TextInput ─────────────────────────────────────────────────────────────────

/// Single-line editable text input widget state.
///
/// Wraps the text string, cursor byte-offset, selection, horizontal scroll,
/// and optional password masking.  All editing operations keep the cursor
/// and selection consistent.
#[derive(Debug, Clone)]
pub struct TextInput {
    text: String,
    /// Byte offset of the cursor position.
    cursor: usize,
    /// Anchor/focus selection over the text.
    selection: Selection,
    /// Horizontal scroll in logical pixels.
    scroll_offset: f32,
    /// Masking character.  `None` = plain text; `Some(c)` = password mode.
    mask_char: Option<char>,
    /// When `true` (and `mask_char.is_some()`), show the real text.
    show_masked: bool,
}

impl TextInput {
    /// Create an empty `TextInput` with no masking.
    pub fn new() -> Self {
        Self {
            text: String::new(),
            cursor: 0,
            selection: Selection::new(0),
            scroll_offset: 0.0,
            mask_char: None,
            show_masked: false,
        }
    }

    /// Create a `TextInput` pre-populated with `text`, cursor at the end.
    pub fn with_text(text: impl Into<String>) -> Self {
        let text = text.into();
        let len = text.len();
        Self {
            text,
            cursor: len,
            selection: Selection::new(len),
            scroll_offset: 0.0,
            mask_char: None,
            show_masked: false,
        }
    }

    /// Enable password masking with the bullet character U+2022.
    pub fn with_password(mut self) -> Self {
        self.mask_char = Some('\u{2022}');
        self
    }

    // ── Getters ───────────────────────────────────────────────────────────────

    /// Borrow the raw (un-masked) text.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Current cursor byte offset.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Current selection.
    pub fn selection(&self) -> &Selection {
        &self.selection
    }

    /// Current horizontal scroll offset in logical pixels.
    pub fn scroll_offset(&self) -> f32 {
        self.scroll_offset
    }

    /// Returns `true` when password masking is active.
    pub fn is_password(&self) -> bool {
        self.mask_char.is_some()
    }

    /// Returns `true` when the underlying password text is currently visible.
    pub fn is_showing_password(&self) -> bool {
        self.show_masked
    }

    /// The text to display — masked when in password mode (unless show-password
    /// is toggled on).
    pub fn display_text(&self) -> String {
        if let Some(mask) = self.mask_char {
            if !self.show_masked {
                return self.text.chars().map(|_| mask).collect();
            }
        }
        self.text.clone()
    }

    /// Toggle the show/hide state for password fields.  No-op on plain fields.
    pub fn toggle_show_password(&mut self) {
        self.show_masked = !self.show_masked;
    }

    /// Borrow the currently selected slice of the raw text.
    ///
    /// Returns an empty string when the selection is collapsed.
    pub fn selected_text(&self) -> &str {
        if self.selection.is_collapsed() {
            return "";
        }
        let (start, end) = self.selection.normalized();
        let start = floor_char_boundary(&self.text, start.min(self.text.len()));
        let end = floor_char_boundary(&self.text, end.min(self.text.len()));
        &self.text[start..end]
    }

    // ── Editing operations ────────────────────────────────────────────────────

    /// Insert a string at the cursor position, replacing any active selection.
    pub fn insert(&mut self, s: &str) {
        self.delete_selection();
        let pos = floor_char_boundary(&self.text, self.cursor.min(self.text.len()));
        self.text.insert_str(pos, s);
        self.cursor = pos + s.len();
        self.selection = Selection::new(self.cursor);
    }

    /// Insert a single character at the cursor position.
    pub fn insert_char(&mut self, c: char) {
        let mut buf = [0u8; 4];
        let s = c.encode_utf8(&mut buf);
        self.insert(s);
    }

    /// Delete the character immediately before the cursor (backspace).
    ///
    /// If there is an active selection, deletes the selection instead.
    pub fn delete_backward(&mut self) {
        if !self.selection.is_collapsed() {
            self.delete_selection();
            return;
        }
        if self.cursor == 0 {
            return;
        }
        let pos = floor_char_boundary(&self.text, self.cursor);
        let mut prev = pos.saturating_sub(1);
        while prev > 0 && !self.text.is_char_boundary(prev) {
            prev -= 1;
        }
        self.text.replace_range(prev..pos, "");
        self.cursor = prev;
        self.selection = Selection::new(self.cursor);
    }

    /// Delete the character immediately after the cursor (Delete key).
    ///
    /// If there is an active selection, deletes the selection instead.
    pub fn delete_forward(&mut self) {
        if !self.selection.is_collapsed() {
            self.delete_selection();
            return;
        }
        let pos = floor_char_boundary(&self.text, self.cursor.min(self.text.len()));
        if pos >= self.text.len() {
            return;
        }
        let mut next = pos + 1;
        while next < self.text.len() && !self.text.is_char_boundary(next) {
            next += 1;
        }
        self.text.replace_range(pos..next, "");
        self.selection = Selection::new(self.cursor);
    }

    // ── Cursor movement ───────────────────────────────────────────────────────

    /// Move the cursor one character to the left.
    ///
    /// When `shift` is `false` and there is an active selection, the cursor
    /// jumps to the start of the selection without extending it.
    pub fn move_left(&mut self, shift: bool) {
        if !shift && !self.selection.is_collapsed() {
            let (start, _) = self.selection.normalized();
            self.cursor = start;
        } else if self.cursor > 0 {
            let mut pos = self.cursor.saturating_sub(1);
            while pos > 0 && !self.text.is_char_boundary(pos) {
                pos -= 1;
            }
            self.cursor = pos;
        }
        if shift {
            self.selection = Selection {
                anchor: self.selection.anchor,
                focus: self.cursor,
            };
        } else {
            self.selection = Selection::new(self.cursor);
        }
    }

    /// Move the cursor one character to the right.
    ///
    /// When `shift` is `false` and there is an active selection, the cursor
    /// jumps to the end of the selection without extending it.
    pub fn move_right(&mut self, shift: bool) {
        if !shift && !self.selection.is_collapsed() {
            let (_, end) = self.selection.normalized();
            self.cursor = end;
        } else {
            let pos = floor_char_boundary(&self.text, self.cursor.min(self.text.len()));
            if pos < self.text.len() {
                let mut next = pos + 1;
                while next < self.text.len() && !self.text.is_char_boundary(next) {
                    next += 1;
                }
                self.cursor = next;
            }
        }
        if shift {
            self.selection = Selection {
                anchor: self.selection.anchor,
                focus: self.cursor,
            };
        } else {
            self.selection = Selection::new(self.cursor);
        }
    }

    /// Move the cursor to the beginning of the text (Home key).
    pub fn move_home(&mut self, shift: bool) {
        self.cursor = 0;
        if shift {
            self.selection = Selection {
                anchor: self.selection.anchor,
                focus: 0,
            };
        } else {
            self.selection = Selection::new(0);
        }
    }

    /// Move the cursor to the end of the text (End key).
    pub fn move_end(&mut self, shift: bool) {
        self.cursor = self.text.len();
        if shift {
            self.selection = Selection {
                anchor: self.selection.anchor,
                focus: self.text.len(),
            };
        } else {
            self.selection = Selection::new(self.text.len());
        }
    }

    /// Move the cursor one word to the left (Ctrl+Left).
    pub fn move_word_left(&mut self, shift: bool) {
        let new_focus = Selection::extend_word_backward(&self.text, self.cursor);
        self.cursor = new_focus;
        if shift {
            self.selection = Selection {
                anchor: self.selection.anchor,
                focus: new_focus,
            };
        } else {
            self.selection = Selection::new(new_focus);
        }
    }

    /// Move the cursor one word to the right (Ctrl+Right).
    pub fn move_word_right(&mut self, shift: bool) {
        let new_focus = Selection::extend_word_forward(&self.text, self.cursor);
        self.cursor = new_focus;
        if shift {
            self.selection = Selection {
                anchor: self.selection.anchor,
                focus: new_focus,
            };
        } else {
            self.selection = Selection::new(new_focus);
        }
    }

    /// Move the cursor to the byte position nearest to the pixel x-coordinate.
    ///
    /// Uses `TextLayout::hit_test` for positioning.  When `shift` is `true`,
    /// the selection anchor is preserved (keyboard-extend behaviour).
    pub fn move_cursor_to_x(&mut self, x: f32, layout: &TextLayout, shift: bool) {
        let byte_offset = layout.hit_test(x, 0.0);
        self.cursor = byte_offset;
        if shift {
            self.selection = Selection {
                anchor: self.selection.anchor,
                focus: byte_offset,
            };
        } else {
            self.selection = Selection::new(byte_offset);
        }
    }

    /// Single-click — move the cursor and collapse the selection.
    pub fn click(&mut self, x: f32, layout: &TextLayout) {
        self.move_cursor_to_x(x, layout, false);
    }

    /// Double-click — select the word nearest to the click position.
    pub fn double_click(&mut self, x: f32, layout: &TextLayout) {
        let pos = layout.hit_test(x, 0.0);
        let word_start = Selection::extend_word_backward(&self.text, pos);
        let word_end = Selection::extend_word_forward(&self.text, pos);
        self.cursor = word_end;
        self.selection = Selection {
            anchor: word_start,
            focus: word_end,
        };
    }

    /// Triple-click — select all text.
    pub fn triple_click(&mut self) {
        self.cursor = self.text.len();
        self.selection = Selection {
            anchor: 0,
            focus: self.text.len(),
        };
    }

    /// Select all text (Ctrl+A equivalent).
    pub fn select_all(&mut self) {
        self.triple_click();
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    fn delete_selection(&mut self) {
        if self.selection.is_collapsed() {
            return;
        }
        let (start, end) = self.selection.normalized();
        let start = floor_char_boundary(&self.text, start.min(self.text.len()));
        let end = floor_char_boundary(&self.text, end.min(self.text.len()));
        self.text.replace_range(start..end, "");
        self.cursor = start;
        self.selection = Selection::new(start);
    }
}

impl Default for TextInput {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{TextAlign, TextLayout};
    use crate::{GlyphPosition, ShapedText};

    /// Build a minimal single-line layout from a string (using fixed 8px/char).
    fn fake_layout(text: &str) -> TextLayout {
        let char_w = 8.0_f32;
        let glyphs: Vec<GlyphPosition> = text
            .char_indices()
            .enumerate()
            .map(|(i, (byte_off, _))| GlyphPosition {
                byte_offset: byte_off,
                x: i as f32 * char_w,
                y: 0.0,
                width: char_w,
                height: 16.0,
            })
            .collect();
        let total_width = glyphs.len() as f32 * char_w;
        let shaped = ShapedText {
            lines: vec![glyphs],
            total_width,
            total_height: 16.0,
        };
        TextLayout {
            shaped,
            align: TextAlign::Left,
            bounds: (total_width, 16.0),
        }
    }

    #[test]
    fn insert_at_cursor() {
        let mut input = TextInput::new();
        input.insert("hello");
        assert_eq!(input.text(), "hello");
        assert_eq!(input.cursor(), 5);
    }

    #[test]
    fn delete_backward_basic() {
        let mut input = TextInput::with_text("hello");
        input.delete_backward();
        assert_eq!(input.text(), "hell");
        assert_eq!(input.cursor(), 4);
    }

    #[test]
    fn delete_backward_no_panic_at_zero() {
        let mut input = TextInput::new();
        input.delete_backward(); // cursor = 0 → no-op, no panic
        assert_eq!(input.text(), "");
    }

    #[test]
    fn delete_forward_basic() {
        let mut input = TextInput::with_text("hello");
        input.move_home(false);
        input.delete_forward();
        assert_eq!(input.text(), "ello");
        assert_eq!(input.cursor(), 0);
    }

    #[test]
    fn move_left_right_simple() {
        let mut input = TextInput::with_text("ab");
        input.move_home(false);
        input.move_right(false);
        assert_eq!(input.cursor(), 1);
        input.move_left(false);
        assert_eq!(input.cursor(), 0);
    }

    #[test]
    fn move_word_left_right() {
        let mut input = TextInput::with_text("hello world");
        // Cursor starts at end (11)
        input.move_word_left(false);
        assert_eq!(
            input.cursor(),
            6,
            "word-left should land at start of 'world'"
        );
        input.move_word_right(false);
        assert_eq!(
            input.cursor(),
            11,
            "word-right should land at end of 'world'"
        );
    }

    #[test]
    fn move_home_end() {
        let mut input = TextInput::with_text("hello");
        input.move_home(false);
        assert_eq!(input.cursor(), 0);
        input.move_end(false);
        assert_eq!(input.cursor(), 5);
    }

    #[test]
    fn triple_click_selects_all() {
        let mut input = TextInput::with_text("hello world");
        input.triple_click();
        assert_eq!(input.selected_text(), "hello world");
    }

    #[test]
    fn select_all() {
        let mut input = TextInput::with_text("hello world");
        input.select_all();
        assert_eq!(input.selected_text(), "hello world");
    }

    #[test]
    fn double_click_selects_word() {
        let mut input = TextInput::with_text("hello world");
        let layout = fake_layout("hello world");
        // Click in the middle of "world" (index 7 → x=56)
        input.double_click(56.0, &layout);
        let sel = input.selected_text();
        // Should select "world" or at least be non-empty
        assert!(!sel.is_empty(), "double-click must select a word");
    }

    #[test]
    fn password_mask_same_length() {
        let input = TextInput::with_text("secret").with_password();
        let display = input.display_text();
        let orig_chars = "secret".chars().count();
        let disp_chars = display.chars().count();
        assert_eq!(
            orig_chars, disp_chars,
            "masked text must have the same char count"
        );
        assert!(
            !display.contains('s'),
            "masked text must not contain raw characters"
        );
    }

    #[test]
    fn password_toggle_show_hide() {
        let mut input = TextInput::with_text("secret").with_password();
        assert!(!input.is_showing_password());
        let masked = input.display_text();
        input.toggle_show_password();
        assert!(input.is_showing_password());
        let visible = input.display_text();
        assert_eq!(visible, "secret");
        assert_ne!(masked, visible);
    }

    #[test]
    fn insert_replaces_selection() {
        let mut input = TextInput::with_text("hello world");
        input.select_all();
        input.insert("replaced");
        assert_eq!(input.text(), "replaced");
    }

    #[test]
    fn shift_right_extends_selection() {
        let mut input = TextInput::with_text("hello");
        input.move_home(false);
        input.move_right(true);
        input.move_right(true);
        assert_eq!(input.selected_text(), "he");
    }
}
