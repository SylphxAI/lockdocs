//! Documentation relevance: keyword and embedding ranking, with entry boosts.

use super::entries::{private_path, split_symbol, tooling_path};
use super::packages::ReadyPkg;
use crate::bm25::{self, Bm25};
use crate::embed;
use crate::extract::{Entry, Kind};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};

/// Share of the fused rank score from the embedding ranking; the rest is the
/// keyword ranking. Small on purpose: measured on the 105-question benchmark,
/// anything above ~0.15 lets the embedding outvote exact API-name matches.
const DENSE_WEIGHT: f32 = 0.1;
/// Reciprocal-rank constant: `1 / (RRF_K + rank)`.
const RRF_K: f32 = 10.0;
/// Share of the embedding in the score mix used when upstream docs are indexed.
const DOCS_DENSE_WEIGHT: f32 = 0.45;
/// Embedding candidates considered per query.
const DENSE_POOL: usize = 300;
/// The keyword leader is final when it scores at least this many times the
/// runner-up (after boosts); the embedding is not even looked at.
const DECISIVE_RATIO: f32 = 2.0;
/// Share of the keyword score from headings and first sentences.
const HEAD_WEIGHT: f32 = 0.25;

/// Identifier-like words in a question (`z.object`, `useEffect`,
/// `model_dump`, `cookies()`); plain English words are not identifiers.
pub(super) fn identifiers(q: &str) -> Vec<String> {
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
pub(super) fn api_words(ready: &[ReadyPkg], refs: &[(usize, usize)], q: &str) -> Vec<String> {
    let words: HashSet<String> = q
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|w| w.len() >= 4)
        .map(|w| w.to_ascii_lowercase())
        .filter(|w| !bm25::is_stop(w))
        .collect();
    let mut out = Vec::new();
    for &(pi, ei) in refs {
        let e = &ready[pi].0.entries[ei];
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
    out
}

pub(super) type Scored = (f32, usize, usize);

pub(super) struct LexicalAnchor {
    pub entry: (usize, usize),
    pub literal: bool,
}

/// Keyword ranking (BM25 over text and headings, then docs-specific boosts)
/// first; when no entry clearly leads, the embedding ranking is fused in by
/// weighted reciprocal rank. Then deprecation redirects ("use X instead")
/// lift the API they point to. The second list carries the leading lexical
/// evidence through score fusion for budgeted packing.
pub(super) fn hybrid_rank(
    ready: &[ReadyPkg],
    refs: &[(usize, usize)],
    (bm, hb): (&Bm25, &Bm25),
    qterms: &[String],
    idents: &[String],
    changes: bool,
    embed_query: impl FnOnce() -> Option<Vec<f32>>,
) -> (Vec<Scored>, Option<LexicalAnchor>) {
    // (full-text BM25, heading BM25), each normalized to its best hit.
    let mut fused: HashMap<usize, (f32, f32)> = HashMap::new();
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
    let dense_w: f32 = std::env::var("LOCKDOCS_DENSE_WEIGHT").ok().and_then(|v| v.parse().ok()).unwrap_or(DENSE_WEIGHT);
    // Query words that are rare in these packages; common ones ("futures" in
    // tokio) say little about which entry is meant.
    let n = refs.len().max(1) as f32;
    let rare: Vec<String> = qterms.iter().filter(|t| (bm.df(t) as f32) < n * 0.03).cloned().collect();
    let weight = |i: usize| {
        let (pi, ei) = refs[i];
        let e = &ready[pi].0.entries[ei];
        boost(e, idents, changes, major_of(&ready[pi].0.version)) * name_hit(e, &rare)
    };
    // Keyword ranking first, with the docs-specific boosts: this is what the
    // answer is built on.
    let mut lex: Vec<(usize, f32)> = fused.iter().map(|(&i, &(b, h))| (i, ((1.0 - head_w) * b + head_w * h) * weight(i))).collect();
    lex.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0)));
    // A clear leader needs no second opinion: skip the embedding entirely.
    let decisive = match (lex.first(), lex.get(1)) {
        (Some(t), Some(s)) => t.1 >= s.1 * DECISIVE_RATIO,
        (Some(_), None) => true,
        _ => false,
    };
    let debug = std::env::var("LOCKDOCS_DEBUG").is_ok();
    let qv = if !decisive && dense_w > 0.0 && ready.iter().all(|r| !r.0.vecs.is_empty()) {
        embed_query()
    } else {
        None
    };
    // Embedding rank of each entry among the 300 nearest to the query.
    let mut dense_rank: HashMap<usize, usize> = HashMap::new();
    let mut dense_cos: HashMap<usize, f32> = HashMap::new();
    if let Some(qv) = &qv {
        let mut sims: Vec<(usize, f32)> = refs
            .par_iter()
            .enumerate()
            .map(|(i, &(pi, ei))| (i, embed::cosine(qv, &ready[pi].0.vecs[ei])))
            .collect();
        sims.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0)));
        for (r, (i, _)) in sims.iter().take(DENSE_POOL).enumerate() {
            dense_rank.insert(*i, r);
        }
        let cmax = sims.first().map_or(1.0, |s| s.1);
        let floor = sims.get(300).map_or(0.0, |s| s.1);
        for (i, c) in sims.iter().take(300) {
            dense_cos.insert(*i, ((c - floor) / (cmax - floor).max(1e-6)).max(0.0));
        }
        if debug {
            for (i, c) in sims.iter().take(10) {
                let (pi, ei) = refs[*i];
                eprintln!("dense cos={c:.3} {}", ready[pi].0.entries[ei].path);
            }
        }
    }
    if debug {
        eprintln!("decisive={decisive} dense={}", qv.is_some());
    }
    // Package files only (API names and signatures): reciprocal-rank fusion
    // weighted towards the keyword ranking, so the embedding breaks near-ties
    // and rescues paraphrases but cannot outvote an exact name match. With
    // upstream docs in the index the answers are prose pages, where the
    // embedding is as reliable as keywords and score magnitudes matter (a
    // rank-only fusion buried select.md under Graceful Shutdown): there each
    // normalized score is mixed, as before. Scores are scaled so the best is 1.
    let with_docs = ready.iter().any(|r| r.0.upstream.is_some());
    let rrf = |r: usize| 1.0 / (RRF_K + r as f32);
    // RRF already keeps the strongest package-file lexical hits near the top.
    // Score fusion can bury them: a short leading keyword window is carried
    // independently of the dense score, with literal API references first.
    let lexical = if qv.is_some() && with_docs {
        lexical_anchor(ready, refs, &lex, idents, changes)
    } else {
        None
    };
    let mut scored: Vec<Scored> = if qv.is_some() && with_docs {
        let mut ids: HashSet<usize> = lex.iter().map(|l| l.0).collect();
        ids.extend(dense_cos.keys().copied());
        let lexm: HashMap<usize, f32> = fused.iter().map(|(&i, &(b, h))| (i, (1.0 - head_w) * b + head_w * h)).collect();
        ids.into_iter()
            .map(|i| {
                let (pi, ei) = refs[i];
                let l = lexm.get(&i).copied().unwrap_or(0.0);
                let d = dense_cos.get(&i).copied().unwrap_or(0.0);
                (((1.0 - DOCS_DENSE_WEIGHT) * l + DOCS_DENSE_WEIGHT * d) * weight(i), pi, ei)
            })
            .collect()
    } else if qv.is_some() {
        let mut ids: HashSet<usize> = lex.iter().map(|l| l.0).collect();
        ids.extend(dense_rank.keys().copied());
        let lex_rank: HashMap<usize, usize> = lex.iter().enumerate().map(|(r, l)| (l.0, r)).collect();
        ids.into_iter()
            .map(|i| {
                let l = lex_rank.get(&i).map_or(0.0, |&r| rrf(r));
                let d = dense_rank.get(&i).map_or(0.0, |&r| rrf(r));
                let (pi, ei) = refs[i];
                ((1.0 - dense_w) * l + dense_w * d, pi, ei)
            })
            .collect()
    } else {
        lex.iter().map(|&(i, s)| (s, refs[i].0, refs[i].1)).collect()
    };
    if qv.is_some() {
        let top = scored.iter().map(|s| s.0).fold(0.0f32, f32::max).max(1e-9);
        scored.iter_mut().for_each(|s| s.0 /= top);
    }
    let sort = |scored: &mut Vec<Scored>| {
        scored.sort_by(|a, b| {
            b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal).then_with(|| {
                if qv.is_some() {
                    (a.1, a.2).cmp(&(b.1, b.2))
                } else {
                    std::cmp::Ordering::Equal
                }
            })
        });
    };
    sort(&mut scored);
    if debug {
        for (s, pi, ei) in scored.iter().take(10) {
            eprintln!("{s:.3} {}", ready[*pi].0.entries[*ei].path);
        }
    }
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
            for &(_, ei) in refs.iter().filter(|(p, _)| *p == pi) {
                let e = &ready[pi].0.entries[ei];
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
        sort(&mut scored);
    }
    (scored, lexical)
}

fn lexical_anchor(ready: &[ReadyPkg], refs: &[(usize, usize)], lex: &[(usize, f32)], idents: &[String], changes: bool) -> Option<LexicalAnchor> {
    let &(_, best) = lex.first()?;
    let mut candidates = lex
        .iter()
        .take_while(|(_, score)| *score * DECISIVE_RATIO >= best)
        .map(|&(i, _)| refs[i])
        .filter(|&(pi, ei)| {
            let idx = &ready[pi].0;
            let e = &idx.entries[ei];
            !e.legacy
                && e.kind != Kind::Alias
                && !private_path(e)
                && !tooling_path(e)
                && !deprecated_page(e)
                && !old_upgrade_guide(e, major_of(&idx.version))
                && (changes || !is_changelog(e) && !e.file.split('/').any(|part| matches!(part, "releases" | "changelogs")))
        });
    let literal = |&(pi, ei): &(usize, usize)| {
        let e = &ready[pi].0.entries[ei];
        let name = e
            .name
            .rsplit(" › ")
            .next()
            .unwrap_or(&e.name)
            .trim()
            .trim_end_matches("()")
            .trim_end_matches('!');
        let named = matches!(e.kind, Kind::Prose | Kind::Class | Kind::Function | Kind::Macro)
            || name.contains('_')
            || name.chars().skip(1).any(|c| c.is_ascii_uppercase());
        idents
            .iter()
            .any(|i| named && name.eq_ignore_ascii_case(i) || (i.contains(['.', ':']) && e.path.eq_ignore_ascii_case(i)))
    };
    let first = candidates.next()?;
    if let Some(entry) = std::iter::once(first).chain(candidates).find(literal) {
        Some(LexicalAnchor { entry, literal: true })
    } else {
        Some(LexicalAnchor { entry: first, literal: false })
    }
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
pub(super) fn heading_terms(h: &str) -> Vec<String> {
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
pub(super) fn old_upgrade_guide(e: &Entry, major: u64) -> bool {
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
pub(super) fn deprecated_page(e: &Entry) -> bool {
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

pub(super) fn change_intent(q: &str) -> bool {
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
