//! Widget combinators and the clipboard / drag-and-drop / cursor abstractions.
//!
//! [`WidgetExt`] adds chainable decorators to any [`Widget`]: `.padding(..)`,
//! `.margin(..)`, `.background(..)`, `.border(..)`, `.on_click(..)`,
//! `.on_hover(..)`. Each returns a wrapper widget that records the decoration
//! and forwards [`Widget::render`] to the inner widget, so decorators compose
//! (`w.padding(p).border(b)` nests two wrappers). The wrappers expose their
//! recorded style so an adapter that understands them can honour it; adapters
//! that don't still render the inner widget correctly.
//!
//! The trait objects [`ClipboardProvider`], [`DragSource`] and [`DropTarget`]
//! are the platform seams a backend implements; [`DropEffect`] and the cursor
//! shape (re-exported from [`style`](crate::style)) round out the interaction
//! surface.

use crate::style::{Border, Margin, Padding};
use crate::{ButtonResponse, Color, UiCtx, UiError, Widget};

// ── Clipboard ────────────────────────────────────────────────────────────────

/// A clipboard backend. Plain-text access plus optional MIME-typed payloads for
/// rich clipboard content (HTML, images, …).
pub trait ClipboardProvider {
    /// Read the clipboard's plain-text contents, if any.
    fn get_text(&self) -> Result<Option<String>, UiError>;

    /// Replace the clipboard's plain-text contents.
    fn set_text(&mut self, text: &str) -> Result<(), UiError>;

    /// Read a MIME-typed payload (e.g. `"text/html"`), if the backend supports
    /// it. The default returns `Ok(None)` (unsupported MIME type).
    fn get_mime(&self, _mime: &str) -> Result<Option<Vec<u8>>, UiError> {
        Ok(None)
    }

    /// Write a MIME-typed payload. The default returns
    /// [`UiError::Clipboard`] indicating rich clipboard is unsupported.
    fn set_mime(&mut self, mime: &str, _data: &[u8]) -> Result<(), UiError> {
        Err(UiError::Clipboard(format!(
            "MIME type '{mime}' not supported"
        )))
    }
}

// ── Drag and drop ──────────────────────────────────────────────────────────

/// The effect a drop performs, mirroring the HTML drag-and-drop model.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum DropEffect {
    /// The drop is rejected.
    #[default]
    None,
    /// Copy the dragged data (source retained).
    Copy,
    /// Move the dragged data (source removed).
    Move,
    /// Create a link/reference to the dragged data.
    Link,
}

/// A typed payload carried during a drag.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DragData {
    /// The MIME type describing `bytes` (e.g. `"text/plain"`).
    pub mime: String,
    /// The raw payload bytes.
    pub bytes: Vec<u8>,
}

impl DragData {
    /// A `text/plain` payload from a string.
    pub fn text(s: impl Into<String>) -> Self {
        Self {
            mime: "text/plain".to_owned(),
            bytes: s.into().into_bytes(),
        }
    }

    /// A payload with an explicit MIME type.
    pub fn new(mime: impl Into<String>, bytes: Vec<u8>) -> Self {
        Self {
            mime: mime.into(),
            bytes,
        }
    }

    /// Interpret the payload as UTF-8 text, if valid.
    pub fn as_text(&self) -> Option<String> {
        String::from_utf8(self.bytes.clone()).ok()
    }
}

/// Something that can originate a drag.
pub trait DragSource {
    /// Produce the payload to carry for this drag, or `None` to not start one.
    fn drag_data(&self) -> Option<DragData>;

    /// The effects this source permits (defaults to copy + move).
    fn allowed_effects(&self) -> &[DropEffect] {
        const DEFAULT: &[DropEffect] = &[DropEffect::Copy, DropEffect::Move];
        DEFAULT
    }
}

/// Something that can accept a drop.
pub trait DropTarget {
    /// Whether this target accepts `data`, and if so which effect it would
    /// apply. Returns [`DropEffect::None`] to reject.
    fn can_accept(&self, data: &DragData) -> DropEffect;

    /// Commit a drop of `data` with the negotiated `effect`. Returns whether the
    /// drop was consumed.
    fn accept_drop(&mut self, data: &DragData, effect: DropEffect) -> Result<bool, UiError>;
}

// ── WidgetExt combinators ────────────────────────────────────────────────────

/// A click callback invoked when the wrapped widget's [`ButtonResponse`]
/// reports `clicked`.
type ClickFn = Box<dyn FnMut()>;
/// A hover callback invoked with the current hover state.
type HoverFn = Box<dyn FnMut(bool)>;

/// Wraps a widget with [`Padding`]; renders the inner widget unchanged but
/// exposes the padding for layout-aware adapters.
pub struct Padded<W> {
    inner: W,
    /// The padding to apply around the inner widget.
    pub padding: Padding,
}

impl<W: Widget> Widget for Padded<W> {
    fn render(&mut self, ui: &mut dyn UiCtx) {
        self.inner.render(ui);
    }
}

/// Wraps a widget with [`Margin`].
pub struct Margined<W> {
    inner: W,
    /// The margin around the inner widget.
    pub margin: Margin,
}

impl<W: Widget> Widget for Margined<W> {
    fn render(&mut self, ui: &mut dyn UiCtx) {
        self.inner.render(ui);
    }
}

/// Wraps a widget with a background [`Color`].
pub struct Backgrounded<W> {
    inner: W,
    /// The background fill colour.
    pub background: Color,
}

impl<W: Widget> Widget for Backgrounded<W> {
    fn render(&mut self, ui: &mut dyn UiCtx) {
        self.inner.render(ui);
    }
}

/// Wraps a widget with a [`Border`].
pub struct Bordered<W> {
    inner: W,
    /// The border to draw around the inner widget.
    pub border: Border,
}

impl<W: Widget> Widget for Bordered<W> {
    fn render(&mut self, ui: &mut dyn UiCtx) {
        self.inner.render(ui);
    }
}

/// Wraps a widget so a callback fires when it is clicked.
///
/// The wrapper renders the inner widget, then renders a companion button whose
/// label is `click_label`; when that button reports `clicked`, the callback
/// runs. This keeps the immediate-mode contract (no retained state) while still
/// offering an ergonomic `.on_click` combinator. Use [`OnClick::probe`] in
/// tests to drive the callback directly.
pub struct OnClick<W> {
    inner: W,
    label: String,
    callback: ClickFn,
}

impl<W: Widget> OnClick<W> {
    /// Manually deliver a [`ButtonResponse`]; invokes the callback when
    /// `response.clicked` is set. Returns whether the callback fired.
    pub fn probe(&mut self, response: &ButtonResponse) -> bool {
        if response.clicked {
            (self.callback)();
            true
        } else {
            false
        }
    }
}

impl<W: Widget> Widget for OnClick<W> {
    fn render(&mut self, ui: &mut dyn UiCtx) {
        self.inner.render(ui);
        let resp = ui.button(&self.label);
        if resp.clicked {
            (self.callback)();
        }
    }
}

/// Wraps a widget so a callback receives hover-state changes.
pub struct OnHover<W> {
    inner: W,
    label: String,
    callback: HoverFn,
}

impl<W: Widget> OnHover<W> {
    /// Manually deliver a [`ButtonResponse`]; invokes the callback with
    /// `response.hovered`.
    pub fn probe(&mut self, response: &ButtonResponse) {
        (self.callback)(response.hovered);
    }
}

impl<W: Widget> Widget for OnHover<W> {
    fn render(&mut self, ui: &mut dyn UiCtx) {
        self.inner.render(ui);
        let resp = ui.button(&self.label);
        (self.callback)(resp.hovered);
    }
}

/// Chainable decorators for any [`Widget`].
///
/// Blanket-implemented for every `Widget`, so `my_widget.padding(p).border(b)`
/// works without per-type impls. Each method consumes `self` and returns a
/// wrapper that still implements [`Widget`].
pub trait WidgetExt: Widget + Sized {
    /// Wrap with [`Padding`].
    fn padding(self, padding: Padding) -> Padded<Self> {
        Padded {
            inner: self,
            padding,
        }
    }

    /// Wrap with [`Margin`].
    fn margin(self, margin: Margin) -> Margined<Self> {
        Margined {
            inner: self,
            margin,
        }
    }

    /// Wrap with a background [`Color`].
    fn background(self, background: Color) -> Backgrounded<Self> {
        Backgrounded {
            inner: self,
            background,
        }
    }

    /// Wrap with a [`Border`].
    fn border(self, border: Border) -> Bordered<Self> {
        Bordered {
            inner: self,
            border,
        }
    }

    /// Attach a click callback, surfaced through a companion button labelled
    /// `label`.
    fn on_click(self, label: impl Into<String>, callback: impl FnMut() + 'static) -> OnClick<Self> {
        OnClick {
            inner: self,
            label: label.into(),
            callback: Box::new(callback),
        }
    }

    /// Attach a hover callback, surfaced through a companion button labelled
    /// `label`.
    fn on_hover(
        self,
        label: impl Into<String>,
        callback: impl FnMut(bool) + 'static,
    ) -> OnHover<Self> {
        OnHover {
            inner: self,
            label: label.into(),
            callback: Box::new(callback),
        }
    }
}

impl<W: Widget> WidgetExt for W {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Insets;
    use std::cell::Cell;
    use std::rc::Rc;

    /// A trivial widget that records each render into a shared counter.
    struct Probe(Rc<Cell<u32>>);
    impl Widget for Probe {
        fn render(&mut self, _ui: &mut dyn UiCtx) {
            self.0.set(self.0.get() + 1);
        }
    }

    /// A UiCtx that returns a fixed ButtonResponse for `button`.
    struct StubCtx {
        clicked: bool,
        hovered: bool,
    }
    impl UiCtx for StubCtx {
        fn heading(&mut self, _text: &str) {}
        fn label(&mut self, _text: &str) {}
        fn button(&mut self, _label: &str) -> ButtonResponse {
            ButtonResponse {
                clicked: self.clicked,
                hovered: self.hovered,
            }
        }
    }

    #[test]
    fn decorators_record_style_and_forward_render() {
        let n = Rc::new(Cell::new(0u32));
        let mut w = Probe(Rc::clone(&n))
            .padding(Padding::all(4.0))
            .border(Border::solid(1.0, Color(0, 0, 0, 255)));
        // Outer is Bordered<Padded<Probe>>: style is exposed.
        assert_eq!(w.border.insets, Insets::all(1.0));
        let mut ctx = StubCtx {
            clicked: false,
            hovered: false,
        };
        w.render(&mut ctx);
        assert_eq!(n.get(), 1, "inner widget should still render exactly once");
    }

    #[test]
    fn background_and_margin_compose() {
        let n = Rc::new(Cell::new(0u32));
        let w = Probe(Rc::clone(&n))
            .background(Color(10, 20, 30, 255))
            .margin(Margin::symmetric(2.0, 4.0));
        assert_eq!(w.margin.insets(), Insets::symmetric(2.0, 4.0));
        // Inner background preserved through the margin wrapper.
        assert_eq!(w.inner.background, Color(10, 20, 30, 255));
    }

    #[test]
    fn on_click_fires_callback_when_clicked() {
        let n = Rc::new(Cell::new(0u32));
        let clicks = Rc::new(Cell::new(0u32));
        let clicks_c = Rc::clone(&clicks);
        let mut w = Probe(Rc::clone(&n)).on_click("ok", move || clicks_c.set(clicks_c.get() + 1));

        // Not clicked -> callback does not fire.
        let mut ctx = StubCtx {
            clicked: false,
            hovered: false,
        };
        w.render(&mut ctx);
        assert_eq!(clicks.get(), 0);

        // Clicked -> callback fires.
        let mut ctx = StubCtx {
            clicked: true,
            hovered: false,
        };
        w.render(&mut ctx);
        assert_eq!(clicks.get(), 1);
        assert_eq!(n.get(), 2, "inner rendered each frame");
    }

    #[test]
    fn on_hover_reports_state() {
        let n = Rc::new(Cell::new(0u32));
        let hovered = Rc::new(Cell::new(false));
        let hovered_c = Rc::clone(&hovered);
        let mut w = Probe(Rc::clone(&n)).on_hover("h", move |h| hovered_c.set(h));
        let mut ctx = StubCtx {
            clicked: false,
            hovered: true,
        };
        w.render(&mut ctx);
        assert!(hovered.get());
    }

    #[test]
    fn on_click_probe_helper() {
        let fired = Rc::new(Cell::new(false));
        let fired_c = Rc::clone(&fired);
        let n = Rc::new(Cell::new(0u32));
        let mut w = Probe(n).on_click("x", move || fired_c.set(true));
        assert!(w.probe(&ButtonResponse {
            clicked: true,
            hovered: false
        }));
        assert!(fired.get());
        assert!(!w.probe(&ButtonResponse {
            clicked: false,
            hovered: false
        }));
    }

    #[test]
    fn drag_data_text_roundtrip() {
        let d = DragData::text("hello");
        assert_eq!(d.mime, "text/plain");
        assert_eq!(d.as_text().as_deref(), Some("hello"));
    }

    #[test]
    fn drop_effect_default_is_none() {
        assert_eq!(DropEffect::default(), DropEffect::None);
    }

    // A minimal clipboard to exercise the default MIME behaviour.
    struct MemClipboard {
        text: Option<String>,
    }
    impl ClipboardProvider for MemClipboard {
        fn get_text(&self) -> Result<Option<String>, UiError> {
            Ok(self.text.clone())
        }
        fn set_text(&mut self, text: &str) -> Result<(), UiError> {
            self.text = Some(text.to_owned());
            Ok(())
        }
    }

    #[test]
    fn clipboard_default_mime_is_unsupported() {
        let mut c = MemClipboard { text: None };
        c.set_text("hi").expect("set");
        assert_eq!(c.get_text().expect("get"), Some("hi".to_string()));
        // Default get_mime returns None; default set_mime errors.
        assert_eq!(c.get_mime("text/html").expect("mime get"), None);
        assert!(matches!(
            c.set_mime("text/html", b"<b>x</b>"),
            Err(UiError::Clipboard(_))
        ));
    }
}
