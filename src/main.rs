//! Binary entry point of the template.
//!
//! Deliberately trivial: this repository is a development-tooling template, not
//! an application skeleton.

use std::process::ExitCode;

fn main() -> ExitCode {
    println!("{}", crwst::greeting());
    ExitCode::SUCCESS
}
