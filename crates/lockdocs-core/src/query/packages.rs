//! Dependency selection, exact-version sources, and index preparation.

mod indexing;
mod selection;
mod sources;

use crate::index::PackageIndex;
use crate::{upstream, Dep};
use serde_json::{json, Value};
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
