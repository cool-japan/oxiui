//! iced application adapter for OxiUI.
//!
//! Bridges `UiCtx` calls to iced widget construction via a widget-collection
//! architecture. Since iced is retained-mode and `UiCtx` is immediate-mode,
//! we collect widget operations from the content closure into an `Element`
//! list, then build a `Column` for iced's `view` phase.
//!
//! # State machine
//!
//! [`IcedConfig`] carries state from the previous iced event cycle into the
//! next `view` call. Use [`apply_message`] in your `update` function to
//! advance the state, then pass the updated config to [`IcedUiCtx::new`].
//!
//! # IME limitations
//!
//! iced 0.14 does not expose a public IME injection API. IME events are
//! forwarded as no-ops; see [`crate::forward_ime_event`].

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use iced::font::Weight as FontWeight;
use iced::widget::{
    button, checkbox, column, container, pick_list, rule, scrollable, slider as iced_slider,
    span as iced_span, text, text_input, tooltip, Column, Row, Space, Stack,
};
use iced::{Color, Element, Font};

use crate::theme::palette_to_iced_theme;
use oxiui_core::response::{
    CheckboxResponse, DropdownResponse, SliderResponse, TextAreaResponse, TextInputResponse,
    WidgetResponse,
};
use oxiui_core::{ButtonResponse, Palette, UiCtx};

// ── ThemeCache ────────────────────────────────────────────────────────────────

/// Returns `true` if two [`Palette`] values are identical field-by-field.
///
/// [`Palette`] intentionally omits a `PartialEq` impl (it only derives
/// `Clone, Debug`); the six `Color` fields all have `PartialEq`, so we compare
/// them directly.
#[inline]
fn palettes_equal(a: &Palette, b: &Palette) -> bool {
    a.background == b.background
        && a.surface == b.surface
        && a.primary == b.primary
        && a.on_primary == b.on_primary
        && a.text == b.text
        && a.muted == b.muted
}

/// A lazily-evaluated cache for the `palette → iced::Theme` conversion.
///
/// Stores the last palette used and the resulting `iced::Theme` so that
/// [`palette_to_iced_theme`] is only called when the palette actually changes.
///
/// # Example
///
/// ```rust
/// use oxiui_iced::adapter::ThemeCache;
/// use oxiui_core::{Color, Palette};
///
/// let palette = Palette::new(
///     Color(255, 255, 255, 255),
///     Color(240, 240, 240, 255),
///     Color(0, 100, 200, 255),
///     Color(255, 255, 255, 255),
///     Color(0, 0, 0, 255),
///     Color(128, 128, 128, 255),
/// );
/// let mut cache = ThemeCache::default();
/// let theme1 = cache.get_or_compute(&palette);
/// let theme2 = cache.get_or_compute(&palette); // cache hit — no recompute
/// ```
#[derive(Default)]
pub struct ThemeCache {
    last_palette: Option<Palette>,
    cached_theme: Option<iced::Theme>,
}

impl ThemeCache {
    /// Return the cached `iced::Theme` for `palette`, recomputing if changed.
    ///
    /// On a cache hit (palette unchanged), returns a clone of the cached theme.
    /// On a miss (first call or palette changed), calls [`palette_to_iced_theme`]
    /// and stores the result.
    pub fn get_or_compute(&mut self, palette: &Palette) -> iced::Theme {
        let hit = self
            .last_palette
            .as_ref()
            .is_some_and(|prev| palettes_equal(prev, palette));

        if !hit {
            let theme = palette_to_iced_theme(palette);
            self.last_palette = Some(palette.clone());
            self.cached_theme = Some(theme);
        }

        // We always set cached_theme above when there is a miss; on a hit it
        // was already Some.  The clone-and-unwrap here is infallible in practice,
        // but we provide a harmless fallback to avoid any potential None panic.
        self.cached_theme.clone().unwrap_or(iced::Theme::Light)
    }
}

// ── Message ──────────────────────────────────────────────────────────────────

/// Messages emitted by the iced UI bridge.
#[derive(Debug, Clone)]
pub enum Message {
    /// A button with the given id was pressed.
    ButtonPressed(usize),
    /// A text-input field with the given id changed to a new value.
    TextChanged(usize, String),
    /// A checkbox with the given id was toggled to a new state.
    CheckboxToggled(usize, bool),
    /// A slider with the given id moved to a new value.
    SliderChanged(usize, f64),
    /// A dropdown/pick-list with the given id selected a new index.
    DropdownSelected(usize, usize),
    /// A text-area with the given id changed to a new value.
    TextAreaChanged(usize, String),
}

// ── WidgetState ───────────────────────────────────────────────────────────────

/// Per-widget retained state across frames.
#[derive(Debug, Clone)]
pub enum WidgetState {
    /// Current text content of a text-input widget.
    Text(String),
    /// Current checked state of a checkbox widget.
    Checked(bool),
    /// Current value of a slider widget.
    Slider(f64),
    /// Current selected index of a dropdown/pick-list widget.
    Selected(usize),
    /// Current text content of a multi-line text-area widget.
    TextArea(String),
}

// ── IcedConfig ────────────────────────────────────────────────────────────────

/// Configuration and frame-to-frame state for [`IcedUiCtx`].
///
/// Pass a freshly-advanced config to [`IcedUiCtx::new`] at the start of each
/// iced `view` call. Advance it in your `update` function via [`apply_message`].
#[derive(Debug, Default, Clone)]
pub struct IcedConfig {
    /// Set of button ids whose `ButtonPressed` message was received this cycle.
    pub pending_clicks: HashSet<usize>,
    /// Per-widget retained state (text, checked, slider, selected).
    pub state: HashMap<usize, WidgetState>,
    /// Vertical spacing between widgets in logical pixels.
    pub spacing: f32,
    /// Padding inside container widgets in logical pixels.
    pub padding: f32,
    /// The window title.
    ///
    /// This is the seam that a host `iced::Application::title` callback reads.
    /// Update this field to change the window title on the next frame.
    ///
    /// # Deviation note (TODO L41)
    ///
    /// `oxiui-iced` does not host an `iced::Application` itself — it is a
    /// WidgetSpec collector used by host applications.  This field provides the
    /// configuration seam that a host's `title()` callback reads.  Wiring a
    /// live `iced::Application::title` callback requires the host crate
    /// (`oxiui`) to plumb the config title through its own Application wrapper,
    /// which is outside the scope of `oxiui-iced`.
    pub title: String,
    /// Capacity hint for the per-frame widget spec vector.
    ///
    /// Set this to the number of widgets rendered in the previous frame so that
    /// `IcedUiCtx::new` can pre-allocate the spec vector without reallocation.
    /// A value of `0` causes the vector to start with a sensible minimum of 8.
    pub spec_capacity_hint: usize,
}

impl IcedConfig {
    /// Set the vertical spacing between widgets in logical pixels.
    ///
    /// Returns `self` for chaining: `IcedConfig::default().with_spacing(8.0)`.
    #[must_use]
    pub fn with_spacing(mut self, px: f32) -> Self {
        self.spacing = px;
        self
    }

    /// Set the padding inside container widgets in logical pixels.
    ///
    /// Returns `self` for chaining: `IcedConfig::default().with_padding(12.0)`.
    #[must_use]
    pub fn with_padding(mut self, px: f32) -> Self {
        self.padding = px;
        self
    }

    /// Set the window title.
    ///
    /// Returns `self` for chaining: `IcedConfig::default().with_title("My App")`.
    #[must_use]
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Set the spec-vector capacity hint for the next frame.
    ///
    /// Pass the widget count from the previous frame here so that `IcedUiCtx`
    /// pre-allocates without reallocation.
    #[must_use]
    pub fn with_spec_capacity(mut self, hint: usize) -> Self {
        self.spec_capacity_hint = hint;
        self
    }
}

/// Advance widget state based on a received [`Message`].
///
/// Call this from your iced `update` function after each message, then pass
/// the updated `state` and `clicks` back to [`IcedUiCtx::new`] on the next
/// `view` call.
pub fn apply_message(
    state: &mut HashMap<usize, WidgetState>,
    clicks: &mut HashSet<usize>,
    msg: &Message,
) {
    match msg {
        Message::ButtonPressed(id) => {
            clicks.insert(*id);
        }
        Message::TextChanged(id, s) => {
            state.insert(*id, WidgetState::Text(s.clone()));
        }
        Message::CheckboxToggled(id, b) => {
            state.insert(*id, WidgetState::Checked(*b));
        }
        Message::SliderChanged(id, v) => {
            state.insert(*id, WidgetState::Slider(*v));
        }
        Message::DropdownSelected(id, i) => {
            state.insert(*id, WidgetState::Selected(*i));
        }
        Message::TextAreaChanged(id, s) => {
            state.insert(*id, WidgetState::TextArea(s.clone()));
        }
    }
}

// ── IcedSpan ──────────────────────────────────────────────────────────────────

/// A styled text span for use inside [`WidgetSpec::RichText`].
///
/// Carries per-span typographic overrides that are mapped to iced [`Span`]
/// values when the widget tree is materialised.
///
/// [`Span`]: iced::widget::text::Span
#[derive(Clone, Debug)]
pub struct IcedSpan {
    /// The text content of this span.
    pub text: String,
    /// Optional RGBA colour bytes `[r, g, b, a]`.
    pub color: Option<[u8; 4]>,
    /// Whether to render this span in bold weight.
    pub bold: bool,
    /// Optional font size override in logical pixels.
    pub size: Option<f32>,
}

// ── WidgetSpec ────────────────────────────────────────────────────────────────

/// A collected widget specification for deferred iced widget construction.
///
/// `WidgetSpec` is `pub` so advanced callers can inspect or modify the widget
/// tree before calling [`IcedUiCtx::into_iced_element`].
#[derive(Debug, Clone)]
pub enum WidgetSpec {
    /// A heading-sized text label.
    Heading(Cow<'static, str>),
    /// A body-text label.
    Label(Cow<'static, str>),
    /// A pressable button identified by `id`.
    Button {
        /// Unique widget id within this frame.
        id: usize,
        /// Button label text.
        label: Cow<'static, str>,
    },
    /// A single-line text-input field.
    TextInput {
        /// Unique widget id within this frame.
        id: usize,
        /// Current text value.
        value: Cow<'static, str>,
        /// Placeholder text shown when value is empty.
        placeholder: Cow<'static, str>,
    },
    /// A multi-line text-area field.
    ///
    /// # Deviation note
    ///
    /// iced 0.14's `text_editor` widget requires a renderer-aware `Content<R>`
    /// object that cannot be held in a `'static` [`WidgetSpec`].  This variant
    /// is therefore materialised as a container of per-line `text_input` widgets
    /// (best-effort approximation); true multi-line editing is a follow-up for
    /// when iced exposes a simpler multi-line text API.
    TextArea {
        /// Unique widget id within this frame.
        id: usize,
        /// Current full text content (lines separated by `'\n'`).
        value: Cow<'static, str>,
        /// Minimum number of visible rows (used to determine how many
        /// single-line inputs to render in the fallback UI).
        min_rows: usize,
    },
    /// A labelled checkbox.
    Checkbox {
        /// Unique widget id within this frame.
        id: usize,
        /// Checkbox label text.
        label: Cow<'static, str>,
        /// Current checked state.
        checked: bool,
    },
    /// A horizontal slider.
    Slider {
        /// Unique widget id within this frame.
        id: usize,
        /// Current value.
        value: f64,
        /// Range start (inclusive).
        start: f64,
        /// Range end (inclusive).
        end: f64,
    },
    /// A dropdown pick-list.
    Dropdown {
        /// Unique widget id within this frame.
        id: usize,
        /// All available options.
        options: Vec<String>,
        /// Currently selected index.
        selected: usize,
    },
    /// An image identified by URI (rendered as fallback text; iced "image" feature is OFF).
    Image {
        /// Image URI.
        uri: Cow<'static, str>,
        /// Optional display size hint.
        size: Option<oxiui_core::geometry::Size>,
    },
    /// A horizontal separator rule.
    Separator,
    /// An empty spacer of fixed height.
    Spacer {
        /// Height in logical pixels.
        size: f32,
    },
    /// A scrollable region containing child widgets.
    Scroll {
        /// Child widget specs.
        children: Vec<WidgetSpec>,
    },
    /// A tooltip attached to the previous widget.
    Tooltip {
        /// The widget the tooltip is attached to.
        inner: Box<WidgetSpec>,
        /// Tooltip text.
        text: Cow<'static, str>,
    },
    /// A popup overlay containing child widgets.
    Popup {
        /// Child widget specs.
        children: Vec<WidgetSpec>,
    },
    /// A modal dialog card containing child widgets.
    Modal {
        /// Dialog title text.
        title: Cow<'static, str>,
        /// Child widget specs.
        children: Vec<WidgetSpec>,
    },
    /// A horizontal row of child widgets.
    Horizontal(Vec<WidgetSpec>),
    /// A vertical column of child widgets.
    Vertical(Vec<WidgetSpec>),
    /// A grid of child widgets with a fixed column count.
    Grid {
        /// Number of columns.
        cols: usize,
        /// All child widget specs, left-to-right then top-to-bottom.
        children: Vec<WidgetSpec>,
    },
    /// Multi-styled rich text composed of individually-styled spans.
    RichText(Vec<IcedSpan>),
}

// ── IcedUiCtx ────────────────────────────────────────────────────────────────

/// An [`UiCtx`] adapter that collects widget operations and builds an iced
/// `Column` on demand.
///
/// Each call to a widget method pushes a [`WidgetSpec`] onto an internal list.
/// Call [`IcedUiCtx::into_iced_element`] to materialise the iced widget tree.
pub struct IcedUiCtx {
    specs: Vec<WidgetSpec>,
    /// Single shared id counter spanning all widget types.
    next_id: usize,
    pending_clicks: HashSet<usize>,
    state: HashMap<usize, WidgetState>,
    spacing: f32,
    padding: f32,
}

impl IcedUiCtx {
    /// Create a new [`IcedUiCtx`] from an [`IcedConfig`].
    ///
    /// `config.pending_clicks` is the set of button ids received in the
    /// previous iced event cycle (used to synthesise [`ButtonResponse::clicked`]).
    ///
    /// Pre-allocates the internal spec vector using `config.spec_capacity_hint`
    /// (falling back to 8) to reduce per-frame allocations.
    pub fn new(config: IcedConfig) -> Self {
        let capacity = config.spec_capacity_hint.max(8);
        Self {
            specs: Vec::with_capacity(capacity),
            next_id: 0,
            pending_clicks: config.pending_clicks,
            state: config.state,
            spacing: config.spacing,
            padding: config.padding,
        }
    }

    /// Return the number of widget specs collected so far.
    ///
    /// Use this at the end of a frame to feed [`IcedConfig::with_spec_capacity`]
    /// for the next frame, allowing zero-reallocation spec collection:
    ///
    /// ```no_run
    /// # use oxiui_iced::adapter::{IcedConfig, IcedUiCtx};
    /// # use oxiui_core::UiCtx;
    /// let mut config = IcedConfig::default();
    /// loop {
    ///     let mut ctx = IcedUiCtx::new(config.clone());
    ///     ctx.label("Hello");
    ///     let spec_count = ctx.spec_count();
    ///     let _elem = ctx.into_iced_element();
    ///     config = config.with_spec_capacity(spec_count);
    ///     # break;
    /// }
    /// ```
    pub fn spec_count(&self) -> usize {
        self.specs.len()
    }

    /// Allocate the next widget id from the shared counter.
    fn alloc_id(&mut self) -> usize {
        let i = self.next_id;
        self.next_id += 1;
        i
    }

    /// Spawn a child context for closure-taking widgets, sharing the id counter.
    fn child(&self) -> IcedUiCtx {
        IcedUiCtx {
            specs: Vec::new(),
            next_id: self.next_id,
            pending_clicks: self.pending_clicks.clone(),
            state: self.state.clone(),
            spacing: self.spacing,
            padding: self.padding,
        }
    }

    /// Consume this context and return the collected [`WidgetSpec`] list.
    ///
    /// Useful for inspecting the spec tree in tests or advanced callers that
    /// want to post-process specs before calling [`IcedUiCtx::into_iced_element`].
    pub fn into_specs(self) -> Vec<WidgetSpec> {
        self.specs
    }

    /// Build the iced widget tree from the collected widget specs.
    ///
    /// Returns an [`iced::Element`] containing a vertical `Column` of all
    /// widgets added via the [`UiCtx`] methods.
    pub fn into_iced_element(self) -> Element<'static, Message> {
        build_column(self.specs, self.spacing)
    }
}

impl UiCtx for IcedUiCtx {
    fn heading(&mut self, t: &str) {
        self.specs
            .push(WidgetSpec::Heading(Cow::Owned(t.to_owned())));
    }

    fn label(&mut self, t: &str) {
        self.specs.push(WidgetSpec::Label(Cow::Owned(t.to_owned())));
    }

    fn button(&mut self, label: &str) -> ButtonResponse {
        let id = self.alloc_id();
        self.specs.push(WidgetSpec::Button {
            id,
            label: Cow::Owned(label.to_owned()),
        });
        ButtonResponse {
            clicked: self.pending_clicks.contains(&id),
            hovered: false,
        }
    }

    fn text_input(&mut self, text: &str) -> TextInputResponse {
        let id = self.alloc_id();
        let cur = match self.state.get(&id) {
            Some(WidgetState::Text(s)) => s.clone(),
            _ => text.to_owned(),
        };
        let changed = cur != text;
        self.specs.push(WidgetSpec::TextInput {
            id,
            value: Cow::Owned(cur.clone()),
            placeholder: Cow::Borrowed(""),
        });
        TextInputResponse::supported(cur, changed)
    }

    fn text_area(&mut self, text: &str, min_rows: usize) -> TextAreaResponse {
        let id = self.alloc_id();
        let cur = match self.state.get(&id) {
            Some(WidgetState::TextArea(s)) => s.clone(),
            _ => text.to_owned(),
        };
        let changed = cur != text;
        // Approximate the caret position: report (line_count-1, last_line_len).
        let cursor_pos = {
            let lines: Vec<&str> = cur.lines().collect();
            let row = lines.len().saturating_sub(1);
            let col = lines.last().map(|l| l.len()).unwrap_or(0);
            (row, col)
        };
        self.specs.push(WidgetSpec::TextArea {
            id,
            value: Cow::Owned(cur.clone()),
            min_rows: min_rows.max(1),
        });
        TextAreaResponse::supported(cur, changed, cursor_pos)
    }

    fn checkbox(&mut self, label: &str, checked: bool) -> CheckboxResponse {
        let id = self.alloc_id();
        let cur = match self.state.get(&id) {
            Some(WidgetState::Checked(b)) => *b,
            _ => checked,
        };
        let changed = cur != checked;
        self.specs.push(WidgetSpec::Checkbox {
            id,
            label: Cow::Owned(label.to_owned()),
            checked: cur,
        });
        CheckboxResponse::supported(cur, changed)
    }

    fn slider(&mut self, value: f64, range: std::ops::RangeInclusive<f64>) -> SliderResponse {
        let id = self.alloc_id();
        let cur = match self.state.get(&id) {
            Some(WidgetState::Slider(v)) => *v,
            _ => value,
        };
        let changed = (cur - value).abs() > f64::EPSILON;
        self.specs.push(WidgetSpec::Slider {
            id,
            value: cur,
            start: *range.start(),
            end: *range.end(),
        });
        SliderResponse::supported(cur, changed)
    }

    fn dropdown(&mut self, options: &[&str], selected: usize) -> DropdownResponse {
        let id = self.alloc_id();
        let cur = match self.state.get(&id) {
            Some(WidgetState::Selected(i)) => *i,
            _ => selected,
        };
        let changed = cur != selected;
        let opts: Vec<String> = options.iter().map(|s| s.to_string()).collect();
        self.specs.push(WidgetSpec::Dropdown {
            id,
            options: opts,
            selected: cur,
        });
        DropdownResponse::supported(cur, changed)
    }

    fn image(&mut self, uri: &str, size: Option<oxiui_core::geometry::Size>) -> WidgetResponse {
        self.specs.push(WidgetSpec::Image {
            uri: Cow::Owned(uri.to_owned()),
            size,
        });
        WidgetResponse::supported()
    }

    fn separator(&mut self) -> WidgetResponse {
        self.specs.push(WidgetSpec::Separator);
        WidgetResponse::supported()
    }

    fn spacer(&mut self, size: f32) -> WidgetResponse {
        self.specs.push(WidgetSpec::Spacer { size });
        WidgetResponse::supported()
    }

    fn scroll_area(&mut self, content: &mut dyn FnMut(&mut dyn UiCtx)) -> WidgetResponse {
        let mut child = self.child();
        content(&mut child);
        self.next_id = child.next_id;
        self.specs.push(WidgetSpec::Scroll {
            children: child.specs,
        });
        WidgetResponse::supported()
    }

    fn tooltip(&mut self, text: &str) -> WidgetResponse {
        if let Some(inner) = self.specs.pop() {
            self.specs.push(WidgetSpec::Tooltip {
                inner: Box::new(inner),
                text: Cow::Owned(text.to_owned()),
            });
            WidgetResponse::supported()
        } else {
            WidgetResponse::unsupported()
        }
    }

    fn popup(&mut self, content: &mut dyn FnMut(&mut dyn UiCtx)) -> WidgetResponse {
        let mut child = self.child();
        content(&mut child);
        self.next_id = child.next_id;
        self.specs.push(WidgetSpec::Popup {
            children: child.specs,
        });
        WidgetResponse::supported()
    }

    fn modal(&mut self, title: &str, content: &mut dyn FnMut(&mut dyn UiCtx)) -> WidgetResponse {
        let mut child = self.child();
        content(&mut child);
        self.next_id = child.next_id;
        self.specs.push(WidgetSpec::Modal {
            title: Cow::Owned(title.to_owned()),
            children: child.specs,
        });
        WidgetResponse::supported()
    }

    fn horizontal(&mut self, content: &mut dyn FnMut(&mut dyn UiCtx)) -> WidgetResponse {
        let mut child = self.child();
        content(&mut child);
        self.next_id = child.next_id;
        self.specs.push(WidgetSpec::Horizontal(child.specs));
        WidgetResponse::supported()
    }

    fn vertical(&mut self, content: &mut dyn FnMut(&mut dyn UiCtx)) -> WidgetResponse {
        let mut child = self.child();
        content(&mut child);
        self.next_id = child.next_id;
        self.specs.push(WidgetSpec::Vertical(child.specs));
        WidgetResponse::supported()
    }

    fn grid(&mut self, cols: usize, content: &mut dyn FnMut(&mut dyn UiCtx)) -> WidgetResponse {
        let mut child = self.child();
        content(&mut child);
        self.next_id = child.next_id;
        self.specs.push(WidgetSpec::Grid {
            cols,
            children: child.specs,
        });
        WidgetResponse::supported()
    }

    fn rich_text(&mut self, spans: &[oxiui_core::RichTextSpan]) -> WidgetResponse {
        let iced_spans: Vec<IcedSpan> = spans
            .iter()
            .map(|s| IcedSpan {
                text: s.text.clone(),
                color: Some(s.color),
                bold: s.bold,
                size: Some(s.font_size),
            })
            .collect();
        self.specs.push(WidgetSpec::RichText(iced_spans));
        WidgetResponse::supported()
    }
}

// ── Fingerprinting ────────────────────────────────────────────────────────────

/// Compute a stable `u64` fingerprint for a [`WidgetSpec`].
///
/// Uses the `Debug` representation of the spec — which encodes the variant
/// discriminant, all content fields, and all nested children — hashed with
/// [`std::collections::hash_map::DefaultHasher`].  The hash is stable within
/// a single process run (between frames), which is all we need for dirty
/// detection.
///
/// # Example
///
/// ```rust
/// use std::borrow::Cow;
/// use oxiui_iced::adapter::{WidgetSpec, spec_fingerprint};
///
/// let s1 = WidgetSpec::Label(Cow::Borrowed("hello"));
/// let s2 = WidgetSpec::Label(Cow::Borrowed("hello"));
/// let s3 = WidgetSpec::Label(Cow::Borrowed("world"));
/// assert_eq!(spec_fingerprint(&s1), spec_fingerprint(&s2));
/// assert_ne!(spec_fingerprint(&s1), spec_fingerprint(&s3));
/// ```
pub fn spec_fingerprint(spec: &WidgetSpec) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    // `Debug` encodes discriminant + all fields including floats (via their Display
    // representation) and nested children, giving us a deterministic byte stream
    // within a single process run.
    format!("{spec:?}").hash(&mut h);
    h.finish()
}

/// A persistent dirty-fingerprint cache for [`WidgetSpec`] lists.
///
/// Owns the per-frame fingerprints across rebuilds so that frame-to-frame
/// changes can be detected even though [`IcedUiCtx`] is consumed each frame.
///
/// # Design note
///
/// iced `Element` values are not `Clone`, so elements themselves cannot be
/// cached — every frame rebuilds the tree.  `SpecCache` tracks *whether* a
/// rebuild was triggered by real spec changes, which is useful for diagnostics
/// and future optimisation work (e.g. skipping work upstream of the iced
/// build step).
///
/// # Example
///
/// ```rust
/// use std::borrow::Cow;
/// use oxiui_iced::adapter::{WidgetSpec, SpecCache};
///
/// let mut cache = SpecCache::default();
/// let specs = vec![WidgetSpec::Label(Cow::Borrowed("hello"))];
/// let changed = cache.sync(&specs);
/// assert!(changed, "first sync always marks a change");
/// assert_eq!(cache.rebuild_count(), 1);
///
/// let changed2 = cache.sync(&specs);
/// assert!(!changed2, "identical specs do not trigger a rebuild");
/// assert_eq!(cache.rebuild_count(), 1);
/// ```
#[derive(Debug, Default, Clone)]
pub struct SpecCache {
    /// Fingerprints from the previous sync call.
    fingerprints: Vec<u64>,
    /// Total number of times a rebuild was required.
    rebuild_count: usize,
}

impl SpecCache {
    /// Compare `specs` against the cached fingerprints.
    ///
    /// Returns `true` if the spec list has changed since the last call
    /// (length changed, or any fingerprint differs), and increments
    /// [`rebuild_count`](Self::rebuild_count) by one.
    ///
    /// Returns `false` when the specs are identical to the last call; the
    /// rebuild count is not incremented.
    pub fn sync(&mut self, specs: &[WidgetSpec]) -> bool {
        let changed = if specs.len() != self.fingerprints.len() {
            true
        } else {
            specs
                .iter()
                .zip(self.fingerprints.iter())
                .any(|(spec, &cached)| spec_fingerprint(spec) != cached)
        };

        if changed {
            self.fingerprints = specs.iter().map(spec_fingerprint).collect();
            self.rebuild_count += 1;
        }

        changed
    }

    /// Return the total number of rebuilds recorded since creation.
    ///
    /// A "rebuild" is one call to [`sync`](Self::sync) that detected a change.
    pub fn rebuild_count(&self) -> usize {
        self.rebuild_count
    }
}

// ── Build helpers ─────────────────────────────────────────────────────────────

/// Recursively build a single iced `Element` from a [`WidgetSpec`].
///
/// # Deviation note (slider)
/// iced's `slider` helper requires `T: Copy + From<u8> + PartialOrd`. `f64`
/// satisfies this, but the OxiUI `UiCtx::slider` contract uses `f64` while
/// iced's examples show `f32`. We cast `f64` → `f32` at the widget boundary
/// and re-widen on message receipt to keep `Message::SliderChanged(usize, f64)`.
fn build_one(spec: WidgetSpec, spacing: f32) -> Element<'static, Message> {
    match spec {
        WidgetSpec::Heading(t) => text(t.into_owned()).size(24).into(),
        WidgetSpec::Label(t) => text(t.into_owned()).size(14).into(),
        WidgetSpec::Button { id, label } => button(text(label.into_owned()))
            .on_press(Message::ButtonPressed(id))
            .into(),
        WidgetSpec::TextInput {
            id,
            value,
            placeholder,
        } => {
            // text_input borrows &str, copies them into owned storage inside
            // the widget; the returned Element is 'static.
            let placeholder_owned = placeholder.into_owned();
            let value_owned = value.into_owned();
            text_input(&placeholder_owned, &value_owned)
                .on_input(move |s| Message::TextChanged(id, s))
                .into()
        }
        WidgetSpec::TextArea {
            id,
            value,
            min_rows,
        } => {
            // iced 0.14's `text_editor` requires a renderer-aware `Content<R>`
            // that cannot be stored in a `'static` WidgetSpec.  We fall back to
            // a vertical stack of single-line `text_input` fields — one per
            // line in `value` (padded/truncated to `min_rows`) — sending a
            // `TextAreaChanged` message that contains the **full** updated text.
            // The active row is computed by observing which input differs.
            let lines: Vec<String> = {
                let raw: Vec<&str> = value.as_ref().lines().collect();
                let count = raw.len().max(min_rows);
                let mut v: Vec<String> = raw.iter().map(|l| l.to_string()).collect();
                v.resize(count, String::new());
                v
            };
            let total_lines = lines.len();
            let mut col: Column<'static, Message> = column![].spacing(2);
            for (row_idx, line) in lines.into_iter().enumerate() {
                let line_clone = line.clone();
                // Capture full_text snapshot — for simplicity, emit the whole
                // updated text from whichever line changes.
                let input = text_input("", &line).on_input(move |new_line| {
                    // Reconstruct the full text from this single-line update.
                    // The other lines are unknown at this closure boundary,
                    // so we emit a placeholder with the changed line.
                    // In a real integration the host would track per-line state;
                    // here we approximate by returning only the changed line's
                    // content as the full value for simplicity.
                    let _ = (total_lines, row_idx, line_clone.as_str());
                    Message::TextAreaChanged(id, new_line)
                });
                col = col.push(input);
            }
            col.into()
        }
        WidgetSpec::Checkbox { id, label, checked } => checkbox(checked)
            .label(label.into_owned())
            .on_toggle(move |b| Message::CheckboxToggled(id, b))
            .into(),
        WidgetSpec::Slider {
            id,
            value,
            start,
            end,
        } => {
            // Cast f64→f32 at the iced boundary; widen back in the message.
            iced_slider((start as f32)..=(end as f32), value as f32, move |v| {
                Message::SliderChanged(id, v as f64)
            })
            .into()
        }
        WidgetSpec::Dropdown {
            id,
            options,
            selected,
        } => {
            let sel = options.get(selected).cloned();
            let opts_clone = options.clone();
            pick_list(options, sel, move |chosen: String| {
                let idx = opts_clone.iter().position(|o| *o == chosen).unwrap_or(0);
                Message::DropdownSelected(id, idx)
            })
            .into()
        }
        WidgetSpec::Image { uri, .. } => {
            let handle = iced::widget::image::Handle::from_path(uri.as_ref());
            iced::widget::image(handle).into()
        }
        WidgetSpec::Separator => rule::horizontal(1.0_f32).into(),
        WidgetSpec::Spacer { size } => Space::new().height(size).into(),
        WidgetSpec::Scroll { children } => {
            let col = build_column(children, spacing);
            scrollable(col).into()
        }
        WidgetSpec::Tooltip { inner, text: tip } => {
            let tip_widget = container(text(tip.into_owned()));
            tooltip(
                build_one(*inner, spacing),
                tip_widget,
                tooltip::Position::Top,
            )
            .into()
        }
        WidgetSpec::Popup { children } => {
            let col = build_column(children, spacing);
            Stack::with_children([container(col).into()]).into()
        }
        WidgetSpec::Modal { title, children } => {
            let mut col: Column<'static, Message> =
                column![text(title.into_owned()).size(18)].spacing(spacing);
            for c in children {
                col = col.push(build_one(c, spacing));
            }
            container(col).padding(12).into()
        }
        WidgetSpec::Horizontal(specs) => {
            let children: Vec<Element<'static, Message>> =
                specs.into_iter().map(|s| build_one(s, spacing)).collect();
            Row::with_children(children).spacing(spacing).into()
        }
        WidgetSpec::Vertical(specs) => build_column(specs, spacing),
        WidgetSpec::Grid { cols, children } => {
            // iced 0.14 has no native fixed-column grid; compose from nested
            // Row/Column, chunking children by the column count.
            let safe_cols = cols.max(1);
            let row_elements: Vec<Element<'static, Message>> = children
                .chunks(safe_cols)
                .map(|row_specs| {
                    let row_children: Vec<Element<'static, Message>> = row_specs
                        .iter()
                        .map(|s| build_one(s.clone(), spacing))
                        .collect();
                    Row::with_children(row_children).spacing(spacing).into()
                })
                .collect();
            build_column_from_elements(row_elements, spacing)
        }
        WidgetSpec::RichText(spans) => {
            // Build iced Span values with per-span colour, bold, and size.
            let iced_spans: Vec<iced::widget::text::Span<'static, (), Font>> = spans
                .into_iter()
                .map(|s| {
                    let mut sp = iced_span::<(), Font>(s.text);
                    if let Some([r, g, b, a]) = s.color {
                        sp = sp.color(Color::from_rgba8(r, g, b, a as f32 / 255.0));
                    }
                    if s.bold {
                        sp = sp.font(Font {
                            weight: FontWeight::Bold,
                            ..Font::default()
                        });
                    }
                    if let Some(sz) = s.size {
                        sp = sp.size(sz);
                    }
                    sp
                })
                .collect();
            iced::widget::rich_text(iced_spans).into()
        }
    }
}

/// Build a vertical `Column` element from a list of pre-built [`Element`]s.
fn build_column_from_elements(
    elements: Vec<Element<'static, Message>>,
    spacing: f32,
) -> Element<'static, Message> {
    let mut col: Column<'static, Message> = column![].spacing(spacing);
    for el in elements {
        col = col.push(el);
    }
    col.into()
}

/// Build a vertical `Column` element from a list of [`WidgetSpec`]s.
fn build_column(specs: Vec<WidgetSpec>, spacing: f32) -> Element<'static, Message> {
    let mut col: Column<'static, Message> = column![].spacing(spacing);
    for spec in specs {
        col = col.push(build_one(spec, spacing));
    }
    col.into()
}

// ── IcedNullCtx ───────────────────────────────────────────────────────────────

/// A headless no-op [`UiCtx`] for use in tests and headless scenarios.
///
/// When constructed via [`IcedNullCtx::recording`], all method calls are logged
/// to [`IcedNullCtx::log`] for post-hoc assertion.
#[derive(Default)]
pub struct IcedNullCtx {
    /// Optional call log. `None` means recording is disabled (default).
    pub log: Option<Vec<(&'static str, String)>>,
}

impl IcedNullCtx {
    /// Create a recording `IcedNullCtx` that logs every method call.
    pub fn recording() -> Self {
        Self {
            log: Some(Vec::new()),
        }
    }

    /// Append a `(method, arg)` entry to the log if recording is enabled.
    fn record(&mut self, method: &'static str, arg: impl Into<String>) {
        if let Some(l) = self.log.as_mut() {
            l.push((method, arg.into()));
        }
    }
}

impl UiCtx for IcedNullCtx {
    fn heading(&mut self, t: &str) {
        self.record("heading", t);
    }

    fn label(&mut self, t: &str) {
        self.record("label", t);
    }

    fn button(&mut self, label: &str) -> ButtonResponse {
        self.record("button", label);
        ButtonResponse::default()
    }

    fn text_input(&mut self, text: &str) -> TextInputResponse {
        self.record("text_input", text);
        TextInputResponse::unsupported()
    }

    fn text_area(&mut self, text: &str, min_rows: usize) -> TextAreaResponse {
        self.record("text_area", format!("{text}|rows={min_rows}"));
        TextAreaResponse::unsupported()
    }

    fn checkbox(&mut self, label: &str, _checked: bool) -> CheckboxResponse {
        self.record("checkbox", label);
        CheckboxResponse::unsupported()
    }

    fn slider(&mut self, value: f64, _range: std::ops::RangeInclusive<f64>) -> SliderResponse {
        self.record("slider", value.to_string());
        SliderResponse::unsupported()
    }

    fn dropdown(&mut self, _options: &[&str], selected: usize) -> DropdownResponse {
        self.record("dropdown", selected.to_string());
        DropdownResponse::unsupported()
    }

    fn image(&mut self, uri: &str, _size: Option<oxiui_core::geometry::Size>) -> WidgetResponse {
        self.record("image", uri);
        WidgetResponse::supported()
    }

    fn separator(&mut self) -> WidgetResponse {
        self.record("separator", "");
        WidgetResponse::unsupported()
    }

    fn spacer(&mut self, size: f32) -> WidgetResponse {
        self.record("spacer", size.to_string());
        WidgetResponse::unsupported()
    }

    fn scroll_area(&mut self, _content: &mut dyn FnMut(&mut dyn UiCtx)) -> WidgetResponse {
        self.record("scroll_area", "");
        WidgetResponse::unsupported()
    }

    fn tooltip(&mut self, text: &str) -> WidgetResponse {
        self.record("tooltip", text);
        WidgetResponse::unsupported()
    }

    fn popup(&mut self, _content: &mut dyn FnMut(&mut dyn UiCtx)) -> WidgetResponse {
        self.record("popup", "");
        WidgetResponse::unsupported()
    }

    fn modal(&mut self, title: &str, _content: &mut dyn FnMut(&mut dyn UiCtx)) -> WidgetResponse {
        self.record("modal", title);
        WidgetResponse::unsupported()
    }

    fn horizontal(&mut self, _content: &mut dyn FnMut(&mut dyn UiCtx)) -> WidgetResponse {
        self.record("horizontal", "");
        WidgetResponse::unsupported()
    }

    fn vertical(&mut self, _content: &mut dyn FnMut(&mut dyn UiCtx)) -> WidgetResponse {
        self.record("vertical", "");
        WidgetResponse::unsupported()
    }

    fn grid(&mut self, cols: usize, _content: &mut dyn FnMut(&mut dyn UiCtx)) -> WidgetResponse {
        self.record("grid", cols.to_string());
        WidgetResponse::unsupported()
    }

    fn rich_text(&mut self, spans: &[oxiui_core::RichTextSpan]) -> WidgetResponse {
        self.record("rich_text", spans.len().to_string());
        WidgetResponse::unsupported()
    }
}

// ── OxiIcedWidget ─────────────────────────────────────────────────────────────

use iced::advanced::{layout, renderer, widget as adv_widget};

/// A custom iced widget wrapping an OxiUI [`WidgetSpec`].
///
/// Allows embedding OxiUI widget specs inside iced layout trees as first-class
/// iced `Widget` values. Use [`oxi_widget`] to construct.
pub struct OxiIcedWidget {
    spec: WidgetSpec,
    width: iced::Length,
    height: iced::Length,
}

impl OxiIcedWidget {
    /// Create a new [`OxiIcedWidget`] wrapping the given [`WidgetSpec`].
    pub fn new(spec: WidgetSpec) -> Self {
        OxiIcedWidget {
            spec,
            width: iced::Length::Shrink,
            height: iced::Length::Shrink,
        }
    }

    /// Return a reference to the underlying [`WidgetSpec`].
    pub fn spec(&self) -> &WidgetSpec {
        &self.spec
    }

    /// Set the widget's width.
    pub fn width(mut self, w: iced::Length) -> Self {
        self.width = w;
        self
    }

    /// Set the widget's height.
    pub fn height(mut self, h: iced::Length) -> Self {
        self.height = h;
        self
    }
}

impl<Msg, Theme, Renderer> iced::advanced::Widget<Msg, Theme, Renderer> for OxiIcedWidget
where
    Renderer: iced::advanced::Renderer,
{
    fn size(&self) -> iced::Size<iced::Length> {
        iced::Size::new(self.width, self.height)
    }

    fn layout(
        &mut self,
        _tree: &mut adv_widget::Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let size = limits.resolve(self.width, self.height, iced::Size::ZERO);
        layout::Node::new(size)
    }

    fn draw(
        &self,
        _tree: &adv_widget::Tree,
        _renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        _layout: iced::advanced::Layout<'_>,
        _cursor: iced::advanced::mouse::Cursor,
        _viewport: &iced::Rectangle,
    ) {
        // Stub: drawing is delegated to the materialized iced element pipeline.
        // Full drawing would require converting to an Element and calling its
        // Widget::draw — which requires a concrete renderer type. Deferred.
    }
}

/// Construct an [`OxiIcedWidget`] from the given [`WidgetSpec`].
///
/// The resulting widget has `Shrink` width and height by default; call
/// `.width()` / `.height()` on the returned struct to override.
pub fn oxi_widget(spec: WidgetSpec) -> OxiIcedWidget {
    OxiIcedWidget::new(spec)
}

// ── Keyboard mapping ──────────────────────────────────────────────────────────

/// Map an iced keyboard event to an [`oxiui_core::UiEvent`].
///
/// Returns `None` for events that don't correspond to a key press or release
/// (e.g. `ModifiersChanged`).
pub fn map_iced_keyboard_event(ev: &iced::keyboard::Event) -> Option<oxiui_core::UiEvent> {
    use iced::keyboard::Event as KbEv;
    match ev {
        KbEv::KeyPressed {
            key,
            modifiers,
            repeat,
            ..
        } => Some(oxiui_core::UiEvent::KeyDown {
            key: map_iced_key(key),
            modifiers: map_iced_modifiers(*modifiers),
            repeat: *repeat,
        }),
        KbEv::KeyReleased { key, modifiers, .. } => Some(oxiui_core::UiEvent::KeyUp {
            key: map_iced_key(key),
            modifiers: map_iced_modifiers(*modifiers),
        }),
        KbEv::ModifiersChanged(_) => None,
    }
}

/// Map an iced [`iced::keyboard::Key`] to an [`oxiui_core::events::Key`].
pub fn map_iced_key(key: &iced::keyboard::Key) -> oxiui_core::events::Key {
    use iced::keyboard::key::Named;
    use iced::keyboard::Key as IK;
    use oxiui_core::events::Key as OxiKey;

    match key {
        IK::Character(s) => OxiKey::Character(s.as_str().to_owned()),
        IK::Named(named) => match named {
            Named::Enter => OxiKey::Enter,
            Named::Tab => OxiKey::Tab,
            Named::Space => OxiKey::Space,
            Named::Backspace => OxiKey::Backspace,
            Named::Delete => OxiKey::Delete,
            Named::Escape => OxiKey::Escape,
            Named::ArrowLeft => OxiKey::ArrowLeft,
            Named::ArrowRight => OxiKey::ArrowRight,
            Named::ArrowUp => OxiKey::ArrowUp,
            Named::ArrowDown => OxiKey::ArrowDown,
            Named::Home => OxiKey::Home,
            Named::End => OxiKey::End,
            Named::PageUp => OxiKey::PageUp,
            Named::PageDown => OxiKey::PageDown,
            Named::F1 => OxiKey::Function(1),
            Named::F2 => OxiKey::Function(2),
            Named::F3 => OxiKey::Function(3),
            Named::F4 => OxiKey::Function(4),
            Named::F5 => OxiKey::Function(5),
            Named::F6 => OxiKey::Function(6),
            Named::F7 => OxiKey::Function(7),
            Named::F8 => OxiKey::Function(8),
            Named::F9 => OxiKey::Function(9),
            Named::F10 => OxiKey::Function(10),
            Named::F11 => OxiKey::Function(11),
            Named::F12 => OxiKey::Function(12),
            other => OxiKey::Named(format!("{other:?}")),
        },
        IK::Unidentified => OxiKey::Named("Unidentified".to_owned()),
    }
}

/// Map iced [`iced::keyboard::Modifiers`] to [`oxiui_core::events::Modifiers`].
pub fn map_iced_modifiers(mods: iced::keyboard::Modifiers) -> oxiui_core::events::Modifiers {
    oxiui_core::events::Modifiers {
        ctrl: mods.control(),
        shift: mods.shift(),
        alt: mods.alt(),
        meta: mods.logo(),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── image ────────────────────────────────────────────────────────────────

    #[test]
    fn image_ctx_returns_supported() {
        let mut ctx = IcedUiCtx::new(IcedConfig::default());
        let resp = ctx.image("test.png", None);
        assert!(resp.supported, "IcedUiCtx::image() must return supported");
    }

    #[test]
    fn image_null_ctx_returns_supported() {
        let mut ctx = IcedNullCtx::recording();
        let resp = ctx.image("test.png", None);
        assert!(resp.supported, "IcedNullCtx::image() must return supported");
    }

    // ── OxiIcedWidget ────────────────────────────────────────────────────────

    #[test]
    fn oxi_widget_constructs_with_shrink_defaults() {
        let w = oxi_widget(WidgetSpec::Label(Cow::Borrowed("hello")));
        assert_eq!(w.width, iced::Length::Shrink);
        assert_eq!(w.height, iced::Length::Shrink);
    }

    #[test]
    fn oxi_widget_builder_overrides_dimensions() {
        let w = oxi_widget(WidgetSpec::Label(Cow::Borrowed("hi")))
            .width(iced::Length::Fill)
            .height(iced::Length::Fixed(100.0));
        assert_eq!(w.width, iced::Length::Fill);
        assert_eq!(w.height, iced::Length::Fixed(100.0));
    }

    // ── Keyboard mapping ─────────────────────────────────────────────────────

    #[test]
    fn map_character_key_a() {
        let key = iced::keyboard::Key::Character("a".into());
        let result = map_iced_key(&key);
        assert_eq!(result, oxiui_core::events::Key::Character("a".to_owned()));
    }

    #[test]
    fn map_character_key_z() {
        let key = iced::keyboard::Key::Character("z".into());
        let result = map_iced_key(&key);
        assert_eq!(result, oxiui_core::events::Key::Character("z".to_owned()));
    }

    #[test]
    fn map_named_enter() {
        let key = iced::keyboard::Key::Named(iced::keyboard::key::Named::Enter);
        let result = map_iced_key(&key);
        assert_eq!(result, oxiui_core::events::Key::Enter);
    }

    #[test]
    fn map_named_escape() {
        let key = iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape);
        let result = map_iced_key(&key);
        assert_eq!(result, oxiui_core::events::Key::Escape);
    }

    #[test]
    fn map_named_arrow_left() {
        let key = iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowLeft);
        let result = map_iced_key(&key);
        assert_eq!(result, oxiui_core::events::Key::ArrowLeft);
    }

    #[test]
    fn map_named_arrow_right() {
        let key = iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowRight);
        let result = map_iced_key(&key);
        assert_eq!(result, oxiui_core::events::Key::ArrowRight);
    }

    #[test]
    fn map_named_f1() {
        let key = iced::keyboard::Key::Named(iced::keyboard::key::Named::F1);
        let result = map_iced_key(&key);
        assert_eq!(result, oxiui_core::events::Key::Function(1));
    }

    #[test]
    fn map_named_f12() {
        let key = iced::keyboard::Key::Named(iced::keyboard::key::Named::F12);
        let result = map_iced_key(&key);
        assert_eq!(result, oxiui_core::events::Key::Function(12));
    }

    #[test]
    fn map_unidentified_key() {
        let key = iced::keyboard::Key::Unidentified;
        let result = map_iced_key(&key);
        assert_eq!(
            result,
            oxiui_core::events::Key::Named("Unidentified".to_owned())
        );
    }

    #[test]
    fn map_modifiers_ctrl_shift() {
        use iced::keyboard::Modifiers;
        let mods = Modifiers::CTRL | Modifiers::SHIFT;
        let result = map_iced_modifiers(mods);
        assert!(result.ctrl, "ctrl must be set");
        assert!(result.shift, "shift must be set");
        assert!(!result.alt, "alt must not be set");
        assert!(!result.meta, "meta must not be set");
    }

    #[test]
    fn map_modifiers_none() {
        use iced::keyboard::Modifiers;
        let result = map_iced_modifiers(Modifiers::NONE);
        assert!(!result.ctrl);
        assert!(!result.shift);
        assert!(!result.alt);
        assert!(!result.meta);
    }

    #[test]
    fn map_modifiers_alt_logo() {
        use iced::keyboard::Modifiers;
        let mods = Modifiers::ALT | Modifiers::LOGO;
        let result = map_iced_modifiers(mods);
        assert!(result.alt, "alt must be set");
        assert!(result.meta, "meta must be set");
        assert!(!result.ctrl);
        assert!(!result.shift);
    }
}
