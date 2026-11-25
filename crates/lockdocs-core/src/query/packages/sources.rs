//! Acquire exact package sources and release-tagged upstream docs.

use super::super::{Answer, Engine};
use super::fetch_report;
use crate::locate::{self, Source};
use crate::{embed, fetch, upstream, Dep, Eco};
use rayon::prelude::*;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::PathBuf;

impl Engine {
    fn block(&self, req: &crate::private::PrivateRequired) {
        let mut b = self.private_blocked.lock().unwrap();
        if !b.iter().any(|r| r.package == req.package) {
            b.push(req.clone());
        }
    }

    /// Find sources for a dependency: installed files first, then a
    /// previously fetched copy, then (only when enabled) the registry.
    pub(super) fn source(&self, dep: &Dep) -> Result<(Source, Option<String>), String> {
        let local = locate::locate(dep, &self.project.root);
        if let Some(s) = &local {
            if s.version == dep.version {
                return Ok((s.clone(), None));
            }
        }
        if let Some(s) = fetch::cached_for(dep, Some(&self.project.root)) {
            return Ok((s, None));
        }
        if !self.opts.private {
            if let Some(req) = crate::private::needs_private(dep, &self.project.root) {
                self.block(&req);
                return Err(format!("{req}. Run `lockdocs licence status`."));
            }
        }
        if self.opts.fetch {
            match fetch::fetch_for(dep, Some(&self.project.root), self.opts.private) {
                Ok(s) => return Ok((s, None)),
                Err(e) => {
                    if let Some(req) = e.downcast_ref::<crate::private::PrivateRequired>() {
                        self.block(req);
                        return Err(format!("{req}. Run `lockdocs licence status`."));
                    }
                    return Err(format!("{}: exact registry fetching failed: {e:#}; no other version substituted", dep.id()));
                }
            }
        }
        if let Some(s) = local {
            return Err(format!(
                "{} pins {}, but {} has {}; refusing to substitute the installed version. Reinstall to sync, or enable exact registry fetching (--fetch).",
                dep.from,
                dep.id(),
                s.label,
                s.version
            ));
        }
        let how = match dep.eco {
            Eco::Npm => "run your package manager's install",
            Eco::PyPI => "create the virtualenv (.venv) and install",
            Eco::Cargo => "run `cargo fetch`",
            Eco::Go => "run `go mod download`",
        };
        Err(format!(
            "{} is pinned in {} but its files are not on this machine: {how}, or enable fetching (--fetch or LOCKDOCS_FETCH=1) to download exactly this version.",
            dep.id(),
            dep.from
        ))
    }

    /// Upstream docs for this exact version: a cached copy, or (when fetching
    /// is enabled) a one-time download.
    pub(super) fn upstream(&self, dep: &Dep, src: &Source) -> (Option<(PathBuf, upstream::Manifest)>, Option<String>) {
        if src.version != dep.version || std::env::var("LOCKDOCS_NO_UPSTREAM").is_ok() {
            return (None, None);
        }
        let enabled = self.opts.fetch || self.opts.upstream;
        if fetch::is_git(dep) {
            return (
                None,
                enabled.then(|| "upstream docs skipped: git dependency's resolved commit is not a release tag; using its checkout files".into()),
            );
        }
        let cached = upstream::cached(dep);
        if let Some(c) = cached.as_ref().filter(|(_, m)| !enabled || !upstream::needs_refresh(m, self.opts.fetch)) {
            let note = if upstream::needs_refresh(&c.1, false) {
                Some(format!(
                    "offline fallback: upstream cache format {} has not been exact-tag revalidated{}{}",
                    c.1.format,
                    if c.1.format < 4 {
                        "; its old commit candidate may have been a branch"
                    } else {
                        ""
                    },
                    c.1.note.as_ref().map(|n| format!("; {n}")).unwrap_or_default()
                ))
            } else {
                c.1.note.clone()
            };
            return (Some(c.clone()), note);
        }
        if enabled {
            let result = if self.opts.fetch {
                upstream::fetch_private(dep, src, self.opts.private)
            } else {
                upstream::fetch_automatic(dep, src)
            };
            return match result {
                Ok(m) => (Some((upstream::dir(dep), m.clone())), m.note.clone()),
                Err(e) => (None, Some(format!("upstream docs fetch failed: {e:#}; using package files"))),
            };
        }
        (None, None)
    }

    /// `lockdocs fetch`: download what makes answers complete, once: missing
    /// packages at their pinned versions, upstream docs at each version's git
    /// tag, and the embedding model.
    pub fn fetch_all(&self, package: Option<&str>) -> Result<Answer, String> {
        let t = std::time::Instant::now();
        let mut text = String::new();
        if embed::enabled() {
            match embed::ensure() {
                Ok(()) => text.push_str(&format!("  model   {} ready\n", embed::MODEL_ID)),
                Err(e) => text.push_str(&format!("  model   {} unavailable ({e:#}); keyword search only\n", embed::MODEL_ID)),
            }
        }
        let deps: Vec<Dep> = match package.filter(|p| !p.trim().is_empty()) {
            Some(p) => {
                let mut out = Vec::new();
                for spec in p.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                    out.extend(self.deps_for(spec)?);
                }
                out
            }
            None => {
                let mut seen = HashSet::new();
                self.project.direct().filter(|d| seen.insert((d.eco, d.name.clone()))).cloned().collect()
            }
        };
        let rows: Vec<(Dep, String, Value)> = deps
            .par_iter()
            .map(|d| {
                if fetch::is_git(d) {
                    let src = locate::locate(d, &self.project.root);
                    return (
                        d.clone(),
                        "upstream skipped: git dependency; use resolved checkout files".into(),
                        json!({"package": d.id(), "status": "git-checkout", "source": src.map(|s| s.label)}),
                    );
                }
                let src = match locate::locate(d, &self.project.root)
                    .filter(|s| s.version == d.version)
                    .or_else(|| fetch::cached_for(d, Some(&self.project.root)))
                {
                    Some(s) => Some(s),
                    None if !fetch::is_git(d) => match fetch::fetch_for(d, Some(&self.project.root), self.opts.private) {
                        Ok(s) => Some(s),
                        Err(e) => {
                            if let Some(req) = e.downcast_ref::<crate::private::PrivateRequired>() {
                                self.block(req);
                            }
                            None
                        }
                    },
                    None => None,
                };
                let Some(src) = src else {
                    return (
                        d.clone(),
                        "package files unavailable".to_string(),
                        json!({"package": d.id(), "status": "missing"}),
                    );
                };
                let status = match upstream::fetch_private(d, &src, self.opts.private) {
                    Ok(m) => m,
                    Err(e) => {
                        return (
                            d.clone(),
                            format!("upstream docs fetch failed: {e:#}"),
                            json!({"package": d.id(), "status": "fetch-failed", "error": format!("{e:#}")}),
                        )
                    }
                };
                let line = match (&status.tag, status.files) {
                    (_, n) if n > 0 => format!(
                        "upstream docs {}{}: {n} files ({:.1} MB)",
                        status.tag.as_ref().map(|t| format!("{}@{t}", status.repo)).unwrap_or_default(),
                        status.site.as_ref().map(|s| format!(" + docs site {s}")).unwrap_or_default(),
                        status.bytes as f64 / 1e6
                    ),
                    _ => format!("no upstream docs ({}: {})", status.repo, status.note.clone().unwrap_or_default()),
                };
                (
                    d.clone(),
                    if let Some(note) = &status.note { format!("{line}; {note}") } else { line },
                    fetch_report(d, &status),
                )
            })
            .collect();
        for (d, line, _) in &rows {
            text.push_str(&format!("  {:<40} {line}\n", d.id()));
        }
        // Rebuild indexes with what was fetched.
        self.indexes.lock().unwrap().clear();
        let ms = t.elapsed().as_millis();
        Ok(Answer {
            text: format!("Fetched for {} packages in {ms} ms (cached; later queries stay offline):\n{text}", rows.len()),
            json: json!({"packages": rows.iter().map(|r| r.2.clone()).collect::<Vec<_>>(), "ms": ms, "model": embed::installed()}),
        })
    }
}
