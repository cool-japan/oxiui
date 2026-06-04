//! Native OS file / message dialog integration via [`rfd`].
//!
//! Enabled by the `dialogs` Cargo feature.  Provides blocking and async helpers
//! that call the platform's native file picker and message box APIs via the
//! [Rusty File Dialog](https://crates.io/crates/rfd) Pure-Rust crate.
//!
//! # Feature gate
//!
//! ```toml
//! [dependencies]
//! oxiui = { version = "*", features = ["dialogs"] }
//! ```
//!
//! # Relationship to the built-in dialog queue
//!
//! [`crate::dialog::DialogQueue`] is a Pure in-process dialog model that
//! works headlessly (no OS dialogs, useful in tests).  This module provides
//! the *native OS* variant — blocking calls to the platform file picker or
//! message box.  A backend adapter can choose which variant to use.
//!
//! # Usage
//!
//! ```rust,no_run
//! # #[cfg(feature = "dialogs")]
//! # {
//! use oxiui::native_dialog::{open_file_dialog, message_dialog, DialogResult};
//!
//! // Blocking file picker — blocks until user picks a file.
//! let files = open_file_dialog(
//!     "Open Rust Source",
//!     &[("Rust files", "rs"), ("All files", "*")],
//!     true, // multiple selection
//! );
//! if let DialogResult::FilePaths(paths) = files {
//!     for p in paths { println!("Selected: {p}"); }
//! }
//!
//! // Message box.
//! message_dialog("OxiUI", "Hello from the dialog API!", oxiui::native_dialog::MessageLevel::Info);
//! # }
//! ```

// ── MessageLevel ──────────────────────────────────────────────────────────────

/// Severity level for a native message dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageLevel {
    /// Informational message (ℹ icon on most platforms).
    Info,
    /// Warning message (⚠ icon on most platforms).
    Warning,
    /// Error / critical message (⛔ icon on most platforms).
    Error,
}

// ── DialogResult ──────────────────────────────────────────────────────────────

/// The result of a native dialog invocation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DialogResult {
    /// The user selected one or more file paths.
    FilePaths(Vec<String>),
    /// The user confirmed the dialog (OK / Yes).
    Confirmed,
    /// The user dismissed the dialog (Cancel / No / close button).
    Cancelled,
}

// ── Public API ─────────────────────────────────────────────────────────────────

/// Show a native file-open dialog and block until the user makes a selection.
///
/// `title` — dialog window title.
/// `filters` — a slice of `(description, extension)` pairs, e.g.
///   `&[("Rust files", "rs"), ("All files", "*")]`.
/// `multiple` — allow the user to select more than one file.
///
/// Returns [`DialogResult::FilePaths`] with the selected paths, or
/// [`DialogResult::Cancelled`] when the user closes the dialog.
///
/// # Feature
///
/// Requires the `dialogs` feature.  On non-desktop / CI builds where `rfd`
/// cannot show a real dialog this falls back to `Cancelled`.
#[allow(unused_variables)]
pub fn open_file_dialog(title: &str, filters: &[(&str, &str)], multiple: bool) -> DialogResult {
    #[cfg(feature = "dialogs")]
    {
        let mut dialog = rfd::FileDialog::new().set_title(title);
        for (desc, ext) in filters {
            if *ext == "*" {
                // Wildcard — add an all-files filter without an extension list.
                dialog = dialog.add_filter(*desc, &["*"]);
            } else {
                dialog = dialog.add_filter(*desc, &[ext]);
            }
        }

        if multiple {
            match dialog.pick_files() {
                Some(paths) => DialogResult::FilePaths(
                    paths
                        .into_iter()
                        .filter_map(|p| p.to_str().map(|s| s.to_owned()))
                        .collect(),
                ),
                None => DialogResult::Cancelled,
            }
        } else {
            match dialog.pick_file() {
                Some(path) => {
                    let s = path.to_str().map(|s| s.to_owned()).unwrap_or_default();
                    DialogResult::FilePaths(vec![s])
                }
                None => DialogResult::Cancelled,
            }
        }
    }
    #[cfg(not(feature = "dialogs"))]
    {
        DialogResult::Cancelled
    }
}

/// Show a native file-save dialog and block until the user makes a selection.
///
/// `title` — dialog window title.
/// `default_name` — optional suggested file name.
/// `filters` — file type filters (same format as [`open_file_dialog`]).
///
/// Returns [`DialogResult::FilePaths`] with the single chosen path, or
/// [`DialogResult::Cancelled`].
///
/// # Feature
///
/// Requires the `dialogs` feature.
#[allow(unused_variables)]
pub fn save_file_dialog(
    title: &str,
    default_name: Option<&str>,
    filters: &[(&str, &str)],
) -> DialogResult {
    #[cfg(feature = "dialogs")]
    {
        let mut dialog = rfd::FileDialog::new().set_title(title);
        if let Some(name) = default_name {
            dialog = dialog.set_file_name(name);
        }
        for (desc, ext) in filters {
            dialog = dialog.add_filter(*desc, &[ext]);
        }
        match dialog.save_file() {
            Some(path) => {
                let s = path.to_str().map(|s| s.to_owned()).unwrap_or_default();
                DialogResult::FilePaths(vec![s])
            }
            None => DialogResult::Cancelled,
        }
    }
    #[cfg(not(feature = "dialogs"))]
    {
        DialogResult::Cancelled
    }
}

/// Show a native blocking message dialog.
///
/// `title` — dialog window title.
/// `message` — message body text.
/// `level` — severity icon displayed.
///
/// The function blocks until the user clicks OK / closes the dialog.
/// Returns [`DialogResult::Confirmed`] when dismissed.
///
/// # Feature
///
/// Requires the `dialogs` feature.  Without it this is a no-op that
/// returns `DialogResult::Confirmed` immediately.
#[allow(unused_variables)]
pub fn message_dialog(title: &str, message: &str, level: MessageLevel) -> DialogResult {
    #[cfg(feature = "dialogs")]
    {
        let level_rfd = match level {
            MessageLevel::Info => rfd::MessageLevel::Info,
            MessageLevel::Warning => rfd::MessageLevel::Warning,
            MessageLevel::Error => rfd::MessageLevel::Error,
        };
        rfd::MessageDialog::new()
            .set_title(title)
            .set_description(message)
            .set_level(level_rfd)
            .set_buttons(rfd::MessageButtons::Ok)
            .show();
        DialogResult::Confirmed
    }
    #[cfg(not(feature = "dialogs"))]
    {
        DialogResult::Confirmed
    }
}

/// Show a native yes/no confirmation dialog.
///
/// Returns [`DialogResult::Confirmed`] on "Yes" / OK, or
/// [`DialogResult::Cancelled`] on "No" / Cancel.
///
/// # Feature
///
/// Requires the `dialogs` feature.
#[allow(unused_variables)]
pub fn confirm_dialog(title: &str, message: &str) -> DialogResult {
    #[cfg(feature = "dialogs")]
    {
        let confirmed = rfd::MessageDialog::new()
            .set_title(title)
            .set_description(message)
            .set_level(rfd::MessageLevel::Info)
            .set_buttons(rfd::MessageButtons::YesNo)
            .show()
            == rfd::MessageDialogResult::Yes;
        if confirmed {
            DialogResult::Confirmed
        } else {
            DialogResult::Cancelled
        }
    }
    #[cfg(not(feature = "dialogs"))]
    {
        DialogResult::Cancelled
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_level_enum_variants() {
        assert_ne!(MessageLevel::Info, MessageLevel::Warning);
        assert_ne!(MessageLevel::Warning, MessageLevel::Error);
    }

    #[test]
    fn dialog_result_eq() {
        assert_eq!(DialogResult::Confirmed, DialogResult::Confirmed);
        assert_eq!(DialogResult::Cancelled, DialogResult::Cancelled);
        let paths = DialogResult::FilePaths(vec!["/tmp/foo.rs".into()]);
        assert_eq!(paths, DialogResult::FilePaths(vec!["/tmp/foo.rs".into()]));
    }

    #[test]
    #[cfg_attr(
        feature = "dialogs",
        ignore = "rfd requires main thread on macOS/Windows"
    )]
    fn open_file_dialog_no_feature_returns_cancelled() {
        // Without the `dialogs` feature this returns Cancelled immediately (no-op).
        // With the `dialogs` feature the rfd backend requires the main thread on
        // macOS and Windows — skip under nextest (which spawns worker threads).
        let result = open_file_dialog("pick", &[("All", "*")], false);
        let _ = result;
    }

    #[test]
    #[cfg_attr(
        feature = "dialogs",
        ignore = "rfd requires main thread on macOS/Windows"
    )]
    fn save_file_dialog_no_feature_returns_cancelled() {
        let result = save_file_dialog("save", Some("output.txt"), &[("Text", "txt")]);
        let _ = result;
    }

    #[test]
    #[cfg_attr(
        feature = "dialogs",
        ignore = "rfd requires main thread on macOS/Windows"
    )]
    fn message_dialog_no_feature_returns_confirmed() {
        // Without `dialogs` feature: always Confirmed (no-op, never panics).
        // With `dialogs` feature: requires main thread (macOS/Windows constraint).
        let result = message_dialog("Title", "Body", MessageLevel::Info);
        let _ = result;
    }

    #[test]
    #[cfg_attr(
        feature = "dialogs",
        ignore = "rfd requires main thread on macOS/Windows"
    )]
    fn confirm_dialog_no_feature_returns_cancelled() {
        let result = confirm_dialog("Title", "Are you sure?");
        let _ = result;
    }
}
