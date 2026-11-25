use crate::bm25;
use crate::extract::{Entry, Kind};

/// Split text into (prose, code) where code is inside ``` fences.
fn split_code(doc: &str) -> (String, String) {
    let mut prose = String::new();
    let mut code = String::new();
    let mut in_fence = false;
    for l in doc.lines() {
        let t = l.trim_start();
        if t.starts_with("```") || t.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        let dst = if in_fence || l.starts_with("    ") && !l.trim().is_empty() && prose.ends_with("\n\n") {
            &mut code
        } else {
            &mut prose
        };
        dst.push_str(l);
        dst.push('\n');
    }
    (prose, code)
}

/// The first sentence of a doc comment.
pub fn summary(prose: &str) -> &str {
    let p = prose.trim_start();
    let end = [p.find(". "), p.find(".\n"), p.find("\n\n")]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(p.len())
        .min(300);
    let mut end = end.min(p.len());
    while !p.is_char_boundary(end) {
        end -= 1;
    }
    &p[..end]
}

/// Weighted terms: names and headings count most, doc prose next, code
/// examples and signatures least (they repeat incidental identifiers).
pub fn entry_tf(e: &Entry) -> (Vec<(String, u16)>, u32) {
    let (prose, code) = split_code(&e.doc);
    if e.kind == Kind::Prose {
        let head = e.name.split(" › ").skip(1).collect::<Vec<_>>().join(" ");
        bm25::tf(&[(&head, 6), (&e.file, 1), (&prose, 2), (&code, 1)])
    } else {
        // The first sentence says what the symbol is for: weigh it like a heading.
        let summary = summary(&prose);
        bm25::tf(&[(&e.name, 8), (&e.path, 2), (&e.sig, 1), (summary, 4), (&prose, 2), (&code, 1)])
    }
}

/// Heading (or name) and first sentence: what an entry says it is about.
pub fn head_tf(e: &Entry) -> (Vec<(String, u16)>, u32) {
    let (prose, _) = split_code(&e.doc);
    let first = summary(&prose);
    if e.kind == Kind::Prose {
        bm25::tf(&[(crate::markdown::topic_heading(&e.name), 2), (first, 1)])
    } else {
        bm25::tf(&[(&e.name, 2), (first, 1)])
    }
}

/// What an entry means, for the embedding: its name and the start of its
/// prose (code examples and long signatures dilute a mean-pooled vector).
pub fn embed_text(e: &Entry) -> String {
    let (prose, code) = split_code(&e.doc);
    let mut body: String = prose.chars().take(600).collect();
    if e.kind == Kind::Prose {
        // A code-only section embedded by its title alone matches short
        // questions far too well; its code says what it is about.
        if body.trim().is_empty() {
            body = code.chars().take(600).collect();
        }
        let head: Vec<&str> = e.name.split(" › ").collect();
        let tail = head[head.len().saturating_sub(2)..].join(" ");
        return format!("{tail}. {body}");
    }
    if body.trim().is_empty() {
        body = e.sig.chars().take(200).collect();
    }
    let parent = e.path.rsplit(['.', ':']).nth(1).unwrap_or("");
    format!("{} {parent}. {body}", e.name)
}
