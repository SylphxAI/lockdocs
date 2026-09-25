//! BM25 over entries (adapted from repomap's engine), with a light English
//! stemmer and query stopwords so questions in plain language rank well.

use std::collections::HashMap;

const K1: f32 = 1.2;
const B: f32 = 0.75;

const STOP: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "by", "can", "do", "does", "for", "from", "get", "how", "i", "if", "in", "is", "it", "me", "my", "of", "on",
    "or", "should", "that", "the", "this", "to", "use", "using", "what", "when", "where", "which", "with", "you", "your", "we", "our", "way", "want", "need",
    "there", "their", "into", "vs", "versus", "about", "example", "show",
];

/// Conservative suffix stripping: plural, -ing, -ed, final -e.
pub fn stem(t: &str) -> String {
    let mut s = stem0(t);
    if s.len() > 4 && s.ends_with('e') && s.is_ascii() {
        s.pop();
    }
    s
}

fn stem0(t: &str) -> String {
    let n = t.len();
    if n <= 3 || !t.is_ascii() {
        return t.to_string();
    }
    if n > 5 && t.ends_with("ies") {
        return format!("{}y", &t[..n - 3]);
    }
    if n > 4 && (t.ends_with("sses") || t.ends_with("shes") || t.ends_with("ches") || t.ends_with("xes")) {
        return t[..n - 2].to_string();
    }
    if t.ends_with('s') && !t.ends_with("ss") && !t.ends_with("us") && !t.ends_with("is") {
        return t[..n - 1].to_string();
    }
    if n > 6 && t.ends_with("ing") {
        return t[..n - 3].to_string();
    }
    if n > 5 && t.ends_with("ed") && !t.ends_with("eed") {
        return t[..n - 2].to_string();
    }
    t.to_string()
}

/// Index-side terms for a text.
pub fn terms(text: &str) -> Vec<String> {
    crate::tokenize::tokenize(text).into_iter().map(|t| stem(&t)).collect()
}

/// Query-side terms: stopwords dropped, deduplicated.
pub fn query_terms(q: &str) -> Vec<String> {
    let mut v: Vec<String> = crate::tokenize::tokenize(q)
        .into_iter()
        .filter(|t| !STOP.contains(&t.as_str()))
        .map(|t| stem(&t))
        .collect();
    v.sort();
    v.dedup();
    v
}

/// Programming synonyms: docs and questions often use different words for
/// the same thing ("convert to dict" vs `model_dump`). Stemmed forms.
const SYNONYMS: &[(&str, &[&str])] = &[
    ("dict", &["dictionary", "dump", "mapping"]),
    ("dictionary", &["dict"]),
    ("convert", &["dump", "serializ", "transform", "cast"]),
    ("serializ", &["dump", "json", "encod"]),
    ("deserializ", &["pars", "load", "decod", "validat"]),
    ("pars", &["validat", "decod", "load"]),
    ("validat", &["pars", "check"]),
    ("remov", &["delet", "drop"]),
    ("delet", &["remov", "drop"]),
    ("creat", &["new", "build", "mak", "init"]),
    ("async", &["await", "promis", "futur"]),
    ("error", &["except", "err", "fail"]),
    ("except", &["error"]),
    ("config", &["setting", "option", "configur"]),
    ("setting", &["config"]),
    ("option", &["config", "param"]),
    ("fetch", &["request", "get"]),
    ("request", &["fetch"]),
    ("redirect", &["navigat"]),
    ("navigat", &["redirect", "router"]),
    ("spawn", &["task"]),
    ("route", &["router", "path"]),
    ("middleware", &["layer"]),
    ("layer", &["middleware"]),
    ("state", &["extension"]),
    ("unknown", &["extra", "strict"]),
    ("extra", &["unknown"]),
];

/// Query terms plus weighted synonyms not already present.
pub fn expand(terms: &[String]) -> Vec<(String, f32)> {
    let mut out: Vec<(String, f32)> = terms.iter().map(|t| (t.clone(), 1.0)).collect();
    for t in terms {
        if let Some((_, syns)) = SYNONYMS.iter().find(|(k, _)| k == t) {
            for s in *syns {
                if !out.iter().any(|(x, _)| x == s) {
                    out.push((s.to_string(), 0.35));
                }
            }
        }
    }
    out
}

/// Weighted term frequencies for one document.
pub fn tf(parts: &[(&str, u16)]) -> (Vec<(String, u16)>, u32) {
    let mut m: HashMap<String, u16> = HashMap::new();
    let mut len = 0u32;
    for (text, w) in parts {
        // Cap very long bodies; the head carries the signal.
        let text = if text.len() > 16 * 1024 { &text[..floor_char(text, 16 * 1024)] } else { text };
        for t in terms(text) {
            let e = m.entry(t).or_insert(0);
            *e = e.saturating_add(*w);
            len += *w as u32;
        }
    }
    let mut v: Vec<(String, u16)> = m.into_iter().collect();
    v.sort();
    (v, len.max(1))
}

fn floor_char(s: &str, mut i: usize) -> usize {
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

#[derive(Default)]
pub struct Bm25 {
    postings: HashMap<String, Vec<(u32, u16)>>,
    lens: Vec<u32>,
    avg: f32,
}

pub struct Hit {
    pub doc: u32,
    pub score: f32,
    /// Distinct query terms matched.
    pub matched: usize,
}

impl Bm25 {
    pub fn build<'a>(docs: impl Iterator<Item = (&'a [(String, u16)], u32)>) -> Bm25 {
        let mut postings: HashMap<String, Vec<(u32, u16)>> = HashMap::new();
        let mut lens = Vec::new();
        let mut total = 0u64;
        for (terms, len) in docs {
            let id = lens.len() as u32;
            lens.push(len);
            total += len as u64;
            for (t, f) in terms {
                match postings.get_mut(t) {
                    Some(v) => v.push((id, *f)),
                    None => {
                        postings.insert(t.clone(), vec![(id, *f)]);
                    }
                }
            }
        }
        let avg = if lens.is_empty() { 1.0 } else { total as f32 / lens.len() as f32 };
        Bm25 { postings, lens, avg }
    }

    pub fn len(&self) -> usize {
        self.lens.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lens.is_empty()
    }

    pub fn contains(&self, term: &str) -> bool {
        self.postings.contains_key(term)
    }

    pub fn search(&self, qterms: &[String]) -> Vec<Hit> {
        let weighted: Vec<(String, f32)> = qterms.iter().map(|t| (t.clone(), 1.0)).collect();
        self.search_weighted(&weighted)
    }

    /// BM25 with per-term weights; coverage counts only full-weight terms.
    pub fn search_weighted(&self, qterms: &[(String, f32)]) -> Vec<Hit> {
        let n = self.lens.len() as f32;
        let mut scores: HashMap<u32, (f32, usize)> = HashMap::new();
        for (t, w) in qterms {
            let Some(list) = self.postings.get(t) else { continue };
            let df = list.len() as f32;
            let idf = ((n - df + 0.5) / (df + 0.5) + 1.0).ln();
            for &(d, tf) in list {
                let tf = tf as f32;
                let norm = tf * (K1 + 1.0) / (tf + K1 * (1.0 - B + B * self.lens[d as usize] as f32 / self.avg));
                let e = scores.entry(d).or_insert((0.0, 0));
                e.0 += idf * norm * w;
                if *w >= 1.0 {
                    e.1 += 1;
                }
            }
        }
        let q = qterms.iter().filter(|(_, w)| *w >= 1.0).count().max(1) as f32;
        let mut hits: Vec<Hit> = scores
            .into_iter()
            .map(|(doc, (s, m))| Hit {
                doc,
                score: s * (0.4 + m as f32 / q),
                matched: m,
            })
            .collect();
        hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal).then(a.doc.cmp(&b.doc)));
        hits
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stems_and_ranks() {
        assert_eq!(stem("schemas"), "schema");
        assert_eq!(stem("validating"), "validat");
        assert_eq!(stem("validated"), "validat");
        assert_eq!(stem("validate"), "validat");
        assert_eq!(stem("parses"), stem("parse"));
        assert_eq!(stem("class"), "class");
        let a = tf(&[("object schema strict", 1)]);
        let b = tf(&[("string schema", 1)]);
        let idx = Bm25::build([(a.0.as_slice(), a.1), (b.0.as_slice(), b.1)].into_iter());
        let hits = idx.search(&query_terms("how do I make strict object schemas"));
        assert_eq!(hits[0].doc, 0);
    }
}
