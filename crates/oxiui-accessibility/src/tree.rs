//! A11y tree builder for OxiUI.
//!
//! Provides [`A11yNode`], [`A11yTree`], and [`WidgetRole`] — together they
//! convert an OxiUI widget graph into an accesskit [`TreeUpdate`].

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use accesskit::{Node, NodeId, Role, Tree, TreeId, TreeUpdate};

use crate::props::{A11yNodeProps, Toggled3};

// ── Widget role mapping ──────────────────────────────────────────────────────

/// The accessibility role of an OxiUI widget node.
///
/// Maps semantic OxiUI widget kinds to their closest ARIA / AccessKit
/// [`Role`] equivalents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidgetRole {
    /// A top-level application window.
    Window,
    /// A generic container group (e.g. a panel or box layout).
    Group,
    /// A clickable button.
    Button,
    /// A read-only text label.
    Label,
    /// An editable text input field.
    TextInput,
    /// A table row (used by `oxiui-table`).
    TableRow,
    /// A single table cell.
    TableCell,
    /// A scrollable view container.
    ScrollView,
    /// An image widget.
    Image,
    /// Any widget whose role is not mapped.
    Unknown,

    // ── Interactive widgets ──────────────────────────────────────────────────
    /// A checkbox control (two or three states).
    Checkbox,
    /// A slider for selecting a numeric value within a range.
    Slider,
    /// A progress bar or loading indicator.
    ProgressBar,
    /// A tab button within a tab strip.
    Tab,
    /// The content panel associated with a `Tab`.
    TabPanel,
    /// A pop-up or drop-down menu container.
    Menu,
    /// A single item inside a `Menu`.
    MenuItem,
    /// A modal dialog.
    Dialog,
    /// An alert or status message.
    Alert,
    /// A tooltip associated with another widget.
    Tooltip,
    /// A hierarchical tree container.
    Tree,
    /// A single item inside a `Tree`.
    TreeItem,
    /// A list item (e.g. an `<li>` equivalent).
    ListItem,
    /// A hyperlink.
    Link,

    // ── Table roles ──────────────────────────────────────────────────────────
    /// A column header cell (e.g. `<th scope="col">`).
    ColumnHeader,

    // ── Landmark roles ───────────────────────────────────────────────────────
    /// The site-wide banner / site header.
    Banner,
    /// A navigation landmark (nav menu).
    Navigation,
    /// The primary main content of the page.
    Main,
    /// Complementary content (e.g. a sidebar).
    Complementary,
    /// The content info / site footer.
    ContentInfo,
}

impl From<WidgetRole> for Role {
    fn from(r: WidgetRole) -> Role {
        match r {
            WidgetRole::Window => Role::Window,
            WidgetRole::Group => Role::Group,
            WidgetRole::Button => Role::Button,
            WidgetRole::Label => Role::Label,
            WidgetRole::TextInput => Role::TextInput,
            WidgetRole::TableRow => Role::Row,
            WidgetRole::TableCell => Role::Cell,
            WidgetRole::ScrollView => Role::ScrollView,
            WidgetRole::Image => Role::Image,
            WidgetRole::Unknown => Role::Unknown,

            WidgetRole::Checkbox => Role::CheckBox,
            WidgetRole::Slider => Role::Slider,
            WidgetRole::ProgressBar => Role::ProgressIndicator,
            WidgetRole::Tab => Role::Tab,
            WidgetRole::TabPanel => Role::TabPanel,
            WidgetRole::Menu => Role::Menu,
            WidgetRole::MenuItem => Role::MenuItem,
            WidgetRole::Dialog => Role::Dialog,
            WidgetRole::Alert => Role::Alert,
            WidgetRole::Tooltip => Role::Tooltip,
            WidgetRole::Tree => Role::Tree,
            WidgetRole::TreeItem => Role::TreeItem,
            WidgetRole::ListItem => Role::ListItem,
            WidgetRole::Link => Role::Link,

            WidgetRole::ColumnHeader => Role::ColumnHeader,

            WidgetRole::Banner => Role::Banner,
            WidgetRole::Navigation => Role::Navigation,
            WidgetRole::Main => Role::Main,
            WidgetRole::Complementary => Role::Complementary,
            WidgetRole::ContentInfo => Role::ContentInfo,
        }
    }
}

impl std::fmt::Display for WidgetRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            WidgetRole::Window => "window",
            WidgetRole::Group => "group",
            WidgetRole::Button => "button",
            WidgetRole::Label => "label",
            WidgetRole::TextInput => "text-input",
            WidgetRole::TableRow => "table-row",
            WidgetRole::TableCell => "table-cell",
            WidgetRole::ScrollView => "scroll-view",
            WidgetRole::Image => "image",
            WidgetRole::Unknown => "unknown",

            WidgetRole::Checkbox => "checkbox",
            WidgetRole::Slider => "slider",
            WidgetRole::ProgressBar => "progress-bar",
            WidgetRole::Tab => "tab",
            WidgetRole::TabPanel => "tab-panel",
            WidgetRole::Menu => "menu",
            WidgetRole::MenuItem => "menu-item",
            WidgetRole::Dialog => "dialog",
            WidgetRole::Alert => "alert",
            WidgetRole::Tooltip => "tooltip",
            WidgetRole::Tree => "tree",
            WidgetRole::TreeItem => "tree-item",
            WidgetRole::ListItem => "list-item",
            WidgetRole::Link => "link",

            WidgetRole::ColumnHeader => "column-header",

            WidgetRole::Banner => "banner",
            WidgetRole::Navigation => "navigation",
            WidgetRole::Main => "main",
            WidgetRole::Complementary => "complementary",
            WidgetRole::ContentInfo => "content-info",
        };
        f.write_str(name)
    }
}

// ── A11y node ────────────────────────────────────────────────────────────────

/// A node in the OxiUI accessibility tree.
///
/// Each node corresponds to a widget in the UI hierarchy. The tree is
/// independent of any rendering backend and can be built and inspected in
/// headless tests.
pub struct A11yNode {
    /// Stable, unique identifier for this node within the tree.
    pub id: NodeId,
    /// The widget's accessibility role.
    pub role: WidgetRole,
    /// Optional human-readable label (e.g. button caption, field placeholder).
    pub label: Option<String>,
    /// Child nodes, in document/render order.
    pub children: Vec<A11yNode>,
    /// Rich property bag — description, state flags, range, relationships, etc.
    pub props: A11yNodeProps,
    /// Optional text content (for editable widgets).
    pub text_content: Option<String>,
}

impl std::fmt::Debug for A11yNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("A11yNode")
            .field("id", &self.id)
            .field("role", &self.role)
            .field("label", &self.label)
            .field("children", &self.children)
            .field("props", &self.props)
            .field("text_content", &self.text_content)
            .finish()
    }
}

impl A11yNode {
    /// Construct a minimal node with just an id, role, and optional label.
    pub fn simple(id: NodeId, role: WidgetRole, label: Option<String>) -> Self {
        Self {
            id,
            role,
            label,
            children: Vec::new(),
            props: A11yNodeProps::default(),
            text_content: None,
        }
    }

    /// Compute a stable hash over this node's OxiUI-side content fields.
    ///
    /// The hash covers the label, role, text_content, and all `A11yNodeProps`
    /// fields. It deliberately excludes `children` (child changes are detected
    /// by the diff walk) and `id` (the id is the lookup key, not content).
    ///
    /// Used by [`A11yTree::diff`] to replace the previous `format!("{:?}")`
    /// deep-equality fallback. `accesskit::Node` does not implement `PartialEq`,
    /// so dirty tracking is performed on the OxiUI wrapper instead.
    pub fn content_hash(&self) -> u64 {
        let mut h = DefaultHasher::new();
        // Label
        self.label.hash(&mut h);
        // Role — WidgetRole derives Debug + PartialEq + Eq; use Debug string
        // so we don't need a custom Hash impl on WidgetRole.
        format!("{:?}", self.role).hash(&mut h);
        // Text content
        self.text_content.hash(&mut h);
        // Props — A11yNodeProps derives Debug; hash via its Debug representation
        // since the individual field types (Option<f64>, etc.) don't all impl Hash.
        // This is correct: two nodes with identical Debug-printed props are equal.
        format!("{:?}", self.props).hash(&mut h);
        // Also include the children ID list so that structural changes (adding /
        // removing children) cause the parent's hash to change too.
        let child_ids: Vec<u64> = self.children.iter().map(|c| c.id.0).collect();
        child_ids.hash(&mut h);
        h.finish()
    }
}

// ── Tree builder ─────────────────────────────────────────────────────────────

/// Builds and diffs accesskit [`TreeUpdate`]s from an [`A11yNode`] tree.
///
/// The struct stores the flat map of nodes (by `NodeId`) after a build, so
/// subsequent calls to [`A11yTree::diff`] can compute minimal deltas.
///
/// The content-hash map (`hashes`) stores the OxiUI-side hash of each node at
/// the time of the last `build_and_store` call. [`A11yTree::diff`] uses these
/// hashes instead of `format!("{:?}", node)` string comparison, which avoids
/// the overhead of serializing `accesskit::Node` (which doesn't impl `PartialEq`).
#[derive(Default)]
pub struct A11yTree {
    /// Root id of the most recent build.
    pub(crate) root_id: Option<NodeId>,
    /// Flat, ordered snapshot: `(NodeId, Node)` from the most recent build.
    pub(crate) snapshot: Vec<(NodeId, Node)>,
    /// Per-node content hashes from the OxiUI `A11yNode` layer, keyed by NodeId.
    pub(crate) hashes: std::collections::HashMap<NodeId, u64>,
    /// Currently focused node, sent via `TreeUpdate::focus`.
    pub(crate) focus: Option<NodeId>,
}

impl A11yTree {
    /// Walk `root` and its descendants depth-first, producing a [`TreeUpdate`]
    /// that describes the full tree.
    ///
    /// Also stores the snapshot internally so future [`diff`] calls can
    /// compute minimal deltas.
    ///
    /// [`diff`]: A11yTree::diff
    pub fn build(root: &A11yNode) -> TreeUpdate {
        let mut nodes: Vec<(NodeId, Node)> = Vec::new();
        collect_nodes(root, &mut nodes);
        let root_id = root.id;
        TreeUpdate {
            nodes,
            tree: Some(Tree::new(root_id)),
            tree_id: TreeId::ROOT,
            focus: root_id,
        }
    }

    /// Build the full tree and store the snapshot for later diffing.
    ///
    /// Returns a `TreeUpdate` identical to [`A11yTree::build`].
    ///
    /// In addition to the AccessKit snapshot, this method collects the
    /// OxiUI-side content hash of every node and stores it in `self.hashes`.
    /// [`A11yTree::diff`] uses these hashes for efficient change detection.
    pub fn build_and_store(&mut self, root: &A11yNode) -> TreeUpdate {
        let mut nodes: Vec<(NodeId, Node)> = Vec::new();
        let mut hashes: std::collections::HashMap<NodeId, u64> = std::collections::HashMap::new();
        collect_nodes(root, &mut nodes);
        collect_hashes(root, &mut hashes);
        let root_id = root.id;
        self.root_id = Some(root_id);
        self.snapshot = nodes.clone();
        self.hashes = hashes;
        let focus = self.focus.unwrap_or(root_id);
        TreeUpdate {
            nodes,
            tree: Some(Tree::new(root_id)),
            tree_id: TreeId::ROOT,
            focus,
        }
    }

    // ── Focus tracking ───────────────────────────────────────────────────────

    /// Set the currently-focused node.
    ///
    /// Pass `None` to clear the focus (the adapter will typically move focus
    /// back to the root).
    pub fn set_focus(&mut self, id: Option<NodeId>) {
        self.focus = id;
    }

    /// Return the currently-focused node, if any.
    pub fn focus(&self) -> Option<NodeId> {
        self.focus
    }

    /// Produce a minimal `TreeUpdate` that only updates the focus field.
    ///
    /// Useful when only focus has changed and no node properties have changed.
    pub fn focus_update(&self) -> TreeUpdate {
        TreeUpdate {
            nodes: Vec::new(),
            tree: None,
            tree_id: TreeId::ROOT,
            focus: self.focus.unwrap_or(NodeId(0)),
        }
    }

    // ── Live-region announce ─────────────────────────────────────────────────

    /// Insert a transient live-region announcement node.
    ///
    /// Creates a synthetic [`accesskit::Role::Status`] node carrying `text`
    /// as its value, with the live-region politeness derived from `urgency`.
    /// The node is appended to the internal snapshot; the caller is responsible
    /// for removing it on the next tick (by calling [`build_and_store`] with a
    /// tree that doesn't include this id).
    ///
    /// Returns the newly-allocated `NodeId` so the caller can track it.
    ///
    /// [`build_and_store`]: A11yTree::build_and_store
    pub fn announce(&mut self, text: &str, urgency: crate::props::LiveSetting) -> NodeId {
        use accesskit::Live;
        // Allocate a fresh id beyond the current max, or start at 0x8000_0000.
        let new_raw: u64 = self
            .snapshot
            .iter()
            .map(|(id, _)| id.0)
            .max()
            .unwrap_or(0)
            .saturating_add(1)
            .max(0x8000_0000);
        let id = NodeId(new_raw);

        let mut node = Node::new(Role::Status);
        node.set_value(text);
        node.set_live(Live::from(urgency));
        node.set_live_atomic();

        self.snapshot.push((id, node));
        id
    }

    // ── Tree diff ────────────────────────────────────────────────────────────

    /// Compute a minimal `TreeUpdate` delta from `old` to `new_tree`.
    ///
    /// Only nodes whose content has changed (or that are brand-new) are
    /// included in the returned `nodes` list. Nodes that were removed are
    /// handled implicitly by AccessKit: when the parent's children list no
    /// longer references a node, the platform adapter orphans it.
    ///
    /// Change detection uses the OxiUI-side content hashes stored in
    /// `self.hashes` rather than `format!("{:?}", accesskit::Node)` string
    /// comparison. This is both faster and correct: `accesskit::Node` does not
    /// implement `PartialEq`, so the Debug-string approach was a pragmatic
    /// workaround. The hash approach is O(1) per node after the initial build.
    ///
    /// The returned update's `focus` is taken from `new_tree.focus`.
    pub fn diff(old: &A11yTree, new_tree: &A11yTree) -> TreeUpdate {
        // Build a NodeId → accesskit::Node map over the new snapshot for O(1) lookup.
        let new_node_map: std::collections::HashMap<NodeId, &Node> = new_tree
            .snapshot
            .iter()
            .map(|(id, node)| (*id, node))
            .collect();

        let mut changed: Vec<(NodeId, Node)> = Vec::new();

        for (id, new_node) in &new_tree.snapshot {
            let should_include = match old.hashes.get(id) {
                // Brand-new node: not present in the old tree at all.
                None => true,
                // Node exists in both trees: compare content hashes.
                Some(&old_hash) => {
                    let new_hash = new_tree.hashes.get(id).copied().unwrap_or(0);
                    old_hash != new_hash
                }
            };
            if should_include {
                changed.push((*id, new_node.clone()));
            }
        }

        // Also emit the parent of any removed node so AccessKit can update its
        // children list.  Removal detection: a node present in `old.hashes` but
        // absent from `new_node_map` has been removed.  Its (former) parent will
        // appear in `changed` already because the parent's child-id list changed
        // and therefore its content_hash changed.  No extra work is needed beyond
        // the hash walk above.
        let _ = new_node_map; // suppress unused-variable warning

        let focus = new_tree.focus.unwrap_or(NodeId(0));
        let tree = if old.root_id != new_tree.root_id {
            new_tree.root_id.map(Tree::new)
        } else {
            None
        };

        TreeUpdate {
            nodes: changed,
            tree,
            tree_id: TreeId::ROOT,
            focus,
        }
    }
}

// ── Table accessibility helpers ───────────────────────────────────────────────

/// Create a table-row [`A11yNode`] with a human-readable row description.
///
/// The `description` is set to `"Row N"` (1-based) so screen readers can
/// announce the row position when the row itself has no label.
pub fn table_row_node(id: NodeId, row_index: usize) -> A11yNode {
    let mut node = A11yNode::simple(id, WidgetRole::TableRow, None);
    node.props.description = Some(format!("Row {}", row_index + 1));
    node
}

/// Create a table-cell [`A11yNode`] carrying the cell's text and coordinates.
///
/// The `description` encodes the row and column (1-based) so assistive
/// technologies can announce the cell position in addition to its content.
pub fn table_cell_node(id: NodeId, row: usize, col: usize, text: &str) -> A11yNode {
    let mut node = A11yNode::simple(id, WidgetRole::TableCell, None);
    node.text_content = Some(text.to_string());
    node.props.description = Some(format!("Row {} Column {}", row + 1, col + 1));
    node
}

/// Create a column-header [`A11yNode`] with a visible label and position hint.
///
/// The `label` is the column header text; the `description` encodes the
/// column position (1-based) for screen readers that don't expose the label
/// separately.
pub fn column_header_node(id: NodeId, col: usize, label: &str) -> A11yNode {
    let mut node = A11yNode::simple(id, WidgetRole::ColumnHeader, None);
    node.label = Some(label.to_string());
    node.props.description = Some(format!("Column {} header", col + 1));
    node
}

/// Build a structured accessible table node tree.
///
/// Returns an [`A11yNode`] with role [`WidgetRole::Group`] (the closest
/// available container role) that has:
///
/// - `col_count` [`WidgetRole::ColumnHeader`] children, one per entry in
///   `col_headers` (or an empty label when the slice is shorter than
///   `col_count`).
/// - `row_count` [`WidgetRole::TableRow`] children, each containing
///   `col_count` [`WidgetRole::TableCell`] children.
///
/// Row and column positions are encoded in each node's `description` field
/// using the format established by the individual helper functions
/// ([`table_row_node`], [`table_cell_node`], [`column_header_node`]).
///
/// Node IDs are minted sequentially starting from 1.  The root node
/// receives id `0`.  Callers that need to merge this sub-tree into a
/// larger id space must renumber the returned nodes.
///
/// # Example
///
/// ```
/// use oxiui_accessibility::build_table_a11y;
///
/// let table = build_table_a11y(2, 3, &["Name", "Age", "City"]);
/// // 3 column headers + 2 rows = 5 direct children
/// assert_eq!(table.children.len(), 5);
/// ```
pub fn build_table_a11y(row_count: usize, col_count: usize, col_headers: &[&str]) -> A11yNode {
    // Sequential id counter: root = 0, then 1..
    let mut next_id: u64 = 0;

    let mut root = A11yNode::simple(NodeId(next_id), WidgetRole::Group, None);
    next_id += 1;

    // Column-header children
    for col_idx in 0..col_count {
        let header_label = col_headers.get(col_idx).copied().unwrap_or("");
        let header = column_header_node(NodeId(next_id), col_idx, header_label);
        next_id += 1;
        root.children.push(header);
    }

    // TableRow children, each containing TableCell children
    for row_idx in 0..row_count {
        let mut row = table_row_node(NodeId(next_id), row_idx);
        next_id += 1;

        for col_idx in 0..col_count {
            let cell = table_cell_node(NodeId(next_id), row_idx, col_idx, "");
            next_id += 1;
            row.children.push(cell);
        }

        root.children.push(row);
    }

    root
}

// ── Text-run synthesis ────────────────────────────────────────────────────────

/// Synthesize [`crate::props::TextRunChild`] segments for a text node.
///
/// Splits `text` at the selection boundaries so that assistive technologies
/// can expose the exact caret or selection range.
///
/// * No selection → one segment for the whole text (`is_selected = false`).
/// * Selection → up to three segments: text before, selected span, text after.
///   Empty leading/trailing segments (selection at start or end) are omitted.
///
/// Byte offsets are clamped to valid char boundaries using
/// [`crate::props::byte_offset_to_char_index`].
pub fn synthesize_text_run_children(
    text: &str,
    selection: Option<&crate::props::TextSelection>,
) -> Vec<crate::props::TextRunChild> {
    use crate::props::{byte_offset_to_char_index, TextRunChild};

    if text.is_empty() {
        return Vec::new();
    }

    let sel = match selection {
        None => {
            return vec![TextRunChild {
                text: text.to_string(),
                char_offset: 0,
                byte_offset: 0,
                is_selected: false,
            }];
        }
        Some(s) => s,
    };

    // Normalise anchor/focus to lo/hi byte offsets, clamped to text length.
    let lo_byte = sel.anchor.min(sel.focus).min(text.len());
    let hi_byte = sel.anchor.max(sel.focus).min(text.len());

    // Snap to nearest char boundary (walk forward until we hit one).
    let lo_byte = snap_to_char_boundary(text, lo_byte);
    let hi_byte = snap_to_char_boundary(text, hi_byte);

    let mut segments: Vec<TextRunChild> = Vec::with_capacity(3);

    // Before selection
    if lo_byte > 0 {
        let before = &text[..lo_byte];
        segments.push(TextRunChild {
            text: before.to_string(),
            char_offset: 0,
            byte_offset: 0,
            is_selected: false,
        });
    }

    // Selected span
    if lo_byte < hi_byte {
        let selected = &text[lo_byte..hi_byte];
        let char_off = byte_offset_to_char_index(text, lo_byte);
        segments.push(TextRunChild {
            text: selected.to_string(),
            char_offset: char_off,
            byte_offset: lo_byte,
            is_selected: true,
        });
    } else {
        // Collapsed caret — emit a zero-length selected segment
        let char_off = byte_offset_to_char_index(text, lo_byte);
        segments.push(TextRunChild {
            text: String::new(),
            char_offset: char_off,
            byte_offset: lo_byte,
            is_selected: true,
        });
    }

    // After selection
    if hi_byte < text.len() {
        let after = &text[hi_byte..];
        let char_off = byte_offset_to_char_index(text, hi_byte);
        segments.push(TextRunChild {
            text: after.to_string(),
            char_offset: char_off,
            byte_offset: hi_byte,
            is_selected: false,
        });
    }

    segments
}

/// Advance `offset` forward to the next UTF-8 char boundary in `text`.
///
/// If `offset` already sits on a boundary it is returned unchanged.
/// If `offset >= text.len()` the string length is returned.
fn snap_to_char_boundary(text: &str, offset: usize) -> usize {
    if offset >= text.len() {
        return text.len();
    }
    let mut pos = offset;
    while pos < text.len() && !text.is_char_boundary(pos) {
        pos += 1;
    }
    pos
}

// ── Internal helpers ─────────────────────────────────────────────────────────

/// Recursively collect content hashes for every node in the subtree.
///
/// The resulting map is stored in [`A11yTree::hashes`] and consulted by
/// [`A11yTree::diff`] to detect per-node changes without serialising
/// `accesskit::Node` to a Debug string.
fn collect_hashes(node: &A11yNode, out: &mut std::collections::HashMap<NodeId, u64>) {
    out.insert(node.id, node.content_hash());
    for child in &node.children {
        collect_hashes(child, out);
    }
}

/// Recursively collect [`A11yNode`] entries into `(NodeId, Node)` pairs.
///
/// Pre-order DFS: the parent is emitted before its children, which is the
/// ordering required by accesskit platform adapters.
fn collect_nodes(node: &A11yNode, out: &mut Vec<(NodeId, Node)>) {
    let child_ids: Vec<NodeId> = node.children.iter().map(|c| c.id).collect();

    let mut ak_node = Node::new(Role::from(node.role));

    if let Some(label) = &node.label {
        ak_node.set_label(label.as_str());
    }

    for &child_id in &child_ids {
        ak_node.push_child(child_id);
    }

    // Apply rich props
    apply_props(&mut ak_node, &node.props);

    // Text content + text-run subnode
    if let Some(text) = &node.text_content {
        ak_node.set_value(text.as_str());
    }

    out.push((node.id, ak_node));

    for child in &node.children {
        collect_nodes(child, out);
    }
}

/// Apply [`A11yNodeProps`] fields onto an accesskit [`Node`].
fn apply_props(ak: &mut Node, props: &A11yNodeProps) {
    if let Some(ref desc) = props.description {
        ak.set_description(desc.as_str());
    }
    if let Some(ref ph) = props.placeholder {
        ak.set_placeholder(ph.as_str());
    }
    if let Some(ref ks) = props.key_shortcut {
        ak.set_keyboard_shortcut(ks.as_str());
    }

    if props.disabled {
        ak.set_disabled();
    }
    if let Some(expanded) = props.expanded {
        ak.set_expanded(expanded);
    }
    if let Some(selected) = props.selected {
        ak.set_selected(selected);
    }
    if let Some(ref checked) = props.checked {
        use accesskit::Toggled;
        ak.set_toggled(Toggled::from(Toggled3::from(checked)));
    }

    if let Some(value) = props.value_now {
        ak.set_numeric_value(value);
    }
    if let Some(min) = props.value_min {
        ak.set_min_numeric_value(min);
    }
    if let Some(max) = props.value_max {
        ak.set_max_numeric_value(max);
    }
    if let Some(step) = props.value_step {
        ak.set_numeric_value_step(step);
    }

    if !props.labelled_by.is_empty() {
        ak.set_labelled_by(props.labelled_by.clone());
    }
    if !props.described_by.is_empty() {
        ak.set_described_by(props.described_by.clone());
    }
    if !props.controlled_by.is_empty() {
        ak.set_controls(props.controlled_by.clone());
    }
    if !props.owns.is_empty() {
        ak.set_owns(props.owns.clone());
    }
}
