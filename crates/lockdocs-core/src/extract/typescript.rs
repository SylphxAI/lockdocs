//! Extract TypeScript and JavaScript declarations, members, and re-exports.

use super::syntax::{clean_block_comment, compact, head, parse_with, txt};
use super::{entry, Entry, Kind};
use tree_sitter::Node;

pub(super) fn ts_file(text: &str, rel: &str, out: &mut Vec<Entry>) {
    let lang = if rel.ends_with(".tsx") || rel.ends_with(".jsx") { "tsx" } else { "ts" };
    let is_js = rel.ends_with(".js") || rel.ends_with(".mjs") || rel.ends_with(".cjs");
    parse_with(lang, text, |root, src| {
        let mut w = TsWalker {
            src,
            file: rel,
            out,
            js: is_js,
        };
        w.block(root, "");
    });
}

struct TsWalker<'a> {
    src: &'a [u8],
    file: &'a str,
    out: &'a mut Vec<Entry>,
    js: bool,
}

impl TsWalker<'_> {
    fn doc(&self, n: Node) -> String {
        let mut p = n.prev_sibling();
        while let Some(c) = p {
            if c.kind() == "comment" {
                let t = txt(c, self.src);
                if t.starts_with("/**") && c.end_position().row + 2 >= n.start_position().row {
                    return clean_block_comment(t);
                }
                if t.starts_with("//") {
                    p = c.prev_sibling();
                    continue;
                }
            }
            break;
        }
        String::new()
    }

    fn line(&self, n: Node) -> u32 {
        n.start_position().row as u32 + 1
    }

    fn block(&mut self, node: Node, prefix: &str) {
        let mut cur = node.walk();
        let children: Vec<Node> = node.named_children(&mut cur).collect();
        for c in children {
            match c.kind() {
                "export_statement" => {
                    let doc = self.doc(c);
                    if let Some(d) = c.child_by_field_name("declaration") {
                        self.decl(d, prefix, doc, c);
                    } else {
                        self.exports(c, prefix);
                    }
                }
                "ambient_declaration" => {
                    let doc = self.doc(c);
                    let mut cc = c.walk();
                    let inner: Vec<Node> = c.named_children(&mut cc).collect();
                    for d in inner {
                        self.decl(d, prefix, doc.clone(), c);
                    }
                }
                "expression_statement" => {
                    let mut cc = c.walk();
                    let inner: Vec<Node> = c.named_children(&mut cc).collect();
                    for d in inner {
                        if matches!(d.kind(), "internal_module" | "module") {
                            let doc = self.doc(c);
                            self.decl(d, prefix, doc, c);
                        }
                    }
                }
                _ => {
                    let doc = self.doc(c);
                    // In JS only documented top-level declarations are API.
                    if !self.js || !doc.is_empty() || !prefix.is_empty() {
                        self.decl(c, prefix, doc, c);
                    }
                }
            }
        }
    }

    fn exports(&mut self, n: Node, prefix: &str) {
        let mut cur = n.walk();
        let children: Vec<Node> = n.named_children(&mut cur).collect();
        for c in children {
            match c.kind() {
                "export_clause" => {
                    let mut cc = c.walk();
                    let specs: Vec<Node> = c.named_children(&mut cc).collect();
                    for s in specs {
                        let (Some(name), alias) = (s.child_by_field_name("name"), s.child_by_field_name("alias")) else {
                            continue;
                        };
                        let Some(alias) = alias else { continue };
                        let (from, to) = (txt(name, self.src), txt(alias, self.src));
                        if from == to || to == "default" {
                            continue;
                        }
                        let mut e = entry(
                            Kind::Alias,
                            to,
                            format!("{prefix}{to}"),
                            self.file,
                            self.line(s),
                            compact(txt(n, self.src), 200),
                            String::new(),
                        );
                        e.alias_of = Some(from.to_string());
                        self.out.push(e);
                    }
                }
                "namespace_export" => {
                    if let Some(id) = c.named_child(0) {
                        let name = txt(id, self.src);
                        self.out.push(entry(
                            Kind::Module,
                            name,
                            format!("{prefix}{name}"),
                            self.file,
                            self.line(n),
                            compact(txt(n, self.src), 200),
                            self.doc(n),
                        ));
                    }
                }
                _ => {}
            }
        }
    }

    fn decl(&mut self, d: Node, prefix: &str, doc: String, outer: Node) {
        let src = self.src;
        let name_of = |n: Node| n.child_by_field_name("name").map(|x| txt(x, src).to_string());
        let line = self.line(outer);
        match d.kind() {
            "ambient_declaration" => {
                let mut cur = d.walk();
                let inner: Vec<Node> = d.named_children(&mut cur).collect();
                for x in inner {
                    self.decl(x, prefix, doc.clone(), outer);
                }
            }
            "function_signature" | "function_declaration" | "generator_function_declaration" => {
                let Some(name) = name_of(d) else { return };
                let sig = head(outer, src, "__none__", 600).trim_end_matches(';').to_string();
                let sig = if d.child_by_field_name("body").is_some() {
                    head(d, src, "body", 600)
                } else {
                    sig
                };
                self.out
                    .push(entry(Kind::Function, &name, format!("{prefix}{name}"), self.file, line, sig, doc));
            }
            "class_declaration" | "abstract_class_declaration" | "class" => {
                let Some(name) = name_of(d) else { return };
                self.out.push(entry(
                    Kind::Class,
                    &name,
                    format!("{prefix}{name}"),
                    self.file,
                    line,
                    head(d, src, "body", 400),
                    doc,
                ));
                if let Some(body) = d.child_by_field_name("body") {
                    self.members(body, &format!("{prefix}{name}."));
                }
            }
            "interface_declaration" => {
                let Some(name) = name_of(d) else { return };
                self.out.push(entry(
                    Kind::Interface,
                    &name,
                    format!("{prefix}{name}"),
                    self.file,
                    line,
                    head(d, src, "body", 400),
                    doc,
                ));
                if let Some(body) = d.child_by_field_name("body") {
                    self.members(body, &format!("{prefix}{name}."));
                }
            }
            "type_alias_declaration" => {
                let Some(name) = name_of(d) else { return };
                self.out.push(entry(
                    Kind::Type,
                    &name,
                    format!("{prefix}{name}"),
                    self.file,
                    line,
                    compact(txt(d, src), 700),
                    doc,
                ));
            }
            "enum_declaration" => {
                let Some(name) = name_of(d) else { return };
                self.out.push(entry(
                    Kind::Enum,
                    &name,
                    format!("{prefix}{name}"),
                    self.file,
                    line,
                    compact(txt(d, src), 500),
                    doc,
                ));
            }
            "lexical_declaration" | "variable_declaration" => {
                let kw = txt(d, src).split_whitespace().next().unwrap_or("const").to_string();
                let mut cur = d.walk();
                let decls: Vec<Node> = d.named_children(&mut cur).filter(|n| n.kind() == "variable_declarator").collect();
                for v in decls {
                    let Some(nn) = v.child_by_field_name("name") else { continue };
                    if nn.kind() != "identifier" {
                        continue;
                    }
                    let name = txt(nn, src).to_string();
                    let value = v.child_by_field_name("value");
                    let is_fn = value.is_some_and(|x| matches!(x.kind(), "arrow_function" | "function_expression" | "function"))
                        || v.child_by_field_name("type").is_some_and(|t| txt(t, src).contains("=>"));
                    let sig = match value {
                        Some(val) if matches!(val.kind(), "arrow_function" | "function_expression" | "function") => {
                            let body_start = val.child_by_field_name("body").map_or(val.end_byte(), |b| b.start_byte());
                            let s = std::str::from_utf8(&src[v.start_byte()..body_start]).unwrap_or("");
                            compact(&format!("{kw} {}", s.trim_end().trim_end_matches("=>").trim_end()), 600)
                        }
                        Some(val) if val.end_byte() - val.start_byte() > 300 => {
                            let s = std::str::from_utf8(&src[v.start_byte()..val.start_byte()]).unwrap_or("");
                            compact(&format!("{kw} {s} …"), 600)
                        }
                        _ => compact(&format!("{kw} {}", txt(v, src)), 600),
                    };
                    let kind = if is_fn { Kind::Function } else { Kind::Const };
                    self.out.push(entry(kind, &name, format!("{prefix}{name}"), self.file, line, sig, doc.clone()));
                }
            }
            "internal_module" | "module" => {
                let Some(nn) = d.child_by_field_name("name") else { return };
                let raw = txt(nn, src);
                let quoted = raw.starts_with('"') || raw.starts_with('\'');
                let name = raw.trim_matches(['"', '\'']).to_string();
                let new_prefix = if quoted { prefix.to_string() } else { format!("{prefix}{name}.") };
                if !quoted {
                    self.out.push(entry(
                        Kind::Module,
                        &name,
                        format!("{prefix}{name}"),
                        self.file,
                        line,
                        head(d, src, "body", 200),
                        doc,
                    ));
                }
                if let Some(body) = d.child_by_field_name("body") {
                    self.block(body, &new_prefix);
                }
            }
            _ => {}
        }
    }

    fn members(&mut self, body: Node, prefix: &str) {
        let src = self.src;
        let mut cur = body.walk();
        let children: Vec<Node> = body.named_children(&mut cur).collect();
        for m in children {
            let kind = match m.kind() {
                "method_definition" | "method_signature" | "abstract_method_signature" => Kind::Method,
                "public_field_definition" | "property_signature" => Kind::Field,
                _ => continue,
            };
            let t = txt(m, src);
            if t.starts_with("private ") || t.starts_with("protected ") || t.contains(" private ") && kind == Kind::Field {
                continue;
            }
            let Some(nn) = m.child_by_field_name("name") else { continue };
            let name = txt(nn, src).trim_matches(['"', '\'']).to_string();
            if name.starts_with('#') || name.starts_with('_') || name.starts_with('[') {
                continue;
            }
            let doc = self.doc(m);
            let sig = if m.child_by_field_name("body").is_some() {
                head(m, src, "body", 500)
            } else {
                compact(t.trim_end_matches(';'), 500)
            };
            self.out.push(entry(
                kind,
                &name,
                format!("{prefix}{name}"),
                self.file,
                m.start_position().row as u32 + 1,
                sig,
                doc,
            ));
        }
    }
}
