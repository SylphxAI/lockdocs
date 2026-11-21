//! Dependency selection, exact-version sources, and index preparation.

use super::{Answer, Engine};
use crate::extract::Kind;
use crate::index::{self, PackageIndex};
use crate::locate::{self, Source};
use crate::project::Spec;
use crate::{embed, fetch, upstream, Dep, Eco};
use rayon::prelude::*;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

/// Cross-dependency searches index at most this many direct dependencies.
const MAX_PACKAGES: usize = 80;

/// An indexed package, the dependency it answers for, and a drift note.
pub(super) type ReadyPkg = (Arc<PackageIndex>, Dep, Option<String>);

/// A resolved package ready to search.
pub(super) enum Resolved {
    Ready(Arc<PackageIndex>, Dep, Option<String>),
    Missing(Dep, String),
}

/// Keep the original flat fields for CLI consumers; richer provenance is additive.
pub(super) fn fetch_report(dep: &Dep, status: &upstream::Manifest) -> Value {
    json!({"package": dep.id(), "repo": status.repo, "tag": status.tag,
        "files": status.files, "note": status.note, "upstream": status})
}

impl Engine {
    pub fn resolve(&self, filter: Option<&str>) -> Answer {
        let p = &self.project;
        let filter = filter.map(|f| f.trim().to_ascii_lowercase()).filter(|f| !f.is_empty());
        let shown: Vec<&Dep> = match &filter {
            Some(f) => p.deps.iter().filter(|d| d.name.to_ascii_lowercase().contains(f.as_str())).collect(),
            None => p.direct().collect(),
        };
        let rows: Vec<(&Dep, Option<Source>)> = shown
            .par_iter()
            .map(|d| (*d, locate::locate(d, &p.root).or_else(|| fetch::cached_for(d, Some(&p.root)))))
            .collect();
        let mut text = format!(
            "Project: {}\nLockfiles: {}\n",
            p.root.display(),
            if p.lockfiles.is_empty() {
                "none found".to_string()
            } else {
                p.lockfiles.join(", ")
            }
        );
        for w in &p.warnings {
            text.push_str(&format!("Warning: {w}\n"));
        }
        let total_direct = p.direct().count();
        text.push_str(&match &filter {
            Some(f) => format!("\n{} of {} dependencies match `{f}`:\n", rows.len(), p.deps.len()),
            None => format!(
                "\n{} direct dependencies ({} total with transitive; pass a filter to search all):\n",
                total_direct,
                p.deps.len()
            ),
        });
        let w = rows.iter().map(|(d, _)| d.name.len() + d.version.len() + 1).max().unwrap_or(10).min(48);
        let mut items = Vec::new();
        for (d, s) in &rows {
            let id = format!("{}@{}", d.name, d.version);
            let (status, src) = match s {
                Some(s) if s.version == d.version => ("docs ready", s.label.clone()),
                Some(s) => ("version drift", format!("installed {} at {}", s.version, s.label)),
                None => (
                    "not installed",
                    if self.opts.fetch {
                        "will fetch on first query".into()
                    } else {
                        "install it, or enable fetching".to_string()
                    },
                ),
            };
            text.push_str(&format!(
                "  {:<6} {:<w$}  {}{}  {}\n",
                d.eco.as_str(),
                id,
                if d.direct { "" } else { "(transitive) " },
                status,
                src
            ));
            items.push(json!({"ecosystem": d.eco.as_str(), "name": d.name, "version": d.version, "direct": d.direct, "lockfile": d.from, "status": status, "source": src}));
        }
        if rows.is_empty() {
            text.push_str("  (none)\n");
        }
        Answer {
            text,
            json: json!({"root": p.root.display().to_string(), "lockfiles": p.lockfiles, "dependencies": items, "total": p.deps.len(), "warnings": p.warnings}),
        }
    }

    /// Dependencies for a package spec. A version not in the project is
    /// fetched when allowed.
    fn deps_for(&self, spec_str: &str) -> Result<Vec<Dep>, String> {
        let spec = Spec::parse(spec_str);
        let found = self.project.find(&spec);
        if let Some(d) = found.iter().find(|d| spec.version_matches(&d.version)) {
            return Ok(vec![(*d).clone()]);
        }
        let eco = spec.eco.or_else(|| found.first().map(|d| d.eco));
        if let (Some(v), Some(eco)) = (&spec.version, eco) {
            if spec.is_exact() {
                let version = if eco == Eco::Go && !v.starts_with('v') { format!("v{v}") } else { v.clone() };
                let dep = Dep {
                    eco,
                    name: found.first().map_or(spec.name.clone(), |d| d.name.clone()),
                    version,
                    direct: false,
                    from: "requested".into(),
                };
                if fetch::cached_for(&dep, Some(&self.project.root)).is_some() || self.opts.fetch {
                    return Ok(vec![dep]);
                }
                return Err(format!(
                    "{}@{} is not in this project{}; enable fetching (--fetch or LOCKDOCS_FETCH=1) to read that exact version.",
                    dep.name,
                    dep.version,
                    found.first().map(|d| format!(" (it pins {})", d.version)).unwrap_or_default()
                ));
            }
        }
        if let Some(d) = found.first() {
            return Err(format!("this project pins {} {}, which does not match `{}`.", d.name, d.version, spec_str));
        }
        if spec.eco.is_none() && spec.version.is_some() && self.opts.fetch {
            return Err(format!(
                "`{}` is not a dependency here; prefix the ecosystem to fetch it (npm:, pypi:, cargo:, go:).",
                spec.name
            ));
        }
        let near = self.project.similar(&spec.name, 6);
        Err(if near.is_empty() {
            format!(
                "`{}` is not a dependency of {} (lockfiles: {}). Use `resolve` to list dependencies.",
                spec.name,
                self.project.root.display(),
                if self.project.lockfiles.is_empty() {
                    "none found".into()
                } else {
                    self.project.lockfiles.join(", ")
                }
            )
        } else {
            format!("`{}` is not a dependency here. Did you mean: {}?", spec.name, near.join(", "))
        })
    }

    /// Direct dependencies mentioned by name in the query, if any.
    fn mentioned(&self, query: &str) -> Vec<Dep> {
        let q = query.to_ascii_lowercase();
        let words: HashSet<&str> = q
            .split(|c: char| c.is_whitespace() || matches!(c, ',' | '?' | '(' | ')' | '`' | '\'' | '"'))
            .collect();
        let mut out: Vec<Dep> = Vec::new();
        for d in self.project.direct() {
            let n = d.name.to_ascii_lowercase();
            let short = n.rsplit('/').next().unwrap_or(&n);
            if (words.contains(n.as_str())
                || (d.eco == Eco::Go && words.contains(short))
                || words.iter().any(|w| w.starts_with(&format!("{n}.")) || w.starts_with(&format!("{n}::"))))
                && !out.iter().any(|o| o.name == d.name)
            {
                out.push(d.clone());
            }
        }
        out
    }

    pub(super) fn select(&self, package: Option<&str>, query: &str) -> Result<(Vec<Dep>, bool), String> {
        if let Some(p) = package.filter(|p| !p.trim().is_empty()) {
            let mut out = Vec::new();
            for spec in p.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                out.extend(self.deps_for(spec)?);
            }
            return Ok((out, false));
        }
        let m = self.mentioned(query);
        if !m.is_empty() {
            return Ok((m, false));
        }
        let mut seen = HashSet::new();
        let direct: Vec<Dep> = self
            .project
            .direct()
            .filter(|d| seen.insert((d.eco, d.name.clone())))
            .take(MAX_PACKAGES)
            .cloned()
            .collect();
        if direct.is_empty() {
            return Err(format!(
                "No dependencies found in {} (looked for {}). Pass `package`, or run from the project directory.",
                self.project.root.display(),
                "package-lock.json, pnpm-lock.yaml, yarn.lock, bun.lock, Cargo.lock, uv.lock, poetry.lock, requirements*.txt, go.mod"
            ));
        }
        Ok((direct, true))
    }

    fn block(&self, req: &crate::private::PrivateRequired) {
        let mut b = self.private_blocked.lock().unwrap();
        if !b.iter().any(|r| r.package == req.package) {
            b.push(req.clone());
        }
    }

    /// Find sources for a dependency: installed files first, then a
    /// previously fetched copy, then (only when enabled) the registry.
    fn source(&self, dep: &Dep) -> Result<(Source, Option<String>), String> {
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

    pub(super) fn index(&self, dep: &Dep) -> Resolved {
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
                    // Rebuild once the embedding model has arrived.
                    if !i.embed.is_empty() || !embed::available() {
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

    /// Upstream docs for this exact version: a cached copy, or (when fetching
    /// is enabled) a one-time download.
    fn upstream(&self, dep: &Dep, src: &Source) -> (Option<(PathBuf, upstream::Manifest)>, Option<String>) {
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

    pub(super) fn resolve_all(&self, deps: &[Dep]) -> (Vec<ReadyPkg>, Vec<(Dep, String)>) {
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
