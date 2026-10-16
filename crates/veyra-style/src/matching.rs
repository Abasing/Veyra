//! Selector matching: `veyra-dom` elements adapted to the `selectors`
//! matcher.
//!
//! The cascade batch's first half. [`VeyraElement`] is the embedding of
//! [`veyra_dom`]'s arena tree into the `selectors` crate's [`Element`]
//! trait — the same adapter shape Servo maintains (ADR-0014 measured the
//! stylo version of this surface at 78 `TElement` methods; the matching
//! subset here is the Phase 3 working set of it). Matching itself —
//! combinators, `:is()`/`:not()`/`:has()`, `nth-child` indexing — is the
//! upstream matcher's; this module supplies tree navigation, name/
//! namespace/attribute access, and the pseudo-class semantics.
//!
//! # Pseudo-class semantics (Phase 3 working set)
//!
//! The engine has no interactive state yet, so user-action pseudo-classes
//! (`:hover`, `:active`, `:focus`, …) match nothing. The forms-UI and
//! link pseudo-classes are evaluated from *static attributes* — the
//! honest approximation for static pages, each deviation recorded:
//!
//! - `:link` — `a[href]`, `area[href]`, `link[href]` (HTML namespace).
//! - `:visited` — never (privacy; unvisited matching only).
//! - `:enabled`/`:disabled` — form controls, own `disabled` attribute
//!   only (the ancestor-`fieldset` propagation rule is deferred).
//! - `:checked` — `input[type=checkbox|radio]`/`option` with a `checked`
//!   attribute (no user toggling exists).
//! - `:required`/`:optional` — the `required` attribute on
//!   `input`/`select`/`textarea`.
//! - `:read-write` — a writable `input` (per the HTML spec's type list),
//!   a `textarea`, or a `contenteditable` element; `:read-only` is the
//!   complement (HTML's definition, where every non-editable element
//!   matches).
//! - `:placeholder-shown` — `input`/`textarea` with a `placeholder`
//!   attribute (no value-driven state).
//! - `:valid`/`:invalid`/`:in-range`-family/`:autofill`/`:indeterminate`
//!   — never (no validation or value state exists).
//! - `:target` — the element whose `id` equals the document's fragment
//!   target, supplied by the caller via [`MatchOptions`] (fragment
//!   parsing is a navigation-layer concern).
//! - `:lang()` — the inherited `lang` attribute (nearest ancestor-or-self
//!   carrying one), matched with RFC 4647 basic range filtering.
//! - `:dir()` — the inherited `dir` attribute (`ltr`/`rtl`); the content-
//!   derived directionality algorithm is deferred with writing modes.
//!
//! # Namespace model
//!
//! Elements carry real namespace URIs (`veyra-dom::Namespace`). With no
//! `@namespace` declared yet, unprefixed type selectors arrive as
//! any-namespace constraints (the upstream parser's implicit form) and
//! match HTML/SVG/MathML elements alike — the Selectors 4 rule. Attribute
//! selectors without an explicit prefix match attributes "in no
//! namespace": `veyra-html` normalizes HTML attributes to the HTML
//! namespace in storage, so that stored form is treated as the
//! no-namespace bucket here (the DOM's attributes have no namespace in
//! CSS semantics).

use selectors::attr::{AttrSelectorOperation, CaseSensitivity, NamespaceConstraint};
use selectors::bloom::BloomFilter;
use selectors::context::{
    MatchingContext, MatchingForInvalidation, MatchingMode, NeedsSelectorFlags, QuirksMode,
    SelectorCaches,
};
use selectors::matching::ElementSelectorFlags;
use selectors::{Element as ElementTrait, OpaqueElement};
use veyra_dom::{Namespace, NodeData, NodeId, QuirksMode as DomQuirksMode, Tree};

use crate::selector_impl::{
    AttrValue, Identifier, LocalName, NamespaceUrl, PseudoClass, PseudoElement, VeyraSelectorImpl,
};
use crate::selectors::SelectorList;

/// Options threading document-level matching state that is not stored on
/// elements.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MatchOptions {
    /// The element matching `:target`, if the document has a fragment
    /// target (`None` — nothing matches `:target`).
    pub target_id: Option<NodeId>,
}

/// A `veyra-dom` element viewed as a selector-match target.
///
/// Construction validates liveness and node kind; a stale or non-element
/// id yields `None` (never a panic — engineering standards §2). The
/// wrapper is cheap to clone and borrows the tree; matching never
/// mutates.
#[derive(Clone)]
pub struct VeyraElement<'tree> {
    tree: &'tree Tree,
    id: NodeId,
    /// Element identity for the matcher (points into the arena — stable
    /// while the borrowed tree is not mutated, which matching guarantees).
    opaque: OpaqueElement,
    /// The `:target` element, carried so traversal-constructed wrappers
    /// (parents, siblings) keep document-level context.
    target_id: Option<NodeId>,
}

impl std::fmt::Debug for VeyraElement<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VeyraElement")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

impl<'tree> VeyraElement<'tree> {
    /// Wraps the element at `id`, if it is a live element of `tree`.
    #[must_use]
    pub fn new(tree: &'tree Tree, id: NodeId) -> Option<Self> {
        if !tree.is_live(id) {
            return None;
        }
        match tree.data(id)? {
            NodeData::Element { .. } => Some(VeyraElement {
                tree,
                id,
                opaque: OpaqueElement::new(tree.data(id)?),
                target_id: None,
            }),
            _ => None,
        }
    }

    /// Wraps the element at `id` with document-level match context.
    pub(crate) fn with_target(
        tree: &'tree Tree,
        id: NodeId,
        target_id: Option<NodeId>,
    ) -> Option<Self> {
        let mut element = Self::new(tree, id)?;
        element.target_id = target_id;
        Some(element)
    }

    /// The wrapped node id.
    #[must_use]
    pub fn node_id(&self) -> NodeId {
        self.id
    }

    /// The borrowed tree.
    #[must_use]
    pub fn tree(&self) -> &'tree Tree {
        self.tree
    }

    /// Whether this complex selector list matches the element (Selectors
    /// 4 matching, unvisited link handling, no bloom filter — batch scale
    /// does not justify one; the matcher's own caches apply).
    #[must_use]
    pub fn matches(&self, selectors: &SelectorList, options: &MatchOptions) -> bool {
        let mut target = self.clone();
        target.target_id = options.target_id;
        let mut caches = SelectorCaches::default();
        let quirks = quirks_of(self.tree);
        selectors
            .upstream
            .slice()
            .iter()
            .any(|selector| match_selector(&target, selector, quirks, &mut caches))
    }

    /// The element's local name (exact compare; the matcher resolves
    /// HTML case-insensitivity through the selector's stored forms).
    fn local_name(&self) -> Option<&str> {
        match self.tree.data(self.id)? {
            NodeData::Element { name, .. } => Some(&name.local),
            _ => None,
        }
    }

    /// The element's namespace.
    fn namespace(&self) -> Option<&Namespace> {
        match self.tree.data(self.id)? {
            NodeData::Element { name, .. } => Some(&name.namespace),
            _ => None,
        }
    }

    /// The element's attribute list.
    fn attributes(&self) -> Option<&[veyra_dom::Attribute]> {
        match self.tree.data(self.id)? {
            NodeData::Element { attributes, .. } => Some(attributes),
            _ => None,
        }
    }

    /// An attribute value by no-namespace local name (the DOM's HTML
    /// attributes normalize to that bucket).
    fn attr(&self, local: &str) -> Option<&str> {
        let attributes = self.attributes()?;
        attributes
            .iter()
            .find(|attribute| attribute.name.local.eq_ignore_ascii_case(local))
            .map(|attribute| attribute.value.as_str())
    }

    /// Whether the element is an HTML-namespace form control (the set
    /// the forms-UI pseudo-classes consider).
    fn is_form_control(&self) -> bool {
        matches!(
            self.local_name(),
            Some("input" | "select" | "textarea" | "button" | "option" | "optgroup" | "fieldset")
        ) && self.namespace() == Some(&Namespace::Html)
    }

    /// The `input` element's type attribute, defaulting to `text` (the
    /// HTML missing-value default).
    fn input_type(&self) -> &str {
        self.attr("type").unwrap_or("text")
    }

    /// Whether the element is read-write per the HTML definition: a
    /// writable `input`, a `textarea` (both without `readonly`/
    /// `disabled`), or a `contenteditable` element.
    fn is_read_write(&self) -> bool {
        if self.attr("contenteditable").is_some() {
            return true;
        }
        if !self.is_form_control() || self.attr("disabled").is_some() {
            return false;
        }
        if self.attr("readonly").is_some() {
            return false;
        }
        match self.local_name() {
            Some("textarea") => true,
            Some("input") => matches!(
                self.input_type(),
                "text"
                    | "search"
                    | "url"
                    | "tel"
                    | "email"
                    | "password"
                    | "date"
                    | "month"
                    | "week"
                    | "time"
                    | "datetime-local"
                    | "number"
            ),
            _ => false,
        }
    }

    /// The nearest ancestor-or-self `lang` attribute (HTML language
    /// inheritance).
    fn inherited_lang(&self) -> Option<String> {
        let mut current = Some(self.clone());
        while let Some(element) = current {
            if let Some(lang) = element.attr("lang") {
                return Some(lang.to_ascii_lowercase());
            }
            current = element.parent_element();
        }
        None
    }

    /// The nearest ancestor-or-self `dir` attribute value (`ltr`/`rtl`).
    fn inherited_dir(&self) -> Option<String> {
        let mut current = Some(self.clone());
        while let Some(element) = current {
            if let Some(dir) = element.attr("dir") {
                let dir = dir.trim().to_ascii_lowercase();
                if dir == "ltr" || dir == "rtl" {
                    return Some(dir);
                }
            }
            current = element.parent_element();
        }
        None
    }
}

/// The document's quirks mode, mapped onto the matcher's (the matcher's
/// default when the tree has no document node).
pub(crate) fn quirks_of(tree: &Tree) -> QuirksMode {
    document_quirks(tree)
        .map(map_quirks)
        .unwrap_or(QuirksMode::NoQuirks)
}

/// Runs one complex selector against an element with a shared cache set
/// (the cascade matches many selectors per element; the nth-index and
/// relative-selector caches persist across them).
pub(crate) fn match_selector(
    element: &VeyraElement<'_>,
    selector: &selectors::parser::Selector<VeyraSelectorImpl>,
    quirks: QuirksMode,
    caches: &mut SelectorCaches,
) -> bool {
    let mut context = MatchingContext::new(
        MatchingMode::Normal,
        None,
        caches,
        quirks,
        NeedsSelectorFlags::No,
        MatchingForInvalidation::No,
    );
    selectors::matching::matches_selector(selector, 0, None, element, &mut context)
}

/// The document's quirks mode, read from the document node.
fn document_quirks(tree: &Tree) -> Option<DomQuirksMode> {
    let document = tree.document()?;
    match tree.data(document)? {
        NodeData::Document { quirks, .. } => Some(*quirks),
        _ => None,
    }
}

/// Maps the DOM's quirks mode onto the matcher's.
fn map_quirks(quirks: DomQuirksMode) -> QuirksMode {
    match quirks {
        DomQuirksMode::No => QuirksMode::NoQuirks,
        DomQuirksMode::Yes => QuirksMode::Quirks,
        DomQuirksMode::Limited => QuirksMode::LimitedQuirks,
    }
}

/// RFC 4647 basic language-range filtering: `*` matches everything;
/// otherwise the range equals the tag or is a prefix followed by `-`.
fn language_range_matches(range: &str, tag: &str) -> bool {
    let range = range.trim().to_ascii_lowercase();
    if range == "*" {
        return true;
    }
    tag == range
        || tag
            .strip_prefix(&range)
            .is_some_and(|rest| rest.starts_with('-'))
}

impl ElementTrait for VeyraElement<'_> {
    type Impl = VeyraSelectorImpl;

    fn opaque(&self) -> OpaqueElement {
        self.opaque
    }

    fn parent_element(&self) -> Option<Self> {
        // parentElement semantics: the immediate parent only — a
        // non-element parent (document, fragment, text, …) ends the
        // element chain.
        let parent = self.tree.parent(self.id)?;
        match self.tree.data(parent)? {
            NodeData::Element { .. } => {
                VeyraElement::with_target(self.tree, parent, self.target_id)
            }
            _ => None,
        }
    }

    fn parent_node_is_shadow_root(&self) -> bool {
        false
    }

    fn containing_shadow_host(&self) -> Option<Self> {
        None
    }

    fn is_pseudo_element(&self) -> bool {
        false
    }

    fn prev_sibling_element(&self) -> Option<Self> {
        self.sibling_element(false)
    }

    fn next_sibling_element(&self) -> Option<Self> {
        self.sibling_element(true)
    }

    fn first_element_child(&self) -> Option<Self> {
        let children = self.tree.children(self.id)?;
        children
            .iter()
            .find(|child| matches!(self.tree.data(**child), Some(NodeData::Element { .. })))
            .and_then(|child| VeyraElement::with_target(self.tree, *child, self.target_id))
    }

    fn is_html_element_in_html_document(&self) -> bool {
        // Parsed documents are HTML documents; the HTML namespace is the
        // "in HTML document" signal (foreign-content elements differ).
        self.namespace() == Some(&Namespace::Html)
    }

    fn has_local_name(&self, local_name: &str) -> bool {
        self.local_name() == Some(local_name)
    }

    fn has_namespace(&self, ns: &str) -> bool {
        // Empty means "no namespace"; every stored element carries a
        // real namespace URI, so only URI equality can match.
        !ns.is_empty()
            && self
                .namespace()
                .is_some_and(|namespace| namespace.uri() == ns)
    }

    fn is_same_type(&self, other: &Self) -> bool {
        match (self.namespace(), other.namespace()) {
            (Some(mine), Some(theirs)) => mine == theirs && self.local_name() == other.local_name(),
            _ => false,
        }
    }

    fn attr_matches(
        &self,
        ns: &NamespaceConstraint<&NamespaceUrl>,
        local_name: &LocalName,
        operation: &AttrSelectorOperation<&AttrValue>,
    ) -> bool {
        let attributes = match self.attributes() {
            Some(attributes) => attributes,
            None => return false,
        };
        let namespace_matches: Box<dyn Fn(&Namespace) -> bool + '_> = match ns {
            NamespaceConstraint::Any => Box::new(|_| true),
            // The empty namespace is "no namespace": the DOM's HTML
            // attributes normalize there, so the stored HTML form is
            // the no-namespace bucket.
            NamespaceConstraint::Specific(url) if url.as_str().is_empty() => {
                Box::new(|stored: &Namespace| matches!(stored, Namespace::Html))
            }
            NamespaceConstraint::Specific(url) => {
                let url = url.as_str();
                Box::new(move |stored: &Namespace| stored.uri() == url)
            }
        };
        for attribute in attributes {
            if !namespace_matches(&attribute.name.namespace) {
                continue;
            }
            // HTML attribute names are stored lowercased; the matcher
            // resolves case per document mode for foreign content.
            let name_matches = if self.is_html_element_in_html_document() {
                attribute
                    .name
                    .local
                    .eq_ignore_ascii_case(local_name.as_str())
            } else {
                attribute.name.local == local_name.as_str()
            };
            if !name_matches {
                continue;
            }
            if operation.eval_str(&attribute.value) {
                return true;
            }
        }
        false
    }

    fn match_non_ts_pseudo_class(
        &self,
        pc: &PseudoClass,
        _context: &mut MatchingContext<VeyraSelectorImpl>,
    ) -> bool {
        match pc {
            // Interactive/document state the engine does not model yet:
            // nothing matches (selectors that require it simply do not
            // apply to static pages).
            PseudoClass::Hover
            | PseudoClass::Active
            | PseudoClass::Focus
            | PseudoClass::FocusVisible
            | PseudoClass::FocusWithin
            | PseudoClass::Visited
            | PseudoClass::UserValid
            | PseudoClass::UserInvalid
            | PseudoClass::Autofill
            | PseudoClass::Indeterminate
            | PseudoClass::Valid
            | PseudoClass::Invalid => false,
            PseudoClass::Link => self.is_link_element(),
            PseudoClass::Target => self.target_id == Some(self.id),
            PseudoClass::Lang(ranges) => {
                let lang = match self.inherited_lang() {
                    Some(lang) => lang,
                    None => return false,
                };
                ranges
                    .iter()
                    .any(|range| language_range_matches(range, &lang))
            }
            PseudoClass::Dir(direction) => self
                .inherited_dir()
                .is_some_and(|dir| dir == direction.as_ref()),
            PseudoClass::Enabled => self.is_form_control() && !self.has_disabled_attr(),
            PseudoClass::Disabled => self.has_disabled_attr(),
            PseudoClass::Checked => self.is_checked_by_attr(),
            PseudoClass::Required => {
                self.is_form_control()
                    && matches!(self.local_name(), Some("input" | "select" | "textarea"))
                    && self.attr("required").is_some()
            }
            PseudoClass::Optional => {
                self.is_form_control()
                    && matches!(self.local_name(), Some("input" | "select" | "textarea"))
                    && self.attr("required").is_none()
            }
            PseudoClass::ReadOnly => !self.is_read_write(),
            PseudoClass::ReadWrite => self.is_read_write(),
            PseudoClass::PlaceholderShown => {
                self.is_form_control()
                    && matches!(self.local_name(), Some("input" | "textarea"))
                    && self.attr("placeholder").is_some()
            }
        }
    }

    fn match_pseudo_element(
        &self,
        _pe: &PseudoElement,
        _context: &mut MatchingContext<VeyraSelectorImpl>,
    ) -> bool {
        // Pseudo-elements never match a real element; their rules are
        // consumed by the generated-content batch.
        false
    }

    fn apply_selector_flags(&self, _flags: ElementSelectorFlags) {
        // No invalidation bookkeeping exists yet (R1 is its own batch);
        // matching runs with NeedsSelectorFlags::No.
    }

    fn is_link(&self) -> bool {
        self.is_link_element()
    }

    fn is_html_slot_element(&self) -> bool {
        false
    }

    fn has_id(&self, id: &Identifier, case_sensitivity: CaseSensitivity) -> bool {
        match self.attr("id") {
            Some(value) => case_sensitivity.eq(value.as_bytes(), id.as_str().as_bytes()),
            None => false,
        }
    }

    fn has_class(&self, name: &Identifier, case_sensitivity: CaseSensitivity) -> bool {
        let class = match self.attr("class") {
            Some(class) => class,
            None => return false,
        };
        class
            .split_ascii_whitespace()
            .any(|token| case_sensitivity.eq(token.as_bytes(), name.as_str().as_bytes()))
    }

    fn has_custom_state(&self, _name: &Identifier) -> bool {
        false
    }

    fn imported_part(
        &self,
        _name: &Identifier,
    ) -> Option<<Self::Impl as selectors::parser::SelectorImpl>::Identifier> {
        None
    }

    fn is_part(&self, _name: &Identifier) -> bool {
        false
    }

    fn is_empty(&self) -> bool {
        let children = match self.tree.children(self.id) {
            Some(children) => children,
            None => return true,
        };
        !children.iter().any(|child| match self.tree.data(*child) {
            Some(NodeData::Element { .. }) => true,
            Some(NodeData::Text { data }) => !data.is_empty(),
            _ => false,
        })
    }

    fn is_root(&self) -> bool {
        // The root element's parent is the document node.
        match self.tree.parent(self.id) {
            Some(parent) => matches!(self.tree.data(parent), Some(NodeData::Document { .. })),
            None => false,
        }
    }

    fn add_element_unique_hashes(&self, _filter: &mut BloomFilter) -> bool {
        false
    }
}

impl VeyraElement<'_> {
    /// Walks to the previous (`forward = false`) or next element sibling,
    /// skipping non-element nodes.
    fn sibling_element(&self, forward: bool) -> Option<Self> {
        let parent = self.tree.parent(self.id)?;
        let children = self.tree.children(parent)?;
        let position = children.iter().position(|child| *child == self.id)?;
        let slice = if forward {
            &children[position + 1..]
        } else {
            &children[..position]
        };
        let ids: &[NodeId] = slice;
        let found = if forward {
            ids.iter()
                .find(|child| matches!(self.tree.data(**child), Some(NodeData::Element { .. })))
        } else {
            ids.iter()
                .rev()
                .find(|child| matches!(self.tree.data(**child), Some(NodeData::Element { .. })))
        }?;
        VeyraElement::with_target(self.tree, *found, self.target_id)
    }

    /// `:link` semantics: `a`, `area`, or `link` in the HTML namespace
    /// carrying an `href` attribute.
    fn is_link_element(&self) -> bool {
        self.namespace() == Some(&Namespace::Html)
            && matches!(self.local_name(), Some("a" | "area" | "link"))
            && self.attr("href").is_some()
    }

    fn has_disabled_attr(&self) -> bool {
        self.is_form_control() && self.attr("disabled").is_some()
    }

    fn is_checked_by_attr(&self) -> bool {
        if !self.is_form_control() {
            return false;
        }
        match self.local_name() {
            Some("option") => self.attr("selected").is_some(),
            Some("input") => {
                matches!(self.input_type(), "checkbox" | "radio") && self.attr("checked").is_some()
            }
            _ => false,
        }
    }
}
