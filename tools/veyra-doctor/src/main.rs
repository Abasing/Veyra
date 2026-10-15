//! CLI entry point for veyra-doctor.

use std::process::ExitCode;

use veyra_doctor::resolve_root;

fn main() -> ExitCode {
    let mut root_arg: Option<String> = None;
    let mut quiet = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--quiet" | "-q" => quiet = true,
            "--help" | "-h" => {
                println!("veyra-doctor — Veyra repository invariant checks");
                println!("Usage: veyra-doctor [--quiet] [repo-root]");
                println!("Defaults to the repository root inferred from the crate location.");
                return ExitCode::SUCCESS;
            }
            other => root_arg = Some(other.to_string()),
        }
    }

    let root = resolve_root(root_arg);
    let report = veyra_doctor::run_all(&root);

    if !quiet {
        let mut failures = 0usize;
        let mut warnings = 0usize;
        for f in &report.findings {
            println!("{f}");
            match f {
                veyra_doctor::Finding::Fail { .. } => failures += 1,
                veyra_doctor::Finding::Warn { .. } => warnings += 1,
            }
        }
        println!();
        println!(
            "veyra-doctor: {failures} failure(s), {warnings} warning(s) — root: {}",
            root.display()
        );
    }

    if report.is_clean() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
