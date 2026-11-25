//! Render package summaries and pack ranked evidence into the answer budget.

use super::super::entries::{private_path, resolve_alias};
use super::super::packages::ReadyPkg;
use super::super::ranking::{LexicalAnchor, Scored};
use super::super::render::{provenance, render_entry, section_context, truncate, Pack};
use super::super::Answer;
use crate::extract::{Entry, Kind};
use crate::{est_tokens, Dep};
use serde_json::json;
use std::collections::{BTreeSet, HashSet};

/// Results scoring below this fraction of the best are left out (after 3).
const RELEVANCE_FLOOR: f32 = 0.3;

pub(super) fn overview(ready: &[ReadyPkg], scopes: &[BTreeSet<usize>], tokens: usize) -> Answer {
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

pub(super) fn search_header(ready: &[ReadyPkg], missing: &[(Dep, String)], broad: bool, tokens: usize) -> String {
    let mut header = String::new();
    for (idx, dep, note) in ready {
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
        header = broad_header(ready, missing, tokens);
    }
    for (d, e) in missing {
        if !broad {
            header.push_str(&format!("Skipped {}: {e}\n", d.id()));
        }
    }
    header
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

pub(super) fn pack_hits(
    out: &mut Pack,
    ready: &[ReadyPkg],
    scopes: &[BTreeSet<usize>],
    mut scored: Vec<Scored>,
    lexical: Option<LexicalAnchor>,
    public_calls: &HashSet<(usize, usize)>,
) -> (Vec<serde_json::Value>, Vec<String>) {
    let anchor = prioritize_lexical(&mut scored, lexical, public_calls);
    let mut seen: HashSet<(usize, String, u32)> = HashSet::new();
    let mut hits_json = Vec::new();
    let mut used_pkgs: Vec<String> = Vec::new();
    let top = scored.first().map_or(0.0, |s| s.0);
    for (rank, (score, pi, ei)) in scored.into_iter().enumerate() {
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
        let primary = out.blocks == 0;
        let ctx = if primary { section_context(idx, e, &scopes[pi]) } else { None };
        let max_doc = if primary {
            2000 - ctx.as_ref().map_or(0, |c| c.len().min(800) / 2)
        } else if anchor == Some(rank) {
            1400
        } else {
            800
        };
        let mut block = render_entry(idx, e, max_doc);
        if let (Some(ctx), Some(nl)) = (ctx, block.find('\n')) {
            block.insert_str(nl + 1, &ctx);
        }
        let added = if let Some(slot) = anchor.filter(|&slot| rank <= slot) {
            // Leave room for the other opening hits. Unused space remains
            // available to later hits, so short evidence costs no quota.
            let limit = if rank < slot {
                out.remaining() * 2 / (slot - rank + 2)
            } else {
                out.remaining()
            };
            out.push_excerpt(&block, limit)
        } else if out.push_block(&block) {
            true
        } else {
            break;
        };
        if !added {
            continue;
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
    (hits_json, used_pkgs)
}

/// Keep the best fused result (or explicit public call) first. Give a missing
/// exact match the second opening slot; for weaker lexical evidence keep the
/// first two fused results ahead of it.
fn prioritize_lexical(scored: &mut [Scored], anchor: Option<LexicalAnchor>, public_calls: &HashSet<(usize, usize)>) -> Option<usize> {
    let anchor = anchor?;
    let rank = scored.iter().position(|&(_, pi, ei)| (pi, ei) == anchor.entry)?;
    let slot = if anchor.literal { 1 } else { 2 };
    if rank < slot || (rank == slot && !anchor.literal) || (rank > slot && public_calls.contains(&(scored[slot].1, scored[slot].2))) {
        return None;
    }
    scored[slot..=rank].rotate_right(1);
    Some(slot)
}
