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

use std::env::args;
use crate::ir;
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
              println!("GET {}", args.url);
            },
            Command::Post(args) => {
              println!("POST {}", args.url);
            },
            Command::Put(args) => {
              println!("PUT {}", args.url);
            },
            Command::Patch(args) => {
              println!("PATCH {}", args.url);
            },
            Command::Delete(args) => {
              println!("DELETE {}", args.url);
            },
            Command::Set { key, value } => {
              println!("Set {}={}", key, value);
            },
            Command::Unset { key } => {
              println!("Unset {}", key);
            },
            Command::Vars => {
              println!("List of variables");
            },
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
/// [`RequestArgs::new`] is what parses them.
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
}

impl RequestArgs {
    /// Builds the request described by these arguments.
    ///
    /// # Arguments
    ///
    /// * `method` - HTTP method the request will be sent with. It is passed
    ///   separately because the option list itself does not name a method.
    /// * `args` - Arguments as parsed from the command line; the URL and body
    ///   are moved out of it unchanged.
    ///
    /// # Errors
    ///
    /// Returns an error when a `--add-header` or `--var` value is malformed,
    /// with a message that describes what was wrong. The rules themselves live
    /// in `Header::parse` and `VariableAssigment::parse`, next to the data
    /// they protect.
    pub fn new(method: ir::HttpMethod, args: RequestArgs) -> Result<ir::HttpRequest, String> {
        let headers = args
            .headers
            .iter()
            .map(|header| ir::Header::parse(header))
            .collect::<Result<Vec<_>, _>>()?;
        let vars = args
            .vars
            .iter()
            .map(|var| ir::VariableAssigment::parse(var))
            .collect::<Result<Vec<_>, _>>()?;
        let request = ir::HttpRequest {
            method,
            headers,
            vars,
            body: args.body,
            url: args.url,
        };
        Ok(request)
    }
}
