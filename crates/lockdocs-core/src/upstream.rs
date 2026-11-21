//! Upstream docs: many packages ship no documentation (Next.js, Django,
//! FastAPI...). Their repositories do, under the exact version's git tag.
//! This module finds the repository from the package's own metadata, finds
//! the tag for the pinned version, and downloads only the docs folders
//! (Markdown, MDX, reStructuredText) from GitHub into the cache, once.
//! First-use queries fetch public docs at an immutable release commit. Explicit
//! `fetch` also supports major-version docs sites and optional GitHub credentials.

use serde::{Deserialize, Serialize};

mod archive;
mod cache_entry;
mod client;
mod docs_sites;
mod fetch;
mod git_refs;
mod github_files;
mod package_docs;
mod repository;

pub use cache_entry::{cached, dir, needs_refresh, stale};
pub use fetch::{fetch, fetch_automatic, fetch_private};
pub use git_refs::tag_candidates;
pub use repository::{parse_github, repo_of, repo_of_with};

const MAX_FILES: usize = 2500;
const MAX_BYTES: u64 = 40 * 1024 * 1024;
const MAX_FILE: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub struct Repo {
    pub owner: String,
    pub name: String,
    /// Package directory inside a monorepo (npm `repository.directory`).
    pub subdir: Option<String>,
    /// The git host when it is not github.com (lockdocs Pro private sources).
    pub host: Option<crate::private::Origin>,
}

/// Bump when what `fetch` downloads changes, so `lockdocs fetch` refreshes
/// older copies (a stale copy is still used until then).
pub const FORMAT: u32 = 7;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    /// `FORMAT` when downloaded (0 before it existed).
    #[serde(default)]
    pub format: u32,
    pub repo: String,
    pub tag: Option<String>,
    /// Immutable package-repository commit resolved from the release tag.
    #[serde(default)]
    pub commit: Option<String>,
    /// Explicit enrichment completed a docs-site lookup (including a genuine empty result).
    #[serde(default)]
    pub docs_sites_checked: bool,
    pub files: usize,
    pub bytes: u64,
    /// Why nothing was downloaded, when files == 0.
    pub note: Option<String>,
    /// A separate docs-site repository also included (see `DOCS_SITES`).
    #[serde(default)]
    pub site: Option<String>,
    /// Docs pages written as JS/TSX components, relative to the cache folder.
    #[serde(default)]
    pub pages: Vec<String>,
    /// How the package repository was read: `codeload` (one tarball) or `rest`
    /// (the per-file fallback, after a codeload failure or an over-size archive).
    #[serde(default)]
    pub via: Option<String>,
}

#[cfg(test)]
mod tests;
