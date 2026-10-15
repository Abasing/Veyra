//! `veyra-dom` — Veyra's DOM core: the arena tree, origin-tagged
//! documents, mutation algorithms, and the events skeleton.
//!
//! This is the system's stable core (dependency-graph §1 invariant 2): it
//! depends on nothing in the engine except [`veyra-url`], because origins
//! live on nodes. Storage is a slot arena with generational ids — the
//! shape the future GC integration (Phase 5) and the WebIDL binding work
//! build on — with the normative node-count limit from the engineering
//! standards limits table (2,000,000, configurable).
//!
//! Scope (roadmap Phase 2):
//!
//! - node kinds: document (origin-tagged at construction — the origin
//!   invariant), doctype, element (namespaced names, attributes, template
//!   contents), text, comment, fragment;
//! - mutation algorithms: append / insert-before / remove / detach with
//!   cycle rejection, parent-type rules, and depth enforcement; text
//!   coalescing for the parser;
//! - traversal: children, ancestors, pre-order descendants (iterative —
//!   deep trees cannot overflow the stack);
//! - events skeleton: core event flags, listener registry, dispatch with
//!   capture/target/bubble and cancelation;
//! - typed errors everywhere: stale ids are reported, never aliased, and
//!   no operation panics on any tree shape.
//!
//! What is deliberately absent: CSS/JS-facing APIs (Phase 3/4), WebIDL
//! reflections, live `NodeList`s, and garbage collection — the arena keeps
//! detached subtrees observable exactly like the real DOM, and reclamation
//! is either explicit (`Tree::destroy_subtree`) or whole-tree drop.
//!
//! # Examples
//!
//! ```
//! use veyra_dom::{Tree, QualName};
//! use veyra_url::{Origin, Url};
//!
//! let url = Url::parse("https://example.org/doc").expect("valid url");
//! let mut tree = Tree::new_document(url.origin()).expect("fresh tree");
//! let doc = tree.document().expect("document exists");
//!
//! let html = tree.create_element(QualName::html("html"), vec![]).expect("limit");
//! tree.append(doc, html).expect("valid hierarchy");
//! let text = tree.append_text(html, "hello").expect("valid hierarchy");
//! assert_eq!(tree.origin_of(text).map(veyra_url::Origin::ascii_serialization),
//!            Some("https://example.org".to_string()));
//! ```

#![forbid(unsafe_code)]

pub mod arena;
pub mod error;
pub mod events;
pub mod node;

use veyra_url::Origin;

pub use arena::{
    DEFAULT_MAX_DEPTH, DEFAULT_MAX_LISTENERS_PER_NODE, DEFAULT_MAX_NODES, NodeId, Tree,
};
pub use error::DomError;
pub use events::{Event, ListenerId};
pub use node::{Attribute, Namespace, NodeData, QualName, QuirksMode};

/// Re-exported so downstream crates can name the origin type carried by
/// documents without depending on `veyra-url` twice in their manifests —
/// the type itself is still owned by `veyra-url` (single URL authority,
/// dependency-graph §5).
pub use veyra_url::Origin as DocumentOrigin;

/// Constructs the origin of a document loaded from `url` — the only way
/// engine code should derive a document origin (parse-derived, never
/// synthesized).
#[must_use]
pub fn origin_of_url(url: &veyra_url::Url) -> Origin {
    url.origin()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use veyra_url::Url;

    fn test_tree() -> Tree {
        let url = Url::parse("https://example.org/").unwrap();
        Tree::new_document(url.origin()).unwrap()
    }

    #[test]
    fn document_carries_origin_and_nodes_inherit_it() {
        let mut tree = test_tree();
        let doc = tree.document().unwrap();
        let html = tree.create_element(QualName::html("html"), vec![]).unwrap();
        tree.append(doc, html).unwrap();
        let text = tree.append_text(html, "x").unwrap();

        assert!(matches!(
            tree.data(doc),
            Some(NodeData::Document {
                quirks: QuirksMode::Yes,
                ..
            })
        ));
        assert_eq!(
            tree.origin_of(text)
                .map(Origin::ascii_serialization)
                .as_deref(),
            Some("https://example.org")
        );
    }

    #[test]
    fn append_moves_between_parents_and_rejects_cycles() {
        let mut tree = test_tree();
        let doc = tree.document().unwrap();
        let a = tree.create_element(QualName::html("a"), vec![]).unwrap();
        let b = tree.create_element(QualName::html("b"), vec![]).unwrap();
        tree.append(doc, a).unwrap();
        tree.append(a, b).unwrap();
        assert_eq!(tree.children(a).unwrap().len(), 1);

        // Moving b from a to doc works.
        tree.append(doc, b).unwrap();
        assert_eq!(tree.children(a).unwrap().len(), 0);
        assert_eq!(tree.children(doc).unwrap().len(), 2);

        // Cycles: appending doc under b would make doc its own descendant.
        assert_eq!(tree.append(b, doc), Err(DomError::HierarchyRequest));
        // Self-append.
        assert_eq!(tree.append(a, a), Err(DomError::HierarchyRequest));
    }

    #[test]
    fn insert_before_orders_children() {
        let mut tree = test_tree();
        let doc = tree.document().unwrap();
        let a = tree.create_element(QualName::html("a"), vec![]).unwrap();
        let b = tree.create_element(QualName::html("b"), vec![]).unwrap();
        let c = tree.create_element(QualName::html("c"), vec![]).unwrap();
        tree.append(doc, a).unwrap();
        tree.append(doc, c).unwrap();
        tree.insert_before(doc, b, Some(c)).unwrap();
        assert_eq!(tree.children(doc).unwrap(), &[a, b, c]);

        // reference=None appends.
        let d = tree.create_element(QualName::html("d"), vec![]).unwrap();
        tree.insert_before(doc, d, None).unwrap();
        assert_eq!(tree.children(doc).unwrap(), &[a, b, c, d]);

        // Reference must be a child of the parent — here the *reference* is
        // invalid (the stray element is not doc's child).
        let stray = tree.create_element(QualName::html("e"), vec![]).unwrap();
        assert_eq!(
            tree.insert_before(doc, stray, Some(stray)),
            Err(DomError::NotAChild)
        );
    }

    #[test]
    fn text_coalescing_merges_runs() {
        let mut tree = test_tree();
        let doc = tree.document().unwrap();
        let t1 = tree.append_text(doc, "hello ").unwrap();
        let t2 = tree.append_text(doc, "world").unwrap();
        assert_eq!(t1, t2, "adjacent text merges into one node");
        assert_eq!(tree.live_nodes(), 2); // document + one text node

        // A non-text sibling breaks the run.
        let br = tree.create_element(QualName::html("br"), vec![]).unwrap();
        tree.append(doc, br).unwrap();
        let t3 = tree.append_text(doc, "!").unwrap();
        assert_ne!(t1, t3);
    }

    #[test]
    fn remove_detaches_but_preserves_subtree() {
        let mut tree = test_tree();
        let doc = tree.document().unwrap();
        let a = tree.create_element(QualName::html("a"), vec![]).unwrap();
        let b = tree.create_element(QualName::html("b"), vec![]).unwrap();
        tree.append(doc, a).unwrap();
        tree.append(a, b).unwrap();

        tree.remove(doc, a).unwrap();
        assert_eq!(tree.parent(a), None);
        assert_eq!(tree.parent(b), Some(a), "detached subtree stays intact");
        assert_eq!(tree.live_nodes(), 3);

        // Not-a-child is a typed error.
        assert_eq!(tree.remove(doc, a), Err(DomError::NotAChild));
    }

    #[test]
    fn stale_ids_are_detected_not_aliased() {
        let mut tree = test_tree();
        let doomed = tree.create_element(QualName::html("x"), vec![]).unwrap();
        tree.destroy_subtree(doomed).unwrap();

        // The slot will be reused by the next allocation; the old id must
        // not alias it.
        let fresh = tree.create_element(QualName::html("y"), vec![]).unwrap();
        assert!(!tree.is_live(doomed));
        assert!(tree.data(doomed).is_none());
        assert_ne!(doomed, fresh);
        assert_eq!(
            tree.append(tree.document().unwrap(), doomed),
            Err(DomError::StaleNodeId)
        );
    }

    #[test]
    fn depth_limit_enforced_on_mutations() {
        let url = Url::parse("https://example.org/").unwrap();
        let mut tree = Tree::new_document_with_limits(url.origin(), 1000, 3).unwrap();
        let doc = tree.document().unwrap();
        let mut chain = vec![doc];
        for _ in 0..3 {
            let el = tree.create_element(QualName::html("div"), vec![]).unwrap();
            tree.append(*chain.last().unwrap(), el).unwrap();
            chain.push(el);
        }
        // depth is now 3; a fourth level must be rejected.
        let deep = tree.create_element(QualName::html("div"), vec![]).unwrap();
        assert_eq!(
            tree.append(*chain.last().unwrap(), deep),
            Err(DomError::HierarchyRequest)
        );
    }

    #[test]
    fn node_limit_enforced() {
        let url = Url::parse("https://example.org/").unwrap();
        let mut tree = Tree::new_document_with_limits(url.origin(), 3, 512).unwrap();
        tree.create_element(QualName::html("a"), vec![]).unwrap();
        tree.create_element(QualName::html("b"), vec![]).unwrap();
        assert_eq!(
            tree.create_element(QualName::html("c"), vec![]),
            Err(DomError::NodeLimitExceeded { max: 3 })
        );
    }

    #[test]
    fn attributes_round_trip_with_first_wins_duplicates() {
        let mut tree = test_tree();
        let doc = tree.document().unwrap();
        let el = tree.create_element(QualName::html("div"), vec![]).unwrap();
        tree.append(doc, el).unwrap();
        tree.set_attribute(el, QualName::html("class"), "a")
            .unwrap();
        tree.set_attribute(el, QualName::html("class"), "b")
            .unwrap();
        tree.set_attribute(el, QualName::html("id"), "x").unwrap();
        assert_eq!(tree.get_attribute(el, "class"), Some("b"));
        assert_eq!(tree.get_attribute(el, "id"), Some("x"));
        assert_eq!(tree.get_attribute(el, "missing"), None);
        // get_attribute on a non-element is None, not a panic.
        assert_eq!(tree.get_attribute(doc, "class"), None);
    }

    #[test]
    fn dispatch_runs_capture_target_bubble_in_order() {
        let mut tree = test_tree();
        let doc = tree.document().unwrap();
        let a = tree.create_element(QualName::html("a"), vec![]).unwrap();
        tree.append(doc, a).unwrap();

        let order: std::sync::Arc<std::sync::Mutex<Vec<&'static str>>> = Default::default();
        let capture_doc = order.clone();
        tree.add_listener(doc, true, move |_event| {
            capture_doc.lock().unwrap().push("capture:doc");
        })
        .unwrap();
        let bubble_doc = order.clone();
        tree.add_listener(doc, false, move |_event| {
            bubble_doc.lock().unwrap().push("bubble:doc");
        })
        .unwrap();
        let target_a = order.clone();
        tree.add_listener(a, false, move |event| {
            target_a.lock().unwrap().push("target:a");
            event.prevent_default();
        })
        .unwrap();

        let mut event = Event::new("click", true, true);
        let canceled = tree.dispatch(a, &mut event).unwrap();
        assert!(canceled && event.default_prevented());
        assert_eq!(
            *order.lock().unwrap(),
            vec!["capture:doc", "target:a", "bubble:doc"]
        );
    }

    #[test]
    fn non_bubbling_events_skip_the_bubble_phase() {
        let mut tree = test_tree();
        let doc = tree.document().unwrap();
        let a = tree.create_element(QualName::html("a"), vec![]).unwrap();
        tree.append(doc, a).unwrap();

        let hits: std::sync::Arc<std::sync::Mutex<Vec<&'static str>>> = Default::default();
        let doc_listener = hits.clone();
        tree.add_listener(doc, false, move |_event| {
            doc_listener.lock().unwrap().push("doc");
        })
        .unwrap();
        let a_listener = hits.clone();
        tree.add_listener(a, false, move |_event| {
            a_listener.lock().unwrap().push("a");
        })
        .unwrap();

        let mut event = Event::new("focus", false, false);
        let canceled = tree.dispatch(a, &mut event).unwrap();
        assert!(!canceled);
        assert_eq!(*hits.lock().unwrap(), vec!["a"]);
    }

    #[test]
    fn descendants_walk_pre_order() {
        let mut tree = test_tree();
        let doc = tree.document().unwrap();
        let a = tree.create_element(QualName::html("a"), vec![]).unwrap();
        let b = tree.create_element(QualName::html("b"), vec![]).unwrap();
        let c = tree.create_element(QualName::html("c"), vec![]).unwrap();
        tree.append(doc, a).unwrap();
        tree.append(a, b).unwrap();
        tree.append(a, c).unwrap();

        let order: Vec<NodeId> = tree.descendants(doc).collect();
        assert_eq!(order, vec![doc, a, b, c]);

        // Iteration is iterative, not recursive: it must not overflow the
        // stack on the deepest legal tree (limit 512). Build to the limit
        // under a dedicated limits constructor.
        let url = Url::parse("https://example.org/").unwrap();
        let mut deep = Tree::new_document_with_limits(url.origin(), 2_000_000, 600).unwrap();
        let ddoc = deep.document().unwrap();
        let mut cursor = ddoc;
        for _ in 0..600 {
            let next = deep.create_element(QualName::html("n"), vec![]).unwrap();
            deep.append(cursor, next).unwrap();
            cursor = next;
        }
        let walked: usize = deep.descendants(ddoc).count();
        assert_eq!(walked, 601); // document + 600 elements
    }
}
