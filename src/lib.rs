//! Library crate of the template.
//!
//! This code is intentionally trivial. It exists so that the tooling in this
//! repository — `rustfmt`, Clippy, Nextest, cargo-deny, cargo-machete, bacon and
//! the CI workflow — has something real to check. Replace it with your own.

/// The greeting the binary prints.
///
/// Kept public so the integration test in `tests/smoke.rs` exercises the crate
/// the same way a downstream user would.
///
/// # Examples
///
/// ```
/// assert_eq!(crwst::greeting(), "Hello from crwst!");
/// ```
#[must_use]
pub fn greeting() -> &'static str {
    "Hello from crwst!"
}

#[cfg(test)]
mod tests {
    use super::greeting;

    #[test]
    fn greeting_mentions_the_crate() {
        assert!(greeting().contains("crwst"));
    }
}
