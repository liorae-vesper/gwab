//! Command line surface of `gwab`.
//!
//! This module owns the command-line schema and parsing. Parsed commands are
//! handed to [`crate::dispatcher`] for execution.
//!
//! Most doc comments here do double duty. `clap` derives the `--help` output
//! from them, so a variant's first line is what "gwab --help" prints for it and
//! a field's first line is that option's help text; the paragraphs after a
//! blank line become the longer help shown by `--help`.

use clap::{Args, Parser, Subcommand, ValueEnum};

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

    /// Replay a saved request from `gwab-requests.toml` in the current directory.
    Run {
        /// Method under which the request was saved; omit with NAME to browse.
        method: Option<SavedMethod>,
        /// Name passed to `--save`; omit with METHOD to browse.
        name: Option<String>,
    },

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
#[derive(Debug, Args)]
pub struct RequestArgs {
    /// The URL to send the request to.
    pub(crate) url: String,
    /// Add a header, written as `NAME: VALUE`. May be repeated.
    #[arg(long = "add-header", value_name = "NAME: VALUE")]
    pub(crate) headers: Vec<String>,
    /// Send this body with the request.
    #[arg(long, short = 'd')]
    pub(crate) body: Option<String>,
    /// Assign a variable, written as `NAME=VALUE`. May be repeated.
    #[arg(long = "var", value_name = "NAME=VALUE")]
    pub(crate) vars: Vec<String>,
    /// Load variables from a named environment in the configuration files.
    #[arg(long = "env", value_name = "NAME")]
    pub(crate) environment: Option<String>,
    /// Print headers, bodies, DNS time, and latency; may expose credentials.
    #[arg(short, long)]
    pub(crate) verbose: bool,
    /// Suppress all output and use the exit code to report request success.
    #[arg(short, long)]
    pub(crate) quiet: bool,
    /// Select response output: body for piping, all for a status line, headers, and body.
    #[arg(long, value_enum, value_name = "MODE")]
    pub(crate) output: Option<OutputMode>,
    /// Save this request under a name in `gwab-requests.toml` in the current directory.
    #[arg(short = 's', long, value_name = "NAME")]
    pub(crate) save: Option<String>,
    /// Describe this saved request in the picker (requires `--save`).
    #[arg(long, value_name = "TEXT")]
    pub(crate) description: Option<String>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum OutputMode {
    Body,
    All,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
/// HTTP method under which a named request is saved.
pub enum SavedMethod {
    /// GET request.
    Get,
    /// POST request.
    Post,
    /// PUT request.
    Put,
    /// PATCH request.
    Patch,
    /// DELETE request.
    Delete,
}

impl SavedMethod {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Get => "get",
            Self::Post => "post",
            Self::Put => "put",
            Self::Patch => "patch",
            Self::Delete => "delete",
        }
    }

    pub(crate) fn from_name(name: &str) -> Option<Self> {
        match name {
            "get" => Some(Self::Get),
            "post" => Some(Self::Post),
            "put" => Some(Self::Put),
            "patch" => Some(Self::Patch),
            "delete" => Some(Self::Delete),
            _ => None,
        }
    }
}

impl From<SavedMethod> for crate::ir::HttpMethod {
    fn from(method: SavedMethod) -> Self {
        match method {
            SavedMethod::Get => Self::Get,
            SavedMethod::Post => Self::Post,
            SavedMethod::Put => Self::Put,
            SavedMethod::Patch => Self::Patch,
            SavedMethod::Delete => Self::Delete,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Cli, Command};
    use clap::Parser;

    #[test]
    fn request_accepts_verbose_flag() {
        let cli = Cli::try_parse_from(["gwab", "get", "https://example.com", "--verbose"])
            .expect("verbose request arguments should parse");

        assert!(matches!(&cli.command, Command::Get(_)));
        let Command::Get(args) = cli.command else {
            return;
        };
        assert!(args.verbose);
    }

    #[test]
    fn request_accepts_environment_flag() {
        let cli = Cli::try_parse_from(["gwab", "get", "/posts", "--env", "staging"])
            .expect("environment request arguments should parse");

        assert!(matches!(&cli.command, Command::Get(_)));
        let Command::Get(args) = cli.command else {
            return;
        };
        assert_eq!(args.environment.as_deref(), Some("staging"));
    }

    #[test]
    fn quiet_flag_is_preserved_in_the_request() {
        let cli = Cli::try_parse_from(["gwab", "get", "/posts", "--quiet"])
            .expect("quiet request arguments should parse");

        let Command::Get(args) = cli.command else {
            return;
        };
        assert!(args.quiet);
    }

    #[test]
    fn save_short_flag_is_preserved_as_a_request_name() {
        let cli = Cli::try_parse_from(["gwab", "get", "/users", "-s", "users"])
            .expect("short save flag should parse");
        let Command::Get(args) = cli.command else {
            return;
        };

        assert_eq!(args.save.as_deref(), Some("users"));
    }

    #[test]
    fn run_command_accepts_no_key_to_open_the_picker() {
        let cli = Cli::try_parse_from(["gwab", "run"])
            .expect("run without a key should open the saved request picker");

        assert!(matches!(
            cli.command,
            Command::Run {
                method: None,
                name: None
            }
        ));
    }
}
