//! Extract Rust items, crate and module docs, and items wrapped in macros.

use super::Entry;
use std::path::Path;

mod walker;

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
    walker::rs_parse(text, rel, &module, 0, 0, None, out);
}
