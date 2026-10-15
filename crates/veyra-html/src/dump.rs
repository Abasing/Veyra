//! The html5lib-format tree dumper.
//!
//! Produces exactly the `#document` dump format of the html5lib tree
//! construction tests (and WPT's tree-construction runner): `| <tag>`,
//! two-space indents per level, attributes sorted by name before
//! children, `content` marker under template elements, quoted and escaped
//! text. This is the conformance-test output AND the shell's `--dump-dom`
//! format — one code path, so what the tests verify is what users see.

use veyra_dom::{Namespace, NodeData, NodeId, QualName, Tree};

/// Dumps the tree rooted at the document in html5lib test format (with a
/// trailing newline per line).
#[must_use]
pub fn dump_tree(tree: &Tree) -> String {
    let mut out = String::new();
    if let Some(document) = tree.document() {
        dump_node(tree, document, 0, &mut out);
    }
    out
}

/// Dumps a fragment: the children of `root`, html5lib format.
#[must_use]
pub fn dump_fragment(tree: &Tree, root: NodeId) -> String {
    let mut out = String::new();
    if let Some(children) = tree.children(root) {
        for child in children.iter().copied() {
            dump_node(tree, child, 0, &mut out);
        }
    }
    out
}

fn indent(out: &mut String, level: usize) {
    out.push_str("| ");
    for _ in 0..level {
        out.push_str("  ");
    }
}

fn dump_node(tree: &Tree, id: NodeId, level: usize, out: &mut String) {
    let Some(data) = tree.data(id) else {
        return;
    };
    match data {
        NodeData::Document { .. } => {
            for child in children_of(tree, id) {
                dump_node(tree, child, level, out);
            }
        }
        NodeData::Doctype {
            name,
            public_id,
            system_id,
        } => {
            indent(out, level);
            out.push_str("<!DOCTYPE ");
            out.push_str(name);
            if let Some(public) = public_id {
                out.push_str(" \"");
                out.push_str(&escape_dump_text(public));
                out.push('"');
            }
            if let Some(system) = system_id {
                out.push_str(" \"");
                out.push_str(&escape_dump_text(system));
                out.push('"');
            }
            out.push_str(">\n");
        }
        NodeData::Element { name, .. } => {
            indent(out, level);
            out.push('<');
            out.push_str(&element_dump_name(name));
            out.push_str(">\n");
            // Attributes first, sorted by their dump name (the html5lib
            // dump format).
            if let Some(atts) = attributes_of(tree, id) {
                let mut rendered: Vec<(String, String)> = atts
                    .into_iter()
                    .map(|(name, value)| (attribute_dump_name(&name), value))
                    .collect();
                rendered.sort_by(|a, b| a.0.cmp(&b.0));
                for (name, value) in rendered {
                    indent(out, level + 1);
                    out.push_str(&name);
                    out.push_str("=\"");
                    out.push_str(&escape_dump_text(&value));
                    out.push_str("\"\n");
                }
            }
            // Template contents: the `content` marker and its children.
            if let Some(contents) = tree.template_contents(id) {
                indent(out, level + 1);
                out.push_str("content\n");
                for child in children_of(tree, contents) {
                    dump_node(tree, child, level + 2, out);
                }
            }
            for child in children_of(tree, id) {
                dump_node(tree, child, level + 1, out);
            }
        }
        NodeData::Text { data } => {
            indent(out, level);
            out.push('"');
            out.push_str(&escape_dump_text(data));
            out.push_str("\"\n");
        }
        NodeData::Comment { data } => {
            indent(out, level);
            out.push_str("<!-- ");
            out.push_str(data);
            out.push_str(" -->\n");
        }
        NodeData::Fragment => {
            for child in children_of(tree, id) {
                dump_node(tree, child, level, out);
            }
        }
    }
}

fn children_of(tree: &Tree, id: NodeId) -> Vec<NodeId> {
    tree.children(id)
        .map(<[NodeId]>::to_vec)
        .unwrap_or_default()
}

fn attributes_of(tree: &Tree, id: NodeId) -> Option<Vec<(QualName, String)>> {
    match tree.data(id) {
        Some(NodeData::Element { attributes, .. }) => Some(
            attributes
                .iter()
                .map(|a| (a.name.clone(), a.value.clone()))
                .collect(),
        ),
        _ => None,
    }
}

/// Element name in dump form: bare local name in the HTML namespace,
/// `abbrev local` otherwise (html5lib format: `svg svg`, `math math`).
fn element_dump_name(name: &QualName) -> String {
    match &name.namespace {
        Namespace::Html => match &name.prefix {
            Some(prefix) => format!("{prefix}:{}", name.local),
            None => name.local.clone(),
        },
        Namespace::Svg => format!("svg {}", name.local),
        Namespace::MathMl => format!("math {}", name.local),
        Namespace::Other(url) => format!("{url} {}", name.local),
    }
}

/// Attribute name in dump form, per the html5lib/WPT dumper: adjusted
/// foreign attributes render as `abbrev local` (space), HTML-namespace
/// attributes with a leftover prefix render as `prefix:local`, and plain
/// HTML attributes are bare.
fn attribute_dump_name(name: &QualName) -> String {
    match &name.namespace {
        Namespace::Html => match &name.prefix {
            Some(prefix) => format!("{prefix}:{}", name.local),
            None => name.local.clone(),
        },
        Namespace::Other(url) if url == "http://www.w3.org/2000/xmlns/" => {
            format!("xmlns {}", name.local)
        }
        Namespace::Other(url) if url == "http://www.w3.org/1999/xlink" => {
            format!("xlink {}", name.local)
        }
        Namespace::Other(url) if url == "http://www.w3.org/XML/1998/namespace" => {
            format!("xml {}", name.local)
        }
        Namespace::Other(url) => match &name.prefix {
            Some(prefix) => format!("{prefix}:{}", name.local),
            None => format!("{url} {}", name.local),
        },
        Namespace::Svg | Namespace::MathMl => name.local.clone(),
    }
}

/// The html5lib text escaping: backslash, quote, and newlines become
/// escape sequences so one text node always renders on one line.
fn escape_dump_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out
}
