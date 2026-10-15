//! Events skeleton.
//!
//! Phase 2 scope per the roadmap: the shape the script phase will inherit
//! — an [`Event`] with the standard's core flags, per-node listener
//! registration, and capture/target/bubble dispatch through the tree.
//! Intentionally absent until there are event producers and WebIDL
//! bindings (Phase 4/5): `EventTarget` as a WebIDL type, trusted-event
//! gating, composed-path/retargeting, and listener options beyond
//! `capture`. What exists is enough to test dispatch order and
//! cancelation semantics, and nothing more is claimed.

use std::fmt;

/// A dispatched event (skeleton).
///
/// Handlers receive `&mut Event` and may call [`Event::prevent_default`]
/// (honored when the event is cancelable) or
/// [`Event::stop_propagation_later`] to end further dispatch phases.
#[derive(Clone, Debug)]
pub struct Event {
    /// The event type name (e.g. `"click"`).
    pub event_type: String,
    /// Whether the event bubbles beyond the target.
    pub bubbles: bool,
    /// Whether handlers may cancel the default action.
    pub cancelable: bool,
    pub(crate) canceled: bool,
    pub(crate) stop_dispatch: bool,
}

impl Event {
    /// Creates a new event.
    pub fn new(event_type: impl Into<String>, bubbles: bool, cancelable: bool) -> Self {
        Self {
            event_type: event_type.into(),
            bubbles,
            cancelable,
            canceled: false,
            stop_dispatch: false,
        }
    }

    /// Cancels the default action (only meaningful when `cancelable`).
    pub fn prevent_default(&mut self) {
        self.canceled = true;
    }

    /// Stops the remaining dispatch phases (skeleton stand-in for the
    /// standard's stop-propagation/stop-immediate-propagation pair).
    pub fn stop_propagation_later(&mut self) {
        self.stop_dispatch = true;
    }

    /// Whether a handler canceled the default action.
    pub fn default_prevented(&self) -> bool {
        self.canceled
    }
}

impl fmt::Display for Event {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Event({})", self.event_type)
    }
}

/// A registered listener (handler + dispatch flags).
pub(crate) struct EventListener {
    pub(crate) id: ListenerId,
    pub(crate) capture: bool,
    pub(crate) handler: Box<dyn FnMut(&mut Event) + 'static>,
}

/// Registry id of a listener (removal arrives with the binding phase).
pub type ListenerId = u64;
