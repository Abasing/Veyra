//! The browser layer: navigation/session state machine, document
//! loading, crash-safe session persistence.
//!
//! Placement per the dependency graph (`docs/architecture/dependency-graph.md`
//! §1): navigation/session lives in the shell→browser layer — the shell is
//! a root, and only it composes the services. The state machine is
//! browser-owned: every transition is a validated method call, callers
//! cannot construct illegal phase changes, and history mutations (forward
//! truncation on new navigation, bounds on back/forward) happen nowhere
//! else.

pub mod loader;
pub mod navigation;
pub mod session;

pub use loader::{LoadError, LoadedDocument, load_document};
pub use navigation::{NavPhase, NavigationController, NavigationError, SessionEntry};
pub use session::{
    SessionError, SessionFileState, entries_from_state, load_session, state_from_history,
    write_session,
};
