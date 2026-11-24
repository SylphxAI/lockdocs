//! Documentation search, import applicability, and package overviews.

use super::entries::{npm_import_base, private_path, resolve_alias};
use super::packages::ReadyPkg;
use super::ranking::{api_words, change_intent, hybrid_rank, identifiers};
use super::render::{provenance, render_entry, section_context, truncate, Pack};
use super::{Answer, Engine};
use crate::bm25::{self, Bm25};
use crate::extract::{Entry, Kind};
use crate::index::PackageIndex;
use crate::project::Spec;
use crate::{est_tokens, upstream, Dep, Eco};
use serde_json::json;
use std::collections::{HashMap, HashSet};

/// Results scoring below this fraction of the best are left out (after 3).
const RELEVANCE_FLOOR: f32 = 0.3;

impl Engine {
    pub fn docs(&self, package: Option<&str>, query: &str, tokens: usize) -> Result<Answer, String> {
        let tokens = tokens.clamp(200, 20000);
        let mut imports = HashMap::new();
        let selector = package.map(|p| {
            p.split(',')
                .map(|p| {
                    let spec = Spec::parse(p.trim());
                    let base = npm_import_base(&spec.name);
                    let npm = spec.eco == Some(Eco::Npm) || spec.eco.is_none() && self.project.deps.iter().any(|d| d.eco == Eco::Npm && d.name == base);
                    if npm && base != spec.name {
                        imports.entry(base.to_string()).or_insert_with(Vec::new).push(spec.name.clone());
                        format!("npm:{base}{}", spec.version.as_ref().map(|v| format!("@{v}")).unwrap_or_default())
                    } else {
                        if npm {
                            imports.entry(base.to_string()).or_insert_with(Vec::new).push(base.to_string());
                        }
                        p.trim().to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join(",")
        });
        let (deps, broad) = self.select(selector.as_deref(), query)?;
        let (ready, missing) = self.resolve_all(&deps);
        if ready.is_empty() {
            return Err(missing.into_iter().map(|(_, e)| e).collect::<Vec<_>>().join("\n"));
        }
        let mut scopes = Vec::new();
        let mut public_calls = HashSet::new();
        for (pi, (idx, _, _)) in ready.iter().enumerate() {
            let mut requested = imports.get(&idx.name).cloned().unwrap_or_else(|| vec![idx.name.clone()]);
            // A question naming known public imports can narrow a root search,
            // but must never override an explicitly selected alternate edition.
            if requested.iter().all(|import| import == &idx.name) {
                if let Some(api) = &idx.npm_api {
                    let named: std::collections::BTreeSet<String> = api
                        .roots
                        .iter()
                        .filter(|r| r.import != idx.name && mentions_import(query, &r.import))
                        .map(|r| r.import.clone())
                        .collect();
                    if !named.is_empty() {
                        requested = named.into_iter().collect();
                    }
                }
            }
            if let Some(api) = &idx.npm_api {
                for import in &requested {
                    if import != &idx.name && !api.roots.iter().any(|r| &r.import == import) {
                        return Err(format!("`{import}` has no public declaration entrypoint in {}.", idx.id()));
                    }
                }
            }
            let mut scope = std::collections::BTreeSet::new();
            for import in requested {
                scope.extend(idx.docs_indices(Some(&import)));
                public_calls.extend(public_call_entries(idx, &import, query).into_iter().map(|ei| (pi, ei)));
            }
            scopes.push(scope);
        }
        let q = query.trim();
        if q.is_empty() {
            return Ok(self.overview(&ready, &scopes, tokens));
        }
        let qterms = bm25::query_terms(q);
        // One BM25 over the selected packages so scores compare.
        let mut refs: Vec<(usize, usize)> = Vec::new();
        for (pi, scope) in scopes.iter().enumerate() {
            for &ei in scope {
                refs.push((pi, ei));
            }
        }
        let bm = Bm25::build(refs.iter().map(|&(pi, ei)| (ready[pi].0.terms[ei].as_slice(), ready[pi].0.lens[ei])));
        let hb = Bm25::build(refs.iter().map(|&(pi, ei)| (ready[pi].0.head[ei].as_slice(), ready[pi].0.head_lens[ei])));
        let mut raw_idents = identifiers(q);
        raw_idents.extend(api_words(&ready, &refs, q));
        let changes = change_intent(q);
        let mut scored = hybrid_rank(&ready, &refs, (&bm, &hb), &qterms, q, &raw_idents, changes);
        // An explicitly called public API is the subject of the question.
        // Referenced type members with the same name remain supporting hits.
        scored.sort_by_key(|&(_, pi, ei)| !public_calls.contains(&(pi, ei)));
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
            header = broad_header(&ready, &missing, tokens);
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
            let e = idx.npm_api.as_ref().map_or_else(|| resolve_alias(idx, e), |api| api.search_alias(e));
            // The top result also carries the lead of its page and parent
            // section, within the same budget.
            let ctx = if out.blocks == 0 { section_context(idx, e, &scopes[pi]) } else { None };
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
            let mut hit = json!({"package": idx.id(), "kind": e.kind.as_str(), "path": e.path, "file": e.file, "line": e.line, "score": score});
            if !e.guide_modes.is_empty() {
                hit["guide_modes"] = json!(e.guide_modes);
            }
            if !e.prose_imports.is_empty() {
                hit["example_imports"] = json!(e.prose_imports);
            }
            hits_json.push(hit);
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
            json: json!({"query": q, "packages": ready.iter().map(|r| r.0.id()).collect::<Vec<_>>(), "hits": hits_json, "provenance": provenance(&ready), "tokens": est_tokens(&out.text)}),
            text: out.text,
        })
    }

    fn overview(&self, ready: &[ReadyPkg], scopes: &[std::collections::BTreeSet<usize>], tokens: usize) -> Answer {
        let mut out = Pack::new(tokens);
        for ((idx, dep, note), scope) in ready.iter().zip(scopes) {
            out.push_raw(&format!("{} · {} · {} · pinned in {}\n", idx.id(), idx.eco, idx.source, dep.from));
            if let Some(u) = &idx.upstream {
                out.push_raw(&format!("Upstream: {u}\n"));
            }
            if let Some(n) = note {
                out.push_raw(&format!("Note: {n}\n"));
            }
            let entries: Vec<&Entry> = scope.iter().map(|&i| &idx.entries[i]).collect();
            let symbols = entries
                .iter()
                .filter(|e| e.kind != Kind::Prose && e.kind != Kind::Alias && !private_path(e))
                .count();
            let prose = entries.len() - symbols;
            out.push_raw(&format!("Indexed {symbols} API symbols and {prose} doc sections.\n"));
            // Top-level API names, shortest paths first.
            let mut top: Vec<&Entry> = entries
                .iter()
                .copied()
                .filter(|e| e.kind != Kind::Prose && !private_path(e) && !e.legacy && !e.doc.is_empty())
                .collect();
            top.sort_by_key(|e| (e.path.matches(['.', ':']).count(), e.path.len()));
            let names: Vec<String> = top.iter().take(60).map(|e| e.path.clone()).collect();
            if !names.is_empty() {
                out.push_raw(&format!("Key API: {}\n", names.join(", ")));
            }
            for e in entries
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
            json: json!({"packages": ready.iter().map(|r| r.0.id()).collect::<Vec<_>>(), "provenance": provenance(ready), "tokens": est_tokens(&out.text)}),
            text: out.text,
        }
    }
}

/// Keep network fallbacks visible on ordinary broad MCP answers without listing
/// every dependency's long header or crowding out the actual answer.
fn broad_header(ready: &[ReadyPkg], missing: &[(Dep, String)], tokens: usize) -> String {
    let fetched = ready.iter().filter(|r| r.0.fetched).count();
    let upstream = ready.iter().filter(|r| r.0.upstream.is_some()).count();
    let notes = ready.iter().filter(|r| r.2.is_some()).count();
    let mut header = format!("Searched {} direct dependencies: {} local package sources, {fetched} registry sources, {upstream} upstream sources; {notes} fallback notes, {} unavailable.\n", ready.len(), ready.len() - fetched, missing.len());
    let max_chars = (tokens / 3).clamp(100, 400) * 3;
    for (idx, _, note) in ready {
        if let Some(note) = note {
            let row = format!("Fallback {}: {}\n", idx.id(), truncate(note, 160));
            if header.chars().count() + row.chars().count() > max_chars {
                break;
            }
            header.push_str(&row);
        }
    }
    for (dep, error) in missing {
        let row = format!("Skipped {}: {}\n", dep.id(), truncate(error, 140));
        if header.chars().count() + row.chars().count() > max_chars {
            break;
        }
        header.push_str(&row);
    }
    for (idx, _, _) in ready {
        if let Some(m) = &idx.upstream_provenance {
            let row = format!(
                "Upstream {}: {}@{} commit {}{}\n",
                idx.id(),
                m.repo,
                m.tag.as_deref().unwrap_or("none"),
                m.commit.as_deref().map(|s| &s[..s.len().min(7)]).unwrap_or("none"),
                if m.site.is_some() { " + opted-in docs site" } else { "" }
            );
            if header.chars().count() + row.chars().count() > max_chars {
                break;
            }
            header.push_str(&row);
        }
    }
    header.push_str("Focus `package` or request JSON for full provenance.\n");
    header
}

/// Exact case-sensitive module token, not a prefix or package-looking substring.
fn mentions_import(query: &str, import: &str) -> bool {
    let module_char = |c: char| c.is_alphanumeric() || matches!(c, '@' | '/' | '_' | '-' | '.');
    query.match_indices(import).any(|(i, _)| {
        let before = query[..i].chars().next_back();
        let after = &query[i + import.len()..];
        before.is_none_or(|c| !module_char(c))
            && (after.chars().next().is_none_or(|c| !module_char(c)) || after.starts_with('.') && after[1..].chars().next().is_none_or(|c| !module_char(c)))
    })
}

/// Resolve explicit calls through the selected public entrypoint, rather than
/// treating every same-named member in its transitive type graph as the API.
fn public_call_entries(idx: &PackageIndex, import: &str, query: &str) -> Vec<usize> {
    let Some(api) = &idx.npm_api else {
        return Vec::new();
    };
    let names: HashSet<&str> = api
        .entries
        .iter()
        .flat_map(|e| [e.name.as_str(), e.path.as_str()])
        .filter(|name| {
            query.match_indices(*name).any(|(i, _)| {
                query[i + name.len()..].starts_with('(')
                    && query[..i]
                        .chars()
                        .next_back()
                        .is_none_or(|c| !(c.is_alphanumeric() || matches!(c, '_' | '$' | '.' | ':' | '/' | '-' | '@')))
            })
        })
        .collect();
    let roots: Vec<_> = api.roots.iter().filter(|r| r.import == import).collect();
    let mut targets = HashSet::new();
    for name in names {
        let resolved: Vec<_> = roots.iter().map(|root| api.resolve(root, name, &api.entries)).collect();
        let Some(first) = resolved.first() else {
            continue;
        };
        if first.entries.is_empty() || resolved.iter().any(|r| r.unresolved || r.entries != first.entries) {
            continue;
        }
        for &i in &first.entries {
            let e = &api.entries[i];
            targets.insert((e.file.as_str(), e.path.as_str(), e.line));
        }
    }
    idx.entries
        .iter()
        .enumerate()
        .filter_map(|(i, e)| targets.contains(&(e.file.as_str(), e.path.as_str(), e.line)).then_some(i))
        .collect()
}
