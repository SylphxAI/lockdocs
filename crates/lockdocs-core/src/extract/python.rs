//! Extract Python definitions and module docstrings.

use super::docs::prose_section;
use super::syntax::{cap_doc, compact, dedent, head, parse_with, txt};
use super::{entry, Entry, Kind};
use crate::markdown;
use tree_sitter::Node;

fn py_module(rel: &str) -> String {
    let r = rel.trim_end_matches(".pyi").trim_end_matches(".py");
    let r = r.trim_end_matches("/__init__");
    r.replace('/', ".")
}

pub(super) fn py_file(text: &str, rel: &str, out: &mut Vec<Entry>) {
    let module = py_module(rel);
    parse_with("py", text, |root, src| {
        if let Some(doc) = py_docstring(root, src) {
            for s in markdown::split(&doc, &module) {
                out.push(prose_section(s, rel, 0));
            }
        }
        py_block(root, src, &module, false, rel, out);
    });
}

fn py_docstring(block: Node, src: &[u8]) -> Option<String> {
    let first = block.named_child(0)?;
    let first = if first.kind() == "comment" { first.next_named_sibling()? } else { first };
    if first.kind() != "expression_statement" {
        return None;
    }
    let s = first.named_child(0)?;
    if s.kind() != "string" {
        return None;
    }
    let raw = txt(s, src);
    let raw = raw.trim_start_matches(['r', 'R', 'u', 'U', 'b', 'B']);
    let inner = raw
        .strip_prefix("\"\"\"")
        .and_then(|x| x.strip_suffix("\"\"\""))
        .or_else(|| raw.strip_prefix("'''").and_then(|x| x.strip_suffix("'''")))
        .or_else(|| raw.strip_prefix('"').and_then(|x| x.strip_suffix('"')))
        .or_else(|| raw.strip_prefix('\'').and_then(|x| x.strip_suffix('\'')))?;
    let lines: Vec<&str> = inner.lines().collect();
    if lines.is_empty() {
        return None;
    }
    // inspect.cleandoc: first line as is, the rest dedented.
    let first_line = lines[0].trim();
    let rest = dedent(&lines[1..]);
    let gap = if lines.get(1).is_some_and(|l| l.trim().is_empty()) { "\n\n" } else { "\n" };
    let doc = if rest.is_empty() {
        first_line.to_string()
    } else if first_line.is_empty() {
        rest
    } else {
        format!("{first_line}{gap}{rest}")
    };
    Some(cap_doc(doc))
}

fn py_block(block: Node, src: &[u8], prefix: &str, in_class: bool, rel: &str, out: &mut Vec<Entry>) {
    let mut cur = block.walk();
    let children: Vec<Node> = block.named_children(&mut cur).collect();
    for c in children {
        let (def, decorators) = if c.kind() == "decorated_definition" {
            let mut dc = c.walk();
            let decs: Vec<String> = c
                .named_children(&mut dc)
                .filter(|n| n.kind() == "decorator")
                .map(|n| compact(txt(n, src), 80))
                .collect();
            match c.child_by_field_name("definition") {
                Some(d) => (d, decs),
                None => continue,
            }
        } else {
            (c, Vec::new())
        };
        let Some(nn) = def.child_by_field_name("name") else { continue };
        let name = txt(nn, src);
        let dunder_ok = matches!(name, "__init__" | "__call__");
        if name.starts_with('_') && !dunder_ok {
            continue;
        }
        let line = c.start_position().row as u32 + 1;
        let mut sig = decorators.join(" ");
        match def.kind() {
            "function_definition" => {
                if !sig.is_empty() {
                    sig.push(' ');
                }
                let end = def.child_by_field_name("body").map_or(def.end_byte(), |b| b.start_byte());
                let raw = strip_py_comments(std::str::from_utf8(&src[def.start_byte()..end]).unwrap_or(""));
                sig.push_str(&compact(raw.trim_end().trim_end_matches(':').trim_end(), 700));
                let doc = def.child_by_field_name("body").and_then(|b| py_docstring(b, src)).unwrap_or_default();
                let kind = if in_class { Kind::Method } else { Kind::Function };
                out.push(entry(kind, name, format!("{prefix}.{name}"), rel, line, sig, doc));
            }
            "class_definition" => {
                if !sig.is_empty() {
                    sig.push(' ');
                }
                sig.push_str(&head(def, src, "body", 400));
                let body = def.child_by_field_name("body");
                let doc = body.and_then(|b| py_docstring(b, src)).unwrap_or_default();
                let path = format!("{prefix}.{name}");
                out.push(entry(Kind::Class, name, path.clone(), rel, line, sig, doc));
                if let Some(b) = body {
                    py_block(b, src, &path, true, rel, out);
                }
            }
            _ => {}
        }
    }
}

/// Drop `# comments` from Python source lines (outside string literals).
fn strip_py_comments(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for line in s.lines() {
        let mut quote: Option<char> = None;
        let mut cut = line.len();
        for (i, c) in line.char_indices() {
            match (quote, c) {
                (None, '#') => {
                    cut = i;
                    break;
                }
                (None, '"' | '\'') => quote = Some(c),
                (Some(q), c) if c == q => quote = None,
                _ => {}
            }
        }
        out.push_str(&line[..cut]);
        out.push('\n');
    }
    out
}
