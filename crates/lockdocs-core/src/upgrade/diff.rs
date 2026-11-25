use super::{Change, Diff};
use crate::extract::{Entry, Kind};
use std::collections::{HashMap, HashSet};

fn is_api(e: &Entry) -> bool {
    e.kind != Kind::Prose && !e.legacy
}

pub(super) fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn parent(path: &str) -> &str {
    path.rfind(['.', ':']).map_or("", |i| path[..i].trim_end_matches(':'))
}

/// The signature with the symbol's own name blanked, to compare across renames.
fn shape(e: &Entry) -> String {
    squash(&e.sig.replacen(&e.name, "_", 1))
}

fn deprecated(e: &Entry) -> bool {
    let doc = e.doc.to_ascii_lowercase();
    let sig = e.sig.to_ascii_lowercase();
    doc.contains("@deprecated") || doc.contains("deprecated:") || doc.contains(".. deprecated::") || sig.contains("#[deprecated") || sig.contains("@deprecated")
}

fn words(s: &str) -> HashSet<String> {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2)
        .map(str::to_ascii_lowercase)
        .collect()
}

fn similarity(a: &str, b: &str) -> f32 {
    let (a, b) = (words(a), words(b));
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    a.intersection(&b).count() as f32 / a.union(&b).count() as f32
}

/// Symbol-level difference. A path present in the new version (as a symbol or
/// as a re-export) is never "removed".
pub fn diff(old: &[Entry], new: &[Entry]) -> Vec<Diff> {
    let new_by_path: HashMap<&str, &Entry> = new.iter().filter(|e| is_api(e)).map(|e| (e.path.as_str(), e)).collect();
    let old_paths: HashSet<&str> = old.iter().filter(|e| is_api(e)).map(|e| e.path.as_str()).collect();
    let mut added: Vec<&Entry> = new
        .iter()
        .filter(|e| is_api(e) && e.alias_of.is_none() && !old_paths.contains(e.path.as_str()))
        .collect();
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for o in old.iter().filter(|e| is_api(e)) {
        if !seen.insert(o.path.as_str()) {
            continue;
        }
        match new_by_path.get(o.path.as_str()) {
            Some(n) => {
                if deprecated(n) && !deprecated(o) {
                    out.push(Diff {
                        change: Change::Deprecated,
                        old: o.clone(),
                        new: Some((*n).clone()),
                    });
                } else if o.alias_of.is_none() && n.alias_of.is_none() && squash(&o.sig) != squash(&n.sig) {
                    out.push(Diff {
                        change: Change::SignatureChanged,
                        old: o.clone(),
                        new: Some((*n).clone()),
                    });
                }
            }
            None => {
                // Best rename candidate: same kind and parent, equal signature shape or similar docs.
                let best = added
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| a.kind == o.kind && parent(&a.path) == parent(&o.path))
                    .map(|(i, a)| {
                        let score = if !o.sig.is_empty() && shape(o) == shape(a) {
                            1.0
                        } else {
                            similarity(&o.doc, &a.doc)
                        };
                        (i, score)
                    })
                    .filter(|(_, s)| *s >= 0.7)
                    .max_by(|x, y| x.1.total_cmp(&y.1));
                match best {
                    Some((i, _)) => {
                        let n = added.remove(i);
                        out.push(Diff {
                            change: Change::Renamed,
                            old: o.clone(),
                            new: Some(n.clone()),
                        });
                    }
                    None => out.push(Diff {
                        change: Change::Removed,
                        old: o.clone(),
                        new: None,
                    }),
                }
            }
        }
    }
    out
}
