//! Exact API lookup and version-to-version API comparison.

use super::entries::{npm_import_base, private_path, resolve_alias, split_symbol, tooling_path};
use super::packages::Resolved;
use super::render::{provenance, render_entry, truncate, Pack};
use super::{Answer, Engine};
use crate::extract::{Entry, Kind};
use crate::project::Spec;
use crate::{est_tokens, Dep, Eco};
use serde_json::json;
use std::collections::HashSet;

impl Engine {
    pub fn api(&self, symbol: &str, package: Option<&str>, tokens: usize) -> Result<Answer, String> {
        let tokens = tokens.clamp(200, 20000);
        let symbol = symbol.trim().trim_end_matches("()").trim();
        if symbol.is_empty() {
            return Err("symbol is required, e.g. `z.object`, `tokio::spawn`, `BaseModel.model_dump`".into());
        }
        let mut requested_import = None;
        let (deps, rest) = match package.filter(|p| !p.trim().is_empty()) {
            Some(p) => {
                let spec = Spec::parse(p);
                let base = npm_import_base(&spec.name);
                if spec.eco.is_none_or(|e| e == Eco::Npm) && base != spec.name && self.project.find(&spec).is_empty() {
                    let selector = format!("npm:{base}{}", spec.version.as_ref().map(|v| format!("@{v}")).unwrap_or_default());
                    requested_import = Some(spec.name);
                    (self.select(Some(&selector), "")?.0, symbol.to_string())
                } else {
                    (self.select(Some(p), "")?.0, symbol.to_string())
                }
            }
            None => match self
                .npm_import_prefix(symbol)
                .map(|(dep, import, rest)| {
                    requested_import = Some(import);
                    (dep, rest)
                })
                .or_else(|| self.package_prefix(symbol))
            {
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
        let mut applicability = Vec::new();
        let mut alternatives = Vec::new();
        let mut unresolved = false;
        let mut npm_scoped = false;
        let last_l = last.to_ascii_lowercase();
        let public_path = segs.join(".");
        for (pi, (idx, _, _)) in ready.iter().enumerate() {
            if let Some(api) = &idx.npm_api {
                npm_scoped = true;
                let import = requested_import.as_deref().unwrap_or(&idx.name);
                let mut has_root = false;
                for root in &api.roots {
                    let found = api.resolve(root, &public_path, idx.api_entries());
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
                cands.push((api_score(e, exact, &quals), pi, ei));
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
        if npm_scoped && (cands.is_empty() || unresolved || ambiguous) {
            let status = if ambiguous {
                "ambiguous"
            } else if unresolved {
                "unresolved"
            } else {
                "not_exported"
            };
            let imports: Vec<_> = ready
                .iter()
                .filter(|r| r.0.eco == Eco::Npm)
                .map(|r| requested_import.as_deref().unwrap_or(&r.0.name))
                .collect();
            let mut out = Pack::new(tokens);
            out.push_raw(&format!(
                "No verified exact public API match for `{public_path}` from `{}` ({}).\n",
                imports.join("`, `"),
                status
            ));
            out.push_block(if ambiguous {
                "Public declarations disagree across entrypoints or export conditions; no signature was selected.\n"
            } else if unresolved {
                "Import applicability could not be fully resolved from the installed declarations; no unrelated same-named symbol was substituted.\n"
            } else {
                "This symbol is not exported by the requested import entrypoint. Same-named internal declarations are not a match.\n"
            });
            for a in alternatives.iter().take(8) {
                out.push_block(&format!(
                    "Alternate import `{}` ({}): `{}` — {} {}:{}\n",
                    a["import"].as_str().unwrap_or(""),
                    a["conditions"].as_str().unwrap_or(""),
                    a["signature"].as_str().unwrap_or(""),
                    a["package"].as_str().unwrap_or(""),
                    a["file"].as_str().unwrap_or(""),
                    a["line"]
                ));
            }
            if ambiguous {
                for a in applicability.iter().take(8) {
                    out.push_block(&format!(
                        "Applicable import `{}` ({}) via {} → {}:{}\n",
                        a["import"].as_str().unwrap_or(""),
                        a["conditions"].as_str().unwrap_or(""),
                        a["entrypoint"].as_str().unwrap_or(""),
                        a["file"].as_str().unwrap_or(""),
                        a["line"]
                    ));
                }
                for (_, pi, ei) in cands.iter().take(8) {
                    let idx = &ready[*pi].0;
                    out.push_block(&render_entry(idx, &idx.api_entries()[*ei], 500));
                }
            }
            return Ok(Answer {
                json: json!({"status": status, "symbol": public_path, "imports": imports, "provenance": provenance(&ready),
                "applicability": applicability, "alternatives": alternatives, "tokens": est_tokens(&out.text)}),
                text: out.text,
            });
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
        let best0 = &idx.api_entries()[bei];
        let best = if idx.npm_api.is_some() { best0 } else { resolve_alias(idx, best0) };
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
        if let Some(u) = &idx.upstream {
            out.push_raw(&format!("Upstream: {u}\n"));
        }
        if let Some(n) = note {
            out.push_raw(&format!("Note: {n}\n"));
        }
        if !std::ptr::eq(best, best0) {
            out.push_raw(&format!(
                "`{}` is a re-export of `{}` ({}:{}).\n",
                best0.path, best.path, best0.file, best0.line
            ));
        }
        if idx.npm_api.is_some() {
            out.push_raw(&format!(
                "Public import: `{}` · symbol `{public_path}`\n",
                requested_import.as_deref().unwrap_or(&idx.name)
            ));
            for a in &applicability {
                out.push_block(&format!(
                    "Declaration entrypoint: {} ({})\n",
                    a["entrypoint"].as_str().unwrap_or(""),
                    a["conditions"].as_str().unwrap_or("")
                ));
            }
        }
        out.push_block(&render_entry(idx, best, 6000));
        // Overloads and declaration merging: same path, other entries.
        let overloads: Vec<&Entry> = idx
            .api_entries()
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
                .api_entries()
                .iter()
                .filter(|e| {
                    e.path.starts_with(&prefix)
                        && !e.path[prefix.len()..].contains(sep)
                        && e.kind != Kind::Prose
                        && (idx.npm_api.is_none() || e.file == best.file)
                })
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
                let e = &ready[*pi].0.api_entries()[*ei];
                !(e.path == best.path && e.file == best.file) && !std::ptr::eq(e, best0)
            })
            .take(8)
            .map(|(_, pi, ei)| {
                let (ix, _, _) = &ready[*pi];
                let e = &ix.api_entries()[*ei];
                format!("- {} `{}` — {} {}:{}", e.kind.as_str(), e.path, ix.id(), e.file, e.line)
            })
            .collect();
        if !others.is_empty() {
            out.push_block(&format!("Other matches:\n{}\n", others.join("\n")));
        }
        Ok(Answer {
            json: json!({
                "provenance": provenance(&ready), "package": idx.id(), "kind": best.kind.as_str(), "path": best.path, "signature": best.sig, "doc": best.doc,
                "file": best.file, "line": best.line, "members": members_json, "tokens": est_tokens(&out.text),
                "status": "exact", "import": if idx.npm_api.is_some() { Some(requested_import.as_deref().unwrap_or(&idx.name)) } else { None },
                "applicability": applicability, "alternatives": alternatives,
            }),
            text: out.text,
        })
    }

    fn npm_import_prefix(&self, symbol: &str) -> Option<(Dep, String, String)> {
        for d in &self.project.deps {
            if d.eco != Eco::Npm {
                continue;
            }
            let Some(rest) = symbol.strip_prefix(&format!("{}/", d.name)) else {
                continue;
            };
            if let Some((subpath, sym)) = rest.split_once('.') {
                return Some((d.clone(), format!("{}/{subpath}", d.name), sym.to_string()));
            }
        }
        None
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

    /// Upgrade report (lockdocs Pro; the caller checks the licence): the API
    /// difference between the pinned version of `package` and `target`,
    /// limited to symbols this project uses. The target version is read from
    /// the same sources as any other version (installed, cached, or fetched
    /// when fetching is enabled).
    pub fn upgrade_report(&self, package: &str, target: &str, tokens: usize) -> Result<Answer, String> {
        let spec = Spec::parse(package);
        let found = self.project.find(&spec);
        let dep = found
            .iter()
            .find(|d| d.direct)
            .or_else(|| found.first())
            .map(|d| (*d).clone())
            .ok_or_else(|| format!("`{package}` is not pinned in this project's lockfiles; run `resolve` to see packages."))?;
        let target = target.trim().trim_start_matches('v');
        if target.is_empty() {
            return Err("`upgrade_to` needs a target version, e.g. 15.0.0".into());
        }
        if target == dep.version.trim_start_matches('v') {
            return Err(format!("{} is already the pinned version; pick a different target.", dep.id()));
        }
        let old = match self.index(&dep) {
            Resolved::Ready(i, _, _) => i,
            Resolved::Missing(_, e) => return Err(e),
        };
        let target_dep = Dep {
            eco: dep.eco,
            name: dep.name.clone(),
            version: target.to_string(),
            direct: false,
            from: "requested".into(),
        };
        let new = match self.index(&target_dep) {
            Resolved::Ready(i, _, _) => i,
            Resolved::Missing(_, e) => {
                return Err(format!(
                    "{e}\nThe target version's files are needed to compare; add --fetch (or LOCKDOCS_FETCH=1) to download exactly {}.",
                    target_dep.id()
                ))
            }
        };
        let r = crate::upgrade::report(&self.project.root, &old, &new);
        let max = tokens.clamp(200, 20000) * 3;
        let text = if r.text.len() > max {
            format!("{}\n... truncated to the token budget; raise `tokens`.\n", truncate(&r.text, max))
        } else {
            r.text
        };
        Ok(Answer { text, json: r.json })
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
