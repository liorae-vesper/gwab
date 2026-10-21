//! Binary entry point of gwab.
//!
//! Parses the command line with [`clap::Parser`], hands it to
//! [`Cli::dispatch`], and reduces the outcome to the process exit code:
//! [`ExitCode::SUCCESS`] when the requested command ran, [`ExitCode::FAILURE`]
//! when it did not. Request commands accept `--quiet` to suppress the error
//! message and use the exit code alone as their result.

use clap::Parser;
use gwab::cli::Cli;
use gwab::dispatcher;
use std::process::ExitCode;

fn main() -> ExitCode {
    let the_cli: Cli = Cli::parse();
    match dispatcher::dispatch(the_cli.command) {
        Ok(..) => ExitCode::SUCCESS,
        Err(error) => {
            if !error.quiet {
                eprintln!("gwab: {}", error.message);
            }
            ExitCode::FAILURE
        }
    }
}
