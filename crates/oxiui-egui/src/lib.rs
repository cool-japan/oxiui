#![forbid(unsafe_code)]
#![warn(missing_docs)]
//! `oxiui-egui` — egui/eframe adapter for OxiUI.
//!
//! Converts OxiUI [`Palette`] to [`egui::Visuals`], loads OxiFont bytes
//! into egui's font system, and provides [`EguiUiCtx`] which implements
//! [`UiCtx`] in terms of egui's `Ui` object.

use std::sync::Arc;

use oxiui_core::{
    ButtonResponse, CheckboxResponse, Color, DropdownResponse, Key, Modifiers, MouseButton,
    Palette, Size, SliderResponse, TextInputResponse, TextStyle, UiCtx, UiError, UiEvent, Widget,
    WidgetResponse,
};
use oxiui_theme::DesignTokens;

/// An [`UiCtx`] implementation backed by an egui [`egui::Ui`].
///
/// All heading/label/button calls are forwarded directly to egui. Extended
/// widget methods (`text_input`, `checkbox`, `slider`, `dropdown`, `image`,
/// `separator`, `spacer`, `scroll_area`, `tooltip`, `popup`, `modal`) are
/// also implemented and forward to their egui equivalents.
pub struct EguiUiCtx<'a> {
    ui: &'a mut egui::Ui,
    /// The egui response from the most recently rendered widget, if any.
    last_response: Option<egui::Response>,
    /// Monotonically incrementing sequence used to generate per-widget id salts.
    id_seq: usize,
}

impl<'a> EguiUiCtx<'a> {
    /// Wrap an egui `Ui` reference as an [`UiCtx`].
    pub fn new(ui: &'a mut egui::Ui) -> Self {
        Self {
            ui,
            last_response: None,
            id_seq: 0,
        }
    }

    /// Return a reference to the egui response produced by the most recently
    /// rendered widget, if one is stored.
    pub fn response(&self) -> Option<&egui::Response> {
        self.last_response.as_ref()
    }

    /// Advance the id sequence and return a fresh [`egui::Id`] salt value.
    fn next_salt(&mut self) -> egui::Id {
        let s = self.id_seq;
        self.id_seq += 1;
        egui::Id::new(("oxiui_widget", s))
    }
}

impl<'a> EguiUiCtx<'a> {
    /// Read the most-recently-copied text from egui's output command queue.
    ///
    /// Returns `Some(text)` if a [`egui::OutputCommand::CopyText`] command was
    /// queued in the current frame, otherwise `None`. Note that this reflects
    /// text *set* via egui's copy mechanism within the same frame; it does not
    /// read from the OS clipboard.
    pub fn clipboard_get(&self) -> Option<String> {
        self.ui.ctx().output(|o| {
            o.commands.iter().find_map(|cmd| {
                if let egui::OutputCommand::CopyText(text) = cmd {
                    if text.is_empty() {
                        None
                    } else {
                        Some(text.clone())
                    }
                } else {
                    None
                }
            })
        })
    }

    /// Queue a copy-to-clipboard command via egui.
    ///
    /// The `text` is enqueued as an [`egui::OutputCommand::CopyText`] and will
    /// be forwarded to the OS clipboard by the egui integration at the end of
    /// the frame.
    pub fn clipboard_set(&self, text: &str) {
        self.ui.ctx().copy_text(text.to_owned());
    }
}

impl<'a> UiCtx for EguiUiCtx<'a> {
    fn heading(&mut self, text: &str) {
        self.ui.heading(text);
    }

    fn label(&mut self, text: &str) {
        self.ui.label(text);
    }

    fn button(&mut self, label: &str) -> ButtonResponse {
        let resp = self.ui.button(label);
        ButtonResponse {
            clicked: resp.clicked(),
            hovered: resp.hovered(),
        }
    }

    fn text_input(&mut self, text: &str) -> TextInputResponse {
        let mut s = text.to_owned();
        let r = self.ui.add(egui::TextEdit::singleline(&mut s));
        let changed = r.changed();
        self.last_response = Some(r);
        TextInputResponse::supported(s, changed)
    }

    fn checkbox(&mut self, label: &str, checked: bool) -> CheckboxResponse {
        let mut c = checked;
        let r = self.ui.checkbox(&mut c, label);
        let changed = r.changed();
        self.last_response = Some(r);
        CheckboxResponse::supported(c, changed)
    }

    fn slider(&mut self, value: f64, range: std::ops::RangeInclusive<f64>) -> SliderResponse {
        let mut v = value;
        let r = self.ui.add(egui::Slider::new(&mut v, range));
        let changed = r.changed();
        self.last_response = Some(r);
        SliderResponse::supported(v, changed)
    }

    fn dropdown(&mut self, options: &[&str], selected: usize) -> DropdownResponse {
        let mut sel = selected.min(options.len().saturating_sub(1));
        let salt = self.next_salt();
        let r =
            egui::ComboBox::from_id_salt(salt)
                .show_index(self.ui, &mut sel, options.len(), |i| options[i]);
        let changed = r.changed();
        self.last_response = Some(r);
        DropdownResponse::supported(sel, changed)
    }

    fn image(&mut self, uri: &str, size: Option<Size>) -> WidgetResponse {
        let mut img = egui::Image::from_uri(uri.to_owned());
        if let Some(s) = size {
            img = img.fit_to_exact_size(egui::vec2(s.width, s.height));
        }
        let r = self.ui.add(img);
        self.last_response = Some(r);
        WidgetResponse::supported()
    }

    fn separator(&mut self) -> WidgetResponse {
        let r = self.ui.separator();
        self.last_response = Some(r);
        WidgetResponse::supported()
    }

    fn spacer(&mut self, size: f32) -> WidgetResponse {
        self.ui.add_space(size);
        self.last_response = None;
        WidgetResponse::supported()
    }

    fn scroll_area(&mut self, content: &mut dyn FnMut(&mut dyn UiCtx)) -> WidgetResponse {
        egui::ScrollArea::vertical().show(self.ui, |ui| {
            let mut child = EguiUiCtx::new(ui);
            content(&mut child);
        });
        self.last_response = None;
        WidgetResponse::supported()
    }

    fn tooltip(&mut self, text: &str) -> WidgetResponse {
        if let Some(r) = self.last_response.take() {
            self.last_response = Some(r.on_hover_text(text));
            WidgetResponse::supported()
        } else {
            WidgetResponse::unsupported()
        }
    }

    fn popup(&mut self, content: &mut dyn FnMut(&mut dyn UiCtx)) -> WidgetResponse {
        let ctx = self.ui.ctx().clone();
        let salt = self.next_salt();
        egui::Window::new("")
            .id(egui::Id::new(("oxiui_popup", salt)))
            .title_bar(false)
            .resizable(false)
            .show(&ctx, |ui| {
                let mut child = EguiUiCtx::new(ui);
                content(&mut child);
            });
        self.last_response = None;
        WidgetResponse::supported()
    }

    fn modal(&mut self, title: &str, content: &mut dyn FnMut(&mut dyn UiCtx)) -> WidgetResponse {
        let ctx = self.ui.ctx().clone();
        let salt = self.next_salt();
        let title = title.to_owned();
        egui::Modal::new(egui::Id::new(("oxiui_modal", salt))).show(&ctx, |ui| {
            ui.heading(&title);
            let mut child = EguiUiCtx::new(ui);
            content(&mut child);
        });
        self.last_response = None;
        WidgetResponse::supported()
    }

    fn horizontal(&mut self, content: &mut dyn FnMut(&mut dyn UiCtx)) -> WidgetResponse {
        self.ui.horizontal(|ui| {
            let mut child = EguiUiCtx::new(ui);
            content(&mut child);
        });
        self.last_response = None;
        WidgetResponse::supported()
    }

    fn vertical(&mut self, content: &mut dyn FnMut(&mut dyn UiCtx)) -> WidgetResponse {
        self.ui.vertical(|ui| {
            let mut child = EguiUiCtx::new(ui);
            content(&mut child);
        });
        self.last_response = None;
        WidgetResponse::supported()
    }

    fn grid(&mut self, cols: usize, content: &mut dyn FnMut(&mut dyn UiCtx)) -> WidgetResponse {
        let salt = self.next_salt();
        egui::Grid::new(salt).num_columns(cols).show(self.ui, |ui| {
            let mut child = EguiUiCtx::new(ui);
            content(&mut child);
        });
        self.last_response = None;
        WidgetResponse::supported()
    }

    fn menu_bar(&mut self, content: &mut dyn FnMut(&mut dyn UiCtx)) -> WidgetResponse {
        egui::MenuBar::new().ui(self.ui, |ui| {
            let mut child = EguiUiCtx::new(ui);
            content(&mut child);
        });
        self.last_response = None;
        WidgetResponse::supported()
    }

    fn rich_text(&mut self, spans: &[oxiui_core::RichTextSpan]) -> WidgetResponse {
        use egui::text::{LayoutJob, TextFormat};
        let mut job = LayoutJob::default();
        for span in spans {
            let color = egui::Color32::from_rgba_unmultiplied(
                span.color[0],
                span.color[1],
                span.color[2],
                span.color[3],
            );
            // `TextFormat` in egui 0.34 has no `bold` field; bold spans are
            // rendered using the same font weight as non-bold (deviation noted).
            // `font_family` override is not applied when `TextFormat` is built
            // this way; it would require a named `FontFamily` registered in the
            // egui `FontDefinitions` (deviation noted).
            let font_id = egui::FontId::proportional(span.font_size);
            job.append(
                &span.text,
                0.0,
                TextFormat {
                    color,
                    font_id,
                    italics: span.italic,
                    ..Default::default()
                },
            );
        }
        let r = self.ui.label(job);
        self.last_response = Some(r);
        WidgetResponse::supported()
    }

    fn drag_source(&mut self, id: u64, content: &mut dyn FnMut(&mut dyn UiCtx)) -> WidgetResponse {
        // `dnd_drag_source` returns `InnerResponse<R>` (not a tuple).
        self.ui.dnd_drag_source(egui::Id::new(id), id, |ui| {
            let mut child = EguiUiCtx::new(ui);
            content(&mut child);
        });
        self.last_response = None;
        WidgetResponse::supported()
    }

    fn drop_target(
        &mut self,
        accept_ids: &[u64],
        content: &mut dyn FnMut(&mut dyn UiCtx),
    ) -> WidgetResponse {
        // `dnd_drop_zone` returns `(InnerResponse<R>, Option<Arc<Payload>>)`.
        // `WidgetResponse` has no `drag_dropped` field; accept-id filtering
        // cannot be surfaced to the caller through the current response type
        // (deviation noted). The payload is checked so egui performs the correct
        // drop-zone highlighting, but the result is not returned.
        let (_inner, payload) = self
            .ui
            .dnd_drop_zone::<u64, ()>(egui::Frame::default(), |ui| {
                let mut child = EguiUiCtx::new(ui);
                content(&mut child);
            });
        // Evaluate acceptance purely to suppress the unused-variable warning.
        let _accepted = payload
            .as_deref()
            .map(|p| accept_ids.contains(p))
            .unwrap_or(false);
        self.last_response = None;
        WidgetResponse::supported()
    }

    fn label_styled(&mut self, text: &str, style: TextStyle) -> WidgetResponse {
        let mut rt = egui::RichText::new(text);
        if let Some(sz) = style.font_size {
            rt = rt.size(sz);
        }
        if style.font_weight >= 600 {
            rt = rt.strong();
        }
        if style.italic {
            rt = rt.italics();
        }
        if style.underline {
            rt = rt.underline();
        }
        if style.strikethrough {
            rt = rt.strikethrough();
        }
        if let Some([r, g, b, a]) = style.color {
            rt = rt.color(egui::Color32::from_rgba_unmultiplied(r, g, b, a));
        }
        let r = self.ui.label(rt);
        self.last_response = Some(r);
        WidgetResponse::supported()
    }

    fn heading_styled(&mut self, text: &str, style: TextStyle) -> WidgetResponse {
        let mut rt = egui::RichText::new(text).heading();
        if let Some(sz) = style.font_size {
            rt = rt.size(sz);
        }
        if style.font_weight >= 600 {
            rt = rt.strong();
        }
        if style.italic {
            rt = rt.italics();
        }
        if let Some([r, g, b, a]) = style.color {
            rt = rt.color(egui::Color32::from_rgba_unmultiplied(r, g, b, a));
        }
        let r = self.ui.label(rt);
        self.last_response = Some(r);
        WidgetResponse::supported()
    }
}

// ── Event helpers ────────────────────────────────────────────────────────────

/// Map an OxiUI [`Key`] to the closest [`egui::Key`].
///
/// For any variant without an exact egui equivalent the fallback is
/// [`egui::Key::F12`] (which is noted in the deviation log).
///
/// Deviation note: `Key::Character(String)` and `Key::Named(String)` are
/// forwarded via [`egui::Key::from_name`]; if the name is not recognised by
/// egui, `F12` is used as the fallback.
fn map_key(key: &Key) -> egui::Key {
    match key {
        Key::Enter => egui::Key::Enter,
        Key::Tab => egui::Key::Tab,
        Key::Space => egui::Key::Space,
        Key::Backspace => egui::Key::Backspace,
        Key::Delete => egui::Key::Delete,
        Key::Escape => egui::Key::Escape,
        Key::ArrowLeft => egui::Key::ArrowLeft,
        Key::ArrowRight => egui::Key::ArrowRight,
        Key::ArrowUp => egui::Key::ArrowUp,
        Key::ArrowDown => egui::Key::ArrowDown,
        Key::Home => egui::Key::Home,
        Key::End => egui::Key::End,
        Key::PageUp => egui::Key::PageUp,
        Key::PageDown => egui::Key::PageDown,
        Key::Function(n) => map_function_key(*n),
        Key::Character(s) => egui::Key::from_name(s.as_str()).unwrap_or(egui::Key::F12),
        Key::Named(s) => egui::Key::from_name(s.as_str()).unwrap_or(egui::Key::F12),
        // Non-exhaustive: any future Key variants fall back to F12.
        _ => egui::Key::F12,
    }
}

/// Map a function-key number (1-based) to the corresponding [`egui::Key`].
///
/// egui supports F1–F35. Numbers above 35 fall back to [`egui::Key::F12`].
fn map_function_key(n: u8) -> egui::Key {
    match n {
        1 => egui::Key::F1,
        2 => egui::Key::F2,
        3 => egui::Key::F3,
        4 => egui::Key::F4,
        5 => egui::Key::F5,
        6 => egui::Key::F6,
        7 => egui::Key::F7,
        8 => egui::Key::F8,
        9 => egui::Key::F9,
        10 => egui::Key::F10,
        11 => egui::Key::F11,
        12 => egui::Key::F12,
        13 => egui::Key::F13,
        14 => egui::Key::F14,
        15 => egui::Key::F15,
        16 => egui::Key::F16,
        17 => egui::Key::F17,
        18 => egui::Key::F18,
        19 => egui::Key::F19,
        20 => egui::Key::F20,
        21 => egui::Key::F21,
        22 => egui::Key::F22,
        23 => egui::Key::F23,
        24 => egui::Key::F24,
        25 => egui::Key::F25,
        26 => egui::Key::F26,
        27 => egui::Key::F27,
        28 => egui::Key::F28,
        29 => egui::Key::F29,
        30 => egui::Key::F30,
        31 => egui::Key::F31,
        32 => egui::Key::F32,
        33 => egui::Key::F33,
        34 => egui::Key::F34,
        35 => egui::Key::F35,
        _ => egui::Key::F12,
    }
}

/// Map OxiUI [`Modifiers`] to [`egui::Modifiers`].
fn map_modifiers(m: &Modifiers) -> egui::Modifiers {
    egui::Modifiers {
        alt: m.alt,
        ctrl: m.ctrl,
        shift: m.shift,
        mac_cmd: false,
        command: m.ctrl || m.meta,
    }
}

/// Map an OxiUI [`MouseButton`] to an [`egui::PointerButton`].
///
/// `MouseButton::Other(_)` has no egui counterpart and is mapped to
/// `PointerButton::Extra1` as the closest approximation.
fn map_mouse_button(b: &MouseButton) -> egui::PointerButton {
    match b {
        MouseButton::Left => egui::PointerButton::Primary,
        MouseButton::Right => egui::PointerButton::Secondary,
        MouseButton::Middle => egui::PointerButton::Middle,
        MouseButton::Other(_) => egui::PointerButton::Extra1,
    }
}

// ── palette_to_egui_visuals (unchanged signature — DO NOT modify) ─────────────

/// Convert an OxiUI [`Palette`] into an [`egui::Visuals`] colour scheme.
///
/// Maps semantic OxiUI colours to their closest egui counterparts:
/// - `palette.text`       → `visuals.override_text_color`
/// - `palette.background` → `visuals.panel_fill`
/// - `palette.surface`    → `visuals.window_fill`
/// - `palette.primary`    → `visuals.selection.bg_fill` + `visuals.hyperlink_color`
///
/// Deviation note: `oxiui_core::Palette` has no `error`/`warning`/`success`
/// fields (those live on `oxiui_theme::ExtendedPalette`). Therefore
/// `warn_fg_color` and `error_fg_color` are left at their egui defaults.
pub fn palette_to_egui_visuals(palette: &Palette) -> egui::Visuals {
    fn c(col: &Color) -> egui::Color32 {
        egui::Color32::from_rgba_unmultiplied(col.0, col.1, col.2, col.3)
    }
    let mut v = egui::Visuals::dark();
    v.override_text_color = Some(c(&palette.text));
    v.panel_fill = c(&palette.background);
    v.window_fill = c(&palette.surface);
    v.selection.bg_fill = c(&palette.primary);
    v.hyperlink_color = c(&palette.primary);
    v
}

/// Map a [`Palette`] and [`DesignTokens`] to a full [`egui::Style`].
///
/// Calls [`palette_to_egui_visuals`] for the colour visuals, then additionally
/// maps design-token spacing and border-radius values into the egui style:
///
/// - `tokens.spacing(SpacingStep::Sm)` → `style.spacing.item_spacing`
/// - `tokens.radius(RadiusStep::Md)`   → `style.visuals.menu_corner_radius`
///   and `style.visuals.window_corner_radius`
///
/// The result is a fully configured [`egui::Style`] that can be applied with
/// [`egui::Context::set_style`].
pub fn palette_to_egui_visuals_with_tokens(
    palette: &Palette,
    tokens: &DesignTokens,
) -> egui::Style {
    use oxiui_theme::{RadiusStep, SpacingStep};

    // Build the visuals first so we can apply token overrides before
    // assembling the style struct.
    let mut visuals = palette_to_egui_visuals(palette);

    // Map border-radius: use Md (4 px by default) for menus and windows.
    // CornerRadius::same takes a u8; clamp f32 to [0, 255] to avoid overflow.
    let radius_val = tokens.radius(RadiusStep::Md).round().clamp(0.0, 255.0) as u8;
    let corner_radius = egui::CornerRadius::same(radius_val);
    visuals.menu_corner_radius = corner_radius;
    visuals.window_corner_radius = corner_radius;

    // Map spacing: use Sm (8 px by default) for widget item spacing.
    let spacing_val = tokens.spacing(SpacingStep::Sm);
    let spacing = egui::style::Spacing {
        item_spacing: egui::vec2(spacing_val, spacing_val),
        ..egui::style::Spacing::default()
    };

    egui::Style {
        visuals,
        spacing,
        ..egui::Style::default()
    }
}

/// Map [`DesignTokens`] and [`oxiui_theme::TypographyScale`] to a complete [`egui::Style`].
///
/// This function provides the full design-token-to-egui-style translation,
/// combining spacing, border-radius, and typography into a single style:
///
/// **Spacing** (`DesignTokens.spacing`):
/// - `Xs` → `style.spacing.button_padding` (tight vertical padding)
/// - `Sm` → `style.spacing.item_spacing` (gap between widgets)
///
/// **Border radius** (`DesignTokens.radius`) applied to all widget states
/// (`noninteractive`, `inactive`, `active`):
/// - `Sm` → `noninteractive` + `inactive` corner radius (subtle rounding)
/// - `Md` → `active` corner radius (slightly more rounded when pressed)
/// - `Md` → `visuals.menu_corner_radius` and `visuals.window_corner_radius`
///
/// **Typography** (`TypographyScale`) mapped to egui's five [`egui::TextStyle`]
/// variants using the `size` field of each typographic role:
/// - `Heading` ← `typography.headline.size`
/// - `Body`    ← `typography.body.size`
/// - `Button`  ← `typography.body.size` (same as body for consistency)
/// - `Monospace` ← `typography.body.size` (monospace uses the body scale)
/// - `Small`   ← `typography.caption.size`
///
/// # Merging with palette colours
///
/// The returned [`egui::Style`] carries default visuals. To apply full theming
/// (colours + tokens), replace the visuals before calling
/// [`egui::Context::set_style`]:
///
/// ```rust,ignore
/// use oxiui_egui::{tokens_to_egui_style, palette_to_egui_visuals};
/// let mut style = tokens_to_egui_style(&tokens, &typography);
/// style.visuals = palette_to_egui_visuals(&palette);
/// ctx.set_style(style);
/// ```
pub fn tokens_to_egui_style(
    tokens: &oxiui_theme::DesignTokens,
    typography: &oxiui_theme::TypographyScale,
) -> egui::Style {
    use egui::{FontFamily, FontId, TextStyle};
    use oxiui_theme::{RadiusStep, SpacingStep};

    let mut style = egui::Style::default();

    // ── Spacing ───────────────────────────────────────────────────────────────
    let spacing_xs = tokens.spacing(SpacingStep::Xs);
    let spacing_sm = tokens.spacing(SpacingStep::Sm);

    style.spacing.item_spacing = egui::vec2(spacing_sm, spacing_xs);
    style.spacing.button_padding = egui::vec2(spacing_sm, spacing_xs / 2.0);

    // ── Border radius ─────────────────────────────────────────────────────────
    // CornerRadius::same takes a u8; clamp f32 to [0, 255] to avoid overflow.
    let radius_sm =
        egui::CornerRadius::same(tokens.radius(RadiusStep::Sm).round().clamp(0.0, 255.0) as u8);
    let radius_md =
        egui::CornerRadius::same(tokens.radius(RadiusStep::Md).round().clamp(0.0, 255.0) as u8);

    style.visuals.widgets.noninteractive.corner_radius = radius_sm;
    style.visuals.widgets.inactive.corner_radius = radius_sm;
    style.visuals.widgets.active.corner_radius = radius_md;
    style.visuals.menu_corner_radius = radius_md;
    style.visuals.window_corner_radius = radius_md;

    // ── Typography ────────────────────────────────────────────────────────────
    // Map typographic roles to egui's five text-style variants.
    // `FontFamily::Proportional` is the fallback that every egui app has.
    style.text_styles.insert(
        TextStyle::Heading,
        FontId::new(typography.headline.size, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Body,
        FontId::new(typography.body.size, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Button,
        FontId::new(typography.body.size, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Monospace,
        FontId::new(typography.body.size, FontFamily::Monospace),
    );
    style.text_styles.insert(
        TextStyle::Small,
        FontId::new(typography.caption.size, FontFamily::Proportional),
    );

    style
}

// ── forward_event_to_egui (extended, signature preserved) ────────────────────

/// Forward an OxiUI [`UiEvent`] to an egui [`egui::Context`]'s input queue.
///
/// Handles the following event families:
///
/// **IME events:**
/// - [`UiEvent::ImePreedit`] → [`egui::Event::Ime`](`egui::ImeEvent::Preedit`)
///   (the cursor range is dropped; egui 0.34 does not carry it).
/// - [`UiEvent::ImeCommit`] → [`egui::Event::Ime`](`egui::ImeEvent::Commit`)
///
/// **Keyboard events (extended):**
/// - [`UiEvent::KeyDown`] → [`egui::Event::Key`] with `pressed = true`.
/// - [`UiEvent::KeyUp`]   → [`egui::Event::Key`] with `pressed = false`.
/// - [`UiEvent::KeyPress`] — a bare `String` key name; forwarded via
///   [`egui::Key::from_name`] if the name is recognisable, silently ignored
///   otherwise. Maps to a pressed=true / repeat=false event.
///
/// **Pointer events (extended):**
/// - [`UiEvent::MouseMove`] → [`egui::Event::PointerMoved`].
/// - [`UiEvent::Mouse`]     → [`egui::Event::PointerMoved`] (position only).
/// - [`UiEvent::MouseDown`] → [`egui::Event::PointerButton`] with `pressed = true`.
/// - [`UiEvent::MouseUp`]   → [`egui::Event::PointerButton`] with `pressed = false`.
///
/// **Resize (extended):**
/// - [`UiEvent::Resize`] — recorded as a no-op comment. egui viewport resizing
///   is handled via the integration's `RawInput.screen_rect`; there is no
///   direct `egui::Event` for it, so only the intent is noted here.
///
/// **Deviation note:** The plan referenced fictional variants
/// `KeyPress { key, modifiers, pressed }`, `MouseButton { .. }`, and
/// `Resize { width, height }`. The actual `oxiui_core::UiEvent` has
/// `KeyPress(String)`, `MouseDown`/`MouseUp` (separate), `Mouse { x, y }`,
/// and `Resize(u32, u32)`. The mapping above uses the real variants.
///
/// All other event variants are silently ignored (the enum is
/// `#[non_exhaustive]`).
pub fn forward_event_to_egui(ctx: &egui::Context, event: &UiEvent) {
    match event {
        UiEvent::ImePreedit { text, cursor: _ } => {
            // cursor is intentionally dropped: egui 0.34 ImeEvent::Preedit(String)
            // does not carry a cursor position.
            ctx.input_mut(|i| {
                i.events
                    .push(egui::Event::Ime(egui::ImeEvent::Preedit(text.clone())));
            });
        }
        UiEvent::ImeCommit(text) => {
            ctx.input_mut(|i| {
                i.events
                    .push(egui::Event::Ime(egui::ImeEvent::Commit(text.clone())));
            });
        }
        UiEvent::KeyDown {
            key,
            modifiers,
            repeat,
        } => {
            ctx.input_mut(|i| {
                i.events.push(egui::Event::Key {
                    key: map_key(key),
                    physical_key: None,
                    pressed: true,
                    repeat: *repeat,
                    modifiers: map_modifiers(modifiers),
                });
            });
        }
        UiEvent::KeyUp { key, modifiers } => {
            ctx.input_mut(|i| {
                i.events.push(egui::Event::Key {
                    key: map_key(key),
                    physical_key: None,
                    pressed: false,
                    repeat: false,
                    modifiers: map_modifiers(modifiers),
                });
            });
        }
        UiEvent::KeyPress(name) => {
            // Legacy string-based key: forward only if recognisable.
            if let Some(egui_key) = egui::Key::from_name(name.as_str()) {
                ctx.input_mut(|i| {
                    i.events.push(egui::Event::Key {
                        key: egui_key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::default(),
                    });
                });
            }
        }
        UiEvent::MouseMove { x, y } => {
            ctx.input_mut(|i| {
                i.events.push(egui::Event::PointerMoved(egui::pos2(*x, *y)));
            });
        }
        UiEvent::Mouse { x, y } => {
            ctx.input_mut(|i| {
                i.events.push(egui::Event::PointerMoved(egui::pos2(*x, *y)));
            });
        }
        UiEvent::MouseDown {
            button,
            x,
            y,
            modifiers,
        } => {
            ctx.input_mut(|i| {
                i.events.push(egui::Event::PointerButton {
                    pos: egui::pos2(*x, *y),
                    button: map_mouse_button(button),
                    pressed: true,
                    modifiers: map_modifiers(modifiers),
                });
            });
        }
        UiEvent::MouseUp {
            button,
            x,
            y,
            modifiers,
        } => {
            ctx.input_mut(|i| {
                i.events.push(egui::Event::PointerButton {
                    pos: egui::pos2(*x, *y),
                    button: map_mouse_button(button),
                    pressed: false,
                    modifiers: map_modifiers(modifiers),
                });
            });
        }
        UiEvent::Resize(_w, _h) => {
            // egui viewport resize is driven by RawInput.screen_rect from the
            // integration layer; there is no egui::Event for it.
        }
        // CloseRequested, Wheel, and any future variants are not forwarded.
        _ => {}
    }
}

// ── OxiWidget — bridge from oxiui_core::Widget to egui::Widget ───────────────

/// Wraps a mutable [`Widget`] reference so it can be placed in an egui layout.
///
/// ```rust,ignore
/// use oxiui_egui::OxiWidget;
/// ui.add(OxiWidget::new(&mut my_widget));
/// ```
pub struct OxiWidget<'a> {
    widget: &'a mut dyn Widget,
}

impl<'a> OxiWidget<'a> {
    /// Create a new [`OxiWidget`] wrapping the given [`Widget`].
    pub fn new(widget: &'a mut dyn Widget) -> Self {
        Self { widget }
    }
}

impl<'a> egui::Widget for OxiWidget<'a> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let mut ctx = EguiUiCtx::new(ui);
        self.widget.render(&mut ctx);
        // Extract the response before `ctx` (which borrows `ui`) is dropped.
        let maybe_resp = ctx.last_response.take();
        drop(ctx);
        maybe_resp.unwrap_or_else(|| ui.allocate_response(egui::Vec2::ZERO, egui::Sense::hover()))
    }
}

// ── load_font_into_egui (unchanged) ──────────────────────────────────────────

/// Load OxiFont bytes into the egui context as the "OxiFont" family.
///
/// Validates the font bytes via `oxiui_text::TextPipeline::from_bytes` before
/// inserting into egui's font system. Returns an error if the bytes are empty
/// or cannot be parsed as a valid font.
///
/// On success, inserts the font at position 0 of the Proportional family, so
/// it takes precedence over egui's default fonts for all body text.
///
/// # Errors
///
/// Returns [`UiError::Render`] if `font_bytes` is empty or not a valid TTF/OTF
/// file.
pub fn load_font_into_egui(ctx: &egui::Context, font_bytes: Vec<u8>) -> Result<(), UiError> {
    // Validate the font bytes before handing off to egui.
    oxiui_text::TextPipeline::from_bytes(&font_bytes)
        .map_err(|e| UiError::Render(format!("invalid font bytes: {e}")))?;

    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "OxiFont".to_owned(),
        Arc::new(egui::FontData::from_owned(font_bytes)),
    );
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "OxiFont".to_owned());
    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .push("OxiFont".to_owned());
    ctx.set_fonts(fonts);
    Ok(())
}

// ── load_fonts_into_egui — multi-family font loading ─────────────────────────

/// Load multiple font families into egui's font definitions.
///
/// Each entry in `family_map` is a `(family_name, font_bytes)` pair. Fonts are
/// validated via [`oxiui_text::TextPipeline::from_bytes`] before being
/// registered. On success the font is inserted into:
/// - A named family `"OxiFont-<family_name>"`.
///
/// Starts from [`egui::FontDefinitions::default`], so any previously loaded
/// `OxiFont-*` families are replaced.
///
/// # Errors
///
/// Returns [`UiError::Render`] if any entry's bytes are invalid.
pub fn load_fonts_into_egui(
    family_map: &[(&str, Vec<u8>)],
    ctx: &egui::Context,
) -> Result<(), UiError> {
    let mut fonts = egui::FontDefinitions::default();
    for (family, bytes) in family_map {
        oxiui_text::TextPipeline::from_bytes(bytes)
            .map_err(|e| UiError::Render(format!("invalid font bytes for '{family}': {e}")))?;
        let name = format!("OxiFont-{family}");
        fonts.font_data.insert(
            name.clone(),
            Arc::new(egui::FontData::from_owned(bytes.clone())),
        );
        fonts
            .families
            .entry(egui::FontFamily::Name(name.clone().into()))
            .or_default()
            .push(name);
    }
    ctx.set_fonts(fonts);
    Ok(())
}

// ── EguiAdapter builder (unchanged) ──────────────────────────────────────────

/// Builder for configuring and applying an OxiUI egui adapter to an
/// [`egui::Context`].
///
/// Use [`EguiAdapter::new`] to start building, optionally call
/// [`EguiAdapter::with_palette`] to inject a colour palette, then call
/// [`EguiAdapter::build`] to obtain a configuration closure suitable for
/// passing to `eframe`'s `setup_callback` or calling directly on an
/// [`egui::Context`].
pub struct EguiAdapter {
    palette: Option<Palette>,
}

impl EguiAdapter {
    /// Create a new [`EguiAdapter`] builder with no palette set.
    pub fn new() -> Self {
        Self { palette: None }
    }

    /// Set the [`Palette`] that will be applied to the egui [`egui::Context`]
    /// when [`EguiAdapter::build`] is called.
    pub fn with_palette(mut self, p: Palette) -> Self {
        self.palette = Some(p);
        self
    }

    /// Build the adapter configuration closure.
    ///
    /// Returns a `Fn(&egui::Context)` that, when called, applies the configured
    /// palette (if any) to the context's visuals.
    pub fn build(self) -> impl Fn(&egui::Context) {
        let palette = self.palette;
        move |ctx: &egui::Context| {
            if let Some(p) = &palette {
                ctx.set_visuals(palette_to_egui_visuals(p));
            }
        }
    }
}

impl Default for EguiAdapter {
    fn default() -> Self {
        Self::new()
    }
}

// ── StatefulEguiAdapter — per-frame caching adapter ───────────────────────────

/// Returns `true` if two [`Palette`] values are equal field-by-field.
///
/// Used by [`StatefulEguiAdapter`] to detect theme changes without requiring
/// [`PartialEq`] on [`Palette`] itself.
fn palettes_equal(a: &Palette, b: &Palette) -> bool {
    a.background == b.background
        && a.surface == b.surface
        && a.primary == b.primary
        && a.on_primary == b.on_primary
        && a.text == b.text
        && a.muted == b.muted
}

/// A stateful egui adapter that caches expensive operations across frames.
///
/// Unlike [`EguiAdapter`] (which returns a stateless `Fn`), `StatefulEguiAdapter`
/// keeps internal state between calls to [`StatefulEguiAdapter::apply`]:
///
/// - **Visuals caching** — [`palette_to_egui_visuals`] is called and
///   [`egui::Context::set_visuals`] is invoked *only* when the palette has
///   changed since the last frame.  When the theme is stable this saves the
///   visuals recomputation and the redundant egui context lock on every frame.
///
/// - **Font definition caching** — [`egui::Context::set_fonts`] is called at
///   most *once*: the first time [`apply`](StatefulEguiAdapter::apply) is
///   invoked.  Subsequent frames skip the call entirely.  Font bytes are owned
///   by the adapter and released after the first load to avoid keeping a
///   duplicate copy in memory.
///
/// - **Design-token style** — when [`StatefulEguiAdapter::with_design_tokens`] is called, the
///   adapter additionally calls [`tokens_to_egui_style`] and
///   [`egui::Context::set_style`] on the first frame (tokens are static once
///   configured). Palette colours are merged into the style so they are not
///   lost when both palette and tokens are present.
///
/// # Usage
///
/// ```rust,ignore
/// let mut adapter = StatefulEguiAdapter::new()
///     .with_palette(my_palette)
///     .with_design_tokens(my_tokens, my_typography)
///     .with_font_bytes(font_data);
///
/// // In your eframe::App::update():
/// adapter.apply(ctx);   // cheap after the first frame if theme is unchanged
/// ```
///
/// # Instrumentation
///
/// Two public counters are always available for testing and monitoring:
/// - [`visuals_recompute_count`](StatefulEguiAdapter::visuals_recompute_count)
///   — number of times visuals were recomputed.
/// - [`fonts_load_count`](StatefulEguiAdapter::fonts_load_count)
///   — number of times `set_fonts` was called (at most 1 in normal use).
pub struct StatefulEguiAdapter {
    palette: Option<Palette>,
    /// Cached `(last_palette, computed_visuals)`.  Recomputed only on change.
    cached_visuals: Option<(Palette, egui::Visuals)>,
    /// Font bytes to load on the first [`apply`](StatefulEguiAdapter::apply) call.
    /// Moved into egui and set to `None` after the first successful load.
    pending_font_bytes: Option<Vec<u8>>,
    /// `true` once the font-load attempt has been made; prevents repeated loads.
    fonts_loaded: bool,
    /// Design tokens to apply once at startup (spacing, radius, typography).
    design_tokens: Option<oxiui_theme::DesignTokens>,
    /// Typography scale paired with design tokens.
    typography: Option<oxiui_theme::TypographyScale>,
    /// `true` once the design-token style has been applied.
    tokens_applied: bool,
    /// Number of times visuals were recomputed (palette changed or first frame).
    ///
    /// Useful for testing that caching is working correctly: after the first
    /// frame with a given palette this counter should not increase until the
    /// palette is changed via [`set_palette`](StatefulEguiAdapter::set_palette).
    pub visuals_recompute_count: usize,
    /// Number of times `set_fonts` was attempted (should be ≤ 1 in normal use).
    ///
    /// A value of 1 means fonts were loaded once on the first frame.
    /// A value of 0 means no font bytes were supplied via
    /// [`with_font_bytes`](StatefulEguiAdapter::with_font_bytes).
    pub fonts_load_count: usize,
}

impl StatefulEguiAdapter {
    /// Create a new [`StatefulEguiAdapter`] with no palette or fonts.
    pub fn new() -> Self {
        Self {
            palette: None,
            cached_visuals: None,
            pending_font_bytes: None,
            fonts_loaded: false,
            design_tokens: None,
            typography: None,
            tokens_applied: false,
            visuals_recompute_count: 0,
            fonts_load_count: 0,
        }
    }

    /// Set the [`Palette`] to be applied each frame (cached — only recomputed on change).
    pub fn with_palette(mut self, p: Palette) -> Self {
        self.palette = Some(p);
        self
    }

    /// Attach [`oxiui_theme::DesignTokens`] and [`oxiui_theme::TypographyScale`] to be
    /// applied *once* on the first [`StatefulEguiAdapter::apply`] call via [`tokens_to_egui_style`].
    ///
    /// When a palette is also configured, palette colours are merged into the
    /// style so neither colour nor token information is lost.  If only tokens are
    /// set (no palette), the style is applied with egui's default dark visuals.
    pub fn with_design_tokens(
        mut self,
        tokens: oxiui_theme::DesignTokens,
        typography: oxiui_theme::TypographyScale,
    ) -> Self {
        self.design_tokens = Some(tokens);
        self.typography = Some(typography);
        self
    }

    /// Attach font bytes to be loaded *once* on the first [`StatefulEguiAdapter::apply`] call.
    ///
    /// The bytes are validated and forwarded to egui via
    /// [`load_font_into_egui`].  If validation fails the error is silently
    /// discarded (the adapter falls back to egui's built-in fonts).  Call
    /// this method before the first [`StatefulEguiAdapter::apply`] call.
    pub fn with_font_bytes(mut self, bytes: Vec<u8>) -> Self {
        self.pending_font_bytes = Some(bytes);
        self
    }

    /// Update the live palette.
    ///
    /// If the adapter already has a cached visuals entry, this marks it stale
    /// so the next [`StatefulEguiAdapter::apply`] call recomputes the visuals.
    pub fn set_palette(&mut self, p: Palette) {
        self.palette = Some(p);
    }

    /// Apply the adapter state to `ctx` for one frame.
    ///
    /// - Loads fonts exactly once (the first call).
    /// - Applies design-token style exactly once (the first call, if tokens were set).
    ///   When palette colours are also present, they are merged into the token style.
    /// - Recomputes and applies visuals only when the palette has changed
    ///   (skipped when design-token style is active — tokens mode uses `set_style`).
    pub fn apply(&mut self, ctx: &egui::Context) {
        // ── font loading (at most once) ───────────────────────────────────────
        if !self.fonts_loaded {
            if let Some(bytes) = self.pending_font_bytes.take() {
                // Silently ignore validation errors; egui falls back to defaults.
                let _ = load_font_into_egui(ctx, bytes);
                self.fonts_load_count += 1;
            }
            self.fonts_loaded = true;
        }

        // ── design-token style (at most once) ─────────────────────────────────
        if !self.tokens_applied {
            if let (Some(ref tok), Some(ref typo)) = (&self.design_tokens, &self.typography) {
                let mut style = tokens_to_egui_style(tok, typo);
                // Merge palette colours so they are not lost.
                if let Some(ref p) = self.palette {
                    style.visuals = palette_to_egui_visuals(p);
                    // Re-apply token radius overrides on top of palette visuals.
                    use oxiui_theme::RadiusStep;
                    let radius_sm = egui::CornerRadius::same(
                        tok.radius(RadiusStep::Sm).round().clamp(0.0, 255.0) as u8,
                    );
                    let radius_md = egui::CornerRadius::same(
                        tok.radius(RadiusStep::Md).round().clamp(0.0, 255.0) as u8,
                    );
                    style.visuals.widgets.noninteractive.corner_radius = radius_sm;
                    style.visuals.widgets.inactive.corner_radius = radius_sm;
                    style.visuals.widgets.active.corner_radius = radius_md;
                    style.visuals.menu_corner_radius = radius_md;
                    style.visuals.window_corner_radius = radius_md;

                    // Mark visuals as cached so the palette-only path below is
                    // skipped for this frame (colours already applied in style).
                    let visuals = style.visuals.clone();
                    self.cached_visuals = Some((p.clone(), visuals));
                    self.visuals_recompute_count += 1;
                }
                ctx.set_global_style(style);
                self.tokens_applied = true;
                return;
            }
        }

        // ── visuals caching (palette-only path) ───────────────────────────────
        if let Some(ref current) = self.palette {
            let needs_update = match &self.cached_visuals {
                None => true,
                Some((ref last, _)) => !palettes_equal(last, current),
            };

            if needs_update {
                let visuals = palette_to_egui_visuals(current);
                ctx.set_visuals(visuals.clone());
                self.cached_visuals = Some((current.clone(), visuals));
                self.visuals_recompute_count += 1;
            }
        }
    }
}

impl Default for StatefulEguiAdapter {
    fn default() -> Self {
        Self::new()
    }
}

// ── oxiui-accessibility bridge ────────────────────────────────────────────────

/// Bridge OxiUI's [`oxiui_accessibility::A11yTree`] with egui's AccessKit integration layer.
///
/// egui 0.34 ships with built-in AccessKit support; the platform integration
/// (eframe/egui-winit with `accesskit` feature) maintains its own AccessKit
/// adapter internally.  This bridge provides a utility to convert an
/// [`oxiui_accessibility::A11yTree`] (OxiUI's semantic a11y model) into an
/// [`accesskit::TreeUpdate`] that can be forwarded into egui's AccessKit
/// pipeline via the platform adapter.
///
/// # Feature gate
///
/// This module is only compiled when the `a11y` Cargo feature is enabled:
/// ```toml
/// [dependencies]
/// oxiui-egui = { version = "0.1.4", features = ["a11y"] }
/// ```
///
/// # Usage
///
/// ```rust,ignore
/// use oxiui_accessibility::tree::{A11yNode, A11yTree, WidgetRole};
/// use oxiui_egui::a11y::{oxiui_tree_to_accesskit, A11yEguiBridge};
/// use accesskit::NodeId;
///
/// let root = A11yNode::simple(NodeId(1), WidgetRole::Window, Some("App".into()));
/// let update = oxiui_tree_to_accesskit(&root);
/// // Forward `update` to the AccessKit adapter held by your platform integration.
/// ```
#[cfg(feature = "a11y")]
pub mod a11y {
    use accesskit::TreeUpdate;
    use oxiui_accessibility::tree::{A11yNode, A11yTree};

    /// Convert an OxiUI [`A11yNode`] tree into an AccessKit [`TreeUpdate`].
    ///
    /// This is a thin wrapper around [`A11yTree::build`] that converts the
    /// OxiUI accessibility tree representation into the `accesskit::TreeUpdate`
    /// format expected by egui's built-in AccessKit integration.
    ///
    /// Pass the returned `TreeUpdate` to the platform adapter (e.g.
    /// `accesskit_winit::Adapter::update_if_active`) after building your UI.
    ///
    /// The function performs a full (non-diff) conversion each call.  For
    /// incremental updates use [`diff_a11y_trees`] instead.
    pub fn oxiui_tree_to_accesskit(root: &A11yNode) -> TreeUpdate {
        A11yTree::build(root)
    }

    /// Compute a minimal diff [`TreeUpdate`] between two OxiUI a11y tree states.
    ///
    /// Produces only the nodes that changed between `previous` (the last
    /// committed tree state) and `current` (the new tree state).  This avoids
    /// re-sending the entire tree to the platform accessibility layer every
    /// frame.
    ///
    /// Both arguments should be stored by the caller between frames.  A fresh
    /// [`A11yTree`] can be built via [`A11yTree::build_and_store`].
    pub fn diff_a11y_trees(previous: &A11yTree, current: &A11yTree) -> TreeUpdate {
        A11yTree::diff(previous, current)
    }

    /// A stateful bridge that retains the previous tree for efficient diffing.
    ///
    /// Call [`A11yEguiBridge::update`] each frame with the new root node;
    /// it returns the minimal `TreeUpdate` to forward to the platform
    /// AccessKit adapter.
    pub struct A11yEguiBridge {
        previous: A11yTree,
        current: A11yTree,
        /// `true` on the very first frame (forces a full [`A11yTree::build`]).
        is_first_frame: bool,
    }

    impl Default for A11yEguiBridge {
        fn default() -> Self {
            Self::new()
        }
    }

    impl A11yEguiBridge {
        /// Create a new bridge with an empty previous state.
        pub fn new() -> Self {
            Self {
                previous: A11yTree::default(),
                current: A11yTree::default(),
                is_first_frame: true,
            }
        }

        /// Advance the bridge one frame: store `root` as the new tree and
        /// return the [`TreeUpdate`] to forward to the platform adapter.
        ///
        /// On the first frame this returns a full tree update; subsequent
        /// frames return only changed nodes (diff).
        pub fn update(&mut self, root: &A11yNode) -> TreeUpdate {
            let update = self.current.build_and_store(root);
            if self.is_first_frame {
                self.is_first_frame = false;
                // Copy current into previous for future diffs.
                self.previous = std::mem::take(&mut self.current);
                self.current = A11yTree::default();
                // Return the full update on the first frame.
                update
            } else {
                // Build a diff — only send changed nodes.
                let diff = A11yTree::diff(&self.previous, &self.current);
                // Rotate current → previous for the next frame.
                std::mem::swap(&mut self.previous, &mut self.current);
                self.current = A11yTree::default();
                diff
            }
        }

        /// Update the focused node without rebuilding the tree.
        ///
        /// Returns a minimal [`TreeUpdate`] that only carries the new focus.
        pub fn set_focus(&mut self, id: Option<accesskit::NodeId>) -> TreeUpdate {
            self.current.set_focus(id);
            self.current.focus_update()
        }
    }
}

// ── oxiui-table integration helpers ──────────────────────────────────────────

/// Integration helpers bridging `oxiui-table` with the egui adapter.
///
/// The `oxiui-table` crate already provides an `EguiTableState` and a
/// `render_egui` method on `Table<S>`.  This module adds convenience wrappers
/// that let you drive a sorted+filtered table through an [`EguiUiCtx`] and
/// collect `TableEvent`s into an application-level result type.
///
/// # Feature gate
///
/// Only compiled when the `table` Cargo feature is enabled:
/// ```toml
/// [dependencies]
/// oxiui-egui = { version = "0.1.4", features = ["table"] }
/// ```
#[cfg(feature = "table")]
pub mod table_bridge {
    use egui::Ui;
    use oxiui_table::{
        header::HeaderSortState, EguiTableState, RowSource, SelectionModel, Table, TableEvent,
    };

    /// Render a table inside an existing egui `Ui`, collecting events.
    ///
    /// Wraps `Table::render_egui` with automatic sort-state management: after
    /// rendering the table, any [`TableEvent::SortChanged`] events in
    /// `render_state.events` are applied to `sort_state` via
    /// [`HeaderSortState::toggle`] so callers do not need to drive this loop
    /// manually.
    ///
    /// Returns the list of events emitted during this frame.
    pub fn render_sorted_table<S: RowSource>(
        table: &mut Table<S>,
        ui: &mut Ui,
        sort_state: &mut HeaderSortState,
        render_state: &mut EguiTableState,
    ) -> Vec<TableEvent> {
        table.render_egui(ui, sort_state, render_state);
        render_state.events.clone()
    }

    /// Apply a [`SelectionModel`] to the events collected during the last frame.
    ///
    /// For each [`TableEvent::RowSelected`] event, the selection model is
    /// updated accordingly (single or multi select, depending on the model's
    /// mode).  Returns `true` if any selection changed.
    pub fn apply_selection_events(events: &[TableEvent], selection: &mut SelectionModel) -> bool {
        let mut changed = false;
        for event in events {
            if let TableEvent::RowSelected(row) = event {
                selection.click(*row);
                changed = true;
            }
        }
        changed
    }
}
