//! `gwab` is a modern HTTP client for the command line.
//!
//! A command line is parsed into a request, and from then on the request is
//! what the crate works with. [`cli`] is the only module that looks at `argv`:
//! it produces the types in `ir`, and the later stages of the pipeline consume
//! them.
//!
//! # Error handling
//!
//! `unsafe` code is forbidden and `unwrap`, `expect` and `panic!` are denied,
//! so a failure is reported as a value instead of by aborting the process.
//! Fallible functions return `Result<_, String>`, where the message is meant
//! to be shown to the user as it is.

pub mod cli;
mod config;
mod cookies;
pub mod dispatcher;
mod engine;
mod ir;
mod preparation;
mod render;
mod saved_requests;
mod state;
mod trace;
