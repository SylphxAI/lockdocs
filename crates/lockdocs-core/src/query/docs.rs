//! Documentation search, import applicability, and package overviews.

mod answer;
mod scope;

#[cfg(test)]
mod tests;

use super::ranking::{api_words, change_intent, hybrid_rank, identifiers};
use super::render::{provenance, Pack};
use super::{Answer, Engine};
use crate::bm25::{self, Bm25};
use crate::extract::Kind;
use crate::{embed, est_tokens, upstream};
use serde_json::json;

use self::answer::{overview, pack_hits, search_header};
use self::scope::{normalize_selector, search_scope, SearchScope};

impl Engine {
    pub fn docs(&self, package: Option<&str>, query: &str, tokens: usize) -> Result<Answer, String> {
        let tokens = tokens.clamp(200, 20000);
        let (selector, imports) = normalize_selector(package, &self.project.deps);
        let (deps, broad) = self.select(selector.as_deref(), query)?;
        let (ready, missing) = self.resolve_all(&deps);
        if ready.is_empty() {
            return Err(missing.into_iter().map(|(_, e)| e).collect::<Vec<_>>().join("\n"));
        }
        let SearchScope { scopes, public_calls } = search_scope(&ready, &imports, query)?;
        let q = query.trim();
        if q.is_empty() {
            return Ok(overview(&ready, &scopes, tokens));
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
        let (mut scored, lexical) = hybrid_rank(&ready, &refs, (&bm, &hb), &qterms, &raw_idents, changes, || {
            embed::query_model().and_then(|m| m.embed(q))
        });
        // An explicitly called public API is the subject of the question.
        // Referenced type members with the same name remain supporting hits.
        scored.sort_by_key(|&(_, pi, ei)| !public_calls.contains(&(pi, ei)));
        let mut out = Pack::new(tokens);
        out.push_raw(&search_header(&ready, &missing, broad, tokens));
        let (hits_json, used_pkgs) = pack_hits(&mut out, &ready, &scopes, scored, lexical, &public_calls);
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
}
