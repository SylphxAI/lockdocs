//! Select and fetch docs from separate sites that match the pinned major.

mod catalog;
mod fetch;

pub(super) use catalog::{latest_major_response, DocsSite, DOCS_SITES};
pub(super) use fetch::{later_major_file, page_file, site_docs, site_kind, site_step, SiteDocs};
