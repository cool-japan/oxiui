//! Action mapping — translates [`accesskit::ActionRequest`] into OxiUI-side
//! [`A11yAction`] values.
//!
//! The mapping is intentionally one-way: the platform adapter drives AT
//! actions; OxiUI receives them as well-typed [`A11yAction`] variants and
//! routes them to the widget event system.  Unknown / unmapped actions are
//! returned as `None` so callers can silently discard them without panicking.

use accesskit::{Action, ActionData, ActionRequest};

// ── OxiUI action enum ────────────────────────────────────────────────────────

/// An accessibility action produced by mapping an [`accesskit::ActionRequest`].
///
/// Variants correspond to the most common assistive-technology actions.
/// Less common or platform-specific actions that have no OxiUI equivalent
/// are discarded by [`map_action`] (returned as `None`).
#[derive(Debug, Clone, PartialEq)]
pub enum A11yAction {
    /// Activate the target widget (equivalent to a left-click or tap).
    Click,
    /// Move keyboard focus to the target widget.
    Focus,
    /// Scroll any scrollable ancestors so the target widget is visible.
    ScrollIntoView,
    /// Replace the target's text value with the given string.
    SetValue(String),
    /// Increment a numeric value by one step.
    Increment,
    /// Decrement a numeric value by one step.
    Decrement,
    /// A platform-defined or application-defined custom action.
    Custom(String),
}

// ── Mapping function ─────────────────────────────────────────────────────────

/// Map an [`accesskit::ActionRequest`] to an OxiUI [`A11yAction`].
///
/// Returns `None` for actions that have no OxiUI equivalent (scroll variants,
/// tooltip show/hide, sequential focus navigation, etc.).
///
/// # Deviations from the plan
///
/// * `Action::Default` does not exist in accesskit 0.24 — only `Action::Click`
///   maps to [`A11yAction::Click`].
/// * `Action::CustomAction` maps to [`A11yAction::Custom`] using the i32 id
///   formatted as a string (`"custom:<id>"`); the plan showed a bare string.
pub fn map_action(req: &ActionRequest) -> Option<A11yAction> {
    match req.action {
        Action::Click => Some(A11yAction::Click),
        Action::Focus => Some(A11yAction::Focus),
        Action::ScrollIntoView => Some(A11yAction::ScrollIntoView),
        Action::SetValue => {
            let val = match &req.data {
                Some(ActionData::Value(s)) => s.to_string(),
                _ => String::new(),
            };
            Some(A11yAction::SetValue(val))
        }
        Action::Increment => Some(A11yAction::Increment),
        Action::Decrement => Some(A11yAction::Decrement),
        Action::CustomAction => {
            let label = match &req.data {
                Some(ActionData::CustomAction(id)) => format!("custom:{id}"),
                _ => "custom".to_string(),
            };
            Some(A11yAction::Custom(label))
        }
        // Blur, Collapse, Expand, HideTooltip, ShowTooltip, ShowContextMenu,
        // ReplaceSelectedText, Scroll*, ScrollToPoint, SetScrollOffset,
        // SetTextSelection, SetSequentialFocusNavigationStartingPoint — no
        // OxiUI equivalent yet.
        _ => None,
    }
}

// ── ActionDispatcher ─────────────────────────────────────────────────────────

/// Type alias for a boxed action handler closure.
///
/// Used internally by [`ActionDispatcher`] to store registered callbacks.
type ActionHandler = Box<dyn Fn(&ActionRequest) + Send + Sync>;

/// Dispatches [`accesskit::ActionRequest`]s to registered OxiUI handler callbacks.
///
/// Handlers receive an immutable reference to the action request; the
/// dispatcher iterates all handlers in registration order.  Multiple handlers
/// may be registered and all will be called for each dispatched request.
///
/// # Example
///
/// ```rust
/// use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
/// use accesskit::{Action, ActionRequest, NodeId, TreeId};
/// use oxiui_accessibility::ActionDispatcher;
///
/// let called = Arc::new(AtomicBool::new(false));
/// let called2 = Arc::clone(&called);
///
/// let mut dispatcher = ActionDispatcher::new();
/// dispatcher.on_action(move |_req| { called2.store(true, Ordering::SeqCst); });
///
/// let req = ActionRequest {
///     action: Action::Click,
///     target_tree: TreeId::ROOT,
///     target_node: NodeId(1),
///     data: None,
/// };
/// dispatcher.dispatch(&req);
/// assert!(called.load(Ordering::SeqCst));
/// ```
#[derive(Default)]
pub struct ActionDispatcher {
    handlers: Vec<ActionHandler>,
}

impl ActionDispatcher {
    /// Create an empty dispatcher with no registered handlers.
    pub fn new() -> Self {
        Self {
            handlers: Vec::new(),
        }
    }

    /// Register a handler to be called for every dispatched action request.
    ///
    /// Handlers are called in registration order.  They receive a shared
    /// reference to the [`ActionRequest`] so no cloning is required.
    pub fn on_action(&mut self, handler: impl Fn(&ActionRequest) + Send + Sync + 'static) {
        self.handlers.push(Box::new(handler));
    }

    /// Dispatch `req` to all registered handlers.
    ///
    /// Every registered handler is called in registration order.  If no
    /// handlers are registered this is a no-op.
    pub fn dispatch(&self, req: &ActionRequest) {
        for handler in &self.handlers {
            handler(req);
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use accesskit::{NodeId, TreeId};

    fn req(action: Action, data: Option<ActionData>) -> ActionRequest {
        ActionRequest {
            action,
            target_tree: TreeId::ROOT,
            target_node: NodeId(1),
            data,
        }
    }

    #[test]
    fn test_map_action_click() {
        let r = req(Action::Click, None);
        assert_eq!(map_action(&r), Some(A11yAction::Click));
    }

    #[test]
    fn test_map_action_set_value() {
        let r = req(Action::SetValue, Some(ActionData::Value("hello".into())));
        assert_eq!(
            map_action(&r),
            Some(A11yAction::SetValue("hello".to_string()))
        );
    }

    #[test]
    fn test_map_action_unknown_returns_none() {
        let r = req(Action::Blur, None);
        assert_eq!(map_action(&r), None);
    }

    #[test]
    fn test_map_action_focus() {
        let r = req(Action::Focus, None);
        assert_eq!(map_action(&r), Some(A11yAction::Focus));
    }

    #[test]
    fn test_map_action_increment() {
        let r = req(Action::Increment, None);
        assert_eq!(map_action(&r), Some(A11yAction::Increment));
    }

    #[test]
    fn test_map_action_decrement() {
        let r = req(Action::Decrement, None);
        assert_eq!(map_action(&r), Some(A11yAction::Decrement));
    }

    #[test]
    fn test_map_action_scroll_into_view() {
        let r = req(Action::ScrollIntoView, None);
        assert_eq!(map_action(&r), Some(A11yAction::ScrollIntoView));
    }

    #[test]
    fn test_map_action_custom() {
        let r = req(Action::CustomAction, Some(ActionData::CustomAction(42)));
        assert_eq!(
            map_action(&r),
            Some(A11yAction::Custom("custom:42".to_string()))
        );
    }
}
