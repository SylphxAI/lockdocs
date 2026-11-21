//! Extract Rust items, crate and module docs, and items wrapped in macros.

use super::docs::prose_section;
use super::syntax::{cap_doc, clean_block_comment, compact, head, parse_with, txt};
use super::{entry, Entry, Kind};
use crate::markdown;
use std::collections::HashMap;
use std::path::Path;
use tree_sitter::Node;

pub(super) fn rust_crate_name(dir: &Path) -> Option<String> {
    let t = std::fs::read_to_string(dir.join("Cargo.toml")).ok()?;
    let v: toml::Table = toml::from_str(&t).ok()?;
    let lib = v.get("lib").and_then(|l| l.get("name")).and_then(|n| n.as_str());
    let pkg = v.get("package").and_then(|p| p.get("name")).and_then(|n| n.as_str());
    lib.or(pkg).map(|n| n.replace('-', "_"))
}

fn rs_module(rel: &str, crate_name: &str) -> String {
    let r = rel.strip_prefix("src/").unwrap_or(rel).trim_end_matches(".rs");
    let mut segs: Vec<&str> = r.split('/').collect();
    if matches!(segs.last(), Some(&"mod") | Some(&"lib") | Some(&"main")) {
        segs.pop();
    }
    let mut p = crate_name.to_string();
    for s in segs {
        if !s.is_empty() {
            p.push_str("::");
            p.push_str(s);
        }
    }
    p
}

pub(super) fn rs_file(text: &str, rel: &str, crate_name: &str, out: &mut Vec<Entry>) {
    let module = rs_module(rel, crate_name);
    rs_parse(text, rel, &module, 0, 0, None, out);
}

/// Docs and `#[macro_export]` a wrapper macro applies to the items it wraps
/// (tokio's `doc! { macro_rules! select { .. } }` pattern).
type Inherit = Option<(String, bool)>;

fn rs_parse(text: &str, rel: &str, module: &str, line_off: u32, depth: u32, inherit: Inherit, out: &mut Vec<Entry>) {
    parse_with("rs", text, |root, src| {
        let mut inner_doc = Vec::new();
        let mut w = RsWalker {
            src,
            file: rel,
            line_off,
            depth,
            out: Vec::new(),
            wrappers: HashMap::new(),
            inherit,
        };
        w.block(root, module, None, &mut inner_doc);
        if !inner_doc.is_empty() {
            let doc = inner_doc.join("\n");
            for s in markdown::split(&doc, module) {
                out.push(prose_section(s, rel, line_off));
            }
        }
        out.append(&mut w.out);
    });
}

struct RsWalker<'a> {
    src: &'a [u8],
    file: &'a str,
    line_off: u32,
    depth: u32,
    out: Vec<Entry>,
    /// Local `macro_rules!` that wrap an `$item` with docs: name -> (doc, exported).
    wrappers: HashMap<String, (String, bool)>,
    inherit: Inherit,
}

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

fn strip_line_doc(t: &str) -> Option<(&str, bool)> {
    if let Some(r) = t.strip_prefix("///") {
        if r.starts_with('/') {
            return None;
        }
        return Some((r.strip_prefix(' ').unwrap_or(r).trim_end(), false));
    }
    if let Some(r) = t.strip_prefix("//!") {
        return Some((r.strip_prefix(' ').unwrap_or(r).trim_end(), true));
    }
    None
}

impl RsWalker<'_> {
    fn line(&self, n: Node) -> u32 {
        n.start_position().row as u32 + 1 + self.line_off
    }

    /// `owner`: impl/trait type whose members we are in (`Some((name, is_trait))`).
    fn block(&mut self, node: Node, module: &str, owner: Option<(&str, bool)>, inner_doc: &mut Vec<String>) {
        let src = self.src;
        let mut cur = node.walk();
        let children: Vec<Node> = node.named_children(&mut cur).collect();
        let mut pending: Vec<String> = Vec::new();
        let mut hidden = false;
        let mut exported_macro = false;
        let mut first_line: Option<u32> = None;
        for c in children {
            let k = c.kind();
            match k {
                "line_comment" => {
                    let t = txt(c, src);
                    if let Some((d, inner)) = strip_line_doc(t) {
                        if inner {
                            inner_doc.push(d.to_string());
                        } else {
                            if pending.is_empty() {
                                first_line = Some(self.line(c));
                            }
                            pending.push(d.to_string());
                        }
                    }
                    continue;
                }
                "block_comment" => {
                    let t = txt(c, src);
                    if t.starts_with("/*!") {
                        inner_doc.push(clean_block_comment(t));
                    } else if t.starts_with("/**") && !t.starts_with("/***") {
                        pending.push(clean_block_comment(t));
                    }
                    continue;
                }
                "attribute_item" => {
                    let t = txt(c, src);
                    if t.contains("doc(hidden)") {
                        hidden = true;
                    }
                    if t.contains("macro_export") {
                        exported_macro = true;
                    }
                    // `#[doc = "..."]`
                    if let Some(rest) = t.strip_prefix("#[doc = \"") {
                        pending.push(rest.trim_end_matches("\"]").replace("\\n", "\n"));
                    }
                    continue;
                }
                "inner_attribute_item" => continue,
                _ => {}
            }
            let mut doc = cap_doc(pending.join("\n").trim().to_string());
            if doc.is_empty() {
                if let Some((d, _)) = &self.inherit {
                    doc = d.clone();
                }
            }
            let _ = first_line.take();
            pending.clear();
            let was_hidden = std::mem::take(&mut hidden);
            let was_macro_export = std::mem::take(&mut exported_macro) || self.inherit.as_ref().is_some_and(|i| i.1);
            if was_hidden {
                continue;
            }
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
                    let (Some(name), true) = (name, public) else { continue };
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
                    let (Some(name), true) = (name, public) else { continue };
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
                    let (Some(name), true) = (name, public) else { continue };
                    self.out
                        .push(entry(Kind::Trait, &name, qual(&name), self.file, line, head(c, src, "body", 400), doc));
                    if let Some(b) = c.child_by_field_name("body") {
                        let mut ignore = Vec::new();
                        self.block(b, module, Some((&name, true)), &mut ignore);
                    }
                }
                "impl_item" => {
                    if c.child_by_field_name("trait").is_some() {
                        continue;
                    }
                    let Some(ty) = c.child_by_field_name("type") else { continue };
                    let ty = txt(ty, src);
                    let ty = ty.split('<').next().unwrap_or(ty).trim().to_string();
                    if let Some(b) = c.child_by_field_name("body") {
                        let mut ignore = Vec::new();
                        self.block(b, module, Some((&ty, false)), &mut ignore);
                    }
                }
                "mod_item" => {
                    let (Some(name), true) = (name, public) else { continue };
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
                    let (Some(name), true) = (name, public) else { continue };
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
                    let Some(name) = name else { continue };
                    if let Some(w) = doc_wrapper(txt(c, src)) {
                        self.wrappers.insert(name.clone(), w);
                        continue;
                    }
                    if !was_macro_export {
                        continue;
                    }
                    // Exported macros live at the crate root.
                    let root = module.split("::").next().unwrap_or(module);
                    let arms = compact(txt(c, src), 300);
                    self.out.push(entry(Kind::Macro, &name, format!("{root}::{name}!"), self.file, line, arms, doc));
                }
                "macro_invocation" if self.depth < 3 => {
                    // cfg_rt! { pub fn spawn ... } and friends: parse the body as items.
                    let Some(tt) = c.named_children(&mut c.walk()).find(|n| n.kind() == "token_tree") else {
                        continue;
                    };
                    let body = txt(tt, src);
                    let called = c.child_by_field_name("macro").map(|m| txt(m, src).to_string()).unwrap_or_default();
                    let wrapper = self.wrappers.get(&called).cloned();
                    let itemish =
                        body.contains("fn ") || body.contains("struct ") || body.contains("mod ") || body.contains("trait ") || body.contains("macro_rules!");
                    if body.len() < 4 || !itemish {
                        continue;
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
}
