//! Load, build, and warm indexes from exact-version sources.

use super::super::{Answer, Engine};
use super::{ReadyPkg, Resolved};
use crate::extract::Kind;
use crate::{embed, index, upstream, Dep};
use rayon::prelude::*;
use serde_json::json;
use std::collections::HashSet;
use std::sync::Arc;

impl Engine {
    pub(in crate::query) fn index(&self, dep: &Dep) -> Resolved {
        match self.source(dep) {
            Ok((src, mut note)) => {
                let key = format!(
                    "{}:{}@{}:{}:{}",
                    dep.eco,
                    dep.name,
                    src.version,
                    src.dir.display(),
                    index::source_stamp(dep, &src, &self.project.root)
                );
                if let Some(i) = self.indexes.lock().unwrap().get(&key) {
                    // A separate CLI fetch can replace a stale cache while MCP is running.
                    let replaced = i
                        .upstream_provenance
                        .as_ref()
                        .is_some_and(|m| upstream::stale(m) && upstream::cached(dep).is_some_and(|(_, cached)| cached.format != m.format));
                    // Also rebuild once the embedding model has arrived.
                    if !replaced && (!i.embed.is_empty() || !embed::available()) {
                        if let Some(n) = self.upstream_notes.lock().unwrap().get(&dep.id()) {
                            note = Some(match note {
                                Some(old) => format!("{old} {n}"),
                                None => n.clone(),
                            });
                        }
                        return Resolved::Ready(i.clone(), dep.clone(), note);
                    }
                }
                let (up, up_note) = self.upstream(dep, &src);
                if let Some(n) = up_note {
                    self.upstream_notes.lock().unwrap().insert(dep.id(), n.clone());
                    note = Some(match note {
                        Some(old) => format!("{old} {n}"),
                        None => n,
                    });
                } else {
                    self.upstream_notes.lock().unwrap().remove(&dep.id());
                }
                let idx = Arc::new(index::load_or_build(dep, &src, &self.project.root, up.as_ref()));
                self.indexes.lock().unwrap().insert(key, idx.clone());
                Resolved::Ready(idx, dep.clone(), note)
            }
            Err(e) => Resolved::Missing(dep.clone(), e),
        }
    }

    pub(in crate::query) fn resolve_all(&self, deps: &[Dep]) -> (Vec<ReadyPkg>, Vec<(Dep, String)>) {
        let results: Vec<Resolved> = deps.par_iter().map(|d| self.index(d)).collect();
        let mut ready = Vec::new();
        let mut missing = Vec::new();
        for r in results {
            match r {
                Resolved::Ready(i, d, n) => ready.push((i, d, n)),
                Resolved::Missing(d, e) => missing.push((d, e)),
            }
        }
        (ready, missing)
    }

    /// Build (or load) indexes ahead of time and report what they hold.
    pub fn warm(&self, package: Option<&str>) -> Result<Answer, String> {
        let t = std::time::Instant::now();
        let deps: Vec<Dep> = match package.filter(|p| !p.trim().is_empty()) {
            Some(p) => self.select(Some(p), "")?.0,
            None => {
                let mut seen = HashSet::new();
                self.project.direct().filter(|d| seen.insert((d.eco, d.name.clone()))).cloned().collect()
            }
        };
        let (ready, missing) = self.resolve_all(&deps);
        let mut text = String::new();
        let mut items = Vec::new();
        for (idx, _, note) in &ready {
            let symbols = idx.entries.iter().filter(|e| e.kind != Kind::Prose).count();
            text.push_str(&format!(
                "  {:<40} {:>6} symbols {:>6} doc sections  ({})\n",
                idx.id(),
                symbols,
                idx.entries.len() - symbols,
                idx.source
            ));
            if let Some(n) = note {
                text.push_str(&format!("    note: {n}\n"));
            }
            items.push(json!({"package": idx.id(), "ecosystem": idx.eco.as_str(), "symbols": symbols, "sections": idx.entries.len() - symbols, "source": idx.source, "upstream": idx.upstream, "note": note, "build_ms": idx.build_ms}));
        }
        for (d, e) in &missing {
            text.push_str(&format!("  {:<40} missing: {e}\n", d.id()));
        }
        let ms = t.elapsed().as_millis();
        text = format!("Indexed {} of {} packages in {ms} ms.\n{text}", ready.len(), deps.len());
        Ok(Answer {
            text,
            json: json!({"packages": items, "missing": missing.iter().map(|(d, e)| json!({"package": d.id(), "error": e})).collect::<Vec<_>>(), "ms": ms}),
        })
    }
}
