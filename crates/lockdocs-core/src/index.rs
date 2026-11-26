//! Per-package index: entries plus their term frequencies, cached on disk
//! by package, exact version and source location.

use crate::embed::Vec8;
use crate::extract::{Entry, Kind};
use crate::upstream::Manifest;
use crate::Eco;
use serde::{Deserialize, Serialize};

mod build;
mod cache;
mod terms;

pub use build::build;
pub use cache::load_or_build;
pub(crate) use cache::source_stamp;
pub use terms::{embed_text, entry_tf, head_tf, summary};

/// Bump when extraction or the on-disk format changes.
pub const FORMAT: u32 = 19;

#[derive(Serialize, Deserialize)]
pub struct PackageIndex {
    pub format: u32,
    pub eco: Eco,
    pub name: String,
    /// The version whose files were indexed.
    pub version: String,
    pub source: String,
    pub fetched: bool,
    pub entries: Vec<Entry>,
    pub npm_api: Option<crate::npm_api::PublicApi>,
    pub terms: Vec<Vec<(String, u16)>>,
    pub lens: Vec<u32>,
    /// Terms of the heading or name plus the first sentence only, ranked on
    /// their own so a long body does not bury what the entry says it is.
    pub head: Vec<Vec<(String, u16)>>,
    pub head_lens: Vec<u32>,
    pub build_ms: u64,
    /// `github.com/o/r@tag (N files)` when upstream docs are included.
    pub upstream: Option<String>,
    /// Fetch manifest used by this index, including immutable commit and failure notes.
    pub upstream_provenance: Option<Manifest>,
    /// Embedding model id, or empty for keyword-only.
    pub embed: String,
    /// One embedding per entry (empty without a model).
    pub vecs: Vec<Vec8>,
}

impl PackageIndex {
    pub fn api_entries(&self) -> &[Entry] {
        self.npm_api.as_ref().map_or(self.entries.as_slice(), |api| api.entries.as_slice())
    }

    pub fn docs_indices(&self, import: Option<&str>) -> Vec<usize> {
        let import = import.unwrap_or(&self.name);
        let files = self.npm_api.as_ref().map(|api| api.search_files(import));
        let compatible = self.npm_api.as_ref().map(|api| api.compatible_imports(import, &self.name));
        let edition_routes: std::collections::BTreeMap<_, _> = self
            .npm_api
            .iter()
            .flat_map(|api| &api.roots)
            .filter_map(|root| {
                let segment = root.import.strip_prefix(&self.name)?.strip_prefix('/')?;
                let digits = segment.strip_prefix('v')?;
                (!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())).then_some((segment, root.import.as_str()))
            })
            .collect();
        let mut seen = std::collections::HashSet::new();
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                if e.kind == Kind::Prose {
                    compatible.as_ref().is_none_or(|imports| {
                        // A versioned website route makes an edition claim only
                        // when it names an actual public import of this package.
                        let route = e.file.strip_prefix("upstream:").and_then(|path| {
                            path.split('/').rev().find_map(|part| {
                                let stem = part.strip_suffix(".mdx").or_else(|| part.strip_suffix(".md")).unwrap_or(part);
                                edition_routes.get(stem)
                            })
                        });
                        route.is_none_or(|route| imports.contains(route))
                            && e.prose_imports
                                .iter()
                                .filter(|s| crate::markdown::import_base(s) == self.name)
                                .all(|s| imports.contains(s.as_str()))
                    })
                } else {
                    files.as_ref().is_none_or(|files| files.contains(&e.file))
                }
            })
            // Deduplicate only after import selection: an identical declaration
            // in another edition must not erase the requested file identity.
            .filter(|(_, e)| seen.insert((e.path.as_str(), e.sig.as_str(), e.doc.as_str(), &e.guide_modes, &e.prose_imports)))
            .map(|(i, _)| i)
            .collect()
    }

    pub fn id(&self) -> String {
        format!("{}@{}", self.name, self.version)
    }
}
