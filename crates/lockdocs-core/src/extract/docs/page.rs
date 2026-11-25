//! Convert JavaScript and JSX documentation pages into Markdown.

use super::super::syntax::{compact, dedent, parse_with, txt};
use tree_sitter::Node;

/// Title for a page file without one: its route (`docs/installation/using-vite`).
pub(in crate::extract) fn page_title(rel: &str) -> String {
    let segs: Vec<&str> = rel.split('/').filter(|s| !(s.starts_with('(') && s.ends_with(')'))).collect();
    let mut segs: Vec<&str> = segs.iter().rev().take(3).rev().copied().collect();
    if let Some(last) = segs.last() {
        if last.starts_with("page.") || last.starts_with("index.") {
            segs.pop();
        }
    }
    segs.join("/")
        .trim_end_matches(".tsx")
        .trim_end_matches(".jsx")
        .trim_end_matches(".js")
        .to_string()
}

/// Markdown from a docs page written as a JS/TSX component: `title:` values
/// of listed steps become headings, `description:` and JSX text become
/// paragraphs, `code:` strings become fenced code (language from `lang:`),
/// and JSX `<h1>`..`<h6>` stay headings. Lines keep their source position
/// where they can. Returns (markdown, page title from `metadata.title`).
pub(in crate::extract) fn page_markdown(text: &str) -> Option<(String, Option<String>)> {
    if text.len() > 200_000 {
        return None;
    }
    parse_with("tsx", text, |root, src| {
        let mut p = PageOut::default();
        page_walk(root, src, &mut p, false);
        p.flush();
        let mut md = String::new();
        let mut n = 0u32;
        for (line, t) in &p.out {
            while n + 1 < *line {
                md.push('\n');
                n += 1;
            }
            md.push_str(t);
            md.push('\n');
            n += 1 + t.matches('\n').count() as u32;
        }
        (md, p.title)
    })
    .filter(|(md, _)| !md.trim().is_empty())
}

#[derive(Default)]
struct PageOut {
    out: Vec<(u32, String)>,
    para: String,
    para_line: u32,
    title: Option<String>,
}

impl PageOut {
    fn text(&mut self, line: u32, t: &str) {
        if self.para.trim().is_empty() {
            self.para_line = line;
        }
        self.para.push_str(t);
    }
    fn flush(&mut self) {
        let t = compact(&self.para, usize::MAX);
        if !t.is_empty() {
            self.out.push((self.para_line, t));
        }
        self.para.clear();
    }
    fn block(&mut self, line: u32, t: String) {
        self.flush();
        self.out.push((line, t));
    }
}

fn line_of(n: Node) -> u32 {
    n.start_position().row as u32 + 1
}

/// The value of a string, template string or tagged template (`dedent`...``).
fn js_string(n: Node, src: &[u8]) -> Option<String> {
    match n.kind() {
        "string" => {
            let raw = txt(n, src);
            let inner = raw.get(1..raw.len().saturating_sub(1)).unwrap_or("");
            Some(inner.replace("\\n", "\n").replace("\\'", "'").replace("\\\"", "\"").replace("\\\\", "\\"))
        }
        "template_string" => {
            let raw = txt(n, src);
            let inner = raw.get(1..raw.len().saturating_sub(1)).unwrap_or("");
            Some(dedent(&inner.trim_matches('\n').lines().collect::<Vec<_>>()))
        }
        "call_expression" => n
            .child_by_field_name("arguments")
            .filter(|a| a.kind() == "template_string")
            .and_then(|a| js_string(a, src)),
        "parenthesized_expression" => n.named_child(0).and_then(|c| js_string(c, src)),
        _ => None,
    }
}

fn jsx_name(n: Node, src: &[u8]) -> String {
    let open = if n.kind() == "jsx_element" {
        n.child_by_field_name("open_tag")
    } else {
        Some(n)
    };
    open.and_then(|o| o.child_by_field_name("name"))
        .map(|x| txt(x, src).to_string())
        .unwrap_or_default()
}

/// The visible text inside a JSX element.
fn jsx_text(n: Node, src: &[u8], out: &mut String) {
    match n.kind() {
        "jsx_text" => out.push_str(txt(n, src)),
        "jsx_expression" => {
            if let Some(v) = n.named_child(0).and_then(|c| js_string(c, src)) {
                out.push_str(&v);
            }
        }
        "jsx_attribute" | "jsx_opening_element" | "jsx_closing_element" => {}
        _ => {
            let mut c = n.walk();
            for ch in n.named_children(&mut c) {
                jsx_text(ch, src, out);
            }
        }
    }
}

fn page_walk(n: Node, src: &[u8], p: &mut PageOut, in_array: bool) {
    match n.kind() {
        "import_statement" | "comment" | "jsx_attribute" => {}
        "pair" => {
            let key = n
                .child_by_field_name("key")
                .map(|k| txt(k, src).trim_matches(['"', '\'']).to_string())
                .unwrap_or_default();
            let Some(value) = n.child_by_field_name("value") else { return };
            match key.as_str() {
                "title" | "heading" => {
                    if let Some(t) = js_string(value, src) {
                        if !in_array && p.title.is_none() {
                            p.title = Some(t);
                        } else if in_array {
                            p.block(line_of(n), format!("## {}", compact(&t, 200)));
                        }
                    }
                }
                "description" => {
                    if let Some(t) = js_string(value, src) {
                        p.block(line_of(value), compact(&t, usize::MAX));
                    }
                }
                "code" => match js_string(value, src) {
                    Some(code) => {
                        let lang = n
                            .parent()
                            .and_then(|obj| {
                                let mut c = obj.walk();
                                let found = obj.named_children(&mut c).find(|x| {
                                    x.kind() == "pair" && x.child_by_field_name("key").is_some_and(|k| txt(k, src).trim_matches(['"', '\'']) == "lang")
                                });
                                found
                            })
                            .and_then(|x| x.child_by_field_name("value"))
                            .and_then(|v| js_string(v, src))
                            .unwrap_or_default();
                        p.block(line_of(value), format!("```{lang}\n{}\n```", code.trim_end()));
                    }
                    None => page_walk(value, src, p, in_array),
                },
                "name" | "lang" | "className" | "href" | "id" | "url" | "src" | "image" | "images" | "icon" | "openGraph" | "twitter" | "alternates" => {}
                _ => page_walk(value, src, p, in_array),
            }
        }
        "jsx_element" | "jsx_self_closing_element" => {
            let name = jsx_name(n, src);
            let b = name.as_bytes();
            if b.len() == 2 && b[0] == b'h' && (b'1'..=b'6').contains(&b[1]) {
                let mut t = String::new();
                jsx_text(n, src, &mut t);
                let t = compact(&t, 200);
                if !t.is_empty() {
                    p.block(line_of(n), format!("{} {t}", "#".repeat((b[1] - b'0') as usize)));
                }
            } else if name == "code" {
                let mut t = String::new();
                jsx_text(n, src, &mut t);
                p.text(line_of(n), &format!("`{}`", t.trim()));
            } else {
                let block = matches!(name.as_str(), "p" | "li" | "div" | "section" | "ul" | "ol" | "pre" | "table" | "tr");
                if block {
                    p.flush();
                }
                let mut c = n.walk();
                for ch in n.named_children(&mut c) {
                    page_walk(ch, src, p, in_array);
                }
                if block {
                    p.flush();
                }
            }
        }
        "jsx_text" => p.text(line_of(n), txt(n, src)),
        "jsx_expression" => match n.named_child(0) {
            Some(c) if matches!(c.kind(), "string" | "template_string") => {
                let v = js_string(c, src).unwrap_or_default();
                p.text(line_of(n), &v);
            }
            Some(c) => page_walk(c, src, p, in_array),
            None => {}
        },
        _ => {
            let arr = in_array || n.kind() == "array";
            let mut c = n.walk();
            for ch in n.named_children(&mut c) {
                page_walk(ch, src, p, arr);
            }
        }
    }
}
