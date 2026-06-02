#![forbid(unsafe_code)]
#![warn(missing_docs)]
//! `oxiui-core` — Pure-Rust UI core traits and types.
//!
//! Zero external dependencies. Adapters (`oxiui-egui`, `oxiui-render-wgpu`, …)
//! implement the traits defined here; the `oxiui` facade wires them together.
//!
//! In addition to the immediate-mode trait surface (`UiCtx`, `Widget`, …) the
//! crate provides foundational building blocks consumed across the stack:
//!
//! - [`geometry`] — `Point`, `Size`, `Rect`, `Insets`, `Constraints`.
//! - [`events`] — `MouseButton`, `Modifiers`, `Key`, `ScrollDelta`.
//! - [`tree`] — a retained `WidgetTree` with stable ids and hit testing.
//! - [`layout`] — a single-line flexbox solver (`FlexLayout`).

pub mod anim;
pub mod cache;
pub mod color_space;
pub mod diff;
pub mod dispatch;
pub mod events;
pub mod focus;
pub mod geometry;
pub mod grid;
pub mod layout;
pub mod paint;
pub mod reactive;
pub mod response;
pub mod scheduler;
pub mod solver;
pub mod style;
pub mod text_style;
pub mod tree;
pub mod widget_ext;

pub use anim::{Animator, Easing, Spring, Transition};
pub use cache::LayoutCache;
pub use color_space::{
    contrast_ratio, ContrastWarning, Hsla, LinearRgba, Oklcha, PaletteBuilder, WcagLevel,
};
pub use diff::{diff, DiffOp};
pub use dispatch::{DispatchEvent, EventDispatcher, EventHandler, HandlerCtx, Phase};
pub use events::{
    GestureKind, Key, KeyboardEvent, Modifiers, MouseButton, MouseEvent, PhysicalKey, Propagation,
    ScrollDelta, TouchEvent,
};
pub use focus::FocusManager;
pub use geometry::{Constraints, Insets, Point, Rect, Size};
pub use grid::{
    compute_grid, GridItem, GridLine, GridPlacement, GridSpan, GridTemplate, TrackSizing,
};
pub use layout::{
    AlignContent, AlignItems, FlexDirection, FlexItem, FlexLayout, FlexWrap, JustifyContent,
};
pub use paint::{DrawCommand, DrawList, RenderBackend};
pub use reactive::{Computed, ReactiveError, ReactiveRuntime, Signal};
pub use response::{
    CheckboxResponse, DropdownResponse, SliderResponse, TextInputResponse, WidgetResponse,
};
pub use scheduler::{Debounce, Scheduler, Throttle, TimerId};
pub use solver::{Constraint, Expression, RelOp, Solver, SolverError, Strength, Term, Variable};
pub use style::{Border, BorderStyle, CursorShape, Margin, Padding};
pub use text_style::TextStyle;
pub use tree::{WidgetId, WidgetIdAllocator, WidgetNode, WidgetTree};
pub use widget_ext::{ClipboardProvider, DragData, DragSource, DropEffect, DropTarget, WidgetExt};

/// RGBA colour value, one `u8` per channel: `Color(r, g, b, a)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Color(pub u8, pub u8, pub u8, pub u8);

/// A palette of semantic colours for a UI theme.
#[derive(Clone, Debug)]
pub struct Palette {
    /// Window / page background colour.
    pub background: Color,
    /// Card / panel surface colour.
    pub surface: Color,
    /// Primary accent colour.
    pub primary: Color,
    /// Text drawn on top of the primary colour.
    pub on_primary: Color,
    /// Main body text colour.
    pub text: Color,
    /// De-emphasised / disabled text colour.
    pub muted: Color,
}

impl Palette {
    /// Construct a [`Palette`] with explicit colour values.
    pub fn new(
        background: Color,
        surface: Color,
        primary: Color,
        on_primary: Color,
        text: Color,
        muted: Color,
    ) -> Self {
        Self {
            background,
            surface,
            primary,
            on_primary,
            text,
            muted,
        }
    }
}

/// The slant style of a font face.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum FontStyle {
    /// Upright (no slant).
    #[default]
    Normal,
    /// True italic (a distinct, cursive face).
    Italic,
    /// Oblique — the upright face slanted by `degrees` (synthetic slant).
    Oblique {
        /// Slant angle in degrees (positive leans right).
        degrees: f32,
    },
}

/// An OpenType feature tag toggle, e.g. `"liga"` on, `"tnum"` on.
///
/// `tag` is the 4-byte OpenType feature tag; `value` is the feature selector
/// (0 = off, 1 = on, or a stylistic-set index).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FontFeature {
    /// The 4-character OpenType feature tag (e.g. `"liga"`, `"smcp"`, `"ss01"`).
    pub tag: String,
    /// The feature value (0 = off, 1 = on, or an index for alternates).
    pub value: u32,
}

impl FontFeature {
    /// Enable a feature (value `1`).
    pub fn on(tag: impl Into<String>) -> Self {
        Self {
            tag: tag.into(),
            value: 1,
        }
    }

    /// Disable a feature (value `0`).
    pub fn off(tag: impl Into<String>) -> Self {
        Self {
            tag: tag.into(),
            value: 0,
        }
    }

    /// A feature with an explicit selector value.
    pub fn value(tag: impl Into<String>, value: u32) -> Self {
        Self {
            tag: tag.into(),
            value,
        }
    }
}

/// Font specification for UI text.
///
/// The three legacy fields (`family`, `size`, `weight`) plus the
/// [`FontSpec::new`] constructor are unchanged. The richer typographic fields
/// (`style`, `letter_spacing`, `line_height`, `features`) are additive and
/// default to "no override", so existing call sites are unaffected.
#[derive(Clone, Debug, PartialEq)]
pub struct FontSpec {
    /// Font family name.
    pub family: String,
    /// Font size in points.
    pub size: f32,
    /// Font weight (100 thin … 900 black; 400 is regular).
    pub weight: u16,
    /// Slant style (normal / italic / oblique).
    pub style: FontStyle,
    /// Additional inter-character spacing in points (`0.0` = font default).
    pub letter_spacing: f32,
    /// Line height (leading) in points. `None` uses the font's natural metrics.
    pub line_height: Option<f32>,
    /// OpenType feature toggles applied to runs using this spec.
    pub features: Vec<FontFeature>,
}

impl FontSpec {
    /// Construct a [`FontSpec`] with explicit `family`/`size`/`weight`; the
    /// typographic extras default to "no override".
    pub fn new(family: impl Into<String>, size: f32, weight: u16) -> Self {
        Self {
            family: family.into(),
            size,
            weight,
            style: FontStyle::Normal,
            letter_spacing: 0.0,
            line_height: None,
            features: Vec::new(),
        }
    }

    /// Builder: set the slant [`FontStyle`].
    pub fn with_style(mut self, style: FontStyle) -> Self {
        self.style = style;
        self
    }

    /// Builder: set additional letter spacing in points.
    pub fn with_letter_spacing(mut self, letter_spacing: f32) -> Self {
        self.letter_spacing = letter_spacing;
        self
    }

    /// Builder: set the line height (leading) in points.
    pub fn with_line_height(mut self, line_height: f32) -> Self {
        self.line_height = Some(line_height);
        self
    }

    /// Builder: append an OpenType [`FontFeature`].
    pub fn with_feature(mut self, feature: FontFeature) -> Self {
        self.features.push(feature);
        self
    }

    /// Returns `true` if the face is italic or oblique.
    pub fn is_slanted(&self) -> bool {
        !matches!(self.style, FontStyle::Normal)
    }
}

impl Default for FontSpec {
    /// Returns Inter / 14 pt / regular (400), upright, no overrides.
    fn default() -> Self {
        Self::new("Inter", 14.0, 400)
    }
}

/// A styled text span for use with [`UiCtx::rich_text`].
///
/// Each span carries its own typographic style, allowing a single call to
/// `rich_text` to render mixed-style text (bold headings, coloured links, …).
#[derive(Clone, Debug)]
pub struct RichTextSpan {
    /// The text content of this span.
    pub text: String,
    /// Render the text in bold weight.
    pub bold: bool,
    /// Render the text in italic.
    pub italic: bool,
    /// RGBA colour bytes `[r, g, b, a]`.
    pub color: [u8; 4],
    /// Font size in logical pixels.
    pub font_size: f32,
    /// Optional font-family override; `None` uses the theme default.
    pub font_family: Option<String>,
}

impl RichTextSpan {
    /// Construct a span with default style (black, 16 px, upright).
    pub fn new(text: impl Into<String>) -> Self {
        RichTextSpan {
            text: text.into(),
            bold: false,
            italic: false,
            color: [0, 0, 0, 255],
            font_size: 16.0,
            font_family: None,
        }
    }

    /// Builder: enable bold weight.
    pub fn bold(mut self) -> Self {
        self.bold = true;
        self
    }

    /// Builder: enable italic.
    pub fn italic(mut self) -> Self {
        self.italic = true;
        self
    }

    /// Builder: set the RGBA colour.
    pub fn color(mut self, c: [u8; 4]) -> Self {
        self.color = c;
        self
    }

    /// Builder: set the font size in logical pixels.
    pub fn font_size(mut self, s: f32) -> Self {
        self.font_size = s;
        self
    }

    /// Builder: set an optional font-family override.
    pub fn font_family(mut self, family: impl Into<String>) -> Self {
        self.font_family = Some(family.into());
        self
    }
}

/// Response from a button widget.
#[derive(Clone, Debug, Default)]
pub struct ButtonResponse {
    /// Whether the button was clicked in this frame.
    pub clicked: bool,
    /// Whether the cursor is hovering over the button.
    pub hovered: bool,
}

/// Layout axis.
#[derive(Clone, Debug)]
pub enum Axis {
    /// Stack children from top to bottom.
    Vertical,
    /// Stack children from left to right.
    Horizontal,
}

/// Events that the UI backend can emit.
///
/// This enum is `#[non_exhaustive]` — match arms must include a catch-all
/// (`_ => {}`) to remain forward-compatible as new variants are added.
///
/// When deserialising with serde (`feature = "serde"`), unknown variants will
/// produce a serde error. This is intentional: the API contract only promises
/// forward-compatibility at the Rust source level via the `#[non_exhaustive]`
/// catch-all; JSON consumers must handle new variants themselves.
#[derive(Clone, Debug)]
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum UiEvent {
    /// The window was resized to the given pixel dimensions.
    Resize(u32, u32),
    /// The user requested the window to close.
    CloseRequested,
    /// A keyboard key was pressed (key name / character string).
    KeyPress(String),
    /// Mouse cursor position.
    Mouse {
        /// Horizontal position in logical pixels.
        x: f32,
        /// Vertical position in logical pixels.
        y: f32,
    },
    /// A mouse button was pressed at the given position.
    MouseDown {
        /// Which button was pressed.
        button: events::MouseButton,
        /// Horizontal position in logical pixels.
        x: f32,
        /// Vertical position in logical pixels.
        y: f32,
        /// Modifier keys held at the time of the press.
        modifiers: events::Modifiers,
    },
    /// A mouse button was released at the given position.
    MouseUp {
        /// Which button was released.
        button: events::MouseButton,
        /// Horizontal position in logical pixels.
        x: f32,
        /// Vertical position in logical pixels.
        y: f32,
        /// Modifier keys held at the time of the release.
        modifiers: events::Modifiers,
    },
    /// The mouse moved to a new position (no button-state change implied).
    MouseMove {
        /// Horizontal position in logical pixels.
        x: f32,
        /// Vertical position in logical pixels.
        y: f32,
    },
    /// A scroll-wheel / trackpad scroll occurred.
    Wheel(events::ScrollDelta),
    /// A key was pressed (or auto-repeated).
    KeyDown {
        /// The logical key.
        key: events::Key,
        /// Modifier keys held.
        modifiers: events::Modifiers,
        /// Whether this is an auto-repeat (key held down).
        repeat: bool,
    },
    /// A key was released.
    KeyUp {
        /// The logical key.
        key: events::Key,
        /// Modifier keys held.
        modifiers: events::Modifiers,
    },
    /// IME preedit — composition in progress.
    ///
    /// `text` is the current composition string. `cursor` is the byte-offset
    /// range `(start, end)` within `text` that should be highlighted as the
    /// cursor/selection; `None` means no explicit cursor hint.
    ///
    /// Note: on the egui forwarding path the cursor range is not forwarded
    /// (egui 0.34's `ImeEvent::Preedit` only accepts a `String`).
    ImePreedit {
        /// Composition string being entered.
        text: String,
        /// Optional byte-range cursor hint within `text`.
        cursor: Option<(usize, usize)>,
    },
    /// IME commit — final committed text after composition ends.
    ///
    /// Callers should insert `text` into the active text-input field.
    ImeCommit(String),
}

// ── Traits ─────────────────────────────────────────────────────────────────

/// Rendering context passed to every [`Widget::render`] call.
///
/// The three core methods ([`heading`](UiCtx::heading), [`label`](UiCtx::label),
/// [`button`](UiCtx::button)) are **required**: every adapter implements them.
///
/// The remaining widget methods are **provided with default implementations**
/// that return a `*Response` whose `supported` field is `false` (see
/// [`response`]). This is a deliberate design choice: an adapter that has not
/// overridden, say, [`slider`](UiCtx::slider) reports `supported == false` to
/// the caller rather than silently rendering nothing and pretending it worked.
/// Adapters override the subset of extended widgets they actually support; the
/// rest degrade visibly. Callers branch on the `supported` flag to fall back.
pub trait UiCtx {
    /// Render a heading-sized text string.
    fn heading(&mut self, text: &str);
    /// Render a body-text label.
    fn label(&mut self, text: &str);
    /// Render a button and return the interaction state.
    fn button(&mut self, label: &str) -> ButtonResponse;

    /// Render a single-line text-input field seeded with `text`.
    ///
    /// Default: unsupported (`supported = false`, empty text).
    fn text_input(&mut self, _text: &str) -> response::TextInputResponse {
        response::TextInputResponse::unsupported()
    }

    /// Render a checkbox labelled `label` in state `checked`.
    ///
    /// Default: unsupported (`supported = false`).
    fn checkbox(&mut self, _label: &str, _checked: bool) -> response::CheckboxResponse {
        response::CheckboxResponse::unsupported()
    }

    /// Render a slider over `range` at `value`.
    ///
    /// Default: unsupported (`supported = false`, value `0.0`).
    fn slider(
        &mut self,
        _value: f64,
        _range: core::ops::RangeInclusive<f64>,
    ) -> response::SliderResponse {
        response::SliderResponse::unsupported()
    }

    /// Render a dropdown of `options` with `selected` chosen.
    ///
    /// Default: unsupported (`supported = false`, selection `0`).
    fn dropdown(&mut self, _options: &[&str], _selected: usize) -> response::DropdownResponse {
        response::DropdownResponse::unsupported()
    }

    /// Render an image identified by `uri` at an optional `size`.
    ///
    /// Default: unsupported (`supported = false`).
    fn image(&mut self, _uri: &str, _size: Option<Size>) -> response::WidgetResponse {
        response::WidgetResponse::unsupported()
    }

    /// Render a separator (horizontal/vertical rule).
    ///
    /// Default: unsupported (`supported = false`).
    fn separator(&mut self) -> response::WidgetResponse {
        response::WidgetResponse::unsupported()
    }

    /// Render empty space of `size` logical pixels along the layout axis.
    ///
    /// Default: unsupported (`supported = false`).
    fn spacer(&mut self, _size: f32) -> response::WidgetResponse {
        response::WidgetResponse::unsupported()
    }

    /// Render `content` inside a scrollable region.
    ///
    /// Default: unsupported (`supported = false`); the closure is **not**
    /// invoked, so a caller can detect non-support before side effects run.
    fn scroll_area(
        &mut self,
        _content: &mut dyn FnMut(&mut dyn UiCtx),
    ) -> response::WidgetResponse {
        response::WidgetResponse::unsupported()
    }

    /// Attach a tooltip with `text` to the most recently rendered widget.
    ///
    /// Default: unsupported (`supported = false`).
    fn tooltip(&mut self, _text: &str) -> response::WidgetResponse {
        response::WidgetResponse::unsupported()
    }

    /// Render a popup containing `content`.
    ///
    /// Default: unsupported (`supported = false`); the closure is not invoked.
    fn popup(&mut self, _content: &mut dyn FnMut(&mut dyn UiCtx)) -> response::WidgetResponse {
        response::WidgetResponse::unsupported()
    }

    /// Render a modal dialog titled `title` containing `content`.
    ///
    /// Default: unsupported (`supported = false`); the closure is not invoked.
    fn modal(
        &mut self,
        _title: &str,
        _content: &mut dyn FnMut(&mut dyn UiCtx),
    ) -> response::WidgetResponse {
        response::WidgetResponse::unsupported()
    }

    /// Lay out `content` in a horizontal row.
    ///
    /// Default: unsupported; the closure is **not** invoked.
    fn horizontal(&mut self, _content: &mut dyn FnMut(&mut dyn UiCtx)) -> response::WidgetResponse {
        response::WidgetResponse::unsupported()
    }

    /// Lay out `content` in a vertical column.
    ///
    /// Default: unsupported; the closure is **not** invoked.
    fn vertical(&mut self, _content: &mut dyn FnMut(&mut dyn UiCtx)) -> response::WidgetResponse {
        response::WidgetResponse::unsupported()
    }

    /// Lay out `content` in a grid with `cols` columns.
    ///
    /// Default: unsupported; the closure is **not** invoked.
    fn grid(
        &mut self,
        _cols: usize,
        _content: &mut dyn FnMut(&mut dyn UiCtx),
    ) -> response::WidgetResponse {
        response::WidgetResponse::unsupported()
    }

    /// Render a menu bar containing `content`.
    ///
    /// Default: unsupported; the closure is **not** invoked.
    fn menu_bar(&mut self, _content: &mut dyn FnMut(&mut dyn UiCtx)) -> response::WidgetResponse {
        response::WidgetResponse::unsupported()
    }

    /// Render multi-styled text from a slice of [`RichTextSpan`]s.
    ///
    /// Default: unsupported (`supported = false`).
    fn rich_text(&mut self, _spans: &[RichTextSpan]) -> response::WidgetResponse {
        response::WidgetResponse::unsupported()
    }

    /// Render a body-text label with an explicit [`TextStyle`].
    ///
    /// The default implementation delegates to [`UiCtx::label`] (text is
    /// always rendered) and ignores `_style`. Adapters that can honour rich
    /// typography should override this method.
    ///
    /// Returns [`WidgetResponse::supported`] because `label` is a *required*
    /// method — the text is guaranteed to appear even if the style is ignored.
    fn label_styled(&mut self, text: &str, _style: TextStyle) -> response::WidgetResponse {
        self.label(text);
        response::WidgetResponse::supported()
    }

    /// Render a heading with an explicit [`TextStyle`].
    ///
    /// The default implementation delegates to [`UiCtx::heading`] (text is
    /// always rendered) and ignores `_style`. Adapters that can honour rich
    /// typography should override this method.
    ///
    /// Returns [`WidgetResponse::supported`] because `heading` is a *required*
    /// method — the text is guaranteed to appear even if the style is ignored.
    fn heading_styled(&mut self, text: &str, _style: TextStyle) -> response::WidgetResponse {
        self.heading(text);
        response::WidgetResponse::supported()
    }

    /// Mark `content` as a drag source with the given `id`.
    ///
    /// Default: unsupported; the closure is **not** invoked.
    fn drag_source(
        &mut self,
        _id: u64,
        _content: &mut dyn FnMut(&mut dyn UiCtx),
    ) -> response::WidgetResponse {
        response::WidgetResponse::unsupported()
    }

    /// Mark `content` as a drop target that accepts drags with any of the given `accept_ids`.
    ///
    /// Default: unsupported; the closure is **not** invoked.
    fn drop_target(
        &mut self,
        _accept_ids: &[u64],
        _content: &mut dyn FnMut(&mut dyn UiCtx),
    ) -> response::WidgetResponse {
        response::WidgetResponse::unsupported()
    }
}

/// A UI widget that can render itself into a [`UiCtx`].
pub trait Widget {
    /// Render the widget into `ui`.
    fn render(&mut self, ui: &mut dyn UiCtx);
}

/// A UI theme that provides a colour palette and font specification.
pub trait Theme: Send + Sync {
    /// Return the colour palette for this theme.
    fn palette(&self) -> &Palette;
    /// Return the font specification for this theme.
    fn font(&self) -> &FontSpec;
}

/// A layout strategy that controls how children are arranged.
pub trait Layout: Send + Sync {
    /// Primary layout axis.
    fn axis(&self) -> Axis;
    /// Spacing between children in logical pixels.
    fn spacing(&self) -> f32;
}

/// An event sink that accepts UI events for processing.
pub trait EventSink {
    /// Push an event into the sink.
    fn push(&mut self, event: UiEvent);
}

// ── Error ───────────────────────────────────────────────────────────────────

/// Errors emitted by the OxiUI stack.
///
/// This enum is `#[non_exhaustive]`: downstream `match` expressions must include
/// a catch-all (`_ => …`) so new variants can be added without a breaking
/// change.
#[derive(Debug)]
#[non_exhaustive]
pub enum UiError {
    /// A backend (windowing / GPU initialisation) error.
    Backend(String),
    /// A render-pipeline error.
    Render(String),
    /// A window-management error.
    Window(String),
    /// The requested feature or backend is not available.
    Unsupported(String),
    /// A layout-engine error (e.g. an unsatisfiable constraint set).
    Layout(String),
    /// A focus-management error (e.g. focusing a non-focusable node).
    Focus(String),
    /// A clipboard access error (e.g. unsupported MIME type, OS denial).
    Clipboard(String),
    /// A drag-and-drop protocol error (e.g. rejected payload).
    DragDrop(String),
    /// Any other error not covered by the above variants.
    Other(String),
}

impl std::fmt::Display for UiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UiError::Backend(s) => write!(f, "UI backend error: {s}"),
            UiError::Render(s) => write!(f, "UI render error: {s}"),
            UiError::Window(s) => write!(f, "UI window error: {s}"),
            UiError::Unsupported(s) => write!(f, "UI unsupported: {s}"),
            UiError::Layout(s) => write!(f, "UI layout error: {s}"),
            UiError::Focus(s) => write!(f, "UI focus error: {s}"),
            UiError::Clipboard(s) => write!(f, "UI clipboard error: {s}"),
            UiError::DragDrop(s) => write!(f, "UI drag-and-drop error: {s}"),
            UiError::Other(s) => write!(f, "UI error: {s}"),
        }
    }
}

impl std::error::Error for UiError {}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ime_preedit_event_roundtrip() {
        let event = UiEvent::ImePreedit {
            text: "日本語".to_string(),
            cursor: Some((0, 9)),
        };
        match event {
            UiEvent::ImePreedit { text, cursor } => {
                assert_eq!(text, "日本語");
                assert!(cursor.is_some());
                let (start, end) = cursor.expect("cursor should be Some");
                assert_eq!(start, 0);
                assert_eq!(end, 9);
            }
            _ => panic!("expected ImePreedit variant"),
        }
    }

    #[test]
    fn ime_commit_event_roundtrip() {
        let event = UiEvent::ImeCommit("確定".to_string());
        match event {
            UiEvent::ImeCommit(text) => {
                assert_eq!(text, "確定");
            }
            _ => panic!("expected ImeCommit variant"),
        }
    }

    #[test]
    fn ime_preedit_no_cursor() {
        let event = UiEvent::ImePreedit {
            text: "abc".to_string(),
            cursor: None,
        };
        match event {
            UiEvent::ImePreedit { text, cursor } => {
                assert_eq!(text, "abc");
                assert!(cursor.is_none());
            }
            _ => panic!("expected ImePreedit variant"),
        }
    }

    #[test]
    fn font_spec_expansion_defaults_and_builders() {
        // Legacy constructor still yields upright/no-override extras.
        let base = FontSpec::new("Inter", 16.0, 500);
        assert_eq!(base.style, FontStyle::Normal);
        assert_eq!(base.letter_spacing, 0.0);
        assert!(base.line_height.is_none());
        assert!(base.features.is_empty());
        assert!(!base.is_slanted());

        // Builders are additive and chainable.
        let rich = FontSpec::new("Inter", 16.0, 500)
            .with_style(FontStyle::Italic)
            .with_letter_spacing(0.5)
            .with_line_height(20.0)
            .with_feature(FontFeature::on("liga"))
            .with_feature(FontFeature::value("ss01", 1));
        assert!(rich.is_slanted());
        assert_eq!(rich.letter_spacing, 0.5);
        assert_eq!(rich.line_height, Some(20.0));
        assert_eq!(rich.features.len(), 2);
        assert_eq!(rich.features[0], FontFeature::on("liga"));

        // Oblique carries its slant angle.
        let obl = FontSpec::default().with_style(FontStyle::Oblique { degrees: 12.0 });
        assert!(
            matches!(obl.style, FontStyle::Oblique { degrees } if (degrees - 12.0).abs() < 1e-6)
        );
    }

    #[test]
    fn extended_uictx_defaults_report_unsupported() {
        // A minimal adapter that overrides only the required methods must see
        // every extended widget report supported == false by default.
        struct BareCtx;
        impl UiCtx for BareCtx {
            fn heading(&mut self, _t: &str) {}
            fn label(&mut self, _t: &str) {}
            fn button(&mut self, _l: &str) -> ButtonResponse {
                ButtonResponse::default()
            }
        }
        let mut ui = BareCtx;
        assert!(!ui.text_input("x").supported);
        assert!(!ui.checkbox("c", true).supported);
        assert!(!ui.slider(0.5, 0.0..=1.0).supported);
        assert!(!ui.dropdown(&["a", "b"], 0).supported);
        assert!(!ui.image("u", None).supported);
        assert!(!ui.separator().supported);
        assert!(!ui.spacer(8.0).supported);
        assert!(!ui.tooltip("t").supported);
        // Container defaults must NOT invoke their content closure.
        let mut invoked = false;
        let r = ui.scroll_area(&mut |_inner| invoked = true);
        assert!(!r.supported);
        assert!(!invoked, "unsupported scroll_area must not run content");
        let mut popup_invoked = false;
        assert!(!ui.popup(&mut |_| popup_invoked = true).supported);
        assert!(!popup_invoked);
        let mut modal_invoked = false;
        assert!(!ui.modal("title", &mut |_| modal_invoked = true).supported);
        assert!(!modal_invoked);
    }

    #[test]
    fn ui_error_new_variants_display() {
        assert!(UiError::Layout("x".into()).to_string().contains("layout"));
        assert!(UiError::Focus("x".into()).to_string().contains("focus"));
        assert!(UiError::Clipboard("x".into())
            .to_string()
            .contains("clipboard"));
        assert!(UiError::DragDrop("x".into()).to_string().contains("drag"));
    }

    #[test]
    fn uictx_extension_defaults_unsupported() {
        struct Bare;
        impl UiCtx for Bare {
            fn heading(&mut self, _: &str) {}
            fn label(&mut self, _: &str) {}
            fn button(&mut self, _: &str) -> ButtonResponse {
                ButtonResponse::default()
            }
        }
        let mut b = Bare;
        assert!(!b.horizontal(&mut |_| {}).supported);
        assert!(!b.vertical(&mut |_| {}).supported);
        assert!(!b.grid(2, &mut |_| {}).supported);
        assert!(!b.menu_bar(&mut |_| {}).supported);
        assert!(!b.rich_text(&[]).supported);
        assert!(!b.drag_source(1, &mut |_| {}).supported);
        assert!(!b.drop_target(&[], &mut |_| {}).supported);
    }

    #[test]
    fn rich_text_span_builder() {
        let span = RichTextSpan::new("Hello")
            .bold()
            .italic()
            .color([255, 0, 0, 255])
            .font_size(24.0);
        assert!(span.bold);
        assert!(span.italic);
        assert_eq!(span.color, [255, 0, 0, 255]);
        assert_eq!(span.font_size, 24.0);
        assert_eq!(span.text, "Hello");
    }
}
