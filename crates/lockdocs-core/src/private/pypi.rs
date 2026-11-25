//! Python private indexes, from uv (`uv.toml`, `[[tool.uv.index]]`,
//! `UV_INDEX_*`), pip (`PIP_INDEX_URL`, `PIP_EXTRA_INDEX_URL`, `pip.conf`),
//! credentials in the URL, `UV_INDEX_<NAME>_USERNAME/_PASSWORD` or `~/.netrc`,
//! and the index `uv.lock` recorded per package.
//!
//! Planning honors the registry recorded in `uv.lock` before consulting
//! configured sources. It produces candidates and credentials without
//! contacting an index.
//!
//! The Simple API client reads HTML or JSON listings, selects the exact
//! version, and uses its advertised filename to choose the archive format.
//! The same unpacking function also serves public PyPI downloads.

mod config;
mod simple;

#[cfg(test)]
mod tests;

pub use config::plan;
pub use simple::{download, parse_simple, pick, unpack};
