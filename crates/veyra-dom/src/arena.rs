//! The arena tree: node storage, ids, mutation algorithms, traversal.
//!
//! Storage is a slot list with generational ids — a removed node's slot
//! records the removal, so a stale [`NodeId`] is detected instead of
//! aliasing a different node. This is the shape the future GC/binding work
//! (Phase 5) builds on: indices, not pointers, and a single owner (the
//! event-loop thread, engineering standards §5).
//!
//! Mutation algorithms implement the DOM standard's pre-insert/insert/
//! replace/remove contract subset the engine needs at this phase:
//! reparent-on-append, cycle rejection, parent-type rules, and depth
//! enforcement. All failures are typed [`DomError`]s; nothing panics on
//! any input, including hostile tree shapes.

use crate::error::DomError;
use crate::events::{Event, EventListener, ListenerId};
use crate::node::{Attribute, NodeData, QualName, QuirksMode};
use veyra_url::Origin;

/// Default node-count limit (engineering standards §6 limits table —
/// owner: veyra-dom).
pub const DEFAULT_MAX_NODES: usize = 2_000_000;

/// Default element-nesting-depth limit (limits table owner for parser
/// depth is veyra-html; the DOM enforces the same bound on mutations).
pub const DEFAULT_MAX_DEPTH: usize = 512;

/// Hard ceiling for one text node's data (defense in depth behind the
/// parser's normative 8 MiB limit — engineering standards §6).
pub const DEFAULT_MAX_TEXT_DATA: usize = 8 * 1024 * 1024;

/// Default per-node listener limit (defensive bound for the events
/// skeleton; revisited with real event semantics in Phase 4/5).
pub const DEFAULT_MAX_LISTENERS_PER_NODE: usize = 1_000;

/// A handle to a node in a [`Tree`].
///
/// Ids are generation-checked: after a slot is freed and reused, the old
/// id's generation no longer matches and every operation reports
/// [`DomError::StaleNodeId`] instead of touching the new node.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeId {
    pub(crate) index: u32,
    pub(crate) generation: u64,
}

impl NodeId {
    /// A sentinel id that is never live: every tree operation on it
    /// reports [`DomError::StaleNodeId`] (or `None`), making it a safe
    /// "null handle" for sinks that must return a `Handle` when the tree
    /// cannot allocate. Compare with [`Tree::is_live`].
    #[must_use]
    pub const fn null() -> Self {
        Self {
            index: u32::MAX,
            generation: u64::MAX,
        }
    }
}

/// One storage slot. `generation` increments on every free so stale ids are
/// detectable.
struct Slot {
    generation: u64,
    node: Option<NodeEntry>,
}

/// The mutable per-node record.
struct NodeEntry {
    data: NodeData,
    parent: Option<NodeId>,
    children: Vec<NodeId>,
    listeners: Vec<EventListener>,
}

/// The DOM tree: an arena of nodes with one document root.
///
/// Cloning a `NodeId` is cheap; the tree itself is not `Clone` (listeners
/// hold closures). Detached subtrees stay alive in the arena until
/// [`Tree::destroy_subtree`] — mirroring the real DOM, where a detached
/// tree is still observable — and the whole arena is dropped with the
/// document.
pub struct Tree {
    slots: Vec<Slot>,
    free: Vec<u32>,
    live: usize,
    max_nodes: usize,
    max_depth: usize,
    root: Option<NodeId>,
    listener_counter: u64,
}

impl std::fmt::Debug for Tree {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tree")
            .field("live_nodes", &self.live)
            .field("max_nodes", &self.max_nodes)
            .field("max_depth", &self.max_depth)
            .finish_non_exhaustive()
    }
}

impl Tree {
    /// Creates a tree with a document root carrying `origin` and the
    /// default limits. Every tree has exactly one document; origin
    /// construction is parse-only upstream (dependency-graph invariant 5),
    /// so an origin here is always derived from a real URL.
    ///
    /// # Errors
    ///
    /// Never fails with the default limits; the `Result` exists for the
    /// configurable-limit constructor's symmetry.
    pub fn new_document(origin: Origin) -> Result<Self, DomError> {
        Self::new_document_with_limits(origin, DEFAULT_MAX_NODES, DEFAULT_MAX_DEPTH)
    }

    /// Creates a tree with explicit limits (engineering standards §6:
    /// limits are configuration). The node limit is floored at 1 — a tree
    /// without its document is not constructible.
    ///
    /// # Errors
    ///
    /// [`DomError::NodeLimitExceeded`] is impossible here (the limit is
    /// floored first); reserved for constructor symmetry with allocation
    /// paths.
    pub fn new_document_with_limits(
        origin: Origin,
        max_nodes: usize,
        max_depth: usize,
    ) -> Result<Self, DomError> {
        let max_nodes = max_nodes.max(1);
        let mut tree = Self {
            slots: Vec::new(),
            free: Vec::new(),
            live: 0,
            max_nodes,
            max_depth: max_depth.max(1),
            root: None,
            listener_counter: 0,
        };
        let data = NodeData::Document {
            origin,
            quirks: QuirksMode::Yes, // parser sets the real mode via set_quirks_mode
        };
        let root = tree.allocate(data)?;
        tree.root = Some(root);
        Ok(tree)
    }

    /// The document node's id.
    pub fn document(&self) -> Option<NodeId> {
        self.root
    }

    /// The document's origin, if the tree has its document yet.
    pub fn document_origin(&self) -> Option<&Origin> {
        match self.root.and_then(|root| self.data(root)) {
            Some(NodeData::Document { origin, .. }) => Some(origin),
            _ => None,
        }
    }

    /// Number of live nodes.
    pub fn live_nodes(&self) -> usize {
        self.live
    }

    /// The configured node limit.
    pub fn max_nodes(&self) -> usize {
        self.max_nodes
    }

    /// The configured nesting-depth limit.
    pub fn max_depth(&self) -> usize {
        self.max_depth
    }

    /// Sets the document's quirks mode (tree-builder callback).
    pub fn set_quirks_mode(&mut self, quirks: QuirksMode) {
        let Some(root) = self.root else { return };
        if let Some(entry) = self.entry_mut(root)
            && let NodeData::Document { quirks: mode, .. } = &mut entry.data
        {
            *mode = quirks;
        }
    }

    // -- construction ----------------------------------------------------

    /// Creates an element node (unattached).
    ///
    /// # Errors
    ///
    /// [`DomError::NodeLimitExceeded`] when the node limit is reached.
    pub fn create_element(
        &mut self,
        name: QualName,
        attributes: Vec<Attribute>,
    ) -> Result<NodeId, DomError> {
        self.allocate(NodeData::Element {
            name,
            attributes,
            template_contents: None,
        })
    }

    /// Creates a text node (unattached).
    ///
    /// # Errors
    ///
    /// [`DomError::NodeLimitExceeded`] when the node limit is reached.
    pub fn create_text(&mut self, data: impl Into<String>) -> Result<NodeId, DomError> {
        self.allocate(NodeData::Text { data: data.into() })
    }

    /// Creates a comment node (unattached).
    ///
    /// # Errors
    ///
    /// [`DomError::NodeLimitExceeded`] when the node limit is reached.
    pub fn create_comment(&mut self, data: impl Into<String>) -> Result<NodeId, DomError> {
        self.allocate(NodeData::Comment { data: data.into() })
    }

    /// Creates a doctype node (unattached).
    ///
    /// # Errors
    ///
    /// [`DomError::NodeLimitExceeded`] when the node limit is reached.
    pub fn create_doctype(
        &mut self,
        name: impl Into<String>,
        public_id: Option<String>,
        system_id: Option<String>,
    ) -> Result<NodeId, DomError> {
        self.allocate(NodeData::Doctype {
            name: name.into(),
            public_id,
            system_id,
        })
    }

    /// Creates a fragment node (unattached): an insertion context or a
    /// template-contents container.
    ///
    /// # Errors
    ///
    /// [`DomError::NodeLimitExceeded`] when the node limit is reached.
    pub fn create_fragment(&mut self) -> Result<NodeId, DomError> {
        self.allocate(NodeData::Fragment)
    }

    // -- accessors ---------------------------------------------------------

    /// The node's payload, or `None` for a stale/foreign id.
    pub fn data(&self, id: NodeId) -> Option<&NodeData> {
        self.entry(id).map(|entry| &entry.data)
    }

    /// The node's parent, or `None` when detached/stale.
    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        self.entry(id).and_then(|entry| entry.parent)
    }

    /// The node's children in order, or `None` for a stale id.
    pub fn children(&self, id: NodeId) -> Option<&[NodeId]> {
        self.entry(id).map(|entry| entry.children.as_slice())
    }

    /// Whether the id refers to a live node in this tree.
    pub fn is_live(&self, id: NodeId) -> bool {
        match self.slots.get(id.index as usize) {
            Some(slot) => slot.generation == id.generation && slot.node.is_some(),
            None => false,
        }
    }

    /// Depth of the node in the tree, counted as the number of ancestors
    /// (document = 0; a detached node = 0, its children 1, …). `None` for
    /// a stale id.
    pub fn depth(&self, id: NodeId) -> Option<usize> {
        if !self.is_live(id) {
            return None;
        }
        let mut depth = 0usize;
        let mut current = self.parent(id);
        while let Some(up) = current {
            depth += 1;
            if depth > self.max_depth.saturating_mul(2) {
                // Defensive bound: a corrupted tree must not loop forever.
                // depth() cannot run away on a well-formed tree because
                // mutations reject cycles.
                return Some(depth);
            }
            current = self.parent(up);
        }
        Some(depth)
    }

    /// Whether `maybe_ancestor` is `node` or one of its ancestors.
    pub fn is_ancestor_or_self(&self, maybe_ancestor: NodeId, node: NodeId) -> bool {
        if maybe_ancestor == node {
            return true;
        }
        let mut current = self.parent(node);
        while let Some(up) = current {
            if up == maybe_ancestor {
                return true;
            }
            current = self.parent(up);
        }
        false
    }

    /// The origin of the document this node belongs to, or `None` when
    /// detached from any document or stale. Every attached node resolves
    /// to its document's origin — the "origin tag present" invariant
    /// traverses the tree.
    pub fn origin_of(&self, id: NodeId) -> Option<&Origin> {
        let root = self.root?;
        if !self.is_ancestor_or_self(root, id) {
            return None;
        }
        match self.data(root) {
            Some(NodeData::Document { origin, .. }) => Some(origin),
            _ => None,
        }
    }

    // -- mutation algorithms ------------------------------------------------

    /// Appends `child` as the last child of `parent`, moving it from any
    /// previous parent (pre-insert validity first: no cycles, no
    /// self-parent, depth respected).
    ///
    /// # Errors
    ///
    /// [`DomError::StaleNodeId`], [`DomError::ForeignNode`],
    /// [`DomError::HierarchyRequest`] (cycle or self-parent),
    /// [`DomError::NodeLimitExceeded`] is not possible for moves of
    /// existing nodes.
    pub fn append(&mut self, parent: NodeId, child: NodeId) -> Result<(), DomError> {
        self.check_move(parent, child)?;
        // Height of the moved subtree must fit under the parent's depth;
        // the limit is on ELEMENT nesting (limits table), so text children
        // (zero height, non-element) are exempted via the depth-aware
        // append_text path rather than this one.
        let parent_depth = self.depth(parent).unwrap_or(0);
        let child_height = self.subtree_height(child);
        if parent_depth + 1 + child_height > self.max_depth {
            return Err(DomError::HierarchyRequest);
        }
        self.unlink(child);
        self.link_child(parent, child);
        Ok(())
    }

    /// Inserts `child` into `parent` before the existing child
    /// `reference`. `reference = None` appends.
    ///
    /// # Errors
    ///
    /// As [`Tree::append`], plus [`DomError::NotAChild`] when `reference`
    /// is not a child of `parent`.
    pub fn insert_before(
        &mut self,
        parent: NodeId,
        child: NodeId,
        reference: Option<NodeId>,
    ) -> Result<(), DomError> {
        let Some(reference) = reference else {
            return self.append(parent, child);
        };
        if self.parent(reference) != Some(parent) {
            return Err(DomError::NotAChild);
        }
        self.check_move(parent, child)?;
        let parent_depth = self.depth(parent).unwrap_or(0);
        let child_height = self.subtree_height(child);
        if parent_depth + 1 + child_height > self.max_depth {
            return Err(DomError::HierarchyRequest);
        }
        self.unlink(child);
        let position = self
            .entry(parent)
            .and_then(|entry| entry.children.iter().position(|id| *id == reference))
            .ok_or(DomError::NotAChild)?;
        if let Some(entry) = self.entry_mut(parent) {
            entry.children.insert(position, child);
        }
        if let Some(entry) = self.entry_mut(child) {
            entry.parent = Some(parent);
        }
        Ok(())
    }

    /// Removes `child` from `parent`. The removed subtree stays alive
    /// (detached), per DOM semantics.
    ///
    /// # Errors
    ///
    /// [`DomError::NotAChild`] when `child`'s parent is not `parent`;
    /// [`DomError::StaleNodeId`] for stale ids.
    pub fn remove(&mut self, parent: NodeId, child: NodeId) -> Result<(), DomError> {
        if !self.is_live(parent) || !self.is_live(child) {
            return Err(DomError::StaleNodeId);
        }
        if self.parent(child) != Some(parent) {
            return Err(DomError::NotAChild);
        }
        self.unlink(child);
        Ok(())
    }

    /// Detaches `node` from its current parent (if any).
    ///
    /// # Errors
    ///
    /// [`DomError::StaleNodeId`] for a stale id.
    pub fn detach(&mut self, node: NodeId) -> Result<(), DomError> {
        if !self.is_live(node) {
            return Err(DomError::StaleNodeId);
        }
        self.unlink(node);
        Ok(())
    }

    /// Appends character data to `parent`, coalescing with a trailing text
    /// child (the parser's text-insertion contract: runs merge).
    ///
    /// # Errors
    ///
    /// [`DomError::StaleNodeId`]/[`DomError::ForeignNode`];
    /// [`DomError::NodeLimitExceeded`] when a new text node must be
    /// created and the limit is reached.
    pub fn append_text(&mut self, parent: NodeId, text: &str) -> Result<NodeId, DomError> {
        if !self.is_live(parent) {
            return Err(DomError::StaleNodeId);
        }
        let last = self.children(parent).and_then(<[NodeId]>::last).copied();
        if let Some(last) = last
            && matches!(self.data(last), Some(NodeData::Text { .. }))
            && let Some(entry) = self.entry_mut(last)
            && let NodeData::Text { data } = &mut entry.data
        {
            data.push_str(text);
            return Ok(last);
        }
        let text_node = self.create_text(text)?;
        // Text does not nest: the element-depth limit does not apply to a
        // text child (limits table: "Nesting depth (elements)").
        self.check_move(parent, text_node)?;
        self.link_child(parent, text_node);
        Ok(text_node)
    }

    /// Destroys `node` and its whole subtree, freeing their slots.
    ///
    /// This is the engine's explicit reclamation path: the DOM itself
    /// keeps detached trees observable, so destruction is a deliberate
    /// act (used by navigation when a document is discarded wholesale via
    /// dropping the `Tree`).
    ///
    /// # Errors
    ///
    /// [`DomError::StaleNodeId`] for a stale id. Destroying the document
    /// node empties the tree (`document()` then returns `None`).
    pub fn destroy_subtree(&mut self, node: NodeId) -> Result<(), DomError> {
        if !self.is_live(node) {
            return Err(DomError::StaleNodeId);
        }
        if self.root == Some(node) {
            self.root = None;
        }
        self.destroy_recursive(node);
        Ok(())
    }

    // -- attributes ---------------------------------------------------------

    /// Sets (replaces) an attribute on an element, creating it when absent.
    ///
    /// # Errors
    ///
    /// [`DomError::StaleNodeId`] for a stale id.
    pub fn set_attribute(
        &mut self,
        element: NodeId,
        name: QualName,
        value: impl Into<String>,
    ) -> Result<(), DomError> {
        let Some(entry) = self.entry_mut(element) else {
            return Err(DomError::StaleNodeId);
        };
        let NodeData::Element { attributes, .. } = &mut entry.data else {
            return Err(DomError::HierarchyRequest);
        };
        if let Some(existing) = attributes.iter_mut().find(|a| a.name == name) {
            existing.value = value.into();
        } else {
            attributes.push(Attribute {
                name,
                value: value.into(),
            });
        }
        Ok(())
    }

    /// The value of the first matching attribute (HTML namespace, no
    /// prefix), or `None`.
    pub fn get_attribute(&self, element: NodeId, local: &str) -> Option<&str> {
        let data = self.data(element)?;
        let NodeData::Element { attributes, .. } = data else {
            return None;
        };
        attributes
            .iter()
            .find(|a| a.name.namespace == crate::node::Namespace::Html && a.name.local == local)
            .map(|a| a.value.as_str())
    }

    /// The value of the attribute with this exact qualified name, or
    /// `None`. Exact matching (namespace + prefix + local) is what the
    /// "add attributes if missing" tree-builder callback needs.
    pub fn attribute(&self, element: NodeId, name: &crate::node::QualName) -> Option<&str> {
        let data = self.data(element)?;
        let NodeData::Element { attributes, .. } = data else {
            return None;
        };
        attributes
            .iter()
            .find(|a| &a.name == name)
            .map(|a| a.value.as_str())
    }

    /// Associates a template-contents fragment with an element (the
    /// parser's `create_element(template)` contract; the tree builder
    /// promises only template elements get contents).
    ///
    /// # Errors
    ///
    /// [`DomError::StaleNodeId`] for a stale id;
    /// [`DomError::HierarchyRequest`] when the element is not an element
    /// node or the contents node is not a fragment.
    pub fn set_template_contents(
        &mut self,
        element: NodeId,
        contents: NodeId,
    ) -> Result<(), DomError> {
        if !self.is_live(element) || !self.is_live(contents) {
            return Err(DomError::StaleNodeId);
        }
        if !matches!(self.data(contents), Some(NodeData::Fragment)) {
            return Err(DomError::HierarchyRequest);
        }
        let Some(entry) = self.entry_mut(element) else {
            return Err(DomError::StaleNodeId);
        };
        let NodeData::Element {
            template_contents, ..
        } = &mut entry.data
        else {
            return Err(DomError::HierarchyRequest);
        };
        *template_contents = Some(contents);
        Ok(())
    }

    /// The template-contents fragment of an element, if any.
    pub fn template_contents(&self, element: NodeId) -> Option<NodeId> {
        match self.data(element)? {
            NodeData::Element {
                template_contents, ..
            } => *template_contents,
            _ => None,
        }
    }

    /// Replaces a text node's data (the DOM's `Text.data` setter, used by
    /// the HTML parser's before-sibling text merging).
    ///
    /// # Errors
    ///
    /// [`DomError::StaleNodeId`] for a stale id;
    /// [`DomError::HierarchyRequest`] when the node is not a text node;
    /// [`DomError::NodeLimitExceeded`] when `data` exceeds the caller's
    /// text-node size contract (the caller enforces the normative 8 MiB
    /// limit; this method only guards against absurd growth).
    pub fn set_text_data(&mut self, node: NodeId, data: String) -> Result<(), DomError> {
        if data.len() > DEFAULT_MAX_TEXT_DATA {
            return Err(DomError::NodeLimitExceeded {
                max: DEFAULT_MAX_TEXT_DATA,
            });
        }
        let Some(entry) = self.entry_mut(node) else {
            return Err(DomError::StaleNodeId);
        };
        let NodeData::Text { data: existing } = &mut entry.data else {
            return Err(DomError::HierarchyRequest);
        };
        *existing = data;
        Ok(())
    }

    // -- events skeleton ------------------------------------------------------

    /// Registers an event listener on `node` (events skeleton; real event
    /// semantics arrive with the script phase).
    ///
    /// # Errors
    ///
    /// [`DomError::StaleNodeId`] for a stale id;
    /// [`DomError::ListenerLimit`] when the per-node listener bound is hit.
    pub fn add_listener(
        &mut self,
        node: NodeId,
        capture: bool,
        handler: impl FnMut(&mut Event) + 'static,
    ) -> Result<ListenerId, DomError> {
        if !self.is_live(node) {
            return Err(DomError::StaleNodeId);
        }
        // Bound check without holding the mutable entry borrow.
        let at_limit = self
            .entry(node)
            .map(|entry| entry.listeners.len() >= DEFAULT_MAX_LISTENERS_PER_NODE)
            .unwrap_or(true);
        if at_limit {
            return Err(DomError::ListenerLimit);
        }
        self.listener_counter += 1;
        let id = self.listener_counter;
        let Some(entry) = self.entry_mut(node) else {
            return Err(DomError::StaleNodeId);
        };
        entry.listeners.push(EventListener {
            id,
            capture,
            handler: Box::new(handler),
        });
        Ok(id)
    }

    /// Dispatches `event` at `target`: capture from the root down, target
    /// phase, then bubble to the root when the event bubbles. Returns
    /// whether a handler prevented the default action.
    pub fn dispatch(&mut self, target: NodeId, event: &mut Event) -> Result<bool, DomError> {
        if !self.is_live(target) {
            return Err(DomError::StaleNodeId);
        }
        // Path: target first, then ancestors toward the root.
        let mut path = vec![target];
        while let Some(last) = path.last().copied()
            && let Some(up) = self.parent(last)
        {
            path.push(up);
        }

        // Capture: root down to (excluding) target — capture-registered
        // listeners only.
        for node in path.iter().skip(1).rev().copied() {
            self.invoke_listeners(node, event, ListenerPhase::Capture);
        }
        // Target phase: every listener fires.
        self.invoke_listeners(target, event, ListenerPhase::Target);
        // Bubble: parent up to root — non-capture listeners only.
        if event.bubbles {
            for node in path.iter().skip(1).copied() {
                self.invoke_listeners(node, event, ListenerPhase::Bubble);
            }
        }
        Ok(event.canceled)
    }

    // -- traversal ----------------------------------------------------------

    /// Iterates `node`'s ancestors (parent first, upward).
    pub fn ancestors(&self, node: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        let mut current = self.parent(node);
        std::iter::from_fn(move || {
            let next = current?;
            current = self.parent(next);
            Some(next)
        })
    }

    /// Iterates the subtree rooted at `node` in pre-order (node first,
    /// then children left-to-right). Iterative: deep trees cannot overflow
    /// the stack.
    pub fn descendants(&self, node: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        Descendants {
            tree: self,
            pending: vec![node],
        }
    }

    // -- internals ------------------------------------------------------------

    fn allocate(&mut self, data: NodeData) -> Result<NodeId, DomError> {
        if self.live >= self.max_nodes {
            return Err(DomError::NodeLimitExceeded {
                max: self.max_nodes,
            });
        }
        let index = match self.free.pop() {
            Some(index) => index,
            None => {
                let index =
                    u32::try_from(self.slots.len()).map_err(|_| DomError::NodeLimitExceeded {
                        max: self.max_nodes,
                    })?;
                self.slots.push(Slot {
                    generation: 0,
                    node: None,
                });
                index
            }
        };
        self.slots[index as usize].node = Some(NodeEntry {
            data,
            parent: None,
            children: Vec::new(),
            listeners: Vec::new(),
        });
        self.live += 1;
        Ok(NodeId {
            index,
            generation: self.slots[index as usize].generation,
        })
    }

    fn entry(&self, id: NodeId) -> Option<&NodeEntry> {
        self.slots
            .get(id.index as usize)
            .filter(|slot| slot.generation == id.generation)
            .and_then(|slot| slot.node.as_ref())
    }

    fn entry_mut(&mut self, id: NodeId) -> Option<&mut NodeEntry> {
        self.slots
            .get_mut(id.index as usize)
            .filter(|slot| slot.generation == id.generation)
            .and_then(|slot| slot.node.as_mut())
    }

    /// Shared pre-checks for `append`/`insert_before`.
    fn check_move(&self, parent: NodeId, child: NodeId) -> Result<(), DomError> {
        if !self.is_live(parent) || !self.is_live(child) {
            return Err(DomError::StaleNodeId);
        }
        if parent == child {
            return Err(DomError::HierarchyRequest);
        }
        // Cycle rejection: the parent must not be inside the child's subtree.
        if self.is_ancestor_or_self(child, parent) {
            return Err(DomError::HierarchyRequest);
        }
        // Document/parent-type rules: only fragments and documents may take
        // arbitrary children at this stage; text/comment/doctype nodes take
        // none.
        match self.data(parent) {
            Some(NodeData::Document { .. } | NodeData::Fragment | NodeData::Element { .. }) => {}
            _ => return Err(DomError::HierarchyRequest),
        }
        Ok(())
    }

    /// Links `child` as the last child of `parent` (after unlink); the
    /// shared tail of the mutation paths.
    fn link_child(&mut self, parent: NodeId, child: NodeId) {
        if let Some(entry) = self.entry_mut(parent) {
            entry.children.push(child);
        }
        if let Some(entry) = self.entry_mut(child) {
            entry.parent = Some(parent);
        }
    }

    /// Removes `node` from its parent's child list (if any).
    fn unlink(&mut self, node: NodeId) {
        if let Some(parent) = self.parent(node)
            && let Some(entry) = self.entry_mut(parent)
        {
            entry.children.retain(|id| *id != node);
        }
        if let Some(entry) = self.entry_mut(node) {
            entry.parent = None;
        }
    }

    /// Height of the subtree rooted at `node` (single node = 0). Iterative
    /// pre-order walk bounded by the live-node count.
    fn subtree_height(&self, node: NodeId) -> usize {
        let mut height = 0usize;
        let mut pending = vec![(node, 0usize)];
        let mut guard = 0usize;
        let bound = self.live.max(1);
        while let Some((current, depth)) = pending.pop() {
            height = height.max(depth);
            guard += 1;
            if guard > bound {
                break; // corrupted tree defense; mutations reject cycles
            }
            if let Some(children) = self.children(current) {
                for child in children {
                    pending.push((*child, depth + 1));
                }
            }
        }
        height
    }

    fn destroy_recursive(&mut self, node: NodeId) {
        // Iterative post-order-ish destruction: detach children first via
        // an explicit stack, then free the slot. Bounded by live count.
        let mut stack = vec![node];
        let mut guard = 0usize;
        let bound = self.live.max(1);
        while let Some(current) = stack.pop() {
            guard += 1;
            if guard > bound {
                break;
            }
            if let Some(entry) = self.entry(current) {
                stack.extend(entry.children.iter().copied());
            }
            if let Some(slot) = self.slots.get_mut(current.index as usize)
                && slot.generation == current.generation
            {
                slot.node = None;
                slot.generation += 1;
                self.free.push(current.index);
                self.live -= 1;
            }
        }
    }

    fn invoke_listeners(&mut self, node: NodeId, event: &mut Event, phase: ListenerPhase) {
        // Collect then invoke: the handler could mutate the tree, but it
        // cannot observe the listener list we are iterating (handlers get
        // `&mut Event` only) — so a two-phase call is sufficient and keeps
        // borrowing simple.
        let Some(entry) = self.entry_mut(node) else {
            return;
        };
        let mut invoked: Vec<ListenerId> = Vec::new();
        for listener in &entry.listeners {
            let fires = match phase {
                ListenerPhase::Capture => listener.capture,
                ListenerPhase::Target => true,
                ListenerPhase::Bubble => !listener.capture,
            };
            if fires {
                invoked.push(listener.id);
            }
        }
        let handlers: Vec<&mut EventListener> = entry
            .listeners
            .iter_mut()
            .filter(|listener| invoked.contains(&listener.id))
            .collect();
        for listener in handlers {
            if event.stop_dispatch {
                break;
            }
            (listener.handler)(event);
        }
    }
}

/// Which dispatch phase is invoking listeners.
enum ListenerPhase {
    /// Capture: capture-registered listeners only.
    Capture,
    /// At the target: every listener fires.
    Target,
    /// Bubble: non-capture listeners only.
    Bubble,
}

/// Pre-order subtree iterator (see [`Tree::descendants`]).
struct Descendants<'a> {
    tree: &'a Tree,
    pending: Vec<NodeId>,
}

impl Iterator for Descendants<'_> {
    type Item = NodeId;

    fn next(&mut self) -> Option<NodeId> {
        let node = self.pending.pop()?;
        if let Some(children) = self.tree.children(node) {
            // Push reversed so children pop in document order.
            self.pending.extend(children.iter().rev().copied());
        }
        Some(node)
    }
}
