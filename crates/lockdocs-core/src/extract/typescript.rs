//! Extract TypeScript and JavaScript declarations, members, and re-exports.

mod declarations;
mod members;
mod modules;

use super::syntax::{clean_block_comment, parse_with, txt};
use super::Entry;
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
}
