//! Binary entry point of gwab.
//!
//! Parses the command line with [`clap::Parser`], hands it to
//! [`Cli::dispatch`], and reduces the outcome to the process exit code:
//! [`ExitCode::SUCCESS`] when the requested command ran, [`ExitCode::FAILURE`]
//! when it did not.
//!
//! The message an error carries is currently discarded, so a failing command
//! only signals itself through the exit code and explains itself to nobody.

use clap::Parser;
use gwab::cli::Cli;
use std::process::ExitCode;

fn main() -> ExitCode {
    let the_cli: Cli = Cli::parse();
    match the_cli.dispatch() {
        Ok(..) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("gwab: {error}");
            ExitCode::FAILURE
        }
    }
}
