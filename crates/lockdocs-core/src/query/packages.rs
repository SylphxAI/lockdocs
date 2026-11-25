//! Dependency selection, exact-version sources, and index preparation.

mod indexing;
mod selection;
mod sources;

use super::Engine;
use crate::index::PackageIndex;
use crate::locate::Source;
use crate::{fetch, upstream, Dep};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;

/// An indexed package, the dependency it answers for, and a drift note.
pub(super) type ReadyPkg = (Arc<PackageIndex>, Dep, Option<String>);

/// A resolved package ready to search.
pub(super) enum Resolved {
    Ready(Arc<PackageIndex>, Dep, Option<String>),
    Missing(Dep, String),
}

/// Keep the original flat fields for CLI consumers; richer provenance is additive.
pub(super) fn fetch_report(dep: &Dep, status: &upstream::Manifest) -> Value {
    json!({"package": dep.id(), "repo": status.repo, "tag": status.tag,
        "files": status.files, "note": status.note, "upstream": status})
}

impl Engine {
    /// Upstream docs for this exact version: a cached copy, or (when fetching
    /// is enabled) a one-time download.
    fn upstream(&self, dep: &Dep, src: &Source) -> (Option<(PathBuf, upstream::Manifest)>, Option<String>) {
        if src.version != dep.version || std::env::var("LOCKDOCS_NO_UPSTREAM").is_ok() {
            return (None, None);
        }
        let enabled = self.opts.fetch || self.opts.upstream;
        if fetch::is_git(dep) {
            return (
                None,
                enabled.then(|| "upstream docs skipped: git dependency's resolved commit is not a release tag; using its checkout files".into()),
            );
        }
        let cached = upstream::cached(dep);
        if let Some(c) = cached.as_ref().filter(|(_, m)| !enabled || !upstream::needs_refresh(m, self.opts.fetch)) {
            let note = if upstream::needs_refresh(&c.1, false) {
                Some(format!(
                    "offline fallback: upstream cache format {} has not been exact-tag revalidated{}{}",
                    c.1.format,
                    if c.1.format < 4 {
                        "; its old commit candidate may have been a branch"
                    } else {
                        ""
                    },
                    c.1.note.as_ref().map(|n| format!("; {n}")).unwrap_or_default()
                ))
            } else {
                c.1.note.clone()
            };
            return (Some(c.clone()), note);
        }
        if enabled {
            let result = if self.opts.fetch {
                upstream::fetch_private(dep, src, self.opts.private)
            } else {
                upstream::fetch_automatic(dep, src)
            };
            return match result {
                Ok(m) => (Some((upstream::dir(dep), m.clone())), m.note.clone()),
                Err(e) => (None, Some(format!("upstream docs fetch failed: {e:#}; using package files"))),
            };
        }
        (None, None)
    }
}
