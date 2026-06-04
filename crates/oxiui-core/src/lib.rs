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
pub mod window;

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
    layout_subtrees_parallel, AlignContent, AlignItems, FlexDirection, FlexItem, FlexLayout,
    FlexWrap, JustifyContent, LayoutTask,
};
pub use paint::{BlendMode, DrawCommand, DrawList, RenderBackend};
pub use reactive::{Computed, ReactiveError, ReactiveRuntime, Signal};
pub use response::{
    CheckboxResponse, DropdownResponse, SliderResponse, TextAreaResponse, TextInputResponse,
    WidgetResponse,
};
pub use scheduler::{Debounce, Scheduler, Throttle, TimerId};
pub use solver::{Constraint, Expression, RelOp, Solver, SolverError, Strength, Term, Variable};
pub use style::{Border, BorderStyle, CursorShape, Margin, Padding};
pub use text_style::TextStyle;
pub use tree::{WidgetId, WidgetIdAllocator, WidgetNode, WidgetTree};
pub use widget_ext::{ClipboardProvider, DragData, DragSource, DropEffect, DropTarget, WidgetExt};
pub use window::{WindowChannel, WindowConfig, WindowEvent, WindowId, WindowManager};

/// RGBA colour value, one `u8` per channel: `Color(r, g, b, a)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, oxicode::Encode, oxicode::Decode)]
pub struct Color(pub u8, pub u8, pub u8, pub u8);

/// A palette of semantic colours for a UI theme.
#[derive(Clone, Debug, PartialEq, oxicode::Encode, oxicode::Decode)]
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

impl Default for Palette {
    /// Returns a neutral light-mode palette (white background, indigo-500 accent).
    fn default() -> Self {
        Self {
            background: Color(255, 255, 255, 255),
            surface: Color(245, 245, 245, 255),
            primary: Color(99, 102, 241, 255),
            on_primary: Color(255, 255, 255, 255),
            text: Color(15, 23, 42, 255),
            muted: Color(100, 116, 139, 255),
        }
    }
}

/// The slant style of a font face.
#[derive(Clone, Copy, Debug, Default, PartialEq, oxicode::Encode, oxicode::Decode)]
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
#[derive(Clone, Debug, PartialEq, Eq, Hash, oxicode::Encode, oxicode::Decode)]
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
#[derive(Clone, Debug, PartialEq, oxicode::Encode, oxicode::Decode)]
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

    /// Render a multi-line text-area seeded with `text`.
    ///
    /// `min_rows` is a hint for the minimum number of visible lines; backends
    /// that do not support multi-line editing fall back to this default
    /// implementation which returns `supported = false`.
    ///
    /// Default: unsupported (`supported = false`, empty text, cursor at (0,0)).
    fn text_area(&mut self, _text: &str, _min_rows: usize) -> response::TextAreaResponse {
        response::TextAreaResponse::unsupported()
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

/// Semantic accessibility role for a widget.
///
/// Used by [`Widget::a11y_role`] to describe a widget's function to
/// assistive technologies. This is a core-level lightweight enum that the
/// `oxiui-accessibility` crate maps to the full `accesskit::Role` set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum A11yRole {
    /// A generic, unlabelled group of widgets.
    Group,
    /// A static text label (non-interactive).
    StaticText,
    /// An interactive button.
    Button,
    /// A heading / section title.
    Heading,
    /// A single-line text-input field.
    TextInput,
    /// A multi-line text-input area.
    TextArea,
    /// A checkbox or toggle control.
    Checkbox,
    /// A slider / range control.
    Slider,
    /// A progress bar.
    ProgressBar,
    /// A tab panel.
    TabPanel,
    /// A tab control.
    Tab,
    /// A scrollable list.
    List,
    /// A single item within a list.
    ListItem,
    /// A table widget.
    Table,
    /// A row within a table.
    TableRow,
    /// A cell within a table row.
    TableCell,
    /// A column header within a table.
    ColumnHeader,
    /// A dialog / modal overlay.
    Dialog,
    /// An image.
    Image,
    /// A hyperlink.
    Link,
    /// A menu widget.
    Menu,
    /// An item within a menu.
    MenuItem,
    /// An alert / status message.
    Alert,
    /// A tooltip.
    Tooltip,
    /// A tree widget.
    Tree,
    /// An item within a tree.
    TreeItem,
    /// Unknown / unspecified role.
    #[default]
    Unknown,
}

impl std::fmt::Display for A11yRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            A11yRole::Group => "group",
            A11yRole::StaticText => "statictext",
            A11yRole::Button => "button",
            A11yRole::Heading => "heading",
            A11yRole::TextInput => "textinput",
            A11yRole::TextArea => "textarea",
            A11yRole::Checkbox => "checkbox",
            A11yRole::Slider => "slider",
            A11yRole::ProgressBar => "progressbar",
            A11yRole::TabPanel => "tabpanel",
            A11yRole::Tab => "tab",
            A11yRole::List => "list",
            A11yRole::ListItem => "listitem",
            A11yRole::Table => "table",
            A11yRole::TableRow => "row",
            A11yRole::TableCell => "cell",
            A11yRole::ColumnHeader => "columnheader",
            A11yRole::Dialog => "dialog",
            A11yRole::Image => "img",
            A11yRole::Link => "link",
            A11yRole::Menu => "menu",
            A11yRole::MenuItem => "menuitem",
            A11yRole::Alert => "alert",
            A11yRole::Tooltip => "tooltip",
            A11yRole::Tree => "tree",
            A11yRole::TreeItem => "treeitem",
            A11yRole::Unknown => "unknown",
        };
        write!(f, "{s}")
    }
}

/// A UI widget that can render itself into a [`UiCtx`].
pub trait Widget {
    /// Render the widget into `ui`.
    fn render(&mut self, ui: &mut dyn UiCtx);

    /// Return the accessibility role for this widget.
    ///
    /// Adapters call this to populate the a11y tree without requiring a full
    /// `oxiui-accessibility` dependency in core. The default returns
    /// [`A11yRole::Unknown`].
    fn a11y_role(&self) -> A11yRole {
        A11yRole::Unknown
    }

    /// Return a human-readable accessibility label for this widget.
    ///
    /// Used as the accessible name. Returns `None` by default (no label).
    fn a11y_label(&self) -> Option<String> {
        None
    }

    /// Return an accessibility description (longer hint text) for this widget.
    ///
    /// Returns `None` by default.
    fn a11y_description(&self) -> Option<String> {
        None
    }
}

/// Spacing design tokens returned by [`Theme::spacing_tokens`].
///
/// Provides semantic spacing values that widgets and layout engines use instead
/// of magic numbers. The values are in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpacingTokens {
    /// Extra-small spacing (e.g. icon padding). Default: 4 px.
    pub xs: f32,
    /// Small spacing (e.g. button inner padding). Default: 8 px.
    pub sm: f32,
    /// Medium spacing (e.g. form field gap). Default: 12 px.
    pub md: f32,
    /// Large spacing (e.g. section gap). Default: 16 px.
    pub lg: f32,
    /// Extra-large spacing (e.g. page margin). Default: 24 px.
    pub xl: f32,
}

impl Default for SpacingTokens {
    /// COOLJAPAN 4-px-based default scale.
    fn default() -> Self {
        Self {
            xs: 4.0,
            sm: 8.0,
            md: 12.0,
            lg: 16.0,
            xl: 24.0,
        }
    }
}

/// Border design tokens returned by [`Theme::border_tokens`].
///
/// Semantic border widths, radii, and style used across the UI.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BorderTokens {
    /// Default border width in logical pixels (e.g. 1 px).
    pub width: f32,
    /// Emphasis border width (e.g. focused outline, 2 px).
    pub width_emphasis: f32,
    /// Small border radius (e.g. tags, 2 px).
    pub radius_sm: f32,
    /// Medium border radius (e.g. cards, 4 px).
    pub radius_md: f32,
    /// Large border radius (e.g. dialogs, 8 px).
    pub radius_lg: f32,
    /// Fully rounded radius (pills, 9999 px).
    pub radius_full: f32,
}

impl Default for BorderTokens {
    /// COOLJAPAN conventional defaults.
    fn default() -> Self {
        Self {
            width: 1.0,
            width_emphasis: 2.0,
            radius_sm: 2.0,
            radius_md: 4.0,
            radius_lg: 8.0,
            radius_full: 9999.0,
        }
    }
}

/// Padding design tokens returned by [`Theme::padding_tokens`].
///
/// Semantic padding presets for common widget types.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaddingTokens {
    /// Padding for compact / icon-only controls (e.g. icon button).
    pub compact: style::Padding,
    /// Padding for standard interactive controls (e.g. button, input).
    pub control: style::Padding,
    /// Padding for card / panel containers.
    pub card: style::Padding,
    /// Padding for page / dialog content areas.
    pub page: style::Padding,
}

impl Default for PaddingTokens {
    /// COOLJAPAN semantic defaults derived from the spacing scale.
    fn default() -> Self {
        Self {
            compact: style::Padding::symmetric(4.0, 6.0),
            control: style::Padding::symmetric(6.0, 12.0),
            card: style::Padding::all(16.0),
            page: style::Padding::all(24.0),
        }
    }
}

/// A UI theme that provides a colour palette, font specification, and design tokens.
///
/// The three required methods ([`palette`](Theme::palette), [`font`](Theme::font),
/// and the standard base) are mandatory. The design-token methods
/// ([`spacing_tokens`](Theme::spacing_tokens),
/// [`border_tokens`](Theme::border_tokens),
/// [`padding_tokens`](Theme::padding_tokens)) have **default implementations**
/// that return COOLJAPAN's standard scales so existing theme implementations
/// continue to compile unchanged. Themes that define a custom token set should
/// override them.
pub trait Theme: Send + Sync {
    /// Return the colour palette for this theme.
    fn palette(&self) -> &Palette;
    /// Return the font specification for this theme.
    fn font(&self) -> &FontSpec;

    /// Return the spacing design-token scale for this theme.
    ///
    /// Default: [`SpacingTokens::default`] (4-px-based COOLJAPAN scale).
    fn spacing_tokens(&self) -> SpacingTokens {
        SpacingTokens::default()
    }

    /// Return the border design tokens (widths and radii) for this theme.
    ///
    /// Default: [`BorderTokens::default`] (conventional 1 px / 4 px radius).
    fn border_tokens(&self) -> BorderTokens {
        BorderTokens::default()
    }

    /// Return the padding design token presets for this theme.
    ///
    /// Default: [`PaddingTokens::default`] (derived from COOLJAPAN spacing).
    fn padding_tokens(&self) -> PaddingTokens {
        PaddingTokens::default()
    }
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

// ── Macros ──────────────────────────────────────────────────────────────────

/// Bind a GPU context from `$expr`, skipping the test when no GPU is available.
///
/// If the environment variable `OXIUI_GPU_TESTS` is set to `"1"`, a missing GPU
/// causes a panic (fail-loud mode for dedicated GPU CI runners). Otherwise the
/// test function returns early with a printed skip notice.
///
/// # Example
/// ```ignore
/// require_gpu!(ctx, ComputeContext::try_new());
/// // ctx: ComputeContext is bound here
/// ```
#[macro_export]
macro_rules! require_gpu {
    ($ctx:ident, $expr:expr) => {
        let $ctx = match $expr {
            Some(c) => c,
            None => {
                if ::std::env::var("OXIUI_GPU_TESTS").as_deref() == Ok("1") {
                    panic!("OXIUI_GPU_TESTS=1 but no GPU adapter is available");
                }
                eprintln!("[skip] no GPU adapter — test skipped");
                return;
            }
        };
    };
}

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
        assert!(!ui.text_area("x", 3).supported);
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

    // ── Theme design-token tests ─────────────────────────────────────────────

    /// A minimal theme that only overrides the required methods.
    struct MinimalTheme {
        palette: Palette,
        font: FontSpec,
    }

    impl Theme for MinimalTheme {
        fn palette(&self) -> &Palette {
            &self.palette
        }
        fn font(&self) -> &FontSpec {
            &self.font
        }
    }

    #[test]
    fn theme_default_spacing_tokens() {
        let t = MinimalTheme {
            palette: Palette::default(),
            font: FontSpec::default(),
        };
        let s = t.spacing_tokens();
        // COOLJAPAN 4-px-based scale.
        assert!((s.xs - 4.0).abs() < 1e-6);
        assert!((s.sm - 8.0).abs() < 1e-6);
        assert!((s.md - 12.0).abs() < 1e-6);
        assert!((s.lg - 16.0).abs() < 1e-6);
        assert!((s.xl - 24.0).abs() < 1e-6);
    }

    #[test]
    fn theme_default_border_tokens() {
        let t = MinimalTheme {
            palette: Palette::default(),
            font: FontSpec::default(),
        };
        let b = t.border_tokens();
        assert!((b.width - 1.0).abs() < 1e-6);
        assert!((b.width_emphasis - 2.0).abs() < 1e-6);
        assert!((b.radius_sm - 2.0).abs() < 1e-6);
        assert!((b.radius_md - 4.0).abs() < 1e-6);
        assert!((b.radius_lg - 8.0).abs() < 1e-6);
        assert!((b.radius_full - 9999.0).abs() < 1.0);
    }

    #[test]
    fn theme_default_padding_tokens() {
        let t = MinimalTheme {
            palette: Palette::default(),
            font: FontSpec::default(),
        };
        let p = t.padding_tokens();
        // compact: symmetric(4,6)  → top=bottom=4, left=right=6
        assert!((p.compact.0.top - 4.0).abs() < 1e-6);
        assert!((p.compact.0.right - 6.0).abs() < 1e-6);
        // control: symmetric(6,12) → top=bottom=6, left=right=12
        assert!((p.control.0.top - 6.0).abs() < 1e-6);
        assert!((p.control.0.right - 12.0).abs() < 1e-6);
        // card: all(16)
        assert!((p.card.0.top - 16.0).abs() < 1e-6);
        assert!((p.card.0.left - 16.0).abs() < 1e-6);
        // page: all(24)
        assert!((p.page.0.top - 24.0).abs() < 1e-6);
    }

    /// A custom theme that overrides the design-token methods.
    struct CustomTheme {
        palette: Palette,
        font: FontSpec,
    }

    impl Theme for CustomTheme {
        fn palette(&self) -> &Palette {
            &self.palette
        }
        fn font(&self) -> &FontSpec {
            &self.font
        }
        fn spacing_tokens(&self) -> SpacingTokens {
            SpacingTokens {
                xs: 2.0,
                sm: 4.0,
                md: 8.0,
                lg: 12.0,
                xl: 16.0,
            }
        }
    }

    #[test]
    fn theme_custom_spacing_overrides_default() {
        let t = CustomTheme {
            palette: Palette::default(),
            font: FontSpec::default(),
        };
        let s = t.spacing_tokens();
        assert!((s.xs - 2.0).abs() < 1e-6, "custom xs must be 2");
        assert!((s.sm - 4.0).abs() < 1e-6, "custom sm must be 4");
        // Border and padding still return defaults.
        let b = t.border_tokens();
        assert!((b.width - 1.0).abs() < 1e-6, "border default width still 1");
    }
}

#[cfg(test)]
mod macro_tests {
    #[test]
    fn require_gpu_binds_some() {
        require_gpu!(val, Some(42u32));
        assert_eq!(val, 42);
    }

    #[test]
    fn require_gpu_skips_on_none() {
        require_gpu!(_val, None::<u32>);
        // If we reach here, env is not OXIUI_GPU_TESTS=1 — that's the non-skip path.
        // The macro either returned early (skip) or panicked. Either way test passes if we get here.
    }
}
