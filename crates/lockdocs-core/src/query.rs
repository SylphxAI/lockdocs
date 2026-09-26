//! The three questions agents ask: which versions do I have (`resolve`),
//! what do the docs say about X (`docs`), and what exactly is this symbol
//! (`api`). Answers are token-budgeted and cite `package@version path:line`.

use crate::bm25::{self, Bm25};
use crate::extract::{Entry, Kind};
use crate::index::{self, PackageIndex};
use crate::locate::{self, Source};
use crate::project::{Project, Spec};
use crate::{embed, est_tokens, fetch, norm_name, upstream, Dep, Eco};
use rayon::prelude::*;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub const DEFAULT_TOKENS: usize = 1200;
/// Results scoring below this fraction of the best are left out (after 3).
const RELEVANCE_FLOOR: f32 = 0.3;
/// Share of the fused score from the embedding similarity.
const DENSE_WEIGHT: f32 = 0.45;
/// Share of the keyword score from headings and first sentences.
const HEAD_WEIGHT: f32 = 0.25;
/// Cross-dependency searches index at most this many direct dependencies.
const MAX_PACKAGES: usize = 80;

#[derive(Debug, Clone)]
pub struct Options {
    /// Allow downloading exact versions that are not installed.
    pub fetch: bool,
}

impl Default for Options {
    fn default() -> Self {
        let fetch = std::env::var("LOCKDOCS_FETCH").is_ok_and(|v| matches!(v.as_str(), "1" | "true" | "yes"));
        Options { fetch }
    }
}

pub struct Answer {
    pub text: String,
    pub json: Value,
}

pub struct Engine {
    pub project: Project,
    opts: Options,
    indexes: Mutex<HashMap<String, Arc<PackageIndex>>>,
}

/// An indexed package, the dependency it answers for, and a drift note.
type ReadyPkg = (Arc<PackageIndex>, Dep, Option<String>);

/// A resolved package ready to search.
enum Resolved {
    Ready(Arc<PackageIndex>, Dep, Option<String>),
    Missing(Dep, String),
}

fn lang(eco: Eco) -> &'static str {
    match eco {
        Eco::Npm => "ts",
        Eco::PyPI => "python",
        Eco::Cargo => "rust",
        Eco::Go => "go",
    }
}

/// Changelog prose: a changelog file, or a section under a version heading
/// (`v1.7 (2020-10-26)`, `## 2.0.0`), as in READMEs that embed history.
fn is_changelog(e: &Entry) -> bool {
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

fn change_intent(q: &str) -> bool {
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

fn private_path(e: &Entry) -> bool {
    e.file.split('/').any(|s| s.starts_with('_') && !s.starts_with("__init__")) || e.path.contains("._") || e.file.contains("/internal/")
}

/// Tooling shipped inside a package (type-checker plugins, test helpers).
fn tooling_path(e: &Entry) -> bool {
    e.file.split('/').any(|s| {
        let stem = s.split('.').next().unwrap_or(s);
        matches!(
            stem,
            "mypy" | "pylint" | "plugin" | "plugins" | "testing" | "conftest" | "scripts" | "codemod" | "codemods" | "eslint" | "babel"
        )
    })
}

impl Engine {
    pub fn new(root: &Path, opts: Options) -> Engine {
        Engine {
            project: Project::load(root),
            opts,
            indexes: Mutex::new(HashMap::new()),
        }
    }

    pub fn root(&self) -> &Path {
        &self.project.root
    }

    pub fn fetch_enabled(&self) -> bool {
        self.opts.fetch
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
        if let Some(s) = fetch::cached(dep) {
            return Ok((s, None));
        }
        if self.opts.fetch {
            match fetch::fetch(dep) {
                Ok(s) => return Ok((s, None)),
                Err(e) => {
                    if local.is_none() {
                        return Err(format!("{} is pinned but not installed, and fetching failed: {e:#}", dep.id()));
                    }
                }
            }
        }
        if let Some(s) = local {
            let note = format!(
                "{} pins {}@{}, but {} has {}; showing the installed {}. Reinstall to sync{}.",
                dep.from,
                dep.name,
                dep.version,
                s.label,
                s.version,
                s.version,
                if self.opts.fetch {
                    ""
                } else {
                    ", or enable fetching (--fetch / LOCKDOCS_FETCH=1) to read the pinned version"
                }
            );
            return Ok((s, Some(note)));
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

    fn index(&self, dep: &Dep) -> Resolved {
        match self.source(dep) {
            Ok((src, note)) => {
                let key = format!("{}:{}@{}:{}", dep.eco, dep.name, src.version, src.dir.display());
                if let Some(i) = self.indexes.lock().unwrap().get(&key) {
                    // Rebuild once the embedding model has arrived.
                    if !i.embed.is_empty() || embed::get().is_none() {
                        return Resolved::Ready(i.clone(), dep.clone(), note);
                    }
                }
                let up = self.upstream(dep, &src);
                let idx = Arc::new(index::load_or_build(dep, &src, &self.project.root, up.as_ref()));
                self.indexes.lock().unwrap().insert(key, idx.clone());
                Resolved::Ready(idx, dep.clone(), note)
            }
            Err(e) => Resolved::Missing(dep.clone(), e),
        }
    }

    /// Upstream docs for this exact version: a cached copy, or (when fetching
    /// is enabled) a one-time download.
    fn upstream(&self, dep: &Dep, src: &Source) -> Option<(PathBuf, upstream::Manifest)> {
        if src.version != dep.version || std::env::var("LOCKDOCS_NO_UPSTREAM").is_ok() {
            return None;
        }
        let cached = upstream::cached(dep);
        if let Some(c) = cached.as_ref().filter(|(_, m)| !(self.opts.fetch && upstream::stale(m))) {
            return Some(c.clone());
        }
        if self.opts.fetch && upstream::repo_of(dep, src).is_some() {
            let _ = upstream::fetch(dep, src);
            return upstream::cached(dep);
        }
        None
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
                let src = match locate::locate(d, &self.project.root)
                    .filter(|s| s.version == d.version)
                    .or_else(|| fetch::cached(d))
                {
                    Some(s) => Some(s),
                    None if !d.from.ends_with("(git)") => fetch::fetch(d).ok(),
                    None => None,
                };
                let Some(src) = src else {
                    return (
                        d.clone(),
                        "package files unavailable".to_string(),
                        json!({"package": d.id(), "status": "missing"}),
                    );
                };
                let status = match upstream::cached(d).filter(|(_, m)| !upstream::stale(m)) {
                    Some((_, m)) => m,
                    None => match upstream::fetch(d, &src) {
                        Ok(m) => m,
                        Err(e) => {
                            return (
                                d.clone(),
                                format!("upstream docs: {e:#}"),
                                json!({"package": d.id(), "status": "no-upstream", "error": format!("{e:#}")}),
                            );
                        }
                    },
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
                    line,
                    json!({"package": d.id(), "repo": status.repo, "tag": status.tag, "files": status.files, "note": status.note}),
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
                if fetch::cached(&dep).is_some() || self.opts.fetch {
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

    fn select(&self, package: Option<&str>, query: &str) -> Result<(Vec<Dep>, bool), String> {
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

    fn resolve_all(&self, deps: &[Dep]) -> (Vec<ReadyPkg>, Vec<(Dep, String)>) {
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
            items.push(json!({"package": idx.id(), "ecosystem": idx.eco.as_str(), "symbols": symbols, "sections": idx.entries.len() - symbols, "source": idx.source, "build_ms": idx.build_ms}));
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

    // ------------------------------------------------------------ resolve

    pub fn resolve(&self, filter: Option<&str>) -> Answer {
        let p = &self.project;
        let filter = filter.map(|f| f.trim().to_ascii_lowercase()).filter(|f| !f.is_empty());
        let shown: Vec<&Dep> = match &filter {
            Some(f) => p.deps.iter().filter(|d| d.name.to_ascii_lowercase().contains(f.as_str())).collect(),
            None => p.direct().collect(),
        };
        let rows: Vec<(&Dep, Option<Source>)> = shown
            .par_iter()
            .map(|d| (*d, locate::locate(d, &p.root).or_else(|| fetch::cached(d))))
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

    // ------------------------------------------------------------ docs

    pub fn docs(&self, package: Option<&str>, query: &str, tokens: usize) -> Result<Answer, String> {
        let tokens = tokens.clamp(200, 20000);
        let (deps, broad) = self.select(package, query)?;
        let (ready, missing) = self.resolve_all(&deps);
        if ready.is_empty() {
            return Err(missing.into_iter().map(|(_, e)| e).collect::<Vec<_>>().join("\n"));
        }
        let q = query.trim();
        if q.is_empty() {
            return Ok(self.overview(&ready, tokens));
        }
        let qterms = bm25::query_terms(q);
        // One BM25 over the selected packages so scores compare.
        let mut refs: Vec<(usize, usize)> = Vec::new();
        for (pi, (idx, _, _)) in ready.iter().enumerate() {
            for ei in 0..idx.entries.len() {
                refs.push((pi, ei));
            }
        }
        let bm = Bm25::build(refs.iter().map(|&(pi, ei)| (ready[pi].0.terms[ei].as_slice(), ready[pi].0.lens[ei])));
        let hb = Bm25::build(refs.iter().map(|&(pi, ei)| (ready[pi].0.head[ei].as_slice(), ready[pi].0.head_lens[ei])));
        let mut raw_idents = identifiers(q);
        raw_idents.extend(api_words(&ready, q));
        let changes = change_intent(q);
        let scored = hybrid_rank(&ready, &refs, (&bm, &hb), &qterms, q, &raw_idents, changes);
        let mut out = Pack::new(tokens);
        let mut header = String::new();
        for (idx, dep, note) in &ready {
            header.push_str(&format!(
                "{} · {} · {}{}\n",
                idx.id(),
                idx.eco,
                idx.source,
                if dep.from == "requested" {
                    String::new()
                } else {
                    format!(" · pinned in {}", dep.from)
                }
            ));
            if let Some(u) = &idx.upstream {
                header.push_str(&format!("Upstream docs: {u}\n"));
            }
            if let Some(n) = note {
                header.push_str(&format!("Note: {n}\n"));
            }
        }
        if broad {
            header = format!("Searched {} direct dependencies. Pass `package` to focus.\n", ready.len());
        }
        for (d, e) in &missing {
            if !broad {
                header.push_str(&format!("Skipped {}: {e}\n", d.id()));
            }
        }
        out.push_raw(&header);
        let mut seen: HashSet<(usize, String, u32)> = HashSet::new();
        let mut hits_json = Vec::new();
        let mut used_pkgs: Vec<String> = Vec::new();
        let top = scored.first().map_or(0.0, |s| s.0);
        for (score, pi, ei) in scored {
            let idx = &ready[pi].0;
            let e = &idx.entries[ei];
            // Stop at weak matches once a few good ones are in.
            if out.blocks >= 3 && score < top * RELEVANCE_FLOOR {
                break;
            }
            if !seen.insert((pi, e.file.clone(), e.line)) {
                continue;
            }
            let e = resolve_alias(idx, e);
            // The top result also carries the lead of its page and parent
            // section, within the same budget.
            let ctx = if out.blocks == 0 { section_context(idx, e) } else { None };
            let max_doc = if out.blocks == 0 {
                2000 - ctx.as_ref().map_or(0, |c| c.len().min(800) / 2)
            } else {
                800
            };
            let mut block = render_entry(idx, e, max_doc);
            if let (Some(ctx), Some(nl)) = (ctx, block.find('\n')) {
                block.insert_str(nl + 1, &ctx);
            }
            if !out.push_block(&block) {
                break;
            }
            if !used_pkgs.contains(&idx.id()) {
                used_pkgs.push(idx.id());
            }
            hits_json.push(json!({"package": idx.id(), "kind": e.kind.as_str(), "path": e.path, "file": e.file, "line": e.line, "score": score}));
        }
        if out.blocks == 0 {
            out.push_raw(&format!(
                "No match for \"{q}\" in {}. Try other words, a symbol name (the `api` tool), or `package` to widen/narrow.\n",
                ready.iter().map(|r| r.0.id()).collect::<Vec<_>>().join(", ")
            ));
        } else if broad {
            out.prepend(&format!("Sources: {}\n", used_pkgs.join(", ")));
        }
        // Packages that ship few docs: point at the one-time upstream fetch.
        for (idx, dep, _) in &ready {
            let prose = idx.entries.iter().filter(|e| e.kind == Kind::Prose).count();
            if !broad && idx.upstream.is_none() && prose < 40 && !self.opts.fetch && upstream::cached(dep).is_none() {
                out.push_raw(&format!(
                    "\nTip: {} ships few docs. `lockdocs fetch {}` adds its upstream docs at this version's git tag (one-time download, then offline).\n",
                    idx.id(),
                    idx.name
                ));
            }
        }
        Ok(Answer {
            json: json!({"query": q, "packages": ready.iter().map(|r| r.0.id()).collect::<Vec<_>>(), "hits": hits_json, "tokens": est_tokens(&out.text)}),
            text: out.text,
        })
    }

    fn overview(&self, ready: &[ReadyPkg], tokens: usize) -> Answer {
        let mut out = Pack::new(tokens);
        for (idx, dep, note) in ready {
            out.push_raw(&format!("{} · {} · {} · pinned in {}\n", idx.id(), idx.eco, idx.source, dep.from));
            if let Some(n) = note {
                out.push_raw(&format!("Note: {n}\n"));
            }
            let symbols = idx
                .entries
                .iter()
                .filter(|e| e.kind != Kind::Prose && e.kind != Kind::Alias && !private_path(e))
                .count();
            let prose = idx.entries.len() - symbols;
            out.push_raw(&format!("Indexed {symbols} API symbols and {prose} doc sections.\n"));
            // Top-level API names, shortest paths first.
            let mut top: Vec<&Entry> = idx
                .entries
                .iter()
                .filter(|e| e.kind != Kind::Prose && !private_path(e) && !e.legacy && !e.doc.is_empty())
                .collect();
            top.sort_by_key(|e| (e.path.matches(['.', ':']).count(), e.path.len()));
            let names: Vec<String> = top.iter().take(60).map(|e| e.path.clone()).collect();
            if !names.is_empty() {
                out.push_raw(&format!("Key API: {}\n", names.join(", ")));
            }
            for e in idx
                .entries
                .iter()
                .filter(|e| e.kind == Kind::Prose && e.file.to_ascii_uppercase().starts_with("README"))
                .take(3)
            {
                if !out.push_block(&render_entry(idx, e, 1500)) {
                    break;
                }
            }
        }
        Answer {
            json: json!({"packages": ready.iter().map(|r| r.0.id()).collect::<Vec<_>>(), "tokens": est_tokens(&out.text)}),
            text: out.text,
        }
    }

    // ------------------------------------------------------------ api

    pub fn api(&self, symbol: &str, package: Option<&str>, tokens: usize) -> Result<Answer, String> {
        let tokens = tokens.clamp(200, 20000);
        let symbol = symbol.trim().trim_end_matches("()").trim();
        if symbol.is_empty() {
            return Err("symbol is required, e.g. `z.object`, `tokio::spawn`, `BaseModel.model_dump`".into());
        }
        let (deps, rest) = match package.filter(|p| !p.trim().is_empty()) {
            Some(p) => (self.select(Some(p), "")?.0, symbol.to_string()),
            None => match self.package_prefix(symbol) {
                Some((dep, rest)) => (vec![dep], rest),
                None => (self.select(None, symbol)?.0, symbol.to_string()),
            },
        };
        let segs: Vec<String> = split_symbol(&rest);
        let Some(last) = segs.last().map(|l| l.trim_end_matches('!').to_string()) else {
            // Only a package name: give the overview.
            return self.docs(
                Some(&deps.iter().map(|d| format!("{}:{}@{}", d.eco, d.name, d.version)).collect::<Vec<_>>().join(",")),
                "",
                tokens,
            );
        };
        let quals: Vec<String> = segs[..segs.len() - 1].iter().map(|s| s.to_ascii_lowercase()).collect();
        let (ready, missing) = self.resolve_all(&deps);
        if ready.is_empty() {
            return Err(missing.into_iter().map(|(_, e)| e).collect::<Vec<_>>().join("\n"));
        }
        let mut cands: Vec<(f32, usize, usize)> = Vec::new();
        let last_l = last.to_ascii_lowercase();
        for (pi, (idx, _, _)) in ready.iter().enumerate() {
            for (ei, e) in idx.entries.iter().enumerate() {
                if e.kind == Kind::Prose {
                    continue;
                }
                let name = e.name.trim_end_matches('!');
                let exact = name == last;
                if !exact && name.to_ascii_lowercase() != last_l {
                    continue;
                }
                cands.push((api_score(e, exact, &quals), pi, ei));
            }
        }
        cands.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        if cands.is_empty() {
            // Nothing by that name: fall back to a docs search scoped to the same packages.
            let pk = ready
                .iter()
                .map(|r| format!("{}:{}@{}", r.0.eco, r.0.name, r.0.version))
                .collect::<Vec<_>>()
                .join(",");
            let mut a = self.docs(Some(&pk), &segs.join(" "), tokens)?;
            a.text = format!(
                "No symbol named `{last}` in {}; closest documentation:\n\n{}",
                ready.iter().map(|r| r.0.id()).collect::<Vec<_>>().join(", "),
                a.text
            );
            return Ok(a);
        }
        let (_, bpi, bei) = cands[0];
        let (idx, dep, note) = &ready[bpi];
        let best0 = &idx.entries[bei];
        let best = resolve_alias(idx, best0);
        let mut out = Pack::new(tokens);
        out.push_raw(&format!(
            "{} · {} · {}{}\n",
            idx.id(),
            idx.eco,
            idx.source,
            if dep.from == "requested" {
                String::new()
            } else {
                format!(" · pinned in {}", dep.from)
            }
        ));
        if let Some(n) = note {
            out.push_raw(&format!("Note: {n}\n"));
        }
        if !std::ptr::eq(best, best0) {
            out.push_raw(&format!(
                "`{}` is a re-export of `{}` ({}:{}).\n",
                best0.path, best.path, best0.file, best0.line
            ));
        }
        out.push_block(&render_entry(idx, best, 6000));
        // Overloads and declaration merging: same path, other entries.
        let overloads: Vec<&Entry> = idx
            .entries
            .iter()
            .filter(|e| e.path == best.path && e.file == best.file && !std::ptr::eq(*e, best) && e.kind != Kind::Prose && e.kind != Kind::Alias)
            .take(8)
            .collect();
        if !overloads.is_empty() {
            let mut s = String::from("Other declarations:\n");
            for e in overloads {
                s.push_str(&format!("- `{}` — {} {}:{}\n", e.sig, idx.id(), e.file, e.line));
            }
            out.push_block(&s);
        }
        let mut members_json = Vec::new();
        if best.kind.is_container() {
            let sep = if idx.eco == Eco::Cargo { "::" } else { "." };
            let prefix = format!("{}{sep}", best.path);
            let members: Vec<&Entry> = idx
                .entries
                .iter()
                .filter(|e| e.path.starts_with(&prefix) && !e.path[prefix.len()..].contains(sep) && e.kind != Kind::Prose)
                .collect();
            if !members.is_empty() {
                let mut s = format!("Members of {} ({}):\n", best.path, members.len());
                for m in members.iter().take(80) {
                    let doc1 = m.doc.lines().next().unwrap_or("");
                    s.push_str(&format!(
                        "- `{}`{}{} — {}:{}\n",
                        m.sig,
                        if doc1.is_empty() { "" } else { " — " },
                        truncate(doc1, 120),
                        m.file,
                        m.line
                    ));
                    members_json.push(json!({"path": m.path, "sig": m.sig, "file": m.file, "line": m.line}));
                }
                out.push_block(&s);
            }
        }
        let others: Vec<String> = cands
            .iter()
            .skip(1)
            .filter(|(_, pi, ei)| {
                let e = &ready[*pi].0.entries[*ei];
                !(e.path == best.path && e.file == best.file) && !std::ptr::eq(e, best0)
            })
            .take(8)
            .map(|(_, pi, ei)| {
                let (ix, _, _) = &ready[*pi];
                let e = &ix.entries[*ei];
                format!("- {} `{}` — {} {}:{}", e.kind.as_str(), e.path, ix.id(), e.file, e.line)
            })
            .collect();
        if !others.is_empty() {
            out.push_block(&format!("Other matches:\n{}\n", others.join("\n")));
        }
        Ok(Answer {
            json: json!({
                "package": idx.id(), "kind": best.kind.as_str(), "path": best.path, "signature": best.sig, "doc": best.doc,
                "file": best.file, "line": best.line, "members": members_json, "tokens": est_tokens(&out.text),
            }),
            text: out.text,
        })
    }

    /// `zod.z.object` -> (zod, `z.object`); `tokio::spawn` -> (tokio, `spawn`);
    /// `@tanstack/react-query.useQuery`; `github.com/gin-gonic/gin.Context`.
    fn package_prefix(&self, symbol: &str) -> Option<(Dep, String)> {
        let mut best: Option<(usize, Dep)> = None;
        let bytes = symbol.as_bytes();
        let mut cuts: Vec<usize> = Vec::new();
        for i in 0..bytes.len() {
            if bytes[i] == b'.' || bytes[i] == b'#' || (bytes[i] == b':' && i + 1 < bytes.len() && bytes[i + 1] == b':') {
                cuts.push(i);
            }
        }
        cuts.push(symbol.len());
        for &c in &cuts {
            let cand = &symbol[..c];
            let spec = Spec::parse(cand);
            if spec.version.is_some() {
                continue;
            }
            let found = self.project.find(&spec);
            if let Some(d) = found.first() {
                if best.as_ref().is_none_or(|(bc, _)| c > *bc) {
                    best = Some((c, (*d).clone()));
                }
            }
        }
        let (c, dep) = best?;
        let rest = symbol[c..].trim_start_matches(['.', '#', ':']).to_string();
        Some((dep, rest))
    }
}

fn split_symbol(s: &str) -> Vec<String> {
    s.replace("::", ".")
        .replace('#', ".")
        .split('.')
        .map(|x| x.trim().trim_end_matches("()").to_string())
        .filter(|x| !x.is_empty())
        .collect()
}

/// Identifier-like words in a question (`z.object`, `useEffect`,
/// `model_dump`, `cookies()`); plain English words are not identifiers.
fn identifiers(q: &str) -> Vec<String> {
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
fn api_words(ready: &[ReadyPkg], q: &str) -> Vec<String> {
    let words: HashSet<String> = q
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|w| w.len() >= 4)
        .map(|w| w.to_ascii_lowercase())
        .filter(|w| !bm25::is_stop(w))
        .collect();
    let mut out = Vec::new();
    for (idx, _, _) in ready {
        for e in &idx.entries {
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
    }
    out
}

/// BM25 and embedding similarity fused (each normalized to its best hit),
/// then docs-specific boosts, then deprecation redirects ("use X instead")
/// lift the API they point to. Returns (score, package, entry), best first.
fn hybrid_rank(
    ready: &[ReadyPkg],
    refs: &[(usize, usize)],
    (bm, hb): (&Bm25, &Bm25),
    qterms: &[String],
    q: &str,
    idents: &[String],
    changes: bool,
) -> Vec<(f32, usize, usize)> {
    // (full-text BM25, heading BM25, dense), each normalized to its best hit.
    let mut fused: HashMap<usize, (f32, f32, f32)> = HashMap::new();
    let expanded = bm25::expand(qterms);
    let bm_hits = bm.search_weighted(&expanded);
    let bmax = bm_hits.first().map_or(1.0, |h| h.score).max(1e-6);
    for h in bm_hits.iter().take(1500) {
        fused.entry(h.doc as usize).or_default().0 = h.score / bmax;
    }
    let head_w = std::env::var("LOCKDOCS_HEAD_WEIGHT").ok().and_then(|v| v.parse().ok()).unwrap_or(HEAD_WEIGHT);
    let h_hits = hb.search_weighted(&expanded);
    let hmax = h_hits.first().map_or(1.0, |h| h.score).max(1e-6);
    for h in h_hits.iter().take(1500) {
        fused.entry(h.doc as usize).or_default().1 = h.score / hmax;
    }
    let dense_w = std::env::var("LOCKDOCS_DENSE_WEIGHT").ok().and_then(|v| v.parse().ok()).unwrap_or(DENSE_WEIGHT);
    let qv = if ready.iter().all(|r| !r.0.vecs.is_empty()) {
        embed::get().and_then(|m| m.embed(q))
    } else {
        None
    };
    if let Some(qv) = &qv {
        let mut sims: Vec<(usize, f32)> = refs
            .par_iter()
            .enumerate()
            .map(|(i, &(pi, ei))| (i, embed::cosine(qv, &ready[pi].0.vecs[ei])))
            .collect();
        sims.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let cmax = sims.first().map_or(1.0, |s| s.1);
        let floor = sims.get(300).map_or(0.0, |s| s.1);
        if std::env::var("LOCKDOCS_DEBUG").is_ok() {
            eprintln!("dense cmax={cmax:.3} floor={floor:.3}");
            for (rank, (i, c)) in sims.iter().enumerate() {
                let (pi, ei) = refs[*i];
                if rank < 10 {
                    eprintln!("dense rank {rank} cos={c:.3} {}", ready[pi].0.entries[ei].path);
                }
            }
        }
        for (i, c) in sims.iter().take(300) {
            fused.entry(*i).or_default().2 = ((c - floor) / (cmax - floor).max(1e-6)).max(0.0);
        }
    }
    let w = if qv.is_some() { dense_w } else { 0.0 };
    // Query words that are rare in these packages; common ones ("futures" in
    // tokio) say little about which entry is meant.
    let n = refs.len().max(1) as f32;
    let rare: Vec<String> = qterms.iter().filter(|t| (bm.df(t) as f32) < n * 0.03).cloned().collect();
    let debug = std::env::var("LOCKDOCS_DEBUG").is_ok();
    let mut scored: Vec<(f32, usize, usize)> = fused
        .into_iter()
        .map(|(i, (b, h, d))| {
            let (pi, ei) = refs[i];
            let e = &ready[pi].0.entries[ei];
            let lexical = (1.0 - head_w) * b + head_w * h;
            let major = major_of(&ready[pi].0.version);
            let s = ((1.0 - w) * lexical + w * d) * boost(e, idents, changes, major) * name_hit(e, &rare);
            if debug && s > 0.3 {
                eprintln!(
                    "{s:.3} bm={b:.3} head={h:.3} dense={d:.3} boost={:.2} name={:.2} {}",
                    boost(e, idents, changes, major),
                    name_hit(e, &rare),
                    e.path
                );
            }
            (s, pi, ei)
        })
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    // Deprecation redirects among the top results.
    let mut lifted: Vec<(f32, usize, String)> = Vec::new();
    for (s, pi, ei) in scored.iter().take(30) {
        let e = &ready[*pi].0.entries[*ei];
        for t in redirects(&format!("{}\n{}", e.sig, e.doc)) {
            if t != e.name {
                lifted.push((*s * 0.95, *pi, t));
            }
        }
    }
    if !lifted.is_empty() {
        let mut have: HashMap<(usize, usize), usize> = scored.iter().enumerate().map(|(i, (_, pi, ei))| ((*pi, *ei), i)).collect();
        for (s, pi, name) in lifted {
            for (ei, e) in ready[pi].0.entries.iter().enumerate() {
                if e.kind == Kind::Prose || e.kind == Kind::Alias || e.name != name || e.legacy {
                    continue;
                }
                match have.get(&(pi, ei)) {
                    Some(&i) => scored[i].0 = scored[i].0.max(s),
                    None => {
                        have.insert((pi, ei), scored.len());
                        scored.push((s, pi, ei));
                    }
                }
            }
        }
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    }
    scored
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
fn heading_terms(h: &str) -> Vec<String> {
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

/// Entries whose name (or section heading) carries a query word are about it.
fn name_hit(e: &Entry, qterms: &[String]) -> f32 {
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

fn major_of(version: &str) -> u64 {
    version.trim_start_matches('v').split('.').next().and_then(|m| m.parse().ok()).unwrap_or(0)
}

/// A migration or upgrade guide to a major older than the pinned one
/// ("Migrating to v6.0.0" in ESLint 9, "Upgrade to Prisma ORM 4" in Prisma 6):
/// history, not how things work now.
fn old_upgrade_guide(e: &Entry, major: u64) -> bool {
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
fn deprecated_page(e: &Entry) -> bool {
    e.name.split(" › ").next().unwrap_or("").to_ascii_lowercase().contains("(deprecated)")
}

fn boost(e: &Entry, idents: &[String], changes: bool, major: u64) -> f32 {
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

fn resolve_alias<'a>(idx: &'a PackageIndex, e: &'a Entry) -> &'a Entry {
    let Some(target) = &e.alias_of else { return e };
    idx.entries
        .iter()
        .filter(|x| &x.name == target && x.kind != Kind::Alias && x.kind != Kind::Prose)
        .max_by_key(|x| (x.file == e.file, !x.doc.is_empty()))
        .unwrap_or(e)
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    let t: String = s.chars().take(n).collect();
    format!("{t}…")
}

/// One result block with its citation.
pub fn render_entry(idx: &PackageIndex, e: &Entry, max_doc: usize) -> String {
    let cite = format!("{} {}:{}", idx.id(), e.file, e.line);
    if e.kind == Kind::Prose {
        return format!("### {} — {}\n{}\n", e.name, cite, truncate(&e.doc, max_doc));
    }
    let mut s = format!("### {} ({}) — {}\n```{}\n{}\n```\n", e.path, e.kind.as_str(), cite, lang(idx.eco), e.sig);
    if !e.doc.is_empty() {
        s.push_str(&truncate(&e.doc, max_doc));
        s.push('\n');
    }
    s
}

/// The first paragraph of text of a section, and the short list or code
/// block right after it (what the section offers). Tag-only lines
/// (`<Deprecated>`) are skipped.
fn lead(doc: &str) -> String {
    let tag_only = |l: &str| l.starts_with('<') && l.ends_with('>');
    let mut lines = doc.lines().map(str::trim_end).peekable();
    let mut para: Vec<&str> = Vec::new();
    for l in lines.by_ref() {
        let t = l.trim();
        if t.starts_with("```") || t.starts_with("~~~") {
            return String::new();
        }
        if t.is_empty() || tag_only(t) {
            if para.is_empty() {
                continue;
            }
            break;
        }
        para.push(t);
    }
    let mut out = truncate(&para.join(" "), 320);
    while lines.peek().is_some_and(|l| l.trim().is_empty()) {
        lines.next();
    }
    let bullet = |l: &str| {
        let t = l.trim_start();
        t.starts_with("* ") || t.starts_with("- ") || t.split_once(". ").is_some_and(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
    };
    if lines.peek().is_some_and(|l| bullet(l)) {
        let mut list = String::new();
        for l in lines.by_ref() {
            if l.trim().is_empty() {
                break;
            }
            let t = l.trim();
            list.push_str(if bullet(l) { "\n" } else { " " });
            list.push_str(t);
        }
        out.push_str(&truncate(&list, 400));
    } else if lines.peek().is_some_and(|l| l.trim_start().starts_with("```")) {
        let mut code = vec![lines.next().unwrap_or_default()];
        for l in lines.by_ref() {
            code.push(l);
            if l.trim_start().starts_with("```") {
                break;
            }
        }
        let code = code.join("\n");
        if code.len() <= 300 && code.matches("```").count() == 2 {
            out.push('\n');
            out.push_str(&code);
        }
    }
    out
}

/// Leads of a prose section's page and parent section, quoted, when they
/// say something the section does not (a deprecation notice on the page,
/// the setup a subsection builds on).
fn section_context(idx: &PackageIndex, e: &Entry) -> Option<String> {
    if e.kind != Kind::Prose {
        return None;
    }
    let parts: Vec<&str> = e.name.split(" › ").collect();
    if parts.len() < 2 {
        return None;
    }
    let mut wanted = vec![parts[0].to_string()];
    if parts.len() > 2 {
        wanted.push(parts[..parts.len() - 1].join(" › "));
    }
    let mut out = String::new();
    for name in wanted {
        let Some(p) = idx
            .entries
            .iter()
            .filter(|x| x.kind == Kind::Prose && x.file == e.file && x.name == name)
            .min_by_key(|x| x.line)
        else {
            continue;
        };
        let l = lead(&p.doc);
        if l.len() < 20 || e.doc.contains(&l) || out.contains(&l) {
            continue;
        }
        for line in format!("{}: {l}", p.name.rsplit(" › ").next().unwrap_or("")).lines() {
            out.push_str("> ");
            out.push_str(line);
            out.push('\n');
        }
    }
    (!out.is_empty()).then(|| out + "\n")
}

/// Token-budgeted output.
struct Pack {
    text: String,
    budget: usize,
    blocks: usize,
    full: bool,
}

impl Pack {
    fn new(budget: usize) -> Pack {
        Pack {
            text: String::new(),
            budget,
            blocks: 0,
            full: false,
        }
    }
    fn used(&self) -> usize {
        est_tokens(&self.text)
    }
    fn push_raw(&mut self, s: &str) {
        self.text.push_str(s);
    }
    fn prepend(&mut self, s: &str) {
        self.text.insert_str(0, s);
    }
    /// Add a block if it fits; a first oversize block is trimmed to fit.
    fn push_block(&mut self, b: &str) -> bool {
        if self.full {
            return false;
        }
        let need = est_tokens(b) + 1;
        let left = self.budget.saturating_sub(self.used());
        if need <= left {
            self.text.push('\n');
            self.text.push_str(b);
            self.blocks += 1;
            return true;
        }
        if left > 120 {
            // Trim to the remaining budget at a line boundary.
            let max_chars = left * 3;
            let mut cut = String::new();
            for line in b.lines() {
                if cut.len() + line.len() + 1 > max_chars {
                    break;
                }
                cut.push_str(line);
                cut.push('\n');
            }
            if cut.matches("```").count() % 2 == 1 {
                cut.push_str("```\n");
            }
            if cut.lines().count() > 1 {
                self.text.push('\n');
                self.text.push_str(&cut);
                self.text.push_str("[…truncated to fit the token budget]\n");
                self.blocks += 1;
            }
        }
        self.full = true;
        false
    }
}

/// Where the engine keeps its per-root state for long-running servers.
#[derive(Default)]
pub struct Workspace {
    engines: Mutex<HashMap<PathBuf, (u64, Arc<Engine>)>>,
}

fn lock_stamp(root: &Path) -> u64 {
    // Re-read the project when any lockfile in the root changes.
    let mut h: u64 = 0;
    if let Ok(rd) = std::fs::read_dir(root) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if crate::lockfile::LOCKFILES.iter().any(|(f, _)| *f == n) || crate::lockfile::is_requirements(&n) || n == "package.json" || n == "pyproject.toml" {
                if let Ok(m) = e.metadata() {
                    let t = m
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map_or(0, |d| d.as_nanos() as u64);
                    h = h.wrapping_mul(31).wrapping_add(t ^ m.len());
                }
            }
        }
    }
    h
}

impl Workspace {
    pub fn engine(&self, root: &Path, opts: &Options) -> Arc<Engine> {
        let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        let stamp = lock_stamp(&root);
        let mut m = self.engines.lock().unwrap();
        if let Some((s, e)) = m.get(&root) {
            if *s == stamp {
                return e.clone();
            }
        }
        let e = Arc::new(Engine::new(&root, opts.clone()));
        m.insert(root, (stamp, e.clone()));
        e
    }
}

/// Normalized dependency key, exposed for callers that dedupe.
pub fn dep_key(d: &Dep) -> String {
    format!("{}:{}", d.eco, norm_name(d.eco, &d.name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_upgrade_guides_and_deprecated_pages() {
        let e = |name: &str, file: &str| Entry {
            kind: Kind::Prose,
            name: name.into(),
            path: name.into(),
            file: file.into(),
            line: 1,
            sig: String::new(),
            doc: String::new(),
            alias_of: None,
            legacy: false,
        };
        assert!(old_upgrade_guide(
            &e("Migrating to v6.0.0 › x", "upstream:docs/src/use/migrating-to-6.0.0.md"),
            9
        ));
        assert!(old_upgrade_guide(
            &e("Upgrade to Prisma ORM 4 › x", "upstream:docs/700-upgrading-to-prisma-4.mdx"),
            6
        ));
        assert!(!old_upgrade_guide(
            &e("Upgrade to Prisma ORM 6 › x", "upstream:docs/500-upgrading-to-prisma-6.mdx"),
            6
        ));
        assert!(!old_upgrade_guide(&e("Migration Guide › x", "upstream:docs/migration.md"), 2));
        assert!(deprecated_page(&e("Configure Language Options (Deprecated) › Globals", "x.md")));
        assert!(!heading_terms("Seeding with TypeScript or JavaScript").contains(&"type".to_string()));
        assert!(heading_terms("useActionState reference").contains(&"action".to_string()));
        let l = lead("Intro line.\n\n```css\n@import \"tailwindcss\";\n\n@custom-variant dark (&:where(.dark, .dark *));\n```\n\nMore.");
        assert!(l.contains("@custom-variant") && l.ends_with("```"), "{l}");
        let l = lead("Three helpers:\n\n* **`parse_obj`**: from a dict\n  more.\n* `parse_raw`\n\nNext.");
        assert!(l.contains("parse_obj") && l.contains("parse_raw") && !l.contains("Next"), "{l}");
        assert!(lead("<Deprecated>\n\nIn React 19, it is no longer necessary.\n\n</Deprecated>").starts_with("In React 19"));
    }

    #[test]
    fn deprecation_redirects() {
        assert_eq!(redirects("Consider `z.strictObject(A.shape)` instead"), vec!["strictObject"]);
        assert_eq!(
            redirects("The `parse_obj` method is deprecated; use `model_validate` instead."),
            vec!["model_validate"]
        );
        assert_eq!(redirects("@deprecated Use .extend instead"), vec!["extend"]);
        assert_eq!(redirects("You can use this to parse data"), Vec::<String>::new());
        assert_eq!(redirects("Renamed to OptionalFromRequestParts."), vec!["OptionalFromRequestParts"]);
    }
}
