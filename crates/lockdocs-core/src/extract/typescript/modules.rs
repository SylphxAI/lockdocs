use super::super::{
    entry,
    syntax::{compact, head, txt},
    Kind,
};
use super::TsWalker;
use tree_sitter::Node;

impl TsWalker<'_> {
    pub(super) fn block(&mut self, node: Node, prefix: &str) {
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

    pub(super) fn module(&mut self, d: Node, prefix: &str, doc: String, line: u32) {
        let src = self.src;
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
}
