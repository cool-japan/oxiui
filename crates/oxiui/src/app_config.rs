//! `AppConfig` — window configuration builder for `App`.

/// Configuration for building an `App`.
///
/// Use the builder methods to configure the window, then pass to `App::new`.
///
/// # Example
///
/// ```rust,no_run
/// use oxiui::AppConfig;
/// let config = AppConfig::new()
///     .title("My App")
///     .size(1024.0, 768.0)
///     .resizable(true)
///     .decorations(true)
///     .transparent(false);
/// ```
#[derive(Debug, Clone)]
pub struct AppConfig {
    /// Window title.
    pub title: String,
    /// Initial window width in logical pixels (0.0 → use default).
    pub width: f32,
    /// Initial window height in logical pixels (0.0 → use default).
    pub height: f32,
    /// Whether the window can be resized by the user.
    pub resizable: bool,
    /// Minimum window size in logical pixels `(width, height)`.
    pub min_size: Option<(f32, f32)>,
    /// Maximum window size in logical pixels `(width, height)`.
    pub max_size: Option<(f32, f32)>,
    /// Whether the window has OS-drawn decorations (title bar, borders).
    ///
    /// Defaults to `true`.
    pub decorations: bool,
    /// Whether the window background is transparent.
    ///
    /// Defaults to `false`.
    pub transparent: bool,
    /// Whether the window is always shown above other windows.
    ///
    /// Defaults to `false`.
    pub always_on_top: bool,
    /// Optional PNG/ICO bytes for the window icon.
    ///
    /// Stored as raw bytes; decoded to RGBA when wiring into the active
    /// rendering backend.  The `png` crate is required for decoding and is
    /// available whenever the `egui` or `software` Cargo feature is enabled.
    /// When neither feature is active the bytes are stored but decoding is a
    /// no-op (the icon is silently omitted).
    pub icon: Option<Vec<u8>>,
    /// Initial window position in logical pixels `(x, y)` from the top-left
    /// of the primary monitor.
    pub position: Option<(f32, f32)>,
    /// Extra font families to load at startup.
    ///
    /// Each entry is `(family_name, raw_font_bytes)`. Passed to the active
    /// backend's font loading path when `App::run` begins (egui path only
    /// in this release; iced font loading is deferred).
    pub extra_fonts: Vec<(String, Vec<u8>)>,
    /// Optional design-token override (spacing / radius / elevation scales).
    ///
    /// When `None`, backends use the theme's default tokens. Set via
    /// `App::with_design_tokens`.
    pub design_tokens: Option<oxiui_theme::DesignTokens>,
    /// Optional typography-scale override.
    ///
    /// When `None`, backends use the theme's default scale. Set via
    /// `App::with_typography`.
    pub typography: Option<oxiui_theme::TypographyScale>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl AppConfig {
    /// Create a new [`AppConfig`] with default settings.
    pub fn new() -> Self {
        Self {
            title: String::new(),
            width: 800.0,
            height: 600.0,
            resizable: true,
            min_size: None,
            max_size: None,
            decorations: true,
            transparent: false,
            always_on_top: false,
            icon: None,
            position: None,
            extra_fonts: Vec::new(),
            design_tokens: None,
            typography: None,
        }
    }

    /// Set the window title.
    pub fn title(mut self, t: impl Into<String>) -> Self {
        self.title = t.into();
        self
    }

    /// Set the initial window size in logical pixels.
    pub fn size(mut self, w: f32, h: f32) -> Self {
        self.width = w;
        self.height = h;
        self
    }

    /// Set whether the window can be resized.
    pub fn resizable(mut self, r: bool) -> Self {
        self.resizable = r;
        self
    }

    /// Set the minimum window size in logical pixels.
    pub fn min_size(mut self, w: f32, h: f32) -> Self {
        self.min_size = Some((w, h));
        self
    }

    /// Set the maximum window size in logical pixels.
    pub fn max_size(mut self, w: f32, h: f32) -> Self {
        self.max_size = Some((w, h));
        self
    }

    /// Set whether the window has OS-drawn decorations (title bar, borders).
    pub fn decorations(mut self, d: bool) -> Self {
        self.decorations = d;
        self
    }

    /// Set whether the window background is transparent.
    pub fn transparent(mut self, t: bool) -> Self {
        self.transparent = t;
        self
    }

    /// Set whether the window is always shown above other windows.
    pub fn always_on_top(mut self, a: bool) -> Self {
        self.always_on_top = a;
        self
    }

    /// Set the window icon from raw PNG/ICO bytes.
    pub fn icon(mut self, bytes: Vec<u8>) -> Self {
        self.icon = Some(bytes);
        self
    }

    /// Set the initial window position in logical pixels from top-left of primary monitor.
    pub fn position(mut self, x: f32, y: f32) -> Self {
        self.position = Some((x, y));
        self
    }
}
