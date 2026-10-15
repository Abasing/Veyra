//! The `veyra-shell` binary entry point: thin dispatch over the library.
//!
//! Exit codes (documented in `--help`): 0 success, 1 operation failure,
//! 2 usage error. Errors are printed with their full source chain.

use std::process::ExitCode;

fn main() -> ExitCode {
    // `args()` panics on non-UTF-8 arguments; `args_os()` + lossy
    // conversion keeps malformed arguments a usage error instead.
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();

    match veyra_shell::cli::parse(&args) {
        Ok(veyra_shell::cli::ParseOutcome::Help(text)) => {
            print!("{text}");
            ExitCode::SUCCESS
        }
        Ok(veyra_shell::cli::ParseOutcome::Version(text)) => {
            println!("{text}");
            ExitCode::SUCCESS
        }
        Ok(veyra_shell::cli::ParseOutcome::Command(command)) => {
            match veyra_shell::execute(command) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    print_error_chain(&error);
                    ExitCode::from(1)
                }
            }
        }
        Err(usage) => {
            eprintln!("usage error: {usage}");
            eprintln!("run `veyra-shell --help` for usage");
            ExitCode::from(2)
        }
    }
}

/// Prints an error and its full source chain to stderr.
fn print_error_chain(error: &(dyn std::error::Error + 'static)) {
    eprintln!("error: {error}");
    let mut source = error.source();
    while let Some(cause) = source {
        eprintln!("  caused by: {cause}");
        source = cause.source();
    }
}
