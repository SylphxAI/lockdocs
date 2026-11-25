//! Extract entries from declarations and parse items wrapped in macros.

use super::{rs_parse, strip_line_doc, RsWalker};
use crate::extract::syntax::{cap_doc, compact, head, txt};
use crate::extract::{entry, Kind};
use tree_sitter::Node;

/// A local macro like `macro_rules! doc { ($x:item) => { /// docs  #[macro_export] $x } }`.
fn doc_wrapper(text: &str) -> Option<(String, bool)> {
    if !text.contains(":item") {
        return None;
    }
    let doc: Vec<&str> = text
        .lines()
        .filter_map(|l| strip_line_doc(l.trim()).filter(|(_, inner)| !inner).map(|(d, _)| d))
        .collect();
    if doc.is_empty() {
        return None;
    }
    Some((cap_doc(doc.join("\n").trim().to_string()), text.contains("macro_export")))
}

fn is_pub(n: Node, src: &[u8]) -> bool {
    let mut cur = n.walk();
    let r = n
        .named_children(&mut cur)
        .any(|c| c.kind() == "visibility_modifier" && txt(c, src).trim() == "pub");
    r
}

impl RsWalker<'_> {
    pub(super) fn item(&mut self, c: Node, module: &str, owner: Option<(&str, bool)>, doc: String, was_macro_export: bool) {
        let src = self.src;
        let k = c.kind();
        let line = self.line(c);
        let sep = "::";
        let in_trait = owner.is_some_and(|o| o.1);
        let public = in_trait || is_pub(c, src);
        let name = c.child_by_field_name("name").map(|n| txt(n, src).to_string());
        let qual = |name: &str| match owner {
            Some((o, _)) => format!("{module}{sep}{o}{sep}{name}"),
            None => format!("{module}{sep}{name}"),
        };
        match k {
            "function_item" | "function_signature_item" => {
                let (Some(name), true) = (name, public) else { return };
                let kind = if owner.is_some() { Kind::Method } else { Kind::Function };
                self.out.push(entry(
                    kind,
                    &name,
                    qual(&name),
                    self.file,
                    line,
                    head(c, src, "body", 600).trim_end_matches(';').to_string(),
                    doc,
                ));
            }
            "struct_item" | "enum_item" | "union_item" => {
                let (Some(name), true) = (name, public) else { return };
                let kind = if k == "enum_item" { Kind::Enum } else { Kind::Struct };
                let full = txt(c, src);
                let sig = if full.len() <= 900 {
                    compact(full, 900)
                } else {
                    format!("{} {{ … }}", head(c, src, "body", 400))
                };
                self.out.push(entry(kind, &name, qual(&name), self.file, line, sig, doc));
            }
            "trait_item" => {
                let (Some(name), true) = (name, public) else { return };
                self.out
                    .push(entry(Kind::Trait, &name, qual(&name), self.file, line, head(c, src, "body", 400), doc));
                if let Some(b) = c.child_by_field_name("body") {
                    let mut ignore = Vec::new();
                    self.block(b, module, Some((&name, true)), &mut ignore);
                }
            }
            "impl_item" => {
                if c.child_by_field_name("trait").is_some() {
                    return;
                }
                let Some(ty) = c.child_by_field_name("type") else { return };
                let ty = txt(ty, src);
                let ty = ty.split('<').next().unwrap_or(ty).trim().to_string();
                if let Some(b) = c.child_by_field_name("body") {
                    let mut ignore = Vec::new();
                    self.block(b, module, Some((&ty, false)), &mut ignore);
                }
            }
            "mod_item" => {
                let (Some(name), true) = (name, public) else { return };
                let path = format!("{module}{sep}{name}");
                if !doc.is_empty() {
                    self.out
                        .push(entry(Kind::Module, &name, path.clone(), self.file, line, format!("pub mod {name}"), doc));
                }
                if let Some(b) = c.child_by_field_name("body") {
                    let mut inner = Vec::new();
                    self.block(b, &path, None, &mut inner);
                }
            }
            "type_item" | "const_item" | "static_item" => {
                let (Some(name), true) = (name, public) else { return };
                let kind = if k == "type_item" { Kind::Type } else { Kind::Const };
                self.out.push(entry(
                    kind,
                    &name,
                    qual(&name),
                    self.file,
                    line,
                    compact(txt(c, src).trim_end_matches(';'), 400),
                    doc,
                ));
            }
            "macro_definition" => {
                let Some(name) = name else { return };
                if let Some(w) = doc_wrapper(txt(c, src)) {
                    self.wrappers.insert(name.clone(), w);
                    return;
                }
                if !was_macro_export {
                    return;
                }
                // Exported macros live at the crate root.
                let root = module.split("::").next().unwrap_or(module);
                let arms = compact(txt(c, src), 300);
                self.out.push(entry(Kind::Macro, &name, format!("{root}::{name}!"), self.file, line, arms, doc));
            }
            "macro_invocation" if self.depth < 3 => {
                // cfg_rt! { pub fn spawn ... } and friends: parse the body as items.
                let Some(tt) = c.named_children(&mut c.walk()).find(|n| n.kind() == "token_tree") else {
                    return;
                };
                let body = txt(tt, src);
                let called = c.child_by_field_name("macro").map(|m| txt(m, src).to_string()).unwrap_or_default();
                let wrapper = self.wrappers.get(&called).cloned();
                let itemish =
                    body.contains("fn ") || body.contains("struct ") || body.contains("mod ") || body.contains("trait ") || body.contains("macro_rules!");
                if body.len() < 4 || !itemish {
                    return;
                }
                let inner = &body[1..body.len() - 1];
                let off = tt.start_position().row as u32 + self.line_off;
                let mut sub = Vec::new();
                rs_parse(
                    inner,
                    self.file,
                    module,
                    off,
                    self.depth + 1,
                    wrapper.or_else(|| self.inherit.clone()),
                    &mut sub,
                );
                // Owner-less items only: prose from nested inner docs stays.
                self.out.extend(sub);
            }
            _ => {}
        }
    }
}
