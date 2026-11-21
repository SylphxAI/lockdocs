//! Extract exported Go declarations and package comments.

use super::docs::prose_section;
use super::syntax::{cap_doc, clean_block_comment, compact, head, parse_with, txt};
use super::{entry, Entry, Kind};
use crate::markdown;
use tree_sitter::Node;

fn exported(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_uppercase())
}

fn go_doc(n: Node, src: &[u8]) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut p = n.prev_sibling();
    let mut next_row = n.start_position().row;
    while let Some(c) = p {
        if c.kind() != "comment" || c.end_position().row + 1 != next_row {
            break;
        }
        let t = txt(c, src);
        if let Some(r) = t.strip_prefix("//") {
            lines.push(r.strip_prefix(' ').unwrap_or(r).trim_end().to_string());
        } else {
            lines.push(clean_block_comment(t));
        }
        next_row = c.start_position().row;
        p = c.prev_sibling();
    }
    lines.reverse();
    cap_doc(lines.join("\n").trim().to_string())
}

pub(super) fn go_file(text: &str, rel: &str, module: &str, out: &mut Vec<Entry>) {
    parse_with("go", text, |root, src| {
        let mut cur = root.walk();
        let children: Vec<Node> = root.named_children(&mut cur).collect();
        let mut pkg = module.rsplit('/').next().unwrap_or(module).to_string();
        for c in children {
            let line = c.start_position().row as u32 + 1;
            match c.kind() {
                "package_clause" => {
                    if let Some(id) = c.named_child(0) {
                        pkg = txt(id, src).to_string();
                    }
                    if pkg == "main" {
                        return;
                    }
                    let doc = go_doc(c, src);
                    if !doc.is_empty() {
                        let dir = rel.rsplit_once('/').map_or("", |x| x.0);
                        let title = if dir.is_empty() {
                            format!("package {pkg}")
                        } else {
                            format!("package {pkg} ({dir})")
                        };
                        for s in markdown::split(&doc, &title) {
                            out.push(prose_section(s, rel, 0));
                        }
                    }
                }
                "function_declaration" => {
                    let Some(name) = c.child_by_field_name("name").map(|n| txt(n, src)) else {
                        continue;
                    };
                    if !exported(name) {
                        continue;
                    }
                    out.push(entry(
                        Kind::Function,
                        name,
                        format!("{pkg}.{name}"),
                        rel,
                        line,
                        head(c, src, "body", 600),
                        go_doc(c, src),
                    ));
                }
                "method_declaration" => {
                    let Some(name) = c.child_by_field_name("name").map(|n| txt(n, src)) else {
                        continue;
                    };
                    let recv = c.child_by_field_name("receiver").map(|r| txt(r, src)).unwrap_or("");
                    let ty: String = recv
                        .trim_matches(['(', ')'])
                        .split_whitespace()
                        .last()
                        .unwrap_or("")
                        .trim_start_matches('*')
                        .split('[')
                        .next()
                        .unwrap_or("")
                        .to_string();
                    if !exported(name) || !exported(&ty) {
                        continue;
                    }
                    out.push(entry(
                        Kind::Method,
                        name,
                        format!("{pkg}.{ty}.{name}"),
                        rel,
                        line,
                        head(c, src, "body", 600),
                        go_doc(c, src),
                    ));
                }
                "type_declaration" | "const_declaration" | "var_declaration" => {
                    let outer_doc = go_doc(c, src);
                    let mut cc = c.walk();
                    let specs: Vec<Node> = c
                        .named_children(&mut cc)
                        .filter(|n| matches!(n.kind(), "type_spec" | "type_alias" | "const_spec" | "var_spec"))
                        .collect();
                    let single = specs.len() == 1;
                    for s in specs {
                        let names: Vec<String> = if s.kind().starts_with("type") {
                            s.child_by_field_name("name").map(|n| vec![txt(n, src).to_string()]).unwrap_or_default()
                        } else {
                            let mut sc = s.walk();
                            s.children_by_field_name("name", &mut sc).map(|n| txt(n, src).to_string()).collect()
                        };
                        let doc = if single { outer_doc.clone() } else { go_doc(s, src) };
                        let sline = s.start_position().row as u32 + 1;
                        for name in names.iter().filter(|n| exported(n)) {
                            if s.kind().starts_with("type") {
                                let full = txt(s, src);
                                let sig = if full.len() <= 900 {
                                    format!("type {}", compact(full, 900))
                                } else {
                                    format!("type {} …", compact(&full[..full.find('{').unwrap_or(200.min(full.len()))], 300))
                                };
                                let kind = if full.contains("interface") { Kind::Interface } else { Kind::Struct };
                                out.push(entry(kind, name, format!("{pkg}.{name}"), rel, sline, sig, doc.clone()));
                                // Interface methods
                                if let Some(t) = s.child_by_field_name("type").filter(|t| t.kind() == "interface_type") {
                                    let mut ic = t.walk();
                                    let elems: Vec<Node> = t.named_children(&mut ic).collect();
                                    for m in elems {
                                        if !matches!(m.kind(), "method_elem" | "method_spec") {
                                            continue;
                                        }
                                        let Some(mn) = m.child_by_field_name("name").map(|n| txt(n, src)) else {
                                            continue;
                                        };
                                        out.push(entry(
                                            Kind::Method,
                                            mn,
                                            format!("{pkg}.{name}.{mn}"),
                                            rel,
                                            m.start_position().row as u32 + 1,
                                            compact(txt(m, src), 400),
                                            go_doc(m, src),
                                        ));
                                    }
                                }
                            } else {
                                let kw = if c.kind() == "const_declaration" { "const" } else { "var" };
                                out.push(entry(
                                    Kind::Const,
                                    name,
                                    format!("{pkg}.{name}"),
                                    rel,
                                    sline,
                                    compact(&format!("{kw} {}", txt(s, src)), 300),
                                    doc.clone(),
                                ));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    });
}
