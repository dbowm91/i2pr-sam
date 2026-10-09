//! Synthetic public-surface fixture for the API snapshot guard self-test.
//!
//! This file is the shared baseline. Every mutation in this directory is a copy
//! of it with exactly one public-surface change, so "unchanged" really is the
//! control case for all of them. Nothing here is compiled.

use std::collections::BTreeMap;

/// Outcome of one synthetic parse attempt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseOutcome {
    Empty,
    Fields(BTreeMap<String, String>),
    Failed { reason: String },
}

/// One decoded key/value pair.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SamLine {
    pub key: String,
    pub value: String,
}

/// Options accepted when opening a session.
#[derive(Clone, Debug)]
pub struct SessionOptions {
    pub id: String,
    pub timeout_ms: u32,
}

pub use crate::ParseOutcome as Outcome;

pub fn parse_line(text: &str) -> Result<SamLine, ParseOutcome> {
    let _ = text;
    Err(ParseOutcome::Empty)
}

pub fn session_options(
    id: &str,
    timeout_ms: u32,
) -> SessionOptions {
    SessionOptions { id: id.to_owned(), timeout_ms }
}