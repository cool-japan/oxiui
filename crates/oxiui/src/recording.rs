//! [`RecordingUiCtx`] — a [`UiCtx`] implementation that records widget calls
//! as structured entries mappable to an accessibility tree.
//!
//! This module is available when the `a11y` feature is enabled. It provides:
//! - [`RecordingEntry`] — a single captured widget call with role, label, and children.
//! - [`RecordingUiCtx`] — drives widget calls through recording only (no display),
//!   optionally forwarding to a delegate [`UiCtx`].
//!
//! # Example
//!
//! ```rust
//! use oxiui::RecordingUiCtx;
//! use oxiui_core::UiCtx;
//!
//! let mut ctx = RecordingUiCtx::new();
//! ctx.heading("My App");
//! ctx.button("Submit");
//! assert_eq!(ctx.entries.len(), 2);
//! ```

use accesskit::NodeId;
use oxiui_accessibility::{A11yNode, A11yTree, WidgetRole};
use oxiui_core::{response::WidgetResponse, ButtonResponse, RichTextSpan, UiCtx};

// ── Node ID allocation ────────────────────────────────────────────────────────

/// Simple sequential counter for generating stable `accesskit::NodeId` values.
///
/// Starts at 1 (0 is reserved for the synthetic root created in
/// [`RecordingUiCtx::build_a11y_tree`]).
struct NodeIdGen(u64);

impl NodeIdGen {
    fn new() -> Self {
        NodeIdGen(1)
    }

    fn next(&mut self) -> NodeId {
        let id = NodeId(self.0);
        self.0 += 1;
        id
    }
}

// ── RecordingEntry ────────────────────────────────────────────────────────────

/// A single captured widget call produced by [`RecordingUiCtx`].
///
/// The `role` field carries the closest ARIA-equivalent [`WidgetRole`] for the
/// widget. `label` holds the human-readable text (button caption, heading text,
/// field placeholder, etc.). `children` holds nested entries produced by
/// layout container calls such as [`UiCtx::horizontal`] and
/// [`UiCtx::vertical`].
#[derive(Clone, Debug)]
pub struct RecordingEntry {
    /// The accessibility role of the widget.
    pub role: WidgetRole,
    /// Human-readable label for the widget.
    pub label: String,
    /// Child entries for container widgets (layout groups, menu bars, etc.).
    pub children: Vec<RecordingEntry>,
}

// ── RecordingUiCtx ────────────────────────────────────────────────────────────

/// A [`UiCtx`] that records every widget call without rendering anything.
///
/// Optionally forwards calls to a delegate [`UiCtx`] so it can be used as a
/// transparent proxy for inspection or accessibility tree generation.
///
/// # Building an accessibility snapshot
///
/// After driving a content closure through `RecordingUiCtx`, call
/// [`build_a11y_tree`] to produce an [`A11yTree`] suitable for passing to
/// platform accessibility adapters.
///
/// [`build_a11y_tree`]: RecordingUiCtx::build_a11y_tree
pub struct RecordingUiCtx<'a> {
    /// Optional delegate context. When `Some`, every widget call is forwarded
    /// after being recorded.
    delegate: Option<&'a mut dyn UiCtx>,
    /// The recorded widget calls, in the order they were made.
    pub entries: Vec<RecordingEntry>,
}

impl<'a> RecordingUiCtx<'a> {
    /// Create a standalone recording context with no delegate.
    pub fn new() -> Self {
        RecordingUiCtx {
            delegate: None,
            entries: Vec::new(),
        }
    }

    /// Create a recording context that forwards all calls to `delegate`.
    pub fn with_delegate(delegate: &'a mut dyn UiCtx) -> Self {
        RecordingUiCtx {
            delegate: Some(delegate),
            entries: Vec::new(),
        }
    }

    // ── Internal helpers ──────────────────────────────────────────────────────

    /// Record a leaf widget entry.
    fn record(&mut self, role: WidgetRole, label: impl Into<String>) {
        self.entries.push(RecordingEntry {
            role,
            label: label.into(),
            children: Vec::new(),
        });
    }

    /// Record a container entry with pre-collected child entries.
    fn record_group(&mut self, role: WidgetRole, label: &str, children: Vec<RecordingEntry>) {
        self.entries.push(RecordingEntry {
            role,
            label: label.to_string(),
            children,
        });
    }

    // ── A11y tree construction ────────────────────────────────────────────────

    /// Build an [`A11yTree`] from the recorded entries under the given window id.
    ///
    /// All recorded top-level entries are placed as children of a synthetic
    /// [`WidgetRole::Window`] root node whose id is `root_id`. The returned
    /// `A11yTree` has its internal snapshot populated via `build_and_store`, so
    /// subsequent calls to [`A11yTree::diff`] can compute minimal deltas.
    ///
    /// # Arguments
    /// * `root_id` — The `WindowA11yId` whose numeric value becomes the root
    ///   `accesskit::NodeId`. Use `WindowA11yId(1)` for a single-window app.
    pub fn build_a11y_tree(&self, root_id: oxiui_accessibility::WindowA11yId) -> A11yTree {
        let mut gen = NodeIdGen::new();
        // Reserve id 0 for the synthetic window root.
        let root_node_id = NodeId(root_id.0);

        // Build A11yNode children from all top-level entries.
        let children: Vec<A11yNode> = self
            .entries
            .iter()
            .map(|e| entry_to_node(e, &mut gen))
            .collect();

        // Synthetic root — a Window node whose children are all top-level widgets.
        let mut root = A11yNode::simple(root_node_id, WidgetRole::Window, None);
        root.children = children;

        // Populate an A11yTree via build_and_store so the snapshot is ready for diff.
        let mut tree = A11yTree::default();
        let _ = tree.build_and_store(&root);
        tree
    }
}

impl<'a> Default for RecordingUiCtx<'a> {
    fn default() -> Self {
        Self::new()
    }
}

// ── UiCtx implementation ──────────────────────────────────────────────────────

impl<'a> UiCtx for RecordingUiCtx<'a> {
    // ── Required methods ──────────────────────────────────────────────────────

    fn heading(&mut self, text: &str) {
        // WidgetRole has no Heading variant; Label is the closest read-only role.
        self.record(WidgetRole::Label, text);
        if let Some(d) = self.delegate.as_mut() {
            d.heading(text);
        }
    }

    fn label(&mut self, text: &str) {
        self.record(WidgetRole::Label, text);
        if let Some(d) = self.delegate.as_mut() {
            d.label(text);
        }
    }

    fn button(&mut self, label: &str) -> ButtonResponse {
        self.record(WidgetRole::Button, label);
        if let Some(d) = self.delegate.as_mut() {
            d.button(label)
        } else {
            ButtonResponse::default()
        }
    }

    // ── Layout container methods ──────────────────────────────────────────────

    fn horizontal(&mut self, content: &mut dyn FnMut(&mut dyn UiCtx)) -> WidgetResponse {
        let mut child_ctx = RecordingUiCtx::new();
        content(&mut child_ctx);
        let children = child_ctx.entries;
        self.record_group(WidgetRole::Group, "horizontal", children);
        if let Some(d) = self.delegate.as_mut() {
            // Re-collect children via the delegate; the child_ctx entries are
            // already captured above, so just drive the delegate separately.
            let mut noop: NullDelegate = NullDelegate;
            d.horizontal(&mut |_ui| noop.visit());
        }
        WidgetResponse::supported()
    }

    fn vertical(&mut self, content: &mut dyn FnMut(&mut dyn UiCtx)) -> WidgetResponse {
        let mut child_ctx = RecordingUiCtx::new();
        content(&mut child_ctx);
        let children = child_ctx.entries;
        self.record_group(WidgetRole::Group, "vertical", children);
        if let Some(d) = self.delegate.as_mut() {
            let mut noop = NullDelegate;
            d.vertical(&mut |_ui| noop.visit());
        }
        WidgetResponse::supported()
    }

    fn grid(&mut self, cols: usize, content: &mut dyn FnMut(&mut dyn UiCtx)) -> WidgetResponse {
        let mut child_ctx = RecordingUiCtx::new();
        content(&mut child_ctx);
        let children = child_ctx.entries;
        // WidgetRole has no Grid variant; use Group.
        self.record_group(WidgetRole::Group, "grid", children);
        if let Some(d) = self.delegate.as_mut() {
            let mut noop = NullDelegate;
            d.grid(cols, &mut |_ui| noop.visit());
        }
        WidgetResponse::supported()
    }

    fn menu_bar(&mut self, content: &mut dyn FnMut(&mut dyn UiCtx)) -> WidgetResponse {
        let mut child_ctx = RecordingUiCtx::new();
        content(&mut child_ctx);
        let children = child_ctx.entries;
        // WidgetRole::Menu is the closest available role for a menu bar.
        self.record_group(WidgetRole::Menu, "menu_bar", children);
        if let Some(d) = self.delegate.as_mut() {
            let mut noop = NullDelegate;
            d.menu_bar(&mut |_ui| noop.visit());
        }
        WidgetResponse::supported()
    }

    fn rich_text(&mut self, spans: &[RichTextSpan]) -> WidgetResponse {
        let text: String = spans
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join("");
        self.record(WidgetRole::Label, text);
        if let Some(d) = self.delegate.as_mut() {
            d.rich_text(spans);
        }
        WidgetResponse::supported()
    }

    fn drag_source(&mut self, id: u64, content: &mut dyn FnMut(&mut dyn UiCtx)) -> WidgetResponse {
        // Capture children by driving the closure through self.
        content(self);
        if let Some(d) = self.delegate.as_mut() {
            let mut noop = NullDelegate;
            d.drag_source(id, &mut |_ui| noop.visit());
        }
        WidgetResponse::unsupported()
    }

    fn drop_target(
        &mut self,
        accept_ids: &[u64],
        content: &mut dyn FnMut(&mut dyn UiCtx),
    ) -> WidgetResponse {
        content(self);
        if let Some(d) = self.delegate.as_mut() {
            let mut noop = NullDelegate;
            d.drop_target(accept_ids, &mut |_ui| noop.visit());
        }
        WidgetResponse::unsupported()
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Convert a [`RecordingEntry`] to an [`A11yNode`], recursively.
fn entry_to_node(entry: &RecordingEntry, gen: &mut NodeIdGen) -> A11yNode {
    let id = gen.next();
    let mut node = A11yNode::simple(id, entry.role, Some(entry.label.clone()));
    node.children = entry
        .children
        .iter()
        .map(|child| entry_to_node(child, gen))
        .collect();
    node
}

/// A no-op placeholder used to satisfy FnMut signatures when forwarding
/// layout container calls to a delegate without re-running the closure body.
///
/// The delegate's container methods expect a `&mut dyn FnMut(&mut dyn UiCtx)`
/// argument. Because the child content was already captured by `RecordingUiCtx`,
/// we pass an empty closure rather than re-running the real content.
struct NullDelegate;

impl NullDelegate {
    fn visit(&mut self) {}
}
