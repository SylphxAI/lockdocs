//! Upgrade report (lockdocs Pro): the API difference between the pinned
//! version of a package and a target version, limited to the symbols the
//! project actually uses, with call sites and migration-guide pointers.
//!
//! This module only compares and scans; whether the caller may run it is
//! decided by the binary's licence gate (`crates/lockdocs/src/pro.rs`).

mod call_sites;
mod diff;
mod report;
#[cfg(test)]
mod tests;

pub use self::{call_sites::call_sites, diff::diff, report::report};

use crate::extract::Entry;
use serde_json::Value;

/// What happened to a symbol between the two versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Removed,
    /// Renamed to the `new` entry (matched by signature shape or doc similarity).
    Renamed,
    SignatureChanged,
    Deprecated,
}

impl Change {
    pub fn as_str(self) -> &'static str {
        match self {
            Change::Removed => "removed",
            Change::Renamed => "renamed",
            Change::SignatureChanged => "signature_changed",
            Change::Deprecated => "deprecated",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Diff {
    pub change: Change,
    pub old: Entry,
    pub new: Option<Entry>,
}

/// A place in the project that mentions a changed symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallSite {
    pub path: String,
    pub line: u32,
}

/// The report: the changes the project is exposed to, the rest as a count.
pub struct Report {
    pub text: String,
    pub json: Value,
}
