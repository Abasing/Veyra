//! Typed errors for DOM operations.
//!
//! Every fallible tree operation returns a [`DomError`] rather than
//! panicking: node ids can go stale (use-after-remove is an ordinary
//! programming situation the API answers, not a crash), limits are
//! configuration, and hierarchy checks are the mutation algorithms'
//! contract (engineering standards §2, §6).

use std::fmt;

/// The single error type of `veyra-dom` operations.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DomError {
    /// The node-count limit was reached; the operation that would have
    /// created another node was rejected. The tree is unchanged.
    NodeLimitExceeded {
        /// The configured maximum number of live nodes.
        max: usize,
    },
    /// A [`NodeId`](crate::NodeId) referred to a slot that has been freed
    /// and reused for a different node (stale id). Nothing was mutated.
    StaleNodeId,
    /// The referenced node is not attached to the tree this operation
    /// applies to (e.g. appending a node owned by another arena).
    ForeignNode,
    /// Hierarchy-request failure: the mutation would create a cycle
    /// (inserting a node into its own descendant) or violate a parent-type
    /// rule (children on a text node, document under an element, …).
    HierarchyRequest,
    /// `remove`/`detach` was called with a node whose parent is not the
    /// expected one.
    NotAChild,
    /// An event listener registry limit was reached (defensive bound on
    /// per-node registrations).
    ListenerLimit,
}

impl fmt::Display for DomError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NodeLimitExceeded { max } => {
                write!(f, "node limit of {max} reached")
            }
            Self::StaleNodeId => write!(f, "stale node id (slot reused after removal)"),
            Self::ForeignNode => write!(f, "node belongs to a different tree"),
            Self::HierarchyRequest => write!(f, "hierarchy request violated"),
            Self::NotAChild => write!(f, "node is not a child of the given parent"),
            Self::ListenerLimit => write!(f, "per-node listener limit reached"),
        }
    }
}

impl std::error::Error for DomError {}
