//! List pinned dependencies and select packages for queries.

use super::super::{Answer, Engine};
use crate::locate::{self, Source};
use crate::project::Spec;
use crate::{fetch, Dep, Eco};
use rayon::prelude::*;
use serde_json::json;
use std::collections::HashSet;

/// Cross-dependency searches index at most this many direct dependencies.
const MAX_PACKAGES: usize = 80;

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
    pub(super) fn deps_for(&self, spec_str: &str) -> Result<Vec<Dep>, String> {
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

    pub(in crate::query) fn select(&self, package: Option<&str>, query: &str) -> Result<(Vec<Dep>, bool), String> {
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
}
