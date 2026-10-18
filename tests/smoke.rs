//! Integration test: exercises the crate through its public API.
//!
//! Run with `mise run test`, which drives `cargo nextest`.

#[test]
fn greeting_is_not_empty() {
    // `assert_ne!` rather than `assert!(!x.is_empty())`: clippy's
    // `assert_is_empty` lint (new in Rust 1.99) prefers the comparison, and it
    // prints the actual value when the assertion fails.
    assert_ne!(crwst::greeting(), "");
}
