use super::super::{
    entry,
    syntax::{compact, head, txt},
    Kind,
};
use super::TsWalker;
use tree_sitter::Node;

impl TsWalker<'_> {
    pub(super) fn members(&mut self, body: Node, prefix: &str) {
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
