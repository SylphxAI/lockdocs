//! Query and entry signals used to rank documentation and API declarations.

mod boosts;

use crate::bm25;
use crate::extract::{Entry, Kind};
use crate::query::entries::{private_path, split_symbol};
use crate::query::packages::ReadyPkg;
use std::collections::HashSet;

pub(super) use boosts::{boost, name_hit};

/// Identifier-like words in a question (`z.object`, `useEffect`,
/// `model_dump`, `cookies()`); plain English words are not identifiers.
pub(in crate::query) fn identifiers(q: &str) -> Vec<String> {
    let words: Vec<&str> = q
        .split(|c: char| c.is_whitespace() || matches!(c, ',' | '?' | '\'' | '"' | '[' | ']'))
        .filter(|w| !w.is_empty())
        .collect();
    let single = words.len() == 1;
    words
        .into_iter()
        .filter(|w| {
            let inner = w.trim_matches(|c: char| !c.is_alphanumeric());
            single || w.starts_with('`') || w.contains(['_', '.', '(', '$']) || w.contains("::") || inner.chars().skip(1).any(|c| c.is_ascii_uppercase())
        })
        .map(|w| w.trim_matches(|c: char| matches!(c, '`' | '(' | ')' | '.' | ':' | '!' | ';')))
        .filter(|w| w.len() >= 2 && w.chars().any(|c| c.is_ascii_alphabetic()))
        .flat_map(|w| {
            let mut v = vec![w.to_ascii_lowercase()];
            if let Some(last) = split_symbol(w).last() {
                v.push(last.to_ascii_lowercase());
            }
            v
        })
        .collect()
}

/// Plain words in the question that name a documented top-level API of the
/// searched packages ("run code *after* the response" in Next.js, which
/// exports `after`). They count as identifiers.
pub(in crate::query) fn api_words(ready: &[ReadyPkg], refs: &[(usize, usize)], q: &str) -> Vec<String> {
    let words: HashSet<String> = q
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|w| w.len() >= 4)
        .map(|w| w.to_ascii_lowercase())
        .filter(|w| !bm25::is_stop(w))
        .collect();
    let mut out = Vec::new();
    for &(pi, ei) in refs {
        let e = &ready[pi].0.entries[ei];
        let top = matches!(e.kind, Kind::Function | Kind::Macro | Kind::Class)
            && !e.doc.is_empty()
            && !private_path(e)
            && !e.legacy
            && e.path.matches(['.', ':']).count() <= 2;
        let n = e.name.trim_end_matches('!').to_ascii_lowercase();
        if top && words.contains(&n) && !out.contains(&n) {
            out.push(n);
        }
    }
    out
}

/// API names a doc points readers to: "use `model_validate` instead",
/// "Consider `z.strictObject(A.shape)`", "renamed to X", "in favor of X".
pub fn redirects(text: &str) -> Vec<String> {
    let lower = text.to_ascii_lowercase();
    let mut out = Vec::new();
    for pat in [
        "use ",
        "consider ",
        "renamed to ",
        "replaced by ",
        "in favor of ",
        "in favour of ",
        "instead use ",
    ] {
        let mut from = 0;
        while let Some(i) = lower[from..].find(pat) {
            let at = from + i + pat.len();
            from = at;
            let rest = &text[at..];
            let rest = rest.trim_start_matches(['`', '\'', '"', '*', ' ']);
            let ident: String = rest.chars().take_while(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | ':' | '$')).collect();
            let ident = ident.trim_end_matches(['.', ':']);
            // Only identifier-looking targets: code spans or names with _ . $ or camelCase.
            let quoted = text[at..].starts_with('`');
            let looks = ident.contains(['_', '.', '$']) || ident.chars().skip(1).any(|c| c.is_ascii_uppercase());
            if ident.len() >= 3 && (quoted || looks) {
                let last = ident.rsplit(['.', ':']).next().unwrap_or(ident).to_string();
                if last.len() >= 3 && !out.contains(&last) {
                    out.push(last);
                }
            }
        }
    }
    out
}

/// Terms of a prose heading. Capitalized words are names (`TypeScript`,
/// `JavaScript`) and stay whole; identifiers (`useActionState`,
/// `model_dump`) are also split into their parts.
pub(in crate::query) fn heading_terms(h: &str) -> Vec<String> {
    let mut out = Vec::new();
    for w in h.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).filter(|w| !w.is_empty()) {
        if w.starts_with(|c: char| c.is_ascii_uppercase()) && !w.contains('_') {
            out.push(bm25::stem(&w.to_ascii_lowercase()));
        } else {
            out.extend(bm25::terms(w));
        }
    }
    out
}

pub(super) fn major_of(version: &str) -> u64 {
    version.trim_start_matches('v').split('.').next().and_then(|m| m.parse().ok()).unwrap_or(0)
}

/// A migration or upgrade guide to a major older than the pinned one
/// ("Migrating to v6.0.0" in ESLint 9, "Upgrade to Prisma ORM 4" in Prisma 6):
/// history, not how things work now.
pub(in crate::query) fn old_upgrade_guide(e: &Entry, major: u64) -> bool {
    let title = e.name.split(" › ").next().unwrap_or("");
    let stem = e.file.rsplit('/').next().unwrap_or("");
    [title, stem].iter().any(|t| {
        let l = t.to_ascii_lowercase();
        (l.contains("migrat") || l.contains("upgrad"))
            && l.split(|c: char| !c.is_ascii_digit())
                .find(|d| !d.is_empty() && d.len() <= 3)
                .and_then(|d| d.parse::<u64>().ok())
                .is_some_and(|v| v < major)
    })
}

/// A page its own title marks as deprecated ("Configure Language Options (Deprecated)").
pub(in crate::query) fn deprecated_page(e: &Entry) -> bool {
    e.name.split(" › ").next().unwrap_or("").to_ascii_lowercase().contains("(deprecated)")
}

/// Changelog prose: a changelog file, or a section under a version heading
/// (`v1.7 (2020-10-26)`, `## 2.0.0`), as in READMEs that embed history.
pub(super) fn is_changelog(e: &Entry) -> bool {
    let f = e.file.rsplit('/').next().unwrap_or(&e.file).to_ascii_uppercase();
    if ["CHANGELOG", "CHANGES", "HISTORY", "NEWS", "RELEASES"].iter().any(|p| f.starts_with(p)) {
        return true;
    }
    e.name.split(" › ").skip(1).any(|h| {
        let h = h.trim().trim_start_matches(['v', 'V', '[']);
        let mut parts = h.split(|c: char| !c.is_ascii_digit());
        matches!((parts.next(), parts.next()), (Some(a), Some(b)) if !a.is_empty() && !b.is_empty()) && h.chars().nth(a_len(h)) == Some('.')
    })
}

fn a_len(h: &str) -> usize {
    h.chars().take_while(|c| c.is_ascii_digit()).count()
}

pub(in crate::query) fn change_intent(q: &str) -> bool {
    let q = q.to_ascii_lowercase();
    [
        "change",
        "migrat",
        "upgrad",
        "breaking",
        "deprecat",
        "removed",
        "renamed",
        "new in",
        "since",
        "release",
        "what's new",
        "differen",
    ]
    .iter()
    .any(|w| q.contains(w))
}
