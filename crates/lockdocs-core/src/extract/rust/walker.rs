//! Walk Rust syntax blocks and collect docs and attributes for each item.

use super::super::docs::prose_section;
use super::super::syntax::{cap_doc, clean_block_comment, parse_with, txt};
use super::super::Entry;
use crate::markdown;
use std::collections::HashMap;
use tree_sitter::Node;

mod items;

/// Docs and `#[macro_export]` a wrapper macro applies to the items it wraps
/// (tokio's `doc! { macro_rules! select { .. } }` pattern).
type Inherit = Option<(String, bool)>;

pub(super) fn rs_parse(text: &str, rel: &str, module: &str, line_off: u32, depth: u32, inherit: Inherit, out: &mut Vec<Entry>) {
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
            self.item(c, module, owner, doc, was_macro_export);
        }
    }
}
