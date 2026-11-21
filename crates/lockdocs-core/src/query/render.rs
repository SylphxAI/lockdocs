//! Token-budgeted answer text, citations, and shared source provenance.

use super::packages::ReadyPkg;
use crate::extract::{Entry, Kind};
use crate::index::PackageIndex;
use crate::{est_tokens, Eco};
use serde_json::{json, Value};

pub(super) fn provenance(ready: &[ReadyPkg]) -> Vec<Value> {
    ready
        .iter()
        .map(|(idx, dep, note)| {
            json!({
                "package": idx.id(), "requested_version": dep.version, "source": idx.source,
                "registry_fetched": idx.fetched, "upstream": idx.upstream_provenance, "upstream_label": idx.upstream, "note": note,
            })
        })
        .collect()
}

fn lang(eco: Eco) -> &'static str {
    match eco {
        Eco::Npm => "ts",
        Eco::PyPI => "python",
        Eco::Cargo => "rust",
        Eco::Go => "go",
    }
}

pub(super) fn truncate(s: &str, n: usize) -> String {
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
pub(super) fn lead(doc: &str) -> String {
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
pub(super) fn section_context(idx: &PackageIndex, e: &Entry, scope: &std::collections::BTreeSet<usize>) -> Option<String> {
    if e.kind != Kind::Prose {
        return None;
    }
    // Keep the page and closest nonempty parent, using logical ancestry rather
    // than heading text (which can repeat), and the same applicability filter
    // as retrieval. A permitted child must not quote an incompatible setup.
    let mut wanted = Vec::new();
    if let Some(line) = e.prose_parents.first() {
        wanted.push(*line);
    }
    if let Some(line) = e.prose_parents.last() {
        if !wanted.contains(line) {
            wanted.push(*line);
        }
    }
    let mut out = String::new();
    for line in wanted {
        let Some(p) = scope
            .iter()
            .map(|&i| &idx.entries[i])
            .find(|p| p.kind == Kind::Prose && p.file == e.file && p.line == line)
        else {
            continue;
        };
        let l = lead(&p.doc);
        if l.len() < 20 || e.doc.contains(&l) || out.contains(&l) {
            continue;
        }
        // The enclosing hit already cites this same file. Keep the ancestor
        // line explicit without repeating its long path in the token budget.
        for line in format!("line {}: {l}", p.line).lines() {
            out.push_str("> ");
            out.push_str(line);
            out.push('\n');
        }
    }
    (!out.is_empty()).then(|| out + "\n")
}

/// Token-budgeted output.
pub(super) struct Pack {
    pub(super) text: String,
    budget: usize,
    pub(super) blocks: usize,
    full: bool,
}

impl Pack {
    pub(super) fn new(budget: usize) -> Pack {
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
    pub(super) fn push_raw(&mut self, s: &str) {
        self.text.push_str(s);
    }
    pub(super) fn prepend(&mut self, s: &str) {
        self.text.insert_str(0, s);
    }
    /// Add a block if it fits; a first oversize block is trimmed to fit.
    pub(super) fn push_block(&mut self, b: &str) -> bool {
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
