//! Shared tree-sitter parser pooling and syntax text normalization.

use std::cell::RefCell;
use tree_sitter::{Node, Parser};

thread_local! {
    static PARSERS: RefCell<Vec<(&'static str, Parser)>> = const { RefCell::new(Vec::new()) };
}

/// Parse with a pooled parser. The parser is taken out of the pool while in
/// use, so `f` may parse again (Rust macro bodies do).
pub(super) fn parse_with<R>(lang: &'static str, text: &str, f: impl FnOnce(Node, &[u8]) -> R) -> Option<R> {
    let pooled = PARSERS.with(|ps| {
        let mut ps = ps.borrow_mut();
        ps.iter().position(|(l, _)| *l == lang).map(|i| ps.swap_remove(i).1)
    });
    let mut parser = match pooled {
        Some(p) => p,
        None => {
            let mut p = Parser::new();
            let language: tree_sitter::Language = match lang {
                "ts" => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
                "tsx" => tree_sitter_typescript::LANGUAGE_TSX.into(),
                "py" => tree_sitter_python::LANGUAGE.into(),
                "rs" => tree_sitter_rust::LANGUAGE.into(),
                _ => tree_sitter_go::LANGUAGE.into(),
            };
            p.set_language(&language).ok()?;
            p
        }
    };
    let tree = parser.parse(text, None);
    PARSERS.with(|ps| ps.borrow_mut().push((lang, parser)));
    let tree = tree?;
    Some(f(tree.root_node(), text.as_bytes()))
}

pub(super) fn txt<'a>(n: Node, src: &'a [u8]) -> &'a str {
    n.utf8_text(src).unwrap_or("")
}

pub(super) fn compact(s: &str, cap: usize) -> String {
    let mut out = String::with_capacity(s.len().min(cap.saturating_add(8)));
    let mut space = false;
    for c in s.chars() {
        if c.is_whitespace() {
            space = true;
            continue;
        }
        if space && !out.is_empty() {
            out.push(' ');
        }
        space = false;
        out.push(c);
        if out.len() >= cap {
            out.push_str(" …");
            break;
        }
    }
    out.replace("( ", "(").replace(", )", ")").replace(",)", ")").replace(" )", ")")
}

/// Source text from `n` up to (not including) its body child.
pub(super) fn head(n: Node, src: &[u8], body_field: &str, cap: usize) -> String {
    let end = n.child_by_field_name(body_field).map_or(n.end_byte(), |b| b.start_byte());
    let s = std::str::from_utf8(&src[n.start_byte()..end]).unwrap_or("");
    compact(s.trim_end().trim_end_matches(['{', ':', '=']).trim_end(), cap)
}

pub(super) fn cap_doc(s: String) -> String {
    const CAP: usize = 4000;
    if s.len() <= CAP {
        return s;
    }
    let mut i = CAP;
    while !s.is_char_boundary(i) {
        i -= 1;
    }
    format!("{} …", &s[..i])
}

pub(super) fn dedent(lines: &[&str]) -> String {
    let indent = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    let v: Vec<&str> = lines.iter().map(|l| if l.len() >= indent { &l[indent..] } else { l.trim_start() }).collect();
    v.join("\n").trim().to_string()
}

pub(super) fn clean_block_comment(s: &str) -> String {
    let s = s
        .trim_start_matches("/**")
        .trim_start_matches("/*!")
        .trim_start_matches("/*")
        .trim_end_matches("*/");
    let lines: Vec<&str> = s
        .lines()
        .map(|l| {
            let t = l.trim_start();
            match t.strip_prefix('*') {
                Some(r) => r.strip_prefix(' ').unwrap_or(r),
                None => l,
            }
        })
        .collect();
    cap_doc(dedent(&lines))
}
