//! Token-budgeted answer text, citations, and shared source provenance.

use super::packages::ReadyPkg;
use super::{Answer, Engine};
use crate::extract::{Entry, Kind};
use crate::index::PackageIndex;
use crate::project::Spec;
use crate::{est_tokens, upstream, Eco};
use serde_json::{json, Value};
use std::collections::HashSet;

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

impl Engine {
    /// Add display-only advice without changing which evidence fits in the answer.
    pub(super) fn with_upstream_hints<'a>(&self, mut answer: Answer, packages: impl IntoIterator<Item = &'a ReadyPkg>) -> Answer {
        use std::fmt::Write;
        let mut hints = String::new();
        let mut shown = HashSet::new();
        for (idx, dep, _) in packages {
            if idx.upstream_provenance.as_ref().is_some_and(|m| m.files > 0 && upstream::stale(m)) && shown.insert((dep.eco, &dep.name, &dep.version)) {
                let spec = if self
                    .project
                    .find(&Spec::parse(&dep.name))
                    .first()
                    .is_some_and(|d| d.eco == dep.eco && d.version == dep.version)
                {
                    dep.name.clone()
                } else {
                    format!("{}:{}", dep.eco, dep.id())
                };
                let _ = writeln!(
                    hints,
                    "Upstream docs for {} were fetched with older rules. Refresh with `lockdocs fetch {spec}`.",
                    idx.id()
                );
            }
        }
        answer.text.insert_str(0, &hints);
        answer
    }
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
        let label = if e.guide_modes.is_empty() {
            String::new()
        } else {
            format!("Guide modes: {}\n", e.guide_modes.join(", "))
        };
        return format!("### {} — {}\n{label}{}\n", e.name, cite, truncate(&e.doc, max_doc));
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
        let Some(p) = scope.iter().map(|&i| &idx.entries[i]).find(|p| {
            p.kind == Kind::Prose
                && p.file == e.file
                && p.line == line
                && (p.guide_modes.is_empty() || (!e.guide_modes.is_empty() && e.guide_modes.iter().all(|mode| p.guide_modes.contains(mode))))
        }) else {
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
    pub(super) fn remaining(&self) -> usize {
        self.budget.saturating_sub(self.used())
    }
    /// Fit a cited excerpt without consuming space reserved for another hit.
    pub(super) fn push_excerpt(&mut self, block: &str, budget: usize) -> bool {
        let budget = budget.min(self.remaining());
        if est_tokens(block) < budget {
            return self.push_block(block);
        }
        const END: &str = "[…truncated to fit the token budget]\n";
        let max_chars = (budget * 3).saturating_sub(1 + END.chars().count() + 4);
        let mut excerpt = String::new();
        let mut len = 0;
        for line in block.lines() {
            len += line.chars().count() + 1;
            if len > max_chars {
                break;
            }
            excerpt.push_str(line);
            excerpt.push('\n');
        }
        if excerpt.matches("```").count() % 2 == 1 {
            excerpt.push_str("```\n");
        }
        // A bare citation is not evidence.
        if excerpt.lines().filter(|line| !line.is_empty() && !line.starts_with("```")).count() < 2 {
            return false;
        }
        excerpt.push_str(END);
        self.push_block(&excerpt)
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
        let left = self.remaining();
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
