//! Logging / tracing integration for OxiUI.
//!
//! Provides a one-call `init_logging` helper that installs a
//! `tracing-subscriber` fmt subscriber respecting the `RUST_LOG` environment
//! variable.  The subscriber is configured for OxiUI-specific spans:
//!
//! | Span name     | Source          |
//! |---------------|-----------------|
//! | `oxiui::frame`| facade render loop |
//! | `oxiui::layout`| layout phase   |
//! | `oxiui::paint` | paint command buffer |
//! | `oxiui::event` | event dispatch  |
//!
//! # Usage
//!
//! ```no_run
//! fn main() {
//!     oxiui::logging::init_logging(oxiui::logging::LogLevel::Info);
//!     // App launch ...
//! }
//! ```
//!
//! Set `RUST_LOG=oxiui=debug` for detailed diagnostics; `RUST_LOG=oxiui=trace`
//! for per-frame tracing (generates a lot of output).

use std::sync::OnceLock;

// ── LogLevel ─────────────────────────────────────────────────────────────────

/// Minimum log level for the subscriber installed by [`init_logging`].
///
/// Maps directly to `tracing`'s level hierarchy.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    /// Error conditions only.
    Error = 0,
    /// Warnings and errors.
    Warn = 1,
    /// Informational messages, warnings, errors (default).
    #[default]
    Info = 2,
    /// Debug messages and above.
    Debug = 3,
    /// Full trace output including per-frame spans (verbose).
    Trace = 4,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            LogLevel::Error => "error",
            LogLevel::Warn => "warn",
            LogLevel::Info => "info",
            LogLevel::Debug => "debug",
            LogLevel::Trace => "trace",
        };
        write!(f, "{s}")
    }
}

impl From<LogLevel> for tracing::Level {
    fn from(level: LogLevel) -> tracing::Level {
        match level {
            LogLevel::Error => tracing::Level::ERROR,
            LogLevel::Warn => tracing::Level::WARN,
            LogLevel::Info => tracing::Level::INFO,
            LogLevel::Debug => tracing::Level::DEBUG,
            LogLevel::Trace => tracing::Level::TRACE,
        }
    }
}

// ── Init guard ───────────────────────────────────────────────────────────────

/// Global guard ensuring the subscriber is only installed once.
static INIT: OnceLock<()> = OnceLock::new();

// ── LoggingConfig ─────────────────────────────────────────────────────────────

/// Configuration for the logging subscriber.
#[derive(Clone, Debug)]
pub struct LoggingConfig {
    /// Minimum log level (may be overridden by `RUST_LOG`).
    pub level: LogLevel,
    /// Whether to emit ANSI colour codes in terminal output.
    pub ansi_colors: bool,
    /// Whether to include the source-code file/line in log output.
    pub with_file: bool,
    /// Whether to include the thread ID in log output.
    pub with_thread_ids: bool,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        LoggingConfig {
            level: LogLevel::Info,
            ansi_colors: true,
            with_file: false,
            with_thread_ids: false,
        }
    }
}

impl LoggingConfig {
    /// Create a config with the given level and all other settings defaulted.
    pub fn new(level: LogLevel) -> Self {
        LoggingConfig {
            level,
            ..Default::default()
        }
    }

    /// Disable ANSI colour codes (useful for log files or CI without terminal).
    pub fn no_ansi(mut self) -> Self {
        self.ansi_colors = false;
        self
    }

    /// Include source file + line in every log line.
    pub fn with_file(mut self) -> Self {
        self.with_file = true;
        self
    }

    /// Include thread IDs in every log line.
    pub fn with_thread_ids(mut self) -> Self {
        self.with_thread_ids = true;
        self
    }
}

// ── init_logging ─────────────────────────────────────────────────────────────

/// Install the tracing subscriber using the given minimum [`LogLevel`].
///
/// The `RUST_LOG` environment variable takes precedence over `level` when set,
/// enabling fine-grained control without recompiling.  If a subscriber is
/// already installed (e.g. by the application itself) this function returns
/// without error.
///
/// This function is idempotent: calling it more than once is safe.
pub fn init_logging(level: LogLevel) {
    init_with_config(LoggingConfig::new(level));
}

/// Install the tracing subscriber using a full [`LoggingConfig`].
///
/// Respects `RUST_LOG` if set.  Idempotent.
pub fn init_with_config(config: LoggingConfig) {
    INIT.get_or_init(|| {
        use tracing_subscriber::{fmt, EnvFilter};

        let filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new(config.level.to_string()));

        let subscriber = fmt::Subscriber::builder()
            .with_env_filter(filter)
            .with_ansi(config.ansi_colors)
            .with_file(config.with_file)
            .with_thread_ids(config.with_thread_ids);

        // Install the subscriber; ignore errors (another subscriber may be set).
        let _ = subscriber.try_init();
    });
}

// ── Convenience span macros (re-exported from tracing) ───────────────────────

/// Emit an OxiUI frame span (visible at `trace` level).
#[macro_export]
#[cfg(feature = "tracing")]
macro_rules! frame_span {
    ($label:expr) => {
        tracing::trace_span!("oxiui::frame", label = $label)
    };
}

/// Emit an OxiUI layout span (visible at `debug` level).
#[macro_export]
#[cfg(feature = "tracing")]
macro_rules! layout_span {
    ($label:expr) => {
        tracing::debug_span!("oxiui::layout", label = $label)
    };
}

/// Emit an OxiUI paint span (visible at `debug` level).
#[macro_export]
#[cfg(feature = "tracing")]
macro_rules! paint_span {
    ($label:expr) => {
        tracing::debug_span!("oxiui::paint", label = $label)
    };
}

/// Emit an OxiUI event span (visible at `debug` level).
#[macro_export]
#[cfg(feature = "tracing")]
macro_rules! event_span {
    ($label:expr) => {
        tracing::debug_span!("oxiui::event", label = $label)
    };
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_level_ordering() {
        assert!(LogLevel::Error < LogLevel::Warn);
        assert!(LogLevel::Warn < LogLevel::Info);
        assert!(LogLevel::Info < LogLevel::Debug);
        assert!(LogLevel::Debug < LogLevel::Trace);
    }

    #[test]
    fn log_level_display() {
        assert_eq!(LogLevel::Error.to_string(), "error");
        assert_eq!(LogLevel::Info.to_string(), "info");
        assert_eq!(LogLevel::Trace.to_string(), "trace");
    }

    #[test]
    fn log_level_default_is_info() {
        assert_eq!(LogLevel::default(), LogLevel::Info);
    }

    #[test]
    fn logging_config_builder() {
        let cfg = LoggingConfig::new(LogLevel::Debug)
            .no_ansi()
            .with_file()
            .with_thread_ids();
        assert_eq!(cfg.level, LogLevel::Debug);
        assert!(!cfg.ansi_colors);
        assert!(cfg.with_file);
        assert!(cfg.with_thread_ids);
    }

    #[test]
    fn init_logging_is_idempotent() {
        // Calling twice should not panic.
        init_logging(LogLevel::Info);
        init_logging(LogLevel::Debug); // Second call is a no-op.
    }

    #[test]
    fn init_with_config_is_idempotent() {
        let cfg = LoggingConfig::default();
        init_with_config(cfg.clone());
        init_with_config(cfg);
    }

    #[test]
    fn log_level_into_tracing_level() {
        let _: tracing::Level = LogLevel::Info.into();
        let _: tracing::Level = LogLevel::Trace.into();
    }
}
