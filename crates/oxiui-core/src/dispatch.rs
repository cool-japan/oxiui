//! Event dispatch with W3C-style capture and bubble phases.
//!
//! [`EventDispatcher`] routes a [`DispatchEvent`] from the tree root down to a
//! target node (the *capture* phase) and back up to the root (the *bubble*
//! phase), invoking the handlers registered for each node along the way. A
//! handler returns a [`Propagation`] result; once `stop_propagation` is set the
//! dispatcher visits no further nodes.
//!
//! ## Handler-safe mutation during dispatch
//!
//! Handlers commonly want to add or remove handlers as a side effect (e.g. a
//! "close" button that detaches its own listener). Mutating the handler list
//! while iterating it is a classic use-after-free / skipped-element bug. The
//! dispatcher avoids it with a **collect-then-apply** protocol: the live
//! registry is never borrowed mutably during a dispatch. Instead, handlers push
//! `RegistryEdit`s into a deferred queue carried by [`HandlerCtx`]; the queue
//! is drained and applied to the registry only after the whole dispatch
//! finishes. Adds and removes therefore take effect on the *next* event, never
//! mid-flight.

use crate::events::{KeyboardEvent, MouseEvent, Propagation, TouchEvent};
use crate::tree::{WidgetId, WidgetTree};

/// The kinds of input events the dispatcher routes.
#[derive(Clone, Debug, PartialEq)]
pub enum DispatchEvent {
    /// A pointer event.
    Mouse(MouseEvent),
    /// A keyboard event.
    Keyboard(KeyboardEvent),
    /// A touch event.
    Touch(TouchEvent),
}

/// The dispatch phase in which a handler is being invoked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Travelling root → target. Capture handlers fire here.
    Capture,
    /// At the target node itself.
    Target,
    /// Travelling target → root. Bubble handlers fire here.
    Bubble,
}

/// A deferred edit to the handler registry, queued by a handler during dispatch
/// and applied after the dispatch completes.
enum RegistryEdit {
    /// Add a handler to a node for a phase.
    Add {
        /// Node the handler is attached to.
        id: WidgetId,
        /// Phase the handler listens in.
        phase: Phase,
        /// The handler to install.
        handler: Box<dyn EventHandler>,
    },
    /// Remove every handler registered on a node.
    RemoveAll {
        /// Node to clear.
        id: WidgetId,
    },
}

/// Context handed to a handler during dispatch.
///
/// A handler reads the event and the current node/phase, and may queue registry
/// edits (which apply only after dispatch). It returns its desired
/// [`Propagation`] from [`EventHandler::handle`].
pub struct HandlerCtx<'a> {
    /// The event being dispatched.
    pub event: &'a DispatchEvent,
    /// The node whose handler is currently running.
    pub current: WidgetId,
    /// The phase this invocation belongs to.
    pub phase: Phase,
    /// The eventual target node (deepest in the path).
    pub target: WidgetId,
    /// Deferred registry edits queued by handlers (applied post-dispatch).
    pending: &'a mut Vec<RegistryEdit>,
}

impl HandlerCtx<'_> {
    /// Queue a handler to be added to `id` for `phase` after dispatch finishes.
    pub fn add_handler(&mut self, id: WidgetId, phase: Phase, handler: Box<dyn EventHandler>) {
        self.pending.push(RegistryEdit::Add { id, phase, handler });
    }

    /// Queue removal of every handler on `id` after dispatch finishes.
    ///
    /// This is the *safe* way to "unregister during dispatch": the removal is
    /// recorded now and applied once iteration is complete, so the handler list
    /// is never mutated while it is being walked.
    pub fn remove_handlers(&mut self, id: WidgetId) {
        self.pending.push(RegistryEdit::RemoveAll { id });
    }
}

/// A typed event handler attached to a node.
pub trait EventHandler {
    /// Handle `ctx.event` and return propagation control.
    fn handle(&mut self, ctx: &mut HandlerCtx<'_>) -> Propagation;
}

/// Blanket impl so plain closures can be used as handlers.
impl<F> EventHandler for F
where
    F: FnMut(&mut HandlerCtx<'_>) -> Propagation,
{
    fn handle(&mut self, ctx: &mut HandlerCtx<'_>) -> Propagation {
        self(ctx)
    }
}

/// Per-node handler lists, split by phase.
#[derive(Default)]
struct NodeHandlers {
    capture: Vec<Box<dyn EventHandler>>,
    bubble: Vec<Box<dyn EventHandler>>,
}

/// Routes events through capture/bubble phases over a [`WidgetTree`].
///
/// The dispatcher owns the handler registry but borrows the tree only
/// immutably (to compute the ancestor path), so a caller can keep mutating the
/// tree between dispatches.
///
/// ## Allocation-free fast path
///
/// The path buffer and the deferred-edit buffer are both held as pre-allocated
/// `Vec`s that are cleared and reused across dispatches. This means that after
/// the first event of each size class, dispatch is completely heap-allocation-free
/// on the hot path (no new `Vec` allocations during capture/bubble traversal).
pub struct EventDispatcher {
    handlers: std::collections::HashMap<WidgetId, NodeHandlers>,
    /// Re-used scratch buffer: the ancestor path (root → target).
    /// Cleared and refilled on every [`dispatch`](EventDispatcher::dispatch) call.
    path_scratch: Vec<WidgetId>,
    /// Re-used scratch buffer: deferred registry edits queued by handlers.
    /// Cleared and drained on every [`dispatch`](EventDispatcher::dispatch) call.
    pending_scratch: Vec<RegistryEdit>,
}

impl Default for EventDispatcher {
    fn default() -> Self {
        Self {
            handlers: std::collections::HashMap::new(),
            // Pre-allocate for 32-node deep trees (typical UI depth is 5–15).
            path_scratch: Vec::with_capacity(32),
            // Pre-allocate for a handful of edits per dispatch (rarely > 2).
            pending_scratch: Vec::with_capacity(4),
        }
    }
}

impl EventDispatcher {
    /// Create an empty dispatcher.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a capture-phase handler on `id`.
    pub fn on_capture(&mut self, id: WidgetId, handler: Box<dyn EventHandler>) {
        self.handlers.entry(id).or_default().capture.push(handler);
    }

    /// Register a bubble-phase handler on `id`. Target-phase handlers are
    /// registered here too (the target node fires its bubble handlers in the
    /// [`Phase::Target`] step).
    pub fn on_bubble(&mut self, id: WidgetId, handler: Box<dyn EventHandler>) {
        self.handlers.entry(id).or_default().bubble.push(handler);
    }

    /// Remove every handler registered on `id`. Returns `true` if any existed.
    pub fn clear_node(&mut self, id: WidgetId) -> bool {
        self.handlers.remove(&id).is_some()
    }

    /// Total number of nodes with at least one registered handler.
    pub fn registered_nodes(&self) -> usize {
        self.handlers.len()
    }

    /// Compute the capture path root → target into `out` (cleared first).
    ///
    /// Uses the pre-allocated scratch buffer to avoid per-dispatch heap allocation.
    fn path_to_reuse(tree: &WidgetTree, target: WidgetId, out: &mut Vec<WidgetId>) {
        out.clear();
        let mut cur = tree.get(target);
        while let Some(node) = cur {
            out.push(node.id);
            cur = node.parent.and_then(|p| tree.get(p));
        }
        out.reverse(); // root → target
    }

    /// Dispatch `event` to `target`, running the capture phase (root → target),
    /// the target phase, then the bubble phase (target → root).
    ///
    /// Returns the merged [`Propagation`] of every handler that ran. Dispatch
    /// stops early as soon as a handler sets `stop_propagation`. Registry edits
    /// queued by handlers are applied only after this call returns.
    ///
    /// This method is **allocation-free on the hot path** after the first call:
    /// it reuses pre-allocated scratch buffers for both the ancestor path and
    /// the deferred-edit queue, so no heap allocation occurs during traversal.
    pub fn dispatch(
        &mut self,
        tree: &WidgetTree,
        target: WidgetId,
        event: DispatchEvent,
    ) -> Propagation {
        // Fill the path into the pre-allocated scratch buffer — no allocation.
        // We must extract the buffer temporarily to avoid a split-borrow conflict
        // between `self.path_scratch` (mutated) and `self.handlers` (read below).
        let mut path = std::mem::take(&mut self.path_scratch);
        Self::path_to_reuse(tree, target, &mut path);

        if path.is_empty() {
            self.path_scratch = path;
            return Propagation::CONTINUE;
        }

        // Similarly extract the pending-edits buffer.
        let mut pending = std::mem::take(&mut self.pending_scratch);
        pending.clear();

        let mut result = Propagation::CONTINUE;
        let actual_target = path.last().copied().unwrap_or(target);

        // ── Capture phase: root → target (exclusive of the target). ──────────
        'capture: for &id in path.iter().take(path.len().saturating_sub(1)) {
            // Take the node's capture handlers OUT of the registry for the
            // duration of iteration, so handlers may freely queue edits that
            // touch the same node without aliasing the list we're walking.
            let mut taken = match self.handlers.get_mut(&id) {
                Some(h) if !h.capture.is_empty() => std::mem::take(&mut h.capture),
                _ => continue,
            };
            for handler in taken.iter_mut() {
                let mut ctx = HandlerCtx {
                    event: &event,
                    current: id,
                    phase: Phase::Capture,
                    target: actual_target,
                    pending: &mut pending,
                };
                let prop = handler.handle(&mut ctx);
                result = result.merge(prop);
                if prop.stop_propagation {
                    self.restore_capture(id, taken);
                    break 'capture;
                }
            }
            self.restore_capture(id, taken);
        }

        if !result.stop_propagation {
            // ── Target + bubble phase: target → root. ────────────────────────
            'bubble: for (i, &id) in path.iter().rev().enumerate() {
                let phase = if i == 0 { Phase::Target } else { Phase::Bubble };
                let mut taken = match self.handlers.get_mut(&id) {
                    Some(h) if !h.bubble.is_empty() => std::mem::take(&mut h.bubble),
                    _ => continue,
                };
                for handler in taken.iter_mut() {
                    let mut ctx = HandlerCtx {
                        event: &event,
                        current: id,
                        phase,
                        target: actual_target,
                        pending: &mut pending,
                    };
                    let prop = handler.handle(&mut ctx);
                    result = result.merge(prop);
                    if prop.stop_propagation {
                        self.restore_bubble(id, taken);
                        break 'bubble;
                    }
                }
                self.restore_bubble(id, taken);
            }
        }

        self.apply_pending(&mut pending);

        // Return the scratch buffers so the next call reuses the allocations.
        // Clear pending (apply_pending already drained it) but keep capacity.
        pending.clear();
        self.pending_scratch = pending;
        self.path_scratch = path;

        result
    }

    /// Put captured capture-phase handlers back, preserving any added during
    /// dispatch via the pending queue (those are applied separately).
    fn restore_capture(&mut self, id: WidgetId, mut taken: Vec<Box<dyn EventHandler>>) {
        let slot = self.handlers.entry(id).or_default();
        // Anything pushed onto the (now-empty) live list while we iterated is
        // impossible because handlers only queue edits; but be defensive and
        // prepend the original handlers ahead of any concurrently-added ones.
        taken.append(&mut slot.capture);
        slot.capture = taken;
    }

    /// Put captured bubble-phase handlers back.
    fn restore_bubble(&mut self, id: WidgetId, mut taken: Vec<Box<dyn EventHandler>>) {
        let slot = self.handlers.entry(id).or_default();
        taken.append(&mut slot.bubble);
        slot.bubble = taken;
    }

    /// Apply deferred registry edits queued during dispatch.
    ///
    /// Drains the edits from `pending` in-place so the buffer's capacity is
    /// preserved for the next dispatch call.
    fn apply_pending(&mut self, pending: &mut Vec<RegistryEdit>) {
        for edit in pending.drain(..) {
            match edit {
                RegistryEdit::Add { id, phase, handler } => match phase {
                    Phase::Capture => self.on_capture(id, handler),
                    Phase::Bubble | Phase::Target => self.on_bubble(id, handler),
                },
                RegistryEdit::RemoveAll { id } => {
                    self.handlers.remove(&id);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{Modifiers, MouseButton};
    use crate::geometry::{Point, Rect};
    use std::cell::RefCell;
    use std::rc::Rc;

    fn mouse_down() -> DispatchEvent {
        DispatchEvent::Mouse(MouseEvent::Down {
            pos: Point::new(5.0, 5.0),
            button: MouseButton::Left,
            modifiers: Modifiers::NONE,
        })
    }

    /// root → a → target tree.
    fn linear_tree() -> (WidgetTree, WidgetId, WidgetId) {
        let mut t = WidgetTree::new(Rect::new(0.0, 0.0, 100.0, 100.0));
        let a = t
            .insert(WidgetId::ROOT, Rect::new(0.0, 0.0, 50.0, 50.0))
            .expect("root");
        let target = t.insert(a, Rect::new(0.0, 0.0, 20.0, 20.0)).expect("a");
        (t, a, target)
    }

    #[test]
    fn capture_then_bubble_ordering() {
        let (tree, a, target) = linear_tree();
        let log = Rc::new(RefCell::new(Vec::<String>::new()));
        let mut d = EventDispatcher::new();

        for (id, name) in [(WidgetId::ROOT, "root"), (a, "a"), (target, "target")] {
            let log_c = Rc::clone(&log);
            d.on_capture(
                id,
                Box::new(move |ctx: &mut HandlerCtx<'_>| {
                    log_c
                        .borrow_mut()
                        .push(format!("cap:{name}:{:?}", ctx.phase));
                    Propagation::CONTINUE
                }),
            );
            let log_b = Rc::clone(&log);
            d.on_bubble(
                id,
                Box::new(move |ctx: &mut HandlerCtx<'_>| {
                    log_b
                        .borrow_mut()
                        .push(format!("bub:{name}:{:?}", ctx.phase));
                    Propagation::CONTINUE
                }),
            );
        }

        d.dispatch(&tree, target, mouse_down());
        let seen = log.borrow().clone();
        assert_eq!(
            seen,
            vec![
                // capture root → a (target excluded from capture loop)
                "cap:root:Capture",
                "cap:a:Capture",
                // target + bubble target → root
                "bub:target:Target",
                "bub:a:Bubble",
                "bub:root:Bubble",
            ]
        );
    }

    #[test]
    fn stop_propagation_halts_bubble() {
        let (tree, a, target) = linear_tree();
        let log = Rc::new(RefCell::new(Vec::<String>::new()));
        let mut d = EventDispatcher::new();

        let log_t = Rc::clone(&log);
        d.on_bubble(
            target,
            Box::new(move |_: &mut HandlerCtx<'_>| {
                log_t.borrow_mut().push("target".to_string());
                Propagation::stop() // stop here; `a` and root must NOT fire
            }),
        );
        let log_a = Rc::clone(&log);
        d.on_bubble(
            a,
            Box::new(move |_: &mut HandlerCtx<'_>| {
                log_a.borrow_mut().push("a".to_string());
                Propagation::CONTINUE
            }),
        );

        let result = d.dispatch(&tree, target, mouse_down());
        assert!(result.stop_propagation);
        assert_eq!(*log.borrow(), vec!["target".to_string()]);
    }

    #[test]
    fn prevent_default_is_reported() {
        let (tree, _a, target) = linear_tree();
        let mut d = EventDispatcher::new();
        d.on_bubble(
            target,
            Box::new(|_: &mut HandlerCtx<'_>| Propagation::prevent()),
        );
        let result = d.dispatch(&tree, target, mouse_down());
        assert!(result.prevent_default);
        assert!(!result.stop_propagation);
    }

    #[test]
    fn handler_removal_during_dispatch_is_deferred() {
        let (tree, _a, target) = linear_tree();
        let count = Rc::new(RefCell::new(0u32));
        let mut d = EventDispatcher::new();

        // Handler removes itself on first fire. Because removal is deferred, it
        // still fires exactly once here; on the *second* dispatch it is gone.
        let count_c = Rc::clone(&count);
        d.on_bubble(
            target,
            Box::new(move |ctx: &mut HandlerCtx<'_>| {
                *count_c.borrow_mut() += 1;
                ctx.remove_handlers(target); // safe: applied after dispatch
                Propagation::CONTINUE
            }),
        );

        d.dispatch(&tree, target, mouse_down());
        assert_eq!(*count.borrow(), 1);
        assert_eq!(
            d.registered_nodes(),
            0,
            "handler should be removed post-dispatch"
        );

        // Second dispatch: no handler remains, count unchanged.
        d.dispatch(&tree, target, mouse_down());
        assert_eq!(*count.borrow(), 1);
    }

    #[test]
    fn handler_add_during_dispatch_is_deferred() {
        let (tree, _a, target) = linear_tree();
        let fired = Rc::new(RefCell::new(Vec::<&'static str>::new()));
        let mut d = EventDispatcher::new();

        let fired_outer = Rc::clone(&fired);
        let fired_inner = Rc::clone(&fired);
        d.on_bubble(
            target,
            Box::new(move |ctx: &mut HandlerCtx<'_>| {
                fired_outer.borrow_mut().push("outer");
                let f = Rc::clone(&fired_inner);
                // Add a new handler mid-dispatch; must NOT fire this dispatch.
                ctx.add_handler(
                    target,
                    Phase::Bubble,
                    Box::new(move |_: &mut HandlerCtx<'_>| {
                        f.borrow_mut().push("inner");
                        Propagation::CONTINUE
                    }),
                );
                Propagation::CONTINUE
            }),
        );

        d.dispatch(&tree, target, mouse_down());
        assert_eq!(
            *fired.borrow(),
            vec!["outer"],
            "added handler must not fire same dispatch"
        );
        d.dispatch(&tree, target, mouse_down());
        // Now both the original and the deferred-added handler fire.
        assert_eq!(*fired.borrow(), vec!["outer", "outer", "inner"]);
    }

    #[test]
    fn dispatch_to_missing_target_is_noop() {
        let (tree, _a, _t) = linear_tree();
        let mut d = EventDispatcher::new();
        let prop = d.dispatch(&tree, WidgetId(9999), mouse_down());
        assert_eq!(prop, Propagation::CONTINUE);
    }
}
