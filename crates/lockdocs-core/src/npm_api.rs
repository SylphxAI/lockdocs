//! Public npm declaration graph. Names are resolved through package entrypoints,
//! not by scoring unrelated declarations which happen to share a short name.
use crate::extract::{Entry, Kind};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};
use tree_sitter::{Node, Parser};

#[derive(Default, Serialize, Deserialize)]
pub struct PublicApi {
    pub roots: Vec<Root>,
    pub entries: Vec<Entry>,
    modules: BTreeMap<String, Module>,
}

#[derive(Serialize, Deserialize)]
pub struct Root {
    pub import: String,
    pub conditions: String,
    pub file: String,
}

#[derive(Default, Serialize, Deserialize)]
struct Module {
    exports: BTreeMap<String, Binding>,
    imports: BTreeMap<String, Binding>,
    stars: Vec<String>,
    incomplete: bool,
    assignment: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
enum Binding {
    Local(String),
    Named(String, String),
    Namespace(String),
}

#[derive(Default)]
pub struct Resolution {
    pub entries: BTreeSet<usize>,
    pub unresolved: bool,
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
fn resolve_file(from: &str, spec: &str, files: &BTreeSet<String>) -> String {
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

fn module(src: &str, file: &str, files: &BTreeSet<String>) -> Module {
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
fn leaves(v: &Value, condition: &str, out: &mut Vec<(String, String)>) {
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

impl PublicApi {
    pub fn build(name: &str, dir: &Path, sources: &BTreeMap<String, String>) -> Self {
        let files: BTreeSet<String> = sources.keys().cloned().collect();
        let modules = sources.iter().map(|(f, s)| (f.clone(), module(s, f, &files))).collect();
        let pkg: Value = std::fs::read_to_string(dir.join("package.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(Value::Null);
        let mut paths = BTreeMap::new();
        if let Some(exports) = pkg.get("exports") {
            if let Some(o) = exports.as_object().filter(|o| o.keys().any(|k| k.starts_with('.'))) {
                for (k, v) in o {
                    paths.insert(k.clone(), v.clone());
                }
            } else {
                paths.insert(".".into(), exports.clone());
            }
        } else {
            let target = pkg
                .get("types")
                .or_else(|| pkg.get("typings"))
                .or_else(|| pkg.get("main"))
                .and_then(Value::as_str)
                .unwrap_or("./index.d.ts");
            paths.insert(".".into(), Value::String(format!("./{}", target.trim_start_matches("./"))));
        }
        let routes: Vec<String> = paths.keys().cloned().collect();
        let mut roots = Vec::new();
        for (path, value) in paths {
            if path == "./package.json" {
                continue;
            }
            let mut targets = Vec::new();
            leaves(&value, "", &mut targets);
            for (conditions, target) in targets {
                if let (Some((pre, post)), Some((tp, ts))) = (path.split_once('*'), target.split_once('*')) {
                    for file in &files {
                        if let Some(mid) = file.strip_prefix(tp.trim_start_matches("./")).and_then(|s| s.strip_suffix(ts)) {
                            let public = format!("{pre}{mid}{post}");
                            let best = routes
                                .iter()
                                .filter(|r| {
                                    if let Some((a, b)) = r.split_once('*') {
                                        public.starts_with(a) && public.ends_with(b)
                                    } else {
                                        **r == public
                                    }
                                })
                                .max_by_key(|r| {
                                    if let Some((a, b)) = r.split_once('*') {
                                        (0, a.len(), b.len())
                                    } else {
                                        (1, r.len(), 0)
                                    }
                                });
                            if best != Some(&path) {
                                continue;
                            }
                            roots.push(Root {
                                import: format!("{name}{}{}{}", pre.trim_start_matches('.'), mid, post),
                                conditions: conditions.clone(),
                                file: file.clone(),
                            });
                        }
                    }
                } else {
                    let file = resolve_file("", &format!("./{}", target.trim_start_matches("./")), &files);
                    roots.push(Root {
                        import: format!("{name}{}", path.trim_start_matches('.')),
                        conditions,
                        file,
                    });
                }
            }
        }
        Self {
            roots,
            entries: crate::extract::npm_entries(sources),
            modules,
        }
    }

    /// Rebase an @types declaration graph onto the extraction's citation labels.
    pub(crate) fn prefixed(mut self, prefix: &str) -> Self {
        let rebase = |s: &mut String| {
            if !s.starts_with("external:") {
                *s = format!("{prefix}{s}");
            }
        };
        let binding = |b: &mut Binding| match b {
            Binding::Named(file, _) | Binding::Namespace(file) => rebase(file),
            _ => (),
        };
        for e in &mut self.entries {
            rebase(&mut e.file);
        }
        for root in &mut self.roots {
            rebase(&mut root.file);
        }
        self.modules = self
            .modules
            .into_iter()
            .map(|(file, mut m)| {
                for b in m.exports.values_mut().chain(m.imports.values_mut()) {
                    binding(b);
                }
                for star in &mut m.stars {
                    rebase(star);
                }
                (format!("{prefix}{file}"), m)
            })
            .collect();
        self
    }

    pub fn resolve(&self, root: &Root, symbol: &str, entries: &[Entry]) -> Resolution {
        let mut out = Resolution::default();
        self.exported(&root.file, symbol, entries, &mut BTreeSet::new(), &mut out);
        out
    }

    fn exported(&self, file: &str, path: &str, entries: &[Entry], seen: &mut BTreeSet<(String, String)>, out: &mut Resolution) {
        if seen.len() > 128 {
            out.unresolved = true;
            return;
        }
        let key = (file.to_string(), path.to_string());
        if !seen.insert(key.clone()) {
            return;
        }
        let Some(m) = self.modules.get(file) else {
            out.unresolved = true;
            seen.remove(&key);
            return;
        };
        out.unresolved |= m.incomplete;
        let (name, tail) = path.split_once('.').unwrap_or((path, ""));
        if let Some(b) = m.exports.get(name) {
            self.binding(file, b, tail, entries, seen, out);
        } else if let Some(namespace) = &m.assignment {
            let local = if path.starts_with(&format!("{namespace}.")) {
                path.to_string()
            } else {
                format!("{namespace}.{path}")
            };
            self.binding(file, &Binding::Local(local), "", entries, seen, out);
        } else if name != "default" {
            for star in &m.stars {
                self.exported(star, path, entries, seen, out);
            }
        }
        seen.remove(&key);
    }

    fn binding(&self, file: &str, b: &Binding, tail: &str, entries: &[Entry], seen: &mut BTreeSet<(String, String)>, out: &mut Resolution) {
        let extend = |s: &str| if tail.is_empty() { s.to_string() } else { format!("{s}.{tail}") };
        match b {
            Binding::Namespace(target) => {
                if tail.is_empty() {
                    out.unresolved = true;
                } else {
                    self.exported(target, tail, entries, seen, out);
                }
            }
            Binding::Named(target, name) => self.exported(target, &extend(name), entries, seen, out),
            Binding::Local(name) => {
                if let Some(import) = self.modules.get(file).and_then(|m| m.imports.get(name)) {
                    self.binding(file, import, tail, entries, seen, out);
                } else {
                    let path = extend(name);
                    let mut found = false;
                    for (i, e) in entries.iter().enumerate() {
                        if e.file == file && e.path == path && e.kind != Kind::Alias && e.kind != Kind::Prose {
                            out.entries.insert(i);
                            found = true;
                        }
                    }
                    if !found {
                        out.unresolved = true;
                    }
                }
            }
        }
    }
}
