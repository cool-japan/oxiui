//! In-app dialog queue.
//!
//! Provides a backend-agnostic dialog request/response model for the [`App`]
//! facade.  Dialogs are queued via [`DialogQueue::request`] and polled each
//! frame via [`DialogQueue::pop_pending`].  Backend adapters consume the
//! pending requests and post responses back through the same queue.
//!
//! This implementation is **pure in-process**: it does not call native OS
//! file-picker dialogs (those require `rfd` or similar C/ObjC bridges which
//! are outside the Pure Rust scope).  For native file dialogs, gate a
//! separate feature on `rfd` and integrate it in the backend adapter.
//!
//! # Usage
//!
//! ```rust
//! use oxiui::dialog::{DialogQueue, DialogKind, DialogResponse};
//!
//! let mut queue = DialogQueue::new();
//!
//! // App code requests a confirmation dialog.
//! let id = queue.request(DialogKind::Confirm {
//!     title: "Exit?".into(),
//!     message: "Are you sure you want to quit?".into(),
//! });
//!
//! // Backend (or test code) posts a response.
//! queue.respond(id, DialogResponse::Confirmed);
//!
//! // App polls the response.
//! assert_eq!(queue.pop_response(id), Some(DialogResponse::Confirmed));
//! ```

use std::collections::HashMap;

// ── DialogId ──────────────────────────────────────────────────────────────────

/// Opaque handle for a pending dialog request.
///
/// Returned by [`DialogQueue::request`]; use it to poll the response with
/// [`DialogQueue::pop_response`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DialogId(pub u64);

impl std::fmt::Display for DialogId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Dialog({})", self.0)
    }
}

// ── DialogKind ────────────────────────────────────────────────────────────────

/// Describes the type and content of a dialog request.
#[derive(Clone, Debug)]
pub enum DialogKind {
    /// Simple informational alert with an OK button.
    Alert {
        /// Dialog window title.
        title: String,
        /// Message body text.
        message: String,
    },
    /// Yes/No confirmation dialog.
    Confirm {
        /// Dialog window title.
        title: String,
        /// Question body text.
        message: String,
    },
    /// Single-line text-input prompt dialog.
    Prompt {
        /// Dialog window title.
        title: String,
        /// Prompt body text shown above the text field.
        message: String,
        /// Optional default / placeholder value for the text field.
        default_text: Option<String>,
    },
    /// File-open picker dialog.
    ///
    /// The response carries the selected path(s) as strings (see
    /// [`DialogResponse::FilePaths`]).  Backend adapters are responsible for
    /// invoking the OS file picker; pure headless backends return
    /// [`DialogResponse::Cancelled`].
    FileOpen {
        /// Title shown in the OS file picker (if supported by the backend).
        title: String,
        /// Accepted file extension filters, e.g. `[("Rust source", "*.rs")]`.
        filters: Vec<(String, String)>,
        /// Whether multiple files may be selected.
        multiple: bool,
    },
    /// File-save picker dialog.
    FileSave {
        /// Title shown in the OS file picker (if supported by the backend).
        title: String,
        /// Suggested default file name.
        default_name: Option<String>,
        /// Accepted file extension filters.
        filters: Vec<(String, String)>,
    },
}

impl DialogKind {
    /// Human-readable label for the dialog type (useful for debug/logging).
    pub fn kind_label(&self) -> &'static str {
        match self {
            DialogKind::Alert { .. } => "Alert",
            DialogKind::Confirm { .. } => "Confirm",
            DialogKind::Prompt { .. } => "Prompt",
            DialogKind::FileOpen { .. } => "FileOpen",
            DialogKind::FileSave { .. } => "FileSave",
        }
    }
}

// ── DialogResponse ────────────────────────────────────────────────────────────

/// The backend's answer to a dialog request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DialogResponse {
    /// The user dismissed an alert (clicked OK).
    Dismissed,
    /// The user confirmed a yes/no dialog.
    Confirmed,
    /// The user cancelled or declined.
    Cancelled,
    /// The user submitted a text-input prompt.
    Text(String),
    /// The user selected one or more file paths.
    FilePaths(Vec<String>),
    /// The user chose a save-as path.
    SavePath(String),
}

// ── DialogRequest ─────────────────────────────────────────────────────────────

/// A pending dialog request in the queue.
#[derive(Clone, Debug)]
pub struct DialogRequest {
    /// Unique identifier for this dialog.
    pub id: DialogId,
    /// Dialog type and content.
    pub kind: DialogKind,
}

// ── DialogQueue ───────────────────────────────────────────────────────────────

/// A FIFO queue of pending dialog requests and a map of posted responses.
///
/// Call [`request`](DialogQueue::request) to enqueue a dialog; the backend
/// picks it up from [`pop_pending`](DialogQueue::pop_pending) and posts
/// the result via [`respond`](DialogQueue::respond).  App code polls with
/// [`pop_response`](DialogQueue::pop_response).
pub struct DialogQueue {
    next_id: u64,
    pending: std::collections::VecDeque<DialogRequest>,
    responses: HashMap<DialogId, DialogResponse>,
}

impl DialogQueue {
    /// Create an empty [`DialogQueue`].
    pub fn new() -> Self {
        Self {
            next_id: 1,
            pending: std::collections::VecDeque::new(),
            responses: HashMap::new(),
        }
    }

    /// Enqueue a dialog request and return its [`DialogId`].
    ///
    /// The id is stable and can be used later to poll the response.
    pub fn request(&mut self, kind: DialogKind) -> DialogId {
        let id = DialogId(self.next_id);
        self.next_id += 1;
        self.pending.push_back(DialogRequest { id, kind });
        id
    }

    /// Dequeue the oldest pending dialog request, if any.
    ///
    /// Back-end adapters call this each frame to discover new dialogs to show.
    pub fn pop_pending(&mut self) -> Option<DialogRequest> {
        self.pending.pop_front()
    }

    /// Post a response for a completed dialog.
    ///
    /// The response is stored until the app code polls it with
    /// [`pop_response`](DialogQueue::pop_response).
    pub fn respond(&mut self, id: DialogId, response: DialogResponse) {
        self.responses.insert(id, response);
    }

    /// Consume and return the response for the given dialog, if one has been posted.
    ///
    /// Returns `None` if the response has not yet been posted or was already consumed.
    pub fn pop_response(&mut self, id: DialogId) -> Option<DialogResponse> {
        self.responses.remove(&id)
    }

    /// Peek at the response without consuming it.
    pub fn peek_response(&self, id: DialogId) -> Option<&DialogResponse> {
        self.responses.get(&id)
    }

    /// Returns `true` if there are no pending (unshown) dialog requests.
    pub fn is_pending_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// Returns the number of pending (unshown) dialog requests.
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Returns `true` if there are no ready (unread) responses.
    pub fn is_responses_empty(&self) -> bool {
        self.responses.is_empty()
    }
}

impl Default for DialogQueue {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_returns_unique_ids() {
        let mut q = DialogQueue::new();
        let id1 = q.request(DialogKind::Alert {
            title: "a".into(),
            message: "m".into(),
        });
        let id2 = q.request(DialogKind::Alert {
            title: "b".into(),
            message: "n".into(),
        });
        assert_ne!(id1, id2);
    }

    #[test]
    fn pop_pending_returns_fifo_order() {
        let mut q = DialogQueue::new();
        let id1 = q.request(DialogKind::Alert {
            title: "first".into(),
            message: "".into(),
        });
        let id2 = q.request(DialogKind::Alert {
            title: "second".into(),
            message: "".into(),
        });
        let first = q.pop_pending().unwrap();
        assert_eq!(first.id, id1);
        let second = q.pop_pending().unwrap();
        assert_eq!(second.id, id2);
        assert!(q.pop_pending().is_none());
    }

    #[test]
    fn respond_and_pop_response() {
        let mut q = DialogQueue::new();
        let id = q.request(DialogKind::Confirm {
            title: "Exit?".into(),
            message: "Sure?".into(),
        });
        q.respond(id, DialogResponse::Confirmed);
        assert_eq!(q.pop_response(id), Some(DialogResponse::Confirmed));
        // second pop returns None (already consumed)
        assert_eq!(q.pop_response(id), None);
    }

    #[test]
    fn peek_response_does_not_consume() {
        let mut q = DialogQueue::new();
        let id = q.request(DialogKind::Alert {
            title: "t".into(),
            message: "m".into(),
        });
        q.respond(id, DialogResponse::Dismissed);
        assert!(q.peek_response(id).is_some());
        assert!(q.peek_response(id).is_some()); // still there
        q.pop_response(id);
        assert!(q.peek_response(id).is_none());
    }

    #[test]
    fn prompt_dialog_text_response() {
        let mut q = DialogQueue::new();
        let id = q.request(DialogKind::Prompt {
            title: "Name".into(),
            message: "Enter your name:".into(),
            default_text: Some("World".into()),
        });
        q.respond(id, DialogResponse::Text("Alice".into()));
        assert_eq!(
            q.pop_response(id),
            Some(DialogResponse::Text("Alice".into()))
        );
    }

    #[test]
    fn file_open_dialog_paths_response() {
        let mut q = DialogQueue::new();
        let id = q.request(DialogKind::FileOpen {
            title: "Open".into(),
            filters: vec![("Rust".into(), "*.rs".into())],
            multiple: false,
        });
        q.respond(id, DialogResponse::FilePaths(vec!["/tmp/foo.rs".into()]));
        if let Some(DialogResponse::FilePaths(paths)) = q.pop_response(id) {
            assert_eq!(paths, vec!["/tmp/foo.rs"]);
        } else {
            panic!("expected FilePaths response");
        }
    }

    #[test]
    fn pending_count_tracks_enqueue() {
        let mut q = DialogQueue::new();
        assert_eq!(q.pending_count(), 0);
        q.request(DialogKind::Alert {
            title: "t".into(),
            message: "m".into(),
        });
        assert_eq!(q.pending_count(), 1);
        q.pop_pending();
        assert_eq!(q.pending_count(), 0);
    }

    #[test]
    fn kind_label_returns_correct_string() {
        assert_eq!(
            DialogKind::Alert {
                title: "".into(),
                message: "".into()
            }
            .kind_label(),
            "Alert"
        );
        assert_eq!(
            DialogKind::Confirm {
                title: "".into(),
                message: "".into()
            }
            .kind_label(),
            "Confirm"
        );
        assert_eq!(
            DialogKind::FileOpen {
                title: "".into(),
                filters: vec![],
                multiple: false,
            }
            .kind_label(),
            "FileOpen"
        );
    }

    #[test]
    fn dialog_id_display() {
        let id = DialogId(7);
        assert_eq!(format!("{id}"), "Dialog(7)");
    }
}
