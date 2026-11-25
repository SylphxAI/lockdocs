//! Collect and rank declarations without guessing across conflicting npm exports.

use crate::extract::{Entry, Kind};
use crate::query::entries::{private_path, split_symbol, tooling_path};
use crate::query::packages::ReadyPkg;
use serde_json::{json, Value};
use std::collections::HashSet;

type Candidate = (f32, usize, usize);

pub(super) struct Matches {
    pub(super) cands: Vec<Candidate>,
    pub(super) applicability: Vec<Value>,
    pub(super) alternatives: Vec<Value>,
    pub(super) npm_miss: Option<NpmMiss>,
}

#[derive(Clone, Copy)]
pub(super) enum NpmMiss {
    Ambiguous,
    Unresolved,
    NotExported,
}

pub(super) fn find(ready: &[ReadyPkg], last: &str, quals: &[String], public_path: &str, requested_import: Option<&str>) -> Matches {
    let mut cands: Vec<Candidate> = Vec::new();
    let mut applicability = Vec::new();
    let mut alternatives = Vec::new();
    let mut unresolved = false;
    let mut npm_scoped = false;
    let last_l = last.to_ascii_lowercase();
    for (pi, (idx, _, _)) in ready.iter().enumerate() {
        if let Some(api) = &idx.npm_api {
            npm_scoped = true;
            let import = requested_import.unwrap_or(&idx.name);
            let mut has_root = false;
            for root in &api.roots {
                let found = api.resolve(root, public_path, idx.api_entries());
                if root.import == import {
                    has_root = true;
                    unresolved |= found.unresolved;
                    for ei in found.entries {
                        let e = &idx.api_entries()[ei];
                        cands.push((10.0, pi, ei));
                        applicability.push(
                            json!({"import": root.import, "conditions": root.conditions.trim_start_matches('/'), "entrypoint": root.file,
                            "file": e.file, "line": e.line, "path": e.path}),
                        );
                    }
                } else if !found.unresolved {
                    for ei in found.entries {
                        let e = &idx.api_entries()[ei];
                        alternatives.push(
                            json!({"package": idx.id(), "import": root.import, "conditions": root.conditions.trim_start_matches('/'),
                            "entrypoint": root.file, "path": e.path, "file": e.file, "line": e.line, "signature": e.sig}),
                        );
                    }
                }
            }
            unresolved |= !has_root;
            continue;
        }
        for (ei, e) in idx.api_entries().iter().enumerate() {
            if e.kind == Kind::Prose {
                continue;
            }
            let name = e.name.trim_end_matches('!');
            let exact = name == last;
            if !exact && name.to_ascii_lowercase() != last_l {
                continue;
            }
            cands.push((api_score(e, exact, quals), pi, ei));
        }
    }
    cands.sort_by_key(|(_, pi, ei)| (*pi, *ei));
    cands.dedup_by_key(|(_, pi, ei)| (*pi, *ei));
    // Multiple overloads at one declaration are valid. Conflicting files,
    // packages or conditional editions are not a license to guess.
    let locations: HashSet<_> = cands
        .iter()
        .map(|(_, pi, ei)| (*pi, &ready[*pi].0.api_entries()[*ei].file, &ready[*pi].0.api_entries()[*ei].path))
        .collect();
    let ambiguous = npm_scoped && locations.len() > 1 && {
        let signatures: HashSet<_> = cands.iter().map(|(_, pi, ei)| &ready[*pi].0.api_entries()[*ei].sig).collect();
        signatures.len() > 1
    };
    let npm_miss = if npm_scoped && (cands.is_empty() || unresolved || ambiguous) {
        Some(if ambiguous {
            NpmMiss::Ambiguous
        } else if unresolved {
            NpmMiss::Unresolved
        } else {
            NpmMiss::NotExported
        })
    } else {
        cands.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        None
    };
    Matches {
        cands,
        applicability,
        alternatives,
        npm_miss,
    }
}

fn api_score(e: &Entry, exact: bool, quals: &[String]) -> f32 {
    let mut s = if exact { 10.0 } else { 7.0 };
    let segs: Vec<String> = split_symbol(&e.path).into_iter().map(|x| x.to_ascii_lowercase()).collect();
    // A bare name means the top-level API, not a same-named member.
    if quals.is_empty() {
        s += if matches!(e.kind, Kind::Method | Kind::Field) { -1.5 } else { 1.5 };
    }
    let file = e.file.to_ascii_lowercase();
    for q in quals {
        if segs.iter().any(|x| x == q) {
            s += 3.0;
        } else if file.contains(q.as_str()) {
            s += 1.0;
        }
    }
    // The qualifier directly before the name matters most (`Router.route`).
    if let (Some(q), Some(parent)) = (quals.last(), segs.len().checked_sub(2).map(|i| &segs[i])) {
        if q == parent {
            s += 2.0;
        }
    }
    if !e.doc.is_empty() {
        s += 2.0;
    }
    if private_path(e) {
        s -= 3.0;
    }
    if e.legacy {
        s -= 5.0;
    }
    if tooling_path(e) {
        s -= 3.0;
    }
    if e.kind == Kind::Alias {
        s -= 0.5;
    }
    s -= 0.2 * segs.len() as f32;
    s
}
