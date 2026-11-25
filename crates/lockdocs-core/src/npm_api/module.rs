//! Parse declaration modules and identify the source files behind module specifiers.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};
use tree_sitter::{Node, Parser};

#[derive(Default, Serialize, Deserialize)]
pub(super) struct Module {
    pub(super) exports: BTreeMap<String, Binding>,
    pub(super) imports: BTreeMap<String, Binding>,
    pub(super) stars: Vec<String>,
    pub(super) incomplete: bool,
    pub(super) assignment: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) enum Binding {
    Local(String),
    Named(String, String),
    Namespace(String),
}

fn text<'a>(n: Node, src: &'a [u8]) -> &'a str {
    n.utf8_text(src).unwrap_or("")
}
fn string(n: Node, src: &[u8]) -> String {
    text(n, src).trim_matches(['\'', '"']).to_string()
}
fn children(n: Node) -> Vec<Node> {
    n.named_children(&mut n.walk()).collect()
}
fn normalize(s: &str) -> Option<String> {
    let mut parts = Vec::new();
    for c in Path::new(s).components() {
        match c {
            Component::Normal(p) => parts.push(p.to_str()?),
            Component::ParentDir => {
                parts.pop()?;
            }
            Component::CurDir => {}
            _ => return None,
        }
    }
    Some(parts.join("/"))
}

/// Node's JS suffixes in declaration imports refer to declaration siblings.
pub(super) fn resolve_file(from: &str, spec: &str, files: &BTreeSet<String>) -> String {
    if !spec.starts_with('.') {
        return format!("external:{spec}");
    }
    let parent = from.rsplit_once('/').map_or("", |(p, _)| p);
    let Some(base) = normalize(format!("{parent}/{spec}").trim_start_matches('/')) else {
        return String::new();
    };
    let suffixes = [(".mjs", ".d.mts"), (".cjs", ".d.cts"), (".js", ".d.ts")];
    for (js, ts) in suffixes {
        if let Some(stem) = base.strip_suffix(js) {
            for candidate in [format!("{stem}{ts}"), format!("{stem}.d.ts"), base.clone()] {
                if files.contains(&candidate) {
                    return candidate;
                }
            }
        }
    }
    for candidate in [
        base.clone(),
        format!("{base}.d.ts"),
        format!("{base}/index.d.ts"),
        format!("{base}.ts"),
        format!("{base}/index.ts"),
    ] {
        if files.contains(&candidate) {
            return candidate;
        }
    }
    base
}

fn names(n: Node, src: &[u8], out: &mut Vec<String>) {
    if let Some(name) = n.child_by_field_name("name") {
        out.push(text(name, src).to_string());
    } else if matches!(n.kind(), "ambient_declaration" | "lexical_declaration" | "variable_declaration") {
        for c in children(n) {
            names(c, src, out);
        }
    }
}

pub(super) fn module(src: &str, file: &str, files: &BTreeSet<String>) -> Module {
    let mut m = Module::default();
    let mut parser = Parser::new();
    let _ = parser.set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into());
    let Some(tree) = parser.parse(src, None) else {
        m.incomplete = true;
        return m;
    };
    // Errors inside a type body do not change which declaration is exported.
    // A top-level recovery node may hide an export, so fail closed for that.
    m.incomplete = children(tree.root_node()).iter().any(|n| n.kind() == "ERROR");
    let bytes = src.as_bytes();
    if (file.ends_with(".js") || file.ends_with(".cjs") || file.ends_with(".mjs")) && (src.contains("module.exports") || src.contains("exports.")) {
        m.incomplete = true;
    }
    for n in children(tree.root_node()) {
        if n.kind() != "import_statement" && n.kind() != "export_statement" {
            continue;
        }
        // A UMD global name does not alter the module's explicit exports.
        if text(n, bytes).trim_start().starts_with("export as namespace ") {
            continue;
        }
        let target = n.child_by_field_name("source").map(|s| resolve_file(file, &string(s, bytes), files));
        if n.kind() == "import_statement" {
            if let Some(clause) = children(n).into_iter().find(|c| c.kind() == "import_clause") {
                for c in children(clause) {
                    match c.kind() {
                        "namespace_import" => {
                            if let (Some(t), Some(id)) = (&target, children(c).last()) {
                                m.imports.insert(text(*id, bytes).into(), Binding::Namespace(t.clone()));
                            }
                        }
                        "named_imports" => {
                            for s in children(c) {
                                if let (Some(t), Some(name)) = (&target, s.child_by_field_name("name")) {
                                    let alias = s.child_by_field_name("alias").unwrap_or(name);
                                    m.imports.insert(text(alias, bytes).into(), Binding::Named(t.clone(), string(name, bytes)));
                                }
                            }
                        }
                        "identifier" => {
                            if let Some(t) = &target {
                                m.imports.insert(text(c, bytes).into(), Binding::Named(t.clone(), "default".into()));
                            }
                        }
                        _ => m.incomplete = true,
                    }
                }
            } else {
                m.incomplete = true;
            }
            continue;
        }
        if let Some(d) = n.child_by_field_name("declaration") {
            let mut ns = Vec::new();
            names(d, bytes, &mut ns);
            let default = text(n, bytes).starts_with("export default ");
            for name in ns {
                m.exports.insert(if default { "default".into() } else { name.clone() }, Binding::Local(name));
            }
        } else if let Some(clause) = children(n).into_iter().find(|c| c.kind() == "export_clause") {
            for s in children(clause) {
                if let Some(name) = s.child_by_field_name("name") {
                    let alias = s.child_by_field_name("alias").unwrap_or(name);
                    let name = string(name, bytes);
                    let b = target
                        .as_ref()
                        .map_or_else(|| Binding::Local(name.clone()), |t| Binding::Named(t.clone(), name.clone()));
                    m.exports.insert(string(alias, bytes), b);
                }
            }
        } else if let Some(ns) = children(n).into_iter().find(|c| c.kind() == "namespace_export") {
            if let (Some(t), Some(id)) = (target, ns.named_child(0)) {
                m.exports.insert(string(id, bytes), Binding::Namespace(t));
            }
        } else if let Some(t) = target {
            m.stars.push(t);
        } else if let Some(value) = n
            .child_by_field_name("value")
            .or_else(|| text(n, bytes).trim_start().starts_with("export =").then(|| n.named_child(0)).flatten())
        {
            let name = text(value, bytes).to_string();
            if text(n, bytes).trim_start().starts_with("export =") {
                m.assignment = Some(name.clone());
            }
            m.exports.insert("default".into(), Binding::Local(name));
        } else {
            m.incomplete = true;
        }
    }
    m
}

/// Prefer the explicit declaration branch; do not silently combine a source
/// condition or an implementation file with a different declaration edition.
pub(super) fn leaves(v: &Value, condition: &str, out: &mut Vec<(String, String)>) {
    match v {
        Value::String(s) => out.push((condition.into(), s.clone())),
        Value::Object(o) => {
            if let Some(t) = o.get("types") {
                leaves(t, &format!("{condition}/types"), out);
            } else {
                for (k, v) in o {
                    leaves(v, &format!("{condition}/{k}"), out);
                }
            }
        }
        Value::Array(a) => {
            for (i, v) in a.iter().enumerate() {
                leaves(v, &format!("{condition}/fallback-{i}"), out);
            }
        }
        _ => {}
    }
}
