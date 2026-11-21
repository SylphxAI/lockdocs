//! Collect declarations and compute cache identity for the npm import graph.

use super::plan::{plan, How, MAX_FILE_BYTES};
use super::typescript::ts_file;
use super::Entry;
use crate::locate::Source;
use crate::Eco;
use std::path::Path;

/// Declaration graph inputs and cache identity use the same file selection as extraction.
pub(crate) fn npm_sources(src: &Source, extra_types: Option<&Path>) -> std::collections::BTreeMap<String, String> {
    plan(Eco::Npm, src, extra_types, None, true)
        .into_iter()
        .filter(|j| j.how == How::Ts)
        .filter_map(|j| {
            if std::fs::metadata(&j.abs).ok()?.len() > MAX_FILE_BYTES {
                return None;
            }
            Some((j.rel, std::fs::read_to_string(j.abs).ok()?))
        })
        .collect()
}

/// Keep declaration identities until the requested import graph is selected.
/// Mirrored declarations are deduplicated within that search scope.
pub(crate) fn npm_entries(sources: &std::collections::BTreeMap<String, String>) -> Vec<Entry> {
    let mut entries = Vec::new();
    for (file, text) in sources {
        ts_file(text, file, &mut entries);
    }
    entries
}

/// Content identity, including names, catches edits, additions and removals even
/// when a package's version and metadata timestamps did not change.
pub(crate) fn npm_stamp(src: &Source, extra_types: Option<&Path>) -> String {
    let mut parts = Vec::new();
    for j in plan(Eco::Npm, src, extra_types, None, true) {
        parts.push(j.rel);
        if let Ok(bytes) = std::fs::read(&j.abs) {
            parts.push(crate::cache::hash(&[&String::from_utf8_lossy(&bytes)]));
        }
    }
    crate::cache::hash(&parts.iter().map(String::as_str).collect::<Vec<_>>())
}
