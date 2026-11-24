//! Run extraction jobs and reconcile entries for the package index.

use super::docs::{example, page_markdown, page_title, prose, prose_titled, Includes};
use super::go::go_file;
use super::plan::{plan, How, MAX_FILE_BYTES};
use super::python::py_file;
use super::rust::{rs_file, rust_crate_name};
use super::typescript::ts_file;
use super::Entry;
use crate::locate::Source;
use crate::markdown;
use crate::Eco;
use rayon::prelude::*;
use std::collections::HashSet;
use std::path::Path;

/// Extract every entry for a package.
pub fn extract(eco: Eco, pkg_name: &str, src: &Source, extra_types: Option<&Path>, upstream: Option<&Path>) -> Vec<Entry> {
    let major: u64 = src.version.trim_start_matches('v').split('.').next().and_then(|m| m.parse().ok()).unwrap_or(0);
    let jobs = plan(eco, src, extra_types, upstream, false);
    let crate_name = if eco == Eco::Cargo {
        rust_crate_name(&src.dir).unwrap_or_else(|| pkg_name.replace('-', "_"))
    } else {
        String::new()
    };
    let includes = upstream.map(Includes::scan).unwrap_or_default();
    let mut all: Vec<Entry> = jobs
        .par_iter()
        .flat_map_iter(|job| {
            let Ok(meta) = std::fs::metadata(&job.abs) else { return Vec::new() };
            if meta.len() > MAX_FILE_BYTES && job.how != How::Prose {
                return Vec::new();
            }
            let Ok(bytes) = std::fs::read(&job.abs) else { return Vec::new() };
            let text = String::from_utf8_lossy(&bytes);
            let mut out = Vec::new();
            let package = (eco == Eco::Npm && job.rel.starts_with("upstream:")).then_some(pkg_name);
            match job.how {
                How::Prose => {
                    prose_titled(&text, &job.rel, 0, None, package, &mut out);
                    if job.rel.starts_with("upstream:") && !includes.files.is_empty() {
                        for e in &mut out {
                            e.doc = includes.expand(&e.doc);
                        }
                    }
                }
                How::Page => {
                    if let Some((md, title)) = page_markdown(&text) {
                        let title = title.unwrap_or_else(|| page_title(&job.rel));
                        prose_titled(&md, &job.rel, 0, Some(title), package, &mut out);
                    }
                }
                How::Example => example(&text, &job.rel, &mut out),
                How::Metadata => {
                    let (body, line) = markdown::metadata_body(&text);
                    prose(&body, &job.rel, line - 1, &mut out)
                }
                How::Ts => ts_file(&text, &job.rel, &mut out),
                How::Python => py_file(&text, &job.rel, &mut out),
                How::Rust => rs_file(&text, &job.rel, &crate_name, &mut out),
                How::Go => go_file(&text, &job.rel, pkg_name, &mut out),
            }
            out
        })
        .collect();
    for e in &mut all {
        e.legacy = legacy_file(&e.file, major);
    }
    // Drop exact duplicates (the same declaration shipped twice).
    let mut seen = HashSet::new();
    all.retain(|e| e.kind == super::Kind::Prose || seen.insert((e.path.clone(), e.sig.clone(), e.doc.len())));
    all
}

/// A path segment `vN` with N below the package's major version.
pub(super) fn legacy_file(file: &str, major: u64) -> bool {
    major > 1
        && file.split('/').any(|seg| {
            seg.len() >= 2 && seg.starts_with('v') && seg[1..].chars().all(|c| c.is_ascii_digit()) && seg[1..].parse::<u64>().is_ok_and(|n| n < major)
        })
}
