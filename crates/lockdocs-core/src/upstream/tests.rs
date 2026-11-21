use std::path::{Path, PathBuf};

use crate::locate::Source;
use crate::{Dep, Eco};

use super::archive::{codeload, read_tar};
use super::cache_entry::enrich_cached;
use super::client::{agent, basic, MAX_ARCHIVE, MAX_ARCHIVE_AUTOMATIC};
use super::docs_sites::{later_major_file, latest_major_response, page_file, site_docs, site_kind, DOCS_SITES};
use super::fetch::build;
use super::git_refs::{list_refs, ls_refs_request, parse_ls_refs, pick_release, GitRef};
use super::github_files::write_file;
use super::package_docs::{doc_file, extract_pkg_docs};
use super::{fetch, fetch_automatic, needs_refresh, parse_github, tag_candidates, Repo, MAX_FILE};

mod fixtures;
use self::fixtures::*;

// Include the scenario files in this module so existing `upstream::tests::`
// filters (including exact names) still select the same tests.
include!("tests/archives.rs");
include!("tests/cache_entry.rs");
include!("tests/docs_sites.rs");
include!("tests/git_transport.rs");
include!("tests/release_fetch.rs");
include!("tests/repository.rs");
