//! Command line surface of `gwab`.
//!
//! This module owns argument parsing and nothing else: it turns the command
//! line into the request description defined by the `ir` module, so the rest
//! of the crate never has to look at raw arguments.
//!
//! Most doc comments here do double duty. `clap` derives the `--help` output
//! from them, so a variant's first line is what "gwab --help" prints for it and
//! a field's first line is that option's help text; the paragraphs after a
//! blank line become the longer help shown by `--help`.

use crate::ir::HttpMethod;
use crate::{engine, ir, state};
use clap::{Args, Parser, Subcommand};

/// The `gwab` command line.
///
/// A command line is one subcommand; the flags that describe the request are
/// shared by every subcommand that sends one.
#[derive(Debug, Parser)]
#[command(name = "gwab", about = "A modern HTTP Client", version)]
pub struct Cli {
    /// The subcommand to run, together with its arguments.
    #[command(subcommand)]
    pub command: Command,
}

impl Cli {
    /// Runs the command the user asked for.
    ///
    /// # Errors
    ///
    /// Returns the message of whichever step failed, written to be shown to
    /// the user unchanged. Deciding what to do with it is the caller's job:
    /// `main` currently only turns it into a non-zero exit code.
    pub fn dispatch(self) -> Result<(), String> {
        match self.command {
            Command::Get(args) => {
                let request = args.to_request(HttpMethod::Get);
                engine::run(request?)?;
            }
            Command::Post(args) => {
                let request = args.to_request(HttpMethod::Post);
                engine::run(request?)?;
            }
            Command::Put(args) => {
                let request = args.to_request(HttpMethod::Put);
                engine::run(request?)?;
            }
            Command::Patch(args) => {
                let request = args.to_request(HttpMethod::Patch);
                engine::run(request?)?;
            }
            Command::Delete(args) => {
                let request = args.to_request(HttpMethod::Delete);
                engine::run(request?)?;
            }
            Command::Set { key, value } => state::RuntimeState::set(key, value)?,
            Command::Unset { key } => state::RuntimeState::unset(key)?,
            Command::Vars => state::RuntimeState::vars()?,
        };
        Ok(())
    }
}

/// The command to run.
///
/// Every request variant wraps the same [`RequestArgs`], so `gwab get ...` and
/// `gwab post ...` accept exactly the same options; the variant only chooses
/// the HTTP method.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Send a GET request.
    Get(RequestArgs),
    /// Send a POST request.
    Post(RequestArgs),
    /// Send a PUT request.
    Put(RequestArgs),
    /// Send a PATCH request.
    Patch(RequestArgs),
    /// Send a DELETE request.
    Delete(RequestArgs),

    /// Remember a variable value under a name.
    Set {
        /// Name to store the value under.
        key: String,
        /// Value to store.
        value: String,
    },

    /// Forget a variable value that was stored earlier.
    Unset {
        /// Name of the variable to remove.
        key: String,
    },

    /// List the variable values that are stored.
    Vars,
}

/// The options shared by every subcommand that sends a request.
///
/// Every flag that may be given more than once is collected in a `Vec`, so the
/// order the user wrote them in is preserved. The URL and body are kept as
/// typed; the header and variable strings are still raw at this point.
/// [`RequestArgs::to_request`] is what parses them.
#[derive(Debug, Args)]
pub struct RequestArgs {
    /// The URL to send the request to.
    url: String,
    /// Add a header, written as `NAME: VALUE`. May be repeated.
    #[arg(long = "add-header", value_name = "NAME: VALUE")]
    headers: Vec<String>,
    /// Send this body with the request.
    #[arg(long, short = 'd')]
    body: Option<String>,
    /// Assign a variable, written as `NAME=VALUE`. May be repeated.
    #[arg(long = "var", value_name = "NAME=VALUE")]
    vars: Vec<String>,
    /// Load variables from a named environment in the configuration files.
    #[arg(long = "env", value_name = "NAME")]
    environment: Option<String>,
    /// Print headers, bodies, DNS time, and latency; may expose credentials.
    #[arg(short, long)]
    verbose: bool,
}

impl RequestArgs {
    /// Builds the request described by these arguments.
    ///
    /// # Arguments
    ///
    /// * `method` - HTTP method the request will be sent with. It is passed
    ///   separately because the option list itself does not name a method.
    /// The URL and body are moved out unchanged.
    ///
    /// # Errors
    ///
    /// Returns an error when a `--add-header` or `--var` value is malformed,
    /// with a message that describes what was wrong. The rules themselves live
    /// in `Header::parse` and `VariableAssigment::parse`, next to the data
    /// they protect.
    fn to_request(self, method: ir::HttpMethod) -> Result<ir::HttpRequest, String> {
        let headers = self
            .headers
            .iter()
            .map(|header| ir::Header::parse(header))
            .collect::<Result<Vec<_>, _>>()?;
        let vars = self
            .vars
            .iter()
            .map(|var| ir::VariableAssigment::parse(var))
            .collect::<Result<Vec<_>, _>>()?;
        let request = ir::HttpRequest {
            method,
            headers,
            vars,
            body: self.body,
            url: self.url,
            verbose: self.verbose,
            environment: self.environment,
        };
        Ok(request)
    }
}

#[cfg(test)]
mod tests {
    use super::{Cli, Command};
    use crate::ir::HttpMethod;
    use clap::Parser;

    #[test]
    fn request_accepts_verbose_flag() {
        let cli = Cli::try_parse_from(["gwab", "get", "https://example.com", "--verbose"])
            .expect("verbose request arguments should parse");

        if let Command::Get(args) = cli.command {
            let request = args
                .to_request(HttpMethod::Get)
                .expect("valid request arguments should form a request");
            assert!(request.verbose);
        } else {
            assert!(false, "expected a GET request");
        }
    }

    #[test]
    fn request_accepts_environment_flag() {
        let cli = Cli::try_parse_from(["gwab", "get", "/posts", "--env", "staging"])
            .expect("environment request arguments should parse");

        if let Command::Get(args) = cli.command {
            let request = args
                .to_request(HttpMethod::Get)
                .expect("valid request arguments should form a request");
            assert_eq!(request.environment.as_deref(), Some("staging"));
        } else {
            assert!(false, "expected a GET request");
        }
    }
}
