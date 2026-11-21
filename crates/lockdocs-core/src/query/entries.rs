//! Shared symbol parsing, entry classification, and alias resolution.

use crate::extract::{Entry, Kind};
use crate::index::PackageIndex;

pub(super) fn npm_import_base(name: &str) -> &str {
    let skip = if name.starts_with('@') {
        name.find('/').map_or(name.len(), |i| i + 1)
    } else {
        0
    };
    name[skip..].find('/').map_or(name, |i| &name[..skip + i])
}

pub(super) fn split_symbol(s: &str) -> Vec<String> {
    s.replace("::", ".")
        .replace('#', ".")
        .split('.')
        .map(|x| x.trim().trim_end_matches("()").to_string())
        .filter(|x| !x.is_empty())
        .collect()
}

pub(super) fn private_path(e: &Entry) -> bool {
    e.file.split('/').any(|s| s.starts_with('_') && !s.starts_with("__init__")) || e.path.contains("._") || e.file.contains("/internal/")
}

/// Tooling shipped inside a package (type-checker plugins, test helpers).
pub(super) fn tooling_path(e: &Entry) -> bool {
    e.file.split('/').any(|s| {
        let stem = s.split('.').next().unwrap_or(s);
        matches!(
            stem,
            "mypy" | "pylint" | "plugin" | "plugins" | "testing" | "conftest" | "scripts" | "codemod" | "codemods" | "eslint" | "babel"
        )
    })
}

pub(super) fn resolve_alias<'a>(idx: &'a PackageIndex, e: &'a Entry) -> &'a Entry {
    let Some(target) = &e.alias_of else { return e };
    idx.entries
        .iter()
        .filter(|x| &x.name == target && x.kind != Kind::Alias && x.kind != Kind::Prose)
        .max_by_key(|x| (x.file == e.file, !x.doc.is_empty()))
        .unwrap_or(e)
}
