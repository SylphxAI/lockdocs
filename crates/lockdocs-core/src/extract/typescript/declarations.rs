use super::super::{
    entry,
    syntax::{compact, head, txt},
    Kind,
};
use super::TsWalker;
use tree_sitter::Node;

impl TsWalker<'_> {
    pub(super) fn decl(&mut self, d: Node, prefix: &str, doc: String, outer: Node) {
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
            "internal_module" | "module" => self.module(d, prefix, doc, line),
            _ => {}
        }
    }
}
