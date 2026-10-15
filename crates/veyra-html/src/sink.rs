//! The `TreeSink` implementation: html5ever tree construction → `veyra-dom`.
//!
//! The tree builder drives this sink through `&self`, so the tree lives in
//! a `RefCell` (the content layer is single-threaded by design —
//! engineering standards §5 — so `RefCell` is sufficient and the borrow
//! cannot be contended). Errors are *collected*, not propagated: the tree
//! builder's sink protocol has no error channel. Limits violations
//! (`HtmlError` outcomes) halt insertion — further sink calls become
//! no-ops absorbed by a trash fragment — and `finish` reports the first
//! error, so an oversized document is *rejected*, never crashed on
//! (engineering standards §6).

use std::cell::RefCell;

use html5ever::interface::{NodeOrText, QuirksMode as HtmlQuirksMode, TreeSink};
use html5ever::tendril::StrTendril;
use html5ever::tree_builder::ElementFlags;
use html5ever::{Attribute as HtmlAttribute, QualName as HtmlQualName};
use veyra_dom::{Attribute, DomError, Namespace, NodeData, NodeId, QualName, QuirksMode, Tree};

use crate::HtmlError;

/// Internal sink state shared across the `&self` methods.
struct SinkState {
    tree: Tree,
    /// Absorbs operations after a limits violation (halted parsing).
    trash: NodeId,
    /// Set when a limits violation occurred; all further tree mutations
    /// become no-ops.
    halted: bool,
    /// The first limits violation, reported by `finish`.
    error: Option<HtmlError>,
    /// Parse-error count from the tokenizer/tree builder (diagnostics).
    parse_errors: usize,
    /// Elements flagged as MathML annotation-xml integration points.
    mathml_integration_points: Vec<NodeId>,
}

/// The tree-building sink. Constructed via [`crate::parse_document`] /
/// [`crate::parse_fragment`]; `finish` yields the [`Tree`].
pub(crate) struct DomSink {
    state: RefCell<SinkState>,
}

impl DomSink {
    /// Wraps `tree` for a document parse.
    pub(crate) fn for_document(mut tree: Tree) -> Self {
        let trash = tree.create_fragment().unwrap_or_else(|_| {
            // A fresh tree always has room for the trash node; if a caller
            // pre-exhausted the limit, parsing halts on the first insert.
            // The null handle keeps the sink total when even the document
            // is missing (a caller destroy edge case): every operation on
            // it is a checked no-op.
            tree.document().unwrap_or_else(NodeId::null)
        });
        Self {
            state: RefCell::new(SinkState {
                tree,
                trash,
                halted: false,
                error: None,
                parse_errors: 0,
                mathml_integration_points: Vec::new(),
            }),
        }
    }

    /// Returns the tree, or the first limits violation recorded during
    /// parsing (the tree is dropped in that case — a rejected document is
    /// not handed on).
    pub(crate) fn finish(self) -> Result<Tree, HtmlError> {
        let state = self.state.into_inner();
        match state.error {
            Some(error) => Err(error),
            None => Ok(state.tree),
        }
    }

    fn halt(&self, error: HtmlError) {
        let mut state = self.state.borrow_mut();
        if state.error.is_none() {
            state.error = Some(error);
        }
        state.halted = true;
    }

    fn trash(&self) -> NodeId {
        self.state.borrow().trash
    }

    fn node_limit_hit(&self, max: usize) {
        self.halt(HtmlError::NodeLimit { max });
    }

    /// Converts an external qualified name into the Veyra one.
    fn convert_name(name: &HtmlQualName) -> QualName {
        QualName {
            prefix: name.prefix.as_ref().map(|p| p.to_string()),
            namespace: convert_namespace(&name.ns),
            local: name.local.to_string(),
        }
    }

    fn convert_attrs(attrs: Vec<HtmlAttribute>) -> (Vec<Attribute>, Option<HtmlError>) {
        let mut out = Vec::with_capacity(attrs.len());
        for attr in attrs {
            let value = attr.value.to_string();
            if value.len() > crate::MAX_TEXT_NODE {
                return (
                    out,
                    Some(HtmlError::AttributeTooLarge {
                        max: crate::MAX_TEXT_NODE,
                        actual: value.len(),
                    }),
                );
            }
            out.push(Attribute {
                name: Self::convert_name(&attr.name),
                value,
            });
        }
        (out, None)
    }
}

fn convert_namespace(ns: &html5ever::Namespace) -> Namespace {
    // The external parser emits HTML-namespace *attributes* with the
    // empty namespace (the tree builder only adjusts foreign-content
    // attribute namespaces), while elements always carry a real URL.
    // Empty therefore means "HTML namespace" in practice.
    let url: &str = ns;
    match url {
        "" | "http://www.w3.org/1999/xhtml" => Namespace::Html,
        "http://www.w3.org/2000/svg" => Namespace::Svg,
        "http://www.w3.org/1998/Math/MathML" => Namespace::MathMl,
        _ => Namespace::Other(url.to_string()),
    }
}

fn external_namespace(namespace: &Namespace) -> html5ever::Namespace {
    match namespace {
        Namespace::Html => html5ever::Namespace::from("http://www.w3.org/1999/xhtml"),
        Namespace::Svg => html5ever::Namespace::from("http://www.w3.org/2000/svg"),
        Namespace::MathMl => html5ever::Namespace::from("http://www.w3.org/1998/Math/MathML"),
        Namespace::Other(url) => html5ever::Namespace::from(url.as_str()),
    }
}

/// Merges `text` into an existing text node (the before-sibling case,
/// where the merge target is not necessarily the last child).
fn merge_text_into(tree: &mut Tree, text_node: NodeId, text: &str) -> Result<(), DomError> {
    let combined = match tree.data(text_node) {
        Some(NodeData::Text { data }) => {
            let mut combined = String::with_capacity(data.len() + text.len());
            combined.push_str(data);
            combined.push_str(text);
            combined
        }
        _ => return Err(DomError::HierarchyRequest),
    };
    tree.set_text_data(text_node, combined)
}

impl TreeSink for DomSink {
    type Handle = NodeId;
    type Output = Result<Tree, HtmlError>;

    type ElemName<'a> = SinkElemName;

    fn finish(self) -> Self::Output {
        DomSink::finish(self)
    }

    fn parse_error(&self, _msg: std::borrow::Cow<'static, str>) {
        self.state.borrow_mut().parse_errors += 1;
    }

    fn get_document(&self) -> Self::Handle {
        let state = self.state.borrow();
        state.tree.document().unwrap_or(state.trash)
    }

    fn elem_name<'a>(&'a self, target: &'a Self::Handle) -> Self::ElemName<'a> {
        let name = match self.state.borrow().tree.data(*target) {
            Some(NodeData::Element { name, .. }) => name.clone(),
            _ => QualName::html(String::new()),
        };
        SinkElemName {
            ns: external_namespace(&name.namespace),
            local: html5ever::LocalName::from(name.local.as_str()),
        }
    }

    fn create_element(
        &self,
        name: HtmlQualName,
        attrs: Vec<HtmlAttribute>,
        flags: ElementFlags,
    ) -> Self::Handle {
        let (attributes, attr_error) = Self::convert_attrs(attrs);
        if let Some(error) = attr_error {
            self.halt(error);
            return self.trash();
        }
        let veyra_name = Self::convert_name(&name);
        let mut state = self.state.borrow_mut();
        if state.halted {
            return state.trash;
        }
        let element = match state.tree.create_element(veyra_name, attributes) {
            Ok(id) => id,
            Err(DomError::NodeLimitExceeded { max }) => {
                drop(state);
                self.node_limit_hit(max);
                return self.trash();
            }
            Err(_) => return state.trash,
        };
        if flags.template {
            match state.tree.create_fragment() {
                Ok(contents) => {
                    let _ = state.tree.set_template_contents(element, contents);
                }
                Err(DomError::NodeLimitExceeded { max }) => {
                    drop(state);
                    self.node_limit_hit(max);
                    return self.trash();
                }
                Err(_) => {}
            }
        }
        if flags.mathml_annotation_xml_integration_point {
            state.mathml_integration_points.push(element);
        }
        element
    }

    fn create_comment(&self, text: StrTendril) -> Self::Handle {
        let mut state = self.state.borrow_mut();
        if state.halted {
            return state.trash;
        }
        match state.tree.create_comment(text.to_string()) {
            Ok(id) => id,
            Err(_) => state.trash,
        }
    }

    fn create_pi(&self, target: StrTendril, data: StrTendril) -> Self::Handle {
        // The HTML parser never produces processing instructions
        // (xml5ever's callback); absorbing as a comment keeps the sink
        // total without inventing a node kind.
        let combined = format!("{}:{}", target, data);
        let mut state = self.state.borrow_mut();
        if state.halted {
            return state.trash;
        }
        match state.tree.create_comment(combined) {
            Ok(id) => id,
            Err(_) => state.trash,
        }
    }

    fn append(&self, parent: &Self::Handle, child: NodeOrText<Self::Handle>) {
        if *parent == self.trash() {
            return;
        }
        match child {
            NodeOrText::AppendNode(node) => {
                let mut state = self.state.borrow_mut();
                if state.halted {
                    return;
                }
                // Depth violations (HierarchyRequest) halt parsing: the
                // tree builder would otherwise keep inserting past the
                // nesting limit.
                if let Err(DomError::HierarchyRequest) = state.tree.append(*parent, node) {
                    drop(state);
                    self.halt(HtmlError::DepthExceeded {
                        max: veyra_dom::DEFAULT_MAX_DEPTH,
                    });
                }
            }
            NodeOrText::AppendText(text) => {
                let text = text.to_string();
                if text.len() > crate::MAX_TEXT_NODE {
                    self.halt(HtmlError::TextTooLarge {
                        max: crate::MAX_TEXT_NODE,
                        actual: text.len(),
                    });
                    return;
                }
                let mut state = self.state.borrow_mut();
                if state.halted {
                    return;
                }
                let _ = state.tree.append_text(*parent, &text);
            }
        }
    }

    fn append_based_on_parent_node(
        &self,
        element: &Self::Handle,
        prev_element: &Self::Handle,
        child: NodeOrText<Self::Handle>,
    ) {
        // Foster-parenting insertion point: when the table element has a
        // parent, insert immediately before it; otherwise fall back into
        // the stack-preceding element (the tree builder supplies both).
        if self.state.borrow().tree.parent(*element).is_some() {
            self.append_before_sibling(element, child);
        } else {
            self.append(prev_element, child);
        }
    }

    fn append_doctype_to_document(
        &self,
        name: StrTendril,
        public_id: StrTendril,
        system_id: StrTendril,
    ) {
        let mut state = self.state.borrow_mut();
        if state.halted {
            return;
        }
        let Some(document) = state.tree.document() else {
            return;
        };
        let doctype = state.tree.create_doctype(
            name.to_string(),
            non_empty(public_id.to_string()),
            non_empty(system_id.to_string()),
        );
        let Ok(doctype) = doctype else {
            return;
        };
        let _ = state.tree.append(document, doctype);
    }

    fn get_template_contents(&self, target: &Self::Handle) -> Self::Handle {
        let state = self.state.borrow_mut();
        if let Some(contents) = state.tree.template_contents(*target) {
            return contents;
        }
        // Defensive: the tree builder promises template-only calls; for
        // any other element, hand back the trash fragment rather than a
        // fabricated contents node.
        state.trash
    }

    fn same_node(&self, x: &Self::Handle, y: &Self::Handle) -> bool {
        x == y
    }

    fn set_quirks_mode(&self, mode: HtmlQuirksMode) {
        let mode = match mode {
            HtmlQuirksMode::Quirks => QuirksMode::Yes,
            HtmlQuirksMode::LimitedQuirks => QuirksMode::Limited,
            HtmlQuirksMode::NoQuirks => QuirksMode::No,
        };
        self.state.borrow_mut().tree.set_quirks_mode(mode);
    }

    fn append_before_sibling(&self, sibling: &Self::Handle, new_node: NodeOrText<Self::Handle>) {
        // Compute the insertion parent and the preceding text node (if any)
        // under an immutable borrow that ends before mutation.
        let (parent, prev_text) = {
            let state = self.state.borrow();
            if state.halted {
                return;
            }
            let Some(parent) = state.tree.parent(*sibling) else {
                return;
            };
            let prev = state.tree.children(parent).and_then(|children| {
                let position = children.iter().position(|id| id == sibling)?;
                if position == 0 {
                    None
                } else {
                    Some(children[position - 1])
                }
            });
            (parent, prev)
        };

        match new_node {
            NodeOrText::AppendNode(node) => {
                let mut state = self.state.borrow_mut();
                if state.halted {
                    return;
                }
                let _ = state.tree.insert_before(parent, node, Some(*sibling));
            }
            NodeOrText::AppendText(text) => {
                let text = text.to_string();
                if text.len() > crate::MAX_TEXT_NODE {
                    self.halt(HtmlError::TextTooLarge {
                        max: crate::MAX_TEXT_NODE,
                        actual: text.len(),
                    });
                    return;
                }
                // Text-before-sibling merges with a text node immediately
                // preceding the insertion point, per the tree builder
                // contract.
                let prev_is_text = prev_text
                    .map(|prev| {
                        matches!(
                            self.state.borrow().tree.data(prev),
                            Some(NodeData::Text { .. })
                        )
                    })
                    .unwrap_or(false);
                if let Some(prev_node) = prev_text
                    && prev_is_text
                {
                    let merged = {
                        let mut state = self.state.borrow_mut();
                        merge_text_into(&mut state.tree, prev_node, &text)
                    };
                    if let Err(DomError::NodeLimitExceeded { max }) = merged {
                        self.halt(HtmlError::TextTooLarge {
                            max,
                            actual: max + 1,
                        });
                    }
                    return;
                }
                let text_node = {
                    let mut state = self.state.borrow_mut();
                    if state.halted {
                        None
                    } else {
                        state.tree.create_text(&text).ok()
                    }
                };
                let Some(text_node) = text_node else {
                    return;
                };
                let mut state = self.state.borrow_mut();
                let _ = state.tree.insert_before(parent, text_node, Some(*sibling));
            }
        }
    }

    fn add_attrs_if_missing(&self, target: &Self::Handle, attrs: Vec<HtmlAttribute>) {
        let (attributes, attr_error) = Self::convert_attrs(attrs);
        if let Some(error) = attr_error {
            self.halt(error);
            return;
        }
        let mut state = self.state.borrow_mut();
        if state.halted {
            return;
        }
        for attr in attributes {
            if state.tree.attribute(*target, &attr.name).is_none() {
                let _ = state.tree.set_attribute(*target, attr.name, attr.value);
            }
        }
    }

    fn remove_from_parent(&self, target: &Self::Handle) {
        let mut state = self.state.borrow_mut();
        if state.halted {
            return;
        }
        if let Some(parent) = state.tree.parent(*target) {
            let _ = state.tree.remove(parent, *target);
        }
    }

    fn reparent_children(&self, node: &Self::Handle, new_parent: &Self::Handle) {
        let mut state = self.state.borrow_mut();
        if state.halted {
            return;
        }
        let Some(children) = state.tree.children(*node).map(<[NodeId]>::to_vec) else {
            return;
        };
        for child in children {
            let _ = state.tree.append(*new_parent, child);
        }
    }

    fn is_mathml_annotation_xml_integration_point(&self, handle: &Self::Handle) -> bool {
        self.state
            .borrow()
            .mathml_integration_points
            .contains(handle)
    }
}

/// The `ElemName` implementation handed back to the tree builder.
///
/// Owns interned copies of the namespace/local-name atoms (cheap: the
/// external atom types are interned, so construction from an existing
/// string is a table hit). The GAT lifetime is intentionally unused —
/// nothing borrows from the sink, so the value can outlive the `elem_name`
/// call inside the builder's temporaries.
#[derive(Debug)]
pub struct SinkElemName {
    ns: html5ever::Namespace,
    local: html5ever::LocalName,
}

impl html5ever::interface::ElemName for SinkElemName {
    fn ns(&self) -> &html5ever::Namespace {
        &self.ns
    }

    fn local_name(&self) -> &html5ever::LocalName {
        &self.local
    }
}

fn non_empty(value: String) -> Option<String> {
    if value.is_empty() { None } else { Some(value) }
}
