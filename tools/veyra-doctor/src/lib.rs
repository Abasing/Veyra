//! `veyra-doctor` — repository invariant checks for Veyra.
//!
//! Validates, without any external dependency:
//! required files and their minimum substance, honest-status phrasing,
//! forbidden marketing claims, research/architecture/roadmap/ADR document
//! structure, source-register dating, compatibility-matrix state
//! vocabulary, relative markdown links, and GitHub Actions action
//! pinning. Designed to be extended per phase (see roadmap Phase 0/1).
//!
//! Exit code 0 = all checks pass; 1 = at least one failure; warnings do
//! not affect the exit code.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

/// One finding from the check run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Finding {
    Fail { check: String, detail: String },
    Warn { check: String, detail: String },
}

impl Finding {
    fn fail(check: &str, detail: impl Into<String>) -> Self {
        Finding::Fail { check: check.to_string(), detail: detail.into() }
    }
    fn warn(check: &str, detail: impl Into<String>) -> Self {
        Finding::Warn { check: check.to_string(), detail: detail.into() }
    }
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Finding::Fail { check, detail } => write!(f, "FAIL [{check}] {detail}"),
            Finding::Warn { check, detail } => write!(f, "WARN [{check}] {detail}"),
        }
    }
}

/// Aggregate result of a full run.
#[derive(Debug, Default)]
pub struct Report {
    pub findings: Vec<Finding>,
}

impl Report {
    fn push(&mut self, f: Finding) {
        self.findings.push(f);
    }

    pub fn failures(&self) -> impl Iterator<Item = &Finding> {
        self.findings.iter().filter(|f| matches!(f, Finding::Fail { .. }))
    }

    pub fn warnings(&self) -> impl Iterator<Item = &Finding> {
        self.findings.iter().filter(|f| matches!(f, Finding::Warn { .. }))
    }

    pub fn is_clean(&self) -> bool {
        self.failures().count() == 0
    }
}

// ---------------------------------------------------------------------------
// Text helpers (regex-free on purpose: zero dependencies)
// ---------------------------------------------------------------------------

/// Case-insensitive substring test on UTF-8 (ASCII-aware lowering).
pub fn contains_ci(haystack: &str, needle: &str) -> bool {
    let h = haystack.to_lowercase();
    let n = needle.to_lowercase();
    h.contains(&n)
}

/// Extracts the first `YYYY-MM-DD` date found in a line, if any.
pub fn find_iso_date(line: &str) -> Option<String> {
    let b = line.as_bytes();
    let mut i = 0;
    while i + 10 <= b.len() {
        let ok = b[i].is_ascii_digit()
            && b[i + 1].is_ascii_digit()
            && b[i + 2].is_ascii_digit()
            && b[i + 3].is_ascii_digit()
            && b[i + 4] == b'-'
            && b[i + 5].is_ascii_digit()
            && b[i + 6].is_ascii_digit()
            && b[i + 7] == b'-'
            && b[i + 8].is_ascii_digit()
            && b[i + 9].is_ascii_digit();
        if ok {
            // Reject accidental matches preceded/followed by more digits.
            let prev_ok = i == 0 || !b[i - 1].is_ascii_digit();
            let next = i + 10;
            let next_ok = next >= b.len() || !b[next].is_ascii_digit();
            if prev_ok && next_ok {
                return Some(line[i..i + 10].to_string());
            }
        }
        i += 1;
    }
    None
}

/// True if `s` (the token after `@` in a `uses:` line) is exactly 40 hex chars.
pub fn is_full_commit_sha(s: &str) -> bool {
    let t = s.trim();
    t.len() == 40 && t.bytes().all(|c| c.is_ascii_hexdigit())
}

/// GitHub-style slug for a markdown heading (best effort, for anchor checks).
pub fn heading_slug(heading: &str) -> String {
    let mut out = String::new();
    for ch in heading.trim().to_lowercase().chars() {
        if ch.is_alphanumeric() {
            out.push(ch);
        } else if ch == ' ' || ch == '-' || ch == '_' {
            out.push('-');
        }
        // other punctuation dropped (GitHub-ish approximation)
    }
    out
}

/// Extracts markdown link targets from inline links `[text](target)`.
pub fn extract_link_targets(md: &str) -> Vec<String> {
    let b = md.as_bytes();
    let mut targets = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'(' && i > 0 && b[i - 1] == b']' {
            // find matching close paren
            let start = i + 1;
            let mut depth = 1;
            let mut j = start;
            while j < b.len() && depth > 0 {
                match b[j] {
                    b'(' => depth += 1,
                    b')' => depth -= 1,
                    _ => {}
                }
                j += 1;
            }
            if depth == 0 {
                let target = md[start..j - 1].trim();
                // Exclude images inside brackets already handled; skip URLs/anchors/mailto
                if !target.starts_with("http://")
                    && !target.starts_with("https://")
                    && !target.starts_with('#')
                    && !target.starts_with("mailto:")
                    && !target.is_empty()
                {
                    targets.push(target.to_string());
                }
            }
            i = j;
        } else {
            i += 1;
        }
    }
    targets
}

/// Extracts markdown headings (lines starting with `#`s).
pub fn extract_headings(md: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for line in md.lines() {
        let trimmed = line.trim_start();
        let level = trimmed.chars().take_while(|c| *c == '#').count();
        if level > 0 && trimmed.len() > level {
            let text = trimmed[level..].trim().to_string();
            out.push((level, text));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Check definitions
// ---------------------------------------------------------------------------

fn required_files() -> Vec<(&'static str, u64)> {
    vec![
        ("README.md", 2_000),
        ("LICENSE", 10_000),
        ("CONTRIBUTING.md", 3_000),
        ("SECURITY.md", 1_500),
        ("CODE_OF_CONDUCT.md", 3_000),
        ("Cargo.toml", 400),
        ("deny.toml", 500),
        ("rust-toolchain.toml", 200),
        ("rustfmt.toml", 40),
        ("pyproject.toml", 200),
        ("docs/README.md", 1_500),
        ("docs/research/README.md", 1_500),
        ("docs/research/chromium-architecture.md", 3_500),
        ("docs/research/web-platform-and-rendering.md", 3_500),
        ("docs/research/javascript-and-wasm.md", 3_500),
        ("docs/research/security-model.md", 3_500),
        ("docs/research/networking-and-storage.md", 3_500),
        ("docs/research/browser-product-features.md", 3_500),
        ("docs/research/testing-and-performance.md", 3_500),
        ("docs/research/dependency-and-license-audit.md", 3_500),
        ("docs/research/source-register.md", 4_000),
        ("docs/research/coverage-matrix.md", 2_000),
        ("docs/architecture/system-overview.md", 3_000),
        ("docs/architecture/process-and-security-model.md", 3_000),
        ("docs/architecture/rendering-pipeline.md", 3_000),
        ("docs/architecture/execution-and-web-apis.md", 3_000),
        ("docs/architecture/dependency-graph.md", 3_000),
        ("docs/architecture/decision-log.md", 1_000),
        ("docs/decisions/README.md", 800),
        ("docs/roadmap/implementation-plan.md", 12_000),
        ("docs/roadmap/compatibility-program.md", 2_500),
        ("docs/roadmap/compatibility-matrix.md", 2_500),
        ("docs/engineering/engineering-standards.md", 4_000),
        ("tools/veyra-doctor/Cargo.toml", 200),
        ("tools/veyra-doctor/src/main.rs", 300),
        ("tools/veyra-doctor/src/lib.rs", 8_000),
        ("tools/linkutil.py", 1_500),
        ("tools/verify_sources.py", 1_500),
        ("tools/check_external_links.py", 1_500),
        ("tests/test_tooling.py", 800),
        (".github/workflows/ci.yml", 1_500),
        (".github/workflows/scheduled.yml", 600),
        (".github/PULL_REQUEST_TEMPLATE.md", 300),
        (".github/ISSUE_TEMPLATE/bug_report.yml", 300),
        (".github/ISSUE_TEMPLATE/engineering_task.yml", 300),
        (".github/ISSUE_TEMPLATE/config.yml", 100),
        (".github/dependabot.yml", 150),
    ]
}

const RESEARCH_DOCS: [&str; 9] = [
    "chromium-architecture.md",
    "web-platform-and-rendering.md",
    "javascript-and-wasm.md",
    "security-model.md",
    "networking-and-storage.md",
    "browser-product-features.md",
    "testing-and-performance.md",
    "dependency-and-license-audit.md",
    "source-register.md",
];

const ARCH_DOCS: [&str; 6] = [
    "system-overview.md",
    "process-and-security-model.md",
    "rendering-pipeline.md",
    "execution-and-web-apis.md",
    "dependency-graph.md",
    "decision-log.md",
];

const MATRIX_STATES: [&str; 7] = [
    "not investigated",
    "investigated",
    "designed",
    "in development",
    "partially implemented",
    "tested with known failures",
    "meets defined acceptance criteria",
];

const FORBIDDEN_CLAIMS: [&str; 7] = [
    "fully compatible",
    "production-ready browser",
    "production ready browser",
    "chrome web store compatible",
    "100% compatible",
    "v8-level performance achieved",
    "web-compatible with all sites",
];

fn read_repo_file(root: &Path, rel: &str) -> Option<String> {
    fs::read_to_string(root.join(rel)).ok()
}

fn check_required_files(root: &Path, report: &mut Report) {
    for (path, min) in required_files() {
        let full = root.join(path);
        match fs::metadata(&full) {
            Ok(m) if m.is_file() => {
                let size = m.len();
                if size < min {
                    report.push(Finding::fail(
                        "required-files",
                        format!("{path} exists but is only {size} bytes (minimum {min})"),
                    ));
                }
            }
            Ok(_) => report.push(Finding::fail("required-files", format!("{path} is not a regular file"))),
            Err(_) => report.push(Finding::fail("required-files", format!("{path} is missing"))),
        }
    }
}

fn check_license_and_status(root: &Path, report: &mut Report) {
    match read_repo_file(root, "LICENSE") {
        Some(text) => {
            if !text.contains("Apache License") || !text.contains("Version 2.0, January 2004") {
                report.push(Finding::fail("license", "LICENSE does not appear to be the Apache-2.0 text"));
            }
        }
        None => report.push(Finding::fail("license", "LICENSE unreadable")),
    }
    match read_repo_file(root, "README.md") {
        Some(text) => {
            for phrase in ["early-stage", "independent", "not yet"] {
                if !contains_ci(&text, phrase) {
                    report.push(Finding::fail(
                        "honest-status",
                        format!("README.md must contain the phrase `{phrase}`"),
                    ));
                }
            }
        }
        None => report.push(Finding::fail("honest-status", "README.md unreadable")),
    }
}

fn check_forbidden_claims(root: &Path, report: &mut Report) {
    let mut stack = vec![root.to_path_buf()];
    let mut visited = 0usize;
    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                if name != ".git" && name != "target" && name != "node_modules" {
                    stack.push(path);
                }
            } else if path.extension().map(|e| e == "md").unwrap_or(false) {
                visited += 1;
                let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().to_string();
                if let Ok(text) = fs::read_to_string(&path) {
                    for claim in FORBIDDEN_CLAIMS {
                        if contains_ci(&text, claim) {
                            report.push(Finding::fail(
                                "forbidden-claims",
                                format!("{rel} contains forbidden claim `{claim}`"),
                            ));
                        }
                    }
                }
            }
        }
    }
    if visited < 20 {
        report.push(Finding::fail("forbidden-claims", format!("only {visited} markdown files scanned — repository structure unexpected")));
    }
}

fn check_research_docs(root: &Path, report: &mut Report) {
    for name in RESEARCH_DOCS {
        let rel = format!("docs/research/{name}");
        match read_repo_file(root, &rel) {
            Some(text) => {
                if !text.contains("## Evidence basis") {
                    report.push(Finding::fail("research-structure", format!("{rel} lacks `## Evidence basis`")));
                }
                if !text.contains("## Open questions") {
                    report.push(Finding::fail("research-structure", format!("{rel} lacks `## Open questions`")));
                }
                if !contains_ci(&text, "confidence") {
                    report.push(Finding::fail("research-structure", format!("{rel} lacks a Confidence section/mention")));
                }
            }
            None => report.push(Finding::fail("research-structure", format!("{rel} unreadable"))),
        }
    }
    // The research index must link every research doc.
    if let Some(index) = read_repo_file(root, "docs/research/README.md") {
        for name in RESEARCH_DOCS {
            if !index.contains(name) {
                report.push(Finding::fail(
                    "research-structure",
                    format!("docs/research/README.md does not reference {name}"),
                ));
            }
        }
    }
}

fn check_source_register(root: &Path, report: &mut Report) {
    let rel = "docs/research/source-register.md";
    match read_repo_file(root, rel) {
        Some(text) => {
            let mut url_rows = 0usize;
            for (idx, line) in text.lines().enumerate() {
                if line.contains("http") && line.contains('|') {
                    url_rows += 1;
                    if find_iso_date(line).is_none() {
                        report.push(Finding::fail(
                            "source-register",
                            format!("{rel}:{} row cites a URL without an access date", idx + 1),
                        ));
                    }
                }
            }
            if url_rows < 60 {
                report.push(Finding::fail(
                    "source-register",
                    format!("{rel} has only {url_rows} URL rows (expected >= 60)"),
                ));
            }
        }
        None => report.push(Finding::fail("source-register", format!("{rel} unreadable"))),
    }
}

fn check_adrs(root: &Path, report: &mut Report) {
    let dir = root.join("docs/decisions");
    let mut adrs: Vec<String> = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.ends_with(".md") && name != "README.md" {
                adrs.push(name);
            }
        }
    }
    adrs.sort();
    if adrs.is_empty() {
        report.push(Finding::fail("adr-structure", "no ADR files found in docs/decisions/"));
        return;
    }
    for (i, name) in adrs.iter().enumerate() {
        let expected_prefix = format!("{:04}", i + 1);
        if !name.starts_with(&expected_prefix) {
            report.push(Finding::fail("adr-structure", format!("ADR files must be numbered sequentially: found {name}, expected prefix {expected_prefix}")));
        }
        let rel = format!("docs/decisions/{name}");
        if let Some(text) = read_repo_file(root, &rel) {
            for heading in ["## Context", "## Decision", "## Consequences"] {
                if !text.contains(heading) {
                    report.push(Finding::fail("adr-structure", format!("{rel} lacks `{heading}`")));
                }
            }
            if !contains_ci(&text, "revisit conditions") {
                report.push(Finding::fail("adr-structure", format!("{rel} lacks revisit conditions")));
            }
        }
    }
    // decision-log.md must index every ADR
    if let Some(log) = read_repo_file(root, "docs/architecture/decision-log.md") {
        for name in &adrs {
            if !log.contains(name.as_str()) {
                report.push(Finding::fail("adr-structure", format!("docs/architecture/decision-log.md does not index {name}")));
            }
        }
    } else {
        report.push(Finding::fail("adr-structure", "docs/architecture/decision-log.md unreadable"));
    }
}

fn check_roadmap(root: &Path, report: &mut Report) {
    let rel = "docs/roadmap/implementation-plan.md";
    match read_repo_file(root, rel) {
        Some(text) => {
            for n in 0..=11 {
                let heading_prefix = format!("## Phase {n}");
                let pos = text.find(&heading_prefix);
                match pos {
                    Some(start) => {
                        let end = text[start + heading_prefix.len()..]
                            .find("\n## ")
                            .map(|off| start + heading_prefix.len() + off)
                            .unwrap_or(text.len());
                        let block = &text[start..end];
                        if !contains_ci(block, "acceptance criteria") {
                            report.push(Finding::fail("roadmap", format!("{rel}: Phase {n} block lacks acceptance criteria")));
                        }
                        if !contains_ci(block, "exit conditions") {
                            report.push(Finding::fail("roadmap", format!("{rel}: Phase {n} block lacks exit conditions")));
                        }
                    }
                    None => report.push(Finding::fail("roadmap", format!("{rel} lacks `## Phase {n}`"))),
                }
            }
            if !text.contains("## Minimum useful milestone") {
                report.push(Finding::fail("roadmap", format!("{rel} lacks `## Minimum useful milestone`")));
            }
        }
        None => report.push(Finding::fail("roadmap", format!("{rel} unreadable"))),
    }
}

fn check_architecture_docs(root: &Path, report: &mut Report) {
    for name in ARCH_DOCS {
        let rel = format!("docs/architecture/{name}");
        match read_repo_file(root, &rel) {
            Some(text) => {
                let headings = extract_headings(&text).into_iter().filter(|(lvl, _)| *lvl >= 2).count();
                if headings < 5 {
                    report.push(Finding::fail("architecture-structure", format!("{rel} has only {headings} level>=2 headings")));
                }
            }
            None => report.push(Finding::fail("architecture-structure", format!("{rel} unreadable"))),
        }
    }
    if let Some(graph) = read_repo_file(root, "docs/architecture/dependency-graph.md") {
        if !graph.contains("```mermaid") {
            report.push(Finding::fail("architecture-structure", "dependency-graph.md lacks a mermaid diagram"));
        }
    }
}

fn check_matrix_states(root: &Path, report: &mut Report) {
    let rel = "docs/roadmap/compatibility-matrix.md";
    if let Some(text) = read_repo_file(root, rel) {
        for state in MATRIX_STATES {
            if !contains_ci(&text, state) {
                report.push(Finding::fail("matrix-states", format!("{rel} must use the canonical state `{state}`")));
            }
        }
        if !text.contains("compatibility-program.md") {
            report.push(Finding::fail("matrix-states", format!("{rel} must reference the compatibility program")));
        }
    } else {
        report.push(Finding::fail("matrix-states", format!("{rel} unreadable")));
    }
}

fn check_workflows(root: &Path, report: &mut Report) {
    let dir = root.join(".github/workflows");
    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => {
            report.push(Finding::fail("workflows", ".github/workflows missing"));
            return;
        }
    };
    let mut found_any = false;
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if !(name.ends_with(".yml") || name.ends_with(".yaml")) {
            continue;
        }
        found_any = true;
        let rel = format!(".github/workflows/{name}");
        let text = match fs::read_to_string(e.path()) {
            Ok(t) => t,
            Err(_) => {
                report.push(Finding::fail("workflows", format!("{rel} unreadable")));
                continue;
            }
        };
        if !text.contains("\npermissions:") && !text.starts_with("permissions:") {
            report.push(Finding::fail("workflows", format!("{rel} lacks a top-level `permissions:` restriction")));
        }
        for (idx, line) in text.lines().enumerate() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("- uses:") {
                let spec = rest.trim();
                let sha_part = spec.rsplit('@').next().unwrap_or("");
                if !is_full_commit_sha(sha_part) {
                    report.push(Finding::fail(
                        "workflows",
                        format!("{rel}:{} action `{spec}` is not pinned to a full 40-hex commit SHA", idx + 1),
                    ));
                }
            }
        }
    }
    if !found_any {
        report.push(Finding::fail("workflows", "no workflow files found"));
    }
}

fn check_md_links(root: &Path, report: &mut Report) {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                if name != ".git" && name != "target" && name != "node_modules" {
                    stack.push(path);
                }
            } else if path.extension().map(|e| e == "md").unwrap_or(false) {
                let text = match fs::read_to_string(&path) {
                    Ok(t) => t,
                    Err(_) => continue,
                };
                let rel_parent = path.parent().unwrap_or(root);
                for target in extract_link_targets(&text) {
                    let file_part = target.split('#').next().unwrap_or("").trim();
                    if file_part.is_empty() {
                        // pure-anchor link: best-effort check
                        let anchor = target.trim_start_matches('#');
                        let slug = heading_slug(anchor);
                        let ok = extract_headings(&text)
                            .iter()
                            .any(|(_, h)| heading_slug(h) == slug);
                        if !ok {
                            report.push(Finding::warn(
                                "md-links",
                                format!("anchor #{anchor} in {} has no matching heading", path.display()),
                            ));
                        }
                        continue;
                    }
                    let dest = rel_parent.join(file_part);
                    if !dest.exists() {
                        let shown = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().to_string();
                        report.push(Finding::fail(
                            "md-links",
                            format!("{shown} links to missing file `{file_part}`"),
                        ));
                    } else if target.contains('#') {
                        // anchor into another file: best-effort
                        let anchor = target.split('#').nth(1).unwrap_or("");
                        if !anchor.is_empty() {
                            if let Ok(dest_text) = fs::read_to_string(&dest) {
                                let slug = heading_slug(anchor);
                                let ok = extract_headings(&dest_text)
                                    .iter()
                                    .any(|(_, h)| heading_slug(h) == slug);
                                if !ok {
                                    let shown = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().to_string();
                                    report.push(Finding::warn(
                                        "md-links",
                                        format!("{shown} anchor `#{anchor}` not found in {file_part} (best-effort check)"),
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn check_toolchain_and_lints(root: &Path, report: &mut Report) {
    if let Some(text) = read_repo_file(root, "rust-toolchain.toml") {
        if !text.contains("channel =") {
            report.push(Finding::fail("toolchain", "rust-toolchain.toml lacks a pinned channel"));
        }
    }
    if let Some(text) = read_repo_file(root, "Cargo.toml") {
        if !text.contains("unsafe_code = \"forbid\"") {
            report.push(Finding::fail("toolchain", "workspace must set unsafe_code = \"forbid\""));
        }
        if !text.contains("unwrap_used = \"deny\"") {
            report.push(Finding::fail("toolchain", "workspace must deny clippy::unwrap_used"));
        }
    }
    if let Some(text) = read_repo_file(root, "deny.toml") {
        if !text.contains("[licenses]") || !text.contains("Apache-2.0") {
            report.push(Finding::fail("toolchain", "deny.toml must define a license allow-list including Apache-2.0"));
        }
    }
}

/// Runs the full check suite against `root`.
pub fn run_all(root: &Path) -> Report {
    let mut report = Report::default();
    check_required_files(root, &mut report);
    check_license_and_status(root, &mut report);
    check_forbidden_claims(root, &mut report);
    check_research_docs(root, &mut report);
    check_source_register(root, &mut report);
    check_adrs(root, &mut report);
    check_roadmap(root, &mut report);
    check_architecture_docs(root, &mut report);
    check_matrix_states(root, &mut report);
    check_workflows(root, &mut report);
    check_md_links(root, &mut report);
    check_toolchain_and_lints(root, &mut report);
    report
}

/// Convenience: map of relative path -> bytes for tests and future tools.
pub fn snapshot_files(root: &Path, rels: &[&str]) -> BTreeMap<String, Option<u64>> {
    rels.iter()
        .map(|r| {
            let size = fs::metadata(root.join(r)).ok().map(|m| m.len());
            (r.to_string(), size)
        })
        .collect()
}

/// Resolve repo root the same way main.rs does (also used by tests).
pub fn resolve_root(explicit: Option<String>) -> PathBuf {
    if let Some(dir) = explicit {
        return PathBuf::from(dir);
    }
    if let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") {
        let p = PathBuf::from(manifest);
        // tools/veyra-doctor -> repo root
        if p.ends_with("tools/veyra-doctor") || p.ends_with("tools\\veyra-doctor") {
            if let Some(gp) = p.parent().and_then(|pp| pp.parent()) {
                return gp.to_path_buf();
            }
        }
        return p;
    }
    PathBuf::from(".")
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
//! Unit tests for veyra-doctor's parsing and check logic.

use std::fs;
use std::path::Path;

use crate::{
    contains_ci, extract_headings, extract_link_targets, find_iso_date, heading_slug,
    is_full_commit_sha, run_all, Finding,
};

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod helpers {
    use super::*;

    /// Builds a throwaway directory tree for checks that need one.
    pub fn tmpdir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "veyra-doctor-test-{tag}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    pub fn write(path: &Path, rel: &str, content: &str) {
        let full = path.join(rel);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(full, content).expect("write file");
    }
}

use helpers::{tmpdir, write};

#[test]
fn iso_date_detection() {
    assert_eq!(find_iso_date("accessed 2026-10-15 and more").as_deref(), Some("2026-10-15"));
    assert_eq!(find_iso_date("| 2026-10-15 | url |").as_deref(), Some("2026-10-15"));
    assert_eq!(find_iso_date("no date here"), None);
    assert_eq!(find_iso_date("12026-10-15"), None, "must not match inside longer digit runs");
}

#[test]
fn sha_pinning_detection() {
    assert!(is_full_commit_sha("11bd71901bbe5b1630ceea73d27597364c9af683"));
    assert!(is_full_commit_sha(
        "3d3c42e5aac5e0f57e1a2b30c25b6b1c0a4d8e9f"
    ));
    assert!(!is_full_commit_sha("v4.2.2"));
    assert!(!is_full_commit_sha("main"));
    assert!(!is_full_commit_sha("11bd719"));
}

#[test]
fn ci_substring() {
    assert!(contains_ci("Fully Compatible with Nothing", "fully compatible"));
    assert!(contains_ci("PRODUCTION-READY BROWSER", "production-ready browser"));
    assert!(!contains_ci("compatibility", "compatible-with"));
}

#[test]
fn heading_slug_matches_github_style() {
    assert_eq!(heading_slug("Phase 0 — Repository foundation"), "phase-0--repository-foundation");
    assert_eq!(heading_slug("What is it?"), "what-is-it");
    assert_eq!(heading_slug("A  B"), "a--b");
}

#[test]
fn link_extraction_skips_urls_and_anchors() {
    let md = "[docs](./docs/README.md) [web](https://example.com) [anchor](#phase) ![img](x.png)";
    let targets = extract_link_targets(md);
    assert!(targets.contains(&"./docs/README.md".to_string()));
    assert!(!targets.iter().any(|t| t.starts_with("http")));
    assert!(!targets.iter().any(|t| t.starts_with('#')));
}

#[test]
fn headings_extraction() {
    let md = "# Title\n\n## Section\n### Sub\n";
    let hs = extract_headings(md);
    assert_eq!(hs.len(), 3);
    assert_eq!(hs[1], (2, "Section".to_string()));
}

#[test]
fn run_all_fails_on_missing_repo() {
    let dir = tmpdir("empty");
    let report = run_all(&dir);
    assert!(!report.is_clean(), "empty tree must fail");
    let has_required = report
        .findings
        .iter()
        .any(|f| matches!(f, Finding::Fail { check, .. } if check == "required-files"));
    assert!(has_required);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn workflow_pinning_check_flags_unpinned_actions() {
    let dir = tmpdir("wf");
    write(
        &dir,
        ".github/workflows/ci.yml",
        "name: CI\non:\n  push:\npermissions:\n  contents: read\njobs:\n  a:\n    steps:\n      - uses: actions/checkout@v4\n",
    );
    let report = run_all(&dir);
    let flagged = report.findings.iter().any(|f| {
        matches!(f, Finding::Fail { check, detail }
            if check == "workflows" && detail.contains("actions/checkout@v4"))
    });
    assert!(flagged, "unpinned action must be flagged");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn forbidden_claim_check_fires() {
    let dir = tmpdir("claims");
    write(&dir, "docs/note.md", "We are fully compatible with the web.\n");
    let report = run_all(&dir);
    let flagged = report.findings.iter().any(|f| {
        matches!(f, Finding::Fail { check, detail }
            if check == "forbidden-claims" && detail.contains("fully compatible"))
    });
    assert!(flagged);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn adr_sequencing_check() {
    let dir = tmpdir("adr");
    write(&dir, "docs/decisions/0001-first.md", "# ADR 1\n\n## Context\nx\n\n## Decision\ny\n\n## Consequences\nz\n\n## Revisit conditions\nw\n");
    // missing 0002
    write(&dir, "docs/decisions/0003-third.md", "# ADR 3\n");
    let report = run_all(&dir);
    let flagged = report.findings.iter().any(|f| {
        matches!(f, Finding::Fail { check, detail }
            if check == "adr-structure" && detail.contains("sequentially"))
    });
    assert!(flagged, "gap in ADR numbering must be flagged");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn roadmap_phase_block_requires_acceptance_criteria() {
    let dir = tmpdir("roadmap");
    let mut plan = String::new();
    for n in 0..=11 {
        plan.push_str(&format!(
            "## Phase {n} — test\n\nsome text\n\nAcceptance criteria: x\n\nExit conditions: y\n\n"
        ));
    }
    plan.push_str("\n## Minimum useful milestone\n\nstuff\n");
    write(&dir, "docs/roadmap/implementation-plan.md", &plan);
    let report = run_all(&dir);
    let phase_failures = report
        .findings
        .iter()
        .filter(|f| {
            matches!(f, Finding::Fail { check, detail }
                if check == "roadmap" && detail.contains("lacks acceptance criteria"))
        })
        .count();
    assert_eq!(phase_failures, 0, "well-formed phases must pass: {report:?}");

    // Now remove acceptance criteria from Phase 2 and expect exactly one failure.
    let broken = plan.replace("## Phase 2 — test\n\nsome text\n\nAcceptance criteria: x", "## Phase 2 — test\n\nsome text");
    write(&dir, "docs/roadmap/implementation-plan.md", &broken);
    let report = run_all(&dir);
    let phase_failures = report
        .findings
        .iter()
        .filter(|f| {
            matches!(f, Finding::Fail { check, detail }
                if check == "roadmap" && detail.contains("Phase 2") && detail.contains("acceptance criteria"))
        })
        .count();
    assert_eq!(phase_failures, 1);
    let _ = fs::remove_dir_all(&dir);
}
}
