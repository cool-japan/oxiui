//! AccessKit a11y bridge for `oxiui-iced`.
//!
//! Converts a [`WidgetSpec`] tree collected by [`crate::IcedUiCtx`] into an
//! [`accesskit::TreeUpdate`] via `oxiui-accessibility`'s infrastructure.
//!
//! iced 0.14 does not ship built-in AccessKit support. This module provides a
//! best-effort semantic bridge: each `WidgetSpec` variant is mapped to the
//! closest [`WidgetRole`] and assigned a stable [`NodeId`] derived from its
//! depth-first position in the spec tree.
//!
//! Compiled only when the `a11y` feature is enabled.

use crate::adapter::WidgetSpec;
use accesskit::{NodeId, TreeUpdate};
use oxiui_accessibility::{
    tree::{A11yNode, A11yTree, WidgetRole},
    A11yNodeBuilder,
};

// ── Node ID counter ───────────────────────────────────────────────────────────

struct IdGen(u64);

impl IdGen {
    fn with_start(start: u64) -> Self {
        Self(start)
    }
    fn next(&mut self) -> NodeId {
        let id = NodeId(self.0);
        self.0 += 1;
        id
    }
}

// ── IcedA11yConfig ─────────────────────────────────────────────────────────────

/// Configuration for the iced → AccessKit bridge.
///
/// Controls the synthesised root `Window` node label and the starting
/// [`NodeId`] counter.
#[derive(Clone, Debug)]
pub struct IcedA11yConfig {
    /// Optional label for the synthesised root `Window` node.
    pub root_label: Option<String>,
    /// Starting [`NodeId`] counter.  Defaults to `1`.
    pub id_start: u64,
}

impl Default for IcedA11yConfig {
    fn default() -> Self {
        Self {
            root_label: None,
            id_start: 1,
        }
    }
}

impl IcedA11yConfig {
    /// Set the root window label.
    #[must_use]
    pub fn with_root_label(mut self, label: impl Into<String>) -> Self {
        self.root_label = Some(label.into());
        self
    }

    /// Set the starting `NodeId` counter.
    #[must_use]
    pub fn with_id_start(mut self, start: u64) -> Self {
        self.id_start = start;
        self
    }
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Convert a single [`WidgetSpec`] into an [`A11yNode`], advancing `counter`.
///
/// Returns `None` for decorative specs (`Separator`, `Spacer`).
pub fn spec_to_a11y_node(spec: WidgetSpec, counter: &mut u64) -> Option<A11yNode> {
    let mut gen = IdGen::with_start(*counter);
    let result = spec_to_node(spec, &mut gen);
    *counter = gen.0;
    result
}

/// Convert a slice of [`WidgetSpec`]s to an [`accesskit::TreeUpdate`].
///
/// All specs are collected under a synthetic root `Window` node.
pub fn spec_to_a11y_tree(specs: &[WidgetSpec], config: &IcedA11yConfig) -> TreeUpdate {
    let mut gen = IdGen::with_start(config.id_start);
    let root_id = gen.next();

    let mut root_builder = A11yNodeBuilder::new(root_id, WidgetRole::Window);
    if let Some(ref label) = config.root_label {
        root_builder = root_builder.label(label.as_str());
    }

    let children: Vec<A11yNode> = specs
        .iter()
        .filter_map(|s| spec_to_node(s.clone(), &mut gen))
        .collect();

    let root = root_builder.build_with_children(children);
    A11yTree::build(&root)
}

// ── Private helpers ───────────────────────────────────────────────────────────

fn spec_to_node(spec: WidgetSpec, gen: &mut IdGen) -> Option<A11yNode> {
    let id = gen.next();
    match spec {
        // ── Decorative ────────────────────────────────────────────────────────
        WidgetSpec::Separator | WidgetSpec::Spacer { .. } => None,

        // ── Text ──────────────────────────────────────────────────────────────
        WidgetSpec::Heading(t) => Some(
            A11yNodeBuilder::new(id, WidgetRole::Label)
                .label(t.as_ref())
                .build(),
        ),
        WidgetSpec::Label(t) => Some(
            A11yNodeBuilder::new(id, WidgetRole::Label)
                .label(t.as_ref())
                .build(),
        ),

        // ── Interactive ───────────────────────────────────────────────────────
        WidgetSpec::Button { label, .. } => Some(
            A11yNodeBuilder::new(id, WidgetRole::Button)
                .label(label.as_ref())
                .build(),
        ),

        WidgetSpec::TextInput { value, .. } => {
            let mut node = A11yNodeBuilder::new(id, WidgetRole::TextInput).build();
            if !value.is_empty() {
                node.text_content = Some(value.into_owned());
            }
            Some(node)
        }

        WidgetSpec::TextArea { value, .. } => {
            let mut node = A11yNodeBuilder::new(id, WidgetRole::TextInput).build();
            if !value.is_empty() {
                node.text_content = Some(value.into_owned());
            }
            Some(node)
        }

        WidgetSpec::Checkbox { label, checked, .. } => {
            use oxiui_accessibility::props::CheckedState;
            Some(
                A11yNodeBuilder::new(id, WidgetRole::Checkbox)
                    .label(label.as_ref())
                    .checked(if checked {
                        CheckedState::True
                    } else {
                        CheckedState::False
                    })
                    .build(),
            )
        }

        WidgetSpec::Slider {
            value, start, end, ..
        } => Some(
            A11yNodeBuilder::new(id, WidgetRole::Slider)
                .value(value, start, end, 0.0)
                .build(),
        ),

        WidgetSpec::Dropdown {
            options, selected, ..
        } => {
            let label = options
                .get(selected)
                .map(String::as_str)
                .unwrap_or("")
                .to_owned();
            let desc = options.join(", ");
            let mut node = A11yNodeBuilder::new(id, WidgetRole::Label)
                .label(label)
                .build();
            if !desc.is_empty() {
                node.props.description = Some(desc);
            }
            Some(node)
        }

        WidgetSpec::Image { uri, .. } => Some(
            A11yNodeBuilder::new(id, WidgetRole::Image)
                .label(uri.as_ref())
                .build(),
        ),

        WidgetSpec::RichText(spans) => {
            let joined: String = spans
                .iter()
                .map(|s| s.text.as_str())
                .collect::<Vec<_>>()
                .join("");
            Some(
                A11yNodeBuilder::new(id, WidgetRole::Label)
                    .label(joined)
                    .build(),
            )
        }

        // ── Containers ───────────────────────────────────────────────────────
        WidgetSpec::Horizontal(specs) | WidgetSpec::Vertical(specs) => {
            let children: Vec<A11yNode> = specs
                .into_iter()
                .filter_map(|s| spec_to_node(s, gen))
                .collect();
            Some(A11yNodeBuilder::new(id, WidgetRole::Group).build_with_children(children))
        }

        WidgetSpec::Grid { children, .. } => {
            let child_nodes: Vec<A11yNode> = children
                .into_iter()
                .filter_map(|s| spec_to_node(s, gen))
                .collect();
            Some(A11yNodeBuilder::new(id, WidgetRole::Group).build_with_children(child_nodes))
        }

        WidgetSpec::Scroll { children } => {
            let child_nodes: Vec<A11yNode> = children
                .into_iter()
                .filter_map(|s| spec_to_node(s, gen))
                .collect();
            Some(A11yNodeBuilder::new(id, WidgetRole::ScrollView).build_with_children(child_nodes))
        }

        WidgetSpec::Tooltip { inner, text } => {
            let inner_node = spec_to_node(*inner, gen);
            let mut node = A11yNodeBuilder::new(id, WidgetRole::Tooltip)
                .label(text.as_ref())
                .build();
            if let Some(child) = inner_node {
                node.children.push(child);
            }
            Some(node)
        }

        WidgetSpec::Popup { children } => {
            let child_nodes: Vec<A11yNode> = children
                .into_iter()
                .filter_map(|s| spec_to_node(s, gen))
                .collect();
            Some(A11yNodeBuilder::new(id, WidgetRole::Dialog).build_with_children(child_nodes))
        }

        WidgetSpec::Modal { title, children } => {
            let child_nodes: Vec<A11yNode> = children
                .into_iter()
                .filter_map(|s| spec_to_node(s, gen))
                .collect();
            Some(
                A11yNodeBuilder::new(id, WidgetRole::Dialog)
                    .label(title.as_ref())
                    .build_with_children(child_nodes),
            )
        }
    }
}
