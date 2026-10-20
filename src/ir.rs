//! The request, as data.
//!
//! These are the types that a command line is parsed into and that the engine
//! will later execute. They are plain data with no I/O in them; parsing is
//! kept here, rather than in the CLI layer, so that the rules each type
//! guarantees (a header name that is a valid token, a header value that cannot
//! smuggle in a newline) sit next to the fields they protect.

/// A single HTTP header, with both parts already checked.
///
/// Nothing is interpreted beyond the checks in [`Header::parse`]: the value is
/// kept as written, because what a value means is a matter for the server.
#[derive(Debug)]
pub(super) struct Header {
    /// Header name. Guaranteed to be a valid token, so it can be written to a
    /// request line or a header line without escaping.
    pub(crate) name: String,
    /// Header value. Guaranteed to be free of CR and LF, so it cannot end the
    /// header early and inject another one.
    pub(crate) value: String,
}

impl Header {
    /// Parses a header written as `NAME: VALUE`.
    ///
    /// The split happens at the *first* colon, so a value may itself contain
    /// colons (`Host: example.com:8443` is two fields, not three); leading and
    /// trailing whitespace around the name and the value is trimmed.
    ///
    /// # Errors
    ///
    /// Returns an error when
    ///
    /// * the input contains no colon at all;
    /// * the name is empty;
    /// * the name contains a byte outside the token characters HTTP allows
    ///   (letters, digits and ``!#$%&'*+-.^_`|~``);
    /// * the value contains CR or LF, which would let the caller end the
    ///   header and append one of their own.
    pub fn parse(input: &str) -> Result<Header, String> {
        let (name, value) = input
            .split_once(':')
            .ok_or_else(|| format!("invalid header {input:?}: expected NAME: VALUE"))?;

        let name = name.trim();
        let value = value.trim();

        if name.is_empty() {
            return Err("header name cannot be empty".into());
        }

        if !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
        {
            return Err(format!("invalid header name: {name:?}"));
        }

        if value.bytes().any(|b| b == b'\r' || b == b'\n') {
            return Err("header value cannot contain CR or LF".into());
        }

        Ok(Self {
            name: name.to_owned(),
            value: value.to_owned(),
        })
    }
}

/// A `NAME=VALUE` assignment collected from the command line.
///
/// The value is deliberately unrestricted: it is data the user chose, and the
/// only structural rule is the split point.
#[derive(Debug)]
pub(super) struct VariableAssigment {
    /// Variable name. Guaranteed to be non-empty, so it can always be looked
    /// up again.
    pub(crate) name: String,
    /// Value assigned to the name. May be empty, because `NAME=` is a valid
    /// way to clear a value.
    pub(crate) value: String,
}

impl VariableAssigment {
    /// Parses an assignment written as `NAME=VALUE`.
    ///
    /// The split happens at the *first* `=`, so the value may contain further
    /// ones (`url=http://host/?a=b` assigns everything after the first `=`).
    ///
    /// # Errors
    ///
    /// Returns an error when the input contains no `=`, or when the name
    /// before it is empty.
    pub fn parse(input: &str) -> Result<VariableAssigment, String> {
        let (name, value) = input
            .split_once('=')
            .ok_or_else(|| format!("invalid variable {input:?}: expected NAME=VALUE"))?;

        if name.is_empty() {
            return Err("variable name cannot be empty".into());
        }

        Ok(Self {
            name: name.to_owned(),
            value: value.to_owned(),
        })
    }
}

/// The HTTP method a request is sent with.
///
/// The set is closed and has one variant per request subcommand, so a method
/// the client cannot actually send is not representable.
#[derive(Debug)]
pub(super) enum HttpMethod {
    /// `GET`: retrieve a representation of the target resource.
    Get,
    /// `POST`: submit data to the target resource.
    Post,
    /// `PUT`: replace the state of the target resource.
    Put,
    /// `PATCH`: apply a partial modification to the target resource.
    Patch,
    /// `DELETE`: remove the target resource.
    Delete,
}

/// A complete request, ready to be executed.
///
/// This is the seam between parsing and the network: the CLI fills it in and
/// the engine consumes it, so neither has to know how the other works.
#[derive(Debug)]
pub(super) struct HttpRequest {
    /// Method to send the request with.
    pub(crate) method: HttpMethod,
    /// URL to send the request to, exactly as given on the command line. It is
    /// not parsed or checked yet; doing so belongs to the engine, where it can
    /// be reported next to the failing request.
    pub(crate) url: String,
    /// Headers to send, in the order they were given, each one already
    /// validated.
    pub(crate) headers: Vec<Header>,
    /// Body to send, or `None` for a request without one. Note that it is not
    /// checked against the method: a body on `GET` is unusual, but the user
    /// asked for it.
    pub(crate) body: Option<String>,
    /// Variable assignments given on the command line, in the order they were
    /// given.
    pub(crate) vars: Vec<VariableAssigment>,
    /// Whether to print request diagnostics and timing metrics.
    pub(crate) verbose: bool,
    /// Named configuration environment selected for this request.
    pub(crate) environment: Option<String>,
}
