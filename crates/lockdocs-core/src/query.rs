//! The three questions agents ask: which versions do I have (`resolve`),
//! what do the docs say about X (`docs`), and what exactly is this symbol
//! (`api`). Answers are token-budgeted and cite `package@version path:line`.

use crate::bm25::{self, Bm25};
use crate::extract::{Entry, Kind};
use crate::index::{self, PackageIndex};
use crate::locate::{self, Source};
use crate::project::{Project, Spec};
use crate::{est_tokens, fetch, norm_name, Dep, Eco};
use rayon::prelude::*;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub const DEFAULT_TOKENS: usize = 2000;
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
                    return Resolved::Ready(i.clone(), dep.clone(), note);
                }
                let idx = Arc::new(index::load_or_build(dep, &src, &self.project.root));
                self.indexes.lock().unwrap().insert(key, idx.clone());
                Resolved::Ready(idx, dep.clone(), note)
            }
            Err(e) => Resolved::Missing(dep.clone(), e),
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
        let raw_idents = identifiers(q);
        let changes = change_intent(q);
        let mut scored: Vec<(f32, usize, usize)> = bm
            .search_weighted(&bm25::expand(&qterms))
            .into_iter()
            .take(400)
            .map(|h| {
                let (pi, ei) = refs[h.doc as usize];
                let e = &ready[pi].0.entries[ei];
                (h.score * boost(e, &raw_idents, changes) * name_hit(e, &qterms), pi, ei)
            })
            .collect();
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
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
        for (score, pi, ei) in scored {
            let idx = &ready[pi].0;
            let e = &idx.entries[ei];
            if !seen.insert((pi, e.file.clone(), e.line)) {
                continue;
            }
            let e = resolve_alias(idx, e);
            let block = render_entry(idx, e, if out.blocks == 0 { 2000 } else { 800 });
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
        let Some(last) = segs.last().cloned() else {
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

/// Entries whose name (or section heading) carries a query word are about it.
fn name_hit(e: &Entry, qterms: &[String]) -> f32 {
    let label = if e.kind == Kind::Prose {
        e.name.rsplit(" › ").next().unwrap_or("")
    } else {
        e.name.as_str()
    };
    let words = bm25::terms(label);
    let n = qterms.iter().filter(|q| words.contains(q)).count();
    match n {
        0 => 1.0,
        1 => 1.5,
        _ => 1.9,
    }
}

fn boost(e: &Entry, idents: &[String], changes: bool) -> f32 {
    let mut b = 1.0;
    if e.kind == Kind::Prose {
        if is_changelog(e) {
            b *= if changes { 1.5 } else { 0.5 };
        } else if e.file.to_ascii_uppercase().starts_with("README") {
            b *= 1.15;
        }
    } else {
        let n = e.name.to_ascii_lowercase();
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
            let max_chars = left * 36 / 10;
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
