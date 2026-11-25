//! Entry score multipliers for query matches, documentation quality, and recency.

use super::is_changelog;
use crate::bm25;
use crate::extract::{Entry, Kind};
use crate::query::entries::{private_path, tooling_path};
use crate::query::ranking::{deprecated_page, heading_terms, old_upgrade_guide};

/// Entries whose name (or section heading) carries a query word are about it.
pub(in crate::query::ranking) fn name_hit(e: &Entry, qterms: &[String]) -> f32 {
    let words = if e.kind == Kind::Prose {
        let last = e.name.rsplit(" › ").next().unwrap_or("");
        if crate::markdown::generic_heading(last) {
            Vec::new()
        } else {
            heading_terms(last)
        }
    } else {
        bm25::terms(&e.name)
    };
    let n = qterms.iter().filter(|q| words.contains(q)).count();
    match n {
        0 => 1.0,
        1 => 1.5,
        _ => 1.9,
    }
}

pub(in crate::query::ranking) fn boost(e: &Entry, idents: &[String], changes: bool, major: u64) -> f32 {
    let mut b = 1.0;
    // Curated guides from the project's own docs folder answer "how do I" better
    // than internal symbols do.
    if e.kind == Kind::Prose && e.file.starts_with("upstream:") {
        b *= std::env::var("LOCKDOCS_UPSTREAM_BOOST").ok().and_then(|v| v.parse().ok()).unwrap_or(1.35);
    }
    if e.kind == Kind::Prose {
        // A section headed by the API the question names (`after`, `select!`).
        let last = e
            .name
            .rsplit(" › ")
            .next()
            .unwrap_or("")
            .trim()
            .trim_end_matches("()")
            .trim_end_matches('!')
            .to_ascii_lowercase();
        if !last.is_empty() && idents.contains(&last) {
            b *= 1.4;
        }
        if !changes && old_upgrade_guide(e, major) {
            b *= 0.5;
        }
        if deprecated_page(e) {
            b *= 0.6;
        }
        if is_changelog(e) {
            b *= if changes { 1.5 } else { 0.5 };
        } else if e.file.to_ascii_uppercase().starts_with("README") {
            b *= 1.15;
        }
    } else {
        let n = e.name.trim_end_matches('!').to_ascii_lowercase();
        let p = e.path.to_ascii_lowercase();
        if idents.contains(&n) {
            b *= 1.8;
        }
        if idents
            .iter()
            .any(|i| i.len() > n.len() && (p.ends_with(i.as_str()) || i.ends_with(&format!(".{n}"))))
        {
            b *= 1.4;
        }
        if e.doc.is_empty() {
            b *= 0.6;
        }
        if e.kind == Kind::Alias {
            b *= 0.6;
        }
    }
    if private_path(e) {
        b *= 0.6;
    }
    if e.legacy {
        b *= 0.35;
    }
    if tooling_path(e) {
        b *= 0.5;
    }
    b
}
