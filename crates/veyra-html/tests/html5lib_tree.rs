// html5lib tree-construction conformance runner (roadmap Phase 2,
// acceptance criterion 1).
//
// Corpus: html5lib/html5lib-tests `tree-construction/*.dat`, vendored at
// tests/data/html5lib-tree-construction/ (MIT license, LICENSE file
// included; revision: commit 9329e64694e7, 2026-06-22, the last
// tree-construction state before upstream moved the suite to WPT).
//
// The runner parses the .dat sections (#data/#errors/#document-fragment/
// #document, #script-on/#script-off), parses each case through
// veyra-html, dumps the tree in html5lib format, and compares. Error-list
// assertions are intentionally not part of the claim (documented in the
// compatibility matrix); tree shape is the conformance surface here.
//
// Documented deltas live in tests/data/deltas.md; the DELTAS table below
// is the only skip mechanism, keyed by (file, 1-based case index), and
// must shrink — never grow silently — as the parser improves.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(missing_docs)]

use veyra_dom::{QualName, Tree};
use veyra_html::{
    ParseOptions, dump_fragment, dump_tree, parse_document_with_options, parse_fragment,
};
use veyra_url::Url;

const FILES: &[&str] = &[
    "tests1.dat",
    "tests4.dat",
    "tests5.dat",
    "adoption01.dat",
    "adoption02.dat",
    "inbody01.dat",
    "template.dat",
    "webkit02.dat",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    None,
    Data,
    Errors,
    FragmentContext,
    Document,
}

/// Parsed .dat test case.
struct Case {
    file: &'static str,
    /// 1-based index of the case within its file.
    index: usize,
    data: String,
    fragment_context: Option<String>,
    expected: Vec<String>,
    scripting: bool,
}

/// Parses a .dat file into cases (html5lib section format: `#`-prefixed
/// markers open sections; blank lines are ignored everywhere, matching
/// upstream's loader).
fn parse_dat(file: &'static str, content: &str) -> Vec<Case> {
    let mut cases = Vec::new();
    let mut current: Option<Case> = None;
    let mut section = Section::None;
    let mut scripting = false;

    fn close(cases: &mut Vec<Case>, current: Option<Case>) {
        if let Some(mut case) = current {
            // The section format appends one newline per data line; the
            // final one is a format artifact, not input (the intended
            // markup is the section text without the terminating break).
            if case.data.ends_with('\n') {
                case.data.pop();
            }
            cases.push(case);
        }
    }

    for line in content.lines() {
        if let Some(marker) = line.strip_prefix('#') {
            match marker.trim() {
                "data" => {
                    close(&mut cases, current.take());
                    current = Some(Case {
                        file,
                        index: cases.len() + 1,
                        data: String::new(),
                        fragment_context: None,
                        expected: Vec::new(),
                        scripting,
                    });
                    section = Section::Data;
                }
                "errors" | "new-errors" => section = Section::Errors,
                "document-fragment" => section = Section::FragmentContext,
                "document" => section = Section::Document,
                // The markers apply to the case in progress when placed
                // inside one (the common layout: same input repeated with
                // #script-on and #script-off); otherwise they set the
                // default for subsequent cases.
                "script-on" => match current.as_mut() {
                    Some(case) => case.scripting = true,
                    None => scripting = true,
                },
                "script-off" => match current.as_mut() {
                    Some(case) => case.scripting = false,
                    None => scripting = false,
                },
                _ => section = Section::None,
            }
            continue;
        }
        if line.is_empty() {
            continue;
        }
        let Some(case) = current.as_mut() else {
            continue;
        };
        match section {
            Section::Data => {
                case.data.push_str(line);
                case.data.push('\n');
            }
            Section::FragmentContext => {
                if case.fragment_context.is_none() {
                    case.fragment_context = Some(line.trim().to_string());
                }
            }
            Section::Document => case.expected.push(line.to_string()),
            Section::Errors | Section::None => {}
        }
    }
    close(&mut cases, current.take());
    cases
}

#[test]
fn corpus_runs_and_records_pass_counts() {
    let mut total = 0usize;
    let mut passed = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for file in FILES {
        let content = load_file(file);
        for case in parse_dat(file, &content) {
            total += 1;
            let key = (case.file, case.index);
            if DELTAS.contains(&key) {
                continue;
            }
            match run_case(&case) {
                Ok(()) => passed += 1,
                Err(message) => failures.push(format!(
                    "{file}#{} (data {:?}): {message}",
                    case.index,
                    first_line(&case.data)
                )),
            }
        }
    }

    println!("html5lib tree-construction: {passed}/{total} passed");
    assert!(
        failures.is_empty(),
        "{} case(s) failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

fn run_case(case: &Case) -> Result<(), String> {
    let url = Url::parse("https://html5lib.example/").unwrap();
    let options = ParseOptions {
        scripting_enabled: case.scripting,
    };
    match &case.fragment_context {
        None => {
            let tree = parse_document_with_options(case.data.as_bytes(), url.origin(), &options)
                .map_err(|error| format!("parse failed: {error}"))?;
            let actual = dump_tree(&tree);
            compare(&case.expected, &actual)
        }
        Some(context_name) => {
            let mut tree = Tree::new_document(url.origin()).unwrap();
            let context = tree
                .create_element(QualName::html(context_name.as_str()), Vec::new())
                .unwrap();
            // A template context gets its contents fragment attached up
            // front so the sink's get_template_contents resolves (the
            // parser inserts under its own synthetic root regardless).
            if context_name == "template" {
                let contents = tree.create_fragment().unwrap();
                tree.set_template_contents(context, contents).unwrap();
            }
            let tree = parse_fragment(&case.data, tree, context, &options)
                .map_err(|error| format!("fragment parse failed: {error}"))?;
            // html5ever's fragment algorithm builds its own synthetic html
            // root under the document and inserts there (the context
            // element steers tokenizer state and insertion mode); the
            // dump is that root's children in every fragment case.
            let root = tree
                .document()
                .and_then(|document| tree.children(document)?.first().copied())
                .ok_or("fragment root missing")?;
            let actual = dump_fragment(&tree, root);
            compare(&case.expected, &actual)
        }
    }
}

fn compare(expected: &[String], actual: &str) -> Result<(), String> {
    let actual_lines: Vec<String> = actual.lines().map(str::to_string).collect();
    if expected == actual_lines {
        return Ok(());
    }
    let mut diff = String::from("tree mismatch\n--- expected\n");
    for line in expected {
        diff.push_str(line);
        diff.push('\n');
    }
    diff.push_str("--- actual\n");
    for line in actual_lines {
        diff.push_str(&line);
        diff.push('\n');
    }
    Err(diff)
}

fn first_line(data: &str) -> String {
    data.lines().next().unwrap_or("").to_string()
}

fn load_file(name: &str) -> String {
    let path = format!("tests/data/html5lib-tree-construction/{name}");
    std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("corpus file {name} must be vendored: {error}"))
}

/// (file, 1-based case index) pairs excluded from the pass assertion; each
/// must have a section in tests/data/deltas.md. See that file for the
/// reason: all four are the deferred `<selectedcontent>` option-cloning
/// behavior (2025 element; needs subtree cloning in veyra-dom).
const DELTAS: &[(&str, usize)] = &[
    ("webkit02.dat", 45),
    ("webkit02.dat", 46),
    ("webkit02.dat", 47),
    ("webkit02.dat", 48),
];
