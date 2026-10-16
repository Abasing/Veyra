//! Style-sheet loading for navigation: the document's `<style>` and
//! `<link rel="stylesheet">` sheets, plus the `@import` closure, feeding
//! the cascade.
//!
//! Placement per the dependency graph (`docs/architecture/
//! dependency-graph.md` §1): the shell's browser layer composes the
//! services — it is the only crate allowed to pair `veyra-net` fetches
//! with `veyra-style` parsing. The loader owns the resource guards the
//! cascade cannot (it has no I/O): per-sheet and total byte caps (the
//! engineering-standards limits discipline), a sheet-count cap, an
//! `@import` depth cap, and a visited set against import cycles.
//!
//! Failure posture: a sheet that cannot be fetched or parsed is a
//! *diagnostic* (counted in [`StyleLoadReport`]), never a document-load
//! failure — real pages reference stylesheets that 404, and the page
//! must still render. Sub-resource requests are GETs issued only from
//! this explicit load path; there is no prefetch or speculation yet.
//!
//! Encoding: UTF-8 (with BOM) per `veyra-style`'s batch-1 discipline;
//! a non-UTF-8 sheet is a counted rejection, not a misparse. The
//! charset stack is the recorded work item (compatibility matrix, ST6).
//!
//! Deviations recorded for this batch (each with a roadmap home):
//!
//! - `@import layer(...)` targets load as unlayered sheets (wrapping
//!   fetched sheets in a layer needs cascade-level support that does
//!   not exist yet).
//! - `@import` with a media prelude is fetched only when the prelude
//!   matches the degenerate evaluator; when it matches, the sheet loads
//!   unscoped (per-`@import` media scoping is not representable yet).
//! - `<link>` `rel` matching accepts `stylesheet` as a whitespace-list
//!   member (the common `rel="preload stylesheet"`-style forms); other
//!   rel values are ignored. `media` attributes gate on the same
//!   degenerate evaluator.

use std::collections::HashSet;

use veyra_dom::{NodeData, NodeId, Tree};
use veyra_style::{ComputedStyleMap, CssRule, Limits, StyleSet, Stylesheet};
use veyra_url::Url;

/// Sheet-loading resource guards (shell-owned; the cascade has no I/O).
#[derive(Debug, Clone, Copy)]
pub struct StyleLoadLimits {
    /// Maximum number of sheets (document sheets + imports).
    pub max_sheets: usize,
    /// Maximum total sheet bytes across the document.
    pub max_total_bytes: usize,
    /// Maximum `@import` nesting depth.
    pub max_import_depth: usize,
}

impl Default for StyleLoadLimits {
    fn default() -> Self {
        StyleLoadLimits {
            max_sheets: 64,
            max_total_bytes: 4 * 1024 * 1024,
            max_import_depth: 8,
        }
    }
}

/// What the style load did (printed by the `open` command's `--styles`
/// report; deterministic ordering).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StyleLoadReport {
    /// Sheets that parsed and joined the style set.
    pub sheets_loaded: usize,
    /// Sheets rejected (fetch failure, limit, encoding, parse).
    pub sheets_failed: usize,
    /// `@import` requests followed (successful or not).
    pub imports_followed: usize,
}

/// The style outcome attached to a loaded document.
pub struct DocumentStyles {
    /// Computed styles for every element (UA sheet + document sheets).
    pub computed: ComputedStyleMap,
    /// The load report (diagnostics).
    pub report: StyleLoadReport,
}

/// Collects the document's sheets, loads them (with guards), and
/// computes the element styles. Never fails as a whole: sheet failures
/// degrade to the remaining sheets (the UA defaults always apply).
pub fn load_document_styles(
    tree: &Tree,
    base: &Url,
    client: &veyra_net::Client,
    jar: Option<&veyra_net::cookie::CookieJar>,
) -> DocumentStyles {
    let limits = StyleLoadLimits::default();
    let mut state = LoadState {
        set: StyleSet::new(),
        report: StyleLoadReport::default(),
        visited: HashSet::new(),
        total_bytes: 0,
        limits,
    };

    for source in collect_style_sources(tree) {
        match source {
            StyleSource::Inline { css, media } => {
                if !veyra_style::media_query_applies(&media) {
                    continue;
                }
                state.load_sheet_text(&css, base, client, jar);
            }
            StyleSource::Linked { href, media } => {
                if !veyra_style::media_query_applies(&media) {
                    continue;
                }
                // A non-absolute href resolves against the document URL
                // (single URL authority: veyra-url does the join).
                let resolved = base.join(href.trim()).ok();
                if let Some(url) = resolved {
                    state.load_sheet_url(&url, client, jar, 0);
                } else {
                    state.report.sheets_failed += 1;
                }
            }
        }
    }

    let computed = veyra_style::compute_styles(tree, &state.set);
    DocumentStyles {
        computed,
        report: state.report,
    }
}

/// One style source found in the document tree.
enum StyleSource {
    /// A `<style>` element: its concatenated text children and the
    /// `media` attribute.
    Inline { css: String, media: String },
    /// A `<link rel~=stylesheet>` element: its `href` and `media`.
    Linked { href: String, media: String },
}

/// Walks the tree collecting `<style>` and `<link rel~=stylesheet>`
/// sources in document order.
fn collect_style_sources(tree: &Tree) -> Vec<StyleSource> {
    let Some(document) = tree.document() else {
        return Vec::new();
    };
    let mut sources = Vec::new();
    for id in tree.descendants(document) {
        let Some(NodeData::Element {
            name, attributes, ..
        }) = tree.data(id)
        else {
            continue;
        };
        if name.namespace != veyra_dom::Namespace::Html {
            continue;
        }
        let attr = |local: &str| {
            attributes
                .iter()
                .find(|attribute| attribute.name.local.eq_ignore_ascii_case(local))
                .map(|attribute| attribute.value.clone())
                .unwrap_or_default()
        };
        match name.local.as_str() {
            "style" => {
                let css = text_children(tree, id);
                sources.push(StyleSource::Inline {
                    css,
                    media: attr("media"),
                });
            }
            "link" => {
                let rel = attr("rel");
                let is_stylesheet = rel
                    .split_ascii_whitespace()
                    .any(|token| token.eq_ignore_ascii_case("stylesheet"));
                if is_stylesheet {
                    let href = attr("href");
                    if !href.trim().is_empty() {
                        sources.push(StyleSource::Linked {
                            href,
                            media: attr("media"),
                        });
                    }
                }
            }
            _ => {}
        }
    }
    sources
}

/// Concatenates the text-node children of an element (the `<style>`
/// content; CDATA-equivalent text only — child elements are ignored,
/// which is what the HTML parser can produce there anyway).
fn text_children(tree: &Tree, id: NodeId) -> String {
    let mut css = String::new();
    if let Some(children) = tree.children(id) {
        for child in children {
            if let Some(NodeData::Text { data }) = tree.data(*child) {
                css.push_str(data);
            }
        }
    }
    css
}

/// Mutable load state: the style set under construction plus the guards.
struct LoadState {
    set: StyleSet,
    report: StyleLoadReport,
    visited: HashSet<String>,
    total_bytes: usize,
    limits: StyleLoadLimits,
}

impl LoadState {
    /// Fetches a sheet URL (network or local file) and loads it.
    fn load_sheet_url(
        &mut self,
        url: &Url,
        client: &veyra_net::Client,
        jar: Option<&veyra_net::cookie::CookieJar>,
        depth: usize,
    ) {
        if !self.reserve_sheet() {
            return;
        }
        match url.scheme() {
            "http" | "https" => match veyra_net::blocking::fetch(client, url, jar) {
                Ok(resource) => {
                    let body = resource.body;
                    if !self.charge_bytes(body.len()) {
                        return;
                    }
                    self.load_sheet_bytes(&body, Some(url.clone()), depth, client, jar);
                }
                Err(_) => {
                    self.report.sheets_failed += 1;
                }
            },
            "file" => match url.to_file_path() {
                Ok(path) => match std::fs::read(&path) {
                    Ok(bytes) => {
                        if !self.charge_bytes(bytes.len()) {
                            return;
                        }
                        self.load_sheet_bytes(&bytes, Some(url.clone()), depth, client, jar);
                    }
                    Err(_) => {
                        self.report.sheets_failed += 1;
                    }
                },
                Err(_) => {
                    self.report.sheets_failed += 1;
                }
            },
            _ => {
                // Unsupported scheme for a sub-resource: a counted
                // rejection (the document itself still loads).
                self.report.sheets_failed += 1;
            }
        }
    }

    /// Loads sheet text collected from a `<style>` element (`base` is
    /// the document URL, where the sheet's `@import` targets resolve).
    fn load_sheet_text(
        &mut self,
        css: &str,
        base: &Url,
        client: &veyra_net::Client,
        jar: Option<&veyra_net::cookie::CookieJar>,
    ) {
        if !self.reserve_sheet() {
            return;
        }
        if !self.charge_bytes(css.len()) {
            return;
        }
        match Stylesheet::parse(css, &Limits::default()) {
            Ok(sheet) => {
                self.load_with_imports(sheet, Some(base.clone()), 0, client, jar);
                self.report.sheets_loaded += 1;
            }
            Err(_) => {
                self.report.sheets_failed += 1;
            }
        }
    }

    /// Loads fetched sheet bytes (UTF-8 discipline; typed rejections
    /// count as failures).
    fn load_sheet_bytes(
        &mut self,
        bytes: &[u8],
        base: Option<Url>,
        depth: usize,
        client: &veyra_net::Client,
        jar: Option<&veyra_net::cookie::CookieJar>,
    ) {
        match Stylesheet::parse_bytes(bytes, &Limits::default()) {
            Ok(sheet) => {
                self.load_with_imports(sheet, base, depth, client, jar);
                self.report.sheets_loaded += 1;
            }
            Err(_) => {
                self.report.sheets_failed += 1;
            }
        }
    }

    /// Follows a parsed sheet's `@import` requests depth-first (imported
    /// rules order before the importing sheet's own rules), then appends
    /// the sheet itself. The depth cap and visited-set cycle guard are
    /// the loader's resource guards.
    fn load_with_imports(
        &mut self,
        sheet: Stylesheet,
        base: Option<Url>,
        depth: usize,
        client: &veyra_net::Client,
        jar: Option<&veyra_net::cookie::CookieJar>,
    ) {
        if depth < self.limits.max_import_depth {
            for (target, layer_and_media) in sheet_imports(&sheet) {
                self.report.imports_followed += 1;
                let resolved = match &base {
                    Some(base) => base.join(target.trim()).ok(),
                    None => veyra_url::Url::parse(target.trim()).ok(),
                };
                let Some(url) = resolved else {
                    self.report.sheets_failed += 1;
                    continue;
                };
                if !self.visited.insert(url.as_str().to_string()) {
                    continue;
                }
                // Media-scoped imports load only when the prelude
                // matches (the degenerate evaluator); layer scoping is
                // the recorded deferral.
                if !veyra_style::media_query_applies(&layer_and_media.to_css_string_trimmed()) {
                    continue;
                }
                self.load_sheet_url(&url, client, jar, depth + 1);
            }
        }
        self.set.append_author(sheet, base);
    }

    /// Reserves a sheet slot against the count cap.
    fn reserve_sheet(&mut self) -> bool {
        let total = self.report.sheets_loaded + self.report.sheets_failed;
        if total >= self.limits.max_sheets {
            self.report.sheets_failed += 1;
            return false;
        }
        true
    }

    /// Charges bytes against the total cap.
    fn charge_bytes(&mut self, len: usize) -> bool {
        let new_total = self.total_bytes.saturating_add(len);
        if new_total > self.limits.max_total_bytes {
            self.report.sheets_failed += 1;
            return false;
        }
        self.total_bytes = new_total;
        true
    }
}

/// A sheet's top-level `@import` requests: (target string, retained
/// layer/media prelude). Nested imports are invalid CSS and dropped by
/// the parser.
fn sheet_imports(sheet: &Stylesheet) -> Vec<(String, veyra_style::RetainedValue)> {
    sheet
        .rules()
        .iter()
        .filter_map(|rule| match rule {
            CssRule::Import {
                url,
                layer_and_media,
            } => Some((url.clone(), layer_and_media.clone())),
            _ => None,
        })
        .collect()
}
